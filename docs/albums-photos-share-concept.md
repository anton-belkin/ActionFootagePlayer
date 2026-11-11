# Albums, photos as objects, renders and Share: one concept

**Status:** a concept, not built. It came out of Anton's discussion of the comparative study
(`reports/library-comparison-2026-11-11.md`) on 2026-11-11.

**Related issues:**
- Albums: #215
- Photos as objects: #216
- Share: #214
- Telemetry search: #213
- The card check: #211
- The tag decisions: the comment on PR #197

It changes the Library's views and adds one kind of object, the photo. The rules of the files stay as they are.

## The rules that stay

- **The files are the truth.** The catalog is a cache. Everything the library knows is in files beside the footage or
  in the project's folder (`docs/library-management.md` §8).
- **Originals are never changed.** Not a byte, not the size. The user is told so plainly. Metadata outside the bytes
  is allowed, such as a Finder tag (an xattr: the content, size and mtime stay the same).
- **The Library doesn't decide where footage lives.** There is no import: you copy a card with Finder into the folder
  you choose, and *Take in* does the rest. The card check (#211) only says whether a card can be formatted.
- **A link finds its clip by what the clip is** (size, `created`, fingerprint), so links survive moves and renames.

## The objects

| Object | What it is | Its file | Where it lives |
|---|---|---|---|
| **Original** | the camera's recording: a clip, or a photo (below) | the file itself, plus its own `<file>.afp.json` | Originals (the locations, as on disk) |
| **Mark** | a time in a clip: a camera highlight or `M` | none: a line in `<clip>.afp.json` | inside its clip |
| **Clip in an album** | the whole clip, placed in an album | a link file (what a clip preview is today) | an album |
| **Moment** | a range of a clip | a link MP4 (150–350 KB, one frame + the link) | an album |
| **Photo** (new) | one frame of a clip | a link file: a small picture + the link | an album |
| **Render** | the real pixels of an item in one profile | a video or photo file with its recipe | `_Renders/`, a cache |

**Marks are raw material; moments and photos are objects.**
- *Moments of the marks* turns marks into moments; *Photos of the marks* turns them into photos.
- A key in the player makes a moment or a photo directly, without a mark.

**Tags** go on originals, moments and photos, never on a clip in an album. The album link is an organizational copy,
and the clip's tags are its original's.

## Albums

The Library's tabs become **Originals** and **Albums**. The Moments, Photos and Exports tabs go.

Inside Albums:

- **Your albums.** Folders of links: clips, moments and photos together, with subfolders. This is what Footage is for
  today, renamed. Albums are optional: someone whose originals are already well arranged, and who finds things by
  time, place or tags, may never make one.
- **Generated.** Where new items land when there's no album to put them in (`Generated/<date> …`, as today). It works
  like Photos' *Recents*: you sort from there.
- **Timeline.** Years, then months, then trips. A trip is clips at most 2 days apart, the existing rule, named from
  its place: "Sharm el-Sheikh, Oct 20–26". It's a view over the catalog, with no files. It replaces the clip previews'
  "by trips" layout.
- **Map.** Everything with a place. Selecting an area selects its items. Places come from the clip's GPS (#213) or
  from nearby Apple Photos (#188), so the map fills in as those do.
- **Smart albums.** Saved filters, each a small file (`<name>.afpsearch`, JSON) in the project's folder, shown as an
  album whose contents are the matches. Built-in ones:
  - by kind: *All moments*, *All photos*, *With a render*;
  - by culling: *Accepted*, *Rejected*, *★★★★ and up*;
  - by telemetry, once #213 is in: *Under water*, *Below 20 m*, *Has GPS*.

**Where a new moment or photo lands:** in the album its clip was opened from; otherwise in `Generated/`.

**One item in two albums** (decided): a link is cheap to copy, and a copy is a separate item with its own tags and
name. An item has one album. "It's also about this" is a tag, and a smart album on that tag shows it.

**Apple Photos sync** stays per folder, as designed in `docs/library-management.md` §5, so an album syncs as an
album. Photos in it are rendered for the sync (below).

## Photos as objects

Until now a photo existed only as the product of an export: rendered on the spot, a full HEIC or JPEG. That's slow
while watching (the clip stutters while the photo renders), and it fixes the format before you know who it's for.

**Now:**
A photo is **a moment and a crop**: one frame, plus the part of it you want. The crop is part of the photo, like a
moment's `in`/`out`. It lives in the photo's link and in its render's recipe (the crop's camera is already in the
recipe), so changing it later makes the render stale.

**Two keys:**
- **Snap.** The whole frame, or the last crop used, becomes a photo at once. Playback doesn't stop.
- **Crop.** Playback pauses, and the crop mode (#112) opens on the frame. Frame it, confirm, and the photo is made and
  playback continues.

Aspect presets (4:3, 1:1, 9:16) belong to the crop. On a 360 clip, the crop is the view you look at. A photo's crop can
be changed later: open the photo, adjust, done.

1. While watching, either key makes the photo object at once, with a cheap preview (the frame as displayed, cropped,
   small). Nothing is rendered.
2. *Photos of the marks* does the same for every mark of a selection.
3. Later, select the photos (in an album, the timeline, a smart album) and **Share** them. Only then is each one
   rendered, in the format the destination needs:
   - for an iPhone or Apple Photos: HEIC with a gain map, or a **Live Photo**;
   - for Android: a **Motion Photo** (a JPEG with the video inside);
   - for anything else: a plain JPEG, SDR.
   Clean still frame (`Ctrl+p`) and denoise become options of the render.

**The photo's file:** `<name>.afplink.jpg`, a small JPEG (around 1280 px) with `afp:link` in its XMP (`kind: "photo"`,
the frame's time, the crop).
Photos already carry this link today. Tags go into `dc:subject`. Finder and Quick Look show it as a picture.
- **Risk:** someone shares the preview from Finder, thinking it's the photo. Two guards (decided): it's visibly
  small, and the `.afplink.jpg` name says it's a link, as `.afplink.mp4` does for a moment.

**Photos made before this** (a full HEIC or JPEG with `afp:link`, a Live Photo's HEIC + MOV): **no feature for it.**
Anton is the only user so far, so a one-off migration script is run once, on his library, when he says so, and then
forgotten.

For each photo, the script:
1. Makes a photo object where the file is now (same folder, same name), with the clip, time and crop read from its
   XMP.
2. Moves the file into `_Renders/` as that object's render, in its profile (HEIC → `Photo HDR`, a Live pair → `Live`,
   JPEG → `JPEG`). A pair moves together.

A render without a recipe counts as current until the photo changes. The script lists what it will do first, and
deletes nothing.

## Photos as originals

Cameras shoot photos too: Action 6 and GoPro JPEG, GoPro GPR raw, drone DNG, Insta360 `.insp` (360°), plus phone
photos in the same folders. Today the Library lists only videos, so photo originals are invisible to it.

**The intention:** a photo original is a full object of the library, like a clip. It lies in Originals, is never
changed, and gets the same treatment our video flows already give footage:
- development and HDR;
- the lens and the water corrected;
- tags, albums and Share.

**Nothing of this is planned for now.** This section records the intention and the order the work would go in.

### Stages

1. **Seen and organized.**
   - **Where they show:** JPEG, HEIC and RAW in Originals, with thumbnails. RAW shows its embedded preview.
   - **Organizing:** a photo original goes into albums by a link, as a clip does. It takes tags and Accept/Reject, by
     the same rules (Finder xattr allowed, the bytes never touched).
   - **Timeline and map:** filled from EXIF `DateTimeOriginal` and EXIF GPS. Photos with GPS can also suggest the
     place of clips shot near them, like #188, but from the library itself.
   - **Share:** the file as it is, or a JPEG for Android and the web.
2. **Lens and water.** Our clips' lenses are known, and so are the cameras that shot the photos (EXIF make, model and
   mode). The same corrections apply:
   - **The lens:** distortion corrected from the Gyroflow lens profile of the camera's photo mode. *(Open: the photo
     modes need their own profiles; the database is mostly video modes.)*
   - **Under water:** the refraction correction, as the player does for video.
   - **Colour fringes:** the chromatic aberration of the housing's port under water, taken out as `Ctrl+a` does for
     video (#73).
   - **Why it matters:** nobody else does this for divers' photos. Lightroom corrects lens profiles but knows nothing of
     the water; the makers' apps do nothing.
   - **As renders:** the result is a photo render (HEIC with a gain map, JPEG), cached like any other.
3. **RAW to HDR.** DNG and GPR developed (LibRaw, or the system's RAW engine on a Mac) into our looks, ending in an HDR
   HEIC with a gain map. It's the same idea as log→HDR for video: the picture right as soon as you browse, with
   nothing to set up.
4. **360° photos** (`.insp`): look around in the 360 view and save a view as a flat photo, as for 360 clips.
5. **Noise.**
   - **What works:** our clean still frame is temporal (5 frames on each side). A single photo has no neighbours, so
     only spatial denoise applies, which is weaker. Bursts are the exception: there the neighbours exist.
   - **Promise:** nothing about noise until it's measured.

### What it needs from the files

- **Identity:** a link to a photo original finds it by size, EXIF `DateTimeOriginal` (in place of the video's `created`)
  and fingerprint.
- **Its own file:** settings such as a lens choice, the water and tags live in `<photo>.afp.json` beside it, like a
  clip's.
- **Where it's viewed:** the player (mpv) shows images, but probably not HDR with a gain map. Quick Look from the
  Library may be the first viewer. *(Open.)*

## Share replaces export

(#214, in short)

**Share is two choices.**
1. **What:**
   - in the player: the clip, the moment, ±2 s around this frame, a photo, a Live Photo;
   - in the Library: the selection.
2. **Where:**
   - the system's share services (AirDrop, Messages, Mail, apps with a share extension);
   - plus our own destinations: **Add to Photos** (PhotoKit, into an album; the only correct way for Live Photos),
     **Save to a folder**, **Show in Finder**.

**Export is the destination "Save to a folder".** `_Exports/` goes. An export is any folder the user saves to.

**Renders are the cache behind it.** They're made in the background, reused while current, remade when stale, and
managed by size and deleted from the Library. "Has a render" is an attribute of a clip, moment or photo, and a
filter.

**The destination picks the render:**
- AirDrop, Photos and a folder get HDR.
- Messengers re-encode (most likely to SDR), so they get the SDR render.
- Photos get a Live Photo or a Motion Photo, depending on the destination.

## Renders

Renders are becoming the centre of the scheme: Share, Apple Photos sync, the TV and playback all run on them. Their
first design is `docs/library-management.md` §4. This section is what Albums, photo objects and Share add to it, and
the questions it leaves.

### What's built (v0.11)

- **Scope:** moments only.
- **Profiles:** `HDR` and `SDR`.
- **Location:** `_Renders/<the moment's path>.<profile>.mov`, in the project folder.
- **Recipe:** carried in the render's link. It covers the clip's size and hash, `in`/`out`, the transform, the source's
  input, the Gyroflow project's fingerprint, the profile and a render version.
- **Status:** current or stale, by comparing recipes.
- **Commands:** *Delete stale renders*, *Delete renders*. Rendering runs as a background job in the player's queue.

### What a render is

The real pixels of one **item** in one **profile**:
- **Items:** a whole clip, a moment, a photo, or a span made on the fly for Share (±2 s around a frame).
- **Recipe:** decides everything about the render. A current render is reused, never redone. It is always a cache:
  deleting it loses nothing but time.

### Profiles

| Profile | For | What |
|---|---|---|
| `HDR` | AirDrop, Apple Photos, a folder, Apple TV | HEVC 10-bit, HLG + Dolby Vision 8.4, as an iPhone records |
| `SDR` | messengers, Android, the web | HEVC or H.264 8-bit, the SDR look |
| `TV` | afp-stream to the Samsung | PQ, as designed |
| `Photo HDR` | iPhone, Photos | HEIC with a gain map |
| `Live` | iPhone, Photos | HEIC + MOV paired; only PhotoKit makes it a Live Photo |
| `Motion` | Android | JPEG with the video inside (Motion Photo) |
| `JPEG` | anything else | SDR JPEG |

- **Who picks the profile:** the destination, not the user. The user can override it.
- **The profiles are a configurable list** (decided). Each profile's values (size, codec, bitrate, look) can be
  changed in the settings, and profiles can be added. It starts with the ones above plus a `Light` one for messengers'
  size limits (1080p, a lower bitrate). A changed profile changes its renders' recipes, so they go stale.

### Found by recipe, not by path (decided: Anton, 2026-11-11)

**The problem:** today a render's path mirrors its moment's. With albums, items move between folders all the time:
- Moving a moment leaves its render behind as an orphan.
- A copy of a moment in a second album has no render at all.

**Proposal:** key renders by their **recipe hash**, `_Renders/<hash>.<profile>.<ext>`, with the item's name in the
render's link for people reading the folder.
- Moving, renaming or copying an item keeps its render.
- Two copies of the same moment share one render.
- A changed transform gives a new hash. The old render becomes unreferenced, and cleaning removes it.
- **Renders are the library's internals.** It manages them itself, and the user never needs to know how they lie on
  disk. An item's renders show in Info, and the room they take shows in the Library.
- **Where they live:** see below. With the library-folder option they'd be hidden as `.afp/renders/`, since they're
  internals.

### Where they're stored (decided: Anton, 2026-11-11)

**Two settings of the library: where renders live, and where pinned renders live.**

**Renders: this computer, by default.**
- **Where:** each computer keeps its own in its own cache (`~/Library/Caches/…` on a Mac, `%LOCALAPPDATA%` on Windows)
  and manages them itself.
- **The NAS's streamer counts as a computer of its own.** afp-stream keeps its `TV` renders on the NAS, in its own
  place. A Mac doesn't need them, and the NAS doesn't need the Mac's.
- **Why not the library's folder:** the NAS folders with the originals are backed up, and renders there would pile up
  in every backup. A Mac's `~/Library/Caches` is also left out of Time Machine.
- **What it costs:** two computers render the same moment twice. That's accepted.
- **Option:** the library's folder, for a user who wants all renders shared.

**Pinned renders: the library's folder, by default.**
- **What pinning is:** a tick, *Keep rendered*, on an item or an album.
  - Its renders are made once, on a computer that can make them.
  - They're kept in the library, where every device finds them, and never evicted.
- **What it's for:**
  - A clip too heavy to play live anywhere but the Mac: pin it, and the phone, the TV (afp-stream, Jellyfin) or a weaker
    laptop play the render, looking as it should rather than in some degraded form.
- **Backups:** pinned renders do go into backups. That's accepted: they're few, chosen, and expensive to make. To keep
  them out, exclude their folder from the NAS's backup by hand. A visible name (`_Renders/`) makes that easier to find
  in a backup app than a hidden `.afp/renders/`. *(Open: which name.)*
- **Option:** pinned renders on this computer only.

**Lookup:** the library's pinned renders first, then this computer's own. afp-stream looks the same way, so a render
pinned from the Mac is what the TV plays, adjusted to the TV as it streams.

**Which profiles a pin makes:** a library setting, `HDR` alone by default, changeable in the settings. No `TV`
render is pinned. `TV` is only the streamer's adjustment to the television (PQ, gain), and afp-stream makes it on the
fly from the pinned 4K `HDR` render. *(To check: that the N100 converts 4K HLG → the TV's PQ in real time with
QuickSync; if not, at 1080p.)*

In the UI, pinning is one click with no choice. The detailed UI comes later.

**Apple Photos needs no pinned renders.** Photos imports what it's given and keeps its own copy. Sync renders only
what's new (or changed), hands it to Photos, and the render is no longer needed: it's an ordinary, evictable render.
Telling what changed goes by the recipe kept with the pair (item ↔ Photos asset), not by the render file.

### Keeping and cleaning

- **Pinned** (*Keep rendered*): never evicted.
- **Everything else is evictable:**
  - on a computer, by a size budget, oldest first, plus stale and unreferenced ones;
  - in the library's folder, only by *Clean renders*: stale or unpinned ones. Unpinning makes a render cleanable.
- **The Library shows:** the room renders take on this computer, per album; how many are current, stale or unreferenced;
  and a Clean button.
- **After an update that changes the picture** (render version raised): renders go stale but aren't remade eagerly.
  They're remade the next time they're needed (Share, sync, playback), and a pinned album can be re-rendered on
  request.

### Who renders, and when

- **The player, headless,** in the background queue. Today's mechanism, on the computer you're at.
- **Never by default:**
  - Share renders what it needs, then hands over.
  - A pinned album renders its new items by itself; an album synced to Photos renders its new items, hands them
    over, and keeps nothing.
  - *Render* on a selection, to prepare ahead.
- **The NAS (N100) overnight:** possible only for a GPU-warp path (stabilization on its CPU runs at ~17 fps at best).
  *(Open, later.)*

### Using them

- **Share:**
  - *Save to a folder* copies the render (an APFS clone, or a server-side copy on the NAS).
  - Other destinations hand over the render file itself.
  - A saved copy is the user's file; its `kind: "export"` link only says what it is.
- **Playback:** play the current render matching the screen's output (designed in §4, *Playing from a render*). The
  original takes over as soon as anything in the recipe changes.
- **Apple Photos and the TV:** they get the `HDR` / `TV` render of each item, as designed.

## Culling and tags

(Decisions in the comment on PR #197)

- **Accept/Reject** and **stars** are two sets of defined tags; the rest are the user's own. `X` in the player becomes
  Reject; it no longer deletes a clip preview.
- **Tag stores:**
  - The library's own.
  - Finder tags, two-way. On originals as an xattr; on moments and photos too.
    - Last change wins.
    - When both sides changed, compare ctime with the journal's time, else the library wins.
- **In Finder:**
  - Stars show as a `★…★` tag.
  - Accept/Reject show as coloured tags.
  - A per-library setting can come later.
- **Windows:** no OS tags, since Explorer would write into the files.
- **NAS systems' tags:** none.

## Finding

- **Telemetry** (#213): start, minimum, maximum and mean per clip. The start value is read with the file; the full
  statistics come from a separate command. All of it becomes filters and smart albums.
- **Timeline and map**: above.

## What changes from today

| Today | Then |
|---|---|
| Tabs: Originals, Footage, Moments, Photos, Exports | Originals, Albums |
| Footage = clip previews arranged in `_Clips` | Your albums, clips and moments and photos mixed |
| `_Clips`, `_Moments`, `_Stills` | shown as albums, as they are: nothing moves |
| `_Exports` | ordinary folders; no tab; exports are files the user saved |
| Clip previews made for every clip (look + organization) | no batch: thumbnails are fast now (#191); a clip link is made when you put a clip in an album |
| Previews "by trips" | the Timeline |
| `X` deletes a clip preview | `X` = Reject |
| A photo = an exported file, rendered at once | a photo object; rendered when shared. Existing photos: a one-off migration script on Anton's library |
| Export | Share → Save to a folder |
| Renders of moments, at a path mirroring the moment's | renders of clips, moments and photos, found by recipe hash; kept on each computer by default; pinned ones (*Keep rendered*) in the library |

## Not in this concept

- **Importing from a card:** by design. The card check (#211) instead.
- **Hand-off to editors** (#57): deferred, possibly a paid add-on. Being GPL, a paid part would be a separate program
  that reads our files.
- **NAS systems' tags; OS tags on Windows.**
- **Visual search ("manta") and faces:** not discussed yet.

## Questions settled (Anton, 2026-11-11)

1. **The photo preview:** small (around 1280 px), named `.afplink.jpg`.
2. **An item in two albums:** copies are separate items. An item has one album; tags and smart albums do the rest.
3. **Share's order** (#214): we list the services ourselves. You choose, it renders in the background, then it hands
   over.
4. **Tag sync over SMB:** as proposed.
   - Last change wins.
   - When both sides changed, compare ctime with the journal's time, else the library wins.
   - No UGREEN-specific work. Checking for `._` files is a single look while building.
5. **The telemetry summary** is stored in `<clip>.afp.json`, and the catalog carries it (#213).
6. **The Timeline** lives inside Albums.
7. **Renders:**
   - found by recipe hash;
   - the library's internals;
   - kept on each computer by default;
   - pinned ones (*Keep rendered*) in the library;
   - pins make `HDR` by default (a setting), and the streamer makes `TV` from it on the fly.
8. **Profiles:** a configurable list, with a `Light` profile for messengers among them.
9. **Photo originals:** not now. The section above records the intention.

## Still open

- **The pinned renders' folder name:** `_Renders/` (visible, easy to exclude from a backup by hand; my
  recommendation) or `.afp/renders/`.
- **To check while building:**
  - the N100 converting 4K HLG → the TV's PQ in real time;
  - whether ctime moves over SMB;
  - whether Telegram and WhatsApp appear in the share menus.
