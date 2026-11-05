# Moments, clips and project folders: design

How the player keeps what you choose in your footage: the marks on a clip, the moments (parts of a clip) you
save, an overview of every clip, and the exports. All of it lives in one folder per trip, the **project folder**,
as plain files that Finder and Explorer show and that the editors take in ([Marks to the
editor](nle-markers.md) reads them). This is a design, not built yet.

Written 2026-11-04. Decided with Anton beforehand: the project folder and the sources file, marks copied on the
first edit, moments and clip overviews as small MP4s, `Exports/`. His answers to the first draft's questions
are in [Decided](#decided). Checked on this Mac, with small files ffmpeg
made: the link inside an MP4 (written by ffmpeg, read by mpv), what a plain remux does to it, which frame Quick
Look shows, and setting a file's dates. Not checked yet: anything on Windows, and typing names in mpv's console.
Those are marked **(to try)**.

## In short

- **A project folder per trip.** By default it's the trip's own folder: `Moments/`, `Clips/`, `Exports/` and
  the per-clip files go in next to the footage. When that folder can't be written (a locked card, a read-only
  share), or you choose so, the project folder is a separate one in `~/Documents/ActionFootagePlayer/<name>/`
  (Windows: `Documents\ActionFootagePlayer\`), named when it's made. It repeats the footage's folder tree and has a `Sources.txt` saying
  where the footage is. You can fix that file by hand, and it can list several paths, one per computer.
- **Marks** (points in a clip): the camera's highlights, until you change one. The first change (adding a mark,
  removing a camera one) writes `<clip>.marks.json` with all of them, the camera's flagged as such. Delete
  the file, and it's the camera's again. Nothing is ever written into a Gyroflow project.
- **Moments** (parts of a clip) are files: `Moments/Turtle.afplink.mp4`, a small real MP4 with a color preview, and
  inside it a link to the part of the original. Finder and Explorer show the frame, Quick Look plays it, and a
  double click opens the player, which plays that part of the original in full quality, stabilized. Deleting
  the file deletes the moment. Subfolders are yours, to sort moments into, and the editors get them as bins.
- **Clips** (the trip at a glance): `Clips/<clip>.afplink.mp4` per clip, a flipbook of a dozen color frames from it,
  linked to the whole clip. The grid of icons is in color, not log gray, and the space bar flips through a clip
  in seconds. Deleting one says "not interesting".
- **Exports** go to `Exports/` (now `stabilized/`), named in the same dialog as moments, with the same link
  inside. Old `stabilized/` folders stay where they are; new exports go to `Exports/`.
- **Rating is the file system**: a moment or clip that exists is kept, a deleted one is not. Finder tags (and
  Explorer's stars, on Windows) go further: favorite, and keywords for the editors.
- **The previews are rendered by gyroflow-export**, which already renders a clip the way the player shows it
  (the same Gyroflow engine and the same LUTs). They're H.264, 8-bit SDR, so that every OS shows them.

## The project folder

### One model, two places

A project has a **sources root** (the folder with the trip's footage) and a **project folder** (where the
player writes). By default they're the same folder:

```
2026-10 Egypt/                        the sources root and the project folder
  Day 1/
    DJI_20261022152841_0068_D.MP4
    DJI_20261022152841_0068_D.gyroflow        the clip's Gyroflow project, as now
    DJI_20261022152841_0068_D.marks.json      its marks, once edited
  Day 2/
    GX011057.MP4
  Moments/
    Turtle.afplink.mp4
    Reef/
      Moray eel.afplink.mp4
  Clips/
    Day 1/DJI_20261022152841_0068_D.afplink.mp4
    Day 2/GX011057.afplink.mp4
  Exports/
    Turtle.mov
  .actionfootageplayer/               the player's state for this project (hidden)
```

A separate project folder holds the same things, minus the footage, plus `Sources.txt`:

```
~/Documents/ActionFootagePlayer/2026-10 Egypt/
  Sources.txt
  Day 1/DJI_20261022152841_0068_D.marks.json
  Day 1/DJI_20261022152841_0068_D.gyroflow
  Moments/  Clips/  Exports/  .actionfootageplayer/
```

The per-clip files sit at the clip's path relative to the sources root, so the tree repeats the footage's
tree. Everything that points at the footage (the links in moments, clips and exports) points at that same
relative path: `Day 1/DJI_20261022152841_0068_D.MP4`. Moving the trip folder, or the footage to a new disk,
breaks nothing as long as the tree inside stays the same.

The first version builds only the first case ("the project folder is the trip folder"). The separate folder
comes second, but every file format here is the same for both, so nothing written by the first version needs
converting.

### `Sources.txt`

A plain text file, one path per line. The first one that exists is the sources root:

```
# Where this project's footage is. The first path that exists is used; edit it if the footage moves.
/Volumes/video/2026-10 Egypt
Z:\2026-10 Egypt
\\nas\video\2026-10 Egypt
```

- Several lines because one NAS share is `/Volumes/video` on a Mac and `Z:\` or `\\nas\video` on Windows.
- A relative path counts from the project folder, so a project folder on the same share as the footage works
  from every computer.
- `#` starts a comment. The player adds a line when you tell it the footage moved (menu: *Project → Footage
  moved…*). It puts the new path on top and keeps the old ones.
