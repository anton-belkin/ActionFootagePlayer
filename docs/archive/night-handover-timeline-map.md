# Timeline and Map inside Albums: handover (2026-11-16)

**Archived:** night handover for Timeline and Map inside Albums, merged as #239 via #241 (v0.14.0). Written 2026-11-16.

Branch `claude/night-timeline-map-d4a2f0`, stacked on `claude/night-albums-3c6687` (PR #235). Two views inside
Albums that aren't folders, as `ALBUM_VIEWS` in `library/ui/albums.js` was reserved for: **Timeline** (everything by
when it was recorded) and **Map** (everything by where). `docs/design/albums-photos-share-concept.md` is the design; this
note is for whoever merges it with the other night branches and for the morning's review.

## What was built

At the top of Albums, above "Your albums", two tiles: Timeline and Map. Each opens as a place inside Albums
(breadcrumbs `Albums › Timeline`, Back / Forward / Up work).

**Timeline** (`library/ui/timeline.js`): every original, moment and photo, the newest first, as Years, Months or
Days (the choice is remembered on this computer).
- Days: a line per month, then each day with its header ("20 Oct 2026 · Dahab", then "3 clips, 1 moment") and its
  items as tiles, the earliest first within the day. Days in a row at one wider place (the place's region, else its
  country) are a **trip**: a line above its newest day ("Dahab, Sharm El Sheikh · 10–16 Oct 2026 · 7 days · 212
  items") and a rule down the left of its days. A trip is two days or more; one empty day between two days doesn't
  end it; a day with no place known goes with the days around it.
- Months: a cover per month under its year, with its count and places. Years: a cover per year. A click on a year
  opens its months; on a month, its days.
- What has no date is one group at the very end, "Without a date" / "No date known".
- The search and the filters narrow it ("5 items of 12 match").

**Map** (`library/ui/map.js`): everything that has a location as pins. Pins near each other at the zoom shown are
one pin with a picture and a count; zooming in splits it; a click lists the pin's items in the ordinary grid
(`Albums › Map › Dahab and 1 more place`). The line above the map: "606 items at 22 places · 106 items without a
location [List them]". `Map | Places` switches to the same places as a list with counts. Without a connection (or
when no map picture loads) the list is shown with a line saying why and "Try the map again".

In both, an item is selected, opened in the player (double click, Enter), has the context menu and the side panel
as in any other view: the page's `items()` gives the view's items while it is open, so `ctx()` and all that is built
on it take them unchanged.

## Where dates and places come from (nothing is read for these views)

- **Date**: an item's `when`, set by `deriveItems` in `app.js` as before: a clip's from its name (DJI) or the
  probe's `movie.created`; a moment's from its link's `created` + `in`; a photo's from its file's `mtime`. A clip
  not read yet (no probe) has no date and sits in "Without a date" until its probe comes; the view says how many
  are still to be read.
- **Place**: an item's `geo`, set by `deriveGeo` in `app.js` as before: the location in the original's own file
  (`<clip>.afp.json` → `location`, written by `library/src/geo.rs`, with its `place` names from the time it was
  set), else the camera's own GPS from the probe with the name from the Library's cache of looked-up places
  (`places.json`). A moment's or a photo's place is its clip's (`clipOf`).
