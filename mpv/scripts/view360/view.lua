-- The 360 view's geometry, the same as view360.glsl's: for main.lua to turn the head so that what's under the mouse
-- stays under it, and for tests (tests/view360_test.lua). Plain Lua, no mpv.
--
--   local view = require 'view'
--   local v = view.default()            { yaw, pitch, roll, fov (degrees), projection }
--   view.ray(v, x, y)                    the direction in the world (x right, y down, z forward) of the screen point
--                                        x, y: half heights of the picture from its middle, y down
--   view.place(v, w)                     where the world's direction w is on the screen (x, y), nil when it isn't
--   view.pan(v, x0, y0, x1, y1)          the yaw and pitch that bring what's at x0, y0 to x1, y1 (the little planet:
--                                        spun and tilted)
--
-- The projections: auto (rectilinear up to 100°, the stereographic from 160°, in between from one to the other),
-- rectilinear, stereographic, planet (the little planet: the stereographic looking straight down; its pitch is from
-- there). See view360.glsl for the conventions.

local M = {}

M.PROJECTIONS = { "auto", "rectilinear", "stereographic", "planet" }
M.PROJECTION_INDEX = { auto = 0, rectilinear = 1, stereographic = 2, planet = 3 }
-- The FOV's range for each, degrees
M.FOV = {
    auto = { min = 20, max = 300, default = 100 },
    rectilinear = { min = 20, max = 150, default = 100 },
    stereographic = { min = 20, max = 300, default = 140 },
    planet = { min = 60, max = 330, default = 270 },
}
M.PITCH_MAX = 89.9
-- The FOV spans a 16:9 picture's width at the window's height: half of it, in half heights
local HALF_WIDTH = 16 / 9

function M.default(projection)
    projection = projection or "auto"
    return { yaw = 0, pitch = 0, roll = 0, fov = M.FOV[projection].default, projection = projection }
end

function M.copy(v)
    return { yaw = v.yaw, pitch = v.pitch, roll = v.roll, fov = v.fov, projection = v.projection }
end

local function smoothstep(a, b, x)
    local t = math.max(0, math.min(1, (x - a) / (b - a)))
    return t * t * (3 - 2 * t)
end

-- d: the projection's distance; k: the screen's scale for r = k sin θ / (cos θ + d)
function M.distance(v)
    if v.projection == "rectilinear" then return 0 end
    if v.projection == "stereographic" or v.projection == "planet" then return 1 end
    return smoothstep(100, 160, v.fov)
end

local function scale(v, d)
    local half = math.rad(v.fov) / 2
    return HALF_WIDTH * (math.cos(half) + d) / math.sin(half)
end

-- The head's rotation into the world: yaw about the vertical, pitch about its own horizontal axis, roll about where
-- it looks (rows of a 3x3)
function M.rotation(v)
    local pitch = v.pitch - (v.projection == "planet" and 90 or 0)
    local y, p, r = math.rad(v.yaw), math.rad(pitch), math.rad(v.roll)
    local cy, sy, cp, sp, cr, sr = math.cos(y), math.sin(y), math.cos(p), math.sin(p), math.cos(r), math.sin(r)
    local Y = { { cy, 0, sy }, { 0, 1, 0 }, { -sy, 0, cy } }
    local P = { { 1, 0, 0 }, { 0, cp, -sp }, { 0, sp, cp } }
    local R = { { cr, -sr, 0 }, { sr, cr, 0 }, { 0, 0, 1 } }
    local function mul(A, B)
        local C = {}
        for i = 1, 3 do
            C[i] = {}
            for j = 1, 3 do C[i][j] = A[i][1] * B[1][j] + A[i][2] * B[2][j] + A[i][3] * B[3][j] end
        end
        return C
    end
    return mul(mul(Y, P), R)
end

function M.ray(v, x, y)
    local d = M.distance(v)
    local k = scale(v, d)
    local r = math.sqrt(x * x + y * y)
    local h
    if r < 1e-12 then
        h = { 0, 0, 1 }
    else
        local t = r / k
        local t2 = t * t
        local z = (-t2 * d + math.sqrt(t2 * (1 - d * d) + 1)) / (t2 + 1)
        local s = t * (z + d)
        h = { x / r * s, y / r * s, z }
    end
    local R = M.rotation(v)
    return { R[1][1] * h[1] + R[1][2] * h[2] + R[1][3] * h[3], R[2][1] * h[1] + R[2][2] * h[2] + R[2][3] * h[3],
             R[3][1] * h[1] + R[3][2] * h[2] + R[3][3] * h[3] }
end

function M.place(v, w)
    local R = M.rotation(v)
    local x = R[1][1] * w[1] + R[2][1] * w[2] + R[3][1] * w[3]
    local y = R[1][2] * w[1] + R[2][2] * w[2] + R[3][2] * w[3]
    local z = R[1][3] * w[1] + R[2][3] * w[2] + R[3][3] * w[3]
    local n = math.sqrt(x * x + y * y + z * z)
    if n < 1e-12 then return nil end
    x, y, z = x / n, y / n, z / n
    local d = M.distance(v)
    -- Behind the projection's point: nowhere on the screen
    if z + d <= 1e-6 then return nil end
    local s = math.sqrt(x * x + y * y)
    if s < 1e-12 then return 0, 0 end
    local r = scale(v, d) * s / (z + d)
    return x / s * r, y / s * r
