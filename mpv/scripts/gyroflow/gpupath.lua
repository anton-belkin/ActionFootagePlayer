-- gpupath: the GPU path, for the gyroflow plugin (main.lua wires it): whether this mpv can, the settings the clip
-- plays with and the table asked for them, the table kept ahead of where it plays, the size it renders at, the
-- shader applied and written again after a change, crop mode's camera in the warp
local mp = require 'mp'
local utils = require 'mp.utils'
local platform = require 'platform'
local still = require 'still'

local M = {}
-- Set by install (main.lua's W): the state the parts share, and what this part takes from the others
local W, S, DENOISE_MB, GPU_HOLD_SHADER, GPU_SERVER, GPU_TABLE, GRID_MB, dn_sync, gpu_reuse, o, trans
-- The other parts' functions, looked up in W as they're called
local function file_exists(...) return W.file_exists(...) end
local function filter_lens(...) return W.filter_lens(...) end
local function gpu_remember(...) return W.gpu_remember(...) end
local function gpu_set_shader(...) return W.gpu_set_shader(...) end
local function has_project(...) return W.has_project(...) end
local function moment_end(...) return W.moment_end(...) end
local function player_args(...) return W.player_args(...) end
local function publish(...) return W.publish(...) end
local function settings_key(...) return W.settings_key(...) end
-- Set further down, or as it installs
local gpu_missing, gpu_settings, gpu_camera, gpu_refresh

-- The GPU path (docs/design/gpu-render.md): Gyroflow's warp as an mpv shader, the frame never leaving the GPU.
-- gyroflow-gpu-table loads the clip from the player's arguments (player_args) and writes a shader with every frame's
-- parameters in it; mpv's PTS finds the frame (mpv 0.42, or the app's mpv, 0.41.0 + 499 commits or later). The warp is
-- the scaler too: it renders at the size of the video's rectangle on screen, turned and cut as the frame says. What it
-- doesn't do (a lens or setting the shader doesn't have: gyroflow-gpu-table says which) plays as recorded, with a card
-- saying why. gpu (declared further up): nil where it isn't stabilized
-- Files the GPU path couldn't do, and why: they play as recorded when loaded again
local gpu_blocked = {}

local function mpv_has_pts()
    local v = mp.get_property("mpv-version") or ""
    local minor = tonumber(v:match("v?0%.(%d+)%.%d+"))
    if not minor then return v:match("v?(%d+)%.") ~= nil and tonumber(v:match("v?(%d+)%.")) > 0 end
    if minor >= 42 then return true end
    if minor < 41 then return false end
    local commits = tonumber(v:match("^mpv v?0%.41%.%d+%-(%d+)%-g"))
    return commits ~= nil and commits >= 499
end

-- Why this player can't stabilize anything (nil: it can): macOS, Windows and Linux (tried on each, Windows on
-- Anton's PC, Linux on the NAS, 2026-11) with mpv 0.42 or later and vo=gpu-next. An mpv without the PTS (a
-- distribution's 0.41) can't
gpu_missing = function()
    if not file_exists(GPU_TABLE) then return "gyroflow-gpu-table isn't installed next to the plugin" end
    if not mpv_has_pts() then return "This mpv is older than 0.42: stabilization needs mpv 0.42 or later" end
    local vo = mp.get_property("vo") or ""
    if vo ~= "" and not vo:find("gpu-next", 1, true) then return "Stabilization needs mpv's vo=gpu-next" end
    return nil
end

-- The file's own turn, the frame's turn, cut and projection are the warp's (vs-gyroflow's Frame, from the same
-- arguments): mpv turns nothing more (video-rotate no) and shows the output's shape (video-aspect-override)
local function gpu_eligible(source)
    if gpu_missing() then return false end
    if not source or (source.kind ~= "project" and source.kind ~= "default" and source.kind ~= "lens") then return false end
    return not gpu_blocked[mp.get_property("path") or ""]
end

-- What gyroflow-gpu-table said it can't do, said for a person (gpu_table.rs's unsupported)
local function unable_text(why)
    why = tostring(why or "")
    local lens = why:match("^lens model (.+)$")
    if lens then
        if lens:lower():find("sony", 1, true) then return "Sony lens profiles aren't supported on the GPU yet" end
        return "This lens model (" .. lens .. ") isn't supported on the GPU yet"
    end
    local digital = why:match("^digital lens (.+)$")
    if digital then return "This digital lens (" .. digital .. ") isn't supported on the GPU yet" end
    if why == "horizontal rolling shutter" then return "A sideways rolling shutter isn't supported on the GPU yet" end
    if why == "IBIS, OIS or lens breathing" then return "In-body or optical stabilization (IBIS, OIS) and lens breathing aren't supported on the GPU yet" end
    if why == "a lens mesh" then return "Lens meshes (Sony's) aren't supported on the GPU yet" end
    if why == "equirect output" then return "An equirectangular output isn't supported on the GPU yet" end
    if why == "" or why == "?" then return "The stabilization couldn't be set up" end
    return "The stabilization couldn't be set up: " .. why
