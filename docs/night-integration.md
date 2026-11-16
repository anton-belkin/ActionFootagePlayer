# The night's Library work in one branch (2026-11-16)

Branch `claude/night-integration-fd9085`: the night's branches merged, for one thing to try in the morning. **It is not
for merging**: the PRs are the seven below; fixes found here are listed so they can go to the PR they belong to.

## What is in it

Cut from `claude/night-media-types-703c0e`, then merged in this order, each a real merge commit:

| Branch | PR | Its commit | Merge commit here |
|---|---|---|---|
| `claude/night-albums-3c6687` (clip links, Albums) | #235 | d6d998a | in the base |
| `claude/night-media-types-703c0e` (photo links, moments and photos in albums, two tabs) | #238 | b274322 | the base |
| `claude/night-timeline-map-d4a2f0` (Timeline, Map) | #239 | 59d5add | 574c7ea |
| `claude/night-responsive-894fc6` (screen first) → `claude/night-responsive-pinpoint` (pinpoint update) | #236, #237 | e58ef55, 670fd4a | 48724da |
| `claude/night-culling-249140` (culling) → `claude/night-default-library` (Open at start) | #233, #234 | 3a8f9e6, 2b276f9 | 61384b1 |

| `claude/night-share-ca5050` (Share, the render cache) | #240 | e735616 | 5c221ee |

## Conflicts, and how each was resolved

**Timeline / Map into media types**: none.

