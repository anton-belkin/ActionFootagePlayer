"use strict";
// The Library's changes as writes.js makes them (docs/library-responsiveness.md): shown at once, written after. Here
// is what ties writes.js to the page (app.js): where the views are kept, how they're drawn again, what's said when a
// change couldn't be saved, the sign that changes are still being written, the window's close; and the changes
// themselves, each a few lines (Writes.define). Loaded before app.js, whose names it uses only when called.
//
// And the pinpoint update: a command says what its items are now (src/changed.rs), and so does the app for what the
// catalog says changed elsewhere (`changes_since`); those items are put into what the disk has and only they are
// made again in what's shown. No view is read again, unless the app says one must be (its folders changed).

// A view as the app last read it from the disk is kept as it came (CONFIRMED); what's shown (RAW_FOOTAGE, DATA) is that
// with the changes still being written put over it. A change is made with Writes.run: shown at once, written when
// its turn comes, taken back with a message when it couldn't be
Writes.host = {
  invoke: (cmd, args) => invoke(cmd, args),
  get: (view) => CONFIRMED[view],
  set: (view, data) => { CONFIRMED[view] = data; },
  // These views made again from what the disk has and what's being written, and drawn: only the change's own items
  // when it's one of their fields (a tag, a location), else the views whole (from memory, nothing read)
  show(views, op) {
    for (const v of views) {
      if (!CONFIRMED[v]) continue;
      const ids = op ? op.targets.filter((t) => t.view === v).map((t) => t.id) : [];
      if (!(ids.length && showItems(v, ids))) showView(v);
    }
    if (S.view === "clip") buildClip();
    drawChanged();
  },
  // A command's answer, when it says what it changed (src/changed.rs)
  changed: (r) => (r && typeof r === "object" && r.items && r.gone && Array.isArray(r.reload) ? r : null),
  // …put into what the disk has, and shown
  // (`op`: the change it's the answer to: its items are made again whatever the disk says of them, since what they
  // showed was the change put over them)
  merge(ch, op) {
    const r = mergeChanged(ch);
    if (op) {
      r.changed = true;
      for (const v of op.shows) {
        if (!CONFIRMED[v] || r.whole.has(v)) continue;
        const ids = op.targets.filter((t) => t.view === v).map((t) => t.id);
        if (!(ids.length && showItems(v, ids))) showView(v);
      }
    }
    if (S.view === "clip" && r.changed) buildClip();
    if (r.changed) drawChanged();
    if (r.reload.size) Writes.host.reload(Array.from(r.reload));
    previewsGone(ch);
  },
  mergedInto: (view, data, ch) => mergedInto(view, data, ch),
  // …read from the disk again (a change that failed, or was written only in part)
  reload(views) { for (const v of views) if (CONFIRMED[v]) loadView(v).then(renderAll, () => {}); },
  failed(f) { UNSAVED.push((f.partly ? "Not all saved: " : "Not saved: ") + f.what + ". " + f.why); renderUnsaved(); },
};
// ---------- the pinpoint update ----------

