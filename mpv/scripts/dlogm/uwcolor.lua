-- Underwater colour for plain SDR clips (GoPro's and the like): the green and brown taken out, the water blue and not
-- purple, the lightness of Action 6's footage under water (research_notes/underwater_color; the rule: estimate.lua).
--
-- Part of the dlogm plugin (required by its main.lua), which shows such a clip through shaders/underwater_hlg.glsl as if it were log (curve "uw"), in its HDR,
-- Dolby Vision or SDR look. This plugin decides how much: a few times a second while it plays (and at once after a
-- seek or a new file) it reads the frame as recorded (ffmpeg, half a second ahead), works out the frame's numbers from
-- a 320x180 grid of it, and the shader's numbers (glsl-shader-opts uw_*) glide to them, quicker when the scene changed.
--
-- Its three settings are the source's input (docs/moments-and-workspaces.md, "Source and transform";
-- mpv/scripts/workspace/transform.lua), as the log curve is: it takes out the colour the camera put in, so every
-- moment and clip preview of a clip plays it as the clip's own file says (Anton, 2026-11-24):
--   uw             the correction on (true) or off
--   uw_highlights  the light end stretched into HDR's room (2.5x SDR white for the camera's white); helps sunlit
--                  water and sand, can blow out a torch's spot at night
--   uw_strength    between the camera's colour (0) and the correction (1), in hundredths
-- Where the source has none of them, uwcolor.conf's (on=, highlights=, strength=). A change by hand is for viewing
-- (the workspace plugin's input-set) until the Source tab's Save writes it into the source's file.
-- Messages (to dlogm): uw-set uw|uw_highlights yes|no|toggle, uw-set uw_strength <0..1>; key dlogm/uw-toggle
-- State: user-data/uwcolor = { uw, uw_highlights, uw_strength, defaults, active (dlogm shows the clip through it),
-- ok (the clip can have it), stats = { what the last frame measured } }

local mp = require 'mp'
local options = require 'mp.options'
local E = require 'uw_estimate'

local o = { on = false, highlights = false, strength = 1.0 }
options.read_options(o, "uwcolor")

local function hundredths(x) return math.floor(math.max(0, math.min(1, tonumber(x) or 1)) * 100 + 0.5) / 100 end
local function from_conf()
    return { uw = o.on == true, uw_highlights = o.highlights == true, uw_strength = hundredths(o.strength) }
end
local BUNDLE = { "uw", "uw_highlights", "uw_strength" }
local DEFAULTS = from_conf()
local cur = {}
for _, k in ipairs(BUNDLE) do cur[k] = DEFAULTS[k] end

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
        uw = cur.uw, uw_highlights = cur.uw_highlights, uw_strength = cur.uw_strength, defaults = DEFAULTS,
        active = active(), ok = mp.get_property_native("user-data/dlogm/uw_ok") == true, stats = stats,
    })
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
    local t = mp.get_property_number("time-pos")
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

-- The source's input as the workspace plugin has it for the file playing (its own file's `source`), nil: none
local function input()
    local ws = mp.get_property_native("user-data/workspace")
    if type(ws) == "table" and ws.path == mp.get_property("path") and type(ws.input) == "table" then return ws.input end
    return nil
end
-- The source's settings taken as what plays (a key it hasn't: the defaults)
local function take(b)
    b = type(b) == "table" and b or {}
    cur.uw = type(b.uw) == "boolean" and b.uw or DEFAULTS.uw
    cur.uw_highlights = type(b.uw_highlights) == "boolean" and b.uw_highlights or DEFAULTS.uw_highlights
    cur.uw_strength = type(b.uw_strength) == "number" and hundredths(b.uw_strength) or DEFAULTS.uw_strength
end
-- What plays applied: the dlogm plugin's curve when on changed, the shader's numbers
local function apply(was_on)
    if cur.uw ~= was_on then mp.commandv("script-message-to", mp.get_script_name(), "uw-color", cur.uw and "yes" or "no") end
    send()
    publish()
end
-- A change by hand: for viewing, the workspace plugin told (its Source tab's Save writes it into the source)
local function set(k, v)
    local was = cur.uw
    cur[k] = v
    local text = type(v) == "boolean" and (v and "yes" or "no") or string.format("%.2f", v)
    mp.commandv("script-message-to", "workspace", "input-set", k, text)
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

mp.add_key_binding(nil, "uw-toggle", function() mp.commandv("script-message-to", mp.get_script_name(), "uw-set", "uw", "toggle") end)

send()
publish()

local M = {}
-- At a file's start (the dlogm plugin's on_preloaded, before it picks the curve): the source's settings taken;
-- whether the correction is on for it
function M.file_start()
    take(input())
    send()
    publish()
    return cur.uw
end
return M
