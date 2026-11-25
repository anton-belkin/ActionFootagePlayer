"use strict";
// Talking to the app (errors, playing), what an item is, the filters, the tree.
// One of the window's scripts that were app.js: index.html loads them in this order before app.js, as classic
// scripts sharing their top-level names, so each uses the others' when called (only earlier ones at load).

// ---------- talking to the app ----------

let errTimer = null;
function showError(text) {
  const el = $("error");
  el.textContent = text || "";
  el.hidden = !text;
  clearTimeout(errTimer);
  if (text) errTimer = setTimeout(() => { el.hidden = true; }, 9000);
}
async function call(cmd, args) {
  try { return await invoke(cmd, args); } catch (e) { showError(String(e)); throw e; }
}
let toastTimer = null;
function toast(t) { $("toastMsg").textContent = t; $("toast").hidden = false; clearTimeout(toastTimer); toastTimer = setTimeout(() => { $("toast").hidden = true; }, 4200); }

// The player's playlist when items are opened (Anton, 2026-11-19): what the window shows at that moment, in its order
// (its search, filters, sort), the item opened playing in it, so the player's Previous and Next walk it. A snapshot:
// a later change here doesn't follow it; another open replaces it. Several selected and opened: those, in the
// window's order. Photos are in it (the player shows them as a phone does: a Live Photo's or Motion Photo's live part
// under its Live icon, a photo's link as its clip at the frame). What the player can't play is left out: folders,
// marks (a time in a clip: opened, a mark plays its clip), clips offline and links whose original isn't found.
function playable(it) {
  if (!it || !(it.kind === "clip" || it.kind === "moment" || it.kind === "export" || it.kind === "still") || it.offline || !it.path) return false;
  if (MISSING.has(it.path)) return false;
  return !(it.kind === "moment" && "original" in it && !it.original && it.link && it.link.clip);
}
// What the window shows, as words: the player's playlist says it ("Originals › Day 1 · “turtle” · filtered")
function shownLabel() {
  const parts = [ctx().viewName].concat(S.view === "clip" ? [] : S.path.map((p) => p.replace(AV_MARK, "")));
  let t = parts.filter(Boolean).join(" › ");
  if (S.q.trim()) t += " · “" + S.q.trim() + "”";
  if (filtering()) t += " · filtered";
  return t;
}
// The list for the player: {paths, titles, at, label}, or null when the item opened isn't one it plays (it opens by
// itself then, as before)
function playList(opened) {
  const byKey = new Map(items().map((it) => [keyOf(it), it]));
  const order = new Map(VISIBLE.map((k, i) => [k, i]));
  let list;
  if (opened.length === 1) {
    if (!playable(opened[0])) return null;
    list = VISIBLE.map((k) => byKey.get(k)).filter(playable);
    // Not among the rows (a view drawn otherwise): it alone
    if (!list.includes(opened[0])) list = [opened[0]];
  } else {
    list = opened.filter(playable);
    const at = (it) => (order.has(keyOf(it)) ? order.get(keyOf(it)) : Infinity);
    list = list.map((it, i) => [it, i]).sort((a, b) => at(a[0]) - at(b[0]) || a[1] - b[1]).map((x) => x[0]);
    if (!list.length) return null;
  }
  return { paths: list.map((it) => it.path), titles: list.map((it) => (it.name || "").replace(/\.(mp4|mov|insv|heic|jpe?g|png)$/i, "")), at: opened.length === 1 ? list.indexOf(opened[0]) : 0, label: shownLabel() };
}

