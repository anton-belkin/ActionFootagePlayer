"use strict";
// The Library's window (docs/library-management.md, the mockup it was settled on): a project's footage, moments,
// photos and exports browsed as folders; filters that narrow the folder you're in, or a search that lists every match;
// the commands on what's selected, in one side panel; the queue as its tab. What it shows comes from the app
// (src/main.rs, src/index.rs): the project's files, and what each clip is (gyroflow-probe --library).
const invoke = window.__TAURI__.core.invoke;
const $ = (id) => document.getElementById(id);
const esc = (t) => String(t == null ? "" : t).replace(/[&<>"]/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;" }[c]));

const ICON = {
  folder: '<svg width="22" height="22" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" aria-hidden="true"><path d="M3 7h6l2 2h10v10H3z"/></svg>',
  fsm: '<svg width="11" height="11" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.6" aria-hidden="true"><path d="M3 7h6l2 2h10v10H3z"/></svg>',
  chev: '<svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.4" aria-hidden="true"><path d="M9 6l6 6-6 6"/></svg>',
  down: '<svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" aria-hidden="true"><path d="M6 9l6 6 6-6"/></svg>',
  x: '<svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.4" aria-hidden="true"><path d="M6 6l12 12M18 6L6 18"/></svg>',
  check: '<svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.6" aria-hidden="true"><path d="M5 12l5 5L19 7"/></svg>',
  search: '<svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" aria-hidden="true"><circle cx="11" cy="11" r="7"/><path d="M20 20l-3.5-3.5"/></svg>',
  filter: '<svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" aria-hidden="true"><path d="M4 5h16l-6 8v6l-4-2v-4z"/></svg>',
  list: '<svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" aria-hidden="true"><path d="M8 6h13M8 12h13M8 18h13M3 6h.01M3 12h.01M3 18h.01"/></svg>',
  grid: '<svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" aria-hidden="true"><rect x="3" y="3" width="7" height="7"/><rect x="14" y="3" width="7" height="7"/><rect x="3" y="14" width="7" height="7"/><rect x="14" y="14" width="7" height="7"/></svg>',
};
const VIEWS = [["sources", "Sources"], ["footage", "Footage"], ["moments", "Moments"], ["stills", "Photos"], ["exports", "Exports"]];
// Footage is the clips as arranged in _Clips (their previews' folders); the ones without a preview are in this one
const NOPREV = "Without a clip preview";
// Where each view is in the project, for the paths a batch is kept to
const BASE = { footage: "", sources: "", moments: "_Moments/", stills: "_Stills/", exports: "_Exports/", clip: "" };
const KIND_BASE = { moment: "_Moments/", still: "_Stills/", export: "_Exports/" };
const NOUN = { footage: ["clip", "clips"], sources: ["clip", "clips"], moments: ["moment", "moments"], stills: ["photo", "photos"], exports: ["export", "exports"], clip: ["item", "items"] };
// Where each view's folders are, for renaming them: Footage's are _Clips'
const ICON_MARK = '<svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M5 21V4"/><path d="M5 4h11l-2 4 2 4H5"/></svg>';
const FOLDER_BASE = { footage: "_Clips", moments: "_Moments", stills: "_Stills", exports: "_Exports" };

const S = {
  root: null, name: "", info: null, macos: true,
  view: "footage", path: [], sel: {}, anchor: null, layout: "list", q: "", flat: false, scope: "here",
  filters: {}, filtersOpen: false, fsec: { src: true, tf: true }, fexp: {}, suggOpen: false, sugg: -1,
  tab: "commands", open: { moments: true, export: true }, menu: false, recent: [],
  photoKind: "live", exportSdr: false, jobs: [], finished: 0,
  locations: [], sheet: null, stab: "loc", settings: null, locEdit: [], localEdit: [], hostsText: "",
  checking: false, checked: null, finding: false, found: null,
  take: null, takePlace: "", taking: false, tagDraft: "",
  profiles: { HDR: true, SDR: false },
  scanning: {}, cachedViews: {}, renaming: null,
  hist: [], histAt: -1,
};
let RAW_FOOTAGE = null; // the footage view as scanned: one item per clip, at its path in the tree
const DATA = {};          // view -> { dirs: [path], items: [item] }
const PROBES = {};        // a clip's path -> gyroflow-probe --library's answer
let probesVersion = 0, probesPending = 0;
let MOMENTS_OF = {};      // a clip's path in the project -> how many moments it has
const THUMBS = new Map(); // a file -> data: URL, or null for none
let VISIBLE = [];

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
  if (it.motion === false && !it.gyroflow) p.push("no motion data");
  else if (it.gyroflow) p.push("Gyroflow project");
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
function deriveItems(view) {
  const d = DATA[view === "sources" ? "footage" : view];
  if (!d) return;
  const root = S.root;
  for (const it of d.items) {
    if (it.kind === "clip") {
      const probe = PROBES[it.path];
      deriveSource(it, probe);
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
      it.thumbPath = it.preview || null;
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
      it.when = new Date(it.mtime * 1000);
      it.thumbPath = it.path;
      it.sub = it.form;
    } else if (it.kind === "export") {
      const l = it.link || {};
      it.when = new Date(it.mtime * 1000);
      it.durS = l.in != null && l.out != null ? l.out - l.in : null;
      it.look = l.look === "nbcu" || l.look === "maker" ? "SDR" : l.look ? "HDR" : "Unknown";
      it.thumbPath = it.path;
      it.sub = (l.clip ? l.clip.split("/").pop() + " · " : "") + it.look;
    }
    it.date = it.when ? dateText(it.when) : "";
    it.dur = it.kind === "still" ? (it.form === "Live Photo" ? "Live" : "") : clock(it.durS);
  }
}
// Footage and Sources from the scan: Footage, each clip in the folder its preview is in (_Clips, as arranged by hand);
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
  const fitems = RAW_FOOTAGE.items.map((it) => Object.assign({}, it, { key: it.id, tdir: it.dir, dir: it.pdir == null ? NOPREV : it.pdir }));
  const fdirs = (RAW_FOOTAGE.clips_dirs || []).slice();
  if (fitems.some((it) => it.dir === NOPREV)) fdirs.push(NOPREV);
  DATA.footage = { dirs: fdirs, items: fitems };
  const sitems = [];
  for (const it of RAW_FOOTAGE.items) {
    for (const c of it.copies || []) {
      const rel = c.rel || it.name;
      const i = rel.lastIndexOf("/");
      const dir = label[c.li] + (i > 0 ? "/" + rel.slice(0, i) : "");
      sitems.push(Object.assign({}, it, { key: "s:" + c.li + "/" + rel, tdir: it.dir, dir, name: rel.slice(i + 1), path: c.path, srcLoc: c.loc }));
    }
  }
  const sdirs = [];
  S.locations.forEach((l, i) => { sdirs.push(label[i]); for (const d of l.dirs || []) sdirs.push(label[i] + "/" + d); });
  DATA.sources = { dirs: sdirs, items: sitems, labels: label };
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

let FACETS = [];
function buildFacets() {
  const items = (DATA[S.view] && DATA[S.view].items) || [];
  const sections = [];
  if (S.view === "footage" || S.view === "sources" || S.view === "moments") {
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
    sections.push({ id: "src", title: "Source", facets: [
      { id: "cam", title: "Camera", opts: camOpts },
      { id: "col", title: "Color", opts: colOpts },
      { id: "mot", title: "Motion data", opts: [L("mot:y", "Gyro in the file", (it) => it.motion === true), L("mot:n", "None", (it) => it.motion === false)] },
      { id: "shp", title: "Shape", opts: [L("shp:flat", "Flat", (it) => it.probed && !it.is360), L("shp:360", "360°", (it) => it.is360)] },
    ] });
    const frames = distinct(items.filter((i) => i.frame && i.frame !== "As recorded"), (i) => i.frame);
    const projs = distinct(items.filter((i) => i.proj && i.proj !== "Linear"), (i) => i.proj);
    sections.push({ id: "tf", title: "Transform", facets: [
      { id: "stab", title: "Stabilization", opts: [L("stab:gf", "By a Gyroflow project", (it) => !!it.gyroflow),
        L("stab:on", "Stabilized", (it) => !it.gyroflow && it.stab === "Stabilized"), L("stab:w", "Wider", (it) => !it.gyroflow && it.stab === "Wider"),
        L("stab:off", "Off", (it) => !it.gyroflow && it.stab === "Off"), L("stab:def", TF_DEFAULT, (it) => !it.gyroflow && !it.stab)] },
      { id: "hor", title: "Horizon", opts: [L("hor:l", "Level", (it) => it.horizon === "Level"), L("hor:f", "Free", (it) => it.horizon === "Free")] },
      { id: "frm", title: "Frame", opts: [L("frm:own", "As recorded", (it) => it.frame === "As recorded")].concat(frames.length ? [P("frm:ch", "Changed", frames.map((f) => L("frm:" + f, f, (it) => it.frame === f)))] : []) },
      { id: "prj", title: "Projection", opts: [L("prj:lin", "Linear", (it) => it.proj === "Linear")].concat(projs.map((p) => L("prj:" + p, p, (it) => it.proj === p))) },
    ].concat(S.view === "footage" || S.view === "sources" ? [{ id: "trn", title: "Recorded turned", opts: [L("trn:y", "Turned", (it) => it.turned), L("trn:n", "As is", (it) => !it.turned)] }] : []) });
  }
  if (S.view === "footage" || S.view === "sources") {
    sections.push({ id: "mk", title: "Marks & previews", facets: [
      { id: "mrk", title: "Marks", opts: [L("mrk:y", "Has marks", (it) => it.marks > 0), L("mrk:n", "No marks", (it) => !(it.marks > 0))] },
      { id: "mom", title: "Moments", opts: [L("mom:y", "Has moments", (it) => it.moments > 0), L("mom:todo", "Marks without moments", (it) => it.marks > 0 && !it.moments)] },
      { id: "prv", title: "Clip preview", opts: [L("prv:y", "Made", (it) => !!it.preview), L("prv:n", "Not made", (it) => !it.preview)] },
    ] });
  }
  if (S.view === "footage" || S.view === "sources") {
    const names = distinct(items, (i) => i.loc);
    const wh = [];
    if (names.length > 1) wh.push({ id: "loc", title: "Plays from", opts: names.map((n) => L("loc:" + n, n, (it) => it.loc === n)) });
    wh.push({ id: "where2", title: "Copies", opts: [L("cp:2", "In two locations or more", (it) => (it.locs || []).length > 1)] });
    wh.push({ id: "iss", title: "Issues", opts: [L("iss:own", "Two own files", (it) => it.conflict)] });
    sections.push({ id: "wh", title: "Where", facets: wh });
  }
  const tagList = [];
  for (const it of items) for (const t of it.tags || []) if (tagList.indexOf(t) < 0) tagList.push(t);
  if (tagList.length) sections.push({ id: "tg", title: "Tags", facets: [{ id: "tag", title: "Tag", opts: tagList.sort().map((t) => L("tag:" + t, t, (it) => (it.tags || []).indexOf(t) >= 0)) }] });
  if (S.view === "moments") {
    const rf = (p, st) => (it) => (it.renders || {})[p] === st;
    sections.push({ id: "rn", title: "Renders", facets: ["HDR", "SDR"].map((p) => ({ id: "rn" + p, title: p, opts: [
      L("rn:" + p + ":c", "Current", rf(p, "current")), L("rn:" + p + ":s", "Stale", rf(p, "stale")), L("rn:" + p + ":n", "None", (it) => !(it.renders || {})[p])] })) });
  }
  if (S.view === "stills") {
    sections.push({ id: "frm2", title: "Kind", facets: [{ id: "form", title: "Kind", opts: distinct(items, (i) => i.form).map((f) => L("form:" + f, f, (it) => it.form === f)) }] });
  }
  if (S.view === "exports") {
    sections.push({ id: "lk", title: "Look", facets: [{ id: "look", title: "Look", opts: distinct(items, (i) => i.look).map((f) => L("look:" + f, f, (it) => it.look === f)) }] });
  }
  FACETS = sections;
}
function allFacets() { const a = []; FACETS.forEach((x) => x.facets.forEach((f) => a.push(f))); return a; }
function facetLeaves(f) { const out = []; f.opts.forEach((o) => (o.kids || [o]).forEach((l) => out.push(l))); return out; }
function findOpt(id) { for (const f of allFacets()) for (const o of f.opts) { if (o.id === id) return o; if (o.kids) for (const k of o.kids) if (k.id === id) return k; } return null; }
function facetMatch(f, it) { const on = facetLeaves(f).filter((l) => S.filters[l.id]); return !on.length || on.some((l) => l.fn(it)); }
function textMatch(it) {
  const q = S.q.trim().toLowerCase();
  if (!q) return true;
  return [it.name, it.cam, it.color, it.sub, it.dir, it.date].concat(it.tags || []).join(" ").toLowerCase().indexOf(q) >= 0;
}
function matches(it, except) { return textMatch(it) && allFacets().every((f) => f === except || facetMatch(f, it)); }
function activeTokens() { const t = []; allFacets().forEach((f) => { const on = facetLeaves(f).filter((l) => S.filters[l.id]); if (on.length) t.push({ f, labels: on.map((l) => l.label) }); }); return t; }
function nActive() { return Object.keys(S.filters).filter((k) => S.filters[k] && findOpt(k)).length; }
function searching() { return S.q.trim() !== "" || S.flat; }
function filtering() { return nActive() > 0; }
function setOpt(id, on) { const o = findOpt(id); if (!o) return; (o.kids || [o]).forEach((k) => { if (on) S.filters[k.id] = true; else delete S.filters[k.id]; }); }

// ---------- the tree ----------

function dirKey() { return S.path.join("/"); }
function items() { return (DATA[S.view] && DATA[S.view].items) || []; }
function under(path) { return path ? items().filter((it) => it.dir === path || it.dir.startsWith(path + "/")) : items(); }
function childDirs(dir) {
  const ds = (DATA[S.view] && DATA[S.view].dirs) || [];
  return ds.filter((d) => (d.lastIndexOf("/") < 0 ? "" : d.slice(0, d.lastIndexOf("/"))) === dir);
}
function scopeItems() { return S.scope === "all" || !S.path.length ? items() : under(dirKey()); }
function plural(n) { const w = NOUN[S.view]; return n + " " + (n === 1 ? w[0] : w[1]); }

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
    if (direct.length) parts.push(plural(direct.length));
    selName = parts.join(" and ");
  }
  // What a batch is kept to: the items, and the folders whole (the player takes what's under a folder) unless filtered
  const filt = filtering() || S.q.trim() !== "";
  // Footage's and Sources' folders aren't the project's tree: their clips, by their paths in it
  const clipsView = S.view === "footage" || S.view === "sources";
  const only = clipsView ? Array.from(new Set(selItems.map((it) => it.id))) : S.view === "clip" ? selItems.map((it) => KIND_BASE[it.kind] + it.id) : filt ? selItems.map((it) => BASE[S.view] + it.id) : selFolders.map((p) => BASE[S.view] + p).concat(direct.map((it) => BASE[S.view] + it.id));
  return { viewName, folderName, selFolders, direct, selItems, any, selName, only, label: plural(selItems.length) };
}

