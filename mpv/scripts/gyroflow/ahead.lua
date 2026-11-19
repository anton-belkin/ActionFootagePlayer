-- The GPU path's table kept ahead of where a clip plays (docs/gpu-table-ahead.md). A clip without a project opens with
-- its table for a stretch of its motion data (the seconds it starts at: gyroflow-gpu-table's "partial" reply, which says
-- what it "covers" and up to where it's "settled"); then, while it plays, the stretch is extended ahead of where it
-- plays, a step at a time, each table taking the place of the one before as it comes in (the frames both have stay as
-- they were). Pure: main.lua hands it where it plays and what the table has, it says what to ask for next and whether
-- playback has to wait for it.
--
--   local ahead = require 'ahead'
--   ahead.next(s)   -> nil, or { start, until_ } to ask for (s into the clip; the server reads from start - 4 s)
--   ahead.wait(s)   -> true while what plays is past what the table has settled and more is on its way
--   s = { pos, duration, paused, covers = { a, b } or nil (all of it), settled, busy (a request under way),
--         loop = { a, b } or nil (an A-B loop) }
--
-- From a NAS the motion data is read through the video (a ~550 B sample every ~190 KB, read in 8 MB stretches): a step
-- of STEP_S costs as much reading as playing it, and the reading warms the Mac's file cache for the video's own reads.
-- 2026-11-19, 1.7 GB 4K60 clip over SMB: 20 s of clip read in ~6-8 s alone; LEAD_S keeps a step's reading twice over.

local M = {}

-- Kept settled ahead of where it plays, s: a step's reading time with the video playing, twice
M.LEAD_S = 25
-- Each step asks for this much past the lead: each new table is a new shader file for mpv to load (~15 MB of text
-- for a 2.5-minute 4K60 clip; the one swap before cost ~0.3 s of picture, 2026-11-10), so few long steps; but at most
-- half as long as the stretch it has, so the first ones come quickly (10 s, 17, 30, then 45)
M.STEP_S = 45
-- Paused (nothing else reads): steps this long until the clip's end
M.PAUSED_STEP_S = 60
-- The server reads this much before the start it's asked for (gpu_table.rs's BEFORE_S: the smoothing's warm-up)
M.BEFORE_S = 4
-- Playing within this of the end of what's settled (s): it waits. Past it the frames are the smoothing's end, which
-- bends back to the camera where the motion data stops: up to ~110 px from what the next table has there (measured,
-- vs-gyroflow/tests/table_ahead.rs), so the picture would jump as the next table came in
M.EDGE_S = 0.3

local function min(a, b) return a < b and a or b end
local function max(a, b) return a > b and a or b end

-- Where the frames it needs start and end: from where it plays, LEAD_S on; in an A-B loop from its start (it jumps
-- there) to its end, past which it doesn't play
local function needed(s)
    local from, to = s.pos, s.pos + M.LEAD_S
    local l = s.loop
    if l and l.a and l.b and l.b > l.a and s.pos >= l.a - 0.5 and s.pos <= l.b + 0.5 then
        from = min(from, l.a)
        to = min(to, l.b + 1)
    end
    return max(0, from), min(s.duration or to, to)
end

-- Whether where it plays (or the loop's start) is in the stretch: past its warm-up and before its end
local function inside(s, t)
    local c = s.covers
    if not c then return true end
    local warm = c[1] > 0 and c[1] + M.BEFORE_S or 0
    return t >= warm - 0.05 and t <= c[2]
end

function M.next(s)
    local c = s.covers
    if not c or s.busy or not s.pos then return nil end
    local duration = s.duration or math.huge
    local from, to = needed(s)
    -- Played somewhere the stretch doesn't have (a seek, a loop's start before it): from there
    if not inside(s, from) then
        return { start = from, until_ = min(duration, max(to, from + M.LEAD_S) + 5) }
    end
    if c[2] >= duration then return nil end
    local settled = s.settled or c[2]
    if settled < to then
        local step = min(M.STEP_S, max(10, (c[2] - c[1]) / 2))
        return { start = s.pos, until_ = min(duration, to + step) }
    end
    -- Paused: the rest, a step at a time
    if s.paused then return { start = s.pos, until_ = min(duration, c[2] + M.PAUSED_STEP_S) } end
    return nil
end

function M.wait(s)
    local c = s.covers
    if not c or not s.busy or not s.pos or s.paused then return false end
    if not inside(s, s.pos) then return true end
    return c[2] < (s.duration or math.huge) and s.pos >= (s.settled or c[2]) - M.EDGE_S
end

return M
