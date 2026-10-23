# DJI D-Log M and Gyroflow stabilization in mpv

Watch DJI Osmo Action footage straight off the camera on a Mac: D-Log M shown in HDR, and
underwater clips stabilized in realtime with [Gyroflow](https://gyroflow.xyz), without rendering anything.
The source files are never re-encoded.

- **Colour recovery for D-Log M in realtime, including HDR.** A GPU shader turns D-Log M into Rec.2100 HLG
  and macOS shows it in HDR (on an SDR screen, macOS tone-maps it down). Applied automatically from the color
  profile the camera wrote into the file (Osmo Action 6, Osmo 360; other DJI models by the `_D` file name).
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

It's two mpv plugins, each a folder in `~/.config/mpv/scripts/` that works without the other, and Steady
Player, a Mac app that carries both and opens your MP4s with them:

- **`dlogm`**: D-Log M recognized from the camera's metadata, and shown in HDR or through DJI's Rec.709 LUT. Plain
  Lua and GPU shaders, so it runs in any mpv on any system (`vo=gpu-next`).
- **`gyroflow`**: the stabilization and the export, with the binaries built from `vs-gyroflow/` (macOS, Apple Silicon).

Neither needs anything in `mpv.conf` or `input.conf`. Made for the DJI Osmo Action 6 on Apple Silicon; the
Gyroflow part works with any camera Gyroflow supports when there's a project.

## Install

Needs a Mac with Apple Silicon (macOS 11 or later), [Homebrew](https://brew.sh) mpv (`brew install mpv`, which
includes VapourSynth) and Homebrew ffmpeg for exporting (`brew install ffmpeg`).

**Steady Player**, the app (it carries both plugins): download `Steady-Player-0.3.0-macos-arm64.zip` from the
[release](https://github.com/anton-belkin/dlogm-gyroflow-player/releases/tag/v0.3.0), unzip it and move
Steady Player to Applications, then open it. It's signed ad hoc, not by an Apple developer account, so the first
time macOS won't open it: System Settings → Privacy & Security → Open Anyway. Opened on its own, it offers its
setup:

- it becomes the default app for MP4 videos (what DJI cameras record): double-clicking one plays it with all
  of this, and any other video simply plays in mpv. macOS asks you to confirm the change in a dialog of its own;
  it applies once you do;
- Finder gets the Quick Actions *Play Stabilized* and *Stabilize + LUT* (see Opening files);
- DJI's Rec.709 LUT is downloaded from DJI (it isn't redistributed here) and checked, for `Ctrl+s` and Rec.709
  exports. HDR works without it.

**Just the plugins**, for mpv you start yourself: unzip them into mpv's `scripts/` folder (`~/.config/mpv/scripts/`).

- `mpv-dlogm-0.3.0.zip`, on any system (`%APPDATA%\mpv\scripts\` on Windows). It has the HDR shader; the Rec.709
  one (`Ctrl+s`) is made from DJI's LUT: download it from DJI (Osmo Action 6 downloads page) and bake it in from a
  checkout of this repo:

  ```bash
  python3 tools/cube_to_glsl.py shaders/dlogm_lut3d.glsl.in "DJI OSMO Action 6 D-LogM to Rec.709 LUT-11.17.cube" \
      ~/.config/mpv/scripts/dlogm/shaders/dlogm_lut3d.glsl
  ```

- `mpv-gyroflow-0.3.0-macos-arm64.zip`, for Homebrew's mpv on Apple Silicon, with its binaries. Downloaded with
  a browser, they carry the quarantine flag, and macOS would refuse them:
  `xattr -dr com.apple.quarantine ~/.config/mpv/scripts/gyroflow`.

**With install.sh**, from the release's tarball (prebuilt, no Rust needed) or from source (needs Rust,
[rustup](https://rustup.rs); the first build takes 5-10 minutes):

```bash
curl -LO https://github.com/anton-belkin/dlogm-gyroflow-player/releases/download/v0.3.0/dlogm-gyroflow-player-0.3.0-macos-arm64.tar.gz
tar -xzf dlogm-gyroflow-player-0.3.0-macos-arm64.tar.gz
cd dlogm-gyroflow-player-0.3.0-macos-arm64     # or: git clone https://github.com/anton-belkin/dlogm-gyroflow-player.git
./install.sh --apps
```

`./install.sh` alone installs the two plugins into `~/.config/mpv/scripts/` (and an `mpv.conf` with a few
recommended settings, when there's none yet); `--apps` also builds Steady Player into `~/Applications` and runs
its setup, and once it's installed, later runs update it too. It downloads DJI's LUT and checks it, bakes the
LUTs into shaders and installs everything; existing files it replaces are kept as `*.bak-<date>` (for
`scripts/`, in `scripts-backup/`, where mpv doesn't load them).

**Upgrading from 0.2**, `install.sh --apps` removes Steady Router: MP4 goes to Steady Player, MOV back to the
player it was taking videos from. `gyroflow-router` is gone with it.

**Upgrading from 0.1**, `install.sh` moves the old setup over: the loose scripts go to `scripts-backup/`, the
D-Log M keys come out of `input.conf` (the rest of it stays), the `mpv.conf` with the D-Log M profiles is replaced
(kept as a `.bak`), and `~/.config/mpv/gyroflow/` is emptied into the plugin, your `default.gyroflow` included.
The commands moved with it: `gyroflow-embed` and `gyroflow-export` are now in `~/.config/mpv/scripts/gyroflow/`.

## Using it

### Opening files

Once Steady Player is set up, double-clicking an MP4 plays it in mpv with both plugins; for other videos, open
them with Steady Player from Finder's Open With, or right click → Quick Actions:

- **Play Stabilized** opens the selected videos in Steady Player.
- **Stabilize + LUT** exports them (see Exporting) in the background, with a notification as each one is done.

Steady Player uses the plugins it carries (`Contents/Resources/plugins/`), or the copy in `~/.config/mpv/scripts/`
when there is one (`install.sh` puts them there for mpv in a terminal), so they're never loaded twice. It reads
your `~/.config/mpv` (`mpv.conf`, `input.conf`, `script-opts/`) like any mpv. The app is read-only once signed;
what's yours is in `~/Library/Application Support/Steady Player/`: DJI's LUT and its shader (`dlogm/`), and
your own default preset if you make one (`gyroflow/default.gyroflow`, see Stabilization).

Steady Player starts mpv through a small launcher app inside it (`open -na`): started from the applet directly,
mpv stayed tied to it and macOS throttled it as a background process.

### Keys

| Key | | `script-binding` |
|---|---|---|
| `Ctrl+g` | Stabilization on/off | `gyroflow/toggle` |
| `Ctrl+b` | What fills the edges the stabilized frame doesn't cover: the project's background, the original frame, the frame zoomed, or a trail of earlier frames (see Stabilization); it sticks for the next clips | `gyroflow/edges` |
| `Ctrl+s` / `Ctrl+h` | D-Log M through DJI's own Rec.709 LUT instead of HDR, and back; it sticks for the next D-Log M clips | `dlogm/sdr`, `dlogm/hdr` |
| `L` | D-Log M handling off for this clip, for `_D` files that aren't D-Log (e.g. HDR photos) | `dlogm/off` |
| `Shift+E` | Export the clip as it plays; with an A-B loop (`l`), only that part | `gyroflow/export` |

To use other keys, bind the names in `input.conf`, e.g. `Ctrl+d script-binding dlogm/sdr`.

### Options

In `~/.config/mpv/script-opts/<plugin>.conf`, or on the command line as `--script-opts=dlogm-mode=sdr`:

| | |
|---|---|
| `dlogm.conf`: `mode=hdr` | what D-Log M clips start in: `hdr`, or `sdr` (DJI's Rec.709 LUT) |
| `gyroflow.conf`: `hwdec=yes` | decode the clips it stabilizes with VideoToolbox whatever mpv's `hwdec` is (software decoding doesn't keep up with 4K); `no` leaves it to `hwdec` |
| `gyroflow.conf`: `workers=3` | GPU worker threads for the stabilization |
| `gyroflow.conf`: `max_fps=30` | clips faster than this play every other frame (60 fps at 30), so the stabilization does half the work; `0`: every frame |
| `gyroflow.conf`: `edges=trail` | what fills the edges the stabilized frame doesn't cover: `trail`, `original`, `zoom`, or `off` (the project's background color) |
| `gyroflow.conf`: `edges_blur=2`, `edges_zoom=1.5` | blur of the edge fill (% of the frame height), and how much `zoom` scales the frame up |

### Stabilization

Make the project in Gyroflow and save it next to the clip (`Ctrl+S` in Gyroflow): mpv uses it. To carry one
file instead of two, move the project into the video (the commands here are in `~/.config/mpv/scripts/gyroflow/`;
with just Steady Player, in `/Applications/Steady Player.app/Contents/Resources/plugins/gyroflow/`):

```bash
~/.config/mpv/scripts/gyroflow/gyroflow-embed --move DJI_0009_D.MP4     # DJI_0009_D.gyroflow → inside the MP4
~/.config/mpv/scripts/gyroflow/gyroflow-embed --extract DJI_0009_D.MP4  # back out, to open it in Gyroflow
~/.config/mpv/scripts/gyroflow/gyroflow-embed --remove DJI_0009_D.MP4
```

The project is appended to the MP4 as a box of its own: the video data isn't touched, the file keeps its
date, players and Gyroflow open it as before, and it moves with the file. Apps that rewrite the file
(trimming, remuxing) drop it.

Clips with gyro data and EIS off play stabilized even without a project, with Gyroflow's default preset
and then `default.gyroflow`: yours in `~/Library/Application Support/Steady Player/gyroflow/` if you put one
there, else the plugin's (`~/.config/mpv/scripts/gyroflow/`, or in Steady Player). Why a clip isn't stabilized is in mpv's log
(`--msg-level=gyroflow=v`; for D-Log M, `--msg-level=dlogm=v`).

**The default preset is made for underwater footage.** In air, DJI's in-camera stabilization (EIS) works well,
so there's little reason to turn it off. Under water it doesn't (as of today's firmware), likely because water
bends light and the lens then behaves like a longer one than EIS is tuned for. So under water you shoot with EIS
off, the camera records gyro data (it only does with EIS off), and those are the clips this stabilizes without
a project. The plugin's `default.gyroflow` is `mpv/scripts/gyroflow/underwater.gyroflow` (`install.sh` keeps one
you've changed in `~/.config/mpv/scripts/gyroflow/`):

- **lens underwater:** light refraction 1.33 (Gyroflow: Lens profile → Advanced → "Lens is underwater"). Without
  it the rotation is corrected by the wrong amount and part of the shake stays: on a test clip, jitter between
  frames went from 0.16 px to 0.02 px with it.
- smoothing 0.5, zoom limit 140 %, horizon lock off.

For clips shot with EIS off in air, use a preset without the refraction (save one from Gyroflow: Export →
Create settings preset, as `default.gyroflow` in one of the places above), or give those clips a project of
their own.

**Edges.** When the project's zoom isn't enough to hide them (a fixed zoom, or zooming off), the stabilized frame
leaves parts of the window empty, which Gyroflow fills with the project's background color. Here they're filled
with something less noticeable, blurred and blended into the picture over a soft edge; `Ctrl+b` switches between
them while playing:

- **trail**, the default: what was last shown at that spot in earlier frames. The stabilized frame stays put while
  the camera shakes, so a part it uncovers now was usually in view a moment ago. Where nothing was yet (the first
  frames, after a seek or a cut), the zoom fill; older trail fades into it too (half-life 3 s).
- **original**: the unstabilized frame behind.
- **zoom**: the stabilized frame scaled up 1.5× behind.
- **off**: the project's background color, as Gyroflow renders it.

With the default preset's zoom limit, Gyroflow lowers the smoothing rather than show edges, so these clips rarely
have any. 60 fps clips play at 30 (`max_fps`), every other frame: that leaves room for the fill (4K60 with the
underwater preset otherwise runs at the limit, ~63 fps of capacity on an M1 Pro).

### Exporting

`Shift+E` exports the clip that's playing, the way it plays: the same project or default preset, and for
D-Log M the same LUT. With an A-B loop set (`l` twice), only that part. Playback goes on meanwhile; a
notification says where the file went. The same from a terminal, for one clip or many:

```bash
~/.config/mpv/scripts/gyroflow/gyroflow-export DJI_0007_D.MP4 DJI_0008_D.MP4
~/.config/mpv/scripts/gyroflow/gyroflow-export --from 1:20 --to 1:45 DJI_0007_D.MP4
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
The dlogm plugin reads it in Lua (`mpv/scripts/dlogm/dji_color.lua`: the first sample of the metadata track,
a few protobuf fields), for the Osmo Action 6 and Osmo 360, whose layouts are known; other models fall back
to the file name. `gyroflow-probe` reads it the same way, for the export. The plugin
(`mpv/scripts/dlogm/main.lua`) sets the shader and the display options as file-local options, added to any
shaders of your own, so mpv undoes them when a playlist moves on to a clip that isn't D-Log M:

- `hdr`, the default: a 65³ D-Log M → Rec.2100 HLG (1000 nit) LUT (`luts/dlogm_hlg.cube`, composed in DaVinci
  Resolve from Zeb Gardner's D-Log M → DaVinci Wide Gamut/Intermediate shaper — see Credits) at `HOOK OUTPUT`,
  re-sampling the source through `BIND MAIN`. At `HOOK MAIN`, mpv would linearize
  the HLG output as the file's BT.709 and encode it again. `target-trc=hlg` with
  `target-colorspace-hint=yes` switches the Metal layer to HDR. On an SDR display macOS tone-maps it down.
- `sdr`, on `Ctrl+s`: DJI's 33³ D-Log M → Rec.709 LUT at `HOOK MAIN`. mpv's color pipeline then runs a
  BT.709 → BT.709 identity, so the LUT's output reaches the display unchanged.

The LUTs are baked into the shaders as 3D `rgba16` textures (`tools/cube_to_glsl.py`, templates in
`shaders/`), so the GPU does the trilinear interpolation in one tap.

### Gyroflow

The gyroflow plugin (`mpv/scripts/gyroflow/main.lua`) adds `vf=vapoursynth` with its `gyroflow.vpy`, which runs the
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
mpv/scripts/dlogm/            the D-Log M plugin  →  ~/.config/mpv/scripts/dlogm (+ shaders/, luts/ at install),
                              and in Steady Player
mpv/scripts/gyroflow/         the stabilization plugin  →  ~/.config/mpv/scripts/gyroflow (+ the binaries), and in
                              Steady Player
mpv/scripts/gyroflow/underwater.gyroflow  the default preset for clips without a project
mpv/mpv.conf                  recommended settings, installed when there's no mpv.conf
shaders/*.glsl.in             shader templates; the LUT goes in at install
luts/dlogm_hlg.cube           D-Log M → Rec.2100 HLG LUT (DJI's Rec.709 one is downloaded)
tools/cube_to_glsl.py         .cube → mpv shader
vs-gyroflow/                  Gyroflow VapourSynth plugin, gyroflow-probe, gyroflow-embed, gyroflow-export (Rust)
macos/                        Steady Player: the applet, its mpv launcher, setup.sh (default app, Quick Actions,
                              DJI's LUT), default-app.py, the Quick Actions' export script
tools/make_app.sh             builds Steady Player.app, with the plugins in it
tools/make_workflow.py        writes the Finder Quick Actions
tools/make_release.sh         release files: Steady Player, each plugin alone, and the tree plus prebuilt binaries
.github/workflows/release.yml builds the release on macOS and publishes it for a pushed v* tag
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