// ---------- drawing ----------

function renderTop() {
  const recent = S.recent.filter((r) => r.path !== S.root);
  $("top").innerHTML =
    '<div class="menuwrap"><button class="proj" type="button" data-a="menu" aria-haspopup="menu" aria-expanded="' + S.menu + '">' + ICON.folder + '<span class="ttl"><span>' + esc(S.root ? S.name : "Action Footage Library") + '</span></span>' + ICON.down + '</button>' +
    (S.menu ? '<div class="menu" role="menu">' +
      (S.root ? '<button type="button" role="menuitem" class="cur" data-a="closemenu"><span class="ck">' + ICON.check + '</span><span style="min-width:0"><span style="display:block">' + esc(S.name) + '</span><span class="sub">' + esc(S.root) + '</span></span></button>' : '') +
      recent.map((r) => '<button type="button" role="menuitem" data-a="openrecent" data-v="' + esc(r.path) + '"><span class="ck"></span><span style="min-width:0"><span style="display:block">' + esc(r.name) + '</span><span class="sub">' + esc(r.path) + '</span></span></button>').join("") +
      '<hr><button type="button" role="menuitem" data-a="choose"><span class="ck"></span>Open a project…</button>' +
      (S.root ? '<hr><button type="button" role="menuitem" data-a="sheet" data-v="settings"><span class="ck"></span>Project settings…</button>' +
        '<button type="button" role="menuitem" data-a="takein"><span class="ck"></span>Take in another project…</button>' +
        '<button type="button" role="menuitem" data-a="moveproject"><span class="ck"></span>Move the project to…</button>' +
        '<button type="button" role="menuitem" data-a="exportjson"><span class="ck"></span>Export the library as JSON…</button>' +
        '<button type="button" role="menuitem" data-a="revealroot"><span class="ck"></span>' + (S.macos ? "Show in Finder" : "Show in Explorer") + '</button>' : '') +
      '</div>' : '') + '</div>' +
    (S.root ? '<div class="seg" role="tablist" aria-label="What to browse">' + VIEWS.map((v) => '<button type="button" role="tab" aria-selected="' + (S.view === v[0]) + '" class="' + (S.view === v[0] ? "on" : "") + '" data-a="view" data-v="' + v[0] + '">' + v[1] + '</button>').join("") + '</div>' +
      '<div class="searchwrap"><div class="search">' + ICON.search + activeTokens().map((t) => '<span class="tok"><span>' + esc(t.f.title + ": " + t.labels.join(", ")) + '</span><button type="button" data-a="tokrm" data-v="' + esc(t.f.id) + '" aria-label="Remove filter ' + esc(t.f.title) + '">' + ICON.x + '</button></span>').join("") +
      '<input id="q" type="search" value="' + esc(S.q) + '" placeholder="' + (nActive() ? "Add words or another filter" : "Search names, cameras, dates, or type a filter: dji, log, wider…") + '" aria-label="Search" autocomplete="off" spellcheck="false"></div><div id="sugg"></div></div>' +
      '<button type="button" class="btn' + (S.filtersOpen ? " on" : "") + '" data-a="filters" aria-pressed="' + S.filtersOpen + '">' + ICON.filter + 'Filters' + (nActive() ? " · " + nActive() : "") + '</button>' +
      '<div class="seg" aria-label="Layout"><button type="button" class="' + (S.layout === "list" ? "on" : "") + '" data-a="layout" data-v="list" aria-label="List">' + ICON.list + '</button><button type="button" class="' + (S.layout === "grid" ? "on" : "") + '" data-a="layout" data-v="grid" aria-label="Grid">' + ICON.grid + '</button></div>' : "");
}

function mosaic(path, filt) {
  const its = under(path).filter((x) => x.thumbPath && (!filt || matches(x))).slice(0, 4);
  let h = '<span class="mosaic">';
  for (let i = 0; i < 4; i++) h += its[i] ? '<i>' + img(its[i].thumbPath) + '</i>' : '<i></i>';
  return h + '<span class="fb">' + ICON.fsm + '</span></span>';
}
function img(path) {
  const t = THUMBS.get(path);
  return '<img data-thumb="' + esc(path) + '" alt=""' + (t ? ' src="' + t + '"' : (t === null ? ' hidden' : '')) + '>';
}

function renderMain() {
  const main = $("main");
  if (!S.root) { main.innerHTML = ""; return; }
  noteHistory();
  const c = ctx(), srch = searching(), filt = filtering();
  const dir = dirKey();
  let hiddenFolders = 0;
  const folderRows = childDirs(dir).map((d) => ({ kind: "folder", path: d, name: d.split("/").pop(), dir }));
  const here = folderRows.concat(items().filter((it) => it.dir === dir));
  const list = srch ? scopeItems().filter((it) => matches(it)) : !filt ? here : here.filter((it) => {
    if (it.kind !== "folder") return matches(it);
    const ok = under(it.path).some((x) => matches(x));
    if (!ok) hiddenFolders++;
    return ok;
  });
  let h = "";
  if (srch) {
    h += '<div class="scope"><span class="sub" style="display:inline">Search in</span>' +
      (S.path.length ? '<div class="seg"><button type="button" class="' + (S.scope === "here" ? "on" : "") + '" data-a="scope" data-v="here">' + esc(c.folderName) + '</button><button type="button" class="' + (S.scope === "all" ? "on" : "") + '" data-a="scope" data-v="all">All of ' + c.viewName + '</button></div>' : '<b>All of ' + c.viewName + '</b>') +
      '<span class="sub" style="display:inline">' + list.length + ' found' + (S.q.trim() ? "" : " by the filters") + '</span><button type="button" class="btn sm" data-a="endsearch" style="margin-left:auto">Back to the folder</button></div>';
  } else if (S.view === "clip") {
    const cl = DATA.clip && DATA.clip.clip;
    const dirs = cl && cl.dir ? cl.dir.split("/") : [];
    h += '<div class="crumbs">' + navButtons() + '<button type="button" data-a="fcrumb" data-v="">Footage</button>';
    dirs.forEach((p, i) => { h += '<span aria-hidden="true">›</span><button type="button" data-a="fcrumb" data-v="' + esc(dirs.slice(0, i + 1).join("/")) + '">' + esc(p) + '</button>'; });
    h += '<span aria-hidden="true">›</span><button type="button" class="here" data-a="gofootage" data-v="' + esc(S.clip) + '" title="Go to the clip in Footage">' + esc(clipName()) + '</button>' +
      '<span class="where">Its marks, moments and photos, in the order they are in the clip</span></div>';
  } else {
    h += '<div class="crumbs">' + navButtons() + '<button type="button" class="' + (S.path.length ? "" : "here") + '" data-a="crumb" data-v="0">' + c.viewName + '</button>';
    S.path.forEach((p, i) => { h += '<span aria-hidden="true">›</span><button type="button" class="' + (i === S.path.length - 1 ? "here" : "") + '" data-a="crumb" data-v="' + (i + 1) + '">' + esc(p) + '</button>'; });
    h += '<span class="where">' + esc(S.view === "footage" ? "As arranged in _Clips" : S.view === "sources" ? (S.path.length ? sourceOf(S.path[0]) : "Where the footage is, first wins") : S.root + "/" + BASE[S.view].replace(/\/$/, "")) + '</span></div>';
    if (S.view === "sources" && !S.path.length) h += '<div class="selbar"><button type="button" class="btn sm" data-a="addsource">Add a source…</button><button type="button" class="btn sm" data-a="sheet" data-v="settings">Sources settings…</button></div>';
    if (filt) {
      const all = under(dir).filter((x) => matches(x)).length;
      h += '<div class="scope"><span class="chip fast">Filtered</span><span class="sub" style="display:inline">' + list.length + ' of ' + here.length + ' here' + (hiddenFolders ? " · " + hiddenFolders + " folder" + (hiddenFolders > 1 ? "s" : "") + " with no match hidden" : "") + '</span>' +
        '<button type="button" class="btn sm" data-a="flat" style="margin-left:auto">List every match below here (' + all + ')</button></div>';
    }
  }
  const scanView = S.view === "sources" ? "footage" : S.view;
  if (S.scanning[scanView] && S.cachedViews[scanView]) h += '<div class="indexing"><span class="spin" aria-hidden="true"></span> Looking at the disk again: what was there last time shows meanwhile</div>';
  if (probesPending && (S.view === "footage" || S.view === "sources" || S.view === "moments")) h += '<div class="indexing">Reading what the clips are: ' + probesPending + ' to go. Filters fill in as they come.</div>';
  if (!DATA[S.view]) { main.innerHTML = h + '<div class="empty"><span class="spin" aria-hidden="true"></span> Scanning the project’s files…</div>'; VISIBLE = []; return; }
  VISIBLE = list.map(keyOf);
  const nSel = VISIBLE.filter((k) => S.sel[k]).length;
  if (list.length) h += '<div class="selbar"><span class="sub" style="display:inline">' + (nSel ? nSel + " of " + list.length + " selected" : "None selected") + '</span><button type="button" class="btn sm" data-a="selall"' + (nSel === list.length ? " disabled" : "") + '>Select all</button><button type="button" class="btn sm" data-a="desel"' + (nSel ? "" : " disabled") + '>Select none</button></div>';
  const thumb = (it) => it.kind === "folder" ? mosaic(it.path, filt) : it.kind === "mark" ? '<span class="th mk" aria-hidden="true">' + ICON_MARK + '</span>' : '<span class="th">' + (it.thumbPath ? img(it.thumbPath) : "") + (it.dur ? '<span class="d">' + esc(it.dur) + '</span>' : "") + '</span>';
  const folderSub = (it) => { const all = under(it.path); const m = filt ? all.filter((x) => matches(x)).length : all.length; return filt && !srch ? m + " of " + all.length + " match" : plural(all.length); };
  const subOf = (it) => it.kind === "folder" ? (S.view === "sources" && !it.dir ? sourceOf(it.name) + " · " + folderSub(it) : folderSub(it)) : it.sub + (srch ? " · in " + (it.dir || "the top folder") : "");
  if (!list.length) {
    if (S.view === "clip" && !srch && !filt) h += '<div class="empty">No marks, moments or photos of this clip yet. Mark it in the player (M), then make Moments or Photos of the marks.</div>'; else
    h += '<div class="empty">' + (srch ? "Nothing found. Search all of " + c.viewName + ", or remove a filter." : filt ? "Nothing in this folder matches. List every match below here, or remove a filter." : "This folder is empty.") + '</div>';
  } else if (S.layout === "list") {
    const clipsV = S.view === "footage" || S.view === "sources";
    const cnt = clipsV ? "Marks · mom." : "";
    h += '<div class="head" aria-hidden="true"><span></span><span>Name</span><span class="hide-s">' + (clipsV ? "Recorded" : "Date") + '</span><span class="hide-s">Length</span><span class="hide-s">' + cnt + '</span><span class="hide-s">Status</span><span>' + (S.locations.length > 1 && S.view === "footage" ? "Plays from" : "Size") + '</span></div><div class="list">';
    for (const it of list) {
      const key = keyOf(it), sel = !!S.sel[key], f = it.kind === "folder";
      h += '<button type="button" class="row' + (sel ? " sel" : "") + '" data-a="pick" data-v="' + esc(key) + '" aria-pressed="' + sel + '">' + thumb(it) +
        '<span style="min-width:0">' + (S.renaming === key ? '<input id="rename" class="renamer" type="text" value="' + esc(renameStem(it)) + '" aria-label="New name">' : '<span class="nm">' + esc(it.name) + '</span>') + '<span class="sub">' + esc(subOf(it)) + '</span></span>' +
        '<span class="cell hide-s">' + (f ? "" : esc(it.date)) + '</span><span class="cell hide-s">' + (f ? "" : esc(it.dur)) + '</span>' +
        '<span class="cell hide-s">' + (it.kind === "clip" ? (it.marks ? it.marks + " · " + (it.moments || 0) : "—") : "") + '</span>' +
        '<span class="chips hide-s">' + chips(it) + '</span><span class="cell">' + (f ? "" : (S.locations.length > 1 && it.kind === "clip" ? esc(it.loc) : sizeText(it.size))) + '</span></button>';
    }
    h += '</div>';
  } else {
    h += '<div class="grid">';
    for (const it of list) {
      const key = keyOf(it), sel = !!S.sel[key];
      h += '<button type="button" class="card' + (sel ? " sel" : "") + '" data-a="pick" data-v="' + esc(key) + '" aria-pressed="' + sel + '">' + thumb(it) +
        (S.renaming === key ? '<input id="rename" class="renamer" type="text" value="' + esc(renameStem(it)) + '" aria-label="New name">' : '<span class="nm">' + esc(it.name) + '</span>') + '<span class="sub">' + esc(subOf(it)) + '</span><span class="chips">' + chips(it) + '</span></button>';
    }
    h += '</div>';
  }
  const scrollTop = main.scrollTop;
  main.innerHTML = h;
  main.scrollTop = scrollTop;
  fillThumbs();
  const ren = $("rename");
  if (ren) { ren.focus(); ren.select(); }
}
function sizeText(n) {
  if (!(n > 0)) return "";
  const u = ["B", "KB", "MB", "GB", "TB"]; let i = 0;
  while (n >= 1000 && i < u.length - 1) { n /= 1000; i++; }
  return (n >= 10 || i === 0 ? Math.round(n) : n.toFixed(1)) + " " + u[i];
}
function chips(it) {
  const c = [];
  for (const t of it.tags || []) c.push([t === "Deleted in Photos" ? "chip sys" : "chip tag", t]);
  if (it.kind === "folder") {
    const inner = under(it.path);
    const prev = inner.filter((x) => x.preview).length;
    if ((S.view === "footage" || S.view === "sources") && inner.length) c.push(["chip " + (prev === inner.length ? "ok" : ""), prev + " of " + inner.length + " previews"]);
  } else if (it.kind === "clip") {
    if (it.conflict) c.push(["chip warn", it.own_conflict.length + " own files"]);
    if ((it.locs || []).length > 1) c.push(["chip fast", "Also in " + it.locs.slice(1).join(", ")]);
    if (!it.preview) c.push(["chip", "No preview"]);
    if (it.gyroflow) c.push(["chip ok", "Gyroflow project"]);
    if (it.motion === false) c.push(["chip", "No motion data"]);
    if (it.is360) c.push(["chip fast", "360°"]);
  } else if (it.kind === "moment") {
    for (const p of ["HDR", "SDR"]) { const st = (it.renders || {})[p]; if (st) c.push(["chip " + (st === "current" ? "ok" : "stale"), p + (st === "current" ? "" : " stale")]); }
    if (it.gyroflow) c.push(["chip ok", "Own Gyroflow project"]);
    if (!it.link) c.push(["chip warn", "No link"]);
  } else if (it.kind === "mark") {
    if (!it.hasMoment) c.push(["chip", "No moment"]);
    if (!it.hasPhoto) c.push(["chip", "No photo"]);
  } else if (it.kind === "still") {
    c.push(["chip tag", it.form]);
  } else if (it.kind === "export") {
    c.push(["chip tag", it.look]);
  }
  return c.map((x) => '<span class="' + x[0] + '">' + esc(x[1]) + '</span>').join("");
}

