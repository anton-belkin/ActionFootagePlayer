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

> **Status: early (alpha).** In daily use with the **DJI Osmo Action 6** on **Macs with Apple Silicon**, and tried on
> a **GoPro HERO12 Black**. Other DJI cameras, other GoPros and Insta360 are built for but not yet tried on real
> footage. Windows has an app of its own and Linux
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
  `Ctrl+[` jump to the next one and back, a couple of seconds before a camera's (yours, `M`, right on them).
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
2. **Install:** download `ActionFootagePlayer-<version>-macos-arm64.dmg` from the
   [latest release](https://github.com/anton-belkin/ActionFootagePlayer/releases/latest), open it and drag the app
   onto Applications. It carries its own mpv and ffmpeg: nothing else to install (macOS 15 or later).
3. **Open it once.** The first time, macOS blocks it, because it isn't signed with a paid Apple developer account:
   System Settings → Privacy & Security → **Open Anyway**. Accept its setup: it becomes the player for MP4 videos.
4. **Copy the clips off the card and double-click one.**

`l` twice marks a part, `Shift+E` exports it, `Ctrl+s` goes through HDR and the SDR looks, `Ctrl+g` goes
through the stabilization, a wider view of it, and off. [All keys](#keys) · [Install in detail](#installing-in-detail)

## Example: back from the dive

You shot the dive in D-Log M, with EIS off because under water it doesn't hold. In the hotel, you copy the card to
the laptop and double-click the first clip: it plays stabilized, in HDR, with the depth in the corner. You skip
through the hour to the 15 seconds with the turtle, press `l` at the start, `l` at the end, `Shift+E`, type "Turtle",
and pause for half a minute. The fragment is `_Exports/Turtle.mov` in the trip's folder, ready to AirDrop to the group
the same evening, in HDR, and the turtle is a [moment](#moments) too, to play again in full quality later. The originals are untouched for proper editing later.

## Why log, and why HDR

The log mode records the most the camera can: often more than its own HDR mode (GoPro's HERO12 makes HDR by merging
two exposures, while its log is a single one, which holds up better with motion and in low light), and on some
cameras, the Osmo Action 6 among them, log is the only way to HDR at all. On an ordinary (SDR) screen, log needs a
colorist to decide what to keep and what to leave out of SDR's narrow range: a creative job. An HDR screen has room
for all of it, so the conversion becomes a technical one, the same for every clip, and can be automatic. That turns
the "pro" mode into the one to use for everyday footage.

On a Mac without an HDR screen (MacBook Air, most external monitors), clips start in HLG with Dolby Vision as macOS
shows it (*HDR, HLG+DV*): macOS tones the HDR down for that screen with trims fitted to NBCUniversal's HLG → SDR conversion (no scene-level offsets, so an HDR screen still gets HDR), the way
QuickTime and Apple's devices show the export, and shows it in HDR on an HDR screen. `Ctrl+s` goes through the video
outputs: *HDR, HLG* (macOS tones it down on an SDR screen), *HDR, HLG+DV*, *SDR, HLG+DV* (toned down by the trims on
an HDR screen too: what an SDR screen shows), mpv's tone mapping, NBCUniversal's neutral HDR → SDR conversion and
the camera maker's own Rec.709 LUT (DJI's, for D-Log M); on an HDR screen the SDR ones show as an SDR screen would. Exports still play in HDR on an iPhone.
Each frame's light levels go along with it (Dolby Vision's L1: its darkest, average and brightest, measured on the GPU
as it plays, on the picture macOS gets), so macOS maps every scene for what's in it rather than for a typical one: a
dark dive and a sunlit beach each get their own room. The Dolby Vision trims go on top of that; they're the author's,
to taste, fitted to NBCUniversal's conversion with each frame's levels. The side panel's Dolby Vision has them as
sliders, with a way back to the author's and to neutral ones (Dolby Vision's own mapping, nothing added). Exports
take what's set there and measure each of their frames the same way, so an Apple device shows them as the player does.

After an HDR display (an HDR TV, say) has been connected to the Mac, macOS can keep its settings for the SDR monitor
plugged in where it was, until the Mac restarts, and shows HDR on it about a stop darker. The player tells when it
sees that, the first time HDR plays on such a monitor: restarting the Mac sets it right.

## Help it work with your camera

Everything was worked out on one camera, and each maker, often each model, records its metadata differently. Most
useful right now:

- **A few seconds of footage** from a GoPro other than the HERO12 Black, an Insta360, another DJI camera or a DJI drone, straight from the card.
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
| GoPro | **verified**: HDR from GP-Log (HERO12 Black on firmware 1.20+). Should work: HERO13 Black, and GP-Log2 (MISSION 1) ⁴ | **verified**: HERO12 Black (HyperSmooth off). Should work: the others | should work | should work: GPS and speed; exposure on one without GPS | should work: HERO5 and later | should work |
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
   checked against the makers' own LUTs; on footage, so far only GP-Log, on a HERO12 Black. HDR only (no Rec.709 LUT yet), in exports too. `Ctrl+l`
   picks the curve for a clip whose metadata doesn't say it.

## Install

