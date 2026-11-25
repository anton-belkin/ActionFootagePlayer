// The responsiveness journal, the page's part (docs/design/responsiveness-journal.md): each thing asked of the Library (a
// click, a double-click, a key, a keystroke in a box, a choice), when it was asked, what it then waited for (the app's
// commands, the page's drawing, the pictures) and when its result was on screen. Loaded first, before every other
// script, so that by default:
//   - every click, double-click, key, input and change on the page is an action, named by what it was on
//     ("click:<data-a>", "key:Meta+a", "input:#q", "change:<data-a>"), with how long the page's own handlers took;
//   - every command the page asks the app for (window.__TAURI__.core.invoke) while an action is going on is its step,
//     counted and timed per command; one slower than SLOW_MS is a record of its own;
//   - the page's render*/load* functions (app.js and the others, found by name once they're loaded) are steps too;
//   - a picture loaded (an <img>) while an action goes on is counted, first and last;
//   - "paint": the first frame drawn after the action's own handlers ran (requestAnimationFrame, then a task: the
//     closest the page can see to "on screen"); "settled": the frame drawn once its last command answered and was
//     handled; "done" sums it up once nothing has happened for it for IDLE_MS.
// The records go to the app (journal_write, journal.rs) every 2 s, which writes them with its own in one file a run.
// Nothing is sent anywhere else. The clock is performance.timeOrigin + performance.now(): ms, monotonic, Unix time.
//
//   Journal.act(name[, fields])   an action asked now from code (not an event): returns its id
//   Journal.step(name[, fields])  a point in the action going on (fields.d: how long it took, ms)
//   Journal.current()             the id of the action going on (for a command that hands it on: open_items)
//   Journal.invoke(cmd, args)     the app's command, journaled (absent when the journal is off: ask the app directly)
"use strict";
(function () {
  const VERSION = 2; // 2: commands through Journal.invoke (the real window's core is frozen)
  const SLOW_MS = 50;        // a command or a render this slow is a record of its own
  const IDLE_MS = 1200;      // an action is over once nothing has happened for it this long (> the 600 ms poll)
  const MAX_LIFE_MS = 60000; // nor does it take what comes later than this
  const FLUSH_MS = 2000;
  // The app's commands the page asks for by itself all the time (its polls): never an action's step
  const BACKGROUND = new Set(["snapshot", "probes_since", "journal_write", "journal_hello"]);

  const tauri = window.__TAURI__ && window.__TAURI__.core;
  const enabled = !!(tauri && tauri.invoke) && !(window.localStorage && safe(() => localStorage.getItem("afl.journal") === "off"));
  function safe(f) { try { return f(); } catch (e) { return undefined; } }
  const now = () => performance.timeOrigin + performance.now();

  let buf = [];
  const cost = { records: 0, rec_ms: 0, flushes: 0, flush_ms: 0, dropped: 0 };
  let pid = "";
  let count = 0;
  const open = [];   // the actions going on, newest last
  let inBackground = 0;

  function rec(r) {
    const t0 = performance.now();
    r.s = "page";
    try { buf.push(JSON.stringify(r)); cost.records++; } catch (e) { cost.dropped++; }
    if (buf.length > 20000) buf = buf.slice(-10000);
    cost.rec_ms += performance.now() - t0;
  }

  const api = { act: () => null, step() {}, current: () => null, enabled, now, _rec: rec, _open: open };
  window.Journal = api;
  if (!enabled) return;

  const rawInvoke = tauri.invoke.bind(tauri);

  // ---- actions

  function newAct(name, fields, w) {
    const a = "L" + pid + ".p" + (++count);
    const act = { a, name, w: w ?? now(), last: now(), pending: 0, parts: {}, images: 0, firstImage: null, lastImage: null,
                  settled: null, paintAsked: false, closed: false };
    rec(Object.assign({ k: "act", a, n: name, w: act.w }, fields || {}));
    open.push(act);
    if (open.length > 16) close(open[0]);
    return act;
  }

  // The action a command, a render or a picture belongs to: the newest one going on
  function owner() {
    const t = now();
    for (let i = open.length - 1; i >= 0; i--) {
      const x = open[i];
      if (!x.closed && t - x.w < MAX_LIFE_MS) return x;
    }
    return null;
  }

  function part(act, name, start, d) {
    const p = act.parts[name] || (act.parts[name] = [0, 0, 0]);
    p[0]++; p[1] += d; if (d > p[2]) p[2] = d;
    if (d >= SLOW_MS) rec({ k: "step", a: act.a, n: name, w: start, d });
  }

  // After the next frame is drawn (rAF runs before it; a task queued from there runs after it)
  function afterPaint(fn) {
    const go = () => setTimeoutRaw(fn, 0);
    if (document.hidden || typeof requestAnimationFrame !== "function") return setTimeoutRaw(fn, 0);
    requestAnimationFrame(go);
  }

  function touched(act) {
    act.last = now();
    schedule();
  }

  function maybeSettle(act) {
    if (act.pending > 0 || act.closed) return;
    afterPaint(() => {
      if (act.pending > 0 || act.closed) return;
      act.settled = now();
      act.last = act.settled;
      schedule();
    });
  }

  function close(act) {
    if (act.closed) return;
    act.closed = true;
    const i = open.indexOf(act);
    if (i >= 0) open.splice(i, 1);
    const parts = {};
    for (const [k, v] of Object.entries(act.parts)) parts[k] = [v[0], round(v[1]), round(v[2])];
    rec({ k: "done", a: act.a, n: act.name, w: act.w, settled: act.settled ?? act.paint ?? null, paint: act.paint ?? null,
          pending: act.pending || undefined, images: act.images || undefined, first_image: act.firstImage ?? undefined,
          last_image: act.lastImage ?? undefined, parts });
  }
  const round = (x) => Math.round(x * 100) / 100;

  let idleTimer = null;
  function schedule() {
    if (idleTimer) return;
    idleTimer = setTimeoutRaw(function check() {
      idleTimer = null;
      const t = now();
      for (const x of open.slice()) {
        if (x.pending === 0 && t - x.last >= IDLE_MS) close(x);
        else if (t - x.w > MAX_LIFE_MS) close(x);
      }
      if (open.length) idleTimer = setTimeoutRaw(check, 400);
    }, IDLE_MS);
  }

  api.act = (name, fields) => {
    const act = newAct(name, fields);
    afterPaint(() => { act.paint = now(); rec({ k: "show", a: act.a, n: "paint", w: act.paint }); maybeSettle(act); });
    touched(act);
    return act.a;
  };
  api.step = (name, fields) => {
    const act = owner();
    const r = Object.assign({ k: "step", a: act ? act.a : undefined, n: name, w: now() }, fields || {});
    if (typeof r.d === "number") r.w -= r.d;
    rec(r);
  };
  api.current = () => { const x = owner(); return x ? x.a : null; };

  // ---- the events: what the user did, named by what it was on

  function describe(t) {
    if (!t || !t.closest) return "page";
    const el = t.closest("[data-a]");
    if (el) return el.dataset.a;
    const id = t.closest("[id]");
    return id ? "#" + id.id : (t.tagName || "page").toLowerCase();
  }
  function keyName(e) {
    const mods = (e.metaKey ? "Meta+" : "") + (e.ctrlKey ? "Ctrl+" : "") + (e.altKey ? "Alt+" : "") + (e.shiftKey ? "Shift+" : "");
    return mods + e.key;
  }
  const typing = (t) => t && /^(INPUT|TEXTAREA|SELECT)$/.test(t.tagName);

  const live = new WeakMap(); // event -> its action, from the capture listener to the bubble one
  function begin(e) {
    let name;
    const t = e.target;
    if (e.type === "keydown") {
      if (["Shift", "Meta", "Control", "Alt", "CapsLock"].includes(e.key)) return;
      // A letter typed in a box is its input event's action
      if (typing(t) && e.key.length === 1 && !e.metaKey && !e.ctrlKey) return;
      name = "key:" + keyName(e);
    } else if (e.type === "input") {
      name = "input:" + (t && t.id ? "#" + t.id : describe(t));
    } else {
      name = e.type + ":" + describe(t);
    }
    const asked = typeof e.timeStamp === "number" && e.timeStamp > 0 && e.timeStamp < performance.now() + 1000
      ? performance.timeOrigin + e.timeStamp : now();
    const fields = { via: e.type, delay: round(now() - asked) };
    const el = t && t.closest && t.closest("[data-a]");
    if (el && el.dataset.v) fields.v = String(el.dataset.v).slice(0, 120);
    const act = newAct(name, fields, asked);
    live.set(e, { act, t0: now() });
    afterPaint(() => { act.paint = now(); rec({ k: "show", a: act.a, n: "paint", w: act.paint }); maybeSettle(act); });
    touched(act);
  }
  function end(e) {
    const x = live.get(e);
    if (!x) return;
    live.delete(e);
    // The page's own handlers for it: from the first listener to the last
    part(x.act, "handlers", x.t0, now() - x.t0);
  }
  for (const type of ["click", "dblclick", "keydown", "input", "change", "contextmenu"]) {
    window.addEventListener(type, begin, true);
    window.addEventListener(type, end, false);
  }

  // ---- the app's commands

  // The id of the action going on goes with the commands that hand it on as `journal`: open_items and open_list (the
  // player's first frame is this action's result) and add_job (the batch's run says which action asked for it)
  const HANDS_ON = new Set(["open_items", "open_list", "add_job"]);
  const journaled = function (cmd, args, options) {
    const act = !BACKGROUND.has(cmd) && !inBackground ? owner() : null;
    if (!act) return rawInvoke(cmd, args, options);
    const start = now();
    act.pending++;
    touched(act);
    const withId = HANDS_ON.has(cmd) && args && typeof args === "object" && !Array.isArray(args) ? Object.assign({ journal: act.a }, args) : args;
    const p = rawInvoke(cmd, withId, options);
    const finish = (ok) => {
      part(act, "invoke:" + cmd, start, now() - start);
      if (!ok) rec({ k: "step", a: act.a, n: "invoke-failed:" + cmd, w: now() });
      // After what the caller does with the answer (its .then / await, queued before this task)
      setTimeoutRaw(() => { act.pending--; touched(act); maybeSettle(act); }, 0);
    };
    p.then(() => finish(true), () => finish(false));
    return p;
  };
  // The page's scripts ask for commands through Journal.invoke (state.js's `invoke`): the app's own object is frozen in the
  // real window (Tauri's window.__TAURI__.core), so swapping its invoke works only in tests. Tried anyway, for a script
  // that still asks the app's object directly
  api.invoke = journaled;
  try { tauri.invoke = journaled; } catch (e) { /* frozen: Journal.invoke only */ }
  const coreJournaled = tauri.invoke === journaled;

  // The page's timers: what an interval (the polls) starts is the page's own, not the action's
  const setTimeoutRaw = window.setTimeout.bind(window);
  const setIntervalRaw = window.setInterval.bind(window);
  window.setInterval = function (fn, ms, ...rest) {
    if (typeof fn !== "function") return setIntervalRaw(fn, ms, ...rest);
    return setIntervalRaw(function (...a) {
      inBackground++;
      try { return fn.apply(this, a); } finally { inBackground--; }
    }, ms, ...rest);
  };

  // ---- pictures

  document.addEventListener("load", (e) => {
    if (!e.target || e.target.tagName !== "IMG") return;
    const act = owner();
    if (!act) return;
    const t = now();
    act.images++;
    if (act.firstImage === null) {
      act.firstImage = t;
      rec({ k: "show", a: act.a, n: "picture", w: t });
    }
    act.lastImage = t;
    touched(act);
  }, true);

  // ---- the page's own drawing: its render*/load* functions, once every script is loaded

  function wrapRenders() {
    for (const name of Object.getOwnPropertyNames(window)) {
      if (!/^(render|load|derive|apply|show)[A-Z]/.test(name)) continue;
      const fn = safe(() => window[name]);
      if (typeof fn !== "function" || fn.__journal) continue;
      const wrapped = function (...a) {
        const act = owner();
        if (!act) return fn.apply(this, a);
        const t0 = now();
        const r = fn.apply(this, a);
        if (r && typeof r.then === "function") {
          act.pending++;
          r.then(() => {}, () => {}).then(() => { part(act, "async:" + name, t0, now() - t0); setTimeoutRaw(() => { act.pending--; touched(act); maybeSettle(act); }, 0); });
        } else part(act, "render:" + name, t0, now() - t0);
        return r;
      };
      wrapped.__journal = true;
      try { window[name] = wrapped; } catch (e) { /* a const: not reachable this way */ }
    }
  }

  // ---- the start: from the app's process starting (journal.rs) to the page usable

  let startAct = null;
  function hello() {
    rawInvoke("journal_hello").then((h) => {
      if (!h || h.off) return;
      pid = String(h.pid || "");
      rec({ k: "load", w: performance.timeOrigin, v: VERSION, page: true, invokes: true, core: coreJournaled });
      rec({ k: "step", a: h.start_a, n: "page-start", w: performance.timeOrigin });
      // The start is an action the app opened (its id from journal.rs): the page's work until it settles is its
      startAct = { a: h.start_a, name: "library/start", w: h.start_w, last: now(), pending: 0, parts: {}, images: 0,
                   firstImage: null, lastImage: null, settled: null, closed: false, fromApp: true };
      open.push(startAct);
      afterPaint(() => { startAct.paint = now(); rec({ k: "show", a: startAct.a, n: "paint", w: startAct.paint }); maybeSettle(startAct); });
      touched(startAct);
    }, () => {});
  }

  function flush() {
    if (!buf.length) return;
    const t0 = performance.now();
    const lines = buf;
    buf = [];
    rawInvoke("journal_write", { lines }).catch(() => {});
    cost.flushes++;
    cost.flush_ms += performance.now() - t0;
  }
  let lastCost = performance.now();
  setIntervalRaw(() => {
    if (performance.now() - lastCost > 60000) {
      lastCost = performance.now();
      rec({ k: "cost", w: now(), records: cost.records, rec_ms: round(cost.rec_ms), flushes: cost.flushes, flush_ms: round(cost.flush_ms), dropped: cost.dropped });
    }
    flush();
  }, FLUSH_MS);
  window.addEventListener("pagehide", flush);
  api.flush = flush;
  api._cost = cost;

  hello();
  if (document.readyState === "loading") {
    document.addEventListener("DOMContentLoaded", () => {
      rec({ k: "step", a: startAct ? startAct.a : undefined, n: "dom-ready", w: now() });
      wrapRenders();
    });
  } else wrapRenders();
})();
