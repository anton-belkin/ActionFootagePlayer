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
  parallel; under a millisecond **(to try: measure)**, with a cache of 100 frames. Smoothing, horizon lock and the
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
| Chromatic (dispersion.rs): the lens geometry stamped into the last luma row; chromatic.glsl re-samples red and blue at the end, from a picture already warped and resampled | **Inside the warp pass**: the green ray refracted per channel (Snell's law with water's indices) and each channel sampled once from the source |
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

This differs from Gyroflow's nearest-row lookup **(to try: the largest difference in pixels over a fast clip, against
`cpu_undistort.rs`; 33 knots, 65, or the full rows if it matters)**. IBIS/OIS and breathing (Sony, some others) go
as further knot tracks only for clips that have them.

### Getting the numbers to the shader

**In mpv.** The shader takes `//!PARAM PTS` (mpv 0.42+, `video/out/gpu/utils.c`) and finds its frame in a table
texture. The table has to be a `//!TEXTURE` in a shader file, which mpv loads with the file, so per clip the plugin
writes a shader file (as dlogm writes its LUT shaders) with the clip's table embedded, and puts it in
`file-local-options/glsl-shaders`. Questions this raises:

- **Size and load time**: 10 MB of floats is 20 MB of hex for libplacebo to parse **(to try: the load time; the LUT
  shaders are 4.4 MB)**. Long clips (an hour at 60 fps: 125 MB) need windows: a table per few minutes, the next one
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

### Where the warp sits in mpv's chain

`MAINPRESUB`, as the 360 view does: after the conversion to RGB, before the looks, and at the size of the video's
rectangle on screen (`//!WIDTH OUTPUT.w`). The looks (dlogm's, at `MAIN`) then run on the output-sized picture, which
is cheaper than now (today they run at the stabilized size). No mpv scaling follows: the warp is the scaler.

Interpolation in the warp is Gyroflow's, and which one is a quality/performance setting, not a rule by frame rate
(see "Quality and speed"). Today main.lua's `interpolation` picks Lanczos up to 31 fps and bicubic above only
because on an M1 Pro through VapourSynth 4K Lanczos kept up at ~38 fps and bicubic at ~54: what that machine managed,
not something about the clip.

Where the warp shrinks the source a lot (4K into a 1080p window: two to one), a plain bicubic aliases; the prototype
widened its taps, Gyroflow's EWA is the exact answer but costs most. **(to try: those, and a prefiltered half-size
source, on detail like a reef; and their cost on the N100.)**

### Chromatic correction inside the warp

Today: the stabilized YUV goes through VapourSynth with one (green) refraction, mpv converts it to RGB, and
chromatic.glsl re-samples red and blue around the lens axis it read from the stamp: a second resampling of an image
already resampled once.

In the warp pass each output pixel has its ray; under water the green ray is refracted by water's index for green
(as now), and red and blue by theirs, and each channel is read from the source with its own coordinate: one
resampling, from the original pixels, with the exact lens model rather than the stamp's approximation. Cost: two
more coordinate computations per pixel and 2 × the taps for red and blue (or bilinear for them, which the eye won't
tell under water) **(to try)**. The export gets the same correction for free.

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
Writing the sphere every frame becomes cheap, so `fill_rate` goes. **(to try: sphere and fill passes at 1080p on the
N100.)**

### Later: temporal denoise and the still frame

A recursive temporal denoise as a shader already worked (shaders/tdenoise.glsl on the `temporal-denoise` branch:
−62 % noise on water at 4K60 on an M1 Pro), but Anton froze it for playback (the eye averages at 60 fps). Two things
make it different here: in the warp pass the motion between frames is known exactly from the gyro (no motion search
for the camera's own motion), and in our renderer a window of frames on both sides can be read. So the still frame,
and a denoise for screenshots and exports, can move into the renderer later. Not part of this work.

## Hosts and platforms

| Host | mpv | Status of `PTS` | Notes |
|---|---|---|---|
| Mac app | our build, pinned at mpv fba6342 with patches | has it | **The DV looks present through AVSampleBufferDisplayLayer (patch 0002), which runs no user shaders**: the warp would need to run before that layer gets its frame, a change to patch 0002, or the DV looks keep the VapourSynth path |
| Windows app | shinchiro's build | has it (the one we ship, 20261018 at mpv 01e2533, is after the change) | nothing else needed |
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
| Chromatic under water | red and blue with the main kernel · bilinear · off | output px × 2 × taps, or × 2 | Exists as `chromatic_skip`; bilinear for the two channels is the middle step the eye shouldn't tell under water **(to try)** |
| Edge fill | sphere · smooth · off; the sphere's resolution; written every frame or every n-th | the composite: output px; the sphere: per frame (its texels) | Exists (`edges`, `edge_fill_rate`); on the GPU the sphere may be cheap enough that the rate goes **(to try)** |
| Conversion before the warp | mpv converts the whole source to RGB and the warp reads that · the warp reads the Y and CbCr planes and converts only what it samples | the first: source px with chroma upsampling; the second: only the warp's taps | A design choice more than a setting, but the biggest saving on weak GPUs, which spend most of a frame on source-sized passes **(to try: whether a MAINPRESUB pass can bind LUMA and CHROMA, and whether libplacebo then skips converting MAIN)** |
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

1. **Spike** (a few days): the opencv_fisheye path of the kernel as an mpv hook with `PTS` and an embedded table
   written by a small CLI on vs-gyroflow; clip 0126 frame-exact against VapourSynth; knots vs rows; the table's load
   time; cost on the N100 and the M1, knob by knob (interpolations, shrinking, the plane-reading warp). Needs mpv 0.42 on the NAS (built in the container).
2. **The player**: all lens models and digital lenses our cameras use, the interpolations, chromatic inside the warp,
   edges `off`; the VapourSynth path stays as the fallback (old mpv, DV looks on the Mac, the still frame).
3. **The edge fill** on the GPU (sphere and smooth).
4. **The renderer and streaming** on the NAS: VAAPI → libplacebo → VDEnc, HLS, Dolby Vision 8.4.
5. **The export** through the renderer; then the still frame, and maybe a denoise for stills and exports.

## Open questions

- The DV looks on the Mac: warp before the sample-buffer layer (patch 0002), or keep VapourSynth for them?
- Motion streaming with a table in a shader file: rewrite the file a few times, or patch mpv to update a texture
  (Mac only)?
- The edge fill: same look on the GPU, or rethought now that it can be done every frame at full rate?
- Gyroflow upstream: a full GLSL kernel is something Gyroflow itself could use (its Qt preview is a simplified one).
  Offer it once it works?
- Streaming: clients (TV, phone, browser), protocol, and whether a clip is streamed as it plays or rendered ahead.
