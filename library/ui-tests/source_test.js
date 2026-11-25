"use strict";
// Source settings as a list of the source's attributes (../ui/source.js sourceBody, Anton 2026-11-24): each shows what
// the clips selected have now (one value, or Mixed), a change is written by Save for the clips it applies to (a lens
// type only where the camera has types, the underwater colour only for a plain SDR clip), nothing is read from the
// clips to show it (the index's probe, its lens_info), Stabilize optically is in its Motion data row (no tab of its
// own), and every attribute is a filter.
//   cd library/ui-tests && npm install && npm test

const test = require("node:test");
const assert = require("node:assert/strict");
const { open, smallViews } = require("./page");
const { fake } = require("./fakeapp");

const A = "Day 1/A.MP4", B = "Day 1/B.MP4", C = "Day 1/C.MP4";
const opened = [];
test.afterEach(() => { while (opened.length) opened.pop().close(); });

// A and C: Action 6 in D-Log M, gyro, the lens in the file, lens types; B: a GoPro HERO7 in plain SDR, stabilized by
// HyperSmooth (no motion data it can use), no ready lens
function views() {
  const v = smallViews();
  const p = v.footage.probes.items;
  const dji = { lens: "in the file", from: "file", types: [{ id: "wide", label: "Wide" }, { id: "normal", label: "Normal" }], type: null, recorded: "wide", mode: 6, mode_key: "DJI OsmoAction6 · wide · 3840x2160 · 50 fps" };
  for (const k of Object.keys(p)) p[k] = Object.assign({}, p[k], { water: true, stabilized_in_camera: false, width: 3840, height: 2160, fps: 50, lens_info: dji });
  p["/F/" + B] = { camera: "GoPro HERO7 Black", color: "normal", mode: null, lens: "", water: false, stabilized_in_camera: "HyperSmooth (HS EIS)", width: 3840, height: 2160, fps: 29.97,
    movie: { duration: 30, created: 1735700000 }, marks: { times: [] },
    lens_info: { camera: "GoPro HERO7 Black", lens: "", from: "", types: [], mode_key: "GoPro HERO7 Black · wide · hseis · 3840x2160 · 29.97 fps" } };
  return v;
}
async function start() {
  const v = views();
  const app = fake(v, { changed: true });
  const lensAsked = [];
  const page = await open({ views: v, commands: Object.assign({}, app.commands, {
    lens_info: (a) => { lensAsked.push(a.path); return { candidates: [{ id: "db:1", label: "GoPro HERO7 Black Wide HS EIS 4K", exact: true, readout: true }] }; },
    assign_lens: (a) => ({ lens: "GoPro HERO7 Black Wide", mode: "the mode", again: 3, asked: a }),
    keep_found_lens: () => ({ lens: "GoPro HERO7 Black Wide, found", mode: "the mode", again: 3 }),
    try_lens: () => ({ lens: "GoPro HERO7 Black Wide HS EIS 4K 59.94" }),
    lens_choices: (a) => ({ path: a.path, mode: "GoPro HERO7 Black · wide · hseis · 3840x2160 · 29.97 fps", now: { name: "", from: "" }, found: null, lenses: [
      { name: "GoPro HERO7 Black Wide HS EIS 4K 59.94", source: "the player’s own", verdict: "untried", fits: false, readout_ms: null, in_use: false, use: { assign: "db:2" } },
      { name: "GoPro HERO7 Black Wide HS EIS 4K 29.97", source: "Gyroflow’s database", verdict: "good", fits: true, readout_ms: 8.1, readout_measured: true, moved: 2, limit: 15, in_use: false, use: { keep: "0" } }] }),
  }) });
  opened.push(page);
  app.attach(page);
  page.get("S.view = 'sources'; S.path = ['Day 1']; S.q = ''; S.filters = {}; showTab('source'); S.sel = {}; renderAll()");
  return { page, app, lensAsked };
}
const rows = (page) => page.$$("#side .attr").map((r) => ({ name: r.querySelector(".an").textContent, cls: r.className.replace("attr", "").trim(),
  sel: r.querySelector("select") ? r.querySelector("select").selectedOptions[0].textContent : null, val: r.querySelector(".val") ? r.querySelector(".val").textContent : null,
  prov: Array.from(r.querySelectorAll(".prov")).map((x) => x.textContent).join(" | ") }));