const VIEW_NAMES = ["footage", "moments", "stills", "exports"];
// Where each item is in the app's answer for its view, by its id. What's shown of the view is in the same order
// (unless a change that moves items is waiting: then the view is made again whole)
const ITEM_AT = new WeakMap();
function placeOf(view, id) {
  const items = CONFIRMED[view] && CONFIRMED[view].items;
  if (!items) return -1;
  let m = ITEM_AT.get(items);
  if (!m) { m = new Map(); items.forEach((it, i) => m.set(it.id, i)); ITEM_AT.set(items, m); }
  const i = m.get(id);
  return i === undefined ? -1 : i;
}
const parentDir = (p) => (p.lastIndexOf("/") < 0 ? "" : p.slice(0, p.lastIndexOf("/")));
// A view's data with a Changed's items of it put in (a copy: for a reading that came in after them, and for changes
// that add, remove or move items): each replaces the one of its id; a new one goes where a reading would have it
// among its folder's (by name), its folder listed; the gone ones go
function mergedInto(view, data, ch) {
  const items = (ch.items || {})[view] || [], gone = new Set((ch.gone || {})[view] || []);
  if (!items.length && !gone.size) return data;
  const by = new Map(items.map((it) => [it.id, it]));
  const list = [];
  for (const it of data.items || []) {
    if (gone.has(it.id)) continue;
    const now = by.get(it.id);
    if (now) by.delete(it.id);
    list.push(now || it);
  }
  const out = Object.assign({}, data, { items: list });
  const folders = (name, dir) => {
    const dirs = (out[name] || []).slice();
    for (let d = dir; d && !dirs.includes(d); d = parentDir(d)) dirs.push(d);
    if (dirs.length !== (out[name] || []).length) out[name] = dirs.sort();
  };
  for (const it of by.values()) {
    // (a clip that's new to Footage isn't placed here: the view is read again, mergeChanged)
    if (view === "footage") continue;
    let at = -1, last = -1;
    list.forEach((x, i) => { if (x.dir === it.dir) { last = i; if (at < 0 && x.id > it.id) at = i; } });
    list.splice(at >= 0 ? at : last + 1 || list.length, 0, it);
    folders("dirs", it.dir);
  }
  // A clip whose preview is in a folder not listed yet (a run's new folder)
  if (view === "footage") for (const it of items) if (it.pdir) folders("clips_dirs", it.pdir);
  return out;
}
// A Changed put into what the disk has, and what's shown made again for it: its items alone when they're where
// they were, the view whole (from memory) when items came, went or moved. What's said back: whether anything shown
// changed, and the views that must be read again after all
function mergeChanged(ch) {
  if (ch.probes) mergeProbes(ch.probes);
  const out = { changed: false, reload: new Set(), whole: new Set() };
  for (const view of VIEW_NAMES) {
    const data = CONFIRMED[view];
    if (!data) continue;
    let items = (ch.items || {})[view] || [];
    const gone = ((ch.gone || {})[view] || []).filter((id) => placeOf(view, id) >= 0);
    // A clip the page has never seen: where it goes in the tree isn't known here
    if (view === "footage" && items.some((it) => placeOf(view, it.id) < 0)) { out.reload.add(view); items = items.filter((it) => placeOf(view, it.id) >= 0); }
    if (!items.length && !gone.length) continue;
    const same = (a, b) => JSON.stringify(a) === JSON.stringify(b);
    const inPlace = !gone.length && items.every((it) => { const i = placeOf(view, it.id); return i >= 0 && data.items[i].dir === it.dir && (view !== "footage" || data.items[i].pdir === it.pdir); });
    if (inPlace) {
      const ids = [];
      for (const it of items) { const i = placeOf(view, it.id); if (!same(data.items[i], it)) { data.items[i] = it; ids.push(it.id); } }
      if (!ids.length) continue;
      if (!showItems(view, ids)) { showView(view); out.whole.add(view); }
    } else {
      CONFIRMED[view] = mergedInto(view, data, { items: { [view]: items }, gone: { [view]: gone } });
      showView(view);
      out.whole.add(view);
    }
    out.changed = true;
  }
  return out;
}
// These items of a view made again in what's shown (the item as the disk has it, the changes waiting for it over it,
// what the page works out of it), where they are. false: it can't be done item by item (a change waiting that moves
// items, an item that changes folders): the view is to be made again whole
function showItems(view, ids) {
  const conf = CONFIRMED[view];
  if (view === "footage") {
    if (!RAW_FOOTAGE || !DATA.footage || !DATA.sources || RAW_FOOTAGE.items.length !== conf.items.length) return false;
    const made = [];
    for (const id of ids) {
      const i = placeOf(view, id);
      const raw = i < 0 ? null : Writes.overItem(view, conf.items[i]);
      const f = DATA.footage.items[i], s = DATA.sources.items[i];
      if (!raw || !f || f.id !== id || !s || s.id !== id) return false;
      const dir = raw.pdir == null ? NOPREV : shownDir("footage", raw.pdir);
      if (dir !== f.dir || raw.dir !== s.dir) return false;
      made.push([i, raw, Object.assign({}, raw, { key: raw.id, tdir: raw.dir, dir }), Object.assign({}, raw, { key: "s:" + raw.id, tdir: raw.dir, dir: raw.dir })]);
    }
    // (what's shown shares the disk's list until a change waits for one of its items)
    if (RAW_FOOTAGE.items === conf.items && made.some((m) => m[1] !== conf.items[m[0]])) RAW_FOOTAGE.items = RAW_FOOTAGE.items.slice();
    for (const [i, raw, f, s] of made) {
      RAW_FOOTAGE.items[i] = raw;
      deriveItems("footage", [f, s]);
      DATA.footage.items[i] = f; DATA.sources.items[i] = s;
    }
    return true;
  }
  const d = DATA[view];
  if (!d || d.items.length !== conf.items.length) return false;
  const made = [];
  for (const id of ids) {
    const i = placeOf(view, id);
    const raw = i < 0 ? null : Writes.overItem(view, conf.items[i]);
    const old = d.items[i];
    if (!raw || !old || old.id !== id) return false;
    const x = Object.assign({}, raw, { rdir: raw.dir, dir: shownDir(view, raw.dir || "") });
    // (a moment of another clip now: the clips' counts of moments change with it)
    if (x.dir !== old.dir || (view === "moments" && (raw.link || {}).clip !== (old.link || {}).clip)) return false;
    made.push([i, x]);
  }
  for (const [i, x] of made) { deriveItems(view, [x]); d.items[i] = x; }
  return true;
}
// Drawn after a change. An item selected that the filters or the search no longer take leaves the list at once, as
// in the culling programs (Lightroom, Capture One); when everything selected left, the item that took its place is
// selected, so the next one is under the hand
function drawChanged() {
  const was = VISIBLE.slice(), sel = Object.keys(S.sel).filter((k) => k.startsWith("i:") && was.includes(k));
  renderAll();
  if (!sel.length || !(filtering() || S.q.trim())) return;
  const left = sel.filter((k) => !VISIBLE.includes(k));
  if (!left.length) return;
  for (const k of left) delete S.sel[k];
  if (!Object.keys(S.sel).length && VISIBLE.length) {
    const k = VISIBLE[Math.min(was.indexOf(left[0]), VISIBLE.length - 1)];
    if (k.startsWith("i:")) { S.sel[k] = true; S.anchor = k; S.cursor = k; }
  }
  renderMain(); renderSide();
}
// Files gone that were clips' previews: those clips asked for as they are now
function previewsGone(ch) {
  const gone = new Set(ch.gone_files || []);
  if (!gone.size || !CONFIRMED.footage) return;
  const ids = CONFIRMED.footage.items.filter((it) => it.preview && gone.has(it.preview)).map((it) => it.id);
  if (ids.length) invoke("view_items", { view: "footage", ids }).then((c) => { if (Writes.host.changed(c)) Writes.host.merge(c); }, () => {});
}
// What the catalog says changed (the app's own commands, the player, another computer, a task's files as they land):
// the items it changed asked for and put in; the views read again only when the app can't say which items (too many,
// folders, the project's file). One at a time; a later one asked meanwhile follows
let changesBusy = false, changesNext = null;
async function catalogChanged(n) {
  if (!S.root || n === undefined || n === S.changes) return;
  if (changesBusy) { changesNext = n; return; }
  changesBusy = true;
  const root = S.root, since = S.changes;
  const reading = Writes.reading("*");
  let r = null;
  try { r = await invoke("changes_since", { since: since || 0 }); } catch (e) { /* an app that can't say */ }
  changesBusy = false;
  if (root !== S.root) { Writes.read(reading, null); return; }
  if (!r || !Writes.host.changed(r.changed)) {
    Writes.read(reading, null);
    refreshSoon(r ? r.upto : n);
  } else {
    S.changes = r.upto;
    const m = mergeChanged(r.changed);
    Writes.after(reading);
    for (const v of r.changed.reload || []) m.reload.add(v);
    if (S.view === "clip" && m.changed) buildClip();
    if (m.changed) drawChanged();
    if (m.reload.size) Writes.host.reload(Array.from(m.reload));
    previewsGone(r.changed);
  }
  if (changesNext !== null) { const next = changesNext; changesNext = null; catalogChanged(next); }
}

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
