-- Play videos stabilized when there's a Gyroflow project for them:
--
--   DJI_..._D.MP4  +  DJI_..._D.gyroflow   ->  plays through Gyroflow (vf=vapoursynth)
--   DJI_..._D.MP4 with a project embedded in it (Gyroflow: "Save project into the video file")   ->  same
--   DJI_..._D.MP4 with gyro data, recorded with EIS off   ->  stabilized with the default preset:
--       Gyroflow's own default preset, then ~~/gyroflow/default.gyroflow if it exists
--
-- The last two are decided by ~~/gyroflow/gyroflow-probe (built with the plugin), which reads the file's metadata
-- (20-100 ms for a DJI clip) before playback starts.
--
-- The stabilization is the gyroflow VapourSynth plugin (~~/gyroflow/libvs_gyroflow.dylib, built from
-- vs-gyroflow/ in this repo), so it's the same as rendering the project in Gyroflow,
-- including the project's smoothing, zoom and output size. It runs before the D-Log M shaders, at the
-- source bit depth, so dlog_auto.lua's SDR/HDR handling is unchanged.
--
-- Video bigger than 4K (8K) is scaled down to 4K on the GPU (VideoToolbox) before it's copied to memory
-- for VapourSynth: copying, converting and uploading 8K frames left too little headroom in a window
-- with the HDR shader (dropped frames, a slow start). The window shows less than 4K anyway.
--
-- mpv standalone only: IINA's libmpv is built without VapourSynth.
--
-- Ctrl+g toggles the stabilization for the current file.

local mp = require 'mp'
local utils = require 'mp.utils'

local LABEL = "gyroflow"
local SCALE_LABEL = "gyroflow-scale"
local PLUGIN = mp.command_native({"expand-path", "~~/gyroflow/libvs_gyroflow.dylib"})
local SCRIPT = mp.command_native({"expand-path", "~~/gyroflow/gyroflow.vpy"})
local PROBE = mp.command_native({"expand-path", "~~/gyroflow/gyroflow-probe"})
-- Applied on top of Gyroflow's default preset to videos stabilized without a project
local PRESET = mp.command_native({"expand-path", "~~/gyroflow/default.gyroflow"})
-- Containers a project can be embedded in, and that DJI cameras record
local PROBE_EXTENSIONS = { mp4 = true, mov = true }
-- bilinear | bicubic | lanczos. Bicubic is the realtime/quality sweet spot on the M1 Pro.
local INTERPOLATION = "bicubic"
-- GPU worker threads. 3 on the M1 Pro: 4K60 ~87 fps of capacity
local WORKERS = 3
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
    return info.width, info.height
end

-- What to stabilize the video with: { project = <.gyroflow or the video itself>, kind, width, height, preset }
local function find_source(path)
    local project = find_project(path)
    if project then
        local w, h = project_video_size(project)
        return { project = project, kind = "project", width = w, height = h }
    end
    if not path or path:find("://", 1, true) then return nil end
    local ext = (path:match("%.([^./]+)$") or ""):lower()
    if not PROBE_EXTENSIONS[ext] or not file_exists(PROBE) then return nil end
    local r = mp.command_native({ name = "subprocess", args = { PROBE, path }, capture_stdout = true, playback_only = false })
    local info = r and r.status == 0 and utils.parse_json(r.stdout or "") or nil
    if not info or not info.mode then
        mp.msg.verbose("gyroflow: not stabilizing: " .. (info and info.reason or "probe failed"))
        return nil
    end
    local preset = nil
    if info.mode == "default" and file_exists(PRESET) then preset = PRESET end
    return { project = path, kind = info.mode, width = info.width, height = info.height, preset = preset }
end

local KIND_LABELS = { project = "", embedded = " (embedded project)", default = " (default preset)" }

-- Even width and height that fit MAX_INPUT_PIXELS, or nil when the video already does
local function scaled_input_size(w, h)
    if not w or w * h <= MAX_INPUT_PIXELS then return nil end
    local s = math.sqrt(MAX_INPUT_PIXELS / (w * h))
    return math.floor(w * s / 2) * 2, math.floor(h * s / 2) * 2
end

local function in_iina()
    return (mp.get_property("options/vo") or ""):find("libmpv", 1, true) ~= nil
end

local function gyroflow_filter(source, enabled)
    return {
        name = "vapoursynth",
        label = LABEL,
        enabled = enabled,
        params = {
            file = SCRIPT,
            ["user-data"] = utils.format_json({ plugin = PLUGIN, project = source.project, preset = source.preset, interpolation = INTERPOLATION, workers = WORKERS }),
            -- Frames mpv requests in parallel: one per GPU worker plus one being decoded. More only costs
            -- memory (an 8K 10-bit frame is ~100 MB); mpv's default (core count) makes 8K crawl
            ["concurrent-frames"] = "4",
        },
    }
end

local current_source = nil

mp.add_hook("on_load", 50, function()
    current_source = nil
    if in_iina() then
        local project = find_project(mp.get_property("path"))
        if project then mp.msg.info("gyroflow: " .. project .. " found, but IINA can't run VapourSynth; play in mpv to stabilize") end
        return
    end
    if not file_exists(PLUGIN) then
        if find_project(mp.get_property("path")) then mp.msg.warn("gyroflow: plugin missing at " .. PLUGIN) end
        return
    end
    local source = find_source(mp.get_property("path"))
    if not source then return end
    current_source = source

    local hwdec = mp.get_property("hwdec") or "no"
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
    mp.msg.info("gyroflow: stabilizing with " .. source.project .. KIND_LABELS[source.kind] .. (source.preset and (" + " .. source.preset) or "")
                .. (sw and string.format(" (input scaled to %dx%d)", sw, sh) or ""))
end)

mp.register_event("file-loaded", function()
    if current_source then mp.osd_message("Gyroflow: stabilized" .. KIND_LABELS[current_source.kind], 1.5) end
end)

mp.add_key_binding("Ctrl+g", "gyroflow-toggle", function()
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
