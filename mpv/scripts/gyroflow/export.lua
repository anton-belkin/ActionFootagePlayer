-- Shift+E: export the current video the way it plays, stabilized and with the D-Log M LUT (the dlogm plugin's),
-- with gyroflow-export (one pass, into a stabilized/ folder next to the video). With an A-B loop set (the `l`
-- key), only that part. Playback goes on while it runs; the result shows on screen and as a notification.
-- Loaded by main.lua.

local mp = require 'mp'
local utils = require 'mp.utils'

local EXPORT = utils.join_path(mp.get_script_directory(), "gyroflow-export")

local running = {}

local function notify(title, text)
    local script = string.format("display notification %q with title %q", text, title)
    mp.command_native_async({ name = "subprocess", args = { "osascript", "-e", script }, playback_only = false }, function() end)
end

-- The last line gyroflow-export printed for this video: "...: skipped, <why>" or "...: failed, <why>"
local function last_line(text)
    local last = nil
    for line in (text or ""):gsub("\r", "\n"):gmatch("[^\n]+") do
        if not line:match("^%s*%d+%%") then last = line end
    end
    return last
end

mp.add_key_binding("E", "export", function()
    local path = mp.get_property("path")
    if not path or path:find("://", 1, true) then
        mp.osd_message("Export: not a local file", 2)
        return
    end
    local info = utils.file_info(EXPORT)
    if not info or not info.is_file then
        mp.osd_message("Export: gyroflow-export isn't installed", 2)
        return
    end
    if running[path] then
        mp.osd_message("Export: already exporting this video", 2)
        return
    end

    local args = { EXPORT, "--force" }
    local a, b = mp.get_property_number("ab-loop-a"), mp.get_property_number("ab-loop-b")
    local range = ""
    if a and b and b > a then
        table.insert(args, "--from"); table.insert(args, string.format("%.3f", a))
        table.insert(args, "--to");   table.insert(args, string.format("%.3f", b))
        local function t(s) return string.format("%d:%02d", math.floor(s / 60), math.floor(s % 60)) end
        range = " " .. t(a) .. "-" .. t(b)
    end
    table.insert(args, path)

    local name = path:match("([^/]+)$")
    running[path] = true
    mp.osd_message("Exporting " .. name .. range .. "...", 3)
    mp.msg.info("export: " .. table.concat(args, " "))
    mp.command_native_async({ name = "subprocess", args = args, playback_only = false, capture_stderr = true }, function(_, r)
        running[path] = nil
        local err = r and r.stderr or ""
        local out = err:match("%-> (/[^\n]+)")
        if r and r.status == 0 and out then
            mp.osd_message("Exported " .. name .. range .. "\n" .. out, 5)
            notify("Exported " .. name, out)
        else
            local why = last_line(err) or "gyroflow-export failed"
            mp.msg.warn("export: " .. err)
            mp.osd_message("Export failed: " .. why, 6)
            notify("Export failed: " .. name, why)
        end
    end)
end)
