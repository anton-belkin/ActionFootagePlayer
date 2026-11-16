"use strict";
// The pinpoint update (../ui/changes.js, docs/library-responsiveness.md): a command says what it changed, the catalog's
// changes come as items, and the page corrects only those: no view is read again. And that it ends up exactly where
// reading the views again would have: a sequence of changes made on two pages, one corrected item by item, one that
// reads every view again after each, compared after every step.
//   cd library/ui-tests && npm install && npm test

const test = require("node:test");
const assert = require("node:assert/strict");
const { open, smallViews, sleep } = require("./page");
const { fake } = require("./fakeapp");

const A = "Day 1/A.MP4", B = "Day 1/B.MP4", C = "Day 1/C.MP4", D = "Day 2/D.MP4", E = "Day 2/E.MP4";
const TURTLE = "_Moments/Reef/Turtle.afplink.mp4", SHARK = "_Moments/Reef/Shark.afplink.mp4", RAY = "_Moments/Reef/Ray.afplink.mp4";

const opened = [];
test.afterEach(() => { while (opened.length) opened.pop().close(); });
async function start(o) {
  const views = smallViews();
  const app = fake(views, { changed: true });
  const page = await open(Object.assign({ views }, o || {}, { commands: Object.assign({}, app.commands, (o && o.commands) || {}) }));
  opened.push(page);
  app.attach(page);
  return { page, app, views };
}
const row = (page, id) => page.$('#main [data-v="' + page.window.CSS.escape("i:" + id) + '"]');
const chips = (page, id) => Array.from(row(page, id).querySelectorAll(".chips .chip.tag")).map((e) => e.textContent);
const shown = (page, view, id) => page.get("(DATA." + view + ".items.find((x) => x.id === " + JSON.stringify(id) + ") || null)");
const val = (page, expr) => JSON.parse(page.get("JSON.stringify(" + expr + ")"));
const unsaved = (page) => (page.$("#unsaved").hidden ? "" : page.$("#unsaved .lines").textContent);
const asked = (page, mark) => page.since(mark).map((c) => (c.cmd === "scan" ? "scan " + c.args.view : c.cmd)).filter((c) => c !== "writes_pending");
function go(page, view, path, tab) {
  page.get("S.view = " + JSON.stringify(view) + "; S.path = " + JSON.stringify(path || []) + "; S.q = ''; S.flat = false; S.filters = {};" + (tab ? "showTab(" + JSON.stringify(tab) + ");" : "") + " S.sel = {}; renderAll()");
}
function addTag(page, tag) { page.get("showTab('tags'); renderSide()"); page.type("#tagdraft", tag); page.click('[data-a="tag"]'); }

test("a tag: shown at once, written, and no view read again", async () => {
  const { page, views } = await start({ delay: { set_tags: 80 } });
  go(page, "footage", ["Day 1"], "tags");
  page.select([A, B]);
  const mark = page.mark();
  addTag(page, "turtle");
  assert.deepEqual(chips(page, B), ["turtle"]);
  await page.quiet(80);
  assert.deepEqual(asked(page, mark), ["set_tags", "changes_since"]);
  assert.deepEqual(chips(page, A), ["reef", "turtle"]);
  // What the disk has is what the command said, not what the page guessed
  assert.deepEqual(val(page, "CONFIRMED.footage.items.find((x) => x.id === " + JSON.stringify(B) + ")"), views.footage.items.find((x) => x.id === B));
  assert.equal(page.get("Writes.pending()"), 0);
  assert.deepEqual(page.errors, []);
});

test("written in part: each item shows as it is, said which change, and still nothing read again", async () => {
  const { page, app } = await start({ delay: { set_tags: 40 } });
  app.failItems = (cmd, id) => (id === B ? "/F/Day 1/B.afp.json: Permission denied" : null);
  go(page, "footage", ["Day 1"], "tags");
  page.select([A, B, C]);
  const mark = page.mark();
  addTag(page, "turtle");
  assert.deepEqual(chips(page, B), ["turtle"]);
  await page.until(() => unsaved(page));
  assert.deepEqual(chips(page, A), ["reef", "turtle"]);
  assert.deepEqual(chips(page, B), []);
  assert.deepEqual(chips(page, C), ["turtle"]);
  assert.match(unsaved(page), /Not all saved: the tag “turtle” on A\.MP4, B\.MP4, C\.MP4\. .*B\.afp\.json: Permission denied/);
  await page.quiet(60);
  assert.ok(!asked(page, mark).some((c) => c.startsWith("scan")), asked(page, mark).join(" "));
});

