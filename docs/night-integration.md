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

Share + render cache (stacked on #238) was still being built when this was written: **not in the branch**.

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
- **Timeline's tiles show the verdict**: a small ✓ / ✕ and the stars on the thumbnail (`cullBadge`; the tiles have
  no room for chips). "Add to an album…" (not "another") in Timeline and Map.

SECTIONS_BELOW
