-- Underwater colour for plain SDR clips (GoPro's and the like): the green and brown taken out, the water blue and not
-- purple, the lightness of Action 6's footage under water (research_notes/underwater_color; the rule: estimate.lua).
--
-- Part of the dlogm plugin (required by its main.lua), which shows such a clip through shaders/underwater_hlg.glsl as if it were log (curve "uw"), in its HDR,
-- Dolby Vision or SDR look. This plugin decides how much: a few times a second while it plays (and at once after a
-- seek or a new file) it reads the frame as recorded (ffmpeg, half a second ahead), works out the frame's numbers from
-- a 320x180 grid of it, and the shader's numbers (glsl-shader-opts uw_*) glide to them, quicker when the scene changed.
--
-- Its three settings are part of the transform bundle (docs/moments-and-workspaces.md, "Source and transform";
-- mpv/scripts/workspace/transform.lua), the clip's or moment's like the stabilization's:
--   uw             the correction on (true) or off
--   uw_highlights  the light end stretched into HDR's room (2.5x SDR white for the camera's white); helps sunlit
--                  water and sand, can blow out a torch's spot at night
--   uw_strength    between the camera's colour (0) and the correction (1), in hundredths
-- As the gyroflow plugin's: the defaults are uwcolor.conf's (on=, highlights=, strength=; Transform's Use as default
-- writes them); the session's start as them and change by hand on a file with nothing saved; a file with a bundle of
-- its own plays with it, a change there is for viewing (or the Transform's Save, Auto save).
-- Messages (to dlogm): uw-set uw|uw_highlights yes|no|toggle, uw-set uw_strength <0..1>; uw-apply-view <json> (Revert
-- to saved); uw-remember (Use as default); key dlogm/uw-toggle
-- State: user-data/uwcolor = { uw, uw_highlights, uw_strength, session, defaults, active (dlogm shows the clip through
-- it), ok (the clip can have it), stats = { what the last frame measured } }

local mp = require 'mp'

-- The time of the frame on screen (docs/seek-and-scrub.md): with the GPU path's temporal denoise the picture is up to
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
local options = require 'mp.options'
local utils = require 'mp.utils'
local E = require 'uw_estimate'

local o = { on = false, highlights = false, strength = 1.0 }
options.read_options(o, "uwcolor")

local function hundredths(x) return math.floor(math.max(0, math.min(1, tonumber(x) or 1)) * 100 + 0.5) / 100 end
local function from_conf()
    return { uw = o.on == true, uw_highlights = o.highlights == true, uw_strength = hundredths(o.strength) }
end
local BUNDLE = { "uw", "uw_highlights", "uw_strength" }
local DEFAULTS = from_conf()
local session, cur = {}, {}
for _, k in ipairs(BUNDLE) do session[k], cur[k] = DEFAULTS[k], DEFAULTS[k] end

local HIGHLIGHTS = 2.5
local GW, GH = 320, 180
local KEYS = { "dr", "db", "k", "ev" }

local params = nil           -- the numbers the shader has, eased
local stats = {}

local function active()
    return mp.get_property_native("user-data/dlogm/curve") == "uw"
end

local function publish()
    mp.set_property_native("user-data/uwcolor", {
        uw = cur.uw, uw_highlights = cur.uw_highlights, uw_strength = cur.uw_strength, session = session, defaults = DEFAULTS,
        active = active(), ok = mp.get_property_native("user-data/dlogm/uw_ok") == true, stats = stats,
    })
end

local function save()
    local path = mp.command_native({ "expand-path", "~~/script-opts/uwcolor.conf" })
    local dir = path:match("^(.*)[/\\]")
    if dir and not utils.file_info(dir) then
        mp.command_native({ name = "subprocess", playback_only = false, args = { "mkdir", "-p", dir } })
    end
    local f = io.open(path, "w")
    if not f then return end
    f:write(string.format("on=%s\nhighlights=%s\nstrength=%.2f\n", o.on and "yes" or "no", o.highlights and "yes" or "no", o.strength))
    f:close()
    return true
end

-- The shader's numbers, set together (the property is the whole table)
local function send()
    local opts = mp.get_property_native("glsl-shader-opts") or {}
    local p = params or { dr = 0, db = 0, k = 0, ev = 0 }
    opts.uw_dr = string.format("%.4f", p.dr)
    opts.uw_db = string.format("%.4f", p.db)
    opts.uw_k = string.format("%.4f", p.k)
    opts.uw_ev = string.format("%.4f", p.ev)
    opts.uw_s = string.format("%.3f", cur.uw_strength)
    opts.uw_hp = cur.uw_highlights and tostring(HIGHLIGHTS) or "1"
    mp.set_property_native("glsl-shader-opts", opts)
end

local function round(x, n)
    if x == nil then return nil end
    local m = 10 ^ (n or 2)
    return math.floor(x * m + 0.5) / m
end

-- The frame as recorded, measured. Read by ffmpeg beside mpv (in the background, so playback never waits), 960x540:
-- while it plays the frame LEAD seconds ahead, as that's about when the numbers are ready (the keyframes alone, a
-- second apart on GoPro, lagged a scene change by up to a second: Anton, 2026-11-24); paused, the frame shown.
-- Decoded by VideoToolbox on macOS (an exact frame costs about what a keyframe did). Not mpv's own screenshot: the
-- video is tagged HLG for the shader, and a software screenshot converts it as if it were.
-- `jump`: the numbers taken at once (a seek, a new file); else they're the target the shader's numbers ease to
local LEAD = 0.5
local MIN_GAP = 0.35         -- seconds between the starts of two measurements while it plays
local busy, started, epoch = false, -1, 0
local target = nil
local HWACCEL = (mp.get_property("platform") or "") == "darwin" and { "-hwaccel", "videotoolbox" } or {}
local function measure(jump)
    if not active() then return end
    if busy then
        -- one in flight from before a seek: its answer is dropped, and this one runs when it's back
        if jump then epoch = epoch + 1; busy = "again" end
        return
    end
    local path = mp.get_property("path")
    local t = shown_time()
    if not path or path:find("://", 1, true) or not t then return end
    local playing = not mp.get_property_native("pause")
    local at = playing and t + LEAD or t
    local dur = mp.get_property_number("duration")
    if dur and at > dur - 0.05 then at = math.max(0, dur - 0.05) end
    busy, started = true, mp.get_time()
    local mine = epoch
    local args = { "ffmpeg", "-v", "error", "-nostdin" }
    for _, a in ipairs(HWACCEL) do args[#args + 1] = a end
    for _, a in ipairs({ "-ss", string.format("%.3f", at), "-i", path, "-map", "0:v:0", "-frames:v", "1",
                         "-vf", string.format("scale=%d:%d:flags=area", GW * 3, GH * 3), "-f", "rawvideo", "-pix_fmt", "bgr0", "-" }) do
        args[#args + 1] = a
    end
    mp.command_native_async({ name = "subprocess", playback_only = false, capture_stdout = true, capture_stderr = true, args = args },
    function(ok, res)
        local again = busy == "again"
        busy = false
        if again or mine ~= epoch then return measure(true) end
        local data = ok and res and res.stdout
        if not data or #data ~= GW * 3 * GH * 3 * 4 then
            mp.msg.verbose("underwater colour: no frame from ffmpeg " .. tostring(res and res.stderr))
            return
        end
        if not active() then return end
        local grabbed = mp.get_time() - started
        local R, G, B = E.grid_from_bgr0(data, GW * 3, GH * 3, GW * 3 * 4, GW, GH)
        data = nil
        local p, st = E.estimate(R, G, B, GW, GH)
        target = p
        if jump or not params then
            params = {}
            for _, key in ipairs(KEYS) do params[key] = p[key] end
            send()
        end
        stats = {
            under_water = round(st.f), water = round(st.water), red = round(st.red), blue = round(st.blue), blue_water = round(st.blue_water),
            leak = round(st.leak), water_hue = round(st.water_hue, 0), water_hue_after = round(st.water_hue_after, 0),
            water_a = round(st.water_a, 3), water_a_after = round(st.water_a_after, 3), lift = round(st.ev),
            median = round(st.median_y), at = round(at, 1),
            applied = { red = round(params.dr), blue = round(params.db), leak = round(params.k), lift = round(params.ev) },
            ms = math.floor((mp.get_time() - started) * 1000 + 0.5), grab_ms = math.floor(grabbed * 1000 + 0.5),
        }
        publish()
    end)
end

-- The shader's numbers eased towards the last measurement, 20 times a second: no step to see. Slowly (TAU_SLOW) for
-- the small wander between frames of one scene; quickly (TAU_FAST) when the scene changed (far from it)
local TAU_SLOW, TAU_FAST = 1.2, 0.35
local FAR = { dr = 0.12, db = 0.12, k = 0.12, ev = 0.25 }
local last_ease = nil
mp.add_periodic_timer(0.05, function()
    local now = mp.get_time()
    local dt = last_ease and math.min(0.2, now - last_ease) or 0.05
    last_ease = now
    if not (active() and params and target) then return end
    local far, moved = false, false
    for _, key in ipairs(KEYS) do
        if math.abs(target[key] - params[key]) > FAR[key] then far = true end
    end
    local a = 1 - math.exp(-dt / (far and TAU_FAST or TAU_SLOW))
    for _, key in ipairs(KEYS) do
        local d = target[key] - params[key]
        if math.abs(d) > 0.002 then params[key] = params[key] + d * a; moved = true end
    end
    if moved then send() end
end)

-- Measured again as soon as there's room while it plays
mp.add_periodic_timer(0.1, function()
    if active() and not busy and mp.get_time() - started >= MIN_GAP and not mp.get_property_native("pause")
       and not mp.get_property_native("seeking") then
        measure(false)
    end
end)
mp.register_event("playback-restart", function() if active() then measure(true) end end)
mp.register_event("file-loaded", function() params, target, stats = nil, nil, {}; send(); publish() end)
mp.observe_property("user-data/dlogm/curve", "native", function()
    if active() then measure(true) end
    publish()
end)
mp.observe_property("user-data/dlogm/uw_ok", "native", publish)

local function flag(v, was)
    if v == "toggle" then return not was end
    return v == "yes" or v == "true" or v == "1"
end

-- The workspace plugin's bundle for the file playing, when one is saved for it; nil: the session's plays
local function fixed()
    local ws = mp.get_property_native("user-data/workspace")
    if type(ws) == "table" and ws.path == mp.get_property("path") and type(ws.view) == "table" then return ws.view end
    return nil
end

-- A bundle taken as what plays (a key it hasn't: the session's)
local function take(b)
    b = type(b) == "table" and b or session
    cur.uw = type(b.uw) == "boolean" and b.uw or session.uw
    cur.uw_highlights = type(b.uw_highlights) == "boolean" and b.uw_highlights or session.uw_highlights
    cur.uw_strength = type(b.uw_strength) == "number" and hundredths(b.uw_strength) or session.uw_strength
end

-- What plays applied: the dlogm plugin's curve when on changed, the shader's numbers
local function apply(was_on)
    if cur.uw ~= was_on then mp.commandv("script-message-to", mp.get_script_name(), "uw-color", cur.uw and "yes" or "no") end
    send()
    publish()
end

-- A change by hand: the session's too when the session's plays (nothing saved for the file)
local function set(k, v)
    local was = cur.uw
    cur[k] = v
    if not fixed() then session[k] = v end
    apply(was)
end

mp.register_script_message("uw-set", function(key, v)
    if key == "uw" or key == "on" then
        set("uw", flag(v, cur.uw))
        mp.osd_message(cur.uw and "Underwater colour on" or "Underwater colour off", 1.5)
    elseif key == "uw_highlights" or key == "highlights" then
        set("uw_highlights", flag(v, cur.uw_highlights))
    elseif key == "uw_strength" or key == "strength" then
        if tonumber(v) then set("uw_strength", hundredths(v)) end
    end
end)

-- A bundle played now, for this viewing (the panel's Revert to saved)
mp.register_script_message("uw-apply-view", function(json)
    local b = utils.parse_json(json or "")
    if type(b) ~= "table" then return end
    local was = cur.uw
    take(b)
    apply(was)
end)

-- What plays as the defaults (Transform's Use as default, with the gyroflow plugin's)
mp.register_script_message("uw-remember", function()
    o.on, o.highlights, o.strength = cur.uw, cur.uw_highlights, cur.uw_strength
    if save() then
        DEFAULTS = from_conf()
        for _, k in ipairs(BUNDLE) do session[k] = cur[k] end
        publish()
    end
end)

mp.add_key_binding(nil, "uw-toggle", function() mp.commandv("script-message-to", mp.get_script_name(), "uw-set", "uw", "toggle") end)

send()
publish()

local M = {}
-- At a file's start (the dlogm plugin's on_preloaded, before it picks the curve): its bundle taken; whether the
-- correction is on for it
function M.file_start()
    take(fixed())
    send()
    publish()
    return cur.uw
end
return M
