# Marks to the editor: design

A trip's moments, marks and choices, taken to Final Cut Pro, DaVinci Resolve, Premiere Pro and Gyroflow as
markers, ranges, bins and ratings on the original clips. A design, not built yet. It covers what to write and
how each editor takes it, how it's started from the player, and how to check it.

What the player keeps, and where, is the other design: [Moments, clips and project folders](moments-and-workspaces.md).
This one reads that model and writes it out.

Written 2026-11-04, and rewritten the same day onto that model. The clips' timecode was checked on this Mac (an
Osmo Action 6, a HERO7 Black). The editors' side comes from their documentation, cited below. What that leaves
open is marked **(to try)**: those are the imports to do by hand before building ([Checking it](#checking-it)).

## In short

- **The project is what's exported**: one trip, one go, from the menu. Its parts are:
  - **Moments** (`Moments/*.mp4`, each a part of a clip): ranges with a name. The subfolders of `Moments/` and the
    files' Finder tags become bins, keyword collections and keywords; a *Favorite* tag makes a favorite.
  - **Clips** (`Clips/*.mp4`, a preview per clip): a clip whose preview was deleted is a reject, a *Favorite*
    tag a favorite, and the folders and tags are bins and keywords again.
  - **Marks** (the clip's marks file, else its camera highlights): markers.
- **Out, one file per editor, in the project folder**:
  - **Final Cut Pro**: FCPXML 1.10. An event of the clips with their markers, the moments as keyword ranges
    (keyword collections per folder and tag) with favorites and rejects, and a project of the moments in a row,
    named.
  - **DaVinci Resolve**: FCPXML too (File → Import → Timeline), for timelines on the original files: the moments,
    one per folder of moments, and the clips. The markers go on as timeline markers from an EDL. The ratings
    and keywords go on as clip colors, flags and keywords from a metadata CSV. That's three kinds of file, all
    taken by the free version. A Resolve script would do it in one, but since 21.1 scripting is Studio's.
  - **Premiere Pro**: FCP7 XML (xmeml): bins as the folders are, the clips with their markers, each moment as a
    named clip with its in and out, and a sequence of the moments. Not an XMP sidecar: for an MP4, Premiere reads
    XMP from inside the file, and the clip isn't written to.
  - **Gyroflow**: only by an explicit command. The moments of a clip go into a project of their own in
    `Exports/` as trims, never into the clip's own project. Also the fork's JSON from #1248 (in/out/marker by
    Unix time).
- **Times by the clip's own timecode.** The brief assumed action cameras' MP4s have no timecode, but these two
  *do* write a `tmcd` track. It's a camera clock, drop-frame on the DJI at 59.94, and the editors place a clip
  at it. So every time written out is the clip's start timecode plus the time in the clip, on a frame. A clip
  without the track starts at 00:00:00:00.
- **Built in plain Lua** like the highlights plugin, with a small MP4 header reader for the timecode, frame rate
  and size, and the moments' link reader from the other design. So it works in any mpv. Tests run with
  `luajit` on small clips that ffmpeg makes with a timecode.

## What it reads

From the project ([the model](moments-and-workspaces.md)):

| | Where it is | What the editors get |
|---|---|---|
| A moment | `Moments/**/<name>.mp4`, its link: clip, `in`, `out` | a range of the clip, named after the file |
| The moment's folders | `Moments/Reef/Moray eel.mp4` → `Reef` | a bin, a keyword collection, a keyword |
| Tags on a moment | Finder tags; Explorer's stars | keywords; *Favorite* (or ★, 4–5 stars) a favorite |
| A clip | the footage under the sources root (not `Moments/`, `Clips/`, `Exports/`) | a clip, at its timecode |
| A clip's rating | `Clips/**/<clip>.mp4`, and `.actionfootageplayer/clips-made.txt` | made and deleted: a reject; tagged *Favorite*: a favorite; never made: no rating |
| A clip's folders and tags | `Clips/Day 1/…`, Finder tags | a bin, keywords |
| Marks | `<clip>.marks.json` in the project, else the clip's highlights | markers; a camera's with its note ("DJI quick switch") |

Nothing more is read from the clips than their header: timecode, frame rate, size, duration and highlights.
The moments' links are in the first 16 KB of each moment file. The player's cache of them (the other design's)
makes it a stat per file.

### The clips' timecode

Checked with `ffprobe` on clips here:

| Clip | Timecode track | Start | Frame rate |
|---|---|---|---|
| Osmo Action 6, `DJI_20261022152841_0068_D.MP4` (47.6 s) | `tmcd`, "TimeCodeHandler" | `20:28:05;12`, drop-frame | 60000/1001 |
| Osmo Action 6, `DJI_20261022152937_0069_D.MP4`, the next recording | the same | `20:29:01;54`: 56 s later, the same gap as between the names | 60000/1001 |
| HERO7 Black, `GX011057.MP4` (169 s) | `tmcd`, "GoPro TCD" | `00:24:35:18`, non-drop | 60000/1001 |

A DJI Mini 4 Pro file has none. The timecode is a camera clock, not the time of day: the DJI's is 5 hours off
the file's name, and the GoPro's clock was never set (`creation_time` 2016-01-01). But it runs on between
recordings. Each editor puts a clip at its file's timecode: FCP reads `tmcd` in MP4 as in MOV (Apple TN2174),
Resolve uses "Embedded in the source clip" by default, and Premiere uses the media's timecode. So:

- **Every time written is the clip's start timecode plus the time in the clip.** A marker written at 45.6 s from
  00:00:00:00 would land 20 hours away, outside the clip. FCP then refuses the clip with "Invalid edit with no
  respective media". Where a format counts frames from the file's first frame (FCP7 XML's `in`/`out`, XMP), the
  timecode is still written so that relinking by media start works.
- **Drop-frame matters**: DJI's is drop-frame at 59.94 (`;`), GoPro's isn't (`:`). FCPXML gives times as
  rational seconds and only needs `tcFormat="DF"`. EDL and FCP7 XML's timecode strings need the drop-frame
  arithmetic.
- **mpv doesn't give it**: it's a stream tag, which mpv doesn't show. The header reader reads the `tmcd` track
  itself. The track's `stsd` gives the drop-frame flag, the timescale and the frame duration; `stco` says where
  the one 4-byte frame number is. It reads them with seeks, as `markers.lua` reads the highlights.
- No track (other models, DJI's LRF proxies, exports): 00:00:00:00, which the editors assume too.

### Split recordings

The brief expected DJI clips cut at 4 GB. The Action 6 here wrote a 5.99 GB clip whole (exFAT has no 4 GB
limit), but GoPro splits into chapters (`GX01xxxx`, `GX02xxxx`), and other cameras or cards may. The player
plays each file on its own, so the marks, the highlights and the moments are per file, and a moment can't cross
from one file into the next. Out, the files of one recording follow each other by name and by timecode, which runs
on, so they sort in order in each editor. A recording as one clip in FCP (a compound clip of its files) or
Resolve waits for the player to play a recording as one: the virtual join in the workflow study.

## The editors and their formats

### Time on a frame

The moments and the marks keep seconds. Each format wants frames, timecode or rational seconds, so all the
conversions are in one module:

- The frame rate is the container's, exactly (`60000/1001`, not 59.94), from the video track's `stts`.
- A mark's frame is `n = floor(t × rate + 0.5)`. A moment's in rounds the same way and its out too, kept at
  least a frame after its in and no later than the clip's last frame.
- FCPXML time = `(start_frame + n) × frame duration`, written as an exact fraction: `(4416688 + 600) × 1001/60000s`
  → `"4421705288/60000s"`. FCP's rule: times "should be a multiple of the frame duration … Otherwise, Final Cut Pro
  inserts a gap" (FCPXML timing attributes).
- A timecode string from a frame count: non-drop is plain division. Drop-frame at 59.94 skips frame numbers 0–3
  of each minute except every tenth; at 29.97 it skips 0–1. The `tmcd` sample holds the frame count; its string
  must come out as ffprobe shows it (`20:28:05;12`), which is the test.

### Final Cut Pro: FCPXML

**How it's imported**: File → Import → XML, double-clicking the file, or `open -a "Final Cut Pro" file.fcpxml`
(the last one is untried). Apple: [Use XML to transfer projects](https://support.apple.com/guide/final-cut-pro/use-xml-to-transfer-projects-verdbd66ae/mac).

**The version**: 1.10, which FCP 10.6 (Oct 2021) introduced, so any FCP since reads it. FCP 11 writes 1.13 and
12 writes 1.14; older versions keep importing (release notes, [support.apple.com/102825](https://support.apple.com/102825)).
A plain `.fcpxml`, not the `.fcpxmld` bundle.

**What's written**, in Apple's terms ([Describing Final Cut Pro items](https://developer.apple.com/documentation/professional-video-applications/describing-final-cut-pro-items-in-fcpxml),
[the DTD](https://developer.apple.com/documentation/professional-video-applications/document-type-definition)):

- `resources`: one `format` per frame rate and size (`frameDuration="1001/60000s" width height`; action
  cameras' 4:3 and 2.7K sizes aren't FCP's predefined formats, so they're described), and one `asset` per
  clip. Each asset has `start` = the timecode as rational seconds, `duration`, and `media-rep
  kind="original-media" src="file:///…"` (an RFC 2396 URL: spaces and non-ASCII percent-encoded). It also has a
  stable `uid` (`ch.actionfootageplayer.<hash of name+size>`), so an import of the same clip later finds the
  same asset.
- `library` → `event name="<project>"` → one `asset-clip` per clip (browser clips, no project needed), with:
  - a `marker` per mark: `value` = its name or "Mark 3", one frame long, `note="DJI quick switch"` or `"GoPro
    HiLight"` for a camera's;
  - a `keyword` per moment, over its range: `value` = its folders and tags, comma-separated, as FCP writes
    several keywords (`"Moments, Reef, Night"`), and `note` = its name. So each moment shows in the event as a
    range, and in the Keyword Collections *Moments*, *Reef* and *Night*;
  - a `rating value="favorite"` over a moment tagged *Favorite*, and over the whole clip when its preview in
    `Clips/` is, `value="reject"` over the whole clip when its preview was deleted. FCP shows them as green and
    red lines, under the browser's Favorites and Hide Rejected filters;
  - in the DTD's order: markers, keywords, ratings, then `metadata`. A file out of order is refused whole.
- the event's `keyword-collection`s, one per keyword, inside a `collection-folder` per folder level of
  `Moments/` (`Reef/Night/` → folder *Reef*, collection *Night*), which is how FCP nests them. The clips'
  folders in `Clips/` are keywords on the whole clip in the same way.
- a `project name="<project> – moments"` in the same event: a `sequence` (`tcFormat` as the clips) whose `spine`
  holds an `asset-clip` per moment, **named after it** (`start` = its in, `duration` = its length), in the
  order of the moments' time. That's a rough cut to start from, with the names showing on the timeline.
- Each moment as a browser clip of its own (an `asset-clip` with the moment's range and name, beside the whole
  clip's) would show the names in the browser too. Whether FCP takes a second `asset-clip` of one asset as a
  clip with that range is **(to try)**, and it would double the clips in the event.
- `import-options`: none. Whether to copy the media or leave it in place is then the user's import setting.
  "Leave files in place" links to the originals, which is what a NAS needs.

Example: the Action 6 clip above, a highlight at 10.01 s and the moment `Moments/Reef/Turtle.mp4` from 20.02 s to
30.03 s, tagged *Favorite*:

```xml
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE fcpxml>
<fcpxml version="1.10">
  <resources>
    <format id="r1" frameDuration="1001/60000s" width="3840" height="2160"/>
    <asset id="r2" name="DJI_20261022152841_0068_D" uid="ch.actionfootageplayer.3f9a1c2b5e"
           start="4421104688/60000s" duration="2854852/60000s" tcFormat="DF" format="r1"
           hasVideo="1" hasAudio="1" audioSources="1" audioChannels="2" audioRate="48000">
      <media-rep kind="original-media" src="file:///Volumes/DJI_Inbox/2026-10-22/DJI_20261022152841_0068_D.MP4"/>
    </asset>
  </resources>
  <library>
    <event name="2026-10 Egypt">
      <asset-clip name="DJI_20261022152841_0068_D" ref="r2" format="r1" tcFormat="DF"
                  start="4421104688/60000s" duration="2854852/60000s">
        <marker start="4421705288/60000s" duration="1001/60000s" value="Mark 1" note="DJI quick switch"/>
        <keyword start="4422305888/60000s" duration="600600/60000s" value="Moments, Reef" note="Turtle"/>
        <rating start="4422305888/60000s" duration="600600/60000s" value="favorite"/>
      </asset-clip>
      <keyword-collection name="Moments"/>
      <collection-folder name="Moments">
        <keyword-collection name="Reef"/>
      </collection-folder>
    </event>
  </library>
</fcpxml>
```

(`4421104688` = 4 416 688 frames × 1001, which is `20:28:05;12` at 59.94 drop-frame. The clip is 2852 frames
long; 10.01 s is frame 600, 20.02–30.03 s frames 1200–1800. Whether a keyword collection may sit both at the top
and as a folder's name, or the folder is better called *Moments folders*, is part of the DTD check.)

**Importing again**: clips go into the event of that name if there is one. For a clip already in it, FCP asks
whether to replace it or keep both, once for the whole import. It doesn't merge. So the export is one go per
trip, after the selection, and the menu says so. A second import with *Replace* gives the newer moments.
(to try: whether Replace keeps what was done to the clip in FCP since.)

**(to try)** FCP isn't installed on this Mac: [Checking it](#checking-it).

### DaVinci Resolve

The free version is the one that matters (it's what the workflow study found people use), and it can't do
everything Studio can:

- **No script in the free version.** Its own README says the scripting API can return False "when the function
  references a Studio function from the free DaVinci Resolve version". External scripts never worked in the
  free version, and Resolve 21.1's notes say "Advanced scripting now requires DaVinci Resolve Studio". This Mac
  has 21.0 (free).
- **No file format puts markers on clips in the media pool.** The Reference Manual documents only one marker
  import, *Timeline Markers from EDL*, which puts them on a timeline. FCPXML import isn't documented to keep
  `marker`, `keyword` or `rating`, and a forum answer says keywords and favorites don't come across (to try).

So, three files and a timeline to carry what the clips can't:

1. **`<project> (Resolve).fcpxml`**, imported with File → Import → Timeline… (Shift+Cmd+I) and *Automatically
   import source clips into media pool*. Resolve finds the media by the file paths, then the clip names, then
   timecode (Reference Manual, Conform). It's the FCP file without the event's browser clips, as projects, which
   Resolve imports as timelines on the original files:
   - **"<project> – moments"**: every moment in a row, named, as for FCP;
   - **"<project> – <folder>"**: one per folder of `Moments/` with moments in it (`Reef`, `Reef / Night`). A
     timeline is the nearest thing to a bin of parts that a file can give Resolve;
   - **"<project> – clips"**: every clip that isn't a reject, whole and in timecode order. This is where the
     marks go, since a timeline is the one place Resolve takes markers from a file.

   With the FCPXML's markers on the timeline clips, step 2 isn't needed, if Resolve keeps them (to try).
   Whether Resolve can take several projects from one file, or only one timeline per import, is to try too. If
   it takes one, these are separate files, and the folders' timelines are a menu choice.
2. **`<project> (Resolve markers).edl`**: the marks and the moments as markers on the "clips" timeline.
   Right-click the timeline in the media pool → Timelines → Import → Timeline Markers from EDL. Resolve's own
   marker EDL is CMX3600 lines, each followed by `|C:ResolveColor<color> |M:<name> |D:<frames>`, at the
   timeline's own timecode. The player wrote that timeline, so it knows where each clip starts on it. It sets
   the timeline to start at 01:00:00:00 (`tcStart="3600s"`, Resolve's default), with `FCM: DROP FRAME` when
   the clips are drop-frame (to try: the manual says its *Use drop frame* option is for 30 fps EDLs). Camera
   marks are blue, your own marks cyan, and moments green with their name and length.

   ```
   TITLE: 2026-10 Egypt markers
   FCM: DROP FRAME

   001  001      V     C        01:00:10;00 01:00:10;01 01:00:10;00 01:00:10;01
    |C:ResolveColorBlue |M:Mark 1 DJI_20261022152841_0068_D |D:1
   002  001      V     C        01:00:20;00 01:00:30;00 01:00:20;00 01:00:30;00
    |C:ResolveColorGreen |M:Turtle |D:600
   ```
3. **`<project> (Resolve metadata).csv`**: ratings and keywords on the clips in the media pool. File → Import
   Metadata To → Media Pool, matched by file name (the manual's *Match using filename*). Columns: File Name,
   Clip Color (Green for a favorite, Red for a reject), Flags, Keywords (the clip's folders and tags in
   `Clips/`, and *Moments* plus its moments' folders and tags), Comments (the moments in words: "0:20–0:30
   Turtle; 1:02–1:10 Moray eel"). A Smart Bin by keyword then gathers a folder's clips, which is Resolve's way
   to a bin from metadata. The exact header names aren't documented: they're taken from a CSV that Resolve
   exports itself (File → Export Metadata From) (to try).

**With Studio**, a script under `Fusion/Scripts/Utility` (Workspace → Scripts → ActionFootagePlayer marks) can
do it all in one: `MediaPool.ImportMedia`, then `MediaPoolItem.AddMarker(frame, color, name, note, duration,
customData)` on the clips themselves, `SetClipColor`, `AddFlag`, `SetMetadata`, and
`MediaPool.CreateTimelineFromClips(name, [{mediaPoolItem, startFrame, endFrame}])` for the moments, and
`MediaPool.AddSubFolder` for real bins as the folders are. The frames are counted from the clip's first frame, so
no timecode is needed. This is the step after the files, not instead
of them (and Divecut, the paid competitor, needs Studio for the same reason).

### Premiere Pro: FCP7 XML

- **Not FCPXML.** Adobe: "Final Cut Pro X exports a .fcpxml file format that Premiere cannot directly import"
  ([Migrate from Final Cut Pro X](https://helpx.adobe.com/premiere/desktop/organize-media/import-files/migrate-from-final-cut-pro-x.html)).
- **Not an XMP sidecar.** For a format that can hold XMP, Premiere keeps it in the file ([Metadata in
  Premiere](https://helpx.adobe.com/premiere/desktop/organize-media/edit-metadata/metadata-in-premiere.html)). MP4
  can, in a `uuid` box (XMP Specification Part 3). So a `GX010123.xmp` next to `GX010123.MP4` probably isn't read.
  Writing the markers into the MP4 would work, as ExifTool users do, but the clip isn't touched. Premiere's own
  *Write clip markers to XMP* would rewrite the camera files too. The XMP form (`xmpDM:Tracks`, `trackType
  Comment`, `frameRate f254016000000`) is noted in case a test shows the sidecar *is* read (to try).
- **No marker CSV import**: Premiere exports markers as CSV but doesn't import them; the request is still open.

So it's **`<project> (Premiere).xml`**, FCP7 XML (`xmeml version="4"`), imported with File → Import. Premiere
makes a bin of it, and inside it the `bin`s the file nests:

- **Clips**: a bin per folder of `Clips/` (`Day 1`), and in it a `clip` per clip (rejects left out), its `file`
  with `pathurl` `file://localhost/Volumes/...` (percent-encoded), `rate` (`<timebase>60</timebase><ntsc>TRUE</ntsc>`
  for 59.94), `duration` in frames, and `timecode` (`string`, `frame`, `displayformat DF`/`NDF`). With the
  timecode, Premiere's Link Media, which matches on File Name and Media Start, finds the file. Each file is
  described once (`file id`), and later references use the id.
- **Markers on each clip**: `name`, `comment`, `in` = the frame from the start of the file, `out` = -1 for a
  point (a mark) or the moment's out frame (a moment, which Premiere shows as a marker with a length).
- **Moments**: a bin per folder of `Moments/` (`Moments`, `Moments/Reef`), and in it a `clip` per moment, named
  after it, on the same `file`, with the moment's `in` and `out`. In Premiere that's the clip with its in and out
  points set, or a subclip (to try which; a `subclipinfo` element is the other way to write it).
- **A sequence** "<project> – moments" of `clipitem`s, each a moment, named, on the same `file`.
- **Ratings**: a label color for the clip's or moment's rating (`<labels><label2>Forest</label2>` favorite,
  `Violet` reject?), and its tags in `comments` (`<mastercomment1>`), since FCP7 XML has no keywords.

Whether the markers on a `clip` in a bin become the master clip's markers in the Project panel, or only those
in the sequence come in, Adobe doesn't say (to try). The sequence has them either way, since its clipitems carry
the same markers.

**(to try)** Premiere isn't installed on this Mac.

### Gyroflow

Two ways in, for two uses:

1. **A project of the moments, by an explicit command.** Upstream Gyroflow (1.6.0 and later) saves the trims as
   `"trim_ranges_ms": [[start_ms, end_ms], ...]`, in milliseconds of the clip (a negative end counts from the
   clip's end), and renders each range as a file of its own when `output.export_trims_separately` is true
   (`-001`, `-002`…). (`src/core/lib.rs`, `trim_ranges_ms` loaded at L2728–2760 on master `9f8c7fa`.)

   **The clip's own project is never written to.** Gyroflow rebuilds the whole file when it saves
   (`export_gyroflow_data` in `core/lib.rs`: `json!` and `merge_json`), so anything of ours in it would go at
   the next save. And the trims aren't a list on the side: they change the smoothing, the sync and the render.
   A clip's playback project with trims in it would play differently.

   So *Gyroflow: render these moments* (a clip's menu, and Marks for the editor for the whole project) writes
   **`Exports/Gyroflow/<clip>.gyroflow`**: a copy of the clip's own project if it has one, else a new one as
   `Ctrl+e` writes it (gyroflow-project). Its trims are the clip's moments, with `export_trims_separately` true,
   and the output folder is `Exports/Gyroflow/`. Gyroflow opens it with the moments as trims, and so does its
   render queue or the CLI (`gyroflow --preset` takes a project). The player never reads these copies.
2. **#1248's fork: a markers JSON for many clips.** In [gyroflow#1248](https://github.com/gyroflow/gyroflow/issues/1248)
   (open, no word from the maintainer yet), TimoLehnertz's fork (branch `feature/import-markers`,
   `src/marker_import.rs`) imports a flat JSON array of
   `{"type":"in","timestamp":…,"name"?,"path"?}`, `{"type":"out","timestamp":…}` and
   `{"type":"marker","timestamp":…,"name":…,"color"?:"#rrggbb"}`, where `timestamp` is **Unix time in seconds**.
   It finds the clip by its `creation_time` (whole seconds): a mark goes to the clip whose creation_time ≤ the
   timestamp < creation_time + duration. The time in the clip is the timestamp minus the creation_time. The fork
   has an offset for cameras that write local time as UTC. Each in is paired with the next out. The fork writes
   the result into the clip's settings as upstream's `trim_ranges_ms`.

   *Gyroflow (#1248 markers)* writes `<project> (Gyroflow markers).json` in that format. For each clip,
   `created` = its `creation_time`, truncated to the second as the fork reads it, so the two agree even when the
   camera's clock or zone is wrong (the GoPro's 2016). A moment becomes `in` (`created + in`, its name, `path`
   = `<moment name>.mp4`) and `out`. A mark becomes a `marker` (its name or `"Mark n"`, blue for a camera's),
   and a favorite clip a `marker` at its start ("Favorite", green), since the format has no rating. A rejected
   clip's whole-clip entries are left out; its moments still go, since a moment that exists is wanted.

   ```json
   [
     {"type": "in", "timestamp": 1792675742.02, "name": "Turtle", "path": "DJI_20261022152841_0068_D-1.mp4"},
     {"type": "out", "timestamp": 1792675752.03},
     {"type": "marker", "timestamp": 1792675732.01, "name": "Highlight 1", "color": "#2f6fdf"}
   ]
   ```

   The same format read *in* (*Project → Import marks…*) would let the player take in/out times from a timing
   system, which is Timo's case (races: a rider's start and finish): each in/out pair a moment, each marker a
   mark. Later, and only if the fork's format holds.

Not chosen: keeping the player's own data in the fork's format. It has no ratings, folders or tags, ties
everything to a clock that's often wrong, and has no clip names.

## How it's started

### The menu

Next to Export, and under *Project*:

```
Marks for the editor            ▸  Final Cut Pro…
                                    DaVinci Resolve…
                                    Premiere Pro…
                                    Gyroflow: render this clip's moments
                                    Gyroflow: #1248 markers…
                                    ───────
                                    Clips: ● with moments or marks  ○ all but rejects
```

The panel's Project section has the same buttons.

### When the file for the editor is written

- **By hand, for the project**: Marks for the editor → the editor. It lists the project's clips, reads the
  header of each (timecode, rate, size, highlights) and the marks files, and takes the moments and the clips'
  previews from the player's cache. That's a few small reads per clip, a second or two for a hundred clips on
  the NAS, with a progress line. Then it writes the file and says where: "Final Cut Pro: 14 clips, 23 moments,
  41 marks → 2026-10 Egypt (Final Cut Pro).fcpxml", and shows it in Finder.
- **Which clips**: by default those with a moment or a mark (the camera's highlights count), leaving out
  rejects. *All but rejects* takes every clip of the project, for a full selection timeline.
- **Not automatically.** FCP and Resolve take an import once and don't merge a second one, so a file rewritten
  on every change would be stale the moment it was imported. The moments and marks are the automatic part; the
  editor's file is the step "I've chosen, now to the edit".
- **Named after the project, in its folder** (next to `Moments/`, not in `Exports/`, which is what gets deleted
  for room): `<project> (Final Cut Pro).fcpxml`, `… (Resolve).fcpxml`, `… (Resolve markers).edl`,
  `… (Resolve metadata).csv`, `… (Premiere).xml`, `… (Gyroflow markers).json`. Each is overwritten when written
  again.
- **Open it now** on a Mac (`open -a "Final Cut Pro" <file>`): offered after writing the FCP file, not done by
  itself, since it takes the screen.
- **From the command line**: not in the first version. The writers are plain Lua and need the header reader
  and the link reader, not mpv, so a `marks-export <project>` can follow if it's asked for.

## Ratings in each editor

| | Clip favorite | Clip reject | Moment | Moment favorite | Folders and tags |
|---|---|---|---|---|---|
| | *Favorite* tag on `Clips/<clip>.mp4` | its preview deleted | the file | *Favorite* tag | `Moments/` and `Clips/` subfolders, Finder tags |
| FCP | `rating value="favorite"` over the clip | `rating value="reject"` over the clip | keyword range, in the *moments* project | `rating value="favorite"` over it | keywords, collections in collection folders |
| Resolve (free) | Clip Color Green, Flag Green (CSV) | Clip Color Red, Flag Red (CSV), left off the timelines | on the *moments* and its folder's timeline; a green marker on *clips* | a green flag on its clip (CSV) | keywords (CSV); a timeline per folder of moments |
| Resolve Studio | `SetClipColor`, `AddFlag` (script) | the same | a clip marker with a length, a subclip | the same | real bins |
| Premiere | a label color | left out | a clip with in/out in its folder's bin; in the *moments* sequence | a label color | bins; tags in a comment |
| Gyroflow | a marker (fork JSON) | left out | a trim range (its own project); in/out (fork JSON) | — | — |

## The plan

Each step is a PR of its own, and each works without the next. It comes after the other design's steps 1–6
(the workspace module, the link, previews, moments, exports, marks), since it reads what they write. Clip
ratings need its step 7 (clip previews), and tags its step 8; without them, every clip is unrated and the
moments have no keywords but their folders.

1. **Clip header reader** (`highlights/mp4info.lua`, next to `markers.lua`, the same box walker): the size,
   the duration (`mvhd`), the video track's frame rate (`mdhd` timescale, `stts`) and size (`tkhd`), the `tmcd`
   start frame and drop-frame flag, and `creation_time`. It's plain Lua, with seeks only. Tests: `luajit` on
   small clips that ffmpeg makes (`-timecode "20:28:05;12"` at 60000/1001, `00:24:35:18` non-drop, 25 fps, none),
   checked against ffprobe; and on the real clips here, outside the repo. (The link reader of the other design
   shares this walker; whichever is built first brings it.)
2. **Time module** (`marks/timing.lua`): seconds → frame, rational seconds, timecode strings DF/NDF and back.
   Tests: every frame of a 10-minute span at 59.94 and 29.97 DF, round trip; the three clips' strings as ffprobe
   gives them.
3. **The project, read** (`marks/project.lua`): the clips, their moments, folders, tags and ratings, and their
   marks, as one table that every writer takes. Tests: a project tree made in a temporary folder (two ffmpeg
   clips, moments made by gyroflow-export's `--preview still` or by ffmpeg with the key, a deleted clip preview
   listed in `clips-made.txt`, tags set with `xattr`).
4. **Writers** (`marks/out/*.lua`): FCPXML, Resolve's FCPXML + EDL + CSV, FCP7 XML, the Gyroflow moments
   project, the fork's JSON; and the menu. Tests: golden files for the project of step 3, compared byte for
   byte. FCPXML is checked against Apple's DTD with `xmllint --dtdvalid` (the DTD is in Final Cut Pro's bundle;
   it's needed once to put into the tests). EDL lines are checked against an EDL Resolve exports itself.
5. **The imports by hand** (below), which may change step 4. They're also worth doing *before* step 4 with
   hand-written files: an afternoon that settles every "to try" above.
6. **README**: Marks for the editor, and in How it works the formats and the timecode.

Later: the Resolve Studio script; reading #1248's JSON in; a recording split into files as one clip.

## Checking it

Automatic (steps 1–4): the tests above, with no camera or editor needed. Clips with highlights come from the
real ones: none of the DJI or GoPro clips here has been checked to have any (the highlights plugin's README says
so). One clip from each camera with three highlights marked, and the seconds they were marked at, is the test
footage that's missing.

By hand, on a Mac, each import in its editor. **Each takes the screen, so Anton is asked first.**

| Editor | What's imported | What to look at |
|---|---|---|
| DaVinci Resolve 21.0 (free, installed) | `(Resolve).fcpxml` with *Automatically import source clips* | all timelines come in (or one per file?); the clips link to the MP4s on the NAS; the Start TC column shows `20:28:05;12`; the moments at the right frames, with their names; whether `marker`/`keyword`/`rating` on the clips survive |
| | `(Resolve markers).edl` → Timeline Markers from EDL, on *clips* | markers at the marks' frames at 59.94 DF; the moments' lengths; colors; names |
| | File → Export Metadata From on one clip, then `(Resolve metadata).csv` | the header names; the clip colors, flags, keywords and comments on the right clips; a Smart Bin by keyword |
| Final Cut Pro (not installed: a 90-day trial) | `(Final Cut Pro).fcpxml`, *Leave files in place* | the event, the clips at their timecode, the markers on the frame, the keyword ranges in their collections and collection folders, favorites and rejects in the filters, the *moments* project with the names; a moment as a browser clip of its own; importing again: replace / keep both |
| Premiere Pro (not installed: a 7-day trial, or a user) | `(Premiere).xml` | the bins as the folders; the master clips' markers (or only the sequence's); the moments as clips with in/out or subclips; the sequence; relinking to the files; and once, `GX….xmp` next to a clip, to know whether the sidecar is read |
| Gyroflow 1.6 (installed) | `Exports/Gyroflow/<clip>.gyroflow` written by the player | the moments as trims on its timeline; rendering them one file each into `Exports/Gyroflow/` |
| #1248's fork (built from `TimoLehnertz/gyroflow`, branch `feature/import-markers`) | `(Gyroflow markers).json` | each moment on the right clip with offset 0, the markers |

Each check gets a line in the PR: the version, what was imported, what came in, with a screenshot.

## Open questions

1. **Final Cut Pro**: there's none on this Mac. Is a trial worth installing, or do you have a Mac with it, for
   the check?
2. **Resolve's checks** on this Mac (21.0 free): may I run them, when the screen is free? And don't update to
   21.1 before then, if the Studio script is to be looked at on the free version too.
3. **Premiere**: test it at all (a trial), or leave it to a user to report?
4. **Test footage with highlights**: a DJI clip (the quick switch button while recording) and a GoPro one, with
   three highlights each and the seconds they were marked at.
5. **Gyroflow**: write the fork's format now, while #1248 has no answer from the maintainer, or only the
   moments project until it's settled? It's a small writer, and the issue's author is the user it's for.
6. **Moments in FCP's browser**: as keyword ranges on the clips only (as designed), or also as clips of their
   own, named, if FCP takes them? Their names show in the *moments* project either way.
7. **Which clips by default**: those with a moment or a mark (as designed), or every clip but the rejects?
