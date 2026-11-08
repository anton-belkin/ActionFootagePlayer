# The render on the GPU: Gyroflow's warp as shaders

How the player, a streaming server and the export could render a stabilized clip with the frame never leaving the
GPU: Gyroflow's per-frame warp ported to GLSL and run by libplacebo, with the color fringes, the edge fill and the
look as passes of the same render rather than steps after it. This is a design, not built yet.

Written 2026-11-06 after measuring the Linux player on the NAS (a UGREEN DXP2800, Intel N100) and a prototype warp
shader there. Anton's framing: port Gyroflow's render as a whole, not a stabilization shader on the side; the
chromatic correction becomes natural instead of a patch at the end, the edge fill belongs in the same place, and a
temporal denoise could later run in real time too. Nothing here has been built: what has to be checked first is
marked **(to try)**.

## In short

- **Why.** Through VapourSynth every frame goes GPU → memory → GPU and back again, plane by plane. On the N100 that,
  not the arithmetic, caps the stabilization at ~17 frames a second (6.8 as the plugin does it today, at 4K60). With
  the warp as a shader the same machine plays 4K60 → 1080p HDR stabilized at 60 fps, and encodes a 1080p60 stream
  beside it. On a Mac it means less CPU, less power and no frame copies for any clip.
- **What moves to the GPU**: Gyroflow's undistort kernel (lens model, rolling shutter, IBIS/OIS, output projection,
  digital lens, background), the chromatic correction under water, the edge fill, and the look after them.
- **What stays on the CPU**: everything that is about the motion and the lens, not the pixels: reading the motion
  data, smoothing, horizon lock, the zoom, lens profiles, the per-frame transforms. vs-gyroflow keeps doing that; it
  hands each frame's numbers to the shader instead of rendering with them.
- **The frame's identity is no longer a problem in mpv**: since mpv 0.42.0 a user shader can declare `//!PARAM PTS`
  and mpv fills it with the time of the frame being drawn. The stamps in the pixels (chromatic, crop mode, 360) were
  workarounds for its absence.
- **Quality is a setting per machine, measured**: interpolation, shrinking, render size, the lens's exactness,
  chromatic, the edge fill and the frame rate are knobs in one Best-to-Lightest list, its default picked from mpv's
  per-pass GPU times, not fixed by frame rate.
- **One shader source, three hosts**: mpv's gpu-next (the player), a small libplacebo renderer of our own (the
  streaming server, later the export), and checks against Gyroflow's own CPU reference.

## What was measured

On the NAS, 2026-11-06 (N100, iGPU at its 750 MHz maximum, 6 W, one DDR5-4800 module; clip 0126, Osmo Action 6,
4K60 10-bit D-Log M with a Gyroflow project; output 1920×1080 HLG; mpv 0.40 gpu-next on a headless Wayland output;
release 0.9.0 plugins):

| Chain | Result |
|---|---|
| D-Log M → HDR, mpv's default downscaler | 35 fps shown, 489 drops in 20 s |
| D-Log M → HDR, `--profile=fast` | 60 fps, no drops (75 fps untimed) |
| Stabilization as the plugin does it on Linux (vaapi-copy, CPU scale, VapourSynth) | 0.3× speed, 6.8 fps shown |
| Same, every 4th frame, scaled on the GPU before the copy | real time at 15 fps |
| Prototype warp shader + D-Log M, VAAPI zero-copy | 60 fps, no drops (72 fps untimed) |
| Same, with a 1080p60 HEVC Main10 encode (VDEnc) running beside it | 60 fps, no drops |

Limits found on the way: VAAPI decodes this clip at ~280 fps; copying decoded 4K frames to memory tops out at ~36
fps; the VapourSynth stage at ~16–19 fps whatever comes before it, with the GPU half idle and mpv's core thread at
~80 %. VDEnc encodes 1080p60 Main10 at 126 fps; the default (VME) mode only 53.

The prototype warp was close to Gyroflow's work per pixel but not the real thing: 32 rotation matrices per frame
from a texture (synthetic), opencv fisheye, the two-step row lookup, bicubic through 4 or 16 bilinear fetches. The
real kernel does more (below), so the ~20 % headroom on the N100 is the number to watch.

## What Gyroflow's kernel is

From gyroflow-core at the fork's d130105 (`src/core/`):

- **The canonical GPU copy** is `gpu/wgpu_undistort.wgsl` (845 lines), assembled as text with the lens model's and
  the digital lens's functions. `gpu/opencl_undistort.cl` is the same logic for OpenCL, `stabilization/cpu_undistort.rs`
  the CPU reference. **`src/qt_gpu/undistort.frag` is already GLSL** (437 lines), Gyroflow's preview shader, with
  every per-frame input in textures: simplified (bilinear only, no source/output rects), but the natural start.
