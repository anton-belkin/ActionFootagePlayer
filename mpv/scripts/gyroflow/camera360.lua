-- camera360: a 360 camera's own file (an Insta360 .insv), its lens tracks joined and the sphere made on the GPU, for the
-- gyroflow plugin. main.lua wires it
local mp = require 'mp'
local utils = require 'mp.utils'
local platform = require 'platform'

local M = {}
-- Set by install (main.lua's W): the state the parts share, and what this part takes from the others
local W, S, GPU_HOLD_SHADER, GPU_TABLE, PROBE, SPHERE_MB, dn_sync, gpu_blocked, o
-- The other parts' functions, looked up in W as they're called
local function gpu_missing(...) return W.gpu_missing(...) end
local function gpu_set_shader(...) return W.gpu_set_shader(...) end
local function gpu_unhold(...) return W.gpu_unhold(...) end
local function play_unstabilized(...) return W.play_unstabilized(...) end
local function time_offset(...) return W.time_offset(...) end
local function unable_text(...) return W.unable_text(...) end

-- A 360 camera's own file (see the top): its lens tracks joined, the plugin's 360 mode after them. None of the flat
-- clip's settings apply (the view, level, edges, crop mode, the still frame): current_source stays nil
local function add_sphere(source)
    local rig = source.rig
    S.rig360 = rig
    local tracks = math.max(1, math.floor(tonumber(rig.tracks) or 1))
    local missing = gpu_missing()
    if missing or gpu_blocked[mp.get_property("path") or ""] then
        return play_unstabilized(missing or unable_text(gpu_blocked[mp.get_property("path") or ""]))
    end
    -- The app's mpv joins two lens tracks itself (tools/mpv-patches/0014, --video-join): each decoded on the GPU
    -- (VideoToolbox's zero-copy) and drawn side by side there, for the GPU path's sphere shader. Elsewhere lavfi-complex's
    -- hstack joins them in memory
    local join = tracks == 2 and mp.get_property_native("option-info/video-join") ~= nil
    if join then
        mp.set_property("file-local-options/video-join", "yes")
        -- The joined frame keeps the aspect view360 sets (the window's area, video-aspect-override): a window resized to
        -- it would change the area, and the aspect again
        mp.set_property("file-local-options/auto-window-resize", "no")
        if o.hwdec then mp.set_property("file-local-options/hwdec", "auto") end
    else
        if tracks > 1 then
            local inputs = ""
            for i = 1, tracks do inputs = inputs .. "[vid" .. i .. "]" end
            mp.set_property("file-local-options/lavfi-complex", inputs .. "hstack=inputs=" .. tracks .. "[vo]")
        end
        -- VideoToolbox's zero-copy has no device under lavfi-complex and turns into software decoding (docs/archive/360-throughput-spike.md)
        if o.hwdec then mp.set_property("file-local-options/hwdec", platform.macos and "videotoolbox-copy" or "auto-copy") end
    end
    -- Each track decoded on a thread of its own: in the playback thread mpv decodes them one after the other, and two
    -- X5 tracks came at ~31 fps together where one alone comes at ~70 (with it, ~65 both; through the plugin 24 -> 30 fps
    -- at 7680, 27 -> 53 at 5760, without the screen)
    mp.set_property("file-local-options/vd-queue-enable", "yes")
    -- What it has for the view's heading modes, for view360 and its panel (the plugin finds the same for its stamp,
    -- reading the whole clip's motion and GPS: about a second for a few minutes of a clip)
    mp.command_native_async({ name = "subprocess", args = { PROBE, "--headings", source.project }, capture_stdout = true, playback_only = true }, function(_, r)
        if S.rig360 ~= rig then return end
        local h = r and r.status == 0 and utils.parse_json(r.stdout or "") or nil
        mp.set_property_native("user-data/gyroflow/headings360", type(h) == "table" and h or { error = "gyroflow-probe --headings failed" })
    end)
    -- The GPU path (docs/design/gpu-render.md, "360 on the GPU"): the sphere made by a shader from the joined tracks
    -- (gyroflow-gpu-table --sphere360: each lens through gyroflow-core's kernel, blended by its share, the stamp for the
    -- view shader), every frame. The clip opens meanwhile, paused on black
    do
        gpu_set_shader(S.gpu and S.gpu.shader, GPU_HOLD_SHADER)
        S.gpu = { shader = GPU_HOLD_SHADER, hold = true, sphere = true }
        if not mp.get_property_native("pause") then
            S.gpu_held = true
            mp.set_property_native("pause", true)
        end
        S.gpu_count = S.gpu_count + 1
        local out = utils.join_path(platform.temp_dir(), string.format("mpv-gyroflow-%d-%d.glsl", utils.getpid(), S.gpu_count))
        local args = { GPU_TABLE, "--sphere360", source.project, out, tostring(o.sphere_width > 0 and o.sphere_width or 0),
                       tostring(o.stitching_distance > 0 and o.stitching_distance or 0), string.format("%.6f", time_offset() or 0) }
        local t = mp.get_time()
        mp.command_native_async({ name = "subprocess", args = args, capture_stdout = true, playback_only = false }, function(_, r)
            local reply = r and r.status == 0 and utils.parse_json((r.stdout or ""):match("[^\n]*")) or nil
            if S.rig360 ~= rig then return end
            if reply and reply.shader then
                gpu_set_shader(GPU_HOLD_SHADER, reply.shader, SPHERE_MB)
                S.gpu = { shader = reply.shader, sphere = true }
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
            gpu_set_shader(GPU_HOLD_SHADER, nil)
            S.gpu = nil
            dn_sync()
            play_unstabilized(unable_text(why))
            gpu_unhold()
        end)
    end
end

function M.install(w)
    W = w
    S = W.S
    GPU_HOLD_SHADER, GPU_TABLE, PROBE, SPHERE_MB, dn_sync, gpu_blocked, o = W.GPU_HOLD_SHADER, W.GPU_TABLE, W.PROBE, W.SPHERE_MB, W.dn_sync, W.gpu_blocked, W.o
    W.add_sphere = add_sphere
end

return M
