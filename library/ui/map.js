// Map: a view inside Albums (albumviews.js) of everything that has a location, as pins on a map. Pins near each
// other at the zoom shown are one pin with a picture and a count; zooming in splits it; a click lists its items in
// the ordinary grid. What has no location is counted, and listed on request.
//
// The map is Leaflet (vendor/leaflet, BSD-2-Clause) with OpenStreetMap's tiles, loaded only when the view is first
// opened. Without a connection (or without Leaflet) the places are a list with their counts; the same list is one
// click away with a connection too. Locations are what the page has already (an item's `geo`): nothing is looked up
// or read for this view. The grouping into pins is the page's own (Web Mercator cells at the zoom), so a pin is a
// place in S.path ("c:<zoom>:<x>:<y>") that Back, Forward and a reload find again.

const MAPV = { mode: avRemembered("afl.map", "map") === "list" ? "list" : "map", cell: 84, el: null, map: null, pins: null, tiles: null,
  loading: false, offline: false, ok: 0, bad: 0, placed: [], fitted: null, dragged: 0 };
const MAP_TILES = "https://tile.openstreetmap.org/{z}/{x}/{y}.png";

ALBUM_VIEWS.push(["map", "Map", drawMap, avEverything]);

// A point on the Web Mercator plane at a zoom, px (256 px tiles): what the map itself draws by
function mapXY(lat, lon, z) {
  const s = 256 * Math.pow(2, z), r = Math.max(-85.0511, Math.min(85.0511, lat)) * Math.PI / 180;
  return [(lon + 180) / 360 * s, (1 - Math.log(Math.tan(r) + 1 / Math.cos(r)) / Math.PI) / 2 * s];
}
function mapCellOf(g, z) { const p = mapXY(g.lat, g.lon, z); return "c:" + z + ":" + Math.floor(p[0] / MAPV.cell) + ":" + Math.floor(p[1] / MAPV.cell); }
// The items in pins at a zoom: [{ id, items, lat, lon }], a pin at the middle of its items
function mapClusters(list, z) {
  const m = new Map();
  for (const it of list) {
    const g = avGeo(it);
    if (!g) continue;
    const id = mapCellOf(g, z);
    let c = m.get(id);
    if (!c) { c = { id, items: [], lat: 0, lon: 0 }; m.set(id, c); }
    c.items.push(it); c.lat += g.lat; c.lon += g.lon;
  }
  const out = Array.from(m.values());
  for (const c of out) { c.lat /= c.items.length; c.lon /= c.items.length; }
  return out;
}
// The items by the name of their place, the fullest first: the list shown without a map
function mapPlaces(list) {
  const m = new Map();
  for (const it of list) {
    const g = avGeo(it);
    if (!g) continue;
    const name = avPlaceName(g);
    let p = m.get(name);
    if (!p) { const pl = g.place || {}; p = { id: "p:" + name, name, sub: [pl.region, pl.country].filter((x) => x && x !== name).join(", "), items: [] }; m.set(name, p); }
    p.items.push(it);
  }
  return Array.from(m.values()).sort((a, b) => b.items.length - a.items.length || a.name.localeCompare(b.name));
}
// What a place in the path holds: "none" (no location), "p:<name>", "c:<zoom>:<x>:<y>"
function mapSel(id, list) {
  if (id === "none") return { label: "Without a location", items: list.filter((it) => !avGeo(it)) };
  if (id.startsWith("p:")) { const name = id.slice(2); return { label: name, items: list.filter((it) => { const g = avGeo(it); return g && avPlaceName(g) === name; }) }; }
  const z = +id.split(":")[1];
  const its = list.filter((it) => { const g = avGeo(it); return g && mapCellOf(g, z) === id; });
  return { label: mapLabel(its), items: its };
}
function mapLabel(its) {
  const p = avPlaces(its);
  return !p.length ? "This place" : p[0][0] + (p.length > 1 ? " and " + (p.length - 1) + " more " + (p.length === 2 ? "place" : "places") : "");
}
function mapIsOffline() { return MAPV.offline || (typeof navigator !== "undefined" && navigator.onLine === false); }

