"use strict";
// Locations.
// One of the window's scripts that were app.js: index.html loads them in this order before app.js, as classic
// scripts sharing their top-level names, so each uses the others' when called (only earlier ones at load).

// ---------- locations (library/src/geo.rs, docs/design/geolocation.md) ----------
// A clip's location is the original's: set from Apple Photos (the photos nearest in time) or by hand, kept in its own
// file; else the camera's own GPS, read from the clip. Place names are looked up once on this computer.

const PLACES = {};          // "lat,lon" (0.01°) -> place, or null while it's being looked up / found nothing
let placesWanted = [], placesBusy = false;
function gkey(lat, lon) { return lat.toFixed(2) + "," + lon.toFixed(2); }
function wantPlace(lat, lon) {
  const k = gkey(lat, lon);
  if (k in PLACES) return;
  PLACES[k] = null;
  placesWanted.push([lat, lon]);
  if (!placesBusy) setTimeout(lookPlaces, 300);
}
async function lookPlaces() {
  if (placesBusy || !placesWanted.length) return;
  placesBusy = true;
  const pts = placesWanted.splice(0, 20);
  try {
    const got = await invoke("places_of", { points: pts, lookup: true });
    pts.forEach((p, i) => { if (got[i]) PLACES[gkey(p[0], p[1])] = got[i]; });
    derive(S.view === "sources" ? "footage" : S.view); renderAll();
  } catch (e) { /* offline: shown as coordinates */ }
  placesBusy = false;
  if (placesWanted.length) setTimeout(lookPlaces, 300);
}
function deriveGeo(it, probe) {
  const own = it.own && it.own.location;
  let g = null;
  if (own && typeof own.lat === "number" && typeof own.lon === "number") {
    g = { lat: own.lat, lon: own.lon, alt: own.alt, source: own.source || "manual", place: own.place || null, gap: own.photo_gap_s };
  } else if (probe && probe.gps && typeof probe.gps.lat === "number") {
    const k = gkey(probe.gps.lat, probe.gps.lon);
    g = { lat: probe.gps.lat, lon: probe.gps.lon, alt: probe.gps.alt, source: "camera", place: PLACES[k] || null };
    if (!(k in PLACES)) wantPlace(probe.gps.lat, probe.gps.lon);
  }
  it.geo = g;
  const p = (g && g.place) || {};
  it.country = g ? (p.country || (p.water ? "At sea" : "Not named yet")) : null;
  it.region = g ? (p.region || p.water || "—") : null;
  it.locality = g ? (p.site || p.locality || p.water || "—") : null;
}
function placeText(p) {
  if (!p) return "";
  const parts = [];
  for (const k of ["site", "locality", "region", "country"]) if (p[k] && parts.indexOf(p[k]) < 0) parts.push(p[k]);
  if (!parts.length && p.water) parts.push(p.water);
  return parts.join(", ");
}
// A small map of points (Apple Maps, drawn by the app on a Mac and kept), filled in after the Info is drawn
const MAPS = new Map(); // its points' key -> data URL, or null: none
function mapHtml(points) {
  if (!S.macos || !points.length) return "";
  const key = Array.from(new Set(points.map((p) => p[0].toFixed(4) + "," + p[1].toFixed(4)))).sort().join(";");
  const u = MAPS.get(key);
  if (u === null) return "";
  return '<div class="minimap"><img data-map="' + esc(key) + '" alt="Map: where ' + (points.length === 1 ? "it was" : "they were") + ' recorded"' + (u ? ' src="' + u + '"' : " hidden") + '></div>';
}
function fillMaps() {
  for (const el of document.querySelectorAll("img[data-map]")) {
    const key = el.getAttribute("data-map");
    if (MAPS.has(key)) continue;
    MAPS.set(key, undefined);
    const points = key.split(";").map((x) => x.split(",").map(Number));
    const dark = !!(window.matchMedia && window.matchMedia("(prefers-color-scheme: dark)").matches);
    invoke("mini_map", { points, w: 300, h: 170, dark }).then((u) => u || null, () => null).then((u) => {
      MAPS.set(key, u);
      for (const im of document.querySelectorAll('img[data-map="' + CSS.escape(key) + '"]')) { if (u) { im.src = u; im.hidden = false; } else im.parentNode.remove(); }
    });
  }
}
function coordText(lat, lon) {
  return Math.abs(lat).toFixed(5) + "° " + (lat < 0 ? "S" : "N") + ", " + Math.abs(lon).toFixed(5) + "° " + (lon < 0 ? "W" : "E");
}
function gapText(s) {
  if (s == null) return "";
  const a = Math.abs(s);
  const t = a < 60 ? "under a minute" : a < 3600 ? Math.round(a / 60) + " min" : a < 86400 ? (Math.round(a / 360) / 10) + " h" : (Math.round(a / 8640) / 10) + " days";
  if (a < 1) return "taken during the clip";
  return t + (s < 0 ? " before" : " after");
}
const GEO_SOURCE = { photos: "From Apple Photos", manual: "Set by hand", camera: "The camera’s GPS" };
function geoFacets(items, sections) {
  const clips = items.filter((i) => i.kind === "clip");
  if (!clips.length) return;
  const geo = clips.filter((i) => i.geo);
  const ctry = distinct(geo, (i) => i.country).map((c) => L("gc:" + c, c, (it) => it.country === c));
  ctry.push(L("gc:none", "No location", (it) => it.kind === "clip" && !it.geo));
  const regs = distinct(geo, (i) => i.country).map((c) => P("gr:" + c, c, distinct(geo.filter((i) => i.country === c), (i) => i.region)
    .map((r) => L("gr:" + c + "|" + r, r, (it) => it.country === c && it.region === r))));
  const pairs = distinct(geo, (i) => i.country + "|" + i.region);
  const locs = pairs.map((cr) => { const [c, r] = cr.split("|"); return P("gl:" + cr, r === "—" ? c : r, distinct(geo.filter((i) => i.country === c && i.region === r), (i) => i.locality)
    .map((l) => L("gl:" + cr + "|" + l, l, (it) => it.country === c && it.region === r && it.locality === l))); });
  const facets = [{ id: "gc", title: "Country", opts: ctry }];
  if (geo.length) {
    facets.push({ id: "gr", title: "Region", opts: regs }, { id: "gl", title: "Place", opts: locs },
      { id: "gs", title: "From", opts: Object.keys(GEO_SOURCE).map((k) => L("gs:" + k, GEO_SOURCE[k], (it) => !!it.geo && it.geo.source === k)) });
  }
  sections.push({ id: "geo", title: "Location", facets });
}
function locationInfo(it) {
  const g = it.geo;
  if (!g) return infoSec("Location", '<p class="sub wrap">None. Select it (or its folder) and use the Location tab: from Apple Photos' + (S.macos ? "" : " (on a Mac)") + ' or by hand.</p>');
  const p = g.place;
  const name = placeText(p) || (g.source === "camera" && !p ? "Looking up the place…" : "");
  return infoSec("Location", mapHtml([[g.lat, g.lon]]) + kv([["Place", name], ["Point", coordText(g.lat, g.lon) + (g.alt != null ? " · " + Math.round(g.alt) + " m" : ""), true],
    ["From", GEO_SOURCE[g.source] + (g.source === "photos" && g.gap != null ? ": the nearest photo " + gapText(g.gap) : "")]]) +
    '<div class="btns"><button type="button" class="btn sm" data-a="geomap" data-v="' + g.lat + "," + g.lon + '">Show on a map</button>' +
    (g.source !== "camera" ? '<button type="button" class="btn sm" data-a="georemove">Remove</button>' : "") + '</div>');
}
function locationBody(c) {
  const clips = c.selItems.filter((it) => it.kind === "clip");
  const own = clips.filter((it) => it.geo && it.geo.source !== "camera").length, cam = clips.filter((it) => it.geo && it.geo.source === "camera").length;
  let body = '<p>' + (c.any ? esc(clips.length + " clip" + (clips.length === 1 ? "" : "s") + ": " + own + " with a location, " + cam + " with the camera’s GPS, " + (clips.length - own - cam) + " without.") : "Select clips or folders.") + '</p>';
  if (S.macos) {
    body += '<div class="btns"><button type="button" class="btn sm pri" data-a="geosuggest"' + (clips.length && !S.geoBusy ? "" : " disabled") + '>' + (S.geoBusy ? '<span class="spin" aria-hidden="true"></span> Looking in Photos…' : "Suggest from Apple Photos") + '</button></div>' +
      '<span class="sub wrap">From the photos with a place you took nearest in time (your iPhone on the same trip). Photos is only read, never changed.</span>';
  } else {
    body += '<span class="sub wrap">Suggestions from Apple Photos are on a Mac. Here, set it by hand.</span>';
  }
  body += '<div class="lab" style="margin-top:10px">By hand</div><div class="tagin"><input id="geohand" type="text" value="' + esc(S.geoHand) + '" placeholder="27.7333, 34.25" aria-label="Latitude, longitude">' +
    '<button type="button" class="btn sm" data-a="geohand"' + (clips.length ? "" : " disabled") + '>Set</button></div>' +
    '<div class="tagin"><input id="geosite" type="text" value="' + esc(S.geoSite) + '" placeholder="Dive site (optional)" aria-label="Dive site"></div>' +
    '<span class="sub wrap">Latitude, longitude as a map copies them. Written into each original’s own file, beside it.</span>';
  if (own) body += '<div class="btns"><button type="button" class="btn sm" data-a="georemove">Remove ' + own + ' location' + (own === 1 ? "" : "s") + '</button></div>';
  return body;
}

