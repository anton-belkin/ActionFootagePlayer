-- The GPU path's table kept ahead of where a clip plays (mpv/scripts/gyroflow/ahead.lua), against a model of the table
-- server on a share of a given speed: playback never shows a frame the table has no motion data for (it waits there
-- instead), seeks and A-B loops ask for what they need, paused it reads on to the end. From the repository's folder:
-- luajit tests/gpu_ahead_test.lua

package.path = "mpv/scripts/gyroflow/?.lua;" .. package.path
local ahead = require 'ahead'

local failures, checks = 0, 0
local function expect(what, got, want)
    checks = checks + 1
    if got ~= want then
        failures = failures + 1
        print(("FAIL %s: got %s, want %s"):format(what, tostring(got), tostring(want)))
    end
end

-- The server as measured (2026-11-19, 1.7 GB 4K60 clip over SMB): a stretch is read at `speed` s of the clip a second
-- (the video's own reads meanwhile), plus `cpu` s to parse and build the table; it reads from start - 4 s and joins
-- the stretch it had when the two are within 60 s (gyroflow-gpu-table's JOIN_S)
local function server(speed, cpu)
    return function(covers, start, until_)
        local from, to = math.max(0, start - ahead.BEFORE_S), until_
        local new_read
        if covers and from <= covers[2] + 60 and to >= covers[1] - 60 then
            new_read = math.max(0, to - covers[2]) + math.max(0, covers[1] - from)
            from, to = math.min(from, covers[1]), math.max(to, covers[2])
        else
            new_read = to - from
        end
        return { from, to }, new_read / speed + cpu
    end
end

-- Plays `duration` s of a clip for `seconds` of wall time in steps of `dt`; events: { [t] = function(sim) } (a seek, a
-- pause, a loop). Returns what happened: the time it waited, the frames shown without motion data, the margin
local function play(o)
    local dt = 0.05
    local srv = server(o.speed, o.cpu or 1.5)
    local sim = { pos = o.from or 0, covers = { math.max(0, (o.from or 0) - 4), (o.from or 0) + 20 }, paused = false,
                  loop = nil, waited = 0, unstabilized = 0, unsettled = 0, min_margin = math.huge, asks = 0, duration = o.duration }
    local pending = nil
    local t = 0
    while t < o.seconds and sim.pos < o.duration - 0.05 do
        if o.events and o.events[math.floor(t * 20 + 0.5) / 20] then o.events[math.floor(t * 20 + 0.5) / 20](sim) end
        if pending and t >= pending.at then sim.covers = pending.covers; pending = nil end
        local whole = sim.covers[1] <= 0 and sim.covers[2] >= o.duration
        local s = { pos = sim.pos, duration = o.duration, paused = sim.paused, covers = not whole and sim.covers or nil,
                    settled = sim.covers[2] >= o.duration and o.duration or sim.covers[2] - 5, busy = pending ~= nil, loop = sim.loop }
        local want = ahead.next(s)
        if want then
            local covers, took = srv(sim.covers, want.start, want.until_)
            pending = { at = t + took, covers = covers }
            sim.asks = sim.asks + 1
            s.busy = true
        end
        local wait = ahead.wait(s)
        if not sim.paused then
            if wait then
                sim.waited = sim.waited + dt
            else
                local c = sim.covers
                local known = whole or (sim.pos >= (c[1] > 0 and c[1] + ahead.BEFORE_S or 0) - 0.05 and sim.pos <= c[2])
                if not known then sim.unstabilized = sim.unstabilized + dt end
                -- Past the settled end (not the clip's): the frames the next table would change
                if not whole and sim.covers[2] < o.duration and sim.pos > sim.covers[2] - 5 then sim.unsettled = sim.unsettled + dt end
                if not whole then sim.min_margin = math.min(sim.min_margin, c[2] - sim.pos) end
                sim.pos = sim.pos + dt
                if sim.loop and sim.pos >= sim.loop.b then sim.pos = sim.loop.a end
            end
        end
        t = t + dt
    end
    sim.t = t
    return sim
end

-- As measured alone on the NAS: ~3 s of clip a second read (20 s in ~6-8 s with the parse): it never waits, the
-- table stays well ahead
local r = play({ speed = 3, duration = 150, seconds = 160 })
expect("NAS speed: nothing unstabilized", r.unstabilized, 0)
expect("NAS speed: never waits", r.waited, 0)
expect("NAS speed: never past what's settled", r.unsettled, 0)
expect("NAS speed: margin over 8 s", r.min_margin > 8, true)
expect("NAS speed: played to the end", r.pos >= 149.9, true)

-- With the video and the catalog on the same share (measured: steps up to ~12 s for 25 s): still ahead
r = play({ speed = 2, cpu = 3, duration = 150, seconds = 160 })
expect("busy share: nothing unstabilized", r.unstabilized, 0)
expect("busy share: never waits", r.waited, 0)

-- Barely faster than playback: it may wait at the start, never plays unstabilized
r = play({ speed = 1.2, cpu = 3, duration = 150, seconds = 400 })
expect("slow share: nothing unstabilized", r.unstabilized, 0)
expect("slow share: never past what's settled", r.unsettled, 0)
expect("slow share: played to the end", r.pos >= 149.9, true)

-- Slower than playback (a share that can't keep up): it waits, a step at a time, and never plays the unstabilized picture
r = play({ speed = 0.6, cpu = 2, duration = 100, seconds = 600 })
expect("too slow: nothing unstabilized", r.unstabilized, 0)
expect("too slow: never past what's settled", r.unsettled, 0)
expect("too slow: it waited", r.waited > 10, true)
expect("too slow: played to the end", r.pos >= 99.9, true)

-- A seek far ahead: asked for from there at once, waits for it, then plays on stabilized
r = play({ speed = 3, duration = 300, seconds = 60, events = { [10] = function(sim) sim.pos = 200 end } })
expect("seek ahead: nothing unstabilized", r.unstabilized, 0)
expect("seek ahead: waited for its stretch", r.waited > 0, true)
expect("seek ahead: plays past it", r.pos > 220, true)

-- A seek back into the first seconds after the table moved on: the stretch has them, nothing asked for
local s = { pos = 3, duration = 300, covers = { 0, 80 }, settled = 75, busy = false }
expect("seek back inside: nothing to ask", ahead.next(s), nil)
-- Back before a stretch that starts later (opened mid-clip): from there, joined to it
s = { pos = 10, duration = 300, covers = { 96, 140 }, settled = 135, busy = false }
local n = ahead.next(s)
expect("seek back before: asks from there", n and n.start, 10)
expect("seek back before: waits for it (busy)", ahead.wait({ pos = 10, duration = 300, covers = { 96, 140 }, busy = true }), true)

-- An A-B loop inside what's known and settled: nothing more asked for while it loops
s = { pos = 30, duration = 300, covers = { 0, 50 }, settled = 45, busy = false, loop = { a = 25, b = 35 } }
expect("loop inside: nothing to ask", ahead.next(s), nil)
-- Its end past what's settled: asked for up to it (and a step on)
s = { pos = 30, duration = 300, covers = { 0, 40 }, settled = 35, busy = false, loop = { a = 25, b = 38 } }
n = ahead.next(s)
expect("loop end past settled: asks", n ~= nil, true)
-- Its start before the stretch (the stretch opened at a seek): asked for from the loop's start
s = { pos = 62, duration = 300, covers = { 56, 100 }, settled = 95, busy = false, loop = { a = 40, b = 70 } }
n = ahead.next(s)
expect("loop start before the stretch: asked from it", n and n.start, 40)
-- Looping, it plays on stabilized however long
r = play({ speed = 3, duration = 300, seconds = 120, events = { [1] = function(sim) sim.loop = { a = 5, b = 15 } end } })
expect("loop: nothing unstabilized", r.unstabilized, 0)
expect("loop: never waits", r.waited, 0)

-- Paused: the rest read a step at a time, until the clip's end
r = play({ speed = 3, duration = 200, seconds = 200, events = { [0.5] = function(sim) sim.paused = true end } })
expect("paused: read to the end", r.covers[2] >= 200, true)
expect("paused: in steps", r.asks >= 3, true)
expect("paused: not waiting", r.waited, 0)

-- The whole clip known: nothing to ask, nothing to wait for
expect("whole: nothing to ask", ahead.next({ pos = 10, duration = 100, covers = nil }), nil)
expect("whole: no wait", ahead.wait({ pos = 10, duration = 100, covers = nil, busy = true }), false)
-- The stretch reaching the clip's end (by frames, a little short of the container's duration): nothing more to ask
expect("near the end: nothing to ask", ahead.next({ pos = 140, duration = 150.10, covers = { 106, 150.08 }, settled = 150.08, busy = false }), nil)
-- The stretch reaching the clip's end: playing to the end, no wait
expect("to the end: no wait", ahead.wait({ pos = 99.9, duration = 100, covers = { 40, 100 }, settled = 100, busy = true }), false)

print(("%d checks, %d failed"):format(checks, failures))
os.exit(failures == 0 and 0 or 1)
