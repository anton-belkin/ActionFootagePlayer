# DJI D-Log M and Gyroflow stabilization in mpv

Watch DJI Osmo Action footage straight off the camera on a Mac: D-Log M shown as HDR (or SDR), and
underwater clips stabilized in realtime with [Gyroflow](https://gyroflow.xyz), without rendering anything.
The source files are never re-encoded.

- **D-Log M → HDR / SDR.** GPU shaders apply DJI's D-Log M LUT (SDR, in IINA) or a D-Log M → Rec.2100 HLG
  LUT (HDR, in mpv, with macOS's HDR mode). Picked automatically from the color profile the camera wrote
  into the file (Osmo Action 6, Osmo 360; other DJI models by the `_D` file name).
- **Underwater stabilisation in realtime.** DJI's in-camera stabilization (EIS) works well in air but not
  under water (as of today's firmware), so under water you shoot with EIS off, and then the camera records
  gyro data. Those clips play stabilized straight away, with an underwater preset (light refraction 1.33)
  and no project to make. A VapourSynth plugin built on [Gyroflow](https://gyroflow.xyz)'s own core renders
  the stabilization while mpv plays, the way Gyroflow would render it (smoothing, zoom, lens, horizon): 4K60
  at ~87 fps of capacity on an M1 Pro. Clips up to 30 fps get Lanczos interpolation, faster ones bicubic
  (Lanczos holds only ~38 fps at 4K). Any clip with a Gyroflow project plays stabilized too, its own way:
  a `.gyroflow` next to the video, or the project stored inside the video (`gyroflow-embed`, below), so
  there's one file to carry.
- **Export what you see.** `Shift+E` in mpv (or `gyroflow-export` in a terminal) renders the clip stabilized
  and color-converted in one pass, 10-bit HEVC HLG, so there's no intermediate file to lose quality in.

Made for the DJI Osmo Action 6 on Apple Silicon; the Gyroflow part works with any camera Gyroflow supports
when there's a project.

## Install

Needs macOS, [Homebrew](https://brew.sh) mpv (`brew install mpv`, which includes VapourSynth), Homebrew
ffmpeg for exporting (`brew install ffmpeg`) and Rust ([rustup](https://rustup.rs)) to build the plugin.

```bash
./install.sh           # into ~/.config/mpv
./install.sh --apps    # also Steady Player, Steady Router and Finder Quick Actions (see Opening files)
```

It downloads DJI's LUT from DJI (it isn't redistributed here) and checks it, bakes the LUTs into shaders,
builds the plugin and installs everything; existing files it replaces are kept as `*.bak-<date>` (for
`scripts/`, in `scripts-backup/`, where mpv doesn't load them).

## Using it

### Opening files

`./install.sh --apps` adds two apps to `~/Applications` and two Quick Actions to Finder (right click on videos):

- **Steady Player** plays videos in mpv with all of this. It's listed in Finder's Open With for MP4 and MOV, but
  doesn't take them over: double-clicking a video still opens your usual player.
- **Quick Actions:** *Play Stabilized* opens the selected videos in Steady Player; *Stabilize + LUT* exports
  them (see Exporting) in the background, with a notification as each one is done.
- **Router mode**, if you want videos to pick their player by themselves: Steady Router becomes the default for
  MP4 and MOV and decides by the first video it's given. One of ours (D-Log M, or with a Gyroflow project or
  motion data to stabilize it with) opens in Steady Player, which then plays the rest of the folder; anything
  else opens in the player that was the default before, or one you choose:

  ```bash
  ~/.config/mpv/gyroflow/gyroflow-router on                    # the current default becomes the fallback
  ~/.config/mpv/gyroflow/gyroflow-router on --fallback IINA
  ~/.config/mpv/gyroflow/gyroflow-router on mp4               # just one type (DJI cameras record MP4)
  ~/.config/mpv/gyroflow/gyroflow-router off                   # back to the previous defaults
  ~/.config/mpv/gyroflow/gyroflow-router status
  ```

Steady Player starts mpv through a small launcher app inside it (`open -na`): started from the applet directly,
mpv stayed tied to it and macOS throttled it as a background process.

### Keys

| Key | |
|---|---|
| `Ctrl+g` | Stabilization on/off |
| `Ctrl+h` / `Ctrl+s` | D-Log M as HDR / SDR (normally picked automatically) |
| `L` | D-Log M handling off, for `_D` files that aren't D-Log (e.g. HDR photos) |
| `Shift+E` | Export the clip as it plays; with an A-B loop (`l`), only that part |

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
and then `~/.config/mpv/gyroflow/default.gyroflow`. Why a clip isn't stabilized is in mpv's log
(`--msg-level=gyroflow_auto=v`).

**The default preset is made for underwater footage.** In air, DJI's in-camera stabilization (EIS) works well,
so there's little reason to turn it off. Under water it doesn't (as of today's firmware), likely because water
bends light and the lens then behaves like a longer one than EIS is tuned for. So under water you shoot with EIS
off, the camera records gyro data (it only does with EIS off), and those are the clips this stabilizes without
a project. `install.sh` puts `mpv/gyroflow/underwater.gyroflow` there as `default.gyroflow`, unless you have one
already:

- **lens underwater:** light refraction 1.33 (Gyroflow: Lens profile → Advanced → "Lens is underwater"). Without
  it the rotation is corrected by the wrong amount and part of the shake stays: on a test clip, jitter between
  frames went from 0.16 px to 0.02 px with it.
- smoothing 0.5, zoom limit 140 %, horizon lock off.

For clips shot with EIS off in air, replace it with a preset without the refraction (save one from Gyroflow:
Export → Create settings preset), or give those clips a project of their own.

### Exporting

`Shift+E` exports the clip that's playing, the way it plays: the same project or default preset, and for
D-Log M the same LUT. With an A-B loop set (`l` twice), only that part. Playback goes on meanwhile; a
notification says where the file went. The same from a terminal, for one clip or many:

```bash
~/.config/mpv/gyroflow/gyroflow-export DJI_0007_D.MP4 DJI_0008_D.MP4
~/.config/mpv/gyroflow/gyroflow-export --from 1:20 --to 1:45 DJI_0007_D.MP4
```

The result goes to a `stabilized/` folder next to the clip, as `<name>_stab.mov` (a part: `<name>_stab_<from>-<to>s.mov`):

- **D-Log M:** HEVC Main10, Rec.2100 HLG, tagged so macOS, iOS and YouTube show it as HDR. `--sdr` makes it
  Rec.709 with DJI's LUT instead.
- **Other clips:** HEVC Main10 in the colors they were recorded in.
- `--prores`: ProRes 422 HQ instead of HEVC, for editing. The bit rate of HEVC follows the source's.
- Audio is copied as it is, and the file keeps the clip's dates. Clips with nothing to stabilize them with are
  skipped; an existing output is kept unless `--force`.

A 4K60 clip exports at ~35 fps on an M1 Pro (Lanczos interpolation; `--interpolation bicubic` is what
playback uses). The video is decoded once and encoded once: ffmpeg decodes it, the plugin's own engine
stabilizes it on the GPU, the LUT is applied on the GPU too (`vs-gyroflow/src/gpu_lut.rs`, the shader's math),
and ffmpeg encodes it with VideoToolbox.

## How it works

### D-Log M

DJI tags D-Log M files as BT.709 SDR, and names Normal-color clips `*_D.MP4` as well. The color profile is in
DJI's own metadata, though: `ColorMode` in the stream metadata of the `CAM meta` track (19 is D-Log M).
`gyroflow-probe` reads it for the Osmo Action 6 and Osmo 360, whose layouts are known; other models fall back
to the file name. Two mpv profiles (`mpv/mpv.conf`), applied by `mpv/scripts/dlog_auto.lua` on load and undone
when a playlist moves on to a clip that isn't D-Log M:

- `dji_sdr` (IINA): DJI's 33³ D-Log M → Rec.709 LUT at `HOOK MAIN`. mpv's color pipeline then runs a
  BT.709 → BT.709 identity, so the LUT's output reaches the display unchanged. IINA's libmpv OpenGL context
  can't switch macOS to HDR, so it stays SDR.
- `dji_hdr` (mpv): a 65³ D-Log M → Rec.2100 HLG (1000 nit) LUT (`luts/dlogm_hlg.cube`, composed in DaVinci
  Resolve from Zeb Gardner's D-Log M → DaVinci Wide Gamut/Intermediate shaper — see Credits) at `HOOK OUTPUT`,
  re-sampling the source through `BIND MAIN`. At `HOOK MAIN`, mpv would linearize
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

The plugin builds on `gyroflow-core` from Gyroflow master, which has the Static zoom modes
([gyroflow/gyroflow#1264](https://github.com/gyroflow/gyroflow/pull/1264)) it renders; nothing else is needed
beyond stock Gyroflow.

## Layout

```
install.sh                    builds and installs everything
mpv/                          mpv.conf, input.conf, scripts/, gyroflow/gyroflow.vpy  →  ~/.config/mpv
mpv/gyroflow/underwater.gyroflow  the default preset for clips without a project
shaders/*.glsl.in             shader templates; the LUT goes in at install
luts/dlogm_hlg.cube           D-Log M → Rec.2100 HLG LUT (DJI's Rec.709 one is downloaded)
tools/cube_to_glsl.py         .cube → mpv shader
vs-gyroflow/                  Gyroflow VapourSynth plugin, gyroflow-probe, gyroflow-embed, gyroflow-export (Rust)
macos/                        Steady Player / Steady Router applets, mpv launcher, gyroflow-router, the Quick
                              Actions' export script
tools/make_workflow.py        writes the Finder Quick Actions
```

## License

GPL-3.0 (the plugin builds on Gyroflow, which is GPL-3.0). DJI's LUT belongs to DJI and is downloaded from
DJI's site at install.

## Credits

`luts/dlogm_hlg.cube` (D-Log M → Rec.2100 HLG, 1000-nit, no DaVinci DRT) was composed in DaVinci Resolve
from [Zeb Gardner](https://www.zebgardner.com/)'s free D-Log M → DaVinci Wide Gamut/Intermediate shaper LUT
(`DLOGM_DWG_I_SHAPER_ZRG_V1`); the HLG output stage and the bake are this project's. Thanks to Zeb Gardner for
the underlying transform. His site doesn't state a license for the shaper; it's used here with credit, and
will be changed if he asks.
