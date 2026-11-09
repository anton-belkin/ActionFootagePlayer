# The player on a phone or an iPad: starting on the trip

How the same work (watching footage stabilized and in color, marks, moments, photos, sorting) could start on a trip
with a phone or an iPad, with no laptop, and go on at home on the computer. This is an idea written down, not a
design decided, and nothing is built.

Written 2026-11-09 from Anton's description. It builds on [Library management](library-management.md) (branch
`claude/library-management-design`): a project's locations, the clip's own file beside it, *Take in*, *From a laptop to
the NAS*, afp-stream. And on [The render on the GPU](gpu-render.md): only the GPU warp can run on a phone. What has to
be checked first is marked **(to try)**.

## The idea

In Anton's words, roughly: the mobile Action Footage Player carries on the same logic. You watch your footage stable
and in color, put marks, sort, pick moments, and you can start doing that on the trip, from the phone. When you're
home in front of the computer, it all moves to the computer and you carry on there. It lets the work start earlier,
and a small trip needs no heavy laptop.

So:

- **The same project, the same files.** `Project.afproject`, `<clip>.afp.json`, `_Moments`, `_Stills`: nothing on the
  phone is in a format of its own. Moving to the computer is a *Take in*, as from a laptop, not a conversion.
- **One way only: phone → computer.** No sync back, in any form. Once handed over, the phone's project is done.
- **Working on the library from the phone** is a different thing: the phone opens the library on the NAS itself, over
  Tailscale ([below](#the-library-from-the-phone-over-tailscale)). Nothing is copied, so nothing has to sync.
- **What the phone adds stays on the phone**: its caches (motion data, previews), as a computer's do
  ([What one computer adds](library-management.md#what-one-computer-adds)).

## What it does, what it doesn't

Does:

- Plays a clip stabilized, with its log curve converted, in HDR where the screen has it: the GPU warp, with the look
  and the edge fill as passes of the same render.
- Marks, moments, clip previews, photos at marks (with the gain map), the clip's input and own view: the same as the
  player's side panel, laid out for a touch screen.
- Sorts and finds clips in the trip's project: folders, filters, the Library's browsing at a small size.
- **Shares from the trip**: a moment rendered on the phone (GPU warp, hardware HEVC encode) or a photo straight into
  Photos / the gallery, or a messenger. Something the laptop way never gave.

Doesn't (in this idea):

- VapourSynth: not on phones. A clip the GPU path can't play stabilized plays plain.
- The Library's management: locations, *Take in*, *Find moved files*, renders kept, Apple Photos albums. That's the
  computer's.
- Any sync.

## Where the footage and the project are

Three ways, chosen per source (a DJI card one way, a GoPro another), not for the whole app.

### A. All on the card

The project is at the **root of the card**: `Project.afproject`, `_Moments`, `_Stills`, … next to `DCIM`, never
inside it. A camera keeps to `DCIM` (and its own folders like `MISC`), so it doesn't touch the project. The
location is `.`, a project next to its footage, as a trip project on a laptop is.

At home the card is the laptop folder: copy it into the archive, *Take in*
([From a laptop to the NAS](library-management.md#from-a-laptop-to-the-nas)).

The clip's own file is the open point. The rule is "beside the clip", which here is inside `DCIM`. Two ways:

- **Beside the clip, as everywhere** (preferred), if the cameras don't mind a `0068_D.afp.json` next to their clips:
  **(to try)** on the Action 6, an Insta360, a GoPro (format check, playback on the camera, the camera's app listing).
- **At the project, until home** (if a camera minds): the card location gets a flag, *clip files at the project*.
  Then they're kept at the project's root in the same tree (`_Clip files/DCIM/DJI_001/0068_D.afp.json`), and *Take
  in* puts each beside its clip. The library's "one per clip" holds: until the take-in, only that one exists.

### B. The project on the phone, the footage on the card

The project is in the app's own folder; the card (or a camera seen as a disk) is its location. Nothing is copied.
For a card that can't be written, a GoPro over MTP, or a camera that minds files on its card.

- The card is a **removable volume**, a kind of location the library doesn't have: known by its volume name and ID,
  phone-only. *Take in* never keeps it ([below](#handing-over-to-the-computer)).
- The clips' own files are kept at the project, as in A's second way.
- Watching needs the card plugged in. Marks, moments' previews and photos are there without it.

### C. The clips copied onto the phone

*Import* from the card or the camera into the app's folder, checked by its hash after the copy. People without a
laptop already do this through the camera's app (DJI Mimo, GoPro Quik); an iPad or an iPhone with 1 TB holds a trip.
Sources:

- **A card or a camera seen as a disk** (USB): plain copy.
- **A camera over MTP/PTP** (GoPro by USB): copied through the system's camera API (ImageCaptureCore on iOS,
  `MtpDevice` on Android). Read only.
- **GoPro over Wi-Fi**: the Open GoPro API lists and downloads its media.
- **Photos / the gallery**: clips the camera's app already put there. **(to try)**: whether Photos keeps the motion
  data track (DJI `djmd`, GoPro GPMF) of a clip saved by Mimo or Quik; without it there's no stabilization.
- **A lighter import (idea)**: DJI writes a low-resolution copy beside each clip (`.LRF`: the same D-Log M, H.264
  720p). The LRF plus the clip's motion data is 1–2 % of the clip: enough to watch stabilized, mark, make moments
  and photos at preview quality, while the originals stay on the card or reach the NAS by themselves. **(to try)**:
  whether the LRF carries the motion data, or it has to be read from the original once.

The copies live as A's project: the project at the root of the app's folder, the clips under it, their own files
beside them.

### Where the app's folder is

- **iOS / iPadOS**: the app's Documents, shown in the Files app ("On My iPhone → Action Footage Player"), so it can be
  copied by cable from Finder, AirDropped, put on a disk.
- **Android**: a folder the user picks once (the Storage Access Framework), e.g. `Movies/Action Footage`, seen by the
  computer over USB like any other.

Sizes: an Action 6 8K50 clip is about 1 GB a minute; a day of diving, 30–60 GB.

## Handing over to the computer

The descriptions (marks, moments, photos, the project) weigh kilobytes; the clips weigh tens of gigabytes and often
reach home on their own: the Action 6 uploads to the NAS inbox by itself as soon as it's on power, a card gets copied.
So the descriptions travel apart from the clips, and the computer finds the clips by their fingerprint (`clip.hash`,
which links and the clip's own file already carry).

| The phone's project | What travels | At home |
|---|---|---|
| A, on the card | the card | copy the card's folder into the archive, *Take in*: as from a laptop |
| B, footage on the card | the project only | *Take in*: the card location is dropped, each clip found in the library by its fingerprint |
| C, clips copied | the project, with or without the clips | with them: as from a laptop; without: as B |

*Take in* for B (and C without clips) needs one thing the library design doesn't have yet: **a location that is gone,
whose clips are looked for by fingerprint** in the project's locations, the way *Find moved files* does. Each found
clip gets its own file put beside it (one already there: the "two files" rule, listed before it starts). Clips not
found (still only on the card, not copied yet) are listed; their moments wait as moments with a missing original, and
*Find moved files* finds them later.

How the project gets to the computer, from the plainest:

1. **The card itself** (A).
2. **The app's folder** in Finder (cable), AirDrop, iCloud Drive, a USB disk.
3. **Send to the NAS**: one button, the project (optionally its copied clips) into a share folder for them
   (`video/From the phone/<trip>/`), over the home network or Tailscale. The Library on the computer shows what's
   waiting there and offers *Take in*.

After the handover the phone marks the project **handed over**: it still opens, read only, and offers to delete the
copied clips once the computer has them.

## The library from the phone, over Tailscale

To work on the library itself from a phone or an iPad (at home on the sofa, or away), the app opens the library on
the NAS over Tailscale. It copies nothing, so there's nothing to sync.

- **Watching**: 8K originals over a remote link are too much (150+ Mbit/s). afp-stream already renders a clip
  stabilized as 1080p HEVC (10–30 Mbit/s), with the clip's own settings: the phone plays that stream. At home, on
  Wi-Fi, an iPad could also play the originals from the share with its own GPU warp.
- **Writing**: marks, moments, photos and the clip's own files go straight into the library's files over SMB, as the
  computer writes them, with the same rules (one own file per clip, `_Moments` in the project).
- **Missing**: afp-stream has no password (home network only): over Tailscale only the tailnet reaches it, which is
  the boundary; a Tailscale address of the NAS has to match as one of its names in the project's network paths
  ([Where a location is](library-management.md#where-a-location-is)). afp-stream needs the archive mounted (already
  in the library design, §6).

## The engine

Only the GPU path ([The render on the GPU](gpu-render.md)): it plays 4K60 → 1080p stabilized on the NAS's N100, so a
recent phone is expected to manage. The motion side (reading the motion data, smoothing, the per-frame table) is Rust
(vs-gyroflow, telemetry-parser) and builds for iOS and Android; the pixel side is the same shaders.

- **iOS / iPadOS**: libmpv with gpu-next over MoltenVK (as the Mac app's Vulkan path), VideoToolbox decode, an EDR
  layer for HDR. **(to try)**: 8K HEVC decode and the warp at 60 fps on an iPhone Pro and an iPad M-series; HDR
  presentation through MoltenVK on iOS.
- **Android**: libmpv (as mpv-android) with gpu-next on Vulkan, MediaCodec decode. **(to try)**: HDR output, which
  devices decode 8K.
- **Exports and moments on the phone**: the same render into a file, encoded by the hardware (VideoToolbox,
  MediaCodec).

## To try first

1. Our files beside the clips on a camera's card (Action 6, Insta360, GoPro): format, playback, the camera's app.
2. Photos / the gallery keeping the motion data track of clips saved by Mimo and Quik.
3. Motion data in DJI's LRF.
4. The GPU warp on an iPad (M-series) and an iPhone Pro: 8K decode, 60 fps, HDR.
5. Reading and writing a card through the Files picker (iOS) and SAF (Android): speed, the bookmark kept between plug-ins.

## Not in this idea

- Sync in either direction beyond the one handover.
- The Library's management commands on the phone.
- Editing (cutting several clips into one video).
