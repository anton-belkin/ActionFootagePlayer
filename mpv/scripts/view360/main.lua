-- view360: a 360° clip shown as a view to look around in, turned by the mouse and the trackpad as crop mode is. An mpv
-- plugin: this folder goes into ~~/scripts/, next to the gyroflow plugin's folder (it uses its gestures.lua, and its
-- gyroflow-probe tells a 360° clip). docs/360-cameras.md has the design.
--
-- Two kinds of clip:
-- - Stitched (equirectangular, with spherical metadata: Spherical Video V2's sv3d, or V1's XMP, as Insta360 Studio,
--   DJI Studio and GoPro Player export them, and as the Theta and QooCam record them): the gyroflow plugin's probe
--   reads the metadata (user-data/gyroflow/spherical) and leaves such a clip unstabilized, decoded by mpv's own hwdec,
--   zero-copy. Its frames carry no stamp: the camera level and fixed, only the head turns.
-- - A 360 camera's own file (user-data/gyroflow/rig360: an Insta360 X5's .insv): the gyroflow plugin's 360 mode makes
--   the sphere in the camera's axes and stamps each frame with the camera in the world and the heading of each mode.
--   The view turns the sphere into the world, the horizon level, and points ahead along the chosen heading: along the
--   camera (its heading smoothed over a few seconds both ways, the default) or in the world (its heading at the start,
--   held). Switching is instant, paused too: every mode's heading is in the frame.
-- view360.glsl draws the view from each frame on screen at the screen's rate, so the head turns without waiting for the
-- video, paused as playing.
--
-- While a 360° clip plays: a drag, or two fingers on a trackpad, turns the head, what's under the mouse staying under
-- it; the wheel or a pinch zooms (the FOV); Alt+drag or turning two fingers rolls it; a double click or Backspace looks
-- ahead again. The gestures are gestures.lua's, crop mode's too; crop mode (c) on top of the view has them while it's
-- on. mpv's window can't be dragged by the video meanwhile.
--
-- Keys (rebind with `<key> script-binding view360/<name>` in input.conf):
--   Alt+v   toggle      the view on/off (off: the sphere as recorded, flat)
--   Alt+p   projection  auto (rectilinear up to 100°, the stereographic from 160°) -> rectilinear -> stereographic ->
--                       little planet
--   Alt+h   heading     what straight ahead follows, on a 360 camera's own file: along the camera <-> in the world
--
-- Script messages (`script-message-to view360 <message>`, what the side panel runs):
--   view360 yes|no|toggle                   the view on or off
--   view360-set yaw|pitch|roll|fov <deg>    the head, the FOV
--   view360-projection <name>|next          auto, rectilinear, stereographic, planet
--   view360-heading <mode>                  what the view follows: world, camera, travel, north (from the stamp; a
--                                           stitched clip has none, and holds still whatever it is)
--   view360-reset                           ahead again, at the projection's own FOV
-- The state is in user-data/view360/ for the panel: available (the clip is a sphere it shows), active, yaw, pitch,
-- roll, fov, fov_min, fov_max, projection, heading, stamped (the frames carry the camera and the headings: a 360
-- camera's own file), stereo, note (why a 360° clip isn't shown).

local mp = require 'mp'

-- gestures.lua is the gyroflow plugin's, in the folder beside this one
do
    local here = mp.get_script_directory() or "."
    local sep = package.config:sub(1, 1)
    local parent = here:match("^(.*)[/\\][^/\\]+$") or "."
    package.path = here .. sep .. "?.lua;" .. parent .. sep .. "gyroflow" .. sep .. "?.lua;" .. package.path
end
local view = require 'view'
local ok_gestures, gestures = pcall(require, 'gestures')
if not ok_gestures then
    mp.msg.warn("no gestures.lua in the gyroflow plugin's folder beside this one: the view can't be turned with the mouse")
    gestures = nil
end

local SHADER = mp.get_script_directory() and (mp.get_script_directory() .. "/view360.glsl")
local HEADING_INDEX = { world = 0, camera = 1, travel = 2, north = 3 }
local STEREO_INDEX = { mono = 0, ["top-bottom"] = 1, ["left-right"] = 2 }

