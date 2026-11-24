-- Underwater colour for plain SDR clips (GoPro's and the like): the green and brown taken out, the water blue and not
-- purple, the lightness of Action 6's footage under water (research_notes/underwater_color; the rule: estimate.lua).
--
-- Part of the dlogm plugin (required by its main.lua), which shows such a clip through shaders/underwater_hlg.glsl as if it were log (curve "uw"), in its HDR,
-- Dolby Vision or SDR look. This plugin decides how much: every second while it plays (and at once after a seek
-- or a new file) it reads the frame as recorded (ffmpeg, the keyframe at or before the moment), works out the frame's
-- numbers from a 320x180 grid of it, eases them in over a second or two and hands them to the shader
-- (glsl-shader-opts uw_*).
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
local PERIOD = 1.0
local TAU = 1.5              -- seconds the numbers take to follow the frame (about 2/3 of the way)
local KEYS = { "dr", "db", "k", "ev" }

local params = nil           -- the numbers the shader has, eased
local stats = {}
local last_time = nil

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

-- The frame as recorded, measured; `jump`: take its numbers at once (a seek, a new file), else ease towards them.
-- Read by ffmpeg beside mpv (in the background, so playback never waits): the keyframe at or before the playing
-- moment, 960x540. Not mpv's own screenshot: the video is tagged HLG for the shader, and a software screenshot
-- converts it as if it were
local busy = false
local function measure(jump)
    if busy or not active() then return end
    local path = mp.get_property("path")
    local t = mp.get_property_number("time-pos")
    if not path or path:find("://", 1, true) or not t then return end
    busy = true
    local t0 = mp.get_time()
    mp.command_native_async({
        name = "subprocess", playback_only = false, capture_stdout = true, capture_stderr = true,
        args = { "ffmpeg", "-v", "error", "-nostdin", "-noaccurate_seek", "-ss", string.format("%.3f", t), "-skip_frame", "nokey",
                 "-i", path, "-map", "0:v:0", "-frames:v", "1", "-vf", string.format("scale=%d:%d:flags=area", GW * 3, GH * 3),
                 "-f", "rawvideo", "-pix_fmt", "bgr0", "-" },
    }, function(ok, res)
        busy = false
        local data = ok and res and res.stdout
        if not data or #data ~= GW * 3 * GH * 3 * 4 then
            mp.msg.verbose("underwater colour: no frame from ffmpeg " .. tostring(res and res.stderr))
            return
        end
        if not active() then return end
        local grabbed = mp.get_time() - t0
        local R, G, B = E.grid_from_bgr0(data, GW * 3, GH * 3, GW * 3 * 4, GW, GH)
        data = nil
        local p, st = E.estimate(R, G, B, GW, GH)
        local now = mp.get_time()
        if jump or not params or not last_time then
            params = p
        else
            local a = 1 - math.exp(-(now - last_time) / TAU)
            for _, key in ipairs(KEYS) do params[key] = params[key] + (p[key] - params[key]) * a end
        end
        last_time = now
        send()
        stats = {
            under_water = round(st.f), water = round(st.water), red = round(st.red), blue = round(st.blue), blue_water = round(st.blue_water),
            leak = round(st.leak), water_hue = round(st.water_hue, 0), water_hue_after = round(st.water_hue_after, 0),
            water_a = round(st.water_a, 3), water_a_after = round(st.water_a_after, 3), lift = round(st.ev),
            median = round(st.median_y), at = round(t, 1),
            applied = { red = round(params.dr), blue = round(params.db), leak = round(params.k), lift = round(params.ev) },
            ms = math.floor((now - t0) * 1000 + 0.5), grab_ms = math.floor(grabbed * 1000 + 0.5),
        }
        publish()
    end)
end

mp.add_periodic_timer(PERIOD, function()
    if active() and not mp.get_property_native("pause") and not mp.get_property_native("seeking") then measure(false) end
end)
mp.register_event("playback-restart", function() if active() then measure(true) end end)
mp.register_event("file-loaded", function() params, last_time, stats = nil, nil, {}; send(); publish() end)
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
