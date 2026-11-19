-- A list opened from Action Footage Library (mpv/scripts/workspace/librarylist.lua): which files are one, and their
-- head read (start, label, count). The playlist mpv makes of it: tests/librarylist_ipc.py, with a headless mpv.
-- From the repository's folder: luajit tests/librarylist_test.lua

package.path = "mpv/scripts/workspace/?.lua;" .. package.path
local ll = require 'librarylist'

local failures, checks = 0, 0
local function expect(what, got, want)
    checks = checks + 1
    if got ~= want then
        failures = failures + 1
        print(("FAIL %s: got %s, want %s"):format(what, tostring(got), tostring(want)))
    end
end

expect("is list", ll.is_list("/Users/a/Library/Caches/x/lists/1763.afplist.m3u8"), true)
expect("is list, case", ll.is_list("C:\\x\\LISTS\\1.AFPLIST.M3U8"), true)
expect("plain m3u", ll.is_list("/a/b.m3u8"), false)
expect("nil", ll.is_list(nil), false)

local text = table.concat({
    "#EXTM3U",
    "#PLAYLIST:Originals › Egypt",
    "#AFP-START:2",
    "#EXTINF:-1,A",
    "/v/A.MP4",
    "#EXTINF:-1,Turtle at the wreck",
    "/v/_Moments/Turtle at the wreck.afplink.mp4",
    "#EXTINF:-1,C",
    "/v/C.MP4",
    "",
}, "\n")
local l = ll.parse(text)
expect("start", l and l.start, 2)
expect("label", l and l.label, "Originals › Egypt")
expect("count", l and l.count, 3)

-- Windows line ends, no label
l = ll.parse("#EXTM3U\r\n#AFP-START:0\r\n#EXTINF:-1,A\r\nC:\\v\\A.MP4\r\n")
expect("crlf start", l and l.start, 0)
expect("crlf count", l and l.count, 1)
expect("no label", l and l.label, nil)

-- A start past its end: its last; none: the first
expect("past end", ll.parse("#EXTM3U\n#AFP-START:9\n/a\n/b\n").start, 1)
expect("no start", ll.parse("#EXTM3U\n/a\n/b\n").start, 0)
-- Not one: no head, or nothing in it
expect("no head", ll.parse("/a\n/b\n"), nil)
expect("empty", ll.parse("#EXTM3U\n#AFP-START:0\n"), nil)
expect("not text", ll.parse(nil), nil)

print(("%d checks, %d failed"):format(checks, failures))
os.exit(failures == 0 and 0 or 1)
