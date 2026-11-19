"use strict";
// The tile of a clip, a moment, a photo or an export: in the grid of every view (app.js's renderMain) and in
// Timeline's (albumviews.js's avCard). Loaded before albumviews.js and app.js, whose state and helpers (S, esc, img,
// RAW_FOOTAGE, albumShown, userAlbum, cullBadge) it uses when called.
//
// Anton's layout (2026-11-18): the picture, and under it only the albums the item is in (one or two, two lines at most,
// shortened in the middle so the beginning and the end both show) and one small tag icon when it has tags. No name, no
// time. What it is (a moment, a photo, a Live Photo, an export) is a small icon over the picture's top-left corner; a
// clip has none (360° says so there). Over the picture too, as before: Accepted / Rejected and the stars bottom-left,
// the length bottom-right, a problem mark top-right (offline, own files in conflict, a moment without its link), the
// dot of an item being written top-right. The rest (camera, curve, renders, Gyroflow project…) is in the list layout
// and in Info.
//
// A clip's picture (its album link's frame, its .THM or LRF, the frame Finder draws) is as the camera recorded it; a
// turn set afterwards lives in the clip's own file (own.source.rotation, clockwise as the player's video-rotate). The
// picture is shown turned by it here (CSS), so a preview made before the turn is shown as the clip is now. A photo
// link's picture is its clip's frame: turned the same. Moments and rendered photos were made turned: left as they are.

const TILE = { w: 0, measure: null, font: "", minCol: 180, gap: 12, pad: 14, inner: 14, tagW: 18, linksOf: null, links: null, clips: null, clipsFrom: null };

const TILE_ICON = {
  // A piece cut out of a clip: the strip with a gap, the piece lifted out of it
  moment: '<svg viewBox="0 0 24 24" width="14" height="14" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">' +
    '<path d="M1 11.5h6.8v7.5H1M23 11.5h-6.8v7.5H23"/><rect x="8.3" y="2.6" width="7.4" height="7.6" rx="1" fill="currentColor" stroke="none" transform="rotate(-14 12 6.4)"/></svg>',
  // Live Photos as macOS marks them: a dot, a ring, and a circle of short rays around them
  live: '<svg viewBox="0 0 24 24" width="14" height="14" fill="none" stroke="currentColor" aria-hidden="true"><circle cx="12" cy="12" r="2.4" fill="currentColor" stroke="none"/>' +
    '<circle cx="12" cy="12" r="5.4" stroke-width="1.6"/><g stroke-width="1.3" stroke-linecap="round">' +
    Array.from({ length: 24 }, (_, i) => { const a = i * Math.PI / 12, c = Math.cos(a), s = Math.sin(a), f = (r) => (12 + r * c).toFixed(2) + " " + (12 + r * s).toFixed(2); return "<path d=\"M" + f(8.6) + "L" + f(10.6) + "\"/>"; }).join("") + '</g></svg>',
  get livelink() { return this.live; },
  // A photo: a picture of hills and the sun
  photo: '<svg viewBox="0 0 24 24" width="14" height="14" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">' +
    '<rect x="3" y="4.5" width="18" height="15" rx="2.2"/><path d="M3.5 17l5-5.2 4.2 4.2 2.8-2.8 4.8 4.8"/><circle cx="15.8" cy="9" r="1.5" fill="currentColor" stroke="none"/></svg>',
  // An export: out of the box
  export: '<svg viewBox="0 0 24 24" width="14" height="14" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">' +
    '<path d="M12 3.5v11M7.8 7.6 12 3.5l4.2 4.1"/><path d="M5 11.5v7.3a1.7 1.7 0 0 0 1.7 1.7h10.6a1.7 1.7 0 0 0 1.7-1.7v-7.3"/></svg>',
  tag: '<svg viewBox="0 0 24 24" width="14" height="14" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">' +
    '<path d="M3 4.6v6.2c0 .5.2.9.5 1.2l8.6 8.6a1.6 1.6 0 0 0 2.3 0l6.2-6.2a1.6 1.6 0 0 0 0-2.3L12 3.5c-.3-.3-.7-.5-1.2-.5H4.6C3.7 3 3 3.7 3 4.6z"/><circle cx="7.6" cy="7.6" r="1.3" fill="currentColor" stroke="none"/></svg>',
  problem: '<svg viewBox="0 0 24 24" width="13" height="13" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">' +
    '<path d="M12 8.5v4.5M12 16.6v.1"/><path d="M10.3 3.9 2.6 17.6A2 2 0 0 0 4.3 20.5h15.4a2 2 0 0 0 1.7-2.9L13.7 3.9a2 2 0 0 0-3.4 0z"/></svg>',
};
const TILE_KIND_NAME = { moment: "Moment", live: "Live Photo", livelink: "Live photo link", photo: "Photo", export: "Export" };

