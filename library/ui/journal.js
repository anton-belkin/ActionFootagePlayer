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
//
// The usage trace (docs/design/usage-trace.md), when the app says it's on (journal_hello's "trace": this computer's
// journal.conf trace=yes): what the actions were about, in the same records. Each action's element by its words
// (`label`; a choice's or a box's value), the dialogs, the toast and the banners as they show (`text` records), a
// command's error and the page's own (`log`), and "Something's wrong here" (Cmd/Ctrl+Shift+B, the flag at the bottom
// left: a `mark` record with what the page showed, then a line about it). No picture of the window here: capturing
// it would need the Screen Recording permission (the player's markers have one).
//   Journal.trace                 whether it's on (after the app answered)
//   Journal.say(n, text, fields)  a text shown (with it on)
//   Journal.mark(via)             a marker, as the key
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

  const api = { act: () => null, step() {}, current: () => null, enabled, now, _rec: rec, _open: open, trace: false,
                say() {}, mark() {} };
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
    // The usage trace's own note box: not an action of the Library's
    if (t && t.closest && t.closest("#afpTraceNote")) return;
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
    if (api.trace) traceFields(e, t, fields);
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
    if (!act) {
      const p = rawInvoke(cmd, args, options);
      if (api.trace && cmd !== "journal_write") p.catch((e) => failed(cmd, e, null));
      return p;
    }
    const start = now();
    act.pending++;
    touched(act);
    const withId = HANDS_ON.has(cmd) && args && typeof args === "object" && !Array.isArray(args) ? Object.assign({ journal: act.a }, args) : args;
    const p = rawInvoke(cmd, withId, options);
    const finish = (ok, e) => {
      part(act, "invoke:" + cmd, start, now() - start);
      if (!ok) rec({ k: "step", a: act.a, n: "invoke-failed:" + cmd, w: now() });
      if (!ok && api.trace) failed(cmd, e, act);
      // After what the caller does with the answer (its .then / await, queued before this task)
      setTimeoutRaw(() => { act.pending--; touched(act); maybeSettle(act); }, 0);
    };
    p.then(() => finish(true), (e) => finish(false, e));
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

  // ---- the usage trace (docs/design/usage-trace.md): only once the app said it's on

  const TEXT_MAX = 500;
  const clip = (x, n) => { x = String(x == null ? "" : x).replace(/\s+/g, " ").trim(); n = n || TEXT_MAX; return x.length > n ? x.slice(0, n) + "…" : x; };
  function say(n, text, fields) {
    if (!api.trace) return;
    const act = owner();
    rec(Object.assign({ k: "text", n, text: clip(text), w: now(), a: act ? act.a : undefined }, fields || {}));
  }
  api.say = say;
  let lastFailure = { key: "", w: 0 };
  function failed(cmd, e, act) {
    const text = clip(e && e.message ? e.message : e);
    const key = cmd + "\u0000" + text;
    // The same failure again within 10 s (a poll's): written once
    if (key === lastFailure.key && now() - lastFailure.w < 10000) return;
    lastFailure = { key, w: now() };
    rec({ k: "log", level: "error", prefix: "invoke:" + cmd, text, w: now(), a: act ? act.a : undefined });
  }

  // What the action was on, in the words it shows: a button's or a tab's, a box's name; a choice's or a box's value
  function words(el) {
    if (!el) return "";
    const own = el.getAttribute && (el.getAttribute("aria-label") || el.getAttribute("title"));
    if (own) return own;
    if (/^(INPUT|TEXTAREA|SELECT)$/.test(el.tagName)) {
      const lab = el.id && document.querySelector('label[for="' + (window.CSS && CSS.escape ? CSS.escape(el.id) : el.id) + '"]');
      return (lab && lab.textContent) || el.getAttribute("placeholder") || el.name || el.id || el.tagName.toLowerCase();
    }
    return el.textContent || "";
  }
  function traceFields(e, t, fields) {
    if (!t || !t.closest) return;
    const el = t.closest("button, a, [role=tab], [role=menuitem], [role=option], [data-a], label, input, select, textarea") || t;
    const label = clip(words(el), 80);
    if (label) fields.label = label;
    if ((e.type === "change" || e.type === "input") && /^(INPUT|TEXTAREA|SELECT)$/.test(t.tagName)) {
      fields.value = t.type === "checkbox" || t.type === "radio" ? !!t.checked : clip(t.value, 120);
    }
  }

  // The dialogs, the toast and the banners, as they show: checked once a task after the page changed
  const shownNow = new Set();
  let checking = false;
  function visible(el) { return el && el.isConnected && !el.closest("[hidden]"); }
  function dialogTitle(d) {
    const id = d.getAttribute("aria-labelledby");
    const h = (id && document.getElementById(id)) || d.querySelector("h1, h2, h3, header");
    return clip(h ? h.textContent : d.textContent, 160);
  }
  function check() {
    checking = false;
    const now_ = new Set();
    for (const d of document.querySelectorAll('[role="dialog"], [role="alertdialog"]')) {
      if (d.id === "afpTraceNote" || !visible(d)) continue;
      const title = dialogTitle(d);
      const key = "dialog\u0000" + title;
      now_.add(key);
      if (!shownNow.has(key)) {
        const body = d.querySelector(".sb");
        say("dialog", title, { body: body ? clip(body.textContent, 300) : undefined });
      }
    }
    for (const [n, id, textId] of [["toast", "toast", "toastMsg"], ["banner", "error"], ["banner", "engine"], ["banner", "unsaved"]]) {
      const el = document.getElementById(id);
      if (!visible(el)) continue;
      const text = clip((textId && document.getElementById(textId) || el).textContent);
      if (!text) continue;
      const key = n + "\u0000" + id + "\u0000" + text;
      now_.add(key);
      if (!shownNow.has(key)) say(n, text, n === "banner" ? { id } : undefined);
    }
    shownNow.clear();
    for (const k of now_) shownNow.add(k);
  }
  function changed() {
    if (checking) return;
    checking = true;
    setTimeoutRaw(check, 0);
  }

  // "Something's wrong here": a marker with what the page showed, written at once; then a line about it
  let marks = 0;
  function mark(via) {
    if (!api.trace) return null;
    const act = owner();
    const id = "L" + pid + ".m" + (++marks);
    const dialogs = [...shownNow].filter((k) => k.startsWith("dialog\u0000")).map((k) => k.split("\u0000")[1]);
    const toast = document.getElementById("toast");
    rec({ k: "mark", id, w: now(), a: act ? act.a : undefined, via, actor: api.actor, title: document.title,
          dialogs: dialogs.length ? dialogs : undefined,
          toast: visible(toast) ? clip(toast.textContent, 300) : undefined,
          focus: document.activeElement && document.activeElement !== document.body ? clip(words(document.activeElement), 80) : undefined,
          shot: null, no_shot: "the Library's window isn't captured (it would need the Screen Recording permission)" });
    flush();
    askNote(id);
    return id;
  }
  api.mark = mark;
  function askNote(id) {
    const old = document.getElementById("afpTraceNote");
    if (old) old.remove();
    const box = document.createElement("div");
    box.id = "afpTraceNote";
    box.setAttribute("role", "dialog");
    box.setAttribute("aria-label", "Something's wrong here");
    box.style.cssText = "position:fixed;left:16px;bottom:56px;z-index:10000;width:340px;padding:12px;border-radius:10px;" +
      "background:var(--surface,#fff);color:var(--fg,#1c1c1f);border:1px solid var(--line,#ddd);box-shadow:0 6px 24px rgba(0,0,0,.3);font:13px var(--font,system-ui)";
    box.innerHTML = '<div style="font-weight:600;margin-bottom:6px">Something’s wrong here</div>' +
      '<div style="color:var(--muted,#5e5e65);margin-bottom:8px">Marked in the usage trace. A line about it for the bug, if you like.</div>' +
      '<input type="text" id="afpTraceNoteText" placeholder="What’s wrong?" style="width:100%;box-sizing:border-box;padding:6px 8px;border-radius:6px;border:1px solid var(--line,#ddd);background:var(--win,transparent);color:inherit;-webkit-user-select:text;user-select:text">' +
      '<div style="display:flex;justify-content:flex-end;gap:8px;margin-top:10px">' +
      '<button type="button" class="btn" data-a="trace-note-skip">No note</button><button type="button" class="btn pri" data-a="trace-note-keep">Keep</button></div>';
    document.body.appendChild(box);
    const input = box.querySelector("input");
    const done = (keep) => {
      const text = keep ? clip(input.value) : "";
      rec({ k: "mark-note", id, w: now(), note: text || undefined, cancelled: keep ? undefined : true });
      flush();
      box.remove();
    };
    box.addEventListener("click", (e) => {
      const b = e.target.closest && e.target.closest("[data-a]");
      if (b && b.dataset.a === "trace-note-keep") done(true);
      else if (b && b.dataset.a === "trace-note-skip") done(false);
    });
    input.addEventListener("keydown", (e) => {
      if (e.key === "Enter") { e.preventDefault(); e.stopPropagation(); done(true); }
      else if (e.key === "Escape") { e.preventDefault(); e.stopPropagation(); done(false); }
      else e.stopPropagation();
    });
    try { input.focus(); } catch (e) { /* no focus in a test */ }
  }
  function markButton() {
    if (document.getElementById("afpTraceMark")) return;
    const b = document.createElement("button");
    b.type = "button";
    b.id = "afpTraceMark";
    b.dataset.a = "trace-mark";
    b.title = "Something’s wrong here (" + (/Mac/.test(navigator.platform || "") ? "⇧⌘B" : "Ctrl+Shift+B") + ")";
    b.setAttribute("aria-label", "Something’s wrong here");
    b.textContent = "⚑";
    b.style.cssText = "position:fixed;left:12px;bottom:12px;z-index:9999;width:32px;height:32px;border-radius:16px;border:1px solid var(--line,#ddd);" +
      "background:var(--surface,#fff);color:var(--muted,#5e5e65);font-size:16px;line-height:1;cursor:pointer;padding:0";
    b.addEventListener("click", () => mark("click"));
    document.body.appendChild(b);
  }

  function startTrace(actor) {
    api.trace = true;
    api.actor = actor || "user";
    // Cmd+Shift+B (Ctrl+Shift+B elsewhere): after begin() made it an action
    window.addEventListener("keydown", (e) => {
      if ((e.metaKey || e.ctrlKey) && e.shiftKey && !e.altKey && (e.key === "b" || e.key === "B")) {
        e.preventDefault();
        e.stopPropagation();
        mark("key");
      }
    }, true);
    window.addEventListener("error", (e) => rec({ k: "log", level: "error", prefix: "page", text: clip(e.message + (e.filename ? " (" + String(e.filename).split("/").pop() + ":" + e.lineno + ")" : "")), w: now() }));
    window.addEventListener("unhandledrejection", (e) => rec({ k: "log", level: "error", prefix: "page", text: clip(e.reason && e.reason.message ? e.reason.message : e.reason), w: now() }));
    const go = () => {
      markButton();
      try {
        new MutationObserver(changed).observe(document.body, { subtree: true, childList: true, characterData: true, attributes: true, attributeFilter: ["hidden"] });
      } catch (e) { /* no observer: no texts */ }
      check();
    };
    if (document.body) go(); else document.addEventListener("DOMContentLoaded", go);
  }

  // ---- the start: from the app's process starting (journal.rs) to the page usable

  let startAct = null;
  function hello() {
    rawInvoke("journal_hello").then((h) => {
      if (!h || h.off) return;
      pid = String(h.pid || "");
      if (h.trace) startTrace(h.actor);
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