test("a change from elsewhere comes as its items; one the app can't name reads the views again", async () => {
  const { page, app, views } = await start();
  go(page, "footage", ["Day 1"]);
  let mark = page.mark();
  views.footage.items.find((it) => it.id === B).tags = ["theirs"];
  app.elsewhere([["footage", B]]);
  await page.until(() => chips(page, B).includes("theirs"));
  await page.quiet(40);
  assert.deepEqual(asked(page, mark), ["changes_since"]);
  // Under a change of ours still waiting: ours stays on top
  page.select([B]);
  mark = page.mark();
  page.get("setTags(['mine'], [])");
  views.footage.items.find((it) => it.id === B).tags = ["theirs", "more"];
  app.elsewhere([["footage", B]]);
  await page.quiet(60);
  assert.deepEqual(chips(page, B), ["theirs", "more", "mine"]);
  assert.ok(!asked(page, mark).some((c) => c.startsWith("scan")));
  // Not said which items (a first look at a location, too many): as before, the views are read
  mark = page.mark();
  views.footage.items.find((it) => it.id === C).tags = ["bulk"];
  app.elsewhere(null);
  await page.until(() => chips(page, C).includes("bulk"));
  await page.quiet(60);
  assert.ok(asked(page, mark).includes("scan footage"));
});

test("what the player makes shows where it belongs: a moment in a folder there is, one in a new folder, a clip's preview gone", async () => {
  const { page, app, views } = await start();
  page.click('[data-a="view"][data-v="moments"]');
  await page.quiet(40);
  const moment = (dir, name, clip) => ({ id: dir + "/" + name + ".afplink.mp4", dir, name, path: "/P/" + dir + "/" + name + ".afplink.mp4", size: 100, mtime: 1700000500, kind: "moment", tags: [],
    gyroflow: false, renders: {}, original: "/F/" + clip, link: { v: 1, kind: "moment", clip, in: 2, out: 5 } });
  const mark = page.mark();
  views.moments.items.splice(1, 0, moment("_Moments/Reef", "Sea fan", C));
  app.elsewhere([["moments", "_Moments/Reef/Sea fan.afplink.mp4"]]);
  await page.until(() => shown(page, "moments", "_Moments/Reef/Sea fan.afplink.mp4"));
  // Among its folder's, by its name, as a reading would have it; its clip's count of moments
  assert.deepEqual(val(page, "DATA.moments.items.map((x) => x.name)"), ["Ray", "Sea fan", "Shark", "Turtle"]);
  assert.equal(shown(page, "footage", C).moments, 1);
  // A run's new folder
  views.moments.items.push(moment("Generated/2026-11-16 Moments", "Gen", D));
  views.moments.dirs.push("Generated", "Generated/2026-11-16 Moments");
  app.elsewhere([["moments", "Generated/2026-11-16 Moments/Gen.afplink.mp4"]]);
  await page.until(() => shown(page, "moments", "Generated/2026-11-16 Moments/Gen.afplink.mp4"));
  assert.ok(val(page, "DATA.moments.dirs").includes("Other folders/Generated/2026-11-16 Moments"));
  // A clip's preview deleted in Finder: the file is gone, the page knows whose it was and asks for that clip
  const e = views.footage.items.find((it) => it.id === E);
  const file = e.preview;
  e.preview = null; e.pdir = null;
  app.elsewhere([], [file]);
  await page.until(() => shown(page, "footage", E).dir === "Without a clip preview");
  await page.quiet(40);
  assert.deepEqual(page.since(mark).filter((c) => c.cmd === "view_items").map((c) => c.args), [{ view: "footage", ids: [E] }]);
  assert.ok(!asked(page, mark).some((c) => c.startsWith("scan")), asked(page, mark).join(" "));
});

