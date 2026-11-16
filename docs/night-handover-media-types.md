# Albums, step 2: photos as links, moments and photos in albums, two tabs (2026-11-16)

Branch `claude/night-media-types-703c0e`, cut from `claude/night-albums-3c6687` (PR #235, step 1:
`docs/night-handover-albums.md`). Design: `docs/albums-photos-share-concept.md`. This note is for the sessions that
build Share ("Save to a folder") and the render cache on top, and for merging with Timeline / Map.

## What was built

### 1. A photo is a link

`<clip's name>_<seconds>s.afplink.jpg`, written by `library/src/albums.rs`:

- `photo_file(jpeg, link)`: the picture as it is, plus one APP1 XMP segment holding `<afp:link>` (namespace
  `https://actionfootageplayer.ch/ns/1.0/`), placed after JFIF / Exif and before the image data. An XMP packet the
  picture already had is dropped.
- The link: `{ "v": 1, "kind": "photo", "clip": <path in the tree>, "t": <seconds>, "size", "roots", "created",
  "crop"?, "camera"? }`. `clip`, `t`, `size`, `roots` are what the player's rendered photos already carry
  (`gyroflow-export.rs photo_link`); `created` is new (as clip links have it); `crop` (x, y, w, h in shares of the
  frame) and `camera` (yaw, pitch, roll, zoom) are **carried when given, but nothing sets them yet**: see Unfinished.
- **Nothing new reads it.** The link is where a rendered photo's is, so `index::link` (Library), `inspect::photo_meta`
  (catalog) and `link.photo` (player) read it as they are. Tested: the Library's and the catalog's readers.
- `add_photos(root, album, photos, cache, luts)` and the command `photo_add { album, photos: [{ tree, path, thumb,
  size, created, curve, t, crop, camera }] }`: the picture is the clip's frame at `t` by the player's ffmpeg
  (`frame_at`: `-ss t`, at most 1280 px wide), through the clip's LUT when its curve is known (as clip links). It is
  the frame **as recorded**: not stabilized, not cropped. A clip that can't be read now gets its thumbnail instead, so
  the photo is made all the same. One at that time of that clip already in the album is skipped.
