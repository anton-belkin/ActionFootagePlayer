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
- **Depth and the rest of the camera's telemetry over the video.** What the camera recorded frame by frame
  (depth under water, pressure, GPS with a remote connected, exposure) shown as mpv plays, from the file itself.

It's three mpv plugins, each a folder in `~/.config/mpv/scripts/` that works without the others, and Steady
Player, a Mac app that carries them and opens your MP4s with them:

- **`dlogm`**: D-Log M recognized from the camera's metadata, and shown in HDR or through DJI's Rec.709 LUT. Plain
  Lua and GPU shaders, so it runs in any mpv on any system (`vo=gpu-next`).
- **`gyroflow`**: the stabilization and the export, with the binaries built from `vs-gyroflow/` (macOS, Apple Silicon).
- **`dji-telemetry`**: the camera's telemetry over the video (DJI, GoPro). Plain Lua, any mpv on any system.

None needs anything in `mpv.conf` or `input.conf`. Made for the DJI Osmo Action 6 on Apple Silicon; the
Gyroflow part works with any camera Gyroflow supports when there's a project. Other DJI models, GoPro and Insta360
are on the way, and need clips from their owners: see [Other cameras, and how to help](#other-cameras-and-how-to-help).

## Install

Needs a Mac with Apple Silicon (macOS 11 or later), [Homebrew](https://brew.sh) mpv (`brew install mpv`, which
includes VapourSynth) and Homebrew ffmpeg for exporting (`brew install ffmpeg`).

**Steady Player**, the app (it carries the plugins; dji-telemetry from the release after 0.3.0): download
`Steady-Player-0.3.0-macos-arm64.zip` from the
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

- `mpv-dji-telemetry-<version>.zip` (releases after 0.3.0), on any system: plain Lua.

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

`./install.sh` alone installs the three plugins into `~/.config/mpv/scripts/` (and an `mpv.conf` with a few
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

Once Steady Player is set up, double-clicking an MP4 plays it in mpv with the plugins; for other videos, open
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
| `Shift+E` | Export the clip as it plays; with an A-B loop (`l`), only that part. On a clip being exported, twice: cancel | `gyroflow/export` |
| `Ctrl+e` | Open the clip's project in Gyroflow (written next to the clip first when it has none); saving it there reloads the stabilization here | `gyroflow/edit` |
| `Ctrl+t` | Telemetry off/on; it sticks for the next clips | `dji_telemetry/toggle` |

To use other keys, bind the names in `input.conf`, e.g. `Ctrl+d script-binding dlogm/sdr`.

### Options

In `~/.config/mpv/script-opts/<plugin>.conf`, or on the command line as `--script-opts=dlogm-mode=sdr` (a value
with commas: `--script-opts-append=dji_telemetry-fields=depth,pressure`):

| | |
|---|---|
| `dlogm.conf`: `mode=hdr` | what D-Log M clips start in: `hdr`, or `sdr` (DJI's Rec.709 LUT) |
| `gyroflow.conf`: `hwdec=yes` | decode the clips it stabilizes with VideoToolbox whatever mpv's `hwdec` is (software decoding doesn't keep up with 4K); `no` leaves it to `hwdec` |
| `gyroflow.conf`: `workers=3` | GPU worker threads for the stabilization |
| `gyroflow.conf`: `max_fps=30` | clips faster than this play every other frame (60 fps at 30), so the stabilization does half the work; `0`: every frame |
| `gyroflow.conf`: `edges=trail` | what fills the edges the stabilized frame doesn't cover: `trail`, `original`, `zoom`, or `off` (the project's background color) |
| `gyroflow.conf`: `edges_blur=2`, `edges_zoom=1.5` | blur of the edge fill (% of the frame height), and how much `zoom` scales the frame up |
| `dji_telemetry.conf`: `fields=depth,water` | what to show, in that order (see Telemetry) |
| `dji_telemetry.conf`: `units=metric` | or `imperial`: feet, mph, °F |
| `dji_telemetry.conf`: `position=bottom-left` | `top-left`, `top-right`, `bottom-right` (top left has mpv's messages, top right the exports' progress) |
| `dji_telemetry.conf`: `font_size=28` | at a 720-pixel-high window; it scales with the window |
| `dji_telemetry.conf`: `visible=yes` | `no`: shown only after `Ctrl+t` |

The telemetry plugin's file is `dji_telemetry.conf`, with an underscore: mpv names a script after its folder, with
anything but letters and digits made `_`.

### Stabilization

Make the project in Gyroflow and save it next to the clip (`Ctrl+S` in Gyroflow): mpv uses it. Or start from
what's playing: `Ctrl+e` pauses the clip and opens its project in Gyroflow. A clip that plays without a
`.gyroflow` next to it (a project stored in the video, or the default preset) gets one first, written by the
plugin from what it renders: the same lens profile, smoothing, zoom and refraction, so Gyroflow starts from
what you saw. A `.gyroflow` that's already there is opened as it is, never overwritten. Each time you save in
Gyroflow (`Cmd+S` overwrites the project it opened), mpv reloads the stabilization with it, at the same spot
in the clip. The project then stays next to the clip, and mpv plays the clip with it from then on.

To carry one file instead of two, move the project into the video (the commands here are in
`~/.config/mpv/scripts/gyroflow/`; with just Steady Player, in
`/Applications/Steady Player.app/Contents/Resources/plugins/gyroflow/`):

```bash
~/.config/mpv/scripts/gyroflow/gyroflow-embed --move DJI_0009_D.MP4     # DJI_0009_D.gyroflow → inside the MP4
~/.config/mpv/scripts/gyroflow/gyroflow-embed --extract DJI_0009_D.MP4  # back out, to open it in Gyroflow (Ctrl+e does it too)
~/.config/mpv/scripts/gyroflow/gyroflow-embed --remove DJI_0009_D.MP4
```

The project is appended to the MP4 as a box of its own: the video data isn't touched, the file keeps its
date, players and Gyroflow open it as before, and it moves with the file. Apps that rewrite the file
(trimming, remuxing) drop it.

Clips with gyro data of their own, recorded with the camera's stabilization off, play stabilized even without a
project: DJI with EIS off, and (not yet tried on a real clip, see Other cameras) GoPro with HyperSmooth off and
Insta360 with FlowState off in the camera, when Gyroflow's database has their lens. Only when the camera recorded
that it was off: footage the camera stabilized already gets worse stabilized again, so a clip that doesn't say
plays as it is. These clips get Gyroflow's default preset
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
D-Log M the same LUT. With an A-B loop set (`l` twice), only that part. Playback goes on meanwhile, with the
progress in the top right corner (several clips can export at once, a line each); a notification says where
the file went. `Shift+E` twice on a clip being exported cancels it: the export stops and the unfinished file is
removed. Quitting mpv cancels the exports too.

The export and playback share the GPU and the video encoder/decoder. At the same priority a 4K60 clip
stabilized in mpv plays at half speed during an export (on an M1 Pro: 480 frames dropped in 20 s instead of
40). So while the video plays, the export runs at background priority, macOS's lowest for CPU, disk and GPU:
playback is as smooth as without it, but the export nearly stops ("slowed while playing"). Pause, and it goes on
at full speed. In short: start the export, then pause to let it run, or keep watching and let it catch up
whenever you pause.

The same from a terminal, for one clip or many:

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
- `Ctrl+C` stops it and removes the unfinished file. `--background` runs it at background priority, to keep
  working while it runs; `kill -USR1 <pid>` / `kill -USR2 <pid>` switch that later. `--progress <file>`
  writes the progress there as JSON twice a second (that's how mpv shows it).

A 4K60 clip exports at ~35 fps on an M1 Pro (Lanczos interpolation; `--interpolation bicubic` is what
playback uses). The video is decoded once and encoded once: ffmpeg decodes it, the plugin's own engine
stabilizes it on the GPU, the LUT is applied on the GPU too (`vs-gyroflow/src/gpu_lut.rs`, the shader's math),
and ffmpeg encodes it with VideoToolbox.

### Telemetry

DJI cameras record, next to the video, a message per frame with what the camera knew at that moment, and GoPro
cameras about a second of it at a time (GPMF). The dji-telemetry plugin reads all of it when a clip opens and
shows, by default, the depth and whether the camera is in the water, bottom left. Each field shows when the clip
has it:

| `fields=` | |
|---|---|
| `depth` | under water: the barometer's altitude below the surface, in 0.1 m steps, about once a second. The camera starts recording it a second or two after it goes in |
| `water` | *In water* / *Out of water*: the barometer's flag, or the camera's own underwater confidence, which comes from the picture and so also works in a dive housing |
| `pressure` | hPa, ~100 more per meter of water |
| `altitude` | the barometer's altitude out of water. It's relative and drifts by meters, so it's a rough guide |
| `gps` | position, when the camera has a fix (DJI: a remote or Mimo connected) |
| `speed` | ground speed from the GPS (Osmo 360 on an e-bike: the bike's) |
| `temperature` | Osmo 360's image sensor, not the water |
| `heartrate` | from a device paired with an Osmo 360 |
| `exposure` | ISO, shutter, aperture, white balance |

GoPro records GPS, shutter, ISO and white balance, and no barometer: for its clips set `fields=gps,speed,exposure`.

**In a dive housing the depth isn't recorded.** The barometer then measures the air in the housing (1050-1100 hPa
whatever the depth), and the camera doesn't set its in-water flag; `water` still shows *In water*, from the picture.
The Osmo Action 6 records no temperature at all: no clip had any.

Models: the Osmo Action 6 (its schema isn't published; the layout was worked out from its recordings with
`tools/dji_meta_dump.py`, below) and the Osmo 360 (from its schema). No Action 6 clip here had a GPS fix, so its
`gps` and `speed` follow the Osmo 360's layout and haven't been seen working yet. GoPro: checked on GoPro's own
sample clips (HERO5 to 8, MAX, Fusion), against what telemetry-parser reads from them. Clips from other cameras, and
files that aren't from a camera at all (exports included), show nothing.

To look at what a clip recorded, or to work out another model's layout:

```bash
python3 tools/dji_meta_dump.py summary DJI_0072_D.MP4                       # every field: min, max, how often
python3 tools/dji_meta_dump.py timeline DJI_0072_D.MP4 3.2.13.2 3.2.13.3 -s 2  # some fields over time
```

## Other cameras, and how to help

Everything here was worked out on clips from an Osmo Action 6. The camera's metadata says what the plugins
need (whether the camera stabilized the picture itself, the color profile, the telemetry), but each maker, and
often each model, writes it differently, and without clips there's nothing to check a guess against. Where
things stand:

| | Stabilization without a project | Log footage (HDR, Rec.709 LUT) | Telemetry |
|---|---|---|---|
| DJI Osmo Action 6 | yes | D-Log M | yes |
| DJI Osmo 360 | EIS off (`eis_status`), single-lens MP4, not seen | D-Log M, from its schema, not seen | from its schema, not seen |
| Other DJI (Action 4, Action 5 Pro, Pocket 3, Osmo 360 II, ...) | EIS off (`eis_status`), not seen | `_D` file name only, a guess | no |
| GoPro | HyperSmooth off (`EISA`/`EISE`), lens from Gyroflow's database; checked only on GoPro's own sample clips | no (GP-Log: the Protune color is in `PTCL`, `FLAT` on a HERO6; what GP-Log writes there isn't known) | GPS, speed, exposure; checked on GoPro's sample clips |
| Insta360 single-lens MP4 (Ace Pro, GO 3, ...) | FlowState off in the camera (`is_flowstate_online`), not seen | no (I-Log) | no |

The stabilization errs on the safe side: a clip is stabilized only when the camera recorded that its own was
off, so a wrong guess about a camera means its clips play as they are, not worse.

**Clips that would help**, straight from the camera (not exported or trimmed by an app: that drops the metadata):

- a few seconds each, any resolution, the camera moving (walking with it is enough);
- the camera's stabilization off, and the same on (DJI: EIS/RockSteady; GoPro: HyperSmooth; Insta360: FlowState);
- in the log profile (D-Log M, GP-Log, I-Log) and in the normal one;
- under water, if the camera records depth or a "wet" flag;
- the model, firmware and settings (lens/FOV, frame rate) in a word.

Share them through a link (Google Drive, Dropbox, WeTransfer, ...) in a
[new issue](https://github.com/anton-belkin/dlogm-gyroflow-player/issues/new). The clips are used only to work out
the camera's metadata; if anything from them goes into the repo, it's the metadata alone as test data (a few kB,
no picture), and only if you're fine with that. The files carry the camera's serial number, and the place they
were shot if the camera had GPS, so shoot somewhere you don't mind showing.

If you can't share the video, its metadata alone helps a lot:

```bash
python3 tools/dji_meta_dump.py summary DJI_0001_D.MP4 > dji.txt            # DJI
gyro2bb --dump GX010001.MP4 > gopro.txt                                     # any camera, telemetry-parser's tool
```

`gyro2bb` is [telemetry-parser](https://github.com/AdrianEddy/telemetry-parser)'s command-line tool (built from
its `bin/gyro2bb`). telemetry-parser is the library Gyroflow, and through it the stabilization here, reads camera
metadata with; what a clip shows about the camera's motion or lens goes there too, so Gyroflow gets it as well. The
color profile and the telemetry are read in plain Lua by the dlogm and dji-telemetry plugins, which don't depend on
Gyroflow, so they work in any mpv on any system.

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
decides first whether the video carries a project, or its own gyro data recorded with the camera's stabilization
off (where each camera records that: `vs-gyroflow/src/probe.rs`). Frames are
stabilized plane by plane at the source bit depth, so 10-bit D-Log M reaches the shaders as 10-bit; 8K is
scaled to 4K on the GPU first. Details, measurements and the plugin's options: [vs-gyroflow/README.md](vs-gyroflow/README.md).

The plugin builds on `gyroflow-core` from Gyroflow master, which has the Static zoom modes
([gyroflow/gyroflow#1264](https://github.com/gyroflow/gyroflow/pull/1264)) it renders; nothing else is needed
beyond stock Gyroflow.

### Telemetry

The metadata track (the first with a `meta` handler, as for D-Log M) holds one protobuf message per frame,
`ProductMeta` in DJI's `dvtm_*.proto`: the clip's and the stream's metadata in the first, then `FrameMeta` in each.
`mpv/scripts/dji-telemetry/telemetry.lua` finds every sample through the track's sample table (`stts`, `stsc`,
`stsz`, `stco`/`co64`), reading each chunk of samples at once, and decodes just the fields the chosen `fields=` need,
along paths of field numbers per model (`LAYOUTS`, by `proto_file_name`). Each value is kept only where it changes;
`main.lua` looks it up by `time-pos` and redraws the OSD overlay (ASS) when the text changes. A 3.7-minute 4K60 clip
(2.5 GB) is read in 0.4 s.

On the Action 6 the barometer is `Barometer { is_in_water = 1; altitude_dm = 2; atmospheric_hpa = 3; }` at
field 13 of the frame's camera part (`3.2.13`); the other camera fields are numbered as on the Osmo 360.

GoPro's metadata track is the one whose samples are `gpmd`: GPMF, nested KLV items (`DEVC` → `STRM` → `GPS5`,
`SHUT`, ...), big-endian, each scaled by the `SCAL` before it. A sample holds about a second; its values are spread
evenly over it. GPS5 (HERO5-10) has the fix for the whole sample in `GPSF`, GPS9 (HERO11 and later) in each row.

## Layout

```
install.sh                    builds and installs everything
mpv/scripts/dlogm/            the D-Log M plugin  →  ~/.config/mpv/scripts/dlogm (+ shaders/, luts/ at install),
                              and in Steady Player
mpv/scripts/gyroflow/         the stabilization plugin  →  ~/.config/mpv/scripts/gyroflow (+ the binaries), and in
                              Steady Player
mpv/scripts/gyroflow/underwater.gyroflow  the default preset for clips without a project
mpv/scripts/dji-telemetry/    the telemetry plugin  →  ~/.config/mpv/scripts/dji-telemetry
mpv/mpv.conf                  recommended settings, installed when there's no mpv.conf
shaders/*.glsl.in             shader templates; the LUT goes in at install
luts/dlogm_hlg.cube           D-Log M → Rec.2100 HLG LUT (DJI's Rec.709 one is downloaded)
tools/cube_to_glsl.py         .cube → mpv shader
tools/dji_meta_dump.py        what's in a DJI clip's metadata track: every field, or some over time
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

Camera metadata (motion data, lens, in-camera stabilization) is read by Adrian Eddy's
[telemetry-parser](https://github.com/AdrianEddy/telemetry-parser), through Gyroflow.

`luts/dlogm_hlg.cube` (D-Log M → Rec.2100 HLG, 1000-nit, no DaVinci DRT) was composed in DaVinci Resolve
from [Zeb Gardner](https://www.zebgardner.com/)'s free D-Log M → DaVinci Wide Gamut/Intermediate shaper LUT
(`DLOGM_DWG_I_SHAPER_ZRG_V1`); the HLG output stage and the bake are this project's. Thanks to Zeb Gardner for
the underlying transform. His site doesn't state a license for the shaper; it's used here with credit, and
will be changed if he asks.