// What an item is, as its icon says: null for a clip. Live: what the player plays as a live photo (its icon there, hover
// or Space): a Live Photo, a Motion Photo, and a photo link (its clip around the frame)
function tileKind(it) {
  if (it.kind === "moment") return "moment";
  if (it.kind === "still") return it.form === "Live Photo" || it.form === "Motion Photo" ? "live" : it.linked ? "livelink" : "photo";
  if (it.kind === "export") return "export";
  return null;
}

// The clips by their path in the tree (for a photo link's clip), made again only when the scan's list is new
function tileClip(id) {
  const list = (typeof RAW_FOOTAGE !== "undefined" && RAW_FOOTAGE && RAW_FOOTAGE.items) || [];
  if (TILE.clipsFrom !== list) { TILE.clipsFrom = list; TILE.clips = new Map(list.map((c) => [c.id, c])); }
  return TILE.clips.get(id) || null;
}
// The turn a clip has in its own file: 0, 90, 180 or 270, clockwise
function clipTurn(c) {
  const src = c && c.own && c.own.source && typeof c.own.source === "object" ? c.own.source : null;
  const r = src ? Math.round(Number(src.rotation) || 0) : 0;
  const n = ((r % 360) + 360) % 360;
  return n === 90 || n === 180 || n === 270 ? n : 0;
}
// The turn an item's picture is shown with: a clip's own, a photo link's clip's; nothing else (made turned already)
function tileTurn(it) {
  if (!it) return 0;
  if (it.kind === "clip") return clipTurn(it);
  if (it.kind === "still" && it.linked && it.link && it.link.clip) return clipTurn(tileClip(it.link.clip));
  return 0;
}
// An item's picture, turned as the item is now; marked tall when it's known to be taller than wide
function itemImg(it) {
  if (!it.thumbPath) return "";
  const t = tileTurn(it), cls = [TILE_TALL.get(it.thumbPath) ? "tall" : "", t ? "t" + t : ""].filter(Boolean).join(" ");
  return cls ? img(it.thumbPath).replace("<img ", '<img class="' + cls + '" ') : img(it.thumbPath);
}

// The albums (the user's: not Generated, not Other folders) an item is in, as the page shows them ("Egypt › Dives").
// A clip: every album with a link to it. A moment, a photo, an export: the album its file is in
function itemAlbums(it) {
  const name = (d) => d.split("/").join(" › ");
  if (it.kind === "clip") {
    const links = (typeof RAW_FOOTAGE !== "undefined" && RAW_FOOTAGE && RAW_FOOTAGE.links) || [];
    if (TILE.links !== links) {
      TILE.links = links;
      TILE.linksOf = new Map();
      for (const l of links) {
        const i = l.rel.lastIndexOf("/"), a = albumShown(i < 0 ? "" : l.rel.slice(0, i));
        if (!userAlbum(a)) continue;
        const list = TILE.linksOf.get(l.clip) || [];
        if (list.indexOf(a) < 0) list.push(a);
        TILE.linksOf.set(l.clip, list);
      }
      for (const list of TILE.linksOf.values()) list.sort((a, b) => a.localeCompare(b));
    }
    return (TILE.linksOf.get(it.id) || []).map(name);
  }
  const rel = typeof it.id === "string" ? it.id : "";
  const i = rel.lastIndexOf("/");
  const a = albumShown(i < 0 ? "" : rel.slice(0, i));
  return userAlbum(a) ? [name(a)] : [];
}

