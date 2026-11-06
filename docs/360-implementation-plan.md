# 360° cameras: implementation plan

How [the architecture](360-cameras.md) gets built: in which steps, which can run side by side, what each one
touches, and how we know it's done. Written 2026-11-05. Sizes are rough working days.

## Before anything

- **Clips.** There's no 360 clip on this Mac or on the NAS (searched `.insv`, `.360`, `.OSV`). Steps 0–3 below
  don't need one: they run on synthetic files. Step 4 does. Needed: a few short clips from the camera that
  matters first, straight off the card (not exported), one walking or riding with GPS if the camera can have
  it, one still on a tripod, and one stitched export of the same clip from the maker's app.
- **The first camera.** Whichever the clips are from. Insta360 X5 is the best documented in telemetry-parser
  (its lens model is already in our gyroflow-core fork); GoPro MAX 2 is the easiest picture (EAC, no lens model)
  and the only one with north built in.
- **Upstream.** Write in Gyroflow's issue #800 before step 3: the equirect output projection and the lens offset
  in the kernel are changes upstream may want, or may already have in its private 360 renderer. Anton's words,
  posted by Anton or with his OK.

## The steps

```
        ┌─ 0 view shader, stitched clips ─────────────┐
        ├─ 1 the stamp ───────────────────────────────┤
start ──┼─ 2 throughput spike ────────────────────────┼─→ 4 raw clips, one camera ─┬─ 5 headings: GPS, north
        └─ 3 equirect projection in the fork ─────────┘        (needs clips)       ├─ 6 below 20 ms
                                                                                    ├─ 7 exports
                                                                                    └─ 8 Gyroflow's part
```

0–3 are independent of each other and can be separate sessions, each in its own worktree. 5–8 are independent
of each other once 4 is in.

### 0. The view shader, on stitched clips (2–3 days)

The responsive part first, on the files that need no plugin.

- `mpv/scripts/view360/view360.glsl`: equirect sphere → view. `DYNAMIC` params for yaw, pitch, roll, FOV,
  projection (rectilinear, stereographic, little planet), heading mode. Bicubic, and a wider filter when the view
  shrinks the sphere. Frames without a stamp: the camera level and fixed. mpv360 (MIT) as a reference or a
  source, credited.
- `mpv/scripts/view360/main.lua`: on for a 360 clip; the gestures, wheel and keys turn the head. The gesture
  handling now in `gyroflow/crop.lua` moves to a module both use, so crop mode and the 360 view turn the same way.
