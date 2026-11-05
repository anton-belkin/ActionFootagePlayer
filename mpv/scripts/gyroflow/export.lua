-- Shift+E: export the current video the way it plays, stabilized and with the log curve's LUT (the dlogm plugin's),
-- with gyroflow-export (one pass, into a stabilized/ folder next to the video). With an A-B loop set (the `l`
-- key), only that part. Playback goes on while it runs; the progress shows in a corner, the result on screen and
-- as a notification. Shift+E again on a video being exported, twice, cancels it.
--
-- As it plays: with the dlogm plugin's log curve, Ctrl+l's included, or none when log is off (L) or the clip isn't
-- log (--curve; without the dlogm plugin gyroflow-export reads the curve from the metadata as it does), in its
-- look (Ctrl+s): HDR, SDR by the maker's LUT (--sdr, DJI's) or by NBCU's conversion (--nbcu), HDR + Dolby Vision with the RPU
-- the look plays (--dv-rpu, with each frame's L1 as the player measures it, --dv-live-l1: Apple's devices then show the
-- export as the player does), not stabilized when Ctrl+g turned it off (--no-stabilize: the color
-- only), at Ctrl+k's level (--level), in Ctrl+g's wider view (--wide) with Ctrl+b's edge fill (--edges,
-- --edges-blur), with the horizon locked or not when the gyroflow plugin's menu says so (--horizon-lock), and the lens
-- under water, above it, as the camera says or as the project says, as the menu has it (--underwater). A log
-- clip with nothing to stabilize it with gyroflow-export converts in color only by itself. Script messages, for the
-- right-click menu:
--   export [sdr]         the same as Shift+E, or in SDR whatever plays: the SDR look it plays in, else the one
--                        the dlogm plugin would choose (its sdr_look)
--   cancel-export        cancel this clip's export, without asking
--   reveal-last-export   show the last exported file in Finder (Explorer, the file manager)
--   photo                the frame on screen as a photo (Alt+s; Enter in crop mode): gyroflow-export --photo, as it
--                        plays (the same stabilization, the frame cleaned up from its neighbours as Ctrl+p does, crop
--                        mode's camera and frame), as an SDR screen shows the look (as_it_plays) with a gain map to the
--                        HLG: a HEIC on macOS, an Ultra HDR JPEG elsewhere, in the stabilized/ folder
--
-- The export and playback share the GPU and the video decoder/encoder, and at the same priority playback drops
-- half its frames. So while the video plays the export runs at background priority (macOS's lowest: playback
-- keeps its frame rate, the export nearly stops; on Linux and Windows a few frames a second), and at full speed
-- (~35 fps for 4K60 on an M1 Pro) while it's paused. gyroflow-export switches itself and its ffmpegs on
-- SIGUSR1/SIGUSR2 (on Windows, which has no signals, a word in a control file it polls), and reports its progress
-- (and pid) in a file this polls: mpv's subprocess only gives the output at the end.
--
-- Loaded by main.lua.

local mp = require 'mp'
local utils = require 'mp.utils'
local platform = require 'platform'

local EXPORT = utils.join_path(mp.get_script_directory(), platform.exe("gyroflow-export"))
-- Shift+E twice within this many seconds cancels
local CONFIRM_SECONDS = 3
-- After SIGTERM, how long gyroflow-export gets to stop before mpv kills it
local KILL_AFTER = 5

-- path -> { name, range, id (the async command), progress (file), control (file, Windows), state (the progress
-- file's last contents), cancelling }
local running = {}
local order = {}
local overlay = mp.create_osd_overlay("ass-events")
local confirm = nil
local playing = false
local timer
local last_export = nil

-- For the right-click menu: whether the clip playing is being exported, and the last file exported
local function publish()
    mp.set_property_native("user-data/gyroflow/exporting", running[mp.get_property("path") or ""] ~= nil)
    mp.set_property_native("user-data/gyroflow/last_export", last_export or "")
end
mp.observe_property("path", "string", publish)

-- On Windows, the OSD message is all
local function notify(title, text)
    local args
    if platform.macos then
        args = { "osascript", "-e", string.format("display notification %q with title %q", text, title) }
    elseif not platform.windows then
        args = { "notify-send", "--app-name=mpv", title, text }
    end
    if args then mp.command_native_async({ name = "subprocess", args = args, playback_only = false }, function() end) end
end

-- What gyroflow-export reads in its control file for each signal
local CONTROL_WORDS = { TERM = "stop", USR1 = "background", USR2 = "normal" }

local function signal(job, sig)
    if job.control then
        local f = io.open(job.control, "w")
        if not f then return false end
        f:write(CONTROL_WORDS[sig])
        f:close()
        return true
    end
    local pid = job.state and job.state.pid
    if not pid then return false end
    mp.command_native({ name = "subprocess", args = { "kill", "-" .. sig, tostring(pid) }, playback_only = false })
    return true
end

-- os.tmpname() on Windows names a file in the root of the drive, which isn't writable
local temp_count = 0
local function temp_name(kind)
    if not platform.windows then return os.tmpname() end
    temp_count = temp_count + 1
    return utils.join_path(platform.temp_dir(), string.format("mpv-gyroflow-%d-%d.%s", utils.getpid(), temp_count, kind))
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
    if job.control then os.remove(job.control) end
    if #order == 0 and timer then timer:kill(); timer = nil end
    redraw()
    publish()
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
        if job.control then os.remove(job.control) end
    end
end)

-- The clip playing, or nil (and why on screen)
local function local_path()
    local path = mp.get_property("path")
    if not path or path:find("://", 1, true) then
        mp.osd_message("Export: not a local file", 2)
        return nil
    end
    return path
end

-- The color and stabilization as they play, into args. `sdr`: in SDR whatever plays; `photo`: the photo's image is
-- what an SDR screen shows of the look (--sdr-image), its gain map leads to the HLG:
--   maker                     the maker's LUT
--   hdr                       the HLG as macOS tones it down for an SDR screen (elsewhere NBCU's)
--   nbcu, tonemap, dv, dvsdr  NBCU's conversion: Dolby Vision's own SDR, as macOS draws it, only exists on screen
--                             (AVFoundation's decoders ignore the RPU), and the RPU's trims are fitted to NBCU's; mpv's
--                             tone mapping isn't in gyroflow-export
local function as_it_plays(args, sdr, photo)
    local look, curve = mp.get_property_native("user-data/dlogm/look"), mp.get_property_native("user-data/dlogm/curve")
    if look == "off" or curve == "" then
        table.insert(args, "--curve"); table.insert(args, "none")
    elseif curve then
        table.insert(args, "--curve"); table.insert(args, curve)
    end
    local sdr_look = (sdr or photo) and mp.get_property_native("user-data/dlogm/sdr_look") or look
    if photo then
        local image = look == "maker" and "maker" or (look == "hdr" and platform.macos) and "macos" or "nbcu"
        table.insert(args, "--sdr-image"); table.insert(args, image)
    else
        if sdr_look == "maker" then table.insert(args, "--sdr") end
        if sdr_look == "nbcu" then table.insert(args, "--nbcu") end
    end
    -- The dv look: the RPU file the player attaches, on every frame of the export too, with each frame's own L1 when
    -- the player measures it (--macos-dovi-live-l1: the export measures its frames the same way)
    local rpu = mp.get_property("macos-dovi-rpu-file") or ""
    if not sdr and not photo and (look == "dv" or look == "dvsdr") and curve and curve ~= "" and rpu ~= "" then
        table.insert(args, "--dv-rpu"); table.insert(args, rpu)
        if mp.get_property_native("macos-dovi-live-l1") == true then table.insert(args, "--dv-live-l1") end
    end
    local function gyroflow(k) return mp.get_property_native("user-data/gyroflow/" .. k) end
    local view = gyroflow("view")
    if view == "off" then table.insert(args, "--no-stabilize") end
    if view == "wide" then table.insert(args, "--wide") end
    if gyroflow("level") then table.insert(args, "--level"); table.insert(args, gyroflow("level")) end
    if gyroflow("edges") then
        table.insert(args, "--edges"); table.insert(args, gyroflow("edges"))
        if gyroflow("edges_blur") then table.insert(args, "--edges-blur"); table.insert(args, string.format("%g", gyroflow("edges_blur"))) end
    end
    local horizon = mp.get_property_native("user-data/gyroflow/horizon_lock")
    if horizon == "yes" or horizon == "no" then
        table.insert(args, "--horizon-lock"); table.insert(args, horizon == "yes" and "100" or "0")
    end
    if gyroflow("underwater") then table.insert(args, "--underwater"); table.insert(args, gyroflow("underwater")) end
end

local function installed(what)
    local info = utils.file_info(EXPORT)
    if not info or not info.is_file then
        mp.osd_message(what .. ": gyroflow-export isn't installed", 2)
        return false
    end
    return true
end

-- `sdr`: in SDR whatever plays
local function start(path, sdr)
    if not installed("Export") then return end
    local progress = temp_name("progress")
    local control = platform.windows and temp_name("control") or nil
    local args = { EXPORT, "--force", "--progress", progress }
    if control then table.insert(args, "--control"); table.insert(args, control) end
    if playing then table.insert(args, "--background") end
    as_it_plays(args, sdr, false)
    local a, b = mp.get_property_number("ab-loop-a"), mp.get_property_number("ab-loop-b")
    local range = ""
    if a and b and b > a then
        table.insert(args, "--from"); table.insert(args, string.format("%.3f", a))
        table.insert(args, "--to");   table.insert(args, string.format("%.3f", b))
        range = " " .. clock(a) .. "-" .. clock(b)
    end
    table.insert(args, path)

    local name = platform.file_name(path)
    local job = { name = name, range = range, progress = progress, control = control }
    running[path] = job
    table.insert(order, path)
    mp.osd_message("Exporting " .. name .. range .. (playing and " (slowly while the video plays)" or ""), 3)
    mp.msg.info("export: " .. table.concat(args, " "))
    job.id = mp.command_native_async({ name = "subprocess", args = args, playback_only = false, capture_stderr = true }, function(_, r)
        finish(path)
        local err = r and r.stderr or ""
        -- gyroflow-export's last line for a video it exported: "  -> <output>"
        local out = ("\n" .. err):match("\n  %-> ([^\r\n]+)")
        if job.cancelling then
            mp.osd_message("Export cancelled: " .. name .. range, 4)
            notify("Export cancelled", name .. range)
        elseif r and r.status == 0 and out then
            last_export = out
            publish()
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
    publish()
end

mp.add_key_binding("E", "export", function()
    local path = local_path()
    if not path then return end
    local job = running[path]
    if not job then return start(path, false) end
    if job.cancelling then return end
    if confirm and confirm.path == path and mp.get_time() < confirm.time + CONFIRM_SECONDS then
        confirm = nil
        cancel(path)
    else
        confirm = { path = path, time = mp.get_time() }
        mp.osd_message("Exporting " .. job.name .. job.range .. ": Shift+E again to cancel", CONFIRM_SECONDS)
    end
end)

mp.register_script_message("export", function(how)
    local path = local_path()
    if not path then return end
    if running[path] then
        mp.osd_message("Exporting " .. running[path].name .. running[path].range .. " already", 2)
        return
    end
    start(path, how == "sdr")
end)

mp.register_script_message("cancel-export", function()
    local path = mp.get_property("path")
    local job = path and running[path]
    if job and not job.cancelling then cancel(path) end
end)

mp.register_script_message("reveal-last-export", function()
    if not last_export then return end
    local args
    if platform.macos then
        args = { "open", "-R", last_export }
    elseif platform.windows then
        args = { "explorer", "/select," .. last_export }
    else
        args = { "xdg-open", last_export:match("^(.*)/") or "." }
    end
    mp.command_native_async({ name = "subprocess", args = args, playback_only = false }, function() end)
end)

-- The frame on screen as a photo. Crop mode's camera and frame (crop.lua's user-data/gyroflow/crop): the frame in
-- shares of the view the camera renders; where mpv's own zoom and pan move the picture (not stabilized), the frame in
-- shares of the video
local photos = 0
local function photo()
    local path = local_path()
    if not path or not installed("Photo") then return end
    local t = mp.get_property_number("time-pos")
    if not t then return end
    local args = { EXPORT, "--photo", string.format("%.4f", t) }
    as_it_plays(args, false, true)
    local still = mp.get_property_native("user-data/gyroflow/still")
    if still == false then table.insert(args, "--radius"); table.insert(args, "0") end
    local crop = mp.get_property_native("user-data/gyroflow/crop") or {}
    local f = crop.active and crop.frame
    if f and crop.camera then
        local c = crop.camera
        table.insert(args, "--camera"); table.insert(args, string.format("%.4f,%.4f,%.4f,%.5f", c.yaw, c.pitch, c.roll, c.zoom))
        table.insert(args, "--crop"); table.insert(args, string.format("%.5f,%.5f,%.5f,%.5f", f.x, f.y, f.w, f.h))
    elseif f and crop.flat then
        -- On screen a point u (in shares of the video as it'd be unzoomed) shows the video's (u - 1/2) / s - pan + 1/2
        local s = 2 ^ crop.flat.zoom
        local function clamp(v) return math.max(0, math.min(1, v)) end
        local x0, y0 = clamp((f.x - 0.5) / s - crop.flat.x + 0.5), clamp((f.y - 0.5) / s - crop.flat.y + 0.5)
        local x1, y1 = clamp((f.x + f.w - 0.5) / s - crop.flat.x + 0.5), clamp((f.y + f.h - 0.5) / s - crop.flat.y + 0.5)
        if x1 > x0 and y1 > y0 then
            table.insert(args, "--crop"); table.insert(args, string.format("%.5f,%.5f,%.5f,%.5f", x0, y0, x1 - x0, y1 - y0))
        end
    end
    table.insert(args, path)
    photos = photos + 1
    local name = platform.file_name(path)
    mp.osd_message("Saving a photo of " .. name .. " at " .. clock(t) .. "…", 30)
    mp.msg.info("photo: " .. table.concat(args, " "))
    mp.command_native_async({ name = "subprocess", args = args, playback_only = false, capture_stderr = true }, function(_, r)
        photos = photos - 1
        local err = r and r.stderr or ""
        local out = ("\n" .. err):match("\n  %-> ([^\r\n]+)")
        if r and r.status == 0 and out then
            last_export = out
            publish()
            mp.osd_message("Photo saved: " .. platform.file_name(out), 4)
            notify("Photo saved", out)
        else
            local why = last_line(err) or "gyroflow-export failed"
            mp.msg.warn("photo: " .. err)
            mp.osd_message("Photo failed: " .. why, 6)
            notify("Photo failed: " .. name, why)
        end
    end)
end

mp.add_key_binding("Alt+s", "photo", photo)
mp.register_script_message("photo", photo)
