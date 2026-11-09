# Черновик комментария в gyroflow/gyroflow#635 (не отправлен)

Куда: https://github.com/gyroflow/gyroflow/issues/635 — комментарий. Перед отправкой вставить ссылку на видео вместо `<video>`.

---

Real-time Gyroflow stabilization, entirely on the GPU — it runs even on a small NAS.

I've ported gyroflow-core's undistort kernel to an mpv/libplacebo user shader (GLSL) and use it in
ActionFootagePlayer, an mpv-based player for action-camera footage. Gyroflow does the motion, smoothing and lens
work on the CPU as usual; the per-pixel warp runs as a shader and the frame never leaves the GPU.

**On a UGREEN DXP2800 NAS (Intel N100, 6 W iGPU):** 4K60 10-bit HEVC (Osmo Action 6, D-Log M) → stabilized 1080p
HDR at 60 fps with Gyroflow's bicubic, 7.9 ms of the 16.7 ms frame budget, in mpv. The same box serves it as a
live HLS stream (VAAPI decode → the warp in libplacebo → VDEnc HEVC Main10, HLG) at 67-69 fps with bilinear
sampling (59 fps with the bicubic), so a TV or phone plays any clip of the library stabilized while it's rendered.

**On a Mac (M1 Pro):** 4K60 stabilized at 34 % CPU (86 % through a VapourSynth filter with gyroflow-core); a clip
opens stabilized in about a second, also when it's read straight from the NAS.

How it fits on a 6 W GPU:
- Accuracy: every pixel of a 1080p output is within 0.037 px of `undistort_coord` (opencv_fisheye and gopro
  checked; your lens models' GLSL, the GoPro digital lenses, rolling shutter and refraction are in).
- Per frame the shader reads a small table: the `KernelParams` fields and the row rotations as 33 quaternion knots
  (within 0.021 px of per-row matrices on a vibrating HERO7 clip), ~600 bytes a frame. mpv ≥ 0.42 gives the shader
  the frame's PTS, so it finds its own row.
- On that iGPU a pass costs its texture fetches, not its math: the geometry is computed on an 8 px grid (≤ 0.06 px
  from exact; the GoPro/Insta360 Newton loops run only at grid points) and the bicubic uses 9 bilinear fetches
  instead of 16.

Video: <video>. Code (GPL-3.0, a derivative of gyroflow-core): `shaders/gyroflow/warp.glsl` and
`vs-gyroflow/src/gpu_table.rs` in https://github.com/anton-belkin/ActionFootagePlayer; measurements in
`docs/gpu-render-spike.md`. Built with Claude Code; every number here is measured, and the per-pixel check is a
test in the repo. Shared for information — Not a Contribution under the CLA.

Separately, two changes to gyroflow-core itself are up as PRs: #1289 (view_rotation, a pan after the smoothing)
and #1290 (equirect output + a rig lens's pose, re #800).

---

## Discord, #dev-live-stabilization (одна строка, после комментария)

Hi Adrian — posted real-time numbers for #635 (Gyroflow's warp as an mpv/libplacebo shader, 4K60 → 1080p HDR at
60 fps on an N100 NAS): <link to the comment>. Not a Contribution, just for info.