test("an item the filter no longer takes leaves at once, and the next one is selected", async () => {
  const { page, app, views } = await start({ delay: { set_source: 30 } });
  const turn = (ids, to) => { page.select(ids); page.get("showTab('source'); renderSide(); srcDraftSet('rotation', '" + to + "')"); page.click('[data-a="srcapply"]'); };
  go(page, "footage", ["Day 1"], "source");
  turn([A, B, C], 180);
  await page.quiet(60);
  // Filtered to the clips recorded turned; one selected is set back
  page.get("buildFacets(); S.filters = { 'trn:y': true }; renderAll()");
  assert.deepEqual(val(page, "VISIBLE"), ["i:" + A, "i:" + B, "i:" + C]);
  turn([B], 0);
  // Gone from the list before the app has answered, and the one that took its place is selected
  assert.equal(page.calls("set_source").slice(-1)[0].took, null);
  assert.deepEqual(val(page, "VISIBLE"), ["i:" + A, "i:" + C]);
  assert.deepEqual(val(page, "Object.keys(S.sel)"), ["i:" + C]);
  assert.match(page.text("#side .ph"), /On 1 clip/);
  await page.quiet(60);
  // Two selected, one of them changed elsewhere so that it leaves: the other stays selected, nothing else is
  page.select([A, C]);
  views.footage.items.find((it) => it.id === A).own.source = null;
  app.elsewhere([["footage", A]]);
  await page.until(() => val(page, "VISIBLE").length === 1);
  assert.deepEqual(val(page, "VISIBLE"), ["i:" + C]);
  assert.deepEqual(val(page, "Object.keys(S.sel)"), ["i:" + C]);
  // The last one leaves: nothing is selected
  turn([C], 0);
  assert.deepEqual(val(page, "VISIBLE"), []);
  assert.deepEqual(val(page, "Object.keys(S.sel)"), []);
  await page.quiet(60);
  assert.equal(unsaved(page), "");
});

// ---------- the same as reading the views again ----------

// Everything the page shows from, as text: the views as shown (every field the page worked out), the counts, and the
// list and the side panel drawn at a few places
function state(page) {
  return page.get(`(() => {
    const keep = { view: S.view, path: S.path, sel: S.sel, tab: S.tab, filters: S.filters, q: S.q };
    const out = { data: {}, drawn: {} };
    for (const v of ["footage", "sources", "moments", "stills", "exports"]) out.data[v] = DATA[v] || null;
    out.raw = RAW_FOOTAGE; out.moments_of = MOMENTS_OF; out.confirmed = CONFIRMED;
    const draw = (name, view, path, sel) => {
      S.view = view; S.path = path; S.sel = {}; S.filters = {}; S.q = ""; S.tab = "info";
      for (const k of sel || []) S.sel[k] = true;
      renderAll();
      out.drawn[name] = [document.getElementById("main").innerHTML, document.getElementById("side").innerHTML, document.getElementById("fpanel").innerHTML, FACETS.map((x) => [x.id, x.facets.map((f) => [f.id, f.opts.map((o) => o.id + (o.kids ? ":" + o.kids.map((k) => k.id) : ""))])])];
    };
    S.filtersOpen = true;
    draw("footage", "footage", []); draw("day1", "footage", ["Day 1"], ["i:Day 1/B.MP4"]); draw("day2", "footage", ["Day 2"], ["i:Day 2/E.MP4"]);
    draw("nopreview", "footage", ["Without a clip preview"]); draw("originals", "sources", ["Day 1"], ["i:s:Day 1/A.MP4"]);
    draw("moments", "moments", []); draw("reef", "moments", ["Reef"]); draw("stills", "stills", []); draw("exports", "exports", []);
    S.filtersOpen = false;
    Object.assign(S, keep);
    renderAll();
    return JSON.stringify(out);
  })()`);
}

