-- Frame steps and the paused frame with the GPU denoise (mpv/scripts/gyroflow/dnstep.lua), against a model of the
-- shader's ring (warp.glsl's dn_window and dn_state, gyroflow_dn_show included) and of mpv (time-pos, seeks,
-- frame-step N): after each step, the frame the shader shows is the one dnstep says is on screen (what the seek bar
-- shows), one frame from the last, with its neighbours. docs/design/seek-and-scrub.md. From the repository's folder:
-- luajit tests/dnstep_test.lua

package.path = "mpv/scripts/gyroflow/?.lua;" .. package.path
local ds = require 'dnstep'

local failures, checks = 0, 0
local function expect(what, got, want)
    checks = checks + 1
    if got ~= want then
        failures = failures + 1
        print(("FAIL %s: got %s, want %s"):format(what, tostring(got), tostring(want)))
    end
end

local FPS = 60
local F = 1 / FPS
local RADIUS, AFTER, RING = 5, 5, 11

-- The shader's ring, as warp.glsl keeps it (times in frames, to compare exactly)
local function new_ring() return { pts = {}, head = 0, ok = false } end
local function render(ring, pts, show)
    for k = 0, RING - 1 do ring.pts[k] = ring.pts[k] or -1e9 end
    local same = ring.ok and math.abs(pts - ring.pts[ring.head]) < 1e-6
    local nw = not ring.ok and 0 or (same and ring.head or (ring.head + 1) % RING)
    local function slot(k) return (nw - k + 2 * RING) % RING end
    local p = {}
    for k = 0, 2 * RADIUS do p[k] = k == 0 and pts or (ring.ok and ring.pts[slot(k)] or -1e9) end
    local back = 0
    for k = 0, 2 * RADIUS - 1 do
        local dt = p[k] - p[k + 1]
        if not (dt > 1e-6 and dt < 2.6) then break end
        back = back + 1
    end
    local kc = math.min(AFTER, back)
    local fwd, bk = kc, math.min(RADIUS, back - kc)
    if show and ring.ok then
        for k = 0, 2 * RADIUS do
            if math.abs(p[k] - show) < 0.5 then
                local in_row = k <= back
                kc, fwd, bk = k, in_row and math.min(RADIUS, k) or 0, in_row and math.min(RADIUS, back - k) or 0
                break
            end
        end
    end
    if not same then ring.head = nw; ring.pts[nw] = pts; ring.ok = true end
    return p[kc], bk, fwd
end

-- mpv and the player's Lua, in frames: t the newest frame, restart the last seek's
local function player()
    local pl = { ring = new_ring(), t = nil, restart = nil, show = nil, duration = 3000 }
    function pl:st()
        return { t = self.t * F, F = F, after = AFTER, radius = RADIUS, restart = self.restart * F,
                 show = self.show and self.show * F or nil, duration = self.duration * F }
    end
    function pl:draw() self.on_screen, self.nb_b, self.nb_a = render(self.ring, self.t, self.show) end
    function pl:seek(x) self.t, self.restart = x, x; self:draw() end
    function pl:play(n) for _ = 1, n do self.t = self.t + 1; self:draw() end end
    local function frames(x) return x and math.floor(x / F + 0.5) or nil end
    -- A step as main.lua does it
    function pl:step(dir)
        local a = ds.step(self:st(), dir)
        if not a then return end
        self.show = frames(a.show)
        if a.seek then self:seek(frames(a.seek))
        elseif a.frames > 0 then self:play(a.frames)
        else self:draw() end
    end
    -- The fill, once the steps stop
    function pl:fill()
        local x = frames(ds.shown(self:st()))
        if ds.complete(self:st(), x * F) then return false end
        local r = ds.refill(self:st(), x * F)
        self.show = x
        self:seek(frames(r.from))
        self:play(r.frames)
        return true
    end
    function pl:bar() return frames(ds.shown(self:st())) end
    return pl
end

-- Played from 600 to 634, paused: the bar says the frame on screen
local p = player()
p:seek(600)
p:play(34)
expect("paused: on screen", p.on_screen, 629)
expect("paused: the bar", p:bar(), 629)
expect("paused: clean", p.nb_b .. "+" .. p.nb_a, "5+5")

