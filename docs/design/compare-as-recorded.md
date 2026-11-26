# As recorded ↔ as it plays here

**Status:** built in this PR (`h`, script message `afp-compare`); latency measured as the PR says; Windows and Linux
not tried live.

A first-time user should see at once what the player does to the shipped sample dive: grey, wobbly and noisy as the
camera recorded it, against colour, steady and clean here. Anton chose to show that inside the player (2026-11-26),
not by asking people to open the file in another player first (on Windows the stock player often can't play 10-bit
HEVC at all). The welcome card names the key and sends the message.

## What the user gets

- **`h` held:** the clip as recorded until the key is let go, then as it plays here. **Tapped** (under 0.35 s): it
  stays as recorded until `h` again. A card says **As recorded**, what's left out on this clip (the colour, the
  stabilization, the denoise) and how to go back.
- **`script-message afp-compare [on|off|toggle]`** (toggle when none) does the same for the welcome card, a button
  or `input.conf`. `user-data/gyroflow/compare` says which is shown.
- Paused or playing, it switches with the next frame drawn. Nothing is rebuilt.

As recorded means: the camera's code values shown as a player that doesn't know the log curve shows them (BT.709,
SDR white at HLG 75 %, as `underwater_hlg.glsl` puts SDR into HLG), the frame as decoded (no stabilization, no lens
correction, no edge fill, no colour fringes taken out), no temporal denoise.

## Why a switch, not a split view

A divider over the picture (as on the site's landing) needs the source and the processed frame in one render. On the
GPU path the warp usually runs on the planes (`Mode::Planes`, LUMA and CHROMA), so the source is gone before MAIN:
a split would need a copy of both planes every frame and our own YUV → RGB for its half, and the denoise shows a
frame 5 behind the newest, so the two halves would be different frames unless the copy kept a ring of its own. A
switch needs none of that: two DYNAMIC parameters, no shader text changed, no cost while it's off.

## How

`mpv/scripts/gyroflow/compare.lua` sets two keys of `glsl-shader-opts` in one change:

- `gyroflow_gpu_on=0`. Every pass gyroflow-gpu-table writes (warp, grid, edge fill, sphere, denoise) has
  `//!WHEN gyroflow_gpu_on` (the export's frame count already relies on it, `gpu_table::passes_per_frame`), so
  libplacebo skips them all and mpv scales the frame as decoded.
- `afp_recorded=1`. The look shaders (`shaders/log_hlg.glsl.in`, `dlogm_hlg.glsl.in`, `dlogm_lut3d.glsl.in`,
  `underwater_hlg.glsl`) declare it as a DYNAMIC int (0 by default, so exports and every other path are unchanged)
  and take their as-recorded branch: the HLG ones map the code values as BT.709 into HLG (the video and the target
  stay tagged HLG, so mpv's colour pipeline doesn't change), the maker's LUT passes them through as Rec.709.
  NBCU's SDR conversion after the HLG shader is left as it is: in the nbcu look, as recorded is NBCU's SDR of that
  HLG, a little darker than a Rec.709 player would show it. The tone-mapped and Dolby Vision looks likewise map it
  their own way.

Back, both keys come out and the shaders' defaults hold. A paused redraw shows the change: our mpv's patch 0011 hashes
the parameters' values; upstream mpv 0.42 (the Windows app's) flushes its frame cache on any render-option change
while paused.

Where the warp turned or reshaped the frame (the file's own turn, a 9:16 output), mpv is given the file's geometry
meanwhile (`video-rotate`, `video-aspect-override`, file-local) and the warp's back after, so as recorded isn't
stretched. A 360 camera's own file isn't compared: its picture is the warp's sphere, nothing without it (a card says
so). Photos, photo links and moment pictures taken meanwhile turn it off first (`export.lua`).

## What it costs, and its limits

- **The denoise's ring isn't fed while skipped.** Let go while playing, the clean picture holds its first frame for
  the 5 frames the denoise shows behind (as after a seek), noisier for those frames, then plays on clean.
- **With the denoise on, as recorded is the newest frame decoded**, the clean one 5 frames before it. Playing, that's
  invisible; paused, the two are 5 frames (83-167 ms) apart. Showing the same paused frame would need a seek there and
  the ring filled again on the way back: not instant, so not done.
- **The first switch of a session** may build the pipelines of the path without the warp (mpv's own conversion and
  scaler at the video's size), once; later ones take them from the cache.

## Measured

`tools/compare_latency/run.py` (takes the screen: [testing.md](../dev/testing.md)): ask (the host clock in
`afp-compare`'s message, `afp_input_time`) to the first frame shown with it (`--macos-latency-log`, patch 0010), on a
4K Osmo Action 6 clip and a 1080p one, playing and paused. The numbers are in the PR.

## Not built

- A button for it in the side panel (the welcome card and `h` are the ways in).
- As recorded for 360 cameras' own files (their dual fisheye shown flat).
- The exact paused frame with the denoise on (above).
