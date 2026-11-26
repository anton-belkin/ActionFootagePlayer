-- tools/compare_latency/drive.lua: one run of the as-recorded switch's latency in the app's mpv (run.py starts it).
-- Once the clip plays through the warp (or after `settle` s for a clip that isn't stabilized), it switches as recorded
-- on and off `cycles` times while playing, then as many paused, `gap` s apart, each by the gyroflow plugin's
-- afp-compare with the host clock's time of the ask (LuaJIT's ffi: mach_absolute_time, as NSEvent.timestamp and the
-- latency log's times are), and quits. Each switch is logged as "switch <on|off> <playing|paused> <time>"; the
-- latency log (--macos-latency-log, patch 0010) says when the first frame drawn with it was shown. With `shots`, the
-- window last, paused, as it plays and as recorded (the card in it): <shots>-plays.png, <shots>-recorded.png.
--   --script-opts=compare_latency-cycles=10,compare_latency-gap=0.6,compare_latency-settle=4,compare_latency-shots=<path>
local mp = require 'mp'
local options = require 'mp.options'
local o = { cycles = 10, gap = 0.6, settle = 4, shots = "" }
options.read_options(o, "compare_latency")

local ffi = require 'ffi'
ffi.cdef [[
    uint64_t mach_absolute_time(void);
    typedef struct { uint32_t numer; uint32_t denom; } mach_timebase_info_data_t;
    int mach_timebase_info(mach_timebase_info_data_t *info);
]]
local tb = ffi.new("mach_timebase_info_data_t")
ffi.C.mach_timebase_info(tb)
local function host_now() return tonumber(ffi.C.mach_absolute_time()) * tb.numer / tb.denom / 1e9 end

local started = false
local function run()
    if started then return end
    started = true
    local gpu = mp.get_property_native("user-data/gyroflow/gpu") or {}
    mp.msg.info(string.format("run: %s, %s, warp %s, video %s", mp.get_property("filename"), mp.get_property("video-params/pixelformat") or "?",
        gpu.mode or "none", mp.get_property("video-params/w") .. "x" .. mp.get_property("video-params/h")))
    local steps = {}
    for _, paused in ipairs({ false, true }) do
        steps[#steps + 1] = function() mp.set_property_native("pause", paused) end
        for _ = 1, o.cycles do
            for _, what in ipairs({ "on", "off" }) do
                steps[#steps + 1] = function()
                    local t = host_now()
                    mp.msg.info(string.format("switch %s %s %.6f", what, paused and "paused" or "playing", t))
                    mp.commandv("script-message", "afp-compare", what, string.format("%.6f", t))
                end
            end
        end
    end
    if o.shots ~= "" then
        steps[#steps + 1] = function() mp.command_native({ "screenshot-to-file", o.shots .. "-plays.png", "window" }) end
        steps[#steps + 1] = function() mp.commandv("script-message", "afp-compare", "on") end
        steps[#steps + 1] = function() mp.command_native({ "screenshot-to-file", o.shots .. "-recorded.png", "window" }) end
        steps[#steps + 1] = function() mp.commandv("script-message", "afp-compare", "off") end
    end
    steps[#steps + 1] = function() mp.command("quit") end
    local i = 0
    local function next_step()
        i = i + 1
        if steps[i] then steps[i](); mp.add_timeout(o.gap, next_step) end
    end
    next_step()
end

local timer
mp.register_event("file-loaded", function()
    local waited = 0
    timer = mp.add_periodic_timer(0.25, function()
        waited = waited + 0.25
        local gpu = mp.get_property_native("user-data/gyroflow/gpu")
        local warp = type(gpu) == "table" and (gpu.mode or "") ~= ""
        if (warp and waited >= o.settle) or waited >= o.settle + 20 then
            timer:kill()
            run()
        end
    end)
end)
