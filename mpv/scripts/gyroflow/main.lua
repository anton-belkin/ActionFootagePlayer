-- gyroflow: videos stabilized with Gyroflow as mpv plays them. An mpv plugin: this folder goes into ~~/scripts/,
-- with the binaries built from vs-gyroflow/ in this repo (install.sh puts them here).
--
--   DJI_..._D.MP4  +  DJI_..._D.gyroflow   ->  plays through Gyroflow (vf=vapoursynth)
--   DJI_..._D.MP4 with gyro data, recorded with EIS off   ->  stabilized with the default preset (also GoPro clips
--       with HyperSmooth off and Insta360 ones with FlowState off in the camera, when Gyroflow has their lens):
--       Gyroflow's own default preset, then default.gyroflow if there is one: in $GYROFLOW_DATA (where an app
--       that ships this plugin read-only, ActionFootagePlayer, keeps yours), else in this folder (install.sh puts
--       the underwater preset there)
--
-- The last is decided by gyroflow-probe, which reads the file's metadata (20-100 ms for a DJI clip, ~1 s from an
-- SD card; ~0.5 s more for GoPro and Insta360 clips it would stabilize, to find their lens in Gyroflow's database)
-- before playback starts. The motion data itself the plugin reads as the filter starts (~20 s for a 7 GB clip from an
-- SD card: issue #40).
--
-- The stabilization is the gyroflow VapourSynth plugin (libvs_gyroflow.dylib on macOS, libvs_gyroflow.so on Linux,
-- vs_gyroflow.dll on Windows), so it's the same as rendering the
-- project in Gyroflow, including the project's smoothing, zoom and output size. It runs before any shaders (the
-- dlogm plugin's D-Log M ones), at the source bit depth.
--
-- The stabilized frame is rendered no bigger than the display the window is on shows it fullscreen: what's on it is
-- the same, and every step after the decoder, the plugin's and mpv's shaders alike, has that many fewer pixels to do
-- (on a 1080p display a quarter of 4K's; #43). The input is scaled down to match on the GPU (VideoToolbox) before
-- it's copied to memory for VapourSynth, keeping a margin for the stabilization's zoom, which enlarges the part of the
-- frame that shows; and never more than 4K's pixels (8K), whatever the display. A move to a display of another size
-- sets it up again, paused for a moment. On Linux and Windows the input is scaled on the CPU, after the copy.
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
--   Ctrl+p   still    the paused frame cleaned of its noise from the frames on both sides of it, on or off: the
--                     clip's transform keeps it (still.lua)
--   Ctrl+f   full     the paused frame rendered at full size now, every frame there, cleaned too when Ctrl+p is on;
--                     pauses first. What paused_full_size=auto does by itself (still.lua)
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
--   quality <key> <value>   fps_limit 0|30|15, resolution auto|display|1080|720|video, paused_full_size off|auto,
--                           edge_fill_rate auto|50|25, edges <mode>, chromatic yes|no, temporal_denoise full|light|off
--                           or ask_when_slow yes|no, kept in gyroflow.conf at once (quality.lua)
--   remember                the view, level, horizon lock, lens, color fringes, edges and clean still frame as the defaults,
--                           in gyroflow.conf (the view, level, horizon lock and lens: those of clips without a project)
-- and export.lua's. The state is in user-data/gyroflow/ for the menu: available (a Gyroflow filter can be set up: a
-- project, motion data, or a lens profile alone), kind (project, default: the preset, lens: no motion data, the lens
-- alone), motion (there's motion data), lens_mode (corrected, as-recorded), filter (stabilized, recorded, only, "":
-- what the filter does), project (the .gyroflow it plays with, "" for none), view (normal, wide, off), level, still (Ctrl+p),
-- horizon_lock (yes, no, project), underwater (yes, auto, project, no), water (the camera records when it's in the water),
-- spherical (a stitched 360° clip: what gyroflow-probe read of its spherical metadata, for the view360 script; false),
-- rig360 (a 360 camera's own file, played through the plugin's 360 mode: its rig; false),
-- chromatic (the color fringes taken out), chromatic_available (the shader is there: a gpu vo),
-- edges, edges_blur, fps_limit, resolution, paused_full_size, edge_fill_rate, ask_when_slow, waiting (for a project from
-- Gyroflow), exporting, last_export, live_photo, aspect, rotation (the frame's turn, on top of the file's own),
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
--   workers=3    GPU worker threads
--   gpu=auto     Gyroflow's warp as a shader (the GPU path, docs/gpu-render.md) wherever mpv can (mpv 0.42 or the
--                app's, vo=gpu-next): macOS, Windows and Linux (tried on all three); no: always through VapourSynth
--   gpu_flush=auto  an mpv without the app's patch 0015 keeps every shader the GPU path has had until its window
--                closes (each with its textures: ~450 MB with Ctrl+p's denoise): once those it no longer uses pass
--                1.5 GB, the window is made again as the next clip opens (at 4 GB, at once). auto: unless mpv is the
--                app's own (it frees them itself); yes: always; no: never
--   max_fps=0    every frame plays: the sphere trail is written as the GPU has room (vs-gyroflow/src/edges.rs).
--                Above 0, clips faster than this play every other frame (30: a 60 fps clip at 30) when the
--                stabilized frame leaves edges anywhere in the clip, as Gyroflow measures it for the project and
--                view (the wider view does), and they're filled (Ctrl+b not off), for a computer where the fill
--                doesn't keep up
--   fps_limit=0  every frame is stabilized. 30 or 15: a faster clip plays at this rate or the nearest below it that
--                skips the same number of frames each time (60 fps at 30 or 15, 50 at 25 or 12.5), for a computer
--                whose GPU doesn't keep up (a laptop's, once it runs at its sustained power). The frames are skipped
--                first in the chain (lavfi framestep), before the input's scaling and VapourSynth, which asks mpv for
--                frames only a few ahead; the clean still frame is made from the frames that play. The quality menu
--                (quality.lua) sets it
--   edge_fill_rate=auto  the sphere trail (edges=sphere) is written whenever a GPU worker is free. 50 or 25: from
--                that % of the frames that play (with fps_limit=30, 25 % is ~7.5 a second), for a laptop whose CPU and
--                GPU share their power: the writes take what the frames need even while a worker is free. Through
--                the edges control file, so it changes without setting the chain up again. The quality menu sets it
--   ask_when_slow=yes  when the stabilization keeps dropping frames, playback pauses and offers a lighter frame
--                rate, resolution or edge fill (quality.lua); no: it plays on as it can
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
--   resolution=auto  the stabilized frame no bigger than the display shows it (above); on the GPU path at most 1080p
--                while the temporal denoise is on (at the screen's size it doesn't keep up full screen on a Retina
--                display: on the M1 Pro's XDR 31 % of the frames dropped, at 1080p 4 %). display: no bigger than the
--                display shows it, denoise or not. video: at the video's own size (up to 4K's pixels), whatever the
--                display. A number, 1080 or 720: no bigger than the display and its short side no longer than that,
--                for a GPU that doesn't keep up at the display's size. On the GPU path the warp, the edge fill, the
--                denoise and the looks render at that size and mpv scales it to the screen (gyroflow_render_scale)
--   zoom_margin=1.25  how much more input it keeps than the output shows, for the stabilization's zoom: the part of
--                the frame that shows reaches the output about 1:1 up to a zoom of this much
--   still=yes    the temporal denoise by default (Ctrl+p, the panel's Transform; a clip's transform keeps its own, Use
--                as default sets this; temporal_denoise=off leaves it out): the paused frame cleaned of its noise from it
--                and the frames on each side, still_delay=0.4 s after it pauses; on the GPU path also while it plays
--                (warp.glsl's dn_ passes). still_grain=0.25 of the grain stays, so it doesn't look smeared (0: none)
--   translation=yes  on the GPU path, the hand's sideways jerks taken out too where they've been computed for the clip
--                (up close they shake the picture however well its turns are taken out): Ctrl+j computes them for the
--                clip playing (gyroflow-export --translation, ~half the clip's length; Osmo Action 6, which records its
--                accelerometer), then switches them off and on. Kept in the cache (vs-gyroflow's translation.rs).
--                The side panel's Transform has it too
--   temporal_denoise=full  how much of it this computer does (Performance & Quality): full, 5 frames on each side
--                (on the GPU path the picture 5 frames behind, the sound delayed as much); light, 3 (about a quarter
--                cheaper, a little less clean); off, none, whatever Ctrl+p says (Ctrl+p's switch kept for when it's
--                on again). still_radius and still_tolerance, from before it, are read and ignored
--   denoise_strong=no  the transform's Stronger by default (the panel's Transform, for a very noisy clip, kept with
--                its transform): no grain kept, more frames taken in, the frame's own pixel judged too, and the luma
--                smoothed where it's flat (the GPU path's; warp.glsl's dn_ passes)
--   paused_full_size=off  paused, the frame stays as it played; Ctrl+f renders it at full size. auto: still_delay after
--                it pauses, it's rendered at full size (no input scaling, the output at the project's or the video's own
--                size up to 4K's pixels) with every frame there, whatever resolution and fps_limit are, in the same
--                re-render as the clean still frame; playing on sets the chain up for playback again, which costs a
--                moment on a slow GPU each time (still.lua). The quality menu sets it
--   live_photo=yes  a photo (Alt+s; Enter in crop mode) with the motion around it, as a phone's: live, a Live Photo
--                (a .mov next to the HEIC, for Apple's devices; macOS only), motion, a Motion Photo (an Ultra HDR JPEG
--                with the video in it, for Android's), no, the still alone; yes: Live Photos on macOS, Motion Photos
--                elsewhere. The Save frame panel's choice sets it, kept here at once (export.lua)
--   gyroflow_app=   Gyroflow for Ctrl+e on Linux and Windows: its executable (default: gyroflow in PATH on Linux,
--                   the app .gyroflow files open with on Windows). macOS opens the app by its bundle id
--   sphere_width=0  a 360 camera's own file (below): the sphere's width (its height half of it), 0 for the plugin's
--                   (5760: an X5 clip at 25 fps plays with no frame dropped up to 6400 on an M1 Pro)
--   stitching_distance=0  where its lenses' seam is right, in metres; 0 for the camera's own or 2 m
--   sphere_max_fps=31  above it, every other frame (8K60 doesn't keep up through VapourSynth: step 2 of
--                   docs/360-implementation-plan.md); 0 every frame
--   sphere_cache_mb=1500  VapourSynth's frame cache for it (its default, 8 GB, took mpv to 5-7 GB on 8K)
--
-- A 360 camera's own file (an Insta360 .insv with both lenses in it; docs/360-cameras.md): gyroflow-probe reads its
-- rig. Its lens tracks are joined side by side (lavfi-complex hstack), the plugin's 360 mode (sphere360.vpy) makes the
-- sphere in the camera's axes with the rolling shutter taken out and stamps each frame with the camera in the world and
-- its headings, and the view360 plugin shows it, the horizon level. user-data/gyroflow/rig360 has its rig (brand, model,
-- lenses, tracks), false for any other clip; user-data/gyroflow/headings360 what it has for the view's heading modes
-- (gyroflow-probe --headings: along the way from GPS, north), a second or so later, false until then. Decoded with the
-- copy mode of hwdec: zero-copy turns into software decoding under lavfi-complex, and the frames go to memory for
-- VapourSynth anyway

local mp = require 'mp'
local options = require 'mp.options'
local utils = require 'mp.utils'

-- Windows: paths past 260 characters (a clip deep in a trip's folders), by the workspace plugin's longpath.lua beside
-- this plugin's folder; without it, as before
do
    package.path = package.path .. ";" .. utils.join_path(utils.join_path(mp.get_script_directory(), "../workspace"), "?.lua")
    local ok, longpath = pcall(require, 'longpath')
    if ok then longpath.install(utils) end
end

-- The clip playing: mpv's path, or the original when the file playing is a moment (the workspace plugin's
-- user-data/workspace/clip, docs/moments-and-workspaces.md)
local function clip_path()
    local path = mp.get_property("path")
    local ws = mp.get_property_native("user-data/workspace")
    if path and type(ws) == "table" and ws.path == path and ws.redirected and ws.clip then return ws.clip end
    return path
end

local o = { gpu = "auto", hwdec = true, workers = 3, max_fps = 0, fps_limit = 0, ask_when_slow = true, view = "normal", level = "medium", lookahead = 4, horizon_lock = "project", underwater = "auto", chromatic = true, chromatic_skip = false, aspect = "original", orientation = "as-is", projection = "linear", lens = "corrected", edges = "sphere", edges_blur = 2, edge_fill_rate = "auto", resolution = "auto", paused_full_size = "off", zoom_margin = 1.25, still = true, translation = true, temporal_denoise = "full", denoise_strong = false, still_radius = 5, still_delay = 0.4, still_grain = 0.25, still_tolerance = 1.5, live_photo = "yes", gyroflow_app = "", sphere_width = 0, stitching_distance = 0, sphere_max_fps = 31, sphere_cache_mb = 1500, gpu_flush = "auto", render_args = "" }
options.read_options(o, "gyroflow")
-- A whole % of the frames, as the plugin takes it, or auto
do
    local rate = tostring(o.edge_fill_rate):match("^%d+$") and tonumber(o.edge_fill_rate)
    if not (rate and rate >= 1 and rate <= 100) then o.edge_fill_rate = "auto" end
end
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

local LABEL = "gyroflow"
local SCALE_LABEL = "gyroflow-scale"
local STEP_LABEL = "gyroflow-step"
local DIR = mp.get_script_directory()
local PLUGIN = utils.join_path(DIR, platform.library("vs_gyroflow"))
local SCRIPT = utils.join_path(DIR, "gyroflow.vpy")
local SPHERE_SCRIPT = utils.join_path(DIR, "sphere360.vpy")
local SPHERE_LABEL = "gyroflow-360"
local PROBE = utils.join_path(DIR, platform.exe("gyroflow-probe"))
local PROJECT_WRITER = utils.join_path(DIR, platform.exe("gyroflow-project"))
local GPU_TABLE = utils.join_path(DIR, platform.exe("gyroflow-gpu-table"))
local CHROMATIC_SHADER = utils.join_path(DIR, "chromatic.glsl")
local CROP_SHADER = utils.join_path(DIR, "crop_preview.glsl")
local GPU_HOLD_SHADER = utils.join_path(DIR, "gpu_hold.glsl")
local GYROFLOW_APP = "xyz.gyroflow"
-- Applied on top of Gyroflow's default preset to videos stabilized without a project: the first that exists
local DATA = os.getenv("GYROFLOW_DATA")
local PRESETS = {}
if DATA and DATA ~= "" then table.insert(PRESETS, utils.join_path(DATA, "default.gyroflow")) end
table.insert(PRESETS, utils.join_path(DIR, "default.gyroflow"))
-- Containers gyroflow-probe looks into: what DJI cameras record, and Insta360's own (both lenses in one .insv)
local PROBE_EXTENSIONS = { mp4 = true, mov = true, insv = true }
-- bilinear | bicubic | lanczos. Lanczos is sharper but slower: on the M1 Pro a 4K clip plays at ~38 fps of
-- capacity with it and ~54 with bicubic, so lanczos up to 30 fps and bicubic above (and when the rate's unknown)
local INTERPOLATION_SLOW = "lanczos"
local INTERPOLATION_FAST = "bicubic"
local SLOW_MAX_FPS = 31
-- Larger video is scaled down to this many pixels (aspect kept) before stabilizing, whatever the display
local MAX_INPUT_PIXELS = 3840 * 2160
-- The display's size as last seen, for a clip that loads before the window is up (the first one): `<width> <height>`
local DISPLAY_STATE = mp.command_native({ "expand-path", "~~state/gyroflow-display" })

local function file_exists(path)
    local info = utils.file_info(path)
    return info ~= nil and info.is_file
end

-- The workspace plugin's word on the file at `path` (user-data/workspace/, published before this plugin's on_load),
-- nil when it isn't installed or speaks of another file
local function workspace_of(path)
    local ws = mp.get_property_native("user-data/workspace")
    if type(ws) == "table" and ws.path == mp.get_property("path") and (ws.path == path or ws.clip == path) then return ws end
    return nil
end

-- Where the video's project may be, in the order looked at, and where a new one goes (the first): the workspace
-- plugin's paths for it in its project, else Gyroflow's two names for it next to the video
local function project_paths(path)
    local ws = workspace_of(path)
    if ws and ws.gyroflow_projects and ws.gyroflow_new then return ws.gyroflow_projects, ws.gyroflow_new end
    local base = path:match("^(.*)%.[^./\\]+$") or path
    return { base .. ".gyroflow", path .. ".gyroflow" }, base .. ".gyroflow"
end

local function find_project(path)
    if not path or path:find("://", 1, true) then return nil end
    for _, candidate in ipairs((project_paths(path))) do
        if file_exists(candidate) then return candidate end
    end
    return nil
end

local function read_json(path)
    local f = io.open(path, "r")
    if not f then return nil end
    local json = utils.parse_json(f:read("*a"))
    f:close()
    return json
end

-- The video size the project was made for (mpv doesn't know the size yet in on_load), its rate, and the size it
-- renders at (nil: the video's)
local function project_video_size(project)
    local json = read_json(project)
    local info = json and json.video_info
    if not info or not info.width or not info.height then return nil end
    local out = json.output or {}
    return info.width, info.height, info.fps, tonumber(out.output_width), tonumber(out.output_height)
end

-- What a stitched 360° clip without a project says it is (gyroflow-probe's "spherical": its projection, stereo
-- layout, pose or initial view), false for any other: published as user-data/gyroflow/spherical, for the view360
-- script, which shows it as a sphere to look around in. Such a clip isn't stabilized: it plays as it is, with mpv's
-- own hwdec (zero-copy), whatever motion data it has (docs/360-cameras.md, "Stitched clips")
local spherical = false
-- A 360 camera's own file: its rig as gyroflow-probe read it (brand, model, lenses, tracks, width, height of a track),
-- false for any other clip
local rig360 = false

-- What to stabilize the video with, to done(): { project = <.gyroflow or the video itself>, kind, width, height,
-- preset }, or nil. The probe isn't waited for in the script's thread: from an SD card it takes a second, and quitting
-- or the next clip ends it (playback_only)
local function find_source(path, done)
    local project = find_project(path)
    if project then
        local w, h, fps, ow, oh = project_video_size(project)
        return done({ project = project, kind = "project", width = w, height = h, fps = fps, out_width = ow, out_height = oh })
    end
    if not path or path:find("://", 1, true) then return done(nil) end
    local ext = (path:match("%.([^./\\]+)$") or ""):lower()
    if not PROBE_EXTENSIONS[ext] or not file_exists(PROBE) then return done(nil) end
    mp.command_native_async({ name = "subprocess", args = { PROBE, path }, capture_stdout = true, playback_only = true }, function(_, r)
        local info = r and r.status == 0 and utils.parse_json(r.stdout or "") or nil
        if info and type(info.rig360) == "table" then
            if info.rig360.error then
                mp.msg.warn("a 360 camera's file the player can't show yet: " .. tostring(info.rig360.error))
                return done(nil)
            end
            return done({ project = path, kind = "rig360", rig = info.rig360, fps = info.fps })
        end
        if info and type(info.spherical) == "table" then
            spherical = info.spherical
            mp.msg.verbose("not stabilizing: a stitched 360° clip (" .. tostring(spherical.projection) .. ")")
            return done(nil)
        end
        if info and not info.mode and (info.lens or "") ~= "" then
            -- Nothing to stabilize with, but a lens profile: the lens can be corrected on its own
            mp.msg.verbose("no motion data (" .. (info.reason or "") .. "), lens " .. info.lens)
            return done({ project = path, kind = "lens", width = info.width, height = info.height, fps = info.fps, water = info.water,
                          rotation = tonumber(info.rotation) or 0, lens = info.lens })
        end
        if not info or not info.mode then
            mp.msg.verbose("not stabilizing: " .. (info and info.reason or "probe failed"))
            return done(nil)
        end
        local preset = nil
        if info.mode == "default" then
            for _, p in ipairs(PRESETS) do
                if not preset and file_exists(p) then preset = p end
            end
        end
        -- rotation: how the file says it's shown (its display matrix, clockwise), which the plugin turns it by
        done({ project = path, kind = info.mode, width = info.width, height = info.height, fps = info.fps, preset = preset, water = info.water,
               rotation = tonumber(info.rotation) or 0 })
    end)
end

local KIND_LABELS = { project = "", default = " (default preset)", lens = " (the lens alone)" }

-- The display the window is on, in its own pixels (a Retina display's, not its points); as last seen while the window
-- isn't up yet, nil when it never was
local function display_size()
    local w, h = mp.get_property_number("display-width"), mp.get_property_number("display-height")
    if w and h and w > 0 and h > 0 then return w, h end
    local f = DISPLAY_STATE and io.open(DISPLAY_STATE, "r")
    if not f then return nil end
    w, h = (f:read("*a") or ""):match("^(%d+) (%d+)")
    f:close()
    return tonumber(w), tonumber(h)
end

local function even(v)
    return math.max(2, math.floor(v / 2) * 2)
end

-- The output's frame, this clip's (the Clip tab's; saved with its view): turned clockwise by 0, 90, 180 or 270
-- degrees, then cut to an aspect ("original": the frame's own). Where it's stabilized, Gyroflow's output size and
-- rotation (vs-gyroflow), its zoom fitting the picture into it; elsewhere mpv's video-rotate and video-crop. A clip
-- with a project has the project's own unless changed here, for viewing
local ASPECTS = { original = true, ["16:9"] = true, ["4:3"] = true, ["5:4"] = true, ["1:1"] = true, ["4:5"] = true, ["3:4"] = true, ["9:16"] = true }
-- Those, or any W:H
local function aspect_ok(a)
    return a ~= nil and (ASPECTS[a] == true or tostring(a):match("^%d+:%d+$") ~= nil)
end
-- The frame's ratio is kept landscape-wise (16:9) with an orientation: as-is (as the source is), landscape, portrait. An
-- aspect from before (9:16: the ratio as it showed) is that ratio turned, portrait; another W:H landscape; original as is
local ORIENTATIONS = { ["as-is"] = true, landscape = true, portrait = true }
local function split_aspect(aspect, orientation)
    if not aspect_ok(aspect) then aspect = "original" end
    local w, h = tostring(aspect):match("^(%d+):(%d+)$")
    if w and tonumber(w) < tonumber(h) then return h .. ":" .. w, "portrait" end
    if ORIENTATIONS[orientation] then return aspect, orientation end
    return aspect, w and "landscape" or "as-is"
end
-- The W:H the frame shows for a source w x h (upright), or original (workspace's transform.lua concrete, the same)
local function concrete(aspect, orientation, w, h)
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
-- The frame's W:H as it shows now (further down, once the source's size is known)
local shown_aspect
-- And the output's lens where it's stabilized, Gyroflow's output projection: linear (rectilinear, straight lines
-- straight), or a fisheye, its edges more and more compressed: stereographic, equidistant, equisolid, orthographic. For
-- Wide footage above water, too wide for linear: its center squeezed, its edges thin
local PROJECTIONS = { linear = 0, stereographic = 1, equidistant = 2, equisolid = 3, orthographic = 4 }
local frame = { aspect = "original", orientation = "as-is", rotation = 0, projection = "linear" }

-- The size of a w x h frame turned and cut to the aspect: its largest centered part of that aspect, each side
-- rounded down to even (vs-gyroflow's Frame::size, which renders it)
local function frame_size(w, h, f)
    f = f or frame
    if f.rotation == 90 or f.rotation == 270 then w, h = h, w end
    local aw, ah = tostring(f.aspect):match("^(%d+):(%d+)$")
    aw, ah = tonumber(aw), tonumber(ah)
    if aw and ah and aw > 0 and ah > 0 then
        if w * ah > h * aw then w = math.floor(h * aw / ah) else h = math.floor(w * ah / aw) end
    end
    return math.max(2, math.floor(w / 2) * 2), math.max(2, math.floor(h / 2) * 2)
end

-- The sizes to stabilize at: { input = { w, h } the video is scaled down to on the way (nil: as it is), output =
-- { w, h } the plugin renders (nil: the project's, or the input's), key: both, to tell a change }. The output fits the
-- display; the input keeps zoom_margin more, so the part of the frame that shows isn't enlarged. full: the sizes for
-- the paused frame, as resolution=video has them whatever it is
local function render_sizes(source, full)
    local w, h = source.width, source.height
    if not w or not h or w <= 0 or h <= 0 then return { key = "" } end
    local ow, oh = source.out_width or w, source.out_height or h
    -- The file's own turn (its display matrix: a camera held upright) comes first, then the frame's: the plugin renders
    -- it upright (vs-gyroflow's Frame), and mpv doesn't turn it again. A project has its own, unless the frame changes it
    local turn = (source.rotation or 0) % 360
    local fa = shown_aspect()
    if frame.rotation ~= 0 or fa ~= "original" or (turn ~= 0 and source.kind ~= "project") then
        ow, oh = frame_size(w, h, { rotation = (turn + frame.rotation) % 360, aspect = fa })
    end
    local scale = 1
    local dw, dh = display_size()
    if o.resolution ~= "video" and dw and not full then scale = math.min(1, dw / ow, dh / oh) end
    local short_side = not full and tonumber(o.resolution)
    if short_side then scale = math.min(scale, short_side / math.min(ow, oh)) end
    local input = math.min(1, scale * o.zoom_margin * math.max(ow / w, oh / h))
    -- Scaling the input costs a pass of its own on the GPU: not for less than a fifth of the pixels
    if input > 0.9 then input = 1 end
    input = math.min(input, math.sqrt(MAX_INPUT_PIXELS / (w * h)))
    -- Nor an output bigger than the input it's made from (8K scaled down to 4K's pixels, on a 5K display)
    scale = math.min(scale, input * math.min(w / ow, h / oh))
    local sizes = {
        input = input < 1 and { even(w * input), even(h * input) } or nil,
        output = scale < 1 and { even(ow * scale), even(oh * scale) } or nil,
    }
    local size = function(s) return s and string.format("%dx%d", s[1], s[2]) or "-" end
    sizes.key = size(sizes.input) .. " " .. size(sizes.output)
    return sizes
end

local function interpolation(source)
    local fps = tonumber(source.fps)
    if fps and fps > 0 and fps <= SLOW_MAX_FPS then return INTERPOLATION_SLOW end
    return INTERPOLATION_FAST
end

-- The plugin fills the uncovered edges itself (vs-gyroflow/src/edges.rs). The mode and the blur go through a control
-- file it reads while playing: changing the filter's parameters would rebuild mpv's filter chain, which drops frames
local EDGE_MODES = { "off", "sphere", "smooth" }
local EDGE_LABELS = { off = "background color", sphere = "trail by direction, blurred",
                      smooth = "frame's edge spread out, blurred" }
local EDGE_CONTROL = utils.join_path(platform.temp_dir(), string.format("mpv-gyroflow-%d.edges", utils.getpid()))
local edge_mode = EDGE_LABELS[o.edges] and o.edges or "sphere"
local edge_blur = o.edges_blur

-- What the clip plays stabilized with (find_source), nil: not stabilized
local current_source = nil
-- The GPU path's state (further down, at add_filter): { shader, output, mode, chromatic } while the clip plays through it,
-- and what writes its shader again after a change
local gpu = nil
-- The translation compensation (Ctrl+j, the panel's Transform): taken out or not, its computing and how far it is
local trans = { on = true, job = nil, progress = 0 }
local gpu_refresh
-- A table on its way: true, or "again" when a change came meanwhile (written again once it's there)
local gpu_pending = false
-- The clip loaded again from where it plays (further down)
local play_again
-- Crop mode's camera in the warp (further down)
local gpu_camera
-- The denoise's delay kept in step (further down)
local dn_sync
-- (further down: still.lua's Ctrl+p calls it)
local publish
-- The lens as Gyroflow takes it for the clip playing (gyroflow-probe --lens, read once it plays): { camera, lens }
local current_lens = nil
-- Paused, the chain is set up at full size, every frame (still.lua): no input scaling, no frames skipped, the output
-- at the project's or the video's own size
local full_size = false
-- mpv's auto-window-resize while the chain is at full size (place_filters)
local window_resize = nil

-- On, or on with a wider view (the plugin's wide FOV, see vs-gyroflow's wide_fov); off is per clip
local view = "normal"

-- The level of a clip without a project (Ctrl+k)
local LEVELS = { "weak", "medium", "strong" }
local LEVEL_LABELS = { weak = "weak (10 % crop, smoothness 50 %)", medium = "medium (20 % crop, smoothness 75 %)",
                       strong = "strong (30 % crop, smoothness 100 %)" }
local level = "medium"

-- The water's color fringes taken out (Ctrl+a), or left as recorded
local chromatic = o.chromatic

-- The horizon lock the preset has (default.gyroflow's, else Gyroflow's own: none), for "as the preset says" from
-- before the horizon was Level or Free only
local function preset_horizon()
    for _, p in ipairs(PRESETS) do
        if file_exists(p) then
            local j = read_json(p)
            local amount = type(j) == "table" and type(j.stabilization) == "table" and tonumber(j.stabilization.horizon_lock_amount)
            return (amount and amount > 0) and "yes" or "no"
        end
    end
    return "no"
end

-- The transform bundle (docs/moments-and-workspaces.md, "Source and transform"): the stabilization (normal, wide; off is
-- a saved bundle's or this viewing's), level, horizon lock, color fringes, aspect, projection, and the lens (corrected,
-- or as recorded) apart from the stabilization. The app's defaults are
-- gyroflow.conf's; the session's start as them and change only by hand, on a file that plays with them (nothing saved
-- for it: the workspace plugin publishes no view). A file with a bundle of its own plays with it, and a change there is
-- for viewing only (or Save, Auto save)
local function defaults_bundle()
    local horizon_default = (o.horizon_lock == "yes" or o.horizon_lock == "no") and o.horizon_lock or preset_horizon()
    return {
        view = o.view == "wide" and "wide" or "normal",
        level = LEVEL_LABELS[o.level] and o.level or "medium",
        horizon_lock = horizon_default,
        chromatic = o.chromatic == true,
        aspect = (split_aspect(o.aspect, o.orientation)),
        orientation = select(2, split_aspect(o.aspect, o.orientation)),
        projection = PROJECTIONS[o.projection] and o.projection or "linear",
        lens = o.lens == "as-recorded" and "as-recorded" or "corrected",
        denoise = o.still == true,
        denoise_strong = o.denoise_strong == true,
        preset_horizon = preset_horizon(),
    }
end
local DEFAULTS = defaults_bundle()
local session = {}
for k, v in pairs(DEFAULTS) do session[k] = v end
level = session.level

-- The mode, then the blur, the view, the level, the color fringes and the edge fill rate: "sphere blur=2 view=wide
-- level=medium chromatic=yes fill_rate=auto". The plugin renders both views from one engine, and switches between
-- them (and levels, smoothing the clip again for one) on the next frame it reads this (vs-gyroflow's Views)
local function write_edge_control()
    local f = io.open(EDGE_CONTROL, "w")
    if not f then return mp.msg.warn("can't write " .. EDGE_CONTROL) end
    -- Quality's skip: not on this computer's screen, whatever the clip's transform says
    f:write(string.format("%s blur=%g view=%s level=%s chromatic=%s fill_rate=%s", edge_mode, edge_blur, view, level, (chromatic and not o.chromatic_skip) and "yes" or "no", o.edge_fill_rate)
            .. crop.control_words())
    f:close()
    -- On the GPU path the view, level, edges and color fringes are the shader's: written again
    if gpu and gpu_refresh then gpu_refresh() end
end
local function has_project(source)
    return source ~= nil and source.kind == "project"
end
local function view_label(v)
    if v == "wide" then return "on, wider view" end
    if v == "normal" then return has_project(current_source) and "on (Gyroflow project)" or "on" end
    return "off"
end
-- true or false: locked or not; nil: the project's (a clip with a Gyroflow project, which decides it)
local HORIZON = { yes = true, no = false }
-- The lens: corrected (and projected as frame.projection says), or as recorded; apart from the stabilization, so a clip
-- stabilized or not, and one with a lens profile and no motion data (kind "lens"), has it corrected or not
local lens_mode = "corrected"
local horizon = session.horizon_lock == "yes"
-- The temporal denoise's Stronger (the transform's denoise_strong): for a very noisy clip, on the GPU path
local denoise_strong = session.denoise_strong == true
-- The lens under water with a flat port (yes), above water or behind a dome port (no), as the camera's telemetry says
-- (auto), or as the project says (project, a clip with one). Input: the source's own correction (the workspace
-- plugin's input), written there as it's changed; the app's default (underwater=) for sources without one. A clip's
-- camera that doesn't record it (source.water) leaves auto to the preset
local UNDERWATER_LABELS = { yes = "lens under water (flat port)", no = "lens above water", auto = "lens under water as the camera says",
                            project = "lens under water or not as the project says" }
local default_underwater = (UNDERWATER_LABELS[o.underwater] and o.underwater ~= "project") and o.underwater or "auto"
local underwater = default_underwater
-- Under water, auto or the project's, before Ctrl+u took the lens out of the water
local underwater_back = underwater ~= "no" and underwater or "auto"

-- The workspace plugin's bundle for the file playing, when one is saved for it (a moment's or clip preview's own,
-- else the source's own file's), the part stabilized on its own (trim: { in, out }, a moment's without a project),
-- and its switch (alone: its project's "only within the trim range"). nil: the session's plays
local function fixed()
    local ws = mp.get_property_native("user-data/workspace")
    if type(ws) == "table" and ws.path == mp.get_property("path") then return ws.view, ws.trim, ws.alone end
    return nil, nil, nil
end

-- The source's input (the workspace plugin's, from its own file): curve, rotation, underwater
local function input()
    local ws = mp.get_property_native("user-data/workspace")
    if type(ws) == "table" and ws.path == mp.get_property("path") and type(ws.input) == "table" then return ws.input end
    return {}
end

-- A bundle taken as what plays: true when it has the stabilization off
local function take(b)
    b = type(b) == "table" and b or session
    view = b.view == "wide" and "wide" or "normal"
    level = LEVEL_LABELS[b.level] and b.level or session.level
    local h = b.horizon_lock
    if h == "project" then h = DEFAULTS.preset_horizon end
    if h ~= "yes" and h ~= "no" then h = session.horizon_lock end
    horizon = h == "yes"
    if type(b.chromatic) == "boolean" then chromatic = b.chromatic else chromatic = session.chromatic end
    frame.aspect, frame.orientation = split_aspect(b.aspect, b.orientation)
    -- "project": a Gyroflow project's own (over one; linear without)
    frame.projection = (PROJECTIONS[b.projection] or b.projection == "project") and b.projection or "linear"
    -- A bundle from before the lens was a key of its own: off showed the lens as recorded too
    if b.lens == "corrected" or b.lens == "as-recorded" then lens_mode = b.lens
    elseif b ~= session and b.view == "off" then lens_mode = "as-recorded"
    else lens_mode = session.lens end
    if type(b.denoise_strong) == "boolean" then denoise_strong = b.denoise_strong else denoise_strong = session.denoise_strong end
    -- The temporal denoise on or off (Ctrl+p): the clip's (off where it harms, as on the water's surface); Performance
    -- & Quality's Off leaves it out whatever this says (still.lua)
    if type(b.denoise) == "boolean" then still.set(b.denoise) else still.set(session.denoise) end
    if dn_sync then dn_sync() end
    return b.view == "off"
end

-- A change by hand to what plays: the session's too when the session's plays (nothing saved for the file, no project)
local function sticks(k, v)
    if not fixed() then session[k] = v end
end

-- The output's lens over a Gyroflow project (#196): the bundle's (saved for the clip, or reverted to), as recorded or
-- corrected in a projection of its own; with none, the project's own ("project"), corrected
local function over_project(b)
    b = type(b) == "table" and b or {}
    lens_mode = b.lens == "as-recorded" and "as-recorded" or "corrected"
    frame.projection = (PROJECTIONS[b.projection] or b.projection == "project") and b.projection or "project"
end

-- The input's turn and lens a clip starts with, and the horizon: a project's own, or the source's
local function clip_settings(source)
    local i = input()
    local r = tonumber(i.rotation)
    frame.rotation = (r == 90 or r == 180 or r == 270) and r or 0
    if has_project(source) then
        horizon, underwater = nil, "project"
    else
        underwater = (UNDERWATER_LABELS[i.underwater] and i.underwater ~= "project") and i.underwater or default_underwater
    end
    underwater_back = underwater ~= "no" and underwater or (has_project(source) and "project" or "auto")
end

-- The water's color fringes, taken out wherever the stabilization has the lens under water (its refraction, frame by
-- frame: vs-gyroflow/src/dispersion.rs), unless Ctrl+a leaves them as recorded (the control file's chromatic=). The
-- shader is in mpv's own glsl-shaders from the start, ahead of the dlogm plugin's, which builds its list from them: it
-- reads the frame's marker before the noise reduction and the log curve. It leaves video without the marker as it is.
-- The stabilization writes one whenever the shader is there, as the shader paints it over; out of the water, or with
-- Ctrl+a off, one that moves no color. Only vo=gpu-next, gpu and libmpv (an app's own window) run shaders: with
-- another vo (or without the file) there's no marker
local chromatic_shader = false
if file_exists(CHROMATIC_SHADER) then
    local vo = mp.get_property("vo") or ""
    if vo == "" or vo:find("gpu", 1, true) or vo:find("libmpv", 1, true) then
        mp.commandv("change-list", "glsl-shaders", "append", CHROMATIC_SHADER)
        chromatic_shader = true
    end
end
-- Crop mode's preview (crop_preview.glsl, crop.lua), there from the start for the same reason; it runs only in crop
-- mode (its crop_on)
local crop_shader = false
if file_exists(CROP_SHADER) then
    local vo = mp.get_property("vo") or ""
    if vo == "" or vo:find("gpu", 1, true) or vo:find("libmpv", 1, true) then
        mp.commandv("change-list", "glsl-shaders", "append", CROP_SHADER)
        crop_shader = true
    end
end

-- Faster than max_fps (when it's set): such a clip plays every other frame if its stabilized frame leaves edges
-- anywhere and they're filled (gyroflow.vpy asks the plugin, which measures it with Gyroflow's zoom for any project and
-- view). Interpolation stays the one for the clip's own rate: at half rate there's headroom for the fill instead
local function fast(source)
    local fps = tonumber(source.fps)
    return o.max_fps > 0 and fps ~= nil and fps > o.max_fps + 1
end

-- Every how many frames one is stabilized for fps_limit: the fewest skipped that bring the clip's rate down to it.
-- Every one paused at full size
local function frame_step(source)
    local fps = tonumber(source.fps)
    if full_size or o.fps_limit <= 0 or not fps or fps <= o.fps_limit + 1 then return 1 end
    return math.ceil(fps / o.fps_limit - 0.05)
end

-- The frames skipped for it, first in the chain (nil: none)
local function step_filter(source, enabled)
    local step = frame_step(source)
    if step == 1 then return nil end
    return { name = "lavfi", label = STEP_LABEL, enabled = enabled, params = { graph = "framestep=" .. step } }
end

-- A moment playing as a video of its own (the workspace plugin's: its part of the clip, from 0): where it starts in
-- the clip, which the filter adds to each frame's time to find its motion
local function time_offset()
    local ws = mp.get_property_native("user-data/workspace")
    if type(ws) == "table" and ws.path == mp.get_property("path") and type(ws.offset) == "number" then return ws.offset end
    return nil
end

-- A moment's end in its clip, nil: not a moment. The GPU path reads its part's motion data alone (gpu_table)
local function moment_end()
    local ws = mp.get_property_native("user-data/workspace")
    if type(ws) == "table" and ws.path == mp.get_property("path") and ws.moment and type(ws.offset) == "number" and type(ws.length) == "number" then
        return ws.offset + ws.length
    end
    return nil
end

-- What the filter does with the lens and the motion (vs-gyroflow's LensMode, further down), nil: no filter
local filter_lens

-- The output projection the filter is given (vs-gyroflow's Frame: 0-4), nil for none of its own: over a Gyroflow
-- project, the one picked whatever it is (linear too: in place of a fisheye project's), "project" its own; without one,
-- linear is the preset's
local function lens_projection(source)
    local p = PROJECTIONS[frame.projection]
    if not p then return nil end
    if has_project(source) then return p end
    return frame.projection ~= "linear" and p or nil
end

local function gyroflow_filter(source, enabled)
    local output = render_sizes(source, full_size).output
    local _, trim, alone = fixed()
    -- A project's "only within the trim range" as it's viewed: true or false, nil for the project's own
    local project_alone = nil
    if has_project(source) and alone ~= nil then project_alone = alone end
    return {
        name = "vapoursynth",
        label = LABEL,
        enabled = enabled,
        params = {
            file = SCRIPT,
            ["user-data"] = utils.format_json({ plugin = PLUGIN, project = source.project, preset = source.preset, interpolation = interpolation(source), workers = o.workers,
                                                fast = not full_size and frame_step(source) == 1 and fast(source),
                                                wide = view == "wide" or nil, level = level, lookahead = o.lookahead, horizon_lock = horizon ~= nil and (horizon and 100 or 0) or nil, underwater = underwater, edges = edge_mode, edges_blur = edge_blur, edges_control = EDGE_CONTROL,
                                                chromatic = chromatic_shader or nil, width = output and output[1], height = output and output[2], still_control = still.CONTROL,
                                                time_offset = time_offset(), trim_start = trim and trim["in"], trim_end = trim and trim.out,
                                                alone = project_alone,
                                                rotation = frame.rotation ~= 0 and frame.rotation or nil,
                                                aspect = shown_aspect() ~= "original" and shown_aspect() or nil,
                                                projection = lens_projection(source),
                                                lens = filter_lens(source) or "stabilized" }),
            -- Frames mpv requests in parallel: one per GPU worker plus one being decoded. More only costs
            -- memory (an 8K 10-bit frame is ~100 MB); mpv's default (core count) makes 8K crawl
            ["concurrent-frames"] = "4",
        },
    }
end

-- Off for this clip (Ctrl+g)
local stab_off = false
-- The stabilization and the lens as the filter does them: stabilized with the lens corrected or as recorded, the lens
-- alone corrected (the stabilization off, or no motion data: kind "lens"), or nothing (off and as recorded: no filter)
filter_lens = function(source)
    source = source or current_source
    if not source then return nil end
    if source.kind ~= "lens" and not stab_off then return lens_mode == "as-recorded" and "recorded" or "stabilized" end
    if lens_mode == "corrected" then return "only" end
    return nil
end
-- The filter is on: it makes the frame (else mpv's own turn and crop do)
local function filtering()
    return current_source ~= nil and filter_lens() ~= nil
end
-- Ctrl+e on a clip without motion data: the project Gyroflow will save for it is waited for
local waiting = false

-- The clean still frame needs the stabilization's filter in the chain, on
still.init(o, function()
    if not filtering() then return false end
    for _, f in ipairs(mp.get_property_native("vf") or {}) do
        if f.label == LABEL and f.enabled ~= false then return true end
    end
    return false
end, function() return filtering() and frame_step(current_source) or 1 end, function(on)
    -- Ctrl+p: the clip's transform keeps it (Save there), the session's too when the session's plays
    sticks("denoise", on)
    publish()
end)

-- Crop mode (c): the camera turned and zoomed by the plugin where it stabilizes the clip, mpv's own pan and zoom elsewhere
crop.init({
    stabilizing = function()
        if not filtering() then return false end
        if gpu then return true end
        for _, f in ipairs(mp.get_property_native("vf") or {}) do
            if f.label == LABEL and f.enabled ~= false then return true end
        end
        return false
    end,
    write = function() write_edge_control() end,
    control_path = function() return EDGE_CONTROL end,
    shader = crop_shader,
    -- The GPU path: the camera in the warp itself, at once (gpu_camera), its focal length from the table
    gpu = function() return gpu ~= nil and gpu.shader ~= nil and not gpu.hold end,
    camera = function(c) gpu_camera(c) end,
    focal = function() return gpu and gpu.focal end,
})

-- The upright source's ratio, W:H reduced
local function source_aspect()
    local src = current_source
    local w, h = src and src.width, src and src.height
    if not (w and h and w > 0 and h > 0) then
        local d = mp.get_property_native("video-dec-params")
        if type(d) ~= "table" or not d.w or not d.h then return nil end
        w, h = d.w, d.h
        if ((tonumber(d.rotate) or 0) % 180) == 90 then w, h = h, w end
    elseif (((src.rotation or 0) + frame.rotation) % 180) == 90 then
        w, h = h, w
    end
    local function gcd(a, b) while b ~= 0 do a, b = b, a % b end return a end
    local g = gcd(math.floor(w), math.floor(h))
    return string.format("%d:%d", math.floor(w) / g, math.floor(h) / g)
end

shown_aspect = function()
    local w, h = tostring(source_aspect() or ""):match("^(%d+):(%d+)$")
    return concrete(frame.aspect, frame.orientation, tonumber(w), tonumber(h))
end

-- For the right-click menu (mpv/menu.conf)
function publish()
    local state = {
        available = current_source ~= nil, kind = current_source and current_source.kind or "",
        project = has_project(current_source) and current_source.project or "",
        view = stab_off and "off" or view, level = level,
        -- The motion data to stabilize with (a project, or the clip's own), and the lens: corrected or as recorded, and
        -- what the filter does (stabilized, recorded, only: the lens alone; "" none)
        motion = current_source ~= nil and current_source.kind ~= "lens", lens_mode = lens_mode, filter = filter_lens() or "",
        horizon_lock = horizon == nil and "project" or (horizon and "yes" or "no"), edges = edge_mode, waiting = waiting,
        underwater = underwater, water = current_source and current_source.water or false,
        chromatic = chromatic, chromatic_available = chromatic_shader,
        edges_blur = edge_blur,
        fps_limit = o.fps_limit, resolution = o.resolution, paused_full_size = o.paused_full_size, edge_fill_rate = o.edge_fill_rate,
        ask_when_slow = o.ask_when_slow, spherical = spherical, rig360 = rig360, aspect = frame.aspect, orientation = frame.orientation, frame_aspect = shown_aspect(),
        rotation = frame.rotation, projection = frame.projection,
        file_rotation = current_source and current_source.rotation or 0,
        chromatic_skip = o.chromatic_skip,
        temporal_denoise = o.temporal_denoise, denoise = still.switched(), denoise_strong = denoise_strong,
        -- The hand's sideways jerks: unavailable (off in gyroflow.conf, or not on the GPU path), measuring, none
        -- (not computed for the clip yet), on, off
        translation = (not o.translation or not (gpu and gpu.shader)) and "unavailable" or trans.job and "measuring"
            or gpu.translation and (trans.on and "on" or "off") or "none",
        translation_progress = trans.progress,
        -- The source's own ratio, upright (the file's turn and the input's): what the frame's "own" is
        source_aspect = source_aspect() or false,
        project_info = has_project(current_source) and current_source.info or false,
        -- The session's bundle and the app's defaults (the workspace plugin fills and makes bundles with them)
        session = session, defaults = DEFAULTS,
        -- The lens as Gyroflow takes it: { camera, lens ("in the file", a profile's name, "" unknown) }
        lens = current_lens or false,
    }
    for k, v in pairs(state) do mp.set_property_native("user-data/gyroflow/" .. k, v) end
end

-- The filter set up again for current_source (the horizon lock, the lens, or the rate changing with the edge fill);
-- mpv rebuilds the filter chain, so it costs a few dropped frames
local function rebuild_filter()
    if not current_source then return end
    if gpu then return gpu_refresh() end
    local vf = mp.get_property_native("vf") or {}
    for i, f in ipairs(vf) do
        if f.label == LABEL then vf[i] = gyroflow_filter(current_source, f.enabled) end
    end
    mp.set_property_native("vf", vf)
end

-- A clip played again from where it was (play_again)
local resume = nil

-- The decoding (mpv's hwdec, nil: as it is) and the input's scaling (a filter, nil: none) for the sizes
local function input_scaling(sizes, enabled)
    local hwdec = o.hwdec and "yes" or mp.get_property("hwdec") or "no"
    local s = sizes.input
    if not platform.macos then
        -- Whichever hardware decoder mpv finds (VAAPI, NVDEC, D3D11...), copied back to memory for VapourSynth,
        -- and scaled down there: there's no one GPU scaler for all of them
        return hwdec ~= "no" and "auto-copy" or nil, s and { name = "lavfi", label = SCALE_LABEL, enabled = enabled,
            params = { graph = string.format("scale=w=%d:h=%d:flags=area", s[1], s[2]) } } or nil
    elseif s and hwdec ~= "no" then
        -- Frames stay on the GPU through scale_vt; mpv copies the smaller result to memory for VapourSynth.
        -- Without hardware frames (software decoding) scale_vt fails and mpv drops just that filter
        return "videotoolbox", { name = "lavfi", label = SCALE_LABEL, enabled = enabled,
                                 params = { graph = string.format("scale_vt=w=%d:h=%d", s[1], s[2]) } }
    elseif hwdec ~= "no" then
        -- VapourSynth needs frames in system memory; keep hardware decoding, copied back
        return "videotoolbox-copy", nil
    end
    return nil, nil
end

-- The GPU path (gpu=auto, docs/gpu-render.md): Gyroflow's warp as an mpv shader, the frame never leaving the GPU, in
-- place of the VapourSynth filter. gyroflow-gpu-table loads the clip as the filter would, from the filter's own
-- arguments, and writes a shader with every frame's parameters in it; mpv's PTS finds the frame (mpv 0.42, or the
-- app's mpv, 0.41.0 + 499 commits or later). The warp is the scaler too: it renders at the size of the video's rectangle
-- on screen, turned and cut as the frame says. What it doesn't do yet keeps the filter: a lens or setting the shader
-- doesn't have (gyroflow-gpu-table says which); crop mode's camera is mpv's own pan and zoom on it. gpu (declared further up): nil on the filter's path
-- Files the GPU path couldn't do, and why: they play through the filter when loaded again
local gpu_blocked = {}
local gpu_count = 0

local function mpv_has_pts()
    local v = mp.get_property("mpv-version") or ""
    local minor = tonumber(v:match("v?0%.(%d+)%.%d+"))
    if not minor then return v:match("v?(%d+)%.") ~= nil and tonumber(v:match("v?(%d+)%.")) > 0 end
    if minor >= 42 then return true end
    if minor < 41 then return false end
    local commits = tonumber(v:match("^mpv v?0%.41%.%d+%-(%d+)%-g"))
    return commits ~= nil and commits >= 499
end

-- gpu=auto and yes: wherever mpv can (macOS, Windows and Linux: tried on each, Windows on Anton's PC, Linux on the
-- NAS, 2026-11), no: never. An mpv without the PTS (a distribution's 0.41) keeps VapourSynth (mpv_has_pts)
local function gpu_wanted() return o.gpu ~= "no" end

local function gpu_eligible(source)
    if not gpu_wanted() or not file_exists(GPU_TABLE) or not mpv_has_pts() then return false end
    local vo = mp.get_property("vo") or ""
    if vo ~= "" and not vo:find("gpu-next", 1, true) then return false end
    if not source or (source.kind ~= "project" and source.kind ~= "default" and source.kind ~= "lens") then return false end
    if gpu_blocked[mp.get_property("path") or ""] then return false end
    -- The file's own turn, the frame's turn, cut and projection are the warp's (vs-gyroflow's Frame, from the same
    -- arguments): mpv turns nothing more (video-rotate no) and shows the output's shape (video-aspect-override)
    return filter_lens(source) ~= nil
end

-- The shaders mpv keeps (docs/gpu-render.md, "Each change is a new shader file"). Each version of the warp is a file
-- of its own: mpv parses a path once and gives the same parse back for it. The app's mpv (patch 0015) lets go of a
-- shader once it's out of glsl-shaders; another keeps every one, with its textures, until its window closes. So:
-- - the same settings for the same clip get the same file again (gpu_reuse), the table not written again, and an mpv
--   that keeps them has it already, at no new cost;
-- - with an mpv that keeps them, what those no longer used hold (gpu_resident, estimated) is counted, and past
--   FLUSH_SOON the window is made again as the next clip opens, past FLUSH_NOW at once: mpv parses the shaders
--   in use again and the rest are gone (gpu_flush)
local FLUSH_SOON_MB, FLUSH_NOW_MB = 1500, 4000
-- What a warp holds besides its text (twice: mpv's copy and libplacebo's): the table's grid, and Ctrl+p's ring of 11
-- frames (gpu_table.rs: GDN_FRAMES 7680x12960 rgb10a2 and GDN_QUART), a 360 sphere
local GRID_MB, DENOISE_MB, SPHERE_MB = 7, 460, 35
local REUSE_MAX = 16

local function mpv_keeps_shaders()
    if o.gpu_flush == "yes" then return true end
    if o.gpu_flush == "no" then return false end
    -- The app's own mpv: patch 0015 came with 0014's --video-join, which no other mpv has
    return mp.get_property("option-info/video-join/name") == nil
end

-- key -> { path, reply, used }; path -> true for the files kept for it
local gpu_reuse, gpu_kept = {}, {}
-- path -> MB, the warps mpv has parsed since its window was made (only with an mpv that keeps them)
local gpu_resident = {}
local flush_due = false
local vo_base, vo_twice = nil, false

-- The window made again (mpv's vo set to a list that differs, the same output): a new VO, with no shader parsed
local function gpu_flush(in_use)
    flush_due = false
    if not vo_base then
        vo_base = mp.get_property_native("vo") or {}
        if #vo_base == 0 then vo_base = { { name = mp.get_property("current-vo") or "gpu-next" } } end
    end
    local list = {}
    for i, v in ipairs(vo_base) do list[i] = v end
    -- The first output twice (mpv takes the first that works): another list, the same output
    vo_twice = not vo_twice
    if vo_twice then table.insert(list, vo_base[1]) end
    local mb = 0
    for p, m in pairs(gpu_resident) do if p ~= in_use then mb = mb + m end end
    mp.msg.info(string.format("mpv keeps every shader it had: its window made again, ~%d MB freed", mb))
    mp.set_property_native("vo", list)
    local cost = in_use and gpu_resident[in_use]
    gpu_resident = {}
    if in_use and cost then gpu_resident[in_use] = cost end
end

-- A warp put in the list: what it holds counted, and the window made again when those no longer used hold too much
local function gpu_resident_add(path, extra_mb)
    if not path or path == GPU_HOLD_SHADER or not mpv_keeps_shaders() then return end
    if not gpu_resident[path] then
        local info = utils.file_info(path)
        gpu_resident[path] = (info and info.size or 0) * 2 / (1024 * 1024) + (extra_mb or 0)
    end
    local unused = 0
    for p, mb in pairs(gpu_resident) do if p ~= path then unused = unused + mb end end
    if unused > FLUSH_NOW_MB then
        gpu_flush(path)
    elseif unused > FLUSH_SOON_MB and not flush_due then
        flush_due = true
        mp.msg.info(string.format("mpv keeps ~%d MB of shaders it no longer uses: its window is made again with the next clip", unused))
    end
end

-- As the next clip opens (its predecessor's file-local glsl-shaders gone)
mp.register_event("start-file", function()
    if flush_due then gpu_flush(nil) end
end)

-- A key for the settings a shader was written with: the same for the same values, whatever order the table has
local function settings_key(v)
    if type(v) ~= "table" then return type(v) .. ":" .. tostring(v) end
    local parts = {}
    for k, x in pairs(v) do parts[#parts + 1] = tostring(k) .. "=" .. settings_key(x) end
    table.sort(parts)
    return "{" .. table.concat(parts, ",") .. "}"
end

local function gpu_remember(key, reply)
    local kept = {}
    for k, v in pairs(reply) do kept[k] = v end
    kept.ms = nil
    gpu_reuse[key] = { path = reply.shader, reply = kept, used = mp.get_time() }
    gpu_kept[reply.shader] = true
    local n, oldest = 0, nil
    for k, e in pairs(gpu_reuse) do
        n = n + 1
        if e.path ~= (gpu and gpu.shader) and (not oldest or e.used < gpu_reuse[oldest].used) then oldest = k end
    end
    if n > REUSE_MAX and oldest then
        local path = gpu_reuse[oldest].path
        gpu_reuse[oldest], gpu_kept[path] = nil, nil
        os.remove(path)
    end
end

mp.register_event("shutdown", function()
    for path in pairs(gpu_kept) do os.remove(path) end
end)

-- The shader in mpv's list (file-local), in place of the one before; the dlogm plugin keeps it in its own list.
-- extra_mb: what it holds on the GPU besides its text (gpu_resident_add)
local function gpu_set_shader(old, new, extra_mb)
    local list, done = {}, false
    for _, sh in ipairs(mp.get_property_native("glsl-shaders") or {}) do
        if old and sh == old then
            if new then table.insert(list, new) end
            done = true
        elseif sh ~= new then
            table.insert(list, sh)
        end
    end
    -- Ahead of everything but the plugin's own shaders: before the looks
    if new and not done then
        local at = #list + 1
        for i, sh in ipairs(list) do
            if sh ~= CHROMATIC_SHADER and sh ~= CROP_SHADER then at = i break end
        end
        table.insert(list, at, new)
    end
    mp.set_property_native("file-local-options/glsl-shaders", list)
    mp.commandv("script-message-to", "dlogm", "gyroflow-shader", old or "", new or "")
    if old and old ~= new and old ~= GPU_HOLD_SHADER and not gpu_kept[old] then os.remove(old) end
    gpu_resident_add(new, extra_mb)
end

-- The player's table server (gyroflow-gpu-table --serve): it keeps the clips it loaded, so a change of the view, the
-- edges, the color fringes or the denoise writes the shader alone (~40 ms, not ~0.4 s). One per player, going with
-- it; a request it can't take, the table tool does by itself. Not on Windows (a Unix socket)
local GPU_SERVER = not platform.windows and utils.join_path(platform.temp_dir(), string.format("mpv-gyroflow-%d.sock", utils.getpid())) or nil
local gpu_server_running = false
local function gpu_server()
    if not GPU_SERVER or gpu_server_running or not gpu_wanted() or not file_exists(GPU_TABLE) or not mpv_has_pts() then return end
    gpu_server_running = true
    mp.command_native_async({ name = "subprocess", args = { GPU_TABLE, "--serve", GPU_SERVER }, playback_only = false }, function()
        gpu_server_running = false
    end)
end
mp.register_event("shutdown", function() if GPU_SERVER then os.remove(GPU_SERVER) end end)
gpu_server()

-- A clip opening on the GPU path doesn't wait for its table: mpv opens it meanwhile (its decoder, window and audio,
-- ~0.7 s), paused on black (gpu_hold.glsl) until the warp is there. held: the pause is the plugin's
local gpu_held = false
local function gpu_unhold()
    if gpu_held then
        gpu_held = false
        mp.set_property_native("pause", false)
    end
end

-- Asks gyroflow-gpu-table for the clip's shader with the current settings, to done(reply): reply.shader, or nil and
-- reply.unsupported / reply.error
-- What gyroflow-gpu-table --player is asked for the clip as it plays now (the filter's own arguments): its project or
-- preset, the transform, the moment's part, the lens
local function gpu_user_data(source)
    local ud = utils.parse_json(gyroflow_filter(source, true).params["user-data"]) or {}
    ud.width, ud.height, ud.fast = nil, nil, nil
    ud.input_width, ud.input_height = source.width, source.height
    ud.gpu_chromatic = chromatic and not o.chromatic_skip
    ud.edges, ud.edges_blur = edge_mode, edge_blur
    return ud
end

-- Ctrl+j's translation compensation computed this many times this session (a new shader each time)
local trans_rev = 0

-- The shader applied (further down): the whole clip's table, once it's there, in place of its first seconds'
local gpu_apply

local function gpu_table(source, done)
    local ud = gpu_user_data(source)
    -- A clip without a project: its motion data from the NAS took seconds (20 s for a 1.5 GB clip, cold), so the
    -- table starts from a stretch of it, as the filter's path does (motion.rs): a moment's part (all it plays), or the
    -- seconds it starts playing at, the reply "partial", the whole clip's table asked for after (below)
    if not has_project(source) then ud.moment_end = moment_end() end
    -- Ctrl+p (the clean frame), unless this computer does none (temporal_denoise=off): on the GPU path the temporal
    -- denoise while it plays and paused (warp.glsl's dn_ passes; dn_sync delays the audio)
    ud.gpu_denoise = still.enabled()
    -- The translation compensation, where it's been computed for the clip (Ctrl+j); a new shader once it is
    ud.gpu_translation = o.translation and true or false
    ud.translation_rev = trans_rev > 0 and trans_rev or nil
    -- Written before with these settings (and the project as it is now): that file again
    local project = source.project and utils.file_info(source.project)
    local key = settings_key(ud) .. (project and string.format("|%s/%s", tostring(project.mtime), tostring(project.size)) or "")
    local hit = gpu_reuse[key]
    if hit and file_exists(hit.path) then
        hit.used = mp.get_time()
        local reply = {}
        for k, v in pairs(hit.reply) do reply[k] = v end
        reply.ms, reply.reused = 0, true
        mp.add_timeout(0, function() done(reply) end)
        return
    end
    local function ask(req, then_)
        gpu_count = gpu_count + 1
        local out = utils.join_path(platform.temp_dir(), string.format("mpv-gyroflow-%d-%d.glsl", utils.getpid(), gpu_count))
        local t = mp.get_time()
        local args = { GPU_TABLE, "--player", utils.format_json(req), out }
        if gpu_server_running then
            table.insert(args, "--server")
            table.insert(args, GPU_SERVER)
        end
        mp.command_native_async({ name = "subprocess", args = args, capture_stdout = true, playback_only = false }, function(_, r)
            local reply = r and r.status == 0 and utils.parse_json((r.stdout or ""):match("[^\n]*")) or nil
            if not reply then reply = { error = "gyroflow-gpu-table failed: " .. tostring(r and (r.error_string ~= "" and r.error_string or r.status)) } end
            reply.ms = math.floor((mp.get_time() - t) * 1000)
            reply.denoise = ud.gpu_denoise
            reply.translation = reply.translation == true
            then_(reply)
        end)
    end
    -- From where it plays now (0 as it opens), not kept by it: the whole clip's table is the same
    if not has_project(source) and not ud.moment_end then
        local req = {}
        for k, v in pairs(ud) do req[k] = v end
        req.start = (mp.get_property_number("time-pos") or 0) + (ud.time_offset or 0)
        return ask(req, function(reply)
            if not reply.partial then
                if reply.shader then gpu_remember(key, reply) end
                return done(reply)
            end
            done(reply)
            -- The whole clip's, once its motion data is all read (the table server reads it meanwhile), in place of
            -- the stretch's: unless something else took its place
            local first = reply.shader
            local whole = {}
            for k, v in pairs(ud) do whole[k] = v end
            whole.full = true
            ask(whole, function(full)
                if not full.shader then return mp.msg.info("the whole clip's motion data: " .. tostring(full.error or full.unsupported)) end
                gpu_remember(key, full)
                if gpu and gpu.shader == first and current_source == source then
                    gpu_apply(full)
                    mp.msg.info(string.format("the whole clip's motion data in %d ms", full.ms or 0))
                end
            end)
        end)
    end
    ask(ud, function(reply)
        if reply.shader then gpu_remember(key, reply) end
        done(reply)
    end)
end

-- What the shader renders, applied: the warp in the list, the video's rectangle in the output's shape, no
-- interpolation (PTS is the frame's, not the mix's)
-- The size the GPU path renders at (warp.glsl's RT: the warp, the edge fill, the denoise, the looks after them), as a
-- share of the video's rectangle on screen (gyroflow_render_scale); mpv scales it to the screen. resolution=display:
-- as on screen; 1080, 720: its short side at most that; auto: as on screen, at most 1080 while the denoise is in the
-- shader (paused too: a new size starts its ring of frames again, and the paused frame would have none around it);
-- video: the video's own, at most 4K's (the denoise's frames and the warp's grid are made for that much)
local render_scale = 1
-- The short side it renders at, for quality.lua (nil: not the GPU path)
local gpu_render_short = nil
local render_logged = nil
local function gpu_render_scale()
    if not gpu or not gpu.shader or gpu.sphere then return end
    local d = mp.get_property_native("osd-dimensions") or {}
    local dw, dh = (d.w or 0) - (d.ml or 0) - (d.mr or 0), (d.h or 0) - (d.mt or 0) - (d.mb or 0)
    if dw <= 0 or dh <= 0 then return end
    local s = 1
    local short = tonumber(o.resolution)
    if o.resolution == "auto" and gpu.denoise then short = 1080 end
    if o.resolution == "video" then
        local ow, oh = gpu.output and gpu.output[1], gpu.output and gpu.output[2]
        if ow and oh then s = math.min(ow / dw, oh / dh) end
        s = math.min(s, 3840 / dw, 2160 / dh)
    elseif short then
        s = math.min(1, short / math.min(dw, dh))
    end
    s = math.max(0.1, math.min(4, s))
    gpu_render_short = math.floor(math.min(dw, dh) * s + 0.5)
    local line = string.format("GPU path: rendering at %dx%d (%s), shown at %dx%d", math.floor(dw * s + 0.5), math.floor(dh * s + 0.5), o.resolution, dw, dh)
    if line ~= render_logged then render_logged = line; mp.msg.info(line) end
    if math.abs(s - render_scale) < 1e-4 and (mp.get_property_native("glsl-shader-opts") or {}).gyroflow_render_scale then return end
    render_scale = s
    local opts = mp.get_property_native("glsl-shader-opts") or {}
    opts.gyroflow_render_scale = string.format("%.4f", s)
    mp.set_property_native("glsl-shader-opts", opts)
end

gpu_apply = function(reply)
    -- The frame on screen, before the new shader (its ring of frames starts again from the newest)
    local shown = dn_sync.shown()
    gpu_set_shader(gpu and gpu.shader, reply.shader, GRID_MB + (reply.denoise and DENOISE_MB or 0))
    -- A file used before: mpv may give its parse back, with its ring of frames as it was left
    if reply.reused then dn_sync.new_ring() end
    local w, h = tonumber(reply.output and reply.output[1]), tonumber(reply.output and reply.output[2])
    local sw, sh = current_source.width, current_source.height
    if w and h and sw and sh and math.abs(w / h - sw / sh) > 0.005 then
        mp.set_property("file-local-options/video-aspect-override", string.format("%d:%d", w, h))
    else
        mp.set_property("file-local-options/video-aspect-override", "no")
    end
    mp.set_property("file-local-options/interpolation", "no")
    -- The RGB layout has mpv convert the whole source first: its chroma upscaled bilinearly, the warp resamples it
    -- anyway (the app's Lanczos took 10 ms of a 5.3K frame on an M1 Pro)
    mp.set_property("file-local-options/cscale", reply.mode == "grid" and "bilinear" or (mp.get_property("options/cscale") or "bilinear"))
    gpu = { shader = reply.shader, output = { w, h }, mode = reply.mode, chromatic = reply.chromatic,
            focal = type(reply.focal) == "table" and reply.focal or nil, denoise = reply.denoise or false, translation = reply.translation or false }
    mp.set_property_native("user-data/gyroflow/gpu", { mode = reply.mode or "", chromatic = reply.chromatic or false })
    gpu_render_scale()
    dn_sync(shown)
    -- The panel's sideways jerks: computed for this clip or not
    publish()
end

-- Crop mode's camera on the GPU path (crop.lua): the warp's gyroflow_cam_ parameters, set as it turns (no table
-- written again, nothing rendered again but the frame): gyroflow-core's view rotation (yaw about y, pitch about x, roll
-- about z, each turned the other way: FrameTransform::view_rotation) in its matrices' axes, y and z turned
-- (gpu_table::camera_quat), and the zoom times the FOV. nil: none
gpu_camera = function(c)
    local function axis(x, y, z, deg)
        local a = math.rad(deg) / 2
        return { x * math.sin(a), y * math.sin(a), z * math.sin(a), math.cos(a) }
    end
    local function mul(a, b)
        return { a[4] * b[1] + b[4] * a[1] + a[2] * b[3] - a[3] * b[2], a[4] * b[2] + b[4] * a[2] + a[3] * b[1] - a[1] * b[3],
                 a[4] * b[3] + b[4] * a[3] + a[1] * b[2] - a[2] * b[1], a[4] * b[4] - a[1] * b[1] - a[2] * b[2] - a[3] * b[3] }
    end
    local q, zoom = { 0, 0, 0, 1 }, 1
    if c then
        q = mul(mul(axis(0, 1, 0, -c.yaw), axis(1, 0, 0, -c.pitch)), axis(0, 0, 1, -c.roll))
        q = { q[1], -q[2], -q[3], q[4] }
        zoom = c.zoom
    end
    local opts = mp.get_property_native("glsl-shader-opts") or {}
    opts.gyroflow_cam_x, opts.gyroflow_cam_y, opts.gyroflow_cam_z, opts.gyroflow_cam_w = string.format("%.9g", q[1]), string.format("%.9g", q[2]),
        string.format("%.9g", q[3]), string.format("%.9g", q[4])
    opts.gyroflow_cam_zoom = string.format("%.9g", zoom)
    if not c then
        for _, k in ipairs({ "gyroflow_cam_x", "gyroflow_cam_y", "gyroflow_cam_z", "gyroflow_cam_w", "gyroflow_cam_zoom" }) do opts[k] = nil end
    end
    mp.set_property_native("glsl-shader-opts", opts)
end

-- The denoise on the GPU path (Ctrl+p; warp.glsl's dn_ passes) averages each frame with DN_AFTER frames after it and as
-- many before: the picture shown is DN_AFTER frames behind the newest decoded, so the audio is delayed as much
-- (audio-delay, by what this adds to it), and after a seek the first frame stays until the frames after it are there.
-- In crop mode none after it (gyroflow_dn_after 0): its camera turns the newest frame as it's dragged. Where that
-- changes, or the shader is new, the shader's ring of frames starts again from the newest frame: paused, the frame on
-- screen is sought to exactly, so the picture stays where it was. user-data/gyroflow/gpu_delay: { delay, restart } for
-- what's on screen (export.lua's photo)
local DN_AFTER = 5
local dn = { after = DN_AFTER, epoch = 0, delay = 0, restart = nil }
local function dn_publish() mp.set_property_native("user-data/gyroflow/gpu_delay", { delay = dn.delay, restart = dn.restart or -1 }) end
local function dn_shown()
    local t = mp.get_property_number("time-pos")
    if not t or dn.delay <= 0 then return t end
    return math.max(math.min(dn.restart or t, t), t - dn.delay)
end
-- The ring started again by its epoch (a shader file used before, which mpv may give back with its ring as it was)
local function dn_new_ring()
    dn.epoch = dn.epoch + 1
    local opts = mp.get_property_native("glsl-shader-opts") or {}
    opts.gyroflow_dn_after, opts.gyroflow_dn_epoch = tostring(dn.after), tostring(dn.epoch)
    mp.set_property_native("glsl-shader-opts", opts)
end
-- shown: the frame on screen before a new shader (gpu_apply), nil otherwise
dn_sync = setmetatable({ shown = dn_shown, new_ring = dn_new_ring }, { __call = function(_, shown)
    local crop_on = (mp.get_property_native("user-data/gyroflow/crop") or {}).active == true
    local frames = DENOISE_FRAMES[o.temporal_denoise] or DN_AFTER
    local after = crop_on and 0 or math.min(DN_AFTER, frames)
    -- The frames on each side (Full or Light) and the grain kept: the shader's own parameters, nothing rebuilt
    local want = { gyroflow_dn_radius = string.format("%d", frames), gyroflow_dn_keep = string.format("%.3f", o.still_grain),
                   gyroflow_dn_strong = denoise_strong and "1" or "0" }
    local opts = mp.get_property_native("glsl-shader-opts") or {}
    local differ = false
    for k, v in pairs(want) do if opts[k] ~= v then opts[k], differ = v, true end end
    if differ then mp.set_property_native("glsl-shader-opts", opts) end
    local fps = current_source and tonumber(current_source.fps)
    local on = gpu ~= nil and gpu.shader ~= nil and gpu.denoise and fps and fps > 0
    local before = shown or dn_shown()
    local restart = shown ~= nil
    if after ~= dn.after then
        dn.after, dn.epoch = after, dn.epoch + 1
        local opts = mp.get_property_native("glsl-shader-opts") or {}
        opts.gyroflow_dn_after, opts.gyroflow_dn_epoch = tostring(after), tostring(dn.epoch)
        mp.set_property_native("glsl-shader-opts", opts)
        restart = true
    end
    local delay = on and after / fps or 0
    if delay ~= dn.delay then
        mp.set_property_number("audio-delay", (mp.get_property_number("audio-delay") or 0) - dn.delay + delay)
        dn.delay = delay
    end
    if restart then
        local t = mp.get_property_number("time-pos")
        if before and t and mp.get_property_native("pause") and fps and math.abs(t - before) > 0.25 / fps then
            mp.commandv("seek", string.format("%.6f", before), "absolute+exact")
            t = before
        end
        dn.restart = t
    end
    dn_publish()
end })
mp.register_event("playback-restart", function()
    dn.restart = mp.get_property_number("time-pos")
    dn_publish()
end)
mp.observe_property("user-data/gyroflow/crop", "native", function() dn_sync() end)
-- A new size of the picture starts the ring again too
mp.observe_property("osd-dimensions", "native", function()
    gpu_render_scale()
    if dn.delay > 0 then dn_sync(dn_shown()) end
end)

-- What add_filter would play the clip with, when render_args asks for it instead
local render_dump = nil

local function add_filter(source, done)
    done = done or function() end
    current_source, full_size = source, false
    if window_resize then
        mp.set_property("auto-window-resize", window_resize)
        window_resize = nil
    end
    -- The bundle saved for it, else the session's; then the input's turn and lens. With a Gyroflow project, what it
    -- has is its own: the horizon and frame. Not the view: saved for the clip it plays so (on, wider: the project's
    -- zoom left out, off: the project not used), with none saved the project as it is; nor the output's lens (as
    -- recorded, or a projection in place of the project's), with none saved the project's
    local saved = fixed()
    local off = take(saved)
    clip_settings(source)
    if has_project(source) then
        if type(saved) ~= "table" or not saved.view then view, off = "normal", false end
        horizon = nil
        over_project(saved)
        frame.aspect, frame.orientation, frame.rotation = "original", "as-is", 0
        -- What it has, for the panel: its horizon lock, output size and projection
        local j = read_json(source.project) or {}
        local st, out = type(j.stabilization) == "table" and j.stabilization or {}, type(j.output) == "table" and j.output or {}
        source.info = { horizon = tonumber(st.horizon_lock_amount) or 0, width = tonumber(out.output_width), height = tonumber(out.output_height),
                        projection = type(st.output_projection) == "string" and st.output_projection or nil }
    end
    stab_off = off
    write_edge_control()

    -- render_args (afp-stream, tools/stream): what it would play with, written down instead of played
    if o.render_args ~= "" then
        render_dump = { stabilize = filter_lens(source) ~= nil, kind = source.kind, user_data = gpu_user_data(source) }
        return done()
    end

    if gpu_eligible(source) then
        if o.hwdec then mp.set_property("file-local-options/hwdec", "auto") end
        mp.set_property("file-local-options/video-rotate", "no")
        -- Opening: mpv opens the clip while the table is written, paused on black; the window in the shape it will
        -- have (the project's output, or the frame's), corrected if the table says otherwise
        local opening = mp.get_property_native("time-pos") == nil
        if opening then
            gpu_set_shader(gpu and gpu.shader, GPU_HOLD_SHADER)
            gpu = { shader = GPU_HOLD_SHADER, hold = true }
            local sz = render_sizes(source)
            local w, h = source.out_width or source.width, source.out_height or source.height
            if sz.output then w, h = sz.output[1], sz.output[2] end
            if w and h and source.width and source.height and math.abs(w / h - source.width / source.height) > 0.005 then
                mp.set_property("file-local-options/video-aspect-override", string.format("%d:%d", w, h))
            end
            if not mp.get_property_native("pause") then
                gpu_held = true
                mp.set_property_native("pause", true)
            end
        end
        gpu_pending = true
        gpu_table(source, function(reply)
            local again = gpu_pending == "again"
            gpu_pending = false
            if reply.shader and current_source == source then
                gpu_apply(reply)
                gpu_unhold()
                mp.msg.info(string.format("stabilizing on the GPU with %s%s (%s%s), its table in %d ms", source.project, KIND_LABELS[source.kind],
                    reply.mode or "", reply.chromatic and ", color fringes taken out" or "", reply.ms or 0))
                if again then gpu_refresh() end
                if not opening then done() end
                return
            end
            if current_source ~= source then return end
            gpu_blocked[mp.get_property("path") or ""] = reply.unsupported or reply.error or "?"
            mp.msg.info("the GPU path can't: " .. tostring(reply.unsupported or reply.error) .. "; through VapourSynth")
            gpu_unhold()
            if opening then return play_again() end
            gpu = nil
            dn_sync()
            add_filter(source, done)
        end)
        if opening then done() end
        return
    end
    gpu_unhold()

    local sizes = render_sizes(source)
    source.sizes = sizes.key
    local hwdec, scale = input_scaling(sizes, true)
    if hwdec then mp.set_property("file-local-options/hwdec", hwdec) end
    -- The rest of its motion data comes from what mpv reads ahead (vs-gyroflow/src/motion.rs): further than the
    -- look-ahead, with room. From a NAS mpv's cache reads further anyway (mpv treats it as a network stream)
    if not has_project(source) and o.lookahead > 0 then
        local ahead = o.lookahead + 8
        if (mp.get_property_number("demuxer-readahead-secs") or 0) < ahead then
            mp.set_property_number("file-local-options/demuxer-readahead-secs", ahead)
        end
    end
    local vf = mp.get_property_native("vf") or {}
    local step = step_filter(source, true)
    if step then table.insert(vf, step) end
    if scale then table.insert(vf, scale) end
    table.insert(vf, gyroflow_filter(source, filter_lens(source) ~= nil))
    mp.set_property_native("file-local-options/vf", vf)
    -- The plugin's frame is upright: mpv's own turn by the file's display matrix would turn it again
    mp.set_property("file-local-options/video-rotate", off and tostring(frame.rotation) or "no")
    mp.msg.info("stabilizing with " .. source.project .. KIND_LABELS[source.kind] .. (source.preset and (" + " .. source.preset) or "") .. ", " .. interpolation(source)
                .. (sizes.input and string.format(" (input scaled to %dx%d)", sizes.input[1], sizes.input[2]) or "")
                .. (sizes.output and string.format(" (rendered at %dx%d)", sizes.output[1], sizes.output[2]) or "") .. (view == "wide" and ", wider view" or "")
                .. ((has_project(source) or filter_lens(source) == "only") and "" or ", " .. level)
                .. (filter_lens(source) == "only" and ", the lens alone corrected" or filter_lens(source) == "recorded" and ", the lens as recorded" or "")
                .. ((horizon ~= nil and filter_lens(source) ~= "only") and (horizon and ", horizon locked" or ", horizon not locked") or "") .. (fast(source) and ", every other frame if it leaves edges" or ""))
    done()
end

-- The chain set up again (further down)
local place_filters
-- Whether the camera of a clip with a project next to it records when it's in the water, and how its file says it's
-- turned (gyroflow-probe; a clip without one was probed before it loaded), for the menu: it plays meanwhile
local function find_water(path, source)
    if not file_exists(PROBE) then return end
    mp.command_native_async({ name = "subprocess", args = { PROBE, path }, capture_stdout = true, playback_only = true }, function(_, r)
        local info = r and r.status == 0 and utils.parse_json(r.stdout or "") or nil
        if not info or current_source ~= source then return end
        -- The file's own turn, for the render size of a frame changed over the project's
        local turn = tonumber(info.rotation) or 0
        if turn ~= (source.rotation or 0) then
            source.rotation = turn
            if (frame.rotation ~= 0 or shown_aspect() ~= "original") and filtering() then place_filters(true) end
        end
        if info.water then
            source.water = true
            publish()
        end
    end)
end

-- A 360 camera's own file (see the top): its lens tracks joined, the plugin's 360 mode after them. None of the flat
-- clip's settings apply (the view, level, edges, crop mode, the still frame): current_source stays nil
local function add_sphere(source)
    local rig = source.rig
    rig360 = rig
    local tracks = math.max(1, math.floor(tonumber(rig.tracks) or 1))
    local vo = mp.get_property("vo") or ""
    local gpu_ok = gpu_wanted() and file_exists(GPU_TABLE) and mpv_has_pts() and (vo == "" or vo:find("gpu-next", 1, true)) and not gpu_blocked[mp.get_property("path") or ""]
    -- The app's mpv joins two lens tracks itself (tools/mpv-patches/0014, --video-join): each decoded on the GPU
    -- (VideoToolbox's zero-copy) and drawn side by side there, for the GPU path's sphere shader. Elsewhere lavfi-complex's
    -- hstack joins them in memory
    local join = gpu_ok and tracks == 2 and mp.get_property_native("option-info/video-join") ~= nil
    if join then
        mp.set_property("file-local-options/video-join", "yes")
        -- The joined frame keeps the aspect view360 sets (the window's area, video-aspect-override), where the filter's
        -- sphere was a frame of its own: a window resized to it would change the area, and the aspect again
        mp.set_property("file-local-options/auto-window-resize", "no")
        if o.hwdec then mp.set_property("file-local-options/hwdec", "auto") end
    else
        if tracks > 1 then
            local inputs = ""
            for i = 1, tracks do inputs = inputs .. "[vid" .. i .. "]" end
            mp.set_property("file-local-options/lavfi-complex", inputs .. "hstack=inputs=" .. tracks .. "[vo]")
        end
        -- VideoToolbox's zero-copy has no device under lavfi-complex and turns into software decoding (docs/360-throughput-spike.md)
        if o.hwdec then mp.set_property("file-local-options/hwdec", platform.macos and "videotoolbox-copy" or "auto-copy") end
    end
    -- Each track decoded on a thread of its own: in the playback thread mpv decodes them one after the other, and two
    -- X5 tracks came at ~31 fps together where one alone comes at ~70 (with it, ~65 both; through the plugin 24 -> 30 fps
    -- at 7680, 27 -> 53 at 5760, without the screen)
    mp.set_property("file-local-options/vd-queue-enable", "yes")
    -- What it has for the view's heading modes, for view360 and its panel (the plugin finds the same for its stamp,
    -- reading the whole clip's motion and GPS: about a second for a few minutes of a clip)
    mp.command_native_async({ name = "subprocess", args = { PROBE, "--headings", source.project }, capture_stdout = true, playback_only = true }, function(_, r)
        if rig360 ~= rig then return end
        local h = r and r.status == 0 and utils.parse_json(r.stdout or "") or nil
        mp.set_property_native("user-data/gyroflow/headings360", type(h) == "table" and h or { error = "gyroflow-probe --headings failed" })
    end)
    -- The GPU path (docs/gpu-render.md, "360 on the GPU"): the sphere made by a shader from the joined tracks
    -- (gyroflow-gpu-table --sphere360: each lens through gyroflow-core's kernel, blended by its share, the stamp for the
    -- view shader), every frame, no VapourSynth. The clip opens meanwhile, paused on black
    if gpu_ok then
        gpu_set_shader(gpu and gpu.shader, GPU_HOLD_SHADER)
        gpu = { shader = GPU_HOLD_SHADER, hold = true, sphere = true }
        if not mp.get_property_native("pause") then
            gpu_held = true
            mp.set_property_native("pause", true)
        end
        gpu_count = gpu_count + 1
        local out = utils.join_path(platform.temp_dir(), string.format("mpv-gyroflow-%d-%d.glsl", utils.getpid(), gpu_count))
        local args = { GPU_TABLE, "--sphere360", source.project, out, tostring(o.sphere_width > 0 and o.sphere_width or 0),
                       tostring(o.stitching_distance > 0 and o.stitching_distance or 0), string.format("%.6f", time_offset() or 0) }
        local t = mp.get_time()
        mp.command_native_async({ name = "subprocess", args = args, capture_stdout = true, playback_only = false }, function(_, r)
            local reply = r and r.status == 0 and utils.parse_json((r.stdout or ""):match("[^\n]*")) or nil
            if rig360 ~= rig then return end
            if reply and reply.shader then
                gpu_set_shader(GPU_HOLD_SHADER, reply.shader, SPHERE_MB)
                gpu = { shader = reply.shader, sphere = true }
                -- The stamp is the shader's own (view360.glsl's LUMA pass would read none from the joined tracks)
                local opts = mp.get_property_native("glsl-shader-opts") or {}
                opts.gyroflow_stamp360 = "1"
                mp.set_property_native("glsl-shader-opts", opts)
                mp.set_property_native("user-data/gyroflow/gpu", { mode = "sphere", chromatic = false })
                gpu_unhold()
                mp.msg.info(string.format("360: %s %s, %d lenses in %d tracks of %sx%s, joined %s; the sphere %sx%s on the GPU, its table in %d ms", tostring(rig.brand),
                    tostring(rig.model), tonumber(rig.lenses) or 0, tracks, tostring(rig.width), tostring(rig.height), join and "on the GPU" or "in memory", tostring(reply.output and reply.output[1]),
                    tostring(reply.output and reply.output[2]), math.floor((mp.get_time() - t) * 1000)))
                return
            end
            local why = reply and reply.error or "gyroflow-gpu-table failed"
            gpu_blocked[mp.get_property("path") or ""] = why
            mp.msg.info("360: the GPU path can't: " .. tostring(why) .. "; through VapourSynth")
            gpu_set_shader(GPU_HOLD_SHADER, nil)
            gpu = nil
            dn_sync()
            gpu_unhold()
            play_again()
        end)
        return
    end
    local vf = mp.get_property_native("vf") or {}
    local fps = tonumber(source.fps)
    local step = (o.sphere_max_fps > 0 and fps and fps > o.sphere_max_fps) and math.ceil(fps / o.sphere_max_fps - 0.05) or 1
    if step > 1 then table.insert(vf, { name = "lavfi", label = STEP_LABEL, params = { graph = "framestep=" .. step } }) end
    table.insert(vf, {
        name = "vapoursynth",
        label = SPHERE_LABEL,
        params = {
            file = SPHERE_SCRIPT,
            ["user-data"] = utils.format_json({ plugin = PLUGIN, video = source.project, workers = o.workers, cache_mb = o.sphere_cache_mb,
                                                width = o.sphere_width > 0 and o.sphere_width or nil,
                                                distance = o.stitching_distance > 0 and o.stitching_distance or nil,
                                                time_offset = time_offset() }),
            ["concurrent-frames"] = "4",
        },
    })
    mp.set_property_native("file-local-options/vf", vf)
    mp.set_property("file-local-options/video-rotate", "no")
    mp.msg.info(string.format("360: %s %s, %d lenses in %d tracks of %sx%s, joined%s", tostring(rig.brand), tostring(rig.model), tonumber(rig.lenses) or 0,
                              tracks, tostring(rig.width), tostring(rig.height), step > 1 and string.format(", every %s frames", step) or ""))
end

-- The filter for the clip, to done() once it's decided
local function setup(done)
    current_source, stab_off, spherical, rig360 = nil, false, false, false
    render_dump = nil
    if gpu then gpu_set_shader(gpu.shader, nil) end
    gpu = nil
    dn_sync()
    -- Crop mode's camera stays with the clip it was turned on
    if (mp.get_property_native("glsl-shader-opts") or {}).gyroflow_cam_w then gpu_camera(nil) end
    if (mp.get_property_native("glsl-shader-opts") or {}).gyroflow_stamp360 then
        local opts = mp.get_property_native("glsl-shader-opts")
        opts.gyroflow_stamp360 = nil
        mp.set_property_native("glsl-shader-opts", opts)
    end
    mp.set_property_native("user-data/gyroflow/gpu", false)
    mp.set_property_native("user-data/gyroflow/headings360", false)
    -- What's saved for it, else the session's; the source's input (a clip without motion data shows the frame and turn
    -- too, by mpv's own crop and rotation)
    frame = { aspect = "original", orientation = "as-is", rotation = 0, projection = "linear" }
    stab_off = take(fixed())
    clip_settings(nil)
    if resume and resume.path == mp.get_property("path") and resume.pos then mp.set_property("file-local-options/start", tostring(resume.pos)) end
    resume = nil
    local path = clip_path()
    if not file_exists(PLUGIN) then
        if find_project(path) then mp.msg.warn("plugin missing at " .. PLUGIN) end
        return done()
    end
    find_source(path, function(source)
        if source and source.kind == "rig360" then
            add_sphere(source)
            return done()
        end
        if not source then return done() end
        add_filter(source, function()
            done()
            if source.kind == "project" then find_water(path, source) end
        end)
    end)
end

-- Loading waits for the probe, with the script free meanwhile: quitting doesn't wait for it
mp.add_hook("on_load", 50, function(hook)
    hook:defer()
    local slow = mp.add_timeout(0.7, function() mp.osd_message("Gyroflow: reading the clip…", 30) end)
    setup(function()
        if not slow:is_enabled() then mp.osd_message("", 0) end
        slow:kill()
        -- Held for a clip before this one that didn't get to play on the GPU path
        if not (gpu and gpu.hold) then gpu_unhold() end
        publish()
        hook:cont()
    end)
end)

-- The clip loaded again from where it plays: to stabilize it with a project that wasn't there when it loaded
play_again = function()
    resume = { path = mp.get_property("path"), pos = mp.get_property_number("time-pos") }
    mp.commandv("playlist-play-index", "current")
end

-- What it does until the first frame shows: the filter reads the clip's motion and is set up (seconds from a NAS),
-- and the window, there at once (the Windows app's force-window=immediate), stays black meanwhile. The label as
-- before, and past a second without a frame, that the first one is being prepared, until it shows
local first_frame = { timer = nil, observer = nil, waiting = false }
local function first_frame_done()
    if first_frame.timer then first_frame.timer:kill() end
    if first_frame.observer then mp.unobserve_property(first_frame.observer) end
    local was = first_frame.waiting
    first_frame.timer, first_frame.observer, first_frame.waiting = nil, nil, false
    return was
end
local function first_frame_message(label)
    first_frame_done()
    mp.osd_message(label, 1.5)
    if mp.get_property_native("video-frame-info") ~= nil then return end
    first_frame.observer = function(_, info)
        if info == nil then return end
        if first_frame_done() then
            mp.msg.verbose("the first frame shows")
            mp.osd_message(label, 1.5)
        end
    end
    mp.observe_property("video-frame-info", "native", first_frame.observer)
    first_frame.timer = mp.add_timeout(1, function()
        first_frame.timer = nil
        first_frame.waiting = true
        mp.msg.verbose("no frame yet: the first one is being prepared")
        mp.osd_message(label .. "\nPreparing the first frame…", 120)
    end)
end
mp.register_event("end-file", function()
    if first_frame_done() then mp.osd_message("", 0) end
end)

mp.register_event("file-loaded", function()
    -- The lens, for the Source tab: read after it starts, so the start doesn't wait for it
    current_lens = nil
    local lens_for = clip_path()
    if file_exists(PROBE) and lens_for and not lens_for:find("://", 1, true) then
        mp.command_native_async({ name = "subprocess", args = { PROBE, "--lens", lens_for }, capture_stdout = true, playback_only = true }, function(_, r)
            local info = r and r.status == 0 and utils.parse_json(r.stdout or "") or nil
            if type(info) == "table" and clip_path() == lens_for then
                -- With lens types (the curated list): the type given by hand, the one recorded, and the camera's own
                -- number for its mode, for "Unknown (mode 6)" while the list doesn't name it
                local label_of = {}
                for _, t in ipairs(type(info.types) == "table" and info.types or {}) do
                    if type(t) == "table" and t.id then label_of[t.id] = t.label or t.id end
                end
                local typ = type(info.type) == "string" and info.type or type(info.recorded) == "string" and info.recorded or nil
                current_lens = { camera = info.camera or "", lens = info.lens or "", mode = tonumber(info.mode),
                                 type_label = typ and (label_of[typ] or typ) or nil }
                publish()
            end
        end)
    end
    local label
    if current_source and current_source.kind == "lens" then
        if filtering() then label = "Gyroflow: no motion data, the lens corrected" end
    elseif current_source and filtering() then
        label = "Gyroflow: " .. (stab_off and "the lens corrected, not stabilized" or "stabilized") .. KIND_LABELS[current_source.kind] .. (has_project(current_source) and "" or ", " .. level)
                .. (view == "wide" and ", wider view" or "")
    end
    if label then first_frame_message(label) end
end)

-- Sticks for the next clips, like the dlogm plugin's modes
local function set_edges(mode)
    local was_off = edge_mode == "off"
    edge_mode = mode
    write_edge_control()
    -- Filling edges or not decides the rate of a fast clip that leaves some (gyroflow.vpy)
    if current_source and fast(current_source) and (edge_mode == "off") ~= was_off then rebuild_filter() end
    publish()
    mp.osd_message("Gyroflow edges: " .. EDGE_LABELS[edge_mode], 2)
end

mp.add_key_binding("Ctrl+b", "edges", function()
    local i = 1
    for k, m in ipairs(EDGE_MODES) do
        if m == edge_mode then i = k end
    end
    set_edges(EDGE_MODES[i % #EDGE_MODES + 1])
end)

mp.register_event("shutdown", function() os.remove(EDGE_CONTROL) end)

-- The input scaling and the stabilization set up again in the filter chain where they are, for the sizes as they are
-- now (the display's); the decoding with them, when the input scaling comes or goes (the Mac's: on the GPU or not).
-- full: at full size, for the paused frame (still.lua)
place_filters = function(enabled, full)
    if gpu then return gpu_refresh() end
    full_size = full or false
    -- The window stays as it is at full size, which mpv would grow it to (up to autofit-larger) while paused; mpv's own
    -- setting back once the frame is at the playback size again
    if full_size and not window_resize then
        window_resize = mp.get_property("auto-window-resize")
        mp.set_property("auto-window-resize", "no")
    elseif not full_size and window_resize then
        local function restore()
            mp.unregister_event(restore)
            if window_resize and not full_size then mp.set_property("auto-window-resize", window_resize) end
            if not full_size then window_resize = nil end
        end
        mp.register_event("playback-restart", restore)
    end
    local sizes = render_sizes(current_source, full_size)
    current_source.sizes = render_sizes(current_source).key
    -- On the Mac, the input at its own size through the GPU scaler when playing scales it there: the decoding stays as
    -- it is, not set up again for the pause and again to play on
    if full_size and platform.macos and not sizes.input and render_sizes(current_source).input then
        sizes.input = { current_source.width, current_source.height }
    end
    local hwdec, scale = input_scaling(sizes, enabled)
    local vf, at = {}, nil
    for _, f in ipairs(mp.get_property_native("vf") or {}) do
        if f.label == LABEL then
            at = #vf + 1
            table.insert(vf, gyroflow_filter(current_source, enabled))
        elseif f.label ~= SCALE_LABEL and f.label ~= STEP_LABEL then
            table.insert(vf, f)
        end
    end
    if not at then return end
    if scale then table.insert(vf, at, scale) end
    local step = step_filter(current_source, enabled)
    if step then table.insert(vf, at, step) end
    if hwdec and hwdec ~= mp.get_property("file-local-options/hwdec") then mp.set_property("file-local-options/hwdec", hwdec) end
    mp.set_property_native("vf", vf)
end

-- The GPU path after a change (view, level, horizon, lens, edges, color fringes, the frame): the shader written again
-- with the new settings and put in place of the old, the playing frame redrawn with it. Settings the shader can't do
-- load the file again through the filter, from where it plays
gpu_refresh = function()
    if not gpu or not current_source then return end
    if gpu_pending then gpu_pending = "again" return end
    if not filter_lens() then
        -- Off, as recorded: the video as it is
        local shown = dn_sync.shown()
        gpu_set_shader(gpu.shader, nil)
        gpu.shader = nil
        mp.set_property("file-local-options/video-aspect-override", "no")
        dn_sync(shown)
        return
    end
    if not gpu_eligible(current_source) then
        gpu_blocked[mp.get_property("path") or ""] = "setting"
        return play_again()
    end
    gpu_pending = true
    local source = current_source
    gpu_table(source, function(reply)
        local again = gpu_pending == "again"
        gpu_pending = false
        if current_source ~= source or not gpu then return end
        if reply.shader then
            gpu_apply(reply)
        else
            gpu_blocked[mp.get_property("path") or ""] = reply.unsupported or reply.error or "?"
            mp.msg.info("the GPU path can't: " .. tostring(reply.unsupported or reply.error) .. "; through VapourSynth")
            return play_again()
        end
        if again then gpu_refresh() end
    end)
end
mp.register_event("shutdown", function() if gpu and gpu.shader then os.remove(gpu.shader) end end)
-- Ctrl+p on the GPU path: the denoise in or out of the shader
mp.observe_property("user-data/gyroflow/still", "native", function() if gpu then gpu_refresh() end end)

-- A change that sets the chain up again while it plays, paused through it with a word on screen: it plays on once
-- the first frame is through (or after a few seconds), instead of dropping frames to catch up
local function with_pause(message, change)
    -- The GPU path writes a shader in a tenth of a second, and mpv sets nothing up again (no playback-restart to wait
    -- for): it plays on
    if gpu then return change() end
    local paused = mp.get_property_native("pause")
    if not paused then mp.set_property_native("pause", true) end
    mp.osd_message(message, 10)
    change()
    if paused then return mp.osd_message("", 0) end
    local timer = nil
    local function resume()
        mp.unregister_event(resume)
        timer:kill()
        mp.add_timeout(0.3, function()
            mp.set_property_native("pause", false)
            mp.osd_message("", 0)
        end)
    end
    mp.register_event("playback-restart", resume)
    timer = mp.add_timeout(5, resume)
end

-- Paused, the frame at full size (still.lua): the chain set up for it while paused, and for playing again as it plays on
still.full_size({
    -- Playing renders it smaller, or skips frames
    needed = function()
        if not filtering() or full_size then return false end
        return render_sizes(current_source).key ~= render_sizes(current_source, true).key or frame_step(current_source) > 1 or fast(current_source)
    end,
    active = function() return full_size end,
    -- By itself on pausing (paused_full_size=auto), or only with Ctrl+f
    auto = function() return o.paused_full_size == "auto" end,
    set = function(on)
        if on then
            local out = render_sizes(current_source, true).output
            mp.msg.info(string.format("paused: stabilizing at full size, %dx%d", out and out[1] or current_source.out_width or current_source.width,
                                      out and out[2] or current_source.out_height or current_source.height))
            place_filters(true, true)
        else
            mp.msg.info("playing: stabilizing at " .. render_sizes(current_source).key)
            with_pause("", function() place_filters(true) end)
        end
    end,
})

-- The display the window is on, kept for the next start, and the stabilization set up for its size when that changes
-- (the window moved to another display, or the first clip loaded before the window was up), once the window settles
local display_timer = nil
local function display_changed()
    local w, h = mp.get_property_number("display-width"), mp.get_property_number("display-height")
    if not w or not h or w <= 0 or h <= 0 then return end
    local f = DISPLAY_STATE and io.open(DISPLAY_STATE, "w")
    if f then
        f:write(string.format("%d %d", w, h))
        f:close()
    end
    if display_timer then display_timer:kill() end
    display_timer = mp.add_timeout(0.5, function()
        display_timer = nil
        -- The GPU path's warp renders at whatever size the video has on screen: nothing to set up
        if not filtering() or o.resolution == "video" or gpu then return end
        local sizes = render_sizes(current_source)
        if sizes.key == current_source.sizes then return end
        mp.msg.info(string.format("display %dx%d: stabilizing at %s", w, h, sizes.key))
        with_pause("Gyroflow: setting up for this display…", function() place_filters(true) end)
    end)
end
mp.observe_property("display-width", "number", display_changed)
mp.observe_property("display-height", "number", display_changed)

local function no_source()
    mp.osd_message("Gyroflow: no project or motion data for this file", 1.5)
end

-- mpv's own turn and crop of the frame (further down)
local mpv_frame
-- The stabilization on (normal), on with the wider view (wide), or off for this clip; normal and wide stick
local function show(v)
    if not current_source then return no_source() end
    if current_source.kind == "lens" then
        return mp.osd_message("Gyroflow: no motion data in this clip, only its lens is corrected (Ctrl+e: optical flow in Gyroflow)", 2.5)
    end
    local before = filter_lens()
    local enabled = v ~= "off"
    if enabled then view = v end
    if enabled and not has_project(current_source) then sticks("view", v) end
    stab_off = not enabled
    write_edge_control()
    -- Between the two views the plugin switches by itself, on the next frame: the chain stays as it is. Not for a fast
    -- clip, whose rate depends on the view's edges (gyroflow.vpy), nor when what the filter does changes (on, off, the
    -- lens alone): the input scaling goes with it, no filter shows the video as it is
    if not (before ~= nil and before == filter_lens() and not fast(current_source)) then place_filters(filtering()) end
    mpv_frame()
    publish()
    mp.msg.info("stabilization " .. view_label(v) .. (filter_lens() == "only" and ", the lens corrected" or ""))
    mp.osd_message("Gyroflow: " .. view_label(v) .. (filter_lens() == "only" and ", the lens corrected" or ""), 1.5)
end

-- The lens corrected or as recorded (the transform's, apart from the stabilization); over a Gyroflow project too, saved
-- with the clip, and not the session's then
local function set_lens(v)
    if v == "toggle" then v = lens_mode == "corrected" and "as-recorded" or "corrected" end
    if v ~= "corrected" and v ~= "as-recorded" then return end
    if not current_source then
        lens_mode = v
        sticks("lens", v)
        publish()
        return mp.osd_message("Gyroflow: no lens profile for this clip", 2)
    end
    local before = filter_lens()
    lens_mode = v
    if not has_project(current_source) then sticks("lens", v) end
    if before ~= filter_lens() then
        with_pause("Gyroflow: the lens " .. (v == "corrected" and "corrected" or "as recorded") .. "…", function() place_filters(filtering()) end)
    end
    mpv_frame()
    publish()
    mp.osd_message("Gyroflow: the lens " .. (v == "corrected" and "corrected" or "as recorded"), 1.5)
end
mp.register_script_message("lens-mode", set_lens)

-- mpv's own turn and crop of the frame where the stabilization doesn't make it (off, or no motion data): the crop in
-- the decoded frame's own coordinates, before the turn
mpv_frame = function()
    if not mp.get_property("path") then return end
    -- Stabilized, the plugin's frame is upright already (the file's turn and the frame's): mpv turns it no more
    local rotation, crop = "no", ""
    if not filtering() then
        -- mpv's turn is added to the file's own (its display matrix), and the crop is in the decoded frame's coordinates
        rotation = tostring(frame.rotation)
        local d = mp.get_property_native("video-dec-params")
        local fa = shown_aspect()
        if fa ~= "original" and type(d) == "table" and d.w and d.h then
            local total = ((tonumber(d.rotate) or 0) + frame.rotation) % 360
            local fw, fh = frame_size(d.w, d.h, { rotation = total, aspect = fa })
            if total == 90 or total == 270 then fw, fh = fh, fw end
            fw, fh = math.min(fw, d.w), math.min(fh, d.h)
            -- The whole frame (9:16 turned a quarter): nothing to crop
            if fw < d.w - 1 or fh < d.h - 1 then crop = string.format("%dx%d+%d+%d", fw, fh, math.floor((d.w - fw) / 4) * 2, math.floor((d.h - fh) / 4) * 2) end
        end
    end
    if mp.get_property("file-local-options/video-rotate") ~= rotation then
        mp.set_property("file-local-options/video-rotate", rotation)
    end
    if (mp.get_property("file-local-options/video-crop") or "") ~= crop then mp.set_property("file-local-options/video-crop", crop) end
end
mp.observe_property("video-dec-params", "native", function() mpv_frame() end)

local ROTATION_LABELS = { [0] = "not turned", [90] = "turned 90° right", [180] = "turned 180°", [270] = "turned 90° left" }

-- The frame turned and cut to an aspect, for this clip: set up again where it's stabilized (its output size changes)
local PROJECTION_LABELS = { linear = "linear lens", stereographic = "stereographic fisheye", equidistant = "equidistant fisheye",
                            equisolid = "equisolid fisheye", orthographic = "orthographic fisheye", project = "the project's lens" }
local ORIENTATION_LABELS = { ["as-is"] = "as the source is", landscape = "landscape", portrait = "portrait" }
local function set_frame(aspect, rotation, projection, orientation)
    projection = projection or frame.projection
    orientation = orientation or frame.orientation
    if aspect == frame.aspect and rotation == frame.rotation and projection == frame.projection and orientation == frame.orientation then return end
    frame = { aspect = aspect, rotation = rotation, projection = projection, orientation = orientation }
    local shown = shown_aspect()
    local message = "Frame: " .. (aspect == "original" and "its own ratio" or aspect) .. ", " .. ORIENTATION_LABELS[orientation]
        .. (shown ~= "original" and shown ~= aspect and (" (" .. shown .. ")") or "") .. ", " .. ROTATION_LABELS[rotation]
        .. ", " .. PROJECTION_LABELS[projection]
    if filtering() then
        with_pause(message, function() place_filters(true) end)
    else
        mp.osd_message(message, 2)
    end
    mpv_frame()
    publish()
end

-- What a Gyroflow project has is changed only there
local function project_owns(what)
    if not has_project(current_source) then return false end
    mp.osd_message("Gyroflow: " .. what .. " as defined in the Gyroflow project (Ctrl+e edits it in Gyroflow)", 2.5)
    return true
end

-- The output's projection; over a Gyroflow project too, for viewing and saved with the clip ("project": the project's
-- own), and not the session's then
local function set_projection(p)
    local project = has_project(current_source)
    if not (PROJECTIONS[p] or (p == "project" and project)) then return end
    set_frame(frame.aspect, frame.rotation, p)
    if not project then sticks("projection", p) end
    publish()
end
mp.register_script_message("projection", set_projection)

-- The Lens drop-down's pick (the side panel's): as-recorded, or corrected in a projection (linear, stereographic,
-- equidistant, equisolid, orthographic; "project": a Gyroflow project's own), both changed at once
mp.register_script_message("lens", function(k)
    if k == "as-recorded" or k == "corrected" then return set_lens(k) end
    local project = has_project(current_source)
    if not (PROJECTIONS[k] or (k == "project" and project)) then return end
    if lens_mode == "corrected" or not current_source then
        if lens_mode ~= "corrected" then set_lens("corrected") end
        return set_projection(k)
    end
    -- From as recorded: the filter set up once, for both
    lens_mode, frame.projection = "corrected", k
    if not project then sticks("lens", "corrected"); sticks("projection", k) end
    with_pause("Gyroflow: the lens corrected, " .. PROJECTION_LABELS[k] .. "…", function() place_filters(filtering()) end)
    mpv_frame()
    publish()
end)

-- The ratio, landscape-wise (original, 16:9, 4:3, 5:4, 1:1); a portrait one (9:16) is that ratio turned, portrait
mp.register_script_message("aspect", function(a)
    if not aspect_ok(a) or project_owns("the frame") then return end
    local ratio, orientation = split_aspect(a, frame.orientation)
    set_frame(ratio, frame.rotation, nil, orientation)
    sticks("aspect", ratio)
    sticks("orientation", orientation)
    publish()
end)

-- as-is (as the source is), landscape, portrait
mp.register_script_message("orientation", function(v)
    if not ORIENTATIONS[v] or project_owns("the frame") then return end
    set_frame(frame.aspect, frame.rotation, nil, v)
    sticks("orientation", v)
    publish()
end)

-- The source turned as it was recorded (input): right, left, a quarter turn clockwise or back; 0, 90, 180, 270: that
-- turn. Written into the source's own file at once (the workspace plugin), for all its moments and clips
mp.register_script_message("rotate", function(v)
    local r = v == "right" and (frame.rotation + 90) % 360 or v == "left" and (frame.rotation + 270) % 360 or tonumber(v)
    if not ROTATION_LABELS[r] or project_owns("the turn") then return end
    set_frame(frame.aspect, r)
    mp.commandv("script-message-to", "workspace", "input-set", "rotation", tostring(r))
end)

-- on -> wider view -> off -> on
mp.add_key_binding("Ctrl+g", "toggle", function()
    if stab_off then show("normal") else show(view == "normal" and "wide" or "off") end
end)

mp.register_script_message("view", function(v)
    if v == "normal" or v == "wide" or v == "off" then show(v) end
end)

-- The plugin smooths the clip again for it, on the next frame it reads the control file: the chain stays as it is
local function set_level(l)
    if not current_source then return no_source() end
    if has_project(current_source) then
        return mp.osd_message("Gyroflow: the project's own smoothing and zoom (edit them in Gyroflow, Ctrl+e)", 2)
    end
    level = l
    sticks("level", l)
    write_edge_control()
    publish()
    mp.msg.info("level " .. level)
    mp.osd_message("Gyroflow: " .. LEVEL_LABELS[level], 1.5)
end

-- weak -> medium -> strong -> weak
mp.add_key_binding("Ctrl+k", "level", function()
    for i, l in ipairs(LEVELS) do
        if l == level then return set_level(LEVELS[i % #LEVELS + 1]) end
    end
end)

mp.register_script_message("level", function(l)
    if LEVEL_LABELS[l] then set_level(l) end
end)

-- Level (locked) or Free; "project", from before, as the preset has it
mp.register_script_message("horizon-lock", function(value)
    if project_owns("the horizon") then return end
    if value == "project" then value = DEFAULTS.preset_horizon end
    if value == "toggle" then
        horizon = not horizon
    elseif HORIZON[value] ~= nil then
        horizon = HORIZON[value]
    else
        return
    end
    sticks("horizon_lock", horizon and "yes" or "no")
    rebuild_filter()
    publish()
    mp.osd_message("Gyroflow: horizon " .. (horizon and "level" or "free"), 1.5)
end)

local function set_underwater(value)
    if value == "project" or project_owns("the lens") then return end
    underwater = value
    if value ~= "no" then underwater_back = value end
    -- Input: the source's correction, into its own file at once
    mp.commandv("script-message-to", "workspace", "input-set", "underwater", value)
    rebuild_filter()
    publish()
    local says = current_source and current_source.kind == "default" and "preset" or "project"
    local label = UNDERWATER_LABELS[value]:gsub("project", says)
    local note = ""
    if value == "auto" and current_source and not current_source.water then
        note = ": this camera doesn't record it, so as the " .. says .. " says"
    end
    mp.msg.info(label .. note)
    mp.osd_message("Gyroflow: " .. label .. note, 2)
end

mp.register_script_message("underwater", function(value)
    if UNDERWATER_LABELS[value] then set_underwater(value) end
end)

-- Above water <-> what it was (under water, as the camera says, or as the project says)
mp.add_key_binding("Ctrl+u", "lens", function()
    set_underwater(underwater == "no" and underwater_back or "no")
end)

-- The color fringes taken out or not. The plugin reads it from the control file on the next frame; a paused frame is
-- rendered again (a seek to it sets the filter up again, which reads the file). It changes nothing where the lens is
-- above water: its marker moves no color either way
local function set_chromatic(value)
    if not chromatic_shader then
        return mp.osd_message("Gyroflow: color fringes are taken out only with vo=gpu-next or gpu", 2)
    end
    chromatic = value
    sticks("chromatic", value)
    write_edge_control()
    publish()
    if filtering() and mp.get_property_native("pause") then
        local pos = mp.get_property_number("time-pos")
        if pos then mp.commandv("seek", tostring(pos), "absolute+exact") end
    end
    local note = ""
    if chromatic and underwater == "no" then note = " (the lens is above water: nothing to take out)" end
    mp.msg.info("color fringes under water " .. (chromatic and "taken out" or "as recorded"))
    mp.osd_message("Gyroflow: color fringes under water " .. (chromatic and "taken out" or "as recorded") .. note, 2)
end

mp.register_script_message("chromatic", function(value)
    if value == "toggle" then
        set_chromatic(not chromatic)
    elseif value == "yes" or value == "no" then
        set_chromatic(value == "yes")
    end
end)

-- Ctrl+j: the hand's sideways jerks taken out (the translation compensation, warp.glsl's trans_shift). Computed for
-- the clip the first time (gyroflow-export --translation, as the clip plays: its project, level, view, horizon and
-- lens under water), kept in the cache; then off and on (gyroflow_trans). The panel's Transform has it too
local EXPORTER = utils.join_path(mp.get_script_directory(), platform.exe("gyroflow-export"))
local function trans_apply()
    local opts = mp.get_property_native("glsl-shader-opts") or {}
    local want = trans.on and "1" or "0"
    if opts.gyroflow_trans ~= want then
        opts.gyroflow_trans = want
        mp.set_property_native("glsl-shader-opts", opts)
    end
end
local function trans_compute()
    local source = current_source
    if not source or not source.project then return end
    local ud = gpu_user_data(source)
    local video, args = source.project, { EXPORTER, "--translation" }
    if video:lower():match("%.gyroflow$") then
        args[#args + 1], args[#args + 2] = "--project", video
        video = mp.get_property("path")
    end
    if ud.level then args[#args + 1], args[#args + 2] = "--level", tostring(ud.level) end
    if ud.underwater then args[#args + 1], args[#args + 2] = "--underwater", tostring(ud.underwater) end
    if tonumber(ud.horizon_lock) then args[#args + 1], args[#args + 2] = "--horizon-lock", string.format("%g", ud.horizon_lock) end
    if ud.wide == true or ud.wide == 1 then args[#args + 1] = "--wide" end
    local progress = utils.join_path(platform.temp_dir(), string.format("mpv-translation-%d.json", utils.getpid()))
    args[#args + 1], args[#args + 2], args[#args + 3] = "--progress", progress, video
    trans.progress = 0
    local poll = mp.add_periodic_timer(0.5, function()
        local f = io.open(progress, "r")
        local p = f and utils.parse_json(f:read("*a") or "") or nil
        if f then f:close() end
        trans.progress = tonumber(p and p.translation) or trans.progress
        mp.osd_message(string.format("Gyroflow: measuring the sideways jerks… %d %%", trans.progress), 1)
        publish()
    end)
    mp.osd_message("Gyroflow: measuring the sideways jerks…", 2)
    trans.job = mp.command_native_async({ name = "subprocess", args = args, capture_stdout = true, capture_stderr = true, playback_only = false }, function(_, r)
        poll:kill()
        trans.job = nil
        os.remove(progress)
        publish()
        local reply = r and r.status == 0 and utils.parse_json((r.stdout or ""):match("[^\n]*")) or nil
        if not (reply and reply.translation) then
            local why = r and (r.stderr or ""):match("failed, ([^\n]*)") or r and (r.stderr or ""):match("skipped, ([^\n]*)")
            return mp.osd_message("Gyroflow: the sideways jerks couldn't be measured" .. (why and (": " .. why) or ""), 4)
        end
        trans_rev, trans.on = trans_rev + 1, true
        trans_apply()
        if current_source == source then gpu_refresh() end
        local share = tonumber(reply.share) or 0
        mp.osd_message(share < 0.25 and "Gyroflow: little to take out of this clip's sideways jerks (no close subject to measure them on)"
            or string.format("Gyroflow: sideways jerks taken out, %.1f → %.1f px (Ctrl+j off and on)", tonumber(reply.before) or 0, tonumber(reply.after) or 0), 4)
    end)
    publish()
end
mp.add_key_binding("Ctrl+j", "translation", function()
    if not (gpu and gpu.shader) then
        return mp.osd_message("Gyroflow: the sideways jerks are taken out on the GPU path only", 3)
    end
    if not o.translation then return mp.osd_message("Gyroflow: translation compensation is off (gyroflow.conf translation=no)", 3) end
    if trans.job then return mp.osd_message("Gyroflow: still measuring the sideways jerks…", 2) end
    if not gpu.translation then return trans_compute() end
    trans.on = not trans.on
    trans_apply()
    publish()
    mp.osd_message("Gyroflow: sideways jerks " .. (trans.on and "taken out" or "left as recorded"), 2)
end)

mp.add_key_binding("Ctrl+a", "fringes", function() set_chromatic(not chromatic) end)

-- The temporal denoise's Stronger (the panel's Transform): kept with the clip's transform; the shader takes it at once
-- (gyroflow_dn_strong, nothing written again)
mp.register_script_message("denoise-strong", function(value)
    local v
    if value == "toggle" then v = not denoise_strong elseif value == "yes" or value == "no" then v = value == "yes" else return end
    denoise_strong = v
    sticks("denoise_strong", v)
    dn_sync()
    publish()
    local note = ""
    if v and not (gpu and gpu.denoise) then note = still.enabled() and " (while it plays on the GPU path)" or " (with Ctrl+p on)" end
    mp.msg.info("temporal denoise: " .. (v and "stronger" or "as usual"))
    mp.osd_message("Temporal denoise: " .. (v and "stronger, for this clip" or "as usual") .. note, 2)
end)

mp.register_script_message("edges", function(mode)
    if EDGE_LABELS[mode] then set_edges(mode) end
end)

-- The quality (quality.lua, the menu's Quality): what's picked is kept in gyroflow.conf at once, as a
-- setting of this computer's, and the chain set up again for a new rate or size
local QUALITY = {
    fps_limit = { ["0"] = "every frame", ["30"] = "up to 30 fps", ["15"] = "up to 15 fps" },
    resolution = { auto = "auto: fits the display, at most 1080p while the temporal denoise is on", display = "fits the display", ["1080"] = "up to 1080p", ["720"] = "up to 720p", video = "the video's own size" },
    edge_fill_rate = { auto = "edge fill rate: auto, as the GPU has room", ["50"] = "edge fill rate: 50 % of the frames", ["25"] = "edge fill rate: 25 % of the frames" },
    chromatic_skip = { yes = "color aberration correction skipped on this screen, for smooth playback", no = "color aberration correction as the clip's transform says" },
    ask_when_slow = { yes = "pauses and asks when it can't keep up", no = "plays on when it can't keep up" },
    paused_full_size = { off = "paused, the frame as it played (Ctrl+f: at full size)", auto = "paused, the frame at full size" },
    temporal_denoise = { full = "temporal denoise: full, 5 frames on each side", light = "temporal denoise: light, 3 frames on each side",
                         off = "temporal denoise: off on this computer" },
    edges = EDGE_LABELS,
}

local function set_quality(key, value)
    if not (QUALITY[key] and QUALITY[key][value]) then return end
    if key == "edges" then
        set_edges(value)
    elseif key == "fps_limit" then
        o.fps_limit = tonumber(value)
    elseif key == "resolution" then
        o.resolution = value
        gpu_render_scale()
    elseif key == "edge_fill_rate" then
        o.edge_fill_rate = value
        write_edge_control()
    elseif key == "chromatic_skip" then
        o.chromatic_skip = value == "yes"
        write_edge_control()
    elseif key == "paused_full_size" then
        o.paused_full_size = value
    elseif key == "temporal_denoise" then
        -- Light or full: the shader's parameters (dn_sync); off or back from it: still.lua says so, and the shader is
        -- written again without the denoise or with it
        o.temporal_denoise = value
        o.still_radius = DENOISE_FRAMES[value] or o.still_radius
        dn_sync()
        still.available_changed()
    else
        o.ask_when_slow = value == "yes"
    end
    if not platform.save_options("gyroflow", { { key, value } }) then mp.msg.warn("couldn't write gyroflow.conf") end
    quality.changed()
    publish()
    mp.msg.info("quality: " .. key .. " " .. value)
    local message = "Gyroflow: " .. (key == "edges" and "edges: " or "") .. QUALITY[key][value]
    if (key == "fps_limit" or key == "resolution") and filtering() then
        with_pause(message .. "…", function() place_filters(true) end)
    elseif key ~= "edges" then
        mp.osd_message(message, 2)
    end
end

mp.register_script_message("quality", set_quality)

quality.init(o, {
    stabilizing = function() return filtering() end,
    short_side = function()
        if not current_source then return nil end
        -- The GPU path's: what the shader renders, mpv scaling it to the screen
        if gpu and gpu.shader and not gpu.sphere and gpu_render_short then return gpu_render_short end
        local out = render_sizes(current_source).output
        local w, h = current_source.out_width or current_source.width, current_source.out_height or current_source.height
        if out then w, h = out[1], out[2] end
        return w and h and math.min(w, h) or nil
    end,
    edges = function() return edge_mode end,
    -- The color fringes being taken out where it can: the shader there, the lens not set above water
    chromatic = function() return chromatic and chromatic_shader and underwater ~= "no" and not o.chromatic_skip end,
    -- The temporal denoise as it runs while it plays (the GPU path's): full, light, or nil for none
    denoise = function() return gpu and gpu.denoise and o.temporal_denoise ~= "off" and o.temporal_denoise or nil end,
    set = set_quality,
})

-- The transform as it plays, whole, as the app's defaults and the session's (the panel's Use as default); what a
-- Gyroflow project has stays the session's. With the edge fill and the clean still frame, which aren't a clip's
local function as_plays()
    local project = has_project(current_source)
    return {
        view = project and session.view or view,
        level = project and session.level or level,
        horizon_lock = (project or horizon == nil) and session.horizon_lock or (horizon and "yes" or "no"),
        chromatic = chromatic == true,
        aspect = project and session.aspect or frame.aspect,
        orientation = project and session.orientation or frame.orientation,
        projection = project and session.projection or frame.projection,
        lens = project and session.lens or lens_mode,
        denoise = still.switched(),
        denoise_strong = denoise_strong,
    }
end

mp.register_script_message("remember", function()
    local b = as_plays()
    local saved = platform.save_options("gyroflow", {
        { "view", b.view }, { "level", b.level }, { "horizon_lock", b.horizon_lock },
        { "chromatic", b.chromatic and "yes" or "no" }, { "aspect", b.aspect }, { "orientation", b.orientation }, { "projection", b.projection },
        { "lens", b.lens }, { "edges", edge_mode }, { "edges_blur", string.format("%g", edge_blur) },
        { "still", b.denoise and "yes" or "no" }, { "denoise_strong", b.denoise_strong and "yes" or "no" },
    })
    if saved then
        o.still, o.denoise_strong = b.denoise, b.denoise_strong
        o.view, o.level, o.horizon_lock, o.chromatic, o.aspect, o.orientation, o.projection, o.lens = b.view, b.level, b.horizon_lock, b.chromatic, b.aspect, b.orientation, b.projection, b.lens
        DEFAULTS = defaults_bundle()
        for k, v in pairs(b) do session[k] = v end
        publish()
    end
    mp.osd_message(saved and "Gyroflow: the transform as it plays is the default now (gyroflow.conf)" or "Gyroflow: couldn't write gyroflow.conf", 2)
end)

-- The edge fill as the default (Performance & Quality's): this computer's, not a clip's. The temporal denoise's switch
-- is the clip's transform's (Use as default there)
mp.register_script_message("remember-quality", function()
    local saved = platform.save_options("gyroflow", {
        { "edges", edge_mode }, { "edges_blur", string.format("%g", edge_blur) },
    })
    mp.osd_message(saved and "Gyroflow: the edge fill is the default now (gyroflow.conf)" or "Gyroflow: couldn't write gyroflow.conf", 2)
end)

-- A bundle played now, for this viewing (the panel's Revert to saved): what a Gyroflow project has stays its own
mp.register_script_message("apply-view", function(json)
    local b = utils.parse_json(json or "")
    if type(b) ~= "table" then return end
    local turn = frame.rotation
    local off = take(b)
    frame.rotation = turn
    if has_project(current_source) then
        -- The bundle's view (on, wider, off over the project) and the output's lens stay; the rest is the project's
        if not b.view then view, off = "normal", false end
        horizon = nil
        over_project(b)
        frame.aspect, frame.orientation = "original", "as-is"
    end
    stab_off = off
    write_edge_control()
    if current_source then
        with_pause("Gyroflow: as saved…", function() place_filters(filtering()) end)
    end
    mpv_frame()
    publish()
end)

-- Edit in Gyroflow. The project goes next to the video (or where the workspace plugin says), where find_project()
-- picks it up from then on: the one it plays with, or for a video without one (its own motion data with the default
-- preset) the same written out by gyroflow-project, so Gyroflow opens it as it plays here. Gyroflow overwrites the
-- project it opened when it saves (Cmd+S), and the stabilization reloads with it while the video stays where it is.
-- A video with no motion data opens as it is: Gyroflow finds the motion from the picture (optical flow), and the
-- project it saves next to the video is waited for, then the video loads again to play with it.
local WATCH_INTERVAL = 0.5
local watch = nil

local function file_version(path)
    local info = utils.file_info(path)
    return info and info.is_file and string.format("%s/%d", tostring(info.mtime), info.size) or nil
end

local function stop_watching()
    if watch then watch.timer:kill() end
    watch = nil
    if waiting then
        waiting = false
        publish()
    end
end

local function reload_project(project, version)
    local w, h, fps, ow, oh = project_video_size(project)
    if not w then
        mp.msg.warn("not reloading " .. project .. ": not a Gyroflow project")
        return
    end
    -- The same video: its camera records the water or not as it did
    local source = { project = project, kind = "project", width = w, height = h, fps = fps, out_width = ow, out_height = oh,
                     water = current_source and current_source.water, sizes = current_source and current_source.sizes }
    -- The project as Gyroflow has it: written from the wider view, it has that view's FOV itself, and the horizon lock
    -- and the lens it played with here
    view = "normal"
    clip_settings(source)
    write_edge_control()
    local vf = mp.get_property_native("vf") or {}
    for i, f in ipairs(vf) do
        if f.label == LABEL then
            local filter = gyroflow_filter(source, f.enabled)
            -- mpv keeps a filter whose parameters didn't change, and the project's path may not have
            local data = utils.parse_json(filter.params["user-data"])
            data.version = version
            filter.params["user-data"] = utils.format_json(data)
            vf[i] = filter
        end
    end
    current_source = source
    mp.set_property_native("vf", vf)
    publish()
    mp.msg.info("reloaded " .. project)
    mp.osd_message("Gyroflow: project reloaded", 3)
end

-- Gyroflow writes the file in place: a version is taken once it stayed the same for an interval. For a video that
-- had no project, the first one found next to it, once it stays the same
local function check_project()
    local project = watch.project or find_project(watch.video)
    if not project then return end
    local v = file_version(project)
    if not v or v == watch.version then
        watch.pending = nil
    elseif v ~= watch.pending then
        watch.pending = v
    elseif not watch.project then
        -- Saved from Gyroflow beside the clip: into the project's catalog (the Library's motion data and badge)
        mp.commandv("script-message-to", "workspace", "catalog-note", project)
        mp.msg.info("found " .. project .. ", loading the video again with it")
        mp.osd_message("Gyroflow: project saved, stabilizing", 3)
        stop_watching()
        play_again()
    else
        watch.version, watch.pending = v, nil
        reload_project(watch.project, v)
    end
end

-- The command that opens the project (or a video) in Gyroflow and returns (status 0) once it's started, without
-- waiting for it. nil on Windows for a video without gyroflow_app: the app videos open with is the player's
local function gyroflow_command(file)
    if platform.macos then return { "/usr/bin/open", "-b", GYROFLOW_APP, file } end
    if platform.windows then
        if o.gyroflow_app ~= "" then return { "cmd", "/d", "/c", "start", "", o.gyroflow_app, file } end
        -- The app .gyroflow files are associated with (Gyroflow's installer sets it)
        if not file:lower():match("%.gyroflow$") then return nil end
        return { "cmd", "/d", "/c", "start", "", file }
    end
    -- Started in the background, and fails when there's no such program
    local app = o.gyroflow_app ~= "" and o.gyroflow_app or "gyroflow"
    return { "sh", "-c", 'command -v "$1" >/dev/null || exit 127; "$1" "$2" >/dev/null 2>&1 &', "sh", app, file }
end

-- `project`: the project to watch for changes; nil for a video without one (`file`, the video itself), to watch
-- for the project Gyroflow saves next to it
local function open_in_gyroflow(file, project, opened_message)
    local args = gyroflow_command(file)
    if not args then
        mp.osd_message("Gyroflow: set gyroflow_app in gyroflow.conf to open videos in it", 4)
        return
    end
    mp.set_property_bool("pause", true)
    local video = clip_path()
    if not (watch and watch.project == project and watch.video == video) then
        stop_watching()
        watch = { project = project, video = video, version = project and file_version(project) or nil }
        watch.timer = mp.add_periodic_timer(WATCH_INTERVAL, check_project)
    end
    waiting = project == nil
    publish()
    mp.command_native_async({ name = "subprocess", args = args, playback_only = false, capture_stderr = true }, function(_, r)
        if r and r.status == 0 then
            mp.osd_message(opened_message, 6)
        else
            mp.msg.warn("open: " .. (r and r.stderr or ""))
            mp.osd_message("Gyroflow: couldn't open the app (is Gyroflow installed?)", 4)
            stop_watching()
        end
    end)
end

local function edit_project(project)
    open_in_gyroflow(project, project, "Gyroflow: editing " .. platform.file_name(project) .. "\nsave it there to see it here")
end

mp.add_key_binding("Ctrl+e", "edit", function()
    local path = clip_path()
    if not path or path:find("://", 1, true) then
        mp.osd_message("Gyroflow: not a local file", 1.5)
        return
    end
    -- One saved meanwhile is the user's: open it as it is
    local project = find_project(path)
    if project then return edit_project(project) end
    if not current_source or current_source.kind == "lens" then
        return open_in_gyroflow(path, nil, "Gyroflow: opened " .. platform.file_name(path) .. ", which has no motion data:\n"
            .. "let it find the motion from the picture (optical flow) and save the project next to the video,\n"
            .. "then it plays here stabilized")
    end
    if not file_exists(PROJECT_WRITER) then
        mp.osd_message("Gyroflow: gyroflow-project isn't installed", 2)
        return
    end
    project = select(2, project_paths(path))
    -- As it plays here: the level, the view (off: the preset's), the horizon lock, the lens
    local args = { PROJECT_WRITER, "--underwater", underwater, "--level", level }
    if view == "wide" and not stab_off then table.insert(args, "--wide") end
    if horizon ~= nil then
        table.insert(args, "--horizon-lock"); table.insert(args, horizon and "100" or "0")
    end
    if current_source.preset then
        table.insert(args, "--preset"); table.insert(args, current_source.preset)
    end
    -- The frame as it plays: the project has it from then on (its output size, rotation and projection)
    if frame.rotation ~= 0 then table.insert(args, "--rotation"); table.insert(args, tostring(frame.rotation)) end
    if shown_aspect() ~= "original" then table.insert(args, "--aspect"); table.insert(args, shown_aspect()) end
    if PROJECTIONS[frame.projection] and frame.projection ~= "linear" then table.insert(args, "--projection"); table.insert(args, frame.projection) end
    -- A moment's own project: its part as the trim range, smoothed on its own or as part of the clip
    local ws = workspace_of(path)
    if ws and ws.moment and ws.moment["in"] and ws.snap then
        table.insert(args, "--trim"); table.insert(args, string.format("%.3f,%.3f", ws.moment["in"], ws.moment.out))
        table.insert(args, "--alone"); table.insert(args, ws.alone and "yes" or "no")
    end
    table.insert(args, path); table.insert(args, project)
    mp.osd_message("Gyroflow: writing the project...", 2)
    mp.command_native_async({ name = "subprocess", args = args, playback_only = false, capture_stderr = true }, function(_, r)
        if r and r.status == 0 then
            mp.msg.info("wrote " .. project)
            mp.commandv("script-message-to", "workspace", "catalog-note", project)
            edit_project(project)
        else
            local err = (r and r.stderr or ""):gsub("%s+$", "")
            mp.msg.warn("gyroflow-project: " .. err)
            mp.osd_message("Gyroflow: couldn't write the project: " .. (err:match("([^\n]+)$") or "failed"), 4)
        end
    end)
end)

mp.register_event("end-file", stop_watching)

-- export.lua's photos read it from here, and its live-photo message sets it
-- (no, live or motion; a Live Photo needs macOS's afp-photo)
do
    local v = tostring(o.live_photo)
    if v ~= "no" and v ~= "motion" and not (v == "live" and platform.macos) then v = platform.macos and "live" or "motion" end
    mp.set_property_native("user-data/gyroflow/live_photo", v)
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
        local out = { clip = clip_path(), offset = ws.offset, length = ws.length, moment = moment, render = render_dump,
                      dlogm = { curve = dl.curve or "", look = dl.look or "", log = dl.log == true },
                      title = mp.get_property("media-title"), duration = mp.get_property_number("duration") }
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