// What's selected, opened (open_items): until the player plays the first video (seconds for a clip on a NAS, with
// nothing to see meanwhile on Windows before), a spinner says so; a quick one shows none. With `list` (playList):
// the items as that list (photos too: a photo not in it, e.g. one of several selected outside the rows, by itself)
let opening = 0;
async function openItems(paths, list) {
  if (!paths.length) return;
  // Clips whose location is offline: their share connected instead (opened again once they show)
  const off = (RAW_FOOTAGE.items || []).filter((it) => it.offline && paths.includes(it.path));
  if (off.length) {
    const locs = S.locations.filter((l) => l.offline && off.some((it) => it.loc === l.name));
    const can = locs.filter((l) => l.connect);
    if (!can.length) { toast((off.length > 1 ? off.length + " clips are" : off[0].name + " is") + " offline: connect " + (locs.map((l) => l.path).join(", ") || "its location") + " first"); return; }
    for (const l of can) connectLocation(l.path);
    return;
  }
  const id = ++opening;
  const name = ((list ? list.paths[list.at] : paths[0]).split(/[\\/]/).pop() || "").replace(/\.afplink\.mp4$/i, "");
  const busy = setTimeout(() => {
    if (opening !== id) return;
    clearTimeout(toastTimer);
    $("toastMsg").innerHTML = '<span class="spin" aria-hidden="true"></span>';
    $("toastMsg").append("Opening " + (paths.length > 1 ? paths.length + " items" : name) + " in the player…");
    $("toast").hidden = false;
    document.body.classList.add("waiting");
  }, 300);
  let ok = true;
  try {
    if (list) {
      // Photos among what's opened that the list hasn't (none, usually): by themselves; the rest, the list
      const listed = new Set(list.paths), photos = paths.filter((p) => !listed.has(p) && /\.(heic|jpe?g|png)$/i.test(p));
      if (photos.length) await call("open_items", { paths: photos });
      ok = await call("open_list", { paths: list.paths, titles: list.titles, at: list.at, label: list.label });
    } else ok = await call("open_items", { paths });
  } catch (e) { ok = null; }
  clearTimeout(busy);
  if (opening !== id) return;
  document.body.classList.remove("waiting");
  if (ok === false) toast("The player hasn’t opened " + name + " yet: is it still working on another clip?");
  else $("toast").hidden = true;
}

// ---------- what an item is ----------

function clock(s) {
  if (!(s >= 0)) return "";
  s = Math.round(s);
  const h = Math.floor(s / 3600), m = Math.floor((s % 3600) / 60), x = s % 60;
  return (h ? h + ":" + String(m).padStart(2, "0") : m) + ":" + String(x).padStart(2, "0");
}
const MONTHS = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];
function dateText(d) {
  if (!d || isNaN(d)) return "";
  return d.getDate() + " " + MONTHS[d.getMonth()] + " " + d.getFullYear() + " " + String(d.getHours()).padStart(2, "0") + ":" + String(d.getMinutes()).padStart(2, "0");
}
// When a clip was recorded: a DJI file's name is its local time; else the file's creation time (UTC)
function recorded(name, created) {
  const m = /DJI_(\d{4})(\d{2})(\d{2})(\d{2})(\d{2})(\d{2})/.exec(name || "");
  if (m) return new Date(+m[1], +m[2] - 1, +m[3], +m[4], +m[5], +m[6]);
  return created ? new Date(created * 1000) : null;
}
function prettyCamera(c) {
  if (!c) return null;
  const s = String(c).trim();
  const i = s.indexOf(" ");
  const brand = i > 0 ? s.slice(0, i) : s;
  let model = i > 0 ? s.slice(i + 1) : "";
  model = model.replace(/([a-z])([A-Z0-9])/g, "$1 $2").replace(/([A-Za-z])(\d)/g, "$1 $2").trim();
  return { brand, model: model || brand };
}
const COLORS = { dlogm: ["Log", "D-Log M"], dlog: ["Log", "D-Log"], dlog2: ["Log", "D-Log 2"], hlg: ["HDR", "HLG"], pq: ["HDR", "PQ"],
  normal: [null, "Normal"], "d-cinelike": [null, "D-Cinelike"], other: [null, "Other"] };
const TF_DEFAULT = "As the defaults";

