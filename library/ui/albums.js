"use strict";
// Albums (docs/albums-photos-share-concept.md, "Albums"; the app's src/albums.rs): the view that was Footage. An album
// is a folder of links in the project's folder: `_Clips/` holds the user's own ("Your albums", the view's top), what's
// anywhere else in the project's folder is under Other folders (Generated among them). An item is a link, shown with
// what's known of its original. Loaded before app.js, whose state (S, DATA, RAW_FOOTAGE, PROBES) and helpers it uses
// when called; app.js calls in at the places marked "albums.js".
//
// What an album holds comes from ALBUM_SOURCES: each a function giving items with `rdir`, the folder (in the
// project's folder) the item's file is in, and `kind`: the clip links, the moments, the photos (a link file
// `.afplink.jpg`, or a rendered one) and the exports. A view inside Albums that isn't a folder (a timeline, a map) is
// one more entry of ALBUM_VIEWS.
//
// An album is one name over the project's own folders: `_Clips/Egypt`, `_Moments/Egypt` and `_Stills/Egypt` are the
// album Egypt (the concept: "_Clips, _Moments, _Stills: shown as albums, as they are: nothing moves"). What's put in
// an album from here goes to `_Clips/<album>`, whatever its kind; `Generated` (in each of the three, and the
// project's own) is where new items land, shown beside the albums. Anything else in the project's folder, `_Exports`
// among it, is under Other folders at its path.

// The kinds an album can hold, as its filter names them
const ALBUM_KINDS = [["clip", "Clips"], ["moment", "Moments"], ["still", "Photos"], ["export", "Exports"]];
const ALBUM_SOURCES = [albumClips, albumMoments, albumPhotos, albumExports];
// The project's folders whose folders are albums, and the one new items land in
const ALBUM_ROOTS = ["_Clips", "_Moments", "_Stills"];
const GENERATED = "Generated";
// An album as shown -> its folders in the project's folder (one to three), filled by albumItems()
let ALBUM_REAL = {};

