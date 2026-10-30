-- Ctrl+p: the paused frame without its noise, on or off (it sticks; still=yes in gyroflow.conf).
--
-- At 60 fps the eye averages the noise over time; a paused frame keeps all of it. So a moment after the video pauses
-- (still_delay), mpv seeks to the same frame again with the frames before it decoded too (hr-seek-framedrop=no: from
-- the keyframe before it), and gyroflow.vpy puts the plugin's Still filter after the stabilization, which rebuilds
-- the frame from it and the still_radius frames on each side, moved onto it by the motion between them
-- (vs-gyroflow/src/still.rs). The stabilization leaves the frames before that window blank and unprocessed (mpv
-- drops them as it seeks), so it takes about a second for a 4K clip. The control file says which frame, only for that
-- seek: the next one (or the chain set up again) plays as usual. Playing on goes on from the cleaned frame, as it was.
--
-- Only video the plugin stabilizes: the filter is where the frames are (not where it failed to load the motion data).
-- Not in the last frames of a clip, whose neighbours after it aren't there.
--
-- Loaded by main.lua, which gives it its options and says when the stabilization is on (init).

local mp = require 'mp'
local utils = require 'mp.utils'
local platform = require 'platform'

local M = {}

-- What gyroflow.vpy reads as it loads (main.lua passes the path in the filter's user-data): the frame's time, the
-- radius, from when the stabilization skips frames, and how much grain stays
M.CONTROL = utils.join_path(platform.temp_dir(), string.format("mpv-gyroflow-%d.still", utils.getpid()))

local o = nil
local stabilizing = nil
local enabled = true
local timer = nil
-- A frame is on screen and no seek is under way: the time to start from is the one shown
local shown = false
-- The stabilization failed (the plugin couldn't load the clip's motion data): mpv disables the filter but leaves it in
-- vf as it was, so only its log says so. Set up again, it may work: a change of vf clears it
local failed = false
mp.enable_messages("error")
mp.register_event("log-message", function(e)
    if e.prefix == "vf" and e.text:find("Disabling filter gyroflow because it has failed", 1, true) then
        mp.msg.verbose("the stabilization failed: no clean still frame for this clip")
        failed = true
    end
end)
mp.observe_property("vf", "native", function() failed = false end)
-- The seek to the paused frame under way: { framedrop = mpv's own hr-seek-framedrop, ours = its seek event not seen yet }
local pending = nil

local function publish()
    mp.set_property_native("user-data/gyroflow/still", enabled)
end

local function finish()
    if not pending then return end
    mp.set_property("hr-seek-framedrop", pending.framedrop)
    pending = nil
    os.remove(M.CONTROL)
end

local function start()
    timer = nil
    if not enabled or pending or failed or not shown or not mp.get_property_native("pause") or not stabilizing() then return end
    local t = mp.get_property_number("time-pos")
    local fps = mp.get_property_number("container-fps") or mp.get_property_number("estimated-vf-fps")
    local duration = mp.get_property_number("duration")
    if not t or not fps or fps <= 0 then return end
    local r = o.still_radius
    if duration and t + (r + 2) / fps > duration then return end
    local f = io.open(M.CONTROL, "w")
    if not f then return mp.msg.warn("can't write " .. M.CONTROL) end
    f:write(utils.format_json({ time = t, radius = r, skip_before = t - (r + 1.5) / fps, luma_keep = o.still_grain }))
    f:close()
    pending = { framedrop = mp.get_property("hr-seek-framedrop"), ours = true }
    mp.set_property("hr-seek-framedrop", "no")
    mp.msg.verbose(string.format("cleaning up the frame at %.3f s", t))
    mp.osd_message("Cleaning up the frame…", 10)
    mp.command_native({ "seek", t, "absolute+exact" })
end

local function schedule()
    if timer then timer:kill() end
    timer = nil
    if enabled and shown and not pending and mp.get_property_native("pause") then timer = mp.add_timeout(o.still_delay, start) end
end

mp.observe_property("pause", "bool", schedule)

mp.register_event("seek", function()
    shown = false
    if timer then timer:kill() end
    timer = nil
    if not pending then return end
    -- Ours, or another one (a seek of the user's, the chain set up again): that one plays as usual
    if pending.ours then pending.ours = false else finish() end
end)

mp.register_event("playback-restart", function()
    shown = true
    if pending and not pending.ours then
        mp.msg.verbose("frame cleaned up")
        finish()
        mp.osd_message("", 0)
    elseif not pending then
        -- Paused on arriving (a seek while paused, a clip opened paused)
        schedule()
    end
end)

mp.register_event("end-file", function()
    if timer then timer:kill() end
    timer, shown = nil, false
    finish()
end)
mp.register_event("shutdown", function() os.remove(M.CONTROL) end)

-- options: main.lua's (still, still_radius, still_delay, still_grain); stabilizing(): the stabilization is on
function M.init(options, is_stabilizing)
    o, stabilizing = options, is_stabilizing
    enabled = o.still
    publish()
end

function M.enabled() return enabled end

mp.add_key_binding("Ctrl+p", "still", function()
    enabled = not enabled
    publish()
    mp.osd_message("Clean still frame: " .. (enabled and "on" or "off"), 1.5)
    if enabled then
        schedule()
    elseif mp.get_property_native("pause") and not pending then
        -- The frame as it is again
        local t = mp.get_property_number("time-pos")
        if t then mp.command_native({ "seek", t, "absolute+exact" }) end
    end
end)

return M
