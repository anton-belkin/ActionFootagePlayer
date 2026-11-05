# Cutting a spoiled part out of a clip, losslessly

The footage is never changed, but for one case: a clip spoiled in part, the record button pressed by accident
(the camera in a pocket for a minute after the dive). The whole of a spoiled clip goes to the Trash; a clip spoiled
in part keeps its good part, **without re-encoding and with everything the camera recorded in it**, and reads in
the player exactly as the original did.

Written 2026-11-05, from tests on Osmo Action 6 clips (0068, 0069, their LRFs, a 4K clip on the NAS). Step 1
(`afp-cut`, below) is built; step 2 (the player's side) is a plan.

## What goes wrong with the usual tools

- **ffmpeg** (`-c copy -map 0`) can't write DJI's metadata track (`djmd`) at all: without it there's no motion
  data, and no stabilization.
- **MP4Box** (`-splitx`) keeps every track and moves the timecode on, but the cut then reads as another camera:
  - DJI writes the clip's own data (product "DJI OsmoAction6", the proto it's in, serial, firmware, the sensor's
    readout time, focal length, distortion, IMU rate, the color mode) **only in the first sample** of `djmd` (and
    of `dbgi`). Cut the start off, and it's gone: Gyroflow then sees a "DJI" with no lens profile, its motion data
    comes out different, and the player says "not stabilizing: in-camera stabilization state unknown" and "not log
    footage it knows" (no stabilization, no D-Log M conversion).
  - it also drops the in-band VPS/SPS/PPS from the keyframes (`--xps_inband` keeps them but makes the track
    `hev1`, which Apple doesn't play), writes itself in as the encoder, and sets the creation time to now.

## How it's done: in layers

1. **The samples as they are.** Every sample of every track (video, audio, the camera's metadata, the timecode,
   the cover) is copied byte for byte. Only the tables that say where they are and how long they last are
   written anew (`stts`, `stss`, `stsz`, `stsc`, `stco`/`co64`, `sdtp`), with the durations and edit lists.
   Every other box (`udta`, the camera's own boxes, `free`, `uuid`) is copied as it is. The start goes back to the
   keyframe at or before it (every 1.001 s on the Action 6); the end can be any frame, the cameras handled having
   no B-frames.
2. **For every camera:** the timecode sample moves on by the frames cut off; the audio is kept from the packet that
   holds the first frame's moment and lined up with an edit list; the creation times (`mvhd`, `tkhd`, `mdhd`, and
   the file's own dates) move on by the time cut off.
3. **An adapter per camera** puts back, the camera's own way, what it keeps for the whole clip in the part cut off,
   so the cut reads as the camera wrote it, in the player and in other tools:
   - **DJI** (a `djmd` track): the top-level protobuf fields of the clip's first `djmd`/`dbgi` sample that the new
     first sample lacks (ClipMeta, StreamMeta) are put in front of it. Protobuf readers merge them. And the
     camera's highlights (the quick switch: a top-level `free` box with `hglg`, times in ms from the clip's start)
     are moved to the cut's time; the ones in the part cut off are dropped. Copied as they were, they'd be off by
     the time cut off (0093: 26.376 s became 7.357 s of a cut from 19.019 s, on the same frame).
   - **GoPro, Insta360, others:** not yet. GoPro's GPMF comes in self-contained 1 s payloads and its settings are
     in `udta`, so it may need nothing; Insta360 keeps the clip's motion data in a trailer after the boxes, which
     isn't cut (an error) until it has an adapter.
4. **Our key**, `ch.actionfootageplayer.cut` (`moov/meta`, mdta, where `ch.actionfootageplayer.link` is), for
   what the camera's way can't carry, and so the player knows where a cut comes from:
   ```json
   {"version":1, "source":"DJI_..._0068_D.MP4", "first_frame":540, "frames":659, "source_frames":2852,
    "start":9.009, "end":20.003333, "source_duration":47.580867, "adapter":"DJI",
    "carried":[{"track":"djmd","fields":[1,2],"bytes":212}, {"track":"dbgi","fields":[1],"bytes":575}],
    "highlights":{"kept":1,"dropped":1},
    "origin":"DJI_..._0068_D.MP4", "origin_start":9.009,
    "read_as":{"stabilization":"ok","log_curve":"dlogm","camera":"DJI OsmoAction6","lens":"in the file"},
    "tool":"afp-cut 0.2.0"}
   ```
   `origin`/`origin_start`: a cut of a cut still says where it is in the first original (for marks and moments).
   `read_as`: how the player read the original, for a camera whose own way can't put it back.
   The camera's own mdta keys stay as they were.
5. **The check.** The cut is read back before it's kept: every sample against the original's (the replaced ones:
   what they must hold), and the cut read as the player reads the original: gyroflow-probe's verdict, the log
   curve, the camera, the lens, DJI's highlights, and the motion data through telemetry-parser (the cut's readings must be a run of
   the original's, the same values at the same moments). A cut that doesn't read the same isn't kept. A camera
   without an adapter is cut only when it passes, and the outcome says so: what else such a camera keeps isn't
   known.

## afp-cut (step 1)

```
afp-cut <video> --from <time> [--to <time>] [--out <file>] [--lrf] [--json]
```

Writes `<video>_<from>-<to>s.<ext>` next to the video (or `--out`, never the video itself), hidden as
`.<name>.part.<ext>` while it's made and checked. `--lrf` cuts DJI's proxy the same (to the main cut's moments;
its samples are checked, telemetry-parser doesn't read `.LRF`). Exit status 0: cut and checked; 1: an error; 2:
not a file it can cut (fragmented, a trailer, B-frames, several sample descriptions); 3: cut but it doesn't read the
same, removed. The code: `vs-gyroflow/src/cut.rs` (the cut, the DJI adapter, verify), `probe.rs`
(`camera_and_lens`), `mp4meta.rs` (`merged_meta`).

Checked on the Action 6 (0068, 0069): cuts of the start, the end, the middle, both sides of a removed middle, odd
times, the whole clip, one second, a cut of a cut, the LRF. Every sample identical (the first `djmd`/`dbgi` with
the header in front), 100 % of Gyroflow's quaternions identical, the timecode moved by the frames cut off,
ffmpeg decodes without an error, AVFoundation opens and decodes, the player stabilizes and converts D-Log M.

The stabilized picture of a cut isn't the original's at the same frames, and needn't be: the smoothing has no
frames before the cut's start (or after its end), and Gyroflow's static zoom (`adaptive_zoom_window` -1) is worked
out over the clip it has. In the middle of a cut the player's render is the original's within 46 dB; near its
ends it's another clip's start. The same render twice is identical, the motion data is.

## In the player (step 2, planned)

- Mark the spoiled part (two points, or "from here to the end" / "from the start to here"), then cut. A part in
  the middle makes two clips (`_a`, `_b`): joining them would make the timecode and the motion data lie about the
  seam.
- The cut takes the original's place: the original goes to the Trash (recoverable) only once the cut is checked;
  the cut gets the original's name (a middle: the two parts `<name>_a`, `<name>_b`).
- What goes with the clip: the LRF cut the same; `<clip>.marks.json` moved by the time cut off (marks in the part
  cut off dropped); moments and clip previews relinked (their link's time moved, or dropped with the part); the
  Gyroflow project (`.gyroflow`) made again, never edited.
- The player reads `read_as` from the key for what the file itself no longer tells (a camera without an adapter).
- A camera without an adapter: cut only when the check passes, with a warning.
