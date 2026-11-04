-- sdr-check: tells when macOS has an external SDR monitor set up as an HDR display, and suggests restarting the Mac.
--
-- On Apple Silicon, once an HDR display (an HDR TV, say) has been on one of the Mac's external display controllers,
-- the controller can keep that display's brightness settings after an SDR monitor takes its place, through
-- unplugging and plugging, until the Mac restarts. macOS then shows HDR video on that monitor (QuickTime, Safari, the
-- player's hdr and dv looks) about a stop darker in the mid-tones than on the same monitor after a restart. The helper
-- beside this (sdr-check, macos/sdr-check.swift) reads the controllers' settings; nothing is changed here.
--
-- The first time an HDR look plays on an SDR screen while such a monitor is connected, the player says so, once a
-- session. "Don't Ask Again" keeps it quiet: ask=no in ~~/script-opts/sdr_check.conf.

local mp = require 'mp'
local options = require 'mp.options'
local utils = require 'mp.utils'

local o = { ask = true }
options.read_options(o, "sdr_check")

if (mp.get_property("platform") or "") ~= "darwin" or not o.ask then return end
local helper = mp.get_script_directory() .. "/sdr-check"
if not utils.file_info(helper) then return end

local DIALOG = [[
on run {theMessage}
    activate
    return button returned of (display dialog theMessage buttons {"Don't Ask Again", "OK"} ¬
        default button "OK" with title "ActionFootagePlayer" with icon note)
end run]]

local asked, checking = false, false

local function save_quiet()
    local path = mp.command_native({ "expand-path", "~~/script-opts/sdr_check.conf" })
    local dir = path:match("^(.*)/")
    if dir and not utils.file_info(dir) then
        mp.command_native({ name = "subprocess", playback_only = false, args = { "mkdir", "-p", dir } })
    end
    local f = io.open(path, "w")
    if not f then return mp.msg.warn("can't write " .. path) end
    f:write("ask=no\n")
    f:close()
end

-- HDR shown by macOS (dlogm's hdr or dv look) on an SDR screen: are any of the SDR monitors set up as HDR ones?
local function check()
    if asked or checking then return end
    local d = mp.get_property_native("user-data/dlogm")
    if not d or d.screen ~= "sdr" or (d.look ~= "hdr" and d.look ~= "dv") then return end
    checking = true
    mp.command_native_async({ name = "subprocess", args = { helper }, playback_only = false, capture_stdout = true },
        function(_, r)
            checking = false
            local j = r and r.status == 0 and utils.parse_json(r.stdout or "")
            if not j or asked then return end
            local names = {}
            for _, s in ipairs(j.screens or {}) do
                if s.stale then table.insert(names, s.name) end
            end
            if #names == 0 then return end
            asked = true
            local paused = mp.get_property_native("pause")
            mp.set_property_native("pause", true)
            local text = "macOS has " .. table.concat(names, " and ") .. " set up as if it were the HDR display "
                .. "connected there before, and shows HDR on it about a stop darker than it should.\n\nRestarting the "
                .. "Mac sets it right. Until then, the SDR looks (Ctrl+s) aren't affected."
            mp.command_native_async({ name = "subprocess", playback_only = false, capture_stdout = true,
                                      args = { "osascript", "-e", DIALOG, text } }, function(_, res)
                mp.set_property_native("pause", paused)
                if res and (res.stdout or ""):find("Don't Ask Again", 1, true) then save_quiet() end
            end)
        end)
end

mp.observe_property("user-data/dlogm/look", "native", check)
mp.observe_property("user-data/dlogm/screen", "native", check)
mp.observe_property("display-names", "native", check)
