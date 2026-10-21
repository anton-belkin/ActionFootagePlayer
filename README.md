# DJI D-Log M and Gyroflow stabilization in mpv

Watch DJI Osmo Action footage straight off the camera on a Mac: D-Log M shown as HDR (or SDR), and clips
stabilized in realtime with [Gyroflow](https://gyroflow.xyz), without rendering anything. The source files
are never re-encoded.

- **D-Log M → HDR / SDR.** GPU shaders apply DJI's D-Log M LUT (SDR, in IINA) or a D-Log M → Rec.2100 HLG
  LUT (HDR, in mpv, with macOS's HDR mode). Picked automatically for `*_D.MP4` / `*_D.MOV` files.
- **Gyroflow in realtime.** A VapourSynth plugin built on Gyroflow's own core renders the stabilization
  while mpv plays, the way Gyroflow would render it (smoothing, zoom, lens, horizon): 4K60 at ~87 fps of
  capacity on an M1 Pro. It's used when
  - a Gyroflow project (`.gyroflow`) sits next to the video,
  - or the project is stored inside the video (`gyroflow-embed`, below), so there's one file to carry,
  - or the clip has gyro data of its own and was shot without in-camera stabilization (DJI with EIS off,
    FOV Wide): it's stabilized with Gyroflow's default settings, no project needed.

Made for the DJI Osmo Action 6 on Apple Silicon; the Gyroflow part works with any camera Gyroflow supports
when there's a project.

## Install

Needs macOS, [Homebrew](https://brew.sh) mpv (`brew install mpv`, which includes VapourSynth) and Rust
([rustup](https://rustup.rs)) to build the plugin.

```bash
./install.sh           # into ~/.config/mpv
./install.sh --apps    # also DJI Router.app and mpv.app in ~/Applications (see Opening files)
```

It downloads DJI's LUT from DJI (it isn't redistributed here) and checks it, bakes the LUTs into shaders,
builds the plugin and installs everything; existing files it replaces are kept as `*.bak-<date>`.

## Using it

### Opening files

`DJI Router.app` (`--apps`) routes clips by name: `*_D.MP4` / `*_D.MOV` to mpv (HDR, stabilization),
everything else to [IINA](https://iina.io). Make it the default for MP4/MOV: Finder → Get Info on a clip →
Open with → DJI Router → Change All. It starts mpv through `mpv.app`, an app wrapper around Homebrew's mpv:
started from the applet directly, mpv stayed tied to it and macOS throttled it as a background process.

### Keys

| Key | |
|---|---|
| `Ctrl+g` | Stabilization on/off |
| `Ctrl+h` / `Ctrl+s` | D-Log M as HDR / SDR (normally picked automatically) |
| `L` | D-Log M handling off, for `_D` files that aren't D-Log (e.g. HDR photos) |

### Stabilization

Make the project in Gyroflow and save it next to the clip (`Ctrl+S` in Gyroflow): mpv uses it. To carry one
file instead of two, move the project into the video:

```bash
~/.config/mpv/gyroflow/gyroflow-embed --move DJI_0009_D.MP4            # DJI_0009_D.gyroflow → inside the MP4
~/.config/mpv/gyroflow/gyroflow-embed --extract DJI_0009_D.MP4         # back out, to open it in Gyroflow
~/.config/mpv/gyroflow/gyroflow-embed --remove DJI_0009_D.MP4
```

The project is appended to the MP4 as a box of its own: the video data isn't touched, the file keeps its
date, players and Gyroflow open it as before, and it moves with the file. Apps that rewrite the file
(trimming, remuxing) drop it.

Clips with gyro data and EIS off play stabilized even without a project, with Gyroflow's default preset
and then `~/.config/mpv/gyroflow/default.gyroflow` if it exists (save one from Gyroflow: Export → Create
settings preset). Why a clip isn't stabilized is in mpv's log (`--msg-level=gyroflow_auto=v`).

## How it works

### D-Log M

DJI tags D-Log M files as BT.709 SDR. Two mpv profiles (`mpv/mpv.conf`), applied by
`mpv/scripts/dlog_auto.lua` on load:

- `dji_sdr` (IINA): DJI's 33³ D-Log M → Rec.709 LUT at `HOOK MAIN`. mpv's color pipeline then runs a
  BT.709 → BT.709 identity, so the LUT's output reaches the display unchanged. IINA's libmpv OpenGL context
  can't switch macOS to HDR, so it stays SDR.
- `dji_hdr` (mpv): a 65³ D-Log M → Rec.2100 HLG (1000 nit) LUT (`luts/dlogm_hlg.cube`, made in DaVinci
  Resolve) at `HOOK OUTPUT`, re-sampling the source through `BIND MAIN`. At `HOOK MAIN`, mpv would linearize
  the HLG output as the file's BT.709 and encode it again. `target-trc=hlg` with
  `target-colorspace-hint=yes` switches the Metal layer to HDR. On an SDR display macOS tone-maps it down.

The LUTs are baked into the shaders as 3D `rgba16` textures (`tools/cube_to_glsl.py`, templates in
`shaders/`), so the GPU does the trilinear interpolation in one tap.

### Gyroflow

`mpv/scripts/gyroflow_auto.lua` adds `vf=vapoursynth` with `mpv/gyroflow/gyroflow.vpy`, which runs the
`gyroflow.Stabilize` plugin from `vs-gyroflow/`. Without a `.gyroflow` next to the video, `gyroflow-probe`
decides first whether the video carries a project, or its own gyro data recorded with EIS off. Frames are
stabilized plane by plane at the source bit depth, so 10-bit D-Log M reaches the shaders as 10-bit; 8K is
scaled to 4K on the GPU first. Details, measurements and the plugin's options: [vs-gyroflow/README.md](vs-gyroflow/README.md).

The plugin builds on `gyroflow-core` from Gyroflow master plus the Static zoom PR
([gyroflow/gyroflow#1264](https://github.com/gyroflow/gyroflow/pull/1264)), which adds a fixed zoom level.
Everything else works with stock Gyroflow.

## Layout

```
install.sh                    builds and installs everything
mpv/                          mpv.conf, input.conf, scripts/, gyroflow/gyroflow.vpy  →  ~/.config/mpv
shaders/*.glsl.in             shader templates; the LUT goes in at install
luts/dlogm_hlg.cube           D-Log M → Rec.2100 HLG LUT (DJI's Rec.709 one is downloaded)
tools/cube_to_glsl.py         .cube → mpv shader
vs-gyroflow/                  Gyroflow VapourSynth plugin, gyroflow-probe, gyroflow-embed (Rust)
macos/                        DJI Router applet source, mpv.app launcher
```

## License

GPL-3.0 (the plugin builds on Gyroflow, which is GPL-3.0). DJI's LUT belongs to DJI and is downloaded from
DJI's site at install.
