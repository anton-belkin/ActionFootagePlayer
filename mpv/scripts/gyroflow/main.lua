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
--   Ctrl+b   edges    what fills the edges the stabilized frame doesn't cover: the project's background,
--                     the original frame, the frame zoomed, or a trail of earlier frames (all blurred)
--   Shift+E  export   the clip as it plays (export.lua)
--
-- Options (~~/script-opts/gyroflow.conf):
--   hwdec=yes    decode the videos it stabilizes on the GPU (VideoToolbox), whatever mpv's hwdec is: software
--                decoding doesn't keep up with 4K. no: leave it to mpv's hwdec
--   workers=3    GPU worker threads
--   max_fps=30   clips faster than this play every other frame (a 60 fps clip at 30), so the stabilization does
--                half the work; 0: every frame
--   edges=trail  what fills the edges: trail (what was last shown there), original, zoom, off (the project's
--                background color)
--   edges_blur=2 blur of the edge fill, % of the frame height
--   edges_zoom=1.5  how much the zoom fill scales the frame up

local mp = require 'mp'
local options = require 'mp.options'
local utils = require 'mp.utils'

local o = { hwdec = true, workers = 3, max_fps = 30, edges = "trail", edges_blur = 2, edges_zoom = 1.5 }
options.read_options(o, "gyroflow")

local LABEL = "gyroflow"
local SCALE_LABEL = "gyroflow-scale"
local DIR = mp.get_script_directory()
local PLUGIN = utils.join_path(DIR, "libvs_gyroflow.dylib")
local SCRIPT = utils.join_path(DIR, "gyroflow.vpy")
local PROBE = utils.join_path(DIR, "gyroflow-probe")
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

-- The plugin fills the uncovered edges itself (vs-gyroflow/src/edges.rs). The mode goes through a control file
-- it reads while playing: changing the filter's parameters would rebuild mpv's filter chain, which drops frames
local EDGE_MODES = { "off", "original", "zoom", "trail" }
local EDGE_LABELS = { off = "background color", original = "original frame, blurred", zoom = "zoomed frame, blurred",
                      trail = "trail of earlier frames, blurred" }
local EDGE_CONTROL = utils.join_path(os.getenv("TMPDIR") or "/tmp", string.format("mpv-gyroflow-%d.edges", utils.getpid()))
local edge_mode = EDGE_LABELS[o.edges] and o.edges or "trail"

local function write_edge_mode()
    local f = io.open(EDGE_CONTROL, "w")
    if not f then return mp.msg.warn("can't write " .. EDGE_CONTROL) end
    f:write(edge_mode)
    f:close()
end

-- Interpolation stays the one for the clip's own rate: at half rate there's headroom for the edge fill instead
local function half_rate(source)
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
            ["user-data"] = utils.format_json({ plugin = PLUGIN, project = source.project, preset = source.preset, interpolation = interpolation(source), workers = o.workers, half_rate = half_rate(source),
                                                edges = edge_mode, edges_blur = o.edges_blur, edges_zoom = o.edges_zoom, edges_control = EDGE_CONTROL }),
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
    write_edge_mode()

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
                .. (sw and string.format(" (input scaled to %dx%d)", sw, sh) or "") .. (half_rate(source) and ", every other frame" or ""))
end)

mp.register_event("file-loaded", function()
    if current_source then mp.osd_message("Gyroflow: stabilized" .. KIND_LABELS[current_source.kind], 1.5) end
end)

-- Sticks for the next clips, like the dlogm plugin's modes
mp.add_key_binding("Ctrl+b", "edges", function()
    local i = 1
    for k, m in ipairs(EDGE_MODES) do
        if m == edge_mode then i = k end
    end
    edge_mode = EDGE_MODES[i % #EDGE_MODES + 1]
    write_edge_mode()
    mp.osd_message("Gyroflow edges: " .. EDGE_LABELS[edge_mode], 2)
end)

mp.register_event("shutdown", function() os.remove(EDGE_CONTROL) end)

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

require 'export'
