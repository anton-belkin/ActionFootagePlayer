"use strict";
// Sources, renaming, the keyboard, Quick Look.
// One of the window's scripts that were app.js: index.html loads them in this order before app.js, as classic
// scripts sharing their top-level names, so each uses the others' when called (only earlier ones at load).

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
  if (/\.afplink\.jpg$/i.test(n)) return n.slice(0, -12);
  if (/\.mp\.jpg$/i.test(n)) return n.slice(0, -7);
  const i = n.lastIndexOf(".");
  return i > 0 ? n.slice(0, i) : n;
}
function startRename() {
  const keys = Object.keys(S.sel);
  if (keys.length !== 1) return;
  const it = currentItems().find((x) => keyOf(x) === keys[0]);
  if (!it || stillWriting([it])) return;
  if (S.view === "sources" || (S.view === "footage" && (it.kind === "clip" || it.path === NOPREV))) {
    toast(it.kind === "folder" ? "Sources’ folders are renamed in Finder (Find moved files then follows)" : "A clip is renamed in Finder: Find moved files then follows. Here: albums, moments, photos, exports");
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
  name = name.trim();
  // An album: each of its folders on disk (in _Clips, _Moments, _Stills) gets the name
  const paths = it.kind === "folder" ? foldersOnDisk(it.path) : [it.path];
  const view = it.kind === "still" ? "stills" : it.kind === "export" ? "exports" : it.kind === "moment" ? "moments" : S.view;
  // What the app would refuse, said before anything shows: a name no file can have, one that's taken
  let why = badName(name);
  const to = it.kind === "folder" ? null : renamed(it, name);
  if (!why && it.kind === "folder" && childDirs(dirKey()).some((d) => d.split("/").pop() === name)) why = name + " exists already";
  if (!why && to && DATA[view] && DATA[view].items.some((x) => x.id === to.id)) why = to.name + " exists already";
  if (why) { showError(why); renderMain(); return; }
  S.sel = {}; S.anchor = null;
  // Shown at once, renamed after (writes.js)
  if (it.kind === "folder" && S.view === "footage") {
    // An album: its folders as one change (shown whole, or taken back whole)
    const folders = paths.map((path) => { const from = path.slice(S.root.length + 1); return { path, from, to: from.slice(0, from.length - it.name.length) + name }; });
    Writes.run("renamealbum", [], { view, name, was: it.name, folders });
  } else if (it.kind === "folder") {
    for (const path of paths) {
      const from = path.slice(S.root.length + 1);
      Writes.run("renamefolder", [], { view, path, name, was: it.name, from, to: from.slice(0, from.length - it.name.length) + name });
    }
  } else Writes.run("rename", [it], { view, in: Writes.VIEW_OF[it.kind], kind: it.kind, path: paths[0], name, was: it.name, id: it.id, to });
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
    if (it) files.push(it.path);
  }
  if (!files.length && !c.any) return;
  invoke("quick_look", { paths: files }).catch((e) => toast(String(e)));
}
function openSelection() {
  const keys = Object.keys(S.sel);
  if (keys.length === 1 && keys[0].startsWith("f:")) { S.path = keys[0].slice(2).split("/"); S.sel = {}; S.anchor = null; renderAll(); $("main").scrollTop = 0; return; }
  openChosen();
}
// What's selected, opened: as the player's list (playList)
function openChosen() {
  const sel = ctx().selItems;
  openItems(sel.map((it) => it.path), playList(sel));
}
function goUp() {
  if (S.view === "clip" && !searching()) { goFootage(S.clip); return; }
  if (!S.path.length || searching()) return;
  if (albumViewOpen()) { S.path = S.path.slice(0, -1); S.sel = {}; S.anchor = null; renderAll(); return; } // albumviews.js: no folder to select
  // An album of the user's: up is My albums, the album selected there
  if (S.view === "footage" && S.path.length === 1 && userAlbum(S.path[0])) { const k = "f:" + S.path[0]; S.path = [AV_MARK + "My albums"]; S.sel = {}; S.sel[k] = true; S.anchor = k; renderAll(); return; }
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
  // ⌘[ ⌘], Alt+← Alt+→: history.js
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