// The sheet: Photos' suggestions by group
function geoClips() {
  return ctx().selItems.filter((it) => it.kind === "clip").map((it) => ({ id: it.id, path: (it.copies && it.copies[0] && it.copies[0].path) || it.path, folder: it.dir || "" }));
}
async function geoSuggest() {
  const status = await invoke("photos_access", { ask: false }).catch(() => "none");
  if (status === "notDetermined") { S.sheet = "geoask"; renderSheet(); return; }
  if (status !== "authorized" && status !== "limited") { S.sheet = "geodenied"; S.geoStatus = status; renderSheet(); return; }
  const sel = ctx().selItems.filter((it) => it.kind === "clip");
  S.geoClips = geoClips();
  S.geoHad = sel.filter((it) => it.geo && it.geo.source !== "camera").map((it) => it.id);
  S.geoKeep = true; S.geoPick = null;
  S.geoBusy = true; renderSide();
  try {
    S.geo = await invoke("suggest_locations", { clips: S.geoClips, shifts: {} });
    S.sheet = S.geo.status === "authorized" || S.geo.status === "limited" ? "geo" : "geodenied"; S.geoStatus = S.geo.status;
  } catch (e) { showError(String(e)); }
  S.geoBusy = false; renderSide(); renderSheet();
}
function zoneText(off) {
  if (off == null) return "UTC";
  const m = Math.round(off / 60), a = Math.abs(m);
  return "UTC" + (m < 0 ? "−" : "+") + Math.floor(a / 60) + (a % 60 ? ":" + String(a % 60).padStart(2, "0") : "");
}
function renderGeoAsk() {
  return '<div class="scrim" data-a="scrim"><div class="sheet narrow" role="dialog" aria-modal="true" aria-labelledby="geoTitle"><header><h2 id="geoTitle">Read your Apple Photos?</h2>' +
    '<span class="sub">To suggest where clips were recorded</span></header><div class="sb"><p class="wrap">The Library reads the time and place of the photos you took around these clips (your iPhone on the same trip), and nothing else: not the pictures, and it never changes your library.</p>' +
    '<p class="wrap">macOS now asks once whether Action Footage Library may access Photos. Its prompt says “access”: macOS has no read-only choice for apps. You can take it back in System Settings › Privacy &amp; Security › Photos.</p></div>' +
    '<footer><span class="sp"></span><button type="button" class="btn" data-a="closesheet">Not now</button><button type="button" class="btn pri" data-a="geoallow">Continue</button></footer></div></div>';
}
function renderGeoDenied() {
  const why = S.geoStatus === "none" ? "Apple Photos isn’t on this computer." : S.geoStatus === "restricted" ? "Access to Photos is restricted on this Mac." :
    "Action Footage Library isn’t allowed to read Photos. Allow it in System Settings › Privacy & Security › Photos, then try again.";
  return '<div class="scrim" data-a="scrim"><div class="sheet narrow" role="dialog" aria-modal="true" aria-labelledby="geoTitle"><header><h2 id="geoTitle">Photos can’t be read</h2></header>' +
    '<div class="sb"><p class="wrap">' + esc(why) + '</p></div><footer><span class="sp"></span><button type="button" class="btn pri" data-a="closesheet">OK</button></footer></div></div>';
}
// The slider's stops, hours: 3 h to 5 days (Anton)
const GEO_STOPS = [3, 6, 9, 12, 18, 24, 36, 48, 72, 96, 120];
function hoursText(h) { return h < 1 ? Math.max(1, Math.round(h * 60)) + " min" : h < 48 ? (Math.round(h * 10) / 10) + " h" : (Math.round(h / 2.4) / 10) + " days"; }
// What the sheet shows for the slider's limit: the places of the nearest photos, by name; the clips too far
function geoPlan() {
  const r = S.geo || {};
  const places = r.places || {};
  const limit = GEO_STOPS[S.geoMax] * 3600;
  const keep = new Set(S.geoKeep ? S.geoHad : []);
  const names = {}, far = [];
  const nameOf = (k) => (places[k] && places[k].text) || (places[k] ? coordText(places[k].lat, places[k].lon) : k);
  for (const g of r.groups || []) for (const c of g.clips || []) {
    if (keep.has(c.id)) continue;
    const p = c.photo;
    if (p && p.place) {
      const n = nameOf(p.place);
      const e = names[n] || (names[n] = { name: n, near: [], points: {}, all: 0 });
      e.all++;
      if (Math.abs(p.gap_s) <= limit) { e.near.push(c); e.points[p.place] = (e.points[p.place] || 0) + 1; continue; }
    }
    far.push({ id: c.id, why: p ? hoursText(Math.abs(p.gap_s) / 3600) + " from a photo" : "no photo within 5 days" });
  }
  for (const u of r.unset || []) if (!keep.has(u.id)) far.push({ id: u.id, why: u.why });
  const list = Object.values(names).sort((a, b) => b.near.length - a.near.length || b.all - a.all);
  for (const e of list) {
    const gaps = e.near.map((c) => Math.abs(c.photo.gap_s) / 3600);
    e.min = gaps.length ? Math.min(...gaps) : null; e.max = gaps.length ? Math.max(...gaps) : null;
    // Where a clip given this place by choice goes: the point most of its clips got (else its first)
    const best = Object.keys(e.points).sort((a, b) => e.points[b] - e.points[a])[0] || Object.keys(places).find((k) => nameOf(k) === e.name);
    e.key = best;
  }
  const pick = S.geoPick != null && (S.geoPick === "" || list.some((e) => e.name === S.geoPick)) ? S.geoPick : (list[0] ? list[0].name : "");
  return { list, far, pick, places, keep: keep.size };
}
function geoApplyText(plan) {
  const n = plan.list.reduce((k, e) => k + e.near.length, 0) + (plan.pick ? plan.far.length : 0);
  return [n, S.geoBusy ? "Writing…" : "Apply to " + n + " clip" + (n === 1 ? "" : "s")];
}
function renderGeo() {
  const r = S.geo || {};
  const plan = geoPlan();
  let h = '<div class="scrim" data-a="scrim"><div class="sheet geosheet" role="dialog" aria-modal="true" aria-labelledby="geoTitle"><header><h2 id="geoTitle">Locations from Apple Photos</h2>' +
    '<span class="sub">' + esc(S.geoClips.length + " clips · " + (r.photos || 0) + " photos with a place around them") + '</span></header><div class="sb">';
  h += '<div class="geomax"><label for="geomax">Nearest photo at most</label><input type="range" id="geomax" min="0" max="' + (GEO_STOPS.length - 1) + '" step="1" value="' + S.geoMax + '">' +
    '<b id="geomaxv">' + esc(hoursText(GEO_STOPS[S.geoMax])) + '</b><span class="sub">from the clip</span></div><div id="geobody">' + geoBody(plan) + '</div>';
  const [n, label] = geoApplyText(plan);
  h += '</div><footer><span class="sp"></span><button type="button" class="btn" data-a="closesheet">Cancel</button>' +
    '<button type="button" class="btn pri" id="geoapply" data-a="geoapply"' + (n && !S.geoBusy ? "" : " disabled") + '>' + esc(label) + '</button></footer></div></div>';
  return h;
}
// The slider moved: what's under it, and the button (the slider itself stays, so dragging goes on)
function geoSlid() {
  const plan = geoPlan();
  $("geomaxv").textContent = hoursText(GEO_STOPS[S.geoMax]);
  $("geobody").innerHTML = geoBody(plan);
  const [n, label] = geoApplyText(plan);
  const b = $("geoapply"); b.textContent = label; b.disabled = !n || S.geoBusy;
}
function geoBody(plan) {
  const r = S.geo || {};
  const nearN = plan.list.reduce((n, e) => n + e.near.length, 0);
  const whys = Array.from(new Set((r.groups || []).map((g) => g.why + (g.local ? " (taken as the place’s " + zoneText(-g.default_shift) + ")" : ""))));
  let h = '<div class="lab">Places found · ' + nearN + ' clip' + (nearN === 1 ? "" : "s") + ' get their nearest photo’s</div><div class="geolist" role="radiogroup" aria-label="Place for the clips too far">';
  for (const e of plan.list) {
    const range = e.near.length ? (e.min === e.max ? hoursText(e.min) : hoursText(e.min) + "–" + hoursText(e.max)) + " from their photos" : "none within " + hoursText(GEO_STOPS[S.geoMax]);
    const pt = plan.places[e.key] || {};
    h += '<label class="geoname"><input type="radio" name="geopick" data-a="geopick" value="' + esc(e.name) + '"' + (plan.pick === e.name ? " checked" : "") + '>' +
      '<span class="wrap"><b>' + esc(e.name) + '</b><br><span class="sub" style="display:inline">' + esc(e.near.length + " clip" + (e.near.length === 1 ? "" : "s") + " · " + range +
      (Object.keys(e.points).length > 1 ? " · " + Object.keys(e.points).length + " spots" : "")) + '</span></span>' +
      (pt.lat != null ? '<button type="button" class="btn sm" data-a="geomap" data-v="' + pt.lat + "," + pt.lon + '">Map</button>' : "") + '</label>';
  }
  h += '<label class="geoname"><input type="radio" name="geopick" data-a="geopick" value=""' + (plan.pick === "" ? " checked" : "") + '><span class="wrap">None: leave them without a location</span></label></div>';
  h += '<div class="lab">Too far · ' + plan.far.length + ' clip' + (plan.far.length === 1 ? "" : "s") + (plan.far.length ? ' get the place ticked above' : "") + '</div>';
  if (plan.far.length) h += '<div class="geofar">' + plan.far.map((f) => '<div><span class="mono">' + esc(f.id.split("/").pop()) + '</span> <span class="sub" style="display:inline">' + esc(f.why) + '</span></div>').join("") + '</div>';
  if (S.geoHad.length) h += '<label class="switch"><input type="checkbox" data-a="geokeep"' + (S.geoKeep ? " checked" : "") + '>Keep the ' + S.geoHad.length + ' location' + (S.geoHad.length === 1 ? "" : "s") + ' already set</label>';
  const notes = [];
  if ((r.own_gps || []).length) notes.push(r.own_gps.length + " with the camera’s own GPS left as they are");
  if (whys.length) notes.push("Clip times: " + whys.join("; "));
  if (notes.length) h += '<p class="sub wrap">' + esc(notes.join(". ") + ".") + '</p>';
  return h;
}
// Locations set (or taken off) for clips: shown at once, written after (writes.js). `more`: a point typed whose
// place's name is still to be looked up
function geoWrite(entries, more) {
  const ids = new Set(entries.flatMap((e) => e.ids));
  const clips = ((DATA.footage && DATA.footage.items) || []).filter((it) => ids.has(it.id));
  return Writes.run("location", clips, Object.assign({ entries, set: new Date().toISOString().replace(/\.\d+Z$/, "Z") }, more || {}));
}
async function geoApply() {
  const plan = geoPlan();
  const entries = [];
  for (const e of plan.list) for (const c of e.near) {
    const p = c.photo, pl = plan.places[p.place] || {};
    entries.push({ ids: [c.id], lat: p.lat, lon: p.lon, source: "photos", place: pl.place || null, photo_gap_s: p.gap_s });
  }
  const chosen = plan.pick ? plan.list.find((e) => e.name === plan.pick) : null;
  const pt = chosen && plan.places[chosen.key];
  if (pt && plan.far.length) entries.push({ ids: plan.far.map((f) => f.id), lat: pt.lat, lon: pt.lon, source: "manual", place: pt.place || null });
  if (!entries.length) return;
  S.sheet = null; renderSheet();
  const done = await geoWrite(entries);
  if (done.ok) toast(entries.reduce((n, x) => n + x.ids.length, 0) + " clips placed");
}
async function geoAct(a, v, el) {
  switch (a) {
    case "geosuggest": return geoSuggest();
    case "geoallow": {
      S.sheet = null; renderSheet();
      const st = await invoke("photos_access", { ask: true }).catch(() => "none");
      if (st === "authorized" || st === "limited") return geoSuggest();
      S.geoStatus = st; S.sheet = "geodenied"; renderSheet(); return;
    }
    case "geomap": { const [lat, lon] = v.split(",").map(Number); call("open_map", { lat, lon }).catch(() => {}); return; }
    case "geoapply": return geoApply();
    case "geohand": {
      const ids = ctx().selItems.filter((it) => it.kind === "clip").map((it) => it.id);
      if (!ids.length) return;
      // "27.7333, 34.25" is set at once, its place's name looked up as it's written; anything else (degrees and
      // minutes) is read by the app first
      const text = S.geoHand, site = S.geoSite;
      const m = /^\s*(-?\d+(?:\.\d+)?)\s*,\s*(-?\d+(?:\.\d+)?)\s*$/.exec(text);
      let p = m && Math.abs(+m[1]) <= 90 && Math.abs(+m[2]) <= 180 ? { lat: +m[1], lon: +m[2] } : null;
      const typed = !!p;
      if (!p) { try { p = await invoke("parse_location", { text }); } catch (e) { showError(String(e)); return; } }
      const done = await geoWrite([{ ids, lat: p.lat, lon: p.lon, source: "manual", place: p.place, site }], typed ? { lookup: text } : null);
      if (done.ok) toast((done.args.placeText || p.place_text || coordText(p.lat, p.lon)) + ": set for " + ids.length + " clip" + (ids.length === 1 ? "" : "s"));
      return;
    }
    case "georemove": {
      const ids = ctx().selItems.filter((it) => it.kind === "clip" && it.geo && it.geo.source !== "camera").map((it) => it.id);
      if (ids.length) geoWrite([{ ids, remove: true }]);
      return;
    }
  }
}

