"use strict";
// How long the Library's window takes to show a change, measured on the page itself (jsdom) over a project of
// thousands of clips, the app's answers taking as long as src/perf.rs measured them (docs/library-responsiveness.md):
//
//   (cd library && AFL_PERF_DIR=/tmp/afl-perf cargo test --release -- --ignored where_the_time_goes --nocapture)
//   AFL_PERF_DIR=/tmp/afl-perf node library/ui-tests/page_perf.js [label]
//
// For each change: when the list shows it, when the page has stopped asking the app, and what it asked for. Three
// times: the app answering at once (the page's own work), as this disk answers, as a NAS would.

const fs = require("fs");
const path = require("path");
const { open } = require("./page");
const { fake } = require("./fakeapp");

const dir = process.env.AFL_PERF_DIR;
if (!dir || !fs.existsSync(path.join(dir, "timings.json"))) { console.error("AFL_PERF_DIR: the folder src/perf.rs's where_the_time_goes left (timings.json, views/)"); process.exit(2); }
const label = process.argv[2] || "now";
const timings = JSON.parse(fs.readFileSync(path.join(dir, "timings.json"), "utf8"));
const ms = {};
for (const t of ["reads", "commands"]) for (const r of timings[t]) ms[r.what] = { local: r.local_ms, nas: r.nas_ms };
const readViews = () => { const v = {}; for (const n of ["footage", "moments", "stills", "exports"]) v[n] = JSON.parse(fs.readFileSync(path.join(dir, "views", n + ".json"), "utf8")); return v; };

// How long a command takes in a mode ("own": at once), by how many items it's for
function delayFor(mode) {
  if (mode === "own") return () => 0;
  const of = (what) => (ms[what] ? ms[what][mode] : 0);
  const sized = (stem, n, unit) => { const k = n <= 1 ? 1 : n <= 20 ? 20 : 200; const base = of(stem + k + unit); return k === 200 || k === 1 ? base * (k === 200 ? n / 200 : 1) : base * n / 20; };
  return (cmd, a) => {
    switch (cmd) {
      case "project": return of("project");
      case "scan": return of("scan " + a.view);
      case "set_tags": return sized("set_tags: add to ", a.items.length, a.items[0] && a.items[0].kind === "moment" ? " moment(s)" : " clip(s)");
      case "set_locations": return sized("set_locations: ", a.entries.reduce((n, e) => n + e.ids.length, 0), " clip(s)");
      case "set_source": return sized("set_source: ", a.entries.length, " clip(s)");
      case "rename_item": return of(a.kind === "folder" ? "rename_item: a folder of 10 moments" : "rename_item: a moment");
      case "delete_items": return sized("delete_items: ", a.items.length, " moment(s)");
      default: return 0;
    }
  };
}

const FOLDER = ["Trip 01", "Day 01"];
// The clips of a folder of Footage that have a preview (shown there), by how many
const clipIds = (views, n) => views.footage.items.filter((it) => it.preview).slice(0, n).map((it) => it.id);
const row = (page, id) => page.$('#main [data-v="' + page.window.CSS.escape("i:" + id) + '"]');
const item = (page, view, id) => page.get("(DATA." + view + ".items.find((x) => x.id === " + JSON.stringify(id) + ") || null)");

