// Views inside Albums that aren't folders (ALBUM_VIEWS in albums.js): what Timeline (timeline.js) and Map (map.js)
// share. Loaded after albums.js and before app.js, whose state and helpers it uses when called; app.js calls in at
// the lines marked "albumviews.js" (items(), buildFacets(), renderMain(), goUp()).
//
// A view is [id, label, draw(main), items()]. While one is open the page's view is still Albums (S.view "footage")
// and S.path[0] is the view's label behind AV_MARK, a character no folder's name starts with: so the breadcrumbs,
// Back and Forward and the keys work as for a folder, and an album of the user's called "Timeline" stays an album.
// The view's items are what items() gives then, so the selection, ctx(), the menu, the side panel and opening in the
// player take them as in any other view.

const AV_MARK = "​";
const AV = { cache: null, more: 400 };
// What was chosen last time on this computer; here, not app.js's remembered(): these files load before it
function avRemembered(k, d) { try { const v = localStorage.getItem(k); return v == null ? d : v; } catch (_) { return d; } }

function albumViewOpen() {
  if (S.view !== "footage" || !S.path.length || S.path[0].charAt(0) !== AV_MARK) return null;
  const label = S.path[0].slice(1);
  return ALBUM_VIEWS.find((v) => v[1] === label) || null;
}
function albumViewItems() { const v = albumViewOpen(); return v ? v[3]() : null; }
// Drawn in place of the folder's list; false: no view is open
function albumViewDraw(main) {
  const v = albumViewOpen();
  if (!v) return false;
  v[2](main);
  return true;
}
// The views, as tiles above the albums at the top of Albums
function albumViewTiles() {
  if (!ALBUM_VIEWS.length) return "";
  return '<div class="avtiles" role="group" aria-label="Views of the whole library">' + ALBUM_VIEWS.map((v) =>
    '<button type="button" class="avtile" data-a="aview" data-v="' + esc(v[0]) + '">' + (AV_ICON[v[0]] || "") + '<span><b>' + esc(v[1]) + '</b><span class="sub">' + esc(AV_SUB[v[0]] || "") + '</span></span></button>').join("") + '</div>';
}
const AV_ICON = {
  timeline: '<svg width="22" height="22" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><rect x="3.5" y="5" width="17" height="15" rx="2"/><path d="M3.5 10h17M8 3v4M16 3v4"/></svg>',
  map: '<svg width="22" height="22" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M12 21s-6.5-6.2-6.5-11a6.5 6.5 0 0 1 13 0c0 4.8-6.5 11-6.5 11z"/><circle cx="12" cy="10" r="2.3"/></svg>',
};
const AV_SUB = { timeline: "Everything by day, month and year", map: "Everything by where it was recorded" };
function albumViewGo(id) {
  const v = ALBUM_VIEWS.find((x) => x[0] === id);
  if (!v) return;
  S.view = "footage"; S.path = [AV_MARK + v[1]]; S.sel = {}; S.anchor = null; S.q = ""; S.flat = false; S.renaming = null;
  renderAll();
  $("main").scrollTop = 0;
  // Moments and photos too, when the page has such views: read once, shown as they come
  const want = ["footage", "moments", "stills"].filter((x) => x === "footage" || (typeof VIEW_ROOT === "object" && VIEW_ROOT[x]));
  const before = want.map((x) => DATA[x]);
  needViews(want).then(() => { if (want.some((x, i) => DATA[x] !== before[i]) && albumViewOpen()) renderAll(); }, () => {});
}

// The Info part with nothing selected: what the view is of (it has no folder of its own)
function albumViewInfo() {
  const v = albumViewOpen(), all = avEverything();
  return '<div class="ihead"><b>' + esc(v[1]) + '</b><span class="sub">' + esc(avKinds(all) || "Nothing yet") + '</span></div>' +
    infoSec("Files", kv([["Project", pathsHtml([S.root]), false, true]])) +
    '<p class="sub wrap" style="padding:0 16px">' + esc(AV_SUB[v[0]] || "") + ': the whole library, not a folder. Select a clip, moment or photo to see what it is.</p>';
}

