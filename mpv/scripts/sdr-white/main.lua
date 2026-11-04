-- sdr-white: how bright macOS takes external SDR monitors' white to be, which decides how dark HDR shows on them.
--
-- macOS (Apple Silicon) shows HDR video on a screen by its SDR white in nits: an HDR pixel of n nits shows at n / white
-- of the screen's white, what's above white compressed into it. For an external monitor whose brightness it doesn't
-- know it assumes one: 200 nits on the first external display controller, none on the second. On an SDR monitor that
-- puts HDR's mid-tones (the hdr and dv looks, QuickTime, Safari) a stop lower than BT.2408 or NBCU's conversion; with
-- 100 they show as NBCU's. The helper beside this (sdr-white, macos/sdr-white.swift) reads and sets the value; it's
-- the system's, for every app, until the monitor is plugged in again or the Mac restarts, so it's set again then.
-- Only external screens without HDR headroom are touched, never the built-in display.
--
-- Modes (`mode` in ~~/script-opts/sdr_white.conf, or the side panel's SDR on macOS):
--   ask      as macOS has it; the first time HDR plays on an SDR screen it assumes 200 nits for, offer light
--   macos    as macOS has it (what was set here is put back)
--   light    100 nits: HDR as NBCU's conversion shows it, for a lit room
--   auto     by the MacBook's light sensor: 100 at 250 lux and more, up to 150 in the dark (lid closed: light)
--   manual   `manual`, 0 (darker) to 1 (lighter), from 250 down to 60 nits
--
-- Script messages (`script-message-to sdr_white <message>`):
--   mode <mode>       as above, kept in sdr_white.conf
--   manual <0..1>     the slider, kept too
--   apply             read the screens and set them again
-- State in user-data/sdr_white: available, mode, manual, white (what's set, nits; 0: macOS's), lux (or nil), screens
-- ({ name, white, sdr, original }).

local mp = require 'mp'
local options = require 'mp.options'
local utils = require 'mp.utils'

local o = { mode = "ask", manual = 0.64 }
options.read_options(o, "sdr_white")

if (mp.get_property("platform") or "") ~= "darwin" then return end
local dir = debug.getinfo(1, "S").source:match("^@(.*)/")
local helper = dir and dir .. "/sdr-white"
if not helper or not utils.file_info(helper) then return end

local MODES = { ask = true, macos = true, light = true, auto = true, manual = true }
local LIGHT, DARK, LIGHTEST, DARKEST = 100, 150, 60, 250
local mode = MODES[o.mode] and o.mode or "ask"
local manual = math.min(1, math.max(0, tonumber(o.manual) or 0.64))
local screens, lux, white = {}, nil, 0
local asked = false
-- Whether the helper runs here (Apple Silicon; not on an Intel Mac): the side panel shows the part once it has
local works = false

local function publish()
    if not works then return end
    mp.set_property_native("user-data/sdr_white", { available = true, mode = mode, manual = manual, white = white,
                                                    lux = lux, screens = screens })
end

-- The value a mode wants, nits; nil: macOS's own
local function wanted()
    if mode == "light" then return LIGHT end
    if mode == "manual" then return DARKEST * (LIGHTEST / DARKEST) ^ manual end
    if mode == "auto" then
        if not lux then return LIGHT end
        -- In log lux: 10 lux and less is the dark end, 250 and more the light one
        local t = math.min(1, math.max(0, (math.log(math.max(lux, 1), 10) - 1) / (math.log(250, 10) - 1)))
        return DARK * (LIGHT / DARK) ^ t
    end
    return nil
end

local function run(args, done)
    mp.command_native_async({ name = "subprocess", args = args, playback_only = false, capture_stdout = true },
        function(_, r)
            local j = r and r.status == 0 and utils.parse_json(r.stdout or "")
            if j then
                works = true
                screens = j.screens or {}
                lux = type(j.lux) == "number" and j.lux or nil
            end
            publish()
            if done then done(j) end
        end)
end

-- The SDR screens' white as the mode wants it: set when it's off by more than 3 % (the sensor moves all the time;
-- each write changes how HDR shows), or put back to macOS's
local function apply()
    run({ helper }, function(j)
        if not j then return end
        local want = wanted()
        if not want then
            white = 0
            for _, s in ipairs(screens) do
                if s.original then return run({ helper, "restore" }) end
            end
            return
        end
        white = want
        for _, s in ipairs(screens) do
            if s.sdr and math.abs(s.white - want) / want > 0.03 then
                return run({ helper, "set", string.format("%.1f", want) })
            end
        end
        publish()
    end)
end

local function save()
    local path = mp.command_native({ "expand-path", "~~/script-opts/sdr_white.conf" })
    local d = path:match("^(.*)/")
    if d and not utils.file_info(d) then
        mp.command_native({ name = "subprocess", playback_only = false, args = { "mkdir", "-p", d } })
    end
    local f = io.open(path, "w")
    if not f then return mp.msg.warn("can't write " .. path) end
    f:write(string.format("mode=%s\nmanual=%.3f\n", mode, manual))
    f:close()
end

local function set_mode(m)
    if not MODES[m] then return end
    mode = m
    save()
    apply()
end

-- The offer: HDR shown by macOS (dlogm's hdr or dv look) on an SDR screen it takes for 200 nits
local DIALOG = [[
on run {theMessage}
    activate
    return button returned of (display dialog theMessage buttons {"Don't Ask Again", "Not Now", "Brighten"} ¬
        default button "Brighten" with title "ActionFootagePlayer" with icon note)
end run]]

local function offer()
    if mode ~= "ask" or asked then return end
    local d = mp.get_property_native("user-data/dlogm")
    if not d or d.screen ~= "sdr" or (d.look ~= "hdr" and d.look ~= "dv") then return end
    run({ helper }, function(j)
        if not j or asked then return end
        local dark = false
        for _, s in ipairs(screens) do dark = dark or (s.sdr and s.white >= 150) end
        if not dark then return end
        asked = true
        local paused = mp.get_property_native("pause")
        mp.set_property_native("pause", true)
        local text = "macOS shows HDR on this SDR monitor as if it were 200 nits bright: about a stop darker than "
            .. "the HDR standards' conversion to SDR.\n\nBrighten it to how a lit room should see it? It's for every app "
            .. "(QuickTime, Safari too), set again whenever the monitor is plugged in. The side panel's SDR on macOS "
            .. "has the other settings."
        mp.command_native_async({ name = "subprocess", playback_only = false, capture_stdout = true,
                                  args = { "osascript", "-e", DIALOG, text } }, function(_, r)
            mp.set_property_native("pause", paused)
            local choice = r and (r.stdout or ""):gsub("%s+$", "") or ""
            if choice == "Brighten" then set_mode("light")
            elseif choice == "Don't Ask Again" then set_mode("macos") end
        end)
    end)
end

mp.register_script_message("mode", set_mode)
-- The slider sends many while it's dragged: shown at once, set and saved once it rests for 0.15 s
local settle = nil
mp.register_script_message("manual", function(v)
    manual = math.min(1, math.max(0, tonumber(v) or manual))
    mode = "manual"
    white = wanted()
    publish()
    if settle then settle:kill() end
    settle = mp.add_timeout(0.15, function()
        save()
        apply()
    end)
end)
mp.register_script_message("apply", apply)

-- A monitor plugged in gets macOS's value again; the sensor moves: read again on a screen change and every 10 s
mp.observe_property("display-names", "native", function() apply() end)
mp.add_periodic_timer(10, function() if mode ~= "ask" and mode ~= "macos" then apply() end end)
mp.observe_property("user-data/dlogm/look", "native", function() offer() end)
mp.observe_property("user-data/dlogm/screen", "native", function() offer() end)
