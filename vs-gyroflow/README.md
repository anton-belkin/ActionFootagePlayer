# vs-gyroflow

Gyroflow stabilization for ActionFootagePlayer: a library (the engine, `src/lib.rs`'s `Engine` and `Views`: a clip
loaded with gyroflow-core the way the player plays it) and the binaries the player, the Library and the NAS Helper
run (`src/bin/`):

| Binary | |
|---|---|
| `gyroflow-gpu-table` | the player's stabilization: the clip's warp as an mpv shader (`shaders/gyroflow/warp.glsl` with every frame's parameters in it, `src/gpu_table.rs`), from the player's request (`--player <json> <out.glsl>`: project or preset, view, level, horizon lock, lens under water, edges, frame, moment); `--serve` keeps the clips it loaded; `--sphere360` a 360 camera's sphere |
| `gyroflow-export` | exports, photos, Live Photos, moments: on the GPU (ffmpeg's libplacebo with the same warp) or through the engine (the "pipes" path) |
| `gyroflow-probe` | what a clip can be stabilized with (below), its lens, a 360 rig's headings |
| `gyroflow-project` | the project a clip without one plays with, written out for Gyroflow |
| `gyroflow-optical` | motion measured from the picture (`<clip>.afp-motion`) |
| `afp-cut`, `afp-meta`, `afp-rpu`, `afp-catalog` | lossless cuts, metadata, Dolby Vision RPUs, the project's catalog |

Up to 2026-11 the crate was also a VapourSynth filter (`core.gyroflow.Stabilize`, mpv's `vf=vapoursynth`): the
player's first render path. It was taken out; the GPU warp is the player's only path. What the warp can't do yet
(`gpu_table::unsupported`: other lens models such as Sony's, a lens mesh, IBIS/OIS, a horizontal rolling shutter, an
equirect output) plays as recorded.

The project is rendered the way the Gyroflow app renders it (smoothing, zoom, lens, output size), with
`gyroflow-core` from [anton-belkin/gyroflow](https://github.com/anton-belkin/gyroflow) at `afp-v1.7.0`: Gyroflow
1.7.0 plus what upstream doesn't have yet, crop mode's view rotation
([gyroflow/gyroflow#1289](https://github.com/gyroflow/gyroflow/pull/1289)), the equirect output and a 360 rig's lenses
([#1290](https://github.com/gyroflow/gyroflow/pull/1290)) and the Insta360 X4/X5 lens terms (see Cargo.toml). Stock
Gyroflow (1.7.0 or newer for Stabilize optically) works for making projects: `gyroflow-project` writes out the project a clip
without one plays with (the default preset's, at the level, in the view and with the horizon lock the player has, with
the motion data in it, made by `src/project.rs` with gyroflow-core's own project writer) next to it, to open it in
Gyroflow. A project saved without the motion data gets the video's read ahead into it before Gyroflow loads it, and
the zoom limit's iterations of a project run once, not also inside the import.

## How the engine works (the export's pipes path)

- **Per plane, at the source bit depth.** Each plane of the YUV frame is stabilized on its own, like
  Gyroflow's exporter does for planar YUV (`src/rendering/mod.rs`): `Luma16` with max 1023 for 10-bit.
  No RGB conversion, so 10-bit D-Log M stays 10-bit.
- **Edges from a coverage mask** (`src/edges.rs`). Which output pixels the frame covers comes from the
  kernel itself: each frame it also renders a white plane with a feathered border (3 % of the height) onto black,
  at 1/4 size, with the frame's transform, so lens, rolling shutter and refraction are all in it. The fills are
  blurred anyway, so they're made on a ~240x135 grid: the source scaled down there on the CPU, the stabilized
  frame rendered from it on the GPU (edges repeated, so it has no hole of its own), the trail kept per cell.
  Only the 32x32 blocks the mask says aren't covered are blended at full size, over 4 threads. The trail starts
  over at a jump in time (a seek), a new engine, or a cut (the frame and the trail differ by >12 % where both
  have content). The sphere trail (`src/sphere.rs`) keeps the same grid by direction instead, in a cube map about
  as fine as the grid: each output cell's direction is the kernel's own geometry (output projection, smoothed
  orientation, video rotation, zoom center), the same for every row because the stabilized frame has the rolling
  shutter undone. It's written every frame from a view of its own, rendered like the coarse frame but centered,
  equidistant and 1.6x as wide as the unzoomed source (on a grid as much bigger), so the whole source frame lands
  in it, shake and all, and what the output's zoom crops away is kept too. The unit tests hold the directions
  against Gyroflow's CPU kernel to 0.25 source pixels, the wide view's included.
- **wgpu (Metal), several GPU workers.** Each frame is uploaded, rendered and read back in blocking calls,
  so three workers keep the GPU busy. Built without OpenCL: with frames in system memory, wgpu is 40-60%
  faster on the M1 Pro (4K60 clip: 55 → 87 fps).

## Fidelity

Matches Gyroflow's own render of the project (CLI, same branch): same mean level, mean abs difference
0.85/255; what differs is the antialiasing of the black border's edge and single pixels of fine detail
(the wgpu bicubic kernel). PSNR ~43 dB (OpenCL: ~49), against ~18 dB for the unstabilized frame.

## Build and install

```bash
cargo build --release
install -m 755 target/release/gyroflow-gpu-table ~/.config/mpv/scripts/gyroflow/
install -m 755 target/release/gyroflow-probe ~/.config/mpv/scripts/gyroflow/
install -m 755 target/release/gyroflow-project ~/.config/mpv/scripts/gyroflow/
install -m 755 target/release/gyroflow-export ~/.config/mpv/scripts/gyroflow/
install -m 755 target/release/afp-cut ~/.config/mpv/scripts/gyroflow/
```

`gyroflow-probe <video>` tells the gyroflow mpv plugin what a video without a `.gyroflow` next to it can be
stabilized with: `default` (its own motion data, recorded with in-camera stabilization off: DJI EIS, GoPro HyperSmooth, Insta360 FlowState;
or a GoPro HERO12+ with HyperSmooth on, which records how it turned the picture: stabilized on top of it, `src/in_camera.rs`),
or nothing, with the reason.

`afp-cut <video> --from <t> [--to <t>]` keeps a part of a camera's MP4 without re-encoding it: every sample as it is,
DJI's clip data (only in the first sample of its metadata track) put back in front of the new first one, the timecode
and creation times moved on, and the key `ch.actionfootageplayer.cut`; the cut is then read back as the player reads
the original and removed when it doesn't read the same ([docs/lossless-cut.md](../docs/lossless-cut.md)).

`install` writes a new file. Copying over an existing binary with `cp` keeps the old inode, and macOS then
kills it when it runs (exit 137): the kernel has the old code signature cached for that file.

On Linux and Windows it builds the same way (`cargo build --release`), as `gyroflow-gpu-table(.exe)`,
`gyroflow-probe(.exe)`, `gyroflow-project(.exe)`, `gyroflow-export(.exe)` and `afp-cut(.exe)`; there wgpu renders
on Vulkan (Windows: DirectX 12 or Vulkan). `../tools/make_plugin_zip.sh <version>` makes the plugin's zip of them,
and cross-compiles with a Rust target, e.g. `x86_64-pc-windows-gnu` on Linux with mingw-w64. A build where
gyroflow-core's build script can't download the lens profiles (no network) needs
`resources/camera_presets/profiles.cbor.gz` in its checkout, from
[gyroflow/lens_profiles](https://github.com/gyroflow/lens_profiles/releases/latest/download/profiles.cbor.gz).

The mpv side is the gyroflow plugin, `mpv/scripts/gyroflow/` in this repo (see `../README.md`);
`../install.sh` builds and installs everything.

Log: `~/Library/Logs/vs-gyroflow.log`, on Linux `~/.local/state/vs-gyroflow.log`, on Windows
`%LOCALAPPDATA%\vs-gyroflow.log` (`VS_GYROFLOW_LOG=debug` for more).