const rowOf = (page, name) => rows(page).find((r) => r.name === name);
const choose = (page, id, value) => {
  const s = page.$("#src-" + id);
  s.value = value;
  s.dispatchEvent(new page.window.Event("change", { bubbles: true }));
};

test("one clip: every attribute says what it is now", async () => {
  const { page, lensAsked } = await start();
  page.select(B);
  assert.deepEqual(rows(page).map((r) => r.name), ["Camera", "Recorded turned", "Recorded", "Log curve", "Underwater color", "Lens", "Lens type", "Under water", "Motion data"]);
  assert.equal(rowOf(page, "Camera").val, "GoPro HERO 7 Black");
  assert.equal(rowOf(page, "Recorded turned").sel, "As is");
  assert.equal(rowOf(page, "Recorded").val, "SDR");
  assert.equal(rowOf(page, "Log curve").sel, "From the clip: not log");
  assert.equal(rowOf(page, "Underwater color").sel, "Off");
  assert.equal(rowOf(page, "Lens").val, "No ready lens");
  assert.ok(page.$('#side [data-a="lenssheet"]'), "Choose lens…");
  assert.equal(rowOf(page, "Lens type").cls, "na", "no lens types for a GoPro");
  assert.equal(rowOf(page, "Under water").sel, "Not set: No");
  assert.equal(rowOf(page, "Motion data").val, "None: stabilized in the camera");
  assert.match(rowOf(page, "Motion data").prov, /HyperSmooth/);
  assert.match(rowOf(page, "Motion data").prov, /needs a lens first/, "no Stabilize optically without a lens");
  // The mode's lenses are the lens dialog's (Choose lens…): the clip isn't read for them here
  await new Promise((r) => setTimeout(r, 50));
  assert.deepEqual(lensAsked, []);
  assert.equal(page.$$("#side .savebar .btn.pri[disabled]").length, 1, "nothing to save yet");
});

test("several clips: Mixed, and a change only where it applies", async () => {
  const { page, app } = await start();
  page.select([A, B, C]);
  assert.equal(rowOf(page, "Camera").val, "Mixed");
  assert.equal(rowOf(page, "Log curve").sel, "From the clip", "all three read as the clip says");
  assert.equal(rowOf(page, "Underwater color").sel, "Off");
  assert.match(rowOf(page, "Underwater color").prov, /not for 2/);
  assert.equal(rowOf(page, "Lens").val, "Mixed");
  assert.match(rowOf(page, "Lens").prov, /one mode at a time/);
  assert.match(rowOf(page, "Lens type").prov, /not for 1/);
  assert.equal(rowOf(page, "Motion data").val, "Mixed");
  choose(page, "underwater", "yes");
  choose(page, "lens_type", "normal");
  choose(page, "uw", "on");
  assert.equal(rowOf(page, "Under water").cls, "chg");
  assert.match(page.text("#side .savebar .what"), /Underwater color On, for 1 of 3 · Lens type Normal, for 2 of 3 · Under water Yes \(a flat port\), for all 3/);
  page.click('#side [data-a="srcapply"]');
  await page.until(() => app.done.some((d) => d[0] === "set_source"), 2000, "the write");
  const sent = app.done.find((d) => d[0] === "set_source")[1].entries;
  const by = JSON.parse(JSON.stringify(Object.fromEntries(sent.map((e) => [e.clip, e.source]))));
  assert.deepEqual(by[A], { underwater: "yes", lens_type: "normal" });
  assert.deepEqual(by[B], { underwater: "yes", uw: true, uw_highlights: false }, "Yes for B's camera too; the colour only for B");
  assert.deepEqual(by[C], { underwater: "yes", lens_type: "normal" });
});

