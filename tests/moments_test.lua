-- The moments library (mpv/scripts/workspace/moments.lua): names, dates, which clip a moment is of, where its
-- original is, the Moments/ folder read, on a folder tree it fakes. From the repository's folder:
-- luajit tests/moments_test.lua

package.path = "mpv/scripts/workspace/?.lua;" .. package.path
local workspace = require 'workspace'
local moments_lib = require 'moments'

local failures, checks = 0, 0
local function expect(what, got, want)
    checks = checks + 1
    if got ~= want then
        failures = failures + 1
        print(("FAIL %s: got %s, want %s"):format(what, tostring(got), tostring(want)))
    end
end

-- Names
local safe = moments_lib.safe_name
expect("plain", safe("Turtle"), "Turtle")
expect("cyrillic", safe("Черепаха у рифа"), "Черепаха у рифа")
expect("forbidden", safe('a/b\\c:d*e?f"g<h>i|j'), "a-b-c-d-e-f-g-h-i-j")
expect("control", safe("a\tb\nc"), "a-b-c")
expect("dots and spaces", safe("  ..Turtle.. "), "Turtle")
expect("empty", safe(" . "), nil)
expect("reserved", safe("con"), "con-")
expect("reserved with ext", safe("NUL.txt"), "NUL.txt-")
expect("not reserved", safe("CONSOLE"), "CONSOLE")
local long = safe(("ж"):rep(150))
expect("long: bytes", #long <= 200, true)
expect("long: whole letters", #long % 2, 0)
expect("display", moments_lib.display_name("/p/Moments/Turtle 2.afplink.mp4"), "Turtle 2")
expect("display upper", moments_lib.display_name("C:\\p\\Moments\\Reef.AFPLINK.MP4"), "Reef")
expect("display renamed", moments_lib.display_name("/p/Moments/Reef.mp4"), "Reef")
local taken = { ["turtle"] = true, ["turtle 2"] = true }
expect("next", moments_lib.next_name("Turtle", taken), "Turtle 3")
expect("next numbered", moments_lib.next_name("Turtle 2", taken), "Turtle 3")
expect("next free", moments_lib.next_name("Reef", taken), "Reef 2")
expect("next 9", moments_lib.next_name("Dive 9", {}), "Dive 10")

-- Dates
local t, off = moments_lib.parse_time("2026-10-24T06:45:41.000000Z")
expect("utc", t, moments_lib.days_from_civil(2026, 10, 24) * 86400 + 6 * 3600 + 45 * 60 + 41)
expect("utc offset", off, 0)
local t2, off2 = moments_lib.parse_time("2026-10-24T09:45:41+03:00")
expect("zoned", t2, t)
expect("zoned offset", off2, 180)
expect("compact zone", (moments_lib.parse_time("2026-10-24T09:45:41+0300")), t)
expect("no zone", select(2, moments_lib.parse_time("2026-10-24T06:45:41")), nil)
expect("bad", moments_lib.parse_time("yesterday"), nil)
expect("format", moments_lib.format_time(t + 26.4, 180), "2026-10-24T09:46:07.400+03:00")
expect("format west", moments_lib.format_time(t, -300), "2026-10-24T01:45:41.000-05:00")
expect("format midnight", moments_lib.format_time(moments_lib.days_from_civil(2027, 1, 1) * 86400, 60), "2027-01-01T01:00:00.000+01:00")
expect("time name", moments_lib.time_name(t + 26.4, 180), "2026-10-24 09.46.07")
expect("DJI zone", moments_lib.zone_of_name("DJI_20261024094541_0093_D.MP4", t), 180)
expect("DJI zone, seconds off", moments_lib.zone_of_name("DJI_20261024094541_0093_D.MP4", t + 3), 180)
expect("GoPro: none", moments_lib.zone_of_name("GX011057.MP4", t), nil)
expect("local zone is a number", type(moments_lib.local_zone(t)), "number")
expect("moment time", moments_lib.moment_time({ created = "2026-10-24T06:45:41Z", ["in"] = 20.5 }), t + 20.5)

-- A project, faked
local files, dirs = {}, {}
local function file(p, size, mtime, link) files[p] = { size = size, mtime = mtime or 1, link = link }
    local d = p:match("^(.*)/[^/]+$")
    while d and d ~= "" do dirs[d] = true; d = d:match("^(.*)/[^/]+$") end
end
local function dir(p) dirs[p] = true end
local reads = 0
local fs = {
    is_dir = function(p) return dirs[p] == true end,
    is_file = function(p) return files[p] ~= nil end,
    info = function(p)
        if files[p] then return { is_file = true, size = files[p].size, mtime = files[p].mtime } end
        if dirs[p] then return { is_dir = true, mtime = 7 } end
    end,
    readdir = function(d, what)
        local out = {}
        local set = what == "files" and files or dirs
        for p in pairs(set) do
            local parent, name = p:match("^(.*)/([^/]+)$")
            if parent == d then out[#out + 1] = name end
        end
        table.sort(out)
        return out
    end,
    read_link = function(p) reads = reads + 1; return files[p] and files[p].link end,
    parse_json = function(s)
        -- The few links here, by hand: not a JSON parser
        local l = {}
        for k, v in s:gmatch('"([%w_]+)":"([^"]*)"') do l[k] = v end
        for k, v in s:gmatch('"([%w_]+)":(%-?[%d%.]+)') do l[k] = tonumber(v) end
        local roots = s:match('"roots":%[(.-)%]')
        if roots then l.roots = {}; for r in roots:gmatch('"([^"]*)"') do l.roots[#l.roots + 1] = r end end
        return l
    end,
}
local ws = workspace.new({ is_dir = fs.is_dir, is_file = fs.is_file, home = "/Users/me" })
local m = moments_lib.new(ws, fs)
local function link(clip, a, b, extra)
    return ('{"v":1,"kind":"moment","clip":"%s","size":100,"created":"2026-10-24T06:45:41Z","in":%s,"out":%s%s}'):format(clip, a, b, extra or "")
end
dir("/T/Egypt/.actionfootageplayer")
file("/T/Egypt/Day 1/A.MP4", 100)
file("/T/Egypt/Day 1/B.MP4", 100)
file("/T/Egypt/Moments/Turtle.afplink.mp4", 10, 1, link("Day 1/A.MP4", 20, 30))
file("/T/Egypt/Moments/Reef/Moray.afplink.mp4", 10, 1, link("Day 1/A.MP4", 5, 8))
file("/T/Egypt/Moments/Other.afplink.mp4", 10, 1, link("Day 1/B.MP4", 1, 2))
file("/T/Egypt/Moments/Wrong size.afplink.mp4", 10, 1, ('{"v":1,"kind":"moment","clip":"Day 1/A.MP4","size":999,"in":1,"out":2}'))
file("/T/Egypt/Moments/Clip preview.afplink.mp4", 10, 1, '{"v":1,"kind":"clip","clip":"Day 1/A.MP4"}')
file("/T/Egypt/Moments/plain video.mp4", 10, 1, nil)
file("/T/Egypt/Moments/.hidden.mp4", 10, 1, link("Day 1/A.MP4", 1, 2))
file("/T/Egypt/Moments/notes.txt", 10, 1, nil)

local cache = {}
local found = m.scan("/T/Egypt", cache)
expect("scan: moments only", #found.list, 4)
expect("scan: dirs watched", found.dirs["/T/Egypt/Moments/Reef"], 7)
local mine = m.of_clip(found.list, { relative = "Day 1/A.MP4", size = 100, created = "2026-10-24T09:45:41+03:00" })
expect("of clip", #mine, 2)
expect("of clip: by in", mine[1].name, "Moray")
expect("of clip: second", mine[2].name, "Turtle")
expect("of clip: other created", #m.of_clip(found.list, { relative = "Day 1/A.MP4", size = 100, created = "2025-01-01T00:00:00Z" }), 0)
expect("of clip: unknown size", #m.of_clip(found.list, { relative = "Day 1/A.MP4" }), 3)
local before = reads
m.scan("/T/Egypt", cache)
expect("cache: nothing read again", reads, before)
files["/T/Egypt/Moments/Turtle.afplink.mp4"].mtime = 2
m.scan("/T/Egypt", cache)
expect("cache: a changed file read again", reads, before + 1)
expect("by time", m.by_time(found.list)[1].name, "Other")

-- Where a moment's original is
expect("project of", m.project_of("/T/Egypt/Moments/Reef/Moray.afplink.mp4"), "/T/Egypt")
expect("project of: not in Moments", m.project_of("/T/Egypt/Day 1/A.MP4"), nil)
expect("project of: copied out", m.project_of("/Users/me/Desktop/Turtle.afplink.mp4"), nil)
local l = fs.parse_json(link("Day 1/A.MP4", 20, 30, ',"roots":["/Old/Egypt","/T/Egypt"]'))
expect("original", m.original(l, "/T/Egypt"), "/T/Egypt/Day 1/A.MP4")
expect("original by roots", m.original(l, nil), "/T/Egypt/Day 1/A.MP4")
expect("original: size differs", m.original(fs.parse_json('{"v":1,"clip":"Day 1/A.MP4","size":5}'), "/T/Egypt"), nil)
expect("original: missing", m.original(fs.parse_json('{"v":1,"clip":"Day 9/X.MP4"}'), "/T/Egypt"), nil)

-- Unique files
local f, n = m.unique_file("/T/Egypt/Moments", "Turtle")
expect("unique file", f, "/T/Egypt/Moments/Turtle 2.afplink.mp4")
expect("unique name", n, "Turtle 2")
expect("free file", (m.unique_file("/T/Egypt/Moments", "Dolphin")), "/T/Egypt/Moments/Dolphin.afplink.mp4")

print(("%d checks, %d failed"):format(checks, failures))
os.exit(failures == 0 and 0 or 1)
