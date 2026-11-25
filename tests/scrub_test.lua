-- The seek bar's picture (mpv/scripts/side-panel/scrub.lua): the sidecar's names (tests/scrub_sidecar_cases.txt, the
-- Library's rule too), what it's made from, its size and place, and, with a stand-in mpv, what's asked of afp-scrub
-- and what goes on screen.
-- From the repository's folder: luajit tests/scrub_test.lua

package.path = "mpv/scripts/side-panel/?.lua;" .. package.path
local scrub = require 'scrub'

local failures, checks = 0, 0
local function expect(what, got, want)
    checks = checks + 1
    if got ~= want then
        failures = failures + 1
        print(("FAIL %s: got %s, want %s"):format(what, tostring(got), tostring(want)))
    end
end

-- The names, as the Library has them
local cases = 0
for line in io.lines("tests/scrub_sidecar_cases.txt") do
    local clip, want = line:match("^([^#].-) %-> (.+)$")
    if clip then
        cases = cases + 1
        local got = table.concat(scrub.sidecar_names("/Volumes/photo/DATA/100GOPRO/" .. clip), "|")
        expect("names of " .. clip, got, want == "-" and "" or want)
    end
end
expect("cases read", cases >= 5, true)

-- Found: the first that's there and not cut short
local files = { ["/d/GOPR1030.LRV"] = 50000, ["/d/GOPR1030.THM"] = 9000, ["/d/DJI_1_D.LRF"] = 10, ["/d/GL010057.lrv"] = 4096 }
local function size_of(p) return files[p] end
expect("HERO4 LRV", scrub.find_sidecar("/d/GOPR1030.MP4", size_of), "/d/GOPR1030.LRV")
expect("HERO5+ GL, lower case", scrub.find_sidecar("/d/GX010057.MP4", size_of), "/d/GL010057.lrv")
expect("cut short: none", scrub.find_sidecar("/d/DJI_1_D.MP4", size_of), nil)
expect("none", scrub.find_sidecar("/d/IMG_1.MOV", size_of), nil)
expect("Windows path", scrub.find_sidecar([[C:\d\GOPR1030.MP4]], function(p) return p == [[C:\d\GOPR1030.LRV]] and 5000 or nil end), [[C:\d\GOPR1030.LRV]])

-- What it's made from
local src = scrub.source("/v/A.MP4", nil, false)
expect("a clip: itself", src.clip, "/v/A.MP4")
expect("a clip: no offset", src.offset, 0)
local ws = { path = "/p/_Moments/Turtle.afplink.mp4", clip = "/v/DJI_1_D.MP4", offset = 12.5 }
src = scrub.source("/p/_Moments/Turtle.afplink.mp4", ws, false)
expect("a moment: its original", src.clip, "/v/DJI_1_D.MP4")
expect("a moment: at the original's time", src.offset, 12.5)
src = scrub.source("/v/B.MP4", ws, false)
expect("another file's workspace isn't this one's", src.clip, "/v/B.MP4")
local none, why = scrub.source("/v/VID_1.insv", nil, false)
expect("Insta360: none", none, nil)
expect("Insta360: why", why, "360")
none, why = scrub.source("/v/A.MP4", nil, true)
expect("a sphere: none", why, "360")
none, why = scrub.source("https://example.com/a.mp4", nil, false)
expect("a URL: none", why, "not a file")
expect("file:// is a file", scrub.source("file:///v/A.MP4", nil, false).clip, "/v/A.MP4")
expect("nothing playing", (scrub.source(nil)), nil)

-- Size: 130 points high, as wide as the video
local w, h = scrub.size(16 / 9, 2)
expect("16:9 at 2x: height", h, 260)
expect("16:9 at 2x: width", w, 462)
w, h = scrub.size(4 / 3, 1)
expect("4:3 at 1x", w .. "x" .. h, "174x130")
w, h = scrub.size(nil, 1)
expect("unknown: 16:9", w .. "x" .. h, "232x130")
w, h = scrub.size(9 / 16, 2)
expect("portrait", w .. "x" .. h, "146x260")

-- Place: centred on the mouse, kept in the window, above the bubble
local x, y = scrub.place(500, 462, 260, 0, 1600, 900, 2)
expect("centred", x, 269)
expect("above the bubble", y, 900 - 60 - 12 - 260)
x = scrub.place(10, 462, 260, 0, 1600, 900, 2)
expect("kept in at the left", x, 8)
x = scrub.place(1590, 462, 260, 0, 1600, 900, 2)
expect("kept in at the right", x, 1600 - 462 - 8)

-- Requests and replies
expect("request", scrub.request(7, "/v/A.LRF", 12.34567, "exact", "/t/7.bgra", 462, 260, "/v/A.MP4"),
       "7\t/v/A.LRF\t12.3457\texact\t/t/7.bgra\t462\t260\t/v/A.MP4")
expect("request, key", scrub.request(8, "/v/B.MP4", -1, "key", "/t/8.bgra", 462, 260), "8\t/v/B.MP4\t0.0000\tkey\t/t/8.bgra\t462\t260")
local r = scrub.parse_reply("7\tok\t27.5\twalk\t12.345")
expect("reply ok", r.ok and r.id == 7 and r.ms == 27.5 and r.kind == "walk" and r.t == 12.345, true)
r = scrub.parse_reply("8\tdropped")
expect("reply dropped", not r.ok and r.status == "dropped", true)
r = scrub.parse_reply("9\terror\tcan't open: No such file")
expect("reply error", r.status == "error" and r.message == "can't open: No such file", true)
expect("reply garbage", scrub.parse_reply("x\ty"), nil)

local s = scrub.summary({ 30, 10, 20, 50, 40 })
expect("summary", s.n == 5 and s.median == 30 and s.p90 == 50 and s.max == 50, true)

-- In a stand-in mpv: what's asked, what's shown
local props = { path = "/v/DJI_1_D.MP4", pid = 42, ["input-ipc-server"] = "/tmp/mpv.sock",
                ["video-params"] = { aspect = 16 / 9 }, ["user-data/view360"] = { available = false } }
local commands, async, set = {}, {}, {}
local clock = 100
local mp = {
    get_property = function(k, def) local v = props[k]; if v == nil then return def end; return tostring(v) end,
    get_property_native = function(k) return props[k] end,
    get_property_number = function(k) return tonumber(props[k]) end,
    set_property = function(k, v) set[#set + 1] = { k, v }; props[k] = v end,
    set_property_native = function(k, v) if k == "user-data/afp-scrub/request" then set[#set + 1] = { k, v } end; props[k] = v end,
    get_script_directory = function() return "/app/plugins/side-panel" end,
    get_script_name = function() return "side_panel" end,
    get_time = function() return clock end,
    command_native = function(c) commands[#commands + 1] = c; return {} end,
    command_native_async = function(c, cb) async[#async + 1] = { c, cb } end,
    msg = { verbose = function() end, warn = function() end },
}
local present = { ["/app/plugins/side-panel/afp-scrub"] = true, ["/v/DJI_1_D.LRF"] = 900000 }
local utils = {
    join_path = function(a, b) return a .. "/" .. b end,
    file_info = function(p) local v = present[p]; if v then return { is_file = true, size = v == true and 1 or v } end end,
    readdir = function() return {} end,
}
local journal = {}
local J = {
    act = function(n, f) journal[#journal + 1] = { "act", n, f }; return "A1" end,
    shown_for = function(a, n, f) journal[#journal + 1] = { "shown", n, f, a } end,
    step = function(n, f) journal[#journal + 1] = { "step", n, f } end,
}
os.getenv = (function(get) return function(k) if k == "TMPDIR" then return "/t" end return get(k) end end)(os.getenv)
local removed = {}
os.remove = function(p) removed[#removed + 1] = p; return true end

local P = scrub.new(mp, utils, J, "darwin")
expect("available", P.available, true)
local rect = P.want(10, 500, 0, 1600, 900, 2)
expect("nothing shown yet", rect, nil)
expect("helper started", async[1] and async[1][1].args[1], "/app/plugins/side-panel/afp-scrub")
expect("helper on the player's socket", async[1] and async[1][1].args[3], "/tmp/mpv.sock")
local req = props["user-data/afp-scrub/request"]
expect("asked: the sidecar's exact frame", req, "1\t/v/DJI_1_D.LRF\t10.0000\texact\t/t/afp-scrub-42/1.bgra\t462\t260\t/v/DJI_1_D.MP4")
expect("journal: the hover", journal[1] and journal[1][2], "bar/preview")
local n_set = #set
P.want(10.001, 501, 0, 1600, 900, 2)
expect("the same time: not asked again", #set, n_set)
P.want(11, 520, 0, 1600, 900, 2)
expect("another time: asked", props["user-data/afp-scrub/request"]:match("^2\t"), "2\t")
-- The first one's answer comes after the second was asked: shown (nearer than nothing) until the second comes
clock = 100.03
P.reply("1\tok\t20\texact\t10.0")
expect("an older one, nothing shown: shown", P.shown() and P.shown().id, 1)
P.reply("2\tok\t25\twalk\t11.0")
expect("the one before removed", removed[#removed], "/t/afp-scrub-42/1.bgra")
P.reply("1\tok\t20\texact\t10.0")
expect("older than the one shown: not shown", P.shown().id, 2)
local sh = P.shown()
expect("the newest shown", sh and sh.id, 2)
local add = commands[#commands]
expect("overlay-add", add[1], "overlay-add")
expect("overlay-add: its id", add[2], scrub.OVERLAY)
expect("overlay-add: at the mouse", add[3] .. "," .. add[4], (520 - 231) .. "," .. (900 - 60 - 12 - 260))
expect("overlay-add: the file", add[5], "/t/afp-scrub-42/2.bgra")
expect("overlay-add: bgra, size, stride", table.concat({ add[7], add[8], add[9], add[10] }, ","), "bgra,462,260,1848")
expect("journal: on screen", journal[#journal][2], "preview")
expect("published", props["user-data/afp-scrub/shown"] and props["user-data/afp-scrub/shown"].kind, "walk")
expect("journal: how long", math.floor(journal[#journal][3].ms + 0.5), 30)
-- The mouse moves on: the picture follows it at once, before the next one comes
rect = P.want(12, 700, 0, 1600, 900, 2)
expect("the frame's place", rect and rect.x, 700 - 231)
expect("moved along", commands[#commands][3], 700 - 231)
-- Dropped and errors change nothing
P.reply("3\tdropped")
P.reply("3\terror\tno frame")
expect("still the one shown", P.shown().id, 2)
-- Left: removed, a late answer not shown, the hover's times journaled
P.hide()
expect("overlay removed", commands[#commands][1], "overlay-remove")
expect("published: none", props["user-data/afp-scrub/shown"], nil)
P.reply("3\tok\t20\tkey\t12.0")
expect("late: not shown", P.shown(), nil)
expect("journal: the pictures", journal[#journal][2], "bar/preview-pictures")
-- A moment of that clip: the original's sidecar, at the original's time
props.path = "/p/M.afplink.mp4"
props["user-data/workspace"] = { path = "/p/M.afplink.mp4", clip = "/v/DJI_1_D.MP4", offset = 30 }
P.forget_clip()
P.want(1, 500, 0, 1600, 900, 2)
expect("moment: the original's sidecar at its time", props["user-data/afp-scrub/request"]:match("\t(/v/DJI_1_D.LRF\t31.0000\texact)"), "/v/DJI_1_D.LRF\t31.0000\texact")
-- No sidecar: the clip's keyframes
props.path = "/v/GX010001.MP4"
props["user-data/workspace"] = nil
P.forget_clip()
P.want(5, 500, 0, 1600, 900, 2)
expect("no sidecar: keyframes", props["user-data/afp-scrub/request"]:match("\t(/v/GX010001.MP4\t5.0000\tkey)\t"), "/v/GX010001.MP4\t5.0000\tkey")
-- A sphere: nothing asked
props.path = "/v/C.MP4"
props["user-data/view360"] = { available = true }
P.forget_clip()
local before = props["user-data/afp-scrub/request"]
expect("360: no picture", P.want(5, 500, 0, 1600, 900, 2), nil)
expect("360: nothing asked", props["user-data/afp-scrub/request"], before)
-- The helper ended: started again at the next hover, three times at most
props["user-data/view360"] = nil
props.path = "/v/D.MP4"
P.forget_clip()
for i = 1, 4 do
    async[#async][2](true, { status = 1 })
    P.want(5 + i, 500, 0, 1600, 900, 2)
end
expect("started again three times, then not", #async, 4)

-- Without the helper (a build without it): not offered
present["/app/plugins/side-panel/afp-scrub"] = nil
expect("no helper: not available", scrub.new(mp, utils, J, "darwin").available, false)

print(("%d checks, %d failed"):format(checks, failures))
os.exit(failures == 0 and 0 or 1)
