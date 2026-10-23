# ActionFootagePlayer

**The open-source desktop player for footage from action cameras and drones.** Copy the clips off the card,
double-click one, and it plays the way it should look: log footage in color and HDR, shaky clips stabilized,
the camera's telemetry on screen. No import, no project, no rendering, and the original files are never
touched. When something is worth sharing, one key exports exactly what you see.

Every camera maker ships its own desktop app for this. ActionFootagePlayer is the open alternative: free, one
player for the cameras you have, no account or cloud, and built on open tools ([mpv](https://mpv.io) for
playback, [Gyroflow](https://gyroflow.xyz) for stabilization), so it keeps working with your own setup
instead of replacing it.

## What it does

### 1. Log footage that looks right straight away

Shooting in a log profile keeps more of the scene, but the clips come out flat and grey, and most players
show them that way. ActionFootagePlayer reads the color profile the camera wrote into the file and converts it
on the GPU while it plays:

- **in HDR**, on screens that have it (on an SDR screen, the system tones it down), or
- **through the maker's own Rec.709 LUT**, the look the camera's app would give you.

Nothing to set up and no LUT to hunt for: a log clip and a normal one in the same folder each play right.

Drones from the same makers shoot log too (DJI's Mini 4 Pro, Air 3 and Avata 2 in D-Log M, the larger ones
in D-Log as well), and they're next: aerial footage is where log flattens the picture most, with sky and
ground in one frame.

### 2. Stabilization in realtime, under water too

Clips shot with the camera's stabilization off carry the camera's gyro data. ActionFootagePlayer renders the
stabilization from it while the clip plays, with Gyroflow's own engine, so a 4K60 clip plays stabilized
without exporting anything first.

- **Underwater, out of the box.** In-camera stabilization works well in air but not under water, where
  water bends the light and the lens behaves like a longer one. The default preset corrects for that
  (refraction 1.33), so underwater clips play steady with no project to make.
- **Gyroflow, both ways.** A clip with a Gyroflow project plays the way the project says (smoothing, zoom,
  lens, horizon). One key opens the clip in Gyroflow to fine-tune it, and saving there updates playback at
  the same spot. The project can be stored inside the video, so there's one file to carry.
- **FPV drones.** Drones without a gimbal, like DJI's Avata, record gyro data the same way, and Gyroflow
  supports them: their clips will play stabilized like the camera's. Drones with a gimbal are steady already.
- **No black edges.** Where the stabilized frame doesn't cover the window, the gap is filled from earlier
  frames, blurred into the picture.

### 3. The camera's telemetry over the video

Depth under water, whether the camera is in the water, pressure, GPS and speed when the camera has a fix,
exposure settings: what the camera recorded frame by frame, shown as the clip plays, read from the file
itself.

For drones that's the flight: height, position and the speed worked out from it, which DJI drones record
in the file, and in a subtitle file (`.SRT`) next to it when video subtitles are on.

### 4. Export what you see

`Shift+E` exports the clip as it plays, stabilized and color-converted in one pass: 10-bit HEVC in HDR,
which iPhone, Mac and YouTube show as HDR. Mark a part first (`l` twice) and only that part is exported.
Playback goes on meanwhile; the export runs in the background and catches up whenever you pause. For
editing, ProRes; for many clips at once, the same from Finder or a terminal.

## Use case: back from the dive

You shot the dive in D-Log M for the color, with the camera's stabilization off, because under water it
doesn't hold.

1. On the boat or in the hotel, copy the card to the laptop and double-click the first clip.
2. It plays stabilized, in HDR, with the depth in the corner. No import, no waiting for a render.
3. You skip through the hour of footage and find the 15 seconds with the turtle.
4. `l` at the start, `l` at the end, `Shift+E`, and pause for half a minute (on an M1 Pro, 15 seconds of
   4K60 export in about 30).
5. The fragment is in a `stabilized/` folder next to the clip, ready to AirDrop to the group or post the
   same evening, in HDR.

The original clips stay as they were for proper editing later; the project, if you made one, travels inside
them.

## Cameras and systems

| | Today | On the way |
|---|---|---|
| **Systems** | macOS on Apple Silicon | Linux and Windows |
| **DJI Osmo Action 6** | log (D-Log M), stabilization, telemetry | |
| **DJI Osmo 360** | log, stabilization, telemetry, from DJI's published layout; not yet checked on real clips | |
| **Other DJI cameras** | log recognized by file name | stabilization, telemetry |
| **DJI drones** (Mini 4 Pro, Air 3, Avata 2, ...) | stabilization of clips with a Gyroflow project | log (D-Log M, D-Log), flight telemetry, stabilization of FPV drones from their gyro data |
| **GoPro, Insta360** | stabilization of clips with a Gyroflow project | stabilization from the camera's own gyro data; GP-Log and I-Log |

New cameras and drones need clips from their owners: a few seconds each, straight from the camera. See
[Other cameras, and how to help](../README.md#other-cameras-and-how-to-help).

## Open source

GPL-3.0, like Gyroflow, which it builds on. Under the hood it's a set of mpv plugins and a small app that
carries them, so the parts also work on their own in an mpv you already use. Install, keys and how it works:
[README](../README.md).