**Responsiveness (#236, #237)**:
- `library/src/main.rs`: both `mod albums;` and `mod changed;`.
- `library/ui/index.html`: `writes.js`, `changes.js` first (as their note asks), then `albums.js`, `albumviews.js`,
  `timeline.js`, `map.js`, then `app.js`.
- `app.js` `deleteSelection`: the screen-first way (`Writes.run("delete", …)`), with the albums' words ("the link of
  …", "the originals are untouched").
- `app.js` `commitRename`: the screen-first way, with the albums' rule kept: an album is up to three folders
  (`_Clips/X`, `_Moments/X`, `_Stills/X`), each renamed by its own `renamefolder` change; an item's view is by its
  kind (a moment renamed in an album is renamed in `moments`).
- `app.js` `applyView` / `showView`: the split kept; the Footage scan's `links` kept in `RAW_FOOTAGE`; after another
  view changes, only the Albums view is made again from memory (`albumsAgain()`), and the clips' counts of moments
  the pinpoint way.
- `app.js` `delrenders`: `Writes.run("renders", …)` on `ctx().moments` (the moments among an album's selection).

**Culling, Open at start (#233, #234)**:
- `index.html`, `style.css`: both sides (scripts: `writes`, `changes`, `albums`, `albumviews`, `timeline`, `map`,
  `culling`, `atstart`, `app`).
- `main.rs` `set_tags`: culling's body (the tags remembered as recent) answering with `Changed` (the pinpoint way);
  the handler list has both sides' commands.
- `app.js` filters: culling's **Keep it?** and **Stars**, offered by the items' kinds instead of "not in the Exports
  tab"; the Renders filter stays the media-types way (`has("moment")`).
- `app.js` `setTags`: `Writes.run("tags", …)` on culling's choice of items, then the recent tags read again.
- `app.js` inputs: both (`albumname`, `tagfrom`, `tagto`).
- **The key `X` in the player** (`mpv/scripts/workspace/main.lua`, `side-panel/main.lua`, `README.md`): albums made
  `X` "this clip out of its album", culling made `X` **Reject**. Here `X` is Reject; taking a clip out of its album
  is the menu's item and the Project tab's button (binding `workspace/not-interesting`, no key). Culling's
  "Remove / Make this clip's preview" is gone with the previews themselves (albums replaced them).
  `tests/sidepanel_test.lua` and `footage-menu/main.lua` follow.

**Share and the render cache (#240)**:
- `index.html`, `style.css`: both sides (`share.js` after `albums.js`).
- `index.rs` `read_json`: shared with `renders.rs` (#240) and still counted by the NAS model (#237).
- `main.rs` `delete_renders`: the render cache's delete (#240), answering with the moments and photo links as they
  are now (#237). `changed.rs`, `perf.rs` call `renders::delete`.
- `app.js` filters: culling's Keep it? and Stars, then Share's Renders filter with its Photo column.
- `app.js` `delrenders`: `Writes.run("renders", …)` on the moments **and photo links** selected.
- `tests/albums.js`: the scripts' check names both sides' scripts.

## Cross-feature work done here (not in any PR)

- **The pinpoint update knows albums** (`changes.js`): `showItems("footage", …)` patches the clip in Originals and
  every copy of it in albums; for a moment, photo or export it patches the item and its copy in its album; Timeline's
  and Map's cache is dropped. The `delete` and `renamefolder` changes carry the clips' links, so a clip taken out of
  an album, an album deleted or renamed shows at once. A link deleted lands in what the page holds of the disk
  (`linksGone`).
- **Album commands read no whole project** (`albums.js`): "Take out of the album" for clips is a `delete` change
  (at once); `album_add`, `album_copy`, `album_move`, `photo_add` are followed by reading only the views they
  touched (`albumRead`), not `refresh()`. They still wait for the command before anything shows: see Unfinished.
- **A link made elsewhere** (`changed.rs` `of_files`): a clip's link made or changed by the player or another
  computer makes the page read Footage again (its list of links is that scan's); one gone does the same from the page.
- **Culling on the screen-first path** (`culling.js`, `main.rs`): Accept / Reject and the stars are `Writes` kinds
  `pick` and `rating`; `set_pick` and `set_rating` answer with `Changed`. No `refresh()` after a key any more.
- **Culling without the removed tabs**: no `S.view === "exports"` left; exports are not cullable by their kind.
  Works on album items of every kind, in Timeline and in a Map place's list (they are the same items).
- **Share from Timeline and from a Map place's list**: "Share…" is on their bar (`albumviews.js`) and in the menu;
  it takes the selection by kind, so it works there and on an album's mixed selection as it is.
- **Timeline's tiles show the verdict**: a small ✓ / ✕ and the stars on the thumbnail (`cullBadge`; the tiles have
  no room for chips). "Add to an album…" (not "another") in Timeline and Map.

## Tests (all on c58326d, with Share merged)

| What | Result |
|---|---|
| `cd library && cargo test` | 56 pass, 0 fail, 13 ignored (the ones needing a player or a NAS) |
| `cd catalog && cargo test` | 32 pass (9 + 1 + 22) |
| Windows: `PATH=/opt/homebrew/opt/llvm@19/bin:$PATH cargo check --target x86_64-pc-windows-msvc` in `library/` | compiles (one warning, an unused variable) |
| The player's Lua tests named in `.github/workflows/release.yml` (18 files, luajit) | all pass |
| `node library/tests/albums.js library/ui` (#235, #238) | 77 checks pass (#240 turned its Export checks into Share's) |
| `node library/tests/share.js library/ui` (#240) | 31 checks pass |
| `node library/tests/views.js library/ui` (#239) | 45 checks pass (one added: Share… in Timeline) |
| `library/ui-tests`: `node --test *_test.js` (#236, #237 + `merged_test.js`) | 31 tests pass |
| `node library/ui-tests/culling_test.js` (#233) | 87 checks pass (was 72) |
| `node library/ui-tests/atstart_test.js` (#234) | 27 checks pass (was 24) |

jsdom and puppeteer-core were installed in a scratch folder (`NODE_PATH` / `JSDOM` / `PUPPETEER`), as the branches'
notes say; nothing was added to the repo for them.

**Expectations changed because of the merge** (the test still checks the same thing, in its new place):
- `tests/albums.js`, `tests/views.js`: the list of scripts in `index.html` (more scripts, `writes.js` first).
- `tests/views.js`: Timeline's menu says "Add to an album…".
- `tests/sidepanel_test.lua`: the Project tab's button is "Take this clip out of its album", not the preview button.
- `ui-tests/writes_test.js`, `pinpoint_test.js`: clips are acted on in Originals, moments in an album (the Footage /
  Moments / Photos tabs they clicked are gone); the Tags part's buttons are culling's; "a folder renamed" is "an
  album renamed"; a clip's link deleted elsewhere expects one read of Footage (links aren't said item by item).
- `ui-tests/culling_test.js`: clips culled in Originals, moments and photos in Albums; no read of the views after a
  change; a write that failed is said in the "Not saved" strip, not the error box; the Exports-tab half is dropped.
- `ui-tests/atstart_test.js`: plus "opened at start on Originals + Albums".
- New `ui-tests/merged_test.js` (8): a tag / P / stars on a clip's link in an album shows there and in Originals at
  once with no view read; the same on a moment and a photo; P / X / digits in Timeline; Take out of the album.

One real bug found by the tests and fixed (`timeline.js`): Hide rejected changed nothing in Timeline until some item
changed (its cached model didn't know the switch).

**A share run the whole way, with the player and no window** (`share_e2e::a_share_with_the_player` in `main.rs`,
skipped without `AFL_PLAYER` and `AFL_SHARE_TRIP`). Run once here on a scratch copy of the installed player with
this branch's Lua scripts put in (only Lua changed in the player tonight) and a 4 s generated 1080p clip, sharing a
moment, a photo link and the whole clip to a folder:
- **The photo link: worked end to end.** The Library planned it, the player's `share` batch rendered it
  (`photo-at` with the link), it was renamed out of `.making/` to `_Renders/<hash>.Photo.heic` (1920×1080 HEIC,
  108 KB) and copied to the folder as `A_1.00s.heic`. Run twice (two recipes): both times in 2–3 s.
- **The moment and the whole clip: not rendered.** The batch asked the exporter for both, and `gyroflow-export`
  refused each: "Unsupported file format, and not log: nothing to change" (the generated clip has no motion data
  and no log curve). The refusal came back per item and the share ended Done with two failures, as designed. So
  the video path is proven up to the exporter's call and back, **not** a video actually landing in the cache: that
  needs a real camera clip. Two things this showed: (1) Share of a clip or moment with nothing to change fails
  instead of copying the original; (2) with `source.curve` set in the clip's own file the Library's recipe changed
  but the batch still treated the clip as not log, so the recipe and what the player renders can differ (as #240's
  note suspects).
The scratch player copy was deleted and unregistered after.

**Pictures**: a headless Chrome walk of the merged page against the mock, light and dark, 70 pictures (the Share
sheet, its progress, its row in Tasks and Share from Originals among them): the albums walk (`tests/shots.js`), Timeline and Map (`tests/views_shots.js`), and a culling walk (Originals, Albums' top, the
album Egypt with a clip, a moment and a photo, the Tags part, X and 4 stars on the three, Hide rejected, P and 5 in
Timeline). All looked at. No page error, no "Not saved", nothing overlapping, the top bar holds both branches'
controls on one line at 1320 px, the map's tiles loaded. Seen and left:
- the Info part's head says the count twice for several items selected ("3 items" / "3 items"): from #238, not the merge;
- with the Filters panel open in an album, "Take out of the album" wraps to a second row of the bar (clean);
- in `timeline-trip-selected` a day's header is half under the bar after a scroll (as in #239's own picture).
They were not committed (the branches' own pictures are in `docs/img/`).

## The trial build

`~/afp-night-build/ActionFootageLibrary.app`, built by `tools/make_library_app.sh` from c58326d (Share included) (release, signed ad
hoc). **The Library alone**: the app, `afp-thumbs`, `afp-quicklook`, `afp-photos`. Not in `~/Applications`, never
launched. macOS registered it with Launch Services by itself when it was signed; it was unregistered right after
(`lsregister -u`), and the folder has `.metadata_never_index`, so `Project.afproject` still opens the installed app.
Launching it will register it again: after trying, `lsregister -u ~/afp-night-build/ActionFootageLibrary.app` (or
delete the folder) if a double click on a project starts opening this copy.

To start it:

```bash
open ~/afp-night-build/ActionFootageLibrary.app
```

What to know before starting it:
- **It uses the installed player** (`~/Applications/ActionFootagePlayer.app`: the Library looks beside itself, then
  there) for probing, frames, renders and for opening clips. That player is the released one: it has **none of the
  night's player changes** (P / X / U and the stars in the player, a photo link opening at its frame, the album
  button). Building the player was left out: `tools/dev_app.sh` builds mpv and the plugins and registers the result.
  To try those, run `APPS_DIR=~/afp-night-build tools/dev_app.sh` from this branch (it registers that copy).
- **It shares this computer's data with the installed Library**: the projects opened before (`recent.json`), the
  thumbnails, this computer's file for each project. "Open at start" marked here is kept in `recent.json`; the
  installed (older) Library was not checked against a `recent.json` that has the mark.
- **It writes to the project it opens**: links in `_Clips/`, tags in the originals' own files and the links. The
  project's version is not bumped by any of the branches, so the installed Library opens the project afterwards.
  For a first look, a copy of a small project (or a new one over a folder of clips) is the safe way; on the real
  library start with things that only read (Timeline, Map, Originals).

## What to click first, for each of the seven

1. **Albums, clip links (#235)**: Originals → select a few clips → *Add to an album…* → a new name. Albums → the
   album: the clips are there in a moment, each a 20 KB link with its picture. Finder: `<project>/_Clips/<album>`.
2. **Photos as links, moments and photos in albums, two tabs (#238)**: the top bar has Originals and Albums only.
   Albums → Generated: the moments and photos made before. Select a moment → *Move to an album…*. In Originals
   select a clip with marks → the side panel's Moments & Photos → *Photos of the marks*: links, at once.
3. **Timeline and Map (#239)**: Albums → the two tiles at the top. Timeline: Days / Months / Years, a trip's line.
   Map: pins, zoom, a click on a pin lists its items; *Places* for the list.
4. **Culling (#233)**: select anything (Originals, an album, Timeline) → `P` accept, `X` reject, `U` undecided,
   `1`–`5` stars, `0` none; the Tags part of the side panel has the same as buttons; *Hide rejected* and *★ first*
   in the top bar; Filters → Keep it? / Stars.
5. **Open at start (#234)**: the project menu (top left) → the tick beside a project; quit and start again: it
   opens by itself, on Originals.
7. **Share and the render cache (#240)**: select anything in an album (or in Timeline) → *Share…* on the bar →
   HDR or SDR → *Save to a folder…*. The sheet lists each item; closing it leaves the share in Tasks. Shared again,
   what was rendered is copied at once. With the installed player a share that needs rendering will not work (its
   scripts don't know the `share` batch): only what is already rendered, rendered photos and exports are copied.
6. **The Library answers at once (#236, #237)**: on a project on the NAS, tag a clip, rate it, rename a moment,
   delete one: it shows at once, "Not saved: …" appears only if the write failed; closing the window while changes
   are being written waits for them.

## Broken, unfinished, not verified

Not verified at all (night rules: no window, no NAS, no real library):
- **The app's real window.** Nothing was launched. Everything above is jsdom and headless Chrome against mocks, and
  Rust tests on temp folders. In particular: the map's tiles through the app's CSP, the keys in WKWebView, the
  window's close while changes are written, the file-open event against "Open at start".
- **The NAS and real data**: speed of anything over SMB, the catalog's own thread feeding `changes_since`, Finder
  tags on the share, real place names and counts, 10-bit HEVC frames for photo links.
- **The player's side** of culling and photo links: compiled (`luajit -b`) and unit-tested, never run with a window;
  and not in the trial build (see above).
- **Windows**: compiles; nothing run.
- The one-off script `tools/one_off/photos_to_links.py` was not run on anything real.

Unfinished, known:
- **Share needs this branch's player to render** (see the trial build); no video render was seen landing in the
  cache (above). Renders nothing refers to any more stay in `_Renders` (no cleaning yet, #240's note).
- **Adding to an album, copying, moving, photo links are not screen-first**: the page waits for the command, then
  reads the views it touched (Footage: one scan; as a NAS that was measured at seconds). Only taking a clip out,
  deleting and renaming show at once. The commands should answer what they made (`Changed` with the new links).
- After `album_add` the catalog's notice may make the page read Footage a second time (the command's own files).
- Renaming a tag over the whole project (`rename_tag`) and Sync tags still end with `refresh()`.
- Renaming an album that is three folders is three writes: if the second fails, the album shows as two until read again.
- A clip's second link made **elsewhere** while its first is unchanged is seen (Footage is read again); a link
  **renamed** elsewhere relies on the same path and was not tested.
- The Info part's count said twice; the side panel still offers clip parts (Source, Location, Stabilize) for a
  moment selected in Timeline or Map (as #239's note says).
- `library/ui-tests/page_perf.js` and `shot.js` (#237's measuring and pictures) still click the removed tabs: not
  adapted, not run. The numbers in `docs/library-responsiveness.md` are #237's own, not this branch's.
- The star chip and "In an album" use the tags' chip style (`chip tag`).

For the PRs: the cross-feature fixes above belong after the seven are merged in the order of the table; the
conflicts will come back in that order and this branch's merge commits show each resolution.