function deriveSource(it, probe) {
  it.probed = !!probe;
  const cam = prettyCamera(probe && probe.camera);
  it.brand = cam ? cam.brand : null;
  it.model = cam ? cam.model : null;
  it.cam = cam ? (cam.brand + " " + cam.model) : "";
  const col = probe && probe.color ? (COLORS[probe.color] || [null, probe.color]) : null;
  it.colorGroup = col ? col[0] : null;
  it.color = col ? col[1] : null;
  it.motion = probe ? probe.mode != null : null;
  // Its motion measured from the picture (<clip>.afp-motion beside it, gyroflow-probe's "motion": "optical")
  it.measured = !!(probe && probe.motion === "optical");
  // Measured with the rolling shutter left out though its lens has a readout (gyroflow-probe's readout_left_out, ms)
  it.readoutLeftOut = it.measured && probe.measured && probe.measured.readout_left_out || null;
  // What the camera stabilized itself (gyroflow-probe's stabilized_in_camera: "HyperSmooth (HS EIS)", "EIS (status 4)",
  // "EIS: RockSteady", "FlowState"); null: off, or not said. With motion data (a GoPro HERO12+ with HyperSmooth on), the
  // player stabilizes on top of it
  it.camStab = probe && typeof probe.stabilized_in_camera === "string" ? probe.stabilized_in_camera : null;
  it.is360 = !!(probe && (probe.spherical || probe.rig360));
}
function deriveTransform(it, view) {
  if (!view || typeof view !== "object" || Array.isArray(view)) {
    it.stab = null; it.horizon = null; it.frame = null; it.proj = null;
    return;
  }
  it.stab = view.view === "off" ? "Off" : view.view === "wide" ? "Wider" : "Stabilized";
  it.horizon = view.horizon_lock === "no" ? "Free" : view.horizon_lock === "yes" ? "Level" : null;
  const aspect = view.aspect && view.aspect !== "original" ? view.aspect : null;
  const portrait = view.orientation === "portrait";
  it.frame = aspect || portrait ? (aspect || "Own ratio") + (portrait ? ", portrait" : "") : "As recorded";
  const p = view.projection || "linear";
  it.proj = p === "linear" ? "Linear" : p.charAt(0).toUpperCase() + p.slice(1);
}
function tfText(it) {
  const p = [];
  if (it.motion === false && !it.gyroflow) p.push(it.camStab ? "stabilized in the camera" : "no motion data");
  else if (it.gyroflow) p.push("Gyroflow project");
  else if (it.measured) p.push("motion from the picture");
  else if (it.stab) p.push(it.stab === "Stabilized" ? "stabilized" : it.stab === "Wider" ? "wider" : "not stabilized");
  if (it.horizon && it.stab !== "Off") p.push(it.horizon === "Level" ? "level" : "horizon free");
  if (it.frame && it.frame !== "As recorded") p.push(it.frame);
  if (it.proj && it.proj !== "Linear") p.push(it.proj.toLowerCase());
  if (it.turned) p.push("turned");
  if (it.is360) p.push("360°");
  return p.join(", ");
}

