-- Windows' long paths (mpv/scripts/workspace/longpath.lua): which get the \\?\ prefix, how, and the file calls
-- installed taking it. Paths only, no Windows needed. From the repository's folder: luajit tests/longpath_test.lua

package.path = "mpv/scripts/workspace/?.lua;" .. package.path
local longpath = require 'longpath'

local failures, checks = 0, 0
local function expect(what, got, want)
    checks = checks + 1
    if got ~= want then
        failures = failures + 1
        print(("FAIL %s: got %s, want %s"):format(what, tostring(got), tostring(want)))
    end
end

local long = string.rep("x", 250)
local short = "C:\\Trips\\Egypt\\DJI_0001.MP4"

expect("short", longpath.extended(short), short)
expect("not a string", longpath.extended(nil), nil)
expect("drive", longpath.extended("C:\\Trips\\" .. long .. "\\a.MP4"), "\\\\?\\C:\\Trips\\" .. long .. "\\a.MP4")
expect("slashes", longpath.extended("C:/Trips/" .. long .. "/a.MP4"), "\\\\?\\C:\\Trips\\" .. long .. "\\a.MP4")
expect("doubled", longpath.extended("C:\\Trips\\\\" .. long .. "\\a.MP4"), "\\\\?\\C:\\Trips\\" .. long .. "\\a.MP4")
expect("share", longpath.extended("\\\\nas\\video\\" .. long .. "\\a.MP4"), "\\\\?\\UNC\\nas\\video\\" .. long .. "\\a.MP4")
expect("share, slashes", longpath.extended("//nas/video/" .. long .. "/a.MP4"), "\\\\?\\UNC\\nas\\video\\" .. long .. "\\a.MP4")
local done = "\\\\?\\C:\\Trips\\" .. long
expect("prefixed already", longpath.extended(done), done)
expect("relative", longpath.extended("Trips\\" .. long), "Trips\\" .. long)
expect("dot dot", longpath.extended("C:\\Trips\\..\\" .. long), "C:\\Trips\\..\\" .. long)
local pipe = "\\\\.\\pipe\\" .. long
expect("device", longpath.extended(pipe), pipe)
expect("at the limit", longpath.extended("C:\\" .. string.rep("y", longpath.LIMIT - 3)), "\\\\?\\C:\\" .. string.rep("y", longpath.LIMIT - 3))
expect("below it", longpath.extended("C:\\" .. string.rep("y", longpath.LIMIT - 4)), "C:\\" .. string.rep("y", longpath.LIMIT - 4))

-- install: off Windows nothing; on it, every call goes through extended()
local open, seen = io.open, {}
longpath.install({}, false)
expect("off Windows", io.open, open)
-- A folder of a clip, a subfolder and a name too long for mpv's own check
local utils = {
    file_info = function(p)
        seen.info = p
        local name = p:match("([^\\]+)$")
        if name:match("%.MP4$") then return { is_file = true } elseif name == "Day 1" then return { is_dir = true } end
    end,
    readdir = function(p, filter) seen.dir, seen.filter = p, filter; return { ".", "..", "a.MP4", "Day 1", "gone" } end,
}
io.open = function(p) seen.open = p end
os.remove = function(p) seen.remove = p end
os.rename = function(a, b) seen.from, seen.to = a, b end
longpath.install(utils, true)
local file = "C:\\Trips\\" .. long .. "\\a.afp.json"
io.open(file, "w"); os.remove(file); os.rename(file .. ".tmp", file); utils.readdir(file); utils.file_info(file)
local want = "\\\\?\\" .. file
expect("io.open", seen.open, want)
expect("os.remove", seen.remove, want)
expect("os.rename from", seen.from, want .. ".tmp")
expect("os.rename to", seen.to, want)
expect("file_info", seen.info, want)
expect("readdir", seen.dir, want)
expect("readdir unfiltered", seen.filter, "all")
-- A short folder too (its entries' whole paths may be long), each entry's kind asked here
local function list(t) return table.concat(t or {}, ",") end
expect("readdir files", list(utils.readdir("C:\\Trips\\Egypt", "files")), "a.MP4")
expect("readdir, short", seen.dir, "\\\\?\\C:\\Trips\\Egypt")
expect("kind asked by the plain path", seen.info, "C:\\Trips\\Egypt\\gone")
expect("readdir dirs", list(utils.readdir("C:\\Trips\\Egypt\\", "dirs")), "Day 1")
expect("readdir normal", list(utils.readdir("C:/Trips/Egypt")), "a.MP4,Day 1")
expect("readdir all", list(utils.readdir("C:\\Trips\\Egypt", "all")), ".,..,a.MP4,Day 1,gone")
utils.readdir("Trips", "files")
expect("readdir, relative", seen.dir, "Trips")
expect("readdir, relative: mpv filters", seen.filter, "files")
io.open(short)
expect("short untouched", seen.open, short)

print(("%d checks, %d failed"):format(checks, failures))
os.exit(failures == 0 and 0 or 1)
