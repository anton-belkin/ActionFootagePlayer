-- Shift+E: export the current video the way it plays, stabilized and with the log curve's LUT (the dlogm plugin's),
-- with gyroflow-export (one pass). With the workspace plugin, it asks the name and the export goes to the project's
-- _Exports/ with its link inside (and the part, when it isn't a moment yet, is saved as one: workspace/main.lua);
-- without it, gyroflow-export's own default (the project's _Exports/). With an A-B loop set (the `l` key), only
-- that part. Playback goes on while it runs; the progress shows in a corner, the result on screen and
-- as a notification. Shift+E again on a video being exported, twice, cancels it.
--
-- As it plays: gyroflow-export takes the clip's settings as the player plays them (--settings: the gyroflow plugin's
-- user-data/gyroflow/settings, the JSON its warp is made from: docs/reference/contracts.md, "The settings") with the
-- dlogm plugin's log curve and look over them (Ctrl+l, Ctrl+s: HDR, SDR by the maker's LUT or by NBCU's conversion, HDR
-- + Dolby Vision with the RPU the look plays and each frame's L1 as the player measures it), Ctrl+j's sideways jerks
-- where they're computed, and the export's own settings (exportsettings.lua). Another clip (a moment's preview, the
-- Library's batch, a photo of a mark) the same way from its saved view (settings.lua of_view). A log clip with
-- nothing to stabilize it with gyroflow-export converts
-- in color only by itself. Script messages, for the right-click menu:
--   export [hdr|sdr|youtube|360|flat]  the same as Shift+E (hdr, or nothing), or in SDR whatever plays: the SDR look
--                        it plays in, else the one the dlogm plugin would choose (its sdr_look); for YouTube: HDR10 in
--                        an MKV with that SDR look's LUT inside for YouTube's SDR version (gyroflow-export --youtube,
--                        a .mkv; issue #95); for a 360 camera's file the whole sphere (360) or the view recorded as you
--                        looked around (flat: docs/design/360-reframe.md)
--   cancel-export        cancel this clip's export, without asking
--   reveal-last-export   show the last exported file in Finder (Explorer, the file manager)
--   preview <json>       a moment's preview for the workspace plugin (gyroflow-export --preview still), in SDR
--                        and stabilized as the clip plays: { id, clip, from, to, at, link, dates, output, force,
--                        reply }; the answer is `script-message-to <reply> preview-done <id> <json>`, { ok, error }
--   photo                the frame on screen as a photo link (Alt+s; Enter in crop mode): in a project, a small
--                        picture of the frame as it shows with its link (clip, time, crop, camera), in _Stills/Generated/,
--                        rendered by the Library's Share when it's shared; outside a project, as photo-full
--   photo-full           the frame on screen as a full photo, rendered now (Alt+Shift+s): gyroflow-export --photo, as it
--                        plays (the same stabilization, the frame cleaned up from its neighbours as Ctrl+p does, crop
--                        mode's camera and frame), as an SDR screen shows the look (as_it_plays) with a gain map to the
--                        HLG: a HEIC on macOS, an Ultra HDR JPEG elsewhere, in the project's _Stills/ (stabilized/
--                        without one). With 3 s of the video around it (--live; gyroflow.conf's live_photo), exported as Shift+E would, at most 1920
--                        wide or high: a Live Photo (macOS: a .mov next to the HEIC) or a Motion Photo (--format jpeg:
--                        <name>.MP.jpg, the video in it)
--   live-photo no|live|motion|next  photos alone, as Live Photos (macOS) or as Motion Photos, kept in gyroflow.conf
--                        at once; next: the next of them
--
-- The clip is the one playing, or the original when the file playing is a moment (user-data/workspace/clip), whose
-- time is the moment's plus its offset.
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
local es = require 'exportsettings'

local EXPORT = utils.join_path(mp.get_script_directory(), platform.exe("gyroflow-export"))
-- Shift+E twice within this many seconds cancels
local CONFIRM_SECONDS = 3
-- After SIGTERM, how long gyroflow-export gets to stop before mpv kills it
local KILL_AFTER = 5

-- path -> { name, range, id (the async command), progress (file), control (file, Windows), state (the progress
-- file's last contents), cancelling }
local running = {}
local order = {}
local confirm = nil
local playing = false
local timer
local last_export = nil

-- The clip playing: mpv's path, or the original when the file playing is a moment (the workspace plugin)
local function clip_path()
    local path = mp.get_property("path")
    local ws = mp.get_property_native("user-data/workspace")
    if path and type(ws) == "table" and ws.path == path and ws.redirected and ws.clip then return ws.clip end
    return path
end

-- For the right-click menu: whether the clip playing is being exported, and the last file exported
local function publish()
    mp.set_property_native("user-data/gyroflow/exporting", running[clip_path() or ""] ~= nil)
    mp.set_property_native("user-data/gyroflow/last_export", last_export or "")
    -- For the workspace plugin's clip previews, which run it themselves
    mp.set_property_native("user-data/gyroflow/exporter", EXPORT)
end
mp.observe_property("path", "string", publish)
mp.observe_property("user-data/workspace/clip", "string", publish)

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
-- Files made, told to the project's catalog (the workspace plugin's catalog-note: afp-catalog note), so the Library's
-- views show them at once: on a share the system's notice comes late or never. A photo's Live Photo video with it
local function tell_catalog(...)
    for _, f in ipairs({ ... }) do
        if type(f) == "string" and f ~= "" then
            local info = utils.file_info(f)
            if info and info.is_file then mp.commandv("script-message-to", "workspace", "catalog-note", f) end
            local stem = f:match("^(.*)%.[^./\\]+$")
            if stem and not f:lower():match("%.mov$") and not f:lower():match("%.mp4$") then
                for _, e in ipairs({ ".mov", ".MOV" }) do
                    local v = utils.file_info(stem .. e)
                    if v and v.is_file then mp.commandv("script-message-to", "workspace", "catalog-note", stem .. e); break end
                end
            end
        end
    end
end

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

-- Each export a card (the side panel's, side-panel/notify.lua): its share done, the speed and the time left
local function redraw()
    for _, path in ipairs(order) do
        local job = running[path]
        local st = job.state or {}
        local detail, share
        if job.cancelling then
            detail = "Cancelling"
        elseif not st.total then
            detail = "Starting"
        else
            share = st.done / math.max(1, st.total)
            -- A flat 360 export measures the hold on what's in the middle first, when it's asked for and not measured yet
            detail = (st.stage == "measuring" and "Measuring the hold on the middle · " or "")
                .. (st.background and "Slowed while the video plays: pause it for full speed"
                    or string.format("%.0f fps · %s left", st.fps or 0, clock(st.eta or 0)))
        end
        mp.notify({ id = "export:" .. path, kind = "working", title = "Exporting " .. job.name .. job.range, detail = detail, share = share })
    end
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
    -- A photo shown: nothing to export or take a frame of (quietly: the key does nothing)
    if platform.photo_shown() then return nil end
    local path = clip_path()
    if not path or path:find("://", 1, true) then
        mp.osd_message("Export: not a local file", 2)
        return nil
    end
    return path
end

local run_export

-- The export's settings (exportsettings.lua, docs/design/export-settings.md): a video's (size, encode, quality) and a photo's
-- (size, quality), kept in gyroflow.conf (export_*, photo_*), published by main.lua as user-data/gyroflow/export_settings
local function settings(kind)
    local all = mp.get_property_native("user-data/gyroflow/export_settings")
    return es.normalize(type(all) == "table" and all[kind] or nil, kind)
end

-- The settings JSON gyroflow-export takes (--settings: docs/reference/contracts.md, "The settings"), one object for every
-- route: the clip as it plays, or a saved view (settings.lua of_view). Nothing here passes a flag the settings carry
-- (tests/settings_json_test.py)
local S = require 'settings'

-- This computer's, for what a saved view doesn't say: the edge fill, how much temporal denoise, the grain it keeps; the
-- session's clean-up, else the app's defaults
local function here()
    local g = function(k) return mp.get_property_native("user-data/gyroflow/" .. k) end
    local live = type(g("settings")) == "table" and g("settings") or {}
    return { edges = g("edges"), edges_blur = g("edges_blur"), temporal_denoise = g("temporal_denoise") or "full",
             denoise_grain = live.denoise_grain, session = g("session") or {}, defaults = g("defaults") or {} }
end

-- The look and the colour as they play (the dlogm plugin's), over the clip's settings. `sdr`: in SDR whatever plays;
-- `photo`: the photo's image is what an SDR screen shows of the look (sdr_image), its gain map leads to the HLG, and its
-- motion (`live`) is the video as Shift+E exports it (sdr, dv_rpu):
--   maker                     the maker's LUT
--   hdr                       the HLG as macOS tones it down for an SDR screen (elsewhere NBCU's)
--   nbcu, tonemap, dv, dvsdr  NBCU's conversion: Dolby Vision's own SDR, as macOS draws it, only exists on screen
--                             (AVFoundation's decoders ignore the RPU), and the RPU's trims are fitted to NBCU's; mpv's
--                             tone mapping isn't in gyroflow-export
local function as_it_looks(st, how)
    local look, curve = mp.get_property_native("user-data/dlogm/look"), mp.get_property_native("user-data/dlogm/curve")
    -- With the dlogm plugin's log curve, Ctrl+l's included, or none when log is off (L) or the clip isn't log; without
    -- the plugin gyroflow-export reads the curve from the metadata as it does
    if look == "off" or curve == "" then st.curve = "none" elseif curve then st.curve = curve end
    -- The underwater colour (curve uw, the dlogm plugin's uwcolor.lua): its strength and highlights; for a photo (and
    -- its motion) the numbers its shader has now, the frame as it shows, else gyroflow-export measures them as it plays
    if curve == "uw" and look ~= "off" then
        S.uw_of(st, mp.get_property_native("user-data/uwcolor"))
        local o = how.photo and mp.get_property_native("glsl-shader-opts") or {}
        local n = { tonumber(o.uw_dr), tonumber(o.uw_db), tonumber(o.uw_k), tonumber(o.uw_ev) }
        if #n == 4 then st.uw_params = n end
    end
    local sdr_look = how.sdr and mp.get_property_native("user-data/dlogm/sdr_look") or look
    if how.photo then st.sdr_image = look == "maker" and "maker" or (look == "hdr" and platform.macos) and "macos" or "nbcu" end
    local video = not how.photo or how.live
    if video and (sdr_look == "maker" or sdr_look == "nbcu") then st.sdr = sdr_look end
    -- The dv look: the RPU file the player attaches, on every frame of the export too, with each frame's own L1 when
    -- the player measures it (--macos-dovi-live-l1: the export measures its frames the same way). A file's own Dolby
    -- Vision isn't carried: its RPU is for the frames as they were, not as they're made here
    local rpu = mp.get_property("macos-dovi-rpu-file") or ""
    local own = mp.get_property_native("user-data/dlogm/dv_own") == true
    if not how.sdr and video and (look == "dv" or look == "dvsdr") and not own and rpu ~= "" then
        st.dv_rpu = rpu
        if mp.get_property_native("macos-dovi-live-l1") == true then st.dv_live_l1 = true end
    end
end

-- The one function every route makes its settings with: `view` nil, the clip as it plays (the gyroflow plugin's own
-- settings, user-data/gyroflow/settings: what the warp plays with, the clean-up, Ctrl+j; with the look and colour as
-- they play); else a saved view (another clip's, a moment's, a Library recipe's: settings.lua of_view, this computer's
-- for what it doesn't say). how: { sdr (a saved view: NBCU's SDR), photo, live, kind (video, photo), export (the
-- export's settings: exportsettings apply), youtube (HDR, without the SDR and Dolby Vision choices) }
local function export_settings(view, how)
    how = how or {}
    local st = {}
    if view == nil then
        local live = mp.get_property_native("user-data/gyroflow/settings")
        for k, v in pairs(type(live) == "table" and live or { lens = "off" }) do st[k] = v end
        as_it_looks(st, how)
    else
        st = S.of_view(view, here())
        if how.sdr then st.sdr = "nbcu" end
    end
    if how.youtube then st.sdr, st.dv_rpu, st.dv_live_l1 = nil, nil, nil end
    if how.export then es.apply(st, how.export, how.kind or "video") end
    return st
end

-- The settings into gyroflow-export's arguments (as JSON: the log shows them, and the command runs again as it is)
local function with_settings(args, st)
    table.insert(args, "--settings")
    table.insert(args, utils.format_json(st))
    return args
end

-- For YouTube (gyroflow-export --youtube): the SDR look of the LUT inside: the one an SDR export takes (the dlogm
-- plugin's sdr_look; the maker's is DJI's, D-Log M only), else gyroflow-export's own choice
local function youtube_look()
    local d = mp.get_property_native("user-data/dlogm") or {}
    return (d.sdr_look == "maker" and d.curve == "dlogm") and "maker" or d.sdr_look == "nbcu" and "nbcu" or "auto"
end

local function installed(what)
    local info = utils.file_info(EXPORT)
    if not info or not info.is_file then
        mp.osd_message(what .. ": gyroflow-export isn't installed", 2)
        return false
    end
    return true
end

-- `how`: hdr (as it plays), sdr (in SDR whatever plays) or youtube. `named`: the workspace plugin's answer,
-- { output, link, from, to, alone }
local function start(path, how, named)
    if not installed("Export") then return end
    local progress = temp_name("progress")
    local control = platform.windows and temp_name("control") or nil
    local args = { EXPORT, "--force", "--progress", progress }
    if control then table.insert(args, "--control"); table.insert(args, control) end
    if playing then table.insert(args, "--background") end
    -- As it plays, with the export's settings: its size, encoder and quality preset
    with_settings(args, export_settings(nil, { sdr = how == "sdr", youtube = how == "youtube", export = settings("video"), kind = "video" }))
    if how == "youtube" then table.insert(args, "--youtube"); table.insert(args, youtube_look()) end
    local a, b = mp.get_property_number("ab-loop-a"), mp.get_property_number("ab-loop-b")
    -- The workspace plugin's part, in the clip's time (a moment playing is a video of its own, from 0)
    if named and named.from and named.to then a, b = named.from, named.to end
    local range = ""
    if a and b and b > a then
        table.insert(args, "--from"); table.insert(args, string.format("%.3f", a))
        table.insert(args, "--to");   table.insert(args, string.format("%.3f", b))
        range = " " .. clock(a) .. "-" .. clock(b)
    end
    if named and named.alone == true then table.insert(args, "--alone") elseif named and named.alone == false then table.insert(args, "--whole") end
    if named and named.output then
        table.insert(args, "--output"); table.insert(args, named.output)
        if named.link then table.insert(args, "--link"); table.insert(args, named.link) end
    end
    table.insert(args, path)
    return run_export(path, args, range, progress, control)
end

function run_export(path, args, range, progress, control)
    local name = platform.file_name(path)
    local job = { name = name, range = range, progress = progress, control = control }
    running[path] = job
    table.insert(order, path)
    mp.msg.info("export: " .. table.concat(args, " "))
    job.id = mp.command_native_async({ name = "subprocess", args = args, playback_only = false, capture_stderr = true }, function(_, r)
        finish(path)
        local err = r and r.stderr or ""
        -- gyroflow-export's last line for a video it exported: "  -> <output>"
        local out = ("\n" .. err):match("\n  %-> ([^\r\n]+)")
        local card = "export:" .. path
        if job.cancelling then
            mp.notify({ id = card, kind = "info", title = "Export cancelled", detail = name .. range, seconds = 4 })
            notify("Export cancelled", name .. range)
        elseif r and r.status == 0 and out then
            last_export = out
            tell_catalog(out)
            publish()
            mp.notify({ id = card, kind = "done", title = "Exported " .. name .. range, detail = platform.file_name(out), seconds = 5 })
            notify("Exported " .. name, out)
        else
            local why = last_line(err) or "gyroflow-export failed"
            mp.msg.warn("export: " .. err)
            mp.notify({ id = card, kind = "error", title = "Export failed: " .. name .. range, detail = why })
            notify("Export failed: " .. name, why)
        end
    end)
    if not timer then timer = mp.add_periodic_timer(1, poll) end
    redraw()
    publish()
end

-- With the workspace plugin: it asks the name, and answers export-start. The look and whether it's stabilized go
-- into the name and the link: " (SDR)", " (Dolby Vision)", " (YouTube)" (an MKV), nothing for HDR
local function begin(path, how)
    local ws = mp.get_property_native("user-data/workspace")
    if type(ws) ~= "table" or not ws.clip or ws.path ~= mp.get_property("path") then return start(path, how) end
    local st = export_settings(nil, { sdr = how == "sdr" })
    local label = st.dv_rpu and " (Dolby Vision)" or st.sdr and " (SDR)" or ""
    local look = st.dv_rpu and "dv" or st.sdr or st.curve == "none" and "recorded" or "hdr"
    if how == "youtube" then label, look = " (YouTube)", "youtube" end
    local g = mp.get_property_native("user-data/gyroflow") or {}
    mp.commandv("script-message-to", "workspace", "export-name", utils.format_json({
        sdr = how == "sdr", youtube = how == "youtube", ext = how == "youtube" and ".mkv" or nil, label = label, look = look,
        stabilized = g.available == true and g.view ~= "off", reply = mp.get_script_name(),
    }))
end

mp.register_script_message("export-start", function(json)
    local r = utils.parse_json(json or "")
    local path = local_path()
    -- Still the clip it was asked for
    if type(r) ~= "table" or not path or r.clip ~= mp.get_property_native("user-data/workspace/clip") or running[path] then return end
    start(path, r.youtube == true and "youtube" or r.sdr == true and "sdr" or "hdr", r)
end)

mp.add_key_binding("E", "export", function()
    local path = local_path()
    if not path then return end
    local job = running[path]
    if not job then return begin(path, "hdr") end
    if job.cancelling then return end
    if confirm and confirm.path == path and mp.get_time() < confirm.time + CONFIRM_SECONDS then
        confirm = nil
        cancel(path)
    else
        confirm = { path = path, time = mp.get_time() }
        mp.osd_message("Exporting " .. job.name .. job.range .. ": Shift+E again to cancel", CONFIRM_SECONDS)
    end
end)

-- A 360° clip (a 360 camera's own file, the view360 plugin's view): `360`, the whole sphere stabilized and turned
-- along what the view follows (gyroflow-export --360); `flat`, the view recorded as you looked around
-- (--reframe recording: the recording in the clip's .afp.json, its frame and size). The part marked (A-B), else for
-- flat the recorded parts' span and for 360 the whole clip. In the look it plays in: the log curve's HLG, or its SDR
-- (--sdr, --nbcu) when the look is SDR; colors as recorded when it isn't log
local FLAT_SIZES = {
    ["16:9"] = { ["1080p"] = "1920x1080", ["4K"] = "3840x2160" },
    ["9:16"] = { ["1080p"] = "1080x1920", ["4K"] = "2160x3840" },
    ["1:1"] = { ["1080p"] = "1080x1080", ["4K"] = "2160x2160" },
}
local function start_360(path, how)
    if not installed("Export") then return end
    local v = mp.get_property_native("user-data/view360")
    if type(v) ~= "table" or not v.available or not v.stamped then
        return mp.osd_message("Export " .. (how == "flat" and "flat" or "360°") .. ": for a 360 camera's own file", 2.5)
    end
    local r = type(v.recording) == "table" and v.recording or {}
    if how == "flat" and (r.parts or 0) == 0 then return mp.osd_message("Export flat: record a view first (Alt+r)", 2.5) end
    if r.taking then return mp.osd_message("Export: stop recording first (Alt+r)", 2) end
    local progress = temp_name("progress")
    local control = platform.windows and temp_name("control") or nil
    local args = { EXPORT, "--force", "--progress", progress }
    if control then table.insert(args, "--control"); table.insert(args, control) end
    if playing then table.insert(args, "--background") end
    -- The look as it plays (the curve, the underwater colour's, the SDR choice), the sphere as the player makes it (its
    -- width, its stitching distance: gyroflow.conf's) and what straight ahead follows; the rest is the 360 view's own
    local st, sphere = export_settings(nil, {}), {}
    for _, k in ipairs({ "curve", "uw_strength", "uw_highlights", "sdr", "sphere_width", "stitching_distance" }) do sphere[k] = st[k] end
    sphere.heading = how ~= "flat" and v.heading or nil
    with_settings(args, sphere)
    if how == "flat" then
        local size = (FLAT_SIZES[r.frame] or FLAT_SIZES["16:9"])[r.size] or "1920x1080"
        table.insert(args, "--reframe"); table.insert(args, "recording")
        table.insert(args, "--size"); table.insert(args, size)
    else
        table.insert(args, "--360")
    end
    local a, b = mp.get_property_number("ab-loop-a"), mp.get_property_number("ab-loop-b")
    local range = how == "flat" and " flat" or " 360°"
    if a and b and b > a then
        table.insert(args, "--from"); table.insert(args, string.format("%.3f", a))
        table.insert(args, "--to");   table.insert(args, string.format("%.3f", b))
        range = range .. " " .. clock(a) .. "-" .. clock(b)
    elseif how == "flat" and type(r.spans) == "table" and #r.spans > 0 then
        range = range .. " " .. clock(r.spans[1][1]) .. "-" .. clock(r.spans[#r.spans][2])
    end
    table.insert(args, path)
    return run_export(path, args, range, progress, control)
end

mp.register_script_message("export", function(how)
    local path = local_path()
    if not path then return end
    if running[path] then
        mp.osd_message("Exporting " .. running[path].name .. running[path].range .. " already", 2)
        return
    end
    if how == "360" or how == "flat" then return start_360(path, how) end
    begin(path, (how == "sdr" or how == "youtube") and how or "hdr")
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

-- A moment's picture, the frame on screen as k is pressed (the workspace plugin's moment-picture { a, b }: the part,
-- in the clip's time): mpv's screenshot of the video as it's rendered (the stabilization, the look; no OSD, panel or
-- console) in SDR (screenshot-tag-colorspace no: an HDR look tone-mapped to sRGB, as Alt+s's photo links have it),
-- for the preview that follows (preview's `picture`). Taken only where it's the moment as it will play: a window that
-- renders (not --vo=null, not the Library's batches), the frame on screen inside the part, Save frame's crop mode off
-- (a moment keeps neither its camera nor its frame; paused, its screenshot can be stale: the denoise's ring). Else
-- none, and the preview is rendered by gyroflow-export as before
local moment_picture = nil

-- A moment's preview, for the workspace plugin: one frame in SDR, stabilized as the clip plays, held for its part.
-- Short (a second or two), so it runs at once, beside an export if there's one. With `picture` (k's): the frame taken
-- as k was pressed (moment-picture) made into it, no clip decoded (a second, where rendering one takes up to 30 s on
-- the NAS); without one taken, rendered
mp.register_script_message("preview", function(json)
    local req = utils.parse_json(json or "")
    if type(req) ~= "table" or not req.clip or not req.output or not req.reply then return end
    local picture = req.picture == true and moment_picture or nil
    moment_picture = nil
    local function reply(ok, err)
        if picture then os.remove(picture) end
        mp.commandv("script-message-to", req.reply, req.done or "preview-done", tostring(req.id or ""), utils.format_json({ ok = ok, error = err }))
    end
    local info = utils.file_info(EXPORT)
    if not info or not info.is_file then return reply(false, "gyroflow-export isn't installed") end
    local args = { EXPORT, "--preview", "still" }
    if req.force then table.insert(args, "--force") end
    if picture then
        table.insert(args, "--picture"); table.insert(args, picture)
    else
        -- Another clip's (req.view: its saved view), or as this one plays. An error here is answered, not left in silence
        -- A moment's own stabilization: its project, or none (false: the preset with the settings as it plays)
        local ok, err = xpcall(function()
            local st = type(req.view) == "table" and export_settings(req.view) or export_settings(nil, { sdr = true })
            if req.project ~= nil then st.project = type(req.project) == "string" and req.project or nil end
            with_settings(args, st)
        end, debug.traceback)
        if not ok then
            mp.msg.error("preview: " .. tostring(err))
            return reply(false, tostring(err):match("^[^\n]*"))
        end
    end
    -- Its part on its own (with a picture: it's in the picture already)
    if picture then
    elseif req.alone == true then table.insert(args, "--alone") elseif req.alone == false and type(req.project) == "string" then table.insert(args, "--whole") end
    local function add(k, v) if v ~= nil then table.insert(args, k); table.insert(args, tostring(v)) end end
    add("--from", req.from and string.format("%.3f", req.from))
    add("--to", req.to and string.format("%.3f", req.to))
    if not picture then add("--at", req.at and string.format("%.3f", req.at)) end
    add("--link", req.link)
    add("--dates", req.dates)
    add("--output", req.output)
    table.insert(args, req.clip)
    mp.msg.info("preview: " .. table.concat(args, " "))
    mp.command_native_async({ name = "subprocess", args = args, playback_only = false, capture_stderr = true }, function(_, r)
        local err = r and r.stderr or ""
        if r and r.status == 0 and ("\n" .. err):match("\n  %-> ") then
            -- The moment and its own Gyroflow project (a batch's moments: k's notes them itself, again is harmless)
            tell_catalog(req.output, type(req.project) == "string" and req.project or nil)
            return reply(true)
        end
        mp.msg.warn("preview: " .. err)
        reply(false, last_line(err) or "gyroflow-export failed")
    end)
end)

-- An export for the workspace plugin's batch mode (Action Footage Library's Export all moments): a clip's part with
-- a saved view, nothing playing. { id, clip, view, project (a moment's own Gyroflow project), alone, from, to, output,
-- link, sdr, progress (gyroflow-export's progress file), reply, done }. The answer: <done> <id> { ok, error, out }.
-- export-batch-stop <id>: cancelled, as Shift+E twice cancels (the unfinished file removed)
local batch_jobs = {}
mp.register_script_message("export-batch", function(json)
    local req = utils.parse_json(json or "")
    if type(req) ~= "table" or not req.clip or not req.output or not req.reply then return end
    local id = tostring(req.id or "")
    local function reply(ok, err, out)
        mp.commandv("script-message-to", req.reply, req.done or "export-batch-done", id, utils.format_json({ ok = ok, error = err, out = out }))
    end
    local info = utils.file_info(EXPORT)
    if not info or not info.is_file then return reply(false, "gyroflow-export isn't installed") end
    local control = platform.windows and temp_name("control") or nil
    local args = { EXPORT, "--force" }
    if req.progress then table.insert(args, "--progress"); table.insert(args, req.progress) end
    if control then table.insert(args, "--control"); table.insert(args, control) end
    -- The Library's export settings (Share's dialog), else the defaults
    local st = export_settings(type(req.view) == "table" and req.view or {}, { sdr = req.sdr == true, export = es.normalize(req.export, "video"), kind = "video" })
    if type(req.project) == "string" then st.project = req.project end
    with_settings(args, st)
    if req.alone == true then table.insert(args, "--alone") elseif req.alone == false and type(req.project) == "string" then table.insert(args, "--whole") end
    local function add(k, v) if v ~= nil then table.insert(args, k); table.insert(args, tostring(v)) end end
    add("--from", req.from and string.format("%.3f", req.from))
    add("--to", req.to and string.format("%.3f", req.to))
    add("--output", req.output)
    add("--link", req.link)
    table.insert(args, req.clip)
    mp.msg.info("export (batch): " .. table.concat(args, " "))
    local job = { control = control, progress = req.progress, state = {} }
    batch_jobs[id] = job
    job.id = mp.command_native_async({ name = "subprocess", args = args, playback_only = false, capture_stderr = true }, function(_, r)
        batch_jobs[id] = nil
        if control then os.remove(control) end
        local err = r and r.stderr or ""
        local out = ("\n" .. err):match("\n  %-> ([^\r\n]+)")
        if job.cancelling then return reply(false, "cancelled") end
        if r and r.status == 0 and out then tell_catalog(out); return reply(true, nil, out) end
        mp.msg.warn("export (batch): " .. err)
        reply(false, last_line(err) or "gyroflow-export failed")
    end)
end)
mp.register_script_message("export-batch-stop", function(id)
    local job = batch_jobs[tostring(id or "")]
    if not job or job.cancelling then return end
    job.cancelling = true
    local f = job.progress and io.open(job.progress, "r")
    if f then
        local st = utils.parse_json(f:read("*a") or "")
        f:close()
        if type(st) == "table" then job.state = st end
    end
    if not signal(job, "TERM") then return mp.abort_async_command(job.id) end
    mp.add_timeout(KILL_AFTER, function()
        if batch_jobs[tostring(id)] ~= job then return end
        mp.abort_async_command(job.id)
        local partial = job.state.partial
        if partial then mp.add_timeout(2, function() os.remove(partial) end) end
    end)
end)

-- The frame on screen as a photo. Crop mode's camera and frame (crop.lua's user-data/gyroflow/crop): the frame in
-- shares of the view the camera renders; where mpv's own zoom and pan move the picture (not stabilized), the frame in
-- shares of the video
local photos = 0

-- The frame as the GPU path renders it (the gyroflow plugin's warp as a shader: crop mode's camera in it, the look
-- in HLG), at the video's size, for gyroflow-export --image: mpv's screenshot as a 16-bit PNG in the video's own
-- colors (BT.2020 HLG; a raw screenshot would be converted to sRGB), uncompressed (quick to write and read). Only where
-- the frame on screen is the HLG (the hdr, tonemap and dv looks; maker and nbcu end in SDR) and the clean frame
-- (Ctrl+p) is off: cleaning up takes the frames around it at full size, gyroflow-export's own. Returns the arguments
-- for it and the file, or nil
local HLG_LOOKS = { hdr = true, tonemap = true, dv = true, dvsdr = true }
local function gpu_frame()
    local gpu = mp.get_property_native("user-data/gyroflow/gpu")
    if type(gpu) ~= "table" or mp.get_property_native("user-data/gyroflow/still") == true then return nil end
    -- Not with Save frame's camera turned or zoomed: paused, mpv's screenshot can come from the denoise's ring of
    -- frames warped before the camera moved (seen live: yaw, pitch and zoom missing). gyroflow-export renders that
    -- camera as the warp does (gpu_table's tests)
    local crop = mp.get_property_native("user-data/gyroflow/crop") or {}
    if crop.active and crop.camera then return nil end
    -- Nor with the color fringes skipped on this screen (Performance & Quality) where the clip has them taken out: the
    -- photo has them out, gyroflow-export's
    if mp.get_property_native("user-data/gyroflow/chromatic_skip") == true and mp.get_property_native("user-data/gyroflow/chromatic") == true then return nil end
    local look, curve = mp.get_property_native("user-data/dlogm/look"), mp.get_property_native("user-data/dlogm/curve")
    local hlg = (mp.get_property("video-params/gamma") or "") == "hlg"
    if not (HLG_LOOKS[look] and (curve and curve ~= "" or hlg)) then return nil end
    local file = utils.join_path(platform.temp_dir(), string.format("afp-photo-%d-%d.png", utils.getpid(), photos))
    local saved = {}
    for k, v in pairs({ ["screenshot-format"] = "png", ["screenshot-tag-colorspace"] = "yes", ["screenshot-high-bit-depth"] = "yes",
                        ["screenshot-png-compression"] = "0" }) do
        saved[k] = mp.get_property(k)
        mp.set_property(k, v)
    end
    -- At the video's own size whatever the resolution chosen for playback (main.lua's gyroflow_render_scale: the
    -- screenshot renders at the video's size, the scale is of that)
    local opts = mp.get_property_native("glsl-shader-opts") or {}
    local scale = opts.gyroflow_render_scale
    if scale then opts.gyroflow_render_scale = "1"; mp.set_property_native("glsl-shader-opts", opts) end
    local t0 = mp.get_time()
    mp.command_native({ "screenshot-to-file", file, "video" })
    if scale then opts.gyroflow_render_scale = scale; mp.set_property_native("glsl-shader-opts", opts) end
    for k, v in pairs(saved) do mp.set_property(k, v) end
    if not utils.file_info(file) then return nil end
    mp.msg.info(string.format("photo: the frame as rendered, in %d ms", math.floor((mp.get_time() - t0) * 1000)))
    return { "--image", file }, file
end

-- The time of the frame on screen (docs/design/seek-and-scrub.md): with the GPU path's temporal denoise the picture is up to
-- its delay behind time-pos (the newest frame decoded), or the frame asked for while paused (gyroflow's
-- user-data/gyroflow/gpu_delay: { delay, restart, show }); otherwise time-pos. As side-panel timeline.lua's shown_time
local function shown_time()
    local t = mp.get_property_number("time-pos")
    local d = mp.get_property_native("user-data/gyroflow/gpu_delay")
    if not t or type(d) ~= "table" or not d.delay or d.delay <= 0 then return t end
    if type(d.show) == "number" and d.show >= 0 then return d.show end
    local restart = (type(d.restart) == "number" and d.restart >= 0) and math.min(d.restart, t) or t
    return math.max(restart, t - d.delay)
end

-- k's moment picture (moment_picture, above)
mp.register_script_message("moment-picture", function(json)
    if moment_picture then os.remove(moment_picture); moment_picture = nil end
    local req = utils.parse_json(json or "")
    if type(req) ~= "table" or type(req.a) ~= "number" or type(req.b) ~= "number" then return end
    local vo = mp.get_property("current-vo")
    if not vo or vo == "null" or vo == "image" or (os.getenv("AFP_BATCH") or "") ~= "" then return end
    if not mp.get_property_native("vo-configured") then return end
    local crop = mp.get_property_native("user-data/gyroflow/crop") or {}
    if crop.active then return end
    local t = shown_time()
    -- A moment playing as a video of its own: its time plus where it starts in the clip
    local ws = mp.get_property_native("user-data/workspace")
    if t and type(ws) == "table" and ws.path == mp.get_property("path") and type(ws.offset) == "number" then t = t + ws.offset end
    -- A frame's slack at the ends (B is where the loop turns back)
    if not t or t < req.a - 0.05 or t > req.b + 0.05 then
        return mp.msg.info("moment picture: the frame on screen isn't inside the part: the preview is rendered")
    end
    local file = utils.join_path(platform.temp_dir(), string.format("afp-moment-%d.jpg", utils.getpid()))
    local saved = {}
    for k, v in pairs({ ["screenshot-format"] = "jpg", ["screenshot-jpeg-quality"] = "95", ["screenshot-tag-colorspace"] = "no", ["screenshot-high-bit-depth"] = "no" }) do
        saved[k] = mp.get_property(k)
        mp.set_property(k, v)
    end
    local t0 = mp.get_time()
    os.remove(file)
    mp.command_native({ "screenshot-to-file", file, "video" })
    for k, v in pairs(saved) do mp.set_property(k, v) end
    if utils.file_info(file) then
        moment_picture = file
        mp.msg.info(string.format("moment picture: the frame at %.3f s, taken in %d ms", t, math.floor((mp.get_time() - t0) * 1000)))
    end
end)

-- Where a photo goes: the project's _Stills/Generated/ (docs/design/library-management.md §8), as the workspace plugin says
-- the project is; with the project's generated_runs, a folder a day in it (<date>); nil: gyroflow-export's own choice
-- (stabilized/ beside a clip of no project)
local function photo_dir()
    local ws = mp.get_property_native("user-data/workspace")
    local project = type(ws) == "table" and ws.project
    if type(project) ~= "string" or project == "" then return nil end
    local sep = platform.windows and "\\" or "/"
    local dir = project .. sep .. "_Stills" .. sep .. "Generated"
    local f = io.open(project .. sep .. "Project.afproject", "r")
    local doc = f and utils.parse_json(f:read("*a") or "")
    if f then f:close() end
    if type(doc) == "table" and doc.generated_runs == true then dir = dir .. sep .. os.date("%Y-%m-%d") end
    return dir
end

-- Save frame's (crop mode's) camera and frame, as a photo takes them: the camera (360: yaw, pitch, roll, zoom) and
-- the frame in shares of the camera's view; flat, the frame in shares of the video as it's recorded. nil, nil: the
-- whole frame
local function save_frame()
    local crop = mp.get_property_native("user-data/gyroflow/crop") or {}
    local f = crop.active and crop.frame
    if f and crop.camera then
        return crop.camera, { x = f.x, y = f.y, w = f.w, h = f.h }
    elseif f and crop.flat then
        -- On screen a point u (in shares of the video as it'd be unzoomed) shows the video's (u - 1/2) / s - pan + 1/2
        local s = 2 ^ crop.flat.zoom
        local function clamp(v) return math.max(0, math.min(1, v)) end
        local x0, y0 = clamp((f.x - 0.5) / s - crop.flat.x + 0.5), clamp((f.y - 0.5) / s - crop.flat.y + 0.5)
        local x1, y1 = clamp((f.x + f.w - 0.5) / s - crop.flat.x + 0.5), clamp((f.y + f.h - 0.5) / s - crop.flat.y + 0.5)
        if x1 > x0 and y1 > y0 then return nil, { x = x0, y = y0, w = x1 - x0, h = y1 - y0 } end
    end
    return nil, nil
end

-- The full photo, rendered now (Alt+Shift+s; Alt+s where there's no project to keep a photo link in)
local function photo_full()
    local path = local_path()
    if not path or not installed("Photo") then return end
    local t = shown_time()
    if not t then return end
    -- A moment plays as a video of its own: its time plus where it starts in the clip
    local ws = mp.get_property_native("user-data/workspace")
    if type(ws) == "table" and ws.path == mp.get_property("path") and type(ws.offset) == "number" then t = t + ws.offset end
    local args = { EXPORT, "--photo", string.format("%.4f", t) }
    local mode = mp.get_property_native("user-data/gyroflow/live_photo") or "no"
    local live = mode == "live" or mode == "motion"
    if live then table.insert(args, "--live") end
    -- A Motion Photo is a JPEG (the default off macOS)
    if mode == "motion" then table.insert(args, "--format"); table.insert(args, "jpeg") end
    -- The photo's settings: its size and quality preset (its motion's too)
    local s = settings("photo")
    with_settings(args, export_settings(nil, { photo = true, live = live, export = s, kind = "photo" }))
    -- A moment stabilized on its own (by its part's motion alone, the gyroflow plugin's fixed()), as it plays: its
    -- part of the clip; one with a project of its own, the project's "only within the trim range" as it's viewed
    local moment = type(ws) == "table" and ws.path == mp.get_property("path")
    if moment and type(ws.trim) == "table" and ws.trim["in"] and ws.trim.out then
        for _, a in ipairs({ "--alone", "--from", string.format("%.4f", ws.trim["in"]), "--to", string.format("%.4f", ws.trim.out) }) do table.insert(args, a) end
    elseif moment and ws.alone ~= nil and (mp.get_property_native("user-data/gyroflow/project") or "") ~= "" then
        table.insert(args, ws.alone and "--alone" or "--whole")
    end
    local c, f = save_frame()
    if c then table.insert(args, "--camera"); table.insert(args, string.format("%.4f,%.4f,%.4f,%.5f", c.yaw, c.pitch, c.roll, c.zoom)) end
    if f then table.insert(args, "--crop"); table.insert(args, string.format("%.5f,%.5f,%.5f,%.5f", f.x, f.y, f.w, f.h)) end
    local dir = photo_dir()
    if dir then table.insert(args, "--out"); table.insert(args, dir) end
    -- On the GPU path, the frame as it's rendered, taken now (quick: no decoding and stabilizing it again); at the
    -- highest quality gyroflow-export renders it itself, at its best
    local frame_args, frame_file
    if s.quality ~= "highest" then frame_args, frame_file = gpu_frame() end
    for _, a in ipairs(frame_args or {}) do table.insert(args, a) end
    table.insert(args, path)
    photos = photos + 1
    local name = platform.file_name(path)
    local kind = mode == "live" and "Live Photo" or mode == "motion" and "Motion Photo" or "photo"
    local card = "photo:" .. photos .. ":" .. tostring(mp.get_time())
    mp.notify({ id = card, kind = "working", title = "Saving a " .. kind, detail = name .. " at " .. clock(t) })
    mp.msg.info("photo: " .. table.concat(args, " "))
    mp.command_native_async({ name = "subprocess", args = args, playback_only = false, capture_stderr = true }, function(_, r)
        photos = photos - 1
        if frame_file then os.remove(frame_file) end
        local err = r and r.stderr or ""
        local out = ("\n" .. err):match("\n  %-> ([^\r\n]+)")
        if r and r.status == 0 and out then
            last_export = out
            tell_catalog(out)
            publish()
            local saved = live and err:find("\n  Live Photo:") and "Live Photo saved" or live and err:find("\n  Motion Photo:") and "Motion Photo saved" or "Photo saved"
            mp.notify({ id = card, kind = "done", title = saved, detail = platform.file_name(out), seconds = 4 })
            notify(saved, out)
        else
            local why = last_line(err) or "gyroflow-export failed"
            mp.msg.warn("photo: " .. err)
            mp.notify({ id = card, kind = "error", title = "Photo failed: " .. name, detail = why })
            notify("Photo failed: " .. name, why)
        end
    end)
end

-- A photo link (Alt+s; Enter in crop mode; docs/design/albums-photos-share-concept.md, "Photos as objects"): the frame as it
-- shows now, Save frame's crop and camera, made into a small picture with its link (clip, time, crop, camera) by the
-- workspace plugin, in the project's _Stills/Generated/. Nothing is rendered: Action Footage Library's Share renders
-- it when it's shared, in the form the destination wants. Playback doesn't stop. Outside a project (nothing would
-- keep or share the link): the full photo, as Alt+Shift+s
local function photo()
    local wsd = mp.get_property_native("user-data/workspace")
    local project = type(wsd) == "table" and wsd.project
    local path = local_path()
    if type(project) ~= "string" or project == "" or not path then return photo_full() end
    local t = shown_time()
    if not t then return end
    if wsd.path == mp.get_property("path") and type(wsd.offset) == "number" then t = t + wsd.offset end
    local camera, frame = save_frame()
    -- The picture: the frame as mpv renders it (the look, the stabilization on the GPU path), a JPEG at the video's
    -- size; the workspace plugin crops and shrinks it
    photos = photos + 1
    local picture = utils.join_path(platform.temp_dir(), string.format("afp-photo-link-%d-%d.jpg", utils.getpid(), photos))
    local saved = {}
    for k, v in pairs({ ["screenshot-format"] = "jpg", ["screenshot-jpeg-quality"] = "92", ["screenshot-tag-colorspace"] = "no", ["screenshot-high-bit-depth"] = "no" }) do
        saved[k] = mp.get_property(k)
        mp.set_property(k, v)
    end
    mp.command_native({ "screenshot-to-file", picture, "video" })
    for k, v in pairs(saved) do mp.set_property(k, v) end
    if not utils.file_info(picture) then picture = nil end
    mp.commandv("script-message-to", "workspace", "photo-link", utils.format_json({
        t = math.floor(t * 1000 + 0.5) / 1000, crop = frame, camera = camera, picture = picture }))
end

mp.add_key_binding("Alt+s", "photo", photo)
mp.register_script_message("photo", photo)
mp.add_key_binding("Alt+S", "photo-full", photo_full)
mp.register_script_message("photo-full", photo_full)

-- A photo of any clip at a time, with its saved view, for the workspace plugin (the Project tab's photos of marks):
-- { id, clip, t, view, reply, crop, camera, link, still }. As Save frame's choice has it (a still, Live or Motion Photo) and the clean still
-- frame on or off; into `out_dir` (the run's Generated/ folder), else the project's Generated/<date> Photos. The
-- answer: photo-done (or req.done) <id> { ok, error, out }
mp.register_script_message("photo-at", function(json)
    local req = utils.parse_json(json or "")
    if type(req) ~= "table" or not req.clip or not req.t or not req.reply then return end
    local function reply(ok, err, out)
        mp.commandv("script-message-to", req.reply, req.done or "photo-done", tostring(req.id or ""), utils.format_json({ ok = ok, error = err, out = out }))
    end
    local info = utils.file_info(EXPORT)
    if not info or not info.is_file then return reply(false, "gyroflow-export isn't installed") end
    local args = { EXPORT, "--photo", string.format("%.4f", req.t) }
    local mode = mp.get_property_native("user-data/gyroflow/live_photo") or "no"
    -- `still`: a still whatever Save frame's choice is (a render for the Library's Share)
    if req.still == true then mode = "no" end
    if mode == "live" or mode == "motion" then table.insert(args, "--live") end
    if mode == "motion" then table.insert(args, "--format"); table.insert(args, "jpeg") end
    local view = type(req.view) == "table" and req.view or {}
    -- Its settings: the Library's (Share's dialog), else this player's for photos
    local s = type(req.export) == "table" and es.normalize(req.export, "photo") or settings("photo")
    -- Cleaned up as that clip's view says (a recipe's denoise and its amount, #319), else as the session has it
    with_settings(args, export_settings(view, { export = s, kind = "photo" }))
    -- A photo link's own frame (its crop in shares of the frame, its camera), and what its link is to carry
    local c, f = req.camera, req.crop
    if type(c) == "table" and c.yaw and c.pitch and c.roll and c.zoom then
        table.insert(args, "--camera"); table.insert(args, string.format("%.4f,%.4f,%.4f,%.5f", c.yaw, c.pitch, c.roll, c.zoom))
    end
    if type(f) == "table" and f.x and f.y and f.w and f.h then
        table.insert(args, "--crop"); table.insert(args, string.format("%.5f,%.5f,%.5f,%.5f", f.x, f.y, f.w, f.h))
    end
    if type(req.link) == "string" then table.insert(args, "--link"); table.insert(args, req.link) end
    -- Where it goes: the place asked for (a run's folder), else the project's _Stills/Generated
    local dir = type(req.out_dir) == "string" and req.out_dir or photo_dir()
    if dir then table.insert(args, "--out"); table.insert(args, dir) end
    table.insert(args, req.clip)
    mp.msg.info("photo: " .. table.concat(args, " "))
    mp.command_native_async({ name = "subprocess", args = args, playback_only = false, capture_stderr = true }, function(_, r)
        local err = r and r.stderr or ""
        local out = ("\n" .. err):match("\n  %-> ([^\r\n]+)")
        if r and r.status == 0 and out then tell_catalog(out); return reply(true, nil, out) end
        mp.msg.warn("photo: " .. err)
        reply(false, last_line(err) or "gyroflow-export failed")
    end)
end)

-- export-setting <video|photo> <size|encode|quality> <value>: one of the export's settings (exportsettings.lua), kept
-- in gyroflow.conf at once (export_size, photo_quality, ...)
mp.register_script_message("export-setting", function(kind, key, value)
    if not es.KEYS[kind] then return end
    local all = mp.get_property_native("user-data/gyroflow/export_settings")
    all = type(all) == "table" and all or {}
    local now = es.normalize(all[kind], kind)
    if now[key] == nil then return end
    now[key] = value
    now = es.normalize(now, kind)
    if now[key] ~= value then return end
    all[kind] = now
    mp.set_property_native("user-data/gyroflow/export_settings", all)
    platform.save_options("gyroflow", { { (kind == "photo" and "photo_" or "export_") .. key, value } })
end)

-- Live Photos need afp-photo (macOS)
local MODES = platform.macos and { "no", "live", "motion" } or { "no", "motion" }
local MODE_NAMES = { no = "stills", live = "Live Photos, with 3 s of video for Apple's devices",
                     motion = "Motion Photos, with 3 s of video for Android's" }
mp.register_script_message("live-photo", function(how)
    local mode = mp.get_property_native("user-data/gyroflow/live_photo") or "no"
    if how == "next" then
        for i, m in ipairs(MODES) do if m == mode then how = MODES[i % #MODES + 1] break end end
    end
    if not MODE_NAMES[how] or (how == "live" and not platform.macos) then return end
    mp.set_property_native("user-data/gyroflow/live_photo", how)
    platform.save_options("gyroflow", { { "live_photo", how } })
    mp.osd_message("Photos: " .. MODE_NAMES[how], 2)
end)
