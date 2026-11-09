-- Paused, the frame at its best: without its noise (Ctrl+p, on or off: it sticks; still=yes in gyroflow.conf; none
-- with temporal_denoise=off, this computer's Performance & Quality, whatever Ctrl+p says), and at full size (Ctrl+f now; by itself with paused_full_size=auto).
--
-- While it plays, the stabilized frame may be rendered smaller than the source (to fit the display, or resolution=1080
-- or 720) from an input scaled down first, and with fps_limit only every 2nd or 4th frame is there. With Ctrl+f, or a
-- moment after the video pauses with paused_full_size=auto (still_delay, so stepping through frames stays quick),
-- main.lua sets the chain up at full size: no input scaling, every frame, the output at the project's or the video's
-- own size (up to 4K's pixels), from the same engine (vs-gyroflow's render_size_for). mpv seeks to the paused frame
-- again with it. Playing on sets it up for playback again at once, paused until its first frame is through; a step to
-- the next frame plays one and pauses again at full size. It's off by default: on a slow GPU each set-up costs a
-- moment, pausing and playing on. On each of these seeks the control file has the plugin leave out the frames before
-- the one on screen, which mpv decodes from the keyframe, and the edge fill's sphere carries on (vs-gyroflow's
-- Edges::seeked).
--
-- At 60 fps the eye averages the noise over time; a paused frame keeps all of it. So that same seek, or one of its
-- own when the chain already is at full size, decodes the frames before it too (hr-seek-framedrop=no: from the
-- keyframe before it), and gyroflow.vpy puts the plugin's Still filter after the stabilization, which rebuilds the frame
-- from it and the still_radius frames on each side, moved onto it by the motion between them (vs-gyroflow/src/still.rs).
-- The stabilization leaves the frames before that window blank and unprocessed (mpv drops them as it seeks), so it
-- takes about a second for a 4K clip. The control file says which frame, only for that seek: the next one (or the
-- chain set up again) plays as usual. Playing on goes on from the cleaned frame, as it was. At full size it's one
-- re-render for both: the cleaned frame is the full-size one.
--
-- Only video the plugin stabilizes: the filter is where the frames are (not where it failed to load the motion data).
-- No clean frame in the last frames of a clip, whose neighbours after it aren't there.
--
-- Loaded by main.lua, which gives it its options, says when the stabilization is on (init) and sets the chain up at
-- full size (full_size).

local mp = require 'mp'
-- Not in crop mode: each turn of the camera renders the frame again, and the cleaned one would come after it
local crop = require 'crop'
local utils = require 'mp.utils'
local platform = require 'platform'

local M = {}

-- What gyroflow.vpy reads as it loads (main.lua passes the path in the filter's user-data): the frame's time, the
-- radius, from when the stabilization skips frames, and how much grain stays
M.CONTROL = utils.join_path(platform.temp_dir(), string.format("mpv-gyroflow-%d.still", utils.getpid()))

-- After playing on from full size, how long it plays before the chain is set up for playback again: a step to the
-- next frame (`.`) plays one and pauses again within it
local PLAYING_ON = 0.25

local o = nil
local stabilizing = nil
local step = nil
-- main.lua's { needed(): playing renders smaller or skips frames, active(): the chain is at full size, auto(): on
-- pausing, set(on) }
local full = nil
-- Ctrl+f while the frame was being cleaned up at the playback size: at full size once that's done
local full_next = false
local enabled = true
local timer = nil
-- Played on from full size: the chain set up for playback once it plays on (PLAYING_ON); paused through that set-up
local playing_on = nil
local resuming = false
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
-- The seek to the paused frame under way: { framedrop = mpv's own hr-seek-framedrop when the frame is cleaned up,
-- ours = its seek event not seen yet }
local pending = nil

-- On: Ctrl+p's switch, where this computer does the temporal denoise (temporal_denoise not off)
local function on() return enabled and o ~= nil and o.temporal_denoise ~= "off" end
local published = nil
local function publish()
    published = on()
    mp.set_property_native("user-data/gyroflow/still", published)
end

local function finish()
    if not pending then return end
    if pending.framedrop then mp.set_property("hr-seek-framedrop", pending.framedrop) end
    pending = nil
    os.remove(M.CONTROL)
end

local function write_control(control)
    local f = io.open(M.CONTROL, "w")
    if not f then
        mp.msg.warn("can't write " .. M.CONTROL)
        return false
    end
    f:write(utils.format_json(control))
    f:close()
    return true
end

