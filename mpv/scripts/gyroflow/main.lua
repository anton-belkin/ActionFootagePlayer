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
--   Ctrl+b   edges    what fills the edges the stabilized frame doesn't cover: the project's background,
--                     the original frame, the frame zoomed, a trail of earlier frames, or the same trail kept by
--                     direction in the world (all blurred)
--   Ctrl+u   lens     the lens above water, and back to under water or as the camera says (what it was); it
--                     sticks for the next clips
--   Ctrl+p   still    the paused frame cleaned of its noise from the frames on both sides of it, on or off; it
--                     sticks (still.lua)
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
--   horizon-lock yes|no|project|toggle   lock the horizon, or not, or leave it to the project
--   underwater yes|auto|no  the lens under water, as the camera says, or above water
--   chromatic yes|no|toggle the water's color fringes taken out (Ctrl+a), or not
--   edges <mode>            what fills the edges (the modes of Ctrl+b)
--   remember                the view, horizon lock, lens, color fringes, edges and clean still frame as the defaults,
--                           in gyroflow.conf
-- and export.lua's. The state is in user-data/gyroflow/ for the menu: available, kind (project, default: the
-- preset), view (normal, wide, off), still (Ctrl+p),
-- horizon_lock (yes, no, project), underwater (yes, auto, no), water (the camera records when it's in the water),
-- chromatic (the color fringes taken out), chromatic_available (the shader is there: a gpu vo),
-- edges, edges_blur, edges_zoom, waiting (for a project from Gyroflow), exporting, last_export.
--
-- Options (~~/script-opts/gyroflow.conf):
--   hwdec=yes    decode the videos it stabilizes on the GPU (VideoToolbox; on Linux and Windows whatever mpv finds,
--                auto-copy), whatever mpv's hwdec is: software decoding doesn't keep up with 4K. no: leave it to
--                mpv's hwdec
--   workers=3    GPU worker threads
--   max_fps=0    every frame plays: the sphere trail is written as the GPU has room (vs-gyroflow/src/edges.rs).
--                Above 0, clips faster than this play every other frame (30: a 60 fps clip at 30) when the
--                stabilized frame leaves edges anywhere in the clip, as Gyroflow measures it for the project and
--                view (the wider view does), and they're filled (Ctrl+b not off), for a computer where the fill
--                doesn't keep up
--   view=normal  the view clips without a project start in: normal, or wide (the wider view)
--   horizon_lock=project  yes: the horizon locked level (Gyroflow's horizon lock at 100 %), no: not locked, project:
--                as the project or preset says
--   underwater=auto  the lens under water (Gyroflow's "Lens is underwater": the light bends where it enters the
--                lens), yes, or above water, no. auto: as the camera says over the clip, its In water / Out of water
--                (the dji-telemetry plugin's `water`; the Osmo Action 6 records it), and for clips from other
--                cameras as the project or preset says (the default preset: under water). vs-gyroflow/src/water.rs
--   chromatic=yes  the water's color fringes taken out where the lens is under water (Ctrl+a), or no: as recorded
--   edges=sphere what fills the edges: sphere (what was last seen in that direction), trail (what was last
--                shown there), original, zoom, off (the project's background color)
--   edges_blur=2 blur of the edge fill, % of the frame height
--   edges_zoom=1.5  how much the zoom fill scales the frame up
--   resolution=display  the stabilized frame no bigger than the display shows it (above), or video: at the video's own
--                size (up to 4K's pixels), whatever the display
--   zoom_margin=1.25  how much more input it keeps than the output shows, for the stabilization's zoom: the part of
--                the frame that shows reaches the output about 1:1 up to a zoom of this much
--   still=yes    the paused frame cleaned of its noise (Ctrl+p), from it and still_radius=5 frames on each side,
--                still_delay=0.4 s after it pauses; still_grain=0.25 of the brightness's grain stays, so it doesn't
--                look smeared (0: none)
--   gyroflow_app=   Gyroflow for Ctrl+e on Linux and Windows: its executable (default: gyroflow in PATH on Linux,
--                   the app .gyroflow files open with on Windows). macOS opens the app by its bundle id

local mp = require 'mp'
local options = require 'mp.options'
local utils = require 'mp.utils'

local o = { hwdec = true, workers = 3, max_fps = 0, view = "normal", horizon_lock = "project", underwater = "auto", chromatic = true, edges = "sphere", edges_blur = 2, edges_zoom = 1.5, resolution = "display", zoom_margin = 1.25, still = true, still_radius = 5, still_delay = 0.4, still_grain = 0.25, gyroflow_app = "" }
options.read_options(o, "gyroflow")

local platform = require 'platform'
local still = require 'still'

local LABEL = "gyroflow"
local SCALE_LABEL = "gyroflow-scale"
local DIR = mp.get_script_directory()
local PLUGIN = utils.join_path(DIR, platform.library("vs_gyroflow"))
local SCRIPT = utils.join_path(DIR, "gyroflow.vpy")
local PROBE = utils.join_path(DIR, platform.exe("gyroflow-probe"))
local PROJECT_WRITER = utils.join_path(DIR, platform.exe("gyroflow-project"))
local CHROMATIC_SHADER = utils.join_path(DIR, "chromatic.glsl")
local GYROFLOW_APP = "xyz.gyroflow"
-- Applied on top of Gyroflow's default preset to videos stabilized without a project: the first that exists
local DATA = os.getenv("GYROFLOW_DATA")
local PRESETS = {}
if DATA and DATA ~= "" then table.insert(PRESETS, utils.join_path(DATA, "default.gyroflow")) end
table.insert(PRESETS, utils.join_path(DIR, "default.gyroflow"))
-- Containers gyroflow-probe looks into: what DJI cameras record
local PROBE_EXTENSIONS = { mp4 = true, mov = true }
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

local function find_project(path)
    if not path or path:find("://", 1, true) then return nil end
    local base = path:match("^(.*)%.[^./\\]+$") or path
    for _, candidate in ipairs({ base .. ".gyroflow", path .. ".gyroflow" }) do
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
        done({ project = path, kind = info.mode, width = info.width, height = info.height, fps = info.fps, preset = preset, water = info.water })
    end)
end

local KIND_LABELS = { project = "", default = " (default preset)" }

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

-- The sizes to stabilize at: { input = { w, h } the video is scaled down to on the way (nil: as it is), output =
-- { w, h } the plugin renders (nil: the project's, or the input's), key: both, to tell a change }. The output fits the
-- display; the input keeps zoom_margin more, so the part of the frame that shows isn't enlarged
local function render_sizes(source)
    local w, h = source.width, source.height
    if not w or not h or w <= 0 or h <= 0 then return { key = "" } end
    local ow, oh = source.out_width or w, source.out_height or h
    local scale = 1
    local dw, dh = display_size()
    if o.resolution == "display" and dw then scale = math.min(1, dw / ow, dh / oh) end
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
local EDGE_MODES = { "off", "original", "zoom", "trail", "sphere" }
local EDGE_LABELS = { off = "background color", original = "original frame, blurred", zoom = "zoomed frame, blurred",
                      trail = "trail of earlier frames, blurred", sphere = "trail by direction, blurred" }
local EDGE_CONTROL = utils.join_path(platform.temp_dir(), string.format("mpv-gyroflow-%d.edges", utils.getpid()))
local edge_mode = EDGE_LABELS[o.edges] and o.edges or "sphere"
local edge_blur = o.edges_blur

-- What the clip plays stabilized with (find_source), nil: not stabilized
local current_source = nil

-- On, or on with a wider view (the plugin's wide FOV, see vs-gyroflow's wide_fov); off is per clip. The view clips
-- without a project start in sticks (sticky_view); a clip with a project starts in it as Gyroflow has it (normal), the
-- wider view a choice for that clip only
local sticky_view = o.view == "wide" and "wide" or "normal"
local view = sticky_view

-- The water's color fringes taken out (Ctrl+a), or left as recorded; sticks for the next clips
local chromatic = o.chromatic

-- The mode, then the blur, the view and the color fringes: "sphere blur=2 view=wide chromatic=yes". The plugin renders
-- both views from one engine, and switches between them on the next frame it reads this (vs-gyroflow's Views)
local function write_edge_control()
    local f = io.open(EDGE_CONTROL, "w")
    if not f then return mp.msg.warn("can't write " .. EDGE_CONTROL) end
    f:write(string.format("%s blur=%g view=%s chromatic=%s", edge_mode, edge_blur, view, chromatic and "yes" or "no"))
    f:close()
end
local function has_project(source)
    return source ~= nil and source.kind == "project"
end
local function view_label(v)
    if v == "wide" then return "on, wider view" end
    if v == "normal" then return has_project(current_source) and "on (Gyroflow project)" or "on" end
    return "off"
end
-- true or false: locked or not, whatever the project says; nil: the project's
local HORIZON = { yes = true, no = false }
local horizon = HORIZON[o.horizon_lock]
-- The lens under water (yes), above water (no), or as the camera says (auto); sticks for the next clips like the
-- horizon lock. A clip's camera that doesn't record it (source.water) leaves auto to the project or preset
local UNDERWATER_LABELS = { yes = "lens under water", no = "lens above water", auto = "lens under water as the camera says" }
local underwater = UNDERWATER_LABELS[o.underwater] and o.underwater or "auto"

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

-- Faster than max_fps (when it's set): such a clip plays every other frame if its stabilized frame leaves edges
-- anywhere and they're filled (gyroflow.vpy asks the plugin, which measures it with Gyroflow's zoom for any project and
-- view). Interpolation stays the one for the clip's own rate: at half rate there's headroom for the fill instead
local function fast(source)
    local fps = tonumber(source.fps)
    return o.max_fps > 0 and fps ~= nil and fps > o.max_fps + 1
end

local function gyroflow_filter(source, enabled)
    local output = render_sizes(source).output
    return {
        name = "vapoursynth",
        label = LABEL,
        enabled = enabled,
        params = {
            file = SCRIPT,
            ["user-data"] = utils.format_json({ plugin = PLUGIN, project = source.project, preset = source.preset, interpolation = interpolation(source), workers = o.workers, fast = fast(source),
                                                wide = view == "wide" or nil, horizon_lock = horizon == nil and nil or (horizon and 100 or 0), underwater = underwater, edges = edge_mode, edges_blur = edge_blur, edges_zoom = o.edges_zoom, edges_control = EDGE_CONTROL,
                                                chromatic = chromatic_shader or nil, width = output and output[1], height = output and output[2], still_control = still.CONTROL }),
            -- Frames mpv requests in parallel: one per GPU worker plus one being decoded. More only costs
            -- memory (an 8K 10-bit frame is ~100 MB); mpv's default (core count) makes 8K crawl
            ["concurrent-frames"] = "4",
        },
    }
end

-- Off for this clip (Ctrl+g)
local stab_off = false
-- Ctrl+e on a clip without motion data: the project Gyroflow will save for it is waited for
local waiting = false

-- The clean still frame needs the stabilization's filter in the chain, on
still.init(o, function()
    if not current_source or stab_off then return false end
    for _, f in ipairs(mp.get_property_native("vf") or {}) do
        if f.label == LABEL and f.enabled ~= false then return true end
    end
    return false
end)

-- For the right-click menu (mpv/menu.conf)
local function publish()
    local state = {
        available = current_source ~= nil, kind = current_source and current_source.kind or "",
        view = stab_off and "off" or view,
        horizon_lock = horizon == nil and "project" or (horizon and "yes" or "no"), edges = edge_mode, waiting = waiting,
        underwater = underwater, water = current_source and current_source.water or false,
        chromatic = chromatic, chromatic_available = chromatic_shader,
        edges_blur = edge_blur, edges_zoom = o.edges_zoom,
    }
    for k, v in pairs(state) do mp.set_property_native("user-data/gyroflow/" .. k, v) end
end

-- The filter set up again for current_source (the horizon lock, the lens, or the rate changing with the edge fill);
-- mpv rebuilds the filter chain, so it costs a few dropped frames
local function rebuild_filter()
    if not current_source then return end
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

local function add_filter(source)
    current_source = source
    view = has_project(source) and "normal" or sticky_view
    write_edge_control()

    local sizes = render_sizes(source)
    source.sizes = sizes.key
    local hwdec, scale = input_scaling(sizes, true)
    if hwdec then mp.set_property("file-local-options/hwdec", hwdec) end
    local vf = mp.get_property_native("vf") or {}
    if scale then table.insert(vf, scale) end
    table.insert(vf, gyroflow_filter(source, true))
    mp.set_property_native("file-local-options/vf", vf)
    mp.msg.info("stabilizing with " .. source.project .. KIND_LABELS[source.kind] .. (source.preset and (" + " .. source.preset) or "") .. ", " .. interpolation(source)
                .. (sizes.input and string.format(" (input scaled to %dx%d)", sizes.input[1], sizes.input[2]) or "")
                .. (sizes.output and string.format(" (rendered at %dx%d)", sizes.output[1], sizes.output[2]) or "") .. (view == "wide" and ", wider view" or "")
                .. (horizon ~= nil and (horizon and ", horizon locked" or ", horizon not locked") or "") .. (fast(source) and ", every other frame if it leaves edges" or ""))
end

-- Whether the camera of a clip with a project next to it records when it's in the water (gyroflow-probe; a clip
-- without one was probed before it loaded), for the menu: it plays meanwhile
local function find_water(path, source)
    if not file_exists(PROBE) then return end
    mp.command_native_async({ name = "subprocess", args = { PROBE, path }, capture_stdout = true, playback_only = true }, function(_, r)
        local info = r and r.status == 0 and utils.parse_json(r.stdout or "") or nil
        if info and info.water and current_source == source then
            source.water = true
            publish()
        end
    end)
end

-- The filter for the clip, to done() once it's decided
local function setup(done)
    current_source, stab_off = nil, false
    local path = mp.get_property("path")
    if resume and resume.path == path and resume.pos then mp.set_property("file-local-options/start", tostring(resume.pos)) end
    resume = nil
    if not file_exists(PLUGIN) then
        if find_project(path) then mp.msg.warn("plugin missing at " .. PLUGIN) end
        return done()
    end
    find_source(path, function(source)
        if source then add_filter(source) end
        done()
        if source and source.kind == "project" then find_water(path, source) end
    end)
end

-- Loading waits for the probe, with the script free meanwhile: quitting doesn't wait for it
mp.add_hook("on_load", 50, function(hook)
    hook:defer()
    local slow = mp.add_timeout(0.7, function() mp.osd_message("Gyroflow: reading the clip…", 30) end)
    setup(function()
        if not slow:is_enabled() then mp.osd_message("", 0) end
        slow:kill()
        publish()
        hook:cont()
    end)
end)

-- The clip loaded again from where it plays: to stabilize it with a project that wasn't there when it loaded
local function play_again()
    resume = { path = mp.get_property("path"), pos = mp.get_property_number("time-pos") }
    mp.commandv("playlist-play-index", "current")
end

mp.register_event("file-loaded", function()
    if current_source then mp.osd_message("Gyroflow: stabilized" .. KIND_LABELS[current_source.kind] .. (view == "wide" and ", wider view" or ""), 1.5) end
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
-- now (the display's); the decoding with them, when the input scaling comes or goes (the Mac's: on the GPU or not)
local function place_filters(enabled)
    local sizes = render_sizes(current_source)
    current_source.sizes = sizes.key
    local hwdec, scale = input_scaling(sizes, enabled)
    local vf, at = {}, nil
    for _, f in ipairs(mp.get_property_native("vf") or {}) do
        if f.label == LABEL then
            at = #vf + 1
            table.insert(vf, gyroflow_filter(current_source, enabled))
        elseif f.label ~= SCALE_LABEL then
            table.insert(vf, f)
        end
    end
    if not at then return end
    if scale then table.insert(vf, at, scale) end
    if hwdec and hwdec ~= mp.get_property("file-local-options/hwdec") then mp.set_property("file-local-options/hwdec", hwdec) end
    mp.set_property_native("vf", vf)
end

-- A change that sets the chain up again while it plays, paused through it with a word on screen: it plays on once
-- the first frame is through (or after a few seconds), instead of dropping frames to catch up
local function with_pause(message, change)
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
        if not current_source or stab_off or o.resolution ~= "display" then return end
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

-- The stabilization on (normal), on with the wider view (wide), or off for this clip; normal and wide stick
local function show(v)
    if not current_source then return no_source() end
    local was_on = not stab_off
    local enabled = v ~= "off"
    if enabled then view = v end
    if enabled and not has_project(current_source) then sticky_view = v end
    stab_off = not enabled
    write_edge_control()
    -- Between the two views the plugin switches by itself, on the next frame: the chain stays as it is. Not for a fast
    -- clip, whose rate depends on the view's edges (gyroflow.vpy), nor on or off: the input scaling goes with it, off
    -- shows the video as it is
    if not (was_on and enabled and not fast(current_source)) then place_filters(enabled) end
    publish()
    mp.msg.info("stabilization " .. view_label(v))
    mp.osd_message("Gyroflow: " .. view_label(v), 1.5)
end

-- on -> wider view -> off -> on
mp.add_key_binding("Ctrl+g", "toggle", function()
    if stab_off then show("normal") else show(view == "normal" and "wide" or "off") end
end)

mp.register_script_message("view", function(v)
    if v == "normal" or v == "wide" or v == "off" then show(v) end
end)

mp.register_script_message("horizon-lock", function(value)
    if value == "toggle" then
        horizon = not horizon
    elseif value == "project" then
        horizon = nil
    elseif HORIZON[value] ~= nil then
        horizon = HORIZON[value]
    else
        return
    end
    rebuild_filter()
    publish()
    mp.osd_message("Gyroflow: horizon " .. (horizon == nil and "as the project says" or (horizon and "locked" or "not locked")), 1.5)
end)

-- Under water or auto, before Ctrl+u took the lens out of the water
local underwater_back = underwater ~= "no" and underwater or "auto"

local function set_underwater(value)
    underwater = value
    if value ~= "no" then underwater_back = value end
    rebuild_filter()
    publish()
    local note = ""
    if value == "auto" and current_source and not current_source.water then
        note = ": this camera doesn't record it, so as the " .. (current_source.kind == "default" and "preset" or "project") .. " says"
    end
    mp.msg.info(UNDERWATER_LABELS[value] .. note)
    mp.osd_message("Gyroflow: " .. UNDERWATER_LABELS[value] .. note, 2)
end

mp.register_script_message("underwater", function(value)
    if UNDERWATER_LABELS[value] then set_underwater(value) end
end)

-- Above water <-> what it was (under water, or as the camera says)
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
    write_edge_control()
    publish()
    if current_source and not stab_off and mp.get_property_native("pause") then
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

mp.add_key_binding("Ctrl+a", "fringes", function() set_chromatic(not chromatic) end)

mp.register_script_message("edges", function(mode)
    if EDGE_LABELS[mode] then set_edges(mode) end
end)

mp.register_script_message("remember", function()
    local saved = platform.save_options("gyroflow", {
        { "view", sticky_view },
        { "horizon_lock", horizon == nil and "project" or (horizon and "yes" or "no") },
        { "underwater", underwater },
        { "chromatic", chromatic and "yes" or "no" },
        { "edges", edge_mode }, { "edges_blur", string.format("%g", edge_blur) },
        { "still", still.enabled() and "yes" or "no" },
    })
    mp.osd_message(saved and "Gyroflow: saved as the defaults (gyroflow.conf)" or "Gyroflow: couldn't write gyroflow.conf", 2)
end)

-- Edit in Gyroflow. The project goes next to the video, where find_project() picks it up from then on: the one it
-- plays with, or for a video without one (its own motion data with the default preset) the same written out by
-- gyroflow-project, so Gyroflow opens it as it plays here. Gyroflow overwrites the
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
    -- The project as Gyroflow has it: written from the wider view, it has that view's FOV itself
    view = "normal"
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
    local video = mp.get_property("path")
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
    local path = mp.get_property("path")
    if not path or path:find("://", 1, true) then
        mp.osd_message("Gyroflow: not a local file", 1.5)
        return
    end
    -- One saved meanwhile is the user's: open it as it is
    local project = find_project(path)
    if project then return edit_project(project) end
    if not current_source then
        return open_in_gyroflow(path, nil, "Gyroflow: opened " .. platform.file_name(path) .. ", which has no motion data:\n"
            .. "let it find the motion from the picture (optical flow) and save the project next to the video,\n"
            .. "then it plays here stabilized")
    end
    if not file_exists(PROJECT_WRITER) then
        mp.osd_message("Gyroflow: gyroflow-project isn't installed", 2)
        return
    end
    project = (path:match("^(.*)%.[^./\\]+$") or path) .. ".gyroflow"
    -- As it plays here: the view (off: the preset's), the horizon lock, the lens
    local args = { PROJECT_WRITER, "--underwater", underwater }
    if view == "wide" and not stab_off then table.insert(args, "--wide") end
    if horizon ~= nil then
        table.insert(args, "--horizon-lock"); table.insert(args, horizon and "100" or "0")
    end
    if current_source.preset then
        table.insert(args, "--preset"); table.insert(args, current_source.preset)
    end
    table.insert(args, path); table.insert(args, project)
    mp.osd_message("Gyroflow: writing the project...", 2)
    mp.command_native_async({ name = "subprocess", args = args, playback_only = false, capture_stderr = true }, function(_, r)
        if r and r.status == 0 then
            mp.msg.info("wrote " .. project)
            edit_project(project)
        else
            local err = (r and r.stderr or ""):gsub("%s+$", "")
            mp.msg.warn("gyroflow-project: " .. err)
            mp.osd_message("Gyroflow: couldn't write the project: " .. (err:match("([^\n]+)$") or "failed"), 4)
        end
    end)
end)

mp.register_event("end-file", stop_watching)

require 'export'
