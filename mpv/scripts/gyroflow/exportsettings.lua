-- An export's settings (docs/design/export-settings.md): one description for every file the player makes and for the
-- Action Footage Library's Share, whose renders the player makes too (export.lua's export-batch and photo-at). A video:
--   size     original (the default: as the export has always been), 2160, 1080, 720: the short side at most, never
--            enlarged (gyroflow-export --resolution)
--   encode   fast (the default: the encoder's real-time mode, the source's bit rate for the size) or quality (its
--            quality mode, half as much bit rate again) (gyroflow-export --encode)
--   quality  displayed (the default: the picture-quality settings this computer plays with, Performance & Quality's,
--            as every export took them so far), quickest (each of them at its lightest) or highest (each at its best)
-- A photo: size and quality, the same names; it has no encoder to choose.
-- The defaults are what was exported before these settings existed: nothing changes for who doesn't touch them.
-- Plain Lua, no mpv (registry.lua on package.path): tests/exportsettings_test.lua

local M = {}

-- Each setting's choices, in order: { value, label, note }, the first its default; the settings each kind of file has.
-- registry.json's (the workspace plugin's, beside this one), which the Library's Share dialog and its recipe read too
local reg = require 'registry'
M.CHOICES, M.DEFAULT = {}, {}
for _, s in ipairs(reg.export.settings) do
    M.CHOICES[s.key] = {}
    for _, c in ipairs(s.choices) do table.insert(M.CHOICES[s.key], { c.value, c.label, c.note }) end
    M.DEFAULT[s.key] = s.choices[1].value
end
M.KEYS = reg.export.kinds

local function valid(key, v)
    for _, c in ipairs(M.CHOICES[key] or {}) do if c[1] == v then return true end end
    return false
end

-- The settings of a kind (video, photo) from what's kept (a table, maybe partial or stale): each one valid, else its
-- default
function M.normalize(t, kind)
    local out = {}
    for _, k in ipairs(M.KEYS[kind] or M.KEYS.video) do
        local v = type(t) == "table" and t[k] ~= nil and tostring(t[k]) or nil
        out[k] = (v and valid(k, v)) and v or M.DEFAULT[k]
    end
    return out
end

function M.is_default(t, kind)
    local s = M.normalize(t, kind)
    for k, v in pairs(s) do if v ~= M.DEFAULT[k] then return false end end
    return true
end

-- How much temporal denoise the export takes (export.lua's cleanup_args `amount`): the clip's transform still says
-- whether; highest takes the full one even where this computer plays it lighter or not at all, quickest none
function M.amount(s, amount)
    if s and s.quality == "highest" then return "full" end
    if s and s.quality == "quickest" then return "off" end
    return amount
end

-- args (gyroflow-export's, as built so far, before the clip): the value after `key`, its index
local function find(args, key)
    for i = 1, #args do if args[i] == key then return args[i + 1], i end end
    return nil, nil
end
local function set(args, key, value)
    local _, i = find(args, key)
    if i then args[i + 1] = value else table.insert(args, key); table.insert(args, value) end
end
local function remove(args, key, values)
    local _, i = find(args, key)
    if i then for _ = 0, values do table.remove(args, i) end end
end

-- The radius a photo is cleaned up from at its best (gyroflow-export's still::MAX_RADIUS)
M.PHOTO_RADIUS_MAX = reg.export.photo_radius_max

-- The settings into gyroflow-export's arguments, for a video or a photo: the size and the encoder as its own options,
-- the quality preset as the options it stands for (each of the picture-quality settings playback has that a file can
-- have; fps and the size rendered on screen aren't: a file has every frame, at its size):
--   quickest  no temporal denoise, edges filled by the frame's edge spread out (not the trail), the water's color
--             fringes left as recorded, bilinear interpolation (both paths), a photo not cleaned up from its neighbours
--   highest   the full temporal denoise where the clip has it, edges by the trail, Lanczos (both paths, at any frame
--             rate: playback takes bicubic above 31 fps), a photo cleaned up from 7 frames each side, rendered by
--             gyroflow-export itself (not the player's frame on screen)
--   displayed nothing added: as it plays here
-- Returns the settings used
function M.apply(args, t, kind)
    local s = M.normalize(t, kind)
    if s.size ~= "original" then set(args, "--resolution", s.size) end
    if kind ~= "photo" and s.encode == "quality" then set(args, "--encode", "quality") end
    if s.quality == "quickest" then
        remove(args, "--denoise", 1)
        remove(args, "--denoise-strong", 0)
        if find(args, "--edges") == "sphere" then set(args, "--edges", "smooth") end
        set(args, "--chromatic", "no")
        set(args, "--gpu-interpolation", "bilinear")
        set(args, "--interpolation", "bilinear")
        if kind == "photo" then set(args, "--radius", "0") end
    elseif s.quality == "highest" then
        if find(args, "--denoise") then set(args, "--denoise", "full") end
        if find(args, "--edges") == "smooth" then set(args, "--edges", "sphere") end
        set(args, "--gpu-interpolation", "lanczos")
        set(args, "--interpolation", "lanczos")
        if kind == "photo" then
            set(args, "--radius", tostring(M.PHOTO_RADIUS_MAX))
            remove(args, "--image", 1)
        end
    end
    return s
end

-- Why the settings can't be had for this export, nil when they can: the 360° tab's exports (the whole sphere, the Flat
-- video along the recording) are their own renderer, with their own size (the Flat video's, the sphere's width)
function M.unavailable(key, export_kind)
    if M.DEFAULT[key] and (export_kind == "360" or export_kind == "flat") then return "Not for 360° exports: their size is set in the 360° tab" end
    return nil
end

return M
