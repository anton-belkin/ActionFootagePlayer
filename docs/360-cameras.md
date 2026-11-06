# 360° cameras: architecture

How the player plays a 360° camera's clip so that you can look around in it while it plays, with the view held
steady by the camera's motion data and pointed where you choose: fixed in the world, along the camera, along the
way you're moving, or north. This is a design, not built yet.

Written 2026-11-05, decided with Anton in conversation that day. The research behind it (what each camera
records, what Gyroflow and ffmpeg have) is in the local report `reports/360-камеры.md`; the facts this design
leans on are repeated here. Nothing below has been tried on a 360 clip yet: what has to be checked first is
marked **(to try)**.

## In short

- **Two speeds.** What depends on the optics is made at the video's rate, in the plugin, where a few frames of
  delay don't matter. What depends on the viewer is applied at the screen's rate, in a shader, as late as possible.
  Turning your head is responsive because it never goes through the stabilization's pipeline.
- **The plugin outputs a sphere as close to the source as it can be**: both lenses mapped to one equirectangular
  sphere in the **camera's own axes**, stitched, with the rolling shutter taken out (to the camera at the middle of
  the frame). Nothing about the view is baked in: not the stabilization, not the horizon, not the heading.
- **The angles travel inside the frame.** A few rows under the sphere carry the camera's orientation in the world
  at the middle of the frame, and the heading of every heading mode, already smoothed. The shader reads them from
  the frame it draws, so they're exact through dropped frames, seeks and pauses, with no mpv patch.
- **The view shader composes it per screen refresh**: the ray of a screen pixel, turned by your head, by the chosen
  heading, into the world, then into the camera of that frame, and looked up in the sphere. No frame around it and
  no margin: the sphere is whole.
- **Switching what the view follows is instant**, paused or playing: every mode's heading is already in the frame.
  So is turning your head while paused: no seek, no new render.
- **What needs tuning goes in Gyroflow** (the stitching distance, the lens guards, the compass's calibration, where
  to look when). What works without it is on in the player.

## Why this shape

Four ways were weighed:

| | Head turned in | Heading in | Cost | Verdict |
|---|---|---|---|---|
| Like crop mode today: the camera into the stabilization | the plugin | the plugin | 4–6 frames behind the hand (~70–100 ms at 60 fps) plus the control file's 50 ms, a margin that runs out | not responsive |
| Everything in a shader from the raw fisheyes | the shader | the shader | the lens models (Mei, DJI's polynomials), the seam, the body masks and the rolling shutter written again in GLSL; the frame's orientation through an mpv patch (no frame time in user shaders) | kept as a later optimization, for bandwidth |
| A world-locked sphere from the plugin, the head in a shader | the shader | the plugin | a heading or smoothing change goes through the pipeline, and the sphere depends on what the viewer chose | superseded |
| **A sphere in the camera's axes, angles in the frame, view in the shader** | the shader | the shader | the same copies as the previous one (throughput, not latency) | **chosen** |

The sphere is the point where the source is normalized the least while everything hard stays in one place, in
Rust: per pixel it's about as big as the two lenses, and the lens models and stitching are the ones Gyroflow and
telemetry-parser already have. The rolling shutter has to be taken out there: within one frame the camera turns
while the sensor is read, which no single orientation per frame can express. It's about the optics and the time
of capture, not about the viewer, so it belongs with the sphere.

## The pipeline

```
decoder (VideoToolbox; two lens tracks)
  → lavfi-complex hstack (one frame, both lenses)                      mpv, as it is
  → vs-gyroflow, at the video's rate:                                   VapourSynth
      per lens: its model and pose (CameraRig) → the sphere in the camera's axes at mid-frame,
      rolling shutter out, seam blended, body masked
      + the stamp rows: the camera in the world, the headings
  → the view shader, at the screen's rate:                              mpv user shader
      screen pixel → ray (output projection, FOV)
      → head (yaw, pitch, roll: DYNAMIC params)
      → heading of the chosen mode (from the stamp)
      → world → camera of this frame (from the stamp)
      → sphere lookup
  → the looks' color shaders (dlogm) → screen
```

## The plugin's part

A new mode of `vs-gyroflow`, chosen when the clip is a 360 rig. It renders no zoom, no edges, no levels and no
smoothing of the picture: a 360 view has no frame to keep filled.

- **Geometry from telemetry-parser's `CameraRig`** (the revision in `Cargo.lock` already has it): each lens's
  model, its pose in the rig (rotation **and** translation: 31.9 mm between the X5's lenses, which shows on the
  seam), which video track it is in, its validity and blend limits, the body masks, the readout time and
  direction, the calibration variants (bare, lens guards, dive case) and the stitching distances. Conventions are
  `video-types`' (OpenCV axes: +x right, +y down, +z forward; Hamilton quaternions `[w, x, y, z]`; equirect
  longitude 0 at +z, +π/2 at +x, latitude +π/2 up), so that the sphere means the same as upstream's.