-- The chain set up anew while the frame at t is on screen: mpv seeks to it again, and the frames before it aren't
-- needed (the plugin leaves them out, and its sphere knows them: vs-gyroflow's set_rejoin)
local function frame_only(t)
    local fps = mp.get_property_number("container-fps") or mp.get_property_number("estimated-vf-fps")
    if t and fps and fps > 0 and write_control({ skip_before = t - 0.5 / fps }) then pending = { ours = true } end
end

-- full_now: Ctrl+f, at full size whatever paused_full_size is
local function start(full_now)
    timer = nil
    if pending or resuming or failed or not shown or not mp.get_property_native("pause") or not stabilizing() or crop.active() then return end
    local bigger = full and (full_now or full.auto()) and full.needed()
    local t = mp.get_property_number("time-pos")
    local fps = mp.get_property_number("container-fps") or mp.get_property_number("estimated-vf-fps")
    local duration = mp.get_property_number("duration")
    if not t then return end
    -- The frames there are: with fps_limit, every step-th (main.lua), and all of them at full size
    local r = o.still_radius
    fps = fps and fps / (bigger and 1 or step()) or nil
    local clean = on() and fps and fps > 0 and not (duration and t + (r + 2) / fps > duration)
    clean = clean and write_control({ time = t, radius = r, skip_before = t - (r + 1.5) / fps, luma_keep = o.still_grain })
    if not clean and not bigger then return end
    if not clean then frame_only(t) end
    pending = { ours = true }
    if clean then
        pending.framedrop = mp.get_property("hr-seek-framedrop")
        mp.set_property("hr-seek-framedrop", "no")
        mp.msg.verbose(string.format("cleaning up the frame at %.3f s", t))
    end
    mp.osd_message(clean and (bigger and "Cleaning up the frame at full resolution…" or "Cleaning up the frame…") or "Full resolution…", 10)
    if bigger then
        -- mpv seeks to the frame again with the chain set up anew
        full.set(true)
    else
        mp.command_native({ "seek", t, "absolute+exact" })
    end
end

local function schedule()
    if timer then timer:kill() end
    timer = nil
    if shown and not pending and not resuming and mp.get_property_native("pause") then
        timer = mp.add_timeout(o.still_delay, function() start() end)
    end
end

local function play_on()
    playing_on = nil
    if mp.get_property_native("pause") or not full.active() then return end
    -- Paused through the set-up (main.lua's with_pause), which plays on once its first frame is through
    resuming = true
    if not pending then frame_only(mp.get_property_number("time-pos")) end
    full.set(false)
end

mp.observe_property("pause", "bool", function(_, paused)
    if playing_on then playing_on:kill() end
    playing_on = nil
    if not paused and resuming then
        resuming = false
    elseif not paused and full and full.active() then
        playing_on = mp.add_timeout(PLAYING_ON, play_on)
    end
    schedule()
end)

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
    local done = pending and not pending.ours
    if pending and not done then return end
    if done then
        mp.msg.verbose(pending.framedrop and "frame cleaned up" or "frame shown again, the chain set up anew")
        finish()
        mp.osd_message("", 0)
    end
    if full_next then
        full_next = false
        start(true)
    elseif not done then
        -- Paused on arriving (a seek while paused, a clip opened paused)
        schedule()
    end
end)

mp.register_event("end-file", function()
    if timer then timer:kill() end
    if playing_on then playing_on:kill() end
    timer, playing_on, shown, resuming, full_next = nil, nil, false, false, false
    finish()
end)
mp.register_event("shutdown", function() os.remove(M.CONTROL) end)

-- Ctrl+p switched it (main.lua's: the clip's transform keeps it)
local switched_by_hand = nil

-- options: main.lua's (still, temporal_denoise, still_radius, still_delay, still_grain); stabilizing(): the stabilization is on;
-- frame_step(): every how many frames one plays (fps_limit); by_hand(on): Ctrl+p switched it
function M.init(options, is_stabilizing, frame_step, by_hand)
    o, stabilizing, step, switched_by_hand = options, is_stabilizing, frame_step, by_hand
    enabled = o.still
    publish()
end

-- hooks: needed() playing renders the frame smaller than full size or skips frames, active() the chain is at full
-- size, auto() it's set up so on pausing (paused_full_size=auto), set(on) sets it up at full size (while paused: mpv
-- seeks to the frame again) or for playback
function M.full_size(hooks)
    full = hooks
end

-- The temporal denoise as it is: on (Ctrl+p's switch, and this computer does it); Ctrl+p's switch alone
function M.enabled() return on() end
function M.switched() return enabled end

-- On again: the paused frame cleaned a moment after; off: the frame as it is again
local function changed()
    local was = published
    publish()
    if published == was then return end
    if published then
        schedule()
    elseif shown and mp.get_property_native("pause") and not pending then
        -- The frame as it is again (the frames before it left out, the sphere carrying on)
        local t = mp.get_property_number("time-pos")
        if t then
            frame_only(t)
            mp.command_native({ "seek", t, "absolute+exact" })
        end
    end
end

mp.add_key_binding("Ctrl+p", "still", function()
    if o.temporal_denoise == "off" then
        return mp.osd_message("Temporal denoise: off on this computer (Performance & Quality)", 2)
    end
    enabled = not enabled
    mp.osd_message("Temporal denoise: " .. (enabled and "on" or "off") .. " for this clip", 1.5)
    changed()
    if switched_by_hand then switched_by_hand(enabled) end
end)

-- temporal_denoise changed (main.lua, Performance & Quality): off, or on again
function M.available_changed() changed() end

-- The clip's transform says on or off (main.lua's take: a clip's saved bundle, else the session's)
function M.set(on)
    on = on == true
    if on == enabled then return end
    enabled = on
    if o then changed() end
end

-- Ctrl+f: the paused frame at full size now (pausing first), cleaned up in the same re-render when Ctrl+p is on
mp.add_key_binding("Ctrl+f", "full", function()
    if not (full and stabilizing()) then return mp.osd_message("Full resolution: only while it's stabilized", 1.5) end
    if full.active() then return mp.osd_message("Full resolution", 1.5) end
    if not full.needed() then return mp.osd_message("Full resolution: it plays at full size already", 1.5) end
    if not mp.get_property_native("pause") then mp.set_property_native("pause", true) end
    if timer then timer:kill() end
    timer = nil
    if pending or not shown then
        -- Once the frame being cleaned up, or sought to, is on screen
        full_next = true
        mp.osd_message("Full resolution…", 10)
    else
        start(true)
    end
end)

return M
