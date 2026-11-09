-- The 360 view recorded as you look around (docs/360-reframe.md, "Recording"): takes of the head, frame by frame,
-- relative to what the view follows; punch-in, undo, thinning, and the head at any time of the clip. Plain Lua, no mpv
-- (tests/view360_test.lua); main.lua feeds it the frames shown and keeps it in the clip's .afp.json.
--
--   local recording = require 'recording'
--   local r = recording.new()
--   r:load(t)                     from the sidecar's view360.recording (anything else: an empty recording)
--   r:to_table()                  for the sidecar: { follows, parts = { { from, to, samples } }, frame, size, ... },
--                                 nil when nothing's recorded; keys it doesn't know kept; tidy: how it's tidied
--   r:start(follows)              a take starts: true, or false and why (the recording follows another mode)
--   r:add(t, yaw, pitch, roll, fov, projection)   a frame shown during the take; false when t went back (a seek):
--                                 the caller ends the take
--   r:stop()                      the take ends: thinned, punched in, undoable. Returns the new part, or nil when the
--                                 take was too short to keep
--   r:taking()                    whether a take is on; r:take_seconds() how long it has run
--   r:undo()                      the last take taken back (what it replaced restored): true, or false with none left
--   r:clear()                     nothing recorded (and no undo)
--   r:head(t)                     { yaw, pitch, roll, fov, projection } at t, or nil with nothing recorded
--   r:spans()                     the parts joined where they touch: { { from, to } }
--   r:empty()
--
-- A sample is [t, yaw, pitch, roll, fov] in the file, with the projection as a sixth value where it changes (and on a
-- part's first sample); in memory every sample has it. The yaw is unwrapped within a take (each sample within 180° of
-- the one before), so playing back between two samples never turns the long way round.
--
-- Where nothing is recorded the head is held: before the first part, its first sample; between two parts, the end of
-- the one before; after the last, its end. Between two samples it's interpolated in a straight line (the export does
-- the same: vs-gyroflow/src/reframe.rs), except across a change of projection, which is a cut at the later sample.

local M = {}

-- Undo goes back this many takes
M.UNDO = 20
-- A sample is dropped where the head moved less than this from the line between its neighbours (degrees; the FOV as
-- 100 · ln(fov), so 0.05 is 0.05° at 100°)
M.TOLERANCE = 0.05
-- Where a take cuts into a part, the part's remains end this long before it and start this long after it (seconds),
-- so the parts never share a time
M.EPSILON = 0.001
M.FRAMES = { ["16:9"] = true, ["9:16"] = true, ["1:1"] = true }
M.SIZES = { ["1080p"] = true, ["4K"] = true }
M.FOLLOWS = { travel = true, camera = true, north = true, world = true }
-- How the recording is tidied (reframe.rs does it: gyroflow-probe --reframe for the preview, the export the same):
-- the turns smoothed (a Gaussian of 0.25, 0.6 or 1.5 s) and the horizon level. Not there: medium and level
M.TURNS = { off = true, light = true, medium = true, strong = true }
function M.default_tidy() return { turns = "medium", level = true, picture = false } end
local PROJECTIONS = { auto = true, rectilinear = true, stereographic = true, planet = true }

local function wrap(a)
    a = a % 360
    if a > 180 then a = a - 360 end
    return a
end

local function copy_sample(s)
    return { t = s.t, yaw = s.yaw, pitch = s.pitch, roll = s.roll, fov = s.fov, projection = s.projection }
end

local function copy_part(p)
    local samples = {}
    for i, s in ipairs(p.samples) do samples[i] = copy_sample(s) end
    return { from = p.from, to = p.to, samples = samples }
end

-- The head within a part's samples at t (t inside them): a straight line between two, held across a projection change
local function within(samples, t)
    local n = #samples
    if t <= samples[1].t then return copy_sample(samples[1]) end
    if t >= samples[n].t then return copy_sample(samples[n]) end
    local lo, hi = 1, n
    while hi - lo > 1 do
        local mid = math.floor((lo + hi) / 2)
        if samples[mid].t <= t then lo = mid else hi = mid end
    end
    local a, b = samples[lo], samples[hi]
    if a.projection ~= b.projection or b.t <= a.t then
        local s = copy_sample(a); s.t = t; return s
    end
    local x = (t - a.t) / (b.t - a.t)
    local function mix(p, q) return p + (q - p) * x end
    return { t = t, yaw = mix(a.yaw, b.yaw), pitch = mix(a.pitch, b.pitch), roll = mix(a.roll, b.roll),
             fov = a.fov == b.fov and a.fov or math.exp(mix(math.log(a.fov), math.log(b.fov))),
             projection = a.projection }
end
M.within = within

