-- Crop mode (c): a frame over the video to choose a still or a short clip by, the picture moved under it.
--
-- A drag, or a two-finger scroll on a trackpad, pans; the mouse wheel, or a pinch, zooms; Alt+drag or a two-finger
-- rotation turns the horizon. Where the gyroflow plugin stabilizes the clip, pan is the camera turned, not the picture
-- slid: the plugin renders it again with the camera's yaw and pitch after the smoothing (gyroflow-core's
-- view_rotation), and zoom is its FOV, through the lens's own projection. The frame sits in the middle at MARGIN of
-- the video, with what's around it dimmed, so the view is rendered that much wider than the frame (zoom 1 / MARGIN to
-- start with: the frame shows what the screen showed). Elsewhere it's mpv's video-zoom and video-pan.
--
-- The camera is on a tripod: yaw about the vertical, pitch about its own horizontal axis, roll about where it looks
-- (gyroflow-core's view_rotation), so the horizon stays as stabilized whatever way it's turned. A drag turns it by as
-- much as the picture moves under the mouse (in the middle), along the picture's horizon when it's rolled.
--
-- The camera goes to the plugin through the edges control file (main.lua's write_edge_control, `camera=`), which it
-- reads within 50 ms while playing. Paused, mpv seeks to the frame on screen again to have it rendered anew; until
-- that's through, mpv's video-zoom and video-pan move the frame that's there by about as much (exact in the middle),
-- from the focal length the plugin writes next to the control file (Edges::write_camera_info). With mpv's GPU video
-- outputs it's crop_preview.glsl that moves it, exactly (a turn, a roll, a zoom): each render of the paused frame
-- carries a parity (the control file's crop_mark), and the shader has the mapping for each, so a new frame shows moved
-- by its own from the moment it's there (mpv shows it before a script hears of it). Elsewhere mpv's video-zoom and
-- video-pan move it, and the frame drawn over it turns the other way for a roll. While a seek's under way the control
-- file stays as it was when it was
-- asked for, so the frame that comes is the one it says (the filter reads the file as mpv sets it up, any time after
-- the seek's asked for); the camera since then goes with the next one.
--
-- Keys while it's on: a / Shift+a the frame's aspect, double click or Backspace back to the start, c or Esc leaves
-- (the camera as the plugin stabilizes it again). The frame's state is published as user-data/gyroflow/crop for the
-- export.
--
-- On macOS the app's mpv sends the trackpad's gestures as script messages while it's on (--macos-gesture-messages,
-- tools/mpv-patches/0008), so a two-finger scroll pans and a mouse wheel zooms. A trackpad's scroll comes in phases
-- (fingers on, moving, off); a mouse's wheel smoothed into fine steps (Logitech's Options+) comes without, and zooms.
-- The coasting after the fingers are off is left out: the picture stops where they did. Elsewhere a wheel's whole clicks zoom
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
-- A smooth wheel's points (macOS) for one click's zoom
local SMOOTH_WHEEL_PT = 10
-- How the picture moves as the camera turns: view_rotation's yaw (looking right) moves it left, pitch (looking down)
-- up, roll counterclockwise (ROLL_SIGN); the camera model below (rotation) has the same
local ROLL_SIGN = 1
-- The control file at most this often while dragging, s
local WRITE_EVERY = 1 / 30
-- Before the plugin has said, a focal length for a 90° wide picture
local DEFAULT_FX = 0.5

local hooks = nil
local active = false
local aspect = 1
-- The camera asked for, the one in the control file, and the one the paused frame on screen was rendered with
local cam = nil
local written = nil
local shown = nil
-- The parity of the render the control file asks for (crop_mark), and each parity's camera (crop_preview.glsl)
local mark = 0
local renders = {}
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

local function copy(c)
    return c and { yaw = c.yaw, pitch = c.pitch, roll = c.roll, zoom = c.zoom }
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
    -- Not over the side panel; turned about the frame's middle the other way from a roll the paused frame doesn't
    -- have yet (what's in it is then what the frame rendered with the roll will show)
    local right = W * (1 - (mp.get_property_number("video-margin-ratio-right") or 0))
    local turn = 0
    if cam and shown and not hooks.shader and mp.get_property_native("pause") then turn = -ROLL_SIGN * (cam.roll - shown.roll) end
    local function style(color, alpha)
        return string.format("{\\an7\\pos(0,0)\\org(%d,%d)\\frz%.2f\\clip(0,0,%d,%d)\\bord0\\shad0\\1c&H%s&\\1a&H%s&\\p1}",
                             (x0 + x1) / 2, (y0 + y1) / 2, turn, right, H, color, alpha)
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

local function mul(A, B)
    local M = {}
    for i = 1, 3 do
        M[i] = {}
        for j = 1, 3 do M[i][j] = A[i][1] * B[1][j] + A[i][2] * B[2][j] + A[i][3] * B[3][j] end
    end
    return M
end

-- From the view asked for to the frame rendered with camera c, in texture coordinates (0-1, y down): both turn about
-- the same point, so it's a homography, K_c · R_c^T · R_cam · K_cam^-1
local function homography(c)
    local fx, fy = focal()
    local fa, ga, fb, gb = fx / cam.zoom, fy / cam.zoom, fx / c.zoom, fy / c.zoom
    local Ra, Rb = rotation(cam), rotation(c)
    local M = {}
    for i = 1, 3 do
        M[i] = {}
        for j = 1, 3 do M[i][j] = Rb[1][i] * Ra[1][j] + Rb[2][i] * Ra[2][j] + Rb[3][i] * Ra[3][j] end
    end
    local unproject = { { 1 / fa, 0, -0.5 / fa }, { 0, 1 / ga, -0.5 / ga }, { 0, 0, 1 } }
    local project = { { fb, 0, 0.5 }, { 0, gb, 0.5 }, { 0, 0, 1 } }
    return mul(project, mul(M, unproject))
end

local IDENTITY = { { 1, 0, 0 }, { 0, 1, 0 }, { 0, 0, 1 } }
local shader_set = nil
-- crop_preview.glsl's parameters, with whatever else glsl-shader-opts has; nil: off
local function set_shader(a, b)
    local opts = mp.get_property_native("glsl-shader-opts") or {}
    for k in pairs(opts) do if k:match("^crop_") then opts[k] = nil end end
    local key = "off"
    if a then
        opts.crop_on = "1"
        for i = 1, 3 do
            for j = 1, 3 do
                opts["crop_a" .. ((i - 1) * 3 + j - 1)] = string.format("%.7g", a[i][j])
                opts["crop_b" .. ((i - 1) * 3 + j - 1)] = string.format("%.7g", b[i][j])
            end
        end
        local parts = {}
        for i = 0, 8 do table.insert(parts, opts["crop_a" .. i] .. "," .. opts["crop_b" .. i]) end
        key = table.concat(parts, ";")
    end
    if key == shader_set then return end
    shader_set = key
    mp.set_property_native("glsl-shader-opts", opts)
end

-- The point the preview keeps where it should be: where the mouse took the picture last (nil: the middle)
local anchor = nil

-- The paused frame moved by mpv to about where the camera asked for puts it, from the one it was rendered with
local function preview()
    if not cam then return end
    local paused = mp.get_property_native("pause")
    if hooks.shader then
        mp.set_property_number("video-zoom", 0)
        mp.set_property_number("video-pan-x", 0)
        mp.set_property_number("video-pan-y", 0)
        -- Playing, the frames come with the camera asked for (a moment later)
        local function h(c) return (paused and c) and homography(c) or IDENTITY end
        set_shader(h(renders[0]), h(renders[1]))
        return
    end
    if not paused or not shown then
        mp.set_property_number("video-zoom", 0)
        mp.set_property_number("video-pan-x", 0)
        mp.set_property_number("video-pan-y", 0)
        return
    end
    local v = video_rect()
    if not v then return end
    -- The frame on screen scaled by r and moved so that the anchor's point in the view asked for (a, b) shows where
    -- it's in the frame on screen (as, bs): mpv puts that at the middle + (as + pan) * r * the video's size
    local r = shown.zoom / cam.zoom
    local a, b = 0, 0
    if anchor then a, b = (anchor.x - (v.x + v.w / 2)) / v.w, (anchor.y - (v.y + v.h / 2)) / v.h end
    local as, bs = place(shown, look(cam, a, b))
    mp.set_property_number("video-zoom", math.log(r) / math.log(2))
    if not as then return end
    mp.set_property_number("video-pan-x", a / r - as)
    mp.set_property_number("video-pan-y", b / r - bs)
end

local function same(a, b)
    return a and b and a.yaw == b.yaw and a.pitch == b.pitch and a.roll == b.roll and a.zoom == b.zoom
end

-- The camera into the control file, and paused, mpv seeks to the frame again, which sets the filter up again, which
-- reads it. Not while such a seek is under way: the frame it brings would be rendered with this camera or that one
local function write_now()
    if write_timer then write_timer:kill(); write_timer = nil end
    local paused = mp.get_property_native("pause")
    if paused and refreshing then dirty = true; return end
    last_write = mp.get_time()
    written = copy(cam)
    local t = paused and not same(written, shown) and mp.get_property_number("time-pos")
    -- A render of the paused frame: the other parity (crop_preview.glsl)
    if t then mark = 1 - mark end
    renders[mark] = copy(written)
    -- The shader's mapping for this render's parity before it can come (it may well come before the mouse moves on)
    preview()
    hooks.write()
    if not t then return end
    refreshing, dirty = copy(written), false
    mp.msg.debug(string.format("refresh: yaw %.3f pitch %.3f roll %.3f zoom %.3f", cam.yaw, cam.pitch, cam.roll, cam.zoom))
    mp.commandv("seek", tostring(t), "absolute+exact")
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
        pan(dx, dy, pos.x, pos.y)
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
        local phase = c
        if phase == "momentum" then return end
        local dx, dy = tonumber(a) or 0, tonumber(b) or 0
        -- A mouse's wheel, in fine steps: up zooms in
        if phase == "none" then
            if dy ~= 0 then zoom(WHEEL_ZOOM ^ (dy / SMOOTH_WHEEL_PT)) end
            return
        end
        local s = mp.get_property_number("display-hidpi-scale") or 1
        local pos = mp.get_property_native("mouse-pos") or {}
        pan(dx * s, dy * s, pos.x and pos.x + dx * s, pos.y and pos.y + dy * s)
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
    refreshing, dirty, written, renders = nil, false, nil, {}
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
        if write_timer then write_timer:kill(); write_timer = nil end
        cam, flat, shown = nil, nil, nil
        if hooks.shader then set_shader(nil) end
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

-- For the control file: the camera written last (write_now), while there's one
function M.control_words()
    if not cam or not written then return "" end
    return string.format(" camera=%.4f,%.4f,%.4f,%.5f crop_mark=%d", written.yaw, written.pitch, written.roll, written.zoom, mark)
end

-- A frame through after a seek: paused, it's the one asked for
mp.register_event("playback-restart", function()
    if not active or not cam then return end
    mp.msg.debug(string.format("restart: refreshing %s, dirty %s", tostring(refreshing ~= nil), tostring(dirty)))
    -- Ours, or a seek of the user's, or the chain set up again: rendered with the camera the file has
    shown, refreshing = copy(written) or default_camera(), nil
    preview()
    draw()
    if dirty then write_now() end
end)

-- Playing, frames come with the camera asked for; paused, the one on screen is the one rendered last
mp.observe_property("pause", "bool", function(_, paused)
    if not active or not cam then return end
    if paused then
        -- Playing, the frames came with the camera written
        shown = copy(written) or default_camera()
        write_now()
    end
    preview()
    draw()
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
