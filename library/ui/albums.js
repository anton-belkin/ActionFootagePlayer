"use strict";
// Albums (docs/albums-photos-share-concept.md, "Albums"; the app's src/albums.rs): the view that was Footage. An album
// is a folder of links in the project's folder: `_Clips/` holds the user's own ("Your albums", the view's top), what's
// anywhere else in the project's folder is under Other folders (Generated among them). An item is a link, shown with
// what's known of its original. Loaded before app.js, whose state (S, DATA, RAW_FOOTAGE, PROBES) and helpers it uses
// when called; app.js calls in at the places marked "albums.js".
//
// What an album holds comes from ALBUM_SOURCES: each a function giving items with `rdir`, the folder (in the
// project's folder) the item's link is in, and `kind`. Today: the clip links. Moments and photos are to join them here
// (another source each), and a view inside Albums that isn't a folder (a timeline, a map) is one more entry of
// ALBUM_VIEWS.

// The kinds an album can hold, as its filter names them
const ALBUM_KINDS = [["clip", "Clips"], ["moment", "Moments"], ["still", "Photos"]];
const ALBUM_SOURCES = [albumClips];
// Views inside Albums that aren't folders: [id, label, a function drawing it]. None yet
const ALBUM_VIEWS = [];
// The log curves a thumbnail is turned to normal colour from (the player's luts/<curve>_hlg.cube)
const ALBUM_CURVES = ["dlogm", "dlog", "dlog2", "gplog", "gplog2", "ilog"];

// Every clip link of the project (the scan's `links`), as its clip with the link's own place: one item a link, so a
// clip in two albums is two items. A link whose clip isn't among the footage isn't shown (the strip on top counts it)
function albumClips() {
  const byId = {};
  for (const it of RAW_FOOTAGE.items) byId[it.id] = it;
  const out = [];
  for (const l of RAW_FOOTAGE.links || []) {
    const clip = byId[l.clip];
    if (!clip) continue;
    const i = l.rel.lastIndexOf("/");
    out.push(Object.assign({}, clip, { key: "a:" + l.rel, tdir: clip.dir, rdir: i < 0 ? "" : l.rel.slice(0, i), preview: l.path, link_file: l.path, draft: l.draft || undefined }));
  }
  return out;
}
// The Albums view from the scan: its folders, and every source's items at the folders the view shows them in
function albumItems() {
  const items = [];
  for (const src of ALBUM_SOURCES) for (const it of src()) { it.dir = shownDir("footage", it.rdir || ""); items.push(it); }
  const dirs = rooted("footage", { dirs: RAW_FOOTAGE.clips_dirs || [], items: [] }).dirs;
  if (items.some((it) => it.dir === OTHER || it.dir.startsWith(OTHER + "/")) && !dirs.includes(OTHER)) dirs.push(OTHER);
  return { dirs, items };
}
// How many albums each clip is in: its path in the tree -> the count
function albumCounts() {
  const n = {};
  for (const l of (RAW_FOOTAGE && RAW_FOOTAGE.links) || []) n[l.clip] = (n[l.clip] || 0) + 1;
  return n;
}
// The filter by kind, among the filters, once an album holds more than one kind
function albumFacets(items, sections) {
  const kinds = ALBUM_KINDS.filter((k) => items.some((it) => it.kind === k[0]));
  if (kinds.length < 2) return;
  if (S.fsec.alb === undefined) S.fsec.alb = true;
  sections.unshift({ id: "alb", title: "Kind", facets: [{ id: "knd", title: "Kind", opts: kinds.map((k) => L("knd:" + k[0], k[1], (it) => it.kind === k[0])) }] });
}
// A clip's log curve when it's known already: chosen in its own file, else as the camera wrote it (the probe's answer)
function clipCurve(it) {
  const own = it.own && it.own.source && typeof it.own.source === "object" ? it.own.source.curve : null;
  if (own && own !== "auto") return own;
  const p = PROBES[it.path];
  return p && ALBUM_CURVES.includes(p.color) ? p.color : null;
}

// ---------- Add to an album…, Move to an album… ----------

