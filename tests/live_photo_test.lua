-- What a photo is (mpv/scripts/live-photo/live.lua): Exif times, Motion Photo XMP, a Live Photo's MOV (its still time,
-- its identifier), the pairing, and the chain rule on made-up capture times.
-- From the repository's folder: luajit tests/live_photo_test.lua
--
-- With real files too: AFP_LIVE_PHOTOS=<folder> (Live Photos, HEIC + .mov, e.g. the player's own export or an
-- iPhone's) and AFP_MOTION_PHOTOS=<folder> (Motion Photos, *.MP.jpg): each photo there must be found live, with a
-- capture time and a still time inside its video; their chain is printed.

package.path = "mpv/scripts/live-photo/?.lua;" .. package.path
local live = require 'live'

local failures, checks = 0, 0
local function expect(what, got, want)
    checks = checks + 1
    if got ~= want then
        failures = failures + 1
        print(("FAIL %s: got %s, want %s"):format(what, tostring(got), tostring(want)))
    end
end
local function near(what, got, want, eps)
    checks = checks + 1
    if type(got) ~= "number" or math.abs(got - want) > (eps or 1e-6) then
        failures = failures + 1
        print(("FAIL %s: got %s, want %s"):format(what, tostring(got), tostring(want)))
    end
end

local function be16(n) return string.char(math.floor(n / 256) % 256, n % 256) end
local function be32(n) return string.char(math.floor(n / 16777216) % 256, math.floor(n / 65536) % 256, math.floor(n / 256) % 256, n % 256) end
local function box(typ, body) return be32(8 + #body) .. typ .. body end
local function full(typ, v, body) return box(typ, string.char(v, 0, 0, 0) .. body) end

local tmpdir = os.getenv("TMPDIR") or "/tmp"
local function write(name, bytes)
    local p = tmpdir .. "/afp-live-test-" .. name
    local f = assert(io.open(p, "wb"))
    f:write(bytes)
    f:close()
    return p
end

-- ---------- Exif times ----------
near("exif seconds, zone", live.exif_seconds("2023:03:13 08:29:29", "382", "+01:00"), 1678692569.382, 1e-6)
local t, zoned = live.exif_seconds("2023:03:13 08:29:29", nil, nil)
near("exif seconds, no zone (camera clock)", t, 1678696169, 1e-6)
expect("no zone said", zoned, false)
near("exif seconds, negative zone", live.exif_seconds("2026:01:01 00:00:00", "5", "-05:30"), 1767225600 + 5.5 * 3600 + 0.5, 1e-6)
expect("not a time", live.exif_seconds("garbage"), nil)

-- A big-endian TIFF: IFD0 with the Exif IFD pointer, the Exif IFD with DateTimeOriginal, SubSecTimeOriginal,
-- OffsetTimeOriginal (as an iPhone writes them)
local function tiff()
    local dt = "2023:01:02 13:37:27\0"   -- 20 bytes, out of line
    local off = "+01:00\0"                -- 7 bytes, out of line
    local sub = "057\0"                   -- 4 bytes, in the entry
    local ifd0_at, exif_at = 8, 8 + 2 + 12 + 4
    local data_at = exif_at + 2 + 3 * 12 + 4
    local ifd0 = be16(1) .. be16(0x8769) .. be16(4) .. be32(1) .. be32(exif_at) .. be32(0)
    local exif = be16(3)
        .. be16(0x9003) .. be16(2) .. be32(#dt) .. be32(data_at)
        .. be16(0x9011) .. be16(2) .. be32(#off) .. be32(data_at + #dt)
        .. be16(0x9291) .. be16(2) .. be32(#sub) .. sub
        .. be32(0)
    assert(ifd0_at == 8)
    return "MM\0\42" .. be32(8) .. ifd0 .. exif .. dt .. off
end
local jpeg_head = "\255\216\255\225" .. be16(2 + 6 + #tiff()) .. "Exif\0\0" .. tiff()
local taken = live.exif_taken(jpeg_head)
expect("exif DateTimeOriginal", taken and taken.datetime, "2023:01:02 13:37:27")
expect("exif SubSecTimeOriginal", taken and taken.subsec, "057")
expect("exif OffsetTimeOriginal", taken and taken.offset, "+01:00")

-- ---------- Motion Photo XMP ----------
local xmp = '<x:xmpmeta xmlns:x="adobe:ns:meta/"><rdf:RDF><rdf:Description GCamera:MotionPhoto="1" GCamera:MotionPhotoVersion="1" '
    .. 'GCamera:MotionPhotoPresentationTimestampUs="1484817"><Container:Directory><rdf:Seq>'
    .. '<rdf:li><Container:Item Item:Mime="image/jpeg" Item:Semantic="Primary"/></rdf:li>'
    .. '<rdf:li><Container:Item Item:Mime="image/jpeg" Item:Semantic="GainMap" Item:Length="237178"/></rdf:li>'
    .. '<rdf:li><Container:Item Item:Mime="video/mp4" Item:Semantic="MotionPhoto" Item:Length="9540780"/></rdf:li>'
    .. '</rdf:Seq></Container:Directory></rdf:Description></rdf:RDF></x:xmpmeta>'
local mp = live.motion_photo(xmp, 11549678)
expect("motion photo offset", mp and mp.offset, 11549678 - 9540780)
near("motion photo still time", mp and mp.still_t, 1.484817)
-- The older MicroVideo, and a timestamp of -1 (unknown)
local micro = live.motion_photo('<x:xmpmeta><rdf:Description GCamera:MicroVideo="1" GCamera:MicroVideoVersion="1" GCamera:MicroVideoOffset="3000" '
    .. 'GCamera:MicroVideoPresentationTimestampUs="-1"/></x:xmpmeta>', 10000)
expect("micro video offset", micro and micro.offset, 7000)
expect("micro video still unknown", micro and micro.still_t, nil)
expect("a plain JPEG's XMP", live.motion_photo('<x:xmpmeta><rdf:Description xmp:Rating="3"/></x:xmpmeta>', 1000), nil)

-- ---------- a Live Photo's MOV ----------
-- moov: mvhd (timescale 600, 3 s), a video track, and a timed-metadata track whose mebx keys say still-image-time,
-- put at 1.5 s by an empty edit; meta keys with the content identifier (QuickTime's meta, no version/flags)
local ID = "06845B52-D886-450C-8ED0-FB889AFB1346"
local function mov(id, still_at)
    local mvhd = full("mvhd", 0, be32(0) .. be32(0) .. be32(600) .. be32(1800) .. string.rep("\0", 80))
    local function trak(handler, extra_stbl, edits)
        local hdlr = full("hdlr", 0, be32(0) .. handler .. string.rep("\0", 12) .. "\0")
        local mdhd = full("mdhd", 0, be32(0) .. be32(0) .. be32(600) .. be32(1800) .. be32(0))
        local stbl = box("stbl", extra_stbl or "")
        local mdia = box("mdia", mdhd .. hdlr .. box("minf", stbl))
        return box("trak", (edits or "") .. mdia)
    end
    local video = trak("vide")
    local key = "com.apple.quicktime.still-image-time"
    local keys = box("keys", box(be32(1), box("keyd", "mdta" .. key)))
    local mebx = box("mebx", string.rep("\0", 6) .. be16(1) .. keys)
    local stsd = full("stsd", 0, be32(1) .. mebx)
    local edts = box("edts", full("elst", 0, be32(2) .. be32(math.floor(still_at * 600)) .. be32(0xFFFFFFFF) .. be32(0x00010000)
                                             .. be32(1) .. be32(0) .. be32(0x00010000)))
    local still = trak("meta", stsd, edts)
    local meta = ""
    if id then
        local k = "com.apple.quicktime.content.identifier"
        local qkeys = full("keys", 0, be32(1) .. be32(8 + #k) .. "mdta" .. k)
        local ilst = box("ilst", box(be32(1), box("data", be32(1) .. be32(0) .. id)))
        meta = box("meta", full("hdlr", 0, be32(0) .. "mdta" .. string.rep("\0", 13)) .. qkeys .. ilst)
    end
    return box("ftyp", "qt  " .. be32(0) .. "qt  ") .. box("mdat", string.rep("\0", 64)) .. box("moov", mvhd .. video .. still .. meta)
end
local mov_path = write("IMG_0001.mov", mov(ID, 1.5))
local m = live.movie(mov_path)
near("mov duration", m and m.duration, 3)
near("mov still time", m and m.still_t, 1.5)
expect("mov identifier", m and m.id, ID)
expect("mov has video", m and m.has_video, true)

-- Its photo: a JPEG with that Exif, the identifier in the maker note (here: anywhere in the head)
local photo = write("IMG_0001.jpg", jpeg_head .. "Apple iOS\0" .. ID .. "\0" .. string.rep("\0", 100))
local d = live.describe(photo)
expect("live photo found", d and d.live, true)
expect("live photo kind", d and d.kind, "apple")
expect("live photo video", d and d.video, mov_path)
near("live photo starts before the still", d and d.start, live.exif_seconds("2023:01:02 13:37:27", "057", "+01:00") - 1.5)
-- Another Live Photo's .mov under its name: not its pair
write("IMG_0002.mov", mov("11111111-2222-3333-4444-555555555555", 1.5))
local other = live.describe(write("IMG_0002.jpg", jpeg_head .. ID))
expect("a .mov of another Live Photo", other and other.live, false)
-- A photo saying no identifier, a .mov beside it: paired by name
write("IMG_0003.mov", mov(ID, 1.2))
local named = live.describe(write("IMG_0003.jpg", jpeg_head))
expect("paired by name", named and named.live, true)
near("its still time", named and named.still_t, 1.2)
-- A plain photo
local plain = live.describe(write("IMG_0004.jpg", jpeg_head))
expect("plain photo", plain and plain.live, false)
expect("not a photo", live.describe(mov_path), nil)
expect("a photo's link is the workspace's", live.is_photo("/p/_Stills/A_12.35s.afplink.jpg"), false)
expect("a Motion Photo's name is a photo's", live.is_photo("/p/PXL_1.MP.jpg"), true)
expect("a URL isn't", live.is_photo("https://x/y.jpg"), false)
expect("the pair's .MOV", live.paired_mov("/p/IMG_1.HEIC", function(p) return p == "/p/IMG_1.MOV" end), "/p/IMG_1.MOV")

-- A Motion Photo made of these parts: the JPEG, then the MP4 (its offset from the XMP's length)
local mp4 = mov(nil, 0)
local mxmp = '<x:xmpmeta><rdf:Description GCamera:MotionPhoto="1" GCamera:MotionPhotoPresentationTimestampUs="1000000">'
    .. '<Container:Item Item:Semantic="Primary"/><Container:Item Item:Semantic="MotionPhoto" Item:Length="' .. #mp4 .. '"/></rdf:Description></x:xmpmeta>'
local mjpg = write("PXL_1.MP.jpg", jpeg_head .. mxmp .. "\255\217" .. mp4)
local md = live.describe(mjpg)
expect("motion photo found", md and md.live, true)
expect("motion photo kind", md and md.kind, "motion")
near("motion photo still", md and md.still_t, 1.0)
near("motion photo duration", md and md.duration, 3)
expect("motion photo url", md and md.url, ("slice://%d@%s"):format(#jpeg_head + #mxmp + 2, mjpg))

-- ---------- chains ----------
-- Photos taken 0.9 s apart, each 3 s of video with the still at 1.5 s: each carries on into the next
local function shot(name, taken_at, dur, still, zoned)
    return { path = name, live = true, taken = taken_at, duration = dur or 3, still_t = still or 1.5, start = taken_at - (still or 1.5), zoned = zoned ~= false }
end
local a, b, c, far = shot("a", 100), shot("b", 100.9), shot("c", 101.8), shot("far", 110)
local run = live.chain(a, { far, c, b, a })
expect("chain length", #run, 3)
expect("chain 2nd", run[2] and run[2].path, "b")
expect("chain 3rd", run[3] and run[3].path, "c")
-- Where b goes on: a's video ends at 98.5 + 3 = 101.5, b's started at 99.4: 2.1 s into b
near("carried on at", live.carry_on_at(a, b), 2.1)
-- A gap: the next one's live part starts after this one's ends
expect("gap ends the run", live.next_in_chain(c, { far }), nil)
-- Taken earlier is never next (a burst played from its middle goes forward)
expect("not backwards", live.next_in_chain(b, { a }), nil)
-- The next taken, not the one starting soonest: an iPhone's still time differs shot to shot
local p1 = shot("p1", 29.382, 2.87, 1.333)
local p2 = shot("p2", 29.706, 3.23, 1.700)   -- starts 0.04 s before p1 (taken later, its still later in its video)
local p3 = shot("p3", 30.853, 2.93, 1.400)
expect("next taken", live.next_in_chain(p1, { p3, p2 }).path, "p2")
-- One whose live part ends with this one's (or before) adds nothing: passed over
local inside = shot("inside", 100.3, 2.2, 1.0)  -- 99.3 .. 101.5, a ends at 101.5
expect("nothing more: passed over", live.next_in_chain(a, { inside, b }).path, "b")
-- Clocks: a photo without a zone and one with can't be lined up
expect("different clocks", live.next_in_chain(a, { shot("z", 100.9, 3, 1.5, false) }), nil)
-- Plain photos and photos without a time don't chain
expect("plain photo", live.next_in_chain(a, { { path = "p", live = false } }), nil)
expect("no time", live.next_in_chain({ live = true, duration = 3 }, { b }), nil)

-- ---------- real files ----------
local function each(dir, pattern, fn)
    local p = io.popen('ls "' .. dir .. '"')
    if not p then return end
    for name in p:lines() do if name:match(pattern) then fn(dir .. "/" .. name) end end
    p:close()
end
local function real(dir, pattern, kind)
    if not dir or dir == "" then return end
    local found = {}
    each(dir, pattern, function(path)
        local r = live.describe(path)
        expect(path .. " live", r and r.live, true)
        expect(path .. " kind", r and r.kind, kind)
        checks = checks + 1
        if not (r and r.taken and r.still_t and r.still_t >= 0 and r.still_t <= r.duration) then
            failures = failures + 1
            print("FAIL " .. path .. ": no capture time or still time outside its video")
        end
        if r then r.path = path found[#found + 1] = r end
    end)
    expect(dir .. " has photos", #found > 0, true)
    if found[1] then
        table.sort(found, function(x, y) return (x.taken or 0) < (y.taken or 0) end)
        local names = {}
        for _, r in ipairs(live.chain(found[1], found)) do names[#names + 1] = r.path:match("[^/]+$") end
        print(dir .. ": the run from the first photo: " .. table.concat(names, " -> "))
    end
end
real(os.getenv("AFP_LIVE_PHOTOS"), "%.[Hh][Ee][Ii][Cc]$", "apple")
real(os.getenv("AFP_MOTION_PHOTOS"), "%.MP%.jpg$", "motion")

print(("%d checks, %d failed"):format(checks, failures))
os.exit(failures == 0 and 0 or 1)
