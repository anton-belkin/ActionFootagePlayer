# Hand-over: culling (PR #233), night of 2026-11-15/16

**Archived:** night handover for culling (#233, replaces #197) and Open at start (#234, landed via #241), v0.14.0. Written 2026-11-16.

For whoever merges the night's branches (albums, responsiveness, this one) and for whoever continues culling.
Branch `claude/night-culling-249140`, PR #233, not merged. *(Now: merged via #241, v0.14.0)* It contains all of PR #197 (`claude/tags`, merged in) and
replaces it. The design is `docs/design/tags-and-culling.md`.

## What changed, by file

### `library/ui/culling.js` (new, loaded before `app.js`)
All of the Library's culling UI. It uses `app.js`'s globals (`S`, `DATA`, `RAW_FOOTAGE`, `VISIBLE`, `ctx`, `items`,
`invoke`, `renderAll`, `renderSide`, `renderMain`, `refresh`, `moveTo`, `keyOf`, `needViews`, `showError`, `esc`, `$`)
only when called. Its own state is the `CULL` object; its buttons carry `data-c` (not `data-a`) and have their own
`click`, `mousedown`, `input` and `focusout` listeners on `document`.

| Function | Called from | Does |
|---|---|---|
| `cullBody(c)` | `tagsBody` | the Tags part's HTML |
| `cullShown(it)` | `matches` | false for a rejected item while Hide rejected is on |
| `cullHiding()` | `filtering` | Hide rejected is hiding something: the list counts as filtered |
| `cullSort(list)` | `renderMain` | ★ first: sorts in place, folders stay on top |
| `cullTop()` | `renderTop` (last line) | appends the two switches to `#top` |
| `cullKeys(e)` | keydown, before `listKeys` | P, X, U, 1–5, 0; true when taken |
| `cullDraftKeys(e)` | keydown, on `#tagdraft` | Down, Up, Enter, Tab, Esc in the new tag's field |
| `cullApply(list, change)` | its own buttons and keys | changes the items on the page at once, queues the write, `refresh()` after the last |
| `stars(n)`, `ci(a, b)` | `app.js` (`chips`, facets) | moved here from #197's `app.js` |

### `library/ui/app.js` (small touches only)
- `S`: `recentTags`, `tagFrom`, `tagTo`, `tagStores`.
- `buildFacets`: facets `pick` ("Keep it?": `pick:a`, `pick:r`, `pick:n`) and `stars` (`stars:5`…`stars:0`), in every view but Exports.
- `chips(it)`: Accepted / Rejected / stars chips.
- `tagsBody(c)`: now `return cullBody(c)`.
- `matches`: `cullShown(it) &&` in front. `filtering`: `|| cullHiding()`.
- `renderMain`: one line `cullSort(list);` before `let h = "";`.
- `renderTop`: last line `cullTop();`.
- `setTags`: `tagItems(c)`, re-reads `recent_tags`. `renameTag()` is #197's. #197's `setRating` / `setPick` are gone (culling.js does them).
- Project settings: a **Tags** tab (`S.stab === "tags"`), `openSettings` reads `tag_stores`, click cases `renametag`, `synctags`, `savetagstores`, change case `tagstore`, inputs `tagfrom` / `tagto`.
- keydown: `cullDraftKeys(e)` before the old `tagdraft` Enter line; `cullKeys(e)` before `listKeys(e)`.
- start-up: `S.recentTags = await invoke("recent_tags")`. `loadProject`: `S.tagStores`.

`index.html`: `<script src="culling.js">` before `app.js`. `style.css`: one block ("Culling and tags") replacing #197's `.stars` / `.picks` rules.

### `library/src/tags.rs`
- `Model { pick, rating, tags }`, `Op::{Tags, Rename, Rating, Pick}`, `apply()` (the one entry; `set()` calls it), `sync()`, `merge()` (three-way), `from_finder` / `to_finder`, `with_tag`, `every_item`, `remember` / `recent`.
- Originals and moments go through `Rec` (own file / link) and are synced with the stores. **Stills**: tags through the `photo` writer (XMP `dc:subject`), pick and rating through `relink` into the still's link; no Finder sync for stills.
- Exports are no longer tagged (main tagged them): `"export"` falls through `apply` untouched, and `index.rs` gives them `tags: []`.
- `Stores::of(root)` reads `locations::tag_stores(root)`: this computer's file.

### `library/src/main.rs`
- Commands: `set_tags` (was there), `set_rating`, `set_pick`, `sync_tags`, `save_tag_stores`, `rename_tag`, `recent_tags`; all in `generate_handler!`.
- `write_tag_stores(file, root, stores)`: `tag_stores` into this computer's file, the rest kept, the file removed when nothing is left. `save_local` now keeps `tag_stores` when it rewrites that file.
- `project_settings` returns `tag_stores`, `finder_here`. `ProjectInfo.tag_stores`.

### `library/src/index.rs`, `catalog/src/`
- `index.rs`: `pick` and `rating` on clips (from the own file), moments and stills (from the link).
- `catalog/src/locations.rs`: `tag_stores(project)` reads this computer's file; `tag_stores_in(&Value)`; `recent_tags_file()`.
- `catalog/src/finder.rs` (#197): Finder tags read and written (xattr plist).

### Lua (the player)
- `mpv/scripts/workspace/tags.lua` (#197): the model, Finder mapping, merge; twin of `tags.rs`.
- `mpv/scripts/workspace/main.lua`: the tags block is inside `;(function() … end)()` near the end (200-locals limit). `locations_of` now reads `tag_stores` from this computer's file into `locs_cache`; `fs.tag_stores(root)`; `enabled_stores()` uses it. #197's unused `fs.finder_tags` is gone.
- `mpv/scripts/side-panel/tagchips.lua`, `side-panel/main.lua`: the Project tab's Accept / Reject, stars, chips.
- Keys P, X, U, 1–5, 0, K; messages `tag`, `tag-toggle`, `tag-rate`, `tag-pick`, `tag-ask`.

## Data shapes
- **An original's own file** `<clip>.afp.json`: `"pick": "accept" | "reject"`, `"rating": 1..5`, `"tags": [..]`, `"synced": { "finder": [..] }`; a key is absent when empty.
- **A moment's link** (MP4 key `ch.actionfootageplayer.link`): the same keys.
- **A still**: `pick`, `rating` in its link (`afp:link` in XMP; one without a link gets `{"v":1,"kind":"still"}`); tags in `dc:subject`.
- **This computer's file** `<data>/ActionFootage/projects/<id>.json`: `{"project", "locations", "never_rename"?, "tag_stores"?: ["finder"]}`.
- **Recent tags** `<data>/ActionFootage/tags-recent.json`: `{"recent": [..]}`, shared by both apps.
- **Scan items** carry `pick`, `rating`, `tags`. Commands take `items: [{kind, id, path}]` with kind `clip` | `moment` | `still`, and return a list of problems (strings).
- **Page storage**: `afl.hideRejected`, `afl.starSort` (`"1"` or empty).

## Tests and pictures
- Rust: `cd library && cargo test` (42 here); `cd catalog && cargo test --lib`. cargo is in `~/.rustup/toolchains/stable-aarch64-apple-darwin/bin`.
- Windows: `PATH=/opt/homebrew/opt/llvm@19/bin:$PATH cargo check --target x86_64-pc-windows-msvc` in `library/` (needs `llvm-rc`).
- Lua: each `luajit tests/<name>.lua` listed in `.github/workflows/release.yml`, from the repo's top. *(Now: `sh tools/run_tests.sh` finds tests by name)*
- The page: `npm install jsdom` in a scratch folder, then `NODE_PATH=<it>/node_modules node library/ui-tests/culling_test.js` (72 checks). `library/ui-tests/mock.js` is the stand-in for the app; it is outside `ui/` because Tauri bundles `ui/` whole. Not in CI. *(Now: `library/ui-tests/*_test.js` run in CI through `npm test`)*
- Pictures: write a copy of `ui/index.html` with `<base href="file://…/library/ui/">`, `mock.js` injected before `culling.js`, and a set-up script at the end; then
  `"/Applications/Google Chrome.app/Contents/MacOS/Google Chrome" --headless=new --user-data-dir=<scratch> --allow-file-access-from-files --force-device-scale-factor=2 --window-size=1280,800 --virtual-time-budget=6000 --screenshot=out.png file://<the copy>`.
  Its own profile keeps Anton's Chrome untouched; the profile's localStorage carries over between shots.

## Unfinished, not verified
- Never seen in the real app window. The player was not run; its panel picture is #197's.
- Finder sync ran only on temp files. The UGREEN share's handling of the attribute is untried.
- A still's link written by the real `afp-meta --link` was not run (a stand-in in the test). `.png` stills can't be culled (no link writer).
- A still's stars are not in `xmp:Rating`: Photos and Lightroom don't show them (designed in the doc).
- A catalog-changed refresh arriving between two queued writes can show the second item without its change for a moment.
- The tag facet compares case-sensitively (`indexOf`), the panel without case.
- Changes by another computer on the NAS reach Finder sync only through *Sync tags now* (as in #197).

## If the other branches collide
- Footage renamed to Albums: `culling.js` counts tags over `RAW_FOOTAGE.items`, `DATA.moments`, `DATA.stills`, treats kinds `clip` / `moment` / `still` as cullable and switches off only for `S.view === "exports"`. *(Now: the Exports view is gone, #241)*
- Commands returning changed items: in `cullApply` replace the final `refresh()` with applying what came back; the page is already updated first.
- `renderTop` rewritten: keep `cullTop();` as its last statement (it appends to `#top`).

## Piece 2: Open at start (PR #234, branch `claude/night-default-library`, stacked on #233)
Done in the same session; not handed to a fresh one.

- `library/ui/atstart.js` (new, loaded before `app.js`): `startMenu()`, `startRow(r, menu)`, `startHint()`, `startOpen()`, `startSet(path)`; buttons carry `data-s` (`start`, `screen`) with their own click listener.
- `app.js`, four lines: `startMenu() +` in `renderTop`'s menu, `startRow(r, menu)` in `recentRow`, `startHint();` in `renderEmpty`, `if (!S.root) await startOpen();` at start-up. `index.html`: the script tag and `<p id="startHint">`. `style.css`: `.rstart` at the end.
- `main.rs`: `RecentEntry.at_start` (in `recent.json`, written only when true), `start_entry`, `set_start`, `remembered` (used by `remember`: keeps the mark, never drops the marked one off the ten), `Recent.at_start`, commands `set_start_project(path: Option<String>)`, `open_at_start() -> Option<String>`, `close_project()`.
- The page asks `open_at_start` once, when it comes up with no project open, so a project given by a double click wins.
- Tests: `cargo test` (44; `one_project_opens_at_start`, `the_start_project_reachable_or_offline`), `library/ui-tests/atstart_test.js` (24 checks).
- Not verified: the real window; the order of the file-open event and the page's question on a cold start by double click on macOS.
