"use strict";
// Loading: the views read, the project opened, the regular look at the app.
// One of the window's scripts that were app.js: index.html loads them in this order before app.js, as classic
// scripts sharing their top-level names, so each uses the others' when called (only earlier ones at load).

// ---------- loading ----------

function applyView(view, r) {
  // As the disk has it; what's shown is that with the changes still being written over it (writes.js)
  CONFIRMED[view] = r;
  mergeProbes(r.probes);
  showView(view);
}
function showView(view) {
  const r = Writes.over(view, CONFIRMED[view]);
  if (view === "footage") { RAW_FOOTAGE = { items: r.items || [], clips_dirs: r.clips_dirs || [], dirs: r.dirs || [], links: r.links || [] }; S.locations = r.locations || []; }
  else DATA[view] = rooted(view, { dirs: r.dirs || [], items: r.items || [] });
  const had = MOMENTS_OF;
  if (view === "moments") countMoments();
  derive(view);
  // Albums show the moments, photos and exports too. The clips' counts of moments: only the clips whose count
  // changed are made again (changes.js), when they're few; the albums' items are made again from what was read
  if (view === "moments") {
    const ids = Array.from(new Set(Object.keys(had).concat(Object.keys(MOMENTS_OF)))).filter((id) => had[id] !== MOMENTS_OF[id]);
    if (ids.length <= 50 && CONFIRMED.footage && showItems("footage", ids.filter((id) => placeOf("footage", id) >= 0))) albumsAgain();
    else derive("footage");
  } else if (view !== "footage") albumsAgain();
}
// The Albums view alone made again from the scans as shown (its own items derived; Originals untouched)
function albumsAgain() {
  if (!RAW_FOOTAGE) return;
  DATA.footage = albumItems();
  derive1("footage");
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
  // What's written while it's read (and may not be in its answer) is put into it: writes.js
  const reading = Writes.reading(view);
  try {
    const r = await call("scan", { view });
    if (r.root !== S.root) return;
    applyView(view, Writes.read(reading, r));
    S.cachedViews[view] = false;
  } finally { S.scanning[view] = false; Writes.read(reading, null); }
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
  // A read of the views asked meanwhile (the catalog's notice) follows this one, never alongside it: refreshSoon
  S.loading = (S.loading || 0) + 1;
  try { await loadProjectNow(); } finally {
    S.loading--;
    if (!S.loading && S.refreshAgain && !S.refreshing) { S.refreshAgain = false; refreshSoon(); }
  }
}
async function loadProjectNow() {
  S.info = await invoke("project").catch(() => null);
  S.name = S.info ? S.info.name : "";
  S.tagStores = (S.info && S.info.tag_stores) || [];
  for (const k of Object.keys(DATA)) delete DATA[k];
  for (const k of Object.keys(CONFIRMED)) delete CONFIRMED[k];
  Writes.reset();
  RAW_FOOTAGE = null; S.cachedViews = {}; S.scanning = {};
  S.path = []; S.sel = {}; S.anchor = null; S.filters = {}; S.q = ""; S.flat = false;
  resetHistory();
  renderAll();
  // What was there last time, at once; then the disk
  for (const v of ["moments", "footage", "stills", "exports"]) {
    const c = await invoke("cached_view", { view: v }).catch(() => null);
    if (c) { applyView(v, c); S.cachedViews[v] = true; }
  }
  renderAll();
  await loadView("moments").catch(() => {});
  renderAll();
  await loadView("footage").catch(() => {});
  renderAll();
  // Photos and exports: items of Albums
  await loadView("stills").catch(() => {});
  await loadView("exports").catch(() => {});
  renderAll();
}
async function refresh() {
  if (!S.root) return;
  S.info = await invoke("project").catch(() => S.info);
  await loadView("moments").catch(() => {});
  await loadView("footage").catch(() => {});
  if (S.view !== "sources") { await loadView("stills").catch(() => {}); if (S.view === "footage") await loadView("exports").catch(() => {}); }
  if (S.view === "clip") buildClip();
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
  // What the project's catalog is doing (made the first time, read again, folded): said once as it changes
  // Links whose original isn't where they say: the strip on top, at once
  if ((s.missing ?? null) !== (S.missing ?? null)) {
    S.missing = s.missing ?? null;
    const list = S.missing ? await invoke("missing_links").catch(() => []) : [];
    MISSING = new Set(list.map((x) => x.file)); S.missingList = list;
    buildFacets(); renderMain(); renderFilters();
  }
  if ((s.catalog || "") !== (S.catalogStatus || "")) { S.catalogStatus = s.catalog || ""; if (S.catalogStatus) toast(S.catalogStatus); }
  if (s.project !== S.root) {
    S.changes = s.changes;
    S.root = s.project;
    // The tasks done before it opened are in what it reads now: only those done after read it again (below)
    S.finished = s.jobs.filter((j) => ["done", "stopped", "failed"].includes(j.state)).length;
    S.recent = await invoke("recent").catch(() => []);
    if (S.root) await loadProject(); else renderAll();
  }
  // Files changed in the project's folder or its locations (the player saving marks, moments, photos; Finder): read
  // again once they've settled; while a batch runs (changes all the time: clip previews coming one by one), every 4 s
  // (what the project's catalog was told comes at once, as an event: refreshSoon below)
  const batchRuns = s.jobs.some((j) => j.state === "running");
  const due = batchRuns ? Date.now() - (S.lastRefresh || 0) > 4000 : s.quiet_ms > 800;
  if (S.root && s.changes !== S.changes && due) catalogChanged(s.changes);
  // No project open: the list of those opened before looked at again every few seconds (a share connected or taken
  // away meanwhile: Offline and Connect come and go)
  if (!S.root && Date.now() - (S.recentAt || 0) > 3000) {
    S.recentAt = Date.now();
    const r = await invoke("recent").catch(() => null);
    if (r && JSON.stringify(r) !== JSON.stringify(S.recent)) { S.recent = r; renderAll(); }
  }
  S.jobs = s.jobs;
  const finished = s.jobs.filter((j) => ["done", "stopped", "failed"].includes(j.state)).length;
  if (finished !== S.finished) {
    const more = finished > S.finished;
    S.finished = finished;
    if (more) refresh();
  }
  renderQueueOnly();
  shareTick(); // share.js
  lensTick(); // lenses.js
  $("ask").hidden = !s.close_asked;
  closing(!!s.close_waits);
  renderMoving(s.moving);
}