end

-- A clip that has motion data plays as recorded: a card says why, and the panel (user-data/gyroflow/unstabilized)
local function play_unstabilized(why)
    S.unstabilized = why
    mp.msg.warn("not stabilized: " .. why)
    mp.notify({ id = "gyroflow:unstabilized", kind = "info", seconds = 6, title = "Playing without stabilization", detail = why })
end

-- The settings JSON's (settings.lua): what the table's request takes of the settings
local settings_json = require 'settings'

-- The clip's settings as it plays now (docs/reference/contracts.md, "The settings"; registry.json's settings_json):
-- its project or preset, the transform, the moment's part, the lens (player_args), the clean-up (the temporal
-- denoise, how much, its Stronger and the grain it keeps; the water's colour fringes), Ctrl+j's sideways jerks, a 360
-- camera's sphere. Published as user-data/gyroflow/settings: export.lua hands them to gyroflow-export as they are,
-- and the table's request is made of them (gpu_user_data). Without a clip to stabilize: nothing done (lens off)
gpu_settings = function(source)
    local s = source and player_args(source) or {}
    s.lens = source and filter_lens(source) or "off"
    s.chromatic = S.chromatic
    s.denoise, s.temporal_denoise, s.denoise_strong, s.denoise_grain = still.switched(), o.temporal_denoise, S.denoise_strong, o.still_grain
    -- Ctrl+j on, where this computer takes the jerks out (gyroflow.conf translation): the export takes them where they're
    -- computed for the clip
    s.translation = (o.translation and trans.on) and true or false
    if o.sphere_width > 0 then s.sphere_width = o.sphere_width end
    if o.stitching_distance > 0 then s.stitching_distance = o.stitching_distance end
    return s
end

-- What gyroflow-gpu-table --player is asked for the clip as it plays now: the settings' keys it reads, with what only
-- playback adds (the video's size; the fringes as this screen shows them, chromatic_skip; the denoise where this
-- computer does it; the jerks in the shader where computed, on and off by its PARAM)
local function gpu_user_data(source)
    local ud = settings_json.request(gpu_settings(source))
    -- (the table is asked only where the warp does something)
    if ud.lens == "off" then ud.lens = "stabilized" end
    ud.input_width, ud.input_height = source.width, source.height
    ud.gpu_chromatic = S.chromatic and not o.chromatic_skip
    ud.gpu_denoise = still.enabled()
    ud.gpu_translation = o.translation and true or false
    ud.translation_rev = S.trans_rev > 0 and S.trans_rev or nil
    ud.lookahead = o.lookahead
    return ud
end

-- The shader applied (further down): a table extended ahead of where it plays in place of the one before
local gpu_apply

-- The table kept ahead of where a clip plays (gpu_ahead.tick, further down; the policy: ahead.lua)
local gpu_ahead = { policy = require 'ahead', busy = nil, held = false, failed = nil }

-- Asks gyroflow-gpu-table (its server when it runs) for the shader of `req`, to then_(reply)
function gpu_ahead.ask(req, then_)
    S.gpu_count = S.gpu_count + 1
    local out = utils.join_path(platform.temp_dir(), string.format("mpv-gyroflow-%d-%d.glsl", utils.getpid(), S.gpu_count))
    local t = mp.get_time()
    local args = { GPU_TABLE, "--player", utils.format_json(req), out }
    if S.gpu_server_running then
        table.insert(args, "--server")
        table.insert(args, GPU_SERVER)
    end
    mp.command_native_async({ name = "subprocess", args = args, capture_stdout = true, playback_only = false }, function(_, r)
        local reply = r and r.status == 0 and utils.parse_json((r.stdout or ""):match("[^\n]*")) or nil
        if not reply then reply = { error = "gyroflow-gpu-table failed: " .. tostring(r and (r.error_string ~= "" and r.error_string or r.status)) } end
        reply.ms = math.floor((mp.get_time() - t) * 1000)
        reply.denoise = req.gpu_denoise
        reply.translation = reply.translation == true
        then_(reply)
    end)
end