// The pictures: from the app, a few at a time, each set where it shows once it's there
let thumbsBusy = 0;
const thumbsWanted = [];
function fillThumbs() {
  for (const el of document.querySelectorAll("img[data-thumb]")) {
    const p = el.getAttribute("data-thumb");
    if (THUMBS.has(p)) continue;
    if (thumbsWanted.indexOf(p) < 0) thumbsWanted.push(p);
  }
  pumpThumbs();
}
function pumpThumbs() {
  while (thumbsBusy < 6 && thumbsWanted.length) {
    const p = thumbsWanted.shift();
    if (THUMBS.has(p)) continue;
    thumbsBusy++;
    invoke("thumb", { path: p }).then((u) => u || null, () => null).then((u) => {
      THUMBS.set(p, u);
      for (const el of document.querySelectorAll('img[data-thumb="' + CSS.escape(p) + '"]')) { if (u) { el.src = u; el.hidden = false; } else el.hidden = true; }
    }).finally(() => { thumbsBusy--; pumpThumbs(); });
  }
}

function optRow(id, label, count, checked, ind, isParent, exp, isKid) {
  const cid = "fo-" + id.replace(/[^a-z0-9]/gi, "_");
  return '<div class="opt' + (isKid ? " kid" : "") + (count ? "" : " zero") + '">' +
    (isParent ? '<button type="button" class="tw" data-a="fexp" data-v="' + esc(id) + '" aria-expanded="' + exp + '" aria-label="' + (exp ? "Collapse " : "Expand ") + esc(label) + '">' + ICON.chev + '</button>' : "<span></span>") +
    '<input type="checkbox" id="' + cid + '" data-a="fopt" data-v="' + esc(id) + '"' + (checked ? " checked" : "") + (ind ? ' data-ind="1"' : "") + '>' +
    '<label for="' + cid + '"><span class="lb">' + esc(label) + '</span></label><span class="ct">' + count + '</span></div>';
}
function renderFilters() {
  const el = $("fpanel");
  el.hidden = !S.root || !S.filtersOpen;
  if (el.hidden) { el.innerHTML = ""; return; }
  const srch = searching();
  const base = srch ? scopeItems().filter(textMatch) : under(dirKey());
  const where = srch ? (S.scope === "all" || !S.path.length ? "all of " + ctx().viewName : S.path[S.path.length - 1] + " and below") : (S.path.length ? S.path[S.path.length - 1] + " and its folders" : "all of " + ctx().viewName);
  let h = '<div class="fh"><b>Filters</b><button type="button" data-a="fclear"' + (nActive() ? "" : " disabled") + '>Clear all</button></div>' +
    '<div class="rule">Ticks in one list: any of them. Different lists: all of them. ' + (srch ? "Applied to the search." : "Applied to the folder you’re in.") + ' Numbers: how many match in ' + esc(where) + '.</div>';
  let hidden = 0;
  for (const sx of FACETS) {
    const open = !!S.fsec[sx.id];
    const n = sx.facets.reduce((a, f) => a + facetLeaves(f).filter((l) => S.filters[l.id]).length, 0);
    let body = "";
    for (const f of sx.facets) {
      const others = base.filter((it) => matches(it, f));
      const cnt = (fns) => others.filter((it) => fns.some((fn) => fn(it))).length;
      let rows = "";
      for (const o of f.opts) {
        if (o.kids) {
          const onN = o.kids.filter((k) => S.filters[k.id]).length;
          const kids = o.kids.map((k) => ({ k, c: cnt([k.fn]) })).filter((x) => x.c > 0 || S.filters[x.k.id]);
          hidden += o.kids.length - kids.length;
          if (!kids.length) { hidden++; continue; }
          const exp = S.fexp[o.id] !== undefined ? S.fexp[o.id] : (kids.length > 1 || onN > 0);
          rows += optRow(o.id, o.label, cnt(o.kids.map((k) => k.fn)), onN === o.kids.length, onN > 0 && onN < o.kids.length, true, exp, false);
          if (exp) kids.forEach((x) => { rows += optRow(x.k.id, x.k.label, x.c, !!S.filters[x.k.id], false, false, false, true); });
        } else {
          const c1 = cnt([o.fn]);
          if (!c1 && !S.filters[o.id]) { hidden++; continue; }
          rows += optRow(o.id, o.label, c1, !!S.filters[o.id], false, false, false, false);
        }
      }
      if (rows) body += (sx.facets.length > 1 ? '<div class="ftitle">' + esc(f.title) + '</div>' : "") + rows;
    }
    if (!body && !n) continue;
    h += '<div class="fsec"><button type="button" class="fsech" data-a="fsec" data-v="' + sx.id + '" aria-expanded="' + open + '">' + ICON.chev + esc(sx.title) + (n ? '<span class="n">' + n + ' on</span>' : "") + '</button>';
    if (open) h += '<div class="fbody">' + body + '</div>';
    h += '</div>';
  }
  if (hidden) h += '<div class="rule" style="padding-top:10px">' + hidden + " filter" + (hidden > 1 ? "s" : "") + " with nothing to show here hidden.</div>";
  const st = el.scrollTop;
  el.innerHTML = h;
  el.scrollTop = st;
  el.querySelectorAll('input[data-ind="1"]').forEach((i) => { i.indeterminate = true; });
}
function suggestions() {
  const q = S.q.trim().toLowerCase();
  if (!q) return [];
  const out = [];
  for (const f of allFacets()) for (const o of f.opts) {
    const list = o.kids ? [o].concat(o.kids.map((k) => Object.assign({ parent: o }, k))) : [o];
    for (const x of list) {
      const path = f.title + (x.parent ? " › " + x.parent.label : "");
      const on = x.kids ? x.kids.every((k) => S.filters[k.id]) : S.filters[x.id];
      if ((x.label + " " + path).toLowerCase().indexOf(q) >= 0 && !on) out.push({ id: x.id, label: x.label, path });
    }
  }
  return out.slice(0, 7);
}
function renderSugg() {
  const el = $("sugg");
  if (!el) return;
  const q = S.q.trim();
  if (!S.suggOpen || !q) { el.innerHTML = ""; return; }
  const list = suggestions();
  let h = '<div class="sugg" role="listbox" aria-label="Suggestions">';
  if (list.length) { h += '<div class="sh">Filters</div>'; list.forEach((x, i) => { h += '<button type="button" role="option" class="' + (S.sugg === i ? "hi" : "") + '" data-a="suggpick" data-v="' + esc(x.id) + '"><span>' + esc(x.label) + '</span><span class="path">' + esc(x.path) + '</span></button>'; }); }
  h += '<div class="sh">Words</div><button type="button" role="option" class="' + (S.sugg === list.length ? "hi" : "") + '" data-a="suggtext"><span>Names, cameras and dates with “' + esc(q) + '”</span></button></div>';
  el.innerHTML = h;
}

function queueHtml() {
  const fin = (j) => ["done", "stopped", "failed"].includes(j.state);
  const live = S.jobs.filter((j) => !fin(j)), done = S.jobs.filter(fin).slice().reverse();
  let h = '<div class="q">';
  if (!live.length) h += '<p class="sub wrap">Nothing running. Commands on what you select land here.</p>';
  for (const j of live) {
    let pct = j.total > 0 ? Math.min(100, ((j.done + (j.fraction || 0)) / j.total) * 100) : 0;
    const what = j.state === "waiting" ? "waiting" : j.state === "starting" ? "starting the player" : j.state === "looking" ? "looking through the project" : j.state === "stopping" ? "stopping" : (j.total > 0 ? j.done + " of " + j.total : "");
    const other = j.project !== S.root ? " · " + j.project_name : "";
    h += '<div class="job"><div class="t"><b>' + esc(j.title + other) + '</b><span class="pill">' + esc(j.state) + '</span></div>' +
      (j.state === "waiting" ? "" : '<div class="bar2"><i style="width:' + pct.toFixed(1) + '%"></i></div>') +
      '<div class="t"><span class="sub" style="flex:1">' + esc([what, j.current].filter(Boolean).join(" · ")) + '</span><button type="button" class="btn sm" data-a="cancel" data-v="' + j.id + '">' + (j.state === "waiting" ? "Remove" : "Stop") + '</button></div></div>';
  }
  if (done.length) {
    h += '<div class="qh" style="display:flex;align-items:center">Done<button type="button" class="btn sm" data-a="clearjobs" style="margin-left:auto">Clear</button></div>';
    for (const j of done) {
      const parts = [];
      if (j.made != null) parts.push(j.made + " made");
      if (j.failed > 0) parts.push(j.failed + " failed");
      if (j.message) parts.push(j.message);
      const other = j.project !== S.root ? " · " + j.project_name : "";
      h += '<div class="job"><div class="t"><b>' + esc(j.title + other) + '</b><span class="pill">' + esc(j.state) + '</span></div><span class="sub wrap' + (j.state === "failed" ? " err" : "") + '">' + esc(parts.join(" · ")) + '</span>' +
        ((j.failures || []).length ? '<details><summary>' + j.failures.length + " failure" + (j.failures.length > 1 ? "s" : "") + '</summary><ul>' + j.failures.map((f) => "<li><code>" + esc(f[0]) + "</code>: " + esc(f[1]) + "</li>").join("") + "</ul></details>" : "") + '</div>';
    }
  }
  h += '<p class="sub wrap">One job at a time, by ActionFootagePlayer without a window. Closing the Library asks first.</p></div>';
  return h;
}
function activeCount() { return S.jobs.filter((j) => !["done", "stopped", "failed"].includes(j.state)).length; }

function sec(key, title, hint, body) {
  const open = !!S.open[key];
  return '<div class="sec"><button type="button" class="sech" data-a="sec" data-v="' + key + '" aria-expanded="' + open + '">' + ICON.chev + esc(title) + '<span class="hint">' + esc(hint) + '</span></button>' + (open ? '<div class="secb">' + body + '</div>' : "") + '</div>';
}
// One clip: its moments and photos; one moment, photo or export: its clip in Footage
function clipButton(c) {
  if (c.direct.length !== 1 || c.selFolders.length) return "";
  const it = c.direct[0];
  if (it.kind === "clip") return '<button type="button" class="btn sm" data-a="showclip" data-v="' + esc(it.id) + '">Marks, moments and photos</button>';
  if (it.kind === "mark") return '<button type="button" class="btn sm" data-a="gofootage" data-v="' + esc(S.clip) + '">Go to the footage</button>';
  const id = clipOf(it);
  return id ? '<button type="button" class="btn sm" data-a="gofootage" data-v="' + esc(id) + '">Go to the footage</button>' : "";
}
function renderSide() {
  const side = $("side");
  side.hidden = !S.root;
  if (!S.root) return;
  const c = ctx();
  let h = '<div class="tabs" role="tablist"><button type="button" role="tab" class="' + (S.tab === "commands" ? "on" : "") + '" data-a="tab" data-v="commands">Commands</button><button type="button" role="tab" class="' + (S.tab === "queue" ? "on" : "") + '" data-a="tab" data-v="queue">Queue<span class="count">' + activeCount() + '</span></button><button type="button" role="tab" class="' + (S.tab === "info" ? "on" : "") + '" data-a="tab" data-v="info">Info</button></div>';
  if (S.tab === "queue") { side.innerHTML = h + '<div id="qtab">' + queueHtml() + '</div>'; return; }
  if (S.tab === "info") { side.innerHTML = h + '<div class="itab">' + infoHtml(c) + '</div>'; fillThumbs(); return; }
  h += '<div class="summary">' + (c.any
    ? '<b>' + esc(c.selName) + '</b><span class="sub wrap" style="margin-top:2px">' + esc("Commands below act on " + plural(c.selItems.length) + (c.selFolders.length ? ", everything inside the selected folders included." : ".")) + '</span>' +
      '<div class="btns" style="margin-top:8px"><button type="button" class="btn sm" data-a="openit">' + (c.selItems.length && c.selItems.every((x) => x.kind === "still") ? "Open" : "Open in the player") + '</button>' +
      (c.selItems.length === 1 && c.selItems[0].gyroflow ? '<button type="button" class="btn sm" data-a="opengf">Open in Gyroflow</button>' : "") + clipButton(c) + '<button type="button" class="btn sm" data-a="revealit">' + (S.macos ? "Show in Finder" : "Show in Explorer") + '</button><button type="button" class="btn sm" data-a="desel">Select none</button></div>'
    : '<b>Nothing selected</b><span class="sub wrap" style="margin-top:2px">Commands act on what you select: ' + NOUN[S.view][1] + ' or whole folders. Their settings can be set beforehand.</span>' +
      '<div class="btns" style="margin-top:8px"><button type="button" class="btn sm" data-a="selall"' + (VISIBLE.length ? "" : " disabled") + '>Select all' + (VISIBLE.length ? " (" + VISIBLE.length + ")" : "") + '</button></div>') + '</div>';
  const info = S.info || {};
  if (S.view === "sources" && c.selFolders.length && c.selFolders.every((f) => !f.includes("/")) && !c.direct.length) {
    h += '<div class="summary"><div class="btns"><button type="button" class="btn sm" data-a="removesource">Remove from the project</button></div><span class="sub wrap" style="margin-top:6px">Its clips stay where they are: the project stops looking there, and its pointer file goes.</span></div>';
  }
  if (S.view === "footage" || S.view === "sources" || S.view === "moments") h += checksSection(c);
  h += tagsSection(c);
  if (S.view === "footage" || S.view === "sources") {
    h += sec("moments", "Moments & previews", "from the clips",
      '<p>For ' + esc(c.any ? c.label : "what you select") + '.</p>' +
      '<div class="btns"><button type="button" class="btn sm" data-a="job" data-v="clips">Make clip previews</button></div>' +
      '<span class="sub wrap">The ones without a preview, removed ones too. A clip identical to another (the same file twice) gets its preview in _Clips/Duplicates, by where it is.</span>' +
      '<div class="field"><label class="sub" for="clips_layout">Into _Clips</label><select id="clips_layout" data-a="layout_set">' +
      [["tree", "As the footage’s folders"], ["trips", "By trips: clips at most 2 days apart"], ["flat", "All in _Clips itself"]].map((o) => '<option value="' + o[0] + '"' + ((info.clips_layout || (info.clips_flat ? "flat" : "tree")) === o[0] ? " selected" : "") + '>' + o[1] + '</option>').join("") + '</select></div>' +
      '<div class="btns"><button type="button" class="btn sm" data-a="job" data-v="moments">Moments of the marks</button></div>' +
      '<label class="switch"><input type="checkbox" id="moments_alone" data-a="setting" data-v="moments_alone"' + (info.moments_alone ? " checked" : "") + '>Stabilized on their own, not as part of the clip</label>' +
      '<div class="field"><label class="sub" for="pkind">Photos as</label><select id="pkind" data-a="pkind"><option value="no"' + (S.photoKind === "no" ? " selected" : "") + '>Stills</option>' +
      (S.macos ? '<option value="live"' + (S.photoKind === "live" ? " selected" : "") + '>Live Photos</option>' : "") + '<option value="motion"' + (S.photoKind === "motion" ? " selected" : "") + '>Motion Photos</option></select></div>' +
      '<div class="btns"><button type="button" class="btn sm" data-a="job" data-v="photos">Photos of the marks</button></div>' +
      '<span class="sub wrap">One of each mark, made once: a deleted one isn’t made again.</span>');
  } else if (S.view === "moments") {
    const on = ["HDR", "SDR"].filter((p) => S.profiles[p]);
    const stats = on.map((p) => { let cu = 0, st = 0, mi = 0; c.selItems.forEach((it) => { const v = (it.renders || {})[p]; if (v === "current") cu++; else if (v === "stale") st++; else mi++; }); return [p, cu, st, mi]; });
    const toMake = stats.reduce((n, x) => n + x[2] + x[3], 0);
    h += sec("render", "Render", on.join(", ") || "no profile",
      '<div class="lab">Profiles</div><div class="checks">' +
      [["HDR", "HLG, as Shift+E exports HDR"], ["SDR", "as Shift+E exports SDR"]].map((p) => '<label title="' + esc(p[1]) + '"><input type="checkbox" id="prof-' + p[0] + '" data-a="prof" data-v="' + p[0] + '"' + (S.profiles[p[0]] ? " checked" : "") + '>' + p[0] + '</label>').join("") + '</div>' +
      (c.any && stats.length ? '<div class="result">' + stats.map((x) => '<div class="it"><b style="width:36px">' + x[0] + '</b><span>' + x[1] + ' current · ' + x[2] + ' stale · ' + x[3] + ' missing</span></div>').join("") + '</div>' : "") +
      '<div class="btns"><button type="button" class="btn sm pri" data-a="job" data-v="render"' + (c.any && on.length && !toMake ? " disabled" : "") + '>' + (c.any ? (toMake ? "Render " + toMake + " (missing and stale)" : "All current") : "Render") + '</button>' +
      '<button type="button" class="btn sm" data-a="delrenders" data-v="stale">Delete stale</button><button type="button" class="btn sm" data-a="delrenders" data-v="all">Delete renders</button></div>' +
      '<span class="sub wrap">Kept in _Renders/ of the project. An export with the same look copies a current render instead of rendering again. Deleting renders breaks nothing.</span>');
    h += sec("export", "Export", "to _Exports",
      '<div class="field"><label class="sub" for="look">Look</label><select id="look" data-a="look"><option value="hdr"' + (S.exportSdr ? "" : " selected") + '>HDR, as the player shows it</option><option value="sdr"' + (S.exportSdr ? " selected" : "") + '>SDR</option></select></div>' +
      '<span class="sub wrap">Each with its own transform. One already exported in that look is skipped.</span>' +
      '<div class="btns"><button type="button" class="btn sm pri" data-a="job" data-v="export">Export ' + esc(c.any ? c.label : "") + '</button></div>');
  } else if (S.view === "clip") {
    h += '<div class="summary"><span class="sub wrap">Rendering and exporting are in Moments; making more moments and photos, on the clip in Footage.</span></div>';
  } else {
    h += '<div class="summary"><span class="sub wrap">Syncing folders of ' + NOUN[S.view][1] + ' with Apple Photos comes in a later version of the Library.</span></div>';
  }
  side.innerHTML = h;
  if (!c.any) side.querySelectorAll('.secb .btn').forEach((x) => { x.disabled = true; });
}

