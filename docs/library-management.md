# Library management: projects anywhere, footage on several disks, renders, Apple Photos

How a project keeps working while its footage moves between disks: from the camera's inbox to the archive, from the
archive to a fast working folder and back, without the app moving anything. How one project takes in another. Renders: a moment rendered once, then
played, exported and sent to the TV from that file. Albums in Apple Photos that follow folders of the library. This is
a design, not built yet.

Written 2026-11-09 from Anton's description of how he works (below). It builds on [Moments, clips and project
folders](moments-and-workspaces.md): the project folder, `Project.afproject`, the links inside moments, the source's
input and the transform bundles stay as they are there. What this page changes there is said where it does. Not
checked yet, marked **(to try)**: PhotoKit's edits of HDR video and Live Photos, network paths on UGOS shares, the
Trash on an SMB share, and Photos reading a title from a file.

Second version, the same day, after Anton's first comments: locations as network paths, the same for every computer
(no lists per computer); what a computer adds kept on it, outside the library; the app moves no footage (*Archive* and
*Work on* gone), *Find moved files* instead; *Take in* without moving footage when it's already inside. Third, after his
second round: a clip may be in several projects; what belongs to a clip lives beside it, one file per clip, and two
stop saving; `.afplocation` files lead from a clip to its projects; from a laptop to the NAS.

## The workflow it's for

Anton's NAS has an SSD (small, fast) and HDDs (large, slow).

1. **The inbox.** The camera uploads its clips by itself over Wi-Fi, whenever it's on power, into a folder on the SSD
   (`DJI_Inbox/DJI Device/OsmoAction6-4E8C/`). The SSD is small, so from time to time the clips move on to the
   **archive** on the HDD, where all the other footage is, from other cameras and other years. That archive wants
   organizing too.
2. **A video on a topic.** To cut a video in Resolve or Final Cut, the clips for it are copied to a **working folder**
   on the SSD, which reads faster than the HDD and doesn't make the HDD seek while it streams other clips.
3. **The project's own files on the SSD.** Marks, settings, moments, previews: small files, read and written often.
   On the SSD they don't make the HDD seek between reads of clips.

So the project must live in a folder of its own, chosen by hand, and know where its footage can be, in several
places, the fast ones first.

## In short

- **The library is its files, on the NAS, the same for whoever opens it.** Nothing in it depends on the computer
  that opens it. The safest project is next to its footage: one folder, moved whole, everything in it still works.
  A project on the SSD with its footage on the HDD is a small risk, manageable while both are on one NAS.
- **A project is a tree of footage** made of several **locations**: folders, each placed at a point of the project's
  tree (its root, or a subfolder such as `Action 6/`), in an order. A location is written once, as a network path
  (`//DISKSTATIONDXP/video/Footage`) or relative to the project folder. A clip is looked for in the locations in that
  order and taken from the first that has it, so a copy in the working folder on the SSD plays in place of the one on
  the HDD.
- **What belongs to a clip lives beside the clip**: its input (curve, turn, water), its marks, its own view, its
  Gyroflow project. One such file per clip in the whole library, wherever the clip is seen from. Two found: the player
  says so and saves nothing until one is gone.
- **Several projects can use the same footage.** Each location has a small visible file per project that uses it,
  pointing at that project, so a clip opened from Finder finds its project; the deepest one wins, and the panel can
  switch. Inside a project's `_Clips` or `_Moments` it's that project. What differs per project is its moments and
  clip previews; what belongs to the clip is shared.
- **Footage is moved by you**, in Finder or the NAS's file manager; the app doesn't manage moves. The locations are set
  up so that a clip moved from one to another (moved, with its files beside it, not copied) keeps its place in the
  tree, and then nothing needs updating. A clip that lost its place anyway is found again by its fingerprint: **Find
  moved files** searches the project's own locations and offers the new paths.
- **What one computer adds** (copies of footage on its own disk, its caches) is kept on that computer, in the
  player's own folder, and added to the project there. It's never part of the library.
- **Taking a project in**, once: when its footage is already inside this project's locations (copied there from a
  laptop, say), only its moments, clip previews, photos, exports and lists move, with their paths made relative to
  this project; otherwise its locations are added too, under a subfolder you choose. The project taken goes to the
  Trash.
- **Renders** are the rendered files of moments, whole clips and photos, made only when asked (*Render* on a
  selection, or a folder that syncs with Photos). Each knows exactly what it was made from: its **recipe**. While the
  recipe matches what would play, the player plays the render instead of rendering the original live, an export
  copies it, and the TV gets it as it is. Change a setting and the player goes back to the original. *Render again*
  makes only what changed. Deleting renders breaks nothing.
- **Apple Photos**: a folder of the library, chosen to sync, is an album. New items are added, changed ones are
  updated, deleted ones are deleted in Photos too (Photos keeps them 30 days). Photos gets the renders.

