# afp-stream: clips stabilized and in HDR over the network

A small service for the NAS (an Intel N100 here): each clip of the library as an HLS stream, stabilized on the GPU with
Gyroflow's warp (shaders/gyroflow/warp.glsl) and, for DJI's D-Log M, given the player's hdr look: HEVC Main10, HLG
(BT.2020), fMP4 segments of 2 s, the audio copied. Nothing leaves the GPU: VAAPI decodes, libplacebo renders into the
encoder's own surfaces, VDEnc encodes (docs/gpu-render.md, "The streaming server").

- `http://<nas>:8099/`: the clips by folder, each with its stream (Safari and Apple TV play HLS by themselves; `watch`
  opens a page with a player) and how much of it is rendered; per folder, **pre-render openings** (the first 8 s of each
  clip, `AFP_OPENING`) and **pre-render whole clips**: queued, rendered one clip at a time at low priority, and only
  while nobody watches (a viewer's render stops the background one). `/status` says what renders and what's queued
- `http://<nas>:8099/playlist.m3u`: every clip as a channel, for Jellyfin: Dashboard → Live TV → Tuner Devices → Add →
  M3U Tuner, this URL. Or any player that takes an M3U (VLC, Infuse)
- `http://<nas>:8099/clip/<id>/index.m3u8`: one clip's stream, as VOD: every segment listed at once (from the clip's
  rate and length, a GOP of 2 s each), so players start at the beginning and can seek. The render starts with the
  playlist; a segment request waits until it's rendered, and one far ahead of the render (a seek) starts it again from
  there. Segments of a second (`AFP_SEGMENT`). **What played stays** in the cache, up to `AFP_CACHE_GB` (40; the clips
  used longest ago go first), and goes when the clip's project or the service's settings change; the render picks up a
  few segments before the end of what's cached.
- Time to the first picture in AVFoundation: a clip never played ~5 s (the renderer's start, ~1.4 s, then its first
  segments while the player builds a buffer); a pre-rendered opening ~1.9 s with 4 s of it; a clip played before 0.4 s. An EVENT playlist that grew as it rendered made Safari start near its end and stall
  at that edge. Byte ranges and HTTP/1.1 keep-alive as AVFoundation likes them

Checked 2026-11-07 with AVFoundation itself (tools/stream/hlsplay.swift, what Safari and iOS play HLS with): clip 0126
and two library clips start in 6-8 s, play at real time without a stall, seek ahead, end cleanly.

Files:

- `afp-render.c`: the renderer (VAAPI decode → each plane imported into Vulkan as a dmabuf → libplacebo with mpv user
  shaders, PTS set per frame → the encoder's VAAPI P010 surfaces → hevc_vaapi low-power → HLS)
- `afp_stream.py`: the service; per clip, moment or clip preview, the player itself is asked how it plays it (below, "As
  the player plays it"), `gyroflow-gpu-table --player` writes the warp from that (else the planes just scaled, which
  also keeps a clip without motion data faster than real time: 75-80 fps against 43 through libplacebo's own conversion
  of the whole 4K source), then afp-render runs
- `jellyfin.py`: a project's moments and clips as a Jellyfin library (below, "In Jellyfin")
- `Dockerfile`: the image, on top of tools/gpu-spike's (mpv 0.42 + libplacebo 7.387 on Fedora 43 with RPM Fusion's ffmpeg)

On the NAS (2026-11-07), in `~claude/afp-test/stream`: `gyroflow-gpu-table` built for Linux there (15 min on the N100),
`dlogm_hlg.glsl` (tools/cube_to_glsl.py), the player's plugins in `player/` (below), and the container:

    docker run -d --name afp-stream --restart unless-stopped --memory=1500m --device /dev/dri -p 8099:8099 \
      -v /home/claude/afp-test/clips:/clips:ro -v /volume1/DJI_Inbox:/library/DJI_Inbox:ro \
      -v /home/claude/afp-test/stream:/stream -e AFP_LIBRARY=/clips:/library/DJI_Inbox -e AFP_BITRATE=30M \
      -e AFP_TV_LOOK=pqg2.5r30 afp-stream:f43 \
      bash -c "cd /stream && cc -O2 afp-render.c -o /stream/afp-render \$(PKG_CONFIG_PATH=/opt/pl/lib/pkgconfig \
        pkg-config --cflags --libs libplacebo libavformat libavcodec libavutil libva libva-drm) -lm && python3 -u afp_stream.py"

