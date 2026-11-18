# edgefill: the sphere edge fill offscreen, before and after

What the sphere's rework (docs/gpu-render.md, "The sphere, 2026-11-18") was checked with. No window: a clip's frames go
through libplacebo with the shader gyroflow-export's GPU path writes for it (the player's warp, edge fill and look), at
a window's size.

- `afpchain.c`: tools/fsrbench's renderer (PR #244), plus `AFP_RAW_OUT` (every frame out, for clips). Build:
  `cc -O2 -o afpchain afpchain.c $(pkg-config --cflags --libs libplacebo)` (Homebrew libplacebo + MoltenVK).
- The shader: `AFP_GPU_KEEP=1 gyroflow-export --gpu on --edges sphere --wide --sdr --from <t> --to <t+0.2> --output x.mp4
  <clip>` keeps `.x.partial.glsl` next to the output (ffmpeg with libplacebo: `AFP_GPU_FFMPEG`, the app's own
  `Contents/Helpers/ffmpeg`). Build gyroflow-export from each branch for before and after.
- `chain.sh <shader> <clip> <start> <frames> <W> <H> - -`: each pass's GPU time per render (mean, p95, max; the first 20
  renders left out) and WALL, the render to the GPU done; `chain.sh ... <prefix> 30,60` saves those frames as rgba64le.
- `clip.sh <shader> <clip> <start> <frames> <W> <H> out.mp4`: the whole render as a clip, for side-by-sides.

Debugging: replace `sphere_fill`'s last line in the kept shader with `return vec4(vec3(ws), 1.0);` (the sphere's
freshness: holes and the pole show) or `return vec4(sum / ws, 1.0);` (the sphere alone).
