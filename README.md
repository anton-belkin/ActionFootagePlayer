# Action Footage Player

**Your action camera's best footage, the way it should look. Just double-click the clip.**

A free, open-source video player for footage from action cameras (drones are next). Clips shot in the camera's
log color mode play in full color and HDR, shaky clips are steadied as they play (under water too), and what the
camera measured, like depth or speed, shows on screen. No import, no editing, nothing to render first, no account
or cloud; your original files are never changed. When a moment is worth sharing, one key saves exactly that part,
ready for your phone.

<!-- HERO, the most important thing on this page:
     1. one frame side by side: as the camera recorded it (flat, grey) | as it plays here
     2. a 5-second GIF: an underwater clip, shaky | stabilized, with the depth in the corner -->

> **Status: early (alpha).** In daily use with the **DJI Osmo Action 6** on **Macs with Apple Silicon**. Other DJI
> cameras, GoPro and Insta360 are built for but not yet tried on real footage; Linux and Windows have the plugins
> only, not yet tried. [What works with your camera](#what-works-camera-by-camera) ·
> [Help it work with yours](#help-it-work-with-your-camera)

## What you get

- **Log footage in full color, straight away.** Log (D-Log M on DJI, GP-Log on GoPro, I-Log on Insta360) is the
  camera's mode that records the most, but its clips come out flat and grey and normally need color grading. Here
  they play in HDR, or with the camera maker's own standard look. The clip says it's log, so there's nothing to set
  up. Normal clips play as usual.
- **Shaky clips steadied as they play**, from the motion data the camera records when its own stabilization is off,
  with [Gyroflow](https://gyroflow.xyz)'s engine. Under water too, where the camera's stabilization doesn't hold.
- **What the camera measured, on screen**: depth under water, GPS and speed, exposure.
- **One key to share**: mark the moment, press `Shift+E`, and get a stabilized file that an iPhone, a Mac or
  YouTube shows in HDR.

## Who it's for

**You just want your footage to look its best**, without learning to grade. An action camera's best picture is
usually hidden behind a "pro" mode, because on an ordinary screen it needs creative grading to look good. On a
screen with HDR (MacBook Pro, recent iPhones and flagship phones) a technical conversion is enough, the same for
every clip, so it's done automatically: log looks good straight away. So you can shoot in the best mode and simply
watch and share. [Why that works](#why-log-and-why-hdr)

**You already shoot log and grade.** This is the step between the card and your editor: go through a card's worth
of clips with a fixed technical transform (D-Log M → Rec.2100 HLG, no creative look), or DJI's own Rec.709 LUT;
see the gyro stabilization without making a project; open the clip in Gyroflow with one key and see the result back
in playback when you save; batch-export ProRes 422 HQ from the command line. 10-bit stays 10-bit, the originals are
never re-encoded, and log is recognized from the camera's metadata, not the file name. Prefer your own mpv? The
three plugins work on their own, on any system. [How it works](#how-it-works)

## Quick start (Mac with Apple Silicon, DJI Osmo Action 6)

1. **On the camera:** set the color mode to **D-Log M**. Under water, turn **EIS off**: the camera then records the
   motion data the stabilization uses.
2. **Install:** with [Homebrew](https://brew.sh), run `brew install mpv ffmpeg` in Terminal. Then download
   `ActionFootagePlayer-<version>-macos-arm64.zip` from the
   [latest release](https://github.com/anton-belkin/ActionFootagePlayer/releases/latest), unzip it and move it to
   Applications.
3. **Open it once.** The first time, macOS blocks it, because it isn't signed with a paid Apple developer account:
   System Settings → Privacy & Security → **Open Anyway**. Accept its setup: it becomes the player for MP4 videos.
4. **Copy the clips off the card and double-click one.**

`l` twice marks a part, `Shift+E` exports it, `Ctrl+s` switches to the standard look instead of HDR, `Ctrl+g` turns
the stabilization on and off. [All keys](#keys) · [Install in detail](#installing-in-detail)

## Example: back from the dive

You shot the dive in D-Log M, with EIS off because under water it doesn't hold. In the hotel, you copy the card to
the laptop and double-click the first clip: it plays stabilized, in HDR, with the depth in the corner. You skip
through the hour to the 15 seconds with the turtle, press `l` at the start, `l` at the end, `Shift+E`, and pause for
half a minute. The fragment is in a `stabilized/` folder next to the clip, ready to AirDrop to the group the same
evening, in HDR. The originals are untouched for proper editing later.

## Why log, and why HDR

The log mode records the most the camera can: often more than its own HDR mode (GoPro's HERO12 makes HDR by merging
two exposures, while its log is a single one, which holds up better with motion and in low light), and on some
cameras, the Osmo Action 6 among them, log is the only way to HDR at all. On an ordinary (SDR) screen, log needs a
colorist to decide what to keep and what to leave out of SDR's narrow range: a creative job. An HDR screen has room
for all of it, so the conversion becomes a technical one, the same for every clip, and can be automatic. That turns
the "pro" mode into the one to use for everyday footage.

On a Mac without an HDR screen (MacBook Air, most external monitors), macOS tones the picture down, or `Ctrl+s`
shows DJI's standard Rec.709 look. Exports still play in HDR on an iPhone.

## Help it work with your camera

Everything was worked out on one camera, and each maker, often each model, records its metadata differently. Most
useful right now:

- **A few seconds of footage** from a GoPro, an Insta360, another DJI camera or a DJI drone, straight from the card.
  [What to shoot and how to share it](#other-cameras-and-how-to-help); if you can't share the video, its metadata
  alone helps.
- **Trying it on Linux or Windows**, and telling how it went.
- **Knowing GP-Log or I-Log well**: their color transforms are the missing piece for GoPro and Insta360.
- **Code**: Lua (the plugins), Rust (`vs-gyroflow/`), GLSL shaders. Issues and pull requests welcome.

---

## What works, camera by camera

In the Mac app, with clips straight from the camera (an app that exports or trims them drops the metadata
this reads):

| | Log in color: HDR, Rec.709 LUT ¹ | Stabilized from the camera's gyro, no project ² | Stabilized with a Gyroflow project ² | Telemetry ³ | Export what you see ² |
|---|---|---|---|---|---|
| DJI Osmo Action 6 | **verified** (D-Log M) | **verified** (EIS off) | **verified** | **verified**: depth, in water. Should work: the rest; GPS not seen | **verified**: HDR. Should work: Rec.709 |
| DJI Osmo 360 (single-lens MP4) | should work (D-Log M) | should work (EIS off) | should work | should work | should work |
| Other DJI cameras (Action 4, Action 5 Pro, Pocket 3, ...) | a guess, by the `_D` file name | should work (EIS off) | should work | not yet | should work |
| DJI drones | not yet | not yet | should work | not yet | should work, with a project |
| GoPro | not yet (GP-Log) | should work (HyperSmooth off) | should work | should work: GPS, speed, exposure | should work |
| Insta360 single-lens MP4 (Ace Pro, GO 3, ...) | not yet (I-Log) | should work (FlowState off) | should work | not yet | should work |
| Other cameras Gyroflow supports | | not yet | should work | not yet | should work, with a project |

**verified**: played in the app on a Mac, on real footage. **should work**: built for it, from the maker's
published metadata layout or sample clips, but not tried on real footage yet. **not yet**: doesn't work yet.
Export renders what plays, so it covers the clips that play stabilized. Details, and how clips from your camera
would help: [Other cameras, and how to help](#other-cameras-and-how-to-help).

The table is for the Mac app (Apple Silicon). Nothing has been tried on Linux or Windows yet, where there's no app,
only the plugins in an mpv you start yourself:

1. D-Log M is plain Lua and GPU shaders, for any mpv on any system. Whether HDR reaches the screen there depends on
   mpv and the system. The Rec.709 LUT is baked in from a checkout of this repo (`install.sh` does it on Linux).
2. The stabilization and the export need an mpv built with VapourSynth, and binaries for macOS on Apple Silicon,
   Linux x86-64 or Windows x86-64: see [Linux and Windows](#linux-and-windows). Projects are made in Gyroflow.
3. Plain Lua, any mpv on any system.

## Install

From the [latest release](https://github.com/anton-belkin/ActionFootagePlayer/releases/latest), take:

| For | File |
|---|---|
| a Mac with Apple Silicon (macOS 11 or later) | `ActionFootagePlayer-<version>-macos-arm64.zip`: the app, with all three plugins in it |
| D-Log M and telemetry in your own mpv, on any system | `mpv-dlogm-<version>.zip`, `mpv-dji-telemetry-<version>.zip` |
| stabilization and export in your own mpv | `mpv-gyroflow-<version>-macos-arm64.zip` (Homebrew's mpv), `-linux-x86_64.zip`, `-windows-x86_64.zip` |

**The app** needs [Homebrew](https://brew.sh)'s mpv (it includes VapourSynth), and its ffmpeg for exporting:

```bash
brew install mpv ffmpeg
```

Unzip ActionFootagePlayer, move it to Applications and open it. It's signed ad hoc, not by an Apple developer
account, so the first time macOS won't open it: System Settings → Privacy & Security → Open Anyway. Opened on its
own, it offers its setup:

- it becomes the default app for MP4 videos (what DJI cameras record): double-clicking one plays it with all
  of this, and any other video simply plays in mpv. macOS asks you to confirm the change in a dialog of its own;
  it applies once you do;
- Finder gets the Quick Actions *Play Stabilized* and *Stabilize + LUT* (see [Opening files](#opening-files));
- DJI's Rec.709 LUT is downloaded from DJI (it isn't redistributed here) and checked, for `Ctrl+s` and Rec.709
  exports. HDR works without it.

Coming from Steady Player, as the app was called up to 0.3.0: see [Upgrading](#upgrading).

**The plugins**: unzip them into mpv's `scripts/` folder (`~/.config/mpv/scripts/`, `%APPDATA%\mpv\scripts\` on
Windows); nothing goes in `mpv.conf` or `input.conf`. DJI's Rec.709 LUT and the stabilization take a step more:
see [Plugins in your own mpv](#plugins-in-your-own-mpv) and [Linux and Windows](#linux-and-windows).

## Using it

### Opening files

Once ActionFootagePlayer is set up, double-clicking an MP4 plays it in mpv with the plugins; for other videos, open
them with ActionFootagePlayer from Finder's Open With, or right click → Quick Actions:

- **Play Stabilized** opens the selected videos in ActionFootagePlayer.
- **Stabilize + LUT** exports them (see [Exporting](#exporting)) in the background, with a notification as each
  one is done.

### Keys

| Key | | `script-binding` |
|---|---|---|
| `Ctrl+g` | Stabilization on/off | `gyroflow/toggle` |
| `Ctrl+b` | What fills the edges the stabilized frame doesn't cover: the project's background, the original frame, the frame zoomed, or a trail of earlier frames, by place on the screen or by direction (see Stabilization); it sticks for the next clips | `gyroflow/edges` |
| `Ctrl+[` / `Ctrl+]` | Less or more blur of the edge fill (0-10 % of the frame height, shown on screen); it sticks for the next clips | `gyroflow/blur-less`, `gyroflow/blur-more` |
| `Ctrl+s` / `Ctrl+h` | D-Log M through DJI's own Rec.709 LUT instead of HDR, and back; it sticks for the next D-Log M clips | `dlogm/sdr`, `dlogm/hdr` |
| `L` | D-Log M handling off for this clip, for `_D` files that aren't D-Log (e.g. HDR photos) | `dlogm/off` |
| `Shift+E` | Export the clip as it plays; with an A-B loop (`l`), only that part. On a clip being exported, twice: cancel | `gyroflow/export` |
| `Ctrl+e` | Open the clip's project in Gyroflow (written next to the clip first when it has none); saving it there reloads the stabilization here | `gyroflow/edit` |
| `Ctrl+t` | Telemetry off/on; it sticks for the next clips | `dji_telemetry/toggle` |

To use other keys, bind the names in `input.conf`, e.g. `Ctrl+d script-binding dlogm/sdr`.

## In detail

### Options

In `~/.config/mpv/script-opts/<plugin>.conf`, or on the command line as `--script-opts=dlogm-mode=sdr` (a value
with commas: `--script-opts-append=dji_telemetry-fields=depth,pressure`):

| | |
|---|---|
| `dlogm.conf`: `mode=hdr` | what D-Log M clips start in: `hdr`, or `sdr` (DJI's Rec.709 LUT) |
| `gyroflow.conf`: `hwdec=yes` | decode the clips it stabilizes with VideoToolbox (Linux, Windows: `auto-copy`) whatever mpv's `hwdec` is (software decoding doesn't keep up with 4K); `no` leaves it to `hwdec` |
| `gyroflow.conf`: `workers=3` | GPU worker threads for the stabilization |
| `gyroflow.conf`: `max_fps=30` | clips faster than this play every other frame (60 fps at 30), so the stabilization does half the work; `0`: every frame |
| `gyroflow.conf`: `edges=sphere` | what fills the edges the stabilized frame doesn't cover: `sphere`, `trail`, `original`, `zoom`, or `off` (the project's background color) |
| `gyroflow.conf`: `edges_blur=2`, `edges_zoom=1.5` | blur of the edge fill (% of the frame height; `Ctrl+[` and `Ctrl+]` change it while playing), and how much `zoom` scales the frame up |
| `gyroflow.conf`: `gyroflow_app=` | Linux, Windows: the Gyroflow executable `Ctrl+e` opens projects with |
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
`~/.config/mpv/scripts/gyroflow/`; with just ActionFootagePlayer, in
`/Applications/ActionFootagePlayer.app/Contents/Resources/plugins/gyroflow/`):

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
and then `default.gyroflow`: yours in `~/Library/Application Support/ActionFootagePlayer/gyroflow/` if you put one
there, else the plugin's (`~/.config/mpv/scripts/gyroflow/`, or in ActionFootagePlayer). Why a clip isn't stabilized is in mpv's log
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
them while playing, and `Ctrl+[` / `Ctrl+]` set how blurred:

- **sphere**, the default: what was last seen in that direction, by the gyro. It's taken from the
  whole source frame, not just what the output shows, so it also has what the zoom crops away and what the camera
  saw while shaking with no edges showing, and it stays where it was in the world, so it lines up with the picture
  through a pan. Only the camera's turning is known: near things that moved past it, or by themselves, are off.
  Where a direction wasn't seen yet, the trail (half-life 5 s).
- **trail**: what was last shown at that spot in earlier frames. The stabilized frame stays put while the camera
  shakes, so a part it uncovers now was usually in view a moment ago. But it's only right while the smoothed view
  holds still: when it pans, what was at a spot of the screen was in another direction, and the fill slides
  against the picture. Where nothing was yet (the first frames, after a seek or a cut), the zoom fill; older trail
  fades into it too (half-life 3 s).
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

## Installing, in detail

### Plugins in your own mpv

Each plugin is a folder in mpv's `scripts/` folder (`~/.config/mpv/scripts/`) and works without the others:

- `mpv-dlogm-<version>.zip`, on any system (`%APPDATA%\mpv\scripts\` on Windows). It has the HDR shader; the
  Rec.709 one (`Ctrl+s`) is made from DJI's LUT: download it from DJI (Osmo Action 6 downloads page) and bake it in
  from a checkout of this repo:

  ```bash
  python3 tools/cube_to_glsl.py shaders/dlogm_lut3d.glsl.in "DJI OSMO Action 6 D-LogM to Rec.709 LUT-11.17.cube" \
      ~/.config/mpv/scripts/dlogm/shaders/dlogm_lut3d.glsl
  ```

- `mpv-dji-telemetry-<version>.zip`, on any system: plain Lua.

- `mpv-gyroflow-<version>-macos-arm64.zip`, for Homebrew's mpv on Apple Silicon (`brew install mpv ffmpeg`), with
  its binaries. Downloaded with a browser, they carry the quarantine flag, and macOS would refuse them:
  `xattr -dr com.apple.quarantine ~/.config/mpv/scripts/gyroflow`. Linux's and Windows': below.

### With install.sh

From the release's tarball (prebuilt, no Rust needed) or from source (needs Rust, [rustup](https://rustup.rs);
the first build takes 5-10 minutes):

```bash
# ActionFootagePlayer-<version>-macos-arm64.tar.gz from the latest release, then:
tar -xzf ActionFootagePlayer-*-macos-arm64.tar.gz
cd ActionFootagePlayer-*-macos-arm64     # or: git clone https://github.com/anton-belkin/ActionFootagePlayer.git
./install.sh --apps
```

`./install.sh` alone installs the three plugins into `~/.config/mpv/scripts/` (and an `mpv.conf` with a few
recommended settings, when there's none yet); `--apps` also builds ActionFootagePlayer into `~/Applications` and runs
its setup, and once it's installed, later runs update it too. It downloads DJI's LUT and checks it, bakes the
LUTs into shaders and installs everything; existing files it replaces are kept as `*.bak-<date>` (for
`scripts/`, in `scripts-backup/`, where mpv doesn't load them).

### Linux and Windows

The gyroflow plugin runs there too, with binaries of its own; dlogm and dji-telemetry are the same zips as on a Mac.
None of it has been tried there by hand yet (the release builds it on Linux and Windows). It needs:

- **mpv built with VapourSynth**: `mpv --vf=help` lists `vapoursynth`. On Windows, the usual builds
  ([shinchiro's](https://github.com/shinchiro/mpv-winbuild-cmake/releases)) have it. On Linux, it depends on the
  distribution: Debian's and Ubuntu's mpv doesn't have it, and needs building with `-Dvapoursynth=enabled`.
- **[VapourSynth](https://github.com/vapoursynth/vapoursynth/releases)** itself, with its Python.
- **A GPU with Vulkan** (on Windows DirectX 12 or Vulkan), which the stabilization renders on. Without one,
  Gyroflow's engine falls back to the CPU, far from realtime.
- **ffmpeg** for exporting: in `PATH`, or on Windows `ffmpeg.exe` and `ffprobe.exe` copied next to
  `gyroflow-export.exe`.

Unzip `mpv-gyroflow-<version>-linux-x86_64.zip` or `mpv-gyroflow-<version>-windows-x86_64.zip`
into mpv's `scripts/` folder (`~/.config/mpv/scripts/`, `%APPDATA%\mpv\scripts\`), with `mpv-dlogm-<version>.zip`
next to it for D-Log M. On Linux, `./install.sh` from a checkout also builds and installs all three plugins (with
DJI's LUT, as on a Mac); `--apps` is macOS's.

What differs from macOS:

- Decoding is mpv's `hwdec=auto-copy` (VAAPI, NVDEC, D3D11...) for clips it stabilizes, and video bigger than 4K is
  scaled down to 4K on the CPU.
- `Ctrl+e` starts `gyroflow` from `PATH` on Linux, and on Windows the app `.gyroflow` files open with (Gyroflow's
  installer sets it). Another one: `gyroflow_app=<path>` in `gyroflow.conf`.
- Exports are encoded with the first of NVENC, VAAPI (Linux), Quick Sync and AMF (Windows) that works, else x265 on the
  CPU (much slower); `--prores` is ffmpeg's `prores_ks`. While the video plays, an export runs at a few frames a
  second rather than at a background priority. Notifications are `notify-send` on Linux; on Windows, mpv's own
  message.
- The log is `~/.local/state/vs-gyroflow.log` on Linux and `%LOCALAPPDATA%\vs-gyroflow.log` on Windows.

### What the app keeps where

ActionFootagePlayer uses the plugins it carries (`Contents/Resources/plugins/`), or the copy in `~/.config/mpv/scripts/`
when there is one (`install.sh` puts them there for mpv in a terminal), so they're never loaded twice. It reads
your `~/.config/mpv` (`mpv.conf`, `input.conf`, `script-opts/`) like any mpv. The app is read-only once signed;
what's yours is in `~/Library/Application Support/ActionFootagePlayer/`: DJI's LUT and its shader (`dlogm/`), and
your own default preset if you make one (`gyroflow/default.gyroflow`, see Stabilization).

ActionFootagePlayer starts mpv through a small launcher app inside it (`open -na`): started from the applet directly,
mpv stayed tied to it and macOS throttled it as a background process.

### Upgrading

**From 0.3**, when the app was called Steady Player (its files were `Steady-Player-<version>-macos-arm64.zip` and
`dlogm-gyroflow-player-<version>-macos-arm64.tar.gz`), `install.sh --apps` replaces it with ActionFootagePlayer:
the old app goes, the new one becomes the default for MP4 videos, and what was yours in
`~/Library/Application Support/Steady Player/` (DJI's LUT, your `default.gyroflow`) moves to
`~/Library/Application Support/ActionFootagePlayer/`. With the app alone, the new one moves it the first time it
plays or sets up; delete Steady Player from Applications yourself.

**From 0.2**, `install.sh --apps` removes Steady Router: MP4 goes to ActionFootagePlayer, MOV back to the
player it was taking videos from. `gyroflow-router` is gone with it.

**From 0.1**, `install.sh` moves the old setup over: the loose scripts go to `scripts-backup/`, the
D-Log M keys come out of `input.conf` (the rest of it stays), the `mpv.conf` with the D-Log M profiles is replaced
(kept as a `.bak`), and `~/.config/mpv/gyroflow/` is emptied into the plugin, your `default.gyroflow` included.
The commands moved with it: `gyroflow-embed` and `gyroflow-export` are now in `~/.config/mpv/scripts/gyroflow/`.

## Other cameras, and how to help

Everything here was worked out on clips from an Osmo Action 6. The camera's metadata says what the plugins
need (whether the camera stabilized the picture itself, the color profile, the telemetry), but each maker, and
often each model, writes it differently, and without clips there's nothing to check a guess against. What each
camera's metadata is read from (what has been tried: [What works, camera by camera](#what-works-camera-by-camera)):

| | Stabilization without a project | Log footage (HDR, Rec.709 LUT) | Telemetry |
|---|---|---|---|
| DJI Osmo Action 6 | yes | D-Log M | yes |
| DJI Osmo 360 | EIS off (`eis_status`), single-lens MP4, not seen | D-Log M, from its schema, not seen | from its schema, not seen |
| Other DJI (Action 4, Action 5 Pro, Pocket 3, Osmo 360 II, ...) | EIS off (`eis_status`), not seen | `_D` file name only, a guess | no |
| GoPro | HyperSmooth off (`EISA`/`EISE`), lens from Gyroflow's database; checked only on GoPro's own sample clips, and on a HERO12 clip from the command line (HyperSmooth on: rightly not stabilized) | no (GP-Log: the Protune color is in `PTCL`, `FLAT` on a HERO6; what GP-Log writes there isn't known) | GPS, speed, exposure; checked on GoPro's sample clips, and the exposure on a HERO12 clip |
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
[new issue](https://github.com/anton-belkin/ActionFootagePlayer/issues/new). The clips are used only to work out
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

It's three mpv plugins, each a folder in `~/.config/mpv/scripts/` that works without the others, and
ActionFootagePlayer, a Mac app that carries them and opens your MP4s with them:

- **`dlogm`**: D-Log M recognized from the camera's metadata, and shown in HDR or through DJI's Rec.709 LUT. Plain
  Lua and GPU shaders, so it runs in any mpv on any system (`vo=gpu-next`).
- **`gyroflow`**: the stabilization and the export, with the binaries built from `vs-gyroflow/` (macOS on Apple
  Silicon; Linux and Windows on x86-64, see [Linux and Windows](#linux-and-windows)).
- **`dji-telemetry`**: the camera's telemetry over the video (DJI, GoPro). Plain Lua, any mpv on any system.

None needs anything in `mpv.conf` or `input.conf`.

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
scaled to 4K on the GPU first. It renders the stabilization while mpv plays, the way Gyroflow would render it
(smoothing, zoom, lens, horizon): 4K60 at ~87 fps of capacity on an M1 Pro. Clips up to 30 fps get Lanczos
interpolation, faster ones bicubic (Lanczos holds only ~38 fps at 4K). Details, measurements and the plugin's
options: [vs-gyroflow/README.md](vs-gyroflow/README.md).

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
                              and in ActionFootagePlayer
mpv/scripts/gyroflow/         the stabilization plugin  →  ~/.config/mpv/scripts/gyroflow (+ the binaries), and in
                              ActionFootagePlayer
mpv/scripts/gyroflow/platform.lua  what differs between macOS, Linux and Windows (binaries' names, temp folder)
mpv/scripts/gyroflow/underwater.gyroflow  the default preset for clips without a project
mpv/scripts/dji-telemetry/    the telemetry plugin  →  ~/.config/mpv/scripts/dji-telemetry
mpv/mpv.conf                  recommended settings, installed when there's no mpv.conf
shaders/*.glsl.in             shader templates; the LUT goes in at install
luts/dlogm_hlg.cube           D-Log M → Rec.2100 HLG LUT (DJI's Rec.709 one is downloaded)
tools/cube_to_glsl.py         .cube → mpv shader
tools/dji_meta_dump.py        what's in a DJI clip's metadata track: every field, or some over time
vs-gyroflow/                  Gyroflow VapourSynth plugin, gyroflow-probe, gyroflow-embed, gyroflow-export (Rust)
macos/                        ActionFootagePlayer: the applet, its mpv launcher, setup.sh (default app, Quick Actions,
                              DJI's LUT), default-app.py, the Quick Actions' export script
macos/icon/                   its icon: AppIcon.svg, and the AppIcon.icns made from it (tools/make_icon.sh)
tools/make_app.sh             builds ActionFootagePlayer.app, with the plugins in it
tools/make_icon.sh            macos/icon/AppIcon.svg → AppIcon.icns (renders with Google Chrome)
tools/make_workflow.py        writes the Finder Quick Actions
tools/make_release.sh         release files: ActionFootagePlayer, each plugin alone, and the tree plus prebuilt binaries
tools/make_plugin_zip.sh      the gyroflow plugin alone with its binaries, for Linux or Windows
.github/workflows/release.yml builds the release on macOS, Linux and Windows and publishes it for a pushed v* tag
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