// The user's albums, as the view shows them ("Egypt", "Egypt/Dives"): the folders of _Clips
function albumList() {
  return ((DATA.footage && DATA.footage.dirs) || []).filter((d) => d !== OTHER && !d.startsWith(OTHER + "/")).sort((a, b) => a.localeCompare(b));
}
// The menu's commands on a selection of clips, and of an album's items
function albumMenu(c, b) {
  const clips = (S.view === "footage" || S.view === "sources") ? c.selItems.filter((x) => x.kind === "clip").length : 0;
  if (!clips) return "";
  const links = S.view === "footage" ? c.direct.filter((x) => x.link_file).length : 0;
  return b("albumadd", S.view === "footage" ? "Add to another album…" : "Add to an album…") + (links ? b("albummove", "Move to an album…") : "");
}
// The same on the bar above the list, once something is selected
function albumBar(c) {
  if (!(S.view === "footage" || S.view === "sources") || !c.selItems.some((x) => x.kind === "clip")) return "";
  const links = S.view === "footage" && c.direct.some((x) => x.link_file);
  return '<button type="button" class="btn sm" data-a="albumadd">' + (S.view === "footage" ? "Add to another album…" : "Add to an album…") + '</button>' +
    (links ? '<button type="button" class="btn sm" data-a="albummove">Move to an album…</button>' : "");
}
async function askAlbum(mode) {
  const c = ctx();
  const clips = c.selItems.filter((x) => x.kind === "clip");
  const files = c.direct.filter((x) => x.link_file).map((x) => x.link_file);
  if (mode === "move" ? !files.length : !clips.length) return;
  if (!DATA.footage) await needViews(["footage"]);
  const list = albumList();
  // The album open now, when one is (Albums): the likely one isn't it, so none chosen; else the last one used
  const last = remembered("afl.album", "");
  const here = S.view === "footage" ? S.path.join("/") : "";
  S.album = { mode, clips, files, to: list.includes(last) && last !== here ? last : "", name: "", busy: false };
  S.sheet = "album";
  renderSheet();
  const el = $("albumname");
  if (el && !list.length) el.focus();
}
function renderAlbumSheet() {
  const a = S.album;
  if (!a) return "";
  const n = a.mode === "move" ? a.files.length : a.clips.length;
  const what = a.mode === "move" ? (n === 1 ? "1 item" : n + " items") : (n === 1 ? "1 clip" : n + " clips");
  const list = albumList();
  const target = a.name.trim() || a.to;
  let h = '<div class="scrim" data-a="scrim"><div class="sheet narrow albumsheet" role="dialog" aria-modal="true" aria-labelledby="albumTitle">' +
    '<header><h2 id="albumTitle">' + (a.mode === "move" ? "Move " : "Add ") + what + ' to an album</h2>' +
    '<span class="sub wrap">' + (a.mode === "move" ? "The links move; the originals stay where they are." : "A link to each original, with its thumbnail: made at once. The originals stay where they are.") + '</span></header><div class="sb">';
  if (list.length) {
    h += '<div class="lab">Your albums</div><div class="albumlist" role="listbox" aria-label="Your albums">' + list.map((d) => {
      const depth = d.split("/").length - 1;
      return '<button type="button" role="option" class="albumrow' + (a.to === d && !a.name.trim() ? " on" : "") + '" aria-selected="' + (a.to === d && !a.name.trim()) + '" data-a="albumpick" data-v="' + esc(d) + '" style="padding-left:' + (10 + depth * 16) + 'px">' +
        ICON.fsm + '<span>' + esc(d.split("/").pop()) + '</span><span class="sub">' + DATA.footage.items.filter((it) => it.dir === d || it.dir.startsWith(d + "/")).length + '</span></button>';
    }).join("") + '</div>';
  }
  h += '<div class="field"><label class="sub" for="albumname">' + (list.length ? "Or a new album" : "A new album") + '</label>' +
    '<input id="albumname" type="text" value="' + esc(a.name) + '" placeholder="Its name (Egypt/Dives: one inside another)" autocomplete="off" spellcheck="false"></div>';
  h += '</div><footer><span class="sp"></span><button type="button" class="btn" data-a="closesheet">Cancel</button>' +
    '<button type="button" class="btn pri" id="albumgo" data-a="albumgo"' + (target && !a.busy ? "" : " disabled") + '>' + (a.mode === "move" ? "Move" : "Add") + (target ? " to " + esc(target.split("/").pop()) : "") + '</button></footer>';
  return h + '</div></div>';
}
// Typing a new album's name: the button follows, the field keeps the cursor
function albumTyped(v) {
  if (!S.album) return;
  S.album.name = v;
  const b = $("albumgo"), target = v.trim() || S.album.to;
  if (b) { b.disabled = !target || S.album.busy; b.textContent = (S.album.mode === "move" ? "Move" : "Add") + (target ? " to " + target.split("/").pop() : ""); }
  document.querySelectorAll(".albumrow").forEach((x) => { const on = !v.trim() && x.dataset.v === S.album.to; x.classList.toggle("on", on); x.setAttribute("aria-selected", on); });
}
async function albumGo() {
  const a = S.album;
  if (!a || a.busy) return;
  const shown = (a.name.trim() || a.to).split("/").map((p) => p.trim()).filter(Boolean).join("/");
  if (!shown) return;
  if (shown === OTHER || shown.startsWith(OTHER + "/")) { showError("That name is taken by the view: choose another"); return; }
  const album = diskRel(shown, "footage");
  a.busy = true; renderSheet();
  try {
    if (a.mode === "move") {
      const r = await invoke("album_move", { album, files: a.files });
      toast((r.moved === 1 ? "1 moved" : r.moved + " moved") + " to " + shown + (r.problems.length ? ". But: " + r.problems.join(" · ") : ""));
    } else {
      const clips = a.clips.map((it) => { const p = PROBES[it.path]; return { tree: it.id, path: it.path, thumb: it.thumb || null, size: it.size, created: p && p.movie && p.movie.created ? Math.round(p.movie.created) : null, curve: clipCurve(it) }; });
      const r = await invoke("album_add", { album, clips });
      const made = r.made.length;
      toast((made === 1 ? "1 clip" : made + " clips") + " added to " + shown + (made ? " in " + (r.ms < 950 ? Math.max(1, Math.round(r.ms / 10) * 10) + " ms" : (r.ms / 1000).toFixed(1) + " s") : "") +
        (r.already ? " · " + r.already + (r.already === 1 ? " was" : " were") + " there already" : "") + (r.failed.length ? ". But " + r.failed.length + " couldn’t be made: " + r.failed[0][1] : ""));
    }
    remember("afl.album", shown);
    S.sheet = null; S.album = null; renderSheet();
    S.sel = {}; S.anchor = null;
    await refresh();
  } catch (e) { a.busy = false; renderSheet(); showError(String(e)); }
}
