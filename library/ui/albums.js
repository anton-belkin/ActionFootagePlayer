"use strict";
// Albums (docs/design/albums-photos-share-concept.md, "Albums"; the app's src/albums.rs): the view that was Footage. An album
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
const ALBUM_CURVES = ["dlogm", "dlog", "gplog", "gplog2", "ilog"];

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
    out.push(Object.assign({}, clip, { key: "a:" + l.rel, tdir: clip.dir, rdir: i < 0 ? "" : l.rel.slice(0, i), preview: l.path, link_file: l.path, draft: l.draft || undefined,
      pending: l.pending ? true : undefined, ph: l.pending ? (l.ph || clip.preview || clip.thumb || clip.path) : undefined }));
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
    (a.out.length && a.out.some((x) => x.kind !== "clip") && !albumWhole() ? b("albumout", "Take out of the album") : "");
}
// The same on the bar above the list, once something is selected
function albumBar(c) {
  const a = albumSel(c);
  return (a.n ? '<button type="button" class="btn sm" data-a="albumadd">' + (S.view === "footage" && !(typeof albumViewOpen === "function" && albumViewOpen()) ? "Add to another album…" : "Add to an album…") + '</button>' : "") +
    (a.files.length ? '<button type="button" class="btn sm" data-a="albummove">Move to an album…</button>' : "") +
    (a.out.length && !albumWhole() ? '<button type="button" class="btn sm" data-a="albumout">Take out of the album</button>' : "");
}
// A view of the whole library (Timeline, Map, a media type: albumviews.js) is no album to take things out of
function albumWhole() { return typeof albumViewOpen === "function" && !!albumViewOpen() && !albumMineOpen(); }
async function askAlbum(mode) {
  const c = ctx();
  const a = albumSel(c);
  if (mode === "move" ? !a.files.length : !a.n) return;
  if (mode === "move" && stillWriting(c.direct)) return;
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
// (all of it at once: the links gone, the moments and photos in Generated; written after, writes.js)
function albumOut() {
  const a = albumSel(ctx());
  if (!a.out.length || stillWriting(a.out)) return;
  const links = a.out.filter((x) => x.link_file).map((x) => ({ kind: "preview", path: x.link_file }));
  S.sel = {}; S.anchor = null;
  if (links.length) Writes.run("delete", [], { items: links, names: a.out.filter((x) => x.link_file).map((x) => "the link of " + x.name) });
  for (const [kind, album] of [["moment", "_Moments/" + GENERATED], ["still", "_Stills/" + GENERATED]]) {
    const files = a.out.filter((x) => x.kind === kind).map((x) => x.path);
    if (files.length) albumMove(album, files, "taking " + namesText(a.out.filter((x) => x.kind === kind).map((x) => x.name)) + " out of the album");
  }
  const n = a.out.length;
  toast(n + (n === 1 ? " item" : " items") + " taken out of the album" + (a.out.some((x) => x.kind !== "clip") ? ": moments and photos are in Generated" : ": the originals are untouched"));
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
  // Shown at once, each with its clip's picture until its frame is read (writes.js, the "photoadd" change below)
  let made = 0;
  for (const album of albums) made += photoAdd(album, by[album]);
  toast((made === 1 ? "1 photo" : made + " photos") + ": links to their frames, in " + (albums.length === 1 ? albumShown(albums[0]) : "their albums"));
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
  // Shown at once in the album (and in Originals' "In an album"), written after (writes.js); the sheet closes now
  S.sheet = null; S.album = null; renderSheet();
  S.sel = {}; S.anchor = null;
  remember("afl.album", shown);
  if (a.mode === "move") {
    const n = albumMove(album, a.files, "moving " + namesText(a.files.map((f) => f.split("/").pop())) + " to " + shown);
    toast((n === 1 ? "1 moved" : n + " moved") + " to " + shown);
  } else {
    const r = albumAdd(album, a.clips);
    const cp = albumCopy(album, a.copies);
    const made = r.made + cp;
    toast((a.copies.length ? (made === 1 ? "1 item" : made + " items") : (made === 1 ? "1 clip" : made + " clips")) + " added to " + shown +
      (r.already ? " · " + r.already + (r.already === 1 ? " was" : " were") + " there already" : ""));
  }
}

// ---------- shown at once, written after (docs/design/library-responsiveness.md) ----------
//
// Each album command is a change of writes.js: what it will make is worked out here by the command's own rules
// (src/albums.rs: a name taken gets " 2", " 3"…; a clip already in the album, a photo of a time already there, is
// left as it is), shown at once (a clip's link with the picture the page has of the clip, a copy with its file's, a
// photo link with its clip's until its frame is read), and the command's answer (src/changed.rs's of_album: the links
// and items as made) replaces it with no view read. A change that fails is taken back and said in the strip.
//
// An item still being written can be selected, tagged, culled and added to another album: those are changes too,
// queued behind its write (and taken back with it when it fails). Moving it, taking it out, renaming, deleting or
// sharing it waits until it's written: refused with a word, since they need its file.

// The files a folder of the project's folder has, as far as the page knows (every view's, the ones being written
// among them): lower-cased names
function namesIn(rdir) {
  const out = new Set();
  const take = (rel) => { if (parentDir(rel) === rdir) out.add(rel.slice(rel.lastIndexOf("/") + 1).toLowerCase()); };
  for (const l of (RAW_FOOTAGE && RAW_FOOTAGE.links) || []) take(l.rel);
  for (const v of ["moments", "stills", "exports"]) for (const it of (DATA[v] && DATA[v].items) || []) take(it.id);
  return out;
}
// A name free in a folder (albums.rs: `stem`, else `stem 2`, `stem 3`…), taken from then on
function freeName(names, stem, ext) {
  let name = stem + ext, n = 2;
  while (names.has(name.toLowerCase())) name = stem + " " + n++ + ext;
  names.add(name.toLowerCase());
  return name;
}
// A file's name split as albums.rs's item_of splits it: what's renamed, and its ending
function nameParts(file) {
  const lower = file.toLowerCase();
  const n = lower.endsWith(".afplink.mp4") || lower.endsWith(".afplink.jpg") ? 12 : lower.endsWith(".mp.jpg") ? 7 : file.lastIndexOf(".") >= 0 ? file.length - file.lastIndexOf(".") : 0;
  return [file.slice(0, file.length - n), file.slice(file.length - n)];
}
const rootOf = () => S.root.replace(/\/$/, "");
// Some of these still being written: said, and true (what needs their files waits for them)
function stillWriting(list) {
  const p = list.filter((x) => x.pending);
  if (!p.length) return false;
  toast((p.length === 1 ? p[0].name + " is" : namesText(p.map((x) => x.name)) + " are") + " still being written: in a moment");
  return true;
}
const nowS = () => Math.floor(Date.now() / 1000);

// Links to clips (album_add): `clips` as albumSel gives them. What's made and what was there already
function albumAdd(album, clips) {
  if (!clips.length) return { made: 0, already: 0 };
  const names = namesIn(album), there = new Set(((RAW_FOOTAGE && RAW_FOOTAGE.links) || []).filter((l) => parentDir(l.rel) === album).map((l) => l.clip));
  const links = [], send = [];
  let already = 0;
  for (const c of clips) {
    if (there.has(c.id)) { already++; continue; }
    there.add(c.id);
    const stem = c.id.split("/").pop().replace(/\.[^.]+$/, "");
    const rel = album + "/" + freeName(names, stem, ".afplink.mp4");
    // (the picture the page shows for the clip where it was selected)
    links.push({ clip: c.id, rel, path: rootOf() + "/" + rel, ph: c.thumbPath || c.preview || c.thumb || c.path, name: c.name });
    send.push(clipForLink(c));
  }
  if (links.length) Writes.run("albumadd", [], { album, links, clips: send, shown: albumShown(album) });
  return { made: links.length, already };
}
Writes.define("albumadd", {
  views: ["footage"],
  what: (a) => "adding " + namesText(a.links.map((l) => l.name)) + " to the album " + a.shown,
  patch(d, a) {
    if (d.view !== "footage") return;
    const links = d.data.links || [];
    const add = a.links.filter((p) => !links.some((l) => l.path === p.path || (l.clip === p.clip && parentDir(l.rel) === a.album)));
    if (!add.length) return;
    const t = nowS();
    d.data.links = links.concat(add.map((p) => ({ path: p.path, rel: p.rel, clip: p.clip, draft: false, size: 0, mtime: t, pending: true, ph: p.ph })));
    d.list("clips_dirs", (l) => { for (let x = a.album; x && !l.includes(x); x = parentDir(x)) l.push(x); return l.sort(); });
    d.hit = true;
  },
  send: (a) => ["album_add", { album: a.album, clips: a.clips }],
  problems: (r) => (r && r.failed ? r.failed.map((f) => f[0] + ": " + f[1]) : []),
});

// Moments and photos that are links, copied into an album (album_copy): each an item of its own there
function albumCopy(album, files) {
  if (!files.length) return 0;
  const names = namesIn(album), made = [];
  for (const f of files) {
    const view = /\.afplink\.mp4$/i.test(f) ? "moments" : "stills";
    const src = ((DATA[view] && DATA[view].items) || []).find((x) => x.path === f);
    if (!src || src.rdir === album) continue;
    const [stem, ext] = nameParts(f.split("/").pop());
    const id = album + "/" + freeName(names, stem, ext);
    made.push({ view, from: src.id, id, path: rootOf() + "/" + id, name: src.name, ph: src.thumbPath || src.ph || src.path });
  }
  if (made.length) Writes.run("albumcopy", [], { album, files, made, shown: albumShown(album) }, { made: made.map((m) => m.view + ":" + m.id) });
  return made.length;
}
// An item of a view as it will be at another place (a copy, or moved): its id, path, folder and name, its picture
// meanwhile, put where a reading would have it
function itemAt(d, it, m, album) {
  const file = m.id.split("/").pop();
  Object.assign(it, { id: m.id, path: m.path, dir: album, name: it.kind === "moment" ? nameParts(file)[0] : file, pending: true, ph: m.ph });
  d.moved(it);
  d.list("dirs", (l) => { for (let x = album; x && !l.includes(x); x = parentDir(x)) l.push(x); return l.sort(); });
  d.hit = true;
}
Writes.define("albumcopy", {
  views: ["moments", "stills"],
  what: (a) => "copying " + namesText(a.made.map((m) => m.name)) + " into the album " + a.shown,
  patch(d, a) {
    for (const m of a.made) {
      if (m.view !== d.view || d.data.items.some((x) => x.id === m.id)) continue;
      const src = d.data.items.find((x) => x.id === m.from);
      if (!src) continue;
      const it = JSON.parse(JSON.stringify(src));
      d.data.items.push(it);
      itemAt(d, it, m, a.album);
    }
  },
  send: (a) => ["album_copy", { album: a.album, files: a.files }],
  problems: (r) => (r && r.problems) || [],
});

// An album's own files (a clip's link, a moment, a photo) moved into another album (album_move): `what` for the strip
function albumMove(album, files, what) {
  const names = namesIn(album), moves = [];
  for (const f of files) {
    const link = ((RAW_FOOTAGE && RAW_FOOTAGE.links) || []).find((l) => l.path === f);
    const view = link ? "footage" : /\.afplink\.mp4$/i.test(f) ? "moments" : "stills";
    const src = link ? null : ((DATA[view] && DATA[view].items) || []).find((x) => x.path === f);
    const from = link ? link.rel : src ? src.id : null;
    if (!from || parentDir(from) === album) continue;
    const [stem, ext] = nameParts(f.split("/").pop());
    const id = album + "/" + freeName(names, stem, ext);
    moves.push({ view, from, fromPath: f, id, path: rootOf() + "/" + id, ph: link ? (link.ph || f) : (src.ph || src.thumbPath || f), name: f.split("/").pop() });
    thumbFollows(f, rootOf() + "/" + id); // the same file in another album: its picture once it's there too
  }
  if (moves.length) Writes.run("albummove", [], { album, files, moves, what }, { made: moves.filter((m) => m.view !== "footage").map((m) => m.view + ":" + m.id) });
  return moves.length;
}
Writes.define("albummove", {
  views: ["footage", "moments", "stills"],
  what: (a) => a.what,
  patch(d, a) {
    for (const m of a.moves) {
      if (m.view !== d.view) continue;
      if (d.view === "footage") {
        const links = d.data.links || [];
        const i = links.findIndex((l) => l.path === m.fromPath);
        if (i < 0 || links.some((l) => l.path === m.path)) continue;
        d.data.links = links.slice();
        d.data.links[i] = Object.assign({}, links[i], { rel: m.id, path: m.path, pending: true, ph: m.ph });
        d.list("clips_dirs", (l) => { for (let x = a.album; x && !l.includes(x); x = parentDir(x)) l.push(x); return l.sort(); });
        d.hit = true;
      } else {
        if (d.data.items.some((x) => x.id === m.id)) continue;
        const it = d.item(m.from);
        if (it) itemAt(d, it, m, a.album);
      }
    }
  },
  send: (a) => ["album_move", { album: a.album, files: a.files }],
  problems: (r) => (r && r.problems) || [],
});

// Photo links of marks (photo_add), in an album: what's made (one at a time of that clip already there: left)
function photoAdd(album, photos) {
  const names = namesIn(album), made = [], send = [];
  const there = ((DATA.stills && DATA.stills.items) || []).filter((p) => p.rdir === album && p.link && p.link.clip).map((p) => [p.link.clip, +p.link.t || 0]);
  for (const ph of photos) {
    if (there.some(([c, t]) => c === ph.tree && Math.abs(t - ph.t) < 0.05)) continue;
    there.push([ph.tree, ph.t]);
    const stem = ph.tree.split("/").pop().replace(/\.[^.]+$/, "") + "_" + ph.t.toFixed(2) + "s";
    const id = album + "/" + freeName(names, stem, ".afplink.jpg");
    const orig = (DATA.sources && DATA.sources.items.find((x) => x.id === ph.tree)) || {};
    made.push({ id, path: rootOf() + "/" + id, clip: ph.tree, t: Math.round(ph.t * 1000) / 1000, created: ph.created, ph: orig.thumbPath || orig.thumb || orig.path || null });
    send.push(ph);
  }
  if (made.length) Writes.run("photoadd", [], { album, photos: send, made, shown: albumShown(album) }, { made: made.map((m) => "stills:" + m.id) });
  return made.length;
}
Writes.define("photoadd", {
  views: ["stills"],
  what: (a) => "making " + (a.made.length === 1 ? "a photo" : a.made.length + " photos") + " of marks in " + a.shown,
  patch(d, a) {
    if (d.view !== "stills") return;
    const t = nowS();
    for (const m of a.made) {
      if (d.data.items.some((x) => x.id === m.id || (x.dir === a.album && x.link && x.link.clip === m.clip && Math.abs((+x.link.t || 0) - m.t) < 0.05))) continue;
      const link = { v: 1, kind: "photo", clip: m.clip, t: m.t };
      if (m.created) link.created = new Date(m.created * 1000).toISOString();
      const it = { kind: "still", id: m.id, dir: a.album, name: m.id.split("/").pop(), path: m.path, size: 0, mtime: t, tags: [], linked: true, form: "Photo link", link, pending: true, ph: m.ph };
      d.data.items.push(it);
      d.moved(it);
      d.list("dirs", (l) => { for (let x = a.album; x && !l.includes(x); x = parentDir(x)) l.push(x); return l.sort(); });
      d.hit = true;
    }
  },
  send: (a) => ["photo_add", { album: a.album, photos: a.photos }],
  problems: (r) => (r && r.failed ? r.failed.map((f) => f[0] + ": " + f[1]) : []),
});

// A new album (or folder): shown at once with the name the app will give it (src/main.rs's new_folder: "untitled
// folder", " 2"… when taken), ready to be renamed; the app is asked for that name
Writes.define("newfolder", {
  views: ["footage", "moments", "stills", "exports"],
  what: (a) => "making the folder " + a.name,
  patch(d, a) {
    if (d.view !== a.in) return;
    d.list(d.view === "footage" ? "clips_dirs" : "dirs", (l) => { for (let x = a.rel; x && !l.includes(x); x = parentDir(x)) l.push(x); return l.sort(); });
  },
  async run(op, ask) {
    const a = op.args;
    const made = await ask("new_folder", { view: a.view, parent: a.parent, name: a.name });
    // (another name after all: the folders read again)
    if (made !== a.rel) setTimeout(() => Writes.host.reload([a.in]), 0);
    return made;
  },
});
