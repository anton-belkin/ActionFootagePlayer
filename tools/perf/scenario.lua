-- tools/perf/scenario.lua: what playback costs around each action, for issue #43. An mpv script that tools/perf/run.sh
-- loads into the app; it plays the clip through a fixed list of actions and writes, into the run's folder:
--
--   summary.csv  one row per step: what the action cost (the counters' increase over the step, how long until the
--                drops stopped, the worst A/V desync, the filter's rate)
--   samples.csv  every 100 ms: the counters as they are, for a timeline of each step
--   meta.txt     what played and how: mpv, the video, the display, the plugins' state, the filter chain, the
--                video output's passes
--
-- Times are unix times to the millisecond, as the plugin's `perf` lines in vs-gyroflow.log (VS_GYROFLOW_PERF=1, which
-- run.sh sets), so the two line up.
--
-- The steps (perf-steps= to pick, in order):
--   start           the app opening the clip, until its first frame (also from run.sh's launch), then `observe` s
--   steady          `steady` s of plain playback; mpv's video output passes after it go into meta.txt. It can come more
--                   than once, to measure the state other steps leave
--   fullscreen_on, fullscreen_off
--   view_wide, view_off, view_normal       the stabilization's view (Ctrl+g), when the clip is stabilized
--   look_sdr, look_hdr                     DJI's Rec.709 look and back (Ctrl+s, Ctrl+h), when the clip is log
--   chromatic_off   the water's color fringe shader (gyroflow's chromatic.glsl) taken out of the file's shaders, to
--                   measure what it costs: an experiment, the app has no such switch
--   seek_forward, seek_back                10 s
--   next_clip       the next clip in the folder's playlist, until its first frame, then `observe` s
--   monitor, monitor_back                  with perf-monitor=yes: the window moved to the other monitor and back, by
--                                          hand, as the OSD asks
-- A step whose action doesn't apply to the clip is a row with its reason in `notes`.
--
-- Options (--script-opts=perf-<name>=<value>, run.sh passes them): out, label, launched (unix time), steps,
-- observe=5, steady=15, monitor=no, quit=yes.

local mp = require 'mp'
local options = require 'mp.options'
local utils = require 'mp.utils'

local o = {
    out = "",
    label = "",
    launched = 0,
    steps = "start,steady,fullscreen_on,fullscreen_off,view_wide,view_off,view_normal,look_sdr,look_hdr,seek_forward,seek_back,next_clip",
    observe = 5,
    steady = 15,
    monitor = false,
    quit = true,
}
options.read_options(o, "perf")

local SAMPLE_INTERVAL = 0.1
-- How long a step waits for the frame after its action, when one comes (a seek, the next clip, the first one)
local START_TIMEOUT = 120
local SEEK_TIMEOUT = 10
local MONITOR_TIME = 15

-- Unix time with milliseconds: os.time() has whole seconds only, so the clock is read once and mpv's own goes from there
local function unix_time()
    local r = mp.command_native({ name = "subprocess", args = { "perl", "-MTime::HiRes=time", "-e", "printf '%.3f', time" },
                                  capture_stdout = true, playback_only = false })
    return r and r.status == 0 and tonumber(r.stdout) or os.time()
end
local clock_unix, clock_mpv = unix_time(), mp.get_time()
local function now() return clock_unix + (mp.get_time() - clock_mpv) end

if o.out == "" then
    o.out = utils.join_path(os.getenv("HOME") or ".", "ActionFootagePlayer-perf/" .. os.date("%Y%m%d-%H%M%S"))
end
mp.command_native({ name = "subprocess", args = { "mkdir", "-p", o.out }, playback_only = false })

local function open(name)
    local f = assert(io.open(utils.join_path(o.out, name), "w"))
    f:setvbuf("line")
    return f
end
local summary, samples, meta = open("summary.csv"), open("samples.csv"), open("meta.txt")

