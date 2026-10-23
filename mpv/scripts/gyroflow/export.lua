-- Shift+E: export the current video the way it plays, stabilized and with the D-Log M LUT (the dlogm plugin's),
-- with gyroflow-export (one pass, into a stabilized/ folder next to the video). With an A-B loop set (the `l`
-- key), only that part. Playback goes on while it runs; the progress shows in a corner, the result on screen and
-- as a notification. Shift+E again on a video being exported, twice, cancels it.
--
-- The export and playback share the GPU and the video decoder/encoder, and at the same priority playback drops
-- half its frames. So while the video plays the export runs at background priority (macOS's lowest: playback
-- keeps its frame rate, the export nearly stops), and at full speed (~35 fps for 4K60 on an M1 Pro) while it's
-- paused. gyroflow-export switches itself and its ffmpegs on SIGUSR1/SIGUSR2, and reports its progress (and pid)
-- in a file this polls: mpv's subprocess only gives the output at the end.
--
-- Loaded by main.lua.

local mp = require 'mp'
local utils = require 'mp.utils'

local EXPORT = utils.join_path(mp.get_script_directory(), "gyroflow-export")
-- Shift+E twice within this many seconds cancels
local CONFIRM_SECONDS = 3
-- After SIGTERM, how long gyroflow-export gets to stop before mpv kills it
local KILL_AFTER = 5

-- path -> { name, range, id (the async command), progress (file), state (its last contents), cancelling }
local running = {}
local order = {}
local overlay = mp.create_osd_overlay("ass-events")
local confirm = nil
local playing = false
local timer

local function notify(title, text)
    local script = string.format("display notification %q with title %q", text, title)
    mp.command_native_async({ name = "subprocess", args = { "osascript", "-e", script }, playback_only = false }, function() end)
end

local function signal(job, sig)
    local pid = job.state and job.state.pid
    if not pid then return false end
    mp.command_native({ name = "subprocess", args = { "kill", "-" .. sig, tostring(pid) }, playback_only = false })
    return true
end

-- The last line gyroflow-export printed for this video: "...: skipped, <why>" or "...: failed, <why>"
local function last_line(text)
    local last = nil
    for line in (text or ""):gsub("\r", "\n"):gmatch("[^\n]+") do
        if not line:match("^%s*%d+%%") then last = line end
    end
    return last
end

local function clock(s)
    s = math.floor(s)
    if s >= 3600 then return string.format("%d:%02d:%02d", math.floor(s / 3600), math.floor(s % 3600 / 60), s % 60) end
    return string.format("%d:%02d", math.floor(s / 60), s % 60)
end

local function line(job)
    local st = job.state or {}
    local text = job.name .. job.range .. "  "
    if job.cancelling then return text .. "cancelling" end
    if not st.total then return text .. "starting" end
    text = text .. string.format("%d%%", math.floor(st.done / st.total * 100))
    if st.background then return text .. "  slowed while playing" end
    return text .. string.format("  %.0f fps  %s left", st.fps, clock(st.eta))
end

local function redraw()
    if #order == 0 then
        overlay:remove()
        return
    end
    local lines = {}
    for _, path in ipairs(order) do
        table.insert(lines, (line(running[path]):gsub("\\", "\\\\"):gsub("{", "\\{")))
    end
    overlay.data = "{\\an9\\fs16\\bord1.2}Exporting\\N" .. table.concat(lines, "\\N")
    overlay:update()
end

local function poll()
    for _, path in ipairs(order) do
        local job = running[path]
        local f = io.open(job.progress, "r")
        if f then
            local st = utils.parse_json(f:read("*a") or "")
            f:close()
            if st then job.state = st end
        end
    end
    redraw()
end

local function finish(path)
    local job = running[path]
    running[path] = nil
    for i, p in ipairs(order) do if p == path then table.remove(order, i) break end end
    os.remove(job.progress)
    os.remove(job.progress .. ".tmp")
    if #order == 0 and timer then timer:kill(); timer = nil end
    redraw()
    return job
end

local function cancel(path)
    local job = running[path]
    job.cancelling = true
    redraw()
    -- SIGTERM: gyroflow-export stops its ffmpegs and removes the unfinished file. mpv's abort is SIGKILL to
    -- gyroflow-export alone, which would leave the encoder finishing a partial file: only a fallback
    if not signal(job, "TERM") then
        mp.abort_async_command(job.id)
        return
    end
    mp.add_timeout(KILL_AFTER, function()
        if running[path] ~= job then return end
        mp.abort_async_command(job.id)
        local partial = job.state.partial
        if partial then mp.add_timeout(2, function() os.remove(partial) end) end
    end)
end

-- Background priority while playing, full speed while paused or idle
local function update_priority()
    local now = not mp.get_property_native("pause") and not mp.get_property_native("idle-active")
    if now == playing then return end
    playing = now
    for _, path in ipairs(order) do signal(running[path], playing and "USR1" or "USR2") end
end
mp.observe_property("pause", "bool", update_priority)
mp.observe_property("idle-active", "bool", update_priority)

-- Quitting the player: mpv kills gyroflow-export (SIGKILL) as it quits, so the files are removed here. Once
-- the partial file is gone the orphaned encoder can't finish it (faststart reopens it by name) and exits
mp.register_event("shutdown", function()
    for _, path in ipairs(order) do
        local job = running[path]
        signal(job, "TERM")
        if job.state and job.state.partial then os.remove(job.state.partial) end
        os.remove(job.progress)
        os.remove(job.progress .. ".tmp")
    end
end)

mp.add_key_binding("E", "export", function()
    local path = mp.get_property("path")
    if not path or path:find("://", 1, true) then
        mp.osd_message("Export: not a local file", 2)
        return
    end
    local job = running[path]
    if job then
        if job.cancelling then return end
        if confirm and confirm.path == path and mp.get_time() < confirm.time + CONFIRM_SECONDS then
            confirm = nil
            cancel(path)
        else
            confirm = { path = path, time = mp.get_time() }
            mp.osd_message("Exporting " .. job.name .. job.range .. ": Shift+E again to cancel", CONFIRM_SECONDS)
        end
        return
    end
    local info = utils.file_info(EXPORT)
    if not info or not info.is_file then
        mp.osd_message("Export: gyroflow-export isn't installed", 2)
        return
    end

    local progress = os.tmpname()
    local args = { EXPORT, "--force", "--progress", progress }
    if playing then table.insert(args, "--background") end
    local a, b = mp.get_property_number("ab-loop-a"), mp.get_property_number("ab-loop-b")
    local range = ""
    if a and b and b > a then
        table.insert(args, "--from"); table.insert(args, string.format("%.3f", a))
        table.insert(args, "--to");   table.insert(args, string.format("%.3f", b))
        range = " " .. clock(a) .. "-" .. clock(b)
    end
    table.insert(args, path)

    local name = path:match("([^/]+)$")
    job = { name = name, range = range, progress = progress }
    running[path] = job
    table.insert(order, path)
    mp.osd_message("Exporting " .. name .. range .. (playing and " (slowly while the video plays)" or ""), 3)
    mp.msg.info("export: " .. table.concat(args, " "))
    job.id = mp.command_native_async({ name = "subprocess", args = args, playback_only = false, capture_stderr = true }, function(_, r)
        finish(path)
        local err = r and r.stderr or ""
        local out = err:match("%-> (/[^\n]+)")
        if job.cancelling then
            mp.osd_message("Export cancelled: " .. name .. range, 4)
            notify("Export cancelled", name .. range)
        elseif r and r.status == 0 and out then
            mp.osd_message("Exported " .. name .. range .. "\n" .. out, 5)
            notify("Exported " .. name, out)
        else
            local why = last_line(err) or "gyroflow-export failed"
            mp.msg.warn("export: " .. err)
            mp.osd_message("Export failed: " .. why, 6)
            notify("Export failed: " .. name, why)
        end
    end)
    if not timer then timer = mp.add_periodic_timer(1, poll) end
    redraw()
end)