test("a sequence of changes, corrected item by item, ends where reading the views again ends, at every step", async () => {
  // One page as it is; the other reads every view again after each step (and is so what the disk says)
  const one = await start({ delay: { set_tags: 20, set_locations: 20, set_source: 20, rename_item: 20, delete_items: 20 } });
  const two = await start({ delay: { set_tags: 20, set_locations: 20, set_source: 20, rename_item: 20, delete_items: 20 } });
  const both = [one, two];
  for (const x of both) {
    for (const v of ["stills", "exports", "moments", "footage"]) { x.page.click('[data-a="view"][data-v="' + v + '"]'); await x.page.quiet(40); }
  }
  const settle = async (label) => {
    for (const x of both) { await x.page.until(() => x.page.get("Writes.pending()") === 0); await x.page.quiet(60); }
    const scans = one.page.calls("scan").length;
    two.page.get("Promise.all(['footage', 'moments', 'stills', 'exports'].map((v) => loadView(v))).then(renderAll)");
    await two.page.quiet(60);
    const a = JSON.parse(state(one.page)), b = JSON.parse(state(two.page));
    for (const k of Object.keys(b.data)) assert.deepEqual(a.data[k], b.data[k], label + ": the " + k + " shown");
    assert.deepEqual(a.confirmed, b.confirmed, label + ": what the disk has");
    assert.deepEqual(a.raw, b.raw, label + ": the footage as read");
    assert.deepEqual(a.moments_of, b.moments_of, label + ": the clips' counts of moments");
    for (const k of Object.keys(b.drawn)) assert.deepEqual(a.drawn[k], b.drawn[k], label + ": " + k + " as drawn");
    assert.equal(unsaved(one.page), "", label);
    return scans;
  };
  const each = (f) => { for (const x of both) f(x.page, x.views, x.app); };
  await settle("as opened");
  const before = one.page.calls("scan").length;

  each((p) => { go(p, "footage", ["Day 1"], "tags"); p.select([A, B]); p.get("setTags(['x', 'night dive'], [])"); });
  await settle("tags on two clips");
  each((p) => { p.select([A]); p.get("setTags([], ['reef'])"); });
  await settle("a tag off a clip (its own file then holds less)");
  each((p) => { go(p, "footage", ["Day 1"], "geo"); p.select([B, C]); p.type("#geohand", "27.7333, 34.25"); p.type("#geosite", "Thistlegorm"); p.click('[data-a="geohand"]'); });
  await settle("a location by hand");
  each((p) => { go(p, "sources", ["Day 1"], "source"); p.select(["s:" + A]); p.get("srcDraftSet('rotation', '180'); srcDraftSet('curve', 'dlogm')"); p.click('[data-a="srcapply"]'); });
  await settle("Source settings, from Originals");
  each((p) => { go(p, "moments", ["Reef"], "tags"); p.select([TURTLE, SHARK]); p.get("setTags(['best'], [])"); });
  await settle("tags on moments");
  each((p) => { p.select([SHARK]); p.get("startRename()"); p.$("#rename").value = "Angel"; p.key("Enter", {}, p.$("#rename")); });
  await settle("a moment renamed");
  each((p) => { p.select(["_Moments/Reef/Angel.afplink.mp4"]); p.get("setTags(['renamed'], [])"); });
  await settle("the renamed moment tagged");
  each((p) => { p.select([RAY]); p.get("deleteSelection()"); });
  await settle("a moment deleted");
  each((p) => { go(p, "stills", [], "tags"); p.select(["_Stills/A_1.5s.jpg"]); p.get("setTags(['print'], [])"); });
  await settle("a photo tagged");
  each((p) => { p.select(["_Stills/B_1.5s.jpg"]); p.get("startRename()"); p.$("#rename").value = "Anemone"; p.key("Enter", {}, p.$("#rename")); });
  await settle("a photo renamed");
  each((p) => { go(p, "exports", [], "tags"); p.select(["_Exports/E.mov"]); p.get("setTags(['sent'], [])"); });
  await settle("an export tagged");
  // From elsewhere: the player's moment, another computer's tag, a clip's preview gone
  each((p, views, app) => {
    views.moments.items.push({ id: "_Moments/Reef/Baby.afplink.mp4", dir: "_Moments/Reef", name: "Baby", path: "/P/_Moments/Reef/Baby.afplink.mp4", size: 100, mtime: 1700000500, kind: "moment", tags: [],
      gyroflow: false, renders: { HDR: "current" }, original: "/F/" + C, link: { v: 1, kind: "moment", clip: C, in: 2, out: 5 } });
    views.moments.items.sort((x, y) => (x.id < y.id ? -1 : 1));   // (as a reading has them: by name in a folder)
    app.elsewhere([["moments", "_Moments/Reef/Baby.afplink.mp4"]]);
  });
  await settle("a moment the player made");
  each((p, views, app) => { const d = views.footage.items.find((it) => it.id === D); d.tags = ["elsewhere"]; d.own = { marks: [{ t: 3 }], view: { view: "wide" }, source: null, location: null }; app.elsewhere([["footage", D]]); });
  await settle("another computer's tag and marks");
  each((p) => { go(p, "footage", ["Day 2"]); p.select([E]); p.get("deleteSelection()"); });
  await settle("a clip's preview deleted");
  each((p) => { go(p, "footage", ["Day 1"], "geo"); p.select([B]); p.click('#side [data-a="georemove"]'); });
  await settle("a location taken off");
  each((p) => { go(p, "footage", [], "tags"); p.select(["Day 1"], "f"); p.get("startRename()"); p.$("#rename").value = "First day"; p.key("Enter", {}, p.$("#rename")); });
  const scans = await settle("a folder renamed (its views are read again: the one case)");
  // Item by item all the way, but for the folder: the first page read views only then
  assert.ok(scans > before, "the folder's rename reads its views again");
  const read = one.page.calls("scan").slice(before);
  assert.ok(read.every((c) => c.at >= one.page.calls("rename_item").filter((r) => r.args.kind === "folder")[0].at), "a view was read before the folder was renamed: " + read.map((c) => c.args.view));
});
