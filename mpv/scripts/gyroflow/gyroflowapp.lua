-- gyroflowapp: Ctrl+e, the clip's project edited in Gyroflow (written beside it first when it plays without one), and
-- a save there taken up here, for the gyroflow plugin. main.lua wires it
local mp = require 'mp'
local utils = require 'mp.utils'
local platform = require 'platform'

local M = {}
-- Set by install (main.lua's W): the state the parts share, and what this part takes from the others
local W, S, GYROFLOW_APP, PROJECTIONS, PROJECT_WRITER, o
-- The other parts' functions, looked up in W as they're called
local function clip_path(...) return W.clip_path(...) end
local function clip_settings(...) return W.clip_settings(...) end
local function file_exists(...) return W.file_exists(...) end
local function find_project(...) return W.find_project(...) end
local function play_again(...) return W.play_again(...) end
local function project_paths(...) return W.project_paths(...) end
local function project_video_size(...) return W.project_video_size(...) end
local function publish(...) return W.publish(...) end
local function settings_changed(...) return W.settings_changed(...) end
local function shown_aspect(...) return W.shown_aspect(...) end
local function workspace_of(...) return W.workspace_of(...) end

-- Edit in Gyroflow. The project goes next to the video (or where the workspace plugin says), where find_project()
-- picks it up from then on: the one it plays with, or for a video without one (its own motion data with the default
-- preset) the same written out by gyroflow-project, so Gyroflow opens it as it plays here. Gyroflow overwrites the
-- project it opened when it saves (Cmd+S), and the stabilization reloads with it while the video stays where it is.
-- A video with no motion data opens as it is: Gyroflow finds the motion from the picture (optical flow), and the
-- project it saves next to the video is waited for, then the video loads again to play with it.
local WATCH_INTERVAL = 0.5
local watch = nil

-- The project's version and when a save is taken up (projectwatch.lua)
local projectwatch = require 'projectwatch'

local function stop_watching()
    if watch then watch.timer:kill() end
    watch = nil
    if S.waiting then
        S.waiting = false
        publish()
    end
end

local function reload_project(project, version)
    local w, h, fps, ow, oh = project_video_size(project)
    if not w then
        mp.msg.warn("not reloading " .. project .. ": not a Gyroflow project")
        return
    end
    -- The same video: its camera records the water or not as it did. The save's version goes into the warp's settings
    -- (player_args): the shader is reused by its settings, and the table server keeps the clips it loaded, and the
    -- project's path hasn't changed
    local source = { project = project, kind = "project", width = w, height = h, fps = fps, out_width = ow, out_height = oh,
                     water = S.current_source and S.current_source.water, version = version }
    -- Playing from now: the settings below write the GPU path's shader again (settings_changed) for this source. A
    -- table asked for the one before is dropped as it comes (gpu_refresh), which left the old shader in place
    S.current_source = source
    -- The project as Gyroflow has it: written from the wider view, it has that view's FOV itself, and the horizon lock
    -- and the lens it played with here
    S.view = "normal"
    clip_settings(source)
    -- Its shader from the project as saved, put in place of the old while the video plays on
    settings_changed()
    publish()
    mp.msg.info("reloaded " .. project)
    mp.osd_message("Gyroflow: project reloaded", 3)
end

-- Gyroflow writes the file in place: a version is taken once it stayed the same for an interval (projectwatch.lua). For
-- a video that had no project, the first one found next to it, once it stays the same
local function check_project()
    local project = watch.project or find_project(watch.video)
    if not project then return end
    local action, version = projectwatch.step(watch, projectwatch.version(utils, project))
    if action == "found" then
        -- Saved from Gyroflow beside the clip: into the project's catalog (the Library's motion data and badge)
        mp.commandv("script-message-to", "workspace", "catalog-note", project)
        mp.msg.info("found " .. project .. ", loading the video again with it")
        mp.osd_message("Gyroflow: project saved, stabilizing", 3)
        stop_watching()
        play_again()
    elseif action == "reload" then
        reload_project(watch.project, version)
    end
end

