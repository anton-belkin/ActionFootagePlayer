# Комментарий в gyroflow/gyroflow#635 — опубликован 2026-11-09: https://github.com/gyroflow/gyroflow/issues/635#issuecomment-6740037218

Куда: https://github.com/gyroflow/gyroflow/issues/635 — комментарий. Без видео (решение Антона, 2026-11-09).

---

Real-time Gyroflow stabilization, entirely on the GPU — it runs even on a small NAS.

I've ported gyroflow-core's undistort kernel to an mpv/libplacebo user shader (GLSL) and use it in
[ActionFootagePlayer](https://actionfootageplayer.ch), an open-source (GPL-3.0) mpv-based player for action-camera
footage. Gyroflow does
the motion, smoothing and lens work on the CPU as usual; the per-pixel warp runs as a shader and the frame never leaves
the GPU.

**On a UGREEN DXP2800 NAS (Intel N100, 6 W iGPU):** 4K60 10-bit HEVC (Osmo Action 6, D-Log M) → stabilized 1080p
HDR at 60 fps with Gyroflow's bicubic, 7.9 ms of the 16.7 ms frame budget, in mpv. The same box serves it as a
live HLS stream (VAAPI decode → the warp in libplacebo → VDEnc HEVC Main10, HLG) at 67-69 fps with bilinear
sampling (59 fps with the bicubic), so a TV or phone plays any clip of the library stabilized while it's rendered.

**On a Mac (M1 Pro):** 4K60 stabilized at 34 % CPU, the warp taking 0.6 ms of GPU time a frame; a clip opens
stabilized in about a second, also when it's read straight from the NAS.

**Live, without reading the whole file first.** You open a video straight off the camera's card — no `.gyroflow`
project made beforehand, the camera's own gyro data and a default preset — and it plays stabilized once the first
~4 s of that gyro data are parsed; the rest is parsed while it plays, the way a live feed would arrive:
- telemetry-parser gets a `Read + Seek` over the file in which the metadata samples not read yet are zeros, which it
  skips, so gyroflow-core's normal loader sees only the seconds read so far. The samples are fetched in parallel from
  the sample table instead of one seek per frame (over SMB that alone took a minute of 4K60 from 14-34 s to under a
  second), and while it plays they're copied from the OS file cache, the pages mpv already read for the video, every
  2 s, then parsed and smoothed again.
- The smoothing is your Default made local in time (a `SmoothingAlgorithm` of its own): the second pass measures the
  distance against the crop margin instead of the clip-wide maximum, so each moment depends only on the few seconds
  around it. With 4 s of look-ahead it stays within 1 px (p99) of the same smoothing run on the whole clip.
- All of it is on gyroflow-core's public API, no patches. From the NAS a clip opens stabilized in ~1.2 s instead of
  3-7 s.

How the warp fits on a 6 W GPU:
- Accuracy: every pixel of a 1080p output is within 0.037 px of `undistort_coord` (opencv_fisheye and gopro
  checked; your lens models' GLSL, the GoPro digital lenses, rolling shutter and refraction are in).
- Per frame the shader reads a small table: the `KernelParams` fields and the row rotations as 33 quaternion knots
  (within 0.021 px of per-row matrices on a vibrating HERO7 clip), ~600 bytes a frame. mpv ≥ 0.42 gives the shader
  the frame's PTS, so it finds its own row.
- On that iGPU a pass costs its texture fetches, not its math: the geometry is computed on an 8 px grid (≤ 0.06 px
  from exact; the GoPro/Insta360 Newton loops run only at grid points) and the bicubic uses 9 bilinear fetches
  instead of 16.

Code (GPL-3.0, a derivative of gyroflow-core) in https://github.com/anton-belkin/ActionFootagePlayer:
`shaders/gyroflow/warp.glsl` and `vs-gyroflow/src/gpu_table.rs` for the warp, `vs-gyroflow/src/motion.rs`,
`prefetch.rs` and `smoothing.rs` for the live loading; measurements in `docs/gpu-render-spike.md`. Built with Claude
Code; every number here is measured, and the per-pixel check is a test in the repo. Shared for information — Not a
Contribution under the CLA.

Separately, two changes to gyroflow-core itself are up as PRs: #1289 (view_rotation, a pan after the smoothing)
and #1290 (equirect output + a rig lens's pose, re #800).

---

## Discord, #dev-live-stabilization (одна строка, после комментария)

Hi Adrian — posted real-time numbers for #635 (Gyroflow's warp as an mpv/libplacebo shader, 4K60 → 1080p HDR at
60 fps on an N100 NAS, starting from the first seconds of motion data): https://github.com/gyroflow/gyroflow/issues/635#issuecomment-6740037218. Not a Contribution,
just for info.
