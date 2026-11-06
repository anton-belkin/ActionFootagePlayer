-- tools/latency360/drive.lua: one latency run in the app's mpv (run.sh starts it). A stitched clip is taken as a sphere
-- without the gyroflow plugin (its probe's user-data/gyroflow/spherical, set here before view360's hook reads it); then,
-- once the view is up, the clip paused or playing, the gestures made up at hz (--macos-latency-test) or left to the
-- fingers for the seconds given, and mpv quits.
--   --script-opts=latency360-hz=120,latency360-seconds=10,latency360-pause=yes
local mp = require 'mp'
local options = require 'mp.options'
local o = { hz = 0, seconds = 10, pause = false, settle = 2 }
options.read_options(o, "latency360")

mp.add_hook("on_load", 50, function()
    mp.set_property_native("user-data/gyroflow/spherical", { projection = "equirect" })
end)

local started = false
mp.register_event("file-loaded", function()
    if started then return end
    started = true
    mp.set_property_native("pause", o.pause)
    mp.add_timeout(o.settle, function()
        mp.msg.info(string.format("latency run: %s, %s for %g s", o.pause and "paused" or "playing",
            o.hz > 0 and (o.hz .. " Hz of made-up scrolls") or "the trackpad", o.seconds))
        if o.hz > 0 then
            mp.set_property_number("macos-latency-test", o.hz)
        else
            mp.osd_message("Two fingers on the trackpad: turn the view, on and off, for " .. o.seconds .. " s", o.seconds)
        end
        mp.add_timeout(o.seconds, function()
            mp.set_property_number("macos-latency-test", 0)
            mp.add_timeout(0.3, function() mp.command("quit") end)
        end)
    end)
end)
