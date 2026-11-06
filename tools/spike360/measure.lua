-- tools/spike360/measure.lua: the throughput spike's mpv side (step 2 of docs/360-implementation-plan.md). For a
-- clip with two video tracks it sets the chain up per file, as the 360 mode of gyroflow/main.lua would: both tracks
-- joined by lavfi-complex (file-local, so the next clip is untouched), then vf=vapoursynth with copy.vpy. Then it
-- samples mpv's counters and writes them to a CSV, or seeks around for the sync check. Options (--script-opts):
--
--   spike360-join=hstack     how the tracks are joined: hstack (lavfi-complex), both (both decoded, the second
--                            thrown away: hstack's own cost is the difference), none (the first track alone, no
--                            lavfi-complex: the baseline)
--   spike360-convert=        a filter after hstack inside the graph, e.g. format=yuv420p10le (empty: none; mpv
--                            converts for VapourSynth itself when it must)
--   spike360-vs=same         copy.vpy's out: same, 8k, none; or off for no vf=vapoursynth at all
--   spike360-vpy=<path>      copy.vpy
--   spike360-sync=<file>     copy.vpy's sync log; with it the script seeks through the clip instead of measuring
--   spike360-csv=<file>      the samples (every 0.5 s): wall time and mpv's counters
--   spike360-warmup=3        seconds before measuring
--   spike360-measure=15      seconds measured, then quit
--   spike360-frames=0        with --untimed: quit after this many frames instead (0: the measure time)
--   spike360-cache=0         VapourSynth's frame cache in MB (0: its default)
--   spike360-hook=on_preloaded  the hook the chain is set up in: on_preloaded (the tracks known) or on_load (where
--                            gyroflow/main.lua sets its vf; the tracks not known yet, so lavfi-complex set blind)
local utils = require "mp.utils"
local o = { join = "hstack", convert = "", vs = "same", vpy = "", sync = "", csv = "", warmup = 3, measure = 15,
            frames = 0, concurrent = 4, cache = 0, hook = "on_preloaded" }
require("mp.options").read_options(o, "spike360")

local function now() return mp.get_time() end

-- The chain, per file, before its decoders start
mp.add_hook(o.hook, 50, function()
    local videos = {}
    for _, t in ipairs(mp.get_property_native("track-list") or {}) do
        if t.type == "video" and not t.albumart then videos[#videos + 1] = t end
    end
    mp.msg.info(("%s: %d video tracks"):format(o.hook, #videos))
    -- in on_load the track list is empty: the clip is taken for a 360 one by its name (*-360.mp4 or the spike's)
    local path = mp.get_property("path") or ""
    local dual = #videos == 2 or (o.hook == "on_load" and (path:match("%-360%.mp4$") or path:match("/%dk.-%.mp4$") or path:match("/5k7")))
    if dual and (o.join == "hstack" or o.join == "both") then
        local graph = "[vid1][vid2]hstack=inputs=2" .. (o.convert ~= "" and ("," .. o.convert) or "") .. "[vo]"
        if o.join == "both" then graph = "[vid2]nullsink;[vid1]" .. (o.convert ~= "" and o.convert or "null") .. "[vo]" end
        -- file-local: mpv puts the global value back when the file ends
        local ok, err = mp.set_property("file-local-options/lavfi-complex", graph)
        mp.msg.info("lavfi-complex = " .. graph .. (ok and "" or (" failed: " .. tostring(err))))
    end
    if o.vs ~= "off" and dual then
        local user = { out = o.vs }
        if o.sync ~= "" then user.sync = o.sync end
        if o.cache > 0 then user.cache = o.cache end
        -- file-local too, as gyroflow/main.lua sets its own
        local vf = mp.get_property_native("vf") or {}
        vf[#vf + 1] = { name = "vapoursynth", label = "spike360", enabled = true,
                        params = { file = o.vpy, ["user-data"] = utils.format_json(user),
                                   ["concurrent-frames"] = tostring(o.concurrent) } }
        mp.set_property_native("file-local-options/vf", vf)
    end
end)

-- The counters every 0.5 s
local PROPS = { "playback-time", "fps", "estimated-frame-number", "frame-drop-count", "decoder-frame-drop-count",
                "vo-delayed-frame-count", "mistimed-frame-count", "estimated-vf-fps", "estimated-display-fps",
                "avsync", "demuxer-cache-duration", "paused-for-cache" }
local csv
-- The clip's rate, its first video track's (with lavfi-complex mpv has no container-fps)
local clip_fps
local function sample()
    if not csv then return end
    local row = { ("%.3f"):format(now()) }
    for _, p in ipairs(PROPS) do
        local v = p == "fps" and clip_fps or mp.get_property_native(p)
        if type(v) == "boolean" then v = v and 1 or 0 end
        row[#row + 1] = v == nil and "" or (type(v) == "number" and ("%.4f"):format(v):gsub("%.?0+$", "") or tostring(v))
    end
    csv:write(table.concat(row, ","), "\n")
    csv:flush()
end

local function sync_run()
    -- Plays, then seeks of each kind, each followed by 1.5 s of playback that copy.vpy's log checks
    local steps = {
        { "seek", "8", "absolute+keyframes" }, { "seek", "17.4", "absolute+exact" }, { "seek", "3", "absolute" },
        { "seek", "6", "relative" }, { "seek", "-7", "relative+exact" }, { "seek", "20.25", "absolute+exact" },
        { "seek", "1", "absolute" }, { "frame-step" }, { "frame-step" }, { "frame-back-step" }, { "seek", "12", "absolute" },
    }
    local i = 0
    local function step()
        i = i + 1
        if i > #steps then mp.command("quit") return end
        local s = steps[i]
        mp.msg.info("sync step " .. table.concat(s, " "))
        mp.commandv(unpack(s))
        if s[1] ~= "seek" then
            -- frame steps pause; resume after a moment
            mp.add_timeout(0.5, function() mp.set_property_bool("pause", false) end)
        end
        mp.add_timeout(2, step)
    end
    mp.add_timeout(2, step)
end

mp.register_event("file-loaded", function()
    for _, t in ipairs(mp.get_property_native("track-list") or {}) do
        if t.type == "video" and t["demux-fps"] then clip_fps = t["demux-fps"] break end
    end
    if o.csv ~= "" then
        csv = io.open(o.csv, "w")
        csv:write("wall," .. table.concat(PROPS, ","), "\n")
        mp.add_periodic_timer(0.5, sample)
    end
    mp.msg.info("vf = " .. mp.get_property("vf") .. "; lavfi-complex = " .. mp.get_property("lavfi-complex"))
    if o.sync ~= "" then return sync_run() end
    if o.frames > 0 then
        mp.observe_property("estimated-frame-number", "number", function(_, n)
            if n and n >= o.frames then sample(); mp.command("quit") end
        end)
    else
        mp.add_timeout(o.warmup + o.measure, function() sample(); mp.command("quit") end)
    end
end)

mp.register_event("end-file", function() if csv then sample(); csv:close(); csv = nil end end)