- Telling a stitched clip: spherical metadata (`sv3d`/`st3d`, or Spherical Video V1's XMP), read by
  `gyroflow-probe` (it already reads the MP4 per clip) and passed to the scripts with the rest of what it finds.
- `hwdec` zero-copy for these clips: no VapourSynth in the chain.
- The side panel: a 360 section (heading mode, FOV, projection, back to the start).

**Done when** a stitched X5 / MAX / Osmo export plays with the head turned by a drag, a trackpad and the wheel,
paused and playing, and an ordinary clip is untouched. Live test on screen (needs Anton's OK, see below).

### 1. The stamp (1–2 days)

- `vs-gyroflow/src/stamp.rs`: the layout of [the architecture](360-cameras.md#the-stamp) (magic, version,
  frame time, `q_world_from_cam`, per-mode heading and confidence), written into the luma plane for 8- and 16-bit
  samples, as `write_crop_mark` does; and its reader, for tests. Rust tests: write → read, every field, both depths.
- In `view360.glsl`: a `LUMA` hook that reads it with texel fetches and saves it (`//!SAVE`) for the view pass.
- **The spike this step is for**: a synthetic clip with known stamps (a test filter in the plugin that only
  stamps), played in gpu-next with software decoding, with `hwdec=videotoolbox-copy`, 10-bit full and limited
  range. The shader paints what it decoded; compared with what was written. If the plane isn't handed over
  unchanged, the codes move to fewer, coarser levels (as the crop mark's 17), or the layout changes here, before
  anything depends on it.

**Done when** every field reads back exactly in every case above.

### 2. Throughput spike (1 day)

- A synthetic file with two 3840² HEVC 10-bit tracks (ffmpeg, `hevc_videotoolbox`, at 30 and 60 fps), and one
  with two 2880² (5.7K).
- mpv with `--lavfi-complex` `hstack` → VapourSynth with a plugin that only copies, out at an equirect size
  (8192×4096). With and without the copy back of an output that size.
- On the perf harness (#43, `tools/perf` on `perf-harness`): fps, dropped frames, CPU, memory traffic. Also: both
  tracks decoded in step after a seek; `lavfi-complex` set per file from the script, and what it does to the
  rest of the chain.

**Done when** there are numbers for 5.7K30, 8K30 and 8K60 on Anton's Mac, and a decision: VapourSynth as it is
for step 4, or the tracks joined on the GPU (`hstack_vt`, our own ffmpeg) first.

### 3. The equirect output projection in the fork (2–4 days)

In `anton-belkin/gyroflow` (the fork the plugin builds from), on a branch of its own:

- `OutputProjection::Equirect = 5` in `stabilization/projection.rs`: output point → longitude, latitude →
  direction → ray. The fork's rays are angle vectors, so directions behind the camera (θ up to 180°) are
  representable already. The same in `wgpu_undistort.wgsl` (the path we use) and the other backends' copies, or
  refused there.
- What the output's FOV and size mean for it: the whole width is 360°, the height 180°.
- **The lens offset.** The kernel turns rays; a rig's lenses are also apart (31.9 mm on the X5). At a stitching
  distance `R`, the ray from the rig's center meets the sphere at `R·d`, and the lens sees it along `R·d − t`. One
  more input to the kernel (`t`, `R`), zero for every flat clip.
- Rays a lens has no picture for (past its validity limit) are invalid, not clamped.
- Tests: an equirect render of a flat clip against gyroflow-core's CPU path, as `sphere.rs` tests do; the offset
  against a hand-computed parallax.

**Done when** the tests pass and a flat clip renders into an equirect where its picture lands in the right place.

**Done** 2026-11-06: fork branch `equirect-projection` (fb35b07, on top of f42a7dd), local in
`~/VSCode workspaces/gyroflow-equirect`, not pushed yet. What step 4 gets from it:

- **The sphere.** `ComputeParams::output_projection = OutputProjection::Equirect as i32` (5). The output's width is
  360° and its height 180° at `fov_scale` 1, whatever the lens and the output size (2:1 for square pixels; the
  sphere's scale is `projection::equirect_focal`, not the lens's focal length). `fov_scale` < 1 shows a part of the
  sphere around its middle; leave it 1 with `fovs` empty (no adaptive zoom). `lens_correction_amount` is ignored:
  an equirect is always fully corrected. Axes as `video-types`: longitude 0 at +z, +90° at +x, latitude +90° up.
- **Pixel centers.** wgpu (the player's backend) draws pixel `(i, j)` at its middle: longitude
  `((i + 0.5) / W − 0.5) · 360°`, latitude up `(0.5 − (j + 0.5) / H) · 180°`, the usual convention. The CPU kernel
  draws pixel `i` at `i`, half a pixel off; that's gyroflow-core's, for every projection.
- **A lens of a rig.** `ComputeParams::rig_lens = RigLens { rotation, translation, stitching_distance, validity }`:
  - `rotation`: the lens in the rig, turning a direction in the lens's axes into the rig's (OpenCV axes, Hamilton);
  - `translation`: the lens's entrance pupil from the rig's center, metres, rig axes;
  - `stitching_distance`: R in metres, 0 = infinity (no parallax); the lens sees the point `R·d` along `R·d − t`;
  - `validity`: radians from the lens's axis, past which rays are background (never clamped); 0 = the lens model's
    own field limit.
  The rig's axes are those of the camera the motion data describes (after `imu_orientation`). `RigLens::default()`
  changes nothing: a flat clip renders bit for bit as before.
- **The camera at mid-frame.** For the sphere in the camera's own axes: smoothing method 0 (none), so the output
  camera is the camera at the middle of the frame and only the rolling shutter is taken out (the readout from the
  rig). `view_rotation` stays 0.
- **One `Stabilization` per lens**, each with its own `ComputeParams` (lens profile, `rig_lens`), the same output
  size, rendered into its own buffer and blended afterwards. `background_mode` 0: the edge modes would smear the
  lens's border over the sphere. The lens's half of the joined frame: the input's `BufferDescription::rect` (the
  kernel's source rect), outside which it reads background (to check on wgpu with two lenses side by side).
- **Under water**: `light_refraction_coefficient` works with the equirect; past Snell's window is background.
- **The point path** (`undistort_points_with_rolling_shutter`) knows both: a source pixel to its place in the
  sphere, with the lens's turn and offset.
- **Not checked**: `framebuffer_inverted` with a `RigLens` (the turn and offset are flipped in y, untested); the
  OpenCL kernel wasn't run (same math, read by eye); the precompiled Qt `.qsb` and rust-gpu `.spv` weren't rebuilt,
  so Gyroflow's own preview draws an equirect project as rectilinear until they are (the GLSL source passes
  `glslangValidator`).
- **Tests**: `src/core/tests/equirect.rs` in the fork (`cargo test --test equirect`, a minute in debug). The ignored
  `a_clip_in_the_sphere` renders a real clip's frame, see its doc comment.
  `output_projection::a_project_from_before_the_setting_…` fails at f42a7dd already, unrelated.

### 4. Raw clips, one camera (5–8 days)

Needs 1, 3 and clips; 2 says how the frames get in.

- `vs-gyroflow/src/rig.rs`: telemetry-parser's `CameraRig` → per lens, gyroflow-core parameters: the lens model
  (Mei → the fork's Insta360 model; DJI's 5-coefficient fisheye and GoPro's EAC checked against what gyroflow-core
  has, a model added to the fork if not), the pose, the offset, the crop of its half of the joined frame, the
  readout.
- The render: each lens into the sphere; **no smoothing**, the target camera being the camera at the middle of the
  frame, so only the rolling shutter is taken out. Blended by each lens's feather and its body mask. Both depend
  only on the direction in the camera's axes, so their weight maps are made once per clip, not per frame.
- GoPro EAC: the faces moved onto the sphere, the doubled overlap blended; no kernel.
- The stamp per frame: `q_world_from_cam` from gyroflow-core's integration (gravity from the accelerometer), and the
  headings in the world and along the camera (`heading.rs`: the camera's yaw smoothed over 2–5 s, with the
  look-ahead as `smoothing.rs` has).
- `gyroflow/main.lua`: a 360 clip gets `lavfi-complex` and the plugin's 360 mode; `view360` turned on with the stamp.
- Defaults from the file: the maker's stitching distance, the calibration variant the clip says.

**Done when** a raw clip from the first camera plays: the horizon level while the camera tilts, the seam
acceptable at the default distance, the head turning freely, the two modes switching instantly, paused too; at
the fps step 2 promised. Side by side with the maker's own stitch of the same clip.

**Built** 2026-11-06 (branch `claude/360-raw-x5-f4131c`), on the Insta360 X5 sample clip (2 × 3840² HEVC 8-bit, 25 fps,
gyro and accelerometer, no GPS). Partly done: see what's open below.

- **The fork** (`anton-belkin/gyroflow`, branch `360-raw-clips`, d130105, on step 3's `equirect-projection`): the
  Insta360 model takes RadtanPro's 13 terms (the X4's and X5's `offset_v6`) on top of v3's, in every backend; a v3
  profile renders bit for bit as before. `Cargo.toml` points at it.
- **`vs-gyroflow/src/rig.rs`**: `CameraRig` → per lens its lens profile (camera matrix in its picture's pixels, a
  mirror as a negative fx), `RigLens` pose and offset (about the lenses' middle: Insta360's origin is lens 0's pupil),
  its rect in the joined tracks, validity, blend, image circle and masks (drawn once into a grid). Only Mei
  (BrownV3, RadtanPro) yet: DJI's ThetaPoly and GoPro's EAC are refused with a reason (`gyroflow-probe` says it).
- **`vs-gyroflow/src/sphere360.rs`**, `core.gyroflow.Sphere360`: one `Stabilization` per lens and plane (equirect,
  no smoothing, the lens's half of the joined frame uploaded alone), the first lens into the output, the others
  blended in by spans made once per clip, the stamp per frame. 5760×2880 and bilinear by default: the kernel's time
  is its taps where a lens has picture (~120 ms a frame a GPU worker with bicubic, ~70 bilinear). Rendering only a
  lens's band of longitudes doesn't help: near the poles every longitude is within a lens's 93°.
- **`vs-gyroflow/src/heading.rs`**: the world (gravity down), the camera's yaw, in the world and along the camera.
- **The motion in the rig's axes.** The X5's IMU isn't in the rig's axes, and its two sensors not in each other's:
  the gyro needs half a turn about the vertical, the accelerometer x → −y, y → −x, z → −z (`sphere360.rs`
  `imu_turn`). Found with `tools/sphere360/horizon.py` from the horizon in the sphere (the rig's own, no IMU in it),
  not derived: to check on other X4/X5 clips. The gyro is integrated alone and the vertical comes from the
  accelerometer averaged over 20 s: in a curve the accelerometer leans with the motorcycle (the camera rolled 50°,
  the accelerometer barely moved), and gyroflow-core's VQF followed it, 30–60° off.
- **The player**: `gyroflow/main.lua` joins the tracks (`lavfi-complex`), `videotoolbox-copy`, no `scale_vt`,
  `sphere360.vpy` (VapourSynth's cache at 1.5 GB), every other frame above 31 fps; `view360.glsl` reads the stamp in
  a `LUMA` pass; `view360/main.lua` follows the camera by default, Alt+h for in the world.

What was checked:

- Rust: 89 tests, the rig, the model against video-types' formula, the headings, the blend; Lua: all of `tests/`.
- **The horizon**: the plugin's stamps against the horizon measured in the sphere every second of the clip: 9.9°
  rms, 18° at worst (the measurement itself is good to ~5°), through braking and a curve with the camera rolling
  50°. On screen (the app's mpv, gpu-next, Anton's OK), paused at 8 s, in both modes and at yaw 0/90/180/−90 and a
  300° FOV: level within a few degrees. Again with the final IMU turns, paused at 8, 11, 12 and 13 s (the curve), the
  head at 0/90/180/−90°: level at 8 s and sideways throughout; looking ahead along the road in the curve (11–13 s),
  tilted ~10–15° with the bike leaning, as the measurement above says (it was 30–60° before).
- **The head and the modes** on screen: the drag/IPC turns, both modes switch at once playing and paused, the stamp
  reads back (`stamped`), the little planet.
- **The seam** at the default 2 m: not visible in the rendered spheres at the clip's scale; not judged closely on
  screen. There is no maker's stitch of this clip to put beside it: not done.
- **Throughput**: without the screen 24–25 fps through the plugin (`--vo=null --untimed`); the same chain with a
  copy in VapourSynth instead of the plugin, 26 fps: mpv's decoding, joining and conversion of two 3840² tracks is
  the limit on this clip, not the plugin: mpv decodes the two tracks one after the other in its playback thread (one
  track alone: ~70 fps; both: ~31). `vd-queue-enable=yes` gives each track's decoder a thread of its own: both
  tracks then ~65 fps, through the plugin 30 fps at 7680 and 53 at 5760 without the screen. **On the screen** (the
  app's mpv, gpu-next, the view on, 15 s of the clip from 1 s): 7680 dropped 143 frames, 6912 17, 6400 and 5760
  none; 5760 with the head turning all the while (IPC, 20 times a second), none. So 5760 by default
  (`sphere_width` raises it), the queue on in 360 mode. Anton's own runs of a test copy at 5760 (2026-11-06): the
  first with the machine busy (load ~12 on 10 cores) dropped 238 frames over the clip; the plugin gave 3.4 fps for
  the first 100 frames (start-up), then 26. The second (with step 6's patches) dropped 33 frames in the first 2 s
  and ~20/s from 7 to 16 s while Time Machine and a VM ran, then none; in his words smooth after a stumble at the
  start, the head and the video both moving smoothly. The drops came from background load and the start, not the
  view shader, so its filter stays as it is for full screen. Open: the start (the motion load and the first
  frames) could wait paused until the sphere is ready.

Open: travel and north (step 5); X4, X3 (two files), DJI
Osmo 360 (ThetaPoly, quaternions only, `imu_from_rig`), GoPro MAX 2 (EAC); the mirror flag (no clip has it); 8K60.

### 5. Headings: along the way, north (3–6 days)

- **Along the way you move**: in `heading.rs`, the gyro's yaw aligned to GPS (one angle and its slow drift fitted
  so that the accelerometer's horizontal acceleration, turned into the world, matches the change of the GPS
  velocity), a confidence from the fit; the course over ground when moving, along the camera below a speed
  threshold. Tests on synthetic trajectories, then on a clip with GPS.
- **North on the MAX 2**: `MNOR` and `OCNF` in telemetry-parser (a pull request upstream, Anton's OK), then into the
  stamp. **On the MAX**: `MAGN` with a hard/soft-iron fit over the clip when the camera turned enough to fit it;
  otherwise a setting for Gyroflow's part.
- The panel and a key for the mode; a mode with no data greyed out, saying why.

**Built for Insta360** 2026-11-06 (branch `claude/360-insta-heading-export-f71dc0`, 8ce554e), from the GoPro MAX 2
session's camera-independent part (`claude/360-max2-headings-0d98b9`; its EAC, MAX 2 IMU turns and GPMF reading not
taken):

- **`heading.rs`**: `align` fits the world's azimuth θ, its drift and the GPS's lag (complex Procrustes of the
  accelerometer's horizontal acceleration, turned into the world, against the GPS track's second difference, both
  less their slow part); its confidence from σ and ρ; `travel`, the course over ground in the world, smoothed, and
  standing (or across a GPS gap) its offset from the camera's heading from before held; true north from the fit where
  there's no compass; the world mode held against the gyro's drift by it. For Insta360: the lag searched over ±3 s
  (`LAG_SEARCH_S`, was ±1), fixes interpolated across gaps up to 2.5 s (1 Hz GPS), and across a gap the accelerometer
  masked as GPS is before both lose their slow part (else a 20 s tunnel threw the drift 0.04°/s off, 5° at the ends).
- **`nav.rs`**: telemetry-parser's `GPS/Data` (`GpsData`, unix time with ms, acquired or void) onto the video's clock
  by `first_gps_timestamp`, which on the X5 is unix ms at the video's start (the sample: 13:36:27.993 UTC, its
  `creation_time` "20250416143628" being BST), not a camera-clock time as its name suggests. A numeric unix
  `creation_time` would do otherwise; a clock more than a minute after the first fix or a day before it is none.
- **`sphere360::Motion`** (shared by the plugin and `gyroflow-probe --headings`): the report per mode, `available`,
  `confidence`, `why`, the fit's σ, ρ, lag and drift, and per second how sure travel and north are.
- **The player**: `view360` has the four modes (Alt+h goes through the ones the clip has, along the way first, and the
  default when present); the panel greys out the others with the reason. North on an Insta360 says "an Insta360 has
  no compass" (and is the GPS fit's true north when there's a track).

Checked: synthetic drives (tests in `heading.rs`): turns, a stop held, a 20 s GPS tunnel and short losses (θ within
0.5°, along the way within 4° away from the gaps and in them), 1 Hz fixes with the GPS clock 2 s off (lag found, within
5°), a straight road and a backwards gyro and standing still refused; the clock placement (`nav.rs`). On the X5 sample
(no GPS record at all): `--headings` says travel "no GPS in this clip (record with a GPS remote or the phone app's
GPS)", north "an Insta360 has no compass, and no GPS track to find north by". Lua: `tests/view360_test.lua`.

Open: **no Insta360 clip with GPS** was at hand: the GPS clock's placement (`first_gps_timestamp`) and the fit on real
Insta360 data are unchecked. No public raw `.insv` with GPS was found (2026-11-06: two forum samples with GPS are
dead links, Mapillary keeps others private, ExifTool's forum was down); a short clip with the GPS remote or the
phone app would settle it. Nothing on screen yet (`--headings` itself: 0.5 s on the X5 clip).

### 6. Below 20 ms (3–5 days)

mpv patches in `tools/mpv-patches/`, macOS:

- Measure first: a patch that logs, per drawn frame, the newest gesture's event time and the drawable's
  `presentedTime`. Then a 240 fps camera on the trackpad and the screen (Anton).
- Redraw at every refresh while the view moves, playing or paused (`video-sync=display-resample` tried first).
- Gestures straight to the view's params in the vo (patch 0008 extended), the script keeping the frame, saving
  and the OSD.
- A short swapchain, direct scanout in full screen; the sample-buffer layer's (patch 0002) latency measured
  separately.

**Done when** measured: ≤ ~25 ms at 120 Hz and ≤ ~45 ms at 60 Hz, finger to light, or the reason it isn't.
The same route can serve crop mode on flat clips afterwards (a stamped camera per frame, corrected in the shader
within the margin); that's a separate change.

**Measured** 2026-11-06 (branch `claude/360-latency-957079`), M1 Pro: the built-in XDR (ProMotion, 120 Hz) and a
DELL P2217H (60 Hz, the main display); a synthetic stitched clip (2880×1440, 30 fps, `tests/make_equirect.py`), the
window 1280×720, made-up two-finger scrolls 97 times a second (`--macos-latency-test`). "Input → light" is from the
gesture's event time to the drawable's `presentedTime`, p50 / p90 in ms, from the first frame that shows the input
rendered fresh. Not in it: the trackpad's own scan and the HID path before the event's time, and the scan-out down
the screen after `presentedTime` (half a refresh to the middle) — the camera's part, below.

| Input → light, ms | XDR 120 Hz paused | XDR playing | DELL 60 Hz paused | DELL playing |
|---|---|---|---|---|
| Before (the app's mpv as it was, step 0's path) | 29.1 / 33.3 | 189 / 493 | 31.7 / 38.6 | 2627 / 4668 |
| `video-sync=display-resample` | — | 43.8 / 58.6 | — | 426 / 893 |
| + 0011 (frames rendered again) and 0012 (gestures in the VO), `video-sync=audio` | 29.1 / 33.5 | 38.3 / 62.5 | 32.9 / 41.7 | 45.5 / 60.7 |
| + `video-timing-offset` a refresh ahead | — | 32.8 / 39.2 | — | 46.2 / 50.1 |
| + no vsync wait in a window (0013): **what view360 sets now** | **8.3 / 10.3** | **8.9–10.0 / 15.5–17.5** | 13.7–42.5 | 31.5–46.0 |
| the same, full screen (vsync kept there) | 30–32, twice 15 | 34 | 35 | 59 |
| full screen without the vsync wait (may tear) | 8.7–9.7 | 17.2 | 9.2 | 15.8 |

What each change did, and why:

- **The log first** (mpv patch 0010, `--macos-latency-log`): per frame the input it shows (view360 passes the
  gesture's event time in `glsl-shader-opts` as `afp_input_time`), when the VO began it, took the Metal drawable
  (`CAMetalLayer.nextDrawable` overridden, with `addPresentedHandler`), submitted it, and `presentedTime`.
  `tools/latency360/run.sh` runs one measurement, `summarize.py` splits it into stages.
- **Before**: paused, the script's path was nearly free (input → VO 0.5 ms); ~27 ms of the 29 went after the submit.
  Playing, the VO drew only the video's frames, and setting `glsl-shader-opts` waits for the VO (up to a refresh),
  so the script fell behind gestures coming faster than the screen's rate: by seconds at 60 Hz.
- **`display-resample`** drew 115 frames/s at 120 Hz, but **the view still moved 30 times a second**: libplacebo
  keeps a frame in its cache by a hash of the render params that leaves out DYNAMIC parameters' values, so a frame
  shown for several vsyncs came from the cache with the old view (the log counts it: 512 of 832 frames with a new
  input came from the cache). Patch **0011** makes the values part of the frames' signatures. And
  `display-resample` keeps the swapchain full: 65 ms after the submit at 60 Hz. Not used.
- **Patch 0012**: gesture messages are numbered; view360 says which one its values include (`afp_input_seq`) and
  applies the messages waiting together once; the VO turns the view by the later ones itself just before it draws
  (the same geometry near the picture's middle, a pixel or two off for a frame until the script's exact values
  come), and is woken (`vo_redraw`) as each comes in, playing or paused. The backlog is gone.
- **`video-timing-offset`**: mpv hands the VO a video frame 50 ms before its time, and the VO draws it at once and
  waits, with no redraws meanwhile and the head as it was 50 ms earlier. view360 sets a refresh + 4 ms: 89 frames
  shown a second instead of 44 while playing at 120 Hz.
- **The vsync wait** is most of what's left: with the Metal layer waiting for the vsync (`displaySyncEnabled`, FIFO)
  a frame reaches the glass 2–3 refreshes after its submit whatever else is done (`swapchain-depth` 1 or 2 and
  `macos-render-timer=system` changed nothing, at 60 Hz they made it worse; mailbox worse). Without it, in a window,
  macOS composites the newest frame at its next refresh: 6 ms after the submit at 120 Hz. Patch **0013**
  (`--macos-display-sync=no|fullscreen`, at runtime); view360 sets `fullscreen` while the view is on: no wait in a
  window, the wait kept in full screen, where macOS may show the layer directly and it could tear (not seen yet).
- **Full screen** with the wait was 15 ms in two runs and 30–32 in five others, the same build: whether macOS gives
  the layer direct-to-display isn't ours to choose.
- **The DELL at 60 Hz** without the wait is bimodal from one run to the next: 10 or 40 ms after the submit, as if the
  compositor's deadline were missed or met for the whole run. Its p50 stays ≤ 46 ms, the target's edge.
- **The sample buffer layer** (`--macos-sample-buffer-layer`, HDR and Dolby Vision): it says nothing of when it
  shows a frame. Input → enqueued: 7.0 / 9.9 ms paused, 9.4 / 14.7 playing (XDR); its light is the camera's to say.

So: **at 120 Hz in a window, ~9 ms to `presentedTime`, under the 25 ms target** with room for the trackpad and the
scan-out; in full screen 30 ms unless macOS shows the layer directly (or the wait is dropped there too, if it doesn't
tear). **At 60 Hz, 14–46 ms**, at the 45 ms target. To check with the camera: that `presentedTime` without the vsync
wait is the light (it could be the compositor's latch), the trackpad's own part, tearing in full screen.

Mouse drags still go through the script (no number on `mouse-pos`): only the trackpad's gestures (scroll, pinch,
rotation, a smooth wheel) are in the VO. The little planet's pan too. Not measured: 8K clips through the plugin (the
VO's frame is heavier, `video-timing-offset` may need more), HDR swapchains, external 120 Hz screens.

**The camera** (Anton, a 240 fps phone), per variant a run of 30 s with the fingers (no `--hz`):

```
tools/latency360/run.sh /tmp/lat-win.log <clip> --seconds=30 "--screen=Built-in Retina Display"
tools/latency360/run.sh /tmp/lat-win-vsync.log <clip> --seconds=30 "--screen=Built-in Retina Display" --sync=auto
tools/latency360/run.sh /tmp/lat-fs-novsync.log <clip> --seconds=30 "--screen=Built-in Retina Display" --fs --sync=no
tools/latency360/run.sh /tmp/lat-sb.log <clip> --seconds=30 "--screen=Built-in Retina Display" -- --macos-sample-buffer-layer=yes
```

The trackpad and the screen in one shot, brightness up; ten short two-finger flicks, each from fingers held still for
half a second. Per flick, the frames from the fingers starting to move to the grid on screen starting to move, ÷ 240
(±4 ms). Against the run's own input → light p50: the difference is the trackpad, HID and scan-out. In full screen
with `--sync=no`, a turning grid is where tearing shows (a line across the picture, the halves offset).

### 7. Exports (3–5 days)

- `gyroflow-export --360`: the stabilized sphere (turned into the world and the chosen heading) with spherical
  metadata, for YouTube and editors.
- A flat reframe: the view along keyframes of the head, as crop mode's photos are made now.
- The view math in WGSL beside the GLSL; a test renders both from the same sphere and stamp and compares.

**Built for Insta360** 2026-11-06 (branch `claude/360-insta-heading-export-f71dc0`: 40c9055, 8b69147):

- **`vs-gyroflow/src/view360.wgsl`**: `view360.glsl`'s view pass function for function, plus projection 4, the
  equirect (the whole sphere turned into the world and the heading). `view360.rs` draws a sphere plane through it
  (R8 or R16 unorm in, R32 float out, its own wgpu device).
- **The comparison test** (`view360::tests`) takes the player's GLSL from `mpv/scripts/view360/view360.glsl` as it is
  in the file, both passes (the stamp read from the luma, then the view), declares libplacebo's names around them and
  compiles them with naga's GLSL front end (wgpu's `glsl` feature, a dev-dependency only), on the same device. Same
  sphere and stamp, seven views (all four projections, FOV 20-250°, every heading mode): at most 0.42/255 apart, no
  pixel over half a code; another mode's heading or a degree of yaw shows on 5 % / 1 % of the pixels or more. A change
  to the GLSL's view math without the WGSL fails it.
- **`export360.rs`** and **`gyroflow-export --360` / `--reframe "t:yaw,pitch,roll,fov;..."`** (`--heading`,
  `--sphere-width`, `--size`, `--view`): ffmpeg decodes the lens tracks joined (hstack), each frame goes through the
  plugin's 360 mode (`Sphere360`, as in the player), its stamp is read back from the frame, then the WGSL view; HEVC
  10-bit at the source's bits per pixel, audio copied. The heading as the player picks it (along the way when there,
  else along the camera); asked for and missing, along the camera with the reason.
- **`mp4meta::write_spherical`**: Spherical Video V2 (`st3d`, `sv3d`/`svhd`/`proj`/`prhd`/`equi`) at the end of the
  video sample entry and V1's GSpherical XMP `uuid` in the track (ffmpeg can't write either from the command line).

Checked on the X5 sample (release build, M-series Mac): the whole clip (24.4 s, 611 frames) as a 360 video 5760×2880
in 62 s (~10 fps), as a 1920×1080 reframe in 45 s; audio kept. ffprobe reads `spherical_mapping: equirectangular`
(also in a test, faststart or not), exiftool both V2 (`Stereoscopic 3D: Monoscopic`, the projection bounds) and V1
(`XMP-GSpherical`), and the player's own `gyroflow-probe` takes the export for a stitched 360 clip (`sv3d`, mono,
equirect), so the player would open it in the view. Frames looked at: the horizon level across the sphere on the
straight; in the curve (12 s) ~12° off, the vertical's error step 4 measured, not the export's.

Open: no player was run on the exports: VLC isn't installed here (a download), QuickTime has no 360 view, and the
player on screen needs Anton's OK; YouTube not tried (an upload). No log LUT in the 360 export (I-Log clips would
come out flat). Head keyframes only from the command line: nothing in the player records them yet (a director's cut,
step 8's part). Stereo and 10-bit sources go the same way but weren't tried (no clip).

### 8. Gyroflow's part (1–2 days here, the rest with upstream)

- Our settings in `"afp_360": { ... }` in the `.gyroflow`: heading mode, head, stitching distance, variant.
  First: check that Gyroflow keeps unknown keys when it saves a project.
- The stitching distance and the variant as settings the plugin reads.
- What Gyroflow itself should get (the distance by keyframes, the compass's calibration, looking by keyframes,
  the heading from optical flow) goes to upstream's 2.0, as issues or pull requests, after #800's answer.

## Other systems

- **Windows and Linux** get 0, 1, 3, 4, 5, 7 with the mpv they have: the shader, VapourSynth and `lavfi-complex`
  are stock. Step 6's patches are macOS only; on Windows they would need our own mpv build (it's shinchiro's,
  pinned, today).
- `hwdec` zero-copy for stitched clips works with whatever mpv's GPU decoding is there.

## Testing

- **Rust**: the stamp, the projection, the offset, the rig → parameters, the headings, the WGSL view: plain
  `cargo test`, no screen.
- **Lua**: detection and mode logic, in `tests/` as the workspace tests are.
- **On screen** (the shader, the gestures, the latency, fps): the app running with the display awake (gpu-next
  hangs while the Mac is locked). These take the screen, so each round needs Anton's OK first.
- **Throughput**: the perf harness, local copies of the clips, nothing else reading the disk.

## Risks, and when to stop and think

- The stamp doesn't survive gpu-next unchanged (step 1): change the encoding; if nothing survives, the angles go
  through an mpv patch (the frame time as a shader param) instead.
- 8K through VapourSynth doesn't keep up (step 2): `hstack_vt` and our own ffmpeg before step 4, or 8K played at
  half the frame rate meanwhile.
- The kernel can't take the lens offset cleanly (step 3): stitch at infinity first (offset zero), the distance
  later.
- Upstream announces its renderer: compare before step 4 and use theirs if it fits.

## Permissions to ask for up front

So that the steps run without stopping:

- **Clips**: Anton puts them somewhere this Mac reads (a local folder, or a path on the NAS that `ssh nas` can
  reach). Downloading makers' sample clips instead needs an OK per download.
- **The screen**: live tests of the shader, gestures and latency (steps 0, 4, 6). A standing OK per step, or per
  round, and the display kept awake meanwhile.
- **Pushing**: branches and pull requests to ActionFootagePlayer, and pushes to the `anton-belkin/gyroflow` fork
  (step 3). Upstream (a telemetry-parser pull request, the #800 comment) is outward-facing: Anton's OK each time,
  in his words for the comment.
- **Builds**: `tools/build_mpv.sh` with the new patches (step 6), cargo with network access, perhaps our own
  ffmpeg build (after step 2).
- **The perf harness**: runs that take the GPU for minutes, with no syncing to the NAS meanwhile.
