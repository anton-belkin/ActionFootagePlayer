"use strict";
// The Library's changes as writes.js makes them (docs/library-responsiveness.md): shown at once, written after. Here
// is what ties writes.js to the page (app.js): where the views are kept, how they're drawn again, what's said when a
// change couldn't be saved, the sign that changes are still being written, the window's close; and the changes
// themselves, each a few lines (Writes.define). Loaded before app.js, whose names it uses only when called.

// A view as the app last read it from the disk is kept as it came (CONFIRMED); what's shown (RAW_FOOTAGE, DATA) is that
// with the changes still being written put over it. A change is made with Writes.run: shown at once, written when
// its turn comes, taken back with a message when it couldn't be
Writes.host = {
  invoke: (cmd, args) => invoke(cmd, args),
  get: (view) => CONFIRMED[view],
  set: (view, data) => { CONFIRMED[view] = data; },
  // These views made again from what the disk has and what's being written, and drawn
  show(views) {
    for (const v of views) if (CONFIRMED[v]) showView(v);
    if (S.view === "clip") buildClip();
    renderAll();
  },
  // …read from the disk again (a change that failed, or was written only in part)
  reload(views) { for (const v of views) if (CONFIRMED[v]) loadView(v).then(renderAll, () => {}); },
  failed(f) { UNSAVED.push((f.partly ? "Not all saved: " : "Not saved: ") + f.what + ". " + f.why); renderUnsaved(); },
};
// What couldn't be saved, said until it's read (not a toast that goes by itself): which change, of what, and why
const UNSAVED = [];
function renderUnsaved() {
  const el = $("unsaved");
  el.hidden = !UNSAVED.length;
  el.innerHTML = UNSAVED.length ? '<div class="lines">' + UNSAVED.map((t) => '<div>' + esc(t) + '</div>').join("") + '</div><button type="button" class="btn sm" data-a="unsavedok">OK</button>' : "";
}
// Changes still being written: the app told (it keeps the window until they are), and said in Tasks once they've
// waited over half a second (a quick one shows nothing)
let writesSlow = false, writesTimer = null, writesTold = 0;
Writes.onChange(() => {
  const n = Writes.pending();
  if (n !== writesTold) { writesTold = n; invoke("writes_pending", { n }).catch(() => {}); }
  if (!n) { clearTimeout(writesTimer); writesTimer = null; if (writesSlow) { writesSlow = false; renderQueueOnly(); } return; }
  if (writesSlow) renderQueueOnly();
  else if (!writesTimer) writesTimer = setTimeout(() => { writesTimer = null; if (Writes.pending()) { writesSlow = true; renderQueueOnly(); } }, 500);
});
// The window closed with changes still being written: they're written first, then it closes
let closingNow = false;
function closing(on) {
  $("closing").hidden = !on;
  if (!on) { closingNow = false; return; }
  const n = Writes.pending();
  $("closingText").textContent = n ? n + (n === 1 ? " change is" : " changes are") + " still being written. The window closes once " + (n === 1 ? "it is" : "they are") + "." : "Closing…";
  if (closingNow) return;
  closingNow = true;
  Writes.drained().then(() => { if (closingNow) invoke("answer_close", { quit: true }).catch(() => {}); });
}
if (window.__TAURI__.event) window.__TAURI__.event.listen("close-waits", () => closing(true));

