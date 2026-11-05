-- Cutting a spoiled part out of a clip (docs/lossless-cut.md): what's kept, what the parts are named, and what
-- follows the clip (its marks file, its moments). Plain Lua with no mpv in it (tests/cut_test.lua); main.lua runs
-- afp-cut, moves the original to the Trash and writes what this works out.

local moments_lib = require 'moments'

local M = {}

-- Less than this left on a side of the part cut out isn't kept: a sliver of a clip
M.MIN_PART = 0.5

-- The parts kept when [a, b) is cut out of a clip `duration` s long: { { from, to } }, `to` nil for its end. Empty
-- when nothing would be left
function M.parts(a, b, duration)
    local out = {}
    a, b = math.max(0, a or 0), math.min(duration, b or duration)
    if a >= M.MIN_PART then out[#out + 1] = { from = 0, to = a } end
    if duration - b >= M.MIN_PART then out[#out + 1] = { from = b } end
    return out
end

-- A clip's name with its recording time moved on by `secs`, for the part that starts later: DJI names say when they
-- were started, by the camera's clock (DJI_20261024094541_0093_D.MP4), and a later part gets its own time, the
-- camera's way. Others get " 2" before the extension. `taken(name)`: a name that can't be used
function M.part_name(name, secs, taken)
    local y, mo, d, h, mi, s, rest = name:match("^DJI_(%d%d%d%d)(%d%d)(%d%d)(%d%d)(%d%d)(%d%d)(_.*)$")
    local candidate
    if y then
        local t = moments_lib.days_from_civil(tonumber(y), tonumber(mo), tonumber(d)) * 86400
            + tonumber(h) * 3600 + tonumber(mi) * 60 + tonumber(s) + math.floor(secs)
        local days = math.floor(t / 86400)
        local cy, cm, cd = moments_lib.civil_from_days(days)
        local r = t - days * 86400
        candidate = string.format("DJI_%04d%02d%02d%02d%02d%02d%s", cy, cm, cd, math.floor(r / 3600), math.floor(r % 3600 / 60), r % 60, rest)
        if not taken(candidate) then return candidate end
    end
    local stem, ext = name:match("^(.*)(%.[^.]*)$")
    stem, ext = stem or name, ext or ""
    for n = 2, 99 do
        candidate = stem .. " " .. n .. ext
        if not taken(candidate) then return candidate end
    end
    return nil
end

-- The marks file of a part, from the original's: the marks in [from, to) at the part's time, `clip` as given (file,
-- size, duration, created); fields it doesn't know kept. nil when no mark falls in it
function M.part_marks(doc, from, to, clip)
    local marks = {}
    for _, m in ipairs(type(doc.marks) == "table" and doc.marks or {}) do
        if type(m) == "table" and type(m.t) == "number" and m.t >= from - 0.0005 and m.t < to then
            local copy = {}
            for k, v in pairs(m) do copy[k] = v end
            copy.t = moments_lib.ms(math.max(0, m.t - from))
            marks[#marks + 1] = copy
        end
    end
    if #marks == 0 then return nil end
    local out = {}
    for k, v in pairs(doc) do out[k] = v end
    local c = {}
    for k, v in pairs(type(doc.clip) == "table" and doc.clip or {}) do c[k] = v end
    for k, v in pairs(clip) do c[k] = v end
    out.clip, out.marks = c, marks
    return out
end

-- Where a moment [in, out] of the original goes: the part it overlaps most (parts: { { from, to } }, the times kept
-- as afp-cut cut them), with its in and out at that part's time and inside it. nil when it was all in the part cut
-- out
function M.moment_part(m_in, m_out, parts)
    local best, overlap = nil, 0
    for i, p in ipairs(parts) do
        local o = math.min(m_out, p.to) - math.max(m_in, p.from)
        if o > overlap then best, overlap = i, o end
    end
    if not best then return nil end
    local p = parts[best]
    return best, moments_lib.ms(math.max(m_in, p.from) - p.from), moments_lib.ms(math.min(m_out, p.to) - p.from)
end

-- A clip's creation time moved on by `secs`, as ffmpeg's creation_time is written (UTC, "Z")
function M.later(created, secs)
    local t = moments_lib.parse_time(created)
    if not t then return created end
    return moments_lib.format_time(t + math.floor(secs), 0):gsub("%.%d+%+00:00$", ".000000Z")
end

return M
