-- Crop mode (c): a frame over the video to choose a still or a short clip by, the picture moved under it.
--
-- A drag, or a two-finger scroll on a trackpad, pans; the mouse wheel, or a pinch, zooms; Alt+drag or a two-finger
-- rotation turns the horizon. Where the gyroflow plugin stabilizes the clip, pan is the camera turned, not the picture
-- slid: the warp (main.lua's GPU path) renders it with the camera's yaw and pitch after the smoothing (gyroflow-core's
-- view_rotation), and zoom is its FOV, through the lens's own projection. The frame sits in the middle at MARGIN of
-- the video, with what's around it dimmed, so the view is rendered that much wider than the frame (zoom 1 / MARGIN to
-- start with: the frame shows what the screen showed). Elsewhere it's mpv's video-zoom and video-pan.
--
-- The camera is on a tripod: yaw about the vertical, pitch about its own horizontal axis, roll about where it looks
-- (gyroflow-core's view_rotation), so the horizon stays as stabilized whatever way it's turned. A drag turns it by as
-- much as the picture moves under the mouse (in the middle), along the picture's horizon when it's rolled.
--
-- The camera is the warp's own parameters (main.lua's gpu_camera): turned at once, paused or playing, with nothing
-- rendered again but the frame on screen.
--
-- Keys while it's on: a / Shift+a the frame's aspect, double click or Backspace back to the start, Enter saves it as a
-- photo (export.lua's photo, with this camera and frame), c or Esc leaves
-- (the camera as the plugin stabilizes it again). The frame's state is published as user-data/gyroflow/crop for the
-- export.
--
-- The mouse, the wheel and the trackpad's gestures are gestures.lua's, which the 360 view (../view360) turns by too.
--
-- Loaded by main.lua, which says whether the plugin stabilizes the clip and sets the warp's camera (init).

local mp = require 'mp'
local gestures = require 'gestures'

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
local ZOOM_MIN, ZOOM_MAX = 0.15, 4
-- How the picture moves as the camera turns: view_rotation's yaw (looking right) moves it left, pitch (looking down)
-- up, roll counterclockwise (ROLL_SIGN); the camera model below (rotation) has the same
local ROLL_SIGN = 1
-- Before the warp's table has said, a focal length for a 90° wide picture
local DEFAULT_FX = 0.5

local hooks = nil
local active = false
local aspect = 1
-- The camera asked for (the warp's)
local cam = nil
-- Without the plugin: mpv's own zoom (log2) and pan
local flat = nil
local overlay = mp.create_osd_overlay("ass-events")

local function default_camera()
    return { yaw = 0, pitch = 0, roll = 0, zoom = 1 / MARGIN }
end

local function copy(c)
    return c and { yaw = c.yaw, pitch = c.pitch, roll = c.roll, zoom = c.zoom }
end

local function stabilizing()
    return hooks ~= nil and hooks.stabilizing()
end

-- The warp's table: the focal length in output widths and heights at zoom 1
local function focal()
    local g = hooks and hooks.focal()
    if g and tonumber(g[1]) and tonumber(g[2]) then return tonumber(g[1]), tonumber(g[2]) end
    return DEFAULT_FX, DEFAULT_FX * 16 / 9
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
    -- Not over the side panel nor the navigation bar below the video
    local right = W * (1 - (mp.get_property_number("video-margin-ratio-right") or 0))
    local bottom = H * (1 - (mp.get_property_number("video-margin-ratio-bottom") or 0))
    local function style(color, alpha)
        return string.format("{\\an7\\pos(0,0)\\clip(0,0,%d,%d)\\bord0\\shad0\\1c&H%s&\\1a&H%s&\\p1}",
                             right, bottom, color, alpha)
    end
    local dim = style("000000", "50")
    -- Far enough out to cover the window turned
    local B = W + H
    local lines = {
        dim .. rect_path(x0 - B, y0 - B, x1 + B, y0),
        dim .. rect_path(x0 - B, y1, x1 + B, y1 + B),
        dim .. rect_path(x0 - B, y0, x0, y1),
        dim .. rect_path(x1, y0, x1 + B, y1),
    }
    -- The frame, and its thirds
    local edge = style("FFFFFF", "20")
    local t = math.max(1, math.floor(H / 600 + 0.5))
    table.insert(lines, edge .. rect_path(x0 - t, y0 - t, x1 + t, y0) .. " " .. rect_path(x0 - t, y1, x1 + t, y1 + t) .. " "
        .. rect_path(x0 - t, y0, x0, y1) .. " " .. rect_path(x1, y0, x1 + t, y1))
    local third = style("FFFFFF", "A0")
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
    table.insert(lines, string.format("{\\an1\\pos(%d,%d)\\fs%d\\bord1.5\\shad0\\1c&HFFFFFF&\\3c&H000000&}%s{\\1a&H60&}   a aspect · Enter photo · double click reset · c leave",
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
        camera = cam, flat = flat, zoom_min = ZOOM_MIN, zoom_max = ZOOM_MAX, margin = MARGIN,
    })
end

-- The camera model: x right, y down, z where it looks. Its rotation (to the world) on a tripod: yaw about the vertical,
-- then pitch about its own horizontal axis, then roll about where it looks, as gyroflow-core's view_rotation
local function rotation(c)
    local y, p, r = math.rad(c.yaw), math.rad(c.pitch), math.rad(c.roll)
    local cy, sy, cp, sp, cr, sr = math.cos(y), math.sin(y), math.cos(p), math.sin(p), math.cos(r), math.sin(r)
    local Y = { { cy, 0, sy }, { 0, 1, 0 }, { -sy, 0, cy } }
    local P = { { 1, 0, 0 }, { 0, cp, sp }, { 0, -sp, cp } }
    local R = { { cr, -sr, 0 }, { sr, cr, 0 }, { 0, 0, 1 } }
    local function mul(A, B)
        local M = {}
        for i = 1, 3 do
            M[i] = {}
            for j = 1, 3 do M[i][j] = A[i][1] * B[1][j] + A[i][2] * B[2][j] + A[i][3] * B[3][j] end
        end
        return M
    end
    return mul(mul(Y, P), R)
end

-- Where the point at a, b (from the view's middle, in its widths and heights) looks, in the world
local function look(c, a, b)
    local fx, fy = focal()
    fx, fy = fx / c.zoom, fy / c.zoom
    local M, v = rotation(c), { a / fx, b / fy, 1 }
    return { M[1][1] * v[1] + M[1][2] * v[2] + M[1][3] * v[3], M[2][1] * v[1] + M[2][2] * v[2] + M[2][3] * v[3],
             M[3][1] * v[1] + M[3][2] * v[2] + M[3][3] * v[3] }
end

-- Where the world's direction w is in the view (nil: behind it)
local function place(c, w)
    local fx, fy = focal()
    fx, fy = fx / c.zoom, fy / c.zoom
    local M = rotation(c)
    local x = M[1][1] * w[1] + M[2][1] * w[2] + M[3][1] * w[3]
    local y = M[1][2] * w[1] + M[2][2] * w[2] + M[3][2] * w[3]
    local z = M[1][3] * w[1] + M[2][3] * w[2] + M[3][3] * w[3]
    if z <= 1e-6 then return nil end
    return fx * x / z, fy * y / z
end

-- The point the camera turned about last: where the mouse took the picture (nil: the middle)
local anchor = nil

-- The camera changed: the warp has it at once (mpv's own zoom and pan without the plugin)
local function changed()
    if cam then
        hooks.camera(cam)
    else
        mp.set_property_number("video-zoom", flat.zoom)
        mp.set_property_number("video-pan-x", flat.x)
        mp.set_property_number("video-pan-y", flat.y)
    end
    draw()
    publish()
end

-- The picture moved by dx, dy screen pixels, the point at x, y (where the mouse is; the middle when nil) taken there
-- from x - dx, y - dy: the camera turns so that what's under the mouse stays under it, in the middle as anywhere
local function pan(dx, dy, x, y)
    local v = video_rect()
    if not v then return end
    if cam then
        local cx, cy = v.x + v.w / 2, v.y + v.h / 2
        x, y = x or cx, y or cy
        local a1, b1 = (x - cx) / v.w, (y - cy) / v.h
        -- What was under the mouse, in the world: the yaw and pitch that put it at a1, b1 (Newton's method, from where
        -- the camera is)
        local w = look(cam, (x - dx - cx) / v.w, (y - dy - cy) / v.h)
        local c = copy(cam)
        local function miss(yaw, pitch)
            c.yaw, c.pitch = yaw, pitch
            local a, b = place(c, w)
            if not a then return nil end
            return a - a1, b - b1
        end
        local yaw, pitch, h = cam.yaw, cam.pitch, 1e-3
        for _ = 1, 6 do
            local e1, e2 = miss(yaw, pitch)
            if not e1 or math.abs(e1) + math.abs(e2) < 1e-7 then break end
            local y1, y2 = miss(yaw + h, pitch)
            local p1, p2 = miss(yaw, pitch + h)
            if not y1 or not p1 then break end
            local j11, j21, j12, j22 = (y1 - e1) / h, (y2 - e2) / h, (p1 - e1) / h, (p2 - e2) / h
            local det = j11 * j22 - j12 * j21
            if math.abs(det) < 1e-12 then break end
            yaw = yaw - (j22 * e1 - j12 * e2) / det
            pitch = pitch - (-j21 * e1 + j11 * e2) / det
        end
        if yaw > 180 then yaw = yaw - 360 elseif yaw < -180 then yaw = yaw + 360 end
        cam.yaw, cam.pitch = yaw, math.max(-89, math.min(89, pitch))
        anchor = { x = x, y = y }
    else
        local u, w = dx / v.w, dy / v.h
        local s = 2 ^ flat.zoom
        flat.x, flat.y = flat.x + u / s, flat.y + w / s
    end
    changed()
end

-- Zoomed in by `factor` (above 1: closer)
local function zoom(factor)
    anchor = nil
    if cam then
        cam.zoom = math.max(ZOOM_MIN, math.min(ZOOM_MAX, cam.zoom / factor))
    else
        flat.zoom = math.max(-1, math.min(3, flat.zoom + math.log(factor) / math.log(2)))
    end
    changed()
end

local function roll(degrees)
    if not cam then return end
    anchor = nil
    cam.roll = cam.roll + ROLL_SIGN * degrees
    if cam.roll > 180 then cam.roll = cam.roll - 360 elseif cam.roll < -180 then cam.roll = cam.roll + 360 end
    changed()
end

local function reset()
    anchor = nil
    if cam then cam = default_camera() else flat = { zoom = 0, x = 0, y = 0 } end
    changed()
end

local input = gestures.new({
    prefix = "crop", pan = pan, zoom = zoom, roll = roll, reset = reset,
    released = function() end,
    width = function() local v = video_rect(); return v and v.w end,
    bindings = {
        { "ENTER", "crop-key-photo", function() mp.commandv("script-message-to", "gyroflow", "photo") end },
        { "KP_ENTER", "crop-key-photo-kp", function() mp.commandv("script-message-to", "gyroflow", "photo") end },
        { "a", "crop-key-aspect", function() aspect = aspect % #ASPECTS + 1; draw(); publish() end },
        { "A", "crop-key-aspect-back", function() aspect = (aspect - 2) % #ASPECTS + 1; draw(); publish() end },
        { "ESC", "crop-key-leave", function() M.set(false) end },
    },
})

function M.set(on)
    if on == active then return end
    -- A photo shown (the live-photo plugin): nothing to save a frame of
    if on and type(mp.get_property_native("user-data/afp/photo")) == "table" then return end
    active = on
    if on then
        if stabilizing() then
            cam, flat = default_camera(), nil
        else
            cam, flat = nil, { zoom = 0, x = 0, y = 0 }
        end
        input:set(true)
        mp.osd_message("Save frame: drag to pan, wheel or pinch to zoom, Alt+drag to level, Enter to save", 3)
    else
        input:set(false)
        if cam then hooks.camera(nil) end
        cam, flat = nil, nil
        mp.set_property_number("video-zoom", 0)
        mp.set_property_number("video-pan-x", 0)
        mp.set_property_number("video-pan-y", 0)
        mp.osd_message("Save frame: off", 1)
    end
    if cam then changed() else draw(); publish() end
end

function M.active() return active end

-- A camera of its own for what plays, outside crop mode: a photo link's (Save frame's camera, kept in its link), so
-- the clip shows as Share renders the link (gyroflow-export --camera). In the warp's parameters (kept there for the
-- shader when it comes). nil: none again
function M.view(c)
    if active then return end
    if not c and not cam then return end
    cam = c and { yaw = tonumber(c.yaw) or 0, pitch = tonumber(c.pitch) or 0, roll = tonumber(c.roll) or 0, zoom = tonumber(c.zoom) or 1 } or nil
    hooks.camera(cam)
end

-- A photo link's camera and crop (Save frame's: the frame in crop mode's middle, in shares of the stabilized frame) as
-- the camera alone where it can be: zoomed in by the crop's larger side, and the rest of the other side cut (in shares
-- of what that camera shows; nil where nothing's left to cut). The same picture as the frame rendered with the camera,
-- then cropped (Share's way): the projection is radial about the middle, so a closer zoom is that crop enlarged. Cut
-- out of the whole stabilized frame instead (mpv's video-crop), the live part's horizon trembled: the wide frame's
-- small leftover shake, enlarged with it, which a camera that close doesn't have (2026-11-20, Anton's 0102 at 49 s:
-- crop 0.8, camera zoom 0.455 trembled; zoom 0.364 without the crop stood still). nil: a crop off the middle (not
-- crop mode's), left to the caller as it is
function M.fold(camera, crop)
    if type(camera) ~= "table" or type(crop) ~= "table" then return nil end
    local x, y, w, h = tonumber(crop.x), tonumber(crop.y), tonumber(crop.w), tonumber(crop.h)
    if not (x and y and w and h) or w <= 0 or h <= 0 or w > 1 or h > 1 then return nil end
    if math.abs(x + w / 2 - 0.5) > 0.002 or math.abs(y + h / 2 - 0.5) > 0.002 then return nil end
    local s = math.max(w, h)
    local cam = { yaw = tonumber(camera.yaw) or 0, pitch = tonumber(camera.pitch) or 0, roll = tonumber(camera.roll) or 0,
                  zoom = (tonumber(camera.zoom) or 1) * s }
    local rw, rh = w / s, h / s
    local rest = nil
    if rw < 0.999 or rh < 0.999 then rest = { x = (1 - rw) / 2, y = (1 - rh) / 2, w = rw, h = rh } end
    return { camera = cam, rest = rest }
end

for _, prop in ipairs({ "osd-dimensions", "video-out-params", "video-margin-ratio-right", "video-margin-ratio-left",
                        "video-margin-ratio-bottom" }) do
    mp.observe_property(prop, "native", function() if active then draw(); publish() end end)
end
mp.register_event("end-file", function() M.set(false) end)

-- hooks: { stabilizing(): the plugin stabilizes the clip, camera(c): the warp's camera set (nil: none), focal(): its
-- focal length }
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

-- For the side panel: crop-set zoom|roll|yaw|pitch <value>, crop-reset
mp.register_script_message("crop-set", function(key, value)
    value = tonumber(value)
    if not active or not value then return end
    anchor = nil
    if cam and key == "zoom" then cam.zoom = math.max(ZOOM_MIN, math.min(ZOOM_MAX, value))
    elseif cam and key == "roll" then cam.roll = math.max(-180, math.min(180, value))
    elseif cam and key == "yaw" then cam.yaw = math.max(-180, math.min(180, value))
    elseif cam and key == "pitch" then cam.pitch = math.max(-89, math.min(89, value))
    elseif flat and key == "zoom" then flat.zoom = math.max(-1, math.min(3, value))
    else return end
    changed()
end)
mp.register_script_message("crop-reset", function() if active then reset() end end)

mp.register_script_message("crop", function(v)
    if v == "toggle" then M.set(not active) elseif v == "yes" or v == "no" then M.set(v == "yes") end
end)

return M