test("a lens is chosen in the lens dialog: Choose lens…, one click selects, Use keeps it for the mode", async () => {
  const { page } = await start();
  page.select(B);
  page.click('#side [data-a="lenssheet"]');
  await page.until(() => page.calls("lens_choices").length === 1, 1000, "the mode's lenses asked");
  await page.until(() => page.$$('#sheet [data-a="lenspick"]').length === 2, 1000, "the lenses listed");
  assert.match(page.text("#sheet h2"), /Lens for GoPro HERO7 Black · wide/);
  // The one that fits first, and the Quick switch on by default
  assert.match(page.$$('#sheet [data-a="lenspick"]')[0].textContent, /fits/);
  assert.equal(page.$('#sheet [data-a="lensquick"]').checked, true);
  assert.ok(page.$('#sheet [data-a="lensuse"]').disabled, "nothing chosen yet");
  page.click('#sheet [data-a="lenspick"][data-v="1"]');
  assert.ok(page.$('#sheet [data-a="lenspick"][data-v="1"]').classList.contains("on"));
  page.click('#sheet [data-a="lensuse"]');
  await page.until(() => page.calls("assign_lens").length === 1, 1000, "assign_lens");
  assert.deepEqual(page.calls("assign_lens")[0].args, { path: "/F/" + B, id: "db:2" });
  await page.until(() => page.get("S.sheet") === null, 1000, "the dialog closed");
  await new Promise((r) => setTimeout(r, 30));
});

test("Try on this clip: the lens kept for the clip alone, the clip measured again; the mode's clips keep theirs", async () => {
  const { page } = await start();
  page.select(B);
  page.click('#side [data-a="lenssheet"]');
  await page.until(() => page.$$('#sheet [data-a="lenspick"]').length === 2, 1000, "the lenses listed");
  page.click('#sheet [data-a="lenspick"][data-v="1"]');
  page.click('#sheet [data-a="lenstry"]');
  await page.until(() => page.calls("try_lens").length === 1, 1000, "try_lens");
  assert.deepEqual(page.calls("try_lens")[0].args, { clip: B, path: "/F/" + B, how: "assign", what: "db:2" });
  assert.equal(page.calls("assign_lens").length, 0, "not the mode's");
  await page.until(() => page.calls("add_job").length === 1, 1000, "measured again");
  assert.deepEqual(page.calls("add_job")[0].args.options, { clips: [B], redo: true });
  await page.until(() => page.get("S.sheet") === null, 1000, "the dialog closed");
  await new Promise((r) => setTimeout(r, 30));
});

test("the lens dialog runs Find with Quick, and a found lens is kept with its readout", async () => {
  const { page } = await start();
  page.select(B);
  page.click('#side [data-a="lenssheet"]');
  await page.until(() => page.$$('#sheet [data-a="lenspick"]').length === 2, 1000, "the lenses listed");
  page.click('#sheet [data-a="lensfind"]');
  await page.until(() => page.calls("add_job").length === 1, 1000, "Find queued");
  const job = page.calls("add_job")[0].args;
  assert.equal(job.kind, "lens-find");
  assert.equal(job.options.quick, true);
  page.click('#sheet [data-a="lenspick"][data-v="0"]');
  page.click('#sheet [data-a="lensuse"]');
  await page.until(() => page.calls("keep_found_lens").length === 1, 1000, "keep_found_lens");
  assert.deepEqual(page.calls("keep_found_lens")[0].args, { path: "/F/" + B, n: "0" });
  await page.until(() => page.get("S.sheet") === null, 1000, "the dialog closed");
  await new Promise((r) => setTimeout(r, 30));
});

test("nothing selected: nothing but what to do", async () => {
  const { page } = await start();
  assert.equal(page.$$("#side .attr").length, 0);
  assert.match(page.text("#side .pane"), /Select clips/);
});

test("Stabilize is Source's now; every attribute a filter", async () => {
  const { page } = await start();
  assert.equal(page.$('#side .rail [data-v="stabilize"]'), null, "no tab of its own");
  page.get("S.tab = 'stabilize'; renderSide()");
  assert.equal(page.get("S.tab"), "source");
  page.get("buildFacets()");
  const src = JSON.parse(page.get("JSON.stringify(FACETS.find((s) => s.id === 'src').facets.map((f) => f.id))"));
  assert.deepEqual(src, ["cam", "col", "crv", "trn", "uwc", "lns", "lnt", "uwl", "mot", "shp"]);
  const match = (id) => JSON.parse(page.get("JSON.stringify(DATA.sources.items.filter((it) => it.dir === 'Day 1' && findOpt(" + JSON.stringify(id) + ").fn(it)).map((it) => it.id))"));
  assert.deepEqual(match("lns:none"), [B]);
  assert.deepEqual(match("lns:file"), [A, C]);
  assert.deepEqual(match("mot:cam"), [B]);
  assert.deepEqual(match("uwc:na"), [A, C]);
  assert.deepEqual(match("lnt:no"), [B]);
});
