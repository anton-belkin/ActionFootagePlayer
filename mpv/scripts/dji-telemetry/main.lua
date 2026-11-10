-- dji-telemetry: what a camera recorded alongside the video (depth under water, pressure, GPS, exposure) shown over
-- it as mpv plays. An mpv plugin: this folder goes into ~~/scripts/.
--
-- The values come from the camera's own metadata track, read whole when the file is loaded (telemetry.lua, plain
-- Lua: 0.1-0.4 s for a few minutes of 4K60 on a local disk, ~20 s for 7 GB from an SD card, in steps that leave mpv
-- free meanwhile). Cameras it reads: DJI Osmo Action 6 (worked out from its recordings)
-- and Osmo 360 (from its schema), and GoPro (GPMF: GPS, speed, exposure). DJI's drones (Mini, Air, Mavic): their
-- camera data written as text, the .SRT beside the clip or a text track in it (exposure, GPS, height; the speed
-- from the GPS). Any other file: nothing happens.
--
-- Fields, shown in the order given, each when the clip has it:
--   depth        under water: the barometer's altitude below the surface. Action 6 records it once it has
--                detected the water, a second or two after going in, in 0.1 m steps, about once a second
--   water        In water / Out of water: the barometer's flag, or the camera's own underwater confidence (it
--                comes from the picture, so it also works in a dive housing, where the barometer can't tell)
--   pressure     hPa. In a dive housing it's the air in the housing, not the depth
--   altitude     the barometer's altitude out of water: relative, and it drifts by meters
--   height       a drone's, above where it took off
--   gps          position, when there's a fix (DJI: a remote or Mimo connected to the camera)
--   speed        ground speed, from the GPS (Osmo 360 on an e-bike: the bike's)
--   temperature  the image sensor's (Osmo 360), not the water's
--   heartrate    from a paired device (Osmo 360)
--   exposure     ISO, shutter, aperture, EV (when not 0), white balance
--
-- Subtitles a camera wrote: DJI's drones (Mini, Air, Mavic) write each frame's camera data (ISO, shutter, EV, GPS,
-- height) as a text track in the video, or as an .SRT beside it, which mpv loads too. mpv would show it as subtitles,
-- over the whole picture; here such a track is off when the clip starts, until v shows it (for this file and the
-- next). Other subtitles play as mpv picks them, and v turns them off and on too.
--
-- Keys (rebind with `<key> script-binding dji_telemetry/<name>` in input.conf):
--   Ctrl+t  toggle     the overlay off/on, for this file and the next
--   v       subtitles  the subtitles off/on (mpv's own v only hides or shows the track picked: here one is picked)
--
-- Script messages (`script-message-to dji_telemetry <message>`, what the right-click menu runs; mpv/menu.conf):
--   position <corner>          where it shows: top-left, top-right, bottom-left, bottom-right
--   units metric|imperial
--   field <name> [yes|no]      show a field or not (without yes or no: the other way round); a field shown anew
--                              goes after the others
--   remember                   these, and whether it shows, as the defaults in dji_telemetry.conf (the fields
--                              only once picked: until then each clip gets its own, as without fields=)
-- The state is in user-data/dji_telemetry/ for the menu: available (the file has telemetry it reads), model (the
-- camera's, as its telemetry says), visible,
-- position, units, and shown/<field> for each field; subtitles: { count (the file's subtitle tracks), camera (one is
-- the camera's data), shown (one shows now), camera_shown (that one) }.
--
-- Options (~~/script-opts/dji_telemetry.conf):
--   fields=              which, from the list above. Not set: each clip's own, those of depth, water, gps, speed
--                        it has (DJI: depth and in water; GoPro with a GPS fix: GPS and speed), else exposure (a
--                        GoPro without GPS, as the HERO12). A drone's: height, GPS and speed
--   units=metric         or imperial: feet, mph, °F
--   position=bottom-left top-left, top-right, bottom-right (top left has mpv's messages, top right the exports'
--                        progress)
--   font_size=28         at a 720-pixel-high window; it scales with the window
--   visible=yes          shown when a file starts; no: only after Ctrl+t

local mp = require 'mp'
local options = require 'mp.options'
local utils = require 'mp.utils'

-- Windows: paths past 260 characters (a clip deep in a trip's folders), by the workspace plugin's longpath.lua beside
-- this plugin's folder; without it, as before
do
    package.path = package.path .. ";" .. utils.join_path(utils.join_path(mp.get_script_directory(), "../workspace"), "?.lua")
    local ok, longpath = pcall(require, 'longpath')
    if ok then longpath.install(utils) end
end

-- The clip playing: mpv's path, or the original when the file playing is a moment (the workspace plugin's
-- user-data/workspace/clip, docs/moments-and-workspaces.md)
local function clip_path()
    local path = mp.get_property("path")
    local ws = mp.get_property_native("user-data/workspace")
    if path and type(ws) == "table" and ws.path == path and ws.redirected and ws.clip then return ws.clip end
    return path
end
local telemetry = require 'telemetry'

local o = { fields = "", units = "metric", position = "bottom-left", font_size = 28, visible = true }
options.read_options(o, "dji_telemetry")

local PROBED_EXTENSIONS = { mp4 = true, mov = true, lrf = true, lrv = true }
-- Action 6's underwater confidence goes 0-256; it crosses this a second before the barometer's flag changes
local UNDERWATER_CONFIDENCE = 128
local ALIGN = { ["top-left"] = 7, ["top-right"] = 9, ["bottom-left"] = 1, ["bottom-right"] = 3 }
local imperial = o.units == "imperial"
local corner = ALIGN[o.position] and o.position or "bottom-left"

local function length(m)
    if imperial then return string.format("%.0f ft", m * 3.28084) end
    return string.format("%.1f m", m)
end

local function speed(ms)
    if imperial then return string.format("%.1f mph", ms * 2.23694) end
    return string.format("%.1f km/h", ms * 3.6)
end

local function coordinate(deg, pos, neg)
    return string.format("%.5f° %s", math.abs(deg), deg < 0 and neg or pos)
end

-- Position in degrees, when the GPS has a fix: DJI's (latitude, longitude: radians or degrees, with a status), else
-- GoPro's (latitude_deg, longitude_deg: only with a fix)
local function position(at)
    local status, lat, lon = at("gps_status"), at("latitude"), at("longitude")
    if status == nil or status == 1 or not lat or not lon or (lat == 0 and lon == 0) then
        lat, lon = at("latitude_deg"), at("longitude_deg")
        if not lat or not lon then return nil end
        return lat, lon
    end
    if at("gps_unit") == 0 then lat, lon = math.deg(lat), math.deg(lon) end
    return lat, lon
end

local function in_water(at)
    local flag, confidence = at("in_water"), at("underwater_confidence")
    if flag == nil and confidence == nil then return nil end
    return flag == 1 or (confidence or 0) >= UNDERWATER_CONFIDENCE
end

-- Each field: the telemetry values it reads, and its line (a label and a value, or nil) from at(<value name>)
local FIELDS = {
    depth = { values = { "in_water", "altitude_dm" }, line = function(at)
        local alt = at("altitude_dm")
        if at("in_water") ~= 1 or not alt then return nil end
        return "Depth", length(math.max(0, -alt / 10))
    end },
    water = { values = { "in_water", "underwater_confidence" }, line = function(at)
        local w = in_water(at)
        if w == nil then return nil end
        return w and "In water" or "Out of water", nil
    end },
    pressure = { values = { "pressure_hpa" }, line = function(at)
        local p = at("pressure_hpa")
        if not p or p == 0 then return nil end
        return "Pressure", string.format("%d hPa", p)
    end },
    altitude = { values = { "in_water", "altitude_dm" }, line = function(at)
        -- 0 is also what a clip without the barometer's altitude reads (proto3 defaults)
        local alt = at("altitude_dm")
        if at("in_water") == 1 or not alt or alt == 0 then return nil end
        return "Altitude", length(alt / 10)
    end },
    height = { values = { "height_m" }, line = function(at)
        local m = at("height_m")
        if not m then return nil end
        return "Height", length(m)
    end },
    gps = { values = { "gps_status", "gps_unit", "latitude", "longitude", "latitude_deg", "longitude_deg" }, line = function(at)
        local lat, lon = position(at)
        if not lat then return nil end
        return "GPS", coordinate(lat, "N", "S") .. "  " .. coordinate(lon, "E", "W")
    end },
    speed = { values = { "ebike_speed_kmh", "gps_status", "gps_unit", "latitude", "longitude", "latitude_deg", "longitude_deg",
                         "velocity_x", "velocity_y", "speed_ms" }, line = function(at)
        local bike = at("ebike_speed_kmh")
        if bike and bike > 0 then return "Speed", speed(bike / 3.6) end
        if not position(at) then return nil end
        local vx, vy = at("velocity_x"), at("velocity_y")
        if vx and vy then return "Speed", speed(math.sqrt(vx * vx + vy * vy)) end
        local ms = at("speed_ms")
        if not ms then return nil end
        return "Speed", speed(ms)
    end },
    temperature = { values = { "sensor_temperature_c" }, line = function(at)
        local c = at("sensor_temperature_c")
        if not c or c == 0 then return nil end
        if imperial then return "Sensor", string.format("%.0f °F", c * 9 / 5 + 32) end
        return "Sensor", string.format("%.0f °C", c)
    end },
    heartrate = { values = { "heart_rate_bpm" }, line = function(at)
        local bpm = at("heart_rate_bpm")
        if not bpm or bpm == 0 then return nil end
        return "Heart rate", string.format("%d bpm", bpm)
    end },
    exposure = { values = { "iso", "exposure_time", "f_number", "ev", "white_balance_k" }, line = function(at)
        local parts = {}
        local iso, shutter, f, wb = at("iso"), at("exposure_time"), at("f_number"), at("white_balance_k")
        local ev = at("ev")
        if iso and iso > 0 then parts[#parts + 1] = string.format("ISO %d", math.floor(iso + 0.5)) end
        if shutter then
            local s = shutter[1] / shutter[2]
            parts[#parts + 1] = (shutter[1] == 1 or s < 0.3) and string.format("1/%d", math.floor(shutter[2] / shutter[1] + 0.5))
                or string.format("%.1f s", s)
        end
        if f then parts[#parts + 1] = string.format("f/%.1f", f[1] / f[2]) end
        if ev and ev ~= 0 then parts[#parts + 1] = string.format("EV %+.1f", ev) end
        if wb and wb > 0 then parts[#parts + 1] = string.format("%d K", wb) end
        if #parts == 0 then return nil end
        return table.concat(parts, "   "), nil
    end },
}

-- Without fields=: what a clip shows, those of these it has, and if none, the fallback
local DEFAULT_FIELDS = { "depth", "water", "gps", "speed" }
local FALLBACK_FIELDS = { "exposure" }

-- The fields shown, in order, and the telemetry values they read
local fields, wanted = {}, {}
local function set_fields(names)
    local seen = {}
    fields, wanted = {}, {}
    for _, name in ipairs(names) do
        if FIELDS[name] then
            fields[#fields + 1] = name
            for _, v in ipairs(FIELDS[name].values) do
                if not seen[v] then seen[v] = true; wanted[#wanted + 1] = v end
            end
        else
            mp.msg.warn("unknown field " .. name)
        end
    end
end
-- Whether the fields are each clip's own: fields= isn't set, and none picked in the menu
local per_clip
do
    local names = {}
    for name in o.fields:gmatch("[^,%s]+") do names[#names + 1] = name end
    per_clip = #names == 0
    set_fields(names)
end

local visible = o.visible
local track = nil
local overlay = mp.create_osd_overlay("ass-events")
local shown = nil

-- For the right-click menu (mpv/menu.conf)
local function publish()
    local state = { available = track ~= nil, model = track and track.model or "", visible = visible, position = corner,
                    units = imperial and "imperial" or "metric" }
    for k, v in pairs(state) do mp.set_property_native("user-data/dji_telemetry/" .. k, v) end
    local on = {}
    for _, name in ipairs(fields) do on[name] = true end
    for name in pairs(FIELDS) do mp.set_property_native("user-data/dji_telemetry/shown/" .. name, on[name] == true) end
end

local function text_at(t)
    local function at(name) return telemetry.at(track.values[name], t) end
    local lines = {}
    for _, name in ipairs(fields) do
        local label, value = FIELDS[name].line(at)
        if label then
            lines[#lines + 1] = value and ("{\\1a&H40&}" .. label .. "{\\1a&H00&}  " .. value) or label
        end
    end
    return table.concat(lines, "\\N")
end

-- A moment playing as a video of its own (the workspace plugin's): its time from 0, its part this far into the clip
local function offset()
    local ws = mp.get_property_native("user-data/workspace")
    if type(ws) == "table" and ws.path == mp.get_property("path") and type(ws.offset) == "number" then return ws.offset end
    return 0
end

local function update()
    local t = mp.get_property_number("time-pos")
    if t then t = t + offset() end
    if not visible or not track or not t then
        if shown then overlay:remove(); shown = nil end
        return
    end
    local w, h = mp.get_osd_size()
    if not w or not h or h == 0 then return end
    local text = text_at(t)
    local key = text .. "|" .. w .. "x" .. h
    if key == shown then return end
    shown = key
    if text == "" then overlay:remove(); return end

    local scale = h / 720
    local margin = math.floor(24 * scale)
    local an = ALIGN[corner]
    -- In the video's area: not under the side panel, nor the navigation bar docked below the video
    local function ratio(name) return mp.get_property_number("video-margin-ratio-" .. name) or 0 end
    local x0, x1, y0, y1 = w * ratio("left"), w * (1 - ratio("right")), h * ratio("top"), h * (1 - ratio("bottom"))
    local x = (an == 7 or an == 1) and x0 + margin or x1 - margin
    local y = (an == 7 or an == 9) and y0 + margin or y1 - margin
    overlay.res_x, overlay.res_y = w, h
    overlay.data = string.format("{\\an%d\\pos(%d,%d)\\fs%d\\bord%.1f\\shad0\\1c&HFFFFFF&\\3c&H000000&\\3a&H40&}",
        an, x, y, math.floor(o.font_size * scale + 0.5), 1.5 * scale) .. text
    overlay:update()
end

-- The values the file was read for
local read_for = {}

-- Whether a field shows anything in the clip: its line at each time one of its values changes
local function has(name)
    for _, v in ipairs(FIELDS[name].values) do
        local series = track.values[v]
        for _, t in ipairs(series and series.t or {}) do
            if FIELDS[name].line(function(n) return telemetry.at(track.values[n], t) end) then return true end
        end
    end
    return false
end

-- The clip's own fields: the defaults it has, else the fallbacks it has
local function own_fields()
    for _, names in ipairs({ track.defaults or DEFAULT_FIELDS, FALLBACK_FIELDS }) do
        local found = {}
        for _, name in ipairs(names) do
            if has(name) then found[#found + 1] = name end
        end
        if #found > 0 then return found end
    end
    return {}
end

-- The read under way: { close } (telemetry.reader)
local reading = nil
-- Reading for this long at a time, then the script's events (quitting among them) before it goes on
local STEP_TIME = 0.02

local function stop_reading()
    if reading then reading.close() end
    reading = nil
end

-- A drone's: its camera data as text, the .SRT beside the clip (mpv loads it as subtitles too) or a text track in it
local function text_telemetry(path)
    local base = path:match("^(.*)%.[^./\\]+$") or path
    local content = nil
    for _, ext in ipairs({ ".SRT", ".srt" }) do
        local f = io.open(base .. ext, "rb")
        if f then
            content = f:read("*a")
            f:close()
            break
        end
    end
    local entries = nil
    if content and telemetry.camera_text(content:sub(1, 4096)) then entries = telemetry.parse_srt(content) end
    if not entries then
        for index, text in pairs(telemetry.text_samples(path, 3) or {}) do
            if telemetry.camera_text(text) then
                entries = telemetry.text_track(path, index)
                break
            end
        end
    end
    if not entries or #entries == 0 then return nil end
    local encoder = mp.get_property("metadata/by-key/encoder") or ""
    return telemetry.from_text(entries, encoder:find("^DJI") and encoder or nil, coroutine.yield)
end

-- The file's telemetry, the values the fields read. In steps: a long clip from an SD card takes ~20 s
local function load()
    stop_reading()
    track, shown, read_for = nil, nil, {}
    overlay:remove()
    -- Each clip's own fields are picked once it's read: it's read for all the ones it could have
    if per_clip then
        local names = {}
        for _, name in ipairs(DEFAULT_FIELDS) do names[#names + 1] = name end
        for _, name in ipairs(FALLBACK_FIELDS) do names[#names + 1] = name end
        set_fields(names)
    end
    local path = clip_path()
    if not path or path:find("://", 1, true) or #wanted == 0 then return end
    local ext = (path:match("%.([^./]+)$") or ""):lower()
    if not PROBED_EXTENSIONS[ext] then return end
    for _, v in ipairs(wanted) do read_for[v] = true end
    local step, close = telemetry.reader(path, wanted)
    local this = { close = close }
    -- Read from a drone's text instead, once the file has no telemetry track with a known layout
    local from_text = false
    reading = this
    local function continue()
        if reading ~= this then return end
        local start = mp.get_time()
        local finished, result, err
        repeat finished, result, err = step() until finished or mp.get_time() - start > STEP_TIME
        if not finished then return mp.add_timeout(0, continue) end
        reading = nil
        if err then mp.msg.warn("couldn't read the telemetry: " .. tostring(err)) end
        if not result and not from_text then
            -- A drone's text: read whole, every value in it, in steps as well
            local co = coroutine.create(function() return text_telemetry(path) end)
            step = function()
                local ok, value = coroutine.resume(co)
                if not ok then return true, nil, value end
                if coroutine.status(co) == "dead" then return true, value end
                return false
            end
            from_text, reading = true, this
            return continue()
        end
        if from_text and result then
            for _, f in pairs(FIELDS) do
                for _, v in ipairs(f.values) do read_for[v] = true end
            end
        end
        if not result then
            mp.msg.verbose("no telemetry with a known layout")
            return
        end
        track = result
        if per_clip then set_fields(own_fields()) end
        mp.msg.verbose("telemetry: " .. track.model .. ", showing " .. table.concat(fields, ","))
        update()
        publish()
    end
    continue()
end

mp.register_event("file-loaded", function()
    load()
    publish()
end)

mp.register_event("end-file", function()
    stop_reading()
    track, shown = nil, nil
    overlay:remove()
    publish()
end)

mp.observe_property("time-pos", "number", update)
mp.observe_property("osd-dimensions", "native", update)
for _, side in ipairs({ "left", "right", "top", "bottom" }) do
    mp.observe_property("video-margin-ratio-" .. side, "number", function() shown = nil; update() end)
end

-- Subtitle tracks the camera wrote, by mpv's track id, for the file playing
local camera_subs = {}
-- The camera's text shown: asked for with v, for this file and the next
local camera_text_wanted = false

local function sub_tracks()
    local list = {}
    for _, tr in ipairs(mp.get_property_native("track-list") or {}) do
        if tr.type == "sub" then list[#list + 1] = tr end
    end
    return list
end

local function publish_subtitles()
    local subs = sub_tracks()
    local cur = mp.get_property_native("current-tracks/sub")
    local shown = type(cur) == "table" and mp.get_property_native("sub-visibility") == true
    mp.set_property_native("user-data/dji_telemetry/subtitles", {
        count = #subs, camera = next(camera_subs) ~= nil, shown = shown, camera_shown = shown and camera_subs[cur.id] == true })
end

-- A track's first lines: an external file's start, or the samples of a text track in an MP4 or MOV
local function first_text(tr, path, embedded)
    if tr.external then
        local name = tr["external-filename"]
        if not name or name:find("://", 1, true) then return nil end
        local f = io.open(name, "rb")
        if not f then return nil end
        local text = f:read(4096)
        f:close()
        return text
    end
    return embedded and embedded[tr["ff-index"]]
end

-- Before mpv picks the tracks: which subtitles are the camera's, and none of those picked unless asked for. mpv would
-- pick them: an MP4's tracks are all marked default, and an .SRT beside the clip is loaded with it
mp.add_hook("on_preloaded", 50, function()
    camera_subs = {}
    local subs = sub_tracks()
    if #subs > 0 then
        local path = mp.get_property("path")
        local ext = path and not path:find("://", 1, true) and (path:match("%.([^./\\]+)$") or ""):lower()
        local embedded = nil
        for _, tr in ipairs(subs) do
            if not tr.external and not embedded and PROBED_EXTENSIONS[ext] then embedded = telemetry.text_samples(path, 3) or {} end
            if telemetry.camera_text(first_text(tr, path, embedded)) then camera_subs[tr.id] = true end
        end
        local others = false
        for _, tr in ipairs(subs) do others = others or not camera_subs[tr.id] end
        if next(camera_subs) and not others and not camera_text_wanted and mp.get_property("sid") == "auto" then
            mp.set_property("file-local-options/sid", "no")
        end
        if next(camera_subs) then mp.msg.verbose("the camera's data as subtitles: off until v") end
    end
    publish_subtitles()
end)

mp.register_event("file-loaded", function()
    -- Real subtitles beside the camera's: the camera's off, should mpv have picked them
    local cur = mp.get_property_native("current-tracks/sub")
    if type(cur) == "table" and camera_subs[cur.id] and not camera_text_wanted then mp.set_property("sid", "no") end
    publish_subtitles()
end)
mp.observe_property("current-tracks/sub", "native", publish_subtitles)
mp.observe_property("sub-visibility", "native", publish_subtitles)

mp.add_key_binding("v", "subtitles", function()
    local subs = sub_tracks()
    if #subs == 0 then
        mp.osd_message("Subtitles: none in this file", 1.5)
        return
    end
    local cur = mp.get_property_native("current-tracks/sub")
    if type(cur) == "table" and mp.get_property_native("sub-visibility") then
        mp.set_property("sid", "no")
        if camera_subs[cur.id] then camera_text_wanted = false end
        mp.osd_message("Subtitles: off", 1.5)
    else
        -- The track picked, else the first: a film's own before the camera's
        local id = type(cur) == "table" and cur.id or nil
        if not id then
            for _, tr in ipairs(subs) do
                if not id and not camera_subs[tr.id] then id = tr.id end
            end
            id = id or subs[1].id
        end
        mp.set_property_native("sub-visibility", true)
        mp.set_property_native("sid", id)
        if camera_subs[id] then camera_text_wanted = true end
        mp.osd_message(camera_subs[id] and "Subtitles: on (the camera's data)" or "Subtitles: on", 1.5)
    end
    publish_subtitles()
end)

mp.add_key_binding("Ctrl+t", "toggle", function()
    if not track then
        mp.osd_message("Telemetry: none in this file", 1.5)
        return
    end
    visible = not visible
    update()
    publish()
    mp.osd_message(visible and "Telemetry: on" or "Telemetry: off", 1.5)
end)

local CORNER_LABELS = { ["top-left"] = "top left", ["top-right"] = "top right", ["bottom-left"] = "bottom left", ["bottom-right"] = "bottom right" }

mp.register_script_message("position", function(where)
    if not ALIGN[where] then return end
    corner, shown, visible = where, nil, true
    update()
    publish()
    mp.osd_message("Telemetry: " .. CORNER_LABELS[where], 1.5)
end)

mp.register_script_message("units", function(units)
    if units ~= "metric" and units ~= "imperial" then return end
    imperial, shown = units == "imperial", nil
    update()
    publish()
    mp.osd_message("Telemetry: " .. units, 1.5)
end)

mp.register_script_message("field", function(name, on)
    if not FIELDS[name] then return end
    local names, has = {}, false
    for _, f in ipairs(fields) do
        if f == name then has = true else names[#names + 1] = f end
    end
    if on == "yes" or (on ~= "no" and not has) then
        if has then return end
        names[#names + 1] = name
    elseif not has then
        return
    end
    set_fields(names)
    per_clip, visible = false, true
    -- Values it didn't read are read now: the file is read again
    local reread = false
    for _, v in ipairs(wanted) do
        if not read_for[v] then reread = true end
    end
    if reread then load() else shown = nil; update() end
    publish()
end)

-- Sets options in ~~/script-opts/dji_telemetry.conf, the rest of the file as it was; false when it can't be written
local function save_options(values)
    local path = mp.command_native({ "expand-path", "~~/script-opts/dji_telemetry.conf" })
    local lines, done = {}, {}
    local f = io.open(path, "r")
    if f then
        for line in f:lines() do
            local key = line:match("^%s*([%w_%-]+)%s*=")
            if not (key and values[key]) then
                table.insert(lines, line)
            elseif not done[key] then
                table.insert(lines, key .. "=" .. values[key])
                done[key] = true
            end
        end
        f:close()
    end
    for key, value in pairs(values) do
        if not done[key] then table.insert(lines, key .. "=" .. value) end
    end
    local dir = path:match("^(.*)[/\\]")
    if dir and not utils.file_info(dir) then
        local windows = package.config:sub(1, 1) == "\\"
        mp.command_native({ name = "subprocess", playback_only = false,
                            args = windows and { "cmd", "/d", "/c", "mkdir", dir } or { "mkdir", "-p", dir } })
    end
    f = io.open(path, "w")
    if not f then return false end
    f:write(table.concat(lines, "\n") .. "\n")
    f:close()
    return true
end

mp.register_script_message("remember", function()
    local saved = save_options({ fields = per_clip and "" or table.concat(fields, ","),
                                 units = imperial and "imperial" or "metric", position = corner,
                                 visible = visible and "yes" or "no" })
    mp.osd_message(saved and "Telemetry: saved as the defaults (dji_telemetry.conf)" or "Telemetry: couldn't write dji_telemetry.conf", 2)
end)
