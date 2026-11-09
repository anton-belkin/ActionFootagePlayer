# The navigation bar and the Playlist tab

Status: agreed with Anton 2026-11-09 (the wheel frame by frame; no ±5 s buttons: a click on the track goes anywhere,
and the arrow keys still seek); built on branch `claude/nav-bar-playlist`.

Why: recording your view of a 360° clip in takes (docs/360-reframe.md) means going back to a moment precisely,
again and again: re-recording a part, punching in. Today's bar is mpv's own on-screen controller (osc.lua). It's fine
over the video alone; with the side panel open, the panel turns it into osc.lua's small "box" at half size, centered
over what's left of the video (`place_osc` in side-panel/main.lua): small, and awkward to aim at. Its title line
("[3/12] DJI_…") opens mpv's own playlist, which reads oddly next to the panel.

## What changes

1. **Our own bar** in place of mpv's: drawn by the side-panel plugin (`side-panel/bar.lua`, its reckoning in
   `side-panel/timeline.lua`) in the panel's style and colors. The app sets `osc=no`; an mpv the plugin is put into
   gets its OSC hidden (`osc-visibility=never`).
2. **A Playlist tab** on the rail, second after Project.
3. **No playlist button**: the bar has no title line, no `[n/N]`, no ☰ menu.

## The bar

```
 ┌──────────────────────────────────────────────────────────────────────────────────────┐
 │        ◆                     ◆        A┌───────◆─────┐B          ◆                    │  marks above the track
 │ ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━●───────┴──────────────────────────────  │  the track
 │          ▔▔▔▔▔▔                                                                       │  a moment below it
 │ ⏮  ▶  ⏭    ‹ 0:31.24 / 0:44 ›                          ◂◆  ◆+  ◆▸   [A–B] [1×] ⟲ 🔊 ⛶  │
 └──────────────────────────────────────────────────────────────────────────────────────┘
```

66 points high: the track's row, then the buttons' row.

**The track**, the full width of the video area:
- played part blue (the panel's accent), what's buffered light grey;
- **the camera's highlights** amber diamonds, **your marks (M)** white diamonds, over the track;
- **the A–B part (l)** a light band behind the track, with A and B under its ends;
- **moments** of the project a grey band under the track; the file's own chapters thin ticks.
- **Hover**: a thin line and a bubble with the time there, in hundredths (`0:33.50`). Within 7 points of a mark, a
  moment's end or the part's end, it snaps to it and the bubble names it (`0:33.50  Turtle`).
- **Click**: the exact frame (`seek … absolute+exact`); on a mark, exactly on the mark. **Drag**: keyframes while it
  moves (fast on 4K HEVC), the exact frame where it's let go.
- **Wheel or a horizontal swipe over the bar**: frame by frame (up/right a frame on, down/left a frame back).

**The buttons**, left: previous clip, play/pause, next clip; then a frame back ‹, the time
(click: the time left), a frame on ›. Right: previous mark, mark here (M; on a mark: remove it), next mark; A–B (l:
start, end, clear; blue while set); speed (1×, 1.5×, 2×, 0.5×); loop the clip; sound on/off; fullscreen. Hovered, a
button says what it does and its key.

**Narrow** (a small window with the panel open): what goes first: speed, loop, sound; previous/next clip;
the frame steps; the mark buttons; A–B. Play, the time and fullscreen always stay.

## With and without the panel

- **Panel closed** (Tab, or only the rail): the bar is over the video, its background the panel's color at about 80 %.
  It shows when the mouse moves over the window and hides 1.5 s after it stops (not while the mouse is over it, nor
  while the track is held); a seek with the keys shows it for 1.5 s.
- **Panel open**: the bar is **docked below the video**, always there, opaque, the panel's color: the video moves up
  for it (`video-margin-ratio-bottom`) as it moves left for the panel. Panel and bar frame the video as one.
  The bar spans the video's width, never under the panel.

## The Playlist tab

The files mpv plays one after the other: the videos in the clip's folder (`autocreate-playlist=filter`, the same
list the Source tab's "In the folder" and "Autoplay next" go through). Only the videos (mpv's `video-exts`), not the
sub-folders and other files mpv also puts in it.

- the folder as a line (`… › OsmoAction6-4E8C › 20261021`), and how many videos;
- one row per video, its name without the extension, cut in the middle when it's long (`DJI_2026102116…0023_D`:
  the ends tell DJI's names apart); the one playing highlighted with ▶;
- click: that video plays (`playlist-play-index`);
- it opens scrolled to the one playing; F8 (mpv's playlist as text) opens it.

Nothing else: no editing, reordering or removing.

## Other

- The agent plugin's notes hide the bar while they're written, as they hid the OSC.
- What goes with mpv's OSC: its audio and subtitle track buttons (an action camera's clip has one audio track; both
  are in the right-click menu), its volume slider (the sound button mutes; 9 and 0 still change the volume), its
  window controls for a borderless window (the app's has a border).
- mpv's own OSD bar for keyboard seeks and the volume stays as it is.
