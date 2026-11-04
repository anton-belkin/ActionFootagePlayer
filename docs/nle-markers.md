# Marks to the editor: design

The camera's highlights, the parts marked in the player and a rating, taken to Final Cut Pro, DaVinci Resolve,
Premiere Pro and Gyroflow as markers, ranges and ratings on the original clips. A design, not built yet: what to
write, how each editor takes it, where the marks live between runs, how it looks in the player, and how to check
it.

Written 2026-11-04. The clips' timecode was checked on this Mac (an Osmo Action 6, a HERO7 Black); the editors'
side comes from their documentation, cited below, with what it leaves open marked **(to try)**: those are the
imports to do by hand before building ([Checking it](#checking-it)).

## In short

- **One marks file per clip, next to it**: `DJI_..._D.marks.json`, as its `.gyroflow` project already is. The
  clip is never written to. Where its folder can't be written, the file goes in the app's folder, found by the
  clip's name and size.
- **In it**: the parts kept (in, out, a name, a rating), a rating for the clip (favorite or reject), and which
  camera highlights were hidden. The highlights themselves aren't copied: they're read from the clip, as now.
- **Out, a folder (a trip) at a time, from the menu**: one file per editor, in the folder:
  - **Final Cut Pro**: FCPXML 1.10, an event of the clips with their markers, keyword ranges and
    favorite/reject ratings, plus a project of the kept parts.
  - **DaVinci Resolve**: FCPXML too (File → Import → Timeline), giving two timelines on the original files, the
    kept parts and the clips with their highlights. The markers go on as timeline markers from an EDL, and the
    ratings as clip colors and flags from a metadata CSV. That's three files, all of which the free version
    takes. A Resolve script would do it in one, but since 21.1 scripting is Studio's.
  - **Premiere Pro**: FCP7 XML (xmeml), a bin of the clips with their markers and a sequence of the kept parts.
    Not an XMP sidecar: for an MP4, Premiere reads XMP from inside the file, and the clip isn't written to.
  - **Gyroflow**: the kept parts as `trim_ranges_ms` in the clip's `.gyroflow` project, and the folder's marks
    in the JSON of #1248's fork (in/out/marker by Unix time).
- **Times by the clip's own timecode.** The brief assumed action cameras' MP4s have no timecode, but these two
  *do* write a `tmcd` track: a camera clock, drop-frame on the DJI at 59.94, and the editors place a clip at it.
  So every time written out is the clip's start timecode plus the time in the clip, on a frame. A clip without
  the track starts at 00:00:00:00.
- **New keys**: `k` keeps the A-B part (`l` twice) and clears it for the next one; `K` makes the clip a
  favorite and `X` a reject (again: no rating). A part exported with `Shift+E` is kept too.
- **Built in plain Lua** like the highlights plugin, with a small MP4 header reader for the timecode, frame rate
  and size, so it works in any mpv. Tests run with `luajit` on small clips that ffmpeg makes with a timecode.

## What there is now

| | Where it comes from | Where it lives | Lost when |
|---|---|---|---|
| The camera's highlights | `highlights/markers.lua` reads them from the MP4 when it opens (DJI `hglg`, GoPro `HMMT`, GPMF `HLMT`) | memory: `user-data/highlights` (`camera`, `times` in seconds), and chapters "Highlight n" | never: read again each time |
| The part marked with `l` | mpv's own A-B loop: `ab-loop-a`, `ab-loop-b` | memory, one per clip | the next clip, quitting |
| An exported part | `gyroflow/export.lua` passes the A-B loop to gyroflow-export as `--from`/`--to` | the export's name (`stabilized/<name>_stab_<from>-<to>s.mov`) | — |
| A rating | none | — | — |

The menu (`footage-menu`) has Mark start of part, Mark end of part, Clear part, Export part and Highlights (n),
and the panel's Clip section has the same. Between runs nothing about a clip is kept except its Gyroflow
project (`<name>.gyroflow` next to it, found by `find_project()`) and its exports (`stabilized/`). The player and
gyroflow-export don't read trims from a project, so trims written into one don't change playback or `Shift+E`.

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
plays each file on its own, so the marks, the highlights and the parts are per file, and a part can't cross from
one file into the next. Out, the files of one recording follow each other by name and by timecode, which runs
on, so they sort in order in each editor. A recording as one clip in FCP (a compound clip of its files) or
Resolve waits for the player to play a recording as one: the virtual join in the workflow study.

## The editors and their formats

### Time on a frame

The marks file keeps seconds. Each format wants frames, timecode or rational seconds, so all the conversions
are in one module:

- The frame rate is the container's, exactly (`60000/1001`, not 59.94), from the video track's `stts`.
- A mark's frame is `n = floor(t × rate + 0.5)` (a highlight, a marker). A part's in rounds the same way and its
  out too, kept at least a frame after its in and no later than the clip's last frame.
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
- `library` → `event name="<folder>"` → one `asset-clip` per clip (browser clips, no project needed), with:
  - a `marker` per highlight: `value="Highlight 3"`, one frame long, `note="DJI quick switch"` or `"GoPro HiLight"`;
  - a `keyword` per kept part, `value="Kept"` plus its name if it has one, so each kept part shows in the event as a
    range, and in a Keyword Collection *Kept*;
  - a `rating` per kept part, `value="favorite"` over the part. The clip's own rating is `value="favorite"` or
    `"reject"` over the whole clip. FCP shows them as green and red lines, under the browser's Favorites and
    Hide Rejected filters;
  - in the DTD's order: markers, keywords, ratings, then `metadata`. A file out of order is refused whole.
- a `project name="<folder> – kept parts"` in the same event: a `sequence` (`tcFormat` as the clips) whose
  `spine` holds an `asset-clip` per kept part (`start` = the part's in, `duration` = its length), in the order
  of the clips' timecode. That's a rough cut to start from.
- `import-options`: none. Whether to copy the media or leave it in place is then the user's import setting.
  "Leave files in place" links to the originals, which is what a NAS needs.

Example: the Action 6 clip above, a highlight at 10.01 s and a part kept from 20.02 s to 30.03 s:

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
    <event name="2026-10-22">
      <asset-clip name="DJI_20261022152841_0068_D" ref="r2" format="r1" tcFormat="DF"
                  start="4421104688/60000s" duration="2854852/60000s">
        <marker start="4421705288/60000s" duration="1001/60000s" value="Highlight 1" note="DJI quick switch"/>
        <keyword start="4422305888/60000s" duration="600600/60000s" value="Kept"/>
        <rating start="4422305888/60000s" duration="600600/60000s" value="favorite"/>
      </asset-clip>
      <keyword-collection name="Kept"/>
    </event>
  </library>
</fcpxml>
```

(`4421104688` = 4 416 688 frames × 1001, which is `20:28:05;12` at 59.94 drop-frame. The clip is 2852 frames
long; 10.01 s is frame 600, 20.02–30.03 s frames 1200–1800.)

**Importing again**: clips go into the event of that name if there is one. For a clip already in it, FCP asks
whether to replace it or keep both, once for the whole import. It doesn't merge. So the export is one go per
trip, after the selection, and the menu says so. A second import with *Replace* gives the newer marks.
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

1. **`<folder> (Resolve).fcpxml`**, imported with File → Import → Timeline… (Shift+Cmd+I) and *Automatically
   import source clips into media pool*. Resolve finds the media by the file paths, then the clip names, then
   timecode (Reference Manual, Conform). It's the FCP file without the event's browser clips, as two projects,
   which Resolve imports as timelines on the original files:
   - **"<folder> – kept parts"**: the kept parts in a row, as for FCP;
   - **"<folder> – clips"**: every clip that isn't rejected, whole and in timecode order. This is where the
     highlights go, since a timeline is the one place Resolve takes markers from a file.

   With the FCPXML's markers on the timeline clips, step 2 isn't needed, if Resolve keeps them (to try).
   Whether Resolve can take both projects from one file, or only one timeline per import, is to try too; if it
   takes one, these are two files.
2. **`<folder> (Resolve markers).edl`**: the highlights and the kept parts as markers on the "clips" timeline.
   Right-click the timeline in the media pool → Timelines → Import → Timeline Markers from EDL. Resolve's own
   marker EDL is CMX3600 lines, each followed by `|C:ResolveColor<color> |M:<name> |D:<frames>`, at the
   timeline's own timecode. The player wrote that timeline, so it knows where each clip starts on it. It sets
   the timeline to start at 01:00:00:00 (`tcStart="3600s"`, Resolve's default), with
   `FCM: DROP FRAME` when the clips are drop-frame (to try: the manual says its *Use drop frame* option is for 30
   fps EDLs). Highlights are blue, kept parts green with their length.

   ```
   TITLE: 2026-10-22 markers
   FCM: DROP FRAME

   001  001      V     C        01:00:10;00 01:00:10;01 01:00:10;00 01:00:10;01
    |C:ResolveColorBlue |M:Highlight 1 DJI_20261022152841_0068_D |D:1
   002  001      V     C        01:00:20;00 01:00:30;00 01:00:20;00 01:00:30;00
    |C:ResolveColorGreen |M:Kept |D:600
   ```
3. **`<folder> (Resolve metadata).csv`**: the ratings, the parts' names and keywords on the clips in the media
   pool. File → Import Metadata To → Media Pool, matched by file name (the manual's *Match using filename*).
   Columns: File Name, Clip Color (Green for a favorite, Red for a reject), Flags, Keyword (`Kept`), Comments
   (the kept parts in words: "0:20–0:30 Turtle; 1:02–1:10"). The exact header names aren't documented: they're
   taken from a CSV that Resolve exports itself (File → Export Metadata From) (to try).

**With Studio**, a script under `Fusion/Scripts/Utility` (Workspace → Scripts → ActionFootagePlayer marks) can
do it all in one: `MediaPool.ImportMedia`, then `MediaPoolItem.AddMarker(frame, color, name, note, duration,
customData)` on the clips themselves, `SetClipColor`, `AddFlag`, `SetMetadata`, and
`MediaPool.CreateTimelineFromClips(name, [{mediaPoolItem, startFrame, endFrame}])` for the kept parts. The frames
are counted from the clip's first frame, so no timecode is needed. This is the step after the files, not instead
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

So it's **`<folder> (Premiere).xml`**, FCP7 XML (`xmeml version="4"`), imported with File → Import. Premiere
makes a bin of it:

- a `clip` per clip, its `file` with `pathurl` `file://localhost/Volumes/...` (percent-encoded), `rate`
  (`<timebase>60</timebase><ntsc>TRUE</ntsc>` for 59.94), `duration` in frames, and `timecode` (`string`,
  `frame`, `displayformat DF`/`NDF`). With the timecode, Premiere's Link Media, which matches on File Name and
  Media Start, finds the file;
- `marker`s on it: `name`, `comment`, `in` = the frame from the start of the file, `out` = -1 for a point (a
  highlight) or the part's out frame (a kept part, which Premiere shows as a marker with a length);
- a `sequence` "<folder> – kept parts" of `clipitem`s, each with the part's `in`/`out` on the same `file`;
- a label color for the clip's rating (`<labels><label2>Forest</label2>` favorite, `Violet` reject?).

Whether the markers on a `clip` in a bin become the master clip's markers in the Project panel, or only those
in the sequence come in, Adobe doesn't say (to try). The sequence has them either way, since its clipitems carry
the same markers.

**(to try)** Premiere isn't installed on this Mac.

### Gyroflow

Two ways in, for two uses:

1. **The clip's own `.gyroflow` project: its trims.** Upstream Gyroflow (1.6.0 and later) saves the trims as
   `"trim_ranges_ms": [[start_ms, end_ms], ...]`, in milliseconds of the clip (a negative end counts from the
   clip's end), and renders each range as a file of its own when `output.export_trims_separately` is true
   (`-001`, `-002`…). Otherwise it renders them as one, which reading the code suggests is the ranges joined.
   (`src/core/lib.rs`, `trim_ranges_ms` loaded at L2728–2760 on master `9f8c7fa`.)
   The menu's *Gyroflow (trim ranges)* writes the kept parts there, and sets `export_trims_separately` to true.
   It does that in the project next to the clip if there is one, else in a new one written as `Ctrl+e` writes
   it (gyroflow-project). Then Gyroflow opens the clip with its parts as trims, and so does its render queue or
   the CLI (`gyroflow --preset` takes a project). The rest of the file is kept as Gyroflow wrote it. The
   player and gyroflow-export ignore trims, so playback doesn't change.
2. **#1248's fork: a markers JSON for many clips.** In [gyroflow#1248](https://github.com/gyroflow/gyroflow/issues/1248)
   (open, no word from the maintainer yet), TimoLehnertz's fork (branch `feature/import-markers`,
   `src/marker_import.rs`) imports a flat JSON array of
   `{"type":"in","timestamp":…,"name"?,"path"?}`, `{"type":"out","timestamp":…}` and
   `{"type":"marker","timestamp":…,"name":…,"color"?:"#rrggbb"}`, where `timestamp` is **Unix time in seconds**.
   It finds the clip by its `creation_time` (whole seconds): a mark goes to the clip whose creation_time ≤ the
   timestamp < creation_time + duration. The time in the clip is the timestamp minus the creation_time. The fork
   has an offset for cameras that write local time as UTC. Each in is paired with the next out. The fork writes
   the result into the clip's settings as upstream's `trim_ranges_ms`.

   *Gyroflow (#1248 markers)* writes `<folder> (Gyroflow markers).json` in that format. For each clip,
   `created` = its `creation_time`, truncated to the second as the fork reads it, so the two agree even when the
   camera's clock or zone is wrong (the GoPro's 2016). A kept part becomes `in` (`created + in`, its name, `path`
   = `<clip>-<n>.mp4`) and `out`. A highlight becomes a `marker` (`"Highlight n"`, blue), and the clip's rating a
   `marker` at its start ("Favorite" green, "Reject" red), since the format has no rating. A rejected clip's
   parts are left out.

   ```json
   [
     {"type": "in", "timestamp": 1792675742.02, "name": "Turtle", "path": "DJI_20261022152841_0068_D-1.mp4"},
     {"type": "out", "timestamp": 1792675752.03},
     {"type": "marker", "timestamp": 1792675732.01, "name": "Highlight 1", "color": "#2f6fdf"}
   ]
   ```

   The same format read *in* (File → Import marks…) would let the player take in/out times from a timing system,
   which is Timo's case (races: a rider's start and finish). It reads them into the clips' marks files, as the
   fork reads them into projects. Later, and only if the fork's format holds.

Not chosen: making the marks file itself in the fork's format. It has no ratings or hidden highlights, ties
everything to a clock that's often wrong, and has no clip names. The player keeps its own file and writes the
fork's.

## Where the marks live

### A file next to the clip

`<clip without extension>.marks.json`, next to the clip, as `<clip>.gyroflow` is:
`DJI_20261022152841_0068_D.MP4` → `DJI_20261022152841_0068_D.marks.json`. JSON because mpv's Lua reads and
writes it (`utils.parse_json`, `format_json`), as the gyroflow plugin reads projects, and because Gyroflow's
projects and #1248's fork are JSON too. XMP was the other choice, but it has no place for a part kept with its
rating, Lua has no XML parser, and Premiere wouldn't read the sidecar for an MP4 anyway.

```json
{
  "format": "actionfootageplayer-marks",
  "version": 1,
  "clip": {
    "file": "DJI_20261022152841_0068_D.MP4",
    "size": 5123456789,
    "duration": 47.580867,
    "frame_rate": "60000/1001",
    "timecode": "20:28:05;12",
    "created": "2026-10-22T13:28:42Z"
  },
  "rating": "favorite",
  "parts": [
    { "in": 20.020, "out": 30.030, "name": "Turtle", "rating": "favorite" },
    { "in": 41.500, "out": 44.000 }
  ],
  "highlights_hidden": [ 2 ]
}
```

- **Times in seconds of the clip** (mpv's `time-pos`, from 0), to the millisecond. Not frames or timecode: the
  player knows seconds, each format wants something else, and the conversions are in one place.
- **`clip`** says which clip it's for and what the exports need without opening it again (a NAS asleep). A
  marks file whose clip now has another size is renamed `.marks.json.old`, not applied to the wrong clip.
- **`rating`**: `favorite`, `reject`, or absent. On a part too, `favorite` when absent (a part is kept because
  it's wanted); `reject` only matters for a part kept by `Shift+E` that turned out bad.
- **`highlights_hidden`**: the camera's highlights (by number) that the user hid, such as a press by mistake.
  They're still read from the clip, just not shown or exported.
- **Points of the user's own** ("a marker here", without a part) aren't in the first version. A `marks` list
  (`time`, `name`, `note`) is the obvious place for them, with a key such as `M`.
- **Written** when something changes, whole, to a temporary file renamed over the old one. **Read** on
  `file-loaded`. Deleting it forgets the clip's marks; nothing else refers to it. **Fields it doesn't know are
  kept** when the player writes it again.
- A clip with nothing kept, rated or hidden has no file. Removing the last part removes the file.

### When the folder can't be written

When the SD card's lock is on, a share is read-only or the folder is someone else's, the file goes to the app's
folder instead: `~/Library/Application Support/ActionFootagePlayer/marks/<clip name>-<size>.marks.json` (Windows
`%LOCALAPPDATA%\ActionFootagePlayer\marks\`; the plugins in your own mpv: `~~state/marks/`). It's found by name
and size, so it still applies once the card is copied to the NAS. The next change there moves it next to the
clip. The menu says where it is.

### Not chosen

- **Writing into the MP4** (as the camera writes its highlights): the original is never touched, and rewriting
  a 4 GB file on a card or a NAS is slow and risky.
- **One file for the folder**: one clip copied elsewhere would leave its marks behind, and two players on one
  NAS folder would overwrite each other. The export reads the per-clip files instead.
- **A database in the app's folder**: the marks would stay on one computer, while files next to the clips go
  with them to the NAS and to the next Mac.

## How it looks in the player

### Keys

| Key | | `script-binding` |
|---|---|---|
| `l` | as now: the start of a part, its end, then clear it (mpv's A-B loop) | — |
| `k` | **keep the part**: the A-B part becomes one of the clip's kept parts, and the loop is cleared for the next. The OSD says "Kept part 2: 0:20–0:30". Without both ends: "Mark a part with l first" | `marks/keep` |
| `K` | the clip is a **favorite**; again: no rating | `marks/favorite` |
| `X` | the clip is a **reject**; again: no rating | `marks/reject` |
| `Shift+E` | as now; a part it exports is kept too | `gyroflow/export` |

`k`, `K` and `X` are free in mpv's default input.conf and in the plugins. FCP's own keys (`F`, `Delete`, `U`)
are taken in mpv (`F` subtitles, `DEL` the OSC).

Not every A-B part is kept as soon as it's marked, because `l` is also how one loops a moment to look at it
closely. Keeping all of those would fill the list with tries. Keeping is a key of its own, and exporting counts
as keeping.

### On the seek bar

The kept parts are chapters, as the highlights are: "Part 1" at its in and "Part 1 end" at its out. So the seek
bar's marks show where they are and `PgUp`/`PgDn` reach them. mpv's OSC can't draw a range; the panel could,
later.

### The menu

At the top, where marking and exporting a part are now:

```
Mark start of part                 l
Mark end of part                   l
Keep part 0:20–0:30                k
Export part 0:20–0:30              Shift+E
Kept parts (2)                   ▸  0:20–0:30 Turtle        ▸  Play it (A-B loop)
                                     0:41–0:44                   Export it
                                     ───────                     Rename…
                                     Export all kept parts       Remove
Highlights (3)                   ▸  as now, and each one: Hide / Show
Favorite                           K   ✓
Reject                             X
```

and next to Export:

```
Marks for the editor            ▸  Final Cut Pro…
                                    DaVinci Resolve…
                                    Premiere Pro…
                                    Gyroflow: trims in this clip's project
                                    Gyroflow: #1248 markers…
                                    ───────
                                    Clips: ● with marks or highlights  ○ all but rejects
                                    Show this clip's marks file
```

The panel's Clip section gets the same: the kept parts as a list, the two ratings as toggles, and the editors
as buttons. *Export all kept parts* queues them in gyroflow-export, one `--from`/`--to` each, which it already
does for a part.

### When the file for the editor is written

- **By hand, for the folder**: Marks for the editor → the editor. It reads every `*.marks.json` in the clip's
  folder and the app's folder, and the header (timecode, rate, size, highlights) of every clip in the folder.
  That's a few small reads per clip, a second or two for a hundred clips on the NAS, with a progress line. Then
  it writes the files and says where: "Final Cut Pro: 14 clips, 23 parts, 41 highlights → 2026-10-22
  (ActionFootagePlayer).fcpxml", and shows it in Finder. The folder is the trip: copying a card into a folder
  per day or per trip is what people do, and it's the playlist the app already opens.
- **Not automatically.** FCP and Resolve take an import once and don't merge a second one, so a file rewritten
  on every mark would be stale the moment it was imported. The marks files are the automatic part; the editor's
  file is the step "I've chosen, now to the edit".
- **Named after the folder, written into it**: `<folder> (ActionFootagePlayer).fcpxml`, `… (Resolve).fcpxml`,
  `… (Resolve markers).edl`, `… (Resolve metadata).csv`, `… (Premiere).xml`, `… (Gyroflow markers).json`, each
  overwritten when written again. If the folder can't be written, they go in the app's folder and Finder opens
  there.
- **Open it now** on a Mac (`open -a "Final Cut Pro" <file>`): offered after writing the FCP file, not done by
  itself, since it takes the screen.
- **From the command line**: not in the first version. The writers are plain Lua and need the clip header
  reader, not mpv, so a `marks-export <folder>` can follow if it's asked for.

## Ratings in each editor

| | Clip favorite | Clip reject | Kept part |
|---|---|---|---|
| FCP | `rating value="favorite"` over the clip | `rating value="reject"` over the clip | `rating value="favorite"` + keyword *Kept*; in the *kept parts* project |
| Resolve (free) | Clip Color Green, Flag Green (CSV) | Clip Color Red, Flag Red (CSV) | on the *kept parts* timeline; a green marker on *clips* |
| Resolve Studio | `SetClipColor`, `AddFlag` (script) | the same | a clip marker with a length, a subclip |
| Premiere | a label color | a label color | a marker with a length; in the *kept parts* sequence |
| Gyroflow | a marker (fork JSON) | the clip's parts left out | a trim range |

A rejected clip is left out of the *kept parts* and *clips* timelines, and out of the file's clips unless the
menu says *all but rejects*.

## The plan

Each step is a PR of its own, and each works without the next.

1. **Clip header reader** (`highlights/mp4info.lua`, next to `markers.lua`, the same box walker): the size,
   the duration (`mvhd`), the video track's frame rate (`mdhd` timescale, `stts`) and size (`tkhd`), the `tmcd`
   start frame and drop-frame flag, and `creation_time`. It's plain Lua, with seeks only. Tests: `luajit` on
   small clips that ffmpeg makes (`-timecode "20:28:05;12"` at 60000/1001, `00:24:35:18` non-drop, 25 fps, none),
   checked against ffprobe; and on the real clips here, outside the repo.
2. **Time module** (`marks/timing.lua`): seconds → frame, rational seconds, timecode strings DF/NDF and back.
   Tests: every frame of a 10-minute span at 59.94 and 29.97 DF, round trip; the three clips' strings as ffprobe
   gives them.
3. **The marks plugin** (`mpv/scripts/marks/`): the marks file (read on `file-loaded`, written atomically, the
   app's folder when the clip's isn't writable, unknown fields kept), `k`/`K`/`X`, the parts as chapters,
   `user-data/marks` for the menu, and `Shift+E` keeping the part (export.lua sends
   `script-message-to marks keep <a> <b>`). Tests: mpv with `--vo=null` over the IPC socket the agent plugin
   opens. Set the A-B loop, send `keep`, read the JSON, reload the clip, check the chapters. It needs no screen.
4. **Menu and panel**: Kept parts, Favorite/Reject, Hide on a highlight (`footage-menu`, `side-panel`).
5. **Writers** (`marks/out/*.lua`): FCPXML, Resolve's FCPXML + EDL + CSV, FCP7 XML, the `.gyroflow` trims, the
   fork's JSON; and *Marks for the editor* reading a folder. Tests: golden files for a fixed folder (two clips
   made by ffmpeg, marks files written by hand), compared byte for byte. FCPXML is checked against Apple's DTD
   with `xmllint --dtdvalid` (the DTD is in Final Cut Pro's bundle; it's needed once to put into the tests).
   EDL lines are checked against an EDL Resolve exports itself.
6. **The imports by hand** (below), which may change step 5. They're also worth doing *before* step 5 with
   hand-written files: an afternoon that settles every "to try" above.
7. **README**: Keeping parts and rating, Marks for the editor, and in How it works the formats and the timecode.

Later: the Resolve Studio script; reading #1248's JSON in; a recording split into files as one clip; the user's
own point markers.

## Checking it

Automatic (steps 1–5): the tests above, with no camera or editor needed. Clips with highlights come from the
real ones: none of the DJI or GoPro clips here has been checked to have any (the highlights plugin's README says
so). One clip from each camera with three highlights marked, and the seconds they were marked at, is the test
footage that's missing.

By hand, on a Mac, each import in its editor. **Each takes the screen, so Anton is asked first.**

| Editor | What's imported | What to look at |
|---|---|---|
| DaVinci Resolve 21.0 (free, installed) | `(Resolve).fcpxml` with *Automatically import source clips* | both timelines come in (or one per file?); the clips link to the MP4s on the NAS; the Start TC column shows `20:28:05;12`; the kept parts are at the right frames; whether `marker`/`keyword`/`rating` on the clips survive |
| | `(Resolve markers).edl` → Timeline Markers from EDL, on *clips* | markers at the highlights' frames at 59.94 DF; the kept parts' lengths; colors; names |
| | File → Export Metadata From on one clip, then `(Resolve metadata).csv` | the header names; the clip colors, flags, keyword and comments on the right clips |
| Final Cut Pro (not installed: a 90-day trial) | `(ActionFootagePlayer).fcpxml`, *Leave files in place* | the event, the clips at their timecode, the markers on the frame, the keyword ranges in *Kept*, favorites and rejects in the filters, the *kept parts* project; importing again: replace / keep both |
| Premiere Pro (not installed: a 7-day trial, or a user) | `(Premiere).xml` | the bin; the master clips' markers (or only the sequence's); the sequence of parts; relinking to the files; and once, `GX….xmp` next to a clip, to know whether the sidecar is read |
| Gyroflow 1.6 (installed) | a project with `trim_ranges_ms` written by the player | the trims on its timeline; rendering them one file each |
| #1248's fork (built from `TimoLehnertz/gyroflow`, branch `feature/import-markers`) | `(Gyroflow markers).json` | each part on the right clip with offset 0, the markers |

Each check gets a line in the PR: the version, what was imported, what came in, with a screenshot.

## Open questions

1. **Keys**: are `k` (keep the part), `K` (favorite) and `X` (reject) all right? And should the A-B part be
   kept as soon as it's marked after all, with no `k`?
2. **Final Cut Pro**: there's none on this Mac. Is a trial worth installing, or do you have a Mac with it, for
   the check?
3. **Resolve's checks** on this Mac (21.0 free): may I run them, when the screen is free? And don't update to
   21.1 before then, if the Studio script is to be looked at on the free version too.
4. **Premiere**: test it at all (a trial), or leave it to a user to report?
5. **Test footage with highlights**: a DJI clip (the quick switch button while recording) and a GoPro one, with
   three highlights each and the seconds they were marked at.
6. **The export's scope**: the folder (as designed), or also a selection of folders (a trip spread over days)?
7. **Gyroflow**: write the fork's format now, while #1248 has no answer from the maintainer, or only the
   `.gyroflow` trims until it's settled? It's a small writer, and the issue's author is the user it's for.
8. **Clip names in the editor**: as the files are (`DJI_20261022152841_0068_D`), or the kept part's name, where
   it has one, for the clip in the *kept parts* timeline?
