# The GPU warp, step 1: the spike

What step 1 of [gpu-render.md](gpu-render.md) built and measured, 2026-11-06. The design is unchanged by it; the
"(to try)" items it answers are marked there with a pointer here.

## What was built

- **`shaders/gyroflow/warp.glsl`**: Gyroflow's per-frame warp as one mpv user-shader pass, a port of gyroflow-core's
  `undistort_coord` / `rotate_and_distort` (fork d130105): opencv_fisheye, full lens correction, no digital lens, the
  non-equirect output projections, the rolling shutter's two-step row lookup, refraction under water, a solid
  background. It hooks `MAIN` (on gpu-next the same as `MAINPRESUB`) at `OUTPUT.w`×`OUTPUT.h`, so it is the scaler.
  It takes `//!PARAM PTS` (mpv ≥ 0.42) and reads its frame from a table texture; no marks in the pixels. The
  interpolation is a define: bilinear, B-spline (4 bilinear fetches), Gyroflow's bicubic (Keys a = −0.75, 16 taps),
  Lanczos4 (64 taps), EWA Robidoux (Gyroflow's ellipse, up to 9×9 taps); `SHRINK` widens the bicubic's and Lanczos's
  taps where the warp shrinks the source (as view360.glsl does).
- **`vs-gyroflow/src/gpu_table.rs`**: the table from vs-gyroflow's engine (`load_manager`, as the filter loads a
  project): per frame 5 texels of parameters (f, c, k, fov, lens correction, refraction, field limit, translations,
  rows) and the row rotations as quaternion knots, evenly over the rows (nlerp between the two nearest in the shader),
  or every row (`--knots 0`). rgba32f, frames packed side by side up to 16384 texels wide.
- **`gyroflow-gpu-table`** (vs-gyroflow/src/bin): writes the shader file for a clip (the template with its table and
  defines); `--check` measures knots against every row on Gyroflow's CPU kernel.
- **A test** (`gpu_table::tests`, `AFP_GPU_CLIP=<project> cargo test --release gpu_table`): the GLSL through naga on
  the machine's GPU, no window, writing each output pixel's source coordinate, against `undistort_coord` at every
  pixel; and every interpolation compiles.

## Results

All on clip 0126 (Osmo Action 6, 4K60 10-bit D-Log M, its Gyroflow project: opencv_fisheye, full correction, no
readout time in the project), output 1920×1080, no look.

### The shader against Gyroflow: the same pixels

The test above, on the M1 Pro through naga/Metal, frames 0, 600, 1049 (the fastest turn), 1181 and 2375, with a
synthetic 10 ms readout so the rolling-shutter path runs: every one of the 2 073 600 output pixels lands within
**0.037 px** of `undistort_coord`'s source coordinate (mean 0.0003–0.001 px), with knots or with every row. That is
float noise: the port is exact.

### Frame identity: by PTS, against today's VapourSynth path

On the NAS (mpv 0.42.0, built in the container with libplacebo 7.387; the 0.9.0 plugin for the VapourSynth path),
paused at frames 600, 1049, 1181, 2000 and shot as shown (1920×1080), the shader's frame N against VapourSynth's
N−1, N, N+1 (PSNR, dB):

| Frame | VS N−1 | **VS N** | VS N+1 |
|---|---|---|---|
| 600 | 33.7 | **42.6** | 33.4 |
| 1049 | 17.7 | **33.4** | 17.4 |
| 1181 | 23.5 | **33.8** | 22.9 |
| 2000 | 20.5 | **34.7** | 20.6 |

