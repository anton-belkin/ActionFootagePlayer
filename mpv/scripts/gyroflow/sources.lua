-- sources: what a clip plays stabilized with, for the gyroflow plugin (main.lua wires it): its Gyroflow project, or
-- gyroflow-probe's word on its own motion data, lens, 360 rig; the plugin's tools and presets; a clip's water and turn
local mp = require 'mp'
local utils = require 'mp.utils'
local platform = require 'platform'

local M = {}
-- Set by install (main.lua's W): the state the parts share, and what this part takes from the others
local W, S
-- The other parts' functions, looked up in W as they're called
local function filtering(...) return W.filtering(...) end
local function gpu_refresh(...) return W.gpu_refresh(...) end
local function publish(...) return W.publish(...) end
local function shown_aspect(...) return W.shown_aspect(...) end

-- The clip playing: mpv's path, or the original when the file playing is a moment (the workspace plugin's
-- user-data/workspace/clip, docs/design/moments-and-workspaces.md)
local function clip_path()
    local path = mp.get_property("path")
    local ws = mp.get_property_native("user-data/workspace")
    if path and type(ws) == "table" and ws.path == path and ws.redirected and ws.clip then return ws.clip end
    return path
end

local DIR = mp.get_script_directory()
local PROBE = utils.join_path(DIR, platform.exe("gyroflow-probe"))
local PROJECT_WRITER = utils.join_path(DIR, platform.exe("gyroflow-project"))
local GPU_TABLE = utils.join_path(DIR, platform.exe("gyroflow-gpu-table"))
local GPU_HOLD_SHADER = utils.join_path(DIR, "gpu_hold.glsl")
local GYROFLOW_APP = "xyz.gyroflow"
-- Applied on top of Gyroflow's default preset to videos stabilized without a project: the first that exists
local DATA = os.getenv("GYROFLOW_DATA")
local PRESETS = {}
if DATA and DATA ~= "" then table.insert(PRESETS, utils.join_path(DATA, "default.gyroflow")) end
table.insert(PRESETS, utils.join_path(DIR, "default.gyroflow"))
-- Containers gyroflow-probe looks into: what DJI cameras record, and Insta360's own (both lenses in one .insv)
local PROBE_EXTENSIONS = { mp4 = true, mov = true, insv = true }
-- bilinear | bicubic | lanczos, the warp's sampling (gyroflow-gpu-table's interpolation): lanczos up to 30 fps and
-- bicubic above (and when the rate's unknown)
local INTERPOLATION_SLOW = "lanczos"
local INTERPOLATION_FAST = "bicubic"
local SLOW_MAX_FPS = 31

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
-- own hwdec (zero-copy), whatever motion data it has (docs/design/360-cameras.md, "Stitched clips")
-- A 360 camera's own file: its rig as gyroflow-probe read it (brand, model, lenses, tracks, width, height of a track),
-- false for any other clip

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
            S.spherical = info.spherical
            mp.msg.verbose("not stabilizing: a stitched 360° clip (" .. tostring(S.spherical.projection) .. ")")
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
        -- rotation: how the file says it's shown (its display matrix, clockwise), which the plugin turns it by.
        -- optical: its motion measured from the picture (<stem>.afp-motion beside it, gyroflow-probe's "measured"), with
        -- no horizon in it to lock. in_camera: how the camera stabilized it itself (GoPro HERO12+ HyperSmooth), stabilized
        -- further on top of it
        done({ project = path, kind = info.mode, width = info.width, height = info.height, fps = info.fps, preset = preset, water = info.water,
               rotation = tonumber(info.rotation) or 0, optical = info.motion == "optical" and (type(info.measured) == "table" and info.measured or {}) or nil,
               in_camera = type(info.stabilized_in_camera) == "string" and info.stabilized_in_camera or nil })
    end)
end

local KIND_LABELS = { project = "", default = " (default preset)", lens = " (the lens alone)" }

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

-- Whether the camera of a clip with a project next to it records when it's in the water, and how its file says it's
-- turned (gyroflow-probe; a clip without one was probed before it loaded), for the menu: it plays meanwhile
local function find_water(path, source)
    if not file_exists(PROBE) then return end
    mp.command_native_async({ name = "subprocess", args = { PROBE, path }, capture_stdout = true, playback_only = true }, function(_, r)
        local info = r and r.status == 0 and utils.parse_json(r.stdout or "") or nil
        if not info or S.current_source ~= source then return end
        -- The file's own turn, for the render size of a frame changed over the project's
        local turn = tonumber(info.rotation) or 0
        if turn ~= (source.rotation or 0) then
            source.rotation = turn
            if (S.frame.rotation ~= 0 or shown_aspect() ~= "original") and filtering() then gpu_refresh() end
        end
        if info.water then
            source.water = true
            publish()
        end
    end)
end

function M.install(w)
    W = w
    S = W.S
    S.spherical = false
    S.rig360 = false
    W.DATA, W.GPU_HOLD_SHADER, W.GPU_TABLE, W.GYROFLOW_APP, W.INTERPOLATION_FAST, W.INTERPOLATION_SLOW = DATA, GPU_HOLD_SHADER, GPU_TABLE, GYROFLOW_APP, INTERPOLATION_FAST, INTERPOLATION_SLOW
    W.KIND_LABELS, W.PRESETS, W.PROBE, W.PROJECT_WRITER, W.SLOW_MAX_FPS, W.clip_path = KIND_LABELS, PRESETS, PROBE, PROJECT_WRITER, SLOW_MAX_FPS, clip_path
    W.file_exists, W.find_project, W.find_source, W.find_water, W.preset_horizon, W.project_paths = file_exists, find_project, find_source, find_water, preset_horizon, project_paths
    W.project_video_size, W.read_json, W.workspace_of = project_video_size, read_json, workspace_of
end

return M