function derive(view) {
  if (view === "footage" || view === "sources") { arrange(); return; }
  deriveItems(view);
}
// (`only`: these items alone, of that view's kind: changes.js's pinpoint update)
function deriveItems(view, only) {
  const d = DATA[view === "sources" ? "footage" : view];
  if (!d) return;
  const root = S.root;
  for (const it of only || d.items) {
    if (it.kind === "clip") {
      const probe = PROBES[it.path];
      deriveSource(it, probe);
      deriveGeo(it, probe);
      const own = it.own || {};
      deriveTransform(it, own.view);
      const src = own.source && typeof own.source === "object" ? own.source : {};
      it.turned = !!(src.rotation && src.rotation !== 0);
      const ownMarks = Array.isArray(own.marks) ? own.marks : null;
      it.marks = ownMarks ? ownMarks.length : (probe && probe.marks && probe.marks.times ? probe.marks.times.length : 0);
      it.moments = MOMENTS_OF[it.id] || 0;
      const mv = probe && probe.movie;
      it.durS = mv ? mv.duration : null;
      it.when = recorded(it.name, mv && mv.created);
      // Its preview's picture, else what the camera made for its own quick look (LRF, THM), else the clip's own, as
      // Finder draws it (a log clip looks flat, as there); an empty preview (its picture still to come): the camera's,
      // else its dark frame
      it.thumbPath = (it.draft ? it.thumb : null) || it.preview || it.thumb || it.path || null;
      it.conflict = Array.isArray(it.own_conflict) && it.own_conflict.length > 1;
      it.locs = (it.copies || []).map((c) => c.loc);
      it.sub = [it.cam || (probe ? "Unknown camera" : "Reading…"), it.color, tfText(it)].filter(Boolean).join(" · ");
    } else if (it.kind === "moment") {
      const l = it.link || {};
      const original = it.original || (l.clip ? root + "/" + l.clip : null);
      deriveSource(it, original && PROBES[original]);
      deriveTransform(it, l.stab);
      it.turned = false;
      it.durS = l.in != null && l.out != null ? l.out - l.in : null;
      const start = l.created ? new Date(l.created) : null;
      it.when = start && !isNaN(start) && l.in != null ? new Date(start.getTime() + l.in * 1000) : new Date(it.mtime * 1000);
      it.thumbPath = it.path;
      const clipName = l.clip ? l.clip.split("/").pop().replace(/\.[^.]+$/, "") : "?";
      it.sub = [clipName + " " + clock(l.in) + "–" + clock(l.out), tfText(it)].filter(Boolean).join(" · ");
    } else if (it.kind === "still") {
      // A photo's time is its frame's in the clip's recording, when its link says both
      const l = it.link || {};
      const start = l.created ? new Date(l.created) : null;
      it.when = start && !isNaN(start) && l.t != null ? new Date(start.getTime() + l.t * 1000) : new Date(it.mtime * 1000);
      it.thumbPath = it.path;
      it.sub = it.linked && l.clip ? [l.clip.split("/").pop().replace(/\.[^.]+$/, "") + " at " + clock(l.t), l.crop || l.camera ? "cropped" : ""].filter(Boolean).join(" · ") : it.form;
    } else if (it.kind === "export") {
      const l = it.link || {};
      it.when = new Date(it.mtime * 1000);
      it.durS = l.in != null && l.out != null ? l.out - l.in : null;
      it.look = l.look === "nbcu" || l.look === "maker" ? "SDR" : l.look ? "HDR" : "Unknown";
      it.thumbPath = it.path;
      it.sub = (l.clip ? l.clip.split("/").pop() + " · " : "") + it.look;
    }
    // An item still being written (albums.js): the picture the page has for it meanwhile (its clip's, the file it's
    // a copy of), replaced by its own once the command says what it made
    if (it.ph) it.thumbPath = it.ph;
    it.date = it.when ? dateText(it.when) : "";
    it.dur = it.kind === "still" ? (it.form === "Live Photo" ? "Live" : it.linked ? "Link" : "") : clock(it.durS);
  }
}
// Footage and Sources from the scan: Footage, each clip in the folder its preview is in (as arranged by hand);
// Sources, each location's own folders as they are on its disk, a clip once per location that has it
function arrange() {
  if (!RAW_FOOTAGE) return;
  const label = {};
  const used = {};
  S.locations.forEach((l, i) => {
    let n = l.name || "Location " + (i + 1);
    while (used[n]) n += " ·";
    used[n] = true;
    label[i] = n;
  });
  // Albums: every link at the folder it's in, a clip once per album it's in (albums.js)
  DATA.footage = albumItems();
  const inAlbums = albumCounts();
  // Originals: each clip at its path in the project's tree (the sources' places in it are Sources settings')
  const sitems = RAW_FOOTAGE.items.map((it) => Object.assign({}, it, { key: "s:" + it.id, tdir: it.dir, dir: it.dir, albums: inAlbums[it.id] || 0 }));
  DATA.sources = { dirs: (RAW_FOOTAGE.dirs || []).slice(), items: sitems, labels: label };
  for (const v of ["footage", "sources"]) derive1(v);
}
function derive1(view) {
  const d = DATA[view];
  if (!d) return;
  const keep = DATA.footage;
  DATA.footage = d;
  deriveItems(view);
  DATA.footage = keep;
}

function countMoments() {
  MOMENTS_OF = {};
  for (const m of (DATA.moments && DATA.moments.items) || []) {
    const c = m.link && m.link.clip;
    if (c) MOMENTS_OF[c] = (MOMENTS_OF[c] || 0) + 1;
  }
}

