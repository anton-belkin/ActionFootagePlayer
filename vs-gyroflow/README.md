# vs-gyroflow

Gyroflow stabilization as a VapourSynth filter, so mpv plays a video stabilized, in realtime, when its
`.gyroflow` project sits next to it.

```python
core.gyroflow.Stabilize(clip, project="/path/clip.gyroflow")
```

| Argument | Default | |
|---|---|---|
| `project` | — | `.gyroflow` project file, or the video itself: its own motion data with Gyroflow's default preset |
| `preset` | — | `.gyroflow` preset applied on top when the video has no project |
| `interpolation` | `bicubic` | `bilinear`, `bicubic`, `lanczos` |
| `width`, `height` | project's output size | render size; the aspect stays the project's |
| `workers` | 3 | GPU worker threads (each has its own GPU context) |
| `start_time`, `time_offset` | 0 | seconds; frame time = pts − start_time + time_offset |
| `edges` | `off` | what fills what the frame doesn't cover: `off` (the project's background), `original`, `zoom`, `trail`, `sphere` |
| `edges_blur`, `edges_zoom` | 2, 1.5 | blur of the fill (sigma, % of the frame height); how much `zoom` scales up |
| `fov` | project's | Gyroflow's FOV (Advanced) in place of the project's; above 1 the view is wider than the zoom covers |
| `wide` | 0 | 1: start in the wider view, no zoom, at the FOV where the stabilized picture covers about as much area as the frame (the area of the frame's outline after the lens correction, `FOV = √(area / frame's area)`, the clip's median). The engine renders both views with the same smoothing, as a FOV change in Gyroflow doesn't recompute it, and the control file's `view=` switches between them on the next frame |
| `horizon_lock` | project's | the horizon lock's amount, 0-100 % (0: off), in place of the project's; its roll, pitch and turn settings stay |
| `edges_control` | — | file with the mode in it, optionally followed by `blur=<%>`, `view=normal\|wide` and `chromatic=yes\|no`, read while playing (at most every 50 ms) |
| `chromatic` | 0 | 1: each frame's lens geometry written into the start of its last row of luma, for mpv's `chromatic.glsl`, which takes out a flat port's color fringes under water and paints the row over (`src/dispersion.rs`); with `edges` off it carries the project's background color too, which the shader leaves as it is. Red and blue move only where the refraction says under water, frame by frame, and while the control file (`edges_control`) doesn't say `chromatic=no` |

The project is rendered the way the Gyroflow app renders it (smoothing, zoom, lens, output size), with
`gyroflow-core` from [anton-belkin/gyroflow](https://github.com/anton-belkin/gyroflow) at `fixed-zoom`: Gyroflow
master plus the Static zoom PR ([gyroflow/gyroflow#1264](https://github.com/gyroflow/gyroflow/pull/1264)). The plugin
uses nothing beyond upstream, so it also builds against gyroflow/gyroflow master; only projects using the new zoom
settings need the PR. Stock Gyroflow works for making projects: `gyroflow-project` writes out the project a clip
without one plays with (the default preset's, in the view and with the horizon lock the player has, made by
`src/project.rs` with gyroflow-core's own project writer) next to it, to open it in Gyroflow.

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
- **Edges from a coverage mask** (`src/edges.rs`). Which output pixels the frame covers comes from the
  kernel itself: each frame it also renders a white plane with a feathered border (3 % of the height) onto black,
  at 1/4 size, with the frame's transform, so lens, rolling shutter and refraction are all in it. The fills are
  blurred anyway, so they're made on a ~240x135 grid: the source scaled down there on the CPU, the stabilized
  frame rendered from it on the GPU (edges repeated, so it has no hole of its own), the trail kept per cell.
  Only the 32x32 blocks the mask says aren't covered are blended at full size, over 4 threads. The trail starts
  over at a jump in time (a seek), a new filter, or a cut (the frame and the trail differ by >12 % where both
  have content). The sphere trail (`src/sphere.rs`) keeps the same grid by direction instead, in a cube map about
  as fine as the grid: each output cell's direction is the kernel's own geometry (output projection, smoothed
  orientation, video rotation, zoom center), the same for every row because the stabilized frame has the rolling
  shutter undone. It's written every frame from a view of its own, rendered like the coarse frame but centered,
  equidistant and 1.6x as wide as the unzoomed source (on a grid as much bigger), so the whole source frame lands
  in it, shake and all, and what the output's zoom crops away is kept too. The unit tests hold the directions
  against Gyroflow's CPU kernel to 0.25 source pixels, the wide view's included. The mode comes
  from a control file rather than a filter argument, so switching it doesn't rebuild mpv's filter chain (7-15
  dropped frames each time).
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
the gyroflow mpv plugin scales >4K video to 4K on the GPU (`scale_vt`) before VapourSynth; the plugin
stabilizes from the smaller frame (the core scales the source rect): mean abs difference to Gyroflow's
8K render 0.99/255. With that, both clips play in a window with 0 drops at start and through seeks;
7-15 when toggling it on mid-playback (mpv rebuilds the filter chain). The processing is the
GPU work itself (`process_pixels`); computing the frame transform is ~0.

Edge fill, per frame on a GPU worker, with ~20 % of a 4K frame uncovered: mask 2-3 ms, the stabilized frame
on the grid 4-7 ms (both mostly waiting on the GPU, which the other workers keep busy), fill 1.7 ms, blending
4 ms; at full 60 fps this cost the underwater 4K60 clip (~63 fps of capacity) a few percent and it dropped
frames. The sphere trail adds writing the sphere from a wider view: in an export with each frame, in playback on a thread of its
own (`sphere_writer`), from the newest frame, only while some GPU worker is free, and without a lock against the frames
reading it (its texels are atomics). Whether a clip has edges at all is
measured when the project loads, with Gyroflow's own numbers: a frame leaves some where the FOV it renders at (the
zoom's, times the FOV setting) is wider than the widest one it still covers (`minimal_fovs`, what Gyroflow's FOV
warning compares), for any project and zoom setting. Only in such a clip is the sphere written, edges showing or not,
so it has what the camera saw when they do; and `core.gyroflow.LeavesEdges(clip, ...)` (Stabilize's arguments, the same
loaded project) tells the mpv plugin whether to play it every other frame when `max_fps` is set. 4K60 in the wider
view on the M1 Pro, untimed: ~84 fps through the engine with the sphere, the same as the trail (~75 with the sphere
written by the frames); in real time 60, with the sphere written ~57 times a second. With two workers instead of
three: 0 frames dropped in 20 s and the sphere written ~25 times a second, where writing it with each frame dropped 69.

Matches Gyroflow's own render of the project (CLI, same branch): same mean level, mean abs difference
0.85/255; what differs is the antialiasing of the black border's edge and single pixels of fine detail
(the wgpu bicubic kernel). PSNR ~43 dB (OpenCL: ~49), against ~18 dB for the unstabilized frame.

## Build and install

```bash
cargo build --release
install -m 755 target/release/libvs_gyroflow.dylib ~/.config/mpv/scripts/gyroflow/
install -m 755 target/release/gyroflow-probe ~/.config/mpv/scripts/gyroflow/
install -m 755 target/release/gyroflow-project ~/.config/mpv/scripts/gyroflow/
install -m 755 target/release/gyroflow-export ~/.config/mpv/scripts/gyroflow/
```

`gyroflow-probe <video>` tells the gyroflow mpv plugin what a video without a `.gyroflow` next to it can be
stabilized with: `default` (its own motion data, recorded with in-camera stabilization off: DJI EIS, GoPro HyperSmooth, Insta360 FlowState),
or nothing, with the reason.

`install` writes a new file. Copying over the existing dylib with `cp` keeps the old inode, and macOS then
kills whatever loads it (exit 137): the kernel has the old code signature cached for that file.

On Linux and Windows it builds the same way (`cargo build --release`), as `libvs_gyroflow.so` and
`vs_gyroflow.dll` with `gyroflow-probe(.exe)`, `gyroflow-project(.exe)` and `gyroflow-export(.exe)`; there wgpu renders
on Vulkan (Windows: DirectX 12 or Vulkan). `../tools/make_plugin_zip.sh <version>` makes the plugin's zip of them,
and cross-compiles with a Rust target, e.g. `x86_64-pc-windows-gnu` on Linux with mingw-w64. A build where
gyroflow-core's build script can't download the lens profiles (no network) needs
`resources/camera_presets/profiles.cbor.gz` in its checkout, from
[gyroflow/lens_profiles](https://github.com/gyroflow/lens_profiles/releases/latest/download/profiles.cbor.gz).

The mpv side is the gyroflow plugin, `mpv/scripts/gyroflow/` in this repo (see `../README.md`);
`../install.sh` builds and installs everything.

Log: `~/Library/Logs/vs-gyroflow.log`, on Linux `~/.local/state/vs-gyroflow.log`, on Windows
`%LOCALAPPDATA%\vs-gyroflow.log` (`VS_GYROFLOW_LOG=debug` for more).
