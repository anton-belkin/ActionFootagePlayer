"use strict";
// What the window's scripts share: the app's commands (invoke), the views' names, the state (S) and what's read (DATA, PROBES, THUMBS…).
// One of the window's scripts that were app.js: index.html loads them in this order before app.js, as classic
// scripts sharing their top-level names, so each uses the others' when called (only earlier ones at load).

// The app's commands, journaled (journal.js, loaded first: the real window's __TAURI__.core is frozen, so the journal
// can't wrap it there; without the journal, the app's own)
const invoke = (window.Journal && window.Journal.invoke) || window.__TAURI__.core.invoke;
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
// Originals and Albums (docs/design/albums-photos-share-concept.md): moments, photos and exports are items of albums, not tabs
// of their own. Their scans are still read as "moments", "stills" and "exports" (DATA has them), for Albums to show
const VIEWS = [["sources", "Originals"], ["footage", "Albums"]];
// Each view's own folder in the project (Anton, 2026-11-10): what's in it is the view's top; what's anywhere else in the
// project's folder is under Other folders, at its path there
const VIEW_ROOT = { footage: "_Clips", moments: "_Moments", stills: "_Stills", exports: "_Exports" };
const OTHER = "Other folders";
// A folder's path in the project's folder -> as the view shows it
function shownDir(view, real) {
  if (view === "footage") return albumShown(real); // albums.js: _Clips, _Moments and _Stills as one, Generated beside
  const r = VIEW_ROOT[view];
  if (!r) return real;
  const lr = real.toLowerCase(), lroot = r.toLowerCase();
  if (lr === lroot) return "";
  if (lr.startsWith(lroot + "/")) return real.slice(r.length + 1);
  return real === "" ? OTHER : OTHER + "/" + real;
}
// …and back: a folder shown -> its path in the project's folder (Other folders itself: the project's folder)
function diskRel(f, view) {
  const r = VIEW_ROOT[view || S.view];
  if (!r || f === NOPREV) return f;
  if (f === OTHER) return "";
  if (f.startsWith(OTHER + "/")) return f.slice(OTHER.length + 1);
  return f ? r + "/" + f : r;
}
// A view's scan, its folders and items moved to where the view shows them (each item keeps its real folder as rdir)
function rooted(view, d) {
  if (!VIEW_ROOT[view]) return d;
  const dirs = new Set();
  for (const x of d.dirs || []) { const sd = shownDir(view, x); if (sd) dirs.add(sd); }
  const items = (d.items || []).map((it) => Object.assign({}, it, { rdir: it.dir, dir: shownDir(view, it.dir || "") }));
  for (const it of items) if (it.dir === OTHER || it.dir.startsWith(OTHER + "/")) dirs.add(OTHER);
  for (const x of Array.from(dirs)) if (x.startsWith(OTHER + "/")) dirs.add(OTHER);
  return Object.assign({}, d, { dirs: Array.from(dirs).sort((a, b) => (a.startsWith(OTHER) - b.startsWith(OTHER)) || a.localeCompare(b)), items });
}
// Footage is the clips as their previews are arranged (the previews' folders in the project's); the ones without a
// preview are in this one
const NOPREV = "Without a clip preview";
// Where each view is in the project, for the paths a batch is kept to: the project's folder itself (its own files are
// wherever they were put in it, docs/design/library-management.md §8)
const BASE = { footage: "", sources: "", moments: "", stills: "", exports: "", clip: "" };
const KIND_BASE = { moment: "", still: "", export: "" };
const NOUN = { footage: ["item", "items"], sources: ["clip", "clips"], moments: ["moment", "moments"], stills: ["photo", "photos"], exports: ["export", "exports"], clip: ["item", "items"] };
// The views whose folders are the project folder's, made, renamed and deleted here (Footage's: its previews' folders)
const ICON_MARK = '<svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M5 21V4"/><path d="M5 4h11l-2 4 2 4H5"/></svg>';
const FOLDER_VIEWS = { footage: true };
// A folder of a view (as the view shows it), on disk
// Other folders selected as a whole: what's in it (it stands for the project's folder outside the view's own)
function otherExpanded(folders) {
  if (!folders.includes(OTHER)) return folders;
  const ds = ((DATA[S.view] && DATA[S.view].dirs) || []).filter((d) => d.startsWith(OTHER + "/") && !d.slice(OTHER.length + 1).includes("/"));
  return folders.filter((f) => f !== OTHER).concat(ds);
}
function folderOnDisk(f) { return foldersOnDisk(f)[0]; }
// …every one of them: an album is the folders of that path in _Clips, _Moments and _Stills together (albums.js)
function foldersOnDisk(f) {
  const rels = S.view === "footage" ? albumRealDirs(f) : [diskRel(f)];
  return rels.map((rel) => S.root + (rel ? "/" + rel : ""));
}