## 1. A project and its locations

### The project's tree

The project sees one tree of footage. Every path the project keeps (a link in a moment, a clip preview, the lists in
`Project.afproject`) is a path in that tree, with `/`. Locations make up the tree:

```
Project "Footage", its folder //DISKSTATIONDXP/ssd/Projects/Footage

  In the tree       comes from (in this order)
  /                 1. Working   //DISKSTATIONDXP/ssd/Work      copies only               SSD
  /                 2. Archive   //DISKSTATIONDXP/video/Footage                           HDD
  Action 6/         3. Inbox     //DISKSTATIONDXP/DJI_Inbox/DJI Device/OsmoAction6-4E8C   SSD
```

- A location is a folder and **where it's placed in the tree**: the root, or any subfolder. The Inbox above shows its
  `DJI_20261022152841_0068_D.MP4` as `Action 6/DJI_20261022152841_0068_D.MP4`.
- **The order is the priority.** A path in the tree is looked for in Working, then Archive, then Inbox, and taken from
  the first that has it, the same file (size and date, and the fingerprint when known). That's what a working copy
  is: the same path in a faster location.
- **Copies only**: a location marked so (Working) holds copies of clips that are somewhere else in the project. Nothing
  is ever written into it ([The clip's own file](#the-clips-own-file)).
- Locations may overlap (Working and Archive both at the root) or not.
- **A location that isn't reachable** is skipped. Moments still show (their previews are in the project folder), and
  say "the original isn't reachable" when played.

### Where a location is

Written once, the same for every computer:

- **Relative to the project folder**: `.` (the project folder itself: a project next to its footage), `Footage`,
  `../Footage`. It moves with the project folder. The first version's projects (in their trip folders) are this: one
  location, `.`, at the root.
- **A network path**: `//DISKSTATIONDXP/video/Footage`, the NAS, the share, the folder in it. Each system reads it
  its own way:
  - Windows takes it as it is (`\\DISKSTATIONDXP\video\Footage`).
  - macOS: the share's mount, from the system's list of mounted volumes (`/Volumes/video`), however it was mounted
    (`antonadmin@DISKSTATIONDXP._smb._tcp.local/video`, by the IP, by another name the NAS answers to). Not mounted:
    the player mounts it (`smb://DISKSTATIONDXP/video`; macOS asks for the password once and keeps it in the
    Keychain). The project can list the NAS's other names (`web.nas`, its IP) so that any of them matches.
  - The NAS itself (afp-stream): the share's own folder (`/volume2/video`), from afp-stream's settings.
  - **(to try)**: UGOS shares as macOS and Windows name them; a share mounted twice under two names.
- No absolute local path (`/Volumes/…`, `Z:\`): it would hold for one computer only.

`Sources.txt` (the design's separate project, never built) is replaced by this list in `Project.afproject`.

### How a clip finds its project

A clip opened from Finder has to find its project without anything the computer remembers, since a project on the SSD
isn't above its footage. So each location has, at its top, a small visible file per project that uses it:
`Footage.afplocation` (JSON: the project's network path, its `id`, the location's place in its tree). The Library
writes it when a location is added, and removes it when the location is.

1. From the clip's folder up to the top of its volume, the player gathers the `.afplocation` files and the
   `Project.afproject` files (a project next to its footage is found by its own file).
2. The deepest one is the clip's project.
3. Several at the same depth (one footage, several projects): the one this computer last chose for it; the side
   panel's Project part shows which and switches ("Project: Footage ▾"). That choice is this computer's convenience,
   not the library's.
4. A location that can't be written (a locked card): no file there; the player then goes by the projects this computer
   has opened.

A clip opened from a project's `_Clips` or `_Moments` is in that project, whatever the files above it say: the link
file is in the project's own folder.

### Several projects on the same footage

The same locations can be in several projects, at different places in their trees. Then:

- **Shared**, because it belongs to the clip and lives beside it: its input, marks, own view (the source's bundle), its
  Gyroflow project.
- **The project's own**: its moments, clip previews, photos, exports, renders, lists. A whole clip shown differently
  in one project is a clip preview with settings of its own there (level 4 of the bundles), as now.
- The Library writes a `.afplocation` per project into each location, so every project can be found from the clip.

### What one computer adds

A computer can have more of a project than the library: a copy of some clips on its own disk, to cut them without the
network; the player's caches (motion data, probes, GPU tables). That's kept on the computer, in the player's own
folder (`~/Library/Application Support/ActionFootagePlayer/projects/<project id>.json`; Windows: `%APPDATA%`), never in
the library:

- **Local locations**, placed in the tree like the library's and looked at before them, always copies only.
- Found by the project's `id`, so it follows the project wherever it's opened from on this computer.
- Delete it, and the computer simply reads the library. Another computer never sees it. *Find moved files* and *Take
  in* never write it.

### Finding a clip

The same in the player (Lua, `workspace.lua`), gyroflow-export and the Library (Rust, `workspace.rs`), with one set of
test cases:

1. **From a path in the tree to a file**: this computer's local locations, then the library's, in order: for each one
   whose place is above the path, the path under it. The first that exists, with the expected size (and date, as
   links have them), is it.
2. **From a file to the tree** (a clip opened from Finder): its project ([above](#how-a-clip-finds-its-project)), the
   location that holds the file, and the clip's path in the tree is the location's place plus the path under it.
   Opening the working copy and opening the archive's gives the same clip, the same marks, the same moments.
3. **Not found**: the player says where it looked, plays the moment's preview, and offers *Find moved files*.

### The clip's own file

What belongs to a clip is beside it, in the location where the clip really is: `<clip>.afp.json` (input, marks, its
own view) and `<clip>.gyroflow`, as now for a project next to its footage, and as Gyroflow itself saves. Not in the
project folder, so every project that sees the clip sees the same.

**One per clip, in the whole library.** The player looks for it beside the clip at the same path in every location of
the project (the copies-only ones too):

- **None**: the clip's own marks and the defaults. The first save writes it beside the clip in the first location
  that has the clip and isn't copies only.
- **One**: it's the clip's. A save writes there, even while the clip plays from a working copy.
- **Two or more** (a clip copied with its file instead of moved, a file copied into Working): the player plays with
  the first one's values, saves **nothing** for that clip, and says where they are: "Two files of this clip:
  Inbox/…/0068_D.afp.json and Archive/…/0068_D.afp.json. Keep one." Saving comes back once one is gone. No merging,
  no guessing which is newer.

The same goes for the Gyroflow project beside it.

**Check the project** (Library) lists, over all locations: clips with two files, files with no clip beside them (left
behind by a move: *Find moved files* puts them back, by the clip's fingerprint they carry, `clip.hash`), and the same
clip at two places in the tree (below).

Names starting with `_` at the project's root are the project's own (`_Moments`, `_Clips`, `_Stills`, `_Exports`,
`_Renders`): a location can't be placed there.

### The file

`Project.afproject` gets `id` and `locations` (`mpv/scripts/workspace/projectfile.lua`):

```json
{
  "format": "actionfootage-project", "version": 2,
  "id": "6f1c2e4a-…",
  "locations": [
    { "name": "Working", "at": "", "path": "//DISKSTATIONDXP/ssd/Work", "copies": true },
    { "name": "Archive", "at": "", "path": "//DISKSTATIONDXP/video/Footage" },
    { "name": "Inbox", "at": "Action 6", "path": "//DISKSTATIONDXP/DJI_Inbox/DJI Device/OsmoAction6-4E8C" }
  ],
  "hosts": { "DISKSTATIONDXP": ["web.nas", "192.168.50.186"] },
  "...": "settings, clips_made, stills_made, moment_names as now"
}
```

- `at`: the place in the tree, `""` for the root. `path`: relative to the project folder, or a network path.
  `copies`: copies only.
- A project without `locations` is a project next to its footage: `{ "at": "", "path": "." }`. Version 1 files read as
  that.

### Moving a project's own files to the SSD

The trip projects made so far are next to their footage (on the NAS: `DJI_Inbox/DJI Device/OsmoAction6-4E8C/` has
`_Moments`, `_Stills`, `Project.afproject`). *Move the project to…* (Library):

1. Asks for the new folder (on the SSD).
2. Moves `Project.afproject`, `_Moments`, `_Clips`, `_Stills`, `_Exports`, `_Renders` there. The clips' own files stay
   beside the clips. The old folder becomes a location at the root, by its network path, with a `.afplocation` in it.
3. The links need no change: paths in the tree are the same.

## 2. Files moved between locations

### Set up so that nothing needs updating

The app doesn't move footage: you do, in Finder or the NAS's file manager. Locations are there so that a move between
them doesn't change a clip's place in the tree. With the project above:

- **The inbox to the archive: move, don't copy.** The Inbox is placed at `Action 6/`, and the Archive at the root has an
  `Action 6/` folder. Clips moved from the inbox into `video/Footage/Action 6/`, **with their `.afp.json` and
  `.gyroflow`**, keep their path in the tree, `Action 6/DJI_…_0068_D.MP4`: their moments, marks and settings don't
  notice. Copied instead, each clip with a file of its own is then in two places with two files, and the player
  stops saving it until one copy goes ([The clip's own file](#the-clips-own-file)).
- **A working copy: copy the clip alone.** Copy `Action 6/DJI_…_0068_D.MP4` into `ssd/Work/Action 6/`: Working comes
  first, so the player, exports and *Marks to the editor* ([nle-markers.md](nle-markers.md)) take the SSD's copy, while
  its marks and settings stay beside the archive's. Delete it when the video is done, and the archive's plays again.
- The Library shows, for each location, where it is in the tree, so the folder to move a clip into is plain.

### Find moved files

A move that changes a clip's place in the tree (sorting the archive into `Action 6/2026-10 Egypt/`, a rename, a
location set up differently): what pointed at it no longer finds it. **Find moved files** (Library; and the player,
when a clip isn't found):

1. Gathers every path the project keeps that finds no file: the links in moments, clip previews, exports and renders,
   the lists in `Project.afproject`; and the clips' own files with no clip beside them.
2. Looks for them in the project's own locations, nowhere else: files of the same size first, then the fingerprint
   (`hash`, already in every link and in every clip's own file: SHA-256 of the first and last MiB, 2 MiB read per
   candidate).
3. Shows what it found: the old path, the new one. A clip found twice at different places in the tree is shown with
   both, to choose.
4. On yes, updates them: the links rewritten in place (the MP4's `moov` keeps its size), the lines in
   `Project.afproject`, a clip's own files moved beside it.

What it doesn't find stays as it is, listed as missing. A cache on this computer (each file's path, size, time and
fingerprint) makes the next search fast; it's this computer's, not the library's.

### The same clip twice

The same footage at two different places in the tree is not a working copy: it's a duplicate (the camera uploaded a
clip to the inbox that was also copied into the archive from a laptop, say). Moments may then point at either.
*Check the project* and *Take in* list such clips, by fingerprint, with where each is; you delete the one you don't
want, and *Find moved files* moves what pointed at it to the other.

## 3. Taking one project into another

**Take in…** (Library, in the project that takes): choose the other project's `Project.afproject`. Each of its
locations is one of two cases:

- **Already inside this project's tree**: its folder is under one of this project's locations (a trip project next to
  its footage, copied into the archive, say: `video/Footage/Action 6/2026-10 Egypt`). Nothing is added and nothing moves
  on disk. Its place is where its folder already is in this tree: `Action 6/2026-10 Egypt/`.
- **Outside it**: its location is added to this project's, after them, at a place you choose (`2026/Egypt/`), with a
  `.afplocation` for this project.

Then, in both cases:

1. The clips' own files stay where they are, beside the clips: they're the clips', not the project's.
2. Its `_Moments`, `_Clips`, `_Stills`, `_Exports`, `_Renders` go into this project folder, to `_Moments/<its place>/`
   and so on (a subfolder is a bin, as before).
3. Every path it kept gets its place in front: the `clip` in every link, the lines in its `Project.afproject` (clips
   made, stills made, failed), the recipes of its renders.
4. Its `Project.afproject`, its `.afplocation` files and its emptied `_` folders go to the Trash. Never footage, even
   when its project folder was its footage's folder. On the NAS a share's deleted files go to its `#recycle` folder
   **(to try)**: Finder's Trash on an SMB share, which may delete at once instead.

Before it starts, the Library lists what will move, and what it found wrong: clips that would have two own files,
clips that are already elsewhere in this project ([The same clip twice](#the-same-clip-twice)). It starts only when
you say so. Two moments with the same name in the same place: the second gets ` 2`.

### From a laptop to the NAS

On a trip: the camera's clips copied to the laptop, a project next to them (`~/Movies/2026-10 Egypt/`, with
`Project.afproject`, `_Moments`, the clips' own files beside the clips). At home:

1. **Copy the whole folder** into the archive, where it's to be in the tree: `video/Footage/Action 6/2026-10 Egypt/`.
   Everything comes along: clips, their own files, the project's folders.
2. **Take in** (Library, in "Footage"): choose the copied `Project.afproject`. Its one location (`.`) is inside the
   Archive, so nothing is added: its moments, clip previews and photos move into Footage's `_Moments/Action 6/2026-10
   Egypt/` and so on, their links get `Action 6/2026-10 Egypt/` in front, its `Project.afproject` goes to the Trash.
3. **Watch for the inbox**: the Action 6 uploads to the inbox by itself as soon as it's on power at home, so the
   trip's clips may be there too. *Take in* lists them; delete the inbox's copies (they have no files of their own,
   unless something was done with them at home).
4. The laptop's folder is now an old copy: nothing points at it. Delete it once the NAS has a backup.

The Gyroflow projects keep the laptop's path of their video inside; the player never reads it (it finds them beside
the clip), and Gyroflow asks for the video the first time it opens one there.

A one-step version (*Bring in from this computer*: the Library copies the folder, checks it, then takes it in) could
come later; it's the only place the app would copy footage.

## 4. Renders

### What a render is

The rendered file of an **item**: a moment, a whole clip, or a photo at a mark. Made with a **render profile** (what
kind of file): `HDR` (HEVC, HLG with Dolby Vision 8.4, as iPhone records, 4K), `SDR` (HEVC 8-bit), `TV` (what afp-stream
sends the Samsung: PQ, gain 2.5). Photos: HEIC with a gain map, Live Photo, as `Alt+s` makes them.

Its **recipe** is everything that decides its pixels and its sound:

- the original: fingerprint, size, `in`/`out` (a photo: its time, and the crop's camera);
- the source's input (curve, turn, under water), the transform bundle it plays with, the Gyroflow project's content
  (its copy, for a moment), the 360 view if any;
- the profile: look, size, codec, bitrate, sound;
- the renderer's **render version**, a number raised only when a change in the engine or the LUTs changes the
  picture on purpose.

The recipe, written out as JSON with its keys sorted, gives a short hash. Both are in the render's link
(`ch.actionfootageplayer.link`, `kind: "render"`), so a render file says by itself what it is.

### Where

`_Renders/`, mirroring the item: `_Moments/Reef/Moray eel.afplink.mp4` → `_Renders/Reef/Moray eel.HDR.mov`; a whole
clip → `_Renders/_Clips/2026/Egypt/Day 1/DJI_0068_D.HDR.mov`; a photo → `_Renders/_Stills/…`. One file per item and
profile. The renders of a 30 s moment take 100–200 MB in HDR. `_Renders/` is in the project folder, on the SSD when
the project is (Anton: keep it simple; another place can be added later).

They are a cache: delete one, a folder, or all of `_Renders/`, and that's all it is: next time it's needed, it's
made again or the original plays.

### Making them

Nothing is rendered by default. **Render** (Library, and the player's menu for what's playing) on a selection of
moments, clips, photos or folders, with the profiles ticked (`HDR`, `SDR`, `TV`: any of them, each a file
of its own). A render exists and is **current** when its recipe is the item's
recipe now; **stale** when not (the moment's transform changed, its Gyroflow project, the source's input).

- **Render** makes the missing and the stale ones, skips the current ones.
- The Library shows, per folder, how many are current, stale, missing, and the room they take. **Delete stale
  renders**, **Delete renders** (all, or of a folder).
- A stale render is kept until the new one replaces it (it still plays nowhere: [below](#playing-from-a-render)).
- Folders that sync with Photos keep their items rendered by themselves ([§5](#5-apple-photos)).

### Playing from a render

Opening a moment (or a clip) whose current render exists in the profile that matches the screen's output (the HDR
render while an HLG look plays on an HDR screen; the SDR one while an SDR look plays): the player plays the render
file. Smooth on any computer, no stabilization work, no GPU warp, no denoise. The marks, telemetry and chapters stay
the item's (read from the original when it's reachable).

As soon as anything that's in the recipe changes while it plays (a transform setting, the look, the crop camera,
`Ctrl+p` clean still frame, a photo): the player switches to the original at the same time and renders it live, as
now. *Revert to saved* comes back to the render. The panel says which plays ("From the render, made 9 Nov").

Not from a render: the 360 view while you look around (only its saved view is in a render), and an output whose look
no render has.

### Exports from renders

`Shift+E` and the Library's *Export all moments*: when the export asks for what a current render is (the same look,
size, codec, the whole moment), the render is copied into `_Exports/` (on APFS a clone, on the NAS a server-side copy)
instead of rendering. The export keeps its own link, `kind: "export"`, as now.

### The TV and Jellyfin

afp-stream (on the NAS) asks the player how each file plays. With a current render, it streams the render: no
stabilization on the N100, which manages it at ~17 fps at best. With a `TV` render it sends that file as it is.
The stream's openings (its first segments, for a fast start) are not renders: they're *Fast opening*'s
([§6](#6-the-nas-prepares-clips-to-open-fast)).

### Photos at marks

`_Stills/` photos are renders already, made once. They get a recipe too (written into the HEIC/JPEG's XMP), so that a
changed transform shows them stale and *Render* remakes them, and so that Apple Photos gets the new one.

## 5. Apple Photos

### What syncs

In the Library, a folder of the project (`_Moments/Reef`, `_Stills`, `_Exports/For family`) can be set to **sync with
Apple Photos**, into an album named after it, inside a Photos folder named after the project (`Action Footage ›
Footage › Reef`). Subfolders follow: a folder with subfolders is a Photos folder, each subfolder an album in it (an album in Photos
holds no albums), and the folder's own items an album of its name.

What Photos gets is each item's render in the `HDR` profile (what iPhone records, so it plays HDR on every Apple
device, and SDR where it must) and the photos as they are: HEIC with gain map, Live Photos as Live Photos. The asset's
date is the moment's real time (the clip's start plus `in`), its place the GPS at that time when the clip has one,
favorite when the moment is.

### What sync does

Sync runs on one Mac (the one where the Photos library is; a project remembers which Mac syncs it, others show "synced
by …"), when the Library opens the project and after renders it made, and by *Sync now*.

- **New** in a synced folder: rendered if needed, then added to Photos (PhotoKit) and to the album.
- **Changed** (its recipe changed: its transform, the source's input): rendered again, then updated in Photos as an
  **edit** of the same asset (PhotoKit's content editing): Photos shows the new version, the asset keeps its place,
  favorite, other albums, comments, and Photos' *Revert* goes back to the first. Where PhotoKit doesn't take an edit
  (HDR video, Live Photo **(to try)**), the asset is replaced: deleted and added again.
- **Moved** out of synced folders, still in the project: taken out of the album, left in Photos.
- **Deleted** from the project: deleted in Photos. Photos keeps it 30 days in Recently Deleted. macOS asks before
  any app deletes photos (its own dialog, not ours); sync asks once for all the deletions it found.
- **Deleted in Photos by you**: the item gets the tag *Deleted in Photos* ([§7](#7-tags)) and isn't added again.
  Take the tag off, and the next sync adds it.

The Library keeps the pairs (item ↔ Photos asset, the recipe sent) in `Project.afproject`, under `photos`, with
Photos' cloud identifier, which is the same on every device of the iCloud library.

### What PhotoKit allows

- Adding: photos, videos, Live Photo pairs, into albums; the date, place and favorite. A title and a caption, not
  through PhotoKit: Photos may read a title written in the file **(to try)**.
- Deleting only with the user's yes, by the system's dialog.
- An app needs the Photos permission (asked once). The Library's app has it; a helper it runs is covered by it **(to
  try)**. Photos sync is macOS only.

## 6. The NAS prepares clips to open fast

Anton, 2026-11-09: from the Library, ask the NAS to prepare chosen folders so their clips open at once. Two different
things open a clip, and what makes each slow differs.

### The command

**Fast opening** (Library), apart from *Render*: select folders or clips, tick what for (**TV**, **iPhone**, **Player**),
start. The NAS takes it as a job of the queue: the stream's openings for TV and iPhone (the first segments, in the
profile each gets), the opening data for the player. A folder can be **kept ready**: its new clips get the same by
themselves. Carefully: that work runs only while nothing else is (nobody watching, no render, no copy in the NAS's
own jobs), at low priority, one clip at a time. All of it within a **limit, 50 GB** by default: past it, the openings
of the folders used longest ago are dropped first.

### The TV, the iPhone (Jellyfin, afp-stream)

afp-stream renders a stream as it's asked for: the first segment comes ~1 s after the render starts, plus Jellyfin's
own start. It **already** pre-renders openings (since 0.10): `GET /prerender?folder=<f>&what=opening|whole` queues a
folder's clips, and the NAS renders the first `AFP_OPENING` seconds (8) of each, or all of it, one clip at a time,
only while nobody watches. A played segment is kept (`AFP_CACHE_GB`, 40 GB, the least recently played go first), and
its key is what the clip plays with (`fingerprint()`: the player's settings, the Gyroflow project, the service's own),
so a change renders it again: the same idea as a render's recipe ([§4](#4-renders)).

What's missing, for the Library:

- **Folders kept ready** (`Prepare on the NAS`: opening / whole / off), kept in `Project.afproject`. afp-stream reads
  them and prepares new clips by itself, so a clip the camera uploads into the inbox at night has its opening ready
  in the morning.
- **Kept, not evicted**: the openings of those folders don't count against the cache's limit (or have their own), so
  watching long clips doesn't throw them out.
- **Status in the Library**: per folder, how many are ready, stale, waiting; what renders now (`/status`), and the room
  they take. An opening at 30 Mbit/s is ~30 MB: 1000 clips are ~30 GB, on the SSD.
- **The project's locations in afp-stream**: today it sees only the inbox (`AFP_LIBRARY=/clips`, mounted read-only).
  It needs the archive and the project folder (read-only), and its settings say which share is which of its folders
  ([Where a location is](#where-a-location-is)); the container is made again for that (a Docker grant).
- The Library talks to afp-stream over HTTP on the home network, as Jellyfin does. It has no password: home network
  only.

### The player on a Mac or PC

The player renders live on the GPU and opens a clip in ~0.8–1.2 s. A pre-rendered opening doesn't fit it: playing a
file, then handing over to the live render a few seconds in, shows as a jump in mpv, and any setting changed in those
seconds misses it. What's slow here is different: on a clip's first open, the motion data (the gyro samples, spread
through the whole file) is read over the network, ~90 % of a cold open before 0.10, and it keeps the HDD seeking
between those samples and the video. After that the player has it in its own cache
(`~/Library/Caches/ActionFootagePlayer/{motion,probe,gpu}`), keyed by the file's path, size and time on this computer.

So what the NAS should prepare for the player is that data, not video:

- **Opening data** for each clip of the folders kept ready: the motion data read, its probe (`gyroflow-probe`), and
  the GPU table where the settings are known. Read on the NAS from its own disks, at night, cheap for the N100 (no
  video: about a second per clip).
- **Kept in the project folder**, `_Cache/` on the SSD, keyed by the clip's fingerprint and size instead of its path,
  so the Mac, the PC and the NAS share it, and a working copy and the archive's use the same. The player looks there
  before its own cache. Deleting `_Cache/` breaks nothing: like `_Renders/`, it's made from the library, not part of
  what the library is ([Q13](#open-questions)). The player's own cache stays on each computer.
- The first open of any clip is then as fast as a second one, from any computer, and the HDD isn't asked for the
  samples while it plays.

**(to try)** before building: a cold and a warm open of the same clip from the NAS, timed headless, to know what's
left to win.

## 7. Tags

Words on items (clips, moments, clip previews, photos, exports), for searching, filtering, and for the commands.
Anton, 2026-11-09: time to have them; the first use is Photos'.

- **Kept with the item, in the library**: a clip's in its own file (`.afp.json`, `tags`); a moment's, clip preview's
  or export's in its link (`tags`); a photo's in its XMP (`dc:subject`, which Photos and Lightroom read as keywords).
  So they travel with the files and are the same from every computer. Finder tags are written alongside where the
  share keeps them, to be seen in Finder; the library's own are the truth.
- **Yours**: any words (`dolphin`, `for family`, `best`). Set and removed in the Library's Tags part, on a selection.
- **The app's**: *Deleted in Photos* (sync sets it), later others; shown apart, removable like any.
- Search and filters take tags; *Marks to the editor* passes them on as keywords.

## Steps

Each a PR, each usable on its own:

1. **Locations** in `Project.afproject` (network paths, relative paths), finding a clip (Lua and Rust, one set of
   tests), `.afplocation` files and the project switch, the clip's own file found over all locations (two: nothing
   saved), this computer's additions, *Move the project to…* for the trip projects there are. Locations edited in the
   Library, each shown with its place in the tree.
2. **Find moved files** and **Check the project**.
3. **Take in**.
4. **Renders**: recipes, `_Renders/`, *Render* in the Library and the player's menu, current/stale, playing from a
   render, exports from renders.
5. **Apple Photos**.
6. afp-stream streams renders.
7. **Prepare on the NAS**: folders kept ready in the Library, afp-stream preparing their openings by itself, kept out
   of the cache's limit, status; then the opening data in `_Cache/` for the player. The first part needs nothing from
   1–6 (a folder of the inbox to start with).

The order (Q9, the order of building): locations first, they're under everything else. 4 needs nothing from 1–3. 5 needs 4.

## Decided

Anton, 2026-11-09:

- The library is self-contained on the NAS, the same whoever opens it and from wherever: locations are network paths
  (or relative to the project folder), never a list per computer. The safest project is next to its footage.
- What a computer has of its own (local copies, caches) is kept on that computer, in the player's folder, as an
  addition the player reads; never part of the library.
- Files are the library: the app doesn't move footage between locations. Locations are set up so a move between them
  keeps a file's place in the tree; *Find moved files* finds the others by fingerprint in the library's own locations
  and offers to update their paths. (Q2–Q4 of the first version, about Archive and Work on, are gone with them.)
- *Take in*: a project already inside this one's locations moves no files and adds no locations, only its
  descriptions, with paths made relative to this project; one outside adds its locations. The project taken goes to
  the Trash.
- **Any number of projects** (Q1): the product keeps several projects side by side (for clients whose files must
  never mix, say), each with its own locations; how to split is each user's choice. Anton's own footage is personal,
  so he keeps one project, "Footage", with trips and topics as its folders: for him the project is the library. The
  design, the tests and the Library's UI cover both.
- **A clip in several projects** is allowed: a clip opened from Finder goes to the deepest project (a file per project
  at the top of each location points at it, since a project apart from its footage can't be found otherwise); inside a
  project's `_Clips` or `_Moments` it's that project.
- **What belongs to a clip lives beside it** (input, marks, its own view, its Gyroflow project), shared by every
  project. The project checks that each clip has at most one such file: two (a clip copied instead of moved) and the
  player saves nothing for it until one is gone. Between the inbox and the archive, clips are moved, not copied.
- **From a laptop**: copy the trip's folder into the archive, then *Take in* (confirmed by Anton as the way).

- **Renders** (Q5, Q13): in the project folder (`_Renders/`, `_Cache/`), on the SSD with it; other places later.
  Profiles are ticked per run (Q6): `HDR`, `SDR`, `TV`, any of them. Where a render is current, it plays, always.
- **Albums** (Q7) follow subfolders. A changed item is an edit of its asset (Q8). Deleted in Photos: tagged, not added
  again until the tag is off ([§7](#7-tags)).
- **Fast opening** (Q11, Q12) is its own command, apart from renders, with TV, iPhone, Player ticked per run; folders
  can be kept ready, carefully, only while the NAS is idle; 50 GB by default, the oldest dropped first.
- **The Library** is where all of it is done: browsing like folders, with search and filters, clips opening in the
  player's window; every command in one side panel of parts, each with its settings, run on the selection; the queue
  of jobs in a view of its own. Mockup: https://claude.ai/artifact/RjUByTPS366nD5x2LLyf7p. After its first round
  (Anton): a click selects a folder too, and commands then act on everything inside it; a double click opens it; a
  folder's icon is a grid of four of its clips; filters by **Source** (camera, log curve or HDR or SDR, motion data,
  360°) and by **Transform** (stabilized, Gyroflow project, horizon, frame, projection, turned); checks (*Check*,
  *Find moved files*) are commands on the selection, since a whole big project takes long; the project's own menu
  (by its name) holds the projects, *Project settings…* (locations edited: order, place in the tree, path, copies
  only; what this computer keeps for it: local copies, caches with their size and a Clear, its own choices; general:
  name, id, the NAS's other names), *Take in…* and *Move the project to…*. Commands act only on what's selected: with nothing selected they're greyed out (their settings can still be set); *Select all*, *Select none*, ⌘A and Esc are at hand. Filters (a panel of nested lists with counts, and tokens in the search field) work two ways: while browsing, they narrow the folder you're in, in place; a search (words typed, or *List every match*) is a flat list, in this folder or everywhere. Filters with nothing to show are hidden. After Anton's first try (2026-11-09): Footage shows the clips as arranged in _Clips (each in its preview's folder; the
  ones without a preview in a folder of their own), and **Sources** is a tab of its own: the project's locations as
  folders, each explored as it is on its disk, added and removed there; nothing in it is renamed. The list follows the
  system's keys (arrows, Shift; a Mac's Return renames, ⌘↓ opens, ⌘↑ goes up, space is Quick Look; Windows' Enter opens,
  F2 renames, Alt+↑ goes up). A clip or moment with a Gyroflow project has Open in Gyroflow. What the Library last saw of
  a project shows at once while it looks again. The player opens without its side panel (Tab shows it), and a video
  opened in a paused player plays.
- **The list's own menu and Info** (Anton's second round, 2026-11-09). A right-click holds the summary's commands and
  Finder's around them: Open, Open in Gyroflow, Quick Look, Show in Finder, Rename, New folder, Delete, Select all and
  none. Delete moves to the Trash, and only the project's own files: folders, moments (with their Gyroflow project and
  renders), photos (with a Live Photo's video), exports, and in Footage a clip's preview (the clip stays; footage is
  deleted in Finder). On a network share, where Finder has no Trash, it goes to the share's own `#recycle`, or isn't
  deleted. **Info**, a tab beside Commands and Queue, tells all there is of the selection: a clip's recording, picture,
  marks, transform, its copies and own file; a moment's clip and times, renders, files; folders and several items
  summed up. A moment, photo or export has **Go to the footage** (its clip in Footage, selected); a clip has **Show
  moments and photos**, a folder of its own with them in the order they are in the clip (up goes back to the clip).
- **Third round** (2026-11-09). A photo keeps its clip in itself: `afp:link` in its XMP (the player's namespace,
  `https://actionfootageplayer.ch/ns/1.0/`, the same JSON a moment keeps as its MP4 key, kind `photo`, `t` the frame's
  time), written by gyroflow-export in a JPEG and by afp-photo in a HEIC (its image data copied as it is: the gain map
  and the Live Photo's identifier stay). Renamed or moved, a photo is still its clip's; older ones go by their name. A
  clip's folder shows its marks too, each before what was made of it. **Export the library as JSON…** (the project's
  menu) writes everything the library keeps, in the open: the project's file and this computer's additions, the
  locations and their pointer files, each clip's own file and what the clip says of itself, every MP4 key of the
  moments, previews, renders and exports, every photo's link. Clip previews: a clip identical to another (the same
  size and fingerprint) gets its preview in `_Clips/Duplicates/<its location>/<its folders>`, the first location's
  copy keeping its place; and a third layout, **by trips** (clips at most 2 days apart: `2026-10 October 20-26`), new
  previews going into the folder their trip's are in already.

## Open questions

- **Q10. Names.** Which shares and folders on the NAS are Anton's archive, working folder and project folder (as
  `//DISKSTATIONDXP/<share>/<folder>`): for the examples, the tests and the first real project. Not needed for the
  mockups.

## Not in this design

- Importing from a card (offload with verification); later.
- A database: still none. The truth stays the project folder's files; the search cache of *Find moved files* is this
  computer's.
