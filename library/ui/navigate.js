"use strict";
// Back, forward, up, new folder; a clip's moments and photos.
// One of the window's scripts that were app.js: index.html loads them in this order before app.js, as classic
// scripts sharing their top-level names, so each uses the others' when called (only earlier ones at load).

// ---------- back, forward, up, new folder ----------

const NAV = {
  back: '<svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.4" aria-hidden="true"><path d="M15 6l-6 6 6 6"/></svg>',
  fwd: '<svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.4" aria-hidden="true"><path d="M9 6l6 6-6 6"/></svg>',
  up: '<svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.4" aria-hidden="true"><path d="M12 19V6M6 11l6-6 6 6"/></svg>',
  newdir: '<svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" aria-hidden="true"><path d="M3 7h6l2 2h10v10H3z"/><path d="M12 12v5M9.5 14.5h5"/></svg>',
};
// Back and Forward: history.js
function navButtons() {
  const canDir = !!FOLDER_VIEWS[S.view];
  const upOn = S.path.length || S.view === "clip";
  return '<span class="navs">' +
    '<button type="button" class="icon" data-a="back" aria-label="Back"' + (S.histAt > 0 ? "" : " disabled") + '>' + NAV.back + '</button>' +
    '<button type="button" class="icon" data-a="fwd" aria-label="Forward"' + (S.histAt < S.hist.length - 1 ? "" : " disabled") + '>' + NAV.fwd + '</button>' +
    '<button type="button" class="icon" data-a="up" aria-label="Enclosing folder"' + (upOn ? "" : " disabled") + '>' + NAV.up + '</button>' +
    (canDir ? '<button type="button" class="icon" data-a="newdir" aria-label="' + (S.view === "footage" ? "New album" : "New folder") + '" title="' + (S.view === "footage" ? "New album" : "New folder") + '">' + NAV.newdir + '</button>' : "") + '</span>';
}
async function newFolder() {
  if (!FOLDER_VIEWS[S.view]) return;
  // A new album is made in My albums: from the top of Albums, it's opened first
  if (S.view === "footage" && (!S.path.length || albumMineOpen())) S.path = [AV_MARK + "My albums"];
  const parent = dirKey();
  // An album's new album is in _Clips (the user's own); under Other folders, where that folder is. Shown at once with
  // the name the app gives it, ready to be renamed (writes.js: the rename waits for it to be made)
  const real = S.view === "footage" && parent === NOPREV ? VIEW_ROOT.footage : diskRel(parent);
  const names = namesIn(real);
  const dirs = [(RAW_FOOTAGE && RAW_FOOTAGE.clips_dirs) || []].concat(["moments", "stills", "exports"].map((v) => (CONFIRMED[v] && CONFIRMED[v].dirs) || []));
  for (const l of dirs) for (const d of l) if (parentDir(d) === real) names.add(d.split("/").pop().toLowerCase());
  let name = "untitled folder", n = 1;
  while (names.has(name)) name = "untitled folder " + ++n;
  const rel = real ? real + "/" + name : name;
  Writes.run("newfolder", [], { view: S.view, in: S.view === "sources" ? "footage" : S.view, parent: real, name, rel });
  const k = "f:" + shownDir(S.view, rel);
  S.sel = {}; S.sel[k] = true; S.anchor = k; S.renaming = k;
  renderAll();
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
  // In Originals: every clip is there (an album has only the ones put in it)
  const it = DATA.sources && DATA.sources.items.find((x) => x.id === id);
  if (!it) { toast("Its clip, " + id + ", isn’t in the project’s footage: Check on the moment shows where it was"); return; }
  S.view = "sources"; S.path = it.dir ? it.dir.split("/") : []; S.q = ""; S.flat = false; S.filters = {}; S.renaming = null;
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
  const clip = DATA.sources && DATA.sources.items.find((x) => x.id === id);
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
