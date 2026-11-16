# fsrbench: the GPU chain offscreen, for FSR's cost and pictures

What `docs/gpu-render.md` ("The upscale to the screen: FSR 1") was measured with. No window, nothing on screen: a
clip's frames go through libplacebo with the player's own shaders (the warp `gyroflow-gpu-table --player` writes for
the clip, the look, `mpv/scripts/gyroflow/fsr.glsl`) into a target the size of the video's rectangle on screen, as
mpv's gpu-next does, and each pass's GPU time per render is printed (mean, p95, max), with the time from a render's
start to the GPU done with it.

- `afpchain.c`: the renderer. Build on macOS with Homebrew's libplacebo (MoltenVK):
  `cc -O2 -o afpchain afpchain.c $(pkg-config --cflags --libs libplacebo)`. Usage in its header.
- `run.sh`: the render sizes of the doc's table (the screen's, 0.8 of it, 1080, 720 on the short side), each with FSR
  and with mpv's own scaler (Lanczos in sigmoid light, mpv's default), with and without the denoise (Light: 3 frames
  on each side). Saves frame 30 of each as rgba64le.

The warp for a clip as the player plays it: the player without a window says what it would ask
(`--script-opts-append=gyroflow-render_args=<file>`, as tools/stream does); take `render.user_data` from it, drop
`still_control`, `edges_control`, `workers`, `plugin` and `lookahead`, add `"gpu_denoise": false` (or `true`) and
`"gpu_translation": false`, and run `gyroflow-gpu-table --player '<that JSON>' warp.glsl`. Run the player on a copy
of the clip, not the original's folder.

Limits: no decoding on the GPU (the frames come from ffmpeg as yuv420p10le), no presentation, no OSD, no other
process's work held off: what it measures is the passes. With the denoise's ~60 passes MoltenVK reports some passes'
time as 0, so for those runs the time to the GPU done (WALL) is the one to read; it also takes the CPU's dispatch.
