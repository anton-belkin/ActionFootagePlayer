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
  blended in by spans made once per clip, the stamp per frame. 7680×3840 and bilinear by default: the kernel's time
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
  300° FOV: level within a few degrees. That screen test was with the earlier IMU turn; the final one is checked
  only without the screen (rendered views, `level-*.png` of the ignored test).
- **The head and the modes** on screen: the drag/IPC turns, both modes switch at once playing and paused, the stamp
  reads back (`stamped`), the little planet.
- **The seam** at the default 2 m: not visible in the rendered spheres at the clip's scale; not judged closely on
  screen. There is no maker's stitch of this clip to put beside it: not done.
- **Throughput**: without the screen 24–25 fps through the plugin (`--vo=null --untimed`); the same chain with a
  copy in VapourSynth instead of the plugin, 26 fps: mpv's decoding, joining and conversion of two 3840² tracks is
  the limit on this clip, not the plugin. **On the screen it doesn't keep up**: ~20–22 fps through the plugin, mpv
  dropping ~12 frames a second of 25. Not solved here; options: a 5760 sphere (half the plugin's GPU time, and less to
  upload), the plugin's GPU time traded against the view shader's, step 6's GPU route.

Open: the throughput on screen; the final IMU turns on screen; travel and north (step 5); X4, X3 (two files), DJI
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

### 7. Exports (3–5 days)

- `gyroflow-export --360`: the stabilized sphere (turned into the world and the chosen heading) with spherical
  metadata, for YouTube and editors.
- A flat reframe: the view along keyframes of the head, as crop mode's photos are made now.
- The view math in WGSL beside the GLSL; a test renders both from the same sphere and stamp and compares.

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