From the [latest release](https://github.com/anton-belkin/ActionFootagePlayer/releases/latest), take:

| For | File |
|---|---|
| a Mac with Apple Silicon (macOS 15 or later) | `ActionFootagePlayer-<version>-macos-arm64.dmg`: the app, with all the plugins and their menu, mpv and ffmpeg in it (or the same app as `-macos-arm64.zip`) |
| a Windows 10 or 11 PC (x86-64) | `ActionFootagePlayer-<version>-windows-x86_64-setup.exe`: the app, with all the plugins and their menu, mpv, VapourSynth and ffmpeg in it |
| a Windows 11 PC with an ARM processor (Snapdragon) | `ActionFootagePlayer-<version>-windows-arm64-setup.exe`: the same, built for it (the x86-64 one runs there too, emulated and slower). Exports encode on the CPU (x265) |
| all the plugins in your own mpv, in one zip | `mpv-plugins-<version>-macos-arm64.zip` (Homebrew's mpv), `-linux-x86_64.zip`, `-windows-x86_64.zip`, `-windows-arm64.zip` |
| D-Log M and telemetry in your own mpv, on any system | `mpv-dlogm-<version>.zip`, `mpv-dji-telemetry-<version>.zip` |
| stabilization and export in your own mpv | `mpv-gyroflow-<version>-macos-arm64.zip` (Homebrew's mpv), `-linux-x86_64.zip`, `-windows-x86_64.zip`, `-windows-arm64.zip` |

**The app** carries its own mpv (with VapourSynth and its Python, for the stabilization) and ffmpeg (for
exporting), [Homebrew](https://brew.sh)'s builds of them: nothing else to install. It needs macOS 15 or later, the
oldest macOS those builds are made for.

Open the disk image, drag ActionFootagePlayer onto Applications and open it from there (from the zip: unzip it and
move it to Applications). It's signed ad hoc, not by an Apple developer
account, so the first time macOS won't open it: System Settings → Privacy & Security → Open Anyway. The first time
it opens, it offers its setup:

- it becomes the default app for MP4 videos (what DJI cameras record): double-clicking one plays it with all
  of this, and any other video simply plays in mpv. macOS asks you to confirm the change in a dialog of its own;
  it applies once you do;
- Finder gets the Quick Actions *Play Stabilized* and *Stabilize + LUT* (see [Opening files](#opening-files));
- DJI's Rec.709 LUT is downloaded from DJI (it isn't redistributed here) and checked, for DJI's SDR look and its
  Rec.709 exports. HDR and NBCU's SDR work without it.

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

With the side panel there (the app, and the plugins' zip), right click is mpv's own menu and the settings are in the
panel. Without it (`footage_menu.conf`: `menu=yes` for it in any case), right-click the video for the plugins' menu:
everything the keys do, with the keys beside it, and what has no key. At the
top, what the clip is (the camera, its log curve, whether it has motion data and telemetry, how many highlights);
then playing, marking a part and exporting it; **Highlights** (the next, the previous, or any one by its time);
**Look** (HDR, the maker's SDR, NBCU's SDR, as recorded, the log curve by hand, noise reduction); **Stabilization** (on,
the wider view, the level, the horizon locked or not, the lens under water or not, what fills the edges, Gyroflow); **Telemetry** (where it shows,
the units, which fields); **Export** (in SDR, the last export in Finder); **Clip** (the next and
previous clip in the folder, Finder, the path, a still); **Player** (speed, tracks, fullscreen); and Note for the
agent… (see [Notes for an AI agent](#notes-for-an-ai-agent)), Keys…, Help and Report this camera…, which opens an
issue on GitHub with what the plugins found in the clip. In the app, that last submenu has the version beside its
name: *ActionFootagePlayer 0.6.4*.

Look, Stabilization and Telemetry each have **Remember as default**: the settings as they are become the plugin's
options (see Options), for the next start. A clip without motion data has **Stabilize in Gyroflow (optical
flow)…** instead: see Stabilization.

The menu is mpv's own, so it needs mpv 0.41 or later (Homebrew's is); with an older mpv, right click pauses as it
always did. It's the footage-menu plugin, which shows only the plugins that are there.

### The panel

The settings are in a panel at the right of the window, beside the video (the video moves over for it, nothing is
covered), as in a photo editor: a rail of icons at the edge, one for each part, and the panel of the one chosen, with
switches and buttons, scrolled with the wheel:

- **Project**, where it opens: the trip's [moments](#moments), marks and [clip previews](#clip-previews-the-trip-at-a-glance),
  with their keys beside them. At the top, the project: with none yet, what one is and **Initialize the project here**
  in a folder you choose from the clip's folder up to the top of the disk (a project covers every folder in it);
  then its counts, **Make clip previews** for clips without one, **Show all clips** and **Show all moments** in
  Finder. Below, this clip: its marks (the one where it plays highlighted, **Add** or **Remove the mark here**),
  its moments (a click plays one; `⋯`: play, rename, remove, show in Finder) and **Mark this clip as not
  interesting**. At the bottom, the part marked now, with **Save as moment** (or **Update**, on a moment whose ends
  were moved).
- **Clip**: what it is, Finder and its path, marking a part, exporting it, the highlights, the next and previous clip.
- **Look**: the video output (HDR, HLG; HDR, HLG+DV; SDR, HLG+DV; mpv's tone mapping; NBCU's and the maker's SDR),
  the log curve by hand, noise reduction.
- **Stabilization**: Gyroflow, the view, the level, the horizon, the lens and its color aberration.
- **Performance & Quality**: the frame rate and size it's stabilized at, the paused frame (full size, temporal
  denoise), what fills the edges and how often the trail is written, whether it asks when the GPU can't keep up.
- **Telemetry**: whether it shows, the corner, the units, the fields.
- **Dolby Vision** (Mac, the app): the trims, as sliders; back to the author's, or neutral ones.
- **Support**, at the bottom: the version, Keys…, Note for the agent…, Report this camera…, Set up again…, and links
  to the website and GitHub.

The keys are beside what they do. Look, Stabilization, Quality and Telemetry have **Make these the defaults** at the
bottom, as the menu's Remember as default. `Tab` hides the panel and its rail, and shows them again; clicking the
chosen icon, or the arrow at the top of the panel, leaves only the rail. While the panel shows, mpv's own controls
(play, the seek bar) are a small box at the bottom of the video. It's the side-panel plugin (`side_panel.conf`:
`shown=`, `open=`, `tab=`); its Clip and Support buttons run footage-menu's commands.

### Keys

| Key | | `script-binding` |
|---|---|---|
| `Ctrl+g` | Stabilization on → on with a wider view (no zoom: the stabilized picture covers about as much of the window as the frame, whatever the lens; the edges show at times, filled as `Ctrl+b` says) → off. On or wider sticks for the next clips without a project; a clip with a `.gyroflow` starts in it as Gyroflow has it (the menu's On (Gyroflow project)) | `gyroflow/toggle` |
| `Ctrl+k` | The level of a clip without a project: weak → medium → strong (10, 20, 30 % cropped, smoothness 50, 75, 100 %; see Stabilization). It sticks for the next clips; a clip with a `.gyroflow` has the project's own | `gyroflow/level` |
| `Ctrl+b` | What fills the edges the stabilized frame doesn't cover: the project's background, what was seen in that direction with the colors around spread in where nothing was yet, or only the frame's own colors spread out (see Stabilization); it sticks for the next clips | `gyroflow/edges` |
| `Ctrl+u` | The lens above water (no refraction), and back to under water, as the camera says or as the project says, whichever it was (Stabilization → Lens in the menu); it sticks for the next clips without a project | `gyroflow/lens` |
| `Ctrl+a` | Color fringes under water taken out, or left as recorded (Stabilization → Lens in the menu). Only where the lens is under water: above water no color moves either way, and the lens stays as `Ctrl+u` has it. It sticks for the next clips | `gyroflow/fringes` |
| `Ctrl+s` | The clip's next video output: HDR (HLG), HLG with Dolby Vision in HDR and in SDR (macOS), SDR tone-mapped by mpv, SDR through NBCUniversal's HLG → SDR conversion (the same for every camera), then SDR through the maker's LUT (DJI's, for D-Log M). Only the ones the clip has: an HDR clip (an export) has HDR as it is and NBCU's SDR. What you choose is what the next clips start in on that kind of screen, HDR or SDR, for the session (Look → Remember as default keeps it); an SDR look chosen on either becomes the SDR screen's. A clip starts in HDR on an HDR screen, in the SDR look last chosen on an SDR one. On an SDR screen HDR shows as macOS tones it down; on an HDR screen SDR shows as an SDR screen would | `dlogm/sdr` |
| `Ctrl+h` | Straight to HDR, as `Ctrl+s` would choose it | `dlogm/hdr` |
| `L` | Log handling off for this clip: the file as it is, flat as the camera recorded it (an HDR clip: in HDR) | `dlogm/off` |
| `Ctrl+l` | This clip's log curve, by hand, in HDR: D-Log M, D-Log, GP-Log, GP-Log2, I-Log (10-bit) in turn. For clips whose metadata doesn't say | `dlogm/curve` |
| `Ctrl+n` | Noise reduction on/off: the blotches of color in shadows and under water mostly go, the detail stays. On the GPU, before the log conversion; any clip, log or not. It sticks for the next clips. Playback only: exports are as before | `dlogm/denoise` |
| `Ctrl+p` | Temporal denoise of the paused frame on/off: a moment after the video pauses, the frame is rebuilt from it and the 5 frames on each side of it, each moved onto it by the motion between them, so its noise and the particles in the water mostly go and the detail stays (about a second for 4K; the stabilization must be on). Playing on goes on from it as usual. It sticks for the next clips | `gyroflow/still` |
| `Ctrl+f` | The paused frame at full size, now (it pauses first): the source's own resolution and every frame, whatever Quality's frame rate and resolution are, cleaned up in the same re-render when `Ctrl+p` is on ("Cleaning up the frame at full resolution…"). Playing on goes back to the playback settings at once. Quality → Paused frame → At full size does it a moment after each pause | `gyroflow/full` |
| `c` | Save frame (the side panel's camera): a frame over the video for a photo of this frame — the whole video's crop is Gyroflow's (`Ctrl+e`) (16:9, 4:3, 1:1, 4:5, 3:4, 9:16 or the video's own; `a` changes it), what's around it dimmed, and the picture moved under it. A drag, or two fingers on a trackpad, pans; the mouse wheel or a pinch zooms; `Alt`+drag or a two-finger rotation levels the horizon; a double click puts it back. Where the clip is stabilized, panning turns the stabilized camera (Gyroflow re-renders the frame looking that way) and zoom is its field of view; elsewhere it moves and scales the picture. `Enter` saves it as a photo; `c` or `Esc` leaves | `gyroflow/crop` |
| `Alt+s` | The frame on screen as a photo, in the project's `_Stills/` folder (beside `_Exports/`; with no project, `stabilized/` next to the clip): stabilized as it plays, cleaned up from the frames around it (as `Ctrl+p` does), with Save frame's frame and camera. The SDR look is the image, with a gain map to the HDR one (HLG), so an HDR screen shows it brighter: a HEIC on macOS, an Ultra HDR JPEG elsewhere (Android, Chrome and Apple read its ISO gain map). With 3 s of the video around it, as a phone's (the Save frame panel's Still / Live Photo / Motion Photo), exported as `Shift+E` would, at most 1920 wide or high: a Live Photo for Apple's devices (macOS: a `.mov` next to the HEIC; Photos and the iPhone take the two as one) or a Motion Photo for Android's (`<name>.MP.jpg`, an Ultra HDR JPEG with the video inside it, as Android's cameras write it) | `gyroflow/photo` |
| `Shift+E` | Export the clip as it plays; with an A-B loop (`l`), only that part. On a clip being exported, twice: cancel | `gyroflow/export` |
| `k` | Save the A-B part (`l`) as a moment, asking its name; on a moment whose part was moved: update it | `workspace/moment` |
| `M` | A mark here; on a mark (within half a second), the camera's too: remove it | `highlights/mark` |
| `X` | This clip isn't interesting: its preview in `_Clips/` to the Trash; again: made again | `workspace/not-interesting` |
| `Ctrl+x` | Cut the A-B part out of a clip spoiled in part, without re-encoding, after asking: the original to the Trash, its marks and moments following ([how](docs/lossless-cut.md)) | `workspace/cut-out` |
| `Ctrl+e` | Open the clip's project in Gyroflow (written next to the clip first when it has none); saving it there reloads the stabilization here. A clip without motion data opens in Gyroflow as it is, to stabilize it from the picture | `gyroflow/edit` |
| `Ctrl+t` | Telemetry off/on; it sticks for the next clips | `dji_telemetry/toggle` |
| `Ctrl+]` / `Ctrl+[` | The next highlight the camera marked, and the one before, landing a couple of seconds before it (`before=`; on your own marks, `M`, right on them); `Ctrl+[` again right after goes on to the one before that. mpv's chapter keys, `Page Up` / `Page Down`, go to them too, at the mark itself | `highlights/next`, `highlights/prev` |
| `Ctrl+r` | A note for an AI agent: the areas of the frame you mean and a comment, saved with what's on the screen and what the player is doing (see [Notes for an AI agent](#notes-for-an-ai-agent)) | `agent/report` |
| right click | The plugins' menu, when there's no side panel (mpv's own menu with it) | `footage_menu/menu` |
| `Tab` | The panel beside the video, hidden or shown | `side_panel/toggle` |

To use other keys, bind the names in `input.conf`, e.g. `Ctrl+d script-binding dlogm/sdr`. What the menu sets
without a key, a key can too, with the plugins' script messages (listed at the top of each plugin's `main.lua`),
e.g. `Ctrl+j script-message-to gyroflow horizon-lock toggle` or `Ctrl+2 script-message-to gyroflow view wide`.

## In detail

### Options

In ActionFootagePlayer, `~/Library/Application Support/ActionFootagePlayer/mpv/script-opts/<plugin>.conf`
(Windows: `%LOCALAPPDATA%\ActionFootagePlayer\mpv\script-opts\`); with the plugins in your own mpv,
`~/.config/mpv/script-opts/<plugin>.conf`, or on the command line as `--script-opts=dlogm-mode=sdr` (a value with
commas: `--script-opts-append=dji_telemetry-fields=depth,pressure`). The menu's Remember as default writes them there:

| | |
|---|---|
| `dlogm.conf`: `mode=hdr` | what D-Log M clips start in: `hdr`, or `sdr` (DJI's Rec.709 LUT) |
| `dlogm.conf`: `dv_live_l1=yes` | Mac, the app: *HLG+DV* measures each frame's light levels as it plays and sends them with it (`--macos-dovi-live-l1`); `no`: the trims' fixed ones, and the side panel's scene-level sliders back |
| `dlogm.conf`: `denoise=no` | `yes`: clips start with the noise reduction on (`Ctrl+n`). How strong it is: `glsl-shader-opts=denoise_strength=2` in `mpv.conf` (1 by default, tuned for 10-bit log) |
| `gyroflow.conf`: `hwdec=yes` | decode the clips it stabilizes with VideoToolbox (Linux, Windows: `auto-copy`) whatever mpv's `hwdec` is (software decoding doesn't keep up with 4K); `no` leaves it to `hwdec` |
| `gyroflow.conf`: `workers=3` | GPU worker threads for the stabilization |
| `gyroflow.conf`: `max_fps=0` | every frame plays. Above 0, clips faster than this play every other frame (`30`: 60 fps at 30) when the stabilized frame leaves edges anywhere in the clip, as Gyroflow measures it for the project and view, and they're filled: for a computer where the fill doesn't keep up |
| `gyroflow.conf`: `fps_limit=0` | every frame is stabilized. `30` or `15`: faster clips play at that rate (60 fps at 30 or 15), the other frames skipped before anything else is done with them; at full size while paused (`Ctrl+f`, `paused_full_size=auto`) every frame is there again (stepping through them, the clean still frame). Right click > Quality sets it |
| `gyroflow.conf`: `edge_fill_rate=auto` | the edge fill rate: how often the sphere trail (`edges=sphere`) is written while playing. `auto`: whenever a GPU worker is free; `50` or `25`: from that % of the frames that play (with `fps_limit=30`, 25 % is ~7.5 a second), for a laptop whose CPU and GPU share their power, where the writes take what the frames need even while a worker is free. Changes without setting the chain up again. Right click > Quality > Edge fill rate sets it |
| `gyroflow.conf`: `ask_when_slow=yes` | when the stabilization keeps dropping frames (more than 2 s), playback pauses and offers a lower frame rate, resolution or edge fill; what's picked is kept here. `no`: it plays on as it can |
| `gyroflow.conf`: `view=normal` | what clips without a project start in: `normal`, or `wide` (the wider view) |
| `gyroflow.conf`: `level=medium` | the level clips without a project start in: `weak`, `medium` or `strong` (`Ctrl+k`) |
| `gyroflow.conf`: `lookahead=4` | a clip without a project plays once the first 4 + this many seconds of its motion data are read; the rest is read as it plays, from what mpv reads anyway (see Stabilization). `0`: all of it before it plays |
| `gyroflow.conf`: `horizon_lock=project` | clips without a project: `yes`, the horizon locked level (Gyroflow's horizon lock at 100 %, in exports too); `no`: not locked; `project`: as the preset says. A clip with a `.gyroflow` plays with the project's |
| `gyroflow.conf`: `underwater=auto` | the lens of clips without a project: `yes` under water, `no` above water, `auto` as the camera says (its *In water* / *Out of water*; for cameras that don't record it, as the preset says), `project` as the preset says. A clip with a `.gyroflow` plays with the project's. See Stabilization |
| `gyroflow.conf`: `chromatic=yes` | the water's color fringes taken out where the lens is under water (`Ctrl+a`), or `no`: as recorded |
| `gyroflow.conf`: `edges=sphere` | what fills the edges the stabilized frame doesn't cover: `sphere` (what was seen in that direction of the world; where nothing was yet, the colors around spread in), `smooth` (only the frame's own colors spread out from its edge), or `off` (the project's background color) |
| `gyroflow.conf`: `edges_blur=2` | blur of the edge fill (% of the frame height) |
| `gyroflow.conf`: `resolution=display` | the stabilized frame rendered no bigger than the display the window is on shows it fullscreen (a 1080p display: a quarter of 4K's pixels to stabilize and shade), from video scaled down to match first; a move to a display of another size sets it up again, paused for a moment. `video`: at the video's own size (up to 4K). `1080` or `720`: no bigger than the display, and its short side no longer than that |
| `gyroflow.conf`: `still=yes`, `still_radius=5`, `still_delay=0.4`, `still_grain=0.25` | the clean still frame (`Ctrl+p`): on or off, how many frames on each side it's made from (1-7), how long after pausing it starts (s), and how much of the brightness's grain stays (0: none, 1: all) |
| `gyroflow.conf`: `live_photo=yes` | photos with the motion around them: `live` (Live Photo, macOS), `motion` (Motion Photo), `no` (the still alone); `yes`: Live Photos on macOS, Motion Photos elsewhere. The Save frame panel sets it |
| `gyroflow.conf`: `paused_full_size=off` | paused, the frame stays as it played; `Ctrl+f` renders it at full size. `auto`: `still_delay` after each pause it's rendered at full size (every frame, whatever `resolution` and `fps_limit` are), in the same re-render as the clean still frame; on a slow GPU each pause and each play costs a moment for it. Right click > Quality > Paused frame sets it |
| `gyroflow.conf`: `zoom_margin=1.25` | how much more of the video it keeps than the display shows, for the stabilization's zoom, which enlarges the part that shows |
| `gyroflow.conf`: `gyroflow_app=` | Linux, Windows: the Gyroflow executable `Ctrl+e` opens projects with |
| `dji_telemetry.conf`: `fields=` | what to show, in that order; not set: what the clip has (see Telemetry) |
| `dji_telemetry.conf`: `units=metric` | or `imperial`: feet, mph, °F |
| `dji_telemetry.conf`: `position=bottom-left` | `top-left`, `top-right`, `bottom-right` (top left has mpv's messages, top right the exports' progress) |
| `dji_telemetry.conf`: `font_size=28` | at a 720-pixel-high window; it scales with the window |
| `dji_telemetry.conf`: `visible=yes` | `no`: shown only after `Ctrl+t` |
| `highlights.conf`: `before=2` | seconds before each of the camera's highlights a jump lands: the button is pressed once the moment has happened (your own marks: right on them) |
| `highlights.conf`: `chapters=yes` | the highlights as chapters too, which mpv's seek bar shows as marks; `no`: only the keys and the menu |
| `footage_menu.conf`: `setup=` | a setup script the menu offers to run again (ActionFootagePlayer sets its own) |
| `footage_menu.conf`: `version=` | the version the menu shows beside the app's name (ActionFootagePlayer sets its own) |
| `agent.conf`: `socket=` | the IPC socket an agent reaches the player by, when mpv has none (`input-ipc-server`); empty: see [Notes for an AI agent](#notes-for-an-ai-agent) |

The telemetry plugin's file is `dji_telemetry.conf`, with an underscore: mpv names a script after its folder, with
anything but letters and digits made `_` (the menu's: `footage_menu.conf`). The menu's Remember as default writes
these files, keeping whatever else is in them.

### Stabilization

Make the project in Gyroflow and save it next to the clip (`Ctrl+S` in Gyroflow): mpv uses it. Or start from
what's playing: `Ctrl+e` pauses the clip and opens its project in Gyroflow. A clip that plays without a
`.gyroflow` next to it (it plays with the default preset) gets one first, written by the
plugin from what it renders: the same lens profile, smoothing, zoom and refraction, so Gyroflow starts from
what you saw. The view and the horizon lock go into it as they are: from the wider view, the project has
Gyroflow's zoom off at that view's FOV, so it looks the same there, and here once it's saved (a clip with a
project starts in it as it is: On (Gyroflow project)). A `.gyroflow` that's already there is opened as it is, never overwritten. Each time you save in
Gyroflow (`Cmd+S` overwrites the project it opened), mpv reloads the stabilization with it, at the same spot
in the clip. The project then stays next to the clip, and mpv plays the clip with it from then on. The project
`Ctrl+e` writes has the level as Gyroflow's Smoothness and FOV with its zoom off (Gyroflow's Default second pass
measures against the clip's largest drift, not the crop, so it smooths a little differently there), and the motion
data in it, as Gyroflow's own Save writes it: Gyroflow opens it without reading the motion data from the video again.

A clip without motion data (shot with the camera's stabilization on, or by a camera that records none) can be
stabilized from the picture: `Ctrl+e` (or the menu's Stabilize in Gyroflow) opens the video itself in Gyroflow,
where its motion is found by optical flow. Save the project next to the clip there, and the clip loads again where
it was, stabilized with it. On Linux and Windows this needs `gyroflow_app` (Options).

**A clip with a project plays as the project says.** It was made in Gyroflow on purpose, so the clip starts with
its own horizon lock and lens (under water or not, keyframed or not), whatever the menu had for the clip before
and whatever `gyroflow.conf` says: those are for clips without a project, which play with the default preset.
Set from the menu or the keys while a clip with a project plays, they're for that clip only; the next one starts
with its project's again. The edge fill (`Ctrl+b`), the color fringes (`Ctrl+a`) and the clean still frame
(`Ctrl+p`) aren't Gyroflow's, so they're the player's for every clip.

The menu's Stabilization → Horizon: *Locked level* keeps the horizon level, whatever the project says (Gyroflow's
horizon lock at 100 %, its roll and pitch as the project has them); *Not locked*; or *As the project says* (as
the preset says, for a clip without one), what a clip with a project starts with. Locked needs more zoom, so the
edges show more.

The project is always the `.gyroflow` next to the clip: the video file itself is never written to, so a backup
of it stays as it is.

Clips with gyro data of their own, recorded with the camera's stabilization off, play stabilized even without a
project: DJI with EIS off, GoPro with HyperSmooth off (tried on a HERO12 Black), and (not yet tried on a real clip, see
Other cameras) Insta360 with FlowState off in the camera, when Gyroflow's database has their lens. Only when the camera recorded
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
- smoothing 0.5, zoom limit 140 %, horizon lock off: what Gyroflow opens it with. Here, the level's smoothing and
  crop take their place (below).

**How strongly: the level.** A clip without a project plays at one of three levels (`Ctrl+k`, the menu's
Stabilization → Level), each a crop and a smoothness, all known when the clip opens: *weak* crops 10 % of the distance
from the frame's center to its edge and smooths at 50 % (Gyroflow's Smoothness), *medium* (the default) 20 % at 75 %,
*strong* 30 % at 100 %. The crop is taken from the widest view the frame covers while the camera holds still (from the
lens alone; the log has it), with Gyroflow's zoom off. The smoothing is Gyroflow's Default, with its second pass held
to the crop: where the camera turns further than the crop has room for, the picture follows it sooner, so the
edges show only when it turns that far, filled as `Ctrl+b` says. The wider view of `Ctrl+g` smooths as the level does.
Switching levels smooths the clip again in a few milliseconds, while it plays (`vs-gyroflow/src/levels.rs`,
`smoothing.rs`).

Because nothing in it depends on the whole clip, each moment only on the few seconds around it, a clip without a
project plays from its first seconds of motion data: from a NAS in about a second, where reading all of it first took
5-10 s. The rest comes in as it plays: not read from the file in between the video's reads (on a NAS that breaks the
read-ahead macOS does for the video: its reads slowed from under a millisecond to 34-45 ms), but taken from the file
cache, where the video's own reads put it, the motion data of a moment sitting next to its video. What's played is as
it would be with all of the motion data once 4 s more are known (`lookahead`; measured on clips from the NAS: p99
0.02-0.9 px at the frame's center, the most in the first seconds, and the video's reads as fast as without it). After
a seek past what's read, the seconds there are read from the file: the frames play uncorrected meanwhile (about 2 s from
the NAS), and the correction comes in over half a second. While it's paused, the rest is read
(`vs-gyroflow/src/motion.rs`). A project's clip, whose motion data the project has, reads only a tenth of its
samples for *In water* / *Out of water* (25 s for a 5-minute clip from the NAS before).

**Lens under water, above it, as the camera says, or as the project says.** The menu's Stabilization → Lens sets
the refraction for any clip, a project's too: *Under water* (1.33, or the project's own when it has one), *Above
water* (none), *As the project says* (as the preset says, for a clip without one), what a clip with a project
starts with, or *Auto*, the default for clips without one: as the camera says over the clip, its *In water* /
*Out of water* (the telemetry's `water`: the barometer's flag, or the camera's underwater confidence). A clip that goes in and out of the water then has
the refraction where it's in and none where it's out, changing over the one frame between. Only a camera that
records it has Auto (the Osmo Action 6); for clips from other cameras it's greyed, and the clip plays as its
project or preset says (the default preset: under water). A project that keyframes the refraction itself keeps
its keyframes in Auto. It sticks for the next clips without a project, like the horizon lock, and goes into exports
and into the project `Ctrl+e` writes (Auto as keyframes, which Gyroflow shows on its timeline: so a project made
that way keeps the camera's *In water* / *Out of water*).

For clips from a camera without Auto shot with EIS off in air, pick Above water, or use a preset without the
refraction (save one from Gyroflow: Export → Create settings preset, as `default.gyroflow` in one of the places
above), or give those clips a project of their own.

**Color fringes under water.** Water bends blue more than red, so behind the flat glass of an action camera red and
blue land apart from green towards the edges of the frame (lateral chromatic aberration): red inwards, blue outwards,
1-2 px of a 4K frame halfway out and more further out. The camera takes out its lens's own fringes, but knows nothing
of the water. Wherever the stabilization has the lens under water (its refraction, frame by frame: as the camera's
*In water* / *Out of water* says, or the project or preset, or as Stabilization → Lens / `Ctrl+u` sets it), each color is taken from where its own ray lands, with water's index for that color: blue, which lands
furthest out, stays, and red and green are moved out to it, so nothing is taken from past the picture's edge. It's
done on the GPU in RGB (`mpv/scripts/gyroflow/chromatic.glsl`), with where the lens's axis is in each stabilized frame
from the plugin (`vs-gyroflow/src/dispersion.rs`). How far each color goes is physics only, nothing fitted to footage
(`tools/water_dispersion.py`): the lens profile gives each pixel's angle to the flat port, and water's index for each
channel is averaged over its spectrum, for a grey object in daylight through 3 m of water. On Osmo Action 6 dive clips
red's fringe at the edges of the frame, up to 3-7 px, goes to within about a pixel; blue's, up to 2-4 px, turns over
to about 1-1.5 px the other way. Where the frame's own border shows (the wider view), it stays as recorded, against
the edge fill or the background alike. Out of the water nothing moves. `Ctrl+a` leaves the fringes as recorded, and
takes them out again; it switches nothing on above water (the lens stays as it is). Playback only,
exports are as before. It needs `vo=gpu-next` or `gpu` (mpv's default is `gpu-next`).

**Edges.** When the project's zoom isn't enough to hide them (a fixed zoom, or zooming off), the stabilized frame
leaves parts of the window empty, which Gyroflow fills with the project's background color. Here they're filled
with something less noticeable, blurred and blended into the picture over a soft edge; `Ctrl+b` switches between
them while playing, and `edges_blur` (Options) sets how blurred:

- **sphere**, the default: what was last seen in that direction, by the gyro. It's taken from the
  whole source frame, not just what the output shows, so it also has what the zoom crops away and what the camera
  saw while shaking with no edges showing, and it stays where it was in the world, so it lines up with the picture
  through a pan. Only the camera's turning is known: near things that moved past it, or by themselves, are off
  (half-life 5 s). Where a direction wasn't seen yet, the colors around spread in, smoother the farther they go, a
  little desaturated and darker, as a lens's periphery falls off; that's made up on the sphere too, from all it
  has, so it stays put in the world and changes slowly instead of flickering with what passes the edge. Where what
  the sphere has just past the frame's edge no longer matches the edge (something moved, the light changed), it's
  taken away for what's made up from the edge as it is, so it doesn't show as a seam.
- **smooth**: only the frame's own colors spread out from its edge the same way: nothing of earlier frames, so
  nothing that can be off, and the cheapest.
- **off**: the project's background color, as Gyroflow renders it.

Where nothing is known, the colors around rather than a copy of the frame (scaled up, or unstabilized): its content
moves at another scale than the world, and the eye catches that at the side even blurred.

A project's zoom limit can lower its smoothing rather than show edges, so such a clip may have none. Whether a clip
with a project does is Gyroflow's own measurement when it loads: where the frame it renders is wider than the part the
stabilized picture covers. A clip without one plays before the rest of its motion data is read, so it's ready to fill
edges wherever they show. A clip with edges anywhere gets the sphere written as it plays,
edges showing or not, so it has what to fill them with when they show. That's on a thread of its own, and only while
the GPU has room: the frames come first, and where they need all of it the sphere is written less often, what it has
fades into the trail by place, and every frame still plays. Where the GPU shares its power with the CPU (a 15 W
laptop), a free worker doesn't mean free power: Quality > Edge fill rate has the sphere written from only 50 % or
25 % of the frames that play (`edge_fill_rate`, Options). A clip without any has no sphere to write. The wider view of
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
of `Ctrl+g`, the edges filled as `Ctrl+b` has them, the horizon locked or not and the lens under water or not as
the menu has them, and the same log curve (`Ctrl+l`'s too, none after `L`) with the same LUT: HDR,
or Rec.709 in an SDR look (`Ctrl+s`): DJI's LUT for D-Log M, or NBCU's conversion for any curve (the menu's
Export in SDR does that whatever plays, in the SDR look last chosen). With an A-B loop set (`l` twice), only that part.

It asks the export's name first, as a moment does (Enter alone takes the suggestion), and the file goes to the
trip's **_Exports** folder: `_Exports/Turtle.mov`, with the look in parentheses unless it's HDR (`Turtle (SDR).mov`,
`Turtle (Dolby Vision).mov`), and ` 2` when the name is taken. A part that's a moment (played from its file or the
menu, or the same part to the frame) is named after it; any other part is also saved as the moment
`export_Turtle`, so it can be found and played in full quality after the export is deleted to make room. Inside the
export is its link to the original: opened in the player, it plays as it is, and the menu's **Open the original**
goes to its part of the clip. Without the workspace plugin (your own mpv with only some of the plugins) it doesn't
ask, and the file is named as below.

Playback goes on meanwhile, with the
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

The result goes to the `_Exports/` folder of the clip's project (the trip folder with `_ActionFootagePlayer/` in it
above the clip, as the player initializes it), or with none to a `stabilized/` folder next to the clip, as
`<name>_stab.mov` (a part: `<name>_stab_<from>-<to>s.mov`). `--out <folder>` puts it elsewhere, `--output <file>`
names the file.

**Up to 0.7, exports went to a `stabilized/` folder next to the clip.** Those folders are left as they are, and
nothing reads them back; a script of yours that expects new exports there passes `--out stabilized` (or the
folder's full path).

What it renders:

- **Log** (D-Log M, D-Log, GP-Log, GP-Log2, 10-bit I-Log: the curve the camera's metadata says, as the dlogm plugin
  reads it, or `--curve dlogm|dlog|gplog|gplog2|ilog`): HEVC Main10, Rec.2100 HLG (the same LUTs as playback,
  `luts/<curve>_hlg.cube`), tagged so macOS, iOS and YouTube show it as HDR. `--sdr` makes D-Log M Rec.709 with
  DJI's LUT instead, `--nbcu` any curve Rec.709 through NBCU's HLG → SDR conversion (the curve's LUT and NBCU's,
  made into one for the export).
- **Other clips** (and `--curve none`): HEVC Main10 in the colors they were recorded in.
- `--level weak|medium|strong`: a clip without a project at that level (`Ctrl+k`; default medium).
- `--wide` renders the wider view of `Ctrl+g`, `--fov <x>` at a FOV of your own; `--edges sphere|smooth|off` fills the
  edges as playback does (`--edges-blur <%>`); without them, as the project says, edges in its
  background color. A clip whose frame covers the output throughout has nothing to fill, and isn't.
- `--underwater yes|no|auto|project`: the lens under water, above it, (the default) as the camera says over the
  clip, or as the project or preset says, as the menu's Lens has it (see Stabilization).
- `--prores`: ProRes 422 HQ instead of HEVC, for editing. The bit rate of HEVC follows the source's.
- **Log clips with nothing to stabilize them with** (D-Log M shot with EIS on, say), and any clip with
  `--no-stabilize` (what `Shift+E` passes after `Ctrl+g` turned the stabilization off): the color conversion only,
  as `<name>_color.mov` in the same folder, at the clip's own size. Clips with neither, nothing to stabilize and
  not log, are skipped: there's nothing to change.
- Audio is copied as it is, and the file keeps the clip's dates. An existing output is kept unless `--force`.
- `Ctrl+C` stops it and removes the unfinished file. `--background` runs it at background priority, to keep
  working while it runs; `kill -USR1 <pid>` / `kill -USR2 <pid>` switch that later. `--progress <file>`
  writes the progress there as JSON twice a second (that's how mpv shows it).
- `--output <file>` writes that file instead (one clip).

**Previews** are what the player's moments are made of ([Moments](#moments), [docs/moments-and-workspaces.md](docs/moments-and-workspaces.md)):
a small MP4 that Finder, Explorer and a phone show and play, in SDR whatever the clip is.

```bash
~/.config/mpv/scripts/gyroflow/gyroflow-export --preview still --from 20 --to 30 --at 26.4 \
    --dates 2026-10-24T09:46:07+03:00 --output _Moments/Turtle.afplink.mp4 DJI_0093_D.MP4
```

- `--preview still`: the frame at `--at` (else the middle of `--from` to `--to`), stabilized as an export would be
  with the same options, held for `--from` to `--to` (the whole clip without them): H.264 at 960 pixels on the
  long side, 10 fps, one keyframe, its frame also the cover art, so the frame is the file's icon. 150-350 KB for a
  camera's frame. D-Log M goes through DJI's Rec.709 LUT, other log curves and HLG through NBCU's conversion;
  a clip with nothing to stabilize or convert gets a preview too. Without `--output`, `<name>.afplink.mp4` next
  to the clip (or in `--out`): the double extension says it's a link, and it opens as an MP4 anywhere.
- `--preview flipbook`: the clip at a glance ([Clip previews](#clip-previews-the-trip-at-a-glance)): a frame at
  each of `--at t1,t2,…` (else about a dozen, 10 s apart at least), the keyframe at or before it, from the `.LRF` or
  `.LRV` beside the clip when there is one, in the same SDR look and not stabilized, 2 a second; `--cover <t>` makes
  the nearest frame its icon. It has the clip's own date.
- `--link <json>`: the link to the original the player keeps in it, an MP4 key (`ch.actionfootageplayer.link`)
  that copying, renaming and tags keep, and a remux without `-map_metadata 0` loses.
- `--dates <time>`: its date, with the time zone: the MP4's creation time, QuickTime's creation date (the day
  Photos files it under) and the file's dates, so a folder of them sorts by when it happened. Without it, the
  clip's file dates.

Exports play back in HDR in the player too, as they are: a file tagged HDR (HLG or PQ: an export, a phone's or
an action camera's HDR video) switches the screen to HDR the way a log clip does, with no LUT, and is never taken
for log and graded a second time, whatever camera metadata or `_D` name it carries. `Ctrl+s` shows it in SDR
through NBCU's HLG or PQ → SDR conversion (one in Display P3 by mpv's tone mapping, NBCU's being for BT.2020); on a
screen without HDR it starts that way, as log clips do.

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
| `water` | *In water* / *Out of water*: the barometer's flag, or the camera's own underwater confidence, which comes from the picture and so also works in a dive housing. The lens's refraction follows it (Stabilization: Lens → Auto) |
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

### Highlights and marks

A press of the camera's highlight button while it records marks that moment in the clip, and the highlights plugin
reads the marks when the clip opens: they show on the seek bar as chapters ("Highlight 1", ...), `Ctrl+]` and
`Ctrl+[` jump to the next and the previous one, and the menu's **Highlights** lists them all by time. A jump lands
two seconds before the mark (`before=` in `highlights.conf`): by the time the button is pressed, the moment has
usually happened. A mark of your own (`M`) is where you put it, and a jump lands right on it.

- **DJI Osmo Action 4, Action 5 Pro and Action 6**: the quick switch button, or the GPS Bluetooth remote, while
  recording (in video, slow motion and hyperlapse; 190 at most). DJI's other cameras (Osmo 360, Pocket 3, the
  drones) don't mark highlights. Whether the ones added later, in the camera's playback, go into the file isn't
  known yet.
- **GoPro HERO5 and later**: the HiLight button (the mode button, while recording), the voice command "GoPro,
  HiLight", or the remote.

**Your own marks**: `M` adds one where the video is, and on a mark (within half a second) removes it, a camera's
too. The first change writes `<clip>.marks.json` beside the clip, with the camera's marks copied in and flagged as
the camera's; from then on that file is the clip's marks. Marks need no project. Delete it,
and the camera's are back. The menu's **Marks** lists them by time, the camera's as "DJI quick switch" or "GoPro
HiLight". A marks file left from another clip with the same name (GoPro reuses them on every card) is renamed
`.marks.json.old` rather than shown on the wrong clip.

Each file of a recording the camera split has its own. None of it has been tried on a real clip yet: the layouts
come from ExifTool (DJI, from an Action 4) and from a HERO7 Black's header (GoPro); see
[Highlights](#highlights) in How it works. A clip with a few marked, and the seconds they were marked at, would
settle it: [Other cameras, and how to help](#other-cameras-and-how-to-help).

### Moments

A moment is a part of a clip you want to keep: the turtle, the jump. Mark it with `l` at its start and `l` at its
end, press `k`, type its name (Enter alone takes the one suggested: the last name with the next number, "Turtle 2",
or the date and time), and it's saved as a small file in a **_Moments** folder of the trip:
`_Moments/Turtle.afplink.mp4`.

- **It's a link, not a copy.** The file is one frame of the moment, in color and stabilized, held for its length,
  some 150-350 KB. Finder shows that frame as its icon, and the double extension says it's a link (it still opens
  as an MP4 anywhere). Inside it is where its part of the original clip is.
- **Opening it in the player plays its part of the original as a video of its own**, in full quality, with the
  original's stabilization, look and telemetry: the seek bar is the moment's, from 0:00 to its length, and the
  panel shows the moment (its marks, Rename, Remove). Then the next moment in the folder, and at the last one it
  stops. Opening the **_Moments** folder plays all of them one after another, in the order they happened.
- **Its whole clip**: *Open the whole clip* in the panel (or a moment's `⋯`: *In its clip*) opens the clip at the
  moment's start, its part marked but not looped, to see what came before and after. Moving its start or end
  there and `k` updates that moment; inside the moment, marking a part of it and `k` trims it.
- **Where**: in the trip's project. A project is never made by itself: the first time something needs one (a
  moment, an export, clip previews), the side panel's **Project** tab opens and asks you to initialize it, in the
  folder you choose among those above the clip (the trip's, say, so that every day's folder is in it). It's a
  `_ActionFootagePlayer` folder there, and `_Moments/`, `_Clips/`, `_Exports/` and `_Stills/` (photos) once
  there's something to keep. A clip under a folder that has one belongs to it. A card or folder that can't be written isn't supported
  yet.
- **On the seek bar** a moment is two chapters, its start and its end ("Turtle", "Turtle end"). The menu's
  **Moments** lists the clip's moments, to play, rename, remove (to the Trash) or show in Finder.
- **Stabilized as it was made**: a moment keeps the stabilization it was saved with. Over the default preset, its
  view, level and horizon lock; a clip with a Gyroflow project gives the
  moment a copy, `_Moments/Turtle.gyroflow`, trimmed to it, so the clip's project changed later doesn't change the
  moment. **Stabilize on its own** (the panel, for the moment playing; *New moments stabilized on their own* in the
  project's `⋯`): its smoothing and crop from its own motion, not the clip's around it. `Ctrl+e` on a moment opens
  its own project in Gyroflow, made then from what it plays with if it has none; a trim changed there becomes the
  moment's ends. While a moment plays, each stabilization setting has **As saved** first: every moment plays as it
  was saved. Pick a value instead and it's for every moment this session (a playlist of moments all at Strong, say),
  without touching how clips play; back to As saved, each moment is its own again. A moment with its own Gyroflow
  project plays as the project says (its horizon, crop and smoothing): what it saves is the project, never settings
  over it. None of it changes a moment: only
  **Keep in Turtle** in the panel writes how it's viewed into it.
- **Changing one**: play it (or open it in its whole clip), move its start or end with `l`, press `k`, and Enter
  updates it; a new name saves a new moment instead. Its Finder tags stay.
- Sort a _Moments folder by date in Finder: the files have the moment's own date and time, so they come in the
  order things happened, whatever their names.

### Clip previews: the trip at a glance

A folder of log footage is a wall of gray icons. **Project → Make clip previews** (right click) makes a small
file for each clip of the trip in a **_Clips** folder, beside the footage's own folders:
`_Clips/Day 1/DJI_0068_D.afplink.mp4`.

- **What a preview shows**: a flipbook of the clip, in color. Its frames are its marks, the camera's highlights and
  yours, or about a dozen frames through it, 2 a second, at 960 pixels. Finder's icons are then the trip in color,
  and Quick Look (space bar) flips through each clip. A double click in the player plays the clip itself.
- **How long it takes**: a second or so per clip. Only keyframes are decoded, from the low-resolution copy the
  camera writes beside the clip when there is one (DJI's `.LRF`, GoPro's `.LRV`). That's far less to read from a
  NAS. They're made one at a time in the background, with the progress in the bottom-left corner, and never by
  themselves: the menu item makes them for the clips that have none, new footage that is.
- **Sorting the trip**: delete a preview (in Finder, or `X` in the player) and the clip isn't interesting. *Make
  clip previews* doesn't bring it back (`_ActionFootagePlayer/clips-made.txt` lists what was made). `X` again, or
  *Make again for removed clips*, does.
- **Variants**: a clip preview plays with its clip's stabilization. **Its own stabilization** (the panel, while it
  plays) gives it a project of its own beside it (a copy of the clip's, or made from its motion data as it plays):
  copy the preview in Finder for another, and each is a variant of the same clip.
- **Rename and group them as you like**: a preview is found by the link inside it, so renamed ("wreck dive"), or moved
  into folders of your own within Clips, it's still the clip's. (Renaming the clips themselves, the originals, would
  lose them: their moments and previews point at their names.)
- **Ignore subfolders** (next to *Make clip previews*): new previews all go in Clips itself, not in the clips'
  folders, for a camera's folders by date that mean nothing to the trip. Two clips with the same name get their
  folder's name in parentheses.
- A clip under a second long has a single keyframe, at its start: its preview is made from the frames as they are.
  If some can't be made, the panel says how many, and *why* opens the list.

### Notes for an AI agent

For working on the player together with an AI agent (Claude Code, in this repository): what you see, and what the
player is doing as you see it, saved for the agent to read. `Ctrl+r` (or the menu's ActionFootagePlayer → Note for
the agent…) pauses the video; drag a rectangle round what you mean (`Shift`+drag adds another, a plain drag starts
again), press `Enter`, write a line, `Enter`. `Esc` in the comment goes back to the rectangles; `Esc` there leaves
without saving. Each note is a folder of its own in
`~/Library/Application Support/ActionFootagePlayer/agent/reports/` (Windows:
`%LOCALAPPDATA%\ActionFootagePlayer\agent\reports\`; the plugins in your own mpv: `~/.config/mpv/agent/reports/`),
named by date, time and clip:

| | |
|---|---|
| `report.json` | the comment; the areas, in the frame's pixels, the window's, and 0..1 of the frame; the clip, the time and the frame number; and the state when the note started: every plugin's (`user-data/`) and the player's (the video before and after the filters, the filters and shaders, the window, the display, dropped frames) |
| `screen.png` | the window as it was, with what was over the video (telemetry, messages) |
| `marked.png` | the same with the areas drawn on it, numbered |
| `frame.png` | the frame at its own size (after the stabilization), with nothing over it |
| `log.txt` | mpv's last 2000 lines of log (verbose) |

Then ask the agent to go through the notes. While the player runs, an agent can also reach it live, through mpv's
[JSON IPC](https://mpv.io/manual/master/#json-ipc): the socket's path is in `agent/session.json` (`socket`, `pid`),
`script-message-to agent snapshot [<file>]` writes the state as `report.json` has it (to `agent/state.json` by
default), and `script-message-to agent say <text> [<seconds>]` shows a line over the video. The socket is mpv's
`input-ipc-server` when one is set (the Windows app's pipe, an `mpv.conf`); otherwise `agent/mpv.sock` (Windows:
`\\.\pipe\ActionFootagePlayer-agent`), or `socket=` in `agent.conf`.

## Installing, in detail

### Plugins in your own mpv

`mpv-plugins-<version>-<os>-<arch>.zip` has them all, as the folders below: unzipped into mpv's `scripts/` folder
(`unzip mpv-plugins-*.zip -d ~/.config/mpv/scripts`; on Windows `%APPDATA%\mpv\scripts\`), with what each one
needs as below. Or one at a time:

Each plugin is a folder in mpv's `scripts/` folder (`~/.config/mpv/scripts/`) and works without the others:

- `mpv-dlogm-<version>.zip`, on any system (`%APPDATA%\mpv\scripts\` on Windows). It has the HDR shaders and NBCU's SDR ones; DJI's
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

- `mpv-side-panel-<version>.zip`, on any system: the settings in a panel beside the video (plain Lua), with
  footage-menu for its Clip buttons.

- `mpv-workspace-<version>.zip`, on any system: the project a clip belongs to, for the other plugins (plain Lua).

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
[Homebrew](https://brew.sh) and Xcode (`brew install ffmpeg rustup`):

```bash
tools/build_mpv.sh
```

```bash
tools/dev_app.sh
```

`tools/build_mpv.sh` builds the app's mpv once (a few minutes): the commit in `tools/mpv.lock`, with libplacebo
linked in, the other libraries Homebrew's (it installs mpv's dependencies with Homebrew first; `--no-deps` skips
that). `tools/dev_app.sh` then builds ActionFootagePlayer the way the release does: the plugins, that mpv, and
Homebrew's ffmpeg, Python and VapourSynth in the app (so it needs no Homebrew to run). Without `tools/build_mpv.sh`'s
mpv it takes Homebrew's (`brew install mpv`); `MPV_BIN` names another. Then it puts it in `~/Applications` in place of the one
there, which it keeps as `build/ActionFootagePlayer-previous.app`, and runs the setup (the default app for MP4, the
Quick Actions, DJI's LUT).
- **What it builds:** uncommitted changes too, and `~/.config/mpv` isn't touched. The version says what it is: `0.6.2`
  on a release's tag, else e.g. `0.6.2-dev.3+1a2b3c4` (`.dirty` with uncommitted changes).
- **How long:** a few minutes, most of it the first cargo build.
- **How it differs from the release:** ffmpeg and the libraries mpv links are this Mac's Homebrew ones, where the
  release's build gets the latest (`brew upgrade ffmpeg`, then `tools/build_mpv.sh` again, first). Each build is signed anew (ad hoc), so macOS may ask
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

Unzip `mpv-gyroflow-<version>-linux-x86_64.zip` or `mpv-gyroflow-<version>-windows-x86_64.zip` (`-arm64.zip`)
into mpv's `scripts/` folder (`~/.config/mpv/scripts/`, `%APPDATA%\mpv\scripts\`), with `mpv-dlogm-<version>.zip`
next to it for D-Log M. On Linux, `./install.sh` from a checkout also builds and installs all the plugins (with
DJI's LUT, as on a Mac). With `--apps` it also puts ActionFootagePlayer in the applications menu: the same mpv, by
its own name and icon (`~/.local/share/applications/`), made the default app for MP4 videos. On Windows, the app
has all of it: [The Windows app](#the-windows-app).

What differs from macOS:

- Decoding is mpv's `hwdec=auto-copy` (VAAPI, NVDEC, D3D11...) for clips it stabilizes, and video bigger than the
  display (or than 4K) is scaled down on the CPU.
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
as default), DJI's LUT and its shader (`dlogm/`), your own default preset if you make one
(`gyroflow/default.gyroflow`, see Stabilization), and the notes for an AI agent (`agent/reports/`, see
[Notes for an AI agent](#notes-for-an-ai-agent)).

ActionFootagePlayer is mpv itself, as an app of its own: the app's executable (`macos/player.sh`) starts the mpv it
carries (`Contents/Helpers/`, or Homebrew's in an app built without it) with the plugins, as the app of the bundle (mpv's `MPVBUNDLE`), so it's ActionFootagePlayer in the Dock, with
its icon, and gets the videos Finder opens with it. A video opened while one plays takes its place. mpv's own menu
items still say mpv (About mpv, Quit mpv). The setup is offered from inside mpv (`macos/setup-offer.lua`), once
it runs.

### Upgrading

**From 0.6.4**, a project stored inside a video (`gyroflow-embed --move`) isn't read any more: the project is
the `.gyroflow` next to the clip, and the video file is never written to. Such a clip plays with the default
preset. Before updating, take its project back out with 0.6.4's command, which also restores the file as the
camera wrote it:

```bash
~/.config/mpv/scripts/gyroflow/gyroflow-embed --extract DJI_0009_D.MP4  # DJI_0009_D.gyroflow next to it
~/.config/mpv/scripts/gyroflow/gyroflow-embed --remove DJI_0009_D.MP4
```

`gyroflow-embed` is now `gyroflow-project`, which writes the project a clip plays with (what `Ctrl+e` runs).

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
| GoPro | HyperSmooth off (`EISA`/`EISE`), lens from Gyroflow's database; played in the app on HERO12 Black clips, checked on GoPro's own sample clips, and on a HERO12 clip from the command line (HyperSmooth on: rightly not stabilized) | GP-Log2: the Protune color, `PTCL` in `udta`'s GPMF, is `GPLOG2` (gpmf-parser's list; read as `NATURAL` on a HERO12, a GP-Log2 clip not seen). GP-Log is `LOG`, used with the camera (`MINF`) and firmware (`FIRM`): HERO12 Black on 1.20 or later and HERO13 Black, where GoPro's own LUTs put 18 % grey where Zeb Gardner's does (HERO12 Black 2.32 read from a clip; GP-Log played in the app on a HERO12 Black clip) | GPS, speed, exposure; checked on GoPro's sample clips, and the exposure on a HERO12 clip | `HMMT` in `udta` (HERO5, HERO6), `HLMT` in `udta`'s GPMF (HERO7 and later): checked on a HERO7 Black's header, no clip seen |
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
- **`workspace`**: the project a clip belongs to, its trip folder, for the other plugins, and its moments (`k`,
  [Moments](#moments)): where a clip's Gyroflow project is, and soon its marks and exports
  ([docs/moments-and-workspaces.md](docs/moments-and-workspaces.md)). Plain Lua, any mpv 0.38 or later for naming
  moments (older ones save them with the suggested name); the moments' previews need the gyroflow plugin. Without
  it, the gyroflow plugin looks next to the clip, which is the same place for now.

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
- `maker`, on `Ctrl+s`: DJI's 33³ D-Log M → Rec.709 LUT at `HOOK MAIN`. The `format` filter tags the video
  BT.709/BT.1886, so mpv's color pipeline runs a BT.709 → BT.709 identity and the LUT's output reaches the display
  unchanged.
- `nbcu`, on `Ctrl+s` again: the `hdr` shader, then NBCUniversal's 33³ HLG → SDR (BT.709) display-light
  conversion (`luts/3-NBCU_HLG2SDR_DL_v1.2.cube`, `shaders/nbcu_sdr.glsl.in`), tagged as `maker`. NBCU's are Type III
  LUTs (full range processing of narrow range signals: the signal's 0..1 is 64..940 of 1023 of the LUT's domain,
  in and out) and their highlight knee needs tetrahedral interpolation, so the shader does it from the texels.
  HLG's 75 % maps to 95 % SDR, highlights roll off to 109 %, clipped at 100 % here. It's a standard for single-master
  HDR/SDR broadcasting rather than a look: next to DJI's, the same clip is brighter in the mid-tones (18 % grey at
  ~0.40 in Rec.709 against DJI's ~0.32) and more saturated. On a file tagged HDR: NBCU's HLG or PQ → SDR alone,
  with its Dolby Vision metadata set aside (`format=dolbyvision=no`): VideoToolbox writes HLG as Dolby Vision 8.4,
  which mpv would otherwise reshape into PQ before the shader sees it; the HLG underneath is what NBCU's LUT takes.

Which one a clip starts in depends on the screen its window is on: macOS's EDR headroom (`NSScreen`'s
`maximumPotentialExtendedDynamicRangeColorComponentValue`, asked once with `osascript`), above 1 on a screen that
shows HDR. A clip keeps its look when its window goes to a screen of the other kind. On Windows and Linux
every screen counts as HDR, as before.

The LUTs are baked into the shaders as 3D `rgba16` textures (`tools/cube_to_glsl.py`, templates in
`shaders/`), so the GPU does the trilinear interpolation in one tap (NBCU's: four texel reads, tetrahedral).

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
stabilized plane by plane at the source bit depth, so 10-bit D-Log M reaches the shaders as 10-bit, and rendered no
bigger than the display shows it: video larger than that (with a margin for the zoom), and 8K always to 4K, is scaled
down on the GPU first. It renders the stabilization while mpv plays, the way Gyroflow would render it
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

### Highlights and marks

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
mpv/scripts/gyroflow/still.lua  the clean still frame (Ctrl+p): the paused frame again, through vs-gyroflow's Still
mpv/scripts/gyroflow/quality.lua  when the stabilization drops frames: pauses and offers a lower frame rate, resolution
                              or edge fill
mpv/scripts/gyroflow/platform.lua  what differs between macOS, Linux and Windows (binaries' names, temp folder)
mpv/scripts/gyroflow/underwater.gyroflow  the default preset for clips without a project
mpv/scripts/gyroflow/chromatic.glsl  the color fringes under water taken out
mpv/scripts/dji-telemetry/    the telemetry plugin  →  ~/.config/mpv/scripts/dji-telemetry
mpv/scripts/highlights/       the highlights plugin  →  ~/.config/mpv/scripts/highlights, and in ActionFootagePlayer;
                              markers.lua reads them from the file
mpv/scripts/footage-menu/     the right-click menu for the plugins  →  ~/.config/mpv/scripts/footage-menu, and in
                              ActionFootagePlayer
mpv/scripts/side-panel/       the plugins' settings in a panel beside the video (ui.lua draws it)  →
                              ~/.config/mpv/scripts/side-panel, and in ActionFootagePlayer
mpv/scripts/agent/            notes for an AI agent (Ctrl+r), and the player's IPC for it  →
                              ~/.config/mpv/scripts/agent, and in ActionFootagePlayer
mpv/scripts/workspace/        the project a clip belongs to, for the other plugins (workspace.lua finds it, plain
                              Lua; vs-gyroflow/src/workspace.rs is its twin), and link.lua, which reads the link in a
                              moment's MP4 (vs-gyroflow/src/mp4meta.rs writes it), and moments.lua, the moments'
                              part with no mpv in it  →  ~/.config/mpv/scripts/workspace, and
                              in ActionFootagePlayer
tests/                        workspace_test.lua (luajit tests/workspace_test.lua) and workspace_cases.txt, which
                              `cargo test` in vs-gyroflow/ reads too; link_test.lua, the MP4 link reader over
                              files made here, by ffmpeg and (GYROFLOW_EXPORT=<binary>) by gyroflow-export;
                              moments_test.lua
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
vs-gyroflow/                  Gyroflow VapourSynth plugin, gyroflow-probe, gyroflow-project, gyroflow-export, afp-cut (Rust)
docs/lossless-cut.md          cutting a spoiled part out of a clip without re-encoding: how afp-cut keeps what the camera wrote
macos/                        ActionFootagePlayer: player.sh (the app's executable, mpv), setup.sh (default app, Quick
                              Actions, DJI's LUT) and setup-offer.lua (offers it at first launch), default-app.py,
                              the Quick Actions' export script
macos/icon/                   its icon: AppIcon.svg, and the AppIcon.icns made from it (tools/make_icon.sh)
tools/make_app.sh             builds ActionFootagePlayer.app, with the plugins in it
tools/dev_app.sh              builds it from the checkout as the release does, and puts it in ~/Applications
tools/build_mpv.sh            builds the app's mpv (tools/mpv.lock: mpv's commit and libplacebo's, linked in)
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

ActionFootagePlayer.app also carries mpv (GPL; built by `tools/build_mpv.sh` from the commit in `tools/mpv.lock`,
with libplacebo, LGPL, linked in) and ffmpeg, Python, VapourSynth, MoltenVK and the libraries they use, as Homebrew
builds them, each under its own license (GPL, LGPL, PSF, Apache, MIT, BSD and others). The app's
`Contents/Resources/ThirdParty/` has each one's license files (and for Homebrew's, the formula it was built by), and
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

The NBCU LUTs (`luts/3-NBCU_HLG2SDR_DL_v1.2.cube`, `luts/5-NBCU_PQ2SDR_DL_v1.2.cube`, and the shaders made from
them, `nbcu_hlg2sdr.glsl` and `nbcu_pq2sdr.glsl`) were developed by NBCUniversal Media, LLC, with Cromorama and Nick
Shaw, and are from NBCU's
[UHD HDR-SDR Single-Master Production Workflow Recommendation](https://github.com/digitaltvguy/NBCUniversal-UHD-HDR-SDR-Single-Master-Production-Workflow-Recommendation-LUTs).
NBCU distributes them publicly and freely, without warranty, on an "as is" basis, to encourage UHD single-master
HDR-SDR production with consistent media exchange; as NBCU asks, their names keep "NBCU" and their headers are
unaltered.

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
