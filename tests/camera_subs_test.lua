-- The dji-telemetry plugin's subtitles a camera wrote (mpv/scripts/dji-telemetry/telemetry.lua): which text reads as
-- a camera's data (DJI's drones: embedded, and the .SRT beside the clip), and the first samples of an MP4's text
-- tracks, over a file ffmpeg makes (skipped without ffmpeg).
-- From the repository's folder: luajit tests/camera_subs_test.lua

package.path = "mpv/scripts/dji-telemetry/?.lua;" .. package.path
local telemetry = require 'telemetry'

local failures, checks = 0, 0
local function expect(what, got, want)
    checks = checks + 1
    if got ~= want then
        failures = failures + 1
        print(("FAIL %s: got %s, want %s"):format(what, tostring(got), tostring(want)))
    end
end

local EMBEDDED = "F/2.8, SS 1000.15, ISO 100, EV 0, DZOOM 1.000, GPS (6.1428, 46.1983, 19), D 12.01m, H 5.60m, H.S 1.20m/s, V.S 0.00m/s"
local SIDECAR = '1\n00:00:00,000 --> 00:00:00,033\n<font size="28">FrameCnt: 1, DiffTime: 33ms\n2023-01-15 13:46:22.001\n'
    .. "[iso : 100] [shutter : 1/1000.0] [fnum : 280] [ev : 0] [ct : 5500] [color_md : default] [focal_len : 240] "
    .. "[latitude: 46.198300] [longitude: 6.142800] [rel_alt: 5.600 abs_alt: 420.100] </font>\n"
expect("a drone's embedded line", telemetry.camera_text(EMBEDDED), true)
expect("a drone's .SRT", telemetry.camera_text(SIDECAR), true)
-- A DJI Mini 4 Pro's .SRT line, in its format (the values made up)
expect("a Mini 4 Pro's .SRT", telemetry.camera_text('1\n00:00:00,000 --> 00:00:00,033\n<font size="28">FrameCnt: 1, DiffTime: 33ms\n'
    .. "2025-01-01 12:00:00.000\n[iso: 110] [shutter: 1/5000.0] [fnum: 1.7] [ev: 0] [color_md: hlg] [focal_len: 24.00] "
    .. "[latitude: 10.000000] [longitude: 20.000000] [rel_alt: 40.000 abs_alt: 400.000] [ct: 5300] </font>\n"), true)
expect("an older drone's line", telemetry.camera_text("HOME(6.1428,46.1983) 2019.11.03 09:49:32\nGPS(6.1430,46.1990,19) BAROMETER:12.3\nISO:100 Shutter:500 EV:0 Fnum:F2.8"), true)
expect("words", telemetry.camera_text("1\n00:00:00,000 --> 00:00:02,500\nHello, this is a real subtitle.\n"), false)
expect("words that name two", telemetry.camera_text("I said the ISO is fine, give me the GPS"), false)
expect("nothing", telemetry.camera_text(nil), false)
expect("empty", telemetry.camera_text(""), false)

-- The text as telemetry: an .SRT of a Mini 4 Pro's lines (made-up values), the drone flying north at 10 m/s
local srt = {}
for i = 0, 59 do
    local t = i / 30 * 2
    srt[#srt + 1] = ("%d\n00:00:%02d,%03d --> 00:00:%02d,%03d\n<font size=\"28\">FrameCnt: %d, DiffTime: 66ms\n2025-01-01 12:00:00.000\n"
        .. "[iso: %d] [shutter: 1/2000.0] [fnum: 1.7] [ev: %s] [color_md: hlg] [focal_len: 24.00] [latitude: %.6f] "
        .. "[longitude: 20.000000] [rel_alt: %.3f abs_alt: 400.000] [ct: 5300] </font>\n")
        :format(i + 1, math.floor(t), math.floor(t * 1000 + 0.5) % 1000, math.floor(t), math.floor(t * 1000 + 0.5) % 1000,
                i + 1, i < 30 and 100 or 200, i < 30 and "0" or "-0.7", 10 + t * 10 / 111195, 40 + t)