- The scan (`index::scan "stills"`) gives such a file `form: "Photo link"` and `linked: true`. A rendered photo is
  still a photo (`linked: false`, its form as before). A photo without a link is now also kept under `_Clips/` (so an
  old one moved into an album doesn't vanish).
- Rename keeps `.afplink.jpg` whole (`rename_in`).
- The player (`mpv/scripts/workspace/main.lua`, inside `open_moment`: the file is at Lua's 200 locals): a file named
  `*.afplink.jpg` with a photo link opens its original, paused at `t`; `state.photo = { file, t, crop, camera }` is
  there for the crop mode to pick up later. Rendered photos open as pictures, as before.

Existing rendered photos are untouched. `tools/one_off/photos_to_links.py` is the one-off migration the concept
describes: **written, not run on real data** (tried on a synthetic folder). Without `--go` it only lists. Its
`--move-renders` step puts the full files in `_Renders/Photos/<path>`: hold that until the render cache says where
photo renders live.

### 2. Moments and photos are items of albums

`library/ui/albums.js`:

- `ALBUM_SOURCES = [albumClips, albumMoments, albumPhotos, albumExports]`; the last three copy the items of
  `DATA.moments`, `DATA.stills`, `DATA.exports` (their scans are unchanged and still read) with keys `m:`, `p:`, `e:`.
- **An album is one name over three folders**: `_Clips/Egypt`, `_Moments/Egypt` and `_Stills/Egypt` are the album
  Egypt (`albumShown`, `ALBUM_REAL`, `albumRealDirs`). Nothing moves on disk. `Generated` (in each of the three, and
  the project's own `Generated/`) is shown as **Generated**, beside the albums, after them. Everything else in the
  project's folder is under Other folders at its path; `_Exports` is there.
- What's put in an album from the Library goes to `_Clips/<album>`, whatever its kind.
- Add / move / take out (`albumSel`, `albumMenu`, `albumBar`, `askAlbum`, `albumGo`, `albumOut`):
  - **Add to another album…**: a clip by a new link (`album_add`); a moment or a photo link by a copy of its link
    file (`album_copy` → `albums::copy_to`), a separate item from then on (the concept's "One item in two albums").
    A rendered photo is not copied.
  - **Move to an album…**: `album_move` → `albums::move_to`, now for every kind: a moment with its `.gyroflow` and
    its renders (they're found by the moment's path, so they follow it), a Live Photo with its `.mov`.
  - **Take out of the album**: a clip's link is removed; a moment goes to `_Moments/Generated`, a photo to
    `_Stills/Generated` (an item has one place; Generated is where what's in no album is).
- The filter by kind (`albumFacets`) appears once the view holds two kinds; the moments' Renders filter, the photos'
  kind and the exports' look appear when the view has such items. Kind + "List every match below here" at the top is
  "all moments" / "all photos".
- **Photos of the marks** in the side panel now makes links (`photoLinks` → `photo_add`), into the album the clips
  were selected in, else `_Stills/Generated`. The player's batch is the button beside it, "…rendered now". A mark in
  a clip's own view has "Make a photo of the mark".
- Renaming or deleting an album acts on each of its folders on disk (`foldersOnDisk`).

`library/ui/app.js`: `ctx()` gives `only` = the clips among the selection and `moments` = the moments;
`countText(list)` names a selection by its kind ("1 moment", "3 items"). Render / Export jobs, `delete_renders` and
Check take the moments; the clip jobs take the clips.

### 3. Two tabs

`VIEWS` is Originals and Albums. Where what the removed tabs did is now:

| Was | Now |
|---|---|
| Moments tab: browse, open, rename, tag, delete, Check | the same on a moment in Albums |
| Moments tab: Render, Delete renders, Export (side panel) | Export in the side panel, on the moments selected in Albums or in a clip's view |
| Photos tab | photos in Albums (in Generated until put in an album) |
| Exports tab | Albums → Other folders → `_Exports` |
| "Not where the link says" strip → the tab | → Albums, across folders, filtered |

Chosen for "in no album": **Generated**, at the top of Albums, plus the Kind filter across folders for an
"everything" listing. Smart albums (*All moments*, *All photos*) are not built.

## Extension points

- **Share**: `exportBody(c)` in `app.js` is the one place Render / Export are offered; it gets a `c` whose `selItems`
  are the moments. Photos have no render or export command yet: a photo link's recipe is its link (`clip`, `t`,
  `crop`, `camera`) plus the clip's own `source`, and the player's `photo-at` message (`gyroflow/export.lua`) already
  renders `{ clip, t, view, out_dir }`. `_Exports` has no special place left in the page except `exportBody`'s label
  and the job's destination in the player.
- **Render cache**: renders are still found by path (`index::render_path`, `renders_of`); `move_to` and `rename_in`
  move them along. Once they're found by recipe hash, drop those lines. `it.renders` on a moment is what the filter
  and the side panel read; a photo item has no `renders` yet.
- **A new kind**: a source in `ALBUM_SOURCES`, a line in `ALBUM_KINDS` and `KIND_NOUN`, a branch in `deriveItems`,
  and `item_of` in `albums.rs` if it's to be moved.
- **Timeline / Map**: untouched here (`ALBUM_VIEWS`, the view's top in `renderMain()`). Items of all kinds are in
  `DATA.footage.items` with `when`; photos' `when` is now the frame's time when the link has `created`.

## Tests

- `cd library && cargo test`: 43 pass. New: `albums::tests::photos_as_links_and_every_kind_moved_and_copied`.
- `JSDOM=<node_modules/jsdom> node library/tests/albums.js library/ui`: 78 checks, against `library/tests/mock.js`
  (which now has moments, photo links, rendered photos and an export).
- `PUPPETEER=<node_modules/puppeteer-core> node library/tests/shots.js library/ui <out>`: headless pictures; the
  ones in the PR are in `docs/img/library-media-types/`.
- Windows: `PATH=/opt/homebrew/opt/llvm@19/bin:$PATH cargo check --target x86_64-pc-windows-msvc` compiles.
- `luajit -b mpv/scripts/workspace/main.lua /dev/null` compiles.

## Unfinished

- **The player doesn't make photo links.** `Alt+s` and the crop mode's Enter still render the full photo
  (`gyroflow-export --photo`). So a photo's crop never reaches a link yet, and the concept's Snap / Crop keys are not
  built. The Library's links are whole frames.
- **A photo link's picture is the recorded frame**: not stabilized, not the look the player shows, HLG clips not
  tone-mapped.
- **Photo links have no render / Share.** Until Share exists, a full photo is made with "…rendered now" or in the
  player.
- **"Photos of the marks … rendered now" skips marks that have a photo link** (the player's `photos_made` counts any
  photo with a link).
- New albums' folders are made in `_Clips` only; a moment the player saves still lands in `_Moments/Generated`, not
  "in the album its clip was opened from".
- Tags on a photo link go through the existing `afp-meta` path (it rewrites a JPEG's XMP): not tried on a link file.
- Merging with #236/#237 (responsiveness): `album_copy`, `photo_add` and `albumOut` call `refresh()`, as step 1's
  commands do; `showItems("footage", …)` should return false for album items (see that branch's note). With #233
  (culling): it switches itself off for `S.view === "exports"`, which no longer exists, and counts tags over
  `DATA.moments` / `DATA.stills`, which still do.

## Not verified

- The app's real window, and anything in the player with a window: opening a `.afplink.jpg` in mpv (only compiled;
  whether `describe()` treats a `.jpg` as a clip there wasn't checked), Quick Look of one from the Library.
- A project on the NAS: speed of `photo_add` over SMB (each photo is an ffmpeg seek into the clip), the catalog
  picking the new files up.
- `frame_at` on real camera files (10-bit HEVC, D-Log M through the LUT): the ffmpeg arguments were run by hand on a
  generated H.264 clip only; the test uses the thumbnail fallback.
- Windows: compiles; nothing run.
- The migration script on Anton's library.
