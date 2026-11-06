-- What a clip is read as and shown with (docs/moments-and-workspaces.md, "Source and transform"), with no mpv in it
-- (tests/transform_test.lua).
--
-- Input: how to read the source right, set once, kept only beside the source (its .afp.json's `source`): the log
-- curve, the turn it was recorded with, the lens under water with a flat port. A correction from anywhere (the source,
-- a moment, a clip preview) is written there at once, and every moment and clip preview of it reads it there.
--
-- Transform: how it's shown, a matter of taste, always kept whole, all its keys at once (a bundle): the
-- stabilization (normal, wide, off), its level, the horizon lock, the water's color fringes, the frame's aspect and
-- lens projection. A bundle is the source's (.afp.json's `view`), or a moment's or clip preview's own (its link's
-- `stab`, with "settings of its own"); with none, the session's plays (the gyroflow plugin's, from the app's defaults
-- and changed only by hand). The topmost bundle there is plays, and bundles never mix.
--
-- A Gyroflow project owns what it has: the smoothing (level), the horizon, the frame, the projection, and of the
-- input the turn and the lens under water. They're shown as the project's, and kept in a bundle beside it only to come
-- back if the project goes.
--
--   local t = require('transform')
--   t.KEYS, t.INPUT_KEYS          the bundle's keys, the input's, in order
--   t.PROJECT_KEYS                the bundle's keys a Gyroflow project owns (and the view: wider or off over it is
--                                 for viewing only)
--   t.NEUTRAL                     what a key that didn't exist yet played as: a bundle from before it is filled so
--   t.complete(bundle, session)   -> the bundle with every key (a missing one: NEUTRAL, else the session's), and
--                                    whether anything was added; horizon_lock "project" (the preset's) as
--                                    session.preset_horizon says
--   t.pick(view)                  -> the bundle's keys of a view, nil for none
--   t.input_of(view)              -> the input keys an old view or link kept, nil for none
--   t.status(saved, now, project) -> "saved", "changed" or "none" (nothing saved), comparing the bundle's keys (a
--                                    project's own left out)
--   t.migrate_source(doc)         -> moves an old .afp.json's input out of `view` into `source`; true when it changed
--   t.concrete(aspect, orientation, w, h) -> the frame's W:H for a source w x h (upright), or "original": the ratio
--                                    (landscape-wise) turned for the orientation; as-is: as the source is

local M = {}

M.KEYS = { "view", "level", "horizon_lock", "chromatic", "aspect", "orientation", "projection" }
M.INPUT_KEYS = { "curve", "rotation", "underwater" }
-- What a Gyroflow project owns of the bundle; a wider view or none over it is for viewing only, so the view too
M.PROJECT_KEYS = { view = true, level = true, horizon_lock = true, aspect = true, orientation = true, projection = true }
M.NEUTRAL = { aspect = "original", orientation = "as-is", projection = "linear" }

local VALID = {
    view = { normal = true, wide = true, off = true },
    level = { weak = true, medium = true, strong = true },
    horizon_lock = { yes = true, no = true },
    chromatic = { [true] = true, [false] = true },
    aspect = { original = true, ["16:9"] = true, ["4:3"] = true, ["5:4"] = true, ["1:1"] = true },
    orientation = { ["as-is"] = true, landscape = true, portrait = true },
    projection = { linear = true, stereographic = true, equidistant = true, equisolid = true, orthographic = true },
}

local function valid(k, v)
    if v == nil then return false end
    -- Any other ratio, landscape-wise (W >= H)
    if k == "aspect" and type(v) == "string" then
        local w, h = v:match("^(%d+):(%d+)$")
        if w then return tonumber(w) >= tonumber(h) and tonumber(h) > 0 end
    end
    return VALID[k][v] == true
end
M.valid = valid

function M.pick(view)
    if type(view) ~= "table" then return nil end
    local out, any = {}, false
    for _, k in ipairs(M.KEYS) do
        if view[k] ~= nil then out[k], any = view[k], true end
    end
    return any and out or nil
end

function M.input_of(view)
    if type(view) ~= "table" then return nil end
    local out, any = {}, false
    for _, k in ipairs(M.INPUT_KEYS) do
        if view[k] ~= nil then out[k], any = view[k], true end
    end
    return any and out or nil
end

-- The frame's aspect, before the orientation was a key of its own, was the ratio as it'd show: 9:16 a portrait
-- frame. Now it's the ratio landscape-wise (16:9) with an orientation: as it was (landscape, portrait), "original" as is
local function split_frame(b)
    if type(b) ~= "table" or b.aspect == nil or (b.orientation ~= nil and VALID.orientation[b.orientation]) then return b, false end
    local out = {}
    for k, v in pairs(b) do out[k] = v end
    local w, h = tostring(b.aspect):match("^(%d+):(%d+)$")
    if w and tonumber(w) < tonumber(h) then
        out.aspect, out.orientation = h .. ":" .. w, "portrait"
    elseif w then
        out.orientation = "landscape"
    else
        out.orientation = "as-is"
    end
    return out, true
end

function M.complete(bundle, session)
    session = session or {}
    local split
    bundle, split = split_frame(bundle)
    local out, added = {}, split
    for _, k in ipairs(M.KEYS) do
        local v = nil
        if type(bundle) == "table" then v = bundle[k] end
        if k == "horizon_lock" and v == "project" then
            v = session.preset_horizon
            added = true
        end
        if not valid(k, v) then
            if M.NEUTRAL[k] ~= nil then v = M.NEUTRAL[k] else v = session[k] end
            if not valid(k, v) then v = nil end
            added = added or v ~= nil
        end
        out[k] = v
    end
    return out, added
end

function M.status(saved, now, project)
    if type(saved) ~= "table" then return "none" end
    for _, k in ipairs(M.KEYS) do
        if not (project and M.PROJECT_KEYS[k]) and now[k] ~= nil and saved[k] ~= nil and now[k] ~= saved[k] then
            return "changed"
        end
    end
    return "saved"
end

function M.concrete(aspect, orientation, w, h)
    local landscape = not (w and h) or w >= h
    if aspect == nil or aspect == "original" then
        if orientation == "as-is" or orientation == nil or not (w and h) then return "original" end
        if (orientation == "portrait") == (w < h) or w == h then return "original" end
        local function gcd(a, b) while b ~= 0 do a, b = b, a % b end return a end
        local g = gcd(math.floor(w), math.floor(h))
        return string.format("%d:%d", math.floor(h) / g, math.floor(w) / g)
    end
    local a, b = tostring(aspect):match("^(%d+):(%d+)$")
    if not a then return "original" end
    local portrait = orientation == "portrait" or (orientation ~= "landscape" and not landscape)
    if portrait and a ~= b then return b .. ":" .. a end
    return a .. ":" .. b
end

-- Before input and transform were apart, `view` held the log curve, the frame's turn and the lens under water too.
-- The turn moves only where the frame kept its own aspect: a turn with another aspect (9:16 and 90°) made a portrait
-- frame of a level scene, which isn't how the camera was held, and stays in `view` (not read) to be looked at
function M.migrate_source(doc)
    if type(doc) ~= "table" or type(doc.view) ~= "table" then return false end
    local v, changed = doc.view, false
    doc.source = type(doc.source) == "table" and doc.source or {}
    local s = doc.source
    if v.curve ~= nil then
        if s.curve == nil then s.curve = v.curve end
        v.curve, changed = nil, true
    end
    if v.underwater ~= nil then
        if s.underwater == nil and v.underwater ~= "project" then s.underwater = v.underwater end
        v.underwater, changed = nil, true
    end
    local r = tonumber(v.rotation)
    if v.rotation ~= nil and (r == 0 or v.aspect == nil or v.aspect == "original") then
        if s.rotation == nil and r and r ~= 0 then s.rotation = r end
        v.rotation, changed = nil, true
    end
    if next(s) == nil then doc.source = nil end
    if next(v) == nil then doc.view = nil end
    return changed
end

return M