// ---------- the albums' text, fitted ----------

// A text's width in the caption's font: measured on a canvas (no layout), else a fair guess (no canvas: the tests)
function tileMeasure() {
  if (TILE.measure) return TILE.measure;
  let ctx = null;
  // (jsdom, the page's tests: no canvas, said loudly when asked)
  if (!/jsdom/i.test(navigator.userAgent || "")) try { ctx = document.createElement("canvas").getContext("2d"); } catch (_) { ctx = null; }
  if (ctx) {
    let fam = "";
    try { fam = getComputedStyle(document.body).fontFamily; } catch (_) { fam = ""; }
    ctx.font = "12px " + (fam || "system-ui, sans-serif");
    const cache = new Map();
    TILE.measure = (s) => { let w = cache.get(s); if (w == null) { w = ctx.measureText(s).width; if (cache.size > 5000) cache.clear(); cache.set(s, w); } return w; };
  } else TILE.measure = (s) => s.length * 6.6;
  return TILE.measure;
}
// The longest start of `text` (by characters) that fits in `w`; at a word's end when that loses little
function fitHead(text, w, measure) {
  let lo = 0, hi = text.length;
  while (lo < hi) { const m = (lo + hi + 1) >> 1; if (measure(text.slice(0, m)) <= w) lo = m; else hi = m - 1; }
  const sp = text.lastIndexOf(" ", lo);
  return sp > lo * 0.7 && lo < text.length ? sp + 1 : lo;
}
// …and the longest end (its first character's index) that fits in `w` after the ellipsis
function fitTail(text, w, measure) {
  let lo = 0, hi = text.length;
  while (lo < hi) { const m = (lo + hi + 1) >> 1; if (measure("…" + text.slice(text.length - m)) <= w) lo = m; else hi = m - 1; }
  const start = text.length - lo, sp = text.indexOf(" ", start);
  return sp >= 0 && sp < start + lo * 0.3 ? sp + 1 : start;
}
// One line: the beginning and the end, the middle taken out ("Egypt 2025 — Da…cond week")
function fitMiddle(text, w, measure) {
  if (measure(text) <= w) return text;
  let lo = 0, hi = text.length;
  const at = (k) => text.slice(0, Math.ceil(k / 2)).trimEnd() + "…" + text.slice(text.length - Math.floor(k / 2)).trimStart();
  while (lo < hi) { const m = (lo + hi + 1) >> 1; if (measure(at(m)) <= w) lo = m; else hi = m - 1; }
  return at(lo);
}
// A text in two lines at most, `w` wide: as it wraps when it fits; else the beginning on the first line and the end on
// the second, the middle taken out ("Egypt 2025 — Dahab, the" / "…Canyon, second week")
function fitLines(text, w, measure) {
  text = String(text || "").trim();
  if (!text || measure(text) <= w) return text ? [text] : [];
  const h = Math.max(1, fitHead(text, w, measure));
  const rest = text.slice(h).trim();
  if (measure(rest) <= w) return [text.slice(0, h).trim(), rest];
  const t = Math.max(h, fitTail(text, w, measure));
  return [text.slice(0, h).trim(), "…" + text.slice(t).trim()];
}
// The caption's lines for a list of albums: one album in two lines at most; two, one line each; more, the first and
// "the second +N"
function albumLines(names, w, measure) {
  if (!names.length) return [];
  if (names.length === 1) return fitLines(names[0], w, measure);
  const more = names.length > 2 ? " +" + (names.length - 2) : "";
  return [fitMiddle(names[0], w, measure), fitMiddle(names[1], w - (more ? measure(more) : 0), measure) + more];
}

// How wide the caption's text is in the grid now: its columns as the CSS lays them out (minmax(180px, 1fr), 12 apart,
// 14 in from the sides), less the tile's padding and border
function tileWidth(main) {
  const mw = (main && main.clientWidth) || 0;
  if (!mw) return 180 - TILE.inner;
  const w = mw - 2 * TILE.pad;
  const cols = Math.max(1, Math.floor((w + TILE.gap) / (TILE.minCol + TILE.gap)));
  return Math.floor((w - (cols - 1) * TILE.gap) / cols) - TILE.inner;
}

