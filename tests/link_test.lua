-- The link reader (mpv/scripts/workspace/link.lua) over MP4s made here, then over what ffmpeg makes and does to
-- them (copied, renamed, tagged, remuxed), and, given GYROFLOW_EXPORT (the binary), over a preview it writes.
-- From the repository's folder: luajit tests/link_test.lua (or lua 5.1 and later). The ffmpeg part needs ffmpeg
-- in PATH (else it's skipped and says so).

package.path = "mpv/scripts/workspace/?.lua;" .. package.path
local link = require 'link'

local failures, checks = 0, 0
local function expect(what, got, want)
    checks = checks + 1
    if got ~= want then
        failures = failures + 1
        print(("FAIL %s: got %s, want %s"):format(what, tostring(got), tostring(want)))
    end
end

local function be32(n)
    return string.char(math.floor(n / 16777216) % 256, math.floor(n / 65536) % 256, math.floor(n / 256) % 256, n % 256)
end
local function box(typ, body) return be32(8 + #body) .. typ .. body end

-- An mdta meta box: Apple's form (no version and flags) or ffmpeg's (full = true)
local function meta(keys, full, handler)
    local list, items = "", ""
    for i, kv in ipairs(keys) do
        list = list .. be32(8 + #kv[1]) .. "mdta" .. kv[1]
        items = items .. box(be32(i), box("data", be32(1) .. be32(0) .. kv[2]))
    end
    local hdlr = box("hdlr", ("\0"):rep(8) .. (handler or "mdta") .. ("\0"):rep(13))
    return box("meta", (full and ("\0"):rep(4) or "") .. hdlr .. box("keys", be32(0) .. be32(#keys) .. list) .. box("ilst", items))
end

local dir = os.tmpname()
os.remove(dir)
assert(os.execute(("mkdir -p '%s'"):format(dir)))
local function write(name, bytes)
    local p = dir .. "/" .. name
    local f = assert(io.open(p, "wb"))
    f:write(bytes)
    f:close()
    return p
end

local LINK = '{"v":1,"kind":"moment","clip":"Day 1/DJI_0068_D.MP4","name":"Черепаха"}'
local ftyp = box("ftyp", "isom" .. be32(0) .. "isom")
local mvhd = box("mvhd", ("\0"):rep(100))
local mdat = box("mdat", ("x"):rep(64))

-- Apple's place, moov first
expect("moov/meta", link.read(write("a.mp4", ftyp .. box("moov", mvhd .. meta({ { link.KEY, LINK } })) .. mdat)), LINK)
-- moov after a large mdat: seeked over
local big = box("mdat", ("\0"):rep(3 * 1024 * 1024))
expect("moov at the end", link.read(write("b.mp4", ftyp .. big .. box("moov", mvhd .. meta({ { link.KEY, LINK } })))), LINK)
-- ffmpeg's place and form
expect("udta/meta", link.read(write("c.mp4", ftyp .. box("moov", mvhd .. box("udta", meta({ { link.KEY, LINK } }, true))) .. mdat)), LINK)
-- Several keys, the link not first; the other key read too
local keys = link.read_keys(write("d.mp4", ftyp .. box("moov", mvhd .. meta({ { "com.apple.quicktime.creationdate", "2026-10-22T15:28:42+0200" }, { link.KEY, LINK } })) .. mdat))
expect("second key", keys[link.KEY], LINK)
expect("first key", keys["com.apple.quicktime.creationdate"], "2026-10-22T15:28:42+0200")
-- Both places: moov/meta wins
expect("both", link.read(write("e.mp4", ftyp .. box("moov", mvhd .. box("udta", meta({ { link.KEY, "{}" } }, true)) .. meta({ { link.KEY, LINK } })) .. mdat)), LINK)
-- iTunes metadata (mdir) only: none
expect("itunes", link.read(write("f.mp4", ftyp .. box("moov", mvhd .. box("udta", meta({ { link.KEY, LINK } }, true, "mdir"))) .. mdat)), nil)
-- None, cut short, not an MP4, missing
expect("none", link.read(write("g.mp4", ftyp .. box("moov", mvhd) .. mdat)), nil)
local whole = ftyp .. box("moov", mvhd .. meta({ { link.KEY, LINK } })) .. mdat
expect("cut short", link.read(write("h.mp4", whole:sub(1, 150))), nil)
expect("text", link.read(write("i.txt", "hello, this is not an MP4 at all")), nil)
expect("missing", link.read(dir .. "/nothing.mp4"), nil)

-- What ffmpeg makes and does
local function run(cmd) return os.execute(cmd .. " >/dev/null 2>&1") end
local function ok(r) return r == true or r == 0 end
local function q(s) return "'" .. s:gsub("'", "'\\''") .. "'" end
if ok(run("ffmpeg -version")) then
    local made = dir .. "/made.mp4"
    assert(ok(run(("ffmpeg -v error -y -f lavfi -i color=gray:s=320x180:r=10 -t 2 -c:v mpeg4 -movflags +faststart+use_metadata_tags -metadata %s %s")
        :format(q(link.KEY .. "=" .. LINK), q(made)))), "ffmpeg didn't make a file")
    expect("ffmpeg's", link.read(made), LINK)
    run(("cp %s %s"):format(q(made), q(dir .. "/copy.mp4")))
    expect("copied", link.read(dir .. "/copy.mp4"), LINK)
    os.rename(dir .. "/copy.mp4", dir .. "/Черепаха 2.mp4")
    expect("renamed", link.read(dir .. "/Черепаха 2.mp4"), LINK)
    if ok(run("xattr -h")) then
        run(("xattr -w com.apple.metadata:_kMDItemUserTags '(\"Favorite\\n2\")' %s"):format(q(made)))
        expect("tagged", link.read(made), LINK)
    end
    run(("ffmpeg -v error -y -i %s -c copy %s"):format(q(made), q(dir .. "/remuxed.mp4")))
    expect("remuxed: lost", link.read(dir .. "/remuxed.mp4"), nil)
    run(("ffmpeg -v error -y -i %s -c copy -map_metadata 0 -movflags use_metadata_tags %s"):format(q(made), q(dir .. "/kept.mp4")))
    expect("remuxed with -map_metadata 0: kept", link.read(dir .. "/kept.mp4"), LINK)

    -- gyroflow-export's preview: its writer and this reader agree
    local exporter = os.getenv("GYROFLOW_EXPORT")
    if exporter and exporter ~= "" then
        local clip, preview = dir .. "/clip.mp4", dir .. "/Moments/Черепаха.mp4"
        assert(ok(run(("ffmpeg -v error -y -f lavfi -i testsrc2=s=640x360:r=25 -t 3 -c:v libx264 -pix_fmt yuv420p %s"):format(q(clip)))))
        local r = run(("%s --preview still --from 1 --to 2.5 --link %s --dates 2026-10-22T15:28:42+02:00 --output %s %s")
            :format(q(exporter), q(LINK), q(preview), q(clip)))
        expect("gyroflow-export ran", ok(r), true)
        local k = link.read_keys(preview) or {}
        -- With the original's fingerprint added (gyroflow-export --fingerprint prints the same)
        local h = io.popen(("%s --fingerprint %s"):format(q(exporter), q(clip))):read("*l")
        expect("gyroflow-export's fingerprint", type(h) == "string" and #h, 64)
        expect("gyroflow-export's link", k[link.KEY], LINK:sub(1, -2) .. ',"hash":"' .. tostring(h) .. '"}')
        expect("gyroflow-export's date", k["com.apple.quicktime.creationdate"], "2026-10-22T15:28:42+0200")
    else
        print("gyroflow-export's preview: skipped (no GYROFLOW_EXPORT)")
    end
else
    print("ffmpeg's files: skipped (no ffmpeg)")
end

run(("rm -rf %s"):format(q(dir)))
print(("%d checks, %d failed"):format(checks, failures))
os.exit(failures == 0 and 0 or 1)
