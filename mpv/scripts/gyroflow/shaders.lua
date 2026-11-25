-- shaders: the warp's shader files in mpv's list, for the gyroflow plugin (main.lua wires it): the same settings
-- the same file, what an mpv that keeps every shader holds (its window made again), the table server, the hold
local mp = require 'mp'
local utils = require 'mp.utils'
local platform = require 'platform'

local M = {}
-- Set by install (main.lua's W): the state the parts share, and what this part takes from the others
local W, S, GPU_HOLD_SHADER, GPU_TABLE, o
-- The other parts' functions, looked up in W as they're called
local function file_exists(...) return W.file_exists(...) end
local function mpv_has_pts(...) return W.mpv_has_pts(...) end

-- The shaders mpv keeps (docs/design/gpu-render.md, "Each change is a new shader file"). Each version of the warp is a file
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
function M.start_file()
    if flush_due then gpu_flush(nil) end
end

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
        if e.path ~= (S.gpu and S.gpu.shader) and (not oldest or e.used < gpu_reuse[oldest].used) then oldest = k end
    end
    if n > REUSE_MAX and oldest then
        local path = gpu_reuse[oldest].path
        gpu_reuse[oldest], gpu_kept[path] = nil, nil
        os.remove(path)
    end
end

function M.shutdown_kept()
    for path in pairs(gpu_kept) do os.remove(path) end
end

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
    -- Journal: the warp's in mpv's list (stabilization on); the picture changes with the next frame mpv renders
    if package.loaded.journal and new then package.loaded.journal.step("warp-set") package.loaded.journal.shown("picture") end
end

-- The player's table server (gyroflow-gpu-table --serve): it keeps the clips it loaded, so a change of the view, the
-- edges, the color fringes or the denoise writes the shader alone (~40 ms, not ~0.4 s). One per player, going with
-- it; a request it can't take, the table tool does by itself. Not on Windows (a Unix socket)
local GPU_SERVER = not platform.windows and utils.join_path(platform.temp_dir(), string.format("mpv-gyroflow-%d.sock", utils.getpid())) or nil
local function gpu_server()
    if not GPU_SERVER or S.gpu_server_running or not file_exists(GPU_TABLE) or not mpv_has_pts() then return end
    S.gpu_server_running = true
    mp.command_native_async({ name = "subprocess", args = { GPU_TABLE, "--serve", GPU_SERVER }, playback_only = false }, function()
        S.gpu_server_running = false
    end)
end
function M.shutdown_server() if GPU_SERVER then os.remove(GPU_SERVER) end end

-- A clip opening on the GPU path doesn't wait for its table: mpv opens it meanwhile (its decoder, window and audio,
-- ~0.7 s), paused on black (gpu_hold.glsl) until the warp is there. held: the pause is the plugin's
local function gpu_unhold()
    if S.gpu_held then
        S.gpu_held = false
        -- Journal: the black hold over, the clip plays stabilized
        if package.loaded.journal then package.loaded.journal.step("unhold") end
        mp.set_property_native("pause", false)
    end
end

function M.install(w)
    W = w
    S = W.S
    GPU_HOLD_SHADER, GPU_TABLE, o = W.GPU_HOLD_SHADER, W.GPU_TABLE, W.o
    S.gpu_server_running = false
    S.gpu_held = false
    W.DENOISE_MB, W.GPU_SERVER, W.GRID_MB, W.SPHERE_MB, W.gpu_remember, W.gpu_reuse = DENOISE_MB, GPU_SERVER, GRID_MB, SPHERE_MB, gpu_remember, gpu_reuse
    W.gpu_server, W.gpu_set_shader, W.gpu_unhold, W.settings_key = gpu_server, gpu_set_shader, gpu_unhold, settings_key
end

return M
