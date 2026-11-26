-- compare: the clip as the camera recorded it, against how it plays here (docs/design/compare-as-recorded.md). Held
-- down, `h` shows it as recorded until it's let go; tapped, it stays so until `h` again. Script message
-- `afp-compare [on|off|toggle] [<input time>]` (toggle when none; what the welcome card sends) does the same, for
-- a button or a menu. user-data/gyroflow/compare says which is shown (true: as recorded).
--
-- As recorded is: no log conversion (the look's shader passes the camera's code values through as a player that
-- doesn't know the curve shows them: BT.709, SDR white at HLG 75 %), no stabilization, no denoise, no colour fringes
-- taken out. Nothing is rebuilt and nothing reloads, so it switches at the next frame drawn, paused too:
--   - glsl-shader-opts gyroflow_gpu_on=0: every pass of the warp (warp.glsl: the warp, edge fill, denoise) has
--     `WHEN gyroflow_gpu_on`, so libplacebo skips them and mpv scales the frame as decoded;
--   - glsl-shader-opts afp_recorded=1: the look shaders (shaders/*_hlg.glsl.in, dlogm_lut3d.glsl.in,
--     underwater_hlg.glsl) take their as-recorded branch.
-- Both in one change of the property, so one frame shows the whole switch. Back again, the keys are taken out and
-- the shaders' own defaults hold. The denoise's ring of frames isn't fed while it's skipped: back from it while
-- playing, the picture holds its first frame for as many frames as the denoise shows behind the newest (5), as after
-- a seek, then plays on clean. While the denoise is on, as recorded is the newest frame decoded (the picture with it
-- shows the one 5 frames before that): paused, the two are those 5 frames apart.
--
-- Where the warp turns or reshapes the frame (the file's own turn, an output of another shape) mpv is told the file's
-- geometry for as long as it's shown as recorded (video-rotate, video-aspect-override, file-local), and what the
-- warp had afterwards. A 360 camera's own file isn't compared: its picture is the warp's sphere, nothing without it.
-- The card ("As recorded") stays while it's shown. A photo, a photo link or a moment's picture taken meanwhile takes
-- the clip as it plays (export.lua calls M.off first).
--
-- <input time>: the host clock's seconds of the input that asked (as NSEvent.timestamp), put into glsl-shader-opts
-- as afp_input_time with the switch, for the app's mpv's --macos-latency-log (patch 0010): how long until it's on
-- screen (tools/compare_latency.py).
--
--   require('compare').setup(W), from main.lua: the key and the message registered, end-file heard

local mp = require 'mp'

local M = {}
-- A press held longer than this is a hold: let go, back to as it plays
local HOLD = 0.35

local S
-- shown: as recorded on screen; since: when the key went down; latched: a tap left it on; ignore_up: the key's up of
-- the press that switched it off; saved: what the warp had of mpv's geometry before
local state = { shown = false, since = 0, latched = false, ignore_up = false, saved = nil }

local function publish() mp.set_property_native("user-data/gyroflow/compare", state.shown) end

-- What as recorded leaves out on this clip, for the card
local function left_out()
    local parts = {}
    local dl = mp.get_property_native("user-data/dlogm") or {}
    if (dl.log == true or dl.curve == "uw") and dl.look ~= "off" then parts[#parts + 1] = "the colour" end
    local gpu = S and S.gpu
    if gpu and gpu.shader and not gpu.hold then
        parts[#parts + 1] = "the stabilization"
        if gpu.denoise then parts[#parts + 1] = "the denoise" end
    end
    if #parts == 0 then return "Nothing is changed on this clip" end
    if #parts == 1 then return "Without " .. parts[1] end
    return "Without " .. table.concat(parts, ", ", 1, #parts - 1) .. " and " .. parts[#parts]
end

-- The key bound to it now (input.conf may have moved it), as the card says it
local function key_name()
    for _, b in ipairs(mp.get_property_native("input-bindings") or {}) do
        if b.cmd and b.cmd:find("gyroflow/compare", 1, true) then return b.key:match("^%l$") and b.key:upper() or b.key end
    end
    return nil
end

local function card()
    if not state.shown then return mp.notify({ id = "gyroflow:compare", kind = "hide" }) end
    local key = key_name()
    local back = state.latched and (key and (key .. " again: as it plays here") or "as it plays here: afp-compare again")
                 or (key and ("Let go of " .. key .. ": as it plays here") or nil)
    mp.notify({ id = "gyroflow:compare", kind = "info", seconds = 86400, title = "As recorded",
                detail = left_out() .. (back and ("\n" .. back) or "") })
end

-- mpv shown the file's own geometry while the warp, which does it otherwise, is skipped; nil: nothing to change
local function geometry()
    local gpu, src = S and S.gpu, S and S.current_source
    if not gpu or not gpu.shader or gpu.hold or not src then return nil end
    local rot = tonumber(src.rotation) or 0
    local aspect = mp.get_property("file-local-options/video-aspect-override")
    local reshaped = aspect ~= nil and aspect ~= "" and aspect ~= "no" and aspect ~= "-1"
    if rot % 360 == 0 and not reshaped then return nil end
    return { rotate = tostring(math.floor(rot % 360)), aspect = "no" }
end

local function apply(on, input_time)
    local opts = mp.get_property_native("glsl-shader-opts") or {}
    if on then
        opts.gyroflow_gpu_on, opts.afp_recorded = "0", "1"
    else
        opts.gyroflow_gpu_on, opts.afp_recorded = nil, nil
    end
    if input_time then opts.afp_input_time = string.format("%.6f", input_time) end
    -- The file's geometry first, so the frame drawn with the switch is drawn in it
    if on then
        local g = geometry()
        if g then
            state.saved = { rotate = mp.get_property("file-local-options/video-rotate"),
                            aspect = mp.get_property("file-local-options/video-aspect-override") }
            mp.set_property("file-local-options/video-rotate", g.rotate)
            mp.set_property("file-local-options/video-aspect-override", g.aspect)
        end
    end
    mp.set_property_native("glsl-shader-opts", opts)
    if not on and state.saved then
        if state.saved.rotate then mp.set_property("file-local-options/video-rotate", state.saved.rotate) end
        if state.saved.aspect then mp.set_property("file-local-options/video-aspect-override", state.saved.aspect) end
        state.saved = nil
    end
end

-- As recorded on or off (input_time: for the latency log, as above). False: not for this clip
function M.set(on, input_time)
    on = on == true
    if on and S and S.rig360 then
        mp.notify({ id = "gyroflow:compare", kind = "info", seconds = 3, title = "No as-recorded view for a 360 camera's file",
                    detail = "its picture is made from both lenses as it plays" })
        return false
    end
    if on == state.shown then
        if input_time then apply(on, input_time) end
        return true
    end
    state.shown = on
    if not on then state.latched, state.ignore_up = false, false end
    apply(on, input_time)
    publish()
    card()
    mp.msg.info(on and "compare: as recorded" or "compare: as it plays")
    return true
end

-- Back to as it plays, if it isn't (a photo taken, the file ended)
function M.off()
    if state.shown then M.set(false) end
end
function M.shown() return state.shown end

-- The key: held, as recorded until it's let go; tapped, until the next tap
local function key(e)
    local now = mp.get_time()
    if e.event == "down" or e.event == "press" then
        if state.shown then
            -- A tap left it on (or the message did): this press ends it, its up does nothing more
            M.set(false)
            state.ignore_up = e.event == "down"
            return
        end
        if not M.set(true) then return end
        state.since, state.latched = now, e.event == "press"
        if state.latched then card() end
    elseif e.event == "up" then
        if state.ignore_up then state.ignore_up = false; return end
        if not state.shown then return end
        if now - state.since >= HOLD then
            M.set(false)
        else
            state.latched = true
            card()
        end
    end
end

-- afp-compare [on|off|toggle] [<input time>]
local function message(what, t)
    local input_time = tonumber(t)
    if what == "on" then M.set(true, input_time)
    elseif what == "off" then M.set(false, input_time)
    else
        if not state.shown then
            if M.set(true, input_time) then state.latched = true; card() end
        else
            M.set(false, input_time)
        end
    end
end

-- The file ended: its file-local geometry goes with it (mpv restores it), the parameters come out
local function ended()
    state.saved = nil
    M.off()
end

function M.setup(w)
    S = w.S
    publish()
    mp.add_key_binding("h", "compare", key, { complex = true })
    mp.register_script_message("afp-compare", message)
    mp.register_event("end-file", ended)
end

return M
