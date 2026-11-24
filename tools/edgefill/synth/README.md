# synth: the sphere edge fill on synthetic scenes, the shader against a plain model

Anton's idea (2026-11-19): simple camera paths along one axis, where what the edges should look like can be worked
out another way, then the shader on the same pictures. Where the model looks wrong too, the rule is wrong; where only
the shader does, the shader is.

- `synth.py`: the geometry the shader uses (a pinhole source camera, the lens model's branch with no coefficients; a
  rectilinear output zoomed out 1.4x like `--wide`), an analytic world (sky, horizon, sea with fine waves, a post every
  10 degrees; and an underwater one for cuts), the scenarios (view yaw/pitch, camera yaw/pitch, world, gain per frame),
  the frames the camera sees, and the table the shader reads its cameras from (`make_shader`: a kept shader with that
  table in place of its clip's, the look dropped, the pass count in PTS fixed).
- `model.py`: the sphere's rule done the plain way: each texel of the sphere the output shows, each frame, from its
  own direction (no pixel lattice, no ownership). Its continuation is the shader's smooth fill (pull-push on the
  output's grid), so the two can be compared texel for texel. `DEFAULTS` holds the shader's rule (v4: split memory,
  a 512 sphere, the frame written hard, a changed sector's continuation out to the point behind the camera); the other
  keys try other rules (v3's: `SW=2048 SOFT=0.10 SOFT_IN=0.005 MEMORY=v3 REACH=falloff`; a ring of continuation
  around the frame: `RING`, `RING_FADE`, `RING_GATE`).
- `run.py`: a scenario through afpchain (offscreen) and the model on the same frames; per scenario a video and a sheet
  (top: shader | model; bottom: the world as it is | the difference x4) and the mean differences and flicker in the
  fill. `--no-shader --set KEY=VALUE` puts the default model where the shader was (`--base KEY=VALUE` changes it) and
  the changed one beside it.

```
python3 run.py --template <.x.partial.glsl kept by gyroflow-export> --afpchain ../afpchain --out <dir> [scenario ...]
python3 run.py ... --no-shader --base MEMORY=v3 --set RING=0.08 shake_stop
```

The template: `AFP_GPU_KEEP=1 gyroflow-export --gpu on --edges sphere --wide --sdr --from 1 --to 1.2 --output x.mp4
<any clip>` (see ../README.md). Its SRC/OUT sizes stay (the frames are fed at 1280x720 and the shader scales).

Results 2026-11-19 (v4): shader and model 0.0015-0.004 apart in the fill on all 18 scenarios. Not covered yet: lens
distortion and the field limit, rolling shutter, seeks, the VapourSynth path.