// ---------- filters ----------

const L = (id, label, fn) => ({ id, label, fn });
const P = (id, label, kids) => ({ id, label, kids });
function distinct(items, f) { const out = []; for (const it of items) { const v = f(it); if (v != null && out.indexOf(v) < 0) out.push(v); } return out.sort(); }

function buildFacets() {
  const items = albumViewItems() || (DATA[S.view] && DATA[S.view].items) || []; // albumviews.js
  const sections = [];
  const has = (k) => S.view === "footage" && items.some((it) => it.kind === k);
  if (S.view === "footage" && MISSING.size && items.some((it) => it.kind !== "clip"))
    sections.push({ id: "lk", title: "Link", facets: [{ id: "lk", title: "Original", opts: [L("lk:broken", "Not where the link says", (it) => MISSING.has(it.path))] }] });
  if (S.view === "footage" || S.view === "sources") {
    const brands = distinct(items, (i) => i.brand);
    const camOpts = brands.map((b) => P("cam:" + b, b, distinct(items.filter((i) => i.brand === b), (i) => i.model).map((m) => L("cam:" + b + "|" + m, m, (it) => it.brand === b && it.model === m))));
    if (items.some((i) => i.probed && !i.brand)) camOpts.push(L("cam:?", "Unknown", (it) => it.probed && !it.brand));
    const colOpts = [];
    for (const g of ["Log", "HDR"]) {
      const kids = distinct(items.filter((i) => i.colorGroup === g), (i) => i.color).map((c) => L("col:" + c, c, (it) => it.color === c));
      if (kids.length) colOpts.push(P("col:" + g, g, kids));
    }
    for (const c of distinct(items.filter((i) => i.color && !i.colorGroup), (i) => i.color)) colOpts.push(L("col:" + c, c, (it) => it.color === c && !it.colorGroup));
    colOpts.push(L("col:?", "Unknown", (it) => it.probed && !it.color));
    // Every attribute of Source settings (Anton, 2026-11-24): filtered by any of them, the clips are changed there at once
    const setLabel = (list, v) => (list.find((x) => x[0] === v) || [v, v])[1];
    const curves = distinct(items, (i) => { const c = srcOf(i).curve; return c && c !== "auto" ? c : null; });
    const typeNames = {};
    items.forEach((i) => lensTypesOf(i).forEach((t) => { typeNames[t[0]] = t[1]; }));
    const chosenTypes = distinct(items, (i) => lensTypeOf(i) || null);
    sections.push({ id: "src", title: "Source", facets: [
      { id: "cam", title: "Camera", opts: camOpts },
      { id: "col", title: "Color", opts: colOpts },
      { id: "crv", title: "Log curve", opts: [L("crv:auto", "From the clip", (it) => it.probed && !curves.includes(srcOf(it).curve))].concat(curves.map((v) => L("crv:" + v, setLabel(SRC_CURVES, v) + ", set", (it) => srcOf(it).curve === v))) },
      { id: "trn", title: "Recorded turned", opts: [L("trn:n", "As is", (it) => !it.turned), L("trn:y", "Turned", (it) => it.turned)] },
      { id: "uwc", title: "Underwater color", opts: [L("uwc:on", "On", (it) => uwOk(it) && uwKey(it) !== "off"), L("uwc:off", "Off", (it) => uwOk(it) && uwKey(it) === "off"), L("uwc:na", "Not for log or HDR", (it) => it.probed && !uwOk(it))] },
      { id: "lns", title: "Lens", opts: Object.keys(LENS_FROM).map((k) => L("lns:" + (k || "none"), LENS_FROM[k], (it) => !it.is360 && lensFrom(it) === k)) },
      { id: "lnt", title: "Lens type", opts: [L("lnt:rec", "As recorded", (it) => lensTypesOf(it).length > 0 && !lensTypeOf(it))]
        .concat(chosenTypes.map((t) => L("lnt:" + t, (typeNames[t] || t) + ", set", (it) => lensTypeOf(it) === t)))
        .concat([L("lnt:no", "No lens types", (it) => !!lensOf(it) && !lensTypesOf(it).length)]) },
      { id: "uwl", title: "Under water", opts: [L("uwl:auto", "Auto", (it) => srcOf(it).underwater === "auto"), L("uwl:yes", "Yes (a flat port)", (it) => srcOf(it).underwater === "yes"),
        L("uwl:no", "No", (it) => srcOf(it).underwater === "no"), L("uwl:unset", "Not set", (it) => it.probed && !srcOf(it).underwater)] },
      { id: "mot", title: "Motion data", opts: ["gyro", "pic", "picro", "project", "cam", "none"].map((k) => L("mot:" + k, MOTION_TEXT[k], (it) => motionKey(it) === k)) },
      { id: "shp", title: "Shape", opts: [L("shp:flat", "Flat", (it) => it.probed && !it.is360), L("shp:360", "360°", (it) => it.is360)] },
    ] });
    const frames = distinct(items.filter((i) => i.frame && i.frame !== "As recorded"), (i) => i.frame);
    const projs = distinct(items.filter((i) => i.proj && i.proj !== "Linear"), (i) => i.proj);
    sections.push({ id: "tf", title: "Transform", facets: [
      // (a Gyroflow project is Source's Motion data)
      { id: "stab", title: "Stabilization", opts: [L("stab:on", "Stabilized", (it) => it.stab === "Stabilized"), L("stab:w", "Wider", (it) => it.stab === "Wider"),
        L("stab:off", "Off", (it) => it.stab === "Off"), L("stab:def", TF_DEFAULT, (it) => !it.stab)] },
      { id: "hor", title: "Horizon", opts: [L("hor:l", "Level", (it) => it.horizon === "Level"), L("hor:f", "Free", (it) => it.horizon === "Free")] },
      { id: "frm", title: "Frame", opts: [L("frm:own", "As recorded", (it) => it.frame === "As recorded")].concat(frames.length ? [P("frm:ch", "Changed", frames.map((f) => L("frm:" + f, f, (it) => it.frame === f)))] : []) },
      { id: "prj", title: "Projection", opts: [L("prj:lin", "Linear", (it) => it.proj === "Linear")].concat(projs.map((p) => L("prj:" + p, p, (it) => it.proj === p))) },
    ] });
  }
  if (S.view === "footage" || S.view === "sources") {
    sections.push({ id: "mk", title: "Marks & albums", facets: [
      { id: "mrk", title: "Marks", opts: [L("mrk:y", "Has marks", (it) => it.marks > 0), L("mrk:n", "No marks", (it) => !(it.marks > 0))] },
      { id: "mom", title: "Moments", opts: [L("mom:y", "Has moments", (it) => it.moments > 0), L("mom:todo", "Marks without moments", (it) => it.marks > 0 && !it.moments)] },
    ].concat(S.view === "sources" ? [{ id: "prv", title: "In an album", opts: [L("prv:y", "Yes", (it) => it.albums > 0), L("prv:n", "No", (it) => !it.albums)] }] : []) });
    if (S.view === "footage") albumFacets(items, sections); // albums.js: the filter by kind
  }
  if (S.view === "footage" || S.view === "sources") {
    const names = distinct(items, (i) => i.loc);
    const wh = [];
    if (names.length > 1) wh.push({ id: "loc", title: "Plays from", opts: names.map((n) => L("loc:" + n, n, (it) => it.loc === n)) });
    wh.push({ id: "where2", title: "Copies", opts: [L("cp:2", "In two locations or more", (it) => (it.locs || []).length > 1)] });
    wh.push({ id: "iss", title: "Issues", opts: [L("iss:own", "Two own files", (it) => it.conflict)] });
    sections.push({ id: "wh", title: "Where", facets: wh });
    geoFacets(items, sections);
  }
  const tagList = [];
  for (const it of items) for (const t of it.tags || []) if (tagList.indexOf(t) < 0) tagList.push(t);
  const tagFacets = [];
  if (tagList.length) tagFacets.push({ id: "tag", title: "Tag", opts: tagList.sort().map((t) => L("tag:" + t, t, (it) => (it.tags || []).indexOf(t) >= 0)) });
  // Keep it? and stars (originals, moments, photos: not exports); Hide rejected is culling.js's switch, kept across the tabs
  const cullable = items.some((it) => it.kind !== "export");
  if (cullable) tagFacets.unshift({ id: "pick", title: "Keep it?", opts: [L("pick:a", "Accepted", (it) => it.pick === "accept"), L("pick:r", "Rejected", (it) => it.pick === "reject"), L("pick:n", "Undecided", (it) => !it.pick)] });
  if (cullable && items.some((it) => it.rating)) tagFacets.push({ id: "stars", title: "Stars", opts: [5, 4, 3, 2, 1].map((n) => L("stars:" + n, n === 5 ? "★★★★★" : stars(n) + " or more", (it) => (it.rating || 0) >= n))
    .concat([L("stars:0", "No stars", (it) => !it.rating)]) });
  if (tagFacets.length) sections.push({ id: "tg", title: "Tags", facets: tagFacets });
  // What's rendered (the render cache: renders.rs): moments in HDR and SDR, photo links as photos
  const photoLinks = items.some((i) => i.kind === "still" && i.linked);
  if (has("moment") || photoLinks) {
    const rf = (p, st) => (it) => (it.renders || {})[p] === st;
    sections.push({ id: "rn", title: "Renders", facets: (has("moment") ? ["HDR", "SDR"] : []).concat(photoLinks ? ["Photo"] : []).map((p) => ({ id: "rn" + p, title: p, opts: [
      L("rn:" + p + ":c", "Current", rf(p, "current")), L("rn:" + p + ":s", "Stale", rf(p, "stale")), L("rn:" + p + ":n", "None", (it) => (p === "Photo" ? it.kind === "still" && it.linked : it.kind === "moment") && !(it.renders || {})[p])] })) });
  }
  if (has("still")) {
    sections.push({ id: "frm2", title: "Photos", facets: [{ id: "form", title: "Photo", opts: distinct(items.filter((i) => i.kind === "still"), (i) => i.form).map((f) => L("form:" + f, f, (it) => it.form === f)) }] });
  }
  if (has("export")) {
    sections.push({ id: "elk", title: "Exports", facets: [{ id: "look", title: "Export’s look", opts: distinct(items.filter((i) => i.kind === "export"), (i) => i.look).map((f) => L("look:" + f, f, (it) => it.look === f)) }] });
  }
  FACETS = sections;
}
function allFacets() { const a = []; FACETS.forEach((x) => x.facets.forEach((f) => a.push(f))); return a; }
function facetLeaves(f) { const out = []; f.opts.forEach((o) => (o.kids || [o]).forEach((l) => out.push(l))); return out; }
function findOpt(id) { for (const f of allFacets()) for (const o of f.opts) { if (o.id === id) return o; if (o.kids) for (const k of o.kids) if (k.id === id) return k; } return null; }
function facetMatch(f, it) { const on = facetLeaves(f).filter((l) => S.filters[l.id]); return !on.length || on.some((l) => l.fn(it)); }
// The search: each word typed is somewhere in the item's name, camera, colour, folder, date, tags or comment (all of
// them must be, in any order, as parts of words too), without case, ё as е
const searchFold = (t) => String(t).toLowerCase().replace(/ё/g, "е");
function textMatch(it) {
  const words = searchFold(S.q).split(/\s+/).filter(Boolean);
  if (!words.length) return true;
  const hay = searchFold([it.name, it.cam, it.color, it.sub, it.dir, it.date, it.comment].concat(it.tags || []).join("\n"));
  return words.every((w) => hay.indexOf(w) >= 0);
}
function matches(it, except) { return cullShown(it) && textMatch(it) && allFacets().every((f) => f === except || facetMatch(f, it)); }
function activeTokens() { const t = []; allFacets().forEach((f) => { const on = facetLeaves(f).filter((l) => S.filters[l.id]); if (on.length) t.push({ f, labels: on.map((l) => l.label) }); }); return t; }
function nActive() { return Object.keys(S.filters).filter((k) => S.filters[k] && findOpt(k)).length; }
function searching() { return S.q.trim() !== "" || S.flat; }
function filtering() { return nActive() > 0 || cullHiding(); }
function setOpt(id, on) { const o = findOpt(id); if (!o) return; (o.kids || [o]).forEach((k) => { if (on) S.filters[k.id] = true; else delete S.filters[k.id]; }); }

