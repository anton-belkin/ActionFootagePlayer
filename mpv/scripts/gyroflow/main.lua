-- gyroflow: videos stabilized with Gyroflow as mpv plays them. An mpv plugin: this folder goes into ~~/scripts/,
-- with the binaries built from vs-gyroflow/ in this repo (install.sh puts them here).
--
--   DJI_..._D.MP4  +  DJI_..._D.gyroflow   ->  plays stabilized as Gyroflow has it (Gyroflow's warp as an mpv shader)
--   DJI_..._D.MP4 with gyro data, recorded with EIS off   ->  stabilized with the default preset (also GoPro clips
--       with HyperSmooth off, or on from a HERO12 or later, which records how it turned the picture: stabilized on top
--       of it with less crop, vs-gyroflow/src/in_camera.rs; and Insta360 ones with FlowState off in the camera, when
--       Gyroflow has their lens):
--       Gyroflow's own default preset, then default.gyroflow if there is one: in $GYROFLOW_DATA (where an app
--       that ships this plugin read-only, ActionFootagePlayer, keeps yours), else in this folder (install.sh puts
--       the underwater preset there)
--
-- The last is decided by gyroflow-probe, which reads the file's metadata (20-100 ms for a DJI clip, ~1 s from an
-- SD card; ~0.5 s more for GoPro and Insta360 clips it would stabilize, to find their lens in Gyroflow's database)
-- before playback starts. The motion data itself gyroflow-gpu-table reads as the clip opens (a stretch of it first for a
-- clip without a project, the rest as it plays: gpu_ahead).
--
-- The stabilization is Gyroflow's warp as an mpv shader (the GPU path, docs/design/gpu-render.md): gyroflow-gpu-table loads
-- the clip with gyroflow-core, as Gyroflow renders the project (its smoothing, zoom and output size), and writes
-- shaders/gyroflow/warp.glsl with every frame's parameters in it; libplacebo runs it ahead of the looks (the dlogm
-- plugin's), the frame never leaving the GPU. It needs mpv 0.42 (or the app's own) with vo=gpu-next. What the shader
-- can't do (gpu_table.rs's unsupported: other lens models such as Sony's, a lens mesh, IBIS/OIS, a horizontal rolling
-- shutter, an equirect output) plays as recorded, a card saying why.
--
-- The warp renders at the size of the video's rectangle on screen (resolution= below), and mpv scales it to the screen.
--
-- Keys (rebind with `<key> script-binding gyroflow/<name>` in input.conf):
--   Ctrl+g   toggle   stabilization on, on with a wider view (no zoom, at the FOV where the stabilized picture covers about
--                     as much area as the frame, for each clip's lens: the edges show at times, filled as Ctrl+b
--                     says), off for the current file. On or wide sticks for the next clips without a project; a
--                     clip with one starts in it as Gyroflow has it
--   Ctrl+k   level    a clip without a project: how strongly it's stabilized, weak -> medium -> strong. 10, 20 or
--                     30 % of the distance from the frame's center to its edge cropped, smoothness 50, 75 or 100 %;
--                     the camera's path kept inside that crop as best it can (vs-gyroflow/src/levels.rs). The wider
--                     view smooths as the level does. It sticks for the next clips; a clip with a project has the
--                     project's own smoothing and zoom
--   Ctrl+b   edges    what fills the edges the stabilized frame doesn't cover: the project's background,
--                     what was seen in that direction of the world, the rest the colors around spread in, or only
--                     the frame's own colors spread out from its edge (both blurred)
--   Ctrl+u   lens     the lens above water, and back to what it was (under water, as the camera says, or as the
--                     project says); it sticks for the next clips without a project
--   Ctrl+p   still    the temporal denoise (each frame cleaned of its noise from the frames on both sides of it), on
--                     or off: the clip's transform keeps it (still.lua; warp.glsl's dn_ passes)
--   Ctrl+a   fringes  the water's color fringes taken out, or left as recorded; only where the lens is under water
--                     (above water nothing moves either way, and the lens stays as it is). It sticks for the next clips
--   Shift+E  export   the clip as it plays (export.lua)
--   Ctrl+e   edit     open the clip's project in Gyroflow, writing one next to it first when it plays without one,
--                     as it plays: in the view (the wider one's FOV, with Gyroflow's zoom off) and with the horizon
--                     lock it has; saving it in Gyroflow reloads the stabilization here. A clip with no motion data opens as it
--                     is, for Gyroflow to find the motion from the picture (optical flow): once the project is saved
--                     next to it, it plays stabilized with it
--
-- Script messages (`script-message-to gyroflow <message>`, what the right-click menu runs; mpv/menu.conf):
--   view normal|wide|off    the stabilization on, on with the wider view, or off for this clip
--   level weak|medium|strong  the level (Ctrl+k)
--   horizon-lock yes|no|project|toggle   lock the horizon, or not, or leave it to the project
--   underwater yes|auto|project|no  the lens under water, as the camera says, as the project says, or above water
--   lens-mode corrected|as-recorded|toggle  the lens corrected (Gyroflow's lens correction and the projection) or as
--                           recorded, apart from the stabilization: off with the lens corrected is the lens alone, nothing
--                           smoothed; a clip with a lens profile and no motion data (kind "lens") plays that way
--   projection <p>          the output's projection: linear, stereographic, equidistant, equisolid, orthographic, or
--                           project (a Gyroflow project's own); over a project too, saved with the clip
--   lens <kind>             as-recorded, or the lens corrected in projection <kind>: lens-mode and projection at once
--   chromatic yes|no|toggle the water's color fringes taken out (Ctrl+a), or not
--   denoise-strong yes|no|toggle  the temporal denoise's Stronger, for a very noisy clip (part of the transform)
--   edges <mode>            what fills the edges (the modes of Ctrl+b)
--   quality <key> <value>   resolution auto|display|1080|720|video, edges <mode>, chromatic_skip yes|no,
--                           temporal_denoise full|light|off or ask_when_slow yes|no, kept in gyroflow.conf at once
--                           (quality.lua)
--   remember                the view, level, horizon lock, lens, color fringes, edges and clean still frame as the defaults,
--                           in gyroflow.conf (the view, level, horizon lock and lens: those of clips without a project)
-- and export.lua's. The state is in user-data/gyroflow/ for the menu: available (it can be stabilized: a
-- project, motion data, or a lens profile alone), kind (project, default: the preset, lens: no motion data, the lens
-- alone), motion (there's motion data), lens_mode (corrected, as-recorded), filter (stabilized, recorded, only, "":
-- what the warp does), project (the .gyroflow it plays with, "" for none), view (normal, wide, off), level, still (Ctrl+p),
-- horizon_lock (yes, no, project), underwater (yes, auto, project, no), water (the camera records when it's in the water),
-- spherical (a stitched 360° clip: what gyroflow-probe read of its spherical metadata, for the view360 script; false),
-- rig360 (a 360 camera's own file, played through the plugin's 360 mode: its rig; false),
-- chromatic (the color fringes taken out), chromatic_available (the warp can: this mpv stabilizes), unstabilized (why a
-- clip that has motion data plays as recorded: this mpv, or what the warp can't do; false),
-- edges, edges_blur, resolution, ask_when_slow, waiting (for a project from Gyroflow), exporting, last_export, live_photo, aspect, rotation (the frame's turn, on top of the file's own),
-- projection, file_rotation (how the file says it's shown: its display matrix, clockwise).
--
-- The horizon lock and the lens set here stick for the next clips without a project. A clip with a project starts with
-- the project's own (Gyroflow's: it was made there on purpose), whatever gyroflow.conf says; set here, they're for that
-- clip only. The edge fill isn't Gyroflow's: it sticks for every clip
--
-- Options (~~/script-opts/gyroflow.conf):
--   hwdec=yes    decode the videos it stabilizes on the GPU (VideoToolbox; on Linux and Windows whatever mpv finds,
--                auto-copy), whatever mpv's hwdec is: software decoding doesn't keep up with 4K. no: leave it to
--                mpv's hwdec
--   lens_quick=yes  the lens dialog's Find lens stops at the first lens that fits (Quick); no: tries every one
--   gpu_flush=auto  an mpv without the app's patch 0015 keeps every shader the GPU path has had until its window
--                closes (each with its textures: ~450 MB with Ctrl+p's denoise): once those it no longer uses pass
--                1.5 GB, the window is made again as the next clip opens (at 4 GB, at once). auto: unless mpv is the
--                app's own (it frees them itself); yes: always; no: never
--   ask_when_slow=yes  when the stabilization keeps dropping frames, playback pauses and offers a lighter
--                resolution, edge fill or temporal denoise (quality.lua); no: it plays on as it can
--   view=normal  the view clips without a project start in: normal, or wide (the wider view)
--   level=medium  the level clips without a project start in: weak, medium or strong (Ctrl+k)
--   lookahead=4  a clip without a project plays once the first 4 + this many seconds of its motion data are read, and
--                the rest is read as it plays: what's this far behind the newest is as it'd be with all of it
--                (vs-gyroflow/src/motion.rs). From a NAS, ~1 s instead of 5-10. 0: all of it first, as a project's
--   horizon_lock=project  clips without a project: yes, the horizon locked level (Gyroflow's horizon lock at 100 %),
--                no: not locked, project: as the preset says. A clip with a project plays with the project's
--   underwater=auto  clips without a project: the lens under water (Gyroflow's "Lens is underwater": the light bends
--                where it enters the lens), yes, or above water, no. auto: as the camera says over the clip, its In
--                water / Out of water (the dji-telemetry plugin's `water`; the Osmo Action 6 records it), and for
--                clips from other cameras as the preset says (under water); project: as the preset says.
--                vs-gyroflow/src/water.rs. A clip with a project plays with the project's (Ctrl+e writes the camera's
--                In water / Out of water into the one it makes)
--   chromatic=yes  the water's color fringes taken out where the lens is under water (Ctrl+a), or no: as recorded
--   edges=sphere what fills the edges: sphere (what was last seen in that direction, the rest the colors around
--                spread in), smooth (the frame's colors spread out), off (the project's background color)
--   edges_blur=2 blur of the edge fill, % of the frame height
--   resolution=auto  the stabilized frame rendered as big as the display shows it, at most 1080p
--                while the temporal denoise is on (at the screen's size it doesn't keep up full screen on a Retina
--                display: on the M1 Pro's XDR 31 % of the frames dropped, at 1080p 4 %). display: no bigger than the
--                display shows it, denoise or not. video: at the video's own size (up to 4K's pixels), whatever the
--                display. A number, 1080 or 720: no bigger than the display and its short side no longer than that,
--                for a GPU that doesn't keep up at the display's size. The warp, the edge fill, the denoise and the
--                looks render at that size and mpv scales it to the screen (gyroflow_render_scale)
--   still=yes    the temporal denoise by default (Ctrl+p, the panel's Transform; a clip's transform keeps its own, Use
--                as default sets this; temporal_denoise=off leaves it out): the paused frame cleaned of its noise from it
--                and the frames on each side, while it plays and paused (warp.glsl's dn_ passes; paused, frame
--                steps fill its frames again still_delay=0.4 s after they stop). still_grain=0.25 of the grain stays, so it doesn't look smeared (0: none)
--   translation=yes  the hand's sideways jerks taken out too where they've been computed for the clip
--                (up close they shake the picture however well its turns are taken out): Ctrl+j computes them for the
--                clip playing (gyroflow-export --translation, ~half the clip's length; Osmo Action 6, which records its
--                accelerometer), then switches them off and on. Kept in the cache (vs-gyroflow's translation.rs).
--                The side panel's Transform has it too
--   temporal_denoise=full  how much of it this computer does (Performance & Quality): full, 5 frames on each side
--                (the picture 5 frames behind, the sound delayed as much); light, 3 (about a quarter
--                cheaper, a little less clean); off, none, whatever Ctrl+p says (Ctrl+p's switch kept for when it's
--                on again). still_radius and still_tolerance, from before it, are read and ignored
--   denoise_strong=no  the transform's Stronger by default (the panel's Transform, for a very noisy clip, kept with
--                its transform): no grain kept, more frames taken in, the frame's own pixel judged too, and the luma
--                smoothed where it's flat (warp.glsl's dn_ passes)
--   live_photo=yes  a photo (Alt+s; Enter in crop mode) with the motion around it, as a phone's: live, a Live Photo
--                (a .mov next to the HEIC, for Apple's devices; macOS only), motion, a Motion Photo (an Ultra HDR JPEG
--                with the video in it, for Android's), no, the still alone; yes: Live Photos on macOS, Motion Photos
--                elsewhere. The Save frame panel's choice sets it, kept here at once (export.lua)
--   gyroflow_app=   Gyroflow for Ctrl+e on Linux and Windows: its executable (default: gyroflow in PATH on Linux,
--                   the app .gyroflow files open with on Windows). macOS opens the app by its bundle id
--   sphere_width=0  a 360 camera's own file (below): the sphere's width (its height half of it), 0 for the plugin's
--   stitching_distance=0  where its lenses' seam is right, in metres; 0 for the camera's own or 2 m
--   (gpu, workers, max_fps, fps_limit, edge_fill_rate, paused_full_size, zoom_margin, sphere_max_fps and sphere_cache_mb,
--   the VapourSynth path's, from before: read and ignored)
--
-- A 360 camera's own file (an Insta360 .insv with both lenses in it; docs/design/360-cameras.md): gyroflow-probe reads its
-- rig. Its lens tracks are joined side by side (the app's mpv on the GPU, --video-join; elsewhere lavfi-complex hstack),
-- gyroflow-gpu-table --sphere360's shader makes the sphere in the camera's axes with the rolling shutter taken out and
-- stamps each frame with the camera in the world and its headings, and the view360 plugin shows it, the horizon level. user-data/gyroflow/rig360 has its rig (brand, model,
-- lenses, tracks), false for any other clip; user-data/gyroflow/headings360 what it has for the view's heading modes
-- (gyroflow-probe --headings: along the way from GPS, north), a second or so later, false until then. Joined by
-- lavfi-complex, decoded with the copy mode of hwdec: zero-copy turns into software decoding under it

--
-- The plugin's parts, each a module of this folder: main.lua wires them, registering every event, key, observed
-- property and script message here, in the order they always were. sources.lua (what a clip is stabilized with),
-- framing.lua (the output's frame), denoise.lua (the temporal denoise in step, frame steps), bundle.lua (the
-- transform, what's published), shaders.lua (the warp's files in mpv's list), translation.lua (Ctrl+j), gpupath.lua
-- (the GPU path: its table, ahead of playback, applied), camera360.lua (a 360 camera's file), play.lua (a clip
-- opening), controls.lua (the keys and messages that change the transform), gyroflowapp.lua (Ctrl+e).
-- What the parts share: functions through W, the values each put there as it installed, and in S the state more
-- than one of them changes as the clip plays (below)
local mp = require 'mp'
-- What it says on screen goes to the side panel's cards and dialog (side-panel/notify_client.lua), when it's there
pcall(function()
    package.path = package.path .. ";" .. mp.get_script_directory():gsub("[^/\\]*$", "") .. "side-panel/?.lua"
    require 'notify_client'
end)
-- (without it: mpv's own text, and the console asks)
mp.notify = mp.notify or function(t) if t.kind ~= "hide" then mp.osd_message((t.title or "") .. (t.detail and ("\n" .. t.detail) or ""), t.kind == "working" and 60 or 4) end end
mp.ask = mp.ask or function() return false end
local options = require 'mp.options'
local utils = require 'mp.utils'
-- The responsiveness journal (workspace/journal.lua, docs/design/responsiveness-journal.md): this plugin's keys, subprocesses,
-- file reads and what it draws, timed by default; its own steps through package.loaded.journal. Nothing where the
-- workspace plugin isn't beside this one
do
    package.path = package.path .. ";" .. (mp.get_script_directory and mp.get_script_directory() or ".") .. "/../workspace/?.lua"
    pcall(require, 'journal')
end

-- Windows: paths past 260 characters (a clip deep in a trip's folders), by the workspace plugin's longpath.lua beside
-- this plugin's folder; without it, as before
do
    package.path = package.path .. ";" .. utils.join_path(utils.join_path(mp.get_script_directory(), "../workspace"), "?.lua")
    local ok, longpath = pcall(require, 'longpath')
    if ok then longpath.install(utils) end
end

local o = { hwdec = true, ask_when_slow = true, view = "normal", level = "medium", lookahead = 4, horizon_lock = "project", underwater = "auto", chromatic = true, chromatic_skip = false, aspect = "original", orientation = "as-is", projection = "linear", lens = "corrected", edges = "sphere", edges_blur = 2, resolution = "auto", still = true, translation = true, temporal_denoise = "full", denoise_strong = false, still_radius = 5, still_delay = 0.4, still_grain = 0.25, still_tolerance = 1.5, live_photo = "yes", export_size = "original", export_encode = "fast", export_quality = "displayed", photo_size = "original", photo_quality = "displayed", gyroflow_app = "", sphere_width = 0, stitching_distance = 0, gpu_flush = "auto", render_args = "", lens_quick = true,
            -- The VapourSynth path's, from before (a gyroflow.conf may have them): read and ignored
            gpu = "", workers = "", max_fps = "", fps_limit = "", edge_fill_rate = "", paused_full_size = "", zoom_margin = "", sphere_max_fps = "", sphere_cache_mb = "" }
options.read_options(o, "gyroflow")
-- The temporal denoise's frames on each side, by how much of it this computer does (still.lua's, the shader's)
local DENOISE_FRAMES = { full = 5, light = 3 }
if o.temporal_denoise ~= "light" and o.temporal_denoise ~= "off" then o.temporal_denoise = "full" end
if not ({ auto = true, display = true, video = true })[o.resolution] and not tonumber(o.resolution) then o.resolution = "auto" end
o.still_radius = DENOISE_FRAMES[o.temporal_denoise] or 5
o.still_grain = math.max(0, math.min(1, tonumber(o.still_grain) or 0.25))

local platform = require 'platform'
local still = require 'still'
local quality = require 'quality'
local crop = require 'crop'

-- What the parts share (each part's install takes what it needs of W and puts in it what it offers the others)
local W = { o = o, DENOISE_FRAMES = DENOISE_FRAMES }
-- The state more than one part changes as a clip plays. What's in S, as the parts set it up:
--   view (on, or on with the wider view; off is stab_off), level, horizon (true or false: locked or not; nil: the
--   project's), lens_mode, chromatic, underwater (and underwater_back: what it was before Ctrl+u), denoise_strong,
--   frame { aspect, orientation, rotation, projection }, edge_mode, DEFAULTS (the app's defaults' bundle): bundle.lua
--   current_source: what the clip plays stabilized with (sources.lua's find_source), nil: not stabilized
--   gpu: the GPU path's state, { shader, output, mode, chromatic, ... } while the clip plays through it (shader nil: the
--     stabilization off and the lens as recorded), nil when it isn't stabilized; gpu_pending: a table on its way (true,
--     or "again" when a change came meanwhile: written again once it's there); gpu_held: the hold's pause is the
--     plugin's; gpu_count: the shader files written; gpu_render_short: the short side it renders at (quality.lua);
--     gpu_server_running: the table server (shaders.lua's gpu_server) runs
--   unstabilized: why a clip with motion data plays as recorded, nil: it doesn't; current_lens: the lens as Gyroflow
--     takes it ({ camera, lens }); spherical, rig360: a stitched 360 clip's metadata, a 360 camera's rig (false: none)
--   stab_off: off for this clip (Ctrl+g); waiting: for a project from Gyroflow (Ctrl+e); trans_rev: Ctrl+j's
--     computations this session; render_dump: what render_args writes
-- (each part sets up its own as it installs)
local S = { gpu_pending = false }
W.S = S
-- The parts' functions this file calls (looked up in W)
local function clip_path(...) return W.clip_path(...) end
local function filtering(...) return W.filtering(...) end
local function gpu_camera(...) return W.gpu_camera(...) end
local function gpu_server(...) return W.gpu_server(...) end
local function mpv_frame(...) return W.mpv_frame(...) end
local function play_again(...) return W.play_again(...) end
local function publish(...) return W.publish(...) end
local function set_lens(...) return W.set_lens(...) end
local function set_projection(...) return W.set_projection(...) end
local function set_quality(...) return W.set_quality(...) end
local function sticks(...) return W.sticks(...) end
local function stop_watching(...) return W.stop_watching(...) end
local sources = require 'sources'
local framing = require 'framing'
local denoise = require 'denoise'
local translation = require 'translation'
local bundle = require 'bundle'
local shaders = require 'shaders'
local gpupath = require 'gpupath'
local camera360 = require 'camera360'
local play = require 'play'
local controls = require 'controls'
local gyroflowapp = require 'gyroflowapp'
for _, part in ipairs({ sources, framing, denoise, translation, bundle, shaders, gpupath, camera360, play, controls, gyroflowapp }) do
    part.install(W)
end

-- Ctrl+p (the temporal denoise in the warp): the clip's transform keeps it (Save there), the session's too when the
-- session's plays
still.init(o, function(on)
    sticks("denoise", on)
    publish()
end)

-- Crop mode (c): the camera turned and zoomed by the warp where it stabilizes the clip, mpv's own pan and zoom elsewhere
crop.init({
    stabilizing = function() return filtering() and S.gpu ~= nil end,
    -- The camera in the warp itself, at once (gpu_camera), its focal length from the table
    camera = function(c) gpu_camera(c) end,
    focal = function() return S.gpu and S.gpu.focal end,
})

mp.register_event("file-loaded", play.photo_loaded)
mp.register_event("start-file", shaders.start_file)
mp.register_event("shutdown", shaders.shutdown_kept)
mp.register_event("shutdown", shaders.shutdown_server)
gpu_server()
mp.add_periodic_timer(0.25, gpupath.ahead_timer)
mp.register_event("seek", gpupath.ahead_seek)
-- The viewer played or paused while it waited: theirs from then on
mp.observe_property("pause", "bool", gpupath.ahead_pause)
mp.register_event("end-file", gpupath.ahead_end)
denoise.start()
-- Loading waits for the probe, with the script free meanwhile: quitting doesn't wait for it
mp.add_hook("on_load", 50, play.on_load)
mp.register_event("end-file", play.first_frame_end)
mp.register_event("file-loaded", play.lens_loaded)
mp.add_key_binding("Ctrl+b", "edges", controls.edges_key)
mp.register_event("shutdown", gpupath.shutdown)
mp.observe_property("user-data/gyroflow/still", "native", gpupath.still_changed)
mp.register_script_message("lens-mode", set_lens)
mp.observe_property("video-dec-params", "native", function() mpv_frame() end)
mp.register_script_message("projection", set_projection)
mp.register_script_message("lens", framing.lens)
mp.register_script_message("aspect", framing.aspect)
mp.register_script_message("orientation", framing.orientation)
mp.register_script_message("rotate", framing.rotate)
-- on -> wider view -> off -> on
mp.add_key_binding("Ctrl+g", "toggle", controls.toggle_key)
mp.register_script_message("view", controls.view)
-- weak -> medium -> strong -> weak
mp.add_key_binding("Ctrl+k", "level", controls.level_key)
mp.register_script_message("level", controls.level)
mp.register_script_message("horizon-lock", controls.horizon_lock)
mp.register_script_message("underwater", controls.underwater)
mp.add_key_binding("Ctrl+u", "above-water", controls.above_water_key)
mp.register_script_message("chromatic", controls.chromatic)
mp.add_key_binding("Ctrl+j", "translation", translation.key)
mp.add_key_binding("Ctrl+a", "fringes", controls.fringes_key)
mp.register_script_message("denoise-strong", controls.denoise_strong)
mp.register_script_message("edges", controls.edges)
mp.register_script_message("quality", set_quality)
controls.init_quality()
mp.register_script_message("remember", controls.remember)
mp.register_script_message("remember-quality", controls.remember_quality)
mp.register_script_message("apply-view", controls.apply_view)
mp.add_key_binding("Ctrl+e", "edit", gyroflowapp.edit_key)
mp.register_event("end-file", stop_watching)

-- export.lua's photos read it from here, and its live-photo message sets it
-- (no, live or motion; a Live Photo needs macOS's afp-photo)
do
    local v = tostring(o.live_photo)
    if v ~= "no" and v ~= "motion" and not (v == "live" and platform.macos) then v = platform.macos and "live" or "motion" end
    mp.set_property_native("user-data/gyroflow/live_photo", v)
end
-- The export's settings (exportsettings.lua: a video's size, encoder and quality preset, a photo's size and preset),
-- as gyroflow.conf keeps them; export.lua's export-setting message changes them
do
    local es = require 'exportsettings'
    mp.set_property_native("user-data/gyroflow/export_settings", {
        video = es.normalize({ size = o.export_size, encode = o.export_encode, quality = o.export_quality }, "video"),
        photo = es.normalize({ size = o.photo_size, quality = o.photo_quality }, "photo"), choices = es.CHOICES })
end
require 'export'

-- The session's bundle and the app's defaults, for the workspace plugin before the first file loads
publish()

-- render_args=<file> (script-opts gyroflow-render_args, afp-stream's: tools/stream/README.md): the player, without a
-- window, says how it would play the file and quits. It writes JSON: clip (the original, also for a moment's or clip
-- preview's file), offset and length (a moment's part of it), render (stabilize, kind, user_data: gyroflow-gpu-table
-- --player's request; absent: not stabilized), dlogm (the log curve and look), title, duration. Each of the player's
-- choices is its own: what's saved for the file, its moment's, its Gyroflow project, the defaults (gyroflow.conf)
if o.render_args ~= "" then
    local written = false
    local function dump()
        if written then return end
        written = true
        local ws = mp.get_property_native("user-data/workspace")
        ws = type(ws) == "table" and ws.path == mp.get_property("path") and ws or {}
        local dl = mp.get_property_native("user-data/dlogm") or {}
        local moment = type(ws.moment) == "table" and { name = ws.moment.name, ["in"] = ws.moment["in"], out = ws.moment.out, missing = ws.moment.missing } or nil
        local out = { clip = clip_path(), offset = ws.offset, length = ws.length, moment = moment, render = S.render_dump,
                      dlogm = { curve = dl.curve or "", look = dl.look or "", log = dl.log == true },
                      title = mp.get_property("media-title"), duration = mp.get_property_number("duration") }
        -- The underwater colour's strength and highlights as it plays (the source's, else uwcolor.conf's), by the
        -- settings JSON's names, as export.lua hands them to gyroflow-export: the stream's afp-uw takes them
        if dl.curve == "uw" then require('settings').uw_of(out.dlogm, mp.get_property_native("user-data/uwcolor")) end
        local f = io.open(o.render_args, "w")
        if f then
            f:write(utils.format_json(out))
            f:close()
        else
            mp.msg.error("can't write " .. o.render_args)
        end
        mp.command("quit")
    end
    -- After the first frame: every plugin has had its say (the dlogm plugin's curve comes with the file)
    mp.register_event("playback-restart", function() mp.add_timeout(0.05, dump) end)
    mp.register_event("end-file", function() dump() end)
end

-- Ctrl+m: the clip's motion measured from its picture into its .afp-motion (measure.lua), then played again with it
require('measure').setup({ source = function() return S.current_source end, path = clip_path, again = function() play_again() end, notify = mp.notify, options = o })