The library is mounted read-only: the service writes only its renders (`/stream/cache`) and what the player said
(`/stream/args`). Read-only matters: opening a moment, the player fills a key missing from its link and writes it at
once, as it does on a Mac; here the write fails and nothing changes.
The memory cap keeps it from pushing the Home Assistant VM out.

Settings (environment): `AFP_INTERP` (bilinear; bspline, bicubic, lanczos), `AFP_CHROMATIC` (0; 1 takes the water's
color fringes out), `AFP_HEIGHT` (1080), `AFP_BITRATE` (12M), `AFP_SEGMENT` (1 s), `AFP_CACHE_GB` (40), `AFP_OPENING`
(8 s), `AFP_LIBRARY`, `AFP_PORT` (8099), `AFP_PLAYER` (/stream/player), `AFP_ARGS` (/stream/args).

## As the player plays it

Each file is streamed the way the player shows it, with nothing to change while it plays: a clip with its saved view
(`<clip>.afp.json`) or its Gyroflow project; a moment (`_Moments/*.afplink.mp4`) as its part of the original, from 0,
with its own settings and its own copy of the project; a clip preview (`_Clips/**.afplink.mp4`) as its whole clip,
with its own settings when it has them. The player says what that is: mpv with the player's plugins (workspace,
gyroflow, dlogm) and no window, `--script-opts-append=gyroflow-render_args=<file>` (mpv/scripts/gyroflow/main.lua),
writes the original, the part (offset, length), gyroflow-gpu-table's request and the log curve, and quits. So every
rule of the player counts (what's saved for the file over the defaults, a project over the bundle, a moment stabilized
on its own), and none is copied here. The stream changes only what it must: the smooth edge fill for the sphere trail,
its own interpolation and color fringes (`AFP_INTERP`, `AFP_CHROMATIC`), the hdr look (HLG) for D-Log M whatever
look the player starts in.

- `/stream/player/`: the plugins (the repo's Lua over the Linux plugin bundle's binaries: gyroflow-probe and the rest),
  and `config/script-opts/gyroflow.conf` and `dlogm.conf`, the defaults (Anton's, copied from the Mac's app:
  `~/Library/Application Support/ActionFootagePlayer/mpv/script-opts/`). Change them there to change what plays.
- The answer is kept (`/stream/args/<id>.json`) with the size and time of each file it depended on (the file, the
  original, their `.afp.json` and `.gyroflow`, the two settings files), and asked again when one of them changes;
  what was rendered then goes too. ~1-3 s per file (7 s for the first), all moments and clip previews in the background
  from the start. afp-render's `-offset` gives a moment's part its own time, from 0, as mpv's EDL does.
