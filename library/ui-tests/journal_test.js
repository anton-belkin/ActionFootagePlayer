"use strict";
// The responsiveness journal's page part (../ui/journal.js, docs/design/responsiveness-journal.md): a click, a keystroke in
// the search box and a double-click each come out as an action with its paint, its commands and its end; the
// double-click hands its id to open_items; the polls are nobody's step; and what a record costs.
//   cd library/ui-tests && npm install && node --test journal_test.js

const test = require("node:test");
const assert = require("node:assert/strict");
const { open, sleep } = require("./page");

const opened = [];
test.afterEach(() => { while (opened.length) opened.pop().close(); });

async function start(o) {
  const lines = [];
  const page = await open(Object.assign({ delay: { scan: 40 } }, o || {}, {
    commands: Object.assign({
      journal_hello: () => ({ pid: 77, start_a: "L77.start", start_w: Date.now() - 500 }),
      journal_write: (a) => { lines.push(...a.lines); return null; },
      open_items: () => true,
    }, (o && o.commands) || {}),
  }));
  opened.push(page);
  const records = async () => {
    page.window.Journal.flush();
    await sleep(5);
    return lines.map((l) => JSON.parse(l));
  };
  return { page, records };
}

// Waits for the actions going on to end (the page's IDLE_MS, 1.2 s, with nothing more for them)
const ended = (page) => page.until(() => page.window.Journal._open.length === 0, 8000, "the actions to end");

test("a click: its action, the frame after it, its commands, its end", async () => {
  const { page, records } = await start();
  await ended(page);
  page.click('[data-a="view"][data-v="sources"]');
  await ended(page);
  const rs = await records();
  const act = rs.find((r) => r.k === "act" && r.n === "click:view");
  assert.ok(act, "the click is an action");
  assert.equal(act.v, "sources");
  assert.equal(act.s, "page");
  assert.match(act.a, /^L77\.p\d+$/);
  const paint = rs.find((r) => r.k === "show" && r.a === act.a && r.n === "paint");
  assert.ok(paint && paint.w >= act.w, "a frame drawn after it");
  const done = rs.find((r) => r.k === "done" && r.a === act.a);
  assert.ok(done, "it ended");
  assert.ok(done.parts.handlers, "the page's own handlers timed");
  assert.ok(done.settled >= paint.w, "settled after its first paint");
  // The start: the app's action, the page's work in it
  assert.ok(rs.some((r) => r.k === "done" && r.a === "L77.start" && r.parts["invoke:scan"]), "the start's scans are its own");
  // The polls are nobody's
  for (const r of rs.filter((r) => r.k === "done")) {
    assert.ok(!r.parts["invoke:snapshot"] && !r.parts["invoke:probes_since"], "no poll in " + r.n);
  }
});

test("a keystroke in the search box: its action, the list drawn in its handler", async () => {
  const { page, records } = await start();
  await ended(page);
  page.type("#q", "reef");
  await ended(page);
  const rs = await records();
  const act = rs.find((r) => r.k === "act" && r.n === "input:#q");
  assert.ok(act);
  const done = rs.find((r) => r.k === "done" && r.a === act.a);
  assert.ok(done.parts["render:renderMain"], "the list drawn, timed as its step");
});

test("a double-click: the list sent to the player carries the action's id (the player's first frame is its result)", async () => {
  const { page, records } = await start({ commands: { open_list: () => true } });
  await ended(page);
  page.get('S.path = ["Day 1"]; renderAll()');
  await ended(page);
  const row = page.row("Day 1/A.MP4");
  row.dispatchEvent(new page.window.MouseEvent("dblclick", { bubbles: true, cancelable: true, detail: 2 }));
  // (the Library's list as the playlist, open_list; one item by itself, open_items)
  const opened = () => page.calls("open_list").concat(page.calls("open_items"));
  await page.until(() => opened().length === 1);
  await ended(page);
  const rs = await records();
  const act = rs.find((r) => r.k === "act" && r.n === "dblclick:pick");
  assert.ok(act);
  const call = opened()[0];
  assert.equal(call.args.journal, act.a);
  // The other commands don't get it
  assert.ok(page.calls("scan").every((c) => !c.args || c.args.journal === undefined));
  const done = rs.find((r) => r.k === "done" && r.a === act.a);
  assert.ok(done.parts["invoke:" + call.cmd]);
});

test("the real window's frozen __TAURI__.core: the commands are journaled all the same, open_items gets the id", async () => {
  const { page, records } = await start({ frozen: true, commands: { open_list: () => true } });
  assert.ok(Object.isFrozen(page.window.__TAURI__.core));
  await ended(page);
  page.get('S.path = ["Day 1"]; renderAll()');
  await ended(page);
  page.row("Day 1/A.MP4").dispatchEvent(new page.window.MouseEvent("dblclick", { bubbles: true, cancelable: true, detail: 2 }));
  const opened = () => page.calls("open_list").concat(page.calls("open_items"));
  await page.until(() => opened().length === 1);
  await ended(page);
  const rs = await records();
  const load = rs.find((r) => r.k === "load");
  assert.equal(load.invokes, true);
  assert.equal(load.core, false, "the app's object untouched");
  assert.ok(rs.some((r) => r.k === "done" && r.a === "L77.start" && r.parts["invoke:scan"]), "the start's scans journaled");
  const act = rs.find((r) => r.k === "act" && r.n === "dblclick:pick");
  assert.equal(opened()[0].args.journal, act.a, "the player is handed the action's id");
  const done = rs.find((r) => r.k === "done" && r.a === act.a);
  assert.ok(done.parts["invoke:" + opened()[0].cmd]);
});