- **Names**: these views call no geocoder. They show what is stored: `place.locality`, else `site`, `region`,
  `water`, `country`. A point with no name stored shows as coordinates to 0.1° ("46.1° N, 7.2° E") in the Places
  list and on pins, and is left out of a day's header when the day has a named place too. One thing to know:
  `deriveGeo` (existing code, PR #188) asks for the name of a camera-GPS point it hasn't seen (`places_of` with
  `lookup: true`: Apple's geocoder on a Mac, Nominatim elsewhere) whenever clips are derived, in every view. Timeline
  and Map add no such call of their own, but opening them derives the clips like any view does.
- Trips group by `place.region` (else country, water, else the point to 1°).

## Files

- `library/ui/albumviews.js` (new): what a view inside Albums is and what the two share: `albumViewOpen`,
  `albumViewItems`, `albumViewDraw`, `albumViewTiles`, `albumViewInfo`, `avEverything` (every original once, plus
  the moments and photos the page has, **by `kind`**, from `DATA.moments`, `DATA.stills` and `DATA.footage`, each
  file once), `avGeo`, place names, the ordinary grid's card, and the click handler for the views' own buttons.
- `library/ui/timeline.js`, `library/ui/map.js` (new): each pushes itself into `ALBUM_VIEWS`
  (`[id, label, draw(main), items()]`). `albums.js` itself is not touched.
- `library/ui/vendor/leaflet/` (new): Leaflet 1.9.4's `leaflet.js` (the line naming its source map removed),
  `leaflet.css` and `LICENSE` (BSD-2-Clause), from npm's `leaflet@1.9.4`. Its marker and layer images are not
  vendored (the pins are the page's own, no layer control). Loaded only when Map is first opened.
- `library/ui/app.js`: six one-line hooks, each marked `albumviews.js`: `items()`, `buildFacets()`, `renderMain()`
  (the view drawn in place of a folder; the tiles at Albums' top), `goUp()`, `infoHtml()` (nothing selected).
- `library/ui/index.html`: three scripts between `albums.js` and `app.js`. `style.css`: one block at the end.
- `library/tauri.conf.json`: the CSP's `img-src` gains `https://tile.openstreetmap.org`; nothing else changed
  (Leaflet's script and style are the app's own files; the page sends nothing but the tile requests).
- `library/tests/views_mock.js`, `views.js`, `views_shots.js` (new); `albums.js`: one check loosened (the scripts'
  order).

How a view is in the page: `S.view` stays `footage`, `S.path[0]` is the view's label behind a zero-width space
(`AV_MARK`), so an album of the user's named "Timeline" stays an album and the breadcrumbs, history and keys need no
change. Map's second path part is the place: `c:<zoom>:<x>:<y>` (a pin: a Web Mercator cell of 84 px at that zoom,
worked out by the page, so Back finds it again), `p:<name>` (a row of Places), `none`.

## For the merge with the other night branches

- **Moments and photos as album kinds** (the parallel session): `avEverything` already takes `moment` and `still`
  items from `DATA.footage.items` too, and de-duplicates by `path`, so they appear in Timeline and Map whether they
  come from the old views' data or from `ALBUM_SOURCES`. If the Moments / Photos views go away entirely,
  `albumViewGo` asks only for views that `VIEW_ROOT` still names; if their data is then read under another name,
  that one line is where to say so.
- Conflicts to expect: `index.html` (the script list), `tests/albums.js` (the script-order check), the end of
  `style.css`, and `app.js` where another branch rewrote the same line (`items()`, the top of `renderMain`,
  `infoHtml`). Each hook is one line; take the other branch's code and put the line back.
- **Responsiveness (#236, #237)**: the views are built from `DATA` at each draw (cached by the identity of
  `DATA.sources` / `moments` / `stills` / `footage`), so they follow any change that makes those again. The pinpoint
  update that patches items in place without making `DATA.sources` again won't be seen until the next whole
  derive: `AV.cache = null` in `showItems` / `drawChanged` fixes that, or have `showItems` return false while a
  view is open.

## Tests, pictures

- `cd library && cargo test`: 42 pass (no Rust changed).
- `JSDOM=<node_modules/jsdom> node library/tests/views.js library/ui`: 44 checks (the model: days, trips, the
  undated group, blocks; the page: tiles at Albums' top, headers, selection, the side panel, opening, the menu,
  Months / Years, search; the map's pins at two zooms, a pin's items, offline list, no-location list).
  `node library/tests/albums.js library/ui`: 41 pass.
- `… node library/tests/views.js library/ui 10000`: timings in jsdom on a synthetic library.
- `PUPPETEER=<node_modules/puppeteer-core> node library/tests/views_shots.js library/ui <out> [dark] [perf]`:
  pictures in a headless Chrome (no window), `perf` for the timings there. Pictures: `docs/img/library-timeline-map/`.
- jsdom runs all the page's scripts as one, so it can't see a script using something from a later script while
  loading (it did happen: `remembered`). The headless Chrome run is the check for that.

## Measured (synthetic library: 10 012 items, 401 days, 61 trips; M1 Pro, headless Chrome 1320×820)

| | |
|---|---|
| The model (sort, days, trips, months, years) | 7 ms, made once per data or filter change |
| Days: one draw (`renderMain`) | 3.3 ms; 33 tiles in the page of 10 012; the body is 353 000 px tall |
| Days: a scroll step's fill (200 steps over the whole timeline) | median 3.2 ms, p95 4.4 ms, max 12.4 ms |
| Days: a selection drawn (`renderAll`, with the filters and side panel) | 22.5 ms |
| Months / Years: one draw | 5.7 ms / 2 ms |
| Map: grouping into pins at zoom 3 / 12 | 2.5 ms / 1.6 ms; drawing the pins 5.6 ms |
| Places list (jsdom) | 10 ms for 49 places; the fullest place's grid 45 ms for the first 400 of 1 262 cards |

Days is drawn by window: each block (month line, trip line, a day with its rows) has a height known from the
tiles' fixed size (168×146, `TL` in `timeline.js` and `.card.tl` in `style.css` hold the same numbers), and only
blocks within 700 px of the window are in the page. A place's grid in Map is the ordinary grid, 400 cards at a
time with "Show 400 more".

## Unfinished, and what to know

- **Commands on moments and photos selected in Timeline / Map** go through Albums' side panel, which is written for
  clips (`tabFits`, `sideBody` by `S.view`). Info, opening, Go to the footage, Tags and Delete act by the item's
  kind and are right; Source settings, Location, Moments & Photos, Stabilize would be offered for a selected moment
  as if it were a clip. The session making moments and photos album kinds has to settle the same thing for albums;
  nothing separate was done here.
- The count under the side panel's title says "clips" for a mixed selection (`NOUN.footage`).
- An original selected here can't be removed from its albums (Delete takes nothing of it: on purpose, the view is of
  the originals). "Add to another album…" should read "Add to an album…" here (`albumMenu` words it by `S.view`).
- Keyboard: arrows move through the tiles in time order; a move to a tile outside the drawn window selects it
  without scrolling to it (`moveTo` looks for the element).
- The newest is first; Apple Photos has the newest last and opens at the bottom. One line to turn (`days.reverse()`
  in `tlModel`) plus the opening scroll, if wanted.
- A day is the day on this computer's clock for clips whose time is UTC (GoPro); the camera clock bases of #188
  aren't applied here (nor are they in the lists' "Recorded" column).