- **Inputs per frame**: `KernelParams` (416 bytes: focal length and center, lens coefficients `k[24]`, fov, field
  limit, lens correction amount, background mode and margin, rotations, translations, rects, digital lens params,
  light refraction, output projection, flags) and **one 4×4 block per sensor row** when the rolling shutter is
  corrected (the frame's height in rows, else one): a 3×3 rotation, IBIS shift and roll, OIS shift, lens breathing.
  The kernel projects through the middle row to find the source row, then uses that row's block, nearest.
- **Lens models**, each written once per backend: opencv_fisheye, opencv_standard, poly3, poly5, ptlens, insta360,
  sony, generic_polynomial, gopro; digital (output) lenses gopro_superview, gopro6_superview, gopro_hyperview,
  gopro_warp, digital_stretch. The warp only needs `distort_point` per pixel at full lens correction; that is closed
  form for opencv_fisheye (our DJI cameras) but iterative inside for gopro (10), sony (8), insta360 (20 × 3), and the
  GoPro digital lenses (12). Below full correction a 6-step root solve runs per pixel.
- **Interpolation**: bilinear, bicubic, Lanczos4 from 32-phase tables, and EWA (Robidoux etc., 3 transforms per pixel
  and a loop up to 128 px). **Background**: solid, repeat, mirror, margin with feather.
- **Mesh correction and focal-plane distortion** (Sony) read a third buffer; the drawing overlay a fourth.
- **On the CPU per frame**: `FrameTransform::at_timestamp`, a quaternion product and a 3×3 matrix per row, in
  parallel; 0.13–0.14 ms a frame with 2160 rows on the M1 (measured in the spike, [gpu-render-spike.md](gpu-render-spike.md)),
  with a cache of 100 frames. Smoothing, horizon lock and the
  adaptive zoom's per-frame FOVs are already done once per clip.
- **License**: every kernel and model file is GPL-3.0-or-later, as this repository is; libplacebo (LGPL-2.1+) is fine
  under it. The port is a derivative of Gyroflow's code and says so.

Nothing in it is hard for GLSL: no doubles, no atomics, no subgroups. Storage buffers become textures (Qt's shader
already does that), override constants become `#define`s (one shader per lens model and digital lens, as Gyroflow
compiles them), the Newton loops keep their `break`s.

## What vs-gyroflow does today, and where each part goes

| Today (vs-gyroflow, VapourSynth) | On the GPU |
|---|---|
| Stabilize: each plane uploaded, warped by gyroflow-core's wgpu kernel, read back; the row matrices rebuilt for each plane | **The warp pass**, once per frame, in RGB, at the output size |
| Chromatic (dispersion.rs): the lens geometry stamped into the last luma row; chromatic.glsl re-samples red and blue at the end, from a picture already warped and resampled | **Inside the warp pass or not at all**: the green ray refracted per channel (Snell's law with water's indices) and each channel sampled once from the source; no pass at the end |
| Edge fill (edges.rs, sphere.rs, smooth.rs): a mask rendered on the GPU, the fill and blur on a coarse CPU grid, the sphere (a cube map with a 5 s memory) on the CPU, the composite on the CPU | **Passes of the render**: the warp knows per pixel that its ray left the frame; the sphere is a storage texture written each frame from the covered pixels; pull-push and blur as a mip chain; the composite in the warp pass |
| Crop mode's preview, the 360 stamp: identity marks in the pixels | `PTS` and parameters; no marks |
| Still (still.rs): on pause, ±5 neighbours re-decoded through VapourSynth, block matching and fusion on wgpu | **Stays as it is in the player for now** (a pause can afford it); moves into our renderer, which can read any frame |
| Levels, motion streaming, water spans, lens profiles, projects | **Unchanged**, CPU, in vs-gyroflow's engine |
| gyroflow-export: the same engine called directly (no VapourSynth), frames through memory as in the player | Later through our renderer, so the export and the player share the shader by construction |

Two things the port fixes in passing: the export currently has no color-fringe correction (`marker = false`), and it
interpolates with Lanczos where the player uses bicubic above 31 fps.

## The design

### Two halves

**The numbers** come from vs-gyroflow's engine as today (`Engine::load`, the views and levels, motion streaming),
built as a library. What changes is the output: instead of rendering, it produces each frame's parameters: the
`KernelParams` the frame needs (most fields are per clip; fov, lens coefficients from per-frame metadata, correction
amount, background margin, zoom center and refraction can change per frame) and the row rotations.

**The pixels** are one GLSL source, `shaders/gyroflow/` (new), generated per lens model and digital lens like
Gyroflow's: the warp pass, the chromatic split inside it, the edge passes, sharing the kernel's lens functions.

### The row rotations: knots, not rows

At full resolution a frame needs 2160 blocks of 16 floats (138 kB); a 5-minute 4K60 clip 2.5 GB. The rotation
along a frame is smooth (the camera can't change direction much in 16 ms), so the table keeps **knots**: a quaternion
at, say, 33 evenly spaced row times per frame, interpolated in the shader (slerp or normalized lerp between the two
nearest knots). That is ~600 bytes a frame with the per-frame parameters, 10 MB for a 5-minute 4K60 clip.

This differs from Gyroflow's nearest-row lookup by at most 0.02 px with 33 knots, measured against `cpu_undistort.rs`
on a vibrating HERO7 flight clip ([gpu-render-spike.md](gpu-render-spike.md)): 33 knots it is; 17 cost 0.04 px, 9 cost 0.07. IBIS/OIS and breathing (Sony, some others) go
as further knot tracks only for clips that have them.

### Getting the numbers to the shader

**In mpv.** The shader takes `//!PARAM PTS` (mpv 0.42+, `video/out/gpu/utils.c`) and finds its frame in a table
texture. The table has to be a `//!TEXTURE` in a shader file, which mpv loads with the file, so per clip the plugin
writes a shader file (as dlogm writes its LUT shaders) with the clip's table embedded, and puts it in
`file-local-options/glsl-shaders`. Questions this raises:

- **Size and load time**: 10 MB of floats is 20 MB of hex for libplacebo to parse. Measured on the N100: a 3.2 MB table
  shader adds 0.1 s to mpv's start, a 165 MB one 2.3 s ([gpu-render-spike.md](gpu-render-spike.md)). Long clips (an hour at 60 fps: 125 MB) need windows: a table per few minutes, the next one
  written ahead and swapped in **(to try: whether swapping `glsl-shaders` drops a frame)**.
- **Motion streaming** (#80: a clip without a project starts after ~1 s and its motion data keeps arriving): the
  table grows as data comes in. Either the shader is rewritten a few times early in the clip (at 2 s, 8 s, then the
  whole clip), with an identity correction where the table has no data yet, as motion.rs does now, or an mpv patch
  lets a script replace a texture's contents. The patch is the cleaner way but only the Mac app has patches.
- **Interpolation and frame mixing**: `PTS` is the current frame's, also for the frames libplacebo mixes in, so
  `interpolation=yes` would warp them with the wrong motion. It is off by default and the app doesn't set it; the
  plugin should turn it off for the clips it warps, in case a user's mpv.conf has it.
- **Patch 0011** (parameter values in the mix cache's key) makes a changing `PTS` re-render the cached frames
  **(to try: with interpolation off, one frame per mix, it should not matter)**.
- **What is drawn when the table is missing** (a seek past what has been read, a failed load): the frame as it is,
  as now.

**In our renderer.** The host knows which frame it renders: it uploads the frame's parameters and knots as a small
texture or uniform buffer each frame, and the table problem disappears. The same GLSL is wrapped with
`pl_mpv_user_shader_parse` and its parameters set per frame, or called as a `pl_shader` of our own.

### Opening a clip and changing a setting

Measured 2026-11-07 on the M1 Pro (the app built from this branch, logs of its own runs, tools in the session's
scratchpad: open/run.sh + report.py), clip 0126 (a project) and three clips without one:

| | Before | Now |
|---|---|---|
| The app started on a clip, to its first frame | 1.1-1.9 s; 3.0 s with the project's video on the NAS | 0.81-0.96 s, stabilized from that frame |
| Another clip in the same player: one seen before | the table again, 0.35-1 s, then mpv's own 0.4 s | 0.1 s for the warp, mpv's first frame at ~0.42 s |
| ... one never opened | the same, and 4-7 s when its motion data came from the NAS | 0.4-1.2 s |
| Edges, color fringes, Ctrl+p, the view | 0.46-0.5 s (each wrote the whole table again) | 0.08-0.12 s |
| A new shader variant's first compile in mpv | 0.3-0.6 s | once per variant (mpv's shader cache keeps it across runs; clips alike share one) |

What changed:

- **The table server** (`gyroflow-gpu-table --serve`, one per player, started by main.lua): it keeps the clips it
  loaded with every view (normal, wide, the levels) and their tables, by the settings that move the geometry
  (`PLAYER_GEOMETRY_KEYS` and the project's file as it is). A change of the edges, the fringes, the denoise, the
  interpolation or the view writes the shader alone: ~10 ms there, ~0.1 s in all with mpv taking the 4 MB file.
- **Shaders kept on disk** (`~/Library/Caches/ActionFootagePlayer/gpu`, 400 MB, the least used first): by every
  setting and the clip's files as they are; a clip opened again, in any session, gets its shader in ~15 ms.
- **The clip opens while its table is written**: mpv's own opening (decoder, window, audio: ~0.4 s, at the app's
  start ~0.8 s) no longer waits for it; the clip is paused on black (gpu_hold.glsl) until the warp is in place.
- **Motion data and the probe's answer kept per file** (`motion/`, `probe/`): a project's video was read again at each
  load to fill in its motion data, and an optical project's (DJI with EIS on, no motion data) was read by
  gyroflow-core itself again too: 1-2 s from the NAS, 7 s cold. Now its metadata is taken as a full project has it.
- **The shader's text is the same for clips alike** (the frame count is in the table, texel 9's w; a row holds as
  many frames as fit, whatever the clip's length): mpv compiles a camera's shader once.
- **The table built on every core**, only the knots' rows made quaternions: a clip with a rolling shutter (2160
  matrices a frame) 0.75 s → 0.43 s.
- **No pause after opening**: on the GPU path main.lua paused for a "set up for this display" that never came (the
  VapourSynth path's playback-restart), up to 5 s after a clip opened.

Left: gyroflow-core's zoom search (0.2 s of a project's load) and the rows' transforms (0.3 s of a no-project table),
done once per clip and settings now; mpv's own 0.4-0.8 s.

**Each change is a new shader file, and mpv has to let the old one go** (tools/mpv-patches/0015, 2026-11-08).
vo_gpu_next parses a user shader once and keeps it by its path until the window closes, with every texture and
buffer its `//!TEXTURE` and `//!BUFFER` blocks make on the GPU. main.lua writes each version of the warp to a new
path (`mpv-gyroflow-<pid>-<n>.glsl`: the same path would get the old parse back), so before the patch every setting
changed, and every clip opened, left a whole warp resident: its table, its grid and, with Ctrl+p, the denoise's ring
of 11 frames (`GDN_FRAMES`, 7680×12960 rgb10a2, ~400 MB, and `GDN_QUART`, ~50 MB). On a 16 GB Mac a session slowed
down and dropped frames after a dozen or two changes, until the player was restarted. The patch destroys the shaders
no longer in `glsl-shaders` when the options change (libplacebo frees them once the GPU is done with them), and
frees the file's text mpv read (~5 MB a change: libplacebo keeps its own copy). Measured on the M1 Pro (16 GB), clip
0068 (4K60), a 1280×720 window, the edges and the view changed over IPC every 2.5 s, the process's footprint:

| Changes | Ctrl+p on, before | Ctrl+p on, patched | Ctrl+p off, before | Ctrl+p off, patched |
|---|---|---|---|---|
| 0 | 986 MB | 991 MB | 1078 MB, 22 dropped / 6 s | 587 MB, 6 dropped |
| 10 | 5680 MB | 975 MB | 1386 MB | 583 MB |
| 20 | 10240 MB, swapping: plays at 0.2× | 1010 MB, 0 dropped | 1793 MB, 42 dropped | 686 MB |
| 40 | (stopped at 9 GB) | 1020 MB, 0 dropped | 2500 MB, 45 dropped | 774 MB (before the text was freed) |

The distribution's mpv on Linux and Windows' mpv (shinchiro's build) don't have the patch, so main.lua copes with an
mpv that keeps them (`gpu_flush=auto`: any mpv without 0014's `--video-join`, which only the app's has):

- **The same settings, the same file**: a shader is kept by the settings it was written with (the gyroflow-gpu-table
  request, and the project's file as it is), 16 of them, and asked for again it's that file, not a new one: mpv gives
  its parse back and holds nothing more (the denoise's ring started again by `gyroflow_dn_epoch`). Changing the edges
  back and forth costs nothing after the first time, and no table is written (on the Mac too).
- **The window made again**: what the shaders no longer used hold is counted (their text twice, the grid, the ring);
  past 1.5 GB, mpv's `vo` is set to another list with the same output (its first entry twice, or once again) as the
  next clip opens, and mpv makes a new window with no shader parsed but those in use; past 4 GB, at once.

Measured as above with 0.10.0's mpv (no patch 0015; `gpu_flush=yes`, as it has `--video-join`), Ctrl+p on: 20 changes
among 4 settings, then 20 that make new ones (the level, the view, the edges), then the next clip:

| | 0 | 4 | 20 | 24 | 28 | 40 | next clip |
|---|---|---|---|---|---|---|---|
| Footprint, MB | 1005 | 2422 | 2905 (the same 4 files again) | 4763 | 1527 (made again at once) | 2893 | 1084 (made again as it opened) |

No frame dropped in any sample. With the app's mpv the same run stays at 964-1018 MB, and nothing is made again.

### Save frame (crop mode and the photo) on the GPU path

**Crop mode's camera** is the warp's own: `gyroflow_cam_x..w` and `gyroflow_cam_zoom` (DYNAMIC parameters, set through
glsl-shader-opts by main.lua's gpu_camera as it turns). gyroflow-core's view_rotation multiplies each row's rotation
on the right by the view's quaternion in its matrices' axes (y and z turned), and the output camera's in the world
too; the zoom is the FOV's (`Camera::apply`). So the shader turns the output ray by it first, and the sphere trail and
the denoise's motion prediction follow. Checked against gyroflow-core with the camera (gpu_table's tests: each row's
rotation to 0.001°; the shader's coordinates within 0.004-0.04 px exact, 0.04-0.17 px through the 8 px grid, which
bends more with the camera zoomed out). Turning it costs nothing: no table, no render of the paused frame again, no
seek, no preview standing in (the VapourSynth path's crop_preview.glsl and its parity marks): the next refresh has it,
paused or playing.

**The photo** (Alt+s; Enter in crop mode) is the frame as the player renders it, where that's the photo: mpv's
screenshot of the video at its size, through the warp, in the video's own colors (BT.2020 HLG: `screenshot-format=png`
with `screenshot-tag-colorspace`; `screenshot-raw` converts to sRGB whatever it's asked), an uncompressed 16-bit PNG
(0.36-0.44 s), and `gyroflow-export --photo --image` makes the SDR image from it and writes the HEIC with its gain map
(or the Ultra HDR JPEG), its metadata and the Live Photo's motion as before. Measured on clip 0068 with a camera
turned: **1.2 s from Enter to the photo saved** (gyroflow-export alone 0.63 s, against 1.69 s rendering the frame
itself, and 4.5-5.2 s with the clean frame); a Live Photo 4.3 s, its 3 s of motion (2.9 s, rendered as before)
taking the most. Against the photo rendered the old way at the same moment: the same picture, a mean difference of 2.4
of 255 inside it; the edges are the warp's own fill, as on screen. Only for the looks whose frame is the HLG (hdr,
tonemap, dv, dvsdr; maker and nbcu end in SDR) and with the clean frame off (it takes the frames around it at full
size: gyroflow-export's own). The photo has the video's pixels across the view (a 0.8 frame of 4K: 3072x1728), where
the old way renders the view bigger by the camera's zoom.

### Where the warp sits in mpv's chain

`MAINPRESUB`, as the 360 view does: after the conversion to RGB, before the looks, and at the size of the video's
rectangle on screen (`//!WIDTH OUTPUT.w`). The looks (dlogm's, at `MAIN`) then run on the output-sized picture, which
is cheaper than now (today they run at the stabilized size). No mpv scaling follows: the warp is the scaler.

Interpolation in the warp is Gyroflow's, and which one is a quality/performance setting, not a rule by frame rate
(see "Quality and speed"). Today main.lua's `interpolation` picks Lanczos up to 31 fps and bicubic above only
because on an M1 Pro through VapourSynth 4K Lanczos kept up at ~38 fps and bicubic at ~54: what that machine managed,
not something about the clip.

Where the warp shrinks the source a lot (4K into a 1080p window: two to one), a plain bicubic aliases; the prototype
widened its taps, Gyroflow's EWA is the exact answer but costs most. Their cost on the N100 and the M1 Pro is in
[gpu-render-spike.md](gpu-render-spike.md): widened bicubic 24 ms on the N100, EWA 42, both too much there **(to try: a
prefiltered half-size source, on detail like a reef)**.

### Chromatic correction inside the warp, or not at all

Today: the stabilized YUV goes through VapourSynth with one (green) refraction, mpv converts it to RGB, and
chromatic.glsl re-samples red and blue around the lens axis it read from the stamp: a second resampling of an image
already resampled once. That patch at the end is not carried over: the correction is done inside the warp, or, where
the machine can't afford that, not at all. It is a small effect on the picture, not worth a second resampling.

In the warp each output pixel has its ray; under water the green ray is refracted by water's index for green (as
now), and red and blue by theirs, and each channel is read from the source at its own coordinate: one resampling, from
the original pixels, with the exact lens model rather than the stamp's approximation.

**It doesn't need the source converted to RGB first.** The conversion from YCbCr to (non-linear) RGB is a linear
matrix, so reading the Y and CbCr planes at the red channel's coordinate and converting gives exactly the red that a
pre-converted RGB frame would give there, and likewise for green and blue. The warp that reads the planes itself
(below, "Conversion before the warp") samples Y and CbCr at three coordinates, converts each, and keeps R from the
first, G from the second, B from the third. The two ways cost the same in the warp (three times the taps); the
plane-reading one saves the source-sized conversion pass and interpolates the chroma once, with the warp's kernel,
instead of twice (mpv's `cscale` to the source's size, then the warp).

**It costs only under water.** Out of water, or with the correction off, the three coordinates are the same and the
warp samples once: a branch per frame (the indices are per frame), not per pixel. Under water: three times the warp's
taps, or the main kernel for green and bilinear for red and blue, which the eye shouldn't tell from it **(to try:
both, against today's chromatic.glsl on a dive clip, and their cost on the N100)**. The export gets the same
correction for free.

### The edge fill on the GPU

The current fill, step by step, and its GPU form:

1. **Which pixels are uncovered**: the warp pass knows, per pixel, whether its ray fell outside the source (with the
   feathered margin Gyroflow uses for the mask). No separate mask render.
2. **The sphere**: today a CPU cube map (6 × n², n up to 512) of what the camera saw, fading with a 5 s half-life,
   written from a separate wide-view render. On the GPU: a storage texture (a cube map or an equirect, rgba16f) that
   a compute pass writes from the covered pixels of the frame each frame, by world direction; the fade, the scene
   cut reset and the erosion of stale content are per-texel arithmetic. The temporal denoise prototype showed that
   storage textures persist across frames in mpv's user shaders, and how to skip redraws (compare against the last
   frame's luma).
3. **The fill**: pull-push and blur as a mip chain over a coarse output-sized texture, the mute (desaturate, darken
   with distance) per texel.
4. **The composite**: in the warp pass, the fill where the ray left the frame, blended by the mask.

This is a redesign, not a port: the GPU version should look the same, but will not be bit-identical to the CPU one.

**Built (2026-11-08)**, the smooth fill as the VapourSynth path makes it (gpu_table's `fill` chain, warp.glsl's
`fill_*` passes): fragment passes at the warp's hook (MAIN; LUMA and CHROMA each in the planes layout), ahead of the
pass that draws. `fill_lo` puts the warped frame on a grid 135 cells high (3x3 rays a cell, the frame's edge repeated
past it) with how much of each cell the frame covers, its feather counted; `fill_pull` and `fill_push` are smooth.rs's
pull-push, one pass a level (saved textures halving down to 1x1, then back up, each level keeping what it knows); two
`fill_blur` passes are the Gaussian of `edges_blur`. The warp's pass reads the grid through the sampler's filter, mutes
it with the distance from the frame (the source's, not smooth.rs's chamfer distance on the grid), and blends the
picture into it over edges.rs's feather (3 % of the height, 0.2 % in), past the lens's field too. Against the
VapourSynth path with crop mode's camera zoomed out (clip 0126): the same soft edge and blur; the old GPU fill (the
nearest edge's colors, 25 taps) streaked outward from the frame and met it in a line.

Writing the sphere every frame becomes cheap, so `fill_rate` goes. **(to try: sphere and fill passes at 1080p on the
N100.)**

### Temporal denoise (Ctrl+p) on the GPU path: 5 frames on each side

A recursive temporal denoise as a shader already worked (shaders/tdenoise.glsl on the `temporal-denoise` branch:
−62 % noise on water at 4K60 on an M1 Pro), but Anton froze it for playback (the eye averages at 60 fps), and a plain
recursive filter is no good under water: it has to follow what moves. The GPU path's Ctrl+p is the paused frame's way
(vs-gyroflow's still.rs, its radius of 5) in warp.glsl's `dn_` passes, after the warp at MAIN:

- **The window:** each frame is the average of itself and its pixels in the 5 frames before and the 5 after, each
  weighted by how much a 3x3 patch (every other pixel) differs beyond the noise (still.rs's weight; the noise measured
  by brightness from the shown frame against the one before). The frames after it are frames already decoded: the
  shader keeps the last 11 warped frames in a ring (`GDN_FRAMES`, rgb10a2, slots up to 4K two across: 400 MB) and
  **shows the one 5 frames back**. main.lua delays the audio as much (`audio-delay` + 5/fps, taken off again when it
  goes); after a seek the first frame stays until the frames after it are there.
- **The motion is searched once per frame:** between the new frame and the one before it, per 32 px block (the gyro's
  prediction from both frames' cameras, a search within 8 px on a quarter-size luma, then within 2 px at full size with
  a sub-pixel fit). Those steps stay in the ring with their frames (`GDN_VEC`). The motion from the shown frame to each
  of its 10 neighbours is the steps chained (backwards as they are, forwards each one inverted by a fixed-point
  iteration), then checked within a pixel at full size (`GDN_NV`, read by the blend through the sampler's filter).
- **Crop mode** shows no frames after (`gyroflow_dn_after` 0): its camera turns the newest frame as it's dragged, and
  the frames before it are matched with the cameras they were rendered with. Where the window changes (crop mode on or
  off, Ctrl+p, a new shader, a new window size) the ring starts again (`gyroflow_dn_epoch`); paused, main.lua seeks to
  the frame on screen exactly, so the picture stays. Save frame takes the time of the frame on screen
  (`user-data/gyroflow/gpu_delay`).

**Its settings** (the panel's Transform, kept with a clip's transform as `denoise_frames`, `denoise_grain` and
`denoise_tolerance`; the app's defaults gyroflow.conf's `still_radius`, `still_grain`, `still_tolerance`): the frames
on each side (1-5; fewer also shortens the delay), the grain kept (`gyroflow_dn_keep`) and the motion tolerance
(`gyroflow_dn_tolerance`, still.rs's weight's scale: how much a patch may differ and still be averaged in). DYNAMIC
parameters of the shader: a change takes nothing written again, only the frames on each side start the ring again (the
delay changes). The VapourSynth path's paused frame takes the frames and the grain (still.rs has no tolerance).

Measured 2026-11-07 on the M1 Pro, clip 0126 (4K60 D-Log M, under water), a 1080p window:

| | the old one (4 frames before, a search per frame) | 5 + 5, the motion chained |
|---|---|---|
| GPU time a frame (warp + denoise) | 8.7 ms | 8.0 ms (chained motion 1.7, blend 4.3) |
| grain taken out of open water (10-bit codes; 8.0 without) | 3.1 | 4.0 |
| low-pass change against the frame without denoise | 1.53 | 1.34 |

The first try, with a ±2 px search at full size per neighbour and all the window's state worked out per pixel, cost
18 ms a frame and dropped frames at 60 fps. Not done: the paused frame after a change of a setting is cleaned only once
it plays on (its ring starts again from it; stepping 5 frames on would fill half of it); frame-back-step while paused
lands on the frame before time-pos, 4 frames after the one shown; with Ctrl+p on, Save frame stays gyroflow-export's
own (a screenshot at the video's size renders at another size, which would start the ring again); the last 5 frames
of a clip are never shown.

## Hosts and platforms

| Host | mpv | Status of `PTS` | Notes |
|---|---|---|---|
| Mac app | our build, pinned at mpv fba6342 with patches | has it | The DV looks present through AVSampleBufferDisplayLayer (patch 0002), which renders through libplacebo like the swapchain, user shaders included: the warp runs under them (checked 2026-11-07) |
| Windows app | shinchiro's build today; our own build with our patches, as on the Mac (decided 2026-11-06) | has it (the build we ship, 20261018 at mpv 01e2533, is after the change) | the generic patches (0001, 0003, 0011) and the 360 tracks patch can then apply there too |
| Linux | the distribution's | mpv 0.42.0, released 2026-11-05; distros will follow | **VapourSynth is no longer needed for playback**: Debian, Ubuntu and Arch's mpv lack it today; the plugin needs only gpu-next and its own binaries |
| Streaming server | none: our renderer | n/a | below |

The motion side, without VapourSynth, runs in a helper binary next to the plugin (gyroflow-probe and
gyroflow-project already are such binaries): it loads the clip like `Engine::load` and writes the table shader; for
motion streaming it keeps running and rewrites it.

## The streaming server

Anton's idea, new 2026-11-06: the NAS plays a clip stabilized, with its log curve converted, and sends it as a
stream (1080p HDR, Dolby Vision if possible). Its render:

- **Decode**: VAAPI (FFmpeg's hwaccel), frames stay in GPU memory.
- **Import** each plane into Vulkan through dmabuf, as mpv does (one image per plane). FFmpeg's own route (`hwmap` to
  Vulkan, then `libplacebo`) fails on this NAS: Mesa's ANV can't import a two-plane P010 surface with modifiers, can't
  render to Vulkan P010 frames, and has no Vulkan Video decode for this GPU yet.
- **Render** with libplacebo: the warp, the look, at 1080p, into images exported back as a VAAPI P010 surface (two
  plane images, the way the Mac app renders into IOSurface planes for the sample-buffer layer).
- **Encode**: `hevc_vaapi` in VDEnc mode (`low_power`), Main10, tagged HLG or PQ. Dolby Vision profile 8.4 (HLG with
  an RPU) is metadata in the bitstream: vs-gyroflow already writes RPUs for exports (dovi.rs, dv_mux.rs); the live L1
  per frame needs the frame's min/average/max, which libplacebo's peak detection measures on the GPU.
- **Serve**: HLS or fMP4 segments **(open: which clients, which protocol)**.

The same renderer, writing a file instead of a stream, is the export.

## 360° cameras: later, on the Mac and Windows, with the same kernel

Not part of the first steps, and not for Linux or the NAS (no need there for now), but the same port is what 360
playback on the Mac needs for 8K60.

**Today** ([360-cameras.md](360-cameras.md)): sphere360.rs renders each lens with gyroflow-core into an
equirectangular sphere (5760 wide by default) in the camera's axes, with the rolling shutter taken out, blends the
lenses on the CPU along feather spans, and stamps the camera's orientation and the headings under it; view360.glsl
turns the view at the screen's rate. Through VapourSynth that is 5.7K30 with room, 8K30 just, 8K60 not: four or more
full-frame copies of 90–100 MB frames, and VapourSynth's cache ([360-throughput-spike.md](360-throughput-spike.md)).

**On the GPU**, the per-lens part is the kernel this design ports: the lens model (insta360's is the most iterative,
so the lens table of "Quality and speed" matters most here), the rig's pose per lens, the rolling shutter per lens,
and the blend along the feather spans. Two shapes:

- **The sphere kept, made on the GPU**: a pass at the video's rate renders both lenses into a sphere texture, and
  view360.glsl reads it as now. It keeps the decided architecture as it is (the sphere close to the source, the view
  in a shader at the screen's rate, turning the head never waits for the video); only where the sphere is made moves.
  The sphere costs its own size once per video frame.
- **No sphere**: the view pass goes from the screen's ray straight to a lens and its pixel. Only the window's pixels,
  but at the screen's rate (120 Hz on the XDR) and with the lens solve in every refresh.

The first is the one to build; the second only if the sphere's size per frame turns out to be the cost **(to try)**.
The stamp under the sphere becomes the per-frame table, as for flat clips.

**Built (2026-11-07), the sphere on the GPU** (`gyroflow-gpu-table --sphere360`, gpu_table::sphere_shader, warp.glsl's
sphere360 pass; main.lua's add_sphere): from sphere360::prepare, what Sphere360 renders with (the rig, each lens's
gyroflow-core parameters: the equirect output, RigLens, no smoothing), the table has per frame each lens's block (its
parameters and knots: RigLens is in them already, its turn in the rows' rotations, its offset in translation3d, its
validity the field limit) and the stamp's 34 codes. The pass maps each sphere pixel through each lens's kernel (the
fork's equirect projection, ported) and blends them by their shares (rig.rs's weights, with the masks, baked into a
texture a tenth of the sphere's size); under the sphere the stamp's two rows, as gray. A shader's saved textures are
its own (libplacebo keeps them per file), so view360.glsl reads the stamp from the picture: a second stamp pass at MAIN
when `gyroflow_stamp360` is 1, its luma pass otherwise. On the X5 sample against Sphere360 at the same frame: the
sphere differs by 1.05 of 255 on average (bilinear here, Sphere360's bicubic; blended in RGB, not per plane), the view
by 2.97; its table in 0.6-0.85 s; 25 fps with no frame dropped in 15 s (19 dropped through VapourSynth).

**The tracks on the GPU** (tools/mpv-patches/0014, `--video-join`; same day): upstream mpv now pairs a second video
track with the first for Dolby Vision's enhancement layer (f_enhancement_pair: a dependent track the demuxer reads
along, decoded with the same hardware decoding, each frame attached to the first track's of the same time, and mapped
by vo_gpu_next). The patch makes a file's first two video tracks of the same size and codec such a pair, and
vo_gpu_next maps both VideoToolbox surfaces and draws them side by side into planes of the frame's own (the mapped
textures can be sampled, not blitted): the frame the sphere shader reads, as hstack's was, without the copy to memory
and back. The joined frame keeps the aspect view360 sets (--video-aspect-override, which each decoder applies to its own
picture), and main.lua keeps the window's size for it (auto-window-resize no: resized to the aspect it would change
the area view360 sets it from, and again). On the X5 sample (two 3840x3840 HEVC tracks, 25 fps): ~29 % of a core
against ~50 % joined in memory and 350-430 % through VapourSynth, no frame dropped; not held to the clock ~48 frames
a second with the view (~50 with no shader at all: the decoding and the join are the ceiling). An X5 records 8K at 30
at most and 5.7K60 is half the pixels: both fit. Not tried: three or more tracks, other cameras' files.

**What it needs: both lens tracks on the GPU.** Under `lavfi-complex` mpv gives no hardware decoding device to the
tracks' decoders, so today they are decoded with a copy to memory and joined there. Keeping them on the GPU needs an
mpv patch that hands both decoded tracks to libplacebo as textures, zero-copy.

Only the Mac app builds its own mpv today (`tools/build_mpv.sh`, in CI); the Windows app ships shinchiro's prebuilt
mpv, and on Linux the plugins go into the distribution's mpv. **Decided 2026-11-06: the Windows app builds its own
mpv too** (shinchiro's mpv-winbuild-cmake with our patches, cross-compiled in CI, x86-64 and arm64, pinned and kept up
with mpv as the Mac build is). It changes nothing for users: the app ships an mpv either way. Linux stays as it is
for now (shipping our own mpv there would change how the Linux version is delivered).

The patch's GPU side is per decoder: VideoToolbox on the Mac, D3D11VA on Windows (VAAPI and NVDEC if Linux ever
needs it). Written generally (through mpv's hwdec interop) it can also go to mpv upstream, where the limit ("decoders
not linked to vo_chain must not use the hwdec context") is a known one; taken, the distributions' mpv gets it too.
Until then Linux keeps the VapourSynth path for 360; our own renderer decodes the tracks itself and needs no patch,
if 360 is ever wanted there.

## Quality and speed

The same clip will run on an N100 at 6 W, a laptop that halves its GPU after half a minute at boost, an M-series Mac,
a desktop card, and a NAS serving two streams at once. So nearly everything that costs GPU time is a setting with a
default chosen per machine, not a constant. What exists now (quality.lua: when more than a tenth of the frames drop
for 2 s, playback pauses and offers lighter settings, the pick kept in gyroflow.conf as this computer's) stays the
mechanism; on the GPU it gets more knobs, a different order, and measurements instead of only dropped frames.

### What each knob costs

What a pass costs scales with one of three things, and that decides which knob helps where:

- **source pixels** (a 4K source: 8.3 M a frame, whatever the window): decoding, the conversion to RGB with chroma
  upsampling, any prefilter of the source;
- **output pixels** (the window's or the stream's size): the warp and its taps, chromatic, the edge composite, the look;
- **per frame**, fixed: the table lookup, the sphere's update, peak detection.

| Knob | Choices, best first | Costs | Note |
|---|---|---|---|
| Interpolation | EWA (Robidoux) · Lanczos4 · bicubic · bilinear | output px × taps: EWA up to hundreds, Lanczos 64, bicubic 16 (or 4 fetches as a B-spline, softer), bilinear 1 | The biggest single knob at a given size |
| Shrinking a large source | EWA · a prefiltered half-size source (one pass over the source) · widened taps · none (aliasing) | the prefilter: source px; widened taps: output px × 4 | Whenever the window is much smaller than the video (4K in a 1080p window, 8K 360). mpv's default downscaler cost the N100 half its frames (35 vs 75 fps) |
| Render size | the video's · display · 1080p · 720p, then mpv scales up | output px | Exists (`resolution`); on a 4K or 5K screen a factor of 4 or more |
| Frame rate | every frame · ≤ 30 · ≤ 15 | everything per frame | Exists (`fps_limit`); the most visible (judder), so last |
| Lens model | exact (per pixel, with the model's Newton loops) · a table made per clip | gopro, sony, insta360 and the GoPro digital lenses: 8–60 iterations per pixel; opencv_fisheye: none | The lens part depends only on the ray's direction in the camera, so a 2D table over directions, made once per clip (again if the lens changes per frame), replaces the loops with one fetch. The same for a lens correction below 100 % (a 6-step root solve per pixel today) |
| Rolling shutter | knots per frame (65 · 33 · 17); the two-step row lookup on or off | small | Mostly quality; off by itself when the clip has no readout time |
| Chromatic under water | inside the warp, every channel with the main kernel · red and blue bilinear · off | under water only: output px × 3 × taps, or × (taps + 2) | Exists as `chromatic_skip`. Never as a pass at the end (above); out of water it costs nothing |
| Edge fill | sphere · smooth · off; the sphere's resolution; written every frame or every n-th | the composite: output px; the sphere: per frame (its texels) | Exists (`edges`, `edge_fill_rate`); on the GPU the sphere may be cheap enough that the rate goes **(to try)** |
| Conversion before the warp | the warp reads the Y and CbCr planes and converts only what it samples · mpv converts the whole source to RGB and the warp reads that | the first: only the warp's taps; the second: source px with chroma upsampling | A design choice more than a setting, and the biggest saving on weak GPUs, which spend most of a frame on source-sized passes. Both allow the chromatic correction. In mpv it works as LUMA and CHROMA passes at the output size (libplacebo then converts at 1080p); it pays only with the geometry computed once for both ([gpu-render-spike.md](gpu-render-spike.md)) |
| HDR on an SDR screen | peak detection on · off | per frame, a compute pass | mpv's `hdr-compute-peak` |
| The stream (server) | 2160p · 1080p · 720p; how many streams | output px | The encoder (VDEnc) is fixed-function hardware and doesn't use the shader cores; streams share the shader cores |

Paused, none of this has to keep up: the paused frame is rendered again at the best settings (EWA, the exact lens,
full chromatic, the video's size), as `paused_full_size` does now.

### Choosing them

- **Presets, not twelve switches.** The menu's Quality keeps one list from Best to Lightest; each step is a set of the
  knobs above. What goes first, least visible first (the order to confirm on the N100 and a laptop **(to try)**): the
  sphere's rate → chromatic to bilinear → the lens to its table → the shrinking to widened taps → interpolation one
  step down (Lanczos → bicubic → bilinear) → render size → frame rate. Each knob can still be set in gyroflow.conf;
  one set by hand is left alone by the presets.
- **Measured, not guessed.** mpv reports each GPU pass's time for fresh frames (`vo-passes/fresh`: last, average and
  peak, per pass by its `//!DESC`). The plugin reads them while the clip plays, so it knows a frame's GPU time against
  the frame's interval, not only whether frames dropped, and what each of our passes costs on this machine. A
  machine's default is the best preset whose measured time leaves a margin (say 25 %) at the clip's rate and the
  window's size, measured again when either changes a lot (a 4K60 clip after 1080p30, the window moved to a 5K
  screen).
- **Down quickly, up slowly, never in the middle of a shot.** A small step down (a knob nobody will see change) is
  taken without pausing; before the frame rate or the size goes, it asks, as now. Back up only after some seconds with
  room to spare, and at a seek, a new clip or a pause, so the picture doesn't change sharpness while you watch. A
  laptop that slows after its boost settles on the step it can hold.
- **The server** picks per stream when the stream starts, from the measured cost and the streams already running, and
  lowers the render (not the encode, which is separate hardware) when another stream joins.

## Checking it

- **Against Gyroflow**: render chosen frames of clips with each lens model we play (DJI opencv_fisheye, GoPro with
  and without SuperView/HyperView, Insta360, Sony if there is one) through `cpu_undistort.rs` and through the shader,
  and compare (largest difference, PSNR), as view360.glsl is checked against its WGSL copy.
- **Against today's player**: the same frames through VapourSynth and through the shader, by `PTS`, on the NAS
  regression set.
- **Frame identity**: a clip with fast motion, played with seeks, pauses, frame steps and drops; any frame warped with
  its neighbour's motion shows as a jump **(to try: an automatic check comparing the shader's PTS against the decoded
  frame's)**.
- **Throughput**: the NAS (N100) and the M1 MacBook, 4K60 → 1080p and to the window's size.

## Steps

1. **Spike** (done 2026-11-06, [gpu-render-spike.md](gpu-render-spike.md)): the opencv_fisheye path of the kernel as an mpv hook with `PTS` and an embedded table
   written by a small CLI on vs-gyroflow; clip 0126 frame-exact against VapourSynth; knots vs rows; the table's load
   time; cost on the N100 and the M1, knob by knob (interpolations, shrinking, the plane-reading warp). Needs mpv 0.42 on the NAS (built in the container).
2. **The player** (parts 1 and 2 done 2026-11-07, [gpu-render-spike.md](gpu-render-spike.md): the geometry on a
   coarse grid, the plane passes, bicubic in 9 fetches, Gyroflow's lens models and digital lenses as GLSL): all lens models and digital lenses our cameras use, the interpolations, chromatic inside the warp,
   edges `off`; the VapourSynth path stays as the fallback (old mpv, DV looks on the Mac, the still frame).
3. **The edge fill** on the GPU (sphere and smooth).
4. **The renderer and streaming** on the NAS: VAAPI → libplacebo → VDEnc, HLS, Dolby Vision 8.4.
5. **The export** through the renderer; then the still frame, and maybe a denoise for stills and exports.
6. **360 on the Mac and Windows** (both with our own mpv): an mpv patch for both lens tracks as textures, the sphere made on the GPU; 8K60.

## Open questions

- The DV looks on the Mac: warp before the sample-buffer layer (patch 0002), or keep VapourSynth for them?
- Motion streaming with a table in a shader file: rewrite the file a few times, or patch mpv to update a texture
  (Mac only)?
- The edge fill: same look on the GPU, or rethought now that it can be done every frame at full rate?
- Gyroflow upstream: a full GLSL kernel is something Gyroflow itself could use (its Qt preview is a simplified one).
  Offer it once it works?
- Streaming: clients (TV, phone, browser), protocol, and whether a clip is streamed as it plays or rendered ahead.
