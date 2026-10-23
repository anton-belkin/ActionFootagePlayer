-- dji-telemetry: what a camera recorded alongside the video (depth under water, pressure, GPS, exposure) shown over
-- it as mpv plays. An mpv plugin: this folder goes into ~~/scripts/.
--
-- The values come from the camera's own metadata track, read whole when the file is loaded (telemetry.lua, plain
-- Lua: 0.1-0.4 s for a few minutes of 4K60). Cameras it reads: DJI Osmo Action 6 (worked out from its recordings)
-- and Osmo 360 (from its schema), and GoPro (GPMF: GPS, speed, exposure). Any other file: nothing happens.
--
-- Fields, shown in the order given, each when the clip has it:
--   depth        under water: the barometer's altitude below the surface. Action 6 records it once it has
--                detected the water, a second or two after going in, in 0.1 m steps, about once a second
--   water        In water / Out of water: the barometer's flag, or the camera's own underwater confidence (it
--                comes from the picture, so it also works in a dive housing, where the barometer can't tell)
--   pressure     hPa. In a dive housing it's the air in the housing, not the depth
--   altitude     the barometer's altitude out of water: relative, and it drifts by meters
--   gps          position, when there's a fix (DJI: a remote or Mimo connected to the camera)
--   speed        ground speed, from the GPS (Osmo 360 on an e-bike: the bike's)
--   temperature  the image sensor's (Osmo 360), not the water's
--   heartrate    from a paired device (Osmo 360)
--   exposure     ISO, shutter, aperture, white balance
--
-- Keys (rebind with `<key> script-binding dji_telemetry/<name>` in input.conf):
--   Ctrl+t  toggle  the overlay off/on, for this file and the next
--
-- Script messages (`script-message-to dji_telemetry <message>`, what the right-click menu runs; mpv/menu.conf):
--   position <corner>          where it shows: top-left, top-right, bottom-left, bottom-right
--   units metric|imperial
--   field <name> [yes|no]      show a field or not (without yes or no: the other way round); a field shown anew
--                              goes after the others
--   remember                   these, and whether it shows, as the defaults in dji_telemetry.conf
-- The state is in user-data/dji_telemetry/ for the menu: available (the file has telemetry it reads), visible,
-- position, units, and shown/<field> for each field.
--
-- Options (~~/script-opts/dji_telemetry.conf):
--   fields=depth,water   which, from the list above
--   units=metric         or imperial: feet, mph, °F
--   position=bottom-left top-left, top-right, bottom-right (top left has mpv's messages, top right the exports'
--                        progress)
--   font_size=28         at a 720-pixel-high window; it scales with the window
--   visible=yes          shown when a file starts; no: only after Ctrl+t

local mp = require 'mp'
local options = require 'mp.options'
local utils = require 'mp.utils'
local telemetry = require 'telemetry'

local o = { fields = "depth,water", units = "metric", position = "bottom-left", font_size = 28, visible = true }
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
    exposure = { values = { "iso", "exposure_time", "f_number", "white_balance_k" }, line = function(at)
        local parts = {}
        local iso, shutter, f, wb = at("iso"), at("exposure_time"), at("f_number"), at("white_balance_k")
        if iso and iso > 0 then parts[#parts + 1] = string.format("ISO %d", math.floor(iso + 0.5)) end
        if shutter then
            local s = shutter[1] / shutter[2]
            parts[#parts + 1] = (shutter[1] == 1 or s < 0.3) and string.format("1/%d", math.floor(shutter[2] / shutter[1] + 0.5))
                or string.format("%.1f s", s)
        end
        if f then parts[#parts + 1] = string.format("f/%.1f", f[1] / f[2]) end
        if wb and wb > 0 then parts[#parts + 1] = string.format("%d K", wb) end
        if #parts == 0 then return nil end
        return table.concat(parts, "   "), nil
    end },
}

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
do
    local names = {}
    for name in o.fields:gmatch("[^,%s]+") do names[#names + 1] = name end
    set_fields(names)
end

local visible = o.visible
local track = nil
local overlay = mp.create_osd_overlay("ass-events")
local shown = nil

-- For the right-click menu (mpv/menu.conf)
local function publish()
    local state = { available = track ~= nil, visible = visible, position = corner, units = imperial and "imperial" or "metric" }
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

local function update()
    local t = mp.get_property_number("time-pos")
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
    local x = (an == 7 or an == 1) and margin or w - margin
    local y = (an == 7 or an == 9) and margin or h - margin
    overlay.res_x, overlay.res_y = w, h
    overlay.data = string.format("{\\an%d\\pos(%d,%d)\\fs%d\\bord%.1f\\shad0\\1c&HFFFFFF&\\3c&H000000&\\3a&H40&}",
        an, x, y, math.floor(o.font_size * scale + 0.5), 1.5 * scale) .. text
    overlay:update()
end

-- The values the file was read for
local read_for = {}

-- The file's telemetry, the values the fields read
local function load()
    track, shown, read_for = nil, nil, {}
    overlay:remove()
    local path = mp.get_property("path")
    if not path or path:find("://", 1, true) or #wanted == 0 then return end
    local ext = (path:match("%.([^./]+)$") or ""):lower()
    if not PROBED_EXTENSIONS[ext] then return end
    for _, v in ipairs(wanted) do read_for[v] = true end
    local err
    track, err = telemetry.read(path, wanted)
    if err then mp.msg.warn("couldn't read the telemetry: " .. tostring(err)) end
    if not track then
        mp.msg.verbose("no telemetry with a known layout")
        return
    end
    mp.msg.verbose("telemetry: " .. track.model)
    update()
end

mp.register_event("file-loaded", function()
    load()
    publish()
end)

mp.register_event("end-file", function()
    track, shown = nil, nil
    overlay:remove()
    publish()
end)

mp.observe_property("time-pos", "number", update)
mp.observe_property("osd-dimensions", "native", update)

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
    visible = true
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
    local saved = save_options({ fields = table.concat(fields, ","), units = imperial and "imperial" or "metric",
                                 position = corner, visible = visible and "yes" or "no" })
    mp.osd_message(saved and "Telemetry: saved as the defaults (dji_telemetry.conf)" or "Telemetry: couldn't write dji_telemetry.conf", 2)
end)