-- The shader's in mpv's glsl-shaders from the start, ahead of the dlogm plugin's, which builds its list from them (as
-- the gyroflow plugin's are); it runs only while the view is on (view360_on). Only vo=gpu-next, gpu and libmpv run
-- shaders
local shader = false
do
    local f = SHADER and io.open(SHADER, "r")
    local vo = mp.get_property("vo") or ""
    if f then
        f:close()
        if vo == "" or vo:find("gpu", 1, true) or vo:find("libmpv", 1, true) then
            mp.commandv("change-list", "glsl-shaders", "append", SHADER)
            shader = true
        end
    end
end

-- The clip: its metadata (nil: not a sphere this shows), and the view on it
local sphere = nil
local active = false
local cur = view.default()
local start = view.default()
local heading = "world"
local note = nil
local crop_on = false
local input = nil

-- The video's area in the window, less the margins (the side panel's), in OSD pixels: while the view is on, the
-- video's aspect is this area's, so the video fills it
local function area()
    local d = mp.get_property_native("osd-dimensions")
    if not d or d.w <= 0 or d.h <= 0 then return nil end
    local function ratio(name) return mp.get_property_number("video-margin-ratio-" .. name) or 0 end
    local x0, x1 = d.w * ratio("left"), d.w * (1 - ratio("right"))
    local y0, y1 = d.h * ratio("top"), d.h * (1 - ratio("bottom"))
    if x1 <= x0 or y1 <= y0 then return nil end
    return { x = x0, y = y0, w = x1 - x0, h = y1 - y0 }
end

local function publish()
    local r = view.FOV[cur.projection]
    mp.set_property_native("user-data/view360", {
        available = sphere ~= nil, active = active, yaw = cur.yaw, pitch = cur.pitch, roll = cur.roll, fov = cur.fov,
        fov_min = r.min, fov_max = r.max, projection = cur.projection, heading = heading, stamped = sphere ~= nil and sphere.stamped == true,
        stereo = sphere and sphere.stereo or "mono", note = note or false,
    })
end

-- The shader's parameters, with whatever else glsl-shader-opts has
local opts_set = nil
local function apply()
    local opts = mp.get_property_native("glsl-shader-opts") or {}
    for k in pairs(opts) do if k:match("^view360_") or k:match("^afp_input_") then opts[k] = nil end end
    local key = "off"
    if active then
        local mine = view.shader_opts(cur, STEREO_INDEX[sphere.stereo] or 0, HEADING_INDEX[heading])
        mine.view360_on = "1"
        -- For the app's mpv (no shader reads them): the last gesture's time (--macos-latency-log) and number, after
        -- which its VO turns the view by the gestures itself, until they come here (tools/mpv-patches/0012)
        if input and input.input_time then mine.afp_input_time = string.format("%.6f", input.input_time) end
        if input and input:acting() then mine.afp_input_seq = tostring(input.input_seq) end
        local parts = {}
        for k, v in pairs(mine) do opts[k] = v; table.insert(parts, k .. "=" .. v) end
        table.sort(parts)
        key = table.concat(parts, ",")
    else
        opts.view360_on = "0"
    end
    if key ~= opts_set then
        opts_set = key
        mp.set_property_native("glsl-shader-opts", opts)
    end
    publish()
end

-- The gestures' changes applied once the messages waiting are handled: setting glsl-shader-opts waits for the VO
-- (up to a refresh), and the gestures come faster than that
local apply_pending = false
local function apply_soon()
    if apply_pending then return end
    apply_pending = true
    mp.add_timeout(0, function()
        apply_pending = false
        apply()
    end)
end

-- The head turned so that what's at x - dx, y - dy (OSD pixels; the area's middle when nil) comes to x, y
local function pan(dx, dy, x, y)
    local a = area()
    if not active or not a then return end
    local cx, cy, half = a.x + a.w / 2, a.y + a.h / 2, a.h / 2
    x, y = x or cx, y or cy
    local yaw, pitch = view.pan(cur, (x - dx - cx) / half, (y - dy - cy) / half, (x - cx) / half, (y - cy) / half)
    cur.yaw, cur.pitch = yaw, pitch
    apply_soon()