end

-- The equirect's texture coordinates (0-1, y down) of a direction, as the shader looks it up
function M.equirect(w)
    local n = math.sqrt(w[1] * w[1] + w[2] * w[2] + w[3] * w[3])
    local lon = math.atan2(w[1], w[3])
    local lat = math.asin(math.max(-1, math.min(1, -w[2] / n)))
    return 0.5 + lon / (2 * math.pi), 0.5 - lat / math.pi
end

local function wrap(a)
    a = a % 360
    if a > 180 then a = a - 360 end
    return a
end
M.wrap = wrap

-- The little planet: a drag around its middle spins it (the yaw), towards or away from the middle tilts it (the
-- pitch, so that what's under the mouse comes as far from the middle as the mouse). Looking straight down, what's at
-- the middle can't be moved off it by a yaw and a pitch, so the rule of the other projections can't hold there
local function pan_planet(v, x0, y0, x1, y1)
    local w = M.ray(v, x0, y0)
    local c = M.copy(v)
    local r0, r1 = math.sqrt(x0 * x0 + y0 * y0), math.sqrt(x1 * x1 + y1 * y1)
    if r0 > 0.05 and r1 > 0.05 then
        local turn = math.deg(math.atan2(y1, x1) - math.atan2(y0, x0))
        -- Looking down with the way ahead at the top, a yaw to the right spins the ground counterclockwise
        c.yaw = M.wrap(v.yaw - M.wrap(turn))
    end
    local function radius(pitch)
        c.pitch = pitch
        local a, b = M.place(c, w)
        return a and math.sqrt(a * a + b * b)
    end
    local pitch, h = v.pitch, 1e-3
    for _ = 1, 8 do
        local r = radius(pitch)
        if not r or math.abs(r - r1) < 1e-10 then break end
        local r2 = radius(pitch + h)
        if not r2 or math.abs(r2 - r) < 1e-12 then break end
        local step = (r - r1) / ((r2 - r) / h)
        pitch = pitch - math.max(-45, math.min(45, step))
    end
    return c.yaw, math.max(-M.PITCH_MAX, math.min(M.PITCH_MAX, pitch))
end

-- The yaw and pitch (Newton's method, from the view's) that put the world's direction under x0, y0 at x1, y1
function M.pan(v, x0, y0, x1, y1)
    if v.projection == "planet" then return pan_planet(v, x0, y0, x1, y1) end
    local w = M.ray(v, x0, y0)
    local c = M.copy(v)
    local function miss(yaw, pitch)
        c.yaw, c.pitch = yaw, pitch
        local a, b = M.place(c, w)
        if not a then return nil end
        return a - x1, b - y1
    end
    local yaw, pitch, h = v.yaw, v.pitch, 1e-3
    for _ = 1, 8 do
        local e1, e2 = miss(yaw, pitch)
        if not e1 or math.abs(e1) + math.abs(e2) < 1e-9 then break end
        local y1, y2 = miss(yaw + h, pitch)
        local p1, p2 = miss(yaw, pitch + h)
        if not y1 or not p1 then break end
        local j11, j21, j12, j22 = (y1 - e1) / h, (y2 - e2) / h, (p1 - e1) / h, (p2 - e2) / h
        local det = j11 * j22 - j12 * j21
        if math.abs(det) < 1e-12 then break end
        local dyaw = (j22 * e1 - j12 * e2) / det
        local dpitch = (-j21 * e1 + j11 * e2) / det
        -- Steps no bigger than a quarter turn: near a pole the first one can be wild
        local big = math.max(math.abs(dyaw), math.abs(dpitch)) / 45
        if big > 1 then dyaw, dpitch = dyaw / big, dpitch / big end
        yaw, pitch = yaw - dyaw, pitch - dpitch
    end
    return wrap(yaw), math.max(-M.PITCH_MAX, math.min(M.PITCH_MAX, pitch))
end

-- The FOV kept within the projection's range
function M.clamp_fov(projection, fov)
    local r = M.FOV[projection]
    return math.max(r.min, math.min(r.max, fov))
end

-- view360.glsl's parameters for the view (glsl-shader-opts), as strings
function M.shader_opts(v, aspect, stereo, heading)
    return {
        view360_yaw = string.format("%.4f", v.yaw), view360_pitch = string.format("%.4f", v.pitch),
        view360_roll = string.format("%.4f", v.roll), view360_fov = string.format("%.4f", v.fov),
        view360_projection = tostring(M.PROJECTION_INDEX[v.projection] or 0),
        view360_aspect = string.format("%.6f", aspect or 0), view360_stereo = tostring(stereo or 0),
        view360_heading = tostring(heading or 0),
    }
end

return M
