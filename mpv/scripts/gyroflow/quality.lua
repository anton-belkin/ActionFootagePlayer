-- When the stabilization doesn't keep up: playback pauses and offers lighter settings, and the one picked sticks (in
-- gyroflow.conf): a lower frame rate (fps_limit), a smaller resolution (resolution), a lighter edge fill (edges), the
-- color aberration correction skipped on this screen (chromatic_skip: the clip's transform keeps it, exports have it),
-- a lighter temporal denoise or none (temporal_denoise).
--
-- A laptop's GPU keeps up for its first half-minute or so, at its boost power, then slows to half at its sustained
-- power, and mpv drops frames to keep the picture with the sound. When it drops more than a tenth of them for more
-- than SLOW_SECONDS seconds in a row, playing at normal speed with no seek or set-up in the GRACE seconds before,
-- playback pauses and the choices come up (mp.input's list; with an mpv without it, a word on screen pointing to the
-- menu's Quality). Picking one sets it up and plays on; it asks again if that isn't enough. Closed
-- without a pick, it asks again on the next clip; "Keep playing as it is" not until the player starts again;
-- ask_when_slow=no never.
--
-- Loaded by main.lua, which gives it its options, says when the stabilization is on and what it renders, and applies
-- what's picked (init).

local mp = require 'mp'
local has_input, input = pcall(require, 'mp.input')
if not (has_input and input and input.select) then input = nil end

local M = {}

local SLOW_SECONDS = 2
-- After a seek, a clip loading, the chain set up again or playback going on after a pause, the drops while it gets
-- going don't count
local GRACE = 3
-- A second is slow when it dropped more than this share of the frames
local SLOW_SHARE = 0.1

local o = nil
local hooks = nil
local slow = 0
local last_drops = nil
local quiet_until = 0
-- Asked and not to be asked again until a setting changes (or, kept as it is, until the player starts again)
local asked = false
local keep = false

local function quiet()
    slow = 0
    quiet_until = mp.get_time() + GRACE
end

mp.register_event("playback-restart", quiet)
-- Each clip may ask once, though the list was closed without a pick
mp.register_event("file-loaded", function() asked = false end)
mp.observe_property("pause", "bool", function(_, paused) if not paused then quiet() end end)

-- The lighter settings than the ones it has, most effective first: { label, { { key, value }, ... } }. As measured on a
-- 15 W laptop at its sustained power with a 4K60 clip: 30 fps with the frame's edge spread out, and 15 fps with any
-- fill, play every frame. 30 fps with the sphere trail doesn't help: the trail is written whenever a GPU worker is
-- free, so it takes the time the skipped frames leave, so the rate comes down with the lighter fill. Writing it from
-- fewer of the frames (edge_fill_rate 50 or 25 %, or 10) didn't change that there: ~45-55 % of the frames shown at
-- 30 fps whatever the rate, the GPU workers at 75-90 ms a frame either way, so it isn't offered here. In those runs
-- 30 fps with the frame's edge spread out showed only ~40 % too, and 15 fps with the sphere every frame: so 30 fps
-- stays first, the lighter loss, and if it isn't enough it asks again and 15 fps is next. The resolution
-- only counts above 1080p: below that the input's scaling costs the same. The color fringes under water last: their
-- cost wasn't measured (the shader didn't compile under D3D11 on that laptop), and they're kept as recorded in
-- gyroflow.conf like the other picks
local function choices()
    local list = {}
    local function add(label, settings) table.insert(list, { label, settings }) end
    -- With the temporal denoise on and rendered bigger than 1080p, Auto first: at most 1080p while it's on, the
    -- screen's size when it's off (on the M1 Pro's XDR full screen 31 % of the frames dropped at its size, 4 % at
    -- 1080p); then the temporal denoise lighter: a little less clean, and the picture no less smooth
    local dn = hooks.denoise()
    local side = hooks.short_side()
    if dn and o.resolution ~= "auto" and side and side > 1080 then
        add("Auto size: at most 1080p while the temporal denoise is on", { { "resolution", "auto" } })
    end
    if dn == "full" then add("Lighter temporal denoise: 3 frames on each side", { { "temporal_denoise", "light" } }) end
    local fps = mp.get_property_number("container-fps") or 0
    local limit = o.fps_limit > 0 and o.fps_limit or fps
    local can30, can15 = fps > 31 and limit > 30, fps > 16 and limit > 15
    local edges = hooks.edges()
    if can30 and edges == "sphere" then
        add("Play at 30 fps, the edges filled with the frame's edge spread out", { { "edges", "smooth" }, { "fps_limit", "30" } })
    elseif can30 then
        add("Play at 30 fps", { { "fps_limit", "30" } })
    end
    if can15 then add("Play at 15 fps", { { "fps_limit", "15" } }) end
    if edges == "sphere" then add("Edges filled with the frame's edge spread out", { { "edges", "smooth" } }) end
    if edges ~= "off" then add("No edge fill: the background color", { { "edges", "off" } }) end
    if side and side > 1080 then add("Render at 1080p", { { "resolution", "1080" } }) end
    if hooks.chromatic() then add("Skip the color aberration correction on this screen", { { "chromatic_skip", "yes" } }) end
    if dn then add("No temporal denoise on this computer", { { "temporal_denoise", "off" } }) end
    return list
end

local function ask()
    asked = true
    local list = choices()
    mp.set_property_native("pause", true)
    if not input then
        mp.osd_message("Stabilization can't keep up: frames are being dropped.\n"
            .. "Right click > Quality for a lower frame rate or resolution", 8)
        return
    end
    local labels = {}
    for _, c in ipairs(list) do table.insert(labels, c[1]) end
    table.insert(labels, "Keep playing as it is")
    table.insert(labels, "Don't ask again")
    mp.msg.info("frames dropped for more than " .. SLOW_SECONDS .. " s: asking for lighter settings")
    input.select({
        prompt = "Stabilization can't keep up, frames are dropped. Play lighter:",
        items = labels,
        submit = function(i)
            local c = list[i]
            -- Playing on first: a set-up pauses through it and plays on once the first frame is through (main.lua)
            mp.set_property_native("pause", false)
            if c then
                for _, s in ipairs(c[2]) do hooks.set(s[1], s[2]) end
            elseif i == #list + 1 then
                keep = true
            else
                hooks.set("ask_when_slow", "no")
            end
        end,
    })
end

local function check()
    local drops = mp.get_property_number("frame-drop-count")
    local dropped = drops and last_drops and drops - last_drops or 0
    last_drops = drops
    if not o.ask_when_slow or asked or keep or not hooks.stabilizing() or mp.get_property_native("pause")
       or mp.get_property_native("seeking") or (mp.get_property_number("speed") or 1) > 1 or mp.get_time() < quiet_until then
        slow = 0
        return
    end
    local fps = mp.get_property_number("estimated-vf-fps") or mp.get_property_number("container-fps") or 30
    slow = dropped > math.max(2, fps * SLOW_SHARE) and slow + 1 or 0
    if slow > SLOW_SECONDS then
        slow = 0
        -- Nothing lighter to offer: it plays on as it can
        if #choices() > 0 then ask() end
    end
end

-- options: main.lua's (fps_limit, resolution, ask_when_slow); hooks: stabilizing() the stabilization is on,
-- short_side() the short side it renders at, edges() the edge fill, chromatic() the color fringes are taken out where they can be, denoise()
-- the temporal denoise while it plays (full, light, nil), set(key, value) applies and keeps a setting
function M.init(options, h)
    o, hooks = options, h
    mp.add_periodic_timer(1, check)
end

-- A setting changed: if it still doesn't keep up, it asks again
function M.changed()
    asked = false
    quiet()
end

return M