// A folder's path in the project's folder -> the album it's shown as
function albumShown(real) {
  const lr = real.toLowerCase();
  for (const r of ALBUM_ROOTS) {
    const l = r.toLowerCase();
    if (lr === l) return "";
    if (lr.startsWith(l + "/")) return real.slice(r.length + 1);
  }
  if (lr === GENERATED.toLowerCase() || lr.startsWith(GENERATED.toLowerCase() + "/")) return real;
  return real === "" ? OTHER : OTHER + "/" + real;
}
// …and back: every folder on disk an album is (the one in _Clips when it has none yet: where it would be made)
function albumRealDirs(shown) {
  if (shown === OTHER) return [""];
  if (shown.startsWith(OTHER + "/")) return [shown.slice(OTHER.length + 1)];
  const real = ALBUM_REAL[shown];
  return real && real.length ? real : [diskRel(shown, "footage")];
}
// Whether a folder shown is an album of the user's (not Generated, not Other folders)
function userAlbum(d) { return !!d && d !== OTHER && !d.startsWith(OTHER + "/") && d !== GENERATED && !d.startsWith(GENERATED + "/"); }
// Views inside Albums that aren't folders: [id, label, draw, items, meta] (albumviews.js): Timeline, Map, the media types
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
// The moments, photos and exports, as their own scans have them (DATA.moments, .stills, .exports), each at the folder
// its file is in
function albumOf(view, prefix) {
  return ((DATA[view] && DATA[view].items) || []).map((it) => Object.assign({}, it, { key: prefix + it.id, rdir: it.rdir != null ? it.rdir : (it.id.lastIndexOf("/") < 0 ? "" : it.id.slice(0, it.id.lastIndexOf("/"))) }));
}
function albumMoments() { return albumOf("moments", "m:"); }
function albumPhotos() { return albumOf("stills", "p:"); }
function albumExports() { return albumOf("exports", "e:"); }
// The Albums view from the scans: its folders, and every source's items at the folders the view shows them in
function albumItems() {
  const items = [];
  const real = {};
  const dirs = new Set();
  // A folder on disk, and the folders above it, as the albums they are
  const note = (rdir) => {
    let d = rdir;
    while (d) {
      const sd = albumShown(d);
      if (sd) { dirs.add(sd); if (sd !== OTHER && !sd.startsWith(OTHER + "/")) { (real[sd] = real[sd] || []); if (!real[sd].includes(d)) real[sd].push(d); } }
      const i = d.lastIndexOf("/");
      d = i < 0 ? "" : d.slice(0, i);
    }
  };
  for (const d of RAW_FOOTAGE.clips_dirs || []) note(d);
  for (const src of ALBUM_SOURCES) for (const it of src()) { it.dir = albumShown(it.rdir || ""); note(it.rdir || ""); if (it.dir === OTHER) dirs.add(OTHER); items.push(it); }
  for (const d of Array.from(dirs)) if (d.startsWith(OTHER + "/")) dirs.add(OTHER);
  // The one in _Clips first: where what's added goes, and what a folder's name stands for
  for (const k of Object.keys(real)) real[k].sort((a, b) => ALBUM_ROOTS.findIndex((r) => a.startsWith(r)) - ALBUM_ROOTS.findIndex((r) => b.startsWith(r)));
  ALBUM_REAL = real;
  // The user's albums, then Generated, then Other folders
  const rank = (d) => (d === OTHER || d.startsWith(OTHER + "/")) ? 2 : (d === GENERATED || d.startsWith(GENERATED + "/")) ? 1 : 0;
  return { dirs: Array.from(dirs).sort((a, b) => rank(a) - rank(b) || a.localeCompare(b)), items };
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
  return ((DATA.footage && DATA.footage.dirs) || []).filter(userAlbum).sort((a, b) => a.localeCompare(b));
}
// What of a selection goes into albums, and how: clips by a new link each (Originals' clips, and an album's, whose
// link is to the same clip); moments and photos that are links by a copy of the link's file; an album's own files
// (a clip's link, a moment, a photo: rendered ones too) by moving them; and what's taken out of an album
function albumSel(c) {
  const inAlbums = S.view === "footage";
  const clips = (inAlbums || S.view === "sources") ? c.selItems.filter((x) => x.kind === "clip") : [];
  const own = inAlbums ? c.direct.filter((x) => x.kind === "moment" || x.kind === "still") : [];
  const copies = own.filter((x) => x.kind === "moment" ? /\.afplink\.mp4$/i.test(x.path) : x.linked).map((x) => x.path);
  const files = inAlbums ? c.direct.filter((x) => x.link_file).map((x) => x.link_file).concat(own.map((x) => x.path)) : [];
  // Out of an album of the user's: a clip's link goes (its original stays), a moment or a photo back to Generated
  const out = inAlbums ? c.direct.filter((x) => userAlbum(x.dir) && (x.link_file || x.kind === "moment" || x.kind === "still")) : [];
  return { clips, copies, files, out, n: clips.length + copies.length };
}
function albumMenu(c, b) {
  const a = albumSel(c);
  return (a.n ? b("albumadd", S.view === "footage" && !(typeof albumViewOpen === "function" && albumViewOpen()) ? "Add to another album…" : "Add to an album…") : "") + (a.files.length ? b("albummove", "Move to an album…") : "") +
    (a.out.length && a.out.some((x) => x.kind !== "clip") ? b("albumout", "Take out of the album") : "");
}
// The same on the bar above the list, once something is selected
function albumBar(c) {
  const a = albumSel(c);
  return (a.n ? '<button type="button" class="btn sm" data-a="albumadd">' + (S.view === "footage" && !(typeof albumViewOpen === "function" && albumViewOpen()) ? "Add to another album…" : "Add to an album…") + '</button>' : "") +
    (a.files.length ? '<button type="button" class="btn sm" data-a="albummove">Move to an album…</button>' : "") +
    (a.out.length ? '<button type="button" class="btn sm" data-a="albumout">Take out of the album</button>' : "");
}
async function askAlbum(mode) {
  const c = ctx();
  const a = albumSel(c);
  if (mode === "move" ? !a.files.length : !a.n) return;
  if (!DATA.footage) await needViews(["footage"]);
  const list = albumList();
  // The album open now, when one is (Albums): the likely one isn't it, so none chosen; else the last one used
  const last = remembered("afl.album", "");
  const here = S.view === "footage" ? S.path.join("/") : "";
  S.album = { mode, clips: a.clips, copies: a.copies, files: a.files, to: list.includes(last) && last !== here ? last : "", name: "", busy: false };
  S.sheet = "album";
  renderSheet();
  const el = $("albumname");
  if (el && !list.length) el.focus();
}
// Out of the album: a clip's link removed (the clip is in Originals), moments and photos moved back to Generated
// (theirs: _Moments/Generated, _Stills/Generated), where what's in no album is
async function albumOut() {
  const a = albumSel(ctx());
  if (!a.out.length) return;
  const links = a.out.filter((x) => x.link_file).map((x) => ({ kind: "preview", path: x.link_file }));
  const problems = [];
  let n = 0;
  try {
    // The links: gone from the album at once, moved to the Trash after (writes.js)
    S.sel = {}; S.anchor = null;
    if (links.length) {
      const done = await Writes.run("delete", [], { items: links, names: a.out.filter((x) => x.link_file).map((x) => "the link of " + x.name) });
      if (done.answer) n += done.answer.done || 0;
    }
    const read = [];
    for (const [kind, album] of [["moment", "_Moments/" + GENERATED], ["still", "_Stills/" + GENERATED]]) {
      const files = a.out.filter((x) => x.kind === kind).map((x) => x.path);
      if (!files.length) continue;
      const r = await invoke("album_move", { album, files });
      n += r.moved; problems.push(...r.problems);
      read.push(kind === "moment" ? "moments" : "stills");
    }
    toast(n + (n === 1 ? " item" : " items") + " taken out of the album" + (a.out.some((x) => x.kind !== "clip") ? ": moments and photos are in Generated" : ": the originals are untouched") + (problems.length ? ". But: " + problems.join(" · ") : ""));
    await albumRead(read);
  } catch (e) { showError(String(e)); }
}
// After an album's change that the page can't show by itself (new links, files moved): only the views it touched
// are read again, never the whole project (docs/library-responsiveness.md)
async function albumRead(views) {
  if (!views.length) return;
  await Promise.all(views.map((v) => loadView(v).catch(() => {})));
  renderAll();
}