// ---------- the tree ----------

function dirKey() { return albumMineOpen() ? "" : S.path.join("/"); } // albumviews.js: My albums is the albums' top
function items() { return albumViewItems() || (DATA[S.view] && DATA[S.view].items) || []; } // albumviews.js: a view inside Albums has its own
function under(path) { const v = albumViewOpen(); return v && !(v[4] && v[4].folders) ? items() : path ? items().filter((it) => it.dir === path || it.dir.startsWith(path + "/")) : items(); }
function childDirs(dir) {
  const ds = (DATA[S.view] && DATA[S.view].dirs) || [];
  return ds.filter((d) => (d.lastIndexOf("/") < 0 ? "" : d.slice(0, d.lastIndexOf("/"))) === dir);
}
function scopeItems() { return S.scope === "all" || !S.path.length ? items() : under(dirKey()); }
function plural(n) { const w = NOUN[S.view]; return n + " " + (n === 1 ? w[0] : w[1]); }
// A count of items by what they are: "3 moments" when they're one kind, else "5 items"
const KIND_NOUN = { clip: ["clip", "clips"], moment: ["moment", "moments"], still: ["photo", "photos"], export: ["export", "exports"], mark: ["mark", "marks"] };
function countText(list) {
  const w = (list.length && list.every((x) => x.kind === list[0].kind) && KIND_NOUN[list[0].kind]) || (S.view === "sources" ? KIND_NOUN.clip : ["item", "items"]);
  return list.length + " " + (list.length === 1 ? w[0] : w[1]);
}

