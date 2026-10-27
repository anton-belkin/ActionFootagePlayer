-- gyroflow: videos stabilized with Gyroflow as mpv plays them. An mpv plugin: this folder goes into ~~/scripts/,
-- with the binaries built from vs-gyroflow/ in this repo (install.sh puts them here).
--
--   DJI_..._D.MP4  +  DJI_..._D.gyroflow   ->  plays through Gyroflow (vf=vapoursynth)
--   DJI_..._D.MP4 with a project embedded in it (gyroflow-embed)   ->  same
--   DJI_..._D.MP4 with gyro data, recorded with EIS off   ->  stabilized with the default preset (also GoPro clips
--       with HyperSmooth off and Insta360 ones with FlowState off in the camera, when Gyroflow has their lens):
--       Gyroflow's own default preset, then default.gyroflow if there is one: in $GYROFLOW_DATA (where an app
--       that ships this plugin read-only, ActionFootagePlayer, keeps yours), else in this folder (install.sh puts
--       the underwater preset there)
--
-- The last two are decided by gyroflow-probe, which reads the file's metadata (20-100 ms for a DJI clip, ~1 s from an
-- SD card; ~0.5 s more for GoPro and Insta360 clips it would stabilize, to find their lens in Gyroflow's database)
-- before playback starts. The motion data itself the plugin reads as the filter starts (~20 s for a 7 GB clip from an
-- SD card: issue #40).
--
-- The stabilization is the gyroflow VapourSynth plugin (libvs_gyroflow.dylib on macOS, libvs_gyroflow.so on Linux,
-- vs_gyroflow.dll on Windows), so it's the same as rendering the
-- project in Gyroflow, including the project's smoothing, zoom and output size. It runs before any shaders (the
-- dlogm plugin's D-Log M ones), at the source bit depth.
--
-- Video bigger than 4K (8K) is scaled down to 4K on the GPU (VideoToolbox) before it's copied to memory
-- for VapourSynth: copying, converting and uploading 8K frames left too little headroom in a window
-- with the HDR shader (dropped frames, a slow start). The window shows less than 4K anyway. On Linux and
-- Windows it's scaled on the CPU, after the copy.
--
-- Keys (rebind with `<key> script-binding gyroflow/<name>` in input.conf):
--   Ctrl+g   toggle   stabilization on, on with a wider view (no zoom, at the FOV where the stabilized picture covers about
--                     as much area as the frame, for each clip's lens: the edges show at times, filled as Ctrl+b
--                     says), off for the current file. On or wide sticks for the next clips
--   Ctrl+b   edges    what fills the edges the stabilized frame doesn't cover: the project's background,
--                     the original frame, the frame zoomed, a trail of earlier frames, or the same trail kept by
--                     direction in the world (all blurred)
--   Ctrl+u   lens     the lens above water, and back to under water or as the camera says (what it was); it
--                     sticks for the next clips
--   Shift+E  export   the clip as it plays (export.lua)
--   Ctrl+e   edit     open the clip's project in Gyroflow, writing one next to it first when it plays without one;
--                     saving it in Gyroflow reloads the stabilization here. A clip with no motion data opens as it
--                     is, for Gyroflow to find the motion from the picture (optical flow): once the project is saved
--                     next to it, it plays stabilized with it
--
-- Script messages (`script-message-to gyroflow <message>`, what the right-click menu runs; mpv/menu.conf):
--   view normal|wide|off    the stabilization on, on with the wider view, or off for this clip
--   horizon-lock yes|no|project|toggle   lock the horizon, or not, or leave it to the project
--   underwater yes|auto|no  the lens under water, as the camera says, or above water
--   edges <mode>            what fills the edges (the modes of Ctrl+b)
--   remember                the view, horizon lock, lens and edges as the defaults, in gyroflow.conf
-- and export.lua's. The state is in user-data/gyroflow/ for the menu: available, kind (project, embedded, default:
-- the preset), view (normal, wide, off),
-- horizon_lock (yes, no, project), underwater (yes, auto, no), water (the camera records when it's in the water),
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
--   view=normal  the view clips start in: normal, or wide (the wider view)
--   horizon_lock=project  yes: the horizon locked level (Gyroflow's horizon lock at 100 %), no: not locked, project:
--                as the project or preset says
--   underwater=auto  the lens under water (Gyroflow's "Lens is underwater": the light bends where it enters the
--                lens), yes, or above water, no. auto: as the camera says over the clip, its In water / Out of water
--                (the dji-telemetry plugin's `water`; the Osmo Action 6 records it), and for clips from other
--                cameras as the project or preset says (the default preset: under water). vs-gyroflow/src/water.rs
--   edges=sphere what fills the edges: sphere (what was last seen in that direction), trail (what was last
--                shown there), original, zoom, off (the project's background color)
--   edges_blur=2 blur of the edge fill, % of the frame height
--   edges_zoom=1.5  how much the zoom fill scales the frame up
--   gyroflow_app=   Gyroflow for Ctrl+e on Linux and Windows: its executable (default: gyroflow in PATH on Linux,
--                   the app .gyroflow files open with on Windows). macOS opens the app by its bundle id

local mp = require 'mp'
local options = require 'mp.options'
local utils = require 'mp.utils'

local o = { hwdec = true, workers = 3, max_fps = 0, view = "normal", horizon_lock = "project", underwater = "auto", edges = "sphere", edges_blur = 2, edges_zoom = 1.5, gyroflow_app = "" }
options.read_options(o, "gyroflow")

local platform = require 'platform'

local LABEL = "gyroflow"
local SCALE_LABEL = "gyroflow-scale"
local DIR = mp.get_script_directory()
local PLUGIN = utils.join_path(DIR, platform.library("vs_gyroflow"))
local SCRIPT = utils.join_path(DIR, "gyroflow.vpy")
local PROBE = utils.join_path(DIR, platform.exe("gyroflow-probe"))
local EMBED = utils.join_path(DIR, platform.exe("gyroflow-embed"))
local CHROMATIC_SHADER = utils.join_path(DIR, "chromatic.glsl")
local GYROFLOW_APP = "xyz.gyroflow"
-- Applied on top of Gyroflow's default preset to videos stabilized without a project: the first that exists
local DATA = os.getenv("GYROFLOW_DATA")
local PRESETS = {}
if DATA and DATA ~= "" then table.insert(PRESETS, utils.join_path(DATA, "default.gyroflow")) end
table.insert(PRESETS, utils.join_path(DIR, "default.gyroflow"))
-- Containers a project can be embedded in, and that DJI cameras record
local PROBE_EXTENSIONS = { mp4 = true, mov = true }
-- bilinear | bicubic | lanczos. Lanczos is sharper but slower: on the M1 Pro a 4K clip plays at ~38 fps of
-- capacity with it and ~54 with bicubic, so lanczos up to 30 fps and bicubic above (and when the rate's unknown)
local INTERPOLATION_SLOW = "lanczos"
local INTERPOLATION_FAST = "bicubic"
local SLOW_MAX_FPS = 31
-- Larger video is scaled down to this many pixels (aspect kept) before stabilizing
local MAX_INPUT_PIXELS = 3840 * 2160

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

-- The video size the project was made for (mpv doesn't know the size yet in on_load)
local function project_video_size(project)
    local json = read_json(project)
    local info = json and json.video_info
    if not info or not info.width or not info.height then return nil end
    return info.width, info.height, info.fps
end

-- What to stabilize the video with, to done(): { project = <.gyroflow or the video itself>, kind, width, height,
-- preset }, or nil. The probe isn't waited for in the script's thread: from an SD card it takes a second, and quitting
-- or the next clip ends it (playback_only)
local function find_source(path, done)
    local project = find_project(path)
    if project then
        local w, h, fps = project_video_size(project)
        return done({ project = project, kind = "project", width = w, height = h, fps = fps })
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

local KIND_LABELS = { project = "", embedded = " (embedded project)", default = " (default preset)" }

-- Even width and height that fit MAX_INPUT_PIXELS, or nil when the video already does
local function scaled_input_size(w, h)
    if not w or w * h <= MAX_INPUT_PIXELS then return nil end
    local s = math.sqrt(MAX_INPUT_PIXELS / (w * h))
    return math.floor(w * s / 2) * 2, math.floor(h * s / 2) * 2
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

-- The mode, then the blur: "sphere blur=2"
local function write_edge_control()
    local f = io.open(EDGE_CONTROL, "w")
    if not f then return mp.msg.warn("can't write " .. EDGE_CONTROL) end
    f:write(string.format("%s blur=%g", edge_mode, edge_blur))
    f:close()
end

-- On, or on with a wider view (the plugin's wide FOV, see vs-gyroflow's wide_fov), for the next clips too; off is per clip
local view = o.view == "wide" and "wide" or "normal"
local function view_label(v)
    if v == "wide" then return "on, wider view" end
    return v == "normal" and "on" or "off"
end
-- true or false: locked or not, whatever the project says; nil: the project's
local HORIZON = { yes = true, no = false }
local horizon = HORIZON[o.horizon_lock]
-- The lens under water (yes), above water (no), or as the camera says (auto); sticks for the next clips like the
-- horizon lock. A clip's camera that doesn't record it (source.water) leaves auto to the project or preset
local UNDERWATER_LABELS = { yes = "lens under water", no = "lens above water", auto = "lens under water as the camera says" }
local underwater = UNDERWATER_LABELS[o.underwater] and o.underwater or "auto"

-- The water's color fringes, taken out wherever the stabilization has the lens under water (its refraction, frame by
-- frame: vs-gyroflow/src/dispersion.rs); there's no switch of its own. The shader is in mpv's own glsl-shaders from
-- the start, ahead of the dlogm plugin's, which builds its list from them: it reads the frame's marker before the
-- noise reduction and the log curve. It leaves video without the marker as it is. The stabilization writes one
-- whenever the shader is there, as the shader paints it over; out of the water, one that moves no color. Only
-- vo=gpu-next, gpu and libmpv (an app's own window) run shaders: with another vo (or without the file) there's no
-- marker
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
    return {
        name = "vapoursynth",
        label = LABEL,
        enabled = enabled,
        params = {
            file = SCRIPT,
            ["user-data"] = utils.format_json({ plugin = PLUGIN, project = source.project, preset = source.preset, interpolation = interpolation(source), workers = o.workers, fast = fast(source),
                                                wide = view == "wide" or nil, horizon_lock = horizon == nil and nil or (horizon and 100 or 0), underwater = underwater, edges = edge_mode, edges_blur = edge_blur, edges_zoom = o.edges_zoom, edges_control = EDGE_CONTROL,
                                                chromatic = chromatic_shader or nil }),
            -- Frames mpv requests in parallel: one per GPU worker plus one being decoded. More only costs
            -- memory (an 8K 10-bit frame is ~100 MB); mpv's default (core count) makes 8K crawl
            ["concurrent-frames"] = "4",
        },
    }
end

local current_source = nil
-- Off for this clip (Ctrl+g)
local stab_off = false
-- Ctrl+e on a clip without motion data: the project Gyroflow will save for it is waited for
local waiting = false

-- For the right-click menu (mpv/menu.conf)
local function publish()
    local state = {
        available = current_source ~= nil, kind = current_source and current_source.kind or "",
        view = stab_off and "off" or view,
        horizon_lock = horizon == nil and "project" or (horizon and "yes" or "no"), edges = edge_mode, waiting = waiting,
        underwater = underwater, water = current_source and current_source.water or false,
        edges_blur = edge_blur, edges_zoom = o.edges_zoom,
    }
    for k, v in pairs(state) do mp.set_property_native("user-data/gyroflow/" .. k, v) end
end

-- The filter set up again for current_source (another view, or the rate changing with the edge fill); mpv rebuilds
-- the filter chain, so it costs a few dropped frames
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

local function add_filter(source)
    current_source = source
    write_edge_control()

    local hwdec = o.hwdec and "yes" or mp.get_property("hwdec") or "no"
    local vf = mp.get_property_native("vf") or {}
    local sw, sh = scaled_input_size(source.width, source.height)
    if not platform.macos then
        -- Whichever hardware decoder mpv finds (VAAPI, NVDEC, D3D11...), copied back to memory for VapourSynth,
        -- and >4K scaled down there: there's no one GPU scaler for all of them
        if hwdec ~= "no" then mp.set_property("file-local-options/hwdec", "auto-copy") end
        if sw then
            table.insert(vf, { name = "lavfi", label = SCALE_LABEL, enabled = true,
                               params = { graph = string.format("scale=w=%d:h=%d:flags=bilinear", sw, sh) } })
        end
    elseif sw and hwdec ~= "no" then
        -- Frames stay on the GPU through scale_vt; mpv copies the 4K result to memory for VapourSynth.
        -- Without hardware frames (software decoding) scale_vt fails and mpv drops just that filter
        mp.set_property("file-local-options/hwdec", "videotoolbox")
        table.insert(vf, { name = "lavfi", label = SCALE_LABEL, enabled = true,
                           params = { graph = string.format("scale_vt=w=%d:h=%d", sw, sh) } })
    elseif hwdec ~= "no" then
        -- VapourSynth needs frames in system memory; keep hardware decoding, copied back
        mp.set_property("file-local-options/hwdec", "videotoolbox-copy")
    end
    table.insert(vf, gyroflow_filter(source, true))
    mp.set_property_native("file-local-options/vf", vf)
    mp.msg.info("stabilizing with " .. source.project .. KIND_LABELS[source.kind] .. (source.preset and (" + " .. source.preset) or "") .. ", " .. interpolation(source)
                .. (sw and string.format(" (input scaled to %dx%d)", sw, sh) or "") .. (view == "wide" and ", wider view" or "")
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

local function no_source()
    mp.osd_message("Gyroflow: no project or motion data for this file", 1.5)
end

-- The stabilization on (normal), on with the wider view (wide), or off for this clip; normal and wide stick
local function show(v)
    if not current_source then return no_source() end
    local enabled = v ~= "off"
    if enabled then view = v end
    stab_off = not enabled
    local vf = mp.get_property_native("vf") or {}
    for i, f in ipairs(vf) do
        -- The input scaling goes with it: off shows the video as it is
        if f.label == SCALE_LABEL then f.enabled = enabled end
        if f.label == LABEL then vf[i] = gyroflow_filter(current_source, enabled) end
    end
    mp.set_property_native("vf", vf)
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

mp.register_script_message("edges", function(mode)
    if EDGE_LABELS[mode] then set_edges(mode) end
end)

mp.register_script_message("remember", function()
    local saved = platform.save_options("gyroflow", {
        { "view", view },
        { "horizon_lock", horizon == nil and "project" or (horizon and "yes" or "no") },
        { "underwater", underwater },
        { "edges", edge_mode }, { "edges_blur", string.format("%g", edge_blur) },
    })
    mp.osd_message(saved and "Gyroflow: saved as the defaults (gyroflow.conf)" or "Gyroflow: couldn't write gyroflow.conf", 2)
end)

-- Edit in Gyroflow. The project goes next to the video, where find_project() picks it up from then on: the one it
-- plays with, or for a video without one (an embedded project, or its own motion data with the default preset) the
-- same written out by gyroflow-embed --extract, so Gyroflow opens it as it plays here. Gyroflow overwrites the
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
    local w, h, fps = project_video_size(project)
    if not w then
        mp.msg.warn("not reloading " .. project .. ": not a Gyroflow project")
        return
    end
    -- The same video: its camera records the water or not as it did
    local source = { project = project, kind = "project", width = w, height = h, fps = fps, water = current_source and current_source.water }
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
    if not file_exists(EMBED) then
        mp.osd_message("Gyroflow: gyroflow-embed isn't installed", 2)
        return
    end
    project = (path:match("^(.*)%.[^./\\]+$") or path) .. ".gyroflow"
    local args = { EMBED, "--extract", "--underwater", underwater }
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
            mp.msg.warn("gyroflow-embed: " .. err)
            mp.osd_message("Gyroflow: couldn't write the project: " .. (err:match("([^\n]+)$") or "failed"), 4)
        end
    end)
end)

mp.register_event("end-file", stop_watching)

require 'export'
