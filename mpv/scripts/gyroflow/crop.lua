-- Crop mode (c): a frame over the video to choose a still or a short clip by, the picture moved under it.
--
-- A drag, or a two-finger scroll on a trackpad, pans; the mouse wheel, or a pinch, zooms; Alt+drag or a two-finger
-- rotation turns the horizon. Where the gyroflow plugin stabilizes the clip, pan is the camera turned, not the picture
-- slid: the plugin renders it again with the camera's yaw and pitch after the smoothing (gyroflow-core's
-- view_rotation), and zoom is its FOV, through the lens's own projection. The frame sits in the middle at MARGIN of
-- the video, with what's around it dimmed, so the view is rendered that much wider than the frame (zoom 1 / MARGIN to
-- start with: the frame shows what the screen showed). Elsewhere it's mpv's video-zoom and video-pan.
--
-- The camera goes to the plugin through the edges control file (main.lua's write_edge_control, `camera=`), which it
-- reads within 50 ms while playing. Paused, mpv seeks to the frame on screen again to have it rendered anew; until
-- that's through, mpv's video-zoom and video-pan move the frame that's there by about as much (exact in the middle),
-- from the focal length the plugin writes next to the control file (Edges::write_camera_info).
--
-- Keys while it's on: a / Shift+a the frame's aspect, double click or Backspace back to the start, c or Esc leaves
-- (the camera as the plugin stabilizes it again). The frame's state is published as user-data/gyroflow/crop for the
-- export.
--
-- On macOS the app's mpv sends the trackpad's gestures as script messages while it's on (--macos-gesture-messages,
-- tools/mpv-patches/0008), so a two-finger scroll pans and a mouse wheel zooms. Elsewhere a wheel's whole clicks zoom
-- and the fractions a touchpad sends pan, and Ctrl+wheel (a pinch, on Windows) zooms.
--
-- Loaded by main.lua, which says whether the plugin stabilizes the clip and writes the control file (init).

local mp = require 'mp'
local utils = require 'mp.utils'

local M = {}

-- The frame's share of the video, at most, either way
local MARGIN = 0.8
local ASPECTS = {
    { name = "original" },
    { name = "16:9", ratio = 16 / 9 },
    { name = "4:3", ratio = 4 / 3 },
    { name = "1:1", ratio = 1 },
    { name = "4:5", ratio = 4 / 5 },
    { name = "3:4", ratio = 3 / 4 },
    { name = "9:16", ratio = 9 / 16 },
}
-- A wheel's click zooms by this much; a pinch's magnification as it comes
local WHEEL_ZOOM = 1.1
local ZOOM_MIN, ZOOM_MAX = 0.15, 4
-- A touchpad scroll's unit (mpv's wheel scale), in screen pixels, off macOS
local SCROLL_PX = 40
-- How the camera turns for the picture to move: view_rotation's yaw moves it left, pitch up, roll counterclockwise
local YAW_SIGN, PITCH_SIGN, ROLL_SIGN = -1, -1, 1
-- The control file at most this often while dragging, s
local WRITE_EVERY = 1 / 30
-- Before the plugin has said, a focal length for a 90° wide picture
local DEFAULT_FX = 0.5

local hooks = nil
local active = false
local aspect = 1
-- The camera asked for, and the one the paused frame on screen was rendered with
local cam = nil
local shown = nil
-- Without the plugin: mpv's own zoom (log2) and pan
local flat = nil
local overlay = mp.create_osd_overlay("ass-events")
-- A seek to the paused frame under way, and whether the camera changed since it was asked
local refreshing = nil
local dirty = false
local write_timer = nil
local last_write = 0
-- The mouse held: its last position, and whether it turns the horizon (Alt)
local drag = nil
local saved = {}

local function default_camera()
    return { yaw = 0, pitch = 0, roll = 0, zoom = 1 / MARGIN }
end

local function stabilizing()
    return hooks ~= nil and hooks.stabilizing()
end

-- What the plugin wrote: the focal length in output widths and heights at zoom 1
local function focal()
    local f = hooks and io.open(hooks.control_path() .. ".camera", "r")
    if not f then return DEFAULT_FX, DEFAULT_FX * 16 / 9 end
    local info = utils.parse_json(f:read("*a") or "")
    f:close()
    if not info or not info.fx or not info.zoom then return DEFAULT_FX, DEFAULT_FX * 16 / 9 end
    return info.fx * info.zoom, info.fy * info.zoom
