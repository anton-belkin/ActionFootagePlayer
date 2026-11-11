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
stop saving; `.afplocation` files lead from a clip to its projects; from a laptop to the NAS. Fourth, 2026-11-10,
after the *Swallow a location* command (PR #181) showed what changing the locations costs: [the project is its
files](#8-the-project-is-its-files): `Project.afproject` keeps only where the footage is, a link finds its clip by what
the clip is rather than where it was, and everything else is read from the files or is a cache. Where §1–§2 say
otherwise, §8 wins.

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

## 8. The project is its files

Anton, 2026-11-10, back to where this started. *Swallow a location* (PR #181) had to rewrite the links of moments,
previews, exports and renders, move previews, rewrite lines of `Project.afproject`, and still left photos pointing at
the old paths, all because one location was taken out while not a single file moved. The cause: a link names its clip
by its **path in the project's tree**, and the tree exists only in `Project.afproject`'s `locations`. Change the
tree, and every path written anywhere is stale.

So: the project file keeps **only where the footage is**. Everything else is either in the files themselves (the
clip's own file, a moment's MP4 key, a photo's XMP) or a **cache** of where things were last seen, on this computer,
read again and forgotten when it doesn't match.

### What `Project.afproject` keeps

```json
{
  "format": "actionfootage-project", "version": 3,
  "id": "6f1c2e4a-…",
  "locations": [ { "name": "Archive", "at": "", "path": "//DISKSTATIONDXP/video/Footage" }, … ],
  "hosts": { "DISKSTATIONDXP": ["web.nas", "192.168.50.186"] }
}
```

Nothing else. What was there goes:

| Was | Becomes |
|---|---|
| `clips_made`, `stills_made` (so a deleted preview or photo isn't made again) | nothing. You make what you ask for: a right-click on a folder, *Make clip previews*. A deleted one is made again only if you ask again |
| `settings.clips_layout` / `clips_flat`, `settings.moments_alone` | a choice in each run's dialog, remembered on this computer as the dialog's last choice |
| `clips_failed` (the last previews that failed, and why) | a log in the app's own folder on this computer (`<data>/ActionFootage/logs/<project id>/`), shown in the Queue |
| `moment_names` (the name dialog's suggestions) | the names of the moments found in the project |
| `photo_tags` | the photo's own XMP, `dc:subject` ([below](#photos-carry-what-they-need)) |

A project file with the old keys is read as before; the first time the Library opens it, the photo tags are written
into the photos, the old file is kept beside it once as `Project.afproject.before-v3`, and the keys are dropped.

**`version`** is what the project's files and its catalog hold, and **an older program refuses a project of a newer
version** (since 2026-11-11, after the NAS's catalog from before locations wrote its records without them over the
Library's, and locations went from the view). The Library, `afp-catalog` (the player's and the NAS's) and the player's
project writer each know theirs (`afp_catalog::project::VERSION`, `projectfile.lua`'s `M.VERSION`): a project of a newer
one isn't opened, read into the catalog or written; one of an older one is raised to theirs as they open it, so the
older programs leave it alone from then on. Raise it whenever what's kept changes. 1: before locations (up to 0.10);
2: with its locations (0.11); 3: the catalog keeps a clip's location (0.12).

### A link finds its clip by what the clip is

Every link (moment, clip preview, export, render, photo) already carries what identifies its clip: `clip` (its path in
the tree), `size`, `created` (the MP4's own `creation_time`, written by the camera inside the file, in `moov/mvhd`:
not the file system's dates, which a copy, an archive or a restore from a backup may change) and `hash` (SHA-256 of
its first and last MiB; links older than it have none). The rule under all of it: **sources never change**, so their
size, fingerprint and own creation time don't either. From now on:

- **`clip` is where it was last seen**, a hint to look first. Nothing depends on it being right, but it's **kept up to
  date** (Anton): when the clip is found elsewhere (opening a moment, the Library's look, a repair), the new path in
  the tree is written into the link, so the next time it's found at once, from any computer.
  - Written in place: a moment's, preview's, export's or render's MP4 key (as tags are now, the `moov` keeping its
    size), a photo's XMP (with the same writer as its tags). The file's modification time changes, and is left so:
    the player's and the Library's caches of what's in a file go by its size and time, and with the time put back
    the other one wouldn't read the new path.
  - Only a path in the library: a clip found in this computer's own location (a local copy) leaves the hint as it
    is, since other computers can't see it there.
  - A file that can't be written (a read-only share, a card) keeps its old hint; the cache on this computer remembers
    where it was found.
- **The clip is the file that matches the criteria**, only the files of the link's size. With a `hash`, it decides:
  each candidate's fingerprint is compared, the ones with the link's name first; `created` isn't needed. Without one
  (older links): name, size and `created`, then size and `created`, then name and size, each taken while it leaves
  one; never size alone, which would be a guess (Find moved files offers it, to confirm). With **"I never rename my
  footage"** ticked (Project settings, this computer's choices), the steps without the name are skipped.
- **Finding:**
  1. The hint: the file at that path in the tree, through the locations in order, if it matches. Most links stop here.
  2. Otherwise the cache: where this computer last found a file with these criteria, if it still matches.
  3. Otherwise a search of the project's locations: their folder listings give every file's name and size for free;
     only the files of the right size are opened, for `created` (a few KB of the file's header) and, when needed,
     the hash (2 MiB). Camera files' sizes are almost unique, so that's a handful of reads, on a local disk or the
     NAS. The listing of a big archive on the NAS is the main cost, seconds; what was read is cached by path, size
     and modification time.
- **Repair** (*Check the project*, *Find moved files*), Anton's order: what isn't found drops criteria by itself, one
  step at a time, until something matches. **Size is always kept.**
  1. name + size + `created`;
  2. size + `created` (renamed);
  3. name + size (something rewrote `created`: an editor's remux, a tool that sets it to the copy's time);
  4. size alone.

  At each step the candidates are few (camera files of one exact size), and where the link has a hash, each is
  compared by it: a find is never a guess, however many criteria were dropped. A link without a hash shows what each
  step found, to choose from.
- **The same clip in two places** (the same size, `created` and hash) is one clip: the first location in order is the
  one played. There's nothing to fix: the inbox inside the archive is simply seen twice. Taking a location out of the
  project is just taking it out: what reached clips through it finds them through the other. *Swallow a location*
  isn't needed (PR #181 closed), and neither is `_Clips/Duplicates`.
- **The clip's own file** (`<clip>.afp.json`, `<clip>.gyroflow`) is still beside the clip: it moves with the clip, and
  carries `clip.hash` to be matched back if it's left behind. When the project has the clip in several places (the
  same size, `created` and hash), **which copy's file is the main one** is decided the same way on every computer
  ([The clip's own file](#the-clips-own-file) said "two: nothing saved"; this replaces it):
  1. The copies are put in one order: by the location, in the project's order, copies-only locations (a working
     folder) and this computer's own last; within a location, by the path in the tree, compared as plain text.
  2. **The main file** is the one beside the first copy in that order that has one. None has one: the first save
     writes it beside the first copy that isn't in a copies-only or local location.
  3. Every other copy's file is **read, never written**: the player plays with the main one's values and saves there,
     and says once that another one exists. *Check the project* lists them, to delete (or to make one the main, by
     deleting the others).

  With Working, Archive, Inbox in that order, a clip copied (not moved) from the inbox into the archive with its file:
  the archive's file is the main one, the inbox's is listed. The `.gyroflow` beside it follows the same rule.

What the cache knows is this computer's (`<data>/ActionFootage/cache/<project id>/`: each file's path, size,
modification time, `created`, hash, link), never the library's, so two computers on one NAS never write over each
other. Deleting it costs one slower look.

### The project's own files are wherever you put them

Moments, clip previews, photos, exports and renders are recognized by **their link inside** (the MP4 key, the photo's
XMP), in **any folder of the project folder**: not only `_Moments/`, `_Clips/`, `_Stills/`, `_Exports/`, `_Renders/`,
which stay as ordinary folders. Organize them as you like, rename them, make folders: they're still found, and
still find their clips.

- The Library reads them when it opens the project, from the cache for files unchanged since, and looks again as
  folders change.
- The player finds a clip's moments the same way: by the clip's identity (size, `created`, hash), not its path. It
  keeps watching the folders where it found moments (as it watches `_Moments/` now), and the project folder's top.
- A project next to its footage reads its clips' keys once too (a clip has no link, so it's footage); cached like the
  rest.

### New files land in `Generated/`

Each run writes into a folder of its own, `Generated/<date> <time> <what>/` (`Generated/2026-11-10 14-32 Clip
previews/`), laid out as chosen in that run's dialog: as the footage's folders, flat, or by trips. From there you move
them where you want; they stay found. Moments saved one by one in the player go to `Generated/<date> Moments/`, one
folder a day. `_Cache/` (the NAS's opening data, [§6](#6-the-nas-prepares-clips-to-open-fast)) stays what it is, a
cache.

### Photos carry what they need

A photo already keeps its clip in itself, `afp:link` in its XMP, as a moment keeps it in its MP4 key; with the link
found by the clip's identity, it never needs writing again when locations change. What's left is the tags: they go
into the photo's XMP too (`dc:subject`, which Photos and Lightroom read as keywords), written **in place, without
re-encoding**:

- HEIC: on a Mac, ImageIO copies the image data as it is with new metadata (`CGImageDestinationCopyImageSource`): the
  gain map and the Live Photo's identifier stay (**to try**).
- JPEG: the XMP segment rewritten; in a Motion Photo the video after the image keeps its length, which its XMP says,
  so that stays right (**to try**).
- Windows and Linux: a writer of our own for the XMP box/segment, or exiv2 (**to try**).

### The Library sees what changes

Anton, 2026-11-10: the player writes marks, moments, photos and settings while the Library is open, and the Library
didn't know. Now it watches the project's folder and its locations with the system's own notices (FSEvents on a Mac,
ReadDirectoryChangesW on Windows, inotify on Linux: `notify`), counts what changes there (hidden and temporary files
aside), and reads the project again once the changes have settled for 0.8 s, while no batch runs (a batch's end reads
it again anyway). Whoever changed the files: the player, Finder, afp-meta. A share watched from a Mac reports what this
Mac changes on it, not another computer's changes.

### Seeing and fixing it without the app: `afp-meta`

The library is its files, so what the player keeps in them can be read and written from the command line (Anton,
2026-11-10). `afp-meta` is beside the player's other tools (`ActionFootagePlayer.app/Contents/Resources/gyroflow/`
on a Mac):

```
afp-meta <file>...                    what of ours each file keeps, as JSON: a video's link and all its MP4 keys,
                                      a photo's link and tags
afp-meta <file> --link <json>         its link written (a video's ch.actionfootageplayer.link, a photo's afp:link)
afp-meta <file> --tags <json list>    its tags written (a video's in its link, a photo's dc:subject)
afp-meta <file> --key <name> <value>  an MP4's metadata key written, its other keys kept
```

Videos (moments, clip previews, exports, renders) and JPEG photos are written by it on every system; a HEIC by
afp-photo on a Mac (ImageIO, the image data copied as it is). Checked on a HEIC: tags and link written, replaced and
cleared, the decoded pixels the same.

Built (2026-11-10): the Library writes a photo's tags with afp-meta and reads them from the photo; tags an older
version kept in `photo_tags` are shown with the photo's own and go into it at its next change of tags.

### What was built (2026-11-10, PR #182)

- **Finding by identity**: the player (`moments.lua`'s original, `link.lua`'s created from `moov/mvhd`) and the Library
  (`checks.rs`'s find_clip, `index.rs`) look for a link's clip at its hint, then where this computer last found it
  (the player's cache), then among the footage of its size: the fingerprint deciding, its name first; without one,
  Anton's steps. Found elsewhere, the hint is written into the link (the player as it opens a moment, clip preview or
  export; the Library as it reads Moments). Find moved files goes down to size alone and offers several to choose.
- **A clip's moments** by fingerprint, else size and creation time, else (older links) path and size.
- **The main own file** of a clip in several places: the rule above, in `workspace.lua`, `workspace.rs` and the
  Library's `tags.rs`; the shared cases test it (`tests/workspace_cases.txt`). The player says once that another one
  exists and saves to the main one.
- **"I never rename my footage"**: Project settings → On this Mac (this computer's file for the project).
- **Own files anywhere**: the player's scans (moments, clip previews, exports, photos of marks) and the Library's views
  read the whole project folder: in its own folders (`_…`, `Generated`) and outside its footage every video and photo,
  among its footage only `.afplink.mp4`. The Library's Moments, Photos and Exports show the project folder's own
  folders (paths from the project's folder: `_Moments/Reef`, `Generated/2026-11-10 14-32 Moments`), empty folders too;
  Footage shows clips as their previews are arranged, wherever. Rename, delete and New folder work there.
- **Generated/**, in each kind's own folder (Anton, 2026-11-10): clip previews into `_Clips/Generated`, moments into
  `_Moments/Generated`, photos into `_Stills/Generated`, exports into `_Exports/Generated`, made by a run or by hand,
  all in it with no folder per run. The project's setting "A folder per run in Generated" (Project.afproject
  `generated_runs`, off by default; Library, project settings, General) brings them back: `<date> <time>` for a run,
  `<date>` for what's saved by hand. A name taken there gets another: a moment, photo or export a number ("Name 2"),
  a clip preview the clip's folder in parentheses, then a number; a clip's own preview is replaced. Projects of
  before keep their `Generated/` at the top as it is (nothing moved: renders sit at their moment's path, and a move
  of thousands of previews over the NAS while a task runs is a risk for nothing); the Library shows it under Other
  folders, from where it's moved like any folder. Clip previews' layout and "stabilized on their own" are chosen in the
  Library for each run (remembered on this computer); in the player, "on their own" is this computer's.
- **Clip previews empty first** (Anton, 2026-11-10: "I want to start organizing, not wait for the pictures"): the
  Library's "Clip previews" writes every clip's preview at once as an empty one (a dark frame, 1.9 KB, the link inside
  with `"draft": true`), 200 at a time by `afp-cut --empty-previews` (16 files at once), each 200 noted to the catalog
  as written (`afp-catalog note` reads 16 at a time: 200 in 0.9 s, 5.3 s before): 450 on the NAS in 6.6 s, the progress
  moving. Footage shows them with the camera's own quick look (LRF/THM) until their picture is there, and reads the
  catalog again every 4 s while a task runs. A second task ("their pictures") then makes each one real where it is now
  (moved meanwhile: found again by its link), each noted as it's done, the folders the Library shows first
  (`set_focus` → `focus.json`, read before each clip). Not at macOS's background priority: that throttles the NAS reads
  too (a minute and more for one clip). The player quits only once its notes are through (a quit cut them off). It
  steps aside for any other task (stopped after the one being made, queued again after it). Drafts of 2 s of video
  were tried first: 0.3–0.75 s and 22 MB a clip, too slow to start organizing.
- **Tasks never lock the Library** (Anton, 2026-11-10: "anything can happen anyway, in Finder too"): rename, delete,
  mend, fix paths, take in and delete renders run while tasks are queued or running; a task copes as with Finder (a
  preview moved meanwhile is found by its link, one deleted is skipped). Moving the project's own files is the one
  that can't run beside its tasks: the running one stops after what it's making, they all wait, the files move, and
  they go on from the new place.
- **Nothing more in Project.afproject**: no `clips_made`, `stills_made`, `clips_failed`, `moment_names` or settings
  written (empty lists of before aren't written back either). Photos of marks made already are found by their links;
  name suggestions are the moments' names; failures are in the player's log and the batch's status. Renders stay in
  `_Renders/`, at the moment's path in `_Moments` or, elsewhere, in the project's folder.
- **Photo tags** in the photo's XMP (afp-meta), **afp-meta** itself, and the Library **watching** the project.

Checked with unit tests (luajit: 12 suites; Rust: the Library's 16, vs-gyroflow's workspace cases and XMP) and the
Library's page against a mock in jsdom. Not checked yet: the player and the Library live on a real project (the
player's batch writing into `Generated/`, a moment opened whose clip moved).

## 9. The catalog: the files, without looking at all of them

Anton, 2026-11-10, after §8 was built: the player looked through the whole project folder each time a clip opened,
to find its moments — on a NAS seconds at 10 000 files, minutes at 100 000 — and the Library didn't hear of what
another computer changed. Two kinds of things link to each other (originals; moments, clip previews, exports, photos),
and finding them by walking the folders doesn't scale. What he settled, step by step:

- The files stay the truth (§8). What's kept besides is a **catalog**: what was last seen of them, which can be lost
  and built again from them, and is never needed for a file to be right.
- It's **shared through the project's own folder** (`.afp/catalog/`), so a computer joining reads it instead of
  looking at every file — not kept on each computer alone.
- What our programs do reaches every computer at once; what's done in **Finder** reaches them when an open Library
  next looks at the folders, and once found it's written for all.
- **The player never folds or looks through folders**: it plays. It reads what it needs of the catalog beside the
  playing video, without waiting for it, and notes what it wrote.
- Nothing open-source fitted (research 2026-11-10): SQLite and LMDB must not be used on a network share (their own
  documentation), delta-rs and SlateDB commit with hard links or a no-overwrite rename the Mac's SMB client and Samba
  don't do safely, restic and Kopia clean up by clocks and locks. So a small format of our own, on what restic,
  Kopia and automerge-repo do: files named by their content, written once, never written over; each computer's own
  journal; snapshots that fold them; no locks, no clocks.

### What's in it

For every file and folder in the project's folder and its locations (hidden ones aside): its size and time, and for
the ones whose insides count what's inside — a moment's, clip preview's, export's or photo's link, a clip's sidecar's
marks and tags. For each folder, its time and a digest of what was in it when it was last listed. Each record has a
version (a Lamport clock: above every version its writer had read; the computer's name breaks ties): a record replaces
another only with a higher version, so every computer that has read the same lines knows the same, whatever order it
read them in.

### The Library's views (Anton, 2026-11-10)

Each view's top is its own folder: Footage `_Clips` (clips as their previews are arranged there), Moments `_Moments`,
Photos `_Stills`, Exports `_Exports`; what's found anywhere else in the project's folder is under **Other folders**, at
its path there (a new folder at a view's top is made in its own folder, in Other folders at the project's top). Sources
is now **Originals**: the clips at their paths in the project's tree; the sources themselves are in Sources settings.

### Three layers, each only faster (Anton, 2026-11-10)

Originals move only when the library is mended after (the Library's Restore: it finds them by what they are and
rewrites the links); new clips come in by Take in, or by a command over a folder (clip previews), which notes what it
meets. So links name their original by its path, and only the project's own files (moments, clip previews, exports,
photos) are watched for what Finder does to them. Then:

| | Who | Finder's changes reach the catalog |
|---|---|---|
| 1 | The player alone | As a clip opens (not more often than every 20 s) and every minute while it plays: `afp-catalog look --own` in the background, at low priority, looks at the time of each of the project's own folders and lists the ones that changed (~0.01–0.1 s on a small project); it folds the journals when they've grown |
| 2 | + the Library | This Mac's changes at once (FSEvents), the rest at its regular look; the footage only when mended, on Take in, after a batch |
| 3 | + the NAS's watcher | At once, for every computer: `afp-catalog watch` in Docker on the NAS, told by inotify what changes on its own disks, every folder's time looked at as it starts and when the system says it lost count |

The player's moments of a clip come from the catalog by the clip's path at once; a moment not where it was is kept
on the seek bar until the look says moved or removed. A moment put back from the trash isn't in any journal: the look
finds it by its folder's time.

### On the share

```
.afp/catalog/journal/<computer>/<first line's number>.log   each computer's own, appended only by it
.afp/catalog/snap/<generation>-<hash>.json                  a snapshot's list: which line of each journal it has
                                                            read up to, its parts
.afp/catalog/parts/<hash>.gz                                its records, about 2000 a part, gzipped
```

- A journal line: 8 hex digits of its text's SHA-256, its text (JSON: number, version, records), a newline. A line
  cut off by a lost connection or a stopped program has no newline or the wrong sum: it's left out. After a write that
  failed, the next line starts a new segment (named by its number), so nothing is appended after a broken line; a line
  that had arrived after all is read once (its number was read).
- Snapshots and parts are named by their content's hash: written into a temporary file and renamed to a name nobody
  else uses (two computers writing it wrote the same bytes). Read, they're checked against their name; a damaged one is
  passed over for the snapshot before it.
- Records go into parts by what a reader looks for: a moment, clip preview, export or photo by its original's
  fingerprint (else its path), anything else by its folder. The player opening a clip reads one or two parts.
- **This computer's name**: made once, kept with the machine's own id (macOS's platform UUID), so a home folder copied
  to another Mac becomes a new computer there, not a second writer of one journal.

### Who does what

- **Reading** (both): the newest snapshot that reads whole (its parts from this computer's cache when it has them:
  they never change), then every journal's lines after what the snapshot covers. A reader whose lines were cleaned up
  meanwhile is told so and reads again from the newer snapshot.
- **Noting** (the player, the Library, afp-meta, afp-catalog): what was written — a moment saved, a link or tags
  rewritten, a sidecar saved — appended to this computer's journal, after reading what's new (so its version is above
  what it could read). A file written in place (a link in a moment's MP4) also has its folder **touched** (a hidden
  file made and removed), so that the folder's time changes and the next look lists it, whatever else happens.
- **Looking** (the Library only, while open): every few minutes it looks at the time of each folder of the project and
  its locations — one question a folder — and lists again only the ones whose time, or whose files on record, aren't
  what was written when they were last listed; what changed is written to its journal. Its own computer's changes it
  hears of from the system (FSEvents) and looks at those folders at once. A folder listed because it changed is listed
  once more by the same computer a minute later (kept on its disk, so a restart doesn't forget): on the NAS the Mac
  gave a folder's new time together with a listing from its cache, from before the rename that changed the time — a
  record right about the time and wrong about the files, which no later look would have questioned. Once a day a deep look lists every folder
  (in the background, slowly): it finds a file changed in place by something not ours.
- **Folding** (the Library only): when the journals have grown past 2000 lines or 2 MB, a new snapshot; only parts
  that changed are written. Two Libraries folding at once both make good snapshots.
- **Cleaning** (the Library only): snapshots older than the newest three, parts none of those has, temporary files
  left, this computer's segments those have read — each only once this computer has seen it so for 10 minutes (its
  own clock against itself). Found on the real NAS: with "seen so at two clean-ups", a Mac whose listing of the
  snapshots was 30 s behind removed the parts of another computer's newest snapshots, and none read whole.

### The NAS's watcher

Shipped as part of the NAS Helper (tools/nas-helper/README.md), its "collaboration" part, with the stream and the
links for Jellyfin in one container; built alone as below.

`catalog/Dockerfile` builds `afp-catalog` alone; run as the project's watcher:

```
docker build -t afp-catalog catalog/
docker run -d --name afp-catalog-trip --restart unless-stopped --hostname nas --user 1000:10 \
  -v /volume1/ssd/Projects/Trip:/project -v /volume1/video:/shares/video -v afp-catalog-data:/data \
  afp-catalog watch /project --share video=/shares/video
```

`--share` says where a share the project's locations name (`//DISKSTATIONDXP/video/…`) is inside the container. It's
a computer of its own in the journals (its name kept in `/data`, a folder the `--user` can write: a Docker volume
is root's). Built and run on the NAS (2026-11-10, Rust 1.91 on Debian bookworm, 88 MB): a test project whose
footage is a location `//DISKSTATIONDXP/ssd/…`; a moment copied in, moved, removed and put back over SMB from a Mac
reached that Mac's player in ~1.3 s each (the watcher waits 1 s for a change to settle), the player looking at
nothing. Once, the first reading of the watcher's journal after a change came from the Mac's cache for some seconds
(the NAS writes it itself, so Samba doesn't tell the Mac it changed): journals' tails are read past the Mac's cache
(F_NOCACHE), and the player asks again every 10 s anyway.

On a Mac a share's notices (FSEvents) are only of what this Mac changes there: another computer's or the NAS's own
changes aren't told to it — which is what the NAS's watcher is for.

### What a look won't do

- **A connection lost is not a folder emptied**: a folder that can't be listed is left as it was. A root (the project,
  a location) is looked at only when its marker is there (`Project.afproject`, the location's `.afplocation`): a share
  not mounted, an empty mount point where it was, removes nothing. A folder that had things in it listed empty is
  listed again and its root's marker looked for again first.
- **Moved folders keep their times**: what's new is found by the folder it moved into (that one's time changed), then
  listed whole, whatever its own time.
- **A file noted in a folder never listed** (the player's moment in a new folder) makes that folder one to list.
  Until then the views take the folder as there (seen.rs's walk): the moment shows at once.

### Who tells the catalog what they changed (audit, 2026-11-10)

The views read the catalog, so whatever changes a file must say so at once; a look comes minutes later, and a file
written in place (a link, a clip's own file) leaves its folder's time as it was, so a look doesn't even list it (only
the daily deep look would). FSEvents tells a Mac only what that Mac changed, and on a share even that is no promise.

- **The Library**, for everything it does: `seen::note_now(paths)` right after it writes, before the page reads the
  view again. Files written, made or removed; folders made, removed, or renamed (both paths: what's in the new one
  listed, what was in the old one gone). It reads them, notes them in this computer's journal through the views' own
  catalog, touches their folders for the other computers, and counts a change for the page. Tags, lens type, location,
  rename, delete, new folder, Mend / Find moved, Take in, renders deleted, links rewritten by a view, and Stabilize
  optically's `.gyroflow` files (as the task's status names each clip done).
- **The player**: `afp-catalog note` for what it writes (workspace plugin's `catalog.note`; other plugins send it
  `catalog-note`). Marks, Save, moments (and their `.gyroflow`), moment rename and remove, anything it moves to the
  Trash, Ctrl+x's parts, Ctrl+e's project, Shift+E and 360 exports, photos, and every batch's files as each lands (clip
  previews, moments, photos, exports, renders). A batch quits only once its notes are written.
- **Move the project…** takes `.afp` along; the new place's thread looks at the project's folder again before the views
  read the catalog (its records of the folder it left are of the footage, now a location).
- A setting written into `Project.afproject` doesn't make the Library read its catalog again: only its locations do.

**On one computer, at once, both ways** (catalog/src/live.rs, in `<cache>/live/`). The journals are on the share and
read every 3 s; the programs running here tell each other instead:

- player → Library: `afp-catalog note`, once its line is in the journal, sends each Library running here (its
  `library-<pid>.json`: a port on 127.0.0.1) the project and files. The Library reads the journal at once, and its page
  hears a `catalog-changed` event and reads the view again (no more polling, no waiting for quiet).
- Library → player: `note_now` appends a line to each player window's inbox, `player-<pid>.in` (a window has one, a
  batch doesn't). The window looks at its size four times a second (a local stat), then asks for its clip's moments
  again and counts the project again; a lens type given to the clip playing plays at once (the clip loaded again where
  it was).

Measured on the NAS (`live_e2e::both_ways_at_once`): a moment a batch makes in the Library's view 0.09 s after it's
made; the Library's rename in the player's list 1.0 s (its catalog asked over the share); a lens type 0.13 s; a mark
(M) in the Library's view 0.55 s (afp-catalog reading and writing on the share).

### What it costs (simulated, 2026-11-10)

Counted on a simulated NAS (the round trips and bytes each thing takes), priced at a local SSD (0.05 ms, 2 GB/s), a
wired NAS (0.4 ms, 110 MB/s), Wi-Fi (4 ms, 30 MB/s) and a VPN (40 ms, 5 MB/s), plus this computer's own time;
`catalog/tests/cost.rs`. 100 000 clips and 100 000 moments in folders of 100:

| | local | NAS wired | Wi-Fi | VPN |
|---|---|---|---|---|
| Built (every folder listed, every moment read — what each look cost before) | 17 s | 2 min | 21 min | 3.4 h |
| A new computer reads it | 0.4 s | 0.6 s | 2 s | 17 s |
| The Library opened again | 0.4 s | 0.4 s | 0.5 s | 1 s |
| The player opens a clip | 12 ms | 23 ms | 95 ms | 0.8 s |
| The player notes a moment; another computer sees it | 1 ms | 15 ms | 0.15 s | 1.5 s |
| A look, nothing changed (each few minutes, in the background) | 0.3 s | 1 s | 8 s | 82 s |
| The deep look (once a day, in the background) | 0.8 s | 4 s | 34 s | 6 min |
| A new snapshot | 0.6 s | 0.6 s | 0.8 s | 2.6 s |

At 10 000 the looks are ten times less; the rest about the same.

### What it costs on the NAS (measured, 2026-11-10)

The real NAS (UGREEN DXP2800, Samba, btrfs; the NVMe volume and the HDD mirror with its NVMe cache) over wired
gigabit from a Mac (0.37 ms ping), with `catalog/examples/rig.rs` and test projects made on the NAS by
`tools/catalog_rig.py`. macOS's SMB client is slower than the network: **a file's size and time ~5 ms, opening and
reading a small file 11–24 ms, listing a folder of 100 ~80 ms** (1 ms once cached), a folder's time ~5 ms. Hence:
folders are listed with their files' sizes and times in one call (`getattrlistbulk`, as Finder does), never a
question per file; new files are read 8 at a time.

| 10 000 clips, 10 000 moments | NVMe volume | HDD volume |
|---|---|---|
| Built (every moment opened once) | 4 min | 2 min |
| Checked against every file | 0 differences | 0 differences |
| A new computer reads it | 0.5 s | 0.5 s |
| The Library opened again (parts on its disk) | 0.05 s | 0.04 s |
| The player opens a clip | 15–18 ms | 15–16 ms |
| Anything new? | 1 ms | 1 ms |
| A look, nothing changed | 0.7–1.2 s | 0.6–1.5 s |

100 000 clips and 100 000 moments (NVMe volume): built in ~23 min (once); a new computer reads it in 1.3 s (3 MB),
again 0.45 s; the player opens a clip in 12–15 ms (the catalog), 50–200 ms through `afp-catalog` as the player calls it;
anything new 3 ms; a look with nothing changed 10–60 s in the background (2 000 folders, 5–30 ms each as the NAS is
busy).

A stress run on the NAS: two Macs' writers killed with -9 at random ~100 times in 12 minutes while noting, looking,
folding and cleaning, Finder-like changes and a third computer's journal on the NAS's side: the catalog read whole
throughout (81 snapshots); it is what found the clean-up and stale-listing faults above, and that after such a beating
the Mac's SMB client can't open a few files for minutes (Invalid argument, 17 s opens) — which a look counts as errors
and leaves as they were.

What another computer changes reaches this Mac: a line appended to its journal at once (a file opened is read as it
is); a new file or folder (a new segment, a new computer's journal, a moment saved) **after ~30 s**, as macOS caches
a share's listings that long — the journals' next segment is therefore also looked for by its name.

### Where it can be wrong, and for how long

The files never are; the catalog can say something old about them for a while. What was found, worst first:

- **A file changed in place by something not ours** (an editor writing a sidecar over itself, a tool rewriting an
  MP4's metadata without a temporary file): its folder's time doesn't change, so the looks don't list it. The player
  sees it at once (it looks at its clip's moments' files when it opens the clip), the Library **only at the daily deep
  look**. Ours always touch the folder.
- **Two computers saying different things at the same moment** (one notes a moment while the other's look lists its
  folder from a listing a few seconds old): the version decides, which may be the older truth; the folder then doesn't
  match what was written of it, and the next look (≤ 5 min) lists it again and settles it. Found by the random test;
  why every write first reads what's new.
- **A computer's change reaches another after ~30 s** when it's a new file (macOS lists a share's folders from a cache
  that long); the journal line saying so is read at once when its segment exists.
- **A note lost** (the program stopped between writing the file and the journal line, the connection gone for both the
  line and the folder's touch): the next look finds the file by its folder's time; only a change in place with both
  lost waits for the deep look.
- **A journal file this Mac can't open** (seen once on the NAS after 100 kills: macOS's SMB client answering
  "Invalid argument" for a file another computer had written, healthy on the NAS, for minutes): that computer's later
  lines wait on this Mac until the file reads or a snapshot folded elsewhere covers them; everything else is read.
- **A card or USB disk formatted FAT/exFAT** as a location: folder times there are coarse (2 s, 10 ms), so a change
  in the same tick as a listing can wait for the deep look. Not tested.
- **Names in two Unicode forms** (a Mac and a Windows PC writing the same name differently): not normalized yet; could
  show a file twice or make looks list a folder each time. Not tested.
- **The NAS's recycle bin** keeps what the clean-ups remove (old segments, snapshots, parts): small, but it grows; the
  `#recycle` folder can be emptied or `.afp` left out of it.
- **The first build** opens every moment, clip preview, export, photo and sidecar once: ~2–4 min per 10 000 on this
  NAS, the views read from the disks meanwhile. A Linux afp-catalog run on the NAS itself (Docker) would build it in
  seconds — not done.
- **A project no Library of this version has opened**: no catalog; the player looks through the folder as before.

What doesn't happen, by construction and tested: a lost connection or a share not mounted removing anything from the
catalog; a damaged or half-written file being used (each is checked against its name or sum); two computers' writes
overwriting each other (each writes only its own files, or files named by their content); a reader left on cleaned-up
lines without knowing (it reads again from the newer snapshot); a look listing folders that didn't change.

### How it's tested

`catalog/tests/`: the scenarios (a new computer reads the snapshot, not the files; an unchanged look lists nothing and
writes nothing; the player's note seen at once elsewhere; Finder moving, copying, renaming, removing; a file changed in
place; a share not mounted; a connection lost mid-look; the player's parts; segments cleaned while a reader is behind;
two computers folding at once; a damaged snapshot; a cut-off note; a stale listing under a fresh folder time) and a
random run: three computers (two Libraries, one player) and Finder doing 400 random things to a project, the
connection losing 5–20 % of operations (writes cut off midway, written but the answer lost, temporary files left),
going down for a while, listings and sizes a few operations stale; then the connection good again, one look — and
every computer's catalog must say exactly what the files say, a second look must list nothing, a newcomer must read
the same, the player must find each clip's moments. 300 seeds pass with and without faults; it found three faults
in the first version, now fixed (two records of one key in one line; a computer's note losing to an older record it
hadn't read; a file noted in a folder never listed, left behind when the folder went).

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

[§8, the project is its files](#8-the-project-is-its-files), before anything else that writes paths, each a PR:

1. **Finding by identity**: the hint, the cache, the search by size, `created`, name and hash, *"I never rename my
   footage"*, the repair's steps; in the three twins with shared cases. Taking a location out then needs nothing more.
2. **Own files anywhere**: the Library's index and the player's moments of a clip found by the link inside, in any
   folder of the project.
3. **`Generated/`**: batches write there with the layout chosen per run; `clips_made`, `stills_made` and the settings
   dropped; failures to the log in the app's folder.
4. **Photo tags in XMP**, `photo_tags` dropped; the old project files cleaned (`Project.afproject.before-v3` kept).

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

- **Fourth round** (2026-11-10): [the project is its files](#8-the-project-is-its-files). `Project.afproject` keeps
  only the locations (and the id and the NAS's names); the rest is in the files or a cache on this computer. A link's
  path is where its clip was last seen, written again when it's found elsewhere; the clip is found by size, `created` and hash, plus the name for someone who
  never renames footage (a tick). Repair drops criteria one at a time, name, then `created`, then both, size always
  kept, the hash comparing what's found. No list of what was made: you make what you ask for. New files land in
  `Generated/<date> <time> <what>/`, laid out as chosen in that run's dialog, and are found wherever you move them.
  Photos keep their tags in their own XMP. A clip in several places has one main own file, chosen the same way
  everywhere: the first copy that has one, by the locations' order (copies-only and local ones last), then the path in
  the tree; the others are read, never written.

## Open questions

- **Q10. Names.** Which shares and folders on the NAS are Anton's archive, working folder and project folder (as
  `//DISKSTATIONDXP/<share>/<folder>`): for the examples, the tests and the first real project. Not needed for the
  mockups.

## Not in this design

- Importing from a card (offload with verification); later.
- A database: still none. The truth stays the project folder's files; the search cache of *Find moved files* is this
  computer's.