// ---------- photos as links: of the marks ----------

// A clip's marks: the player's (its own file's), else the camera's
function marksOf(clip) {
  const own = clip.own && Array.isArray(clip.own.marks) ? clip.own.marks.map((m) => m.t) : null;
  const p = PROBES[clip.path];
  return own || (p && p.marks && p.marks.times) || [];
}
// What photo_add needs of a clip (as album_add's)
function clipForLink(it) {
  const p = PROBES[it.path];
  return { tree: it.id, path: it.path, thumb: it.thumb || null, size: it.size, created: p && p.movie && p.movie.created ? Math.round(p.movie.created) : null, curve: clipCurve(it) };
}
// "Photos of the marks" (the selected clips' marks without a photo anywhere in the project) and, in a clip's own
// view, of the marks selected: each a photo link, made at once by the Library (src/albums.rs photo_add). They land in
// the album the clip was selected in, else in Generated (_Stills/Generated)
async function photoLinks() {
  const c = ctx();
  await needViews(["footage", "stills"]);
  const taken = (id, t) => ((DATA.stills && DATA.stills.items) || []).some((p) => p.link && p.link.clip === id && Math.abs((+p.link.t || 0) - t) < 0.6);
  const by = {};
  const want = (album, clip, t) => { (by[album] = by[album] || []).push(Object.assign(clipForLink(clip), { t })); };
  if (S.view === "clip") {
    const clip = DATA.clip && DATA.clip.clip;
    if (!clip) return;
    for (const m of c.direct.filter((x) => x.kind === "mark")) want("_Stills/" + GENERATED, clip, m.t);
  } else {
    const seen = {};
    for (const it of c.selItems.filter((x) => x.kind === "clip")) {
      if (seen[it.id]) continue;
      seen[it.id] = true;
      const album = S.view === "footage" && it.link_file && userAlbum(it.dir) ? diskRel(it.dir, "footage") : "_Stills/" + GENERATED;
      for (const t of marksOf(it)) if (!taken(it.id, t)) want(album, it, t);
    }
  }
  const albums = Object.keys(by);
  if (!albums.length) { toast(S.view === "clip" ? "Select marks: a photo is made of each" : "No mark without a photo in what’s selected"); return; }
  let made = 0, already = 0, ms = 0;
  const failed = [];
  try {
    for (const album of albums) {
      const r = await invoke("photo_add", { album, photos: by[album] });
      made += r.made.length; already += r.already; ms += r.ms; failed.push(...r.failed);
    }
    toast((made === 1 ? "1 photo" : made + " photos") + " made" + (made ? " in " + (ms < 950 ? Math.max(1, Math.round(ms / 10) * 10) + " ms" : (ms / 1000).toFixed(1) + " s") : "") +
      ": links to their frames, in " + (albums.length === 1 ? albumShown(albums[0]) : "their albums") + (already ? " · " + already + (already === 1 ? " was" : " were") + " there already" : "") +
      (failed.length ? ". But " + failed.length + " couldn’t be made: " + failed[0][1] : ""));
    await albumRead(["stills"]);
  } catch (e) { showError(String(e)); }
}
function renderAlbumSheet() {
  const a = S.album;
  if (!a) return "";
  const n = a.mode === "move" ? a.files.length : a.clips.length + a.copies.length;
  const what = a.mode === "move" || a.copies.length ? (n === 1 ? "1 item" : n + " items") : (n === 1 ? "1 clip" : n + " clips");
  const list = albumList();
  const target = a.name.trim() || a.to;
  let h = '<div class="scrim" data-a="scrim"><div class="sheet narrow albumsheet" role="dialog" aria-modal="true" aria-labelledby="albumTitle">' +
    '<header><h2 id="albumTitle">' + (a.mode === "move" ? "Move " : "Add ") + what + ' to an album</h2>' +
    '<span class="sub wrap">' + (a.mode === "move" ? "The links move (a moment with its renders, a photo as it is); the originals stay where they are." : a.copies.length ? "A link to each original, made at once; a moment or a photo there is a copy of its link, an item of its own from then on. The originals stay where they are." : "A link to each original, with its thumbnail: made at once. The originals stay where they are.") + '</span></header><div class="sb">';
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
      const r = a.clips.length ? await invoke("album_add", { album, clips: a.clips.map(clipForLink) }) : { made: [], already: 0, failed: [], ms: 0 };
      // Moments and photos: a copy of each link there
      const cp = a.copies.length ? await invoke("album_copy", { album, files: a.copies }) : { copied: 0, problems: [] };
      const made = r.made.length + cp.copied;
      for (const pr of cp.problems) r.failed.push(["", pr]);
      toast((a.copies.length ? (made === 1 ? "1 item" : made + " items") : (made === 1 ? "1 clip" : made + " clips")) + " added to " + shown + (made ? " in " + (r.ms < 950 ? Math.max(1, Math.round(r.ms / 10) * 10) + " ms" : (r.ms / 1000).toFixed(1) + " s") : "") +
        (r.already ? " · " + r.already + (r.already === 1 ? " was" : " were") + " there already" : "") + (r.failed.length ? ". But " + r.failed.length + " couldn’t be made: " + r.failed[0][1] : ""));
    }
    remember("afl.album", shown);
    S.sheet = null; S.album = null; renderSheet();
    S.sel = {}; S.anchor = null;
    await albumRead(a.mode === "move" ? ["footage", "moments", "stills"] : (a.clips.length ? ["footage"] : []).concat(a.copies.length ? ["moments", "stills"] : []));
  } catch (e) { a.busy = false; renderSheet(); showError(String(e)); }
}
