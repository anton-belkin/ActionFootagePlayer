-- translation: Ctrl+j, the hand's sideways jerks taken out (the translation compensation, warp.glsl's trans_shift),
-- for the gyroflow plugin. main.lua wires it
local mp = require 'mp'
local utils = require 'mp.utils'
local platform = require 'platform'

local M = {}
-- Set by install (main.lua's W): the state the parts share, and what this part takes from the others
local W, S, o
-- The other parts' functions, looked up in W as they're called
local function gpu_refresh(...) return W.gpu_refresh(...) end
local function gpu_settings(...) return W.gpu_settings(...) end
local function publish(...) return W.publish(...) end

-- The translation compensation (Ctrl+j, the panel's Transform): taken out or not, its computing and how far it is
local trans = { on = true, job = nil, progress = 0 }

-- Ctrl+j's translation compensation computed this many times this session (a new shader each time)

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
    local source = S.current_source
    if not source or not source.project then return end
    -- Measured on the stabilization as it plays (the settings' project, level, view, horizon, water: what moves the
    -- camera, not the frame it's shown in)
    local s, how = gpu_settings(source), {}
    for _, k in ipairs({ "project", "level", "underwater", "horizon_lock", "wide" }) do how[k] = s[k] end
    local video, args = source.project, { EXPORTER, "--translation", "--settings", utils.format_json(how) }
    if video:lower():match("%.gyroflow$") then video = mp.get_property("path") end
    local progress = utils.join_path(platform.temp_dir(), string.format("mpv-translation-%d.json", utils.getpid()))
    args[#args + 1], args[#args + 2], args[#args + 3] = "--progress", progress, video
    trans.progress = 0
    local poll = mp.add_periodic_timer(0.5, function()
        local f = io.open(progress, "r")
        local p = f and utils.parse_json(f:read("*a") or "") or nil
        if f then f:close() end
        trans.progress = tonumber(p and p.translation) or trans.progress
        mp.notify({ id = "gyroflow:translation", kind = "working", title = "Measuring the sideways jerks", detail = "In the picture, for this clip (kept for next time)",
                    share = (tonumber(trans.progress) or 0) / 100 })
        publish()
    end)
    mp.notify({ id = "gyroflow:translation", kind = "working", title = "Measuring the sideways jerks", detail = "In the picture, for this clip (kept for next time)" })
    trans.job = mp.command_native_async({ name = "subprocess", args = args, capture_stdout = true, capture_stderr = true, playback_only = false }, function(_, r)
        poll:kill()
        trans.job = nil
        os.remove(progress)
        publish()
        local reply = r and r.status == 0 and utils.parse_json((r.stdout or ""):match("[^\n]*")) or nil
        if not (reply and reply.translation) then
            local why = r and (r.stderr or ""):match("failed, ([^\n]*)") or r and (r.stderr or ""):match("skipped, ([^\n]*)")
            return mp.notify({ id = "gyroflow:translation", kind = "error", title = "The sideways jerks couldn't be measured", detail = why })
        end
        S.trans_rev, trans.on = S.trans_rev + 1, true
        trans_apply()
        if S.current_source == source then gpu_refresh() end
        local share = tonumber(reply.share) or 0
        mp.notify({ id = "gyroflow:translation", kind = "done", seconds = 4, title = share < 0.25 and "Little to take out of the sideways jerks" or "Sideways jerks taken out",
                    detail = share < 0.25 and "No close subject to measure them on"
                        or string.format("%.1f → %.1f px (Ctrl+j off and on)", tonumber(reply.before) or 0, tonumber(reply.after) or 0) })
    end)
    publish()
end
function M.key()
    if platform.photo_shown() then return end
    if not (S.gpu and S.gpu.shader) then
        return mp.osd_message("Gyroflow: the sideways jerks are taken out only where it's stabilized", 3)
    end
    if not o.translation then return mp.osd_message("Gyroflow: translation compensation is off (gyroflow.conf translation=no)", 3) end
    if trans.job then return mp.osd_message("Gyroflow: still measuring the sideways jerks…", 2) end
    if not S.gpu.translation then return trans_compute() end
    trans.on = not trans.on
    trans_apply()
    publish()
    mp.osd_message("Gyroflow: sideways jerks " .. (trans.on and "taken out" or "left as recorded"), 2)
end

function M.install(w)
    W = w
    S = W.S
    o = W.o
    S.trans_rev = 0
    W.trans = trans
end

return M
