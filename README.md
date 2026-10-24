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
> cameras, GoPro and Insta360 are built for but not yet tried on real footage. Windows has an app of its own and Linux
> the plugins, neither tried yet. [What works with your camera](#what-works-camera-by-camera) ·
> [Help it work with yours](#help-it-work-with-your-camera)

## What you get

- **Log footage in full color, straight away.** Log (D-Log M on DJI, GP-Log on GoPro, I-Log on Insta360) is the
  camera's mode that records the most, but its clips come out flat and grey and normally need color grading. Here
  they play in HDR, or with the camera maker's own standard look. The clip says it's log (Osmo Action 6, Osmo 360,
  recent GoPros), so there's nothing to set up; for other cameras, one key picks the log curve. Normal clips play
  as usual.
- **Shaky clips steadied as they play**, from the motion data the camera records when its own stabilization is off,
  with [Gyroflow](https://gyroflow.xyz)'s engine, at the same time as the log conversion.
- **Steady footage under water. Yes, really.** Every diver knows in-camera stabilization gives up under water. Turn
  it off, and the clip plays steady here: stabilized from the camera's own motion data, corrected for how water
  bends light (on a test clip, the jitter between frames went from 0.16 px to 0.02 px).
  <!-- before/after GIF of an underwater clip here: this is the proof divers need -->
- **What the camera measured, on screen**: depth under water, GPS and speed, exposure.
- **The moments you marked, a key away.** Press the camera's highlight button while it records (GoPro's HiLight; on
  a DJI Osmo Action, the quick switch button or the remote): the marks show on the seek bar, and `Ctrl+]` /
  `Ctrl+[` jump to the next one and back, a couple of seconds before it.
- **One key to share**: mark the moment, press `Shift+E`, and get a file as it plays, stabilized or not, that an
  iPhone, a Mac or YouTube shows in HDR.

## Who it's for

**You just want your footage to look its best**, without learning to grade. An action camera's best picture is
usually hidden behind a "pro" mode, because on an ordinary screen it needs creative grading to look good. On a
screen with HDR (MacBook Pro, recent iPhones and flagship phones) a technical conversion is enough, the same for
every clip, so it's done automatically: log looks good straight away. So you can shoot in the best mode and simply
watch and share. [Why that works](#why-log-and-why-hdr)

**You dive.** Shoot in D-Log M with EIS off: back on the boat, the clips play steady, in HDR, with the depth in the
corner, and the turtle is on everyone's phone the same evening. [Back from the dive](#example-back-from-the-dive)

**You already shoot log and grade.** This is the step between the card and your editor. Stabilization and the
log-to-HDR conversion happen together, while the clip plays, and `Shift+E` exports exactly that in one pass:
no project, no render, no separate grading step. In Gyroflow,
stabilization is a render of its own, and the color comes later in an editor. Go through a card's worth of clips
with a fixed technical transform (D-Log M → Rec.2100 HLG, no creative look), or DJI's own Rec.709 LUT; open a clip
in Gyroflow with one key to fine-tune it and see the result back in playback when you save; batch-export ProRes
422 HQ from the command line. 10-bit stays 10-bit, the originals are never re-encoded, and log is recognized from
the camera's metadata, not the file name. Prefer your own mpv? The plugins work on their own, on any system.
[How it works](#how-it-works)

## Quick start (Mac with Apple Silicon, DJI Osmo Action 6)

1. **On the camera:** set the color mode to **D-Log M**. Under water, turn **EIS off**: the camera then records the
   motion data the stabilization uses.
2. **Install:** download `ActionFootagePlayer-<version>-macos-arm64.zip` from the
   [latest release](https://github.com/anton-belkin/ActionFootagePlayer/releases/latest), unzip it and move it to
   Applications. It carries its own mpv and ffmpeg: nothing else to install (macOS 15 or later).
3. **Open it once.** The first time, macOS blocks it, because it isn't signed with a paid Apple developer account:
   System Settings → Privacy & Security → **Open Anyway**. Accept its setup: it becomes the player for MP4 videos.
4. **Copy the clips off the card and double-click one.**

`l` twice marks a part, `Shift+E` exports it, `Ctrl+s` switches to the standard look instead of HDR, `Ctrl+g` goes
through the stabilization, a wider view of it, and off. [All keys](#keys) · [Install in detail](#installing-in-detail)

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
- **Knowing Insta360's 8-bit I-Log well**, or GP-Log on a MAX2: their transforms are the missing piece for those
  cameras (D-Log, GP-Log2 and 10-bit I-Log are done from their makers' formulas, GP-Log from Zeb Gardner's LUT).
- **Code**: Lua (the plugins), Rust (`vs-gyroflow/`), GLSL shaders. Issues and pull requests welcome.

---

## What works, camera by camera

In the Mac app, with clips straight from the camera (an app that exports or trims them drops the metadata
this reads):

| | Log in color: HDR, Rec.709 LUT ¹ | Stabilized from the camera's gyro, no project ² | Stabilized with a Gyroflow project ² | Telemetry ³ | Highlights ³ | Export what you see ² |
|---|---|---|---|---|---|---|
| DJI Osmo Action 6 | **verified** (D-Log M) | **verified** (EIS off) | **verified** | **verified**: depth, in water. Should work: the rest; GPS not seen | should work | **verified**: HDR. Should work: Rec.709 (`Ctrl+s`) |
| DJI Osmo 360 (single-lens MP4) | should work (D-Log M) | should work (EIS off) | should work | should work | none: the camera doesn't mark them | should work |
| Other DJI cameras (Action 4, Action 5 Pro, Pocket 3, ...) | not yet | should work (EIS off) | should work | not yet | should work: Action 4, Action 5 Pro | should work |
| DJI drones | by hand (`Ctrl+l`): HDR from D-Log M or D-Log ⁴ | not yet | should work | not yet | | should work, with a project |
| GoPro | should work: HDR from GP-Log (HERO12 Black on firmware 1.20+, HERO13 Black) and GP-Log2 (MISSION 1) ⁴ | should work (HyperSmooth off) | should work | should work: GPS and speed; exposure on one without GPS | should work: HERO5 and later | should work |
| Insta360 single-lens MP4 (Ace Pro, GO 3, ...) | by hand (`Ctrl+l`): HDR from 10-bit I-Log (X6, Luna) ⁴. Not yet: 8-bit I-Log | should work (FlowState off) | should work | not yet | not yet | should work |
| Other cameras Gyroflow supports | | not yet | should work | not yet | | should work, with a project |

**verified**: played in the app on a Mac, on real footage. **should work**: built for it, from the maker's
published metadata layout or sample clips, but not tried on real footage yet. **not yet**: doesn't work yet.
Export covers what plays: stabilized, in color, or both. Details, and how clips from your camera
would help: [Other cameras, and how to help](#other-cameras-and-how-to-help).

The table is for the Mac app (Apple Silicon). Nothing has been tried on Linux or Windows yet: the Windows app
carries the same plugins, and on Linux they're in an mpv of your own:

1. D-Log M is plain Lua and GPU shaders, for any mpv on any system. Whether HDR reaches the screen there depends on
   mpv and the system. The Rec.709 LUT is baked in from a checkout of this repo (`install.sh` does it on Linux).
2. The stabilization and the export need an mpv built with VapourSynth, and binaries for macOS on Apple Silicon,
   Linux x86-64 or Windows x86-64: see [Linux and Windows](#linux-and-windows). Projects are made in Gyroflow.
3. Plain Lua, any mpv on any system.
4. From the makers' published formulas, or for GP-Log Zeb Gardner's LUT ([Other log curves](#other-log-curves)),
   checked against the makers' own LUTs, not yet on footage from these cameras. HDR only (no Rec.709 LUT yet), in exports too. `Ctrl+l`
   picks the curve for a clip whose metadata doesn't say it.

## Install

From the [latest release](https://github.com/anton-belkin/ActionFootagePlayer/releases/latest), take:

| For | File |
|---|---|
| a Mac with Apple Silicon (macOS 15 or later) | `ActionFootagePlayer-<version>-macos-arm64.zip`: the app, with all the plugins and their menu, mpv and ffmpeg in it |
| a Windows 10 or 11 PC (x86-64) | `ActionFootagePlayer-<version>-windows-x86_64-setup.exe`: the app, with all the plugins and their menu, mpv, VapourSynth and ffmpeg in it |
| D-Log M and telemetry in your own mpv, on any system | `mpv-dlogm-<version>.zip`, `mpv-dji-telemetry-<version>.zip` |
| stabilization and export in your own mpv | `mpv-gyroflow-<version>-macos-arm64.zip` (Homebrew's mpv), `-linux-x86_64.zip`, `-windows-x86_64.zip` |

**The app** carries its own mpv (with VapourSynth and its Python, for the stabilization) and ffmpeg (for
exporting), [Homebrew](https://brew.sh)'s builds of them: nothing else to install. It needs macOS 15 or later, the
oldest macOS those builds are made for.

Unzip ActionFootagePlayer, move it to Applications and open it. It's signed ad hoc, not by an Apple developer
account, so the first time macOS won't open it: System Settings → Privacy & Security → Open Anyway. The first time
it opens, it offers its setup:

- it becomes the default app for MP4 videos (what DJI cameras record): double-clicking one plays it with all
  of this, and any other video simply plays in mpv. macOS asks you to confirm the change in a dialog of its own;
  it applies once you do;
- Finder gets the Quick Actions *Play Stabilized* and *Stabilize + LUT* (see [Opening files](#opening-files));
- DJI's Rec.709 LUT is downloaded from DJI (it isn't redistributed here) and checked, for `Ctrl+s` and Rec.709
  exports. HDR works without it.

Coming from Steady Player, as the app was called up to 0.3.0: see [Upgrading](#upgrading).

**On Windows**, run the installer: it needs nothing else installed and no admin rights (it installs for you alone,
in `%LOCALAPPDATA%\Programs\ActionFootagePlayer`). It isn't signed, so Windows SmartScreen warns about it the first
time: More info → Run anyway. It puts ActionFootagePlayer in Open With for MP4 and MOV videos, and Explorer's menu
for them gets *Play in ActionFootagePlayer* and *Stabilize + LUT* (on Windows 11 under Show more options). It
downloads DJI's Rec.709 LUT, and, if you leave that ticked, opens Settings to make it the default app for MP4
videos: Windows doesn't let an app do that itself. See [The Windows app](#the-windows-app).

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

On Windows, the same two are in Explorer's menu for MP4 and MOV videos: *Play in ActionFootagePlayer*, and
*Stabilize + LUT*, which exports in a window of its own that shows how it goes (videos sent while it runs are
exported after the others, in the same window).
### The menu

Right-click the video for the menu: everything the keys do, with the keys beside it, and what has no key. At the
top, what the clip is (the camera, its log curve, whether it has motion data and telemetry, how many highlights);
then playing, marking a part and exporting it; **Highlights** (the next, the previous, or any one by its time);
**Look** (HDR, the standard look, as recorded, the log curve by hand, noise reduction); **Stabilization** (on,
the wider view, the horizon locked level, what fills the edges, Gyroflow); **Telemetry** (where it shows,
the units, which fields); **Export** (in the standard look, the last export in Finder); **Clip** (the next and
previous clip in the folder, Finder, the path, a still); **Player** (speed, tracks, fullscreen); and Keys…, Help and
Report this camera…, which opens an issue on GitHub with what the plugins found in the clip.

Look, Stabilization and Telemetry each have **Remember as default**: the settings as they are become the plugin's
options (see Options), for the next start. A clip without motion data has **Stabilize in Gyroflow (optical
flow)…** instead: see Stabilization.

The menu is mpv's own, so it needs mpv 0.41 or later (Homebrew's is); with an older mpv, right click pauses as it
always did. It's the footage-menu plugin, which shows only the plugins that are there.

### Keys

| Key | | `script-binding` |
|---|---|---|
| `Ctrl+g` | Stabilization on → on with a wider view (no zoom: the stabilized picture covers about as much of the window as the frame, whatever the lens; the edges show at times, filled as `Ctrl+b` says) → off. On or wider sticks for the next clips | `gyroflow/toggle` |
| `Ctrl+b` | What fills the edges the stabilized frame doesn't cover: the project's background, the original frame, the frame zoomed, or a trail of earlier frames, by place on the screen or by direction (see Stabilization); it sticks for the next clips | `gyroflow/edges` |
| `Ctrl+s` / `Ctrl+h` | D-Log M through DJI's own Rec.709 LUT instead of HDR, and back; it sticks for the next log clips. On an HDR clip (an export): tone-mapped to SDR, and back to HDR, for that clip | `dlogm/sdr`, `dlogm/hdr` |
| `L` | Log handling off for this clip: the file as it is, flat as the camera recorded it (an HDR clip: in HDR) | `dlogm/off` |
| `Ctrl+l` | This clip's log curve, by hand, in HDR: D-Log M, D-Log, GP-Log, GP-Log2, I-Log (10-bit) in turn. For clips whose metadata doesn't say | `dlogm/curve` |
| `Ctrl+n` | Noise reduction on/off: the blotches of color in shadows and under water mostly go, the detail stays. On the GPU, before the log conversion; any clip, log or not. It sticks for the next clips. Playback only: exports are as before | `dlogm/denoise` |
| `Shift+E` | Export the clip as it plays; with an A-B loop (`l`), only that part. On a clip being exported, twice: cancel | `gyroflow/export` |
| `Ctrl+e` | Open the clip's project in Gyroflow (written next to the clip first when it has none); saving it there reloads the stabilization here. A clip without motion data opens in Gyroflow as it is, to stabilize it from the picture | `gyroflow/edit` |
| `Ctrl+t` | Telemetry off/on; it sticks for the next clips | `dji_telemetry/toggle` |
| `Ctrl+]` / `Ctrl+[` | The next highlight the camera marked, and the one before, landing a couple of seconds before it (`before=`); `Ctrl+[` again right after goes on to the one before that. mpv's chapter keys, `Page Up` / `Page Down`, go to them too, at the mark itself | `highlights/next`, `highlights/prev` |
| right click | The menu | `footage_menu/menu` |

To use other keys, bind the names in `input.conf`, e.g. `Ctrl+d script-binding dlogm/sdr`. What the menu sets
without a key, a key can too, with the plugins' script messages (listed at the top of each plugin's `main.lua`),
e.g. `Ctrl+k script-message-to gyroflow horizon-lock toggle` or `Ctrl+2 script-message-to gyroflow view wide`.

## In detail

### Options

In ActionFootagePlayer, `~/Library/Application Support/ActionFootagePlayer/mpv/script-opts/<plugin>.conf`
(Windows: `%LOCALAPPDATA%\ActionFootagePlayer\mpv\script-opts\`); with the plugins in your own mpv,
`~/.config/mpv/script-opts/<plugin>.conf`, or on the command line as `--script-opts=dlogm-mode=sdr` (a value with
commas: `--script-opts-append=dji_telemetry-fields=depth,pressure`). The menu's Remember as default writes them there:

| | |
|---|---|
| `dlogm.conf`: `mode=hdr` | what D-Log M clips start in: `hdr`, or `sdr` (DJI's Rec.709 LUT) |
| `dlogm.conf`: `denoise=no` | `yes`: clips start with the noise reduction on (`Ctrl+n`). How strong it is: `glsl-shader-opts=denoise_strength=2` in `mpv.conf` (1 by default, tuned for 10-bit log) |
| `gyroflow.conf`: `hwdec=yes` | decode the clips it stabilizes with VideoToolbox (Linux, Windows: `auto-copy`) whatever mpv's `hwdec` is (software decoding doesn't keep up with 4K); `no` leaves it to `hwdec` |
| `gyroflow.conf`: `workers=3` | GPU worker threads for the stabilization |
| `gyroflow.conf`: `max_fps=0` | every frame plays. Above 0, clips faster than this play every other frame (`30`: 60 fps at 30) when the stabilized frame leaves edges anywhere in the clip, as Gyroflow measures it for the project and view, and they're filled: for a computer where the fill doesn't keep up |
| `gyroflow.conf`: `view=normal` | what clips start in: `normal`, or `wide` (the wider view) |
| `gyroflow.conf`: `horizon_lock=project` | `yes`: the horizon locked level (Gyroflow's horizon lock at 100 %, in exports too); `no`: not locked; `project`: as the project or preset says |
| `gyroflow.conf`: `edges=sphere` | what fills the edges the stabilized frame doesn't cover: `sphere`, `trail`, `original`, `zoom`, or `off` (the project's background color) |
| `gyroflow.conf`: `edges_blur=2`, `edges_zoom=1.5` | blur of the edge fill (% of the frame height), and how much `zoom` scales the frame up |
| `gyroflow.conf`: `gyroflow_app=` | Linux, Windows: the Gyroflow executable `Ctrl+e` opens projects with |
| `dji_telemetry.conf`: `fields=` | what to show, in that order; not set: what the clip has (see Telemetry) |
| `dji_telemetry.conf`: `units=metric` | or `imperial`: feet, mph, °F |
| `dji_telemetry.conf`: `position=bottom-left` | `top-left`, `top-right`, `bottom-right` (top left has mpv's messages, top right the exports' progress) |
| `dji_telemetry.conf`: `font_size=28` | at a 720-pixel-high window; it scales with the window |
| `dji_telemetry.conf`: `visible=yes` | `no`: shown only after `Ctrl+t` |
| `highlights.conf`: `before=2` | seconds before each highlight a jump lands: the button is pressed once the moment has happened |
| `highlights.conf`: `chapters=yes` | the highlights as chapters too, which mpv's seek bar shows as marks; `no`: only the keys and the menu |
| `footage_menu.conf`: `setup=` | a setup script the menu offers to run again (ActionFootagePlayer sets its own) |

The telemetry plugin's file is `dji_telemetry.conf`, with an underscore: mpv names a script after its folder, with
anything but letters and digits made `_` (the menu's: `footage_menu.conf`). The menu's Remember as default writes
these files, keeping whatever else is in them.

### Stabilization

Make the project in Gyroflow and save it next to the clip (`Ctrl+S` in Gyroflow): mpv uses it. Or start from
what's playing: `Ctrl+e` pauses the clip and opens its project in Gyroflow. A clip that plays without a
`.gyroflow` next to it (a project stored in the video, or the default preset) gets one first, written by the
plugin from what it renders: the same lens profile, smoothing, zoom and refraction, so Gyroflow starts from
what you saw. A `.gyroflow` that's already there is opened as it is, never overwritten. Each time you save in
Gyroflow (`Cmd+S` overwrites the project it opened), mpv reloads the stabilization with it, at the same spot
in the clip. The project then stays next to the clip, and mpv plays the clip with it from then on.

A clip without motion data (shot with the camera's stabilization on, or by a camera that records none) can be
stabilized from the picture: `Ctrl+e` (or the menu's Stabilize in Gyroflow) opens the video itself in Gyroflow,
where its motion is found by optical flow. Save the project next to the clip there, and the clip loads again where
it was, stabilized with it. On Linux and Windows this needs `gyroflow_app` (Options).

The menu's Lock horizon keeps the horizon level, whatever the project says (Gyroflow's horizon lock at 100 %, its
roll and pitch as the project has them); unticked, it isn't locked. It needs more zoom, so the edges show more.

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
them while playing, and `edges_blur` (Options) sets how blurred:

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
have any. Whether a clip does is Gyroflow's own measurement when it loads, for any project: where the frame it renders
is wider than the part the stabilized picture covers. A clip with edges anywhere gets the sphere written as it plays,
edges showing or not, so it has what to fill them with when they show. That's on a thread of its own, and only while
the GPU has room: the frames come first, and where they need all of it the sphere is written less often, what it has
fades into the trail by place, and every frame still plays. A clip without any has no sphere to write. The wider view of
`Ctrl+g` shows more of the picture and has edges throughout: it turns the zoom off and picks Gyroflow's FOV for the
clip so the stabilized picture covers about as much area as the frame, what falls outside the frame and the edges
inside it taken together. That's the area of the frame's outline after the lens correction (the one Gyroflow's zoom
fits its rectangle in), `FOV = √(area / frame's area)`, the median over the clip (`vs-gyroflow/src/lib.rs`,
`wide_fov`; the log has it). On an M1 Pro, 4K60 in the wider view, the filter
keeps up with ~84 fps with the sphere as with the trail (it was ~75 with the sphere written with each frame), and
plays at 60 with the sphere written ~57 times a second; with the GPU short of room (two workers instead of three),
every frame still plays and the sphere is written ~25 times a second, where it used to drop frames.

### Exporting

`Shift+E` exports the clip that's playing, the way it plays: the same project or default preset (or none, when it
plays without stabilization: after `Ctrl+g` turned it off, or with nothing to stabilize it with), the wider view
of `Ctrl+g`, the edges filled as `Ctrl+b` has them, the horizon locked or not as the menu
has it, and the same log curve (`Ctrl+l`'s too, none after `L`) with the same LUT: HDR,
or for D-Log M DJI's Rec.709 one in the standard look (`Ctrl+s`; the menu's Export in standard look does that
whatever plays). With an A-B loop set (`l` twice), only that part. Playback goes on meanwhile, with the
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

- **Log** (D-Log M, D-Log, GP-Log, GP-Log2, 10-bit I-Log: the curve the camera's metadata says, as the dlogm plugin
  reads it, or `--curve dlogm|dlog|gplog|gplog2|ilog`): HEVC Main10, Rec.2100 HLG (the same LUTs as playback,
  `luts/<curve>_hlg.cube`), tagged so macOS, iOS and YouTube show it as HDR. `--sdr` makes D-Log M Rec.709 with
  DJI's LUT instead.
- **Other clips** (and `--curve none`): HEVC Main10 in the colors they were recorded in.
- `--wide` renders the wider view of `Ctrl+g`, `--fov <x>` at a FOV of your own; `--edges sphere|trail|original|zoom|off` fills the
  edges as playback does (`--edges-blur <%>`, `--edges-zoom <x>`); without them, as the project says, edges in its
  background color. A clip whose frame covers the output throughout has nothing to fill, and isn't.
- `--prores`: ProRes 422 HQ instead of HEVC, for editing. The bit rate of HEVC follows the source's.
- **Log clips with nothing to stabilize them with** (D-Log M shot with EIS on, say), and any clip with
  `--no-stabilize` (what `Shift+E` passes after `Ctrl+g` turned the stabilization off): the color conversion only,
  as `<name>_color.mov` in the same folder, at the clip's own size. Clips with neither, nothing to stabilize and
  not log, are skipped: there's nothing to change.
- Audio is copied as it is, and the file keeps the clip's dates. An existing output is kept unless `--force`.
- `Ctrl+C` stops it and removes the unfinished file. `--background` runs it at background priority, to keep
  working while it runs; `kill -USR1 <pid>` / `kill -USR2 <pid>` switch that later. `--progress <file>`
  writes the progress there as JSON twice a second (that's how mpv shows it).

Exports play back in HDR in the player too, as they are: a file tagged HDR (HLG or PQ: an export, a phone's or
an action camera's HDR video) switches the screen to HDR the way a log clip does, with no LUT, and is never taken
for log and graded a second time, whatever camera metadata or `_D` name it carries. `Ctrl+s` shows it tone-mapped
to SDR instead, for that clip only. On a screen without HDR, macOS tones it down, as it does log clips.

A 4K60 clip exports at ~35 fps on an M1 Pro (Lanczos interpolation; `--interpolation bicubic` is what
playback uses), without edges to fill. Filling them costs more: the export writes the sphere with every frame, where
playback writes it as the GPU has room. The video is decoded once and encoded once: ffmpeg decodes it, the plugin's own engine
stabilizes it on the GPU, the LUT is applied on the GPU too (`vs-gyroflow/src/gpu_lut.rs`, the shader's math),
and ffmpeg encodes it with VideoToolbox.

### Telemetry

DJI cameras record, next to the video, a message per frame with what the camera knew at that moment, and GoPro
cameras about a second of it at a time (GPMF). The dji-telemetry plugin reads all of it when a clip opens and
shows it bottom left. Each field shows when the clip has it:

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

Without `fields=`, each clip shows what it has of `depth`, `water`, `gps` and `speed`, and `exposure` only when it
has none of them: a DJI clip shows the depth and whether the camera is in the water, a GoPro one the position and
speed once the GPS has a fix (GoPro has no barometer), and a GoPro without GPS (HERO12 Black), or with it off, the
exposure. Exposure isn't shown next to the others by default: it's a long line that changes all the time, so it's
there for the asking (`fields=`, or the right-click menu). A `fields=` you set is shown as it is, in every clip, and
so are fields picked in the menu, from then on (Remember as default keeps them).

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

### Highlights

A press of the camera's highlight button while it records marks that moment in the clip, and the highlights plugin
reads the marks when the clip opens: they show on the seek bar as chapters ("Highlight 1", ...), `Ctrl+]` and
`Ctrl+[` jump to the next and the previous one, and the menu's **Highlights** lists them all by time. A jump lands
two seconds before the mark (`before=` in `highlights.conf`): by the time the button is pressed, the moment has
usually happened.

- **DJI Osmo Action 4, Action 5 Pro and Action 6**: the quick switch button, or the GPS Bluetooth remote, while
  recording (in video, slow motion and hyperlapse; 190 at most). DJI's other cameras (Osmo 360, Pocket 3, the
  drones) don't mark highlights. Whether the ones added later, in the camera's playback, go into the file isn't
  known yet.
- **GoPro HERO5 and later**: the HiLight button (the mode button, while recording), the voice command "GoPro,
  HiLight", or the remote.

Each file of a recording the camera split has its own. None of it has been tried on a real clip yet: the layouts
come from ExifTool (DJI, from an Action 4) and from a HERO7 Black's header (GoPro); see
[Highlights](#highlights-1) in How it works. A clip with a few marked, and the seconds they were marked at, would
settle it: [Other cameras, and how to help](#other-cameras-and-how-to-help).

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

- `mpv-highlights-<version>.zip`, on any system: plain Lua.

- `mpv-footage-menu-<version>.zip`, on any system with mpv 0.41 or later: the right-click menu (plain Lua). It
  shows the plugins that are there.

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

`./install.sh` alone installs the plugins and their menu into `~/.config/mpv/scripts/` (and an `mpv.conf` with a few
recommended settings, when there's none yet); `--apps` also builds ActionFootagePlayer into `~/Applications` and runs
its setup, and once it's installed, later runs update it too. It downloads DJI's LUT and checks it, bakes the
LUTs into shaders and installs everything; existing files it replaces are kept as `*.bak-<date>` (for
`scripts/`, in `scripts-backup/`, where mpv doesn't load them).

### The app from a checkout, as the release builds it

To try a branch or a fix in the app without waiting for a release, from a checkout on a Mac with
[Homebrew](https://brew.sh) (`brew install mpv ffmpeg rustup`):

```bash
tools/dev_app.sh
```

It builds ActionFootagePlayer the way the release does: the plugins, and Homebrew's mpv, ffmpeg, Python and
VapourSynth in the app (so it needs no Homebrew to run). Then it puts it in `~/Applications` in place of the one
there, which it keeps as `build/ActionFootagePlayer-previous.app`, and runs the setup (the default app for MP4, the
Quick Actions, DJI's LUT).
- **What it builds:** uncommitted changes too, and `~/.config/mpv` isn't touched. The version says what it is: `0.6.2`
  on a release's tag, else e.g. `0.6.2-dev.3+1a2b3c4` (`.dirty` with uncommitted changes).
- **How long:** a few minutes, most of it the first cargo build.
- **How it differs from the release:** the mpv and ffmpeg in it are this Mac's Homebrew ones, where the release's
  build gets the latest (`brew upgrade mpv ffmpeg` first). Each build is signed anew (ad hoc), so macOS may ask
  again for access to a folder like Downloads.
- `--no-install` only builds `build/ActionFootagePlayer.app`.

### Linux and Windows

The gyroflow plugin runs there too, with binaries of its own; dlogm, dji-telemetry and highlights are the same zips as on a Mac.
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
next to it for D-Log M. On Linux, `./install.sh` from a checkout also builds and installs all the plugins (with
DJI's LUT, as on a Mac). With `--apps` it also puts ActionFootagePlayer in the applications menu: the same mpv, by
its own name and icon (`~/.local/share/applications/`), made the default app for MP4 videos. On Windows, the app
has all of it: [The Windows app](#the-windows-app).

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

### The Windows app

The installer (`tools/make_windows_app.sh`, `windows/`) puts everything in the app's folder, so nothing else needs
installing: mpv ([shinchiro's build](https://github.com/shinchiro/mpv-winbuild-cmake), with VapourSynth support,
and with the app's icon), Python with VapourSynth, the VC++ runtime VapourSynth needs, a Vulkan loader for a PC whose graphics driver hasn't
installed one (mpv can't start without), ffmpeg and ffprobe for exports
([Gyan's build](https://www.gyan.dev/ffmpeg/builds/)), and the plugins. `ActionFootagePlayer.exe`
(`windows/launcher/`) starts that mpv with the plugins and tells it where VapourSynth is, and plays a video opened
while one plays in its place. Like the Mac app, it's whole by itself: its own plugins only, and its own mpv settings
and config folder, not an mpv you may also have (`%APPDATA%\mpv`). What's yours is in
`%LOCALAPPDATA%\ActionFootagePlayer\`: what you set in the plugins (`mpv\script-opts\`), DJI's LUT and its shader
(`dlogm\`, fetched in the background if the installer couldn't), and your own default preset if you make one
(`gyroflow\default.gyroflow`); uninstalling leaves it. The stabilization renders on DirectX 12
or Vulkan.

### What the app keeps where

ActionFootagePlayer is whole by itself: it plays with the plugins it carries (`Contents/Resources/plugins/`) and no
others, with its own mpv settings (`Contents/Resources/app.conf`: hardware decoding, the window at most 85% of the
screen, a playlist of the folder's videos) and a config folder of its own. An mpv you may also have isn't read: its
`~/.config/mpv` (`mpv.conf`, `input.conf`, scripts), and the plugins `install.sh` puts there, are for that mpv. To
put the plugins together with other scripts of your own, take the plugins instead of the app (see
[Plugins in your own mpv](#plugins-in-your-own-mpv)). The app is read-only once signed; what's yours is in
`~/Library/Application Support/ActionFootagePlayer/`: what you set in the plugins (`mpv/script-opts/`, with Remember
as default), DJI's LUT and its shader (`dlogm/`), and your own default preset if you make one
(`gyroflow/default.gyroflow`, see Stabilization).

ActionFootagePlayer is mpv itself, as an app of its own: the app's executable (`macos/player.sh`) starts the mpv it
carries (`Contents/Helpers/`, or Homebrew's in an app built without it) with the plugins, as the app of the bundle (mpv's `MPVBUNDLE`), so it's ActionFootagePlayer in the Dock, with
its icon, and gets the videos Finder opens with it. A video opened while one plays takes its place. mpv's own menu
items still say mpv (About mpv, Quit mpv). The setup is offered from inside mpv (`macos/setup-offer.lua`), once
it runs.

### Upgrading

**From 0.6.0**, the app no longer reads mpv's own config (`~/.config/mpv`, `%APPDATA%\mpv`) nor the plugins in it:
it plays with its own plugins and settings only. The first time it starts, the plugins' settings
(`script-opts/dlogm.conf`, `gyroflow.conf`, `dji_telemetry.conf`, `footage_menu.conf`) are copied over to its own
config folder; the rest of mpv's config stays with that mpv. Plugins `install.sh` put in `~/.config/mpv/scripts/`
are only for mpv in a terminal now: delete them if that's not how you use it.

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

| | Stabilization without a project | Log footage (HDR, Rec.709 LUT) | Telemetry | Highlights |
|---|---|---|---|---|
| DJI Osmo Action 6 | yes | D-Log M | yes | the `hglg` box, as ExifTool reads it from an Action 4, not seen |
| DJI Osmo 360 | EIS off (`eis_status`), single-lens MP4, not seen | D-Log M, from its schema, not seen | from its schema, not seen | none |
| Other DJI (Action 4, Action 5 Pro, Pocket 3, Osmo 360 II, ...) | EIS off (`eis_status`), not seen | not yet: `ColorMode` is read only where it was checked against the model's schema | no | Action 4, Action 5 Pro: as the Action 6, not seen |
| GoPro | HyperSmooth off (`EISA`/`EISE`), lens from Gyroflow's database; checked only on GoPro's own sample clips, and on a HERO12 clip from the command line (HyperSmooth on: rightly not stabilized) | GP-Log2: the Protune color, `PTCL` in `udta`'s GPMF, is `GPLOG2` (gpmf-parser's list; read as `NATURAL` on a HERO12, a GP-Log2 clip not seen). GP-Log is `LOG`, used with the camera (`MINF`) and firmware (`FIRM`): HERO12 Black on 1.20 or later and HERO13 Black, where GoPro's own LUTs put 18 % grey where Zeb Gardner's does (HERO12 Black 2.32 read from a clip) | GPS, speed, exposure; checked on GoPro's sample clips, and the exposure on a HERO12 clip | `HMMT` in `udta` (HERO5, HERO6), `HLMT` in `udta`'s GPMF (HERO7 and later): checked on a HERO7 Black's header, no clip seen |
| Insta360 single-lens MP4 (Ace Pro, GO 3, ...) | FlowState off in the camera (`is_flowstate_online`), not seen | 10-bit I-Log by hand (`Ctrl+l`): where Insta360 records it isn't known. 8-bit I-Log: no transform yet | no | not yet (telemetry-parser reads them) |

The stabilization errs on the safe side: a clip is stabilized only when the camera recorded that its own was
off, so a wrong guess about a camera means its clips play as they are, not worse.

**Clips that would help**, straight from the camera (not exported or trimmed by an app: that drops the metadata):

- a few seconds each, any resolution, the camera moving (walking with it is enough);
- the camera's stabilization off, and the same on (DJI: EIS/RockSteady; GoPro: HyperSmooth; Insta360: FlowState);
- in the log profile (D-Log M, GP-Log, I-Log) and in the normal one;
- under water, if the camera records depth or a "wet" flag;
- with a few highlights marked while recording, and the seconds you marked them at;
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
color profile, the telemetry and the highlights are read in plain Lua by the dlogm, dji-telemetry and highlights
plugins, which don't depend on Gyroflow, so they work in any mpv on any system.

## How it works

It's four mpv plugins, each a folder in `~/.config/mpv/scripts/` that works without the others, and
ActionFootagePlayer, a Mac app that carries them and opens your MP4s with them:

- **`dlogm`**: D-Log M recognized from the camera's metadata, and shown in HDR or through DJI's Rec.709 LUT; D-Log,
  GoPro's GP-Log and GP-Log2, and Insta360's 10-bit I-Log in HDR. Plain Lua and GPU shaders, so it runs in any mpv on any
  system (`vo=gpu-next`).
- **`gyroflow`**: the stabilization and the export, with the binaries built from `vs-gyroflow/` (macOS on Apple
  Silicon; Linux and Windows on x86-64, see [Linux and Windows](#linux-and-windows)).
- **`dji-telemetry`**: the camera's telemetry over the video (DJI, GoPro). Plain Lua, any mpv on any system.
- **`highlights`**: the moments the camera marked (DJI, GoPro), on the seek bar and a key away. Plain Lua, any mpv
  on any system.

None needs anything in `mpv.conf` or `input.conf`.

### D-Log M

DJI tags D-Log M files as BT.709 SDR, and names Normal-color clips `*_D.MP4` as well. The color profile is in
DJI's own metadata, though: `ColorMode` in the stream metadata of the `CAM meta` track (19 is D-Log M).
The dlogm plugin reads it in Lua (`mpv/scripts/dlogm/dji_color.lua`: the first sample of the metadata track,
a few protobuf fields), for the Osmo Action 6 and Osmo 360, whose layouts are known; other models' clips play as
they are. `gyroflow-probe` reads it the same way, for the export. The plugin
(`mpv/scripts/dlogm/main.lua`) sets the shader and the display options as file-local options, added to any
shaders of your own, so mpv undoes them when a playlist moves on to a clip that isn't D-Log M:

- `hdr`, the default: a 65³ D-Log M → Rec.2100 HLG (1000 nit) LUT (`luts/dlogm_hlg.cube`, composed in DaVinci
  Resolve from Zeb Gardner's D-Log M → DaVinci Wide Gamut/Intermediate shaper — see Credits) at `HOOK MAIN`.
  A `format` filter at the end of the chain tags the video as HLG/BT.2020, the target is the same
  (`target-trc=hlg`, `target-prim=bt.2020`, `target-peak=1000`), so mpv's color pipeline runs an HLG → HLG
  identity; tagged as the file's BT.709, mpv would linearize the HLG output as BT.709 and encode it again.
  `target-colorspace-hint=yes` switches the Metal layer to HDR. On an SDR display macOS tone-maps it down.
  (Up to 0.5.0 the LUT was at `HOOK OUTPUT`, re-sampling the source through `BIND MAIN`: libplacebo redraws a
  paused frame from its cache, where `MAIN` can't be bound, and skipped it, so paused video went grey.)
- `sdr`, on `Ctrl+s`: DJI's 33³ D-Log M → Rec.709 LUT at `HOOK MAIN`. mpv's color pipeline then runs a
  BT.709 → BT.709 identity, so the LUT's output reaches the display unchanged.

The LUTs are baked into the shaders as 3D `rgba16` textures (`tools/cube_to_glsl.py`, templates in
`shaders/`), so the GPU does the trilinear interpolation in one tap.

### Other log curves

D-Log, GP-Log, GP-Log2 and 10-bit I-Log get the same `hdr` mode as D-Log M, a 65³ LUT to Rec.2100 HLG at
`HOOK MAIN` (`shaders/log_hlg.glsl.in`). D-Log's, GP-Log2's and I-Log's aren't in the repo: `tools/make_hdr_lut.py`
makes them at install from each maker's own publication, which [docs/luts.md](docs/luts.md) lists with the rest of
what the makers publish:

| Curve | Decoding, and the gamut | From |
|---|---|---|
| DJI D-Log | DJI's `dlog_to_lin`, D-Gamut | DJI's ACES input transform (`Dlog_DGamut_to_ACES_AP0_cat02_v1`, in its ACES Workflow Guide, 2023) |
| GoPro GP-Log2 | log base 600, +1.8 EV (18 % grey at 0.542), Rec.2020 | GoPro Labs' [GP-Log2 LUT Generator](https://gopro.github.io/labs/gplog2/) (Apache-2.0; formulas reimplemented) |
| Insta360 10-bit I-Log | the white paper's I-Log to scene-linear, BT.2020 | Insta360's 10-bit I-Log White Paper (2026) |

From scene light, every curve is rendered the way `luts/dlogm_hlg.cube` renders D-Log M, so they look alike: scaled
by 0.254 and rolled off by `x / (1 + (x/0.69)^1.5)^(2/3)`, numbers fitted to that LUT (18 % grey at about 37 % HLG;
ITU-R BT.2408 says 38 %), then HLG-encoded in Rec.2020. Checked against the makers' own LUTs: the GP-Log2 decoding
gives GoPro's Rec.709 LUT to within 0.001 in the mid-tones; 18 % grey lands where Insta360's I-Log → HLG and
DJI's (Zenmuse X9) D-Log → HLG LUTs put it, theirs with a contrast curve on top; and rebuilt this way from Zeb
Gardner's D-Log M → DWG/I, D-Log M comes within about 0.025 of `dlogm_hlg.cube` on natural colors.

GoPro publishes no gamut for GP-Log, so its LUT, `luts/gplog_hlg.cube`, is made from
[Zeb Gardner](https://www.zebgardner.com/photo-and-video-editing/gopro-log-color-management-accuracy-testing)'s
GP-Log → DaVinci Wide Gamut/Intermediate shaper LUT, fitted from test charts, with the same rendering. The tool
takes any LUT that ends in DWG/I (`--from-dwgi`), and keeps its comments and `--note`s as credits:

```bash
python3 tools/make_hdr_lut.py --from-dwgi GP_LOG-to-DWG_I-ZRG-V1-j3dz.cube luts/gplog_hlg.cube --note "..."
```

GoPro moved GP-Log's exposure with firmware: its own LUTs put 18 % grey at GP-Log 0.33 for HERO12 Black firmware
1.10/1.12, 0.53 for 1.20 and later and for HERO13 Black, 0.72 for MAX2. Zeb Gardner's puts it at 0.50, so only
the middle ones are recognized as GP-Log; the others would play two stops off, and `Ctrl+l` picks it for them by
hand.

Which curve a clip is in: DJI's `ColorMode` (2 is D-Log, from the same metadata as D-Log M), GoPro's `PTCL`
with the camera and firmware (`mpv/scripts/dlogm/gopro_color.lua`), or `Ctrl+l`.

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
`stsz`, `stco`/`co64`), reading each chunk of samples at once, and decodes just the values the fields shown need,
along paths of field numbers per model (`LAYOUTS`, by `proto_file_name`). Each value is kept only where it changes;
`main.lua` looks it up by `time-pos` and redraws the OSD overlay (ASS) when the text changes. A 3.7-minute 4K60 clip
(2.5 GB) is read in 0.4 s.

On the Action 6 the barometer is `Barometer { is_in_water = 1; altitude_dm = 2; atmospheric_hpa = 3; }` at
field 13 of the frame's camera part (`3.2.13`); the other camera fields are numbered as on the Osmo 360.

GoPro's metadata track is the one whose samples are `gpmd`: GPMF, nested KLV items (`DEVC` → `STRM` → `GPS5`,
`SHUT`, ...), big-endian, each scaled by the `SCAL` before it. A sample holds about a second; its values are spread
evenly over it. GPS5 (HERO5-10) has the fix for the whole sample in `GPSF`, GPS9 (HERO11 and later) in each row.

### Highlights

`mpv/scripts/highlights/markers.lua` walks the file's boxes with seeks (mdat isn't read) and reads the few that
hold the highlights:

- **DJI** (Osmo Action 4, 5 Pro, 6): a `free` box at the top level that starts `data`, then a box `hglg` (size
  32-bit big-endian, its type, 5 bytes), then 5 bytes per highlight: a byte, and the time in ms, 32-bit
  little-endian. Another `free` box lists where `hglg` is, with the cover (`covr`): the camera seems to keep room
  to write it again. The layout is ExifTool's (`HighlightMarkers`, worked out on an Action 4); what the single
  byte and the 5 bytes say isn't known.
- **GoPro HERO5 and HERO6**: `moov/udta/HMMT`, the count, then each time in ms, 32-bit big-endian. FFmpeg already
  makes chapters of these: the plugin gives them their titles rather than adding them twice.
- **GoPro HERO7 and later**: `udta`'s `GPMF` box (the one the Protune settings are in), the `DEVC` whose `DVID` is
  `HLMT` ("Highlights"): an `HLMT` item of one struct per highlight, as its `TYPE` lays it out (on a HERO7 Black
  `LLLllfFff`: time, in, out in ms, the place, the type, `MANL` for one marked by hand, a confidence and a score).
  The time is the first.

`main.lua` adds them to mpv's `chapter-list`, next to the file's own chapters, and its keys seek to them
themselves, `before=` seconds early.

## Layout

```
install.sh                    builds and installs everything
mpv/scripts/dlogm/            the log plugin  →  ~/.config/mpv/scripts/dlogm (+ shaders/, luts/ at install), and in
                              ActionFootagePlayer; dji_color.lua and gopro_color.lua read the cameras' color profile
mpv/scripts/gyroflow/         the stabilization plugin  →  ~/.config/mpv/scripts/gyroflow (+ the binaries), and in
                              ActionFootagePlayer
mpv/scripts/gyroflow/platform.lua  what differs between macOS, Linux and Windows (binaries' names, temp folder)
mpv/scripts/gyroflow/underwater.gyroflow  the default preset for clips without a project
mpv/scripts/dji-telemetry/    the telemetry plugin  →  ~/.config/mpv/scripts/dji-telemetry
mpv/scripts/highlights/       the highlights plugin  →  ~/.config/mpv/scripts/highlights, and in ActionFootagePlayer;
                              markers.lua reads them from the file
mpv/scripts/footage-menu/     the right-click menu for the plugins  →  ~/.config/mpv/scripts/footage-menu, and in
                              ActionFootagePlayer
mpv/mpv.conf                  recommended settings, installed when there's no mpv.conf
shaders/*.glsl.in             shader templates; the LUT goes in at install
shaders/denoise.glsl          the noise reduction (Ctrl+n), installed with the log plugin as it is
luts/dlogm_hlg.cube           D-Log M → Rec.2100 HLG LUT (DJI's Rec.709 one is downloaded)
luts/gplog_hlg.cube           GoPro GP-Log → Rec.2100 HLG LUT
tools/cube_to_glsl.py         .cube → mpv shader
tools/make_hdr_lut.py         D-Log, GP-Log2, 10-bit I-Log (or a LUT to DWG/I) → Rec.2100 HLG LUT, from the makers'
                              formulas
docs/luts.md                  the makers' log LUTs and formulas: where they are, what's in them, their terms
tools/dji_meta_dump.py        what's in a DJI clip's metadata track: every field, or some over time
vs-gyroflow/                  Gyroflow VapourSynth plugin, gyroflow-probe, gyroflow-embed, gyroflow-export (Rust)
macos/                        ActionFootagePlayer: player.sh (the app's executable, mpv), setup.sh (default app, Quick
                              Actions, DJI's LUT) and setup-offer.lua (offers it at first launch), default-app.py,
                              the Quick Actions' export script
macos/icon/                   its icon: AppIcon.svg, and the AppIcon.icns made from it (tools/make_icon.sh)
tools/make_app.sh             builds ActionFootagePlayer.app, with the plugins in it
tools/dev_app.sh              builds it from the checkout as the release does, and puts it in ~/Applications
windows/launcher/             the Windows app's ActionFootagePlayer.exe (starts the mpv it carries), and set-icon, which
                              gives mpv.exe the app's icon (Rust, no dependencies)
windows/setup.py              fetches DJI's LUT for the Windows app; windows/installer.iss is its installer (Inno Setup)
windows/AppIcon.ico           the Windows app's icon (tools/make_icon.sh, tools/png_to_ico.py)
tools/make_windows_app.sh     builds the Windows app and its installer, with mpv, Python, VapourSynth and ffmpeg pinned
tools/make_icon.sh            macos/icon/AppIcon.svg → AppIcon.icns and windows/AppIcon.ico (renders with Google Chrome)
tools/make_workflow.py        writes the Finder Quick Actions
tools/make_release.sh         release files: ActionFootagePlayer, each plugin alone, and the tree plus prebuilt binaries
tools/make_plugin_zip.sh      the gyroflow plugin alone with its binaries, for Linux or Windows
.github/workflows/release.yml builds the release on macOS, Linux and Windows (with the Windows app) and publishes it
                              for a pushed v* tag
```

## License

GPL-3.0 (the plugin builds on Gyroflow, which is GPL-3.0). DJI's LUT belongs to DJI and is downloaded from
DJI's site at install.

ActionFootagePlayer.app also carries mpv, ffmpeg, Python, VapourSynth, MoltenVK and the libraries they use, as
Homebrew builds them, each under its own license (GPL, LGPL, PSF, Apache, MIT, BSD and others). The app's
`Contents/Resources/ThirdParty/` has each one's license files and the Homebrew formula it was built by, and
`SOURCES.txt` there lists the version of each and where its source is (`tools/bundle_runtime.py` writes them
at build). The sources are also available from this project on request.

## Credits

Camera metadata (motion data, lens, in-camera stabilization) is read by Adrian Eddy's
[telemetry-parser](https://github.com/AdrianEddy/telemetry-parser), through Gyroflow.

Where the cameras keep their highlights: DJI's from Phil Harvey's [ExifTool](https://exiftool.org)
(`HighlightMarkers`), GoPro's `HMMT` as VLC and FFmpeg read it, and its GPMF `HLMT` from Juan Irache's
[gopro-telemetry](https://github.com/JuanIrache/gopro-telemetry) and the HERO7 Black header among its samples.

`luts/dlogm_hlg.cube` (D-Log M → Rec.2100 HLG, 1000-nit, no DaVinci DRT) was composed in DaVinci Resolve
from [Zeb Gardner](https://www.zebgardner.com/)'s free D-Log M → DaVinci Wide Gamut/Intermediate shaper LUT
(`DLOGM_DWG_I_SHAPER_ZRG_V1`); the HLG output stage and the bake are this project's. Thanks to Zeb Gardner for
the underlying transform.

`luts/gplog_hlg.cube` (GoPro GP-Log → Rec.2100 HLG) is made by `tools/make_hdr_lut.py` from Zeb Gardner's GP-Log →
DaVinci Wide Gamut/Intermediate shaper LUT (`GP_LOG to DWG_I ZRG V1`, from his
[GoPro Log accuracy testing](https://www.zebgardner.com/photo-and-video-editing/gopro-log-color-management-accuracy-testing)).

His site states no license for his LUTs; Zeb Gardner has agreed to their use here, with this credit. The HDR
rendering of the other log curves is fitted to `dlogm_hlg.cube`, so it builds on his work too.

The other log curves are decoded from their makers' own publications (`tools/make_hdr_lut.py`, which says the
same in every LUT it writes):

- DJI D-Log and D-Gamut: DJI's ACES input transform `Dlog_DGamut_to_ACES_AP0_cat02_v1`, from DJI's "ACES Workflow
  Guide" (2023).
- GoPro GP-Log2: GoPro Labs' [GP-Log2 LUT Generator](https://gopro.github.io/labs/gplog2/)
  ([gopro/labs](https://github.com/gopro/labs), Copyright GoPro, Inc., Apache License 2.0); its formulas are
  reimplemented, no code copied. GoPro's Protune color tag, `PTCL`, is documented in GoPro's
  [gpmf-parser](https://github.com/gopro/gpmf-parser).
- Insta360 10-bit I-Log: Insta360's "10-bit I-Log White Paper" (v1.0, June 2026).
- DaVinci Wide Gamut and DaVinci Intermediate (for `--from-dwgi`): as Blackmagic Design publishes them.

## Known issues

Each goes from here once it's fixed.

None at the moment.
