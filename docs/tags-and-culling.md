# Tags and culling

Anton, 2026-11-11, on the first version (PR #197): tags need a product vision. What is tagged, where the tags live,
which place is the truth, how they sync with the systems' own tags, and how culling fits in. This is that vision,
written first; the code follows it. Correct it here.

## In short

- **What's tagged: originals, moments, stills.** Never a clip preview, never an export: those are derivatives made
  to organise or to deliver. A clip preview playing shows its original's tags and tags its original.
- **One model, three parts**: a **pick** (*Accept* or *Reject*, one or the other, or neither), a **rating** (1–5
  stars, or none), and **custom tags** (any words).
- **The library's own store is the truth**: the item's own file (an original's sidecar, a moment's link, a still's
  XMP). It's in the library, travels with it, and every computer reads the same.
- **Other stores are mirrors the library chooses** (Project settings → Tags): macOS Finder tags now; XMP sidecars,
  Windows and the NAS photo apps designed below. Each enabled store is **synced both ways**: a tag set in Finder comes
  into the library, a tag set in the library goes to Finder.
- **Culling is tagging.** `P` accept, `X` reject, `U` neither, `1`–`5` stars, `0` none, `K` a custom tag. The old
  "not interesting" (`X` trashing a clip preview) and the planned *Favorite* Finder tag are replaced by it.

## What is tagged

| Item | Tagged? | Why |
|---|---|---|
| An original (the camera's clip) | yes | the footage itself: what you keep or throw away |
| A moment (`.afplink.mp4`) | yes | a part you chose: it has its own worth |
| A still (`_Stills/`, photo, Live Photo, Motion Photo) | yes | a picture you made |
| A clip preview (`_Clips/`) | **no**: shows and sets its original's | an organisational copy of the original |
| An export, a render | **no** | made from a moment or an original, for delivery |

The player tags what plays: the original (also while its clip preview plays), or the moment. An export playing has
nothing to tag. The Library tags its selection; in Exports there is no Tags part.

## The model

| Part | Values | In the library's store (`<clip>.afp.json`, a link) |
|---|---|---|
| Pick | Accept, Reject, or neither | `"pick": "accept"` / `"reject"` (absent: neither) |
| Rating | 1–5 stars, or none | `"rating": 3` (absent: none) |
| Custom tags | any words, compared without case, the first spelling kept | `"tags": ["Turtle", "for family"]` |
| Sync state | what each store and the library last agreed on | `"synced": { "finder": [...] }` (see Sync) |

Accept and Reject are one choice: setting one clears the other. Rating and pick are independent (a rejected clip may
keep its stars; filters decide what you see). *Deleted in Photos* stays the app's own custom tag.

Stills keep custom tags in their XMP (`dc:subject`), as now. Their pick and rating go to `xmp:Rating` (−1 = rejected,
1–5) and `xmpDM:pick` (1 = accepted), the Adobe/darktable convention **(designed: afp-meta to write them)**.

## Where tags live: stores

A **store** is a place tags are kept. The library's own is always on; the others are chosen per library, in Project
settings → Tags, and kept in `Project.afproject` as `"tag_stores": ["finder"]`: the library's config, the same from
every computer (like its locations; a per-computer choice would make two computers sync differently).

| Store | Kind | Originals | Moments | Stills | Status |
|---|---|---|---|---|---|
| **Library** (sidecar, link, XMP) | the truth | `<clip>.afp.json` | its link | embedded XMP | built |
| **macOS Finder tags** | xattr `com.apple.metadata:_kMDItemUserTags` | on the file | on the file | on the file | built, two-way |
| **XMP sidecar** (Lightroom, Bridge, digiKam, darktable) | `<name>.xmp` beside the file | `clip.xmp` | — (has its link) | — (embedded) | designed |
| **Windows Explorer** | the file's own metadata | not possible without rewriting the video | `Xtra` atom in our own file | embedded XMP (read) | designed |
| **Synology Photos** | its database, web API | via API | via API | via API, or embedded XMP it reads | designed |
| **QNAP QuMagie** | its database | ? | ? | reads XMP on import (Photo Station did) | to find out |
| **UGREEN UGOS Photos** | its database | no keyword tags known | | | to find out |

### macOS Finder tags (built)

- An attribute beside the file's bytes: the video isn't written (its bytes and modification time stay; its change
  time moves). On the NAS, Samba keeps it as a stream (`vfs_fruit`/`streams_xattr`); UGOS's share **to try**.
- **Mapping**: a custom tag is a Finder tag of the same name. *Accept* is the Finder tag `Accept` (green), *Reject*
  `Reject` (red), a rating the tag `★★★` (as many stars). A Finder tag's colour is kept as it is; the library doesn't
  keep colours.
- Read back: `Accept` / `Reject` (any case) is the pick (both: Reject), a tag of only ★ is the rating (several: the
  most), every other tag a custom tag. So a tag added by hand in Finder comes into the library.
- Spotlight finds them on a local disk; on a network volume Finder shows them but Spotlight doesn't search.

### XMP sidecars (designed)

For the editors and DAMs (Lightroom Classic, Bridge, digiKam, darktable) and for Windows' and Linux's tools:
`<clip>.xmp` (Adobe's naming; `photoshop:SidecarForExtension` says which file when two share a base name), with
`dc:subject` (tags), `xmp:Rating` (−1 reject, 1–5), `xmpDM:pick` (1 accept, −1 reject: Lightroom Classic 13.2+).
Lightroom doesn't read sidecars for videos (its catalog keeps them); digiKam can (its "sidecar for read-only files"
mode), darktable imports them. The video is never written. Synced both ways like Finder's.

### Windows Explorer (designed)

Explorer shows Tags (`System.Keywords`) and Rating for MP4, but its MP4 handler keeps them **inside the file** (an
`Xtra` atom in `moov/udta`), so writing them rewrites the file. Alternate data streams aren't read by Explorer since
Vista. So: **originals: no** (the rule: the video is never written); a custom property handler reading sidecars is
possible but fragile and system-wide, not planned. **Our own files** (moments): the `Xtra` atom could be written as
afp-cut writes the link key, for Explorer's Tags and stars **(designed)**. Stills: Explorer reads the embedded XMP
keywords of a JPEG (HEIC: shown, not editable).

### NAS photo apps (designed)

- **Synology Photos** keeps tags and ratings in its database. It reads embedded XMP `dc:subject` / `xmp:Rating` of
  photos, not sidecars, and may write tags into photos (so never point it at originals with write-back). Its web API
  (undocumented: `SYNO.API.Auth` login, `SYNO.Foto.Browse.Item` `add_tag`/`set`, `SYNO.Foto.Browse.GeneralTag`) can
  sync both ways: a store that logs in with an account kept in this computer's keychain.
- **QNAP QuMagie** keeps tags, rating, colour label in its database (exportable); no public API found. Its import
  reads embedded XMP. Designed as "our stills' embedded XMP is what it reads".
- **UGREEN UGOS Photos**: no keyword tags or API found (its Files app has its own file tags, stored unknown). **To
  find out on the NAS.**

## Sync

Each enabled store is synced with the library item by item, **three-way**: the library now, the store now, and what
they last agreed on (`synced.<store>` in the item's own file; absent: nothing agreed yet, an empty set).

- **Custom tags: union of the changes.** What either side added since the agreement is added, what either removed is
  removed. A tag added in Finder and another removed in the library: both happen.
- **Pick and rating: the side that changed.** Changed on one side only: that side's. Changed on both, differently:
  **the last writer** — the library's file's modification time against the store's change time (a Finder tag changes
  the file's change time, `ctime`). A tie: the library's.
- The result is written to both and becomes the new agreement. Nothing differs: nothing is written.
- Writing is idempotent: our own writes, noticed again, find nothing to do.

**When it runs:**

- **On every change** made in the player or the Library: the library is written, then each enabled store.
- **When the player opens an item**: its stores read, changes from outside brought in (one `getxattr`).
- **The Library's watch**: an attribute changed on this Mac's disks (FSEvents `ItemXattrMod`) → that item synced.
- **On the NAS** FSEvents report only this Mac's own changes, and a tag changes neither the file's nor its folder's
  modification time, so the catalog's looks don't see it. So: *Sync tags* (Project → over the selection or all), and
  **(designed)** the catalog's deep look reading each media file's Finder tags into its record (`finder` in the
  file's meta, with `ctime`): a difference from the agreement then queues that item for sync, and the views can show
  and filter Finder's tags without reading attributes. The journals carry those records to the other computers.

## Culling

One model replaces the culling-like features there were:

| Before | Now |
|---|---|
| `X`: a clip "not interesting": its clip preview to the Trash, `X` again makes it again | `X`: **Reject** what plays (the original or the moment); again: neither. The preview stays (it's not culling any more: *Remove this clip's preview* is a menu item) |
| The planned `K` *Favorite* Finder tag (green) | `P`: **Accept** (Finder's green `Accept`) |
| "A moment that exists is liked" | still true; Accept / Reject / stars say more |
| Library tags + tag filter | Pick, Stars and Tags filters; the selection gets Accept / Reject / stars / tags |
| Marks (`M`) | unchanged: points in a clip, not a verdict |

Filters: **Pick** (Accepted, Rejected, Not picked), **Stars** (n or more, none), **Tags**. "Hide rejected" is the
Pick filter with Rejected off.

## Keys (player)

| Key | | Was |
|---|---|---|
| `P` | Accept (again: neither) | mpv's *show progress* (the panel's bar shows it) |
| `X` | Reject (again: neither) | "not interesting" (see Culling) |
| `U` | Neither: the pick taken off | free |
| `1`–`5` | That many stars (the same again: none) | mpv's contrast, brightness, gamma (not for log footage) |
| `0` | No stars | mpv's volume up (`9` stays volume down; `/` `*` too) |
| `K` | A custom tag, typed (`-name` off, `+name` on, commas; `*3` stars) | free |

In a Cyrillic layout `З`, `Ч`, `Г`, `Л` pass on as `P`, `X`, `U`, `K` (any-layout). The Project tab shows the same:
Accept / Reject, the stars, the custom tags as chips, the recent ones to add, *Tag…*.

## Open questions

1. `tag_stores` in `Project.afproject`: an exception to "the project keeps only its locations". Right?
2. Finder's names `Accept` / `Reject` / `★★★`: English, fixed. A setting?
3. Digits for stars take mpv's video equalizer keys and `0` volume up. OK?
4. Exports without tags: the old tags in export links are ignored now. OK?
5. Which designed store next: XMP sidecars (editors) or Synology Photos (the NAS)?
