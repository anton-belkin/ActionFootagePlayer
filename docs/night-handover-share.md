# Share and the render cache (2026-11-16)

Branch `claude/night-share-ca5050`, cut from `claude/night-media-types-703c0e` (PR #238, itself on #235). Design:
`docs/albums-photos-share-concept.md` ("Share replaces export", "Renders"). Before this: `docs/night-handover-albums.md`,
`docs/night-handover-media-types.md`.

## What was built

### 1. The render cache (`library/src/renders.rs`)

A render is the pixels of one item (a moment, a photo link, a whole clip) in one profile. Its **recipe** is everything
that decides those pixels; the recipe's hash names the file:

    <project>/_Renders/<hash>.<profile>.<ext>        hash: 24 hex digits of SHA-256

- Profiles: `HDR` and `SDR` (videos, `.mov`), `Photo` (a photo link: `.heic` where the system writes one, else `.jpg`).
- `renders::find(ctx, item, profile)` → the recipe, its hash, the current file if there is one. Asking again costs a
  look at one name. A changed recipe is another hash, so another file; the old one is left (see Unfinished: cleaning).
- An item's name and place are not in the recipe: moved, renamed or copied items keep their render, and two copies of
  a moment share one.
- **Only the Library hashes.** It gives the player the file to write and the link to put inside
  (`renders::link`: kind `render`, `id` = the hash, `recipe`, `profile`, the item's `name` and `item` path when it
  was made, for people reading the folder). A photo's render keeps kind `photo` (gyroflow-export sets it) with these
  under `render`.
- `it.renders` (`index::scan`) comes from `renders::states`: `{ HDR: "current" | "stale" }` for moments as before,
  and now `{ Photo: "current" }` for photo links. The Renders filter has a Photo column when the view holds photo
  links. The cache's own files are never listed as photos or exports.
- `delete_renders` takes moments and photo links.

**Renders made the old way are found by the path lookup, kept as a fallback: there is no migration script.** A file
at `_Renders/<the moment's path>.<profile>.mov` whose v1 recipe (`index::render_recipe`, unchanged) is still the
moment's counts as current and is what Share copies; one whose recipe isn't is "stale". `rename_in`, `move_to` and
delete still move those files along with their moment, because the fallback is by path. Once nothing of the old kind
is left, those lines and the fallback in `renders::find` can go.

### The recipe's exact fields

Empty values are left out; numbers are written one way (whole ones without a fraction, others to six places), keys
in order, then hashed.

| Field | In | What |
|---|---|---|
| `v` | all | `renders::VERSION` (2). Raise it when the renderer's pixels change |
| `kind` | all | `moment`, `photo`, `clip` |
| `clip.hash` | all | the original's fingerprint, when the item's link has it |
| `clip.path` | all | the original's path in the project's tree, only when there is no `hash` (clips, the Library's photo links) |
| `clip.size` | all | the original's size (the link's, else the file's) |
| `in`, `out` | moment | the part |
| `stab`, `alone` | moment | the moment's own transform bundle (stabilization, level, horizon, aspect, projection, lens, denoise…) and whether it's stabilized on its own |
| `t`, `crop`, `camera` | photo | the frame, its crop (x, y, w, h in shares of the frame), its camera (yaw, pitch, roll, zoom) |
| `view` | photo, clip | the clip's saved transform (`view` of `<clip>.afp.json`) |
| `input` | all | the clip's source settings (`source` of `<clip>.afp.json`: curve, rotation, underwater, lens type) |
| `gyroflow` | all | fingerprint of the Gyroflow project used: the moment's own, or the clip's (`<stem>.gyroflow` beside it) |
| `defaults` | all | the player's own defaults from its `gyroflow.conf`, as written there: `view`, `level`, `horizon_lock`, `chromatic`, `aspect`, `orientation`, `projection`, `lens`, `still`, `denoise_strong`, `temporal_denoise`, `edges`, `edges_blur` |
| `profile` | all | `{ name, look }`: HDR/`hdr`, SDR/`nbcu`, Photo/`photo` |

Not in it: the profile's size, codec and bitrate. They aren't settings yet (the player fixes them per profile), so
`profile.name` and `v` stand for them. When profiles become the configurable list of the concept, their values go
into `profile`.

### 2. Share (`library/src/share.rs`, `library/ui/share.js`)

One command on any selection in Albums and Originals: clips, moments, photo links, rendered photos, exports, mixed.

- **Where it is:** "Share…" on the bar above the list and in the context menu; the side panel's Export part is now
  **Share** (what's selected by kind, what the cache has, "Share…", Delete stale / Delete renders).
- **The sheet:** what (counts by kind, "rendered already" / "to render"), the videos' look (HDR or SDR, remembered),
  where: the list `SHARE_DESTS`, today one entry, **Save to a folder…**. The folder is asked by the system's dialog.
  Then the sheet shows each item's state (Waiting for its render, Rendering, Copying, Saved, Failed with why), and
  "Show the folder". Closing it leaves the share running; it's a row in Tasks with the same list.
- **The command** `share { items: [{ kind: moment | photo | clip | file, path, name }], dest: "folder", folder?, look? }`:
  1. `share::plan`: each item's render looked up. A hit is copied at once. A `file` (a rendered photo, an export: its
     own pixels already) is copied as it is, a Live Photo's `.mov` with it.
  2. What the cache lacks goes to the player as one job of the new batch kind **`share`**: `items` each
     `{ of, name, file, clip, t, crop, camera, sdr, out, link }`. Two items of one recipe are asked for once.
  3. When the run ends, `share_collect` (the worker thread, away from the lock) copies what was made and marks the
     rest failed with the player's reason.
- **Copies** (`share::save`): under a readable name (`Turtle.mov`, `Turtle (SDR).mov`, `A_2.50s.heic`), never over a
  file that's there (`Turtle 2.mov`), written as `.name.part` then renamed. `std::fs::copy` clones on APFS.
- **Originals are never written.** Share reads the cache and the items; the player reads the originals.
- **Another destination later**: a line in `SHARE_DESTS` (share.js), in `share::DESTINATIONS`, and an arm in
  `share::deliver`. None is built.
- **`_Exports`**: the page no longer offers Render or Export (`exportBody` is gone, with its "to _Exports/Generated"
  label), so the Library sends no `export` or `render` job. Files in `_Exports` stay and show under Other folders, as
  before. A render saved by Share into a folder inside the project is listed there too (the exports' scan takes kind
  `render` outside `_Renders`).

The player (`mpv/scripts/workspace/main.lua`, `gyroflow/export.lua`):
- `render_moments(root, profiles, items)`: with `items` it makes exactly those (a moment by `export-batch` as before;
  a clip the same without `from`/`to`; a photo by `photo-at`), each into the file the Library named.
- `photo-at` takes `crop`, `camera`, `link` and `still` (a still whatever Save frame's choice is). A photo is made in
  `<out>.making/` (gyroflow-export names photos itself) and renamed to `<hash>.Photo.<heic|jpg>`.
- The `render` and `export` batch kinds are still there (unreachable from the Library's page), and Shift+E in the
  player still exports to `_Exports`.

## Where I decided myself

- **The cache is `_Renders/` in the project**, flat by hash. The task said "under the project (`Generated/…`)"; the
  concept says `_Renders/<hash>.<profile>.<ext>` and also "this computer's cache by default, the library's folder for
  pinned ones". `_Renders` is the concept's name, is where renders already are, and is already kept out of the views,
  the project's move and the catalog's own area. The per-computer cache and pinning are not built.
- **No migration script**: the path lookup stays as the fallback (above).
- **The player's defaults are in the recipe** as `gyroflow.conf` has them, read by the Library
  (`~/Library/Application Support/ActionFootagePlayer/mpv/script-opts/gyroflow.conf`, `%LOCALAPPDATA%\…` on Windows,
  `AFL_PLAYER_OPTS` to point elsewhere). Without them a clip with no saved view would keep a render made under other
  defaults.
- **A folder gets HDR** (the concept: "the destination picks the render"), with SDR as the user's override.
- **Clips' renders aren't worked out in the views** (each needs its own file read): a clip has no `it.renders`; Share
  looks when asked.
- **A share copies after the whole run**, not item by item: a render being written can't be told from a finished one
  by its name.

## Tests

- `cd library && cargo test`: 47 pass. New: `renders::tests` (the same recipe → the same hash, each pixel-changing
  setting → another; hit, miss, moved item, changed recipe, a photo's two forms, the scan; a render made the old way)
  and `share::tests::a_selection_saved_to_a_folder` (hits copied, misses asked of the player, one render for two
  copies, names never overwritten, a Live Photo's video along, failures by item, originals untouched).
- `JSDOM=<node_modules/jsdom> node library/tests/share.js library/ui`: 31 checks (the bar, the menu, the side part,
  the sheet, the folder cancelled, progress, a failure, Tasks, files copied as they are, Originals).
  `library/tests/albums.js`: 77 (its Export checks became Share's).
- `PUPPETEER=<node_modules/puppeteer-core> node library/tests/shots.js library/ui <out> [dark]`: the Share scenes are
  in it; the PR's pictures are in `docs/img/library-share/`.
- Windows: `PATH=/opt/homebrew/opt/llvm@19/bin:$PATH cargo check --target x86_64-pc-windows-msvc` compiles.
- `luajit -b` compiles `workspace/main.lua` and `gyroflow/export.lua`.

## Not verified

- **No end-to-end render was run.** The player's `share` batch and `photo-at`'s new fields are compiled, not run:
  no headless player with this branch's scripts was set up tonight. Untried: a moment, a whole clip and a photo link
  actually rendered into `_Renders/<hash>…`, gyroflow-export accepting the `render` object inside a photo's `--link`,
  the rename out of `.making/`, the status a `share` run writes.
- The app's real window (the sheet was seen only in headless Chrome), and the system's folder dialog.
- A project on the NAS: the scan's cost for photo links (each clip's own file is read once per scan), copying to and
  from a share.
- Real camera files; Windows (compiles only).
- That the Library's recipe matches what the player really renders with in every case: the player also reads its
  session (`user-data/gyroflow/session`, none in a batch) and the dlogm plugin's look and curve, which aren't in it.

## Unfinished

- **Cleaning**: renders whose recipe no item has any more stay in `_Renders`. "Delete renders" removes the selected
  items' current ones; there is no "Clean renders" (unreferenced ones), no size shown, no eviction.
- **Per-computer cache, pinning (*Keep rendered*)**, profiles as a configurable list, `Live` / `Motion` / `JPEG`
  photo profiles: not built. A shared photo is a still.
- **Stale** is only told for renders of the old kind (by path). By hash a render is there or it isn't.
- A photo link's card is still the recorded frame, and the player still doesn't make photo links (as in the
  media-types note).
- A video saved by Share keeps the render's link (kind `render`), not an export's.
- `README.md` and `docs/library-management.md` still describe Export and `_Exports` as before.
- `tools/one_off/photos_to_links.py --move-renders` still files full photos under `_Renders/Photos/<path>`: with the
  cache by hash that step should go (or name them by hash). Still not run.
- Merging: `share.js` is a new script in `index.html` after `albums.js`; `app.js` calls it at lines marked
  `share.js`. The side panel's tab id `export` became `share`. With #236/#237 (responsiveness) `share` doesn't call
  `refresh()`; the page follows the job through `poll()`.
