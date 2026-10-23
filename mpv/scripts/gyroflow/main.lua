-- gyroflow: videos stabilized with Gyroflow as mpv plays them. An mpv plugin: this folder goes into ~~/scripts/,
-- with the binaries built from vs-gyroflow/ in this repo (install.sh puts them here).
--
--   DJI_..._D.MP4  +  DJI_..._D.gyroflow   ->  plays through Gyroflow (vf=vapoursynth)
--   DJI_..._D.MP4 with a project embedded in it (gyroflow-embed)   ->  same
--   DJI_..._D.MP4 with gyro data, recorded with EIS off   ->  stabilized with the default preset:
--       Gyroflow's own default preset, then default.gyroflow if there is one: in $GYROFLOW_DATA (where an app
--       that ships this plugin read-only, Steady Player, keeps yours), else in this folder (install.sh puts
--       the underwater preset there)
--
-- The last two are decided by gyroflow-probe, which reads the file's metadata (20-100 ms for a DJI clip) before
-- playback starts.
--
-- The stabilization is the gyroflow VapourSynth plugin (libvs_gyroflow.dylib), so it's the same as rendering the
-- project in Gyroflow, including the project's smoothing, zoom and output size. It runs before any shaders (the
-- dlogm plugin's D-Log M ones), at the source bit depth.
--
-- Video bigger than 4K (8K) is scaled down to 4K on the GPU (VideoToolbox) before it's copied to memory
-- for VapourSynth: copying, converting and uploading 8K frames left too little headroom in a window
-- with the HDR shader (dropped frames, a slow start). The window shows less than 4K anyway.
--
-- Keys (rebind with `<key> script-binding gyroflow/<name>` in input.conf):
--   Ctrl+g   toggle   stabilization on/off for the current file
--   Shift+E  export   the clip as it plays (export.lua)
--   Ctrl+e   edit     open the clip's project in Gyroflow, writing one next to it first when it plays without one;
--                     saving it in Gyroflow reloads the stabilization here
--
-- Options (~~/script-opts/gyroflow.conf):
--   hwdec=yes    decode the videos it stabilizes on the GPU (VideoToolbox), whatever mpv's hwdec is: software
--                decoding doesn't keep up with 4K. no: leave it to mpv's hwdec
--   workers=3    GPU worker threads

local mp = require 'mp'
local options = require 'mp.options'
local utils = require 'mp.utils'

local o = { hwdec = true, workers = 3 }
options.read_options(o, "gyroflow")

local LABEL = "gyroflow"
local SCALE_LABEL = "gyroflow-scale"
local DIR = mp.get_script_directory()
local PLUGIN = utils.join_path(DIR, "libvs_gyroflow.dylib")
local SCRIPT = utils.join_path(DIR, "gyroflow.vpy")
local PROBE = utils.join_path(DIR, "gyroflow-probe")
local EMBED = utils.join_path(DIR, "gyroflow-embed")
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
    local base = path:match("^(.*)%.[^./]+$") or path
    for _, candidate in ipairs({ base .. ".gyroflow", path .. ".gyroflow" }) do
        if file_exists(candidate) then return candidate end
    end
    return nil
end

-- The video size the project was made for (mpv doesn't know the size yet in on_load)
local function project_video_size(project)
    local f = io.open(project, "r")
    if not f then return nil end
    local json = utils.parse_json(f:read("*a"))
    f:close()
    local info = json and json.video_info
    if not info or not info.width or not info.height then return nil end
    return info.width, info.height, info.fps
end

-- What to stabilize the video with: { project = <.gyroflow or the video itself>, kind, width, height, preset }
local function find_source(path)
    local project = find_project(path)
    if project then
        local w, h, fps = project_video_size(project)
        return { project = project, kind = "project", width = w, height = h, fps = fps }
    end
    if not path or path:find("://", 1, true) then return nil end
    local ext = (path:match("%.([^./]+)$") or ""):lower()
    if not PROBE_EXTENSIONS[ext] or not file_exists(PROBE) then return nil end
    local r = mp.command_native({ name = "subprocess", args = { PROBE, path }, capture_stdout = true, playback_only = false })
    local info = r and r.status == 0 and utils.parse_json(r.stdout or "") or nil
    if not info or not info.mode then
        mp.msg.verbose("not stabilizing: " .. (info and info.reason or "probe failed"))
        return nil
    end
    local preset = nil
    if info.mode == "default" then
        for _, p in ipairs(PRESETS) do
            if not preset and file_exists(p) then preset = p end
        end
    end
    return { project = path, kind = info.mode, width = info.width, height = info.height, fps = info.fps, preset = preset }
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

local function gyroflow_filter(source, enabled)
    return {
        name = "vapoursynth",
        label = LABEL,
        enabled = enabled,
        params = {
            file = SCRIPT,
            ["user-data"] = utils.format_json({ plugin = PLUGIN, project = source.project, preset = source.preset, interpolation = interpolation(source), workers = o.workers }),
            -- Frames mpv requests in parallel: one per GPU worker plus one being decoded. More only costs
            -- memory (an 8K 10-bit frame is ~100 MB); mpv's default (core count) makes 8K crawl
            ["concurrent-frames"] = "4",
        },
    }
end

local current_source = nil

mp.add_hook("on_load", 50, function()
    current_source = nil
    if not file_exists(PLUGIN) then
        if find_project(mp.get_property("path")) then mp.msg.warn("plugin missing at " .. PLUGIN) end
        return
    end
    local source = find_source(mp.get_property("path"))
    if not source then return end
    current_source = source

    local hwdec = o.hwdec and "yes" or mp.get_property("hwdec") or "no"
    local vf = mp.get_property_native("vf") or {}
    local sw, sh = scaled_input_size(source.width, source.height)
    if sw and hwdec ~= "no" then
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
                .. (sw and string.format(" (input scaled to %dx%d)", sw, sh) or ""))
end)

