-- The navigation bar's reckoning and the Playlist's rows (mpv/scripts/side-panel/timeline.lua).
-- From the repository's folder: luajit tests/timeline_test.lua

package.path = "mpv/scripts/side-panel/?.lua;" .. package.path
local tl = require 'timeline'

local failures, checks = 0, 0
local function expect(what, got, want)
    checks = checks + 1
    if got ~= want then
        failures = failures + 1
        print(("FAIL %s: got %s, want %s"):format(what, tostring(got), tostring(want)))
    end
end
local function near(what, got, want)
    checks = checks + 1
    if type(got) ~= "number" or math.abs(got - want) > 1e-6 then
        failures = failures + 1
        print(("FAIL %s: got %s, want %s"):format(what, tostring(got), tostring(want)))
    end
end

-- Times as text
expect("clock 0", tl.clock(0), "0:00.00")
expect("clock hundredths", tl.clock(31.244), "0:31.24")
expect("clock rounds up into the minute", tl.clock(59.999), "1:00.00")
expect("clock an hour", tl.clock(3723.456), "1:02:03.46")
expect("clock whole", tl.clock(44.6, 0), "0:45")
expect("clock negative", tl.clock(-12.5), "-0:12.50")
expect("clock nil", tl.clock(nil), "–:––")

-- The buttons that fit: the least kept go first, the later of equals first, the order kept
local items = {
    { id = "prev", w = 30, keep = 3 }, { id = "play", w = 30, keep = math.huge }, { id = "next", w = 30, keep = 3 },
    { id = "time", w = 100, keep = math.huge }, { id = "speed", w = 30, keep = 2 }, { id = "loop", w = 30, keep = 2 },
    { id = "fs", w = 30, keep = 7 },
}
local function ids(list)
    local out = {}
    for _, it in ipairs(list) do out[#out + 1] = it.id end
    return table.concat(out, ",")
end
expect("fit: all", ids((tl.fit(items, 1000))), "prev,play,next,time,speed,loop,fs")
expect("fit: the later of the least kept first", ids((tl.fit(items, 250))), "prev,play,next,time,speed,fs")
expect("fit: both least kept", ids((tl.fit(items, 220))), "prev,play,next,time,fs")
expect("fit: then the clips", ids((tl.fit(items, 160))), "play,time,fs")
expect("fit: what's always kept stays", ids((tl.fit(items, 10))), "play,time")
local _, total = tl.fit(items, 220)
expect("fit: total", total, 220)

-- A time's place on the track and back
near("x_of middle", tl.x_of(5, 100, 300, 10), 200)
near("x_of past the end", tl.x_of(20, 100, 300, 10), 300)
near("x_of before 0", tl.x_of(-1, 100, 300, 10), 100)
near("x_of no duration", tl.x_of(5, 100, 300, nil), 100)
near("t_of", tl.t_of(150, 100, 300, 10), 2.5)
near("t_of left of the track", tl.t_of(50, 100, 300, 10), 0)
near("t_of right of it", tl.t_of(400, 100, 300, 10), 10)

-- What's on the track: the marks (the camera's, yours), the moments, the file's own chapters (not the marks' and
-- the moments' chapters again)
local highlights = { marks = { { t = 26.376, camera = "dji" }, { t = 8.2 }, { t = 33.5, name = "Turtle" } } }
local workspace = { moments = { { name = "Ray", ["in"] = 12, out = 18.5 } } }
local chapters = { { time = 8.2, title = "Mark 1" }, { time = 12, title = "Ray" }, { time = 18.5, title = "Ray end" },
                   { time = 20, title = "Chapter 2" }, { time = 26.376, title = "Highlight 2" }, { time = 33.5, title = "Turtle" } }