-- Ramer-Douglas-Peucker over a run of samples of one projection: the error of a dropped sample is how far the straight
-- line between the kept ones misses it, in any of the angles or the FOV's log
local function thin_run(samples, i0, i1, keep, tol)
    keep[i0], keep[i1] = true, true
    local stack = { { i0, i1 } }
    while #stack > 0 do
        local seg = table.remove(stack)
        local a, b = seg[1], seg[2]
        if b - a > 1 then
            local sa, sb = samples[a], samples[b]
            local dt = sb.t - sa.t
            local la, lb = 100 * math.log(sa.fov), 100 * math.log(sb.fov)
            local worst, at = -1, nil
            for i = a + 1, b - 1 do
                local s = samples[i]
                local x = dt > 0 and (s.t - sa.t) / dt or 0
                local e = math.max(math.abs(s.yaw - (sa.yaw + (sb.yaw - sa.yaw) * x)),
                                   math.abs(s.pitch - (sa.pitch + (sb.pitch - sa.pitch) * x)),
                                   math.abs(s.roll - (sa.roll + (sb.roll - sa.roll) * x)),
                                   math.abs(100 * math.log(s.fov) - (la + (lb - la) * x)))
                if e > worst then worst, at = e, i end
            end
            if worst > tol then
                keep[at] = true
                table.insert(stack, { a, at })
                table.insert(stack, { at, b })
            end
        end
    end
end

-- The samples thinned: each run of one projection on its own, its ends kept
function M.thin(samples, tol)
    tol = tol or M.TOLERANCE
    local n = #samples
    if n <= 2 then return samples end
    local keep = {}
    local start = 1
    for i = 2, n + 1 do
        if i > n or samples[i].projection ~= samples[start].projection then
            thin_run(samples, start, i - 1, keep, tol)
            start = i
        end
    end
    local out = {}
    for i = 1, n do if keep[i] then table.insert(out, samples[i]) end end
    return out
end

local R = {}
R.__index = R

function M.new()
    return setmetatable({ follows = nil, parts = {}, frame = "16:9", size = "1080p", tidy = M.default_tidy(), extra = {}, history = {},
                          take = nil }, R)
end

function R:empty() return #self.parts == 0 end
function R:taking() return self.take ~= nil end