local function gpu_table(source, done)
    local ud = gpu_user_data(source)
    -- A clip without a project: its motion data from the NAS took seconds (20 s for a 1.5 GB clip, cold), so the
    -- table starts from a stretch of it, as the engine does (motion.rs): a moment's part (all it plays), or the
    -- seconds it starts playing at, the reply "partial", extended ahead of where it plays after (gpu_ahead)
    if not has_project(source) then ud.moment_end = moment_end() end
    -- (gpu_user_data: Ctrl+p's clean frame where this computer does it, on the GPU path the temporal denoise while it
    -- plays and paused, warp.glsl's dn_ passes, dn_sync delays the audio; the translation compensation where it's been
    -- computed for the clip, Ctrl+j, a new shader once it is)
    -- Written before with these settings (and the project as it is now): that file again
    local project = source.project and utils.file_info(source.project)
    local key = settings_key(ud) .. (project and string.format("|%s/%s", tostring(project.mtime), tostring(project.size)) or "")
    local hit = gpu_reuse[key]
    if hit and file_exists(hit.path) then
        hit.used = mp.get_time()
        local reply = {}
        for k, v in pairs(hit.reply) do reply[k] = v end
        reply.ms, reply.reused = 0, true
        if package.loaded.journal then package.loaded.journal.step("gpu-table", { reused = true }) end
        mp.add_timeout(0, function() done(reply) end)
        return
    end
    local ask = gpu_ahead.ask
    -- From where it plays now (0 as it opens), not kept by it: the whole clip's table is the same. A stretch: the
    -- request kept with the reply, gpu_ahead asks for more of it as it plays
    if not has_project(source) and not ud.moment_end then
        local req = {}
        for k, v in pairs(ud) do req[k] = v end
        req.start = (mp.get_property_number("time-pos") or 0) + (ud.time_offset or 0)
        return ask(req, function(reply)
            if not reply.partial then
                if reply.shader then gpu_remember(key, reply) end
                return done(reply)
            end
            reply.request = ud
            done(reply)
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
-- (The line last logged inside a block of its own)
local gpu_render_scale
do
local render_logged = nil
function gpu_render_scale()
    if not S.gpu or not S.gpu.shader or S.gpu.sphere then return end
    local d = mp.get_property_native("osd-dimensions") or {}
    local dw, dh = (d.w or 0) - (d.ml or 0) - (d.mr or 0), (d.h or 0) - (d.mt or 0) - (d.mb or 0)
    if dw <= 0 or dh <= 0 then return end
    local s = 1
    local short = tonumber(o.resolution)
    if o.resolution == "auto" and S.gpu.denoise then short = 1080 end
    if o.resolution == "video" then
        local ow, oh = S.gpu.output and S.gpu.output[1], S.gpu.output and S.gpu.output[2]
        if ow and oh then s = math.min(ow / dw, oh / dh) end
        s = math.min(s, 3840 / dw, 2160 / dh)
    elseif short then
        s = math.min(1, short / math.min(dw, dh))
    end
    s = math.max(0.1, math.min(4, s))
    S.gpu_render_short = math.floor(math.min(dw, dh) * s + 0.5)
    local line = string.format("GPU path: rendering at %dx%d (%s), shown at %dx%d", math.floor(dw * s + 0.5), math.floor(dh * s + 0.5), o.resolution, dw, dh)
    if line ~= render_logged then render_logged = line; mp.msg.info(line) end
    if math.abs(s - render_scale) < 1e-4 and (mp.get_property_native("glsl-shader-opts") or {}).gyroflow_render_scale then return end
    render_scale = s
    local opts = mp.get_property_native("glsl-shader-opts") or {}
    opts.gyroflow_render_scale = string.format("%.4f", s)
    mp.set_property_native("glsl-shader-opts", opts)
end
end

gpu_apply = function(reply)
    -- The frame on screen, before the new shader (its ring of frames starts again from the newest)
    local shown = dn_sync.shown()
    gpu_set_shader(S.gpu and S.gpu.shader, reply.shader, GRID_MB + (reply.denoise and DENOISE_MB or 0))
    -- A file used before: mpv may give its parse back, with its ring of frames as it was left
    if reply.reused then dn_sync.new_ring() end
    local w, h = tonumber(reply.output and reply.output[1]), tonumber(reply.output and reply.output[2])
    local sw, sh = S.current_source.width, S.current_source.height
    if w and h and sw and sh and math.abs(w / h - sw / sh) > 0.005 then
        mp.set_property("file-local-options/video-aspect-override", string.format("%d:%d", w, h))
    else
        mp.set_property("file-local-options/video-aspect-override", "no")
    end
    mp.set_property("file-local-options/interpolation", "no")
    -- The RGB layout has mpv convert the whole source first: its chroma upscaled bilinearly, the warp resamples it
    -- anyway (the app's Lanczos took 10 ms of a 5.3K frame on an M1 Pro)
    mp.set_property("file-local-options/cscale", reply.mode == "grid" and "bilinear" or (mp.get_property("options/cscale") or "bilinear"))
    S.gpu = { shader = reply.shader, output = { w, h }, mode = reply.mode, chromatic = reply.chromatic,
            focal = type(reply.focal) == "table" and reply.focal or nil, denoise = reply.denoise or false, translation = reply.translation or false }
    -- A stretch of the clip's motion data (gpu_ahead extends it as it plays): what it covers, up to where it's
    -- settled, and the request it came from
    if reply.partial and type(reply.covers) == "table" and type(reply.request) == "table" then
        S.gpu.covers, S.gpu.settled, S.gpu.duration, S.gpu.request = reply.covers, reply.settled, reply.duration, reply.request
    end
    mp.set_property_native("user-data/gyroflow/gpu", { mode = reply.mode or "", chromatic = reply.chromatic or false })
    gpu_render_scale()
    dn_sync(shown)
    -- The panel's sideways jerks: computed for this clip or not
    publish()
end

-- A clip without a project plays from a stretch of its motion data: the table is extended ahead of where it plays,
-- a step at a time, the new one in place of the one before as it comes in (its frames where both have them are the
-- old one's: nothing on screen changes), ahead.lua says when and how far (docs/design/gpu-table-ahead.md). Before, the whole
-- clip's motion data was read in one go after the first stretch: 1.7 GB from the NAS, 35-48 s, and playback ran past
-- the stretch's 20 s into the unstabilized picture until it came (2026-11-19).
-- If it still plays up to the end of what the table knows (a share slower than the video), it waits there, paused
-- with a card, rather than playing on unstabilized: the picture holds the last known correction past it, which is
-- the shaking camera turned by a fixed angle, the dropout itself. Paused by the plugin (`held`): unpaused when the
-- table is there, unless the viewer paused or played meanwhile
function gpu_ahead.card(t)
    if mp.notify then return mp.notify(t) end
    if t.kind == "hide" then mp.osd_message("", 0) else mp.osd_message(t.title or "", t.kind == "working" and 30 or 3) end
end
function gpu_ahead.release()
    if not gpu_ahead.held then return end
    gpu_ahead.held = false
    gpu_ahead.card({ kind = "hide", id = "gyroflow-ahead" })
    if mp.get_property_native("pause") then mp.set_property_native("pause", false) end
end
function gpu_ahead.tick()
    local g = S.gpu
    if not g or not g.covers or not g.request or g.shader == gpu_ahead.failed then return gpu_ahead.release() end
    local tp = mp.get_property_number("time-pos")
    if not tp then return end
    local offset = g.request.time_offset or 0
    local la, lb = mp.get_property_number("ab-loop-a"), mp.get_property_number("ab-loop-b")
    local s = { pos = tp + offset, duration = g.duration, paused = mp.get_property_native("pause") and not gpu_ahead.held,
                covers = g.covers, settled = g.settled, busy = gpu_ahead.busy == g.shader,
                loop = (la and lb) and { a = la + offset, b = lb + offset } or nil }
    local want = gpu_ahead.policy.next(s)
    if want then
        local req = {}
        for k, v in pairs(g.request) do req[k] = v end
        req.start, req["until"], req.urgent = want.start, want.until_, want.urgent
        local source, shader = S.current_source, g.shader
        gpu_ahead.busy, s.busy = shader, true
        gpu_ahead.ask(req, function(reply)
            if gpu_ahead.busy == shader then gpu_ahead.busy = nil end
            if S.current_source ~= source or not S.gpu or S.gpu.shader ~= shader then return end
            if not reply.shader then
                gpu_ahead.failed = shader
                mp.msg.warn("the table ahead of where it plays: " .. tostring(reply.error or reply.unsupported))
                return gpu_ahead.release()
            end
            reply.request = g.request
            gpu_apply(reply)
            mp.msg.verbose(string.format("the table now covers %s s (asked from %.1f s), %d ms", reply.covers and
                string.format("%.1f-%.1f", reply.covers[1], reply.covers[2]) or "all", want.start, reply.ms or 0))
            gpu_ahead.tick()
        end)
    end
    if gpu_ahead.policy.wait(s) then
        if not gpu_ahead.held and not mp.get_property_native("pause") then
            gpu_ahead.held = true
            mp.set_property_native("pause", true)
            gpu_ahead.card({ kind = "working", id = "gyroflow-ahead", title = "Stabilization is catching up",
                   detail = "Reading the clip's motion data from the share; it plays on in a moment" })
            mp.msg.info(string.format("waiting at %.1f s for the table (it has %.1f-%.1f s)", s.pos, g.covers[1], g.covers[2]))
        end
    else
        gpu_ahead.release()
    end
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

-- The GPU path after a change (view, level, horizon, lens, edges, color fringes, the frame): the shader written again
-- with the new settings and put in place of the old, the playing frame redrawn with it. Settings the shader can't do
-- leave the clip as recorded, a card saying why
gpu_refresh = function()
    if not S.gpu or not S.current_source then return end
    if S.gpu_pending then S.gpu_pending = "again" return end
    if not filter_lens() then
        -- Off, as recorded: the video as it is
        local shown = dn_sync.shown()
        gpu_set_shader(S.gpu.shader, nil)
        S.gpu.shader = nil
        mp.set_property("file-local-options/video-aspect-override", "no")
        dn_sync(shown)
        return
    end
    S.gpu_pending = true
    local source = S.current_source
    gpu_table(source, function(reply)
        local again = S.gpu_pending == "again"
        S.gpu_pending = false
        if S.current_source ~= source or not S.gpu then
            -- Asked again meanwhile for what plays now (a project saved in Gyroflow while this one was written): that
            -- one's written, not dropped with this
            if again and S.gpu and S.current_source then gpu_refresh() end
            return
        end
        if reply.shader then
            gpu_apply(reply)
        else
            -- These settings the warp can't do: as recorded until they change (the warp out, mpv's own turn)
            local shown = dn_sync.shown()
            gpu_set_shader(S.gpu.shader, nil)
            S.gpu.shader = nil
            mp.set_property("file-local-options/video-aspect-override", "no")
            dn_sync(shown)
            mp.notify({ id = "gyroflow:unstabilized", kind = "info", seconds = 6, title = "Not stabilized with these settings",
                        detail = unable_text(reply.unsupported or reply.error) })
            mp.msg.warn("the warp can't: " .. tostring(reply.unsupported or reply.error))
        end
        if again then gpu_refresh() end
    end)
end

-- The table ahead: looked at four times a second and after a seek; the viewer's play or pause is theirs; a new file starts again
function M.ahead_timer() if S.gpu and S.gpu.covers then gpu_ahead.tick() elseif gpu_ahead.held then gpu_ahead.release() end end
function M.ahead_seek() if S.gpu and S.gpu.covers then gpu_ahead.tick() end end
function M.ahead_pause(_, paused)
    if gpu_ahead.held and paused == false then gpu_ahead.held = false; gpu_ahead.card({ kind = "hide", id = "gyroflow-ahead" }) end
end
function M.ahead_end() gpu_ahead.release(); gpu_ahead.busy = nil end
-- (never the hold shader: it is the plugin's own file, not one written for a clip. Quitting while it held removed
-- it from the app, and the next start had no shader to hold with)
function M.shutdown() if S.gpu and S.gpu.shader and S.gpu.shader ~= GPU_HOLD_SHADER then os.remove(S.gpu.shader) end end
-- Ctrl+p on the GPU path: the denoise in or out of the shader
function M.still_changed() if S.gpu then gpu_refresh() end end

function M.install(w)
    W = w
    S = W.S
    DENOISE_MB, GPU_HOLD_SHADER, GPU_SERVER, GPU_TABLE, GRID_MB, dn_sync, gpu_reuse, o, trans = W.DENOISE_MB, W.GPU_HOLD_SHADER, W.GPU_SERVER, W.GPU_TABLE, W.GRID_MB, W.dn_sync, W.gpu_reuse, W.o, W.trans
    S.gpu_count = 0
    S.gpu_render_short = nil
    W.gpu_apply, W.gpu_blocked, W.gpu_camera, W.gpu_eligible, W.gpu_missing, W.gpu_refresh = gpu_apply, gpu_blocked, gpu_camera, gpu_eligible, gpu_missing, gpu_refresh
    W.gpu_render_scale, W.gpu_settings, W.gpu_table, W.gpu_user_data, W.mpv_has_pts, W.play_unstabilized = gpu_render_scale, gpu_settings, gpu_table, gpu_user_data, mpv_has_pts, play_unstabilized
    W.unable_text = unable_text
end

return M