local it = tl.track_items(highlights, workspace, chapters)
expect("marks", #it.marks, 3)
expect("marks in order", it.marks[1].t, 8.2)
expect("own mark", it.marks[1].kind, "own")
expect("own mark's title", it.marks[1].title, "Mark 2")
expect("camera's", it.marks[2].kind, "camera")
expect("camera's title", it.marks[2].title, "Highlight 1")
expect("named", it.marks[3].title, "Turtle")
expect("moments", #it.moments, 1)
expect("moment", it.moments[1].title .. " " .. it.moments[1].a .. "-" .. it.moments[1].b, "Ray 12-18.5")
expect("only the file's own chapters", #it.chapters, 1)
expect("the file's chapter", it.chapters[1].title, "Chapter 2")
-- A moment playing as a video of its own: the clip's moments aren't on its track
local inside = tl.track_items(highlights, { offset = 10, moments = workspace.moments }, {})
expect("moment playing: no moments", #inside.moments, 0)
local empty = tl.track_items(nil, nil, nil)
expect("nothing", #empty.marks + #empty.moments + #empty.chapters, 0)

-- The point near the mouse: a mark first, within reach
-- (a track 0..1000 px for 50 s: 20 px a second)
local s1 = tl.snap(670, it, nil, nil, 0, 1000, 50, 7)
expect("snap to the mark", s1 and s1.title, "Turtle")
near("snap: its time", s1 and s1.t, 33.5)
expect("too far", tl.snap(690, it, nil, nil, 0, 1000, 50, 7), nil)
local s2 = tl.snap(581, it, 29, 36.5, 0, 1000, 50, 7)
expect("the part's start", s2 and s2.title, "Part start")
local s3 = tl.snap(371, it, nil, nil, 0, 1000, 50, 7)
expect("a moment's end", s3 and s3.title, "Ray end")
-- A mark and a chapter as near: the mark
local tie = tl.track_items({ marks = { { t = 10 } } }, nil, { { time = 10.4, title = "Chapter" } })
local s4 = tl.snap(204, tie, nil, nil, 0, 1000, 50, 7)
expect("a mark before a chapter as near", s4 and s4.title, "Mark 1")

-- The A–B button
expect("ab none", tl.ab_state("no", "no"), "none")
expect("ab start", tl.ab_state("12.5", "no"), "a")
expect("ab both", tl.ab_state(12.5, 20), "ab")

-- The playlist's rows: the videos only (not the folders mpv put in it), the one playing marked, names without the
-- extension, the indexes mpv's
local playlist = {
    { filename = "/v/DJI_0068_D.MP4" }, { filename = "/v/DJI_0068_D.LRF" }, { filename = "/v/stabilized" },
    { filename = "/v/DJI_0069_D.MP4", current = true, playing = true }, { filename = "/v/notes.txt" },
    { filename = "/v/clip.mov", title = "Turtle" },
}
local rows = tl.playlist_rows(playlist, tl.set_of({ "mp4", "mov" }))
expect("rows", #rows, 3)
expect("row 1", rows[1].name, "DJI_0068_D")
expect("row 1 index", rows[1].index, 0)
expect("current", rows[2].current, true)
expect("current index", rows[2].index, 3)
expect("current name", rows[2].name, "DJI_0069_D")
expect("title", rows[3].name, "Turtle")
expect("not current", rows[1].current, false)
-- The one playing even when it isn't a video by its extension
local odd = tl.playlist_rows({ { filename = "/v/a.mp4" }, { filename = "/v/b.xyz", current = true } }, tl.set_of({ "mp4" }))
expect("the one playing stays", #odd, 2)
expect("no list", #tl.playlist_rows(nil), 0)
-- Windows paths
local win = tl.playlist_rows({ { filename = "C:\\Trips\\Egypt\\GX010123.MP4" } }, tl.set_of({ "mp4" }))
expect("windows name", win[1].name, "GX010123")

-- The folder they're in
expect("common folder", tl.common_folder({ { filename = "/v/a.mp4" }, { filename = "/v/b.mp4" } }), "/v")
expect("two folders", tl.common_folder({ { filename = "/v/a.mp4" }, { filename = "/w/b.mp4" } }), nil)
expect("windows folder", tl.common_folder({ { filename = "C:\\Trips\\a.mp4" } }), "C:\\Trips")

-- A name cut in its middle
expect("short stays", tl.cut_middle("DJI_0068_D", 20), "DJI_0068_D")
expect("cut", tl.cut_middle("DJI_20261021161350_0023_D", 15), "DJI_202…_0023_D")
expect("cut utf-8", tl.cut_middle("Тюлень у рифа, утро", 9), "Тюле…утро")

print(("%d checks, %d failed"):format(checks, failures))
os.exit(failures == 0 and 0 or 1)