// Check and Find moved files, on the selection (library/src/checks.rs)
const KIND_TEXT = { own2: ["warn", "2 own files"], orphan: ["stale", "no clip"], twice: ["stale", "twice"], missing: ["warn", "original missing"] };
function issueText(i) {
  const short = (p) => String(p || "").split("/").slice(-2).join("/");
  if (i.kind === "own2") return i.tree + " has its own file in " + i.files.length + " places: " + i.files.map(short).join(", ") + ". Nothing is saved for it until one goes.";
  if (i.kind === "orphan") return short(i.file) + " has no clip beside it. Find moved files puts it back by the clip’s fingerprint.";
  if (i.kind === "twice") return "The same clip at " + i.trees.join(" and ") + ". Delete one.";
  if (i.kind === "missing") return short(i.file) + ": its original isn’t at " + i.tree + ".";
  return JSON.stringify(i);
}
function checksSection(c) {
  let body = '<p>Over ' + esc(c.any ? c.label : "what you select") + '. Kept to the selection: a whole big project takes long.</p>' +
    '<div class="btns"><button type="button" class="btn sm' + (S.checked ? " on" : "") + '" data-a="runcheck">' + (S.checking ? "Checking…" : "Check") + '</button>' +
    '<button type="button" class="btn sm" data-a="findmoved"' + (S.checked && S.checked.issues.some((i) => i.kind === "missing" || i.kind === "orphan") ? "" : " disabled") + '>' + (S.finding ? "Looking…" : "Find moved files") + '</button></div>';
  if (S.checked) {
    const list = S.checked.issues;
    body += '<div class="result"><b>' + (list.length ? list.length + " issue" + (list.length > 1 ? "s" : "") : "Nothing wrong") + '</b>' +
      list.slice(0, 40).map((i) => { const k = KIND_TEXT[i.kind] || ["", i.kind]; return '<div class="it"><span class="chip ' + k[0] + '">' + esc(k[1]) + '</span><span class="wrap">' + esc(issueText(i)) + '</span></div>'; }).join("") +
      (list.length > 40 ? '<span class="sub">…and ' + (list.length - 40) + ' more</span>' : "") + '</div>';
  }
  if (S.found) {
    const ok = S.found.filter((f) => f.found);
    body += '<div class="result"><b>Found ' + ok.length + ' of ' + S.found.length + '</b>' +
      S.found.map((f) => '<div class="it"><span class="mono">' + esc(f.issue.tree) + (f.found ? '<br>→ ' + esc(f.found.tree) + ' <span class="sub" style="display:inline">(' + esc(f.by) + ')</span>' : '<br>not found') + '</span></div>').join("") +
      (ok.length ? '<div class="btns"><button type="button" class="btn sm pri" data-a="applymoved">Update ' + ok.length + '</button></div>' : "") + '</div>';
  }
  return sec("checks", "Checks", "issues, moved files", body);
}
// Tags on the selection (library/src/tags.rs): kept with each item, in the library
function tagsSection(c) {
  const on = [];
  for (const it of c.selItems) for (const t of it.tags || []) if (on.indexOf(t) < 0) on.push(t);
  const body = c.any
    ? '<div class="chips">' + (on.length ? on.map((t) => '<button type="button" class="chip ' + (t === "Deleted in Photos" ? "sys" : "tag") + '" data-a="untag" data-v="' + esc(t) + '" aria-label="Remove tag ' + esc(t) + ' from the selection">' + esc(t) + ' ×</button>').join("") : '<span class="sub">No tags yet</span>') + '</div>' +
      '<div class="tagin"><input id="tagdraft" type="text" value="' + esc(S.tagDraft) + '" placeholder="Add a tag to ' + esc(c.label) + '" aria-label="New tag"><button type="button" class="btn sm" data-a="tag">Add</button></div>' +
      '<span class="sub wrap">Kept with each: a clip’s in its own file, a moment’s or export’s in its link, a photo’s in the project’s file.</span>'
    : '<p>Select items or folders to tag them.</p>';
  return sec("tags", "Tags", c.any ? on.length + " on the selection" : "select to tag", body);
}
async function setTags(add, remove) {
  const c = ctx();
  if (!c.any) return;
  try {
    const problems = await invoke("set_tags", { items: c.selItems.map((it) => ({ kind: it.kind, id: it.id, path: it.path })), add, remove });
    if (problems.length) showError(problems.join(" · "));
    await refresh();
  } catch (e) { showError(String(e)); }
}

async function runCheck() {
  const c = ctx();
  if (!c.any) return;
  S.checking = true; S.found = null; renderSide();
  try {
    S.checked = await invoke("run_check", S.view === "moments" ? { sel: [], links: c.only } : { sel: c.only, links: [] });
  } catch (e) { showError(String(e)); }
  S.checking = false; renderSide();
}
async function findMoved() {
  if (!S.checked) return;
  S.finding = true; renderSide();
  try { S.found = await invoke("find_moved", { issues: S.checked.issues.filter((i) => i.kind === "missing" || i.kind === "orphan") }); } catch (e) { showError(String(e)); }
  S.finding = false; renderSide();
}
async function applyMoved() {
  try {
    const problems = await invoke("apply_moved", { found: (S.found || []).filter((f) => f.found) });
    toast(problems.length ? "Updated, but: " + problems.join(" · ") : "Updated: links and own files follow the clips");
    S.found = null; S.checked = null;
    await refresh();
  } catch (e) { showError(String(e)); }
}

function renderEmpty() {
  $("empty").hidden = !!S.root;
  $("bodyrow").hidden = !S.root;
  if (!S.root) $("recentEmpty").innerHTML = S.recent.length ? '<div class="lab" style="margin-top:12px">Opened before</div><div class="rlist">' + S.recent.map((r) => '<button type="button" data-a="openrecent" data-v="' + esc(r.path) + '"><span class="nm">' + esc(r.name) + '</span><span class="sub">' + esc(r.path) + '</span></button>').join("") + '</div>' : "";
}
function renderAll() {
  const a = document.activeElement, id = a && a.id, pos = a && a.selectionStart;
  buildFacets();
  renderEmpty(); renderTop(); renderMain(); renderSide(); renderFilters(); renderSugg();
  if (id) { const e = $(id); if (e) { e.focus(); try { if (pos != null) e.setSelectionRange(pos, pos); } catch (_) { /* */ } } }
}
function renderQueueOnly() {
  if (S.tab === "queue") { const b = $("qtab"); if (b) b.innerHTML = queueHtml(); }
  document.querySelectorAll(".count").forEach((e) => { e.textContent = activeCount(); });
}

// ---------- the project's settings ----------