- For a removable card the player also writes `# volume <id>`: the volume's UUID (`diskutil info`; Windows: the
  volume serial). Every card of a camera mounts at the same path (`/Volumes/OSMO ACTION`). So without the id,
  next month's trip on a reformatted card would match last month's project by path, and its moments would land
  there. Formatting gives a card a new id, so the old project no longer matches and the new trip gets a new
  one. The id only applies to projects of read-only cards: a writable card's project is on the card itself.
  Without the line (written by hand, say), the path alone decides.

### Which project a clip belongs to

When a clip opens, from wherever, the player looks for its project in this order and stops at the first match:

1. **A separate project folder whose sources root contains the clip.** The player reads the `Sources.txt` of
   every folder in `~/Documents/ActionFootagePlayer/`, plus any project folder you've opened from elsewhere (kept in
   a list in the app's settings). That's a few small files, read once at start and again when one changes. If
   several roots contain the clip, the longest wins: a project for `Egypt/Day 2` beats one for `Egypt`.
2. **A project in the trip folder**: the nearest folder above the clip, or the clip's own folder, that has
   `.actionfootageplayer/` in it. The search stops at the volume's root or the home folder.
3. **No project**: only what's in the clip itself (its highlights, its timecode). Nothing is written until
   something is saved.

The first match is the only one read. A clip whose separate project exists is never also read from files next
to it. Reading two places would make "where is my moment" a question with two answers.

**Where a new project goes** (at the first save: a moment, a mark, an export, clip previews):

- Its folder is the clip's folder, or the folder above `DCIM` when the clip is in a camera's own tree
  (`DCIM/100MEDIA`, `DCIM/DJI_001`), since that's the card or the copy of one. For a trip folder with day
  subfolders, *Project → Use the folder above* moves the project up a level before anything is saved. Once
  something's saved, it's a move of files, which the player can do while nothing is playing (later).
