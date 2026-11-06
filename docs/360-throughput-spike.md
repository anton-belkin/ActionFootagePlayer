# 360°: the throughput spike

Step 2 of [the plan](360-implementation-plan.md): whether two lens tracks joined by mpv's `--lavfi-complex`
`hstack` and passed through VapourSynth keep up, before the plugin's 360 mode is built on that route
([the architecture](360-cameras.md#throughput)). Measured only: no product code changed.

Written 2026-11-06, on Anton's MacBook Pro (M1 Pro, 10 cores, 16 GB), macOS 27, with the app's own mpv (v0.41.0
with our patches, FFmpeg 9.0.2, VapourSynth R81) from ActionFootagePlayer 0.7.0-dev.44.

## The answer

- **5.7K30 keeps up with room to spare.** On the screen it plays at 30 fps with no drops, the output at 8192×4096
  included. Without the screen the chain does ~70 fps, 2.3× real time.
- **8K30 keeps up, but just.** On the screen it plays at 30 fps, but one run in two dropped 60–100 frames in 15 s.
  Without the screen it does ~40–50 fps, and ~22 when the kernel is busy with memory (see
  [memory](#memory-the-other-limit)). The real plugin adds copies of its own (an upload and a read-back) and its
  GPU time, which this stand-in doesn't have.
- **8K60 doesn't keep up.** Without the screen VapourSynth tops out at ~55 fps, even when it hands the frame back
  without a copy. On the screen the chain delivers 40–50 fps, but the vo then drops most of them: **9–10 fps
  shown**. Joining the tracks alone reaches 60 on the screen, with the GPU at 91%.
- **The two tracks stay in step.** Every frame read had both lenses on the same frame number, 3,483 frames over
  six runs: keyframe and exact seeks, relative seeks back and forth, frame steps and back steps; with and without
  the screen.
- **A script can set `lavfi-complex` per file**, and it fits with the `vf` chain `gyroflow/main.lua` builds. One
  thing must change in its 360 mode: `hwdec=videotoolbox` (which `main.lua` sets for `scale_vt`) silently turns
  into **software decoding** under `lavfi-complex` (see [below](#lavfi-complex-from-a-script)).

**Recommendation: VapourSynth as it is for step 4**, on 5.7K and 8K30 clips, with the three changes under
[what step 4 needs](#what-step-4-needs). **No `hstack_vt` before it.** A GPU join alone wouldn't make 8K60 fit:
it saves one copy of the six, about 4 ms a frame. It also needs an mpv patch anyway, because decoders under
`lavfi-complex` can't get hardware frames. 8K60 is played at half rate (every other frame, `fps_limit`) until an
8K60 camera is on the table. Then the route to choose is one where the frames stay on the GPU from the decoder to
the screen (both tracks as textures to libplacebo, the render in the shader), not a faster way into VapourSynth.

## Results

### Without the screen: what the chain can do

`--vo=null --untimed`, `hwdec=videotoolbox-copy`; fps through the chain, the median of three runs (the best in
brackets). The work added at each row is on top of the row above.

| chain                                         | 5.7K30      | 8K30                 | 8K60        |
|-----------------------------------------------|-------------|----------------------|-------------|
| one track decoded (no `lavfi-complex`)        | 234 (242)   | 173 (180)            | 225 (226)   |
| both decoded, the second thrown away          | 115 (117)   | 86 (86)              |             |
| both joined, `hstack`                         | 91 (93)     | 65 (65)              | 73 (75)     |
| + VapourSynth, the frame handed back          | 74 (76)     | 26 (41)              | 56 (57)     |
| + a copy at the input's size                  | 73 (74)     | 25 (38)              | 55 (55)     |
| + a copy into 8192×4096                       | 68 (71)     | 22 (23); 40–50 rerun | 52 (54)     |
| the same, VapourSynth's cache at 1 GB         |             | 38 (39)              | 41 (42)     |

mpv's CPU with VapourSynth was 1.9–2.6 cores of 10, and the GPU idle, so none of it is short of cores. The chain is
**a line of copies of 50–100 MB each**, several on one thread: decoder → download, `hstack`, mpv's conversion
p010 → yuv420p10 (VapourSynth has no semi-planar formats), the copy into VapourSynth's frame, the plugin's copy,
the copy back out (`vf_vapoursynth.c`: `mp_image_copy` in, `mp_image_new_copy` out), and on the screen the upload.

Memory traffic, counted from those seven copies (10-bit samples are 16 bits in memory: a 3840² lens is 44 MB, both
joined 88 MB, an 8192×4096 frame 101 MB). An 8K frame out at 8192×4096 writes ~0.66 GB and moves ~1.3 GB, read and
write together. That's **~39 GB/s at 30 fps and ~79 GB/s at 60**. At 5.7K it's ~0.5 GB a frame, ~30 GB/s. The
M1 Pro has ~200 GB/s, so the total isn't the wall. The wall is that several of these copies run one after another
on one thread, and that the kernel has to supply fresh pages for them. The architecture's estimate (~8 GB/s at
8K30) counted the plugin's copies only, not mpv's. By the table:

- the hardware decoder isn't the limit: both 8K tracks decode and download at 86 fps;
- `hstack` costs ~4 ms a frame at 8K (86 → 65 fps), ~2.3 ms at 5.7K;
- getting into VapourSynth and out again costs ~4 ms more at 8K60 (73 → 56). The copy inside VapourSynth is
  nearly free next to that;
- the 8K30 rows with VapourSynth were very noisy: the same run did 22, 38, 48 or 50 fps, from one minute to the
  next, with the system's CPU time anywhere from 6% to 60% (the kernel's). With a fixed 300 frames, two runs in a
  row did 33 and 39 fps. The 8K60 clip has the same frame size and was steadier, for reasons these runs don't
  show; the slow runs are the ones where the kernel was busy with [memory](#memory-the-other-limit).

### On the screen: what playback gets

gpu-next, full screen on the built-in XDR display (3024×1964), at the clip's rate; 15 s measured, two runs each.
"Shown" is frames a second not dropped; drops are over the 15 s.

| chain                           | shown fps       | drops      | mpv CPU (cores) | GPU %  | mpv memory |
|---------------------------------|-----------------|------------|-----------------|--------|------------|
| 5.7K30 `hstack`                 | 30.0 / 30.0     | 0 / 0      | 0.36            | 56–64  | 1.3 GB     |
| 5.7K30 + VS, input's size       | 30.0 / 29.9     | 0 / 0      | 0.83            | 56–62  | 3.6 GB     |
| 5.7K30 + VS, 8192×4096          | 30.0 / 30.0     | 0 / 0      | 1.27            | 68–69  | 6.3 GB     |
| 8K30 `hstack`                   | 30.0 / 30.0     | 0 / 0      | 0.56            | 63–64  | 2.0 GB     |
| 8K30 + VS, input's size         | 22.5 / 29.9     | 103 / 1    | 1.67–2.19       | 65–67  | 6.8 GB     |
| 8K30 + VS, 8192×4096            | 25.9 / 30.0     | 60 / 0     | 1.56–2.62       | 65–71  | 7.0 GB     |
| 8K60 `hstack`                   | 60.0 / 53.6     | 0 / 90     | 1.33            | 91–92  | 2.4 GB     |
| 8K60 + VS, input's size         | 10.5 / 10.7     | 594 / 575  | 2.17            | 69–70  | 6.6 GB     |
| 8K60 + VS, 8192×4096            | 8.9 / 8.8       | 502 / 470  | 2.6             | 67–71  | 7.2 GB     |

- At 8K60 through VapourSynth the chain itself still delivered 40–51 fps (0.7–0.85× real time), but the vo got
  them late and dropped most: **a chain that is a little too slow plays very badly**, not a little worse. So 8K60
  needs a lower rate up front (`fps_limit`), not a hope that it nearly fits.
- The 8K30 drops came in the runs where the kernel's CPU time jumped (34–37% of the Mac, against 9–15% in the
  clean runs), the same thing as without the screen.
- The GPU is 56–70% busy at 30 fps with no shader of ours, and 91% at 8K60 with the join alone: uploading
  50–100 MB frames and scaling them to the screen. The real chain adds the plugin's kernel (per lens) and the view
  shader. That's a budget to watch at 8K60 whatever the route.

### Memory: the other limit

mpv grew to **5–7 GB** with VapourSynth in the chain (1–2.4 GB without), on a 16 GB Mac. Most of it is
VapourSynth's frame cache: 8 GB by default at R81, ~90 frames of 8K, which `gyroflow.vpy` doesn't set either. The
system then had 4–5 GB compressed. The runs that went slow are the ones where the kernel's time went up. Capping
the cache at 1 GB (`core.max_cache_size`) took ~1.2 GB off and made 8K30 steady (38–39 fps, the kernel at 9–12%),
but cost 8K60 ~20% (53 → 41 fps). Big frames that are allocated, copied once and dropped are what the kernel ends
up zero-filling and compressing.

### The two tracks in step

`copy.vpy` read each frame's number from both lenses' barcodes (`matrix.sh sync`). It ran through this sequence:
a 2 s play, then a keyframe seek to 8 s, an exact seek to 17.4 s, a seek to 3 s, +6 s, −7 s exact, an exact seek
to 20.25 s, 1 s, two frame steps, a frame back step, 12 s, each followed by 2 s of playback. It ran on all three
clips, without the screen and with it. **The lenses were never apart: 0 of 3,483 frames.**

One cost shows in the same logs. An exact seek runs every frame from the keyframe before it through VapourSynth
(17.4 s on the 60 fps clip: frames 1020–1043 rendered and thrown away), so it costs up to a second of 8K frames.
Real clips have a keyframe every 0.5–1 s, so that's what an exact seek costs on top of the frame itself; the
plugin could skip rendering frames mpv will throw away, if mpv told it which they are.

### `lavfi-complex` from a script

- **Per file, from a hook.** `mp.set_property("file-local-options/lavfi-complex", "[vid1][vid2]hstack=inputs=2[vo]")`
  works in `on_preloaded` (where the tracks are known) and in `on_load`, where `main.lua` sets its `vf` (the
  labels are resolved when the decoders start, so the track list needn't be known yet: the 360 decision can come
  from `gyroflow-probe`, as the rest of `main.lua`'s does). Being file-local, it's gone for the next clip: a plain
  clip after a 360 one plays with no `lavfi-complex` and no leftover `vf`.
- **The `vf` chain is unchanged by it.** `vf` filters run after the graph's `[vo]` as usual. Setting `vf` again in
  the middle of playback (as `rebuild_filter` does) rebuilds only the `vf` chain: the decoders and the graph stay,
  and the two tracks stay in step.
- **`hwdec=videotoolbox` becomes software decoding.** mpv gives the VO's hardware device only to the decoder of a
  track linked to the VO (`player/video.c`, `init_video_decoder`: "decoders not linked to vo_chain must not use
  the hwdec context"). Under `lavfi-complex` neither track is linked to the VO, so zero-copy VideoToolbox can't
  create its device and mpv falls back to software decoding, with only a verbose line in the log. The copy modes
  make a device of their own and work. So in 360 mode `main.lua` must set `videotoolbox-copy` (`auto-copy`
  elsewhere) and **not** put `scale_vt` in the `vf` chain (it would get frames in memory). If the input is
  scaled, it's done in VapourSynth or by the plugin. The same rule shuts `hstack_vt` out: no hardware frames ever
  reach the graph unless mpv is patched.
- Track switching (`vid`) has no effect while `lavfi-complex` is set (mpv's documentation; not tried): the 360
  mode should hide that menu entry.

## What step 4 needs

1. **`hwdec=videotoolbox-copy` in 360 mode, no `scale_vt`**: otherwise software decoding of two 8K streams.
2. **A cap on VapourSynth's frame cache** for 360 clips (`core.max_cache_size` in `gyroflow.vpy`), around 1–2 GB.
   Measure it again with the real plugin, which keeps its own frames too: 1 GB steadied 8K30 and slowed 8K60.
3. **8K60 at half rate** (`fps_limit` / the `framestep` that `main.lua` already has) by default, until the GPU
   route below exists. A chain a little too slow drops to ~9 fps shown.

And to measure again, with the plugin's real 360 mode, on the same clips (`tools/spike360/matrix.sh`): its upload
and read-back of 90–100 MB frames and its kernel time come on top of everything here.

## If 8K60 is wanted later

Every route that keeps VapourSynth leaves at least four full-frame copies at 8K60 (download, conversion, into
VapourSynth, out of it), plus the plugin's upload and read-back. The 8K60 rows show that four is already too many.
So:

- **`hstack_vt` (Metal, our own ffmpeg build) alone: no.** It saves the `hstack` copy (~4 ms of ~19), and it can't
  work without an mpv patch too, since hardware frames never reach `lavfi-complex` (above).
- **The frames kept on the GPU: yes, when it's needed.** An mpv patch that hands both decoded tracks to libplacebo
  as textures (zero-copy VideoToolbox, `IOSurface` → Metal/Vulkan), and the per-lens render done in a shader at the
  video's rate. That is the all-in-shader option of [the architecture](360-cameras.md), with the lens models
  ported to GLSL. It's a bigger change than step 4, and only worth it for 8K60.

## How it was measured

Scripts in `tools/spike360/`:

- **`make_clips.sh`**: the synthetic clips, each an MP4 with two HEVC Main10 video tracks encoded by VideoToolbox
  (`hevc_videotoolbox`), 25 s, a keyframe every second:

  | clip     | tracks        | rate   | bitrate (both) | as                                   |
  |----------|---------------|--------|----------------|--------------------------------------|
  | `5k7-30` | 2 × 2880²     | 30 fps | 120 Mbit/s     | 5.7K: X3, X4                          |
  | `8k-30`  | 2 × 3840²     | 30 fps | 160 Mbit/s     | 8K: X5, Osmo 360                      |
  | `8k-60`  | 2 × 3840²     | 60 fps | 180 Mbit/s     | 8K60: the next ones (X6, Osmo 360 II) |

  The picture is `testsrc2` under temporal noise, so the encoder spends the bits a real clip gets (the bitrates
  above are what came out); the second lens is another hue. Each frame carries its number as a 32-block barcode in
  each lens.
- **`measure.lua`**: sets the chain up per file, from the script, the way the plugin's 360 mode would:
  `file-local-options/lavfi-complex` = `[vid1][vid2]hstack=inputs=2[vo]` in the `on_preloaded` hook, then
  `vf=vapoursynth` (with `concurrent-frames=4`, as `gyroflow/main.lua` sets it) appended to `vf`. It samples mpv's
  counters every 0.5 s, or, for the sync check, seeks around. Its other joins: `both` (both tracks decoded, the
  second thrown away: `hstack`'s own cost is the difference) and `none` (one track, no `lavfi-complex`).
- **`copy.vpy`**: stands in for the plugin, with stock VapourSynth functions: `std.FlipVertical` (one plain copy
  into a new frame of the input's size), or `std.AddBorders` out to **8192×4096**, the sphere's size (one copy and
  the borders filled), or the frame handed back with no copy. With a log file it also reads both lenses' barcodes.
  (`std.Expr` was the first choice for "a copy", but on arm64 it has no JIT: 8 fps on a 7680×3840 frame alone.)
- **`run.sh`**: one run, with `hwdec=videotoolbox-copy`; mpv's CPU and memory and the system's from `top`, and the
  GPU's utilization from `ioreg` (`Device Utilization %`), every second. Either **without the screen**
  (`--vo=null --untimed`: the chain as fast as it goes, so what decoding, joining and VapourSynth can do) or **on
  the screen** (gpu-next, full screen on the built-in XDR display, at the clip's rate: what playback gets,
  uploading the output to the GPU included). Before each run it waits for the Mac to be 85% idle: other sessions
  were building and testing meanwhile.
- **`matrix.sh`**, **`summarize.py`**: the runs below and their table.

The perf harness (`tools/perf` on `perf-harness`) drives the whole app through the gyroflow scenario (views, looks,
seeks); this needs the bare chain with no plugin, so the spike has its own small runner on the same idea.

Memory traffic: Apple Silicon has no counters for it without root (`powermetrics` doesn't report DRAM bandwidth on
the M1 either), so it's counted from the copies the chain makes, below, and checked against the CPU time they take.

## Running it again

With the real plugin's 360 mode in place of `copy.vpy` (or on another Mac), from the repository:

```bash
tools/spike360/make_clips.sh ~/Movies/spike360
```

```bash
REPEAT=3 tools/spike360/matrix.sh ~/Movies/spike360 ~/Movies/spike360/results null
```

```bash
REPEAT=2 tools/spike360/matrix.sh ~/Movies/spike360 ~/Movies/spike360/results screen
```

```bash
tools/spike360/matrix.sh ~/Movies/spike360 ~/Movies/spike360/sync sync
```

The `screen` and `sync` runs take the built-in display in full screen for ~10 minutes; the clips are ~1.4 GB
together, to be deleted after.
