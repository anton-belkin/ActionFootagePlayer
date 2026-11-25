"use strict";
// The Library's start (../ui/loading.js loadProject, docs/design/library-responsiveness.md): each view read once, and never two
// reads of the views at the same time. The 2026-11-24 journal had moments and footage read twice at every start, side by
// side: the catalog, read as the project opened, told the page "something changed, can't say what" and the page read
// everything again alongside its own first read.
//   cd library/ui-tests && npm install && node --test start_scans_test.js

const test = require("node:test");
const assert = require("node:assert/strict");
const { open } = require("./page");

const opened = [];
test.afterEach(() => { while (opened.length) opened.pop().close(); });

const scans = (page) => page.calls("scan").map((c) => ({ view: c.args.view, at: c.at, end: c.at + c.took }));
function overlapping(list) {
  const out = [];
  for (let i = 0; i < list.length; i++) for (let j = i + 1; j < list.length; j++) {
    const a = list[i], b = list[j];
    if (a.view === b.view && a.at < b.end && b.at < a.end) out.push(a.view);
  }
  return out;
}

test("a plain start: each view read once", async () => {
  const page = await open({ delay: { scan: 30 } });
  opened.push(page);
  await page.quiet(200);
  const n = {};
  for (const s of scans(page)) n[s.view] = (n[s.view] || 0) + 1;
  assert.deepEqual(n, { moments: 1, footage: 1, stills: 1, exports: 1 });
});

test("the catalog's \"can't say what changed\" during the start: read again after the start's read, not alongside it", async () => {
  let told = false;
  const page = await open({
    delay: { scan: 60 },
    // (the app's answer to a change it can't name: read the views again)
    commands: { changes_since: () => ({ upto: 2, changed: null }) },
    atStart: (cmd, args, rec, app) => {
      if (cmd === "scan" && args.view === "moments" && !told) { told = true; app.state.changes = 2; setTimeout(() => app.emit("catalog-changed", 2), 5); }
    },
  });
  opened.push(page);
  await page.quiet(300);
  const list = scans(page);
  assert.deepEqual(overlapping(list), [], "no view read twice at once: " + JSON.stringify(list));
  assert.ok(told);
});

test("tasks finished before the project opened: no second read once it has", async () => {
  const page = await open({
    delay: { scan: 30 },
    commands: { snapshot: () => ({ project: "/P", jobs: [{ id: 1, state: "done", kind: "render", label: "x", done: 1, total: 1 }], close_asked: false, macos: true,
      changes: 1, quiet_ms: 5000, catalog: "", missing: 0, moving: null, close_waits: false }) },
  });
  opened.push(page);
  await page.sleep(1500); // (a couple of the page's polls, 600 ms)
  await page.quiet(200);
  const n = {};
  for (const s of scans(page)) n[s.view] = (n[s.view] || 0) + 1;
  assert.deepEqual(n, { moments: 1, footage: 1, stills: 1, exports: 1 });
});
