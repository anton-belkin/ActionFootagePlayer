"use strict";
// The context menu, deleting, Info.
// One of the window's scripts that were app.js: index.html loads them in this order before app.js, as classic
// scripts sharing their top-level names, so each uses the others' when called (only earlier ones at load).

// ---------- the context menu, deleting, Info ----------

// What Delete takes of the selection: the project's own files and folders only. Footage's clips give their previews
// (the clip stays); Sources' clips are footage, deleted in Finder
function deletable(c) {
  if (S.view === "sources" || (!FOLDER_VIEWS[S.view] && S.view !== "clip")) return [];
  const list = c.selFolders.filter((f) => f !== NOPREV && !f.startsWith(NOPREV + "/") && f !== OTHER).flatMap((f) => foldersOnDisk(f).map((path) => ({ kind: "folder", path })));
  for (const it of c.direct) {
    if (it.kind === "mark") continue;
    if (it.kind === "clip") { if (it.preview) list.push({ kind: "preview", path: it.preview }); }
    else list.push({ kind: it.kind, path: it.path });
  }
  return list;
}
function deleteLabel(c, list) {
  const trash = S.macos ? "Move to Trash" : "Delete";
  if (S.view === "footage" && list.length && list.every((x) => x.kind === "preview")) return list.length === 1 ? "Remove from the album" : "Remove " + list.length + " from the album";
  return trash;
}
async function deleteSelection() {
  const c = ctx();
  if (stillWriting(c.direct)) return;
  const list = deletable(c);
  if (!list.length) {
    if (c.any) toast(S.view === "sources" ? "Footage is deleted in Finder: Check then shows what pointed at it" : S.view === "footage" ? "Nothing here to remove: the clips themselves are deleted in Finder" : "Nothing here to delete");
    return;
  }
  // Gone from the window at once, moved to the Trash after (writes.js); what couldn't be comes back, said so
  const names = c.selFolders.map((f) => f.split("/").pop()).concat(c.direct.filter((it) => it.kind !== "mark" && (it.kind !== "clip" || it.preview)).map((it) => (it.kind === "clip" ? "the link of " : "") + it.name));
  for (const x of list) if (x.kind === "folder") x.rel = x.path.slice(S.root.length + 1);
  S.sel = {}; S.anchor = null;
  const done = await Writes.run("delete", [], { items: list, names });
  const r = done.answer;
  if (r && r.done) toast((r.done === 1 ? "1 moved" : r.done + " moved") + (r.to.length ? " to " + r.to.join(" and ") : "") + (list.every((x) => x.kind === "preview") ? ": the originals are untouched" : ""));
}
function canRename(c) {
  if (c.selFolders.length + c.direct.length !== 1 || S.view === "sources") return false;
  if (c.selFolders.length === 1 && c.selFolders[0] === OTHER) return false;
  if (S.view === "footage") return c.selFolders.length === 1 ? c.selFolders[0] !== NOPREV : c.direct[0].kind !== "clip";
  if (S.view === "clip") return c.direct.length === 1 && c.direct[0].kind !== "mark";
  return true;
}

