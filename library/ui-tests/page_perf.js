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

// Whether the page is the one that takes a command's own account of what it changed (the pinpoint update): the mock
// then answers as the app does since, each command taking the time to say it too
const UIDIR = process.env.AFL_UI || path.join(__dirname, "..", "ui");
const pinpoint = fs.readFileSync(path.join(UIDIR, "changes.js"), "utf8").includes("changes_since");
// …and the album commands' account of what they made (albums shown at once)
const albumsSay = fs.readFileSync(path.join(UIDIR, "albums.js"), "utf8").includes("albumadd");

// How long a command takes in a mode ("own": at once), by how many items it's for
function delayFor(mode) {
  if (mode === "own") return () => 0;
  const of = (what) => (ms[what] ? ms[what][mode] : 0);
  // Saying what changed, after the write (src/changed.rs): by how many items
  const saying = (n, moments) => (!pinpoint ? 0 : moments ? of("saying what changed: 20 moments") * Math.max(1, n) / 20 : n <= 1 ? of("saying what changed: 1 clip(s)") : of("saying what changed: 200 clip(s)") * n / 200);
  const sized = (stem, n, unit) => { const k = n <= 1 ? 1 : n <= 20 ? 20 : 200; const base = of(stem + k + unit); return k === 200 || k === 1 ? base * (k === 200 ? n / 200 : 1) : base * n / 20; };
  return (cmd, a) => {
    switch (cmd) {
      case "project": return of("project");
      case "scan": return of("scan " + a.view);
      case "set_tags": { const m = a.items[0] && a.items[0].kind === "moment"; return sized("set_tags: add to ", a.items.length, m ? " moment(s)" : " clip(s)") + saying(a.items.length, m); }
      case "set_locations": { const n = a.entries.reduce((k, e) => k + e.ids.length, 0); return sized("set_locations: ", n, " clip(s)") + saying(n); }
      case "set_source": return sized("set_source: ", a.entries.length, " clip(s)") + saying(a.entries.length);
      case "rename_item": return of(a.kind === "folder" ? "rename_item: a folder of 10 moments" : "rename_item: a moment") + saying(2, true);
      case "delete_items": return sized("delete_items: ", a.items.length, " moment(s)") + saying(a.items.length, true);
      // What the catalog says changed, as items (the command's own change, told back)
      case "changes_since": return of("what the catalog says changed, as items: 1 clip's own file");
      // Albums: the command, and (the page that takes it) saying what it made
      case "album_add": { const n = a.clips.length; return (n <= 1 ? of("album_add: 1 clip(s)") : of("album_add: 300 clip(s)") * n / 300) + (albumsSay ? of(n <= 1 ? "saying what changed: album_add 1" : "saying what changed: album_add 300") : 0); }
      case "album_move": return of("album_move: 50 links") * a.files.length / 50 + (albumsSay ? of("saying what changed: album_move 50") : 0);
      case "photo_add": return of("photo_add: 20 marks") * a.photos.length / 20 + (albumsSay ? of("saying what changed: photo_add 20") : 0);
      default: return 0;
    }
  };
}

const FOLDER = ["Trip 01", "Day 01"];
// The clips of a folder of Footage that have a preview (shown there), by how many
const clipIds = (views, n) => views.footage.items.filter((it) => it.preview).slice(0, n).map((it) => it.id);
const row = (page, id) => page.$('#main [data-v="' + page.window.CSS.escape("i:" + id) + '"]');
const item = (page, view, id) => page.get("(DATA." + view + ".items.find((x) => x.id === " + JSON.stringify(id) + ") || null)");

// Clips with no link yet (Originals: not in an album), and a clip's row in Originals
const unlinked = (views, n) => { const has = new Set((views.footage.links || []).map((l) => l.clip)); return views.footage.items.filter((it) => !has.has(it.id)).slice(0, n).map((it) => it.id); };
const inAlbum = (page, id) => page.get("((DATA.sources.items.find((x) => x.id === " + JSON.stringify(id) + ") || {}).albums || 0)") > 0;
// Into the album sheet as the menu opens it, a new album typed: the click on Add / Move is what's timed
async function sheet(page, mode, name) {
  await page.get("askAlbum(" + JSON.stringify(mode) + ")");
  await page.until(() => page.get("S.sheet === 'album'"), 60000);
  page.get("albumTyped(" + JSON.stringify(name) + ")");
  return () => page.click('[data-a="albumgo"]');
}
const SCENARIOS = [
  { name: "Add 1 clip to a new album", async run(page, views) {
    const ids = unlinked(views, 1);
    page.get("S.view = 'sources'; S.path = " + JSON.stringify(FOLDER) + "; renderAll()");
    page.select(ids, "i:s");
    return { act: await sheet(page, "add", "Perf"), shown: () => inAlbum(page, ids[0]) };
  } },
  { name: "Add 300 clips to a new album", async run(page, views) {
    const ids = unlinked(views, 300);
    page.get("S.view = 'sources'; S.path = []; S.flat = true; renderAll()");
    page.select(ids, "i:s");
    return { act: await sheet(page, "add", "Perf"), shown: () => inAlbum(page, ids[0]) && inAlbum(page, ids[299]) };
  } },
  { name: "Move 50 links to a new album", async run(page, views) {
    const links = views.footage.links.filter((l) => l.rel.startsWith("_Clips/Trip 01/")).slice(0, 50);
    page.get("S.view = 'footage'; S.path = ['Trip 01']; S.flat = true; renderAll()");
    page.select(links.map((l) => l.rel), "i:a");
    const last = "_Clips/Moved/" + links[49].rel.split("/").pop();
    return { act: await sheet(page, "move", "Moved"), shown: () => page.get("RAW_FOOTAGE.links.some((l) => l.rel === " + JSON.stringify(last) + ")") };
  } },
  { name: "Photo links of 20 marks", async run(page, views) {
    // 5 clips with 4 marks each (their probes say so), selected in Originals
    const ids = unlinked(views, 5);
    page.get("for (const id of " + JSON.stringify(ids) + ") { const c = RAW_FOOTAGE.items.find((x) => x.id === id); PROBES[c.path] = Object.assign({}, PROBES[c.path] || {}, { marks: { times: [2, 4, 6, 8] } }); } S.view = 'sources'; S.path = []; S.flat = true; renderAll()");
    page.select(ids, "i:s");
    const last = ids[4];
    return { act: () => page.get("photoLinks()"), shown: () => page.get("(DATA.stills.items || []).filter((p) => p.link && p.link.clip === " + JSON.stringify(last) + ").length === 4") };
  } },
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
    if (only && !only.split("|").some((o) => sc.name.toLowerCase().includes(o.toLowerCase()))) continue;
    for (const mode of modes) {
      const views = readViews();
      const app = fake(views, { changed: pinpoint, albums: albumsSay });
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