const ICON_UP = '<svg width="10" height="10" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.6" aria-hidden="true"><path d="M6 15l6-6 6 6"/></svg>';
const ICON_DN = '<svg width="10" height="10" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.6" aria-hidden="true"><path d="M6 9l6 6 6-6"/></svg>';
function hostsToText(h) { return Object.keys(h || {}).map((k) => k + " = " + (h[k] || []).join(", ")).join("\n"); }
function textToHosts(t) {
  const out = {};
  for (const line of String(t || "").split("\n")) {
    const m = /^\s*([^=\s]+)\s*=\s*(.*)$/.exec(line);
    if (m) out[m[1]] = m[2].split(",").map((x) => x.trim()).filter(Boolean);
  }
  return out;
}
async function openSettings() {
  S.menu = false;
  renderTop();
  const st = await call("project_settings").catch(() => null);
  if (!st) return;
  S.settings = st;
  const ptr = st.name + ".afplocation";
  S.locEdit = st.locations.map((l) => ({ name: l.name, at: l.at, path: l.path, copies: l.copies,
    st: (l.reachable ? "Reachable" : "Not reachable here") + (l.pointer ? " · " + ptr + " in it" : l.reachable ? " · its " + ptr + " is written on Save" : "") }));
  S.localEdit = st.local.map((l) => ({ name: l.name, at: l.at, path: l.path, bytes: l.bytes, files: l.files, reachable: l.reachable }));
  S.hostsText = hostsToText(st.hosts);
  S.sheet = "settings";
  renderSheet();
}
function renderTake() {
  const t = S.take;
  const counts = t.counts || {};
  const what = Object.keys(counts).filter((k) => counts[k]).map((k) => counts[k] + " in " + k).join(", ") || "nothing in its own folders";
  let h = '<div class="scrim" data-a="scrim"><div class="sheet" role="dialog" aria-modal="true" aria-labelledby="takeTitle">' +
    '<header><h2 id="takeTitle">Take in “' + esc(t.name) + '”</h2><span class="sub">Into ' + esc(S.name) + ', once. Nothing stays in sync afterwards.</span></header><div class="sb">' +
    '<div class="field"><span class="sub">Project</span><span class="mono">' + esc(t.other) + '/Project.afproject</span></div><div class="result">';
  for (const l of t.landings) {
    h += '<div class="it">' + (l.inside ? '<span class="chip ok">inside</span><span class="wrap">' + esc(l.name || "Its folder") + ' is already in this project, at ' + esc(l.at_here || "the top") + '/. No location added, no footage moved.</span>'
      : l.dir ? '<span class="chip fast">added</span><span class="wrap">' + esc(l.name) + ' becomes a location of this project at ' + esc(l.at_here || "?") + '/ (' + esc(l.path_here) + ').</span>'
      : '<span class="chip warn">not here</span><span class="wrap">' + esc(l.name) + ' (' + esc(l.path_here) + ') isn’t reachable from this computer: it’s added as it is.</span>') + '</div>';
  }
  h += '<div class="it"><span class="wrap">' + esc(what) + ' move into this project’s, under ' + esc(t.place || "?") + '/; their links get “' + esc(t.place) + '/” in front. The clips’ own files stay beside the clips.</span></div>';
  for (const i of t.issues || []) { const k = KIND_TEXT[i.kind] || ["", i.kind]; h += '<div class="it"><span class="chip ' + k[0] + '">' + esc(k[1]) + '</span><span class="wrap">' + esc(issueText(i)) + '</span></div>'; }
  h += '<div class="it"><span class="sub wrap">Its Project.afproject then goes to the Trash.</span></div></div>';
  if (t.landings.some((l) => !l.inside && l.dir)) {
    h += '<div class="field"><label class="sub" for="takeplace">Its other locations go at</label><span class="tagin"><input id="takeplace" type="text" value="' + esc(S.takePlace) + '" placeholder="2025/Red Sea"><button type="button" class="btn sm" data-a="takeplan">Show</button></span></div>';
  }
  h += '</div><footer><button type="button" class="btn" data-a="takein">Choose another…</button><span class="sp"></span><button type="button" class="btn" data-a="closesheet">Cancel</button><button type="button" class="btn pri" data-a="takerun"' + (t.needs_place || S.taking ? " disabled" : "") + '>' + (S.taking ? "Taking in…" : "Take in") + '</button></footer>';
  return h + '</div></div>';
}
async function chooseTakeIn() {
  S.menu = false; renderTop();
  const other = await call("choose_other").catch(() => null);
  if (!other) return;
  S.takePlace = "";
  await planTakeIn(other);
}
async function planTakeIn(other) {
  try { S.take = await invoke("take_in_plan", { other, place: S.takePlace }); S.sheet = "take"; renderSheet(); } catch (e) { showError(String(e)); }
}
async function runTakeIn() {
  if (!S.take) return;
  S.taking = true; renderSheet();
  try {
    const r = await invoke("take_in_run", { other: S.take.other, place: S.takePlace });
    S.sheet = null; S.take = null;
    toast("Taken in: " + r.moved + " files under " + r.place + "/" + (r.added ? ", " + r.added + " location" + (r.added > 1 ? "s" : "") + " added" : "") + "; its project file " + r.project_file + (r.problems.length ? ". But: " + r.problems.join(" · ") : ""));
    await refresh();
  } catch (e) { showError(String(e)); }
  S.taking = false; renderSheet();
}
function renderSheet() {
  const el = $("sheet");
  if (S.sheet === "take" && S.take) { el.innerHTML = renderTake(); return; }
  if (!S.sheet || !S.settings) { el.innerHTML = ""; return; }
  const st = S.settings;
  const tabs = [["loc", "Locations"], ["mac", S.macos ? "On this Mac" : "On this computer"], ["gen", "General"]];
  let h = '<div class="scrim" data-a="scrim"><div class="sheet" role="dialog" aria-modal="true" aria-labelledby="sheetTitle">' +
    '<header><h2 id="sheetTitle">' + esc(st.name) + ': project settings</h2><span class="mono">' + esc(st.path) + '/Project.afproject</span></header>' +
    '<div class="tabs" role="tablist" style="margin-top:8px">' + tabs.map((t) => '<button type="button" role="tab" class="' + (S.stab === t[0] ? "on" : "") + '" data-a="stab" data-v="' + t[0] + '">' + t[1] + '</button>').join("") + '</div><div class="sb">';
  if (S.stab === "loc") {
    h += '<p class="sub wrap">Where the footage is. The first that has a clip wins. Each is a network path (//nas/share/folder) or a path relative to the project folder: the same on every computer. “In the tree” is where its folders show in the project.' +
      (S.locEdit.length ? "" : " With none, the project is next to its footage: its own folder.") + '</p>';
    if (S.locEdit.length) h += '<div class="lhead" aria-hidden="true"><span></span><span>Name</span><span>In the tree</span><span>Where</span><span>Copies only</span><span></span></div>';
    S.locEdit.forEach((l, i) => {
      h += '<div class="lrow"><span class="mv"><button type="button" data-a="locup" data-v="' + i + '" aria-label="Move ' + esc(l.name || "location") + ' up"' + (i === 0 ? " disabled" : "") + '>' + ICON_UP + '</button><button type="button" data-a="locdn" data-v="' + i + '" aria-label="Move ' + esc(l.name || "location") + ' down"' + (i === S.locEdit.length - 1 ? " disabled" : "") + '>' + ICON_DN + '</button></span>' +
        '<input type="text" id="locname-' + i + '" data-l="name" data-i="' + i + '" value="' + esc(l.name) + '" aria-label="Name" placeholder="Name">' +
        '<input type="text" id="locat-' + i + '" data-l="at" data-i="' + i + '" value="' + esc(l.at) + '" aria-label="In the tree" placeholder="/ (the top)">' +
        '<input type="text" id="locpath-' + i + '" data-l="path" data-i="' + i + '" value="' + esc(l.path) + '" aria-label="Where" placeholder="//nas/share/folder" class="mono">' +
        '<label class="switch" style="min-height:30px"><input type="checkbox" id="loccp-' + i + '" data-a="loccopies" data-v="' + i + '"' + (l.copies ? " checked" : "") + '><span class="sub" style="display:inline">copies</span></label>' +
        '<button type="button" class="rm" data-a="locrm" data-v="' + i + '" aria-label="Remove ' + esc(l.name || "location") + '">' + ICON.x + '</button>' +
        '<span class="st">' + esc(l.st || "") + '</span></div>';
    });
    h += '<div class="btns"><button type="button" class="btn sm" data-a="locadd">Add a location…</button>' + (st.next_to_footage ? '<button type="button" class="btn sm" data-a="locself">Add the project folder</button>' : "") + '</div>' +
      '<p class="sub wrap">Saving writes them into Project.afproject, and a ' + esc(st.name) + '.afplocation file at the top of each, so a clip opened from Finder finds this project. A location taken out loses its file; nothing else in it changes.</p></div>' +
      '<footer><span class="sp"></span><button type="button" class="btn" data-a="closesheet">Cancel</button><button type="button" class="btn pri" data-a="savelocs">Save</button></footer>';
  } else if (S.stab === "mac") {
    h += '<p class="sub wrap">Kept only on this computer, never part of the library: delete any of it and this computer simply reads the library again.</p>' +
      '<div class="lab">Local copies of clips</div>';
    if (!S.localEdit.length) h += '<p class="sub wrap">None. A folder here with copies of some clips, at their places in the tree, plays before the library’s.</p>';
    S.localEdit.forEach((l, i) => {
      h += '<div class="kv"><span style="min-width:0"><span class="nm">' + esc(l.path) + '</span><span class="sub">In the tree at ' + esc(l.at ? l.at + "/" : "the top") + (l.reachable === false ? " · not there now" : "") + '</span></span><span class="num">' + (l.files != null ? l.files + " files · " + sizeText(l.bytes) : "") + '</span><button type="button" class="btn sm" data-a="localrm" data-v="' + i + '">Remove</button></div>';
    });
    h += '<div class="btns"><button type="button" class="btn sm" data-a="localadd">Add a local copy…</button></div>' +
      '<div class="lab">Caches on this computer, for all projects</div><div class="kv">';
    for (const c of st.caches) h += '<span style="min-width:0"><span class="nm">' + esc(c.key) + '</span><span class="sub">' + esc(c.what) + '</span></span><span class="num">' + c.files + " files · " + sizeText(c.bytes) + '</span><button type="button" class="btn sm" data-a="clearcache" data-v="' + esc(c.key) + '"' + (c.files ? "" : " disabled") + '>Clear</button>';
    h += '</div></div><footer><span class="sp"></span><button type="button" class="btn" data-a="closesheet">Close</button><button type="button" class="btn pri" data-a="savelocal">Save</button></footer>';
  } else {
    h += '<div class="field"><span class="sub">Project folder</span><span class="mono">' + esc(st.path) + '</span></div>' +
      '<div class="field"><span class="sub">Its id</span><span class="mono">' + esc(st.id || "made when it’s first needed") + '</span></div>' +
      '<div class="field" style="align-items:start"><label class="sub" for="hosts">The NAS’s other names</label><textarea id="hosts" rows="3" class="mono" style="border:1px solid var(--line);border-radius:7px;padding:6px 8px;background:var(--surface);color:var(--fg)" placeholder="DISKSTATIONDXP = web.nas, 192.168.50.186">' + esc(S.hostsText) + '</textarea></div>' +
      '<p class="sub wrap">One line per NAS: the name its locations use, then the other names it answers to. Any of them matches a share mounted under that name.</p>' +
      '<div class="btns"><button type="button" class="btn sm" data-a="moveproject">Move the project to…</button></div>' +
      '<p class="sub wrap">Moves Project.afproject and the project’s _ folders to another folder (the SSD, say). The clips and their own files stay where they are; this folder becomes a location.</p></div>' +
      '<footer><span class="sp"></span><button type="button" class="btn" data-a="closesheet">Cancel</button><button type="button" class="btn pri" data-a="savelocs">Save</button></footer>';
  }
  el.innerHTML = h + '</div></div>';
}
async function saveLocations() {
  const hosts = textToHosts(S.hostsText);
  try {
    const problems = await invoke("save_locations", { list: S.locEdit.map((l) => ({ name: l.name, at: l.at, path: l.path, copies: !!l.copies })), hosts });
    S.sheet = null; renderSheet();
    toast(problems && problems.length ? "Saved. " + problems.join(" · ") : "Saved in Project.afproject; each location has its pointer file");
    await refresh();
  } catch (e) { showError(String(e)); }
}

// ---------- back, forward, up, new folder ----------

const NAV = {
  back: '<svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.4" aria-hidden="true"><path d="M15 6l-6 6 6 6"/></svg>',
  fwd: '<svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.4" aria-hidden="true"><path d="M9 6l6 6-6 6"/></svg>',
  up: '<svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.4" aria-hidden="true"><path d="M12 19V6M6 11l6-6 6 6"/></svg>',
  newdir: '<svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" aria-hidden="true"><path d="M3 7h6l2 2h10v10H3z"/><path d="M12 12v5M9.5 14.5h5"/></svg>',
};
// Where one has been: each place (a tab and a folder) once as it's left, as a file manager's Back and Forward have it
function place() { return S.view + "\u0000" + (S.view === "clip" ? S.clip : S.path.join("/")); }
function noteHistory() {
  const here = place();
  if (S.hist[S.histAt] === here) return;
  S.hist = S.hist.slice(0, S.histAt + 1);
  S.hist.push(here);
  S.histAt = S.hist.length - 1;
}
function goHistory(step) {
  const i = S.histAt + step;
  if (i < 0 || i >= S.hist.length) return;
  S.histAt = i;
  const [view, path] = S.hist[i].split("\u0000");
  const left = S.path.slice();
  const fromView = S.view;
  if (view === "clip") { S.clip = path; S.path = []; } else { S.view = view; S.path = path ? path.split("/") : []; }
  if (view === "clip") { S.view = view; if (DATA.moments && DATA.stills) buildClip(); else showClip(path); }
  S.sel = {}; S.anchor = null; S.q = ""; S.flat = false; S.renaming = null;
  if (view !== fromView) S.filters = {};
  // Back to a folder above: the one left, selected
  if (view === fromView && left.length > S.path.length && S.path.every((p, k) => p === left[k])) { const k = "f:" + left.slice(0, S.path.length + 1).join("/"); S.sel[k] = true; S.anchor = k; }
  if (!DATA[view] && view !== "clip") loadView(view).then(renderAll, () => {});
  renderAll();
}
function navButtons() {
  const canDir = !!FOLDER_BASE[S.view];
  const upOn = S.path.length || S.view === "clip";
  return '<span class="navs">' +
    '<button type="button" class="icon" data-a="back" aria-label="Back"' + (S.histAt > 0 ? "" : " disabled") + '>' + NAV.back + '</button>' +
    '<button type="button" class="icon" data-a="fwd" aria-label="Forward"' + (S.histAt < S.hist.length - 1 ? "" : " disabled") + '>' + NAV.fwd + '</button>' +
    '<button type="button" class="icon" data-a="up" aria-label="Enclosing folder"' + (upOn ? "" : " disabled") + '>' + NAV.up + '</button>' +
    (canDir ? '<button type="button" class="icon" data-a="newdir" aria-label="New folder" title="New folder">' + NAV.newdir + '</button>' : "") + '</span>';
}
async function newFolder() {
  if (!FOLDER_BASE[S.view]) return;
  const parent = S.path.join("/");
  try {
    const made = await invoke("new_folder", { view: S.view, parent: S.view === "footage" && parent === NOPREV ? "" : parent });
    await refresh();
    const k = "f:" + made;
    S.sel = {}; S.sel[k] = true; S.anchor = k; S.renaming = k;
    renderAll();
  } catch (e) { showError(String(e)); }
}

// ---------- a clip's moments and photos, and back to the footage ----------

// A photo's clip and time, from its name: <clip's name>_<seconds>s.<ext>
function stillFrom(name) {
  const m = /_(\d+(?:\.\d+)?)s\.(?:MP\.)?[^.]+$/i.exec(name || "");
  return m ? { stem: name.slice(0, m.index), t: +m[1] } : null;
}
function stemOf(name) { return String(name || "").replace(/\.[^.]+$/, ""); }
// The footage clip a moment, photo or export was taken from: its id (its path in the project's tree), or null
function clipOf(it) {
  if (!it) return null;
  if (it.kind === "clip") return it.id;
  if (it.link && it.link.clip) return it.link.clip;
  const f = it.kind === "still" ? stillFrom(it.name) : null;
  if (!f || !RAW_FOOTAGE) return null;
  const c = RAW_FOOTAGE.items.find((x) => stemOf(x.name) === f.stem);
  return c ? c.id : null;
}
async function needViews(list) {
  for (const v of list) if (!(v === "footage" ? RAW_FOOTAGE : DATA[v])) await loadView(v).catch(() => {});
}
// Footage, at the folder the clip is in, the clip selected
async function goFootage(id) {
  if (!id) { toast("Its clip isn’t known: it was taken from a clip outside this project, or its name changed"); return; }
  await needViews(["footage"]);
  const it = DATA.footage && DATA.footage.items.find((x) => x.id === id);
  if (!it) { toast("Its clip, " + id + ", isn’t in the project’s footage: Check on the moment shows where it was"); return; }
  S.view = "footage"; S.path = it.dir ? it.dir.split("/") : []; S.q = ""; S.flat = false; S.filters = {}; S.renaming = null;
  const k = "i:" + it.key;
  S.sel = {}; S.sel[k] = true; S.anchor = k; S.cursor = k;
  renderAll();
  const el = document.querySelector('#main [data-v="' + CSS.escape(k) + '"]');
  if (el) el.scrollIntoView({ block: "center" });
}
// The clip's own folder: its moments and photos, in the order they are in the clip
function buildClip() {
  const id = S.clip;
  const out = [];
  for (const m of (DATA.moments && DATA.moments.items) || []) {
    if (m.link && m.link.clip === id) out.push(Object.assign({}, m, { key: "m:" + m.id, dir: "", at: m.link.in || 0 }));
  }
  const clip = DATA.footage && DATA.footage.items.find((x) => x.id === id);
  const stem = clip ? stemOf(clip.name) : stemOf(id.split("/").pop());
  for (const p of (DATA.stills && DATA.stills.items) || []) {
    // By the link in the photo; one made before photos had it, by its name
    const l = p.link && p.link.clip ? p.link : null;
    const nf = stillFrom(p.name);
    const f = l ? (l.clip === id ? { t: +l.t || 0 } : null) : nf && nf.stem === stem ? nf : null;
    if (!f) continue;
    // Its time in the clip's recording, as the moments have it
    const when = clip && clip.when ? new Date(clip.when.getTime() + f.t * 1000) : p.when;
    out.push(Object.assign({}, p, { key: "p:" + p.id, dir: "", at: f.t, sub: p.form + " · at " + clock(f.t), when, date: when ? dateText(when) : p.date }));
  }
  // Its marks among them: the player's (its own file's), else the camera's; each with what was made of it
  const own = clip && clip.own && Array.isArray(clip.own.marks) ? clip.own.marks.map((m) => m.t) : null;
  const probe = clip && PROBES[clip.path];
  const times = own || (probe && probe.marks && probe.marks.times) || [];
  times.slice().sort((a, b) => a - b).forEach((t, i) => {
    const moment = out.find((x) => x.kind === "moment" && x.link && t >= x.link.in - 0.05 && t <= x.link.out + 0.05);
    const photo = out.find((x) => x.kind === "still" && Math.abs(x.at - t) < 0.6);
    const when = clip && clip.when ? new Date(clip.when.getTime() + t * 1000) : null;
    out.push({ kind: "mark", key: "k:" + i, id: "mark " + i, dir: "", at: t, t, n: i + 1, name: "Mark " + (i + 1) + " at " + clock(t),
      sub: [own ? "the player’s mark" : "the camera’s mark", moment ? "in moment " + moment.name : "no moment", photo ? "has a photo" : "no photo"].join(" · "),
      path: clip ? clip.path : "", when, date: when ? dateText(when) : "", dur: "", hasMoment: !!moment, hasPhoto: !!photo });
  });
  // In the clip's order; at one time, the mark first, then what was made of it
  const rank = { mark: 0, moment: 1, still: 2 };
  const tenth = (x) => Math.round(x.at * 10);
  out.sort((a, b) => tenth(a) - tenth(b) || rank[a.kind] - rank[b.kind]);
  DATA.clip = { dirs: [], items: out, clip: clip || null };
}
async function showClip(id) {
  if (!id) return;
  await needViews(["footage", "moments", "stills"]);
  S.view = "clip"; S.clip = id; S.path = []; S.q = ""; S.flat = false; S.filters = {}; S.renaming = null;
  S.sel = {}; S.anchor = null;
  buildClip();
  renderAll();
  $("main").scrollTop = 0;
}
function clipName() { return S.clip ? S.clip.split("/").pop() : ""; }

