# Night handover: the Library answers at once (2026-11-16)

**Archived:** night handover for screen-first writes (#236) and the pinpoint update (#237, landed via #241), v0.14.0. Written 2026-11-16.

For the session that merges the night's branches, and for whoever goes on from here. Both paths are built:

- **#236** `claude/night-responsive-894fc6` (on main 18ee53e): screen first, disk after.
- **#237** `claude/night-responsive-pinpoint` (base: #236's branch): the pinpoint update. This note is on this
  branch; merging #237's branch brings both.

Neither is merged. *(Now: #236 merged; #237 landed via #241, v0.14.0)* Rules, the reasons and the full tables: `docs/design/library-responsiveness.md`.

## The pieces and where they live

| Piece | File | What it is |
|---|---|---|
| The mechanism | `library/ui/writes.js` (new, loaded first) | `Writes`: the queue of changes, what's shown = what the disk has + the changes still being written, one write at a time. Knows the page only through `Writes.host` |
| The host and the changes | `library/ui/changes.js` (new, loaded before `app.js`) | `Writes.host`; the kinds `tags`, `location`, `source`, `renders`, `rename`, `renamefolder`, `delete`; the strip "Not saved" (`#unsaved`); the sign in Tasks; the window's close (`#closing`); the pinpoint update (`mergeChanged`, `mergedInto`, `showItems`, `drawChanged`, `catalogChanged`) |
| The page's call sites | `library/ui/app.js` | `CONFIRMED`; `applyView` split into `applyView` (keeps the app's answer) and `showView` (makes what's shown); `loadView` (`Writes.reading` / `read`); `loadProject`; `deriveItems(view, only)`; `setTags`, `applySource`, `deleteSelection`, `geoWrite` / `geoApply` / `geohand` / `georemove`, `commitRename`, `delrenders`; `poll` and the `catalog-changed` listener call `catalogChanged`; `backgroundTasks` (one line) |
| What a command answers | `library/src/changed.rs` (new) | `Changed { items, gone, gone_files, reload, probes, problems, answer }`; `of_items`, `of_clips`, `of_rename`, `of_delete`, `of_files` |
| Only-these-items | `library/src/index.rs` | `scan_only(root, view, flat, only)`, `footage(.., only)`: the views' own code, the wanted items looked up instead of the walk |
| Which files changed | `library/src/seen.rs`, `catalog/src/catalog.rs` | `LOG`, `since`, `changed_what`; `seen_keys`, `take_seen` |
| Commands | `library/src/main.rs`, `geo.rs` | `set_tags`, `set_source`, `rename_item`, `delete_items`, `delete_renders`, `set_locations` answer with `Changed`; new: `changes_since`, `view_items`, `writes_pending`; `close_waits`, `close_decision` |
| Measuring | `library/src/perf.rs`, `catalog/src/store.rs` (`nas`), `library/ui-tests/page_perf.js` | below |
| Page tests | `library/ui-tests/` | `page.js` (the page in jsdom + mock), `fakeapp.js` (the commands stood in for), `writes_test.js` (15), `pinpoint_test.js` (6), `shot.js` (pictures) |

## How a command joins

Page (`changes.js`, or any script loaded after `writes.js`):

```js
Writes.define("rating", {
  what: (a, op) => "the rating of " + namesText(op.targets.map((t) => t.name)),   // for "Not saved: …"
  item(it, a) { it.rating = a.rating; },    // the item as the views have it, changed as the command changes it
  send: (a) => ["set_rating", { items: a.items, rating: a.rating }],
  problems: (r) => r,                       // only for a command that still answers with its list of problems
});
Writes.run("rating", items, { items: items.map((it) => ({ kind: it.kind, id: it.id, path: it.path })), rating: 3 });
```

`item` must do exactly what the command does and be safe to do twice. A change that moves or removes items gives
`patch(draft, args)` instead (see `rename`, `delete`). `Writes.run` gives a promise of `{ ok, answer, args }`.

Rust: the command's last line is `changed::of_items(&root, &items, problems).with_probes(&probes)` (or `of_clips`),
its return type `changed::Changed`. Until a command does that it still works: the page reads the views again only
when it lists problems.

Never put `await refresh()` after a command again.

## The harness

```
cd library && AFL_PERF_DIR=/tmp/afl-perf CARGO_PROFILE_RELEASE_STRIP=false \
  cargo test --release -- --ignored where_the_time_goes --nocapture        # ~2 min; AFL_PERF_CLIPS=10000, AFL_PERF_NAS=5,5,15
cd library/ui-tests && npm install && AFL_PERF_DIR=/tmp/afl-perf node page_perf.js <label>   # ~4 min; AFL_PERF_MODES=own,local,nas
cd library/ui-tests && npm test                                            # 21 tests
cd library && cargo test                                                   # 40; catalog: 29
PATH=/opt/homebrew/opt/llvm@19/bin:$PATH cargo check --target x86_64-pc-windows-msvc   # in library/
node library/ui-tests/shot.js <unsaved|saving|closing|leaves> out.png [--ui <dir>]      # headless Chrome
```

The first makes the project, times every command and every reading as the disk is and as a NAS
(`afp_catalog::store::nas`: each stat, read, write waits), and leaves `timings.json` and the views; the second runs
the page on those with the mock app answering that slowly. To measure another branch's page, check it out and run
the second with another label (it reads `ui/` as it is).

## The numbers (10,000 clips, as a NAS: 5 / 5 / 15 ms a stat / read / write)

| Change | Shown: main | #236 | #237 | Page done: main | #236 | #237 |
|---|---:|---:|---:|---:|---:|---:|
| A tag on 1 clip | 21.6 s | 0.11 s | 0.03 s | 21.8 s | 21.8 s | 0.45 s |
| A tag on 200 clips | 35.7 s | 0.19 s | 0.04 s | 35.9 s | 35.8 s | 16.3 s |
| A location for 20 clips | 22.0 s | 0.12 s | 0.03 s | 22.2 s | 22.1 s | 0.71 s |
| Source settings for 20 clips | 22.6 s | 0.13 s | 0.04 s | 22.8 s | 22.7 s | 1.45 s |
| A moment renamed | 18.0 s | 0.11 s | 0.02 s | 21.7 s | 21.8 s | 0.44 s |
| A moment deleted | 17.8 s | 0.31 s | 0.02 s | 21.6 s | 21.8 s | 0.39 s |

This disk, a tag on 1 clip: 2.2 s, 0.11 s, 0.03 s shown. What the page read after a command on main: `project`
3.0 s, `scan footage` 3.6 s, `scan moments` 14.5 s as a NAS, each twice. Saying what changed: 40 ms on this disk at
10,000 clips, 11 ms at 1,000.

## Unfinished, and not checked

- **Not seen in the real window, on the real NAS, or running on Windows.** The NAS column is a model. The window's
  close with changes waiting is tested as a decision (Rust) and as the page's part (mock), not as the window's event.
  `changes_since` fed by the catalog's own thread has not run in the app: `of_files` is tested with files noted by
  hand, the page with the mock.
- **Still the old way** (wait, then read the views): New folder, Take in, Project settings, Find moved files / Mend,
  the lens for Stabilize optically (`set_lens_type`), the end of a task (`refresh()` in `poll`). A folder renamed or
  deleted shows at once and then reads the views once.
- **Saying what changed still grows with the archive** for clips: `index::previews` walks the project's clip
  previews from the catalog to find one clip's. The next thing to shave: look the wanted clips' previews up in the
  catalog instead.
- **`tags::set` finds the project's locations again for every clip** (runs `mount` each time): 5 ms a clip on this
  disk, 1 s for 200. Not touched, because #233 rewrites `tags.rs`; `clip_own_in` with the locations found once fixes it.
- **`project` lists 467 folders for four counts** on every call, and `scan` calls it for one setting. No longer on
  the path of a change; still on opening a project and switching tabs. `changed::flat_previews` reads the setting
  alone and could serve `scan` too.
- **The list on screen is drawn again from memory** after a change (as on every click), not patched row by row. A
  flat list of thousands of matches is as slow to draw as it was.
- **A new clip in Footage** from the watcher reads Footage again (its place in the tree isn't worked out on the page).
- CI on #237 was still running when this was written; #236's builds passed.

## For the merge

- `index.html` loads `writes.js`, `changes.js` before `app.js`; culling's `culling.js`, `atstart.js` and albums'
  `albums.js` also load before `app.js`. Order: `writes.js` first, the rest in any order, `app.js` last.
- **Culling (#233, #234)**: `setTags` in `app.js` is changed by both: keep culling's choice of items, then
  `Writes.run("tags", …)`. `cullApply()` shows first and calls `refresh()` after: replace by a kind per command
  (`set_rating`, `set_pick`) and `Writes.run`; in Rust give them `changed::of_items`. `main.rs`'s `set_tags`: take
  #233's body, keep the `Changed` answer. If culling moves the selection itself after a key, keep one of that and
  `drawChanged`'s.
- **Albums**: `index::footage` is changed by both (here: the `only` parameter and a branch at the walk; there: it
  returns `links`); in only-mode `links` can be left out. `album_add`, `album_move` and deleting a link should answer
  `Changed { reload: ["footage"] }` until `links` is given item by item; the `delete` and `renamefolder` kinds patch
  `pdir`, `preview`, `clips_dirs` and need `links` added. If `DATA.footage` is no longer one item per clip in the
  scan's order, make `showItems("footage", …)` return false: the view is then made again whole from memory, which
  is always right.
- `library/ui-tests/`: these files beside culling's `mock.js`, `culling_test.js`, `atstart_test.js`; none shared.
  `package.json` here runs `*_test.js` with `node --test`; culling's tests are plain scripts, so name them in it or
  run them as before.
- Longer notes for both branches: the side-session note `night-responsive.md`.