// ---------- the tile ----------

// A tile's picture with what's over it
function tilePicture(it) {
  const k = tileKind(it);
  let over = "";
  if (k) over += '<span class="kd" title="' + TILE_KIND_NAME[k] + '" aria-label="' + TILE_KIND_NAME[k] + '" role="img">' + TILE_ICON[k] + '</span>';
  else if (it.is360) over += '<span class="kd tx" title="360° video">360°</span>';
  const why = tileProblems(it);
  if (why.length && !it.pending) over += '<span class="pb" role="img" title="' + esc(why.join("; ")) + '" aria-label="' + esc(why.join("; ")) + '">' + TILE_ICON.problem + '</span>';
  if (it.dur && it.kind !== "still") over += '<span class="d">' + esc(it.dur) + '</span>';
  over += cullBadge(it);
  return '<span class="th">' + itemImg(it) + over + '</span>';
}
// What's wrong with an item, as the list layout's chips say it
function tileProblems(it) {
  const out = [];
  if (it.offline) out.push("Offline");
  if (it.kind === "clip" && it.conflict) out.push(it.own_conflict.length + " own files");
  if (it.kind === "moment" && !it.link) out.push("No link");
  return out;
}
// The caption: the albums, and the tag icon; `w`: its text's width
function tileCaption(it, w) {
  const tags = (it.tags || []).length > 0, albums = itemAlbums(it);
  const lines = albumLines(albums, w - (tags ? TILE.tagW : 0), tileMeasure());
  return '<span class="cap"' + (albums.length ? ' title="' + esc((albums.length > 1 ? "In the albums " : "In the album ") + albums.join(", ")) + '"' : "") + '><span class="alb">' +
    lines.map((l) => '<span class="al">' + esc(l) + '</span>').join("") + '</span>' +
    (tags ? '<span class="tg" role="img" title="Tagged: ' + esc(it.tags.join(", ")) + '" aria-label="Tagged">' + TILE_ICON.tag + '</span>' : "") + '</span>';
}
// The tile of an item, as a button of the grid. `o`: { cls, sel, renaming (the field's HTML), w (the caption's width) }
function itemTile(it, key, o) {
  o = o || {};
  const label = [TILE_KIND_NAME[tileKind(it)] || (it.kind === "clip" ? "Clip" : ""), it.name].filter(Boolean).join(" ");
  return '<button type="button" class="card tile' + (o.cls || "") + (o.sel ? " sel" : "") + (it.offline ? " off" : "") + (it.pending ? " pending" : "") + '"' + (it.pending ? ' title="Being written…"' : "") +
    ' data-a="pick" data-v="' + esc(key) + '" aria-pressed="' + !!o.sel + '" aria-label="' + esc(label) + '">' + tilePicture(it) +
    (o.renaming || tileCaption(it, o.w || tileWidth(null))) + '</button>';
}

// A picture that's taller than wide is shown whole in a tile (not cut to its middle strip): marked as it loads, and
// remembered for the next time it's drawn (a picture already in the page loads at once, maybe before it's watched)
const TILE_TALL = new Map();
document.addEventListener("load", (e) => {
  const el = e.target;
  if (!el || el.tagName !== "IMG" || !el.naturalWidth) return;
  const tall = el.naturalHeight > el.naturalWidth * 1.05, p = el.getAttribute("data-thumb");
  if (p) TILE_TALL.set(p, tall);
  el.classList.toggle("tall", tall);
}, true);

// The grid laid out again when its columns' width changes (the window, the side panel): the albums' text is fitted to it
let TILE_OBS = null, TILE_T = 0;
function tileWatch(main) {
  if (TILE_OBS || typeof ResizeObserver !== "function" || !main) return;
  TILE_OBS = new ResizeObserver(() => {
    clearTimeout(TILE_T);
    TILE_T = setTimeout(() => { if (S.layout === "grid" && TILE.w && tileWidth(main) !== TILE.w && main.querySelector(".grid > .tile")) renderMain(); }, 120);
  });
  TILE_OBS.observe(main);
}