// ---------- the context menu, deleting, Info ----------

// What Delete takes of the selection: the project's own files and folders only. Footage's clips give their previews
// (the clip stays); Sources' clips are footage, deleted in Finder
function deletable(c) {
  if (S.view === "sources" || (!FOLDER_BASE[S.view] && S.view !== "clip")) return [];
  const list = c.selFolders.filter((f) => f !== NOPREV && !f.startsWith(NOPREV + "/")).map((f) => ({ kind: "folder", path: S.root + "/" + FOLDER_BASE[S.view] + "/" + f }));
  for (const it of c.direct) {
    if (it.kind === "mark") continue;
    if (it.kind === "clip") { if (it.preview) list.push({ kind: "preview", path: it.preview }); }
    else list.push({ kind: it.kind, path: it.path });
  }
  return list;
}
function deleteLabel(c, list) {
  const trash = S.macos ? "Move to Trash" : "Delete";
  if (S.view === "footage" && list.length && list.every((x) => x.kind === "preview")) return list.length === 1 ? "Delete the clip preview" : "Delete " + list.length + " clip previews";
  return trash;
}
async function deleteSelection() {
  const c = ctx();
  const list = deletable(c);
  if (!list.length) {
    if (c.any) toast(S.view === "sources" ? "Footage is deleted in Finder: Check then shows what pointed at it" : S.view === "footage" ? "These clips have no preview to delete: the clips themselves are deleted in Finder" : "Nothing here to delete");
    return;
  }
  try {
    const r = await invoke("delete_items", { items: list });
    const where = r.to.length ? " to " + r.to.join(" and ") : "";
    toast((r.done === 1 ? "1 moved" : r.done + " moved") + where + (r.problems.length ? ". But: " + r.problems.join(" · ") : ""));
    S.sel = {}; S.anchor = null;
    await refresh();
  } catch (e) { showError(String(e)); }
}
function canRename(c) {
  if (c.selFolders.length + c.direct.length !== 1 || S.view === "sources") return false;
  if (S.view === "footage") return c.selFolders.length === 1 && c.selFolders[0] !== NOPREV;
  if (S.view === "clip") return c.direct.length === 1 && c.direct[0].kind !== "mark";
  return true;
}

// The menu under the right button: the summary's commands, and Finder's own around them
let CMENU = null;
function closeMenu() { if (CMENU) { CMENU.remove(); CMENU = null; } }
function openMenu(x, y) {
  closeMenu();
  const c = ctx();
  const mac = S.macos;
  const k = (m, w) => '<span class="kb">' + (mac ? m : w) + '</span>';
  const b = (a, label, keys, off, v) => '<button type="button" role="menuitem" data-a="' + a + '"' + (v ? ' data-v="' + esc(v) + '"' : "") + (off ? " disabled" : "") + '><span>' + esc(label) + '</span>' + (keys || "") + '</button>';
  const parts = [];
  if (c.any) {
    const oneFolder = c.selFolders.length === 1 && !c.direct.length;
    const g = [];
    g.push(oneFolder ? b("enter", "Open", k("⌘↓", "Enter")) : b("openit", c.selItems.length && c.selItems.every((x) => x.kind === "still") ? "Open" : "Open in the player", k("⌘↓", "Enter"), !c.selItems.length));
    if (c.selItems.length === 1 && c.selItems[0].gyroflow && !oneFolder) g.push(b("opengf", "Open in Gyroflow"));
    if (c.direct.length === 1 && !c.selFolders.length) {
      const it = c.direct[0];
      if (it.kind === "clip") g.push(b("showclip", "Show marks, moments and photos", "", false, it.id));
      else if (it.kind === "mark") g.push(b("gofootage", "Go to the footage", "", false, S.clip));
      else { const id = clipOf(it); g.push(b("gofootage", "Go to the footage", "", !id, id || "")); }
    }
    if (mac && c.direct.length) g.push(b("ql", "Quick Look", k("Space", "")));
    g.push(b("revealit", mac ? "Show in Finder" : "Show in Explorer"));
    parts.push(g.join(""));
    const e = [];
    if (canRename(c)) e.push(b("renameit", "Rename", k("Return", "F2")));
    if (FOLDER_BASE[S.view]) e.push(b("newdir", "New folder", k("⇧⌘N", "Ctrl+Shift+N")));
    const del = deletable(c);
    const marksOnly = c.direct.length && !c.selFolders.length && c.direct.every((x) => x.kind === "mark");
    if (S.view !== "sources" && !marksOnly) e.push(b("trash", deleteLabel(c, del), k("⌘⌫", "Delete"), !del.length));
    if (S.view === "sources" && c.selFolders.length && c.selFolders.every((f) => !f.includes("/")) && !c.direct.length) e.push(b("removesource", "Remove from the project"));
    if (e.length) parts.push(e.join(""));
    parts.push(b("info", "Info") + b("selall", "Select all", k("⌘A", "Ctrl+A")) + b("desel", "Select none", k("Esc", "Esc")));
  } else {
    const g = [];
    if (FOLDER_BASE[S.view]) g.push(b("newdir", "New folder", k("⇧⌘N", "Ctrl+Shift+N")));
    if (S.view === "sources" && !S.path.length) g.push(b("addsource", "Add a source…"));
    g.push(b("revealhere", mac ? "Show in Finder" : "Show in Explorer"));
    parts.push(g.join(""));
    parts.push(b("selall", "Select all", k("⌘A", "Ctrl+A"), !VISIBLE.length));
  }
  const m = document.createElement("div");
  m.className = "menu cmenu";
  m.setAttribute("role", "menu");
  m.innerHTML = parts.join("<hr>");
  document.body.appendChild(m);
  const r = m.getBoundingClientRect();
  m.style.left = Math.max(4, Math.min(x, innerWidth - r.width - 4)) + "px";
  m.style.top = Math.max(4, Math.min(y, innerHeight - r.height - 4)) + "px";
  CMENU = m;
}
// The folder shown, on disk: its own folder, or a source's
function hereOnDisk() {
  if (S.view === "clip") { const cl = DATA.clip && DATA.clip.clip; return cl ? cl.path : null; }
  if (S.view === "sources") {
    const labels = (DATA.sources && DATA.sources.labels) || {};
    const i = Object.keys(labels).find((x) => labels[x] === S.path[0]);
    const l = i != null ? S.locations[+i] : null;
    return l && l.dir ? l.dir + (S.path.length > 1 ? "/" + S.path.slice(1).join("/") : "") : null;
  }
  const base = FOLDER_BASE[S.view];
  const p = S.path.filter((x) => x !== NOPREV).join("/");
  return S.root + "/" + base + (p ? "/" + p : "");
}
document.addEventListener("contextmenu", (e) => {
  if (e.target.closest("input, textarea, select") || !S.root) { closeMenu(); return; }
  if (!e.target.closest("#main")) { e.preventDefault(); closeMenu(); return; }
  e.preventDefault();
  const el = e.target.closest('[data-a="pick"]');
  if (el) {
    const v = el.dataset.v;
    // As in Finder: a right-click outside the selection selects what's under it
    if (!S.sel[v]) { S.sel = {}; S.sel[v] = true; S.anchor = v; S.cursor = v; S.checked = null; S.found = null; renderMain(); renderSide(); }
  } else if (!e.target.closest(".crumbs,.scope,.selbar") && Object.keys(S.sel).length) { S.sel = {}; S.anchor = null; renderMain(); renderSide(); }
  openMenu(e.clientX, e.clientY);
});
window.addEventListener("blur", closeMenu);
window.addEventListener("resize", closeMenu);
document.addEventListener("scroll", closeMenu, true);

// Info: what there is to know of the selection, one item in full, several or folders summed up
function kv(rows) {
  rows = rows.filter((r) => r && r[1] !== "" && r[1] != null && r[1] !== false);
  return rows.length ? '<dl class="kv">' + rows.map((r) => '<dt>' + esc(r[0]) + '</dt><dd' + (r[2] ? ' class="mono"' : "") + '>' + (r[3] ? r[1] : esc(r[1])) + '</dd>').join("") + '</dl>' : "";
}
function infoSec(title, body) { return body ? '<div class="isec"><div class="lab">' + esc(title) + '</div>' + body + '</div>' : ""; }
function stabText(st) {
  if (!st || typeof st !== "object") return null;
  const d = {};
  deriveTransform(d, st);
  return [["Stabilization", d.stab], ["Horizon", d.horizon], ["Frame", d.frame], ["Projection", d.proj],
    ["Level", st.level], ["Lens", st.lens === "corrected" ? "Corrected" : st.lens], ["Color fringes", st.chromatic === true ? "Taken out" : st.chromatic === false ? "As recorded" : null]];
}
function resText(p) {
  if (!p) return "";
  const w = p.width || (p.movie && p.movie.width), h = p.height || (p.movie && p.movie.height);
  if (!w || !h) return "";
  const fps = p.fps ? " · " + (Math.round(p.fps * 100) / 100) + " fps" : "";
  return w + " × " + h + fps;
}
function marksList(it, probe) {
  const own = it.own && Array.isArray(it.own.marks) ? it.own.marks.map((m) => m.t) : null;
  const t = own || (probe && probe.marks && probe.marks.times) || [];
  return t.length ? t.slice(0, 30).map((s) => clock(s)).join(", ") + (t.length > 30 ? " …" : "") : "";
}
function pathsHtml(list) { return list.map((p) => '<span class="mono blk">' + esc(p) + '</span>').join(""); }
function itemInfo(it) {
  let h = '<div class="ihead">' + (it.thumbPath ? '<span class="ith">' + img(it.thumbPath) + '</span>' : "") + '<b class="wrap">' + esc(it.name) + '</b><span class="sub">' + esc(it.sub || "") + '</span></div>';
  const tags = (it.tags || []).length ? '<span class="chips">' + it.tags.map((t) => '<span class="chip tag">' + esc(t) + '</span>').join("") + '</span>' : "";
  if (it.kind === "clip") {
    const probe = PROBES[it.path];
    h += infoSec("Clip", kv([["Recorded", it.date], ["Length", it.dur], ["Size", sizeText(it.size)], ["Camera", it.cam || (probe ? "Unknown" : "Reading…")],
      ["Color", it.color], ["Picture", resText(probe)], ["Shape", it.is360 ? "360°" : probe ? "Flat" : ""], ["Under water", probe && probe.water ? "Yes" : ""],
      ["Motion data", it.motion === true ? "In the clip" + (probe.mode && probe.mode !== "default" ? " (" + probe.mode + ")" : "") : it.motion === false ? "None" : ""],
      ["Tags", tags, false, true]]));
    h += infoSec("Marks and moments", kv([["Marks", it.marks ? it.marks + (marksList(it, probe) ? ": " + marksList(it, probe) : "") : "None"], ["Moments", it.moments || "None"]]));
    const tf = stabText(it.own && it.own.view);
    h += infoSec("Transform", tf ? kv(tf.concat([["Turned", it.turned ? "Yes" : ""]])) : '<p class="sub wrap">' + (it.gyroflow ? "Its Gyroflow project" : TF_DEFAULT) + (it.turned ? ", turned" : "") + '</p>');
    const conflict = Array.isArray(it.own_conflict) && it.own_conflict.length > 1;
    h += infoSec("Files", kv([["In the project", it.id, true], ["Copies", pathsHtml((it.copies || []).map((c) => c.path + "  (" + c.loc + ")")), false, true],
      ["Own file", conflict ? '<span class="chip warn">' + it.own_conflict.length + ' own files</span>' + pathsHtml(it.own_conflict) : pathsHtml(it.own_files || []) || (it.own ? "" : "None yet"), false, true],
      ["Gyroflow", it.gyroflow ? "A project beside the clip" : ""], ["Clip preview", it.preview ? pathsHtml([it.preview]) : "None", false, !!it.preview]]));
  } else if (it.kind === "moment" || it.kind === "export") {
    const l = it.link || {};
    h += infoSec(it.kind === "moment" ? "Moment" : "Export", kv([["From", l.clip ? l.clip + " · " + clock(l.in) + "–" + clock(l.out) : "", true], ["Length", it.dur], ["Taken", it.date],
      ["Look", it.kind === "export" ? it.look : ""], ["Stabilized", it.kind === "export" ? (l.stabilized ? "Yes" : "No") : ""],
      ["Stabilized as", it.kind === "moment" ? (l.alone ? "On its own" : "Part of the clip") : ""], ["Size", sizeText(it.size)], ["Tags", tags, false, true]]));
    if (it.kind === "moment") {
      const tf = stabText(l.stab);
      if (tf) h += infoSec("Transform", kv(tf));
      const rs = it.renders || {};
      h += infoSec("Renders", kv(["HDR", "SDR"].map((p) => [p, rs[p] === "current" ? "Current" : rs[p] === "stale" ? "Stale: the moment changed" : "None"])));
    }
    h += infoSec("Files", kv([["File", pathsHtml([it.path]), false, true], ["Original", it.original ? pathsHtml([it.original]) : l.clip ? "Not found in the locations" : "", false, !!it.original],
      ["Gyroflow", it.gyroflow ? "Its own project beside it" : ""]]));
  } else if (it.kind === "mark") {
    h += infoSec("Mark", kv([["At", clock(it.t) + " in the clip"], ["Recorded", it.date], ["Set by", it.sub.split(" · ")[0].replace("the ", "").replace("’s mark", "")],
      ["Moment", it.hasMoment ? "Made" : "Not yet"], ["Photo", it.hasPhoto ? "Made" : "Not yet"]]));
    h += infoSec("Files", kv([["Clip", pathsHtml([it.path]), false, true]]));
  } else {
    const l = it.link || {};
    const nf = stillFrom(it.name);
    const from = l.clip ? l.clip + " at " + clock(+l.t || 0) : nf ? nf.stem + " at " + clock(nf.t) + " (by its name)" : "";
    h += infoSec("Photo", kv([["Kind", it.form], ["From", from, true], ["Changed", it.date], ["Size", sizeText(it.size)], ["Tags", tags, false, true]]));
    h += infoSec("Files", kv([["File", pathsHtml([it.path]), false, true]]));
  }
  return h;
}
function manyInfo(c) {
  const its = c.selItems;
  const size = its.reduce((n, it) => n + (it.size || 0), 0);
  const dur = its.reduce((n, it) => n + (it.durS > 0 ? it.durS : 0), 0);
  const whens = its.map((it) => it.when).filter((d) => d && !isNaN(d)).sort((a, b) => a - b);
  const count = (f) => { const m = {}; its.forEach((it) => { const v = f(it); if (v) m[v] = (m[v] || 0) + 1; }); return Object.keys(m).sort((a, b) => m[b] - m[a]).map((k) => k + " " + m[k]).join(", "); };
  const one = c.selFolders.length === 1 && !c.direct.length;
  let h = '<div class="ihead"><b class="wrap">' + esc(c.selName) + '</b><span class="sub">' + esc(plural(its.length) + (c.selFolders.length ? ", everything inside included" : "")) + '</span></div>';
  h += infoSec("All of it", kv([["Length", dur ? clock(dur) : ""], ["Size", sizeText(size)], ["Recorded", whens.length ? dateText(whens[0]) + (whens.length > 1 ? " – " + dateText(whens[whens.length - 1]) : "") : ""],
    ["Cameras", count((it) => it.cam)], ["Color", count((it) => it.color)], ["Marks", its.reduce((n, it) => n + (it.marks || 0), 0) || ""], ["Moments", its.reduce((n, it) => n + (it.moments || 0), 0) || ""],
    ["Kinds", count((it) => it.kind === "still" ? it.form : "")], ["Tags", Array.from(new Set(its.flatMap((it) => it.tags || []))).join(", ")]]));
  if (one && FOLDER_BASE[S.view] && c.selFolders[0] !== NOPREV) h += infoSec("Files", kv([["Folder", pathsHtml([S.root + "/" + FOLDER_BASE[S.view] + "/" + c.selFolders[0]]), false, true]]));
  return h;
}
function infoHtml(c) {
  if (!c.any) {
    const all = items();
    const here = hereOnDisk();
    return '<div class="ihead"><b>' + esc(c.folderName) + '</b><span class="sub">' + esc(plural(scopeItems().length) + " here, " + plural(all.length) + " in " + c.viewName) + '</span></div>' +
      infoSec("Files", kv([["Folder", here ? pathsHtml([here]) : "", false, true], ["Project", pathsHtml([S.root]), false, true]])) +
      '<p class="sub wrap" style="padding:0 16px">Select a clip, moment, photo or folder to see what it is.</p>';
  }
  if (c.direct.length === 1 && !c.selFolders.length) { const b = clipButton(c); return itemInfo(c.direct[0]) + (b ? '<div class="isec"><div class="btns">' + b + '</div></div>' : ""); }
  return manyInfo(c);
}