end
local entries = telemetry.parse_srt(table.concat(srt, "\n"))
expect("the .SRT's entries", #entries, 60)
expect("…their times", entries[31] and math.floor(entries[31].t * 1000 + 0.5), 2000)
local tel = telemetry.from_text(entries, "DJI Mini4 Pro")
expect("telemetry from it", tel ~= nil, true)
tel = tel or { values = {} }
local function at(name, t) return telemetry.at(tel.values[name], t) end
expect("the model", tel.model, "DJI Mini4 Pro")
expect("a drone's fields", tel.defaults and table.concat(tel.defaults, ","), "height,gps,speed")
expect("ISO", at("iso", 0.5), 100)
expect("ISO later", at("iso", 3), 200)
expect("shutter", at("exposure_time", 1)[2], 2000)
expect("aperture", at("f_number", 1)[1], 170)
expect("EV", at("ev", 3), -0.7)
expect("white balance", at("white_balance_k", 1), 5300)
expect("latitude", at("latitude_deg", 0), 10)
expect("longitude", at("longitude_deg", 3), 20)
expect("height", at("height_m", 2), 42)
local v = at("speed_ms", 3)
expect("speed from the GPS, ~10 m/s", v and math.abs(v - 10) < 0.3, true)
expect("no speed in the first second", at("speed_ms", 0.5), nil)

-- The other way DJI writes it (Mavic Air 2, Mavic 2): the line's own height and speed
local air = telemetry.from_text({ { t = 0, text = EMBEDDED } })
expect("a comma-separated line", air ~= nil, true)
air = air or { values = {} }
expect("…its shutter", telemetry.at(air.values.exposure_time, 0)[2], 1000.15)
expect("…its aperture", telemetry.at(air.values.f_number, 0)[1], 280)
expect("…its position, lon and lat", telemetry.at(air.values.latitude_deg, 0), 46.1983)
expect("…its height", telemetry.at(air.values.height_m, 0), 5.6)
expect("…its speed", telemetry.at(air.values.speed_ms, 0), 1.2)
expect("…a model by default", air.model, "DJI drone")
expect("a film's subtitles aren't telemetry", telemetry.from_text(telemetry.parse_srt("1\n00:00:00,000 --> 00:00:02,500\nHello there.\n")), nil)

-- An MP4 with a video track and two text tracks: the camera's, then a film's
local dir = os.tmpname()
os.remove(dir)
os.execute("mkdir -p '" .. dir .. "'")
local function write(name, text)
    local f = assert(io.open(dir .. "/" .. name, "w"))
    f:write(text)
    f:close()
end
write("cam.srt", "1\n00:00:00,000 --> 00:00:00,500\n" .. EMBEDDED .. "\n\n2\n00:00:00,500 --> 00:00:01,000\n" .. EMBEDDED .. "\n")
write("film.srt", "1\n00:00:00,000 --> 00:00:01,000\nHello there.\n")
local mp4 = dir .. "/clip.mp4"
local made = os.execute("ffmpeg -v error -y -f lavfi -i color=c=black:s=64x64:d=1 -i '" .. dir .. "/cam.srt' -i '" .. dir
    .. "/film.srt' -map 0 -map 1 -map 2 -c:v mpeg4 -c:s mov_text '" .. mp4 .. "' 2>/dev/null")
if made == true or made == 0 then
    local texts = telemetry.text_samples(mp4, 3)
    expect("text tracks read", texts ~= nil, true)
    texts = texts or {}
    expect("the video track isn't one", texts[0], nil)
    expect("the camera's track, by its index in the file", texts[1] and texts[1]:sub(1, #EMBEDDED), EMBEDDED)
    expect("…its samples joined", texts[1] and select(2, texts[1]:gsub("\n", "")), 1)
    expect("…read as the camera's", telemetry.camera_text(texts[1]), true)
    expect("the film's track", texts[2], "Hello there.")
    expect("…not the camera's", telemetry.camera_text(texts[2]), false)
    expect("no file", telemetry.text_samples(dir .. "/none.mp4", 3), nil)
    local track = telemetry.text_track(mp4, 1) or {}
    expect("the camera's track, all its samples", #track, 2)
    expect("…with their times", track[2] and track[2].t, 0.5)
    local from = telemetry.from_text(track)
    expect("…as telemetry", from and telemetry.at(from.values.iso, 0.7), 100)
else
    print("ffmpeg didn't make the MP4: its part skipped")
end
os.execute("rm -rf '" .. dir .. "'")

print(("%d checks, %d failed"):format(checks, failures))
os.exit(failures == 0 and 0 or 1)