function R:take_seconds()
    local s = self.take and self.take.samples
    if not s or #s == 0 then return 0 end
    return s[#s].t - s[1].t
end

local function num(v) return type(v) == "number" and v == v and v ~= math.huge and v ~= -math.huge end

-- From the sidecar's table: what's not right is left out (a part without two good samples, a follows it doesn't know)
function R:load(t)
    self.follows, self.parts, self.extra, self.history, self.take = nil, {}, {}, {}, nil
    self.frame, self.size, self.tidy = "16:9", "1080p", M.default_tidy()
    if type(t) ~= "table" then return self end
    for k, v in pairs(t) do
        if k ~= "follows" and k ~= "parts" and k ~= "frame" and k ~= "size" and k ~= "tidy" then self.extra[k] = v end
    end
    if type(t.tidy) == "table" then
        if M.TURNS[t.tidy.turns] then self.tidy.turns = t.tidy.turns end
        if type(t.tidy.level) == "boolean" then self.tidy.level = t.tidy.level end
        if type(t.tidy.picture) == "boolean" then self.tidy.picture = t.tidy.picture end
    end
    if M.FRAMES[t.frame] then self.frame = t.frame end
    if M.SIZES[t.size] then self.size = t.size end
    if not M.FOLLOWS[t.follows] then return self end
    for _, p in ipairs(type(t.parts) == "table" and t.parts or {}) do
        local samples, projection, last = {}, "auto", nil
        for _, s in ipairs(type(p) == "table" and type(p.samples) == "table" and p.samples or {}) do
            if type(s) == "table" and num(s[1]) and num(s[2]) and num(s[3]) and num(s[4]) and num(s[5]) and s[5] > 0
               and (last == nil or s[1] > last) then
                if PROJECTIONS[s[6]] then projection = s[6] end
                table.insert(samples, { t = s[1], yaw = s[2], pitch = s[3], roll = s[4], fov = s[5], projection = projection })
                last = s[1]
            end
        end
        if #samples >= 2 then
            table.insert(self.parts, { from = samples[1].t, to = samples[#samples].t, samples = samples })
        end
    end
    table.sort(self.parts, function(a, b) return a.from < b.from end)
    if #self.parts > 0 then self.follows = t.follows end
    return self
end

local function round(v, places)
    local m = 10 ^ places
    return math.floor(v * m + 0.5) / m
end

function R:to_table()
    if #self.parts == 0 then return nil end
    local t = {}
    for k, v in pairs(self.extra) do t[k] = v end
    t.follows, t.frame, t.size = self.follows, self.frame, self.size
    t.tidy = { turns = self.tidy.turns, level = self.tidy.level, picture = self.tidy.picture }
    t.parts = {}
    for _, p in ipairs(self.parts) do
        local samples, projection = {}, nil
        for _, s in ipairs(p.samples) do
            local row = { round(s.t, 4), round(s.yaw, 3), round(s.pitch, 3), round(s.roll, 3), round(s.fov, 3) }
            if s.projection ~= projection then row[6] = s.projection; projection = s.projection end
            table.insert(samples, row)
        end
        table.insert(t.parts, { from = round(p.from, 4), to = round(p.to, 4), samples = samples })
    end
    return t
end

function R:start(follows)
    if not M.FOLLOWS[follows] then return false, "unknown" end
    if #self.parts > 0 and self.follows ~= follows then return false, self.follows end
    self.take = { follows = follows, samples = {} }
    return true
end

function R:add(t, yaw, pitch, roll, fov, projection)
    local take = self.take
    if not take or not num(t) or not num(yaw) or not num(fov) or fov <= 0 then return true end
    local s = take.samples
    local last = s[#s]
    if last and t < last.t then return false end
    if last and t == last.t then
        -- The same frame again (a redraw): the newest head
        table.remove(s)
        last = s[#s]
    end
    if last then yaw = last.yaw + wrap(yaw - last.yaw) end
    table.insert(s, { t = t, yaw = yaw, pitch = pitch or 0, roll = roll or 0, fov = fov,
                      projection = PROJECTIONS[projection] and projection or "auto" })
    return true
end

-- What's left of part q outside [from, to]: the part before it and the one after, each ending (starting) with the
-- head where the take cut in
local function cut(q, from, to)
    local out = {}
    local e = M.EPSILON
    if q.from < from - e then
        local samples = {}
        for _, s in ipairs(q.samples) do if s.t < from - e then table.insert(samples, s) end end
        table.insert(samples, within(q.samples, from - e))
        table.insert(out, { from = samples[1].t, to = samples[#samples].t, samples = samples })
    end
    if q.to > to + e then
        local samples = { within(q.samples, to + e) }
        for _, s in ipairs(q.samples) do if s.t > to + e then table.insert(samples, s) end end
        table.insert(out, { from = samples[1].t, to = samples[#samples].t, samples = samples })
    end
    return out
end

local function remove(list, item)
    for i, x in ipairs(list) do if x == item then table.remove(list, i) return true end end
    return false
end

function R:stop()
    local take = self.take
    self.take = nil
    if not take or #take.samples < 2 then return nil end
    local samples = M.thin(take.samples)
    local part = { from = samples[1].t, to = samples[#samples].t, samples = samples }
    local record = { removed = {}, added = { part }, follows = self.follows }
    local kept = {}
    for _, q in ipairs(self.parts) do
        if q.from <= part.to and q.to >= part.from then
            table.insert(record.removed, q)
            for _, r in ipairs(cut(q, part.from, part.to)) do
                table.insert(kept, r)
                table.insert(record.added, r)
            end
        else
            table.insert(kept, q)
        end
    end
    table.insert(kept, part)
    table.sort(kept, function(a, b) return a.from < b.from end)
    self.parts = kept
    self.follows = take.follows
    table.insert(self.history, record)
    while #self.history > M.UNDO do table.remove(self.history, 1) end
    return part
end

function R:undo()
    local record = table.remove(self.history)
    if not record then return false end
    for _, p in ipairs(record.added) do remove(self.parts, p) end
    for _, p in ipairs(record.removed) do table.insert(self.parts, p) end
    table.sort(self.parts, function(a, b) return a.from < b.from end)
    self.follows = #self.parts > 0 and record.follows or nil
    return true
end

function R:undoable() return #self.history end

-- Nothing to tidy: the samples play as recorded
function R:untidied() return self.tidy.turns == "off" and not self.tidy.level end

function R:clear()
    self.parts, self.history, self.follows, self.take = {}, {}, nil, nil
end

function R:head(t)
    local parts = self.parts
    if #parts == 0 then return nil end
    if t < parts[1].from then return copy_sample(parts[1].samples[1]) end
    local lo, hi = 1, #parts
    while lo < hi do
        local mid = math.floor((lo + hi + 1) / 2)
        if parts[mid].from <= t then lo = mid else hi = mid - 1 end
    end
    return within(parts[lo].samples, t)
end

-- The parts as spans, joined where one starts within `gap` seconds (a frame or so) of the end of the one before
function R:spans(gap)
    gap = gap or 0.1
    local out = {}
    for _, p in ipairs(self.parts) do
        local last = out[#out]
        if last and p.from <= last.to + gap then
            last.to = math.max(last.to, p.to)
        else
            table.insert(out, { from = p.from, to = p.to })
        end
    end
    return out
end

M.copy_part = copy_part
return M