mp.register_event("file-loaded", function()
    if current_source then mp.osd_message("Gyroflow: stabilized" .. KIND_LABELS[current_source.kind], 1.5) end
end)

mp.add_key_binding("Ctrl+g", "toggle", function()
    if not current_source then
        mp.osd_message("Gyroflow: no project or motion data for this file", 1.5)
        return
    end
    local vf = mp.get_property_native("vf") or {}
    local enabled = nil
    for _, f in ipairs(vf) do
        if f.label == LABEL then enabled = not f.enabled end
    end
    if enabled == nil then return end
    -- The input scaling goes with it: off shows the video as it is
    for _, f in ipairs(vf) do
        if f.label == LABEL or f.label == SCALE_LABEL then f.enabled = enabled end
    end
    mp.set_property_native("vf", vf)
    mp.osd_message(enabled and "Gyroflow: on" or "Gyroflow: off", 1.5)
end)

-- Edit in Gyroflow. The project goes next to the video, where find_project() picks it up from then on: the one it
-- plays with, or for a video without one (an embedded project, or its own motion data with the default preset) the
-- same written out by gyroflow-embed --extract, so Gyroflow opens it as it plays here. Gyroflow overwrites the
-- project it opened when it saves (Cmd+S), and the stabilization reloads with it while the video stays where it is.
local WATCH_INTERVAL = 0.5
local watch = nil

local function file_version(path)
    local info = utils.file_info(path)
    return info and info.is_file and string.format("%s/%d", tostring(info.mtime), info.size) or nil
end

local function stop_watching()
    if watch then watch.timer:kill() end
    watch = nil
end

local function reload_project(project, version)
    local w, h, fps = project_video_size(project)
    if not w then
        mp.msg.warn("not reloading " .. project .. ": not a Gyroflow project")
        return
    end
    local source = { project = project, kind = "project", width = w, height = h, fps = fps }
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

-- Gyroflow writes the file in place: a version is taken once it stayed the same for an interval
local function check_project()
    local v = file_version(watch.project)
    if not v or v == watch.version then
        watch.pending = nil
    elseif v ~= watch.pending then
        watch.pending = v
    else
        watch.version, watch.pending = v, nil
        reload_project(watch.project, v)
    end
end

local function open_in_gyroflow(project)
    mp.set_property_bool("pause", true)
    if not (watch and watch.project == project) then
        stop_watching()
        watch = { project = project, version = file_version(project) }
        watch.timer = mp.add_periodic_timer(WATCH_INTERVAL, check_project)
    end
    mp.command_native_async({ name = "subprocess", args = { "/usr/bin/open", "-b", GYROFLOW_APP, project }, playback_only = false, capture_stderr = true }, function(_, r)
        if r and r.status == 0 then
            mp.osd_message("Gyroflow: editing " .. project:match("([^/]+)$") .. "\nsave it there to see it here", 4)
        else
            mp.msg.warn("open: " .. (r and r.stderr or ""))
            mp.osd_message("Gyroflow: couldn't open the app (is Gyroflow installed?)", 4)
        end
    end)
end

mp.add_key_binding("Ctrl+e", "edit", function()
    local path = mp.get_property("path")
    if not current_source then
        mp.osd_message("Gyroflow: no project or motion data for this file", 1.5)
        return
    end
    -- One saved meanwhile is the user's: open it as it is
    local project = find_project(path)
    if project then return open_in_gyroflow(project) end
    if not file_exists(EMBED) then
        mp.osd_message("Gyroflow: gyroflow-embed isn't installed", 2)
        return
    end
    project = (path:match("^(.*)%.[^./]+$") or path) .. ".gyroflow"
    local args = { EMBED, "--extract" }
    if current_source.preset then
        table.insert(args, "--preset"); table.insert(args, current_source.preset)
    end
    table.insert(args, path); table.insert(args, project)
    mp.osd_message("Gyroflow: writing the project...", 2)
    mp.command_native_async({ name = "subprocess", args = args, playback_only = false, capture_stderr = true }, function(_, r)
        if r and r.status == 0 then
            mp.msg.info("wrote " .. project)
            open_in_gyroflow(project)
        else
            local err = (r and r.stderr or ""):gsub("%s+$", "")
            mp.msg.warn("gyroflow-embed: " .. err)
            mp.osd_message("Gyroflow: couldn't write the project: " .. (err:match("([^\n]+)$") or "failed"), 4)
        end
    end)
end)

mp.register_event("end-file", stop_watching)

require 'export'
