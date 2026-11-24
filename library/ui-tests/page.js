"use strict";
// The Library's page (../ui) run in jsdom against a mock of the app, for its tests and its measurement: the page's
// own files loaded as the window loads them, `window.__TAURI__` answering its commands from `app` (below), every call
// recorded. `app.js` keeps `invoke` from the moment it loads, so the mock is in place before the scripts run.
//
//   const page = await open({ views: { footage, moments, … }, commands: { set_tags: (args) => … }, delay: { scan: 100 } });
//   page.click('[data-a="tag"]'); await page.until(() => …); page.calls("scan"); page.close();

const { JSDOM } = require("jsdom");
const fs = require("fs");
const path = require("path");
const vm = require("vm");

// (AFL_UI: another copy of the page, an older one to measure against)
const UI = process.env.AFL_UI || path.join(__dirname, "..", "ui");
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

// A small project, for the tests that need no more: two folders of clips, each clip with a link in the album of its
// folder's name (_Clips/Day 1, _Clips/Day 2: the scan's `links`), moments (the album Reef), photos (in _Stills itself:
// Albums' top), an export (Other folders › _Exports)
function smallViews(root) {
  root = root || "/P";
  const clip = (dir, name, extra) => Object.assign({ id: dir + "/" + name, dir, name, path: "/F/" + dir + "/" + name, size: 8, mtime: 1700000000, kind: "clip", loc: "Archive",
    copies: [{ loc: "Archive", li: 0, rel: dir + "/" + name, path: "/F/" + dir + "/" + name, offline: false }], own_files: [], gyroflow: false,
    pdir: "_Clips/" + dir, preview: root + "/_Clips/" + dir + "/" + name.replace(/\.MP4$/, ".afplink.mp4"), tags: [] }, extra || {});
  const moment = (dir, name, clipId, extra) => Object.assign({ id: "_Moments/" + dir + "/" + name + ".afplink.mp4", dir: "_Moments/" + dir, name, path: root + "/_Moments/" + dir + "/" + name + ".afplink.mp4",
    size: 100, mtime: 1700000100, kind: "moment", tags: [], gyroflow: false, renders: {}, original: "/F/" + clipId, link: { v: 1, kind: "moment", clip: clipId, in: 1, out: 4 } }, extra || {});
  const still = (name, clipId, extra) => Object.assign({ id: "_Stills/" + name, dir: "_Stills", name, path: root + "/_Stills/" + name, size: 50, mtime: 1700000200, kind: "still", tags: [],
    form: "Ultra HDR JPEG", link: { v: 1, kind: "photo", clip: clipId, t: 1.5 } }, extra || {});
  const footage = {
    root, view: "footage", dirs: ["Day 1", "Day 2"], clips_dirs: ["_Clips", "_Clips/Day 1", "_Clips/Day 2"],
    locations: [{ name: "Archive", at: "", path: "../F", dir: "/F", copies: false, local: false, dirs: ["Day 1", "Day 2"], offline: false, connect: false, names: [] }],
    items: [clip("Day 1", "A.MP4", { tags: ["reef"], own: { marks: [{ t: 1.5 }], view: null, source: null, location: null }, own_files: ["/F/Day 1/A.afp.json"] }),
      clip("Day 1", "B.MP4"), clip("Day 1", "C.MP4"), clip("Day 2", "D.MP4"), clip("Day 2", "E.MP4")],
    probes: { version: 1, pending: 0, items: {} },
  };
  footage.links = footage.items.map((it) => ({ path: it.preview, rel: it.preview.slice(root.length + 1), clip: it.id, draft: false, size: 2100, mtime: 1700000050 }));
  for (const it of footage.items) footage.probes.items[it.path] = { camera: "DJI OsmoAction6", color: "dlogm", mode: "default", movie: { duration: 30, created: 1735700000 }, marks: { times: [] } };
  const moments = { root, view: "moments", dirs: ["_Moments", "_Moments/Reef"], items: [moment("Reef", "Ray", "Day 2/D.MP4"), moment("Reef", "Shark", "Day 1/B.MP4"), moment("Reef", "Turtle", "Day 1/A.MP4")],
    probes: { version: 1, pending: 0, items: {} } };
  // (the moments' scan answers with their originals' probes, as the app's does: what a moment is, is its clip's)
  for (const m of moments.items) if (footage.probes.items[m.original]) moments.probes.items[m.original] = footage.probes.items[m.original];
  const stills = { root, view: "stills", dirs: ["_Stills"], items: [still("A_1.5s.jpg", "Day 1/A.MP4"), still("B_1.5s.jpg", "Day 1/B.MP4")], probes: { version: 1, pending: 0, items: {} } };
  const exports = { root, view: "exports", dirs: ["_Exports"], items: [{ id: "_Exports/E.mov", dir: "_Exports", name: "E.mov", path: root + "/_Exports/E.mov", size: 900, mtime: 1700000300, kind: "export", tags: [],
    link: { v: 1, kind: "export", clip: "Day 1/A.MP4", in: 1, out: 4, look: "hlg" } }], probes: { version: 1, pending: 0, items: {} } };
  return { footage, moments, stills, exports };
}