// What the project's catalog knows changed (the Library's own commands, the player's notes, a task's files as they
// land): the view read again at once. One read at a time; another asked meanwhile follows it
function refreshSoon(n) {
  if (!S.root) return;
  if (n !== undefined) { if (n === S.changes) return; S.changes = n; }
  if (S.refreshing || S.loading) { S.refreshAgain = true; return; }
  S.refreshing = true;
  S.lastRefresh = Date.now();
  refresh().finally(() => {
    S.refreshing = false;
    if (S.refreshAgain) { S.refreshAgain = false; refreshSoon(); }
  });
}
// (the items that changed, not the views read again: changes.js)
if (window.__TAURI__.event) window.__TAURI__.event.listen("catalog-changed", (e) => catalogChanged(e.payload));

// Move the project…: how far the copy is (a move on one disk is a rename, done at once)
function renderMoving(m) {
  $("moving").hidden = !m;
  if (!m) return;
  $("movingTo").textContent = "To " + m.to;
  const pct = m.total > 0 ? Math.min(100, (m.done / m.total) * 100) : 0;
  $("movingBar").firstElementChild.style.width = pct.toFixed(1) + "%";
  $("movingBar").setAttribute("aria-valuenow", Math.round(pct));
  $("movingText").textContent = m.total > 0
    ? (m.done > 0 ? sizeText(m.done) : "0 B") + " of " + sizeText(m.total) + " copied (" + Math.floor(pct) + " %)" + (m.current ? " · " + m.current : "") + ". Another disk: each folder is copied, then removed here."
    : "Moving…";
}
