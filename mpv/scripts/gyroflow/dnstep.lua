-- The frame on screen while the GPU path's temporal denoise is on, and frame steps and the paused frame around it
-- (docs/design/seek-and-scrub.md, problem 1). Plain Lua: main.lua feeds it the player's state and does what it says;
-- tests/dnstep_test.lua tries it.
--
-- warp.glsl's dn_ passes keep the last 11 frames rendered (a ring) and show one with up to `after` frames after it and
-- `radius` before it, all in a row. Playing, that's the frame `after` before the newest decoded (mpv's time-pos);
-- after a seek, the first frame stays until the frames after it are there. Paused, main.lua may ask for another frame
-- of the ring (gyroflow_dn_show): a step back is then a frame already there, at once. Where the ring doesn't hold
-- the frame with all its neighbours (after a seek, a step back past it), it's filled again: a seek to `radius` frames
-- before it and that many and `after` more played (mpv's frame-step N), the frame held on screen meanwhile.
--
-- st, the state: t (time-pos: the newest frame decoded), F (a frame's duration), after, radius (the frames after and
-- before it the denoise takes: Full 5, Light 3), restart (where the row last started: the last seek's frame),
-- show (the frame asked for, or nil), duration (the clip's, or nil).

local M = {}

-- The ring's slots (warp.glsl's 2 x DN_RADIUS + 1)
M.RING = 11

local function near(a, b, F) return math.abs(a - b) < 0.5 * F end

-- The frame the shader shows by itself: `after` before the newest, not before the row's start
function M.natural(st)
    if not st.t then return nil end
    if (st.after or 0) <= 0 then return st.t end
    local r = st.restart and math.min(st.restart, st.t) or st.t
    return math.max(r, st.t - st.after * st.F)
end

-- The frame on screen
function M.shown(st)
    if st.show then return st.show end
    return M.natural(st)
end

-- The oldest frame of the row in the ring (the newest is t)
function M.row_start(st)
    local r = st.restart and math.min(st.restart, st.t) or st.t
    return math.max(r, st.t - (M.RING - 1) * st.F)
end

-- The last frame of the clip (nil: not known)
local function last_frame(st)
    return st.duration and (st.duration - st.F) or nil
end

-- How many frames around x the shader can take (before, after), and how many it would want there: radius before
-- (fewer at the clip's start) and `after` after (fewer at its end)
function M.neighbours(st, x)
    local lo = M.row_start(st)
    local have_b = x >= lo - 0.5 * st.F and math.floor((x - lo) / st.F + 0.5) or 0
    local have_a = x <= st.t + 0.5 * st.F and math.floor((st.t - x) / st.F + 0.5) or 0
    local want_b = math.min(st.radius, math.floor(x / st.F + 0.5))
    local last = last_frame(st)
    local want_a = st.after
    if last then want_a = math.max(0, math.min(want_a, math.floor((last - x) / st.F + 0.5))) end
    return math.min(have_b, st.radius), math.min(have_a, st.radius), want_b, want_a
end

-- Whether the frame at x, shown from this ring, has all the neighbours it could have
function M.complete(st, x)
    local b, a, wb, wa = M.neighbours(st, x)
    return b >= wb and a >= wa
end

-- A frame step, paused: dir +1 or -1. What to do: { show = x, frames = n } (the frame to show; n > 0: play n more
-- frames with mpv's frame-step first, for the frames after it), or { show = x, seek = x } (not in the ring: seek to
-- it, exactly; it shows as decoded, then fill), or nil (nothing: the clip's start or end)
function M.step(st, dir)
    local s = M.shown(st)
    if not s then return nil end
    local x = s + dir * st.F
    if x < -0.5 * st.F then return nil end
    local last = last_frame(st)
    if last and x > last + 0.5 * st.F then return nil end
    if dir > 0 then
        -- Forward: the frames after it decoded first, as many as the shader takes (playing them as frame-step does)
        local want = x + st.after * st.F
        if last then want = math.min(want, last) end
        local n = math.max(0, math.floor((want - st.t) / st.F + 0.5))
        return { show = x, frames = n }
    end
    if x >= M.row_start(st) - 0.5 * st.F then return { show = x, frames = 0 } end
    return { show = x, seek = x }
end

-- The ring filled again around x: from where to seek (exactly) and how many frames to play from there
function M.refill(st, x)
    local from = math.max(0, x - st.radius * st.F)
    local to = x + st.after * st.F
    local last = last_frame(st)
    if last then to = math.min(to, last) end
    return { from = from, frames = math.max(0, math.floor((to - from) / st.F + 0.5)), until_ = to }
end

-- Playing on from the shown frame: the seek it needs first (nil: none, it's the shader's own choice already)
function M.play_from(st)
    if not st.show then return nil end
    local n = M.natural(st)
    if n and near(n, st.show, st.F) then return nil end
    return st.show
end

M.near = near

return M