- Photos' dates are their files' `mtime` (as the Photos view has them), not the frame's time in the clip. *(Now: a photo link's date is its clip's start + `t`, `app.js` `deriveItems`; a rendered photo's is its mtime)*
- Map: no list of the pin's items beside the map (the click goes to the grid, Back returns to the map as it was);
  no "fit to selection"; pins aren't keyboard-reachable. The dark map is OpenStreetMap's tiles inverted by CSS.
- OpenStreetMap's tile servers ask for light use and a visible credit (the line under the map; no link, since a
  link would navigate the app's window). For more than one user's worth of traffic another tile source is needed:
  `MAP_TILES` in `map.js` and the CSP's host.

## Not verified

- **The app's real window**: nothing was launched (night rule). In particular not seen: the CSP actually letting
  the tiles through in Tauri's webview (the pictures are from Chrome on `file://`, where the CSP doesn't apply),
  Leaflet's drag and wheel zoom in WKWebView, and what OpenStreetMap answers to a request from `tauri://localhost`.
  If tiles are refused there, the view falls back to the Places list by itself.
- **Real library data**: only the mock. Real place names (long `site` names, places at sea), real counts per place
  (home may hold thousands), clips whose probes aren't in yet on a cold start.
- **The NAS**: nothing here reads or writes files; not run against a project on the share.
- Windows: not built (no Rust changed; the page is the same).