// The page opened on a project. `o.views`: what `scan` answers for each view (read each time: a test changes them as
// the disk would change); `o.commands`: answers of other commands (a function of its arguments, which may throw or
// return a promise), over the defaults; `o.delay`: milliseconds each command takes ({ scan: 50, set_tags: 200 }, or a
// function (cmd, args) -> ms); `o.atStart(cmd, args, rec, { state, emit })`: called as a command starts (its answer's function
// gets `rec` too; `emit` an event of the app's); `o.scripts`: the page's scripts, when not as index.html names them
async function open(o) {
  o = o || {};
  const root = o.root || (o.views && o.views.footage && o.views.footage.root) || "/P";
  const views = o.views || smallViews(root);
  const log = [];          // { cmd, args, at (ms since opened), took }
  const listeners = {};    // event -> [fn]
  const state = { changes: 1, jobs: [], close_asked: false, quiet_ms: 5000 };
  const t0 = Date.now();
  const copy = (v) => (v == null ? v : JSON.parse(JSON.stringify(v)));
  const defaults = {
    snapshot: () => ({ project: root, jobs: state.jobs, close_asked: state.close_asked, macos: true, changes: state.changes, quiet_ms: state.quiet_ms, catalog: "", missing: 0, moving: null,
      close_waits: !!state.close_waits }),
    project: () => ({ path: root, name: root.split("/").pop(), legacy: false, clips_flat: false, clips_layout: "tree", moments_alone: false }),
    engine: () => "/Applications/ActionFootagePlayer.app",
    recent: () => [],
    recent_tags: () => [],
    cached_view: () => null,
    scan: (a) => { const v = views[a.view]; if (!v) return { root, view: a.view, dirs: [], items: [] }; return copy(v); },
    probes_since: () => ({ version: 1, items: {}, pending: 0 }),
    thumb: () => null,
    set_focus: () => null,
    missing_links: () => [],
    lens_types: () => [],
    lens_info: () => ({}),
    places_of: (a) => (a.points || []).map(() => null),
    mini_map: () => null,
  };
  const commands = Object.assign({}, defaults, o.commands || {});
  const delayOf = (cmd, args) => {
    const d = typeof o.delay === "function" ? o.delay(cmd, args) : (o.delay || {})[cmd];
    return typeof d === "function" ? d(args) : (d || 0);
  };
  let inFlight = 0;
  const invoke = async (cmd, args) => {
    const rec = { cmd, args: copy(args), at: Date.now() - t0, took: null };
    log.push(rec);
    inFlight++;
    try {
      // (what a test wants done as a command starts, before it takes its time: a view as it was then)
      if (o.atStart) o.atStart(cmd, args || {}, rec, { state, emit: (name, payload) => { for (const f of listeners[name] || []) f({ payload }); } });
      const ms = delayOf(cmd, args);
      if (ms > 0) await sleep(ms); else await Promise.resolve();
      const f = commands[cmd];
      if (!f) return null;
      return await f(args || {}, rec);
    } finally { inFlight--; rec.took = Date.now() - t0 - rec.at; }
  };
  const html = fs.readFileSync(path.join(UI, "index.html"), "utf8").replace(/<script src="[^"]+"><\/script>\s*/g, "");
  const names = o.scripts || Array.from(fs.readFileSync(path.join(UI, "index.html"), "utf8").matchAll(/<script src="([^"]+)"><\/script>/g)).map((m) => m[1]);
  const dom = new JSDOM(html, { runScripts: "outside-only", pretendToBeVisual: true, url: "http://library.test/" });
  const w = dom.window;
  w.__TAURI__ = {
    // (`o.frozen`: as the real window's, which no script can change: Tauri freezes window.__TAURI__.core)
    core: o.frozen ? Object.freeze({ invoke }) : { invoke },
    event: { listen: (name, fn) => { (listeners[name] = listeners[name] || []).push(fn); return Promise.resolve(() => {}); } },
    webview: { getCurrentWebview: () => ({ onDragDropEvent: () => {} }) },
  };
  w.CSS = w.CSS || {};
  if (!w.CSS.escape) w.CSS.escape = (s) => String(s).replace(/[^a-zA-Z0-9_\-]/g, (c) => "\\" + c);
  w.Element.prototype.scrollIntoView = function () {};
  const errors = [];
  w.addEventListener("error", (e) => errors.push(e.error || e.message));
  w.addEventListener("unhandledrejection", (e) => errors.push(e.reason));
  // As the window's own <script>s run (a script's top-level names stay for the later ones and for `get`)
  const ctx = dom.getInternalVMContext();
  for (const n of names) new vm.Script(fs.readFileSync(path.join(UI, n), "utf8"), { filename: n }).runInContext(ctx);
  const page = {
    window: w, document: w.document, views, state, log, errors,
    // A value of the page's own (its top-level names aren't the window's: asked for by name)
    get: (expr) => w.eval(expr),
    $: (sel) => w.document.querySelector(sel),
    $$: (sel) => Array.from(w.document.querySelectorAll(sel)),
    text: (sel) => { const e = w.document.querySelector(sel); return e ? e.textContent : null; },
    calls: (cmd) => log.filter((c) => !cmd || c.cmd === cmd),
    // The calls made since `mark()`, without the page's own regular questions
    mark: () => log.length,
    since: (n) => log.slice(n).filter((c) => !["snapshot", "probes_since", "thumb", "set_focus"].includes(c.cmd)),
    emit: (name, payload) => { for (const f of listeners[name] || []) f({ payload }); },
    // The disk changed (by a command, the player, another computer): the app counts it and tells the page
    changed: () => { state.changes++; state.quiet_ms = 5000; page.emit("catalog-changed", state.changes); },
    click: (sel, init) => {
      const e = typeof sel === "string" ? w.document.querySelector(sel) : sel;
      if (!e) throw new Error("nothing to click: " + sel);
      e.dispatchEvent(new w.MouseEvent("click", Object.assign({ bubbles: true, cancelable: true, detail: 1 }, init || {})));
    },
    dblclick: (sel) => {
      const e = typeof sel === "string" ? w.document.querySelector(sel) : sel;
      if (!e) throw new Error("nothing to double-click: " + sel);
      for (const d of [1, 2]) e.dispatchEvent(new w.MouseEvent("click", { bubbles: true, cancelable: true, detail: d }));
      e.dispatchEvent(new w.MouseEvent("dblclick", { bubbles: true, cancelable: true, detail: 2 }));
    },
    key: (key, init, target) => (target || w.document.body).dispatchEvent(new w.KeyboardEvent("keydown", Object.assign({ key, bubbles: true, cancelable: true }, init || {}))),
    type: (sel, value) => { const e = w.document.querySelector(sel); e.value = value; e.dispatchEvent(new w.Event("input", { bubbles: true })); },
    // The key of an item where the page is now, by its id (a clip's path in the tree, a moment's or photo's path in the
    // project) or by its own key there ("a:<link>", "s:<clip>", "m:<moment>", "p:<photo>"): Originals and Albums key
    // the same clip differently
    keyOf: (id) => w.eval("(() => { const id = " + JSON.stringify(id) + "; const it = items().find((x) => x.key === id) || items().find((x) => x.id === id); return it ? keyOf(it) : 'i:' + id; })()"),
    // Its row in the list
    row: (id) => w.document.querySelector('#main [data-v="' + w.CSS.escape(page.keyOf(id)) + '"]'),
    // Items of the list selected (as a click on each row), by id or key; `kind` "f": folders, by their path as shown
    select: (ids, kind) => { ids = [].concat(ids); w.eval("S.sel = {}; S.anchor = null;"); for (const id of ids) w.eval("S.sel[" + JSON.stringify(kind ? kind + ":" + id : page.keyOf(id)) + "] = true"); w.eval("renderMain(); renderSide();"); },
    until: async (f, ms, what) => {
      const end = Date.now() + (ms || 5000);
      for (;;) {
        let v; try { v = f(); } catch (e) { v = false; }
        if (v) return v;
        if (Date.now() > end) throw new Error("waited " + (ms || 5000) + " ms for " + (what || f.toString()));
        await sleep(2);
      }
    },
    // Nothing asked of the app (but its regular questions) for `ms`
    quiet: async (ms, most) => {
      ms = ms || 60;
      const end = Date.now() + (most || 20000);
      let last = log.length, since = Date.now();
      for (;;) {
        await sleep(5);
        const busy = log.some((c) => c.took === null && !["snapshot", "probes_since"].includes(c.cmd));
        const n = log.filter((c) => !["snapshot", "probes_since", "thumb", "set_focus"].includes(c.cmd)).length;
        if (busy || n !== last) { last = n; since = Date.now(); }
        else if (Date.now() - since >= ms) return;
        if (Date.now() > end) throw new Error("the page never went quiet");
      }
    },
    sleep,
    close: () => { w.close(); },
  };
  // Open: the project read, its first views shown
  await page.until(() => w.eval("typeof S !== 'undefined' && S.root && DATA.footage && DATA.moments && !S.scanning.footage && !S.scanning.moments"), o.openTimeout || 60000, "the project to open");
  await page.quiet(30, o.openTimeout || 60000);
  return page;
}

module.exports = { open, smallViews, sleep, UI };