// This computer's choices for the batches (the page's own storage; none: the defaults)
function remembered(k, d) { try { const v = localStorage.getItem(k); return v == null ? d : v; } catch (_) { return d; } }
function remember(k, v) { try { localStorage.setItem(k, v); } catch (_) { /* */ } }
const S = {
  clipsLayout: ["tree", "flat", "trips"].includes(remembered("afl.layout", "tree")) ? remembered("afl.layout", "tree") : "tree",
  alone: remembered("afl.alone", "") === "1",
  root: null, name: "", info: null, macos: true,
  view: "sources", path: [], sel: {}, anchor: null, layout: "list", q: "", flat: false, scope: "here",
  filters: {}, filtersOpen: false, fsec: { src: true, tf: true }, fexp: {}, fmore: {}, ffind: {}, ffreeze: null, suggOpen: false, sugg: -1,
  // The side panel: the part shown (its column of icons: renderSide), and whether the panel is open beside the column
  tab: remembered("afl.tab", "info"), sideOpen: remembered("afl.side", "1") === "1", menu: false, recent: [],
  // Source settings changed in the Source Settings tab, not written yet (key -> value), and for which clips
  srcDraft: {}, srcFor: null,
  photoKind: "live", jobs: [], finished: 0,
  locations: [], connecting: {}, via: null, sheet: null, stab: "loc", settings: null, locEdit: [], localEdit: [], hostsText: "",
  checking: false, checked: null, finding: false, found: null,
  take: null, takePlace: "", taking: false, tagDraft: "",
  // Tags: the ones used last on this computer (the player's K too), a rename's two names, Finder's on this computer
  recentTags: [], tagFrom: "", tagTo: "", tagStores: [],
  geoHand: "", geoSite: "", geo: null, geoBusy: false, geoClips: [], geoStatus: "", geoMax: 3, geoPick: null, geoKeep: true, geoHad: [],
  scanning: {}, cachedViews: {}, renaming: null,
  hist: [], histAt: -1,
  // Stabilize optically: the clips measured already measured again (Source settings' Measure again)
  opticalRedo: false,
};
let RAW_FOOTAGE = null; // the footage view as scanned: one item per clip, at its path in the tree
const DATA = {};          // view -> { dirs: [path], items: [item] }
const CONFIRMED = {};     // view -> the app's last answer for it: what the disk has (what's shown: with the changes being written over it)
const PROBES = {};        // a clip's path -> gyroflow-probe --library's answer
let probesVersion = 0, probesPending = 0;
let MOMENTS_OF = {};      // a clip's path in the project -> how many moments it has
const THUMBS = new Map(); // a file -> data: URL, or null for none
// A file renamed or moved (by itself or with its folder): the same file, so the same picture at its new path, shown at
// once. The window shows the new path before the disk has it (writes.js), so asked for there it would have none yet
function thumbFollows(from, to) {
  const t = THUMBS.get(from);
  if (t && from !== to && !THUMBS.get(to)) THUMBS.set(to, t);
}
let VISIBLE = [];
// The filters offered (items.js buildFacets; drawn by drawing.js)
let FACETS = [];
// The files whose link's original isn't where it says (the Library counts them on its own)
let MISSING = new Set();
// The context menu open now (menu.js openMenu; closed by app.js's events)
let CMENU = null;