function drawMap(main) {
  const v = albumViewOpen(), c = ctx();
  const all = avEverything(), list = avShown(all);
  const st = main.scrollTop;
  if (S.path.length > 1) {
    const sel = mapSel(S.path[1], list);
    if (sel.items.length || !DATA.sources) {
      VISIBLE = sel.items.map(keyOf);
      main.innerHTML = '<div class="mbar">' + avCrumbs(v, sel.label, S.path[1] === "none" ? "Set a location in the side panel’s Location part" : avKinds(sel.items) + " recorded here") +
        '<div class="selbar">' + avSelBar(sel.items.length, c) + '</div></div>' + avGrid(sel.items);
      main.scrollTop = st;
      fillThumbs();
      return;
    }
    S.path = [S.path[0]];
  }
  VISIBLE = [];
  const placed = list.filter((it) => avGeo(it)), none = list.length - placed.length;
  const places = mapPlaces(placed);
  const offline = mapIsOffline();
  const asMap = MAPV.mode === "map" && !offline;
  let h = avCrumbs(v, null, "Every original, moment and photo by where it was recorded");
  h += '<div class="selbar"><div class="seg" role="tablist" aria-label="Places as">' + [["map", "Map"], ["list", "Places"]].map((x) =>
    '<button type="button" role="tab" aria-selected="' + ((x[0] === "map") === asMap) + '" class="' + ((x[0] === "map") === asMap ? "on" : "") + '" data-a="mapmode" data-v="' + x[0] + '">' + x[1] + '</button>').join("") + '</div>' +
    '<span class="sub" style="display:inline">' + avCount(placed.length, "item", "items") + " at " + avCount(places.length, "place", "places") + (list.length !== all.length ? " (filtered)" : "") + '</span>' +
    (none ? '<span class="sub" style="display:inline">· ' + avCount(none, "item", "items") + ' without a location</span><button type="button" class="btn sm" data-a="mapplace" data-v="none">List them</button>' : "") + '</div>';
  h = '<div class="mbar">' + h + '</div>'; // pinned while the places' list scrolls (drawing.js pinBar)
  if (!DATA.sources) { main.innerHTML = h + '<div class="empty"><span class="spin" aria-hidden="true"></span> Scanning the project’s files…</div>'; return; }
  if (!placed.length) {
    main.innerHTML = h + '<div class="empty">' + (list.length ? "Nothing here has a location yet. The side panel’s Location part sets one from Apple Photos or by hand; a camera with GPS writes its own." : "Nothing in the library yet.") + '</div>';
    return;
  }
  if (asMap) {
    h += '<div class="mapbox" id="mapbox">' + (window.L ? "" : '<div class="empty"><span class="spin" aria-hidden="true"></span> Loading the map…</div>') + '</div>' +
      '<div class="mapnote">Map: © OpenStreetMap contributors (openstreetmap.org/copyright). Its pictures come from tile.openstreetmap.org as you look; nothing of the library is sent.</div>';
    main.innerHTML = h;
    MAPV.placed = placed;
    mapAttach();
    return;
  }
  if (offline) h += '<div class="indexing">The map can’t be reached now (its pictures come from OpenStreetMap over the network): the places as a list. <button type="button" class="btn sm" data-a="mapretry">Try the map again</button></div>';
  h += '<div class="placelist" role="list">' + places.map((p) => {
    const cover = (p.items.find((it) => it.thumbPath) || {}).thumbPath;
    return '<button type="button" role="listitem" class="placerow" data-a="mapplace" data-v="' + esc(p.id) + '"><span class="th">' + (cover ? img(cover) : "") + '</span>' +
      '<span style="min-width:0"><span class="nm">' + esc(p.name) + '</span><span class="sub">' + esc(p.sub) + '</span></span><span class="cnt">' + esc(avKinds(p.items)) + '</span></button>';
  }).join("") + '</div>';
  main.innerHTML = h;
  main.scrollTop = st;
  fillThumbs();
}
// Leaflet, once: its style and its script from the app's own files
function mapLoad() {
  if (MAPV.loading || window.L) return;
  MAPV.loading = true;
  const css = document.createElement("link");
  css.rel = "stylesheet"; css.href = "vendor/leaflet/leaflet.css";
  document.head.appendChild(css);
  const s = document.createElement("script");
  s.src = "vendor/leaflet/leaflet.js";
  const done = (ok) => { if (!MAPV.loading) return; MAPV.loading = false; if (!ok || !window.L) MAPV.offline = true; if (albumViewOpen()) renderMain(); };
  s.onload = () => done(true);
  s.onerror = () => done(false);
  document.head.appendChild(s);
  setTimeout(() => done(!!window.L), 8000);
}
// The map into its place in the page: made once and kept (the page around it is drawn again at each change)
function mapAttach() {
  const box = $("mapbox");
  if (!box) return;
  if (!window.L) { mapLoad(); return; }
  const L = window.L;
  if (!MAPV.map) {
    MAPV.el = document.createElement("div");
    box.appendChild(MAPV.el);
    MAPV.map = L.map(MAPV.el, { minZoom: 2, maxZoom: 18, worldCopyJump: true, attributionControl: false, zoomSnap: 1 });
    MAPV.tiles = L.tileLayer(MAP_TILES, { maxZoom: 19, crossOrigin: false });
    MAPV.tiles.on("tileload", () => { MAPV.ok++; });
    MAPV.tiles.on("tileerror", () => {
      // No picture came at all: no connection. Some came: a gap in the map, left as it is
      if (++MAPV.bad === 1) setTimeout(() => { if (!MAPV.ok && MAPV.bad) { MAPV.offline = true; if (albumViewOpen()) renderMain(); } }, 2500);
    });
    MAPV.tiles.addTo(MAPV.map);
    MAPV.pins = L.layerGroup().addTo(MAPV.map);
    MAPV.map.on("zoomend moveend", mapPins);
    MAPV.map.on("dragend", () => { MAPV.dragged = Date.now(); });
  } else box.appendChild(MAPV.el);
  MAPV.map.invalidateSize();
  // The whole library in view when it's first shown, and again when what's shown changed (a filter, another project)
  const sig = AV.cache && AV.cache.items, n = MAPV.placed.length;
  if (MAPV.fitted !== sig || MAPV.fittedN !== n) {
    MAPV.fitted = sig; MAPV.fittedN = n;
    const pts = MAPV.placed.map((it) => { const g = avGeo(it); return [g.lat, g.lon]; });
    MAPV.map.fitBounds(L.latLngBounds(pts).pad(0.15), { maxZoom: 12, animate: false });
  }
  mapPins();
}
function mapPinHtml(c) {
  const cover = (c.items.find((it) => it.thumbPath) || {}).thumbPath;
  const n = c.items.length;
  return '<button type="button" class="mappin" data-a="mapplace" data-v="' + esc(c.id) + '" aria-label="' + esc(avCount(n, "item", "items") + " at " + mapLabel(c.items)) + '" title="' + esc(mapLabel(c.items)) + '">' +
    '<span class="th">' + (cover ? img(cover) : "") + '</span><span class="n">' + (n > 9999 ? Math.round(n / 1000) + "k" : n) + '</span></button>';
}
function mapPins() {
  if (!MAPV.map || !MAPV.el.isConnected) return;
  const L = window.L, z = MAPV.map.getZoom(), b = MAPV.map.getBounds().pad(0.25);
  MAPV.pins.clearLayers();
  for (const c of mapClusters(MAPV.placed, z)) {
    if (!b.contains([c.lat, c.lon])) continue;
    L.marker([c.lat, c.lon], { keyboard: false, icon: L.divIcon({ className: "", html: mapPinHtml(c), iconSize: [64, 64], iconAnchor: [32, 32] }) }).addTo(MAPV.pins);
  }
  fillThumbs();
}
// The view's own buttons (albumviews.js's click handler)
function mapClick(a, v) {
  if (a === "mapmode") { MAPV.mode = v; remember("afl.map", v); if (v === "map") { MAPV.offline = false; MAPV.ok = MAPV.bad = 0; } renderMain(); return true; }
  if (a === "mapretry") { MAPV.offline = false; MAPV.ok = MAPV.bad = 0; MAPV.mode = "map"; if (MAPV.tiles) MAPV.tiles.redraw(); renderMain(); return true; }
  if (a === "mapplace") {
    // A click that ends a drag of the map isn't a click on the pin under it
    if (Date.now() - MAPV.dragged < 250) return true;
    S.path = [S.path[0], v]; S.sel = {}; S.anchor = null; AV.more = 400;
    renderAll();
    $("main").scrollTop = 0;
    return true;
  }
  return false;
}