test("a slow command is a record of its own; a failing one is said", async () => {
  const { page, records } = await start({ delay: { scan: 40, set_focus: 0, recent: 80 }, commands: { recent: () => { throw new Error("no"); } } });
  await ended(page);
  page.click('[data-a="menu"]');
  await ended(page);
  const rs = await records();
  const act = rs.find((r) => r.k === "act" && r.n === "click:menu");
  assert.ok(rs.some((r) => r.k === "step" && r.a === act.a && r.n === "invoke:recent" && r.d >= 50), "slow: its own record");
  assert.ok(rs.some((r) => r.k === "step" && r.a === act.a && r.n === "invoke-failed:recent"));
});

// The usage trace (docs/design/usage-trace.md): on when the app says so
const traced = (o) => start(Object.assign({}, o || {}, {
  commands: Object.assign({ journal_hello: () => ({ pid: 77, start_a: "L77.start", start_w: Date.now() - 500, trace: true, actor: "claude" }) },
                          (o && o.commands) || {}),
}));

test("the trace off (the default): no words, no texts, no flag", async () => {
  const { page, records } = await start();
  await ended(page);
  page.click('[data-a="view"][data-v="sources"]');
  page.get('toast("Saved")');
  await ended(page);
  const rs = await records();
  assert.equal(page.window.Journal.trace, false);
  assert.ok(!rs.find((r) => r.k === "act" && r.n === "click:view").label, "no label");
  assert.ok(!rs.some((r) => r.k === "text" || r.k === "log" || r.k === "mark"), "no trace records");
  assert.equal(page.window.document.getElementById("afpTraceMark"), null, "no flag");
});

test("the trace on: what was clicked in its words, the toast and a dialog's text, a command's error", async () => {
  const { page, records } = await traced({ delay: { scan: 40, recent: 5 }, commands: { recent: () => { throw new Error("the share isn't there"); } } });
  await ended(page);
  assert.equal(page.window.Journal.trace, true);
  page.click('[data-a="view"][data-v="sources"]');
  await ended(page);
  page.get('toast("Taken in: 3 files")');
  await sleep(5);
  page.get('$("ask").hidden = false');
  await sleep(5);
  page.click('[data-a="menu"]');
  await ended(page);
  const rs = await records();
  const act = rs.find((r) => r.k === "act" && r.n === "click:view");
  const el = page.window.document.querySelector('[data-a="view"][data-v="sources"]');
  assert.equal(act.label, (el.getAttribute("aria-label") || el.getAttribute("title") || el.textContent).replace(/\s+/g, " ").trim().slice(0, 80));
  assert.ok(act.label, "a label");
  assert.ok(rs.some((r) => r.k === "text" && r.n === "toast" && r.text.startsWith("Taken in: 3 files")), "the toast's words");
  assert.ok(rs.some((r) => r.k === "text" && r.n === "dialog" && r.text === "Jobs are still in the queue" && /Closing the window/.test(r.body)), "the dialog's title and words");
  const err = rs.find((r) => r.k === "log" && r.prefix === "invoke:recent");
  assert.ok(err && err.level === "error" && /the share isn't there/.test(err.text), "the command's error, in its words");
  // Written once though shown on: the toast is one record
  assert.equal(rs.filter((r) => r.k === "text" && r.n === "toast").length, 1);
});

test("the trace on: a choice's value; Something's wrong here by its key, then its note", async () => {
  const { page, records } = await traced();
  await ended(page);
  const w = page.window;
  assert.ok(w.document.getElementById("afpTraceMark"), "the flag");
  w.dispatchEvent(new w.KeyboardEvent("keydown", { key: "B", metaKey: true, shiftKey: true, bubbles: true, cancelable: true }));
  const input = w.document.getElementById("afpTraceNoteText");
  assert.ok(input, "a line asked");
  input.value = "the list jumped to the top";
  input.dispatchEvent(new w.KeyboardEvent("keydown", { key: "Enter", bubbles: true, cancelable: true }));
  assert.equal(w.document.getElementById("afpTraceNote"), null, "the box gone");
  // The flag clicked, and Esc: a marker with no note
  w.document.getElementById("afpTraceMark").dispatchEvent(new w.MouseEvent("click", { bubbles: true, cancelable: true }));
  w.document.getElementById("afpTraceNoteText").dispatchEvent(new w.KeyboardEvent("keydown", { key: "Escape", bubbles: true, cancelable: true }));
  await ended(page);
  const rs = await records();
  const marks = rs.filter((r) => r.k === "mark");
  assert.equal(marks.length, 2);
  assert.equal(marks[0].via, "key");
  assert.equal(marks[0].actor, "claude");
  assert.equal(marks[1].via, "click");
  assert.match(marks[0].id, /^L77\.m1$/);
  const note = rs.find((r) => r.k === "mark-note" && r.id === marks[0].id);
  assert.equal(note.note, "the list jumped to the top");
  assert.equal(rs.find((r) => r.k === "mark-note" && r.id === marks[1].id).cancelled, true);
  assert.ok(rs.some((r) => r.k === "act" && r.n === "key:Meta+Shift+B"), "the key is an action");
  assert.ok(!rs.some((r) => r.k === "act" && /afpTraceNote/.test(r.n)), "typing the note isn't the Library's action");
});

test("cost: a record takes microseconds; the page doesn't write, the app does (every 2 s)", async () => {
  const { page } = await start();
  const J = page.window.Journal;
  const n = 20000;
  const t0 = page.window.performance.now();
  for (let i = 0; i < n; i++) J._rec({ k: "step", n: "bench", w: J.now(), a: "L77.p1", d: 1.5 });
  const us = (page.window.performance.now() - t0) * 1000 / n;
  console.log(`journal.js: ${us.toFixed(2)} µs a record (jsdom, node)`);
  assert.ok(us < 50, us + " µs a record");
});