local function csv(...)
    local fields = {}
    for i = 1, select("#", ...) do
        local v = select(i, ...)
        if v == nil then
            v = ""
        elseif type(v) == "number" then
            v = (v == math.floor(v) and math.abs(v) < 1e15) and string.format("%d", v) or string.format("%.3f", v)
        else
            v = tostring(v)
            if v:find('[,"\n]') then v = '"' .. v:gsub('"', '""'):gsub("\n", " ") .. '"' end
        end
        fields[i] = v
    end
    return table.concat(fields, ",") .. "\n"
end

summary:write(csv("label", "step", "start", "restart_s", "file_loaded_s", "drops", "decoder_drops", "delayed", "max_avsync_ms",
                  "last_drop_s", "vf_fps_min", "vf_fps_avg", "duration_s", "notes"))
samples:write(csv("time", "step", "t", "drops", "decoder_drops", "delayed", "avsync_ms", "vf_fps", "time_pos", "paused",
                  "fullscreen", "osd_w", "osd_h"))

local function value(v)
    if type(v) == "table" then return utils.format_json(v) end
    return tostring(v)
end

local META = { "mpv-version", "ffmpeg-version", "platform", "path", "playlist-pos", "playlist-count", "current-vo",
               "current-gpu-context", "hwdec", "hwdec-current", "video-params", "video-dec-params", "container-fps",
               "display-names", "display-width", "display-height", "display-hidpi-scale", "display-fps",
               "estimated-display-fps", "fullscreen", "osd-dimensions", "video-sync", "framedrop", "vf", "glsl-shaders",
               "target-trc", "target-colorspace-hint", "user-data/gyroflow", "user-data/dlogm", "user-data/dji_telemetry" }

local function write_meta(heading)
    meta:write("[" .. heading .. " " .. string.format("%.3f", now()) .. "]\n")
    for _, name in ipairs(META) do
        local v = mp.get_property_native(name)
        if v ~= nil then meta:write(name .. "=" .. value(v) .. "\n") end
    end
    meta:write("\n")
end

meta:write("label=" .. o.label .. "\nlaunched=" .. tostring(o.launched) .. "\nsteps=" .. o.steps .. "\nobserve=" .. o.observe
           .. "\nsteady=" .. o.steady .. "\n\n")

-- The step being measured: its name, start, the counters as it started, and what happened since
local step = nil
-- The counters run over the steps; mpv starts them over with each file, so a drop back is a new file's count
local counters = { drops = "frame-drop-count", decoder_drops = "decoder-frame-drop-count", delayed = "vo-delayed-frame-count" }
local last_raw = {}
local totals = { drops = 0, decoder_drops = 0, delayed = 0 }

local function read_counters()
    for k, prop in pairs(counters) do
        local v = mp.get_property_number(prop)
        if v then
            local before = last_raw[k]
            if before == nil or v < before then
                totals[k] = totals[k] + (before == nil and 0 or v)
            else
                totals[k] = totals[k] + (v - before)
            end
            last_raw[k] = v
        end
    end
end

