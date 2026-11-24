# Seek, frame steps and scrubbing

Analysis 2026-11-24, then Anton's answers the same day (below). Nothing is built yet.

Two related problems:

1. With the temporal denoise on (the GPU path's `dn_` passes), what the seek bar says, what the window shows and
   where a frame step lands don't agree, paused and stepping most of all.
2. The seek bar is slow and finding a place in a clip is hard. Idea: a small picture under the cursor as it hovers,
   from the camera's low-resolution file (GoPro `.LRV`, DJI `.LRF`) or the clip's keyframes, and a click lands exactly
   on the moment shown.

Rigs used here are in `tools/scrub/`: `dnrepro.py` (problem 1 without a window) and `thumbbench.c` (picture timings).

---

## Decided (Anton, 2026-11-24)

- **Problem 1: proposal A.** Paused after a click, the raw frame at once and the clean one replaces it on screen, as
  long as showing the raw frame first costs no extra time.
- **Problem 2:**
  - No snapping. The bar goes to any time, as now; a click is an exact seek to the cursor's time.
  - With an LRF/LRV beside the clip, the picture is that exact frame (any frame, not only keyframes: measured fast
    enough, below). Without one, the picture is the clip's keyframe at or before the cursor (the last one that has
    already happened), and the click still lands at the cursor's time.
  - The picture shows on plain hover, and while dragging; dragging doesn't seek: one exact seek on release.
  - Nothing is built ahead or in the background: every picture is made when the mouse asks for it (~15-50 ms).
    Pictures already made may be kept, in memory and on this computer's disk only (never beside the clips).
  - Size: about twice the height of the bar (the bar is 66 pt, so ~130 pt high, ~230 pt wide for 16:9).
  - Colours as the file has them: no LUT, no stabilization (a log clip without a sidecar looks flat; that's fine).
  - Insta360 / 360 clips: postponed.


## 1. Pause, seek and frame steps with the temporal denoise

### How it works now

The denoise averages each frame with the frames before and after it. The frames after it must already have been
decoded, so the picture on screen lags behind the newest decoded frame:

- `shaders/gyroflow/warp.glsl:1515-1531` (the comment) and `dn_window()` at `warp.glsl:1582`: the frames, as warped,
  go into a ring of `DN_RING` = 11 slots. Each render stores its frame (`dn_state`, `gdn_head = nw` at
  `warp.glsl:2086`) and shows the frame `dn_kc` before the newest: `dn_kc = min(dn_after(), back)`
  (`warp.glsl:1597`), where `back` counts how many frames before the newest are in an unbroken row (each step between
  0 and 2.6 frame durations). With Full, `gyroflow_dn_after` = 5.
- A render of the same frame again (paused, a redraw: `dn_same()`, `warp.glsl:1567`) stores nothing and shows the
  same window.
- `mpv/scripts/gyroflow/main.lua:1450-1520` (`dn_sync`): delays the audio by 5 frames (`audio-delay`,
  `main.lua:1497`) so sound matches the picture, and keeps `dn.restart` (where the row last started, a seek) so that
  `dn_shown()` (`main.lua:1460`) can tell the time of the frame on screen: `max(restart, time-pos − 5 frames)`.
- The only reader of that corrected time is the photo/moment export (`export.lua:786`, `shown_time()`). Everything
  else reads mpv's `time-pos`, which is the **newest decoded** frame, not the one on screen.
- `still.lua` (the paused clean frame on the VapourSynth path) does nothing on the GPU path: its `stabilizing()`
  looks for the vf filter (`main.lua:816-821`), which the GPU path doesn't have. Paused on the GPU path, what you see
  is whatever the ring holds.
- Frame steps: `,`/`.`, Option+←/→ (`navkeys.lua:16-17`), the bar's buttons (`bar.lua:124,127`) and the wheel over
  the bar (`side-panel/main.lua:2041`) all run mpv's own `frame-back-step` / `frame-step`, which know nothing of the
  ring. `frame-step` plays one more frame; `frame-back-step` is an exact seek to `time-pos` − 1 frame.

### What goes wrong (reproduced)

`tools/scrub/dnrepro.py` drives the app's own mpv with `--vo=null` over IPC (no window), takes the frame mpv hands
the VO after each command, and runs the shader's ring logic (`dn_window`/`dn_state`, ported line by line) on that
sequence. Clip `DJI_20261022152841_0068_D` (Action 6, 59.94 fps), Full denoise. Frame numbers from the clip's start;
"neighbours" = frames before + after the shown one that went into its average (5+5 = fully clean).

| step | seek bar (`time-pos`) | on screen | difference | neighbours |
|---|---:|---:|---:|---:|
| seek to 10 s, paused | 600 | 600 | 0 | 0+0 (noisy) |
| play 2 s, pause | 634 | 629 | **−5** | 5+5 |
| `.` ×3 | 635…637 | 630…632 | −5 | 5+5 |
| `,` (1st) | 636 | **636** | 0 | **0+0** |
| `,` (2nd, 3rd) | 635, 634 | 635, 634 | 0 | 0+0 |
| `.` (1st…5th) | 635…639 | **634, 634, 634, 634, 634** | −1…−5 | 0+1…0+5 |
| `.` (6th) | 640 | 635 | −5 | 1+5 |
| click on the bar at 20 s, paused | 1199 | 1199 | 0 | **0+0 (noisy)** |
| `.` (1st…5th) | 1200…1204 | **1199 ×5** | −1…−5 | 0+1…0+5 |
| `.` (6th, 7th) | 1205, 1206 | 1200, 1201 | −5 | 1+5, 2+5 |

So, concretely:

1. **Paused after playing, the bar is 5 frames ahead of the picture** (83 ms at 60 fps, 167 ms at 30). Marks (`m`),
   A–B (`set ab-loop-a ${=time-pos}`, `side-panel/main.lua:736`), cut before/after (`workspace/main.lua:2484-2488`),
   highlights (`highlights/main.lua:256,331,340`), telemetry, the 360 recording, the under-water colour grab
   (`uwcolor.lua:109`) all take `time-pos`, i.e. a frame 5 later than the one you looked at.
2. **The first `,` jumps forward**: from the shown 632 to 636 (4 frames *later*), and the picture turns noisy (no
   neighbours: the new frame is older than the newest in the ring, the row is broken). Each further `,` steps one
   back, still noisy.
3. **After a `,` or a click on the bar, `.` does nothing five times**: the first frame after a break stays on screen
   until the five after it are there ("after a seek the first frame stays", by design for playback), so `.` ×5 shows
   the same frame, then it moves on, 5 behind the bar again.
4. **Paused, a click on the bar shows the noisy frame** and it stays noisy: nothing refills the ring around it. The
   clean paused frame only exists after playing.
5. Playing on after a seek, the first frame is held for 5 renders (83–167 ms): harmless (the audio is delayed as
   much) but it adds to "the seek feels slow".

### Fix (proposal A)

Three parts, each small, from the bottom up.

**A1. One time for "the frame on screen".** `main.lua` publishes it (`user-data/gyroflow/shown-pos`, from the same
rule as `dn_shown()`, plus A2's override), observed instead of `time-pos` by: the bar (knob, time, hover), marks,
A–B, cut, highlights, telemetry overlay, 360 recording, uwcolor, the agent's position, export's `shown_time()`
(which then just reads it). Without the denoise it is `time-pos` itself. This alone fixes point 1.

**A2. The shader can show a chosen frame from its ring.** A new param `gyroflow_dn_show` (a PTS; 0 = as now). When
set, `dn_window()` finds the slot with that PTS in the unbroken row and averages it with what the row has on each side
(up to 5 and 5), instead of always "newest − 5". Changing a param re-renders at once (patch 0011). Then, paused:

- `.` when the frame after the shown one is in the ring (after a `,`): just `dn_show` = it — instant, no decode.
  When it isn't (the usual case: shown = newest − 5): mpv `frame-step` as now, which already shows the next frame
  fully clean.
- `,`: `dn_show` = the shown frame − 1, while the ring still has it (5 steps back from a played pause, with 4, 3, 2,
  1, 0 frames before it — and 5 after). Instant, no jump, mostly clean.
- `,`/`.`/Option+arrows/the bar's buttons/the wheel go through one script binding (`gyroflow/step` −1/+1) that does
  this when the denoise delay is on, and mpv's own commands otherwise.

**A3. Paused, the ring is refilled around the shown frame.** When the shown frame has fewer than 5+5 neighbours
(after a click on the bar, a `,` past the ring, a clip opened paused): after a click, at once, once the raw frame is
on screen (the raw frame is the click's own exact seek, as now: no extra time before it); after frame steps, once
they stop for `still_delay` (0.4 s), so quick stepping isn't held up. The refill: seek exact to the shown frame − 5 frames, then
`frame-step 10` (mpv 0.41 takes a count; measured: 10 frames played in ~180 ms at 60 fps, ~330 ms at 30 fps), with
`dn_show` held on the shown frame the whole time so the picture doesn't move — it only gets cleaner. The old raw
copy of that frame stays in the ring until the new, clean one replaces it (the ring is 11 slots; the refill writes
exactly 11). Cost: one exact seek (~150–250 ms on a 4K60 clip, below) + 10 frames; the clean frame ~0.3–0.6 s after
you stop, the raw one at once, as now. Playing on from a refilled frame then needs no 5-frame hold.

Edge cases to handle: the last 5 frames of a clip (no frames after: show what there is), crop mode (after = 0
already), `fps_limit` (frames every 2nd/4th: steps are in played frames), a moment link (its own time offset),
Light (3+3).

### As built (A1–A3)

- `mpv/scripts/gyroflow/dnstep.lua` (plain Lua): the frame on screen, the ring's row, a step (from the ring, a few
  frames played on, or a seek), the fill (seek to the frame − radius, play radius + after frames), playing on from the
  shown frame. `tests/dnstep_test.lua` runs it against a model of the shader's ring.
- `main.lua` (the dn block): `,` `.` are the plugin's `frame-back` / `frame-on`; Option+arrows (`navkeys.lua`), the
  bar's buttons and the wheel over it call them. Without the delay (no denoise, crop mode, the VapourSynth path)
  they run mpv's own `frame-back-step` / `frame-step`. The fill: after a seek of someone else's at once, after steps
  once they stop for `still_delay`, after pausing when it played less than the ring since a seek. A step pressed
  during a fill or frames being played waits its turn. A seek of someone else's clears the frame asked for only
  once its frame is there (the shader keeps the old one meanwhile: no flash).
- `user-data/gyroflow/gpu_delay` = `{ delay, restart, show }`; the readers' `shown_time()` (side panel:
  `timeline.lua`'s, tested) for the bar's knob and time, marks (add, next/previous, the panel's), the panel's A–B
  buttons (`script-message-to side_panel part-a|b`), cut before/after, the menu, telemetry, uwcolor, export.
- `warp.glsl`: `gyroflow_dn_show` (−1: none); `dn_window()` looks the frame up by its time in the ring.
- Not changed: mpv's own `l` (A–B with the key) still takes `time-pos`; mpv's A–B loop runs on `time-pos`, so the
  loop's last 5 frames aren't shown before it jumps back.

Checked (2026-11-24):

- `luajit tests/dnstep_test.lua` (the logic against the ring's model), `tests/dnstep_ipc.py` (the app's mpv
  headless: steps, the fill, a click, seven steps back, playing on; 26 checks).
- The shader's `gyroflow_dn_show` by export (`AFP_GPU_KEEP`, the kept shader with its default set to frame 30):
  output frames whose newest is 30…40 all show frame 30 (PSNR 58–63 dB against the normal output's frame 30), the
  next one falls back to the shader's own choice; with −1 the output is bit for bit the old shader's.
- Live, 0068 in the installed build (Light, 3+3): paused after playing the picture is 3 behind time-pos; `,` and
  `.` from the ring change the frame on screen at once (the per-frame Dolby Vision L1 measure changes with them),
  time-pos stays; a click at 25 s: its frame on screen 0.24 s after the click, the fill's seek 0.25 s more, its 6
  frames 0.14 s (no drops), so the clean frame 0.64 s after the click (Full: 4 more frames, ~+0.07 s). A fill
  timed to the same millisecond as a click no longer swallows the click (seen in the first live run; fixed: no fill
  over a seek under way, a seek of ours told apart by where it lands).
- Not checkable by screenshots: mpv's `screenshot-to-file … window` renders at 1581×889 where the window renders
  1580×888, so the shader takes the ring as stale (its size differs) and the screenshot is the newest frame, raw.
  k's moment picture (`export.lua`, a screenshot with the denoise on) is therefore the newest decoded frame, not the
  one on screen: a follow-up.

### Simpler alternative (proposal B, not chosen)

Paused, use no frames after the shown one (after = 0, as crop mode does) — then the newest decoded frame *is* the
shown one, `time-pos` is right, and mpv's `frame-step` is always correct. Costs: at the moment of pausing the
picture jumps 5 frames forward (to the newest), the paused frame is cleaned from 5 frames before it only (one-sided:
weaker, about like Light); `,` still breaks the row (noisy until a refill of 5 frames before it). Less code, worse
picture. Anton chose A.

---

## 2. The seek bar

### How it works now

- Drawn as ASS by the side panel's `bar.lua` (`docs/navigation-bar.md`), redrawn every 0.1 s while playing and on
  each mouse move over it. Hover shows only a time bubble (`bar.lua:211-221`).
- Click: `seek <t> absolute+exact`. Drag: `seek <t> absolute+keyframes` on every mouse move, exact on release
  (`bar.lua:223-231`, `side-panel/main.lua:1962-1965`).

### Why it feels slow

- **An exact seek decodes from the keyframe before the target.** Action 6, HERO12 and Mini 4 Pro put a keyframe
  every 1.001 s (HERO4: 0.5 s), so a click decodes up to 59 frames of 4K 10-bit HEVC (30 on average at 60 fps).
  Measured (`thumbbench --hw --exact`, VideoToolbox, 0068): **139 ms median, 253 ms p90** local; on the NAS cold
  **157–205 ms median, ~260–280 ms p90**, reading ~6–7 MB each. Then the whole GPU chain renders.
- Anton's own seeks (responsiveness journal, 266 seeks from a seek to its first frame): **median 100 ms, p75 223,
  p90 393, p95 734, p99 1368 ms**.
- Each drag step is a keyframe seek of the full 4K clip plus the full warp/denoise render: on the NAS each one also
  waits for the share, and the next one queues behind it — the picture lags the mouse.
- With the denoise, playing on after a seek holds the first frame for 5 more frames (above).
- On a clip without a Gyroflow project, a seek beyond the GPU table's stretch asks for an urgent 20 s stretch
  (`ahead.lua`, #272) and can wait at the settled end (not measured here).
- Nothing shows *where* you'd land before you click, so finding a place is click-and-look, each costing the above.

### Preview on hover: design

**What Anton asked for:** a small picture at the cursor at once while hovering the bar; no colour correction or
stabilization; a click lands exactly on the moment shown.

**Which picture, and where the click lands** (as decided, no snapping):

- With an LRF/LRV: the exact frame at the cursor's time (the sidecar's own frame nearest it: it has 30 frames a
  second, the clip may have 60, so within 1/60 s). The click seeks exactly to the cursor's time: what you saw is
  where you land.
- Without one: the picture is the keyframe at or before the cursor (up to 1 s earlier on Action 6 / HERO12, 0.5 s on
  HERO4), labelled with the cursor's time; the click still seeks exactly to the cursor's time. So the picture can be
  up to a second before where you land — enough to find the place, then `,` `.` or Shift+←/→.
- A mark still wins within 7 pt (the bar's snap to marks stays).

**Sources, per camera** (checked on the NAS, `/Volumes/photo/DATA` incl. DJI_Inbox, `#recycle` left out):

| camera | preview file beside the clip | its video | keyframes | clips with / without it on the NAS |
|---|---|---|---|---|
| DJI new naming (Osmo Action 6, Mini 4 Pro, Mavic 3…): `DJI_<date>_<n>_D.MP4` | `<same stem>.LRF` | H.264 1280×720 29.97p, 7.6–10 Mb/s | every 1.001 s, as the clip | 431 / 58 (Maldives 2025 drone copied without) |
| GoPro HERO5+ (`GX`/`GH<ch><n>.MP4`) | `GL<ch><n>.LRV` | H.264 768×432 29.97p, ~2.9 Mb/s (HERO12) | 1.001 s | 817 / 690 (imported copies: Indonesia 2019, Maldives 2025, SD 01.2023…) |
| GoPro HERO3/4 (`GOPR<n>.MP4`, chapters `GP<ch><n>`) | `<same stem>.LRV` | H.264 432×240 | 0.5 s | 506 / 767 (iMovie Edits, Movies: imported) |
| older DJI (`DJI_0063.MOV`, Mavic Pro/2) | none | — | — | 0 / 500 |
| Insta360 X3/X4/X5 (`VID_….insv`) | `LRV_….lrv` per the cameras' docs, dual fisheye | not checked: no Insta360 originals on the NAS, the samples in ~/Downloads have none | | |
| iPhone, others | none | | | |

- The sidecars have the same duration and start (checked on Action 6, HERO12, HERO4 and Mini 4 Pro files), so a
  time in one is the same time in the other.
- GoPro `.THM` and the MJPEG cover stream inside DJI's MP4/LRF are one picture per clip: no use for scrubbing.
- **The LRF is not log.** On a D-Log M Action 6 clip the LRF frame has the camera's normal look while the MP4's is
  flat grey (`docs/img/seek-scrub-lrf-vs-mp4.jpg`, left LRF, right MP4, same time). Keyframes of a log clip will look
  flat (as they do in Finder); no LUT for now (decided).
- The Library already finds these files (`library/src/index.rs:359`, `camera_thumb()`); its rule lacks HERO3/4's
  `GOPR<n>.LRV` — one rule should serve both.
- Where there's no sidecar: the clip's own keyframes.

**Measured** (`tools/scrub/thumbbench.c`: one long-lived process, the file opened once, then 40 pictures at
shuffled times, each = seek to the keyframe, decode that one frame, scale to 320 px wide; "cold" = the file dropped
from the Mac's cache first with `msync(MS_INVALIDATE)`):

| file | where | open (index) | per picture, median / p90 | read per picture |
|---|---|---:|---:|---:|
| Action 6 0068 `.LRF` (720p H.264, CPU) | local | 28–34 ms | 12 / 16 ms | 176 KB |
| Action 6 0068 `.MP4` keyframe (4K HEVC, VideoToolbox) | local | 4–8 ms (5 MB) | 14.5 / 16 ms | 672 KB |
| same, CPU decode | local | | 96 / 115 ms | |
| Action 6 0068 `.LRF` | NAS cold | 145 ms | 23 / 38 ms | 176 KB |
| Action 6 0068 `.MP4` keyframe, VT | NAS cold | 41 ms | 28 / 48 ms (max 149) | 672 KB |
| Action 6 0087 `.LRF` (669 MB clip) | NAS cold | 65 ms | 22 / 43 ms | 177 KB |
| Action 6 0087 `.MP4` keyframe, VT | NAS cold | 53 ms | 33 / 68 ms | 745 KB |
| HERO12 GX010057 (5.3K HEVC) keyframe, VT | NAS cold | 13 ms | 21 / 38 ms | 691 KB |
| HERO12 GL010057 `.LRV` | NAS cold | 32 ms | 4.8 / 9.4 ms | 78 KB |
| HERO4 GOPR1030 (1080p H.264) keyframe, VT / CPU | NAS cold | 77 ms | 33 / 46 ms; CPU 70 / 78 ms | 757 KB |
| Insta360 X5 `.insv` (2×3840²) keyframe, VT | local | 4 ms | 24 / 40 ms | 1.25 MB |
| any warm (in the Mac's cache) | NAS warm | | LRF 12 ms, MP4 15 ms | |
| for comparison: `ffmpeg -ss t -skip_frame nokey … -frames 1`, a process per picture | local | | **160–180 ms** | |
| for comparison: exact frame, VT (what a click costs now) | NAS cold | | 157–205 / 261–280 ms | 6–7 MB |

For the record (a strip built ahead was dropped, see Decided): all keyframes in one pass with `ffmpeg -skip_frame
nokey` took 0.62 s for a 48 s LRF locally, 0.93 s for a 58 s LRF on the NAS.

**Any frame from the sidecar** (Anton's question: is a non-key frame from the LRF/LRV fast too? — yes). Same rig,
`--exact`: seek to the keyframe before the time, decode on to the exact frame (on average ~17 frames of the small
H.264), picture 460 px wide. The decoder's threading matters: frame threads, not slice threads (slice: 74 ms local).

| sidecar | where | per picture, median / p90 | read per picture |
|---|---|---:|---:|
| Action 6 0068 `.LRF` (720p) | local | 27 / 34 ms | 1 MB |
| Action 6 0087 `.LRF` | NAS warm | 26 / 35 ms | 1 MB |
| Action 6 0087 `.LRF` | NAS cold | 52 / 133 ms (max 191) | 1 MB |
| HERO12 GL010057 `.LRV` (768×432) | NAS cold | 15 / 109 ms | 0.4 MB |
| HERO4 GOPR1030 `.LRV` (432×240) | NAS cold | 6 / 39 ms | 0.1 MB |
| Action 6 0087 `.LRF`, the mouse moving along the bar (0.1 s steps: decode on, no seek) | NAS cold | **1.7 / 5.7 ms** | 0.1 MB |
| VideoToolbox for the LRF instead | local | 59 / 101 ms (slower for small H.264) | |

So any frame from the sidecar takes ~15-50 ms, with cold NAS outliers up to ~0.1-0.2 s (each picture reads up to a
second of the sidecar in small requests; a bigger read buffer should cut those), and as the mouse moves along the bar
the next picture comes from the decoder already there in ~2 ms. A keyframe of the clip itself (no sidecar):
~15 ms local, ~20-35 ms from the NAS cold (table above).


Conclusions:

- A picture under the cursor in **~15–50 ms**, even from the NAS cold, if the helper stays running and keeps the
  file and its decoder open. A process per picture (160–180 ms) is too slow: that's the main design constraint.
  That's fast enough to make every picture on demand: nothing needs building ahead.
- LRF/LRV vs the clip's keyframes: about the same speed on this Mac (VideoToolbox decodes a 4K keyframe in ~15 ms),
  but the sidecar reads **4–9× less** from the NAS, decodes on the CPU (no contention with the player's own GPU
  decode while it plays) and, for DJI log clips, already looks normal. Prefer the sidecar; keyframes otherwise.

**The helper.** A small long-lived program, `afp-scrub`, on the libav libraries mpv already ships (macOS, Windows,
Linux alike), doing what `thumbbench.c` does: keep the file and a decoder open, answer `<file> <time> <exact|key>`
requests — seek to the keyframe before it (or not, when the time is just ahead of the frame the decoder is at),
decode on to the frame (sidecar, CPU, frame threads) or take the keyframe (the clip, VideoToolbox / D3D11VA /
VA-API) — scale it and write BGRA into a picture file. The newest request wins: one under way is dropped when the
mouse has moved on. The player's Lua shows the picture with mpv's `overlay-add` (raw BGRA from a file: no decoding in
mpv), placed above the bar beside its time bubble. thumbfast does the same with a second headless mpv; ours avoids a
second player's memory and start-up and does the exact sidecar frame. (macOS-only alternative, `afp-thumbs` +
`AVAssetImageGenerator`: not for Windows/Linux, so not preferred.)

**When it reads.** Only while the mouse is over the bar: the helper opens the clip's sidecar (or the clip) on the
first hover and keeps it open while the clip plays; nothing is read ahead or in the background, so playback on the
NAS has the share to itself (the lesson of #272).

**Keeping what's made.** In memory, the pictures made for this clip (460×260 BGRA ≈ 480 KB each; a few hundred kept,
the oldest dropped), so going back over the same stretch is instant. On disk, only on this computer, optionally: the
same pictures as small JPEGs in `~/Library/Caches/ActionFootage/scrub/<file identity>/` (size + mtime + path, like the
Library's identity), so a clip opened again scrubs from its cache at once. Never beside the clips. Disk cache can
come second: it's not needed for speed.

**Drag.** The picture follows the mouse; the video doesn't seek while dragging (today's keyframe seek on each move is
what makes dragging lag on the NAS); one exact seek on release.

**Sidecar rules.** One rule shared with the Library (`index.rs:359`, `camera_thumb()`), extended with HERO3/4's
`GOPR<n>.LRV` / `GP<ch><n>.LRV`; a sidecar is used only when its duration matches the clip's (±0.1 s). A moment
link plays a stretch of its original: the original's sidecar, at the original's time.


## 3. Order of steps

1. **A1, shown time** everywhere (bar, marks, A–B, cut, highlights, telemetry, 360 recording, uwcolor, export).
   Small; fixes the bar-vs-picture offset. Test: `dnrepro.py` extended to read `shown-pos`; Lua unit tests.
2. **A2, `dn_show` in the shader + one step binding** for `,` `.` Option+arrows, the bar's buttons, the wheel.
   Test offscreen (the libplacebo rig of `denoise-noise-gate`/export `AFP_WARP_TEMPLATE`) plus a short live check.
3. **A3, refill when paused** (raw frame first, then seek − 5, `frame-step 10`, `dn_show` held). Live check of the
   timing and that the picture doesn't move.
4. **`afp-scrub` helper** + build scripts (macOS/Windows/Linux), sidecar rules shared with the Library.
5. **Picture on hover and drag** in `bar.lua` (`overlay-add`): sidecar exact frame, else the keyframe before;
   click/release = exact seek to the cursor's time; no seeks while dragging. In-memory keep.
6. Journal timings (hover → picture on screen); disk keep only if the numbers ask for it.
7. UI PR with before/after screenshots.

Steps 1–3 and 4–6 are independent and can go in parallel.

## 4. Open points

- The refill's real cost live (each of the 10 frames is a full warp + denoise render, ~10 ms with Full): measured
  only headless (`frame-step 10` = ~180 ms at 60 fps with `--vo=null`). Whether `speed` > 1 with `framedrop=no`
  makes it faster is for the live check.
- The cold-NAS outliers of the sidecar's exact frame (p90 ~0.1 s): try a larger read buffer in the helper.
- Insta360 and other 360 clips: postponed (Anton).
