-- Ctrl+m: the clip's motion measured from its picture (vs-gyroflow's gyroflow-optical, Gyroflow's own analysis run by
-- the player: no Gyroflow app, no project), kept in <stem>.afp-motion beside it, and the clip played again with it from
-- where it was (docs/design/optical-without-gyroflow.md). For a clip without motion data the player stabilizes with: none
-- recorded, or the camera's own stabilization on. A clip measured before is measured again; one with a Gyroflow project
-- isn't (the project plays).
--
-- How far it is: user-data/gyroflow/measuring, the fraction done (false when nothing is measured), for the side panel,
-- and a card. The measuring goes on while the clip plays; another clip opened meanwhile doesn't stop it.
--
-- Also the lens chosen in the side panel for the clip's whole mode (script messages assign-lens <id>, forget-lens), and
-- the panel's lens dialog (side-panel/lensdialog.lua), the Library's in the player: lens-dialog (the mode's lenses,
-- gyroflow-optical --lenses, in user-data/gyroflow/lenses), lens-find / lens-estimate (run on the clip playing, how far
-- in user-data/gyroflow/lens_job, the list read again as lenses are tried), lens-quick (Find stopped at the first lens
-- that fits: gyroflow.conf lens_quick, on by default), lens-use keep <n> | assign <id> | promote (the lens kept for the
-- mode, then the clip measured again where its motion is measured from the picture or it has none), lens-try keep <n> |
-- assign <id> (this clip alone takes it: source.lens in its own file, which wins over its mode's), lens-forget clip|mode.
--
--   require('measure').setup(env), env: { source = function() -> the plugin's current source, path = clip_path,
--                                         again = play_again, notify = mp.notify, options = the plugin's options }

local mp = require 'mp'
local utils = require 'mp.utils'
local platform = require 'platform'

local M = {}

local OPTICAL = utils.join_path(mp.get_script_directory(), platform.exe("gyroflow-optical"))
local PROBE = utils.join_path(mp.get_script_directory(), platform.exe("gyroflow-probe"))

function M.setup(env)
    local job, again_after = nil, false
    local function measuring(v) mp.set_property_native("user-data/gyroflow/measuring", v) end
    measuring(false)

    local function start()
        if platform.photo_shown() then return end
        -- One under way: this one after it (a lens tried or used meanwhile: measured with it then, not lost)
        if job then
            again_after = true
            return env.notify({ id = "gyroflow:measure", kind = "info", seconds = 3, title = "Still measuring the motion…", detail = "measured again after it" })
        end
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
            -- Asked again meanwhile: measured again now, with what it has now
            if again_after then
                again_after = false
                mp.add_timeout(0.1, start)
            end
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
            -- The motion measured beside the clip (<stem>.afp-motion) noted to the project's catalog, as the player's
            -- other writes are: the Library shows the clip as measured (its probe answer worked out again)
            mp.commandv("script-message-to", "workspace", "catalog-note", (path:gsub("%.[^./\\]+$", "") .. ".afp-motion"))
            env.notify({ id = "gyroflow:measure", kind = "done", seconds = 4, title = "Motion measured", detail = name })
            -- Played again with it, from where it is, when it's still the clip playing
            if env.path() == path then env.again() end
        end)
    end

    mp.add_key_binding("Ctrl+m", "measure-motion", start)

    -- A lens chosen in the side panel's Source for the clip's whole mode (gyroflow-probe --assign-lens, the player's own
    -- lens database), or the one assigned taken away (--forget-lens); the clip played again with what it gives
    local function lens_command(args, done_title)
        local path = env.path()
        if not path or path:find("://", 1, true) then return end
        local name = path:match("([^/\\]+)$") or path
        table.insert(args, 3, path)
        mp.command_native_async({ name = "subprocess", args = args, capture_stdout = true, capture_stderr = true, playback_only = false }, function(_, r)
            local out = r and utils.parse_json((r.stdout or ""):match("[^\n]*") or "") or nil
            if not (r and r.status == 0 and type(out) == "table") then
                return env.notify({ id = "gyroflow:lens", kind = "error", title = "The lens couldn't be kept",
                                    detail = type(out) == "table" and tostring(out.error) or name })
            end
            env.notify({ id = "gyroflow:lens", kind = "done", seconds = 5, title = done_title, detail = tostring(out.mode or name) })
            if env.path() == path then env.again() end
        end)
    end
    mp.register_script_message("assign-lens", function(id)
        if id and id ~= "" then lens_command({ PROBE, "--assign-lens", id }, "Lens kept for this mode") end
    end)
    mp.register_script_message("forget-lens", function()
        lens_command({ PROBE, "--forget-lens" }, "Lens taken away from this mode")
    end)

    -- The lens dialog's
    local o = env.options or {}
    local lens_job = nil
    local function publish(k, v) mp.set_property_native("user-data/gyroflow/" .. k, v) end
    publish("lens_quick", o.lens_quick ~= false)
    local function read_lenses(path)
        mp.command_native_async({ name = "subprocess", args = { OPTICAL, "--lenses", path }, capture_stdout = true, capture_stderr = true, playback_only = false }, function(_, r)
            if env.path() ~= path then return end
            local out = r and utils.parse_json((r.stdout or ""):match("[^\n]*") or "") or nil
            publish("lenses", type(out) == "table" and out or { error = "its lenses couldn't be read" })
        end)
    end
    mp.register_event("file-loaded", function() publish("lenses", false); if not lens_job then publish("lens_job", false) end end)
    mp.register_script_message("lens-dialog", function()
        local path = env.path()
        if not path or path:find("://", 1, true) then return end
        publish("lenses", false)
        read_lenses(path)
    end)
    mp.register_script_message("lens-quick", function()
        o.lens_quick = not (o.lens_quick ~= false)
        publish("lens_quick", o.lens_quick)
        platform.save_options("gyroflow", { { "lens_quick", o.lens_quick and "yes" or "no" } })
    end)
    local function lens_run(action)
        if lens_job then return end
        local path = env.path()
        if not path or path:find("://", 1, true) then return end
        local dir = platform.temp_dir()
        local tag = string.format("mpv-lens-%d-%d", utils.getpid(), math.floor(mp.get_time() * 1000))
        local request, status = utils.join_path(dir, tag .. ".json"), utils.join_path(dir, tag .. "-status.json")
        local f = io.open(request, "w")
        if not f then return env.notify({ id = "gyroflow:lens", kind = "error", title = "The lens couldn't be found", detail = "can't write " .. request }) end
        f:write(utils.format_json({ action = action, clips = { path }, quick = o.lens_quick ~= false, status = status }))
        f:close()
        local kind = action == "estimate-lens" and "estimate" or "find"
        lens_job = { kind = kind, state = "running", fraction = 0 }
        publish("lens_job", lens_job)
        local seen = ""
        local poll = mp.add_periodic_timer(0.5, function()
            local sf = io.open(status, "r")
            if not sf then return end
            local st = utils.parse_json(sf:read("*a") or "")
            sf:close()
            if type(st) ~= "table" then return end
            lens_job.fraction = tonumber(st.fraction) or lens_job.fraction
            publish("lens_job", lens_job)
            -- A lens more tried: the list read again, with what's found so far
            local r = type(st.results) == "table" and st.results[1] or nil
            local sig = r and (tostring(type(r.tried) == "table" and #r.tried or 0) .. (r.estimate and "e" or "")) or ""
            if sig ~= seen and env.path() == path then seen = sig; read_lenses(path) end
        end)
        mp.command_native_async({ name = "subprocess", args = { OPTICAL, "--batch", request }, capture_stdout = true, capture_stderr = true, playback_only = false }, function(_, r)
            poll:kill()
            local sf = io.open(status, "r")
            local st = sf and utils.parse_json(sf:read("*a") or "") or nil
            if sf then sf:close() end
            os.remove(request); os.remove(status)
            local res = type(st) == "table" and type(st.results) == "table" and st.results[1] or nil
            local err = type(res) == "table" and res.error or (not res and (r and (r.stderr or ""):match("([^\n]+)\n?$")) or nil)
            lens_job = nil
            publish("lens_job", { kind = kind, state = err and "failed" or "done", message = err and tostring(err) or nil })
            if env.path() == path then read_lenses(path) end
        end)
    end
    mp.register_script_message("lens-find", function() lens_run("find-lens") end)
    mp.register_script_message("lens-estimate", function() lens_run("estimate-lens") end)
    -- The clip played again with its lens as it is now: measured again with it first where its motion is the
    -- picture's or it has none (a clip with gyro waiting for a lens plays from its gyro now)
    local function after_lens(path, measure)
        if env.path() ~= path then return end
        local source = env.source()
        local own = source and (source.kind == "project" or (source.kind == "default" and not source.optical))
        if measure and not own then start() else env.again() end
    end
    -- Its own file written (a lens tried on it): noted to the project's catalog, as the player's other writes are
    local function noted(file) if file then mp.commandv("script-message-to", "workspace", "catalog-note", file) end end
    local function run(args, fail, done)
        mp.command_native_async({ name = "subprocess", args = args, capture_stdout = true, capture_stderr = true, playback_only = false }, function(_, r)
            local out = r and utils.parse_json((r.stdout or ""):match("[^\n]*") or "") or nil
            if not (r and r.status == 0 and type(out) == "table" and not out.error) then
                return env.notify({ id = "gyroflow:lens", kind = "error", title = fail, detail = type(out) == "table" and tostring(out.error) or "" })
            end
            done(out)
        end)
    end
    -- Use for the mode: the lens chosen kept for the whole mode (promote: the one tried on this clip, taken away from
    -- it then: the mode's is the same), the clip measured again with it
    mp.register_script_message("lens-use", function(how, what)
        local path = env.path()
        if not path or path:find("://", 1, true) or not what then return end
        if how == "promote" then
            return run({ OPTICAL, "--promote-lens", path }, "The lens couldn't be given to the mode", function(out)
                run({ OPTICAL, "--try-lens", path, "forget" }, "The clip's own lens couldn't be taken away", function(o2)
                    noted(o2.file)
                    env.notify({ id = "gyroflow:lens", kind = "done", seconds = 4, title = "Lens kept for this mode", detail = tostring(out.mode or "") })
                    after_lens(path, false)
                end)
            end)
        end
        local args = how == "keep" and { OPTICAL, "--keep-lens", path, what } or { PROBE, "--assign-lens", path, what }
        run(args, "The lens couldn't be kept", function(out)
            env.notify({ id = "gyroflow:lens", kind = "done", seconds = 4, title = "Lens kept for this mode", detail = tostring(out.mode or "") })
            after_lens(path, true)
        end)
    end)
    -- Try on this clip: the lens kept for this clip alone (source.lens in its own file), the clip measured again with it
    mp.register_script_message("lens-try", function(how, what)
        local path = env.path()
        if not path or path:find("://", 1, true) or not what then return end
        run({ OPTICAL, "--try-lens", path, how, what }, "The lens couldn't be tried", function(out)
            noted(out.file)
            env.notify({ id = "gyroflow:lens", kind = "done", seconds = 4, title = "Trying the lens on this clip alone", detail = tostring(out.lens or "") })
            after_lens(path, true)
        end)
    end)
    -- Forget: the lens tried on this clip (back to its mode's, measured again with it), or the mode's chosen one
    mp.register_script_message("lens-forget", function(which)
        local path = env.path()
        if not path or path:find("://", 1, true) then return end
        if which == "clip" then
            return run({ OPTICAL, "--try-lens", path, "forget" }, "The clip's own lens couldn't be taken away", function(out)
                noted(out.file)
                env.notify({ id = "gyroflow:lens", kind = "done", seconds = 4, title = "Back to the mode's lens" })
                after_lens(path, true)
            end)
        end
        lens_command({ PROBE, "--forget-lens" }, "Lens taken away from this mode")
    end)
end

return M
