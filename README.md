# vs-gyroflow

Gyroflow stabilization as a VapourSynth filter, so mpv plays a video stabilized, in realtime, when its
`.gyroflow` project sits next to it.

```python
core.gyroflow.Stabilize(clip, project="/path/clip.gyroflow")
```

| Argument | Default | |
|---|---|---|
| `project` | — | `.gyroflow` project file, or the video itself: the project embedded in it, else its own motion data with Gyroflow's default preset |
| `preset` | — | `.gyroflow` preset applied on top when the video has no project |
| `interpolation` | `bicubic` | `bilinear`, `bicubic`, `lanczos` |
| `width`, `height` | project's output size | render size; the aspect stays the project's |
| `workers` | 3 | GPU worker threads (each has its own GPU context) |
| `start_time`, `time_offset` | 0 | seconds; frame time = pts − start_time + time_offset |

The project is rendered the way the Gyroflow app renders it (smoothing, zoom, lens, output size), with
`gyroflow-core` from [anton-belkin/gyroflow](https://github.com/anton-belkin/gyroflow) at `fixed-zoom`: Gyroflow
master plus the Static zoom PR ([gyroflow/gyroflow#1264](https://github.com/gyroflow/gyroflow/pull/1264)). The plugin
uses nothing beyond upstream, so it also builds against gyroflow/gyroflow master; only projects using the new zoom
settings need the PR. A project embedded in the video is read by the plugin itself (`src/embedded.rs`), so stock
Gyroflow works for making projects: `gyroflow-embed` moves the saved `.gyroflow` into the video.

## How it works

- **Per plane, at the source bit depth.** Each plane of the YUV frame is stabilized on its own, like
  Gyroflow's exporter does for planar YUV (`src/rendering/mod.rs`): `Luma16` with max 1023 for 10-bit.
  No RGB conversion, so 10-bit D-Log M reaches mpv's shaders as 10-bit.
- **Frame time from mpv's pts.** mpv's VapourSynth source restarts at frame 0 after every seek, and
  doesn't set `_AbsoluteTime`. It does attach its `struct mp_image` as `_MP_IMAGE`, which holds the pts:
  the plugin finds the field once per process from two neighbouring frames (the first double that
  advances by exactly the frame duration), instead of hardcoding one mpv version's struct layout.
- **Cached across seeks.** mpv reloads the script on every seek; the loaded project, its GPU contexts and
  worker threads are kept per (project, mtime, size, format, settings), the last two.
- **wgpu (Metal), several GPU workers.** Each frame is uploaded, rendered and read back in blocking calls,
  so three workers keep the GPU busy. Built without OpenCL: with frames in system memory, wgpu is 40-60%
  faster on the M1 Pro (4K60 clip: 55 → 87 fps). They set up their GPU contexts and render
  one frame while the filter is created, when mpv is still waiting, not against the playback clock.

## Performance (M1 Pro, DJI Osmo Action 6 10-bit D-Log M, `--untimed`, frames/s of capacity)

| | 4K 59.94 fps → 4K | 8K 29.97 fps → 4K |
|---|---|---|
| mpv decode only (videotoolbox-copy) | 190 | 75 |
| mpv → VapourSynth passthrough | 162 | 33 (`concurrent-frames=1`; 9 at mpv's default 10) |
| Stabilize, OpenCL, 2 workers | 55 | 32 |
| **Stabilize, wgpu, 3 workers, `concurrent-frames=4`** | **87** | **48** |

Realtime (`--vo=null`): 4K60 — 0 dropped frames at start and through seeks; 8K30 — ~8 at start, 0 through
seeks. In a window with the HDR shader, 8K input left too little headroom (10-50 drops at start), so
`gyroflow_auto.lua` scales >4K video to 4K on the GPU (`scale_vt`) before VapourSynth; the plugin
stabilizes from the smaller frame (the core scales the source rect): mean abs difference to Gyroflow's
8K render 0.99/255. With that, both clips play in a window with 0 drops at start and through seeks;
7-15 when toggling it on mid-playback (mpv rebuilds the filter chain). The processing is the
GPU work itself (`process_pixels`); computing the frame transform is ~0.

Matches Gyroflow's own render of the project (CLI, same branch): same mean level, mean abs difference
0.85/255; what differs is the antialiasing of the black border's edge and single pixels of fine detail
(the wgpu bicubic kernel). PSNR ~43 dB (OpenCL: ~49), against ~18 dB for the unstabilized frame.

## Build and install

```bash
cargo build --release
install -m 755 target/release/libvs_gyroflow.dylib ~/.config/mpv/gyroflow/
install -m 755 target/release/gyroflow-probe ~/.config/mpv/gyroflow/
install -m 755 target/release/gyroflow-embed ~/.config/mpv/gyroflow/
```

`gyroflow-probe <video>` tells `gyroflow_auto.lua` what a video without a `.gyroflow` next to it can be
stabilized with: `embedded` (a project stored in the video by `gyroflow-embed`), `default` (DJI clip with gyro data and EIS off),
or nothing, with the reason.

`install` writes a new file. Copying over the existing dylib with `cp` keeps the old inode, and macOS then
kills whatever loads it (exit 137): the kernel has the old code signature cached for that file.

The mpv side is `mpv/scripts/gyroflow_auto.lua` and `mpv/gyroflow/gyroflow.vpy` in this repo (see `../README.md`);
`../install.sh` builds and installs everything.

Log: `~/Library/Logs/vs-gyroflow.log` (`VS_GYROFLOW_LOG=debug` for more).