end

local function zoom(factor)
    if not active or not factor or factor <= 0 then return end
    cur.fov = view.clamp_fov(cur.projection, cur.fov / factor)
    apply_soon()
end

local function roll(degrees)
    if not active then return end
    cur.roll = view.wrap(cur.roll + degrees)
    apply_soon()
end

-- Ahead again (where the clip's metadata says to look first), at the projection's own FOV
local function reset()
    if not sphere then return end
    local projection = cur.projection
    cur = view.copy(start)
    cur.projection = projection
    cur.fov = view.FOV[projection].default
    if projection == "planet" then cur.pitch = 0 end
    apply()
end

local function set_projection(name)
    if not view.FOV[name] then return end
    local was = cur.projection
    cur.projection = name
    -- The little planet looks down from where the head is: its own pitch and FOV, and back
    if (name == "planet") ~= (was == "planet") then
        cur.pitch = 0
        cur.fov = view.FOV[name].default
    else
        cur.fov = view.clamp_fov(name, cur.fov)
    end
    apply()
    if active then mp.osd_message("360°: " .. ({ auto = "auto projection", rectilinear = "rectilinear", stereographic = "stereographic",
                                                  planet = "little planet" })[name], 1.5) end
end

if gestures then
    input = gestures.new({
        prefix = "view360", pan = pan, zoom = zoom, roll = roll, reset = reset,
        width = function() local a = area(); return a and a.w end,
    })
end

-- The video's aspect the area's (video-aspect-override, as whole pixels), so mpv puts the view over all of it; and the
-- window free to take any shape (keepaspect-window), which then changes the aspect. saved: both as they were
local saved = nil
local aspect_set = nil
local function fit()
    local a = area()
    if not active or not a then return end
    local value = string.format("%d:%d", math.floor(a.w + 0.5), math.floor(a.h + 0.5))
    if value == aspect_set then return end
    aspect_set = value
    mp.set_property("file-local-options/video-aspect-override", value)
end

-- The view on or off. ending: the file's ending, and mpv puts its file-local options back itself
local function set_active(on, ending)
    on = on and sphere ~= nil and shader
    if on == active then return end
    active = on
    aspect_set = nil
    if on then
        saved = { aspect = mp.get_property("video-aspect-override"), window = mp.get_property("keepaspect-window") }
        mp.set_property("file-local-options/keepaspect-window", "no")
        fit()
        if input then
            input:set(true)
            input:pause(crop_on)
        end
    else
        if input then input:set(false) end
        if not ending and saved then
            mp.set_property("file-local-options/video-aspect-override", saved.aspect or "-1")
            mp.set_property("file-local-options/keepaspect-window", saved.window or "yes")
        end
        saved = nil
    end
    apply()
end

-- A clip is a sphere this shows when the gyroflow plugin's probe says so (before this hook: its on_load is at 50)
mp.add_hook("on_load", 60, function()
    sphere, note, heading = nil, nil, "world"
    local s = mp.get_property_native("user-data/gyroflow/spherical")
    local rig = mp.get_property_native("user-data/gyroflow/rig360")
    if type(rig) == "table" then
        -- The plugin's sphere, stamped: along the camera unless chosen otherwise
        if shader then
            sphere = { stereo = "mono", stamped = true }
            heading = "camera"
        else
            note = "The 360 view needs mpv's gpu-next, gpu or libmpv video output"
            mp.msg.warn(note)
        end
    elseif type(s) == "table" then
        if s.projection ~= "equirect" then
            note = "A 360° clip in a projection not shown yet (" .. tostring(s.projection) .. ")"
        elseif s.stereo ~= nil and not STEREO_INDEX[s.stereo] then
            note = "A 360° clip with a stereo layout not shown yet (" .. tostring(s.stereo) .. ")"
        elseif not shader then
            note = "The 360 view needs mpv's gpu-next, gpu or libmpv video output"
        else
            sphere = { stereo = s.stereo or "mono" }
            if type(s.bounds) == "table" then
                mp.msg.warn("a partial sphere (its bounds): shown as if it were whole")
            end
        end
        if note then mp.msg.warn(note) end
    end
    local projection = cur.projection
    start = view.default(projection)
    local iv = sphere and type(s) == "table" and type(s.initial_view) == "table" and s.initial_view
    if iv then
        start.yaw = view.wrap(tonumber(iv.heading) or 0)
        start.pitch = math.max(-view.PITCH_MAX, math.min(view.PITCH_MAX, tonumber(iv.pitch) or 0))
        start.roll = view.wrap(tonumber(iv.roll) or 0)
        if projection == "planet" then start.pitch = 0 end
    end
    cur = view.copy(start)
    -- Set up before the first frame shows
    set_active(sphere ~= nil)
    if not sphere then apply() end
end)

mp.register_event("file-loaded", function()
    if active then mp.osd_message("360°: drag to look around, wheel or pinch to zoom, double click to look ahead", 3) end
end)

mp.register_event("end-file", function()
    -- The hint isn't the next clip's
    if active then mp.osd_message("", 0) end
    set_active(false, true)
    sphere, note = nil, nil
    publish()
end)

-- The window or the side panel changing the video's area: the view's size
for _, prop in ipairs({ "osd-dimensions", "video-margin-ratio-right", "video-margin-ratio-left", "video-margin-ratio-top",
                        "video-margin-ratio-bottom" }) do
    mp.observe_property(prop, "native", function() if active then fit() end end)
end

-- Crop mode on top of the view has the gestures while it's on
mp.observe_property("user-data/gyroflow/crop", "native", function(_, c)
    crop_on = type(c) == "table" and c.active == true
    if input and active then
        input:pause(crop_on)
        -- The VO turns the view by the gestures only while the view has them
        apply()
    end
end)

mp.add_key_binding("Alt+v", "toggle", function()
    if not sphere then return mp.osd_message(note or "Not a 360° clip", 2) end
    set_active(not active)
    mp.osd_message(active and "360° view" or "360°: as recorded", 1.5)
end)

mp.add_key_binding("Alt+p", "projection", function()
    if not active then return end
    local i = view.PROJECTION_INDEX[cur.projection] or 0
    set_projection(view.PROJECTIONS[(i + 1) % #view.PROJECTIONS + 1])
end)

-- The stamped modes there are so far (travel and north: step 5)
local HEADINGS = { "camera", "world" }
local HEADING_LABELS = { camera = "along the camera", world = "in the world", travel = "along the way", north = "north" }

mp.add_key_binding("Alt+h", "heading", function()
    if not active then return end
    if not sphere.stamped then return mp.osd_message("360°: this clip has no camera motion to follow", 2) end
    heading = heading == HEADINGS[1] and HEADINGS[2] or HEADINGS[1]
    apply()
    mp.osd_message("360°: " .. HEADING_LABELS[heading], 1.5)
end)

mp.register_script_message("view360", function(v)
    if v == "toggle" then set_active(not active) elseif v == "yes" or v == "no" then set_active(v == "yes") end
end)

mp.register_script_message("view360-set", function(key, value)
    value = tonumber(value)
    if not active or not value then return end
    if key == "yaw" then cur.yaw = view.wrap(value)
    elseif key == "pitch" then cur.pitch = math.max(-view.PITCH_MAX, math.min(view.PITCH_MAX, value))
    elseif key == "roll" then cur.roll = view.wrap(value)
    elseif key == "fov" then cur.fov = view.clamp_fov(cur.projection, value)
    else return end
    apply()
end)

mp.register_script_message("view360-projection", function(name)
    if name == "next" then
        local i = view.PROJECTION_INDEX[cur.projection] or 0
        name = view.PROJECTIONS[(i + 1) % #view.PROJECTIONS + 1]
    end
    set_projection(name)
end)

mp.register_script_message("view360-heading", function(mode)
    if not HEADING_INDEX[mode] then return end
    heading = mode
    apply()
end)

mp.register_script_message("view360-reset", reset)

publish()