function keyOf(it) { return it.kind === "folder" ? "f:" + it.path : "i:" + (it.key || it.id); }
function ctx() {
  const viewName = S.view === "clip" ? clipName() : VIEWS.find((v) => v[0] === S.view)[1];
  const folderName = S.path.length ? S.path[S.path.length - 1] : viewName;
  const selFolders = Object.keys(S.sel).filter((k) => k.startsWith("f:")).map((k) => k.slice(2));
  const direct = items().filter((it) => S.sel["i:" + (it.key || it.id)]);
  const seen = {}, selItems = [];
  direct.concat(...selFolders.map((p) => under(p).filter((it) => matches(it)))).forEach((it) => { const k = it.key || it.id; if (!seen[k]) { seen[k] = 1; selItems.push(it); } });
  const any = selFolders.length + direct.length > 0;
  const fnames = selFolders.map((p) => p.split("/").pop());
  let selName = "";
  if (any) {
    const parts = [];
    if (selFolders.length) parts.push(selFolders.length === 1 ? fnames[0] : selFolders.length + " folders");
    if (direct.length) parts.push(countText(direct));
    selName = parts.join(" and ");
  }
  // What a batch is kept to: the items, and the folders whole (the player takes what's under a folder) unless filtered
  const filt = filtering() || S.q.trim() !== "";
  // Footage's and Sources' folders aren't the project's tree: their clips, by their paths in it
  const clipsView = S.view === "footage" || S.view === "sources";
  // (an album holds moments and photos too: the clips among them; the moments are `moments`, for what's done to moments)
  const moments = selItems.filter((it) => it.kind === "moment");
  const only = clipsView ? Array.from(new Set(selItems.filter((it) => it.kind === "clip").map((it) => it.id))) : S.view === "clip" ? selItems.map((it) => KIND_BASE[it.kind] + it.id) : filt ? selItems.map((it) => BASE[S.view] + it.id) : otherExpanded(selFolders).map((p) => diskRel(p)).concat(direct.map((it) => BASE[S.view] + it.id));
  return { viewName, folderName, selFolders, direct, selItems, any, selName, only, moments, label: countText(selItems) };
}