-- . three times: one frame each, clean
for i = 1, 3 do
    p:step(1)
    expect(". " .. i .. ": on screen", p.on_screen, 629 + i)
    expect(". " .. i .. ": the bar", p:bar(), 629 + i)
    expect(". " .. i .. ": clean", p.nb_b .. "+" .. p.nb_a, "5+5")
end
-- , back: from the ring, at once, no seek, no jump; the frames before it fewer each step
local newest = p.t
for i = 1, 5 do
    p:step(-1)
    expect(", " .. i .. ": on screen", p.on_screen, 632 - i)
    expect(", " .. i .. ": the bar", p:bar(), 632 - i)
    expect(", " .. i .. ": no decoding", p.t, newest)
end
expect(", 5: frames before it (the oldest of the ring)", p.nb_b, 0)
-- . again: from the ring too
p:step(1)
expect(", then .: on screen", p.on_screen, 628)
expect(", then .: no decoding", p.t, newest)
-- The steps stop: filled again around it, the same frame, clean
expect("fill needed", p:fill(), true)
expect("fill: on screen", p.on_screen, 628)
expect("fill: the bar", p:bar(), 628)
expect("fill: clean", p.nb_b .. "+" .. p.nb_a, "5+5")
expect("fill: not again", p:fill(), false)
-- , past the ring: a seek to the frame, on screen at once (as decoded), then filled
for _ = 1, 6 do p:step(-1) end
expect("6 ,: on screen", p.on_screen, 622)
expect("6 ,: the bar", p:bar(), 622)
p:step(-1)
expect("7th , past the ring: on screen", p.on_screen, 621)
p:step(-1)
expect("8th , past the ring: on screen", p.on_screen, 620)
p:fill()
expect("filled: on screen", p.on_screen, 620)
expect("filled: clean", p.nb_b .. "+" .. p.nb_a, "5+5")

-- A click on the bar while paused (a seek of the user's: main.lua clears show): the frame at once, as decoded
p.show = nil
p:seek(1200)
expect("click: on screen", p.on_screen, 1200)
expect("click: the bar", p:bar(), 1200)
-- . right after it: one frame on, not the same frame five times
p:step(1)
expect("click, .: on screen", p.on_screen, 1201)
expect("click, .: the bar", p:bar(), 1201)
p:step(1)
expect("click, . .: on screen", p.on_screen, 1202)
-- Filled while paused: the frame stays, clean
p:fill()
expect("click, filled: on screen", p.on_screen, 1202)
expect("click, filled: clean", p.nb_b .. "+" .. p.nb_a, "5+5")

-- The fill holds the frame on screen while it seeks before it and plays up to it
local q = player()
q:seek(300)
q.show = 300
local r = ds.refill(q:st(), 300 * F)
q:seek(math.floor(r.from / F + 0.5))
expect("fill under way: the frame held", q.on_screen, 300)
for i = 1, r.frames do
    q:play(1)
    expect("fill under way, frame " .. i .. ": the frame held", q.on_screen, 300)
end

-- The clip's start and end: no step past them, fewer neighbours wanted
local e = player()
e.duration = 100
e:seek(0)
e.show = 0
expect("at the start: , does nothing", ds.step(e:st(), -1), nil)
e:fill()
expect("start filled: on screen", e.on_screen, 0)
expect("start filled: none before, 5 after", e.nb_b .. "+" .. e.nb_a, "0+5")
e.show = nil
e:seek(99)
e.show = 99
expect("at the end: . does nothing", ds.step(e:st(), 1), nil)
expect("at the end: complete", ds.complete(e:st(), 99 * F), false)
e:fill()
expect("end filled: 5 before, none after", e.nb_b .. "+" .. e.nb_a, "5+0")

-- Playing on from a frame stepped back to: a seek to it first; from the shader's own frame, none
local g = player()
g:seek(500); g:play(20)
expect("play from the natural frame: no seek", ds.play_from(g:st()), nil)
g:step(-1)
expect("play from a step back: seek to it", math.floor(ds.play_from(g:st()) / F + 0.5), 514)

-- Without the denoise's frames after (crop mode: after 0) the newest is the one shown
expect("after 0: time-pos", ds.shown({ t = 2.0, F = F, after = 0, radius = 5, restart = 1.0 }), 2.0)

print(("%d checks, %d failed"):format(checks, failures))
os.exit(failures == 0 and 0 or 1)