// The changes that show at once. Each says what it does to an item as the views have it (the command's own rule, as
// src/tags.rs and src/geo.rs do it on the disk), and how it's written
const quoted = (list) => list.map((t) => "“" + t + "”").join(", ");
function namesText(names) { return names.slice(0, 3).join(", ") + (names.length > 3 ? " and " + (names.length - 3) + " more" : ""); }
function tagsChanged(old, add, remove) {
  const list = (old || []).slice();
  for (let a of add) { a = a.trim(); if (a && !list.some((x) => x.toLowerCase() === a.toLowerCase())) list.push(a); }
  return list.filter((x) => !remove.some((r) => r.toLowerCase() === x.toLowerCase()));
}
function ownOf(it) { if (!it.own || typeof it.own !== "object") it.own = { marks: null, view: null, source: null, location: null }; return it.own; }
Writes.define("tags", {
  what: (a, op) => (a.add.length ? "the tag " + quoted(a.add) + " on " : "taking the tag " + quoted(a.remove) + " off ") + namesText(op.targets.map((t) => t.name)),
  item(it, a) {
    it.tags = tagsChanged(it.tags, a.add, a.remove);
    if ((it.kind === "moment" || it.kind === "export") && it.link) { if (it.tags.length) it.link.tags = it.tags.slice(); else delete it.link.tags; }
  },
  send: (a) => ["set_tags", { items: a.items, add: a.add, remove: a.remove }],
  problems: (r) => r,
});
// A location as it's written (src/geo.rs's location_value)
function locationValue(e, set) {
  const v = { lat: Math.round(e.lat * 1e6) / 1e6, lon: Math.round(e.lon * 1e6) / 1e6, source: e.source || "manual", set };
  if (e.alt != null) v.alt = Math.round(e.alt * 10) / 10;
  const p = Object.assign({}, e.place && typeof e.place === "object" ? e.place : {});
  delete p.by; delete p.tz; delete p.tz_offset_s;
  if (e.site && e.site.trim()) p.site = e.site.trim();
  if (Object.keys(p).length) v.place = p;
  if (e.photo_gap_s != null) v.photo_gap_s = Math.round(e.photo_gap_s);
  return v;
}
Writes.define("location", {
  what: (a, op) => (a.entries.every((e) => e.remove) ? "taking the location off " : "the location of ") + namesText(op.targets.map((t) => t.name)),
  item(it, a) {
    const e = a.entries.find((x) => x.ids.includes(it.id));
    if (!e) return;
    ownOf(it).location = e.remove ? null : locationValue(e, a.set);
  },
  async run(op, ask) {
    const a = op.args;
    // A point typed: its place's name looked up while it waits its turn (the point shows meanwhile)
    if (a.lookup) { const p = await ask("parse_location", { text: a.lookup }); a.entries[0].place = p.place; a.placeText = p.place_text; op.show(); }
    return ask("set_locations", { entries: a.entries });
  },
  problems: (r) => r,
});
Writes.define("source", {
  what: (a, op) => "the Source settings of " + namesText(op.targets.map((t) => t.name)),
  item(it, a) {
    const e = a.entries.find((x) => x.clip === it.id);
    if (!e) return;
    const own = ownOf(it);
    const src = Object.assign({}, own.source && typeof own.source === "object" ? own.source : {});
    for (const k of Object.keys(e.source)) { if (e.source[k] === null) delete src[k]; else src[k] = e.source[k]; }
    own.source = Object.keys(src).length ? src : null;
  },
  send: (a) => ["set_source", { entries: a.entries }],
  problems: (r) => r,
});
Writes.define("renders", {
  what: (a, op) => "deleting the renders of " + namesText(op.targets.map((t) => t.name)),
  item(it, a) {
    const r = {};
    if (a.staleOnly) for (const p of Object.keys(it.renders || {})) if (it.renders[p] !== "stale") r[p] = it.renders[p];
    it.renders = r;
  },
  send: (a) => ["delete_renders", { moments: a.moments, staleOnly: a.staleOnly }],
  problems: (r) => r.problems,
});
// A name a file can have on every system (src/main.rs's good_name); "": it can, else why not
function badName(name) {
  if (!name || name === "." || name === ".." || name.startsWith(".")) return "That name can’t be a file’s";
  if (/[\/\\:*?"<>|\u0000-\u001f]/.test(name)) return "A name can’t have / \\ : * ? \" < > |";
  return "";
}
// A moment, photo or export as it is once renamed (src/main.rs's rename_in): its extension kept (.afplink.mp4, a
// Motion Photo's .MP.jpg whole)
function renamed(it, name) {
  const file = it.id.split("/").pop(), lower = file.toLowerCase();
  const dot = file.lastIndexOf(".");
  const ext = it.kind === "moment" && lower.endsWith(".afplink.mp4") ? file.slice(-12) : it.kind !== "moment" && lower.endsWith(".mp.jpg") ? file.slice(-7) : dot >= 0 ? file.slice(dot) : "";
  return { id: it.id.slice(0, it.id.length - file.length) + name + ext, name: it.kind === "moment" ? name : name + ext, path: it.path.slice(0, it.path.length - file.length) + name + ext };
}
Writes.define("rename", {
  what: (a) => "renaming " + a.was + " to " + a.name,
  patch(d, a) {
    const it = d.item(a.id);
    if (!it) return;
    Object.assign(it, a.to);
    d.moved(it);
  },
  send: (a) => ["rename_item", { view: a.view, kind: a.kind, path: a.path, name: a.name }],
  made: (a) => [a.in + ":" + a.to.id],
});
// A folder of the project's renamed: what's in it follows, in every view that shows it (Footage: the clip previews in it)
function inFolder(p, f) { return p != null && (p === f || p.startsWith(f + "/")); }
Writes.define("renamefolder", {
  views: ["footage", "moments", "stills", "exports"],
  what: (a) => "renaming the folder " + a.was + " to " + a.name,
  patch(d, a) {
    const move = (p) => (p === a.from ? a.to : p.startsWith(a.from + "/") ? a.to + p.slice(a.from.length) : p);
    const was = a.from.split("/").pop();
    // A file's path on the disk, the folder's name in it changed (`rest`: what follows the folder in its path)
    const disk = (path, rest) => path.slice(0, path.length - rest - was.length) + a.name + path.slice(path.length - rest);
    if (d.view === "footage") {
      d.list("clips_dirs", (l) => l.map(move));
      for (const it of d.where((x) => inFolder(x.pdir, a.from))) {
        if (it.preview) it.preview = disk(it.preview, it.pdir.length - a.from.length + 1 + it.preview.split(/[\\/]/).pop().length);
        it.pdir = move(it.pdir);
      }
    } else {
      d.list("dirs", (l) => l.map(move).sort());
      for (const it of d.where((x) => inFolder(x.dir, a.from))) {
        it.path = disk(it.path, it.id.length - a.from.length);
        it.dir = move(it.dir);
        it.id = it.dir + "/" + it.id.split("/").pop();
      }
    }
  },
  send: (a) => ["rename_item", { view: a.view, kind: "folder", path: a.path, name: a.name }],
});
// Deleted: a folder with what's in it, a moment, photo or export, a clip's preview (the clip then has none)
Writes.define("delete", {
  views: ["footage", "moments", "stills", "exports"],
  what: (a) => "deleting " + namesText(a.names),
  patch(d, a) {
    const dirs = a.items.filter((x) => x.kind === "folder").map((x) => x.rel);
    const inside = (p) => dirs.some((f) => inFolder(p, f));
    if (d.view === "footage") {
      const previews = new Set(a.items.filter((x) => x.kind === "preview").map((x) => x.path));
      for (const it of d.where((x) => x.preview && (previews.has(x.preview) || inside(x.pdir)))) { it.preview = null; it.pdir = null; delete it.draft; }
      if (dirs.length) d.list("clips_dirs", (l) => l.filter((p) => !inside(p)));
    } else {
      const gone = new Set(a.items.filter((x) => x.kind !== "folder" && x.kind !== "preview").map((x) => x.path));
      d.remove((x) => gone.has(x.path) || inside(x.dir));
      if (dirs.length) d.list("dirs", (l) => l.filter((p) => !inside(p)));
    }
  },
  send: (a) => ["delete_items", { items: a.items.map((x) => ({ kind: x.kind, path: x.path })) }],
  problems: (r) => r.problems,
});