The shader shows the right frame's motion every time. What is left of the difference at N is no offset (phase
correlation finds none; after a 2 px blur the PSNR is 39.6 dB on frame 1049) but fine texture everywhere: the two
paths resample differently at 2:1 (the shader: RGB from mpv's 4K conversion, Keys bicubic; VapourSynth: each YUV plane
on its own, then mpv's conversion at 1080p).

### Knots against every row

`gyroflow-gpu-table --check` on Gyroflow's CPU kernel, the largest difference over an 8 px grid of the output, in source
pixels. Clip 0126 has no readout time, so it was given 10 ms; a HERO7 flight clip (vibration, `~/Desktop/garbage/flight/GX011058.MP4`,
read only) 9 ms; both with their real motion:

| Knots | 0126 (≤ 0.75° over a frame) | HERO7 flight (≤ 1.06°) |
|---|---|---|
| 9 | 0.035 | 0.071 |
| 17 | 0.035 | 0.044 |
| 33 | 0.035 | **0.021** |
| 65 | 0.032 | 0.021 |

33 knots are at the noise floor (f32 matrices to quaternions and back): the knots question is settled. Computing a
frame's transforms (2160 rows, in parallel) takes 0.13–0.14 ms on the M1 Pro; the table for the 40 s clip is built in
0.3 s (5 ms without rolling shutter).

### Cost, knob by knob

mpv's per-pass GPU times (`vo-passes`, fresh frames, average / peak µs) while playing 20 s from 1 s in, hardware
decoding (VAAPI zero-copy on the NAS, VideoToolbox on the Mac), `--profile=fast`, fullscreen 1920×1080. "Decode" is
libplacebo's source-sized conversion pass (4K, with the chroma upsampling), "Encode" its output pass. The frame's
budget at 60 fps is 16.7 ms.

| Shader | N100: warp | N100: decode + encode | N100 shown fps (drops / 20 s) | M1 Pro: warp | M1 Pro: decode |
|---|---|---|---|---|---|
| none (mpv's bilinear downscale) | — | 1.8 | 59.9 (0) | — | 0.1 (with encode) |
| bilinear | **5.0** | 3.6 + 1.1 | 60.0 (0) | 1.6 | 0.8 |
| B-spline (4 fetches) | **6.2** | 3.6 + 1.1 | 59.9 (0) | 1.8 | 0.8 |
| bicubic, Gyroflow's (16 taps) | 10.9 | 4.4 + 1.0 | 56.0 (81) | 2.6 | 0.8 |
| bicubic, widened (shrink) | 23.8 | 4.0 + 1.0 | 32.4 (552) | 4.8 | 0.8 |
| Lanczos4 | 38.6 | 4.3 + 1.0 | 21.9 (764) | 8.0 | 0.7 |
| Lanczos4, widened | 82.3 | 5.4 + 1.0 | 11.0 (980) | 9.0 | 0.7 |
| EWA Robidoux | 42.4 | 4.7 + 1.0 | 19.8 (802) | 7.4 | 0.7 |
| bicubic + rolling shutter (33 knots) | 13.2 | 4.2 + 1.1 | 50.6 (186) | 3.2 | 0.8 |
| bicubic + rolling shutter, every row | 12.8 | 4.3 + 1.1 | 51.7 (165) | — | — |
| planes, bicubic (LUMA + CHROMA passes) | 10.6 + 10.2 | — + 1.4 | 42.2 (355) | 2.3 + 2.1 | — |
| planes, bilinear | 4.7 + 4.7 | — + 1.3 | 59.9 (0) | — | — |

- **The M1 Pro has room for every knob**: no drops anywhere, Lanczos4 or EWA at ~8 ms. (The Mac's "untimed" runs stayed
  at the display's 60 Hz, so only the pass times say how much room.)
- **The N100 fits bilinear and B-spline** with room (≈10–11 ms of 16.7), Gyroflow's bicubic only just not (16.3 ms,
  4 % dropped), nothing heavier. The prototype's 60 fps (gpu-render.md, "What was measured") was B-spline-like too.
- **On the N100 bilinear (one tap) already costs 5.0 ms**: not the lens math, as first thought, but the fetches per
  pixel: 5 parameter texels and 2 knots from the table (see "Step 2, part 1" below: without them the warp costs what
  sampling the source alone does). The rolling shutter's second projection adds 2.3 ms; knots against every row cost
  the same.
- **The plane-reading warp as two passes does not pay**: it saves the 4K conversion (≈4 ms) but each plane's pass
  does the whole geometry again (≈4.5 ms). LUMA and CHROMA hooks at the output size do work in mpv (frame 1049 through
  them: 53.8 dB against the single-pass shader's, 33.5 against VapourSynth's; libplacebo converts at 1080p and doesn't
  scale), so the shape is possible; it pays only if the geometry is
  done once.
- **Table load**: mpv to its first frame and out, 3 runs: no shader 0.86–0.96 s; the clip's 3.2 MB table 0.95–1.09 s
  (+0.1 s); a 165 MB table (the every-row table of this clip, about as large as an hour of 4K60 with knots) 3.2 s
  (+2.3 s). Windows of a few minutes, as the design says, keep it at a tenth of a second.

## What it means for step 2 (go)

Go: the port is exact, the frame is found by PTS on mpv 0.42 (the Mac app's mpv loads and runs it too), the table is small and loads
fast, and the Mac has room for the best settings. Two things change in the plan, both for weak GPUs like the N100:

1. **The geometry once, coarse, then the taps.** The warp's coordinates are smooth across the output (the lens and
   rotation change slowly from pixel to pixel), so a first pass can compute them on a coarse grid (say every 8th
   output pixel: 1/64 of the work) and the sampling pass interpolate them. The error of that against the exact
   coordinates is the thing to measure first **(to try)**; if it is a few hundredths of a pixel, the N100's bilinear
   warp drops from 5 ms to about 1, and Gyroflow's bicubic fits.
2. **Then the plane-reading warp**, with the coordinates from that pass: LUMA and CHROMA passes that only sample,
   which removes the 4K conversion (≈4 ms on the N100, 0.8 on the M1 Pro) for the cost of sampling chroma once more.

Interpolation defaults from these numbers (gpu-render.md, "Quality and speed"): bicubic on the M1 Pro and up (EWA or
Lanczos4 fit too), B-spline on the N100 until the coarse geometry lands. The widened taps cost as much as Lanczos4:
the shrinking question (4K into a 1080p window) needs the half-size prefilter measured instead **(to try)**.

## Step 2, part 1: the geometry on a coarse grid

Built 2026-11-07 (same branch). warp.glsl is now sections that gyroflow-gpu-table assembles into one of three layouts
(`--mode`): **exact** (the spike's single pass), **grid** (a compute pass writes each grid point's source coordinate
into an rgba32f storage texture, once a frame; a sampling pass at MAIN interpolates it) and **planes** (the grid, then
sampling passes at LUMA and CHROMA at the output size, so libplacebo converts at 1080p). A sampling pass reads the
table only where a grid point has no image (past the lens's field), where it falls back to the exact geometry.

**Accuracy.** On the CPU (`--check-grid`, every pixel of a 1080p output, the fastest frames of 0126 and the HERO7
flight clip with synthetic readouts): Catmull-Rom over the grid stays within 0.04 px of the exact coordinates even at a
32 px step; bilinear needs 8 px (≤ 0.06 px; 16 px gives 0.17). On the GPU (the naga test): grid 16 Catmull-Rom 0.034 px
from `undistort_coord` at every pixel, grid 8 bilinear 0.058 px. In mpv on the NAS, frame 1049: grid 16 against the
exact pass 67.5 dB (the same picture), grid 32 64.6 dB, planes 53.7 dB (chroma read once, at its own siting); passes
in order grid → luma → chroma → color decoding.

**Cost on the N100** (per-pass GPU times, µs average; the whole frame adds libplacebo's own passes):

| Layout | Grid | Warp | Frame total | Shown fps (drops / 20 s) |
|---|---|---|---|---|
| floor: one bilinear tap of MAIN, no geometry | — | 2120 | 7240 | 59.9 (0) |
| floor: one tap each of LUMA and CHROMA | — | 1180 + 1080 | 4100 | 59.9 (1) |
| exact, bilinear (stage 1) | — | 5000 | 9700 | 60.0 (0) |
| grid 16 Catmull-Rom, bilinear, table read per pixel | 40 | 6200 | 10850 | 60.0 (0) |
| grid 16 Catmull-Rom, bilinear, no table read | 39 | 5860 | 10740 | 59.9 (2) |
| **grid 8 bilinear, bilinear** | 95 | **2400** | **7330** | 59.9 (2) |
| **planes, grid 8 bilinear, bilinear** | 113 | **2180 + 2150** | **6130** | 60.0 (0) |
| exact, Gyroflow's bicubic (stage 1) | — | 10900 | 16340 | 55.8 (83) |
| grid 16 Catmull-Rom, bicubic | 40 | 11660 | 16890 | 54.6 (109) |
| grid 8 bilinear, bicubic | 100 | 8570 | 13580 | 58.4 (30) |
| planes, grid 16 Catmull-Rom, bicubic | 66 | 11950 + 11040 | 24360 | 38.3 (433) |

**On the M1 Pro** (same clip, the app's mpv, fullscreen 1920×1080): everything at 60 fps; the grid doesn't help there
(grid 16 Catmull-Rom bicubic 3.3 ms against exact 2.6; planes bicubic 3.1 + 3.0): its GPU does the lens math cheaply,
and 16 storage-image reads a pixel cost more. Not measured there yet: grid 8 bilinear.

What it shows: on the N100 a warp pass costs its **fetches**, not its math. Per pixel, every texel read counts: the 7
table fetches of the exact pass cost ~3 ms, 16 grid reads ~3.5, 16 bicubic taps ~6. With 4 grid reads and no table
reads the geometry costs 0.3 ms over sampling the source alone, and reading the planes saves the 4K conversion
(≈ 1.2 ms net at bilinear). So for step 2:

- **The default layout is planes with a grid every 8 px, bilinear** (0.06 px): 6.1 ms a frame on the N100 at
  bilinear, a third of the budget. Catmull-Rom over the grid stays as an option for larger steps.
- **Gyroflow's bicubic needs fewer fetches**: 16 taps cost 6 ms on the N100 (and twice in the planes layout). The
  9-fetch form (bilinear fetches combining the middle taps, as for Catmull-Rom) and bilinear chroma under bicubic luma
  are the next things to measure **(to try)**; B-spline (4 fetches) is the cheap smooth option.
- On the Mac the choice of layout hardly matters (all under 4 ms of 16.7); the M1 Pro's numbers for grid 8 bilinear
  are still to take.

## Not covered

- Chromatic correction, other lens models, digital lenses, edges: step 2 and 3.
- The dlogm look on top (the prototype ran with it; the look passes are cheap at 1080p).
- The Mac's frame-identity check through the app's mpv: only the NAS's mpv 0.42.0 was checked; on the Mac the shaders
  loaded and ran with no errors in the log, but no frame was shot or compared. The DV looks (patch 0002) still run no user shaders.
- Seeks, frame steps and drops with a fast clip: the shaders find frames by PTS, which the paused seeks above exercised;
  playback with drops was watched only through the drop counters.

## Reproducing

- Shaders: `gyroflow-gpu-table [--interp …] [--shrink] [--readout-ms t] [--knots n] <project> <out.glsl>`.
- `tools/gpu-spike/`: the NAS image (Dockerfile: mpv 0.42.0 + libplacebo 7.387 on the afp-test image), `check.sh`
  (frame identity), `bench.sh` (per-knob timing), `loadtime.sh`, `mac_bench.sh` (the app's mpv; opens a player
  window). The plane variants were made from the bicubic and bilinear files by a script (two passes, LUMA and CHROMA,
  `PLANE_OFFSET` 0.25 for MPEG-2 chroma), not by the CLI.

