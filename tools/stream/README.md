# afp-stream: clips stabilized and in HDR over the network

A small service for the NAS (an Intel N100 here): each clip of the library as an HLS stream, stabilized on the GPU with
Gyroflow's warp (shaders/gyroflow/warp.glsl) and, for DJI's D-Log M, given the player's hdr look: HEVC Main10, HLG
(BT.2020), fMP4 segments of 2 s, the audio copied. Nothing leaves the GPU: VAAPI decodes, libplacebo renders into the
encoder's own surfaces, VDEnc encodes (docs/gpu-render.md, "The streaming server").

- `http://<nas>:8099/`: the clips, each with its stream (Safari and Apple TV play HLS by themselves; `watch` opens a
  page with a player)
- `http://<nas>:8099/playlist.m3u`: every clip as a channel, for Jellyfin: Dashboard → Live TV → Tuner Devices → Add →
  M3U Tuner, this URL. Or any player that takes an M3U (VLC, Infuse)
- `http://<nas>:8099/clip/<id>/index.m3u8`: one clip's stream, as VOD: every segment listed at once (from the clip's
  rate and length, a GOP of 2 s each), so players start at the beginning and can seek. The render starts with the
  playlist; a segment request waits until it's rendered (a player starts after ~6 s), and one far ahead of the render (a
  seek) starts it again from there. An EVENT playlist that grew as it rendered made Safari start near its end and stall
  at that edge. Byte ranges and HTTP/1.1 keep-alive as AVFoundation likes them

Checked 2026-11-07 with AVFoundation itself (tools/stream/hlsplay.swift, what Safari and iOS play HLS with): clip 0126
and two library clips start in 6-8 s, play at real time without a stall, seek ahead, end cleanly.

Files:

- `afp-render.c`: the renderer (VAAPI decode → each plane imported into Vulkan as a dmabuf → libplacebo with mpv user
  shaders, PTS set per frame → the encoder's VAAPI P010 surfaces → hevc_vaapi low-power → HLS)
- `afp_stream.py`: the service; per clip, `gyroflow-gpu-table --player` writes the warp from the clip's project (else its
  own motion data with the underwater preset, at the medium level; else the lens alone; else the planes just scaled,
  which also keeps a clip without motion data faster than real time: 75-80 fps against 43 through libplacebo's own
  conversion of the whole 4K source), then afp-render runs. The first play of a clip without motion data waits ~10 s
  for those tries; their answer is kept
- `Dockerfile`: the image, on top of tools/gpu-spike's (mpv 0.42 + libplacebo 7.387 on Fedora 43 with RPM Fusion's ffmpeg)

On the NAS (2026-11-07), in `~claude/afp-test/stream`: `gyroflow-gpu-table` built for Linux there (15 min on the N100),
`dlogm_hlg.glsl` (tools/cube_to_glsl.py), `underwater.gyroflow` (mpv/scripts/gyroflow), and the container:

    docker run -d --name afp-stream --restart unless-stopped --memory=1500m --device /dev/dri -p 8099:8099 \
      -v /home/claude/afp-test/clips:/clips:ro -v /volume1/DJI_Inbox:/library/DJI_Inbox:ro \
      -v /home/claude/afp-test/stream:/stream -e AFP_LIBRARY=/clips:/library/DJI_Inbox afp-stream:f43 \
      bash -c "cd /stream && cc -O2 afp-render.c -o /usr/local/bin/afp-render \$(PKG_CONFIG_PATH=/opt/pl/lib/pkgconfig \
        pkg-config --cflags --libs libplacebo libavformat libavcodec libavutil libva libva-drm) -lm && python3 -u afp_stream.py"

The library is mounted read-only: the service writes only its renders (`/stream/cache`, gone after 12 h unasked).
The memory cap keeps it from pushing the Home Assistant VM out.

Settings (environment): `AFP_INTERP` (bilinear; bspline, bicubic, lanczos), `AFP_CHROMATIC` (0; 1 takes the water's
color fringes out), `AFP_HEIGHT` (1080), `AFP_BITRATE` (12M), `AFP_LIBRARY`, `AFP_PORT` (8099).

Measured on the N100, clip 0126 (Osmo Action 6, 4K60 D-Log M), 1080p HLG out, with the look:

| Warp | Render |
|---|---|
| planes, bilinear (the default) | 67-69 fps |
| planes, B-spline | 61 fps |
| planes, Gyroflow's bicubic (9 fetches) | 59 fps |
| RGB with the fringes taken out, bicubic | 48 fps |

Faster than the clip's rate is what a stream needs: below it the player catches up with the render and waits. 8K clips
(the Action 6's 4:3 trip clips) render at ~42 fps against their 50, the N100's decoder being the limit; players buffer.

Not done: Dolby Vision 8.4 (an RPU per frame in the bitstream; vs-gyroflow's dovi.rs writes them for exports), the edge
fill's sphere trail (the smooth fill streams), several streams at once beyond two renders.