// ---------- Sources, renaming, the keyboard, Quick Look ----------

// A source's line: where it is, its place in the tree, copies only, not here
function sourceOf(label) {
  const labels = (DATA.sources && DATA.sources.labels) || {};
  const i = Object.keys(labels).find((k) => labels[k] === label);
  const l = i != null ? S.locations[+i] : null;
  if (!l) return "";
  return [l.path === "." ? "the project’s own folder" : l.path, l.at ? "in the tree at " + l.at + "/" : "at the top of the tree", l.copies ? "copies only" : "", l.local ? "this computer’s" : "", l.dir ? "" : "not reachable here"].filter(Boolean).join(" · ");
}
async function addSource() {
  const r = await call("choose_location").catch(() => null);
  if (!r) return;
  const st = await call("project_settings").catch(() => null);
  if (!st) return;
  const list = st.locations.map((l) => ({ name: l.name, at: l.at, path: l.path, copies: l.copies }));
  // A project next to its footage keeps its own folder as a source
  if (st.next_to_footage) list.unshift({ name: S.name, at: "", path: ".", copies: false });
  list.push({ name: r.name, at: "", path: r.path, copies: false });
  try {
    const problems = await invoke("save_locations", { list, hosts: st.hosts || {} });
    toast("Added " + r.name + (problems.length ? ". But: " + problems.join(" · ") : ". Its place in the tree and order are in Sources settings"));
    await refresh();
  } catch (e) { showError(String(e)); }
}
async function removeSources(labels) {
  const st = await call("project_settings").catch(() => null);
  if (!st) return;
  const all = DATA.sources && DATA.sources.labels || {};
  const gone = new Set(Object.keys(all).filter((k) => labels.includes(all[k])).map((k) => S.locations[+k] && S.locations[+k].path));
  const list = st.locations.filter((l) => !gone.has(l.path)).map((l) => ({ name: l.name, at: l.at, path: l.path, copies: l.copies }));
  if (list.length === st.locations.length) { showError("That source is this computer’s own or the project’s folder: change it in Sources settings"); return; }
  try {
    await invoke("save_locations", { list, hosts: st.hosts || {} });
    S.sel = {}; S.anchor = null;
    toast("Removed from the project: its clips stay where they are");
    await refresh();
  } catch (e) { showError(String(e)); }
}

// What a rename starts from: a moment's name, a photo's or export's without its extension, a folder's
function renameStem(it) {
  if (it.kind === "folder" || it.kind === "moment") return it.name;
  const n = it.name;
  if (/\.mp\.jpg$/i.test(n)) return n.slice(0, -7);
  const i = n.lastIndexOf(".");
  return i > 0 ? n.slice(0, i) : n;
}
function startRename() {
  const keys = Object.keys(S.sel);
  if (keys.length !== 1) return;
  const it = currentItems().find((x) => keyOf(x) === keys[0]);
  if (!it) return;
  if (S.view === "sources" || (S.view === "footage" && (it.kind === "clip" || it.path === NOPREV))) {
    toast(it.kind === "folder" ? "Sources’ folders are renamed in Finder (Find moved files then follows)" : "A clip is renamed in Finder: Find moved files then follows. Here: its folders in _Clips, moments, photos, exports");
    return;
  }
  S.renaming = keys[0];
  renderMain();
}
async function commitRename(name) {
  const key = S.renaming;
  const it = currentItems().find((x) => keyOf(x) === key);
  S.renaming = null;
  if (!it || !name.trim() || name.trim() === renameStem(it)) { renderMain(); return; }
  const path = it.kind === "folder" ? S.root + "/" + FOLDER_BASE[S.view] + "/" + it.path : it.path;
  try {
    await invoke("rename_item", { view: S.view === "clip" ? (it.kind === "still" ? "stills" : "moments") : S.view, kind: it.kind, path, name: name.trim() });
    S.sel = {}; S.anchor = null;
    await refresh();
  } catch (e) { showError(String(e)); renderMain(); }
}
// The rows shown: folders and items, as VISIBLE has them
function currentItems() {
  const dir = dirKey();
  const folders = childDirs(dir).map((d) => ({ kind: "folder", path: d, name: d.split("/").pop(), dir }));
  return folders.concat(items());
}
function quickLook() {
  const c = ctx();
  const files = [];
  for (const k of Object.keys(S.sel)) {
    if (k.startsWith("f:")) continue;
    const it = items().find((x) => keyOf(x) === k);
    // A clip's preview (its flipbook, in color) when it has one: what Finder's space bar would show of _Clips
    if (it) files.push(it.kind === "clip" && it.preview ? it.preview : it.path);
  }
  if (!files.length && !c.any) return;
  invoke("quick_look", { paths: files }).catch((e) => toast(String(e)));
}
function openSelection() {
  const keys = Object.keys(S.sel);
  if (keys.length === 1 && keys[0].startsWith("f:")) { S.path = keys[0].slice(2).split("/"); S.sel = {}; S.anchor = null; renderAll(); $("main").scrollTop = 0; return; }
  const p = selectedPaths();
  if (p.length) call("open_items", { paths: p }).catch(() => {});
}
function goUp() {
  if (S.view === "clip" && !searching()) { goFootage(S.clip); return; }
  if (!S.path.length || searching()) return;
  const left = S.path[S.path.length - 1];
  S.path = S.path.slice(0, -1);
  const k = "f:" + (S.path.length ? S.path.join("/") + "/" : "") + left;
  S.sel = {}; S.sel[k] = true; S.anchor = k;
  renderAll();
}
// The grid's columns, from where its cards are
function gridColumns() {
  const cards = document.querySelectorAll("#main .card");
  if (cards.length < 2) return 1;
  const top = cards[0].getBoundingClientRect().top;
  let n = 0;
  for (const c of cards) { if (Math.abs(c.getBoundingClientRect().top - top) < 2) n++; else break; }
  return Math.max(1, n);
}
function moveTo(index, extend) {
  if (!VISIBLE.length) return;
  index = Math.max(0, Math.min(VISIBLE.length - 1, index));
  const k = VISIBLE[index];
  if (extend && S.anchor && VISIBLE.indexOf(S.anchor) >= 0) {
    const a = VISIBLE.indexOf(S.anchor), lo = Math.min(a, index), hi = Math.max(a, index);
    S.sel = {};
    VISIBLE.slice(lo, hi + 1).forEach((x) => { S.sel[x] = true; });
  } else { S.sel = {}; S.sel[k] = true; S.anchor = k; }
  S.cursor = k;
  S.checked = null; S.found = null;
  renderMain(); renderSide();
  const el = document.querySelector('#main [data-v="' + CSS.escape(k) + '"]');
  if (el) el.scrollIntoView({ block: "nearest" });
}
// The list's keys, as the system has them: Finder's on a Mac (Return renames, ⌘↓ opens, ⌘↑ goes up, space is Quick
// Look), Explorer's on Windows (Enter opens, F2 renames, Alt+↑ or Backspace goes up). true: taken
function listKeys(e) {
  const mac = S.macos;
  const cur = S.cursor && VISIBLE.indexOf(S.cursor) >= 0 ? VISIBLE.indexOf(S.cursor) : (S.anchor ? VISIBLE.indexOf(S.anchor) : -1);
  const grid = S.layout === "grid";
  const cols = grid ? gridColumns() : 1;
  const cmd = mac ? e.metaKey : e.ctrlKey;
  if (e.key === "ArrowDown" && cmd && mac) { openSelection(); return true; }
  if (e.key === "ArrowUp" && ((cmd && mac) || (e.altKey && !mac))) { goUp(); return true; }
  if (e.key === "Backspace" && !mac) { goUp(); return true; }
  if ((mac && cmd && e.key === "[") || (!mac && e.altKey && e.key === "ArrowLeft")) { goHistory(-1); return true; }
  if ((mac && cmd && e.key === "]") || (!mac && e.altKey && e.key === "ArrowRight")) { goHistory(1); return true; }
  if (cmd && e.shiftKey && (e.key === "n" || e.key === "N")) { newFolder(); return true; }
  if (e.key === "ArrowDown") { moveTo(cur < 0 ? 0 : cur + cols, e.shiftKey); return true; }
  if (e.key === "ArrowUp") { moveTo(cur < 0 ? 0 : cur - cols, e.shiftKey); return true; }
  if (grid && e.key === "ArrowRight") { moveTo(cur < 0 ? 0 : cur + 1, e.shiftKey); return true; }
  if (grid && e.key === "ArrowLeft") { moveTo(cur < 0 ? 0 : cur - 1, e.shiftKey); return true; }
  if (e.key === "Home") { moveTo(0, e.shiftKey); return true; }
  if (e.key === "End") { moveTo(VISIBLE.length - 1, e.shiftKey); return true; }
  if (e.key === "Enter") { if (mac) startRename(); else openSelection(); return true; }
  if (e.key === "F2" && !mac) { startRename(); return true; }
  if ((mac && e.key === "Backspace" && e.metaKey) || (!mac && e.key === "Delete")) { deleteSelection(); return true; }
  if ((e.key === "o" || e.key === "O") && cmd) { openSelection(); return true; }
  if (e.key === " " ) { if (mac) quickLook(); else openSelection(); return true; }
  return false;
}

// ---------- loading ----------

function applyView(view, r) {
  if (view === "footage") { RAW_FOOTAGE = { items: r.items || [], clips_dirs: r.clips_dirs || [] }; S.locations = r.locations || []; }
  else DATA[view] = { dirs: r.dirs || [], items: r.items || [] };
  mergeProbes(r.probes);
  if (view === "moments") countMoments();
  derive(view);
  if (view === "moments") derive("footage");
}
// A view: what was there last time (the app's cache) at once, then the disk looked at again
async function loadView(view) {
  if (view === "sources") view = "footage";
  const root = S.root;
  if (!(view === "footage" ? RAW_FOOTAGE : DATA[view])) {
    const c = await invoke("cached_view", { view }).catch(() => null);
    if (c && root === S.root && !(view === "footage" ? RAW_FOOTAGE : DATA[view])) { applyView(view, c); S.cachedViews[view] = true; }
  }
  S.scanning[view] = true;
  renderMain();
  try {
    const r = await call("scan", { view });
    if (r.root !== S.root) return;
    applyView(view, r);
    S.cachedViews[view] = false;
  } finally { S.scanning[view] = false; }
}
function mergeProbes(p) {
  if (!p) return false;
  let n = 0;
  for (const k of Object.keys(p.items || {})) { PROBES[k] = p.items[k]; n++; }
  if (p.version > probesVersion) probesVersion = p.version;
  probesPending = p.pending || 0;
  return n > 0;
}
async function loadProject() {
  S.info = await invoke("project").catch(() => null);
  S.name = S.info ? S.info.name : "";
  for (const k of Object.keys(DATA)) delete DATA[k];
  RAW_FOOTAGE = null; S.cachedViews = {}; S.scanning = {};
  S.path = []; S.sel = {}; S.anchor = null; S.filters = {}; S.q = ""; S.flat = false;
  renderAll();
  // What was there last time, at once; then the disk
  for (const v of ["moments", "footage"]) {
    const c = await invoke("cached_view", { view: v }).catch(() => null);
    if (c) { applyView(v, c); S.cachedViews[v] = true; }
  }
  renderAll();
  await loadView("moments").catch(() => {});
  renderAll();
  await loadView("footage").catch(() => {});
  if (!["footage", "sources", "moments"].includes(S.view)) await loadView(S.view).catch(() => {});
  renderAll();
}
async function refresh() {
  if (!S.root) return;
  S.info = await invoke("project").catch(() => S.info);
  await loadView("moments").catch(() => {});
  if (S.view === "clip") { await loadView("stills").catch(() => {}); await loadView("footage").catch(() => {}); buildClip(); }
  else await loadView(S.view === "moments" || S.view === "sources" ? "footage" : S.view).catch(() => {});
  renderAll();
}

let probeTimer = null;
async function pollProbes() {
  if (!S.root) return;
  const r = await invoke("probes_since", { since: probesVersion }).catch(() => null);
  const before = probesPending;
  if (r && (mergeProbes(r) || before !== probesPending)) {
    derive("footage"); derive("moments");
    clearTimeout(probeTimer);
    probeTimer = setTimeout(() => { buildFacets(); renderMain(); renderFilters(); }, 150);
  }
}

async function poll() {
  let s;
  try { s = await invoke("snapshot"); } catch (e) { return; }
  S.macos = s.macos;
  if (s.project !== S.root) {
    S.root = s.project;
    S.recent = await invoke("recent").catch(() => []);
    if (S.root) await loadProject(); else renderAll();
  }
  S.jobs = s.jobs;
  const finished = s.jobs.filter((j) => ["done", "stopped", "failed"].includes(j.state)).length;
  if (finished !== S.finished) {
    const more = finished > S.finished;
    S.finished = finished;
    if (more) refresh();
  }
  renderQueueOnly();
  $("ask").hidden = !s.close_asked;
}

// ---------- doing ----------

