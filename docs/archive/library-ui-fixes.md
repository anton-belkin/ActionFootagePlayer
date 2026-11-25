# Library page: four fixes from the first real try (2026-11-16)

**Archived:** fix notes for four Library page fixes after Anton's first real-window try, merged as #243 (v0.14.0). Written 2026-11-16.

Branch `claude/fix-library-ui-0e9b94`, cut from `claude/night-integration-fd9085` (#241). Anton tried the night's trial
build in a real window and asked for four things; this is what changed, the decisions taken, and what was not seen.
Pictures (before / after, light and dark): `docs/img/library-ui-fixes/`.

## 1. The bar above the list stays in place

**What changed.** Everything above a list is one block, `.mbar`, the first child of `#main` (the scrolling box), with
`position: sticky; top: 0`: the breadcrumbs with Back / Forward / Up, the search's or the filters' line, the selection's
bar (Select all, Add to an album…, Share…, Take out of the album), and the list's column head. In every view:
Originals, an album, Albums' top, a media-type album (`renderMain` in `app.js`), Timeline (`timeline.js`), Map and a
Map place's list (`map.js`). What only informs (scanning again, offline locations, probes still coming, broken links)
comes after the block and scrolls away. A thin line and shadow under the block once the list is under it
(`#main.scrolled`).

**Timeline's day headers** stick too, below the block: `top: var(--mbar-h)`, the block's height measured by
`pinBar()` (`app.js`) after each draw and again by a `ResizeObserver` when the bar wraps (a narrow window, a long
selection bar). A day's block lost its `overflow: hidden` for this (an ancestor clipping its overflow becomes the
sticky header's scrolling box and the header would not stick at all). The header half under the bar that the
integration note saw is gone: the check scrolls to three places and looks for any header partly behind the bar.

Also fixed on the way: the list's column head was 14 px left of its rows (it was so before; it now lines up).
Leaflet's panes have z-indexes up to 1000: `.mapbox` gets `z-index: 0` so the map stays under the pinned bar.

**WKWebView.** Plain CSS sticky positioning, no JS scrolling: WebKit has `position: sticky` unprefixed since Safari 13
(macOS 10.15, Tauri 2's minimum) and the prefixed `-webkit-sticky` line is there for anything older. It is used as
WebKit handles it: the sticky element is a direct child of the `overflow: auto` box that scrolls (`#main`, a flex
column), it has an opaque background, and no box between it and `#main` clips overflow. The day header's containing
block is the absolutely positioned day block (sticky inside an absolutely positioned parent works in WebKit; the
header is pushed off when its day ends). `:scope` in `querySelector` (Safari 7) and `ResizeObserver` (Safari 13.1;
without it the height is measured on each draw only) are the only newer things the JS uses. **Not seen in the real
window**: all of this is headless Chrome and jsdom.

## 2. Location in Filters (and every nested list)

- **Nested lists start collapsed** (Region in a country, a place in a region, a camera's models, Log / HDR curves,
  Frame → Changed): a parent row shows its count, its checkbox, and a twisty. Opening one is remembered while the
  window is open (`S.fexp`), across closing the panel and changing views; nothing opens by itself any more.
- **A collapsed group with something ticked in it says so**: its checkbox is half-ticked and the row names what's
  ticked ("South Sinai · Dahab", in the accent colour). A whole group ticked shows a full tick.
- **A long list (more than 8 rows) shows its 6 largest, then "N more…"** (and "Fewer" after), with a small box to find
  a value by name (`Find a country…`). Ordered by count, the largest first, when laid out; "No location" and
  "Unknown" last. Short lists keep their own
  order (Keep it?, Stars, Motion data… have a meaning in their order). While finding, a group whose children match
  opens to show them.
- Decided against: changing Location's three lists (Country / Region / Place) into one drill-down, and against a
  search box in every list. Collapsing + top 6 + find covers the depth Anton disliked with the panel's own pattern;
  the lists themselves (`geoFacets`) are as before.

## 3. A tick doesn't move under the pointer

The panel's layout (which rows show, the order of a long list, its first 6) is made once and kept while it's used
(`S.ffreeze` in `renderFilters`). It is made again only when: the panel is opened, the view / folder / search / search
scope changes, or the items themselves change (a scan, the clips' probes coming in: `probesVersion` and the identity
of `items()`). A tick changes none of these. So after a tick:
- the same rows, in the same order; counts change in place; a row whose count fell to 0 stays, greyed (`.zero`);
- the panel's scroll and the keyboard focus (on the checkbox, or in a find box while typing) are kept;
- a row that wasn't shown when the panel was laid out isn't brought in by a tick (an option that appears from new
  data is, as before).

Why the items' identity is in the key: the first version froze the layout before the clips' probes had come in (all
counts 0), so the panel stayed nearly empty until reopened. Found by the headless walk.

The check (`ui_fixes_shots.js`, headless Chrome): the panel scrolled, the largest country's checkbox clicked at its
screen point; the same point then holds the same row, ticked; the panel has as many rows and the same scroll; a
second click at that point unticks it. On the old page the same tick took the panel from 32 rows to 13.

## 4. Albums' top is one list

Albums' top is now one list (or grid) of rows of one look. Anton corrected the first version: **his own albums come
first, as one row you enter**, then all the rest:

| Rows | |
|---|---|
| **My albums** | one row (four pictures, "3 albums · 9 items"); inside it his albums from `_Clips/`, `_Moments/`, `_Stills/` as the top listed them before: albums in albums, New album, rename, delete, Add to an album |
| *The whole library* | Timeline, Map (an icon for a picture) |
| *Media types* | Videos (every original clip), Moments, Photos, Live Photos, 360° videos (four pictures of their items, the count) |
| *Folders* | Generated, Other folders (as they were) |

**My albums is a place in the page only**: nothing moves on disk, an album's own path is as before (`S.path` is
`["Egypt"]`), so adding to an album, the album sheet, renaming and the player's links don't change. The path shows
`Albums › My albums › Egypt` (the middle part opens My albums); Up from an album goes to My albums with the album
selected; Back and Forward work through it. New album from the top of Albums opens My albums and makes it there.
Inside My albums the page is the old top of Albums (`dirKey()` is `""` there): the same folders, filters, search,
"List every match below here".

Live Photos and 360° videos show only when the library has some (`form === "Live Photo"` from the Photos scan,
`is360` from the probe: both already in the page). A click opens a row (they aren't selectable: nothing to rename or
delete). The tiles (`albumViewTiles`) are gone, and the list's column head is not drawn over these rows.

**A media-type album** lists every item of its kind in the library from what the page holds (`avEverything()`, the
same as Timeline and Map: the originals once, the moments and photos from their scans; no scan of its own) and is
drawn by `renderMain` as any album: list or grid, selection, the menu, the side panel, culling keys, Add to an album…,
Share…, filters, search. It isn't an album of files, so it has no "Take out of the album" (nor have Timeline and Map
now). How: `ALBUM_VIEWS` stays the extension point. An entry is now `[id, label, draw, items, meta]`; My albums
(`meta.folders`) and the media types (`meta.media`, `AV_MEDIA` in `albumviews.js`) are entries with `draw: null`,
which `albumViewDraw` leaves to `renderMain`. `under()` gives a media type's or Timeline's items while one is open
(before, the filters' counts inside Timeline were counted over nothing). The items are cached per `avEverything()`.

## Tests

| What | Result |
|---|---|
| `JSDOM=… node library/tests/albums.js library/ui` | 81 pass (the top's rows; albums opened through My albums; its path, Up; New album lands in it) |
| `… tests/share.js` | 31 pass |
| `… tests/views.js` | 45 pass (`ALBUM_VIEWS` has My albums and the media types too) |
| `… tests/ui_fixes.js` (new) | 51 pass: the bar is one pinned block in eight places; the top's rows, groups, grid; My albums, an album's path, Up, Back; Videos / Moments / Photos / Live Photos list every item of their kind, select, menu, P, filters, no scan of their own; Filters collapsed, counts, top 6 + more + find, opened groups remembered, a ticked value named on a collapsed group, tick / untick leave the rows, order and counts in place, reopening lays out again |
| `library/ui-tests`: `node --test *_test.js` | 31 pass (culling, Open at start, pinpoint, writes: what was at Albums' top is looked for in My albums) |
| `PUPPETEER=… node library/tests/ui_fixes_shots.js library/ui <out> [dark]` (new) | all pass, light and dark; on the old page 9 fail (the bar scrolls away, rows vanish on a tick, nested lists open) |

`views_mock.js` takes `places: true` for twelve more places (long Country / Region lists). No Rust changed:
`cargo test` not run again.

## Not seen

- **The real window** (WKWebView): sticky positioning, the shadow, `ResizeObserver`, the map under the bar. Expected to
  work (above), not seen.
- The 360° videos row: the mock has no 360 clip, so the row is only checked to be absent.
- Keyboard: arrows move through the items, not the rows at Albums' top (they are buttons: Tab reaches them).
- In Timeline a sticky day header covers the top of its first row of tiles while that row scrolls under it; their
  captions show below the header for a moment (as in Photos). Left as it is.
- Seen in the review of the pictures and left: "Marks · mom." wraps to two lines in the list's head (as before); a
  photo says it's a Live Photo three times on its card (badge, line, chip; as before).
- A very long Location (hundreds of places) was not measured; the panel draws the first 6 of a long list unless
  "more" is open.
