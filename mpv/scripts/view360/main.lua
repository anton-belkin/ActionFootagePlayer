-- view360: a 360° clip shown as a view to look around in, turned by the mouse and the trackpad as crop mode is. An mpv
-- plugin: this folder goes into ~~/scripts/, next to the gyroflow plugin's folder (it uses its gestures.lua, and its
-- gyroflow-probe tells a 360° clip). docs/360-cameras.md has the design.
--
-- Two kinds of clip:
-- - Stitched (equirectangular, with spherical metadata: Spherical Video V2's sv3d, or V1's XMP, as Insta360 Studio,
--   DJI Studio and GoPro Player export them, and as the Theta and QooCam record them): the gyroflow plugin's probe
--   reads the metadata (user-data/gyroflow/spherical) and leaves such a clip unstabilized, decoded by mpv's own hwdec,
--   zero-copy. Its frames carry no stamp: the camera level and fixed, only the head turns.
-- - A 360 camera's own file (user-data/gyroflow/rig360: an Insta360 X5's .insv): the gyroflow
--   plugin's 360 mode makes the sphere in the camera's axes and stamps each frame with the camera in the world and the
--   heading of each mode. The view turns the sphere into the world, the horizon level, and points ahead along the
--   chosen heading: along the way you move (the GPS course, the gyro tied to it; standing, the way it went held against
--   the camera), along the camera (its heading smoothed over a few seconds both ways), north (the camera's compass, or
--   the GPS's true north), or in the world (its heading at the start, held: in the real world where north or GPS say
--   how the gyro drifts). Along the way when the clip has it, else along the camera. Switching is instant, paused too:
--   every mode's heading is in the frame. Which modes a clip has, how sure they are and why not comes from
--   user-data/gyroflow/headings360 (gyroflow-probe --headings, a second or so after the clip opens).
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
--   Alt+h   heading     what straight ahead follows, on a 360 camera's own file: along the way -> along the camera ->
--                       north -> in the world, the ones the clip has
--   Alt+r   record      record where you look while the clip plays (a take), or stop (docs/360-reframe.md)
--   Alt+R   play-recording  the view follows the recording, or your gestures again (Alt+Shift+r)
--
-- Script messages (`script-message-to view360 <message>`, what the side panel runs):
--   view360 yes|no|toggle                   the view on or off
--   view360-set yaw|pitch|roll|fov <deg>    the head, the FOV
--   view360-projection <name>|next          auto, rectilinear, stereographic, planet
--   view360-heading <mode>                  what the view follows: world, camera, travel, north (from the stamp; a
--                                           stitched clip has none, and holds still whatever it is)
--   view360-reset                           ahead again, at the projection's own FOV
--   view360-record [toggle|start|stop]      a take of the recording
--   view360-play-recording [toggle|yes|no]  the view following the recording
--   view360-undo                            the last take taken back (up to 20, for as long as the clip is open)
--   view360-clear                           nothing recorded (the panel asks first)
--   view360-frame 16:9|9:16|1:1, view360-size 1080p|4K   the flat export's, kept with the recording
--   view360-tidy turns off|light|medium|strong|toggle, view360-tidy level yes|no|toggle   how the recording is tidied
--                                           (smoothed, leveled), kept with it: the preview and the export both
-- The state is in user-data/view360/ for the panel: available (the clip is a sphere it shows), active, yaw, pitch,
-- roll, fov, fov_min, fov_max, projection, heading, stamped (the frames carry the camera and the headings: a 360
-- camera's own file), stereo, note (why a 360° clip isn't shown), modes (per heading mode: available, why not,
-- confidence, and what it's from: false until the clip's been read), heading_confidence (the chosen mode's, now: 0-1,
-- or false), recording (the flat video recorded as you look around: taking, seconds, spans, parts, follows, playing,
-- frame, size, undo; can, and why not when it can't).
--
-- The recording (recording.lua) is kept in the clip's own file, <clip>.afp.json, under view360.recording (the
-- workspace plugin's sidecar.lua). Each frame shown during a take records the head relative to what the view follows,
-- and the projection; pausing, seeking, the clip's end or leaving the view end a take. What the view follows is the
-- recording's: locked during a take and while the recording plays, and a take under another is refused.

local mp = require 'mp'

-- gestures.lua is the gyroflow plugin's, in the folder beside this one
do
    local here = mp.get_script_directory() or "."
    local sep = package.config:sub(1, 1)
    local parent = here:match("^(.*)[/\\][^/\\]+$") or "."
    package.path = here .. sep .. "?.lua;" .. parent .. sep .. "gyroflow" .. sep .. "?.lua;" .. parent .. sep .. "workspace"
                   .. sep .. "?.lua;" .. package.path
end
local view = require 'view'
local recording = require 'recording'
-- The clip's own file: the workspace plugin's sidecar.lua, in the folder beside this one
local sidecar = nil
do
    local ok_utils, utils = pcall(require, 'mp.utils')
    local ok_sidecar, sidecar_lib = pcall(require, 'sidecar')
    if ok_utils and ok_sidecar then
        local windows = (mp.get_property("platform") or "") == "windows" or package.config:sub(1, 1) == "\\"
        sidecar = sidecar_lib.new({ parse_json = function(text) return (utils.parse_json(text)) end, format_json = utils.format_json,
                                    windows = windows })
    else
        mp.msg.warn("no sidecar.lua in the workspace plugin's folder beside this one: the 360 recording isn't kept")
    end
end
local ok_gestures, gestures = pcall(require, 'gestures')
if not ok_gestures then
    mp.msg.warn("no gestures.lua in the gyroflow plugin's folder beside this one: the view can't be turned with the mouse")
    gestures = nil
end

local SHADER = mp.get_script_directory() and (mp.get_script_directory() .. "/view360.glsl")
-- gyroflow-probe, in the gyroflow plugin's folder beside this one: the recording tidied (--reframe) for the preview
local PROBE = nil
do
    local here = mp.get_script_directory()
    local sep = package.config:sub(1, 1)
    local parent = here and here:match("^(.*)[/\\][^/\\]+$")
    local windows = (mp.get_property("platform") or "") == "windows" or sep == "\\"
    if parent then PROBE = parent .. sep .. "gyroflow" .. sep .. "gyroflow-probe" .. (windows and ".exe" or "") end
end
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
-- The heading modes a stamped clip has (from the gyroflow plugin's headings360), and whether the viewer chose one
local modes = false
local seconds = nil
local heading_chosen = false
local confidence_now = false
local note = nil
local crop_on = false
local input = nil
-- The recording: the clip's file it's kept for (nil: none can be, and why), whether the view follows it now
local rec = recording.new()
local rec_clip, rec_why = nil, nil
local rec_playing = false
-- The recording tidied as its tidy says (gyroflow-probe --reframe: reframe.rs, as the export does it), what the view
-- follows while the recording plays; nil while it's worked out (the samples as recorded meanwhile) or not needed
local tidied = nil
local tidy_run, tidying = 0, false
-- The smoothing chosen before it was turned off, to turn it on again with
local turns_before = "medium"
local rec_overlay = mp.create_osd_overlay and mp.create_osd_overlay("ass-events") or nil
local rec_shown = nil
local HEADING_LABELS = { camera = "along the camera", world = "in the world", travel = "along the way", north = "north" }
local FOLLOWS_NAMES = { camera = "the camera", world = "the world", travel = "the way", north = "north" }

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

local function rec_state()
    local spans = {}
    for _, sp in ipairs(rec:spans()) do table.insert(spans, { sp.from, sp.to }) end
    return {
        can = rec_clip ~= nil and sphere ~= nil, why = rec_why or false, taking = rec:taking(), seconds = rec:take_seconds(),
        spans = spans, parts = #rec.parts, follows = rec.follows or false, playing = rec_playing, frame = rec.frame,
        size = rec.size, undo = rec:undoable(), tidy = { turns = rec.tidy.turns, level = rec.tidy.level }, tidying = tidying,
    }
end

local function publish()
    local r = view.FOV[cur.projection]
    mp.set_property_native("user-data/view360", {
        available = sphere ~= nil, active = active, yaw = cur.yaw, pitch = cur.pitch, roll = cur.roll, fov = cur.fov,
        fov_min = r.min, fov_max = r.max, projection = cur.projection, heading = heading, stamped = sphere ~= nil and sphere.stamped == true,
        stereo = sphere and sphere.stereo or "mono", note = note or false, modes = modes, heading_confidence = confidence_now,
        recording = rec_state(),
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

-- A gesture while the recording plays: the head is yours again, from where the recording had it
local function yours()
    if not rec_playing then return end
    rec_playing = false
    mp.osd_message("Your view: the recording is paused", 2)
    publish()
end

-- The head turned so that what's at x - dx, y - dy (OSD pixels; the area's middle when nil) comes to x, y
local function pan(dx, dy, x, y)
    local a = area()
    if not active or not a then return end
    yours()
    local cx, cy, half = a.x + a.w / 2, a.y + a.h / 2, a.h / 2
    x, y = x or cx, y or cy
    local yaw, pitch = view.pan(cur, (x - dx - cx) / half, (y - dy - cy) / half, (x - cx) / half, (y - cy) / half)
    cur.yaw, cur.pitch = yaw, pitch
    apply_soon()
end

local function zoom(factor)
    if not active or not factor or factor <= 0 then return end
    yours()
    cur.fov = view.clamp_fov(cur.projection, cur.fov / factor)
    apply_soon()
end

local function roll(degrees)
    if not active then return end
    yours()
    cur.roll = view.wrap(cur.roll + degrees)
    apply_soon()
end

-- Ahead again (where the clip's metadata says to look first), at the projection's own FOV
local function reset()
    if not sphere then return end
    yours()
    local projection = cur.projection
    cur = view.copy(start)
    cur.projection = projection
    cur.fov = view.FOV[projection].default
    if projection == "planet" then cur.pitch = 0 end
    apply()
end

local function set_projection(name)
    if not view.FOV[name] then return end
    yours()
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

-- The recording ----------------------------------------------------------------------------------------------------

local function clock(s)
    s = math.max(0, math.floor(s + 0.5))
    return string.format("%d:%02d", math.floor(s / 60), s % 60)
end

-- The red dot and the time recorded so far, in the video's top right corner (mpv's messages are top left)
local function draw_rec()
    if not rec_overlay then return end
    local a = area()
    if not rec:taking() or not a then
        if rec_shown then rec_overlay:remove(); rec_shown = nil end
        return
    end
    local text = clock(rec:take_seconds())
    local d = mp.get_property_native("osd-dimensions") or {}
    local key = text .. ":" .. a.x .. ":" .. a.y .. ":" .. tostring(d.w)
    if key == rec_shown then return end
    rec_shown = key
    local s = mp.get_property_number("display-hidpi-scale") or 1
    rec_overlay.res_x, rec_overlay.res_y = d.w or 0, d.h or 0
    rec_overlay.data = string.format("{\\an9\\pos(%d,%d)\\bord%.1f\\shad0\\fs%d\\1c&H3030E8&}●{\\1c&HFFFFFF&\\fs%d}  %s",
                                     math.floor(a.x + a.w - 18 * s), math.floor(a.y + 14 * s), 1.5 * s, math.floor(22 * s),
                                     math.floor(18 * s), text)
    rec_overlay:update()
end

local function save_recording()
    if not sidecar or not rec_clip then return end
    local t = rec:to_table()
    local ok, err = sidecar.update(rec_clip, function(d)
        local v = type(d.view360) == "table" and d.view360 or {}
        if t == nil and v.recording == nil then return false end
        v.recording = t
        d.view360 = next(v) ~= nil and v or nil
    end)
    if not ok then
        mp.msg.warn("couldn't keep the 360 recording in " .. sidecar.path(rec_clip) .. ": " .. tostring(err))
        mp.osd_message("The recording couldn't be saved: " .. tostring(err), 4)
    end
end

-- The view at t from the recording, while it plays: tidied when that's worked out
local function follow_recording(t)
    if not rec_playing or not active then return end
    t = t or mp.get_property_number("time-pos")
    local h = t and (tidied or rec):head(t)
    if not h then return end
    cur.yaw, cur.pitch, cur.roll = view.wrap(h.yaw), math.max(-view.PITCH_MAX, math.min(view.PITCH_MAX, h.pitch)), view.wrap(h.roll)
    cur.projection = view.FOV[h.projection] and h.projection or "auto"
    cur.fov = view.clamp_fov(cur.projection, h.fov)
    apply()
end

-- The recording tidied again (after a take, an undo, a change of how): gyroflow-probe reads the clip's file, so it's
-- saved first. The samples as recorded play meanwhile; an answer for a recording since changed is dropped
local function retidy()
    tidy_run = tidy_run + 1
    tidied, tidying = nil, false
    if rec:empty() or rec:untidied() or not rec_clip or not PROBE or not mp.command_native_async then return end
    local run = tidy_run
    tidying = true
    mp.command_native_async({ name = "subprocess", args = { PROBE, "--reframe", rec_clip }, capture_stdout = true, playback_only = false },
        function(_, r)
            if run ~= tidy_run then return end
            tidying = false
            local ok_utils, utils = pcall(require, 'mp.utils')
            local doc = ok_utils and r and r.status == 0 and utils.parse_json(r.stdout or "") or nil
            if type(doc) ~= "table" or doc.error then
                mp.msg.warn("the 360 recording couldn't be tidied: " .. tostring(type(doc) == "table" and doc.error or (r and r.error_string) or "no answer"))
            else
                local t = recording.new():load(doc)
                if not t:empty() then tidied = t end
            end
            follow_recording()
            publish()
        end)
end

local function set_playing(on, quiet)
    on = on and active and not rec:empty() and not rec:taking()
    if on == rec_playing then return publish() end
    rec_playing = on
    if on then
        -- The recording's own heading: the head was recorded relative to it
        if rec.follows and heading ~= rec.follows then heading = rec.follows end
        follow_recording()
        if not quiet then mp.osd_message("Playing the recording", 1.5) end
    elseif not quiet then
        mp.osd_message("Your view: the recording is paused", 2)
    end
    publish()
end

local function stop_take(why)
    if not rec:taking() then return end
    local part = rec:stop()
    draw_rec()
    if part then
        save_recording()
        retidy()
        mp.osd_message(string.format("Recorded %s–%s%s", clock(part.from), clock(part.to), why and (" (" .. why .. ")") or ""), 2.5)
        -- What was recorded, shown
        set_playing(true, true)
    else
        mp.osd_message("Recording stopped: too short to keep", 2)
    end
    publish()
end

local function add_frame(t)
    if not rec:taking() or not t then return end
    local s = rec.take.samples
    local last = s[#s]
    -- A jump forward of more than a second is a seek the event didn't tell
    if last and t > last.t + 1 then return stop_take("a seek") end
    local before = last and math.floor(rec:take_seconds()) or -1
    if not rec:add(t, cur.yaw, cur.pitch, cur.roll, cur.fov, cur.projection) then return stop_take("a seek") end
    draw_rec()
    if math.floor(rec:take_seconds()) ~= before then publish() end
end

local function start_take()
    if rec:taking() then return end
    if not active then return mp.osd_message("Recording: turn the 360 view on first (Alt+v)", 2) end
    if not rec_clip then return mp.osd_message("Recording: " .. (rec_why or "this clip can't keep one"), 3) end
    local ok, other = rec:start(heading)
    if not ok then
        return mp.osd_message(string.format("The recording follows %s: switch to it, or clear the recording",
                                            FOLLOWS_NAMES[other] or tostring(other)), 4)
    end
    rec_playing = false
    if mp.get_property_native("pause") then mp.set_property_native("pause", false) end
    add_frame(mp.get_property_number("time-pos"))
    mp.osd_message("Recording: look around. Alt+r stops", 2)
    draw_rec()
    publish()
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

-- Turning the head shows sooner (docs/360-implementation-plan.md, step 6): a video frame is drawn a refresh before its
-- time rather than 50 ms ahead (video-timing-offset), so the VO is free to redraw the view meanwhile and the frame has
-- the newest head; and in a window the frames are shown as they come, not at a vsync first (the app's mpv,
-- macos-display-sync: macOS composites a window at its refresh anyway; in full screen it would tear)
local function pace()
    if not active then return end
    local fps = mp.get_property_number("display-fps")
    local offset = (fps and fps > 0) and math.max(0.008, math.min(0.05, 1 / fps + 0.004)) or 0.02
    mp.set_property("file-local-options/video-timing-offset", string.format("%.4f", offset))
end

-- The view on or off. ending: the file's ending, and mpv puts its file-local options back itself
local function set_active(on, ending)
    on = on and sphere ~= nil and shader
    if on == active then return end
    active = on
    aspect_set = nil
    if on then
        saved = { aspect = mp.get_property("video-aspect-override"), window = mp.get_property("keepaspect-window"),
                  timing = mp.get_property("video-timing-offset"), sync = mp.get_property("macos-display-sync") }
        mp.set_property("file-local-options/keepaspect-window", "no")
        if saved.sync then mp.set_property("file-local-options/macos-display-sync", "fullscreen") end
        fit()
        pace()
        if input then
            input:set(true)
            input:pause(crop_on)
        end
    else
        stop_take("left the 360 view")
        rec_playing = false
        if input then input:set(false) end
        if not ending and saved then
            mp.set_property("file-local-options/video-aspect-override", saved.aspect or "-1")
            mp.set_property("file-local-options/keepaspect-window", saved.window or "yes")
            if saved.timing then mp.set_property("file-local-options/video-timing-offset", saved.timing) end
            if saved.sync then mp.set_property("file-local-options/macos-display-sync", saved.sync) end
        end
        saved = nil
    end
    apply()
end

-- A clip is a sphere this shows when the gyroflow plugin's probe says so (before this hook: its on_load is at 50)
mp.add_hook("on_load", 60, function()
    sphere, note, heading = nil, nil, "world"
    modes, seconds, heading_chosen, confidence_now = false, nil, false, false
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
    -- The clip's recording: a local file of its own (not a moment playing, which has a time of its own)
    rec:load(nil)
    rec_playing, rec_clip, rec_why = false, nil, nil
    if sphere then
        local path = mp.get_property("path")
        local ws = mp.get_property_native("user-data/workspace")
        if not sidecar then rec_why = "the workspace plugin's sidecar.lua isn't installed"
        elseif not path or path:find("://", 1, true) then rec_why = "not a local file"
        elseif type(ws) == "table" and ws.path == path and ws.redirected then rec_why = "a moment: open the clip itself to record"
        else
            rec_clip = path
            local doc, _, problem = sidecar.read(path)
            if problem then mp.msg.warn(problem) end
            local v = type(doc) == "table" and type(doc.view360) == "table" and doc.view360 or {}
            rec:load(v.recording)
        end
    end
    retidy()
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
    stop_take("the end of the clip")
    set_active(false, true)
    sphere, note = nil, nil
    rec:load(nil)
    rec_clip, rec_playing = nil, false
    publish()
end)

-- The window or the side panel changing the video's area: the view's size
for _, prop in ipairs({ "osd-dimensions", "video-margin-ratio-right", "video-margin-ratio-left", "video-margin-ratio-top",
                        "video-margin-ratio-bottom" }) do
    mp.observe_property(prop, "native", function() if active then fit() end end)
end

mp.observe_property("display-fps", "native", function() pace() end)

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

-- The heading modes in the order Alt+h goes through them: along the way first (for a 360 camera what matters most:
-- on a stick or a helmet the camera's own direction means little), then the camera's, north, the world
local HEADINGS = { "travel", "camera", "north", "world" }

-- How sure the chosen mode is now (the probe's confidence per second), for the panel
local function update_confidence()
    local c = false
    local col = heading == "travel" and 1 or heading == "north" and 2 or nil
    local t = mp.get_property_number("time-pos")
    if col and seconds and t then
        local row = seconds[math.max(1, math.min(#seconds, math.floor(t) + 1))]
        c = type(row) == "table" and tonumber(row[col]) or false
    end
    if c ~= confidence_now then
        confidence_now = c
        publish()
    end
end

local function available(mode)
    if mode == "camera" or mode == "world" then return true end
    return type(modes) == "table" and type(modes[mode]) == "table" and modes[mode].available == true
end

local function set_heading(mode, by_viewer)
    if not HEADING_INDEX[mode] then return false end
    -- The recording's own while a take records or the recording plays (re-expressing it in another is step 2 of
    -- docs/360-reframe.md)
    if mode ~= heading and (rec:taking() or rec_playing) then
        mp.osd_message(string.format("%s: following %s. %s", rec:taking() and "Recording" or "Playing the recording",
                                     FOLLOWS_NAMES[heading] or heading, rec:taking() and "Alt+r stops" or "A drag gives you the view"), 2.5)
        return false
    end
    if sphere and sphere.stamped and not available(mode) then
        local why = type(modes) == "table" and type(modes[mode]) == "table" and modes[mode].why
        mp.osd_message("360°: " .. HEADING_LABELS[mode] .. ": " .. (why or (modes and "not in this clip" or "reading the clip…")), 3)
        return false
    end
    heading = mode
    if by_viewer then heading_chosen = true end
    update_confidence()
    apply()
    return true
end

-- What the clip has for the modes: the gyroflow plugin's probe, a second or so after the clip opens. Along the way
-- becomes the mode if it's there and nothing else was chosen meanwhile
mp.observe_property("user-data/gyroflow/headings360", "native", function(_, h)
    if type(h) ~= "table" or not (sphere and sphere.stamped) then
        if modes then modes, seconds = false, nil; publish() end
        return
    end
    local function mode(m, fallback)
        local v = type(h[m]) == "table" and h[m] or {}
        return { available = v.available == true, why = v.why or (h.error and tostring(h.error)) or (v.available ~= true and fallback) or false,
                 confidence = tonumber(v.confidence) or false, source = v.source or false, sigma = tonumber(v.sigma_deg) or false,
                 against_gps = tonumber(v.against_gps_deg) or false }
    end
    local world = type(h.world) == "table" and h.world or {}
    modes = {
        travel = mode("travel", "no GPS in this clip"),
        north = mode("north", "no compass or GPS in this clip"),
        camera = { available = true },
        world = { available = true, held_by = world.held_by or false, drift = tonumber(world.drift_deg_min) or false },
    }
    seconds = type(h.seconds) == "table" and h.seconds or nil
    if not heading_chosen and modes.travel.available and not rec:taking() and not rec_playing then heading = "travel" end
    update_confidence()
    apply()
end)

mp.observe_property("time-pos", "number", function(_, t)
    if active and (heading == "travel" or heading == "north") then update_confidence() end
    -- Each frame shown: recorded during a take, or the view from the recording while it plays
    if rec:taking() then add_frame(t) elseif rec_playing then follow_recording(t) end
end)

-- What ends a take: pausing, seeking, the clip's end
mp.observe_property("pause", "bool", function(_, paused) if paused then stop_take() end end)
mp.observe_property("eof-reached", "bool", function(_, eof) if eof then stop_take("the end of the clip") end end)
mp.register_event("seek", function() stop_take("a seek") end)
mp.observe_property("osd-dimensions", "native", function() draw_rec() end)

local function toggle_take()
    if rec:taking() then stop_take() else start_take() end
end

local function toggle_playing()
    if not active then return end
    if rec:empty() then return mp.osd_message("Nothing recorded yet: Alt+r records where you look", 2) end
    set_playing(not rec_playing)
end

mp.add_key_binding("Alt+r", "record", toggle_take)
mp.add_key_binding("Alt+R", "play-recording", toggle_playing)

mp.register_script_message("view360-record", function(how)
    if how == "start" then start_take() elseif how == "stop" then stop_take() else toggle_take() end
end)

mp.register_script_message("view360-play-recording", function(how)
    if how == "yes" or how == "no" then
        if how == "yes" and rec:empty() then return toggle_playing() end
        set_playing(how == "yes")
    else
        toggle_playing()
    end
end)

mp.register_script_message("view360-undo", function()
    if rec:taking() then return end
    if not rec:undo() then return mp.osd_message("Nothing to undo", 1.5) end
    save_recording()
    retidy()
    if rec:empty() then rec_playing = false else follow_recording() end
    mp.osd_message(rec:empty() and "The last take undone: nothing recorded now" or "The last take undone", 2)
    publish()
end)

mp.register_script_message("view360-clear", function()
    if rec:taking() then stop_take() end
    rec:clear()
    rec_playing = false
    save_recording()
    retidy()
    mp.osd_message("The recording cleared", 2)
    publish()
end)

mp.register_script_message("view360-frame", function(f)
    if not recording.FRAMES[f] then return end
    rec.frame = f
    if not rec:empty() then save_recording() end
    publish()
end)

-- How the recording is tidied: view360-tidy turns off|light|medium|strong|toggle, view360-tidy level yes|no|toggle
mp.register_script_message("view360-tidy", function(what, how)
    if rec:taking() then return end
    if what == "turns" then
        if how == "toggle" then how = rec.tidy.turns == "off" and turns_before or "off" end
        if not recording.TURNS[how] or how == rec.tidy.turns then return end
        if rec.tidy.turns ~= "off" then turns_before = rec.tidy.turns end
        rec.tidy.turns = how
    elseif what == "level" then
        local on = how == "yes" or (how == "toggle" and not rec.tidy.level)
        if how ~= "yes" and how ~= "no" and how ~= "toggle" or on == rec.tidy.level then return end
        rec.tidy.level = on
    else
        return
    end
    if not rec:empty() then save_recording() end
    retidy()
    follow_recording()
    publish()
end)

mp.register_script_message("view360-size", function(v)
    if not recording.SIZES[v] then return end
    rec.size = v
    if not rec:empty() then save_recording() end
    publish()
end)

mp.add_key_binding("Alt+h", "heading", function()
    if not active then return end
    if not sphere.stamped then return mp.osd_message("360°: this clip has no camera motion to follow", 2) end
    local at = 1
    for i, m in ipairs(HEADINGS) do if m == heading then at = i end end
    for k = 1, #HEADINGS do
        local m = HEADINGS[(at + k - 1) % #HEADINGS + 1]
        if available(m) then
            set_heading(m, true)
            return mp.osd_message("360°: " .. HEADING_LABELS[m], 1.5)
        end
    end
end)

mp.register_script_message("view360", function(v)
    if v == "toggle" then set_active(not active) elseif v == "yes" or v == "no" then set_active(v == "yes") end
end)

mp.register_script_message("view360-set", function(key, value)
    value = tonumber(value)
    if not active or not value then return end
    yours()
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
    set_heading(mode, true)
end)

mp.register_script_message("view360-reset", reset)

publish()