const SCENARIOS = [
  { name: "Add a tag to 1 clip", async run(page, views) {
    const ids = clipIds(views, 1);
    page.get("S.path = " + JSON.stringify(FOLDER) + "; showTab('tags'); renderAll()");
    page.select(ids);
    page.type("#tagdraft", "perf");
    return { act: () => page.click('[data-a="tag"]'), shown: () => /perf/.test(row(page, ids[0]).querySelector(".chips").textContent) };
  } },
  { name: "Add a tag to 200 clips", async run(page, views) {
    const ids = clipIds(views, 200);
    page.get("S.path = " + JSON.stringify(FOLDER) + "; showTab('tags'); renderAll()");
    page.select(ids);
    page.type("#tagdraft", "perf");
    return { act: () => page.click('[data-a="tag"]'), shown: () => /perf/.test(row(page, ids[0]).querySelector(".chips").textContent) };
  } },
  { name: "Set a location for 20 clips", async run(page, views) {
    const ids = clipIds(views, 20);
    page.get("S.path = " + JSON.stringify(FOLDER) + "; showTab('geo'); renderAll()");
    page.select(ids);
    page.type("#geohand", "27.7333, 34.25");
    return { act: () => page.click('[data-a="geohand"]'), shown: () => { const it = item(page, "footage", ids[0]); return it && it.geo && it.geo.lat === 27.7333; } };
  } },
  { name: "Source settings for 20 clips", async run(page, views) {
    const ids = clipIds(views, 20);
    page.get("S.path = " + JSON.stringify(FOLDER) + "; showTab('source'); renderAll()");
    page.select(ids);
    page.get("srcDraftSet('rotation', '180')");
    return { act: () => page.click('[data-a="srcapply"]'), shown: () => /turned/.test(row(page, ids[0]).querySelector(".sub").textContent) };
  } },
  { name: "Rename a moment", async run(page, views) {
    const m = views.moments.items.find((it) => it.dir === "_Moments/Trip 01/Day 01");
    page.click('[data-a="view"][data-v="moments"]');
    await page.until(() => page.get("S.view === 'moments' && !S.scanning.moments"), 60000);
    await page.quiet(30, 60000);
    page.get("S.path = " + JSON.stringify(FOLDER) + "; renderAll()");
    page.select([m.id]);
    page.get("startRename()");
    page.$("#rename").value = "Turtle";
    return { act: () => page.key("Enter", {}, page.$("#rename")), shown: () => page.$$("#main .nm").some((e) => e.textContent === "Turtle") };
  } },
  { name: "Delete a moment", async run(page, views) {
    const m = views.moments.items.filter((it) => it.dir === "_Moments/Trip 01/Day 01")[1];
    page.click('[data-a="view"][data-v="moments"]');
    await page.until(() => page.get("S.view === 'moments' && !S.scanning.moments"), 60000);
    await page.quiet(30, 60000);
    page.get("S.path = " + JSON.stringify(FOLDER) + "; renderAll()");
    page.select([m.id]);
    return { act: () => page.get("deleteSelection()"), shown: () => !row(page, m.id) };
  } },
];

(async () => {
  const only = process.env.AFL_PERF_ONLY;
  const modes = (process.env.AFL_PERF_MODES || "own,local,nas").split(",");
  const out = [];
  for (const sc of SCENARIOS) {
    if (only && !sc.name.toLowerCase().includes(only.toLowerCase())) continue;
    for (const mode of modes) {
      const views = readViews();
      const app = fake(views);
      // Opening isn't what's measured: the project opens with the app answering at once
      let live = false;
      const d = delayFor(mode);
      const page = await open({ views, commands: app.commands, delay: (cmd, a) => (live ? d(cmd, a) : 0), openTimeout: 120000 });
      app.attach(page);
      const { act, shown } = await sc.run(page, views);
      await page.quiet(30, 120000);
      live = true;
      const mark = page.mark();
      const t = Date.now();
      act();
      await page.until(shown, 600000, sc.name + " to show");
      const tShown = Date.now() - t;
      await page.quiet(150, 600000);
      const tQuiet = Date.now() - t - 150;
      const asked = page.since(mark);
      const count = {};
      for (const c of asked) { const k = c.cmd === "scan" ? "scan " + c.args.view : c.cmd; count[k] = (count[k] || 0) + 1; }
      const r = { scenario: sc.name, mode, shown_ms: tShown, quiet_ms: Math.max(tShown, tQuiet), asked: count, errors: page.errors.map(String) };
      out.push(r);
      console.error(sc.name + " [" + mode + "]: shown " + r.shown_ms + " ms, quiet " + r.quiet_ms + " ms, " + JSON.stringify(count) + (r.errors.length ? " ERRORS " + r.errors.join("; ") : ""));
      page.close();
    }
  }
  const names = { own: "the page's own work", local: "this disk", nas: "as a NAS" };
  console.log("\n### The window, " + timings.clips + " clips (" + label + ")\n\n| Change | Answers taking | Shown after, ms | Page done after, ms | Asked of the app |\n|---|---|---:|---:|---|");
  for (const r of out) console.log("| " + r.scenario + " | " + names[r.mode] + " | " + r.shown_ms + " | " + r.quiet_ms + " | " + Object.keys(r.asked).map((k) => k + (r.asked[k] > 1 ? " ×" + r.asked[k] : "")).join(", ") + " |");
  fs.writeFileSync(path.join(dir, "page-" + label + ".json"), JSON.stringify(out, null, 1));
  process.exit(0);
})().catch((e) => { console.error(e); process.exit(1); });
