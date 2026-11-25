-- What a clip is read as and shown with (docs/design/moments-and-workspaces.md, "Source and transform"), with no mpv in it
-- (tests/transform_test.lua).
--
-- Input: how to read the source right, set once, kept only beside the source (its .afp.json's `source`): the log
-- curve, the turn it was recorded with, the lens under water with a flat port, the underwater colour (on, brighter
-- highlights, strength: it takes out the colour the camera put in, so it's the source's, Anton 2026-11-24). A correction from anywhere (the source,
-- a moment, a clip preview) is for viewing until saved (the Source tab's Save), then written there, and every moment
-- and clip preview of it reads it there.
--
-- Transform: how it's shown, a matter of taste, always kept whole, all its keys at once (a bundle): the
-- stabilization (normal, wide, off), its level, the horizon lock, the water's color fringes, the frame's aspect and
-- orientation, the lens's projection, and the lens corrected or as recorded (apart from the stabilization: corrected
-- with it off is the lens alone), the temporal denoise on or off (off for a clip it harms, as the water's surface) and
-- its Stronger (for a very noisy clip). How much temporal denoise there is is the computer's (gyroflow.conf's
-- temporal_denoise, whose Off leaves it out whatever the clip's says), and a bundle's denoise_frames, _grain and
-- _tolerance from before that are left out. A bundle is the source's (.afp.json's `view`), or a moment's or clip preview's own (its link's
-- `stab`, with "settings of its own"); with none, the session's plays (the gyroflow plugin's, from the app's defaults
-- and changed only by hand). The topmost bundle there is plays, and bundles never mix.
--
-- A Gyroflow project owns what it has: the smoothing (level), the horizon, the frame, and of the input the turn and the
-- lens under water. They're shown as the project's, and kept in a bundle beside it only to come back if the project goes.
-- Not the output's lens (#196): as recorded, or corrected in a projection, "project" for the project's own, picked and
-- saved over a project as for any clip.
--
--   local t = require('transform')
--   t.KEYS, t.INPUT_KEYS          the bundle's keys, the input's, in order (t.UW_KEYS: the underwater colour's, input
--                                 too, never moved out of an old view by migrate_source: a one-off script does that)
--   t.PROJECT_KEYS                the bundle's keys a Gyroflow project owns. Not the view: on, wider (the project's
--                                 zoom left out) or off (the project kept but not used) is saved over it too, and the lens
--   t.NEUTRAL                     what a key that didn't exist yet played as: a bundle from before it is filled so
--   t.complete(bundle, session)   -> the bundle with every key (a missing one: NEUTRAL, else the session's), and
--                                    whether anything was added; horizon_lock "project" (the preset's) as
--                                    session.preset_horizon says
--   t.pick(view)                  -> the bundle's keys of a view, nil for none
--   t.session(get)                -> the sessions' values a bundle is completed with (get: mp.get_property_native)
--   t.input_of(view)              -> the input keys an old view or link kept, nil for none
--   t.status(saved, now, project) -> "saved", "changed" or "none" (nothing saved), comparing the bundle's keys (a
--                                    project's own left out)
--   t.migrate_source(doc)         -> moves an old .afp.json's input out of `view` into `source`; true when it changed
--   t.concrete(aspect, orientation, w, h) -> the frame's W:H for a source w x h (upright), or "original": the ratio
--                                    (landscape-wise) turned for the orientation; as-is: as the source is

local M = {}

-- The keys, their values, what a key that didn't exist yet plays as and what a project owns: registry.json's settings
-- (shared with the Library's renders.rs and gyroflow-export)
local reg = require 'registry'
M.KEYS = reg.keys("bundle")
M.INPUT_KEYS = reg.keys("input")
-- The underwater colour (the dlogm plugin's uwcolor.lua): the source's, as its input (tools/migrate_uw_to_source.py
-- moves an old bundle's there once). A bundle from before keeps them unread: not the bundle's any more
M.UW_KEYS = reg.keys("uw")
-- What a Gyroflow project owns of the bundle. The view isn't: a project that came out badly can be set Off (kept, not
-- used) or Wider (its zoom left out) and saved so (Anton, 2026-11-10). Nor the lens and its projection: the player's
-- choice over the project's, for viewing and exports (Anton, 2026-11-11, #196)
M.PROJECT_KEYS, M.NEUTRAL = {}, {}
-- Each listed key's valid values (projection's project: a Gyroflow project's own, linear without one)
local VALID = {}
for _, k in ipairs(M.KEYS) do
    local s = reg.setting(k)
    if s.project then M.PROJECT_KEYS[k] = true end
    if s.neutral ~= nil then M.NEUTRAL[k] = s.neutral end
end
for _, s in ipairs(reg.settings) do
    if s.group == "bundle" or s.group == "uw" then
        local v = {}
        if s.kind == "bool" then v[true], v[false] = true, true end
        for _, x in ipairs(s.values or {}) do v[x] = true end
        VALID[s.key] = v
    end
end

local function valid(k, v)
    if v == nil then return false end
    local s = reg.setting(k)
    -- Between its min and max (uw_strength: between the camera's colour, 0, and the correction, 1)
    if s and s.kind == "number" then return type(v) == "number" and v >= s.min and v <= s.max end
    -- Any other ratio, landscape-wise (W >= H)
    if s and s.kind == "ratio" and type(v) == "string" then
        local w, h = v:match("^(%d+):(%d+)$")
        if w then return tonumber(w) >= tonumber(h) and tonumber(h) > 0 end
    end
    return VALID[k] ~= nil and VALID[k][v] == true
end
M.valid = valid

-- The app's defaults a render is made with (transform.render_settings): the gyroflow plugin's bundle and the dlogm
-- plugin's underwater colour (uwcolor.conf), the source's where it has none. get: mp.get_property_native
function M.defaults(get)
    local out = {}
    for _, name in ipairs({ "user-data/gyroflow/defaults", "user-data/uwcolor/defaults" }) do
        local t = get(name)
        if type(t) == "table" then for k, v in pairs(t) do out[k] = v end end
    end
    return out
end

-- What a key missing from a bundle plays with: the session's of the gyroflow plugin. get: mp.get_property_native
function M.session(get)
    local out = {}
    for _, name in ipairs({ "user-data/gyroflow/session" }) do
        local t = get(name)
        if type(t) == "table" then for k, v in pairs(t) do out[k] = v end end
    end
    return out
end

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
            -- The lens before it was a key of its own: corrected wherever it was stabilized, as recorded with it off
            if k == "lens" and type(bundle) == "table" and next(bundle) ~= nil then v = out.view == "off" and "as-recorded" or "corrected"
            elseif M.NEUTRAL[k] ~= nil then v = M.NEUTRAL[k] else v = session[k] end
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

-- What a render of a source is made with (the Library's Share, the batches): the bundle (a moment's own `own`, else
-- the source's saved view, else the app's `defaults`) made whole with the defaults, and the source's input (its
-- .afp.json's `source`, an old view's moved there first) over it. `doc`: the source's .afp.json as read (changed by
-- migrate_source). Action Footage Library works it out the same way (library/src/renders.rs `settings`), checked
-- against the same cases (tests/render_settings.json): the recipe it hashes is what the player renders with
function M.render_settings(doc, own, defaults)
    local bundle = M.complete(M.pick(own) or M.pick(type(doc) == "table" and doc.view or nil) or defaults, defaults)
    local rendered = {}
    for k, v in pairs(bundle) do rendered[k] = v end
    if type(doc) == "table" then M.migrate_source(doc) end
    for k, v in pairs(type(doc) == "table" and type(doc.source) == "table" and doc.source or {}) do rendered[k] = v end
    -- The lens under water where the source doesn't say: the defaults' (gyroflow.conf's underwater, #317), as it plays;
    -- auto (gyroflow-export's own) left out, so a recipe without it keeps its hash
    if rendered.underwater == nil and type(defaults) == "table" and (defaults.underwater == "yes" or defaults.underwater == "no") then
        rendered.underwater = defaults.underwater
    end
    -- The underwater colour (the source's): where the source doesn't say, the defaults' (uwcolor.conf, as it plays
    -- then: M.defaults). In the recipe only when it's on, so a recipe without it keeps the hash it had
    for _, k in ipairs(M.UW_KEYS) do
        if rendered[k] == nil and type(defaults) == "table" and valid(k, defaults[k]) then rendered[k] = defaults[k] end
    end
    if rendered.uw ~= true then for _, k in ipairs(M.UW_KEYS) do rendered[k] = nil end end
    return bundle, rendered
end

return M