- `GET /catalog.json`: the moments and clip previews the player could play, each with its project, folder, name, part,
  start (the camera's clock), whether it's stabilized, and its stream and cover (`GET /poster/<id>.jpg`, the frame
  the file shows). `pending`: those not asked yet.

Checked 2026-11-08: the moment "Maks diving close up" streamed as its 6.6 s part from 0, the frame and colors those of
the export of the same moment (`_Exports/`); its project's settings (wide, horizon locked, medium) as on the Mac.

## In Jellyfin

`jellyfin.py` writes a folder for a Jellyfin library of the type Home Videos: per moment and clip preview a `.strm`
(its stream's URL), a `.nfo` (title, sort by time, date, what it is) and its cover (`-poster.jpg`; a folder's
`folder.jpg` is its first item's):

    Моменты/<trip>/<moment>.strm      a moment: its subfolder in _Moments/, else its clip's trip, else Без поездки
    Клипы/<trip>/<clip>.strm          a clip preview, the whole clip: its folder in _Clips/ (the trips sorted there)

A clip preview deleted (not interesting) is gone from Jellyfin, a moment deleted too: the files it wrote are listed in
`.afp-jellyfin.json` and only those are removed. Nothing is written beside the footage. After a change it refreshes
that one library (the HDD libraries stay asleep). On the NAS since 2026-11-08:

    docker run -d --name afp-jellyfin --restart unless-stopped --network host --memory=128m --user 1007:10 \
      -v /home/claude/afp-test/stream/jellyfin.py:/jellyfin.py:ro -v /home/claude/afp-test/jellyfin.key:/run/jellyfin.key:ro \
      -v "/volume1/ssd/video/_Action Footage:/out" afp-stream:f43 python3 -u /jellyfin.py \
      --stream http://192.168.50.186:8099 --out /out --jellyfin http://127.0.0.1:8899 \
      --jellyfin-path "/data/_Action Footage" --key-file /run/jellyfin.key --names ru --every 120

The library "Action Footage" (Home Videos) has `/data/_Action Footage` (Jellyfin's container sees
`/volume1/ssd/video` as `/data`; it doesn't need the footage, only the streams), with Jellyfin writing nothing into it
(no NFO or image savers), no remote metadata, and no trickplay or chapter images (they would render every clip). Jellyfin
reads each stream when it plays: HEVC Main10 HLG 1080p, direct play where the device has HEVC (iPhone, a Samsung TV)
from afp-stream itself, so the device must reach `--stream`'s address; else Jellyfin converts it (H.264 SDR, checked).
The library's runtime shows once an item has played (Jellyfin reads a .strm only then).

Older TVs play HLS only in MPEG-TS segments (Samsung's Tizen before 5, 2020: jellyfin-web's `canPlayNativeHlsInFmp4`;
LG's webOS before 3.5), and Jellyfin can't tell TS from fMP4 inside its "hls" container, so it hands them the stream to
play directly. A 2017 Samsung (Tizen 3.0) hung on the fMP4 one. afp-stream gives those players (by their User-Agent, or
`?fmt=ts`) the same segments remuxed to TS (`seg<n>.ts`, made once from `seg<n>.m4s`, HEVC with its parameter sets in
band, the timestamps kept). Every request is logged with the client and its User-Agent (`AFP_LOG_REQUESTS=0`: none). The TV doesn't even fetch the `.strm`'s URL itself: Jellyfin passes the stream on from
its own address (`/Videos/<id>/stream`, User-Agent `Jellyfin-Server`), where a playlist's relative segment names lead
nowhere. Jellyfin gets a master playlist pointing back here instead, so the device fetches the stream from afp-stream
with its own User-Agent.

Looks: a stream id may end in `~pq` (the hdr look's HLG made HDR10 by libplacebo, `afp-render --pq`) or `~sdr` (the
player's nbcu look, `nbcu_hlg2sdr.glsl` last, BT.709: `--sdr`); the TS players get `AFP_TV_LOOK` unless the URL names
one. On the NAS it's `pq` since 2026-11-08: the Samsung TV (Tizen 3.0) showed our HLG gray through Jellyfin's app
(its web engine doesn't take HLG as HDR), while HDR10 and SDR looked right and alike (Anton, on "Maks diving close
up"); HDR10 keeps the HDR. The HDR10 is the HLG as BT.2100 displays it, at its nominal 1000 nits: libplacebo renders HLG
for the target's peak, which for PQ without metadata is 10000 nits, and the first version had it so (Anton: "very
contrasty, colors not saturated, shadows crushed, sky to white"). Measured on a Mountains clip, 2026-11-09: mid-tones
2.5x the HLG's, highlights 6x (peaks 2450-3080 nits), shadows halved. Fixed: mid-tones 1.03x, highlights 1.006x, peaks
442 against 440 nits; the frames carry HDR10's static metadata (mastering display BT.2020/D65 0.005-1000 nits, MaxCLL
1000) as SEI, in the TS segments too. HDR10+ (per-frame metadata) isn't written: hevc_vaapi can't, and whether a 2017
Tizen honours it in Jellyfin's web player is unknown.

Brighter for a room: a look may add `x<peak>` (`hlg_highlights.glsl`: the highlights expanded into the headroom, the
LUT's top, 422 nits, to `peak`, mid grey kept) and `g<gain>` (`hlg_gain.glsl`: every pixel's display light times the
gain, up to 500 nits, then a shoulder below 1000). The HDR10 at BT.2100's levels (mid-tones 35-55 nits, the brightest
at ~203, peaks 300-450) looked dark on the TV in a lit room, its HDR mode keeping to absolute nits where a Mac's EDR
follows the brightness slider; expanding the highlights alone didn't help (Anton: the mid-tones and shadows are what's
dark). Of x1, 1.5, 2 and 2.5 on clip 0065 (median 45 -> 67, 90, 112 nits; peaks up to 760) Anton chose 2.5 for that TV,
2026-11-09: `AFP_TV_LOOK=pqg2.5`.

At 30 fps for the TV: with the PQ conversion and the gain the TV's renders ran at 33-54 fps against the clips' 59.94,
and it stopped between segments. A look may end in `r<fps>` (`afp-render -every n`: all frames decoded, every n-th
rendered and encoded, keyframes and segments where they were; the TV interpolates). Clip 0087 (57.8 s): 103.8 s to
render at 60 fps (0.56x real time), 40.1 s at 30 (1.44x); clip 0082: 0.57x and 1.34x. `AFP_TV_LOOK=pqg2.5r30` since.

Pre-rendering for the TV: the TV plays its own renders (its look), so what was pre-rendered for the player's HLG doesn't
help it. `GET /prerender` takes `look=tv` (`AFP_TV_LOOK`, or a look as in a stream id), `recursive=1` (the folder and
all below it; `folder=` empty: everything) and `links=1` (only moments and clip previews, what Jellyfin shows). The
openings (8 s each, `AFP_OPENING`) of everything in Jellyfin, for the TV, so that a folder played in a row starts each
clip at once:

    curl "http://192.168.50.186:8099/prerender?folder=&recursive=1&links=1&what=opening&look=tv"

A whole run (337 openings) took 43 min on 2026-11-09, 9 GB.

What renders while the TV plays (found from the cache files' times, 2026-11-09): Jellyfin's own probe of each stream
(ffprobe, User-Agent `Lavf`, ~5 s before the TV starts) set off a render of the whole clip in HLG, which nobody watched
and which halved the GPU for the TV's (27-29 fps where alone it does 40+). A probe now gets at most the 2 segments it
reads rendered. When Jellyfin passes a stream on (`Jellyfin-Server`), the TV look's render starts at once from the
first segment not in the cache, instead of when the TV gets near it, and nothing in the look Jellyfin's request names
(that started a second HLG render too); and a render nobody has asked for in 15 s (the player went to another clip)
stops. Measured after (clip 0127, opening pre-rendered): only the TV's render runs, 1.7x real time at 30 fps. What
was rendered stays in the cache (`AFP_CACHE_GB`): played again or sought back, it's served from there with nothing
rendered.

It renders one clip at a time at low priority, only while nobody watches; `/status` shows it. Per folder, the index page
has "openings for the TV". Run 2026-11-09: 337 openings in 43 min, 9 GB.

Jellyfin's iOS app (1.8) with **Use Native Video Player** (beta) on says it takes HEVC only as SDR or HDR10, so
Jellyfin converts the HLG to SDR itself (tone-mapped, desaturated) on the N100. With it off (the default) the app's
web player takes HLG, and the iPhone plays the stream directly from afp-stream (checked 2026-11-08: fMP4 from here, no
transcode, the colors as the player's).

To remove it all: the library in Jellyfin's dashboard, `docker rm -f afp-jellyfin`, the folder `_Action Footage`.

Measured on the N100, clip 0126 (Osmo Action 6, 4K60 D-Log M), 1080p HLG out, with the look:

| Warp | Render |
|---|---|
| planes, bilinear (the default) | 67-69 fps |
| planes, B-spline | 61 fps |
| planes, Gyroflow's bicubic (9 fetches) | 59 fps |
| RGB with the fringes taken out, bicubic | 48 fps |

The bitrate costs no speed: 12, 30 and 50 Mbit/s rendered alike (2026-11-08, 20 s of a 4K60 clip, VDEnc). On the NAS
it's 30M since then, Anton finding 12M too little on a TV; a Samsung TV's Ethernet port is 100 Mbit/s.

Faster than the clip's rate is what a stream needs: below it the player catches up with the render and waits. 8K clips
(the Action 6's 4:3 trip clips) render at ~42 fps against their 50, the N100's decoder being the limit; players buffer.

Not done: Dolby Vision 8.4 (an RPU per frame in the bitstream; vs-gyroflow's dovi.rs writes them for exports), the edge
fill's sphere trail (the smooth fill streams), several streams at once beyond two renders.
