-- Ctrl+m: the clip's motion measured from its picture (vs-gyroflow's gyroflow-optical, Gyroflow's own analysis run by
-- the player: no Gyroflow app, no project), kept in <stem>.afp-motion beside it, and the clip played again with it from
-- where it was (docs/optical-without-gyroflow.md). For a clip without motion data the player stabilizes with: none
-- recorded, or the camera's own stabilization on. A clip measured before is measured again; one with a Gyroflow project
-- isn't (the project plays).
--
-- How far it is: user-data/gyroflow/measuring, the fraction done (false when nothing is measured), for the side panel,
-- and a card. The measuring goes on while the clip plays; another clip opened meanwhile doesn't stop it.
--
--   require('measure').setup(env), env: { source = function() -> the plugin's current source, path = clip_path,
--                                         again = play_again, notify = mp.notify }

local mp = require 'mp'
local utils = require 'mp.utils'
local platform = require 'platform'

local M = {}

local OPTICAL = utils.join_path(mp.get_script_directory(), platform.exe("gyroflow-optical"))

function M.setup(env)
    local job = nil
    local function measuring(v) mp.set_property_native("user-data/gyroflow/measuring", v) end
    measuring(false)

    local function start()
        if platform.photo_shown() then return end
        if job then return env.notify({ id = "gyroflow:measure", kind = "info", seconds = 3, title = "Still measuring the motion…" }) end
        local path = env.path()
        if not path or path:find("://", 1, true) then return end
        local source = env.source()
        if source and source.kind == "project" then
            return env.notify({ id = "gyroflow:measure", kind = "info", seconds = 4, title = "It has a Gyroflow project",
                                detail = "The project plays: its motion is the project's" })
        end
        if source and source.kind == "default" and not source.optical then
            return env.notify({ id = "gyroflow:measure", kind = "info", seconds = 4, title = "Stabilized with its own gyro",
                                detail = "The camera's motion data is better than what the picture says" })
        end
        local dir = platform.temp_dir()
        local tag = string.format("mpv-measure-%d-%d", utils.getpid(), math.floor(mp.get_time() * 1000))
        local request = utils.join_path(dir, tag .. ".json")
        local status = utils.join_path(dir, tag .. "-status.json")
        local f = io.open(request, "w")
        if not f then return env.notify({ id = "gyroflow:measure", kind = "error", title = "The motion couldn't be measured", detail = "can't write " .. request }) end
        f:write(utils.format_json({ clips = { path }, redo = true, status = status }))
        f:close()
        local name = path:match("([^/\\]+)$") or path
        local card = function(p)
            env.notify({ id = "gyroflow:measure", kind = "working", title = "Measuring the motion in the picture",
                         detail = name .. (p and string.format(" · %d %%", math.floor(p * 100)) or "") })
        end
        card(nil)
        measuring(0)
        local poll = mp.add_periodic_timer(0.5, function()
            local s = io.open(status, "r")
            if not s then return end
            local st = utils.parse_json(s:read("*a") or "")
            s:close()
            local p = type(st) == "table" and tonumber(st.fraction)
            if p then measuring(p) card(p) end
        end)
        job = mp.command_native_async({ name = "subprocess", args = { OPTICAL, "--batch", request }, capture_stdout = true, capture_stderr = true,
                                        playback_only = false }, function(_, r)
            poll:kill()
            job = nil
            measuring(false)
            local s = io.open(status, "r")
            local st = s and utils.parse_json(s:read("*a") or "") or nil
            if s then s:close() end
            os.remove(request)
            os.remove(status)
            local made = type(st) == "table" and tonumber(st.made) == 1
            if not made then
                local why = type(st) == "table" and (type(st.failures) == "table" and st.failures[1] and st.failures[1][2]
                    or type(st.left_out) == "table" and st.left_out[1] and st.left_out[1][2] or st.message)
                    or r and (r.stderr or ""):match("([^\n]+)\n?$") or "gyroflow-optical failed"
                return env.notify({ id = "gyroflow:measure", kind = "error", title = "The motion couldn't be measured", detail = tostring(why) })
            end
            env.notify({ id = "gyroflow:measure", kind = "done", seconds = 4, title = "Motion measured", detail = name })
            -- Played again with it, from where it is, when it's still the clip playing
            if env.path() == path then env.again() end
        end)
    end

    mp.add_key_binding("Ctrl+m", "measure-motion", start)
end

return M
