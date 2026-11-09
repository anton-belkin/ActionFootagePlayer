# 360°: a flat video by looking around

Design, not built. Written 2026-11-09. Follows [the 360 architecture](360-cameras.md) and its plan's step 7, which
already exports a flat view along keyframes of the head (`gyroflow-export --reframe`).

## In short

- **Recording the view.** In the side panel's 360° tab, **Record** saves where you look while the clip plays: the
  head's turn, tilt, roll and field of view, frame by frame. Recording again over a part replaces just that part.
- **Tidying the recording.** Three checkboxes, all undoable: **smooth the turns** (on the angles), **level the
  horizon**, and **hold on what's in the middle**, which smooths by the picture: the motion between frames, measured in
  the view itself. **Play the recording** shows the result before anything is exported.
- **Exporting.** **Export flat** in the same tab renders the recorded view into a normal video. The Output tab gets one
  **Export** button with a list beside it: *As it plays (HDR)*, *In SDR*, *360° video* and *Flat, the recording*. The
  360° video and the flat recording are there only for a 360° clip.

How others do it, and why recording comes first here, is in [What others do](#what-others-do).

## What others do

Every maker's app and every editor that reframes 360° keeps the two exports apart:
- a 360° video (equirect with spherical metadata, stabilized, the horizon level), for any 360 player;
- a flat video, where someone decides where the picture looks at every moment.

That decision is made in one of six ways, usually one way per part of the clip:

| Way | Who has it |
|---|---|
| Keyframes with easing per segment | Everyone: Insta360 (7 curves), GoPro, DJI, Premiere/FCP/Resolve plugins |
| Looking around while it plays, recorded | Insta360 Quick Edit, GoPro MotionFrame ("smooth" or "original"), DJI GyroFrame |
| Tracking a subject picked with a box | Insta360 Deep Track, GoPro AI tracking, DJI tracking, PowerDirector |
| Locked to a heading | GoPro POV (the GPS course) and its reverse, Selfie |
| Template moves | Insta360 Movement, GoPro CameraFx |
| AI picking highlights | Insta360 AI Frame |

None of them publishes how its paths are smoothed or tracked. The one open implementation, Reframe360 XL, blends
Euler angles through an easing curve.

**Why recording first here.** The player already turns the view at the screen's rate, ~9 ms from a gesture at 120 Hz,
and the heading modes already do what GoPro's POV does. Recording what the head does is a few hundred lines on top of
that. Keyframes and tracking can come later on the same path format (see [Later](#later)).

## Recording

### In the 360° tab

A new part at the bottom of the tab, **Flat video**, under *Follows*:

```
 Flat video                                   Alt+r
 ┌────────────────────────────────────────────────┐
 │  ●  Record                                     │   (while recording:  ■  Stop · 0:23)
 └────────────────────────────────────────────────┘
 Recorded 0:12–0:47 and 1:05–1:31 · 2 parts
 [ Play the recording ]                    ( on )
 ☑ Smooth the turns      [ Light | Medium | Strong ]
 ☑ Level the horizon
 ☐ Hold on what's in the middle (by the picture)
   Measuring… 40 %
 Frame  [ 16:9 | 9:16 | 1:1 ]   Size  [ 1080p | 4K ]
 ┌───────────────────────┐ ┌──────────────────────┐
 │  Export flat          │ │  Export 360°         │
 └───────────────────────┘ └──────────────────────┘
 Undo the last take          Clear the recording
```

Before anything is recorded, the part is just **Record** and a line: "Look around as it plays: what you see becomes a
flat video".

### What recording does

- **Record** (or Alt+r) starts recording at the current position and starts playback if it was paused. Each frame
  shown records the head as it was when that frame was on screen:
  - turn (yaw), tilt (pitch), roll and FOV, in the player's degrees;
  - the projection;
  - the heading mode in use.
- **Stop** (Alt+r again, or the end of the clip) ends the take. So do pausing, seeking and leaving the 360 view: a
  take is a single stretch of playback. Recording resumes with the next Record.
- **Punch-in.** A take replaces only the time it covers. Recording 0:20–0:30 over a recording of 0:12–0:47 keeps
  0:12–0:20 and 0:30–0:47 as they were. Where two parts meet, the smoothing below blends them; with smoothing off,
  it's a cut.
- **Undo the last take** restores what that take replaced; it goes back one take at a time, up to 20.
  **Clear the recording** asks first.
- **Where nothing is recorded**, the view holds the nearest recorded one, as `--reframe` does with keyframes now.
  Before the first part it holds the first frame of it, and after the last part the last.
- **While recording:**
  - a red dot and the time recorded so far appear in the corner of the video;
  - the chosen frame (16:9, 9:16 or 1:1) is drawn over the view as a light outline, the way crop mode draws its frame,
    so what lands in the video is what's inside it;
  - when the video's own area already has the frame's shape, there's no outline.
- **Paused**, the head turns as always but nothing is recorded, since there's no time to record it against.
  Recording while paused could later mean "hold this frame while the view moves"; it isn't part of this design.

### Playing the recording

**Play the recording** makes the view follow the tidied recording instead of your gestures: a preview of the export,
at the screen's rate, with the options applied. It turns on by itself after a take ends, so you see the result.
A drag, a scroll or a pinch turns it off ("Your view: the recording is paused") and the head is yours again, from
where the recording had it. Record starts from there too.

### Where it's kept

In the clip's own file, `<clip>.afp.json`, beside its marks and saved view:

```json
"view360": {
  "recording": {
    "parts": [
      { "from": 12.32, "to": 47.10,
        "samples": [[12.32, 0.0, -4.1, 0.0, 100.0, "auto", "travel"], [12.36, 0.4, -4.1, 0.0, 100.0], ...] }
    ],
    "tidy": { "turns": "medium", "level": true, "picture": false },
    "frame": "16:9", "size": "1080p",
    "undo": [ ... ]
  }
}
```

- **What a sample holds:** `[t, yaw, pitch, roll, fov]`, where `t` is the clip's own time. A sample that also lists a
  projection or a heading mode marks a change; later samples keep it until the next change.
- **Thinning:** samples are thinned as they're written. A sample is dropped where the head moved less than 0.05°
  from a straight line between its neighbours (Ramer-Douglas-Peucker on the angles and the FOV's log). A still head is
  two samples; a 10-minute take of constant looking around is a few hundred kilobytes at most.
- **Undo** keeps what each of the last takes replaced, and the history is cleared when the clip closes.

**Open:** whether a moment made from a recorded part carries that part of the recording (its own `view360` in its
link file). That's natural, but it touches the Library's batches; it's left for after this design.

## Tidying

All of it runs in Rust, in one module (`vs-gyroflow/src/reframe.rs`). The preview and the export use the same code, so
Play the recording shows exactly what Export flat makes.

### Where it works: the view in the world

The head is recorded relative to the heading mode: straight ahead is the GPS course, the camera, north or the world.
What the eye sees is the composition:

```
q_view(t) = R_up(heading_yaw(t)) · R(head(t))       (world axes, gravity down: the view in the world)
```

`heading_yaw(t)` is each frame's yaw for that mode, the one stamped in the frame. It is also what
`sphere360::Motion` computes from the motion data alone, without rendering, as `gyroflow-probe --headings` does already.

Tidying smooths `q_view(t)`, not the head. A wobble of the heading and a wobble of your hand are the same wobble on
screen, and both are taken out together. A switch of heading mode mid-take becomes a smooth pan instead of a jump.
The result goes back into a head relative to the same heading, for the player:
`head'(t) = R_up(heading_yaw(t))⁻¹ · q_view'(t)`.

### Smooth the turns (on the angles)

- **Rotation:** a Gaussian over time on the rotation (on SO(3)). Its width σ is 0.25 s for Light, 0.6 s for Medium
  and 1.5 s for Strong.
  - Each frame's smoothed rotation is the weighted mean of its neighbours' quaternions, made sign-consistent, then
    normalized. Within a window of a few σ the spread is small, so this matches a proper mean on the sphere to well
    under a degree.
  - Near a part's end the window shrinks to what the part has, so the ends stay where they were recorded.
- **FOV:** smoothed the same way, in log(FOV), so a zoom in and a zoom out of the same factor take the same time.
- **Hard moves:** a deliberate fast turn (a whip pan) is smoothed into a slower one. If that turns out to bother,
  the Gaussian becomes adaptive: narrower where the turn is fast and steady, like a one-euro filter. Measure on real
  recordings first.

### Level the horizon

- The view's roll against gravity is set to 0, so the picture is level whatever the hand did. A roll you meant (a
  barrel roll) is lost while this is on, so it's a checkbox and on by default.
- The heading frame is level already, with gravity from the stamp. Leveling sets the head's roll to 0 after the
  composition, before the smoothing.

### Hold on what's in the middle (by the picture)

The angles say where you meant to look. The picture says what was there. Hand jitter, the camera's own residual
motion and parallax all make what's in the middle of the view drift and shake, even with the angles smooth. This
option makes the path follow the content near the middle, then smooths that.

- **Render** the view along the angle-smoothed path, small: 480 px wide, luma only, every frame, on the GPU, with
  `view360.wgsl` as `--reframe` does.
- **Measure** frame to frame how the middle of the view moved. The middle is the central 40 % of the frame. The
  measure is block matching, from `still.rs` (the clean still frame's motion search): a grid of 8×8 blocks over the
  middle, each block's shift, and the median shift with its spread.
  - The shift is turned into an angle at the view's middle: `dx / f`, `dy / f`, where `f` is the projection's focal
    length at the centre.
  - The confidence comes from the spread and from how many blocks matched.
- **Follow:** the measured angles accumulate into where the content went, and the path is pulled towards it.
  - Weight by confidence: a frame with a low confidence (a flash, water, the sky, a fast subject across the middle)
    keeps the angle-smoothed path.
  - Never more than 8° from the recorded path, so it can't wander off what you looked at.
  - Smoothed again with the chosen strength, which takes out the matching noise.
- **Cost:** one small render and one match per frame, 2–4 ms on the M1 Pro estimated from `still.rs`'s timings at this
  size. A 10-minute clip at 25 fps is about a minute, run in the background with its progress in the panel
  ("Measuring… 40 %"). The result is cached beside the clip's temp files, keyed by the recording and the options, so
  turning the option off and on again is instant.
- **What it isn't:** subject tracking. It holds on whatever dominates the middle, mostly the scene. A rider crossing
  the middle fast is outvoted by the background or lowers the confidence, so the path is left alone there. Tracking a
  chosen subject is a [later](#later) way of authoring, which can reuse this measurement inside a box.

### When it runs

- **Smooth the turns** and **Level** are instant even on a long recording: a few thousand quaternions. The player
  runs them through `gyroflow-probe --reframe <clip>` after each take and each change of option, which reads the
  recording and the motion and prints the tidied head per frame. The view360 script plays that back.
- **Hold on what's in the middle** renders, so it runs in the background and the panel shows its progress. Play the
  recording uses the angle-smoothed path until the measurement is done.

## Exporting

### Export flat (the 360° tab)

```
gyroflow-export --reframe recording --size 1920x1080 <clip>
```

`--reframe recording` takes the recording and its tidying options from the clip's `.afp.json`, so the export and the
preview are the same code and the same file. The `"t:yaw,…;…"` keyframes form stays for scripts.

- **Frame and size:** from the tab. 16:9, 9:16 and 1:1, at 1080p or 4K: 1920×1080, 1080×1920, 1080×1080 or
  3840×2160, 2160×3840, 2160×2160.
- **FOV:** the recorded one, measured across the picture's width as in the player. For 9:16 that is the width of a
  standing frame, so a recording made in a landscape window keeps its horizontal FOV and loses the top and bottom,
  as the outline showed while recording.
- **Part:** the recorded parts' span, from the first sample to the last. When the Output tab's start and end marks are
  set (`ab-loop`), the part between them.
- **Look:** the one the clip plays in: HDR as recorded or the SDR look. The same as the Output tab's *As it plays*
  and *In SDR*.
- **Projection:** per sample, as recorded. A change mid-take is a cut on that frame; a crossfade can come later.
- **Name:** `<name>_reframe.mp4`, as now. A named export (Shift+E) goes into `_Exports` with a link back, as other
  exports do.

**Export 360°** beside it is `gyroflow-export --360`: the whole sphere, stabilized, turned into the world and the
heading, with spherical metadata. The recording plays no part in it, as in every other app.

### One Export button in the Output tab

Today Output has *Export the clip* (HDR, as it plays) and *Export in SDR* as two buttons. They become one, with a
list:

```
 Export
 A file of what plays: its source, its transform, in this look
 [ Mark start ]  [ Mark end ]
 ┌──────────────────────────────────┬────┐
 │  Export the clip · HDR           │ ▾  │
 └──────────────────────────────────┴────┘
                                    ┌─────────────────────────┐
                                    │ ✓ As it plays (HDR)     │
                                    │   In SDR                │
                                    │   360° video            │   (a 360° clip only)
                                    │   Flat, the recording   │   (a 360° clip with a recording)
                                    └─────────────────────────┘
 [ Show the last export ]
```

- **The main part** of the button exports the chosen kind. Its label says which: "Export the clip · HDR", "… · SDR",
  "… · 360°", "… · flat". With a part marked, it's "Export the part 0:12–0:47 · flat".
- **The ▾ part** opens the list under the button, drawn over the panel's content (the panel's first popover). A click
  picks a kind and closes the list; Esc or a click outside closes it without a change. The choice is remembered for the session, per
  kind of clip (360 or flat), so a 360 clip opens with what was last used on one.
- **What's listed:**
  - *In SDR*: where the clip has an SDR look (`sdr_look`), as now.
  - *360° video*: only for a 360° clip.
  - *Flat, the recording*: only when it has a recording. Otherwise its line says "Flat: record a view first" and is
    greyed out.
- **Same jobs as before:** the list sets what the existing `script-message-to gyroflow export <how>` gets.
  `how` becomes `hdr`, `sdr`, `360` or `flat`; `sdr` stays as it is, and nothing passed means `hdr`, as now.
- **The panel's UI** (`side-panel/ui.lua`) gets a split button and a dropdown list: the two halves of the button, and
  a list of rows with a check on the chosen one. The list closes when the panel scrolls or the tab changes.
- **The 360° tab's Export flat and Export 360°** run the same two kinds directly, so a 360° clip can be exported from
  where it's being looked at.

### The export itself

`export.lua`'s job (progress in the panel, Cancel, Show the last export) runs `gyroflow-export` with `--360` or
`--reframe recording` for those two kinds. It's the same subprocess and the same progress lines (`--progress`) as
for flat clips.

## Keys

| Key | What |
|---|---|
| Alt+r | Record / Stop |
| Alt+Shift+r | Play the recording on/off |

Alt+r is free in the player's bindings and mpv's defaults. Both can be rebound as `script-binding view360/record` and
`view360/play-recording`.

## What goes where

| Piece | Where |
|---|---|
| Recording, punch-in, undo, sidecar writes | `mpv/scripts/view360/recording.lua` (plain Lua, tested in `tests/view360_test.lua` with recorded sample streams) |
| The 360° tab's *Flat video* part | `side-panel/main.lua`, `view360_part` |
| Split button and dropdown | `side-panel/ui.lua` |
| Tidying: world composition, SO(3) Gaussian, level, picture hold | `vs-gyroflow/src/reframe.rs`, with Rust tests: a synthetic jittered path comes out smooth, a level path stays level, a held subject's drift is taken out in a rendered synthetic sphere |
| `gyroflow-probe --reframe <clip>` | the tidied head per frame, for the player's preview |
| `gyroflow-export --reframe recording` | reads the sidecar, renders through `export360.rs` |
| Export kinds, `how` = hdr/sdr/360/flat | `gyroflow/export.lua` |

## Steps

1. **Recording and playback, no tidying:** Record, Stop, punch-in, undo and the sidecar; Play the recording with the
   raw samples; Export flat through `--reframe recording`. That's already a usable feature.
2. **Smooth the turns and Level**, in `reframe.rs`, with the preview through `gyroflow-probe --reframe`.
3. **The Output tab's one Export button** with its list, and Export 360° in the 360° tab.
4. **Hold on what's in the middle:** the measurement, its cache and its progress.
5. **The frame outline while recording**, and 9:16 and 1:1.

Each step's live check (recording with real gestures, the preview, the exports played back) needs Anton's OK for
the screen.

## Later

- **Keyframes as a way of authoring:** turning a recording into editable keyframes, or the reverse. They share the
  path format; the keyframes are a thinned recording with an easing per segment.
- **Tracking a subject:** a box drawn on the view, the same block matching kept inside the box from frame to frame.
  It writes a part of the recording, so punch-in and tidying apply to it.
- **The heading as a part:** "look along the way here" written into the recording as a part with the head straight
  ahead. Today you get that by recording without moving.
- **Moments of a 360° clip** carrying their part of the recording.
- **A crossfade** where the projection changes.
- **The latency offset:** the recorded head is the one on screen with that frame. If measurement shows the frame on
  screen lags its `time-pos` by a refresh or two, the samples shift by it.
