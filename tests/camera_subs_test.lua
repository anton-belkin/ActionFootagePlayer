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
else
    print("ffmpeg didn't make the MP4: its part skipped")
end
os.execute("rm -rf '" .. dir .. "'")

print(("%d checks, %d failed"):format(checks, failures))
os.exit(failures == 0 and 0 or 1)