// Everything in the library, for these views: every original once (as Originals has them), and the moments and photos
// the page has, by their kind: from their own views, and from Albums once albums hold them (each file once)
function avEverything() {
  const src = DATA.sources, mom = DATA.moments, st = DATA.stills, foot = DATA.footage;
  const c = AV.cache;
  if (c && c.src === src && c.mom === mom && c.st === st && c.foot === foot) return c.items;
  const out = [], seen = new Set(), clips = {};
  for (const it of (src && src.items) || []) {
    // Not its link in an album: Delete here must not take a clip out of one
    const x = Object.assign({}, it, { preview: null });
    clips[it.id] = x;
    out.push(x);
  }
  const other = (list) => {
    for (const it of list || []) {
      if (it.kind !== "moment" && it.kind !== "still") continue;
      if (seen.has(it.path)) continue;
      seen.add(it.path);
      out.push(it);
    }
  };
  other(mom && mom.items); other(st && st.items); other(foot && foot.items);
  AV.cache = { src, mom, st, foot, items: out, clips, geo: new Map(), sorted: null };
  return out;
}
// Where an item was recorded: a clip's own location; a moment's or a photo's is its clip's
function avGeo(it) {
  if (it.geo) return it.geo;
  if (it.kind === "clip") return null;
  avEverything();
  const m = AV.cache.geo;
  if (m.has(it)) return m.get(it);
  const id = clipOf(it), c = id ? AV.cache.clips[id] : null;
  const g = (c && c.geo) || null;
  m.set(it, g);
  return g;
}
// A place's short name ("Dahab"), as a day's header and a pin have it; with no name known, the point to 0.1°
function avPlaceName(g) {
  if (!g) return "";
  const p = g.place || {};
  return p.locality || p.site || p.region || p.water || p.country || avCoord(g.lat, g.lon);
}
// The wider place a trip is at ("South Sinai"), else the point to 1°
function avRegion(g) {
  if (!g) return "";
  const p = g.place || {};
  return p.region || p.country || p.water || Math.round(g.lat) + "," + Math.round(g.lon);
}
function avCoord(lat, lon) {
  return Math.abs(lat).toFixed(1) + "° " + (lat < 0 ? "S" : "N") + ", " + Math.abs(lon).toFixed(1) + "° " + (lon < 0 ? "W" : "E");
}
// The names in a list of items, the most frequent first: [[name, count], …]
function avPlaces(items, nameOf) {
  const n = new Map();
  for (const it of items) { const g = avGeo(it); if (!g) continue; const k = (nameOf || avPlaceName)(g); n.set(k, (n.get(k) || 0) + 1); }
  return Array.from(n).sort((a, b) => b[1] - a[1] || a[0].localeCompare(b[0]));
}
// …as a line's text: the first few. Points without a name (shown as coordinates) only when no place has one, and then one
function avNames(places, max) {
  const named = places.filter((p) => !/^\d+\.\d° [NS], /.test(p[0]));
  places = named.length ? named : places.slice(0, 1);
  const names = places.slice(0, max).map((p) => p[0]);
  return names.join(", ") + (places.length > max ? " +" + (places.length - max) : "");
}
function avCount(n, one, many) { return n.toLocaleString("en-US") + " " + (n === 1 ? one : many); }
function avKinds(items) {
  let c = 0, m = 0, s = 0;
  for (const it of items) { if (it.kind === "clip") c++; else if (it.kind === "moment") m++; else s++; }
  return [c ? avCount(c, "clip", "clips") : "", m ? avCount(m, "moment", "moments") : "", s ? avCount(s, "photo", "photos") : ""].filter(Boolean).join(", ");
}
// What the search and the filters leave of a list
function avShown(list) { return S.q.trim() || filtering() ? list.filter((it) => matches(it)) : list; }

// The view's top lines: the breadcrumbs (Albums › the view › what's open in it) and what the view says of itself
function avCrumbs(v, more, where) {
  let h = '<div class="crumbs">' + navButtons() + '<button type="button" data-a="crumb" data-v="0">Albums</button><span aria-hidden="true">›</span>' +
    '<button type="button" class="' + (more ? "" : "here") + '" data-a="crumb" data-v="1">' + esc(v[1]) + '</button>';
  if (more) h += '<span aria-hidden="true">›</span><button type="button" class="here" data-a="crumb" data-v="2">' + esc(more) + '</button>';
  return h + '<span class="where">' + esc(where) + '</span></div>';
}
function avSelBar(n, c) {
  const nSel = VISIBLE.filter((k) => S.sel[k]).length;
  return '<span class="sub" style="display:inline">' + (nSel ? nSel + " of " + n + " selected" : "None selected") + '</span><button type="button" class="btn sm" data-a="selall"' + (nSel === n ? " disabled" : "") + '>Select all</button><button type="button" class="btn sm" data-a="desel"' + (nSel ? "" : " disabled") + '>Select none</button>' + albumBar(c);
}
// An item as the ordinary grid has it
function avCard(it, cls, sub) {
  const key = keyOf(it), sel = !!S.sel[key];
  return '<button type="button" class="card' + (cls || "") + (sel ? " sel" : "") + (it.offline ? " off" : "") + '" data-a="pick" data-v="' + esc(key) + '" aria-pressed="' + sel + '">' +
    '<span class="th">' + (it.thumbPath ? img(it.thumbPath) : "") + (it.dur ? '<span class="d">' + esc(it.dur) + '</span>' : "") + (cls && typeof cullBadge === "function" ? cullBadge(it) : "") + '</span>' +
    '<span class="nm">' + esc(it.name) + '</span><span class="sub">' + esc(sub != null ? sub : it.sub) + '</span>' + (cls ? "" : '<span class="chips">' + chips(it) + '</span>') + '</button>';
}
// A list of items in the ordinary grid, the first AV.more of a long one and a button for the next
function avGrid(list) {
  const shown = list.slice(0, AV.more);
  return '<div class="grid">' + shown.map((it) => avCard(it)).join("") + '</div>' +
    (list.length > shown.length ? '<div class="selbar"><button type="button" class="btn sm" data-a="avmore">Show ' + Math.min(400, list.length - shown.length) + ' more</button><span class="sub" style="display:inline">' + shown.length + ' of ' + list.length + ' shown</span></div>' : "");
}

// The views' own buttons: taken here, before the page's own handler (which leaves what it doesn't know)
document.addEventListener("click", (e) => {
  const el = e.target.closest && e.target.closest("[data-a]");
  if (!el || el.disabled) return;
  const a = el.dataset.a, v = el.dataset.v;
  if (a === "aview") { albumViewGo(v); return; }
  if (a === "avmore") { AV.more += 400; renderMain(); return; }
  if (!albumViewOpen()) return;
  if (typeof timelineClick === "function" && timelineClick(a, v)) return;
  if (typeof mapClick === "function") mapClick(a, v);
});