- **Each lens rendered by gyroflow-core's kernel** into the sphere, with its pose, then blended by the lens's own
  feather (`BlendMode::Feather`). The kernel is single-lens; it's called once per lens. It needs an **equirectangular
  output projection** in our fork (`OutputProjection` has rectilinear, stereographic, equidistant, equisolid and
  orthographic): output pixel → longitude, latitude → ray; rays behind a lens are invalid, not clamped. Insta360's
  unified-sphere (Mei) model is in the fork already. GoPro's EAC is no lens at all: its faces are moved onto the
  sphere and the doubled overlap blended (telemetry-parser's `gopro/eac.rs` describes it).
- **The camera at mid-frame.** Each row is moved from the camera at its own time to the camera at the middle of the
  frame, from the gyro (gyroflow-core's integration, or DJI's own fused quaternion), as gyroflow-core already does
  for flat clips.
- **Size.** About 1.2× the lenses' own density, so the second resampling (sphere → view) loses little. An X5's
  lens is ~19 px/° (3840 px over ~200°): an equirect 8192×4096. Equirect wastes pixels at the poles; a cubemap or
  EAC sphere would be ~30 % smaller, a later option.

### The stamp

Rows appended under the sphere, which the view shader reads and never shows. Written by the plugin per frame,
in the luma plane only (chroma is subsampled and left alone). `vs-gyroflow/src/stamp.rs` writes and reads it;
`shaders/stamp_probe.glsl` is the shader's side, as a probe.

- **Where**: the last 2 rows, from the left: 34 codes, each a block of 2×2 pixels of one value (68 pixels across).
  The shader reads a block with one texel fetch; the block keeps a code whole through anything short of a scaler.
- **The header**, codes 0–6, in coarse codes: 16 levels, each in the middle of its sixteenth, so the shader reads
  them as `floor(v · 16)` before it knows the plane's depth: the magic 12, 3, 9, 6; the depth written at, less one;
  the bits per fine code, less one; the version (1).
- **Two references**, codes 7 and 8: black (code 0) and white (the depth's top code, 1023 at 10-bit). The shader
  reads every code through them, `round((v − black) / (white − black) · (2^depth − 1))`: gpu-next doesn't normalize
  every format to `code / (2^depth − 1)` (below), and the references make the reading exact whatever it does short
  of a curve.
- **The fields**, codes 9–33, in fine codes of the depth's bits (at most 16), fixed point, most significant code
  first:
  - the frame's time in µs, 4 codes (modulo 2^(4·bits): 40 bits at 10-bit), for checking that stamp and picture go
    together;
  - `q_world_from_cam`, w x y z, 2 codes a component over [−1, 1]: the camera at mid-frame, in the world (gravity
    down, yaw arbitrary unless a mode has an absolute reference). 20 bits at 10-bit: ~2·10⁻⁶, a ten-thousandth of
    a degree;
  - per heading mode (in the world, along the camera, along the way you move, north): the heading's yaw in the
    world, 2 codes over a turn (wrapping), then whether it's valid there, 1 code over [0, 1];
  - a check: the sum of the fields' codes, modulo 2^bits.
- A frame without the magic (any other clip) is no stamp; the shader passes it through.

**What reaches the shader** (measured with `tools/stamp360/`, step 1 of the plan; the app's mpv 0.41, libplacebo
7.387, an M-series Mac): synthetic 1080p clips, 8- and 10-bit, limited and full range; decoded in software and with
`videotoolbox-copy`; through VapourSynth as the player has them (the plugin's test filter `Stamp360Test` stamps
known values) and, to get VideoToolbox's p010/nv12 to the renderer as they are, stamped lossless HEVC with no
VapourSynth; and through VapourSynth converted to nv12/p010 after it. The probe reads the stamp in a `LUMA` hook
and paints what it decoded; a screenshot of each frame is compared with what the plugin logged.

- The planes reach gpu-next unchanged: every code of every block reads back, in all 24 cases, with the references.
- Without them, 10-bit is off by one at the top: limited range (any format) reads `v = code / 1023 × 0.99904`
  (`65472 / 65535` exactly), p010 converted from full-range planar `× 1.00096`. 8-bit reads
  `code / 255` everywhere. What the shader gets is linear in the code in every case (residuals under 0.02 of a
  code), which is what two references undo.
- VapourSynth has no semi-planar formats, so in the player gpu-next gets planar `yuv420p` / `yuv420p10` whatever the
  decoder gives; nv12 and p010 reach it only without the plugin.
- The vo's own screenshots fail now and then ("Unknown error occurred while trying to take screenshot!") and fall
  back to one without shaders; the test plays the case again then.
- mpv's conversion to p010 from full-range planar dithers (zimg), which the test turns off; nothing in the player
  converts so.
- The AVFoundation sample-buffer vo (patch 0002) runs no user shaders: the stamp, like the view, needs gpu-next.

### The heading modes

The headings are computed in Rust over the whole clip (smoothing with a look-ahead, as `smoothing.rs` does), so
the shader only picks one. Pitch and roll always come from gravity: the horizon is level in every mode.

| Mode | The heading | Needs | Where it comes from |
|---|---|---|---|
| In the world | the camera's yaw at the clip's start, held | nothing; a slow drift of the gyro's yaw, about degrees per minute without a magnetometer | any camera |
| Along the camera | the camera's yaw, smoothed over 2–5 s | the camera facing the way you go (helmet, handlebars); on a selfie stick it turns with the hand | any camera |
| Along the way you move | the GPS course over ground, the gyro bridging between fixes | a GPS fix, and the gyro's yaw aligned to the GPS: one angle (and its slow drift) fitted so the accelerometer's horizontal acceleration matches the GPS velocity's change. Below a speed threshold, along the camera | GoPro MAX / MAX 2 built in; Insta360 and DJI with a GPS remote |
| North | magnetic north | MAX 2: `MNOR` with its confidence `OCNF` (telemetry-parser doesn't read them yet); MAX: `MAGN` and its calibration; elsewhere the GPS alignment above gives it too | MAX 2 without setup; MAX after calibration |

Along the way you move is what matters most for 360: on a stick the camera's direction means nothing, and on a
helmet it's where the head looks. Without GPS it can only come from the picture (the focus of expansion of the
optical flow, well defined on a full sphere), which is Gyroflow's part, later.

## The view shader's part

An mpv user shader (gpu-next), in the spirit of [mpv360](https://github.com/kasper93/mpv360) (MIT): reusing its
filtering and projections is an option.

- **Inputs**: the sphere and the stamp (the frame), and as `DYNAMIC` params (no recompiling): the head's yaw,
  pitch, roll, the FOV, the output projection, the heading mode.
- **Output projection**: rectilinear up to ~110° across, stereographic beyond; a little planet as a choice.
- **Filtering**: bicubic or Lanczos; a wide view shrinks the sphere, which shimmers without a wider filter.
- **Order**: the stamp read first (raw luma), the view next, the looks' color shaders on the view.
- **Frames without a stamp** (any other clip) pass through untouched.

## The head: input and latency

The head is turned by the same gestures as crop mode today (a drag, a two-finger scroll; a pinch or the wheel for
the FOV; a double click back), through the same `--macos-gesture-messages` (mpv patch 0008).

The shader alone makes it responsive within mpv's own display latency. Below that, to ~20 ms from finger to light:

| Step | Gain |
|---|---|
| The vo redraws at every refresh while the view moves, playing or paused (`video-sync=display-resample` or a small patch) | the view moves at 120 Hz with 30/60 fps video |
| The vo takes the gestures' accumulated deltas just before it draws, not through Lua (patch 0008 extended: the view params set from the gesture thread) | a few ms, and no jitter from the scripts' thread |
| A short swapchain (`swapchain-depth` 1–2), full screen with direct scanout | one to two refreshes |
| 120 Hz (ProMotion) | half of everything that counts refreshes |

Estimated, not measured: ~18–30 ms at 120 Hz, ~35–45 ms at 60 Hz. Through the macOS sample-buffer layer
(patch 0002, HDR and Dolby Vision) the latency is unknown. **(to try: measure with the gesture event's time against
the drawable's `presentedTime`, and with a 240 fps camera on finger and screen.)**

## What goes where: player or Gyroflow

**In the player, with no setup:**

- telling a 360 clip by its rig (`.insv`, `.360`, `.OSV`) or, for a stitched one, by its spherical metadata;
- the sphere from the file's own calibration, at the maker's default stitching distance;
- the camera's orientation from its gyro, the horizon from gravity, the rolling shutter from the rig's readout;
- the heading modes in the world, along the camera, along the way you move (when there's GPS and the alignment is
  sure of itself) and north (MAX 2);
- turning the head, the FOV, the projection; crop mode's frame and photos on top of the view.

**In Gyroflow, as settings:**

- the stitching distance per clip or by keyframes (on an X5 the lenses disagree by ~13 px at infinity on a
  2048-wide layer; on a stick the subject is at 0.4–0.8 m);
- the calibration variant: lens guards, dive case (DJI records it, Insta360 doesn't);
- the magnetometer's calibration, the heading's offset by hand, the GPS to video sync;
- keyframes of where to look (a director's cut), the exports;
- later, from the picture: the heading from optical flow, a better seam.

Until Gyroflow's project has 360 fields (issue #800 is in its 2.0.0 milestone), ours go in a block of their own in
the `.gyroflow` (`"afp_360": { ... }`). **(to try: that Gyroflow keeps unknown keys when it saves.)**

## Stitched clips

Equirect clips with spherical metadata (exports from Insta360 Studio, DJI Studio, GoPro Player; the Theta X and
QooCam stitch in the camera) usually have their stabilization baked in and no motion data. They go to the view
shader directly, with no plugin and with `hwdec` zero-copy: their stamp is absent, so the shader takes the camera
as level and fixed, and only the head turns. This is the first step to build: it proves the shader and the input.

## Exports

The same sphere and stamp, the same view math in Rust (WGSL), so that an export looks as the player shows:

- **stabilized 360**: the sphere turned into the world and the chosen heading, written with spherical metadata;
- **flat reframe**: the view along keyframes of the head, through gyroflow-export as crop mode's photos are.

The view math exists twice, in GLSL and WGSL, so a test renders both from the same sphere and stamp and compares.

## Throughput

Latency no longer depends on it; the frame rate still does. Per frame of an X5 (two 3840² lenses, 10-bit):
~88 MB in (copied out of the decoder, uploaded for the kernel), ~50 MB out (read back, uploaded by mpv), about
260 MB of copies: ~8 GB/s at 30 fps. 4K60 today is ~6 GB/s. The same order, so likely fine; twice that for 8K60
cameras (Insta360 X6, Osmo 360 II). **(to try: on the perf harness, #43.)** If it doesn't fit, in order:
the two tracks joined on the GPU (an `hstack_vt` filter in Metal, as `yadif_videotoolbox` is, which needs our own
ffmpeg build; or an mpv patch that hands a second track to the shader as a texture), then the all-in-shader option
from the table above.

## The plan

In detail, with what each step touches and when it's done: [360-implementation-plan.md](360-implementation-plan.md).

0. **Stitched clips**: the view shader with the head, gestures, FOV; spherical metadata recognized.
1. **The stamp**: read raw luma in gpu-next, on a synthetic stamped clip. Done: exact with the references
   ("What reaches the shader" above).
2. **A throughput spike**: two 8K HEVC 10-bit tracks through lavfi-complex into VapourSynth, no render.
3. **Raw clips**: the equirect output projection in the fork, the per-lens render from `CameraRig`, the stamp,
   the modes in the world and along the camera. One camera first, the one we have clips from.
4. **Along the way you move and north**: the GPS alignment; `MNOR`/`OCNF` in telemetry-parser (a pull request);
   `MAGN` with its calibration.
5. **Below 20 ms**: redraw per refresh, gestures to the vo, a short swapchain; measured.
6. **Gyroflow's part**: the stitching distance, variants, keyframes, exports, ideally with upstream's 2.0.

## Open questions

- Which 360 cameras and clips we have: steps 2–3 start from them. `telemetry.lua` reads the Osmo 360's schema,
  but no recording has checked it.
- Talking to Gyroflow's author before step 3: upstream's 360 renderer isn't public yet, and our kernel should
  stay close to what it will be (the same `video-types` geometry).
- Equirect or a cubemap / EAC for the sphere.
- Whether crop mode on flat clips takes the same route: a stamped camera per frame and the view corrected in the
  shader at every refresh, within the margin it has.
