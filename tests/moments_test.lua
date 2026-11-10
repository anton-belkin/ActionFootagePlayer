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
        local out = what == "all" and { ".", ".." } or {}
        for _, set in ipairs({ (what == "files" or what == "all") and files or {}, (what == "dirs" or what == "all") and dirs or {} }) do
            for p in pairs(set) do
                local parent, name = p:match("^(.*)/([^/]+)$")
                if parent == d then out[#out + 1] = name end
            end
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
dir("/T/Egypt/_ActionFootagePlayer")
file("/T/Egypt/Day 1/A.MP4", 100)
file("/T/Egypt/Day 1/B.MP4", 100)
file("/T/Egypt/_Moments/Turtle.afplink.mp4", 10, 1, link("Day 1/A.MP4", 20, 30))
file("/T/Egypt/_Moments/Reef/Moray.afplink.mp4", 10, 1, link("Day 1/A.MP4", 5, 8))
file("/T/Egypt/_Moments/Other.afplink.mp4", 10, 1, (link("Day 1/B.MP4", 1, 2):gsub("06:45:41", "06:40:00")))
file("/T/Egypt/_Moments/Wrong size.afplink.mp4", 10, 1, ('{"v":1,"kind":"moment","clip":"Day 1/A.MP4","size":999,"in":1,"out":2}'))
file("/T/Egypt/_Moments/Clip preview.afplink.mp4", 10, 1, '{"v":1,"kind":"clip","clip":"Day 1/A.MP4"}')
file("/T/Egypt/_Moments/plain video.mp4", 10, 1, nil)
file("/T/Egypt/_Moments/.hidden.mp4", 10, 1, link("Day 1/A.MP4", 1, 2))
file("/T/Egypt/_Moments/notes.txt", 10, 1, nil)

local cache = {}
local found = m.scan("/T/Egypt", cache)
expect("scan: moments only", #found.list, 4)
expect("scan: dirs watched", found.dirs["/T/Egypt/_Moments/Reef"], 7)
local mine = m.of_clip(found.list, { relative = "Day 1/A.MP4", size = 100, created = "2026-10-24T09:45:41+03:00" })
expect("of clip", #mine, 2)
expect("of clip: by in", mine[1].name, "Moray")
expect("of clip: second", mine[2].name, "Turtle")
expect("of clip: other created", #m.of_clip(found.list, { relative = "Day 1/A.MP4", size = 100, created = "2025-01-01T00:00:00Z" }), 0)
expect("of clip: unknown size", #m.of_clip(found.list, { relative = "Day 1/A.MP4" }), 3)
-- Moments anywhere in the project's folder (§8): in Generated/, beside the footage when named .afplink.mp4; among
-- the footage a plain .mp4 isn't read
file("/T/Egypt/Generated/2026-11-10 14-32 Moments/Gen.mp4", 10, 1, link("Day 1/A.MP4", 40, 41))
file("/T/Egypt/Day 1/Beside.afplink.mp4", 10, 1, link("Day 1/A.MP4", 50, 51))
file("/T/Egypt/Day 2/renamed.mp4", 10, 1, link("Day 1/A.MP4", 60, 61))
local anywhere = m.scan("/T/Egypt", {})
local names = {}
for _, x in ipairs(anywhere.list) do names[x.name] = true end
expect("anywhere: in Generated", names["Gen"], true)
expect("anywhere: beside the footage", names["Beside"], true)
expect("anywhere: a plain video among the footage isn't read", names["renamed"], nil)
expect("anywhere: Generated's folder watched", anywhere.dirs["/T/Egypt/Generated/2026-11-10 14-32 Moments"], 7)
expect("anywhere: a folder with none not watched", anywhere.dirs["/T/Egypt/Day 2"], nil)
expect("anywhere: kept to a folder", #m.scan("/T/Egypt", {}, "Generated").list, 1)
local t1432 = os.time({ year = 2026, month = 11, day = 10, hour = 14, min = 32 })
expect("generated", moments_lib.generated_rel("Moments", t1432), "_Moments/Generated")
expect("generated, clip previews", moments_lib.generated_rel("Clip previews", t1432), "_Clips/Generated")
expect("generated, photos", moments_lib.generated_rel("Photos", t1432, true), "_Stills/Generated")
expect("generated, exports", moments_lib.generated_rel("Exports", t1432), "_Exports/Generated")
expect("generated, a folder per run", moments_lib.generated_rel("Moments", t1432, false, true), "_Moments/Generated/2026-11-10 14-32")
expect("generated, a folder a day", moments_lib.generated_rel("Moments", t1432, true, true), "_Moments/Generated/2026-11-10")
for _, f in ipairs({ "/T/Egypt/Generated/2026-11-10 14-32 Moments/Gen.mp4", "/T/Egypt/Day 1/Beside.afplink.mp4", "/T/Egypt/Day 2/renamed.mp4" }) do files[f] = nil end
local before = reads
m.scan("/T/Egypt", cache)
expect("cache: nothing read again", reads, before)
files["/T/Egypt/_Moments/Turtle.afplink.mp4"].mtime = 2
m.scan("/T/Egypt", cache)
expect("cache: a changed file read again", reads, before + 1)
expect("by time", m.by_time(found.list)[1].name, "Other")

-- Where a moment's original is
expect("project of", m.project_of("/T/Egypt/_Moments/Reef/Moray.afplink.mp4"), "/T/Egypt")
expect("project of: anywhere in it", m.project_of("/T/Egypt/Day 1/A.MP4"), "/T/Egypt")
expect("project of: copied out", m.project_of("/Users/me/Desktop/Turtle.afplink.mp4"), nil)
local l = fs.parse_json(link("Day 1/A.MP4", 20, 30, ',"roots":["/Old/Egypt","/T/Egypt"]'))
expect("original", m.original(l, "/T/Egypt"), "/T/Egypt/Day 1/A.MP4")
expect("original by roots", m.original(l, nil), "/T/Egypt/Day 1/A.MP4")
expect("original: size differs", m.original(fs.parse_json('{"v":1,"clip":"Day 1/A.MP4","size":5}'), "/T/Egypt"), nil)
expect("original: missing", m.original(fs.parse_json('{"v":1,"clip":"Day 9/X.MP4"}'), "/T/Egypt"), nil)
-- A clip's moments by what the clip is: the same clip at another path (moved, renamed) still has them
expect("of clip: moved", #m.of_clip(found.list, { relative = "Elsewhere/A2.MP4", size = 100, created = "2026-10-24T06:45:41Z" }), 2)
expect("of clip: same fingerprint", #m.of_clip({ { name = "x", link = { clip = "Old/A.MP4", hash = "h", size = 1, ["in"] = 1, out = 2 } } },
    { relative = "New/A.MP4", size = 1, hash = "h" }), 1)
expect("of clip: other fingerprint", #m.of_clip({ { name = "x", link = { clip = "A.MP4", hash = "h", size = 1, ["in"] = 1, out = 2 } } },
    { relative = "A.MP4", size = 1, hash = "g" }), 0)

-- A clip moved away from the path its link names: found by what it is (docs/library-management.md §8)
local T0 = moments_lib.parse_time("2026-10-24T06:45:41Z")
local hashes, created, remembered, never_rename = {}, {}, {}, false
fs.fingerprint = function(f) return hashes[f] end
fs.created = function(f) return created[f] end
fs.found_get = function(project, key) return remembered[project .. "|" .. key] end
fs.found_set = function(project, key, tree) remembered[project .. "|" .. key] = tree end
fs.never_rename = function() return never_rename end
dir("/T/Moved/_ActionFootagePlayer")
file("/T/Moved/Trip/X.MP4", 300); hashes["/T/Moved/Trip/X.MP4"] = "h1"
file("/T/Moved/Trip/Y.MP4", 300); hashes["/T/Moved/Trip/Y.MP4"] = "h2"
file("/T/Moved/a/C1.MP4", 400); created["/T/Moved/a/C1.MP4"] = T0
file("/T/Moved/b/C2.MP4", 400); created["/T/Moved/b/C2.MP4"] = T0 + 100
file("/T/Moved/New/D.MP4", 500); created["/T/Moved/New/D.MP4"] = T0 + 5
file("/T/Moved/New/E.MP4", 500); created["/T/Moved/New/E.MP4"] = T0 + 9
file("/T/Moved/New/G.MP4", 600); created["/T/Moved/New/G.MP4"] = T0 + 1
file("/T/Moved/_Moments/Y.MP4", 300); hashes["/T/Moved/_Moments/Y.MP4"] = "h1"
local function find(json) return m.original(fs.parse_json(json), "/T/Moved") end
-- The player looks only where the link says (originals move only when the library is mended after): not found
expect("moved: where the link says only", m.original(fs.parse_json('{"v":1,"clip":"Day 1/Y.MP4","size":300,"hash":"h1"}'), "/T/Moved", { hint_only = true }), nil)
local o, tree, _, how = find('{"v":1,"clip":"Day 1/Y.MP4","size":300,"hash":"h1"}')
expect("moved: by fingerprint", o, "/T/Moved/Trip/X.MP4")
expect("moved: its new path", tree, "Trip/X.MP4")
expect("moved: searched", how, "search")
expect("moved: remembered", remembered["/T/Moved|h1"], "Trip/X.MP4")
o, tree, _, how = find('{"v":1,"clip":"Day 1/Y.MP4","size":300,"hash":"h1"}')
expect("moved: from memory", how, "cache")
expect("moved: no such fingerprint", find('{"v":1,"clip":"Day 1/Y.MP4","size":300,"hash":"h9"}'), nil)
expect("moved: by creation time", (find('{"v":1,"clip":"Old/C9.MP4","size":400,"created":"2026-10-24T06:45:41Z"}')), "/T/Moved/a/C1.MP4")
expect("moved: creation time changed, by name", (find('{"v":1,"clip":"Old/D.MP4","size":500,"created":"2026-10-24T06:45:41Z"}')), "/T/Moved/New/D.MP4")
expect("moved: size alone is no find", (find('{"v":1,"clip":"Old/F.MP4","size":600,"created":"2026-10-24T06:45:41Z"}')), nil)
created["/T/Moved/New/E.MP4"] = T0 + 5
m.forget_footage()
expect("moved: two of that time, no name", (find('{"v":1,"clip":"Old/Q.MP4","size":500,"created":"2026-10-24T06:45:46Z"}')), nil)
expect("moved: name and time first", (find('{"v":1,"clip":"Old/E.MP4","size":500,"created":"2026-10-24T06:45:46Z"}')), "/T/Moved/New/E.MP4")
never_rename, remembered = true, {}
expect("moved: never renamed, the name counts", (find('{"v":1,"clip":"Old/C7.MP4","size":400,"created":"2026-10-24T06:45:41Z"}')), nil)
never_rename = false
expect("moved: at its path, nothing searched", select(4, find('{"v":1,"clip":"Trip/Y.MP4","size":300,"hash":"zz"}')), "hint")

-- Unique files
local f, n = m.unique_file("/T/Egypt/_Moments", "Turtle")
expect("unique file", f, "/T/Egypt/_Moments/Turtle 2.afplink.mp4")
expect("unique name", n, "Turtle 2")
expect("free file", (m.unique_file("/T/Egypt/_Moments", "Dolphin")), "/T/Egypt/_Moments/Dolphin.afplink.mp4")
file("/T/Egypt/_Exports/Turtle (SDR).mov", 10)
expect("unique export", (m.unique_file("/T/Egypt/_Exports", "Turtle", " (SDR).mov")), "/T/Egypt/_Exports/Turtle 2 (SDR).mov")
expect("export's project", m.project_of("/T/Egypt/_Exports/Turtle (SDR).mov", "_Exports"), "/T/Egypt")

-- Clip previews
expect("clip preview", moments_lib.clip_preview_relative("Day 1/DJI_0068_D.MP4"), "Day 1/DJI_0068_D.afplink.mp4")
expect("clip preview, card", moments_lib.clip_preview_relative("DCIM/100GOPRO/GX011057.MP4"), "GX011057.afplink.mp4")
expect("clip preview, card in a day", moments_lib.clip_preview_relative("Day 2/DCIM/DJI_001/A.MP4"), "Day 2/A.afplink.mp4")
expect("clip preview, a file named DCIM", moments_lib.clip_preview_relative("DCIM/A.MP4"), "DCIM/A.afplink.mp4")
expect("clip preview, flat", moments_lib.clip_preview_relative("Day 2/DCIM/DJI_001/A.MP4", true), "A.afplink.mp4")
local made = moments_lib.parse_made(moments_lib.made_line("Day 1/A.MP4", 100) .. "\n" .. moments_lib.made_line("B.MP4", 7) .. "\r\n")
expect("made", made["Day 1/A.MP4\t100"], true)
expect("made, other size", made["Day 1/A.MP4\t101"], nil)
expect("made, CRLF", made["B.MP4\t7"], true)
file("/T/Egypt/Day 1/A.LRF", 10)
file("/T/Egypt/.hidden/C.MP4", 10)
file("/T/Egypt/_Exports/x.mov", 10)
file("/T/Egypt/_Clips/Day 1/A.afplink.mp4", 10)
-- A Live Photo's .mov in _Stills isn't a clip
file("/T/Egypt/_Stills/A_6.00s.mov", 10)
local clips = m.footage("/T/Egypt")
local rels = {}
for _, c in ipairs(clips) do rels[#rels + 1] = c.relative end
expect("footage", table.concat(rels, ","), "Day 1/A.MP4,Day 1/B.MP4")
expect("footage: its size from the walk", clips[1].size, files["/T/Egypt/Day 1/A.MP4"].size)
-- Clip previews found by their link, whatever their name
file("/T/Egypt/_Clips/Reef/my favourite.mp4", 10, 1, '{"v":1,"kind":"clip","clip":"Day 1/B.MP4","size":100}')
local previews = m.scan("/T/Egypt", {}, "_Clips", "clip").list
expect("clip previews by link", #previews, 1)
expect("a renamed one", previews[1] and previews[1].link.clip, "Day 1/B.MP4")

-- Parts for the marks in no moment
do
    local lp = moments_lib.loose_parts
    local p = lp({ 10, 30 }, {}, 60, 2)
    expect("loose: two", #p, 2)
    expect("loose: first a", p[1].a, 8); expect("loose: first b", p[1].b, 12)
    p = lp({ 10, 13 }, {}, 60, 2)
    expect("loose: joined", #p, 1)
    expect("loose: joined a", p[1].a, 8); expect("loose: joined b", p[1].b, 15); expect("loose: joined marks", #p[1].marks, 2)
    p = lp({ 1, 59 }, {}, 60, 2)
    expect("loose: clip start", p[1].a, 0); expect("loose: clip end", p[2].b, 60)
    p = lp({ 10, 30 }, { { 9, 11 } }, 60, 2)
    expect("loose: one in a moment", #p, 1); expect("loose: the other", p[1].marks[1], 30)
    p = lp({ 30, 10 }, {}, nil, 2)
    expect("loose: sorted, no duration", p[1].marks[1], 10)
    expect("loose: none", #lp({ 10 }, { { 0, 60 } }, 60, 2), 0)
end

-- Trips: clips at most 2 days apart are one, named by their year, month and days
do
    local function at(y, mo, d) return os.time({ year = y, month = mo, day = d, hour = 12 }) end
    local tn = moments_lib.trip_name
    expect("trip: one day", tn(at(2026, 10, 20), at(2026, 10, 20)), "2026-10 October 20")
    expect("trip: days", tn(at(2026, 10, 20), at(2026, 10, 26)), "2026-10 October 20-26")
    expect("trip: months", tn(at(2026, 9, 28), at(2026, 10, 3)), "2026-09 September 28 - October 3")
    expect("trip: years", tn(at(2026, 12, 28), at(2027, 1, 2)), "2026-12 December 28 - January 2, 2027")
    local clips = {
        { relative = "A/DJI_20261020101010_0001_D.MP4" }, { relative = "A/DJI_20261022090000_0002_D.MP4" },
        { relative = "B/DJI_20261023150000_0003_D.MP4" }, { relative = "B/DJI_20261101100000_0004_D.MP4" },
        { relative = "GoPro/GX010123.MP4", mtime = at(2026, 11, 2) }, { relative = "x/no time.MP4" },
    }
    moments_lib.trips(clips)
    expect("trips: first", clips[1].trip, "2026-10 October 20-23")
    expect("trips: 2 days on", clips[3].trip, "2026-10 October 20-23")
    expect("trips: by the file's time too", clips[5].trip, "2026-11 November 1-2")
    expect("trips: no time", clips[6].trip, nil)
end

print(("%d checks, %d failed"):format(checks, failures))
os.exit(failures == 0 and 0 or 1)