async function addJob(kind) {
  const c = ctx();
  if (!c.any) return;
  const options = kind === "photos" ? { photo: S.photoKind } : kind === "export" ? { sdr: S.exportSdr } : kind === "render" ? { profiles: ["HDR", "SDR"].filter((p) => S.profiles[p]) } : {};
  try {
    await invoke("add_job", { kind, options, only: c.only, label: c.label });
    toast("In the queue: " + c.label);
    S.tab = "queue";
    renderSide();
  } catch (e) { showError(String(e)); }
}
function selectedPaths() {
  const c = ctx();
  return c.selItems.map((it) => it.path);
}

document.addEventListener("mousedown", (e) => { if (e.shiftKey && e.target.closest('[data-a="pick"]')) e.preventDefault(); });
document.addEventListener("click", async (e) => {
  const el = e.target.closest("[data-a]");
  if (CMENU) { const inMenu = el && el.closest(".cmenu"); closeMenu(); if (!inMenu) return; if (el.disabled) return; }
  if (el && el.tagName === "INPUT" && el.dataset.a === "loccopies") return;
  if (S.menu && !(el && el.closest(".menu")) && !(el && el.dataset.a === "menu")) { S.menu = false; if (!el) { renderTop(); renderSugg(); return; } }
  if (S.suggOpen && !e.target.closest(".searchwrap")) { S.suggOpen = false; renderSugg(); }
  if (!el) {
    if (e.target.closest("#main") && !e.target.closest(".row,.card,.crumbs,.scope,.selbar") && Object.keys(S.sel).length) { S.sel = {}; S.anchor = null; renderMain(); renderSide(); }
    return;
  }
  const a = el.dataset.a, v = el.dataset.v;
  if (el.tagName === "INPUT" || el.tagName === "SELECT") return;
  switch (a) {
    case "menu": S.menu = !S.menu; if (S.menu) S.recent = await invoke("recent").catch(() => S.recent); break;
    case "closemenu": S.menu = false; break;
    case "choose": S.menu = false; await call("choose_project").catch(() => {}); return poll();
    case "openrecent": S.menu = false; await call("open_project", { path: v }).catch(() => {}); return poll();
    case "revealroot": S.menu = false; call("reveal", {}).catch(() => {}); break;
    case "view":
      S.view = v; S.path = []; S.sel = {}; S.anchor = null; S.filters = {}; S.q = ""; S.flat = false; S.renaming = null;
      if (!DATA[v]) { renderAll(); await loadView(v).catch(() => {}); }
      break;
    case "addsource": return addSource();
    case "filters": S.filtersOpen = !S.filtersOpen; break;
    case "fclear": S.filters = {}; break;
    case "fsec": S.fsec[v] = !S.fsec[v]; break;
    case "fexp": { const o = findOpt(v); const cur = S.fexp[v] !== undefined ? S.fexp[v] : (o && o.kids && (o.kids.length > 1 || o.kids.some((k) => S.filters[k.id]))); S.fexp[v] = !cur; break; }
    case "tokrm": { const f = allFacets().find((x) => x.id === v); if (f) facetLeaves(f).forEach((l) => { delete S.filters[l.id]; }); break; }
    case "suggpick": setOpt(v, true); S.q = ""; S.suggOpen = false; S.sugg = -1; renderAll(); $("q").focus(); return;
    case "suggtext": S.suggOpen = false; renderSugg(); $("q").focus(); return;
    case "scope": S.scope = v; S.sel = {}; S.anchor = null; break;
    case "flat": S.flat = true; S.scope = "here"; S.sel = {}; S.anchor = null; break;
    case "endsearch": S.q = ""; S.flat = false; S.sel = {}; S.anchor = null; break;
    case "layout": S.layout = v; break;
    case "crumb": S.path = S.path.slice(0, +v); S.sel = {}; S.anchor = null; S.flat = false; break;
    case "pick": {
      if (e.detail > 1) return;
      if (e.target.id === "rename") return;
      const multi = e.metaKey || e.ctrlKey;
      if (e.shiftKey && S.anchor && VISIBLE.indexOf(S.anchor) >= 0) {
        const i = VISIBLE.indexOf(S.anchor), j = VISIBLE.indexOf(v), lo = Math.min(i, j), hi = Math.max(i, j);
        if (!multi) S.sel = {};
        VISIBLE.slice(lo, hi + 1).forEach((k) => { S.sel[k] = true; });
      } else if (multi) { if (S.sel[v]) delete S.sel[v]; else S.sel[v] = true; S.anchor = v; }
      else { S.sel = {}; S.sel[v] = true; S.anchor = v; }
      S.checked = null; S.found = null;
      renderMain(); renderSide(); return;
    }
    case "selall": S.sel = {}; VISIBLE.forEach((k) => { S.sel[k] = true; }); S.anchor = VISIBLE[0] || null; break;
    case "desel": S.sel = {}; S.anchor = null; break;
    case "tab": S.tab = v; break;
    case "sec": S.open[v] = !S.open[v]; break;
    case "job": return addJob(v);
    case "openit": { const p = selectedPaths(); if (p.length) call("open_items", { paths: p }).catch(() => {}); return; }
    case "revealit": { const c = ctx(); const first = c.selFolders.length ? S.root + "/" + BASE[S.view] + c.selFolders[0] : c.direct[0] && c.direct[0].path; if (first) call("reveal_path", { path: first }).catch(() => {}); return; }
    case "cancel": await call("cancel_job", { id: Number(v) }).catch(() => {}); return poll();
    case "clearjobs": await call("clear_finished").catch(() => {}); return poll();
    case "keep": await call("answer_close", { quit: false }).catch(() => {}); return poll();
    case "quit": return call("answer_close", { quit: true }).catch(() => {});
    case "toastok": $("toast").hidden = true; return;
    case "sheet": return openSettings();
    case "takein": S.sheet = null; renderSheet(); return chooseTakeIn();
    case "takeplan": return planTakeIn(S.take.other);
    case "takerun": return runTakeIn();
    case "runcheck": return runCheck();
    case "back": return goHistory(-1);
    case "fwd": return goHistory(1);
    case "up": return goUp();
    case "newdir": return newFolder();
    case "enter": return openSelection();
    case "showclip": return showClip(v);
    case "gofootage": return goFootage(v);
    case "fcrumb": S.view = "footage"; S.path = v ? v.split("/") : []; S.sel = {}; S.anchor = null; S.filters = {}; if (!DATA.footage) await needViews(["footage"]); break;
    case "ql": return quickLook();
    case "renameit": { const c = ctx(); if (canRename(c)) startRename(); else toast(S.view === "sources" ? "Sources’ folders and clips are renamed in Finder" : "A clip is renamed in Finder: Find moved files then follows"); return; }
    case "trash": return deleteSelection();
    case "info": S.tab = "info"; break;
    case "revealhere": { const p = hereOnDisk(); if (p) call("reveal_path", { path: p }).catch(() => {}); return; }
    case "opengf": { const it = ctx().selItems[0]; if (it) call("open_in_gyroflow", { paths: it.kind === "clip" ? (it.copies || []).map((x) => x.path) : [it.path] }).catch(() => {}); return; }
    case "removesource": return removeSources(ctx().selFolders);
    case "delrenders": {
      const c = ctx(); if (!c.any) return;
      try { const r = await invoke("delete_renders", { moments: c.selItems.map((it) => it.id), staleOnly: v === "stale" }); toast(r.deleted + " render" + (r.deleted === 1 ? "" : "s") + " deleted" + (r.problems.length ? ". But: " + r.problems.join(" · ") : "")); await refresh(); } catch (er) { showError(String(er)); }
      return;
    }
    case "tag": { const t = S.tagDraft.trim(); if (!t) return; S.tagDraft = ""; return setTags([t], []); }
    case "untag": return setTags([], [v]);
    case "findmoved": return findMoved();
    case "applymoved": return applyMoved();
    case "scrim": if (e.target !== el) return; S.sheet = null; renderSheet(); return;
    case "closesheet": S.sheet = null; renderSheet(); return;
    case "stab": S.stab = v; renderSheet(); return;
    case "locup": { const i = +v, l = S.locEdit; [l[i - 1], l[i]] = [l[i], l[i - 1]]; renderSheet(); return; }
    case "locdn": { const i = +v, l = S.locEdit; [l[i + 1], l[i]] = [l[i], l[i + 1]]; renderSheet(); return; }
    case "locrm": S.locEdit.splice(+v, 1); renderSheet(); return;
    case "locadd": { const r = await call("choose_location").catch(() => null); if (r) { S.locEdit.push({ name: r.name, at: "", path: r.path, copies: false, st: "Chosen: " + r.dir }); renderSheet(); } return; }
    case "locself": S.locEdit.unshift({ name: S.name, at: "", path: ".", copies: false, st: "The project folder itself" }); renderSheet(); return;
    case "savelocs": return saveLocations();
    case "localadd": { const d = await call("choose_local").catch(() => null); if (d) { S.localEdit.push({ name: "On this computer", at: "", path: d }); renderSheet(); } return; }
    case "localrm": S.localEdit.splice(+v, 1); renderSheet(); return;
    case "savelocal": try { await invoke("save_local", { list: S.localEdit.map((l) => ({ name: l.name, at: l.at, path: l.path })) }); S.sheet = null; renderSheet(); toast("Saved on this computer"); await refresh(); } catch (er) { showError(String(er)); } return;
    case "clearcache": try { await invoke("clear_cache", { key: v }); toast("Cleared"); await openSettings(); } catch (er) { showError(String(er)); } return;
    case "exportjson": {
      S.menu = false; renderTop();
      try { const f = await invoke("export_library"); if (f) { toast("Exported: " + f); call("reveal_path", { path: f }).catch(() => {}); } else $("toast").hidden = true; } catch (er) { showError(String(er)); }
      return;
    }
    case "moveproject": {
      S.menu = false; S.sheet = null; renderSheet(); renderTop();
      try { const r = await invoke("move_project"); if (r) { toast("The project is now in " + r); return poll(); } } catch (er) { showError(String(er)); }
      return;
    }
    default: return;
  }
  renderAll();
});
document.addEventListener("dblclick", (e) => {
  const el = e.target.closest('[data-a="pick"]');
  if (!el) return;
  const v = el.dataset.v;
  if (v.startsWith("f:")) { S.q = ""; S.flat = false; S.path = v.slice(2).split("/"); S.sel = {}; S.anchor = null; renderAll(); $("main").scrollTop = 0; return; }
  if (S.renaming) return;
  const it = items().find((x) => keyOf(x) === v);
  if (it) { S.sel = {}; S.sel[v] = true; S.anchor = v; renderMain(); renderSide(); call("open_items", { paths: [it.path] }).catch(() => {}); }
});
document.addEventListener("input", (e) => {
  const t = e.target;
  if (t.id === "q") { S.q = t.value; S.suggOpen = true; S.sugg = -1; S.sel = {}; S.anchor = null; renderMain(); renderSide(); renderFilters(); renderSugg(); }
  else if (t.dataset.l) { S.locEdit[+t.dataset.i][t.dataset.l] = t.value; }
  else if (t.id === "hosts") { S.hostsText = t.value; }
  else if (t.id === "takeplace") { S.takePlace = t.value; }
  else if (t.id === "tagdraft") { S.tagDraft = t.value; }
});
document.addEventListener("change", (e) => {
  const el = e.target, a = el.dataset.a, v = el.dataset.v;
  if (!a) return;
  if (a === "loccopies") { S.locEdit[+v].copies = el.checked; return; }
  if (a === "prof") { S.profiles[v] = el.checked; renderSide(); return; }
  if (a === "fopt") setOpt(v, el.checked);
  else if (a === "pkind") S.photoKind = el.value;
  else if (a === "look") S.exportSdr = el.value === "sdr";
  else if (a === "layout_set") { call("set_setting", { key: "clips_layout", value: el.value }).then(() => invoke("project")).then((i) => { S.info = i; renderSide(); }).catch(() => {}); return; }
  else if (a === "setting") { call("set_setting", { key: v, value: el.checked }).then(() => invoke("project")).then((i) => { S.info = i; renderSide(); }).catch(() => {}); return; }
  else return;
  renderAll();
});
document.addEventListener("keydown", (e) => {
  const typing = /^(INPUT|TEXTAREA|SELECT)$/.test(e.target.tagName);
  if (e.target.id === "tagdraft" && e.key === "Enter") { e.preventDefault(); const t = S.tagDraft.trim(); if (t) { S.tagDraft = ""; setTags([t], []); } return; }
  if (e.target.id === "q") {
    const list = suggestions(), max = list.length;
    if (e.key === "ArrowDown" && S.suggOpen) { e.preventDefault(); S.sugg = Math.min(max, S.sugg + 1); renderSugg(); }
    else if (e.key === "ArrowUp" && S.suggOpen) { e.preventDefault(); S.sugg = Math.max(-1, S.sugg - 1); renderSugg(); }
    else if (e.key === "Enter") { e.preventDefault(); if (S.suggOpen && S.sugg >= 0 && S.sugg < max) { setOpt(list[S.sugg].id, true); S.q = ""; } S.suggOpen = false; S.sugg = -1; renderAll(); $("q").focus(); }
    else if (e.key === "Backspace" && e.target.value === "") { const t = activeTokens(); if (t.length) { facetLeaves(t[t.length - 1].f).forEach((l) => { delete S.filters[l.id]; }); renderAll(); $("q").focus(); } }
    else if (e.key === "Escape" && S.suggOpen) { S.suggOpen = false; renderSugg(); }
    return;
  }
  if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === "a" && !typing && S.root) { e.preventDefault(); S.sel = {}; VISIBLE.forEach((k) => { S.sel[k] = true; }); S.anchor = VISIBLE[0] || null; renderMain(); renderSide(); return; }
  if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === "f" && S.root) { e.preventDefault(); const q = $("q"); if (q) q.focus(); return; }
  if (CMENU && e.key === "Escape") { closeMenu(); return; }
  if (CMENU) closeMenu();
  if (e.key === "Escape" && S.sheet) { S.sheet = null; renderSheet(); return; }
  if (e.key === "Escape" && S.menu) { S.menu = false; renderTop(); return; }
  if (e.target.id === "rename") {
    if (e.key === "Enter") { e.preventDefault(); commitRename(e.target.value); }
    else if (e.key === "Escape") { e.preventDefault(); S.renaming = null; renderMain(); }
    return;
  }
  if (e.key === "Escape" && !typing && Object.keys(S.sel).length) { S.sel = {}; S.anchor = null; renderMain(); renderSide(); return; }
  if (typing || S.sheet || !S.root) return;
  if (listKeys(e)) e.preventDefault();
});

// A Project.afproject (or a folder of the project) dropped on the window
try {
  window.__TAURI__.webview.getCurrentWebview().onDragDropEvent(async (ev) => {
    if (ev.payload.type === "drop" && ev.payload.paths.length) {
      await call("open_project", { path: ev.payload.paths[0] }).catch(() => {});
      poll();
    }
  });
} catch (e) { /* no drops then */ }

(async () => {
  try { await invoke("engine"); } catch (e) { $("engine").textContent = String(e); $("engine").hidden = false; }
  S.recent = await invoke("recent").catch(() => []);
  await poll();
  if (!S.root) renderAll();
  setInterval(poll, 600);
  setInterval(pollProbes, 900);
})();