// Connect, and when the NAS has other names (Project settings: The NAS's other names), ▾ with a list of them: Connect
// tries each in turn, a name from the list only that one
function connectButton(a, v, busy, names) {
  const key = a + "|" + v;
  let h = '<span class="split"><button type="button" class="btn sm" data-a="' + a + '" data-v="' + esc(v) + '"' + (busy ? " disabled" : "") + '>' + (busy ? "Connecting…" : "Connect") + '</button>';
  if ((names || []).length > 1 && !busy) {
    h += '<button type="button" class="btn sm caret" data-a="connectvia" data-v="' + esc(key) + '" aria-haspopup="menu" aria-expanded="' + (S.via === key) + '" aria-label="Connect by another name">' + ICON.down + '</button>';
    if (S.via === key) h += '<span class="cvia" role="menu"><span class="sub">Connect by</span>' + names.map((n) => '<button type="button" role="menuitem" data-a="' + a + '" data-v="' + esc(v) + '" data-h="' + esc(n) + '">' + esc(n) + '</button>').join("") + '</span>';
  }
  return h + '</span>';
}

// An offline location's share connected (library/src/main.rs connect_location): its clips show once it's there
async function connectLocation(path, host) {
  if (S.connecting[path]) return;
  S.connecting[path] = true;
  toast("Connecting " + path + (host ? " by " + host : "") + "…");
  renderAll();
  try { await invoke("connect_location", { path, host: host || null }); toast("Connected " + path + ": its clips show in a moment"); }
  catch (e) { showError(String(e)); }
  delete S.connecting[path];
  renderAll();
}