- If that folder can't be written, the project is a separate one in the player's own folder,
  `~/Documents/ActionFootagePlayer/<name>/` (Windows: `Documents\ActionFootagePlayer\`), and **the player asks
  for its name** in the same dialog as a moment's:

  ```
  The card is read-only. Project name (Enter: OSMO ACTION 2026-10-22): █
  ```

  The suggestion is the trip folder's name, or for a card the volume's name and the first clip's date. A name
  already taken gets ` 2`. Esc saves nothing. The OSD then says where it went.
- *Project → Keep this project separately* makes a separate one for a writable folder too, for people who don't
  want anything next to their footage.

### What changes in the code

- **The Gyroflow project.** `find_project()` (gyroflow plugin) looks for `<name>.gyroflow` or `<path>.gyroflow`
  next to the clip. It becomes a call to the workspace module, which gives the same paths for a project in the
  trip folder and `<project folder>/<relative dir>/<name>.gyroflow` for a separate one. `Ctrl+e` (open in
  Gyroflow) writes a new project there too. A project in a separate folder still works in Gyroflow, which
  stores the video's absolute path. If the footage moves, Gyroflow asks for it, while the player finds the
  project by its place in the tree and never reads that path.
- **gyroflow-export** looks for the project next to the video, and writes to `stabilized/`. The player passes
  both (`--project`, `--out`). The Finder quick action and the command line, which run it without the player,
  need the same lookup in Rust. It's one small module, and the tests run both versions over the same tree of
  folders.
- **Every plugin that reads mpv's `path`** to find the clip (gyroflow and its export, highlights, the
  telemetry, dlogm, the menu, the panel: 24 reads in 8 files) takes it from the workspace plugin instead
  (`user-data/workspace/clip`, when `user-data/workspace/path` is the file playing), because a moment's file isn't
  the clip ([Playing a moment](#playing-a-moment)).

### Read-only cards and the NAS

- **A card with its lock on** (or a camera mounted read-only): the first save makes a separate project, as
  above. Clip previews and moments are written there and point at the card. Once the card is copied to the
  NAS: *Project → Footage moved…*, or a new line in `Sources.txt`, and the same project points at the copy.
  The links are relative, so every moment and preview follows.
- **The NAS, writable** (the usual case): the project is in the trip folder on the NAS, so the Mac and the
  Windows PC see the same moments and clips with no `Sources.txt` at all. Two players writing the same moment
  at once: the writes are whole files renamed into place, so the last one wins and neither is left half-written.
- **A read-only share**: a separate project, whose `Sources.txt` lists the share as each computer mounts it.
  The project folder can itself be on a writable share, opened once with *Project → Open project folder…* on
  each computer, to share it between them.
- **A NAS asleep or away**: the project folder may be local while the footage isn't. Moments still show (their
  previews are local) and say "the original isn't reachable" when played. Clip previews wait for the footage.
- **Finder tags on the NAS** are extended attributes, which an SMB share keeps when it supports streams (Samba's
  `vfs_fruit`/`streams_xattr`, which Synology and most NAS systems enable). UGREEN's isn't checked **(to try)**.
  Spotlight doesn't index network volumes, so tags are read from the files, never searched.

## Marks: the camera's, then a copy

A mark is a point in a clip: a camera highlight (DJI's quick switch, GoPro's HiLight) or one of your own.

- **While you've changed nothing**, the marks are the ones in the clip, read when it opens, as the highlights
  plugin does now.
- **The first change** writes `<clip>.marks.json` in the project, with every camera mark copied in. From then
  on, the file is the truth. A camera mark you remove is simply gone from it, with no list of hidden ones.
- **Delete the file**, and the clip's own highlights are the marks again.
- **A camera mark stays flagged** (`"camera"`), so the player can draw it differently from yours, and the editors
  get "DJI quick switch" as its note.

```json
{
  "format": "actionfootageplayer-marks",
  "version": 1,
  "clip": {
    "file": "DJI_20261022152841_0068_D.MP4",
    "size": 5123456789,
    "duration": 47.580867,
    "created": "2026-10-22T13:28:42Z"
  },
  "marks": [
    { "t": 10.010, "camera": "dji" },
    { "t": 33.400, "name": "Dolphin" }
  ]
}
```

- Times are seconds of the clip, to the millisecond.
- `clip` says which file the marks are for. A marks file whose clip now has another size is renamed
  `.marks.json.old` rather than applied to the wrong clip, which happens with GoPro's `GX010123` names, reused on
  every card.
- The file is written whole, to a temporary file renamed over the old one, and fields it doesn't know are kept.

**Gyroflow's project is never written to.** Gyroflow rebuilds the whole project when it saves
(`export_gyroflow_data` in `core/lib.rs` builds it with `json!` and `merge_json`), so keys of ours would be lost
at the next save. And its `trim_ranges_ms` aren't just a list: they change the smoothing, the sync and the
render. Moments go to Gyroflow only through an explicit command, into a project of their own ([Marks to the
editor](nle-markers.md#gyroflow)).

## Moments: a part as a small MP4

### The file

`Moments/<name>.afplink.mp4`, one per moment: a real, small MP4 that any OS can show and play, with the link to the
original inside.

- **The first version**: one frame, in color, held for the moment's length (`gyroflow-export --preview still`).
  960 pixels on the long side, H.264 by x264 at 10 fps with one keyframe: **150–350 KB** for a camera's frame,
  most of it that keyframe (a noisy underwater D-Log M frame is the 350). A test pattern made here first was 35 KB;
  a keyframe every 10 s, x264's default, made a 30-second one 1 MB, and VideoToolbox spends some KB on every
  repeated frame. Quick Look shows the picture with the moment's real length.
- **Later**: the moment itself, 540p–720p, 1–3 MB, with its sound. Quick Look and the phone then play the moment.
  The player still plays the original.
- **The icon** is the frame in the file's cover art (MP4 `covr`, written as an `attached_pic` stream). Without
  cover art, Quick Look picks its own frame. With it, Quick Look's thumbnail (`qlmanage -t`) is the cover, not
  the video: a red video with a lake as its cover got the lake. Windows Explorer shows MP4 cover art as the
  thumbnail too **(to try)**. ffmpeg writes the cover only from a picture it copies (`-c copy` of a JPEG), not
  one it encodes alongside the held frame (the video then stops after its first second), so the frame is made as
  a JPEG first.
- **H.264, 8-bit, SDR, `yuv420p`**: Explorer can't make thumbnails of HEVC without Microsoft's paid HEVC
  extension, phones and every app play H.264, and an SDR picture looks right in every icon.

### The link inside

A metadata key in the MP4, `ch.actionfootageplayer.link`, whose value is JSON:

```json
{
  "v": 1,
  "kind": "moment",
  "clip": "Day 1/DJI_20261022152841_0068_D.MP4",
  "size": 5123456789,
  "created": "2026-10-22T13:28:42Z",
  "in": 20.020,
  "out": 30.030,
  "name": "Turtle",
  "roots": ["/Volumes/video/2026-10 Egypt"]
}
```

- `kind`: `moment`, `clip` (a clip preview: the whole clip, no `in`/`out`) or `export` (with `look`, the
  look it was rendered in, and `stabilized`).
- `clip`: the original's path **relative to the sources root**, with `/` on every OS. It's not relative to the
  moment file, so moving a moment between subfolders changes nothing.
- `size` and `created` (the original's `creation_time`) say it's the right file, not another `GX010123.MP4`.
- `in`/`out`: seconds of the original, to the millisecond. They're put on frames when written out for an editor.
- `name`: the name it was saved with. The file's own name wins. A moment renamed in Finder is renamed.
- `roots`: the absolute sources roots when it was made, a hint for a moment file copied out of its project (to
  the Desktop, say) and opened there.

**Where it is in the file**: an `mdta` key in `moov/meta` (`hdlr` mdta, `keys`, `ilst`), with no version and
flags before its boxes: where and how Apple's own files keep `com.apple.quicktime.*` keys. Checked here:

- **AVFoundation** (Quick Look, Photos) reads mdta keys only there and in that form. ffmpeg's own
  `-movflags use_metadata_tags` puts them in `moov/udta/meta`, with version and flags, where AVFoundation takes
  them for iTunes items named 1, 2, 3; and with that flag ffmpeg leaves out the cover art. So gyroflow-export
  writes the file with ffmpeg (cover included) and then adds the keys itself (`vs-gyroflow/src/mp4meta.rs`):
  the `moov` grows, the media after it moves, and the tracks' chunk offsets with it.
- **mpv reads it** as `metadata/by-key/ch.actionfootageplayer.link` (printed intact, Cyrillic too), and ffprobe
  shows it as a format tag. The same goes for `com.apple.quicktime.creationdate`, written beside it.
- **The plugin's own reader** (`mpv/scripts/workspace/link.lua`) walks the boxes as `markers.lua` does and reads
  only the `meta` boxes, so it gets the key without mpv opening the file. It needs that to find a clip's moments
  ([below](#finding-a-clips-moments)). With `+faststart` the `moov` is near the start; where it's at the end
  (a large export), the reader seeks over `mdat`. It reads `moov/udta/meta` too, where a remux with
  `-map_metadata 0 -movflags use_metadata_tags` leaves it.

Not chosen:

- **A `uuid` box of our own**: ffmpeg can't write one while encoding (the player would append it afterwards),
  and mpv doesn't read it, so every read would need our parser.
- **`udta` text atoms** (`©cmt`): they're shown and edited by other apps as a comment.
- **XMP**: an XML packet to write and parse for five fields, and Adobe's apps rewrite it.
- **An extended attribute**: Explorer, FAT/exFAT and many copies drop it, while the bytes of a file go wherever
  the file goes.

**What it survives:**

| | The link |
|---|---|
| Copy, move or rename in Finder or Explorer, `cp`, `rsync`, copying to the NAS, AirDrop, iCloud Drive | kept: it's in the file's bytes |
| A Finder tag, a Finder comment | kept: those are attributes, not the file |
| Moving the moment within `Moments/`, or moving the whole project with the footage | kept and still right: the path is relative to the sources root |
| ffmpeg `-c copy` remux | **lost** (checked). Kept with `-map_metadata 0 -movflags use_metadata_tags`, in ffmpeg's place: the player reads it, Apple's apps no longer do |
| Explorer's star rating (Windows rewrites the `moov` to add its `Xtra` atom) | probably kept **(to try)** |
| Photos (Mac, iPhone) | the original file is kept as imported; an edited or shared copy loses it |
| A messenger, a re-encode, an editor's export | lost. It's then simply a video |

### Finding a clip's moments

- The player reads every `.mp4` under `Moments/`, subfolders included, for its key. A moment belongs to the
  clip whose relative path, size and `created` match.
- **A cache** in the app's cache folder (not in the project, so two computers on one NAS don't fight over it)
  keeps each file's key by path, size, modification time and change time. A scan then only lists the folders
  and stats the files. Even without the cache it's cheap: 300 moments × 16 KB is about 5 MB from the NAS.
- **The scan runs** when a clip of the project opens, and again when `Moments/`'s modification time changes,
  which a file added or removed changes. A rename in a subfolder doesn't change `Moments/`'s own time, so the
  player also lists the folders again every few seconds while a clip plays: a stat per folder.
- The clip's moments show on the seek bar as chapters ("Turtle" at its in, "Turtle end" at its out) with the
  marks, and in the menu.

### Playing a moment

Opening a moment's file (a double click, the player being the default app for MP4 on a Mac; or the menu):

1. An `on_load` hook reads the key before mpv opens the file.
2. It finds the original: the moment's project is the folder above `Moments/` (with `.actionfootageplayer/`
   in it), its sources root `Sources.txt`'s or the project folder itself, then `clip` under it, then the
   `roots` hints. Size and `created` must match.
3. Found: mpv opens the original instead (`stream-open-filename`), at `in` (`start`), with `in`–`out` as the
   A-B part. The title shows the moment's name. The playlist entry stays the moment, and the workspace module
   tells the other plugins the clip's real path. So stabilization, looks, highlights and telemetry are the
   original's.
4. Not found: the OSD says so ("Turtle: the original isn't at …/Day 1/DJI_…_D.MP4"), and the preview plays.
   The menu offers *Project → Footage moved…*.

A single moment loops, as an A-B part does now. **Opening the `Moments/` folder** plays them all, one after
another, each part of its original in full quality, sorted by the time of the moment (not by name). That's a
highlight reel of the trip, with no rendering.

### Naming

Saving a moment asks for its name, in mpv's own console (`mp.input.get`, mpv 0.38 and later; the app's mpv is
newer, Homebrew's is 0.41):

```
Moment name (Enter: Turtle 2): █
```

- **Enter** with nothing typed saves the suggestion; **typing** replaces it; **Esc** cancels and saves nothing.
  The console can't show a pre-filled text as selected, so that typing replaces it (it has a cursor and no
  selection; `default_text` and `cursor_position` are all it takes). The suggestion therefore sits in the
  prompt, with the field empty. **Tab** puts it in the field to edit, as a completion. **↑** brings back
  earlier names (`history_path`, kept in the project's state).
- **The suggestion**: the last name typed with the next free number ("Turtle" → "Turtle 2"; "Turtle 2" →
  "Turtle 3"). With no name typed yet, it's the moment's date and time by the camera's clock, `2026-10-22
  15.28.52` (colons aren't allowed in file names on Windows, and Finder shows them as slashes).
- **The file name is the name**: `Turtle.afplink.mp4`, or `Turtle 2.afplink.mp4` if it's taken (in that folder). Characters a
  file name can't have on Windows or macOS (`/ \ : * ? " < > |`, control characters) become `-`. Leading and
  trailing dots and spaces go, Windows' reserved names (`CON`, `NUL`, `COM1`…) get a `-` added, and names are
  kept under 200 bytes.
- **Typing in other alphabets** **(to try)**: on macOS mpv takes the keys itself. A Cyrillic layout gives
  Cyrillic letters, since each key is a character, but there's no input method: `--input-ime` is "Wayland and
  Windows only", and the manual lists "Non-ASCII keyboard input has restrictions" among the console's known
  issues. So the check is: Russian and Ukrainian layouts (`ё`, `ї`), Option-accents (`é`), German, a
  Chinese or Japanese input method (expected not to work on a Mac), and pasting with Cmd+V. If it falls
  short, there's a fallback on the Mac: a native dialog (`osascript … display dialog … default answer`). It has
  input methods and selects the suggestion for retyping, at the cost of a separate window that may pull a
  fullscreen player out of its Space. That would be a setting, not the default.
- An mpv older than 0.38 (Debian 12's 0.35) has no `mp.input`. The moment is then saved with the suggestion,
  and named in Finder.

### Dates

The moment's own time, the original's `creation_time` plus `in`, goes into the file three times:

- the MP4's `creation_time` (`mvhd`), which is what Finder's Info and most apps show;
- `com.apple.quicktime.creationdate` with the time zone, which Photos on the iPhone and the Mac use for the
  day. The zone comes from the clip where it can: a DJI clip's name is local time and its `creation_time` is
  UTC, so the difference is the trip's zone. Otherwise it's the computer's zone;
- the file's creation and modification dates. On a Mac, setting the modification time earlier than the
  creation time moves the creation time back too (checked: `touch -t` gave both). Windows sets creation time on
  its own.

So sorting by date in Finder or Explorer puts the moments in the order they happened, across days and
cameras, while the names stay yours. A GoPro whose clock was never set (`creation_time` 2016-01-01) sorts
right within the trip, on the wrong day.

### Rating and tags

- **A moment that exists is liked; one that isn't, isn't.** Deleting the file (in Finder, or *Remove* in the
  player, which moves it to the Trash) is the reject.
- **Finder tags** on a moment are kept in the file's attributes and never change the file. The player reads
  them (`xattr`), and the editors get them as keywords.
- **The favorite is a Finder tag named *Favorite*, in green.** `K` sets it and takes it off. A tag is a name
  plus a color, and Finder shows the color as the dot. The player counts as a favorite a tag with that name
  (in any color, in any case) or Finder's own *Green* tag, so tagging green by hand in Finder works too.
  *Favorite* is English because tags go to the editors as keywords, and the other names are the user's. The
  name is a setting: `favorite_tag=Favorite` in `workspace.conf` (Right click → Project → Favorite tag, written
  as the dlogm plugin writes its settings). It's read when deciding, so renaming it later doesn't need the files
  retagged, provided the old tag is renamed in Finder too.
- **Windows**: Explorer has no tags for an MP4, but its star rating (Properties → Details) is written *into*
  the file, as an `Xtra` atom. For our own small files that's fine, unlike the originals. 4 or 5 stars is a
  favorite (`favorite_stars=4`), and `K` sets 5. The player reads the atom with the same box reader. Writing it
  goes through the Windows property system (`System.Rating`), so that Explorer reads it back as its own **(to try)**.

### Editing

- **Moving its ends**: play the moment (from the menu or its file), move A or B with `l`, then `k`. The player
  asks "Update Turtle?" (Enter), or for a new name, which keeps both. The file is written again: the new
  preview, the key, the dates. On a Mac the Finder tags are copied over, and the new file is renamed over the
  old. On Windows the stars live in the file, so the writer copies the `Xtra` atom from the old file into the
  new one **(to try)**.
- **Renaming**: in Finder, or *Rename…* in the player's menu (the same dialog), which renames the file.
- **Removing**: in Finder, or *Remove* in the player (to the Trash, never deleted for good).

## Clips: the trip at a glance

`Clips/` has a small MP4 for each clip, built the same way as a moment's but showing the whole clip. It solves
"a folder of log footage is a wall of gray icons":

- **What's in it**: a flipbook. If the clip has marks, it shows the marked frames. Otherwise it shows about 12
  frames spread evenly through the clip, no closer than 10 s apart, skipping the first second. Each frame lasts
  0.5 s (the file plays at 2 fps). The cover art, and so the icon, is the first mark or the middle of the clip.
  At 960 px wide (`960×540`, or `960×720` for 4:3), 12 frames come to about 0.5–1 MB.
- **What it does**: the folder in icon view is the trip in color. In Quick Look (space bar), each clip flips
  through in 6 seconds, and the arrow keys go to the next clip. On Windows, Explorer's icons and its preview
  pane do the same.
- **The key**: `kind: "clip"`, the whole clip. A double click plays the original, all of it.
- **The tree**: `Clips/` repeats the footage's folders (`Clips/Day 1/…`), without camera folders such as
  `DCIM/100MEDIA`. The file has the clip's name and dates (its `creation_time`). Moving the files into folders
  of your own is fine, and the editors get those folders as bins.
- **Deleting one** says the clip isn't interesting: that's sorting the trip in Finder. Tags on them are the
  clip's rating and keywords, as for moments.
- **Deleted vs. not made yet**: `.actionfootageplayer/clips-made.txt` lists every clip a preview was made for
  (relative path and size). *Make clip previews* makes them only for clips not on the list, new footage that is,
  so a deleted one stays deleted. *Make again for removed clips* is the way back.
- **When**: from the menu (*Project → Make clip previews*), in the background, with the progress in the corner
  and the menu ("Clip previews: 34 of 120"). Never by itself on opening a clip: a card of 300 clips is minutes
  of work and a few hundred MB read from the NAS, and it would slow the playback it runs beside. New footage
  added to the project gets its previews the next time the menu item is used.

## Exports

`Exports/` in the project folder, instead of `stabilized/` next to the clip.

- **Kept apart from `Moments/`**: there may be several exports of one moment (HDR, SDR, ProRes), and exports
  are what gets deleted to make room. A moment is a few KB and stays.
- **Named in the same dialog** (`Shift+E`), with the suggestion being the moment's name when the A-B part is a
  moment, else as for a moment. The file: `Exports/<name>.mov`, with the look in parentheses unless it's the
  default HDR: `Turtle (SDR).mov`, `Turtle (Dolby Vision).mov`, `Turtle (ProRes).mov`, and ` 2` when taken.
- **The same link inside**: `kind: "export"`, with the look and whether it's stabilized. gyroflow-export writes
  it with the other metadata (ffmpeg's muxer, or `dv_mux.rs` for Dolby Vision). Opening an export plays the
  export, which is already stabilized and in its look, so the plugins leave it alone. The menu offers *Open
  the original*.
- **A moment comes with it**: exporting a part also saves it as the moment `export_<name>`, so every export
  can be found from its moment and played in full quality later, even after the export is deleted to make
  room. The exception is when the part *is* a moment, played from the menu or its file, or equal to one to the
  frame: then no second moment is made, and the export is named after it ([Decided](#decided)). A whole clip
  exported makes no moment; its preview in `Clips/` is that.

### From `stabilized/` to `Exports/`

- **New exports go to `Exports/`**, from the player, the Finder quick action and the command line. With no
  project yet, the first export makes one, as any save does.
- **Old `stabilized/` folders are left alone.** The player never reads them back: "Show the last export" is
  only the last one this session, and the skip-if-it-exists check looks where the new export would go. So
  nothing breaks, and nothing of anyone's is moved without asking.
- **The release notes and the README say so.** A script of yours that reads `stabilized/` passes `--out`.
- *Project → Move old exports here* (the `stabilized/` folders of the project's clips into `Exports/`) is
  for later, if someone asks.

## Making the previews

**gyroflow-export renders them**, with new options, because it already renders a clip the way the player shows
it. It uses the same Gyroflow engine, and the LUTs through ffmpeg's `lut3d` "in RGB, the same place mpv's shader
applies it". mpv itself was the other candidate, but its encoding mode (`--o`) doesn't run the GPU's shaders, so
there are no looks. And `screenshot window` needs a window on an awake display, which a background job can't
count on.

| | A moment (first version) | A moment (later) | A clip |
|---|---|---|---|
| Option | `--preview still --at <t>` | `--preview video` | `--preview flipbook --at <t1,t2,…>` |
| Frames | one, held for its length: the first mark inside the moment, else its middle | the moment, 540p–720p | ~12, at the marks or evenly |
| Stabilized | as it plays | as it plays | no: color only, and only keyframes are decoded |
| Size | ~35–100 KB | 1–3 MB | 0.5–1 MB |

All of them:

- **The look in SDR**: the player's SDR look for the clip, whichever one `export sdr` would pick (the dlogm
  plugin's `sdr_look`). A clip that isn't log keeps its colors. An HLG one goes through NBCU's HLG → SDR.
- **H.264 8-bit**: x264 for stills, which spends some 40 bytes on a repeated frame where VideoToolbox spends
  KB, then the hardware encoders, the first that encodes a test frame (VideoToolbox, NVENC, Quick Sync, AMF,
  Media Foundation). x264 is in Homebrew's ffmpeg, which the app carries, and in Gyan's build on Windows. For
  video previews, the hardware ones first, as for exports.
- **`--link <json>`** writes the key and `--dates <time>` the dates ([above](#dates)), an ISO 8601 time with its
  zone; the player works the zone out. A still's cover art is its own frame; `--cover <t>`, a cover from another
  time, comes with the flipbooks and video previews. gyroflow-export already gives its exports the source's
  dates, and `--output <file>` names the file.
- **Only keyframes for clips**: ffmpeg `-skip_frame nokey`, a seek to each time, and the keyframe nearest each
  time taken. Action cameras put one every half second to a second, so a frame is at most that far off.
- **The low-res copies** where they're next to the clip: DJI's `.LRF` and GoPro's `.LRV` (`GL010123.LRV` for
  `GX010123.MP4`) are small H.264 files with the same timing. They're quicker to decode and far less to read
  from the NAS. Whether a log clip's LRF is log too, so that the same LUT applies, is to check on real files
  **(to try)**.
- **In the background**, one at a time, with exports going first. They use the export's priority switching
  (macOS background priority while a clip plays), and progress shows in the corner.
- **Speed** (to measure): a still is gyroflow-export starting, reading the gyro data and decoding from a
  keyframe, a second or two. A flipbook is a dozen keyframe decodes plus the reads. A 4K HEVC keyframe is 1–2
  MB, so about 20 MB per clip from the NAS, or far less from an LRF.

## How it looks in the player

### Keys

| Key | | Free? |
|---|---|---|
| `l` | as now: the A-B part | — |
| `k` | **save the A-B part as a moment** (asks for its name). On a moment being played: update it | free in mpv and the plugins |
| `M` | **a mark here**; on a mark (within half a second): remove it. Final Cut's key for a marker | free (`m` is mute) |
| `X` | **this clip isn't interesting**: its preview in `Clips/` goes to the Trash; again: made again | free |
| `K` | **favorite**: the *Favorite* tag on the moment playing, else on the clip's preview (5 stars on Windows); again: off | free |
| `Shift+E` | export, asking for a name | as now |

### The menu

```
Mark start of part                 l
Mark end of part                   l
Save as moment 0:20–0:30…          k
Export part 0:20–0:30…             Shift+E
Moments (3)                      ▸  Turtle  0:20–0:30        ▸  Play
                                     Moray eel  0:41–0:44        Export…
                                     ───────                     Rename…
                                     Show in Finder              Remove
Marks (4)                        ▸  10.0 s  DJI quick switch
                                     33.4 s  Dolphin
                                     Add mark here      M
Project                          ▸  2026-10 Egypt (in the trip folder)
                                     Make clip previews
                                     Make again for removed clips
                                     Footage moved…
                                     Keep this project separately
                                     Use the folder above
                                     Show project folder
```

The panel's Clip section gets the moments and marks as lists, the same as the menu.

## The plan

Each step is a PR of its own and works without the next. The order follows "the project is the trip folder
first".

1. **The workspace plugin** (`mpv/scripts/workspace/`, **done**): `workspace.lua` finds the project of a clip
   (only in the trip folder for now) and the paths of its files; `main.lua` publishes them in
   `user-data/workspace/` before the other plugins load the clip (on_load, priority 10), since each mpv plugin
   runs on its own and the plugins also ship one by one. The gyroflow plugin's `find_project()` and Ctrl+e take
   the Gyroflow project's paths from there (next to the clip without the plugin, the same place for now), and
   its export passes the project it plays with (`--project`). gyroflow-export's own lookup is the Rust twin,
   `vs-gyroflow/src/workspace.rs`. Tests: `luajit tests/workspace_test.lua` and `cargo test` over the same cases,
   `tests/workspace_cases.txt` (nested trips, `DCIM`, the home folder, volumes, Windows paths). The clip's real
   path for the other plugins comes with moments (step 4), the first time it differs from mpv's `path`.
2. **The link** (**done**): `mpv/scripts/workspace/link.lua` reads the keys (`moov/meta`, then
   `moov/udta/meta`), `vs-gyroflow/src/mp4meta.rs` writes them for gyroflow-export. Tests:
   `luajit tests/link_test.lua` (boxes made in the test; ffmpeg's files copied, renamed, tagged with `xattr`,
   remuxed: lost, remuxed with `-map_metadata 0`: kept) and `cargo test` (the chunk offsets moved, moov first
   or last, `co64`, other `meta` boxes and the cover kept).
3. **Still previews** (**done**): `gyroflow-export --preview still`, `--at`, `--link`, `--dates`, `--output`.
   Tests: `GYROFLOW_EXPORT=<binary> luajit tests/link_test.lua` makes one from an ffmpeg clip and reads its keys
   back, in CI after the build. Checked by hand without a screen: clips 0093 (underwater, stabilized) and 0072
   (D-Log M above water) in the right colors; ffprobe: H.264 and the cover stream, the key and the date; `stat`:
   the file's dates, creation time included; `qlmanage -t`: the thumbnail is the cover; mpv reads both keys.
   Not checked: Photos' day from `com.apple.quicktime.creationdate` (AVFoundation's `creationDate` stays nil
   for an MP4), Explorer.
4. **Moments** (**done**, but the side panel's lists and typing in other alphabets): in the workspace plugin.
   `moments.lua` is the part with no mpv in it (names, dates, which clip a moment is of, where its original is,
   `Moments/` read; `luajit tests/moments_test.lua`), `main.lua` the rest:
   - `k` asks the name (`mp.input`), makes the project at the first save, and asks the gyroflow plugin for the
     preview (`script-message-to gyroflow preview`, which renders with the same options as an export, in SDR).
     The history of names (↑) is `.actionfootageplayer/moment-names.txt`.
   - `Moments/` is read when a clip of the project loads and again when one of its folders changes (looked at
     every 5 s); what's read of each file is cached in mpv's cache folder by size and time.
   - The clip's moments are chapters beside the highlights (put back when another plugin sets the chapters) and
     a *Moments* submenu: play, rename, remove (to the Trash: macOS's `trash`, Windows' recycle bin, `gio trash`),
     show in Finder.
   - A moment's file plays its original (`stream-open-filename`), looping its part, titled with its name; the
     other plugins read the clip from `user-data/workspace/clip` (gyroflow and its export, dlogm, the telemetry,
     highlights, the menu, the agent's notes). Opened on its own, mpv first makes the playlist of the files beside
     it (the app's `autocreate-playlist`), then loads it again from there, and only that load is redirected:
     redirected at once, the playlist became the original's folder. So the moments beside it are next and
     previous.
   - `Moments/` (or a folder in it) opened plays its moments one after another, each ending at its out, in the
     order they happened.
   - Updating (`k` on a moment whose part was moved) writes the file again; gyroflow-export keeps the Finder tags,
     label and comment of a file it replaces.
   Checked headless on clip 0093 (mpv `--vo=null` over the IPC socket): the first moment makes the project; the
   chapters; `k` and Enter saves the suggestion ("Лодка 2"); the history; the moment's file plays the original,
   stabilized, 20–30 s looped, titled; the update (out 30 → 31) and a Finder tag kept through it; rename by typing
   in the console; remove; the folder in time order. Not checked: typing other alphabets and input methods, the
   OSD and the console as they look, Windows and Linux.
5. **Exports to `Exports/`** (**done**):
   - `Shift+E` asks the workspace plugin for the name (`export-name`, answered with `export-start`), in the
     moments' dialog. The file is `Exports/<name>[ (SDR| Dolby Vision)].mov`, ` 2` when taken.
   - The part when it's a moment names it. A moment of the user's comes before one an earlier export made, whose
     `export_` is left out of the name. Otherwise the moment `export_<name>` is saved with it, quietly.
   - The link (`kind: "export"`, `look`: `hdr`, `maker`, `nbcu`, `dv` or `recorded`, `stabilized`) goes in with
     `--link`. gyroflow-export then reserves room after the `moov` (`-moov_size`, as for Dolby Vision, in place of
     `+faststart`), and `mp4meta.rs` writes the key into it in place: a 4K export isn't written twice.
   - gyroflow-export's default is `<project>/Exports/`, and it makes the project when there's none, as the
     player's first save does.
   - An export opened plays as it is. The menu's *Open the original* sets its part as the original loads: options
     given to `loadfile` are lost when mpv loads it again from the playlist of its folder.
   - The README says where old `stabilized/` folders stand.

   Checked headless on clip 0093:
   - a part named by date and time (Dolby Vision, the look playing) with its `export_` moment;
   - a moment's part named after it, in Dolby Vision and SDR, with no second moment;
   - an earlier export's part named without `export_`;
   - the links' look and range;
   - an export opened, then its original at 30–32 s;
   - gyroflow-export alone: the project made, the key written in place (the file's length the same).
6. **Marks**: the marks file on the first change, `M`, the camera flag, the highlights plugin reading the file
   when there is one.
7. **Clips**: `--preview flipbook` (keyframes, LRF/LRV), the background queue, `clips-made.txt`, the menu, `X`.
8. **Tags**: Finder tags and Explorer's stars read; `K`.
9. **Separate project folders**: `Sources.txt`, the list, the longest match, read-only folders, *Footage
   moved…*, *Keep this project separately*, the volume id.
10. **Video previews** of moments, in place of stills.

## Checking it

Automatic, with no screen: the tests in each step, on files ffmpeg makes, and `qlmanage -t` for Finder's
thumbnails.

By hand, each with Anton's go-ahead, since they take the screen or another computer:

| What | Where | What to look at |
|---|---|---|
| Typing a name | mpv on this Mac | Russian, Ukrainian, accents, German; an input method; pasting; Esc; Tab; ↑ |
| Icons and Quick Look | Finder, on a project on the NAS | the icons are the cover art; space bar plays the flipbook; sort by date; tags on the NAS |
| Icons, preview pane, stars | Explorer, on the Windows PC | cover art as the thumbnail; H.264 plays; stars written and read back; the key kept after stars |
| A moment on the phone | AirDrop to an iPhone, Photos | the day and time it shows |
| Footage moved | a card, then its copy on the NAS | the separate project follows with one line in `Sources.txt` |
| LRF/LRV colors | a DJI LRF and a GoPro LRV of log clips | the same LUT gives the same colors as the clip |

## Decided

Anton's answers to the first draft's questions, 2026-11-04:

1. **Keys**: `k`, `M`, `X`, `K` as above.
2. **The name dialog**: an empty field, with the suggestion in the prompt.
3. **A name not typed** is the date and time: `2026-10-22 15.28.52`.
4. **The favorite**: left to the design. It's the green *Favorite* tag (or Finder's *Green*), 4 stars and up
   on Windows, kept in the player's settings ([Rating and tags](#rating-and-tags)).
5. **The moment's icon**: the first mark inside it, else the middle frame.
6. **A new project**: in the footage's own folder when it can be written; otherwise in the player's folder in
   Documents, after asking for its name.
7. **The card's volume id**: left to the design. Written, for read-only cards ([`Sources.txt`](#sourcestxt)).
8. **Clip previews**: never by themselves, only from the menu.

Anton's answer to the second draft's question, 2026-11-05:

9. **Exporting a part that is already a moment** makes no second moment: the export is named after the moment
   (`Turtle.mov`), and its link points at the same range.

Anton's idea while it was built, 2026-11-05:

10. **Link files have a double extension**, `.afplink.mp4`: moments (`Moments/Turtle.afplink.mp4`) and clip
    previews. It says what the file is, and every system still opens it as an MP4. Exports are videos of their
    own and keep `.mov`. A link file renamed in Finder without it is still read (any `.mp4` under `Moments/`), and
    its name is shown without `.mp4`.

## Open questions

None left.