// The menu under the right button: the summary's commands, and Finder's own around them
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
    if (FOLDER_VIEWS[S.view]) e.push(b("newdir", S.view === "footage" ? "New album" : "New folder", k("⇧⌘N", "Ctrl+Shift+N")));
    const del = deletable(c);
    const marksOnly = c.direct.length && !c.selFolders.length && c.direct.every((x) => x.kind === "mark");
    if (marksOnly) e.push(b("photolinks", c.direct.length === 1 ? "Make a photo of the mark" : "Make photos of the marks"));
    if (S.view !== "sources" && !marksOnly) e.push(b("trash", deleteLabel(c, del), k("⌘⌫", "Delete"), !del.length));
    if (e.length) parts.push(e.join(""));
    const clips = (S.view === "footage" || S.view === "sources") ? c.selItems.filter((x) => x.kind === "clip").length : 0;
    { const am = albumMenu(c, b); if (am) parts.push(am); } // albums.js
    { const sm = shareMenu(c, b); if (sm) parts.push(sm); } // share.js
    if (clips) parts.push(b("srcsettings", clips > 1 ? "Batch set Source settings…" : "Source settings…"));
    parts.push(b("info", "Info") + b("selall", "Select all", k("⌘A", "Ctrl+A")) + b("desel", "Select none", k("Esc", "Esc")));
  } else {
    const g = [];
    if (FOLDER_VIEWS[S.view]) g.push(b("newdir", S.view === "footage" ? "New album" : "New folder", k("⇧⌘N", "Ctrl+Shift+N")));
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
// A folder of the project's tree, on disk: where a clip in it is (the first source that has it)
function treeOnDisk(f) {
  const its = (DATA.sources && DATA.sources.items) || [];
  const it = its.find((x) => x.id.startsWith(f ? f + "/" : ""));
  if (!it || !it.path) return null;
  const rest = f ? it.id.slice(f.length) : "/" + it.id;
  return it.path.endsWith(rest) ? it.path.slice(0, it.path.length - rest.length) : null;
}
// The folder shown, on disk: its own folder, or a source's
function hereOnDisk() {
  if (S.view === "clip") { const cl = DATA.clip && DATA.clip.clip; return cl ? cl.path : null; }
  if (S.view === "sources") return treeOnDisk(S.path.join("/"));
  const p = S.path.filter((x) => x !== NOPREV).join("/");
  return folderOnDisk(p);
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
  let h = '<div class="ihead">' + (it.thumbPath ? '<span class="ith">' + itemImg(it) + '</span>' : "") + '<b class="wrap">' + esc(it.name) + '</b><span class="sub">' + esc(it.sub || "") + '</span></div>';
  const tags = (it.tags || []).length ? '<span class="chips">' + it.tags.map((t) => '<span class="chip tag">' + esc(t) + '</span>').join("") + '</span>' : "";
  if (it.kind === "clip") {
    const probe = PROBES[it.path];
    h += infoSec("Clip", kv([["Recorded", it.date], ["Length", it.dur], ["Size", sizeText(it.size)], ["Camera", it.cam || (probe ? "Unknown" : "Reading…")], ["Lens", it.is360 ? "" : (lensName(it) || (probe ? "" : "Reading…")) + (lensFrom(it) && lensFrom(it) !== "file" && lensFrom(it) !== "project" ? " · " + LENS_FROM[lensFrom(it)].toLowerCase() : "")],
      ["Color", it.color], ["Picture", resText(probe)], ["Shape", it.is360 ? "360°" : probe ? "Flat" : ""], ["In or out of water", probe && probe.water ? "Recorded by the camera" : ""],
      ["Motion data", it.measured ? "Measured from the picture" + (probe.measured && probe.measured.in_water ? ", in water" : ", in air") + (it.readoutLeftOut ? ", without the rolling shutter: measure again" : "") : it.motion === true ? "In the clip" + (probe.mode && probe.mode !== "default" ? " (" + probe.mode + ")" : "") + (it.camStab ? ", stabilized on top of the camera's " + it.camStab : "") : it.motion === false ? (it.camStab ? "None, stabilized in the camera (" + it.camStab + ")" : "None") : ""],
      ["Tags", tags, false, true]]));
    h += locationInfo(it);
    h += infoSec("Marks and moments", kv([["Marks", it.marks ? it.marks + (marksList(it, probe) ? ": " + marksList(it, probe) : "") : "None"], ["Moments", it.moments || "None"]]));
    const tf = stabText(it.own && it.own.view);
    h += infoSec("Transform", tf ? kv(tf.concat([["Turned", it.turned ? "Yes" : ""]])) : '<p class="sub wrap">' + (it.gyroflow ? "Its Gyroflow project" : TF_DEFAULT) + (it.turned ? ", turned" : "") + '</p>');
    const conflict = Array.isArray(it.own_conflict) && it.own_conflict.length > 1;
    h += infoSec("Files", kv([["In the project", it.id, true], ["Copies", pathsHtml((it.copies || []).map((c) => c.path + "  (" + c.loc + ")")), false, true],
      ["Own file", conflict ? '<span class="chip warn">' + it.own_conflict.length + ' own files</span>' + pathsHtml(it.own_conflict) : pathsHtml(it.own_files || []) || (it.own ? "" : "None yet"), false, true],
      ["Gyroflow", it.gyroflow ? "A project beside the clip" : ""], [it.link_file ? "Its link here" : "In albums", it.link_file ? pathsHtml([it.link_file]) : (RAW_FOOTAGE.links || []).filter((l) => l.clip === it.id).length ? pathsHtml((RAW_FOOTAGE.links || []).filter((l) => l.clip === it.id).map((l) => l.rel)) : "None", false, true]]));
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
  let h = '<div class="ihead"><b class="wrap">' + esc(c.selName) + '</b><span class="sub">' + esc(countText(its) + (c.selFolders.length ? ", everything inside included" : "")) + '</span></div>';
  h += infoSec("All of it", kv([["Length", dur ? clock(dur) : ""], ["Size", sizeText(size)], ["Recorded", whens.length ? dateText(whens[0]) + (whens.length > 1 ? " – " + dateText(whens[whens.length - 1]) : "") : ""],
    ["Cameras", count((it) => it.cam)], ["Color", count((it) => it.color)], ["Marks", its.reduce((n, it) => n + (it.marks || 0), 0) || ""], ["Moments", its.reduce((n, it) => n + (it.moments || 0), 0) || ""],
    ["Kinds", count((it) => it.kind === "still" ? it.form : "")], ["Tags", Array.from(new Set(its.flatMap((it) => it.tags || []))).join(", ")],
  ]));
  // Where its clips were recorded: a small map with each place's dot, and the places by name
  const clips = its.filter((it) => it.kind === "clip");
  if (clips.length) {
    const placed = clips.filter((it) => it.geo);
    const m = {};
    placed.forEach((it) => { const n = placeText(it.geo.place) || coordText(it.geo.lat, it.geo.lon); m[n] = (m[n] || 0) + 1; });
    const names = Object.keys(m).sort((a, b) => m[b] - m[a]).map((n) => '<span class="blk">' + esc(n) + ' <span class="sub" style="display:inline">' + m[n] + '</span></span>').join("");
    h += infoSec("Location", placed.length ? mapHtml(placed.map((it) => [it.geo.lat, it.geo.lon])) + kv([["Places", names, false, true], ["No location", clips.length - placed.length || ""]])
      : '<p class="sub wrap">None of these clips has a location yet.</p>');
  }
  if (one && FOLDER_VIEWS[S.view] && c.selFolders[0] !== NOPREV) h += infoSec("Files", kv([["Folder", pathsHtml([folderOnDisk(c.selFolders[0])]), false, true]]));
  return h;
}
function infoHtml(c) {
  if (!c.any) {
    if (albumViewOpen()) return albumViewInfo(); // albumviews.js: no folder behind it
    const all = items();
    const here = hereOnDisk();
    return '<div class="ihead"><b>' + esc(c.folderName) + '</b><span class="sub">' + esc(plural(scopeItems().length) + " here, " + plural(all.length) + " in " + c.viewName) + '</span></div>' +
      infoSec("Files", kv([["Folder", here ? pathsHtml([here]) : "", false, true], ["Project", pathsHtml([S.root]), false, true]])) +
      '<p class="sub wrap" style="padding:0 16px">Select a clip, moment, photo or folder to see what it is.</p>';
  }
  if (c.direct.length === 1 && !c.selFolders.length) { const b = clipButton(c); return itemInfo(c.direct[0]) + (b ? '<div class="isec"><div class="btns">' + b + '</div></div>' : ""); }
  return manyInfo(c);
}