local function sample()
    read_counters()
    if not step then return end
    local t = now()
    local avsync = mp.get_property_number("avsync")
    local vf_fps = mp.get_property_number("estimated-vf-fps")
    local paused = mp.get_property_native("pause")
    local w, h = mp.get_osd_size()
    samples:write(csv(t, step.name, t - step.start, totals.drops, totals.decoder_drops, totals.delayed,
                      avsync and avsync * 1000 or nil, vf_fps, mp.get_property_number("time-pos"), paused and 1 or 0,
                      mp.get_property_native("fullscreen") and 1 or 0, w, h))
    if totals.drops > step.seen.drops or totals.delayed > step.seen.delayed then step.last_drop = t - step.start end
    step.seen = { drops = totals.drops, delayed = totals.delayed }
    -- A step that waits for a frame (the first one, a seek's) is timed from it
    if not step.ready then return end
    if avsync and not paused then step.max_avsync = math.max(step.max_avsync, math.abs(avsync)) end
    if vf_fps and vf_fps > 0 then
        step.fps_min = math.min(step.fps_min or vf_fps, vf_fps)
        step.fps_sum, step.fps_n = step.fps_sum + vf_fps, step.fps_n + 1
    end
end
mp.add_periodic_timer(SAMPLE_INTERVAL, sample)

local first_restart = nil
mp.register_event("playback-restart", function()
    local t = now()
    first_restart = first_restart or t
    if step and not step.restart then step.restart, step.ready = t, true end
end)
mp.register_event("file-loaded", function()
    if step and not step.loaded then step.loaded = now() end
end)

local function begin(name)
    read_counters()
    step = { name = name, start = now(), ready = true, base = { drops = totals.drops, decoder_drops = totals.decoder_drops, delayed = totals.delayed },
             seen = { drops = totals.drops, delayed = totals.delayed }, max_avsync = 0, fps_sum = 0, fps_n = 0, notes = {} }
    mp.msg.info("perf step " .. name)
end

local function finish_step()
    sample()
    local s = step
    local rel = function(t) return t and (t - s.start) or nil end
    summary:write(csv(o.label, s.name, s.start, rel(s.restart), rel(s.loaded), totals.drops - s.base.drops,
                      totals.decoder_drops - s.base.decoder_drops, totals.delayed - s.base.delayed, s.max_avsync * 1000,
                      s.last_drop, s.fps_min, s.fps_n > 0 and s.fps_sum / s.fps_n or nil, now() - s.start,
                      table.concat(s.notes, "; ")))
    step = nil
end

local function skipped(name, reason)
    begin(name)
    step.ready = false
    table.insert(step.notes, "skipped: " .. reason)
    finish_step()
end

-- The scenario runs as a coroutine: sleep() and wait_restart() hand mpv back its event loop meanwhile
local co = nil
local function resume()
    local ok, err = coroutine.resume(co)
    if not ok then
        mp.msg.error("perf: " .. tostring(err))
        meta:write("error=" .. tostring(err) .. "\n")
    end
end
local function sleep(seconds)
    mp.add_timeout(seconds, resume)
    coroutine.yield()
end
-- Until the step's first frame after its action, at most `timeout` s; false when none came
local function wait_restart(timeout)
    local deadline = mp.get_time() + timeout
    while not step.restart and mp.get_time() < deadline do sleep(SAMPLE_INTERVAL) end
    return step.restart ~= nil
end

local function gyroflow(key) return mp.get_property_native("user-data/gyroflow/" .. key) end
local function dlogm(key) return mp.get_property_native("user-data/dlogm/" .. key) end

local function message_gyroflow(view)
    return {
        needs = function() return not gyroflow("available") and "the clip isn't stabilized" or nil end,
        run = function()
            table.insert(step.notes, "from " .. tostring(gyroflow("view")))
            mp.commandv("script-message-to", "gyroflow", "view", view)
        end,
    }
end

local function seek(seconds)
    return { run = function() mp.commandv("seek", tostring(seconds), "relative") end, wait = SEEK_TIMEOUT }
end

local function monitor(name)
    return {
        needs = function() return not o.monitor and "perf-monitor=no" or nil end,
        run = function()
            table.insert(step.notes, "displays before: " .. value(mp.get_property_native("display-names")))
            mp.osd_message("Perf: move the window to the " .. name .. " monitor now", MONITOR_TIME - 2)
        end,
        duration = MONITOR_TIME,
        after = function() table.insert(step.notes, "after: " .. value(mp.get_property_native("display-names"))) end,
    }
end

local STEPS = {
    start = {
        wait = START_TIMEOUT,
        arrived = function()
            if not step.restart then table.insert(step.notes, "no frame in " .. START_TIMEOUT .. " s") end
            if first_restart and tonumber(o.launched) and tonumber(o.launched) > 0 then
                table.insert(step.notes, string.format("first frame %.3f s after launch", first_restart - tonumber(o.launched)))
            end
            write_meta("start")
        end,
    },
    steady = {
        duration = function() return o.steady end,
        after = function()
            meta:write("[vo-passes after step " .. step.number .. ", steady, " .. string.format("%.3f", now()) .. "]\nvo-passes="
                       .. value(mp.get_property_native("vo-passes")) .. "\nglsl-shaders=" .. value(mp.get_property_native("glsl-shaders"))
                       .. "\nuser-data/gyroflow=" .. value(mp.get_property_native("user-data/gyroflow")) .. "\n\n")
        end,
    },
    chromatic_off = {
        needs = function()
            for _, shader in ipairs(mp.get_property_native("glsl-shaders") or {}) do
                if shader:find("chromatic.glsl", 1, true) then return nil end
            end
            return "no chromatic.glsl in the shaders"
        end,
        run = function()
            local kept = {}
            for _, shader in ipairs(mp.get_property_native("glsl-shaders") or {}) do
                if not shader:find("chromatic.glsl", 1, true) then table.insert(kept, shader) end
            end
            mp.set_property_native("file-local-options/glsl-shaders", kept)
        end,
    },
    fullscreen_on = { run = function() mp.set_property_bool("fullscreen", true) end },
    fullscreen_off = { run = function() mp.set_property_bool("fullscreen", false) end },
    view_wide = message_gyroflow("wide"),
    view_off = message_gyroflow("off"),
    view_normal = message_gyroflow("normal"),
    look_sdr = {
        needs = function()
            if not dlogm("log") then return "not log footage" end
            if not dlogm("sdr") then return "DJI's Rec.709 LUT isn't installed" end
        end,
        run = function() mp.commandv("script-binding", "dlogm/sdr") end,
    },
    look_hdr = {
        needs = function() return not dlogm("log") and "not log footage" or nil end,
        run = function() mp.commandv("script-binding", "dlogm/hdr") end,
    },
    seek_forward = seek(10),
    seek_back = seek(-10),
    next_clip = {
        wait = START_TIMEOUT,
        needs = function()
            local pos, count = mp.get_property_number("playlist-pos", 0), mp.get_property_number("playlist-count", 0)
            return pos + 1 >= count and "no next clip in the playlist" or nil
        end,
        run = function() mp.commandv("playlist-next") end,
        arrived = function()
            if not step.restart then table.insert(step.notes, "no frame in " .. START_TIMEOUT .. " s") end
            table.insert(step.notes, "clip " .. tostring(mp.get_property("filename")))
            write_meta("next_clip")
        end,
    },
    monitor = monitor("other"),
    monitor_back = monitor("first"),
}

local function names()
    local list = {}
    for name in o.steps:gmatch("[^,%s]+") do table.insert(list, name) end
    if o.monitor and not o.steps:find("monitor") then
        table.insert(list, "monitor")
        table.insert(list, "monitor_back")
    end
    return list
end

local function scenario()
    for number, name in ipairs(names()) do
        local s = STEPS[name]
        local reason = not s and "unknown step" or (s.needs and s.needs())
        if reason then
            skipped(name, reason)
        else
            begin(name)
            step.number = number
            step.ready = not s.wait
            if s.run then s.run() end
            if s.wait then wait_restart(s.wait) end
            if s.arrived then s.arrived() end
            local duration = type(s.duration) == "function" and s.duration() or s.duration or o.observe
            sleep(duration)
            if s.after then s.after() end
            finish_step()
        end
    end
    write_meta("end")
    meta:write("results=" .. o.out .. "\n")
    summary:close(); samples:close(); meta:close()
    mp.msg.info("perf: done, " .. o.out)
    mp.osd_message("Perf scenario done: " .. o.out, 5)
    if o.quit then
        sleep(1)
        mp.command("quit")
    end
end

co = coroutine.create(scenario)
-- The first step waits for the clip's first frame itself; mpv's event loop has to run first
mp.add_timeout(0, resume)