end

-- The video's rectangle in the window as it'd be without mpv's zoom and pan (those are the preview's): the window less
-- the margins (the side panel's), the video's aspect kept
local function video_rect()
    local d = mp.get_property_native("osd-dimensions")
    local params = mp.get_property_native("video-out-params")
    if not params or not params.dw or params.dw <= 0 then return nil end
    -- No OSD (vo=image): the video's own size
    if not d or d.w <= 0 then d = { w = params.dw, h = params.dh } end
    local function ratio(name) return mp.get_property_number("video-margin-ratio-" .. name) or 0 end
    local x0, x1 = d.w * ratio("left"), d.w * (1 - ratio("right"))
    local y0, y1 = d.h * ratio("top"), d.h * (1 - ratio("bottom"))
    local aw, ah = x1 - x0, y1 - y0
    local va = params.dw / params.dh
    local w, h = aw, aw / va
    if h > ah then w, h = ah * va, ah end
    return { x = x0 + (aw - w) / 2, y = y0 + (ah - h) / 2, w = w, h = h, osd_w = d.w, osd_h = d.h, aspect = va }
end

-- The frame: centered, at most MARGIN of the video either way
local function frame_rect(v)
    local ratio = ASPECTS[aspect].ratio or v.aspect
    local w, h = v.w * MARGIN, v.w * MARGIN / ratio
    if h > v.h * MARGIN then w, h = v.h * MARGIN * ratio, v.h * MARGIN end
    return { x = v.x + (v.w - w) / 2, y = v.y + (v.h - h) / 2, w = w, h = h }
end

local function rect_path(x0, y0, x1, y1)
    return string.format("m %d %d l %d %d l %d %d l %d %d", x0, y0, x1, y0, x1, y1, x0, y1)
end

local function draw()
    if not active then overlay:remove(); return end
    local v = video_rect()
    if not v then return end
    local r = frame_rect(v)
    local W, H = v.osd_w, v.osd_h
    local x0, y0, x1, y1 = r.x, r.y, r.x + r.w, r.y + r.h
    local dim = "{\\an7\\pos(0,0)\\bord0\\shad0\\1c&H000000&\\1a&H50&\\p1}"
    local lines = {
        dim .. rect_path(0, 0, W, y0),
        dim .. rect_path(0, y1, W, H),
        dim .. rect_path(0, y0, x0, y1),
        dim .. rect_path(x1, y0, W, y1),
    }
    -- The frame, and its thirds
    local edge = "{\\an7\\pos(0,0)\\bord0\\shad0\\1c&HFFFFFF&\\1a&H20&\\p1}"
    local t = math.max(1, math.floor(H / 600 + 0.5))
    table.insert(lines, edge .. rect_path(x0 - t, y0 - t, x1 + t, y0) .. " " .. rect_path(x0 - t, y1, x1 + t, y1 + t) .. " "
        .. rect_path(x0 - t, y0, x0, y1) .. " " .. rect_path(x1, y0, x1 + t, y1))
    local third = "{\\an7\\pos(0,0)\\bord0\\shad0\\1c&HFFFFFF&\\1a&HA0&\\p1}"
    local parts = {}
    for i = 1, 2 do
        local x = x0 + r.w * i / 3
        local y = y0 + r.h * i / 3
        table.insert(parts, rect_path(x, y0, x + 1, y1))
        table.insert(parts, rect_path(x0, y, x1, y + 1))
    end
    table.insert(lines, third .. table.concat(parts, " "))
    -- What it is, above the frame
    local fs = math.max(12, math.floor(H / 45))
    local label
    if cam then
        label = string.format("%s · zoom %.0f %% · yaw %.1f° pitch %.1f° roll %.1f°", ASPECTS[aspect].name,
                              100 / (cam.zoom * MARGIN), cam.yaw, cam.pitch, cam.roll)
    else
        label = string.format("%s · zoom %.0f %%", ASPECTS[aspect].name, 100 * 2 ^ flat.zoom)
    end
    table.insert(lines, string.format("{\\an1\\pos(%d,%d)\\fs%d\\bord1.5\\shad0\\1c&HFFFFFF&\\3c&H000000&}%s{\\1a&H60&}   a aspect · double click reset · c leave",
                                      x0, y0 - fs * 0.4, fs, label))
    overlay.res_x, overlay.res_y = W, H
    overlay.data = table.concat(lines, "\n")
    overlay.z = -10
    overlay:update()
end

local function publish()
    if not active then
        mp.set_property_native("user-data/gyroflow/crop", { active = false })
        return
    end
    local v = video_rect()
    local r = v and frame_rect(v)
    mp.set_property_native("user-data/gyroflow/crop", {
        active = true, aspect = ASPECTS[aspect].name,
        -- The frame in the video, as shares of its width and height
        frame = r and { x = (r.x - v.x) / v.w, y = (r.y - v.y) / v.h, w = r.w / v.w, h = r.h / v.h } or nil,
        camera = cam, flat = flat,
    })
end

-- The paused frame moved by mpv to about where the camera asked for puts it, from the one it was rendered with
local function preview()
    if not cam then return end
    local paused = mp.get_property_native("pause")
    if not paused or not shown then
        mp.set_property_number("video-zoom", 0)
        mp.set_property_number("video-pan-x", 0)
        mp.set_property_number("video-pan-y", 0)
        return
    end
    local fx, fy = focal()
    local r = shown.zoom / cam.zoom
    fx, fy = fx / cam.zoom, fy / cam.zoom
    mp.set_property_number("video-zoom", math.log(r) / math.log(2))
    mp.set_property_number("video-pan-x", YAW_SIGN * fx * math.tan(math.rad(cam.yaw - shown.yaw)))
    mp.set_property_number("video-pan-y", PITCH_SIGN * fy * math.tan(math.rad(cam.pitch - shown.pitch)))
end

local function same(a, b)
    return a and b and a.yaw == b.yaw and a.pitch == b.pitch and a.roll == b.roll and a.zoom == b.zoom
end

-- Paused: mpv seeks to the frame again, which sets the filter up again, which reads the control file
local function refresh()
    if not mp.get_property_native("pause") or same(cam, shown) then return end
    if refreshing then dirty = true; return end
    local t = mp.get_property_number("time-pos")
    if not t then return end
    refreshing = { yaw = cam.yaw, pitch = cam.pitch, roll = cam.roll, zoom = cam.zoom }
    dirty = false
    mp.msg.debug(string.format("refresh: yaw %.3f pitch %.3f zoom %.3f", cam.yaw, cam.pitch, cam.zoom))
    mp.commandv("seek", tostring(t), "absolute+exact")
end

local function write_now()
    if write_timer then write_timer:kill(); write_timer = nil end
    last_write = mp.get_time()
    hooks.write()
    refresh()
end

-- The camera changed: the preview at once, the control file at most every WRITE_EVERY
local function changed()
    if cam then
        preview()
        local wait = WRITE_EVERY - (mp.get_time() - last_write)
        if wait <= 0 then write_now()
        elseif not write_timer then write_timer = mp.add_timeout(wait, write_now) end
    else
        mp.set_property_number("video-zoom", flat.zoom)
        mp.set_property_number("video-pan-x", flat.x)
        mp.set_property_number("video-pan-y", flat.y)
    end
    draw()
    publish()
end

-- The picture moved by dx, dy screen pixels
local function pan(dx, dy)
    local v = video_rect()
    if not v then return end
    local u, w = dx / v.w, dy / v.h
    if cam then
        local fx, fy = focal()
        fx, fy = fx / cam.zoom, fy / cam.zoom
        cam.yaw = math.max(-180, math.min(180, cam.yaw + YAW_SIGN * math.deg(math.atan(u / fx))))
        cam.pitch = math.max(-89, math.min(89, cam.pitch + PITCH_SIGN * math.deg(math.atan(w / fy))))
    else
        local s = 2 ^ flat.zoom
        flat.x, flat.y = flat.x + u / s, flat.y + w / s
    end
    changed()
end

-- Zoomed in by `factor` (above 1: closer)
local function zoom(factor)
    if cam then
        cam.zoom = math.max(ZOOM_MIN, math.min(ZOOM_MAX, cam.zoom / factor))
    else
        flat.zoom = math.max(-1, math.min(3, flat.zoom + math.log(factor) / math.log(2)))
    end
    changed()
end

local function roll(degrees)
    if not cam then return end
    cam.roll = cam.roll + ROLL_SIGN * degrees
    if cam.roll > 180 then cam.roll = cam.roll - 360 elseif cam.roll < -180 then cam.roll = cam.roll + 360 end
    changed()
end

local function reset()
    if cam then cam = default_camera() else flat = { zoom = 0, x = 0, y = 0 } end
    changed()
end

-- The mouse: a drag pans, Alt+drag turns the horizon
local function button(alt)
    return function(info)
        if info.event == "down" then
            local pos = mp.get_property_native("mouse-pos")
            if pos then drag = { x = pos.x, y = pos.y, alt = alt } end
        elseif info.event == "up" then
            drag = nil
            if cam then write_now() end
        end
    end
end

local function on_mouse(_, pos)
    if not drag or not pos then return end
    local dx, dy = pos.x - drag.x, pos.y - drag.y
    drag.x, drag.y = pos.x, pos.y
    if drag.alt then
        local v = video_rect()
        -- To the right: clockwise
        if v then roll(-dx / v.w * 90) end
    else
        pan(dx, dy)
    end
end

local gestures = false

local function whole(scale)
    return scale and scale >= 1 and math.abs(scale - math.floor(scale + 0.5)) < 1e-6
end

-- A wheel: up zooms in. A touchpad's fractions pan where the trackpad's gestures don't come as messages
local function wheel(dx, dy, ctrl)
    return function(info)
        if info.event == "up" then return end
        local scale = info.scale or 1
        if dx == 0 and (gestures or ctrl or whole(scale)) then
            return zoom(WHEEL_ZOOM ^ (dy * scale))
        end
        pan(dx * scale * SCROLL_PX, dy * scale * SCROLL_PX)
    end
end

mp.register_script_message("gesture", function(kind, a, b, c)
    if not active then return end
    if kind == "scroll" then
        local s = mp.get_property_number("display-hidpi-scale") or 1
        pan((tonumber(a) or 0) * s, (tonumber(b) or 0) * s)
    elseif kind == "magnify" then
        zoom(1 + (tonumber(a) or 0))
    elseif kind == "rotate" then
        -- Counterclockwise as the fingers turn
        roll(tonumber(a) or 0)
    end
end)

-- Named apart from the script messages: mpv registers a binding's name as a script message too
local BINDINGS = {
    { "MBTN_LEFT", "crop-key-drag", button(false), { complex = true } },
    { "Alt+MBTN_LEFT", "crop-key-roll", button(true), { complex = true } },
    { "MBTN_LEFT_DBL", "crop-key-reset-dbl", reset },
    { "BS", "crop-key-reset", reset },
    { "WHEEL_UP", "crop-key-wheel-up", wheel(0, 1), { complex = true } },
    { "WHEEL_DOWN", "crop-key-wheel-down", wheel(0, -1), { complex = true } },
    { "WHEEL_LEFT", "crop-key-wheel-left", wheel(1, 0), { complex = true } },
    { "WHEEL_RIGHT", "crop-key-wheel-right", wheel(-1, 0), { complex = true } },
    { "Ctrl+WHEEL_UP", "crop-key-pinch-in", wheel(0, 1, true), { complex = true } },
    { "Ctrl+WHEEL_DOWN", "crop-key-pinch-out", wheel(0, -1, true), { complex = true } },
    { "a", "crop-key-aspect", function() aspect = aspect % #ASPECTS + 1; draw(); publish() end },
    { "A", "crop-key-aspect-back", function() aspect = (aspect - 2) % #ASPECTS + 1; draw(); publish() end },
    { "ESC", "crop-key-leave", function() M.set(false) end },
}

function M.set(on)
    if on == active then return end
    active = on
    if on then
        if stabilizing() then
            cam, flat = default_camera(), nil
        else
            cam, flat = nil, { zoom = 0, x = 0, y = 0 }
        end
        shown = nil
        saved.dragging = mp.get_property_native("window-dragging")
        mp.set_property_native("window-dragging", false)
        -- The app's mpv (tools/mpv-patches/0008); others have no such option
        gestures = mp.set_property_native("macos-gesture-messages", true) == true
        for _, b in ipairs(BINDINGS) do mp.add_forced_key_binding(b[1], b[2], b[3], b[4]) end
        mp.osd_message("Crop: drag to pan, wheel or pinch to zoom, Alt+drag to level", 3)
    else
        for _, b in ipairs(BINDINGS) do mp.remove_key_binding(b[2]) end
        if saved.dragging ~= nil then mp.set_property_native("window-dragging", saved.dragging) end
        if gestures then mp.set_property_native("macos-gesture-messages", false) end
        gestures, drag = false, nil
        local had_camera = cam ~= nil
        cam, flat, shown = nil, nil, nil
        mp.set_property_number("video-zoom", 0)
        mp.set_property_number("video-pan-x", 0)
        mp.set_property_number("video-pan-y", 0)
        if had_camera then
            hooks.write()
            if mp.get_property_native("pause") then
                local t = mp.get_property_number("time-pos")
                if t then mp.commandv("seek", tostring(t), "absolute+exact") end
            end
        end
        mp.osd_message("Crop: off", 1)
    end
    if cam then changed() else draw(); publish() end
end

function M.active() return active end

-- For the control file: the camera, while there's one
function M.control_words()
    if not cam then return "" end
    return string.format(" camera=%.4f,%.4f,%.4f,%.5f", cam.yaw, cam.pitch, cam.roll, cam.zoom)
end

-- A frame through after a seek: paused, it's the one asked for
mp.register_event("playback-restart", function()
    if not active or not cam then return end
    mp.msg.debug(string.format("restart: refreshing %s, dirty %s", tostring(refreshing ~= nil), tostring(dirty)))
    if refreshing then
        shown, refreshing = refreshing, nil
    else
        -- A seek of the user's, or the chain set up again: rendered with the camera the file has now
        shown = { yaw = cam.yaw, pitch = cam.pitch, roll = cam.roll, zoom = cam.zoom }
    end
    preview()
    if dirty then refresh() end
end)

-- Playing, frames come with the camera asked for; paused, the one on screen is the one rendered last
mp.observe_property("pause", "bool", function(_, paused)
    if not active or not cam then return end
    if paused then shown = { yaw = cam.yaw, pitch = cam.pitch, roll = cam.roll, zoom = cam.zoom } end
    preview()
end)

mp.observe_property("mouse-pos", "native", on_mouse)
for _, prop in ipairs({ "osd-dimensions", "video-out-params", "video-margin-ratio-right", "video-margin-ratio-left" }) do
    mp.observe_property(prop, "native", function() if active then draw(); publish() end end)
end
mp.register_event("end-file", function() M.set(false) end)

-- hooks: { stabilizing(): the plugin stabilizes the clip, write(): the control file written anew (with
-- control_words), control_path(): where it is }
function M.init(h)
    hooks = h
    publish()
end

mp.add_key_binding("c", "crop", function() M.set(not active) end)
-- What it draws, as an .ass file, for a render rig (vo=image draws no OSD)
mp.register_script_message("crop-dump", function(path)
    draw()
    local f = io.open(path, "w")
    if not f or not overlay.res_x then return mp.msg.error("can't write " .. path) end
    f:write("[Script Info]\nScriptType: v4.00+\nPlayResX: " .. overlay.res_x .. "\nPlayResY: " .. overlay.res_y .. "\nScaledBorderAndShadow: yes\n\n")
    f:write("[V4+ Styles]\nFormat: Name, Fontname, Fontsize, PrimaryColour, SecondaryColour, OutlineColour, BackColour, Bold, Italic, Underline, StrikeOut, ScaleX, ScaleY, Spacing, Angle, BorderStyle, Outline, Shadow, Alignment, MarginL, MarginR, MarginV, Encoding\n")
    f:write("Style: Default,sans-serif,20,&H00FFFFFF,&H00FFFFFF,&H00000000,&H00000000,0,0,0,0,100,100,0,0,1,0,0,7,0,0,0,1\n\n")
    f:write("[Events]\nFormat: Layer, Start, End, Style, Name, MarginL, MarginR, MarginV, Effect, Text\n")
    for line in (overlay.data or ""):gmatch("[^\n]+") do
        f:write("Dialogue: 0,0:00:00.00,9:00:00.00,Default,,0,0,0,," .. line .. "\n")
    end
    f:close()
end)

mp.register_script_message("crop-aspect", function(v)
    if v == "next" then aspect = aspect % #ASPECTS + 1
    elseif v == "prev" then aspect = (aspect - 2) % #ASPECTS + 1
    else
        for i, a in ipairs(ASPECTS) do if a.name == v then aspect = i end end
    end
    draw(); publish()
end)

mp.register_script_message("crop", function(v)
    if v == "toggle" then M.set(not active) elseif v == "yes" or v == "no" then M.set(v == "yes") end
end)

return M