-- The command that opens the project (or a video) in Gyroflow and returns (status 0) once it's started, without
-- waiting for it. nil on Windows for a video without gyroflow_app: the app videos open with is the player's
-- `preset`: Gyroflow's preset as JSON (' for "), applied once the video is open (its --open and --preset): a video
-- without a project opened with what the player knows of it (the lens under water)
local function gyroflow_command(file, preset)
    if platform.macos then
        -- A new one: one already open wouldn't be given the preset
        if preset then return { "/usr/bin/open", "-n", "-b", GYROFLOW_APP, "--args", "--open", file, "--preset", preset } end
        return { "/usr/bin/open", "-b", GYROFLOW_APP, file }
    end
    if platform.windows then
        if o.gyroflow_app ~= "" then
            if preset then return { "cmd", "/d", "/c", "start", "", o.gyroflow_app, "--open", file, "--preset", preset } end
            return { "cmd", "/d", "/c", "start", "", o.gyroflow_app, file }
        end
        -- The app .gyroflow files are associated with (Gyroflow's installer sets it)
        if not file:lower():match("%.gyroflow$") then return nil end
        return { "cmd", "/d", "/c", "start", "", file }
    end
    -- Started in the background, and fails when there's no such program
    local app = o.gyroflow_app ~= "" and o.gyroflow_app or "gyroflow"
    if preset then
        return { "sh", "-c", 'command -v "$1" >/dev/null || exit 127; "$1" --open "$2" --preset "$3" >/dev/null 2>&1 &', "sh", app, file, preset }
    end
    return { "sh", "-c", 'command -v "$1" >/dev/null || exit 127; "$1" "$2" >/dev/null 2>&1 &', "sh", app, file }
end

-- `project`: the project to watch for changes; nil for a video without one (`file`, the video itself), to watch
-- for the project Gyroflow saves next to it
local function open_in_gyroflow(file, project, opened_message, preset)
    local args = gyroflow_command(file, preset)
    if not args then
        mp.osd_message("Gyroflow: set gyroflow_app in gyroflow.conf to open videos in it", 4)
        return
    end
    mp.set_property_bool("pause", true)
    local video = clip_path()
    if not (watch and watch.project == project and watch.video == video) then
        stop_watching()
        watch = { project = project, video = video, version = project and projectwatch.version(utils, project) or nil }
        watch.timer = mp.add_periodic_timer(WATCH_INTERVAL, check_project)
    end
    S.waiting = project == nil
    publish()
    mp.command_native_async({ name = "subprocess", args = args, playback_only = false, capture_stderr = true }, function(_, r)
        if r and r.status == 0 then
            mp.osd_message(opened_message, 6)
        else
            mp.msg.warn("open: " .. (r and r.stderr or ""))
            mp.osd_message("Gyroflow: couldn't open the app (is Gyroflow installed?)", 4)
            stop_watching()
        end
    end)
end

local function edit_project(project)
    open_in_gyroflow(project, project, "Gyroflow: editing " .. platform.file_name(project) .. "\nsave it there to see it here")
end

function M.edit_key()
    if platform.photo_shown() then return end
    local path = clip_path()
    if not path or path:find("://", 1, true) then
        mp.osd_message("Gyroflow: not a local file", 1.5)
        return
    end
    -- One saved meanwhile is the user's: open it as it is
    local project = find_project(path)
    if project then return edit_project(project) end
    if not S.current_source or S.current_source.kind == "lens" then
        -- Gyroflow's "Lens is underwater" for it: the player's Lens under water (yes: water's refraction, no: none);
        -- auto says nothing (Gyroflow's own default)
        local r = S.underwater == "yes" and "1.33" or S.underwater == "no" and "1.0" or nil
        local preset = r and ("{'version': 2, 'light_refraction_coefficient': " .. r .. "}") or nil
        return open_in_gyroflow(path, nil, "Gyroflow: opened " .. platform.file_name(path) .. ", which has no motion data"
            .. (preset and (S.underwater == "yes" and " (its lens under water)" or " (its lens above water)") or "") .. ":\n"
            .. "let it find the motion from the picture (optical flow) and save the project next to the video,\n"
            .. "then it plays here stabilized", preset)
    end
    if not file_exists(PROJECT_WRITER) then
        mp.osd_message("Gyroflow: gyroflow-project isn't installed", 2)
        return
    end
    project = select(2, project_paths(path))
    -- As it plays here: the level, the view (off: the preset's), the horizon lock, the lens
    local args = { PROJECT_WRITER, "--underwater", S.underwater, "--level", S.level }
    if S.view == "wide" and not S.stab_off then table.insert(args, "--wide") end
    if S.horizon ~= nil then
        table.insert(args, "--horizon-lock"); table.insert(args, S.horizon and "100" or "0")
    end
    if S.current_source.preset then
        table.insert(args, "--preset"); table.insert(args, S.current_source.preset)
    end
    -- The frame as it plays: the project has it from then on (its output size, rotation and projection)
    if S.frame.rotation ~= 0 then table.insert(args, "--rotation"); table.insert(args, tostring(S.frame.rotation)) end
    if shown_aspect() ~= "original" then table.insert(args, "--aspect"); table.insert(args, shown_aspect()) end
    if PROJECTIONS[S.frame.projection] and S.frame.projection ~= "linear" then table.insert(args, "--projection"); table.insert(args, S.frame.projection) end
    -- A moment's own project: its part as the trim range, smoothed on its own or as part of the clip
    local ws = workspace_of(path)
    if ws and ws.moment and ws.moment["in"] and ws.snap then
        table.insert(args, "--trim"); table.insert(args, string.format("%.3f,%.3f", ws.moment["in"], ws.moment.out))
        table.insert(args, "--alone"); table.insert(args, ws.alone and "yes" or "no")
    end
    table.insert(args, path); table.insert(args, project)
    mp.notify({ id = "gyroflow:project", kind = "working", title = "Writing the Gyroflow project", detail = platform.file_name(project) })
    mp.command_native_async({ name = "subprocess", args = args, playback_only = false, capture_stderr = true }, function(_, r)
        if r and r.status == 0 then
            mp.msg.info("wrote " .. project)
            mp.notify({ id = "gyroflow:project", kind = "hide" })
            mp.commandv("script-message-to", "workspace", "catalog-note", project)
            edit_project(project)
        else
            local err = (r and r.stderr or ""):gsub("%s+$", "")
            mp.msg.warn("gyroflow-project: " .. err)
            mp.notify({ id = "gyroflow:project", kind = "error", title = "Couldn't write the Gyroflow project", detail = err:match("([^\n]+)$") or "gyroflow-project failed" })
        end
    end)
end


function M.install(w)
    W = w
    S = W.S
    GYROFLOW_APP, PROJECTIONS, PROJECT_WRITER, o = W.GYROFLOW_APP, W.PROJECTIONS, W.PROJECT_WRITER, W.o
    W.stop_watching = stop_watching
end

return M
