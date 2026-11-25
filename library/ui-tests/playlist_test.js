"use strict";
// The player's playlist from what the window shows (items.js's playList, open_list): an item double-clicked plays in
// the list as the window shows it then (search, filters, sort), the item at its place; what the player can't play
// left out; several selected and opened: those, in the window's order; photos are in the list (the player shows them
// as a phone does), a photo double-clicked plays at its place in it. What the
// player makes of it: tests/librarylist_ipc.py.
//   cd library/ui-tests && npm install && npm test

const test = require("node:test");
const assert = require("node:assert/strict");
const { open, smallViews } = require("./page");

const opened = [];
test.afterEach(() => { while (opened.length) opened.pop().close(); });
async function start(change) {
  const views = smallViews();
  if (change) change(views);
  const page = await open({ views, commands: { open_list: () => true, open_items: () => true } });
  opened.push(page);
  return page;
}
const dbl = (page, el) => el.dispatchEvent(new page.window.MouseEvent("dblclick", { bubbles: true, cancelable: true, detail: 2 }));
const name = (p) => p.split("/").pop();
// The last list sent to the player: its items by file name, the one opened, its label
function sent(page) {
  const c = page.calls("open_list").pop();
  return c && { names: c.args.paths.map(name), titles: c.args.titles, at: c.args.at, opened: name(c.args.paths[c.args.at]), label: c.args.label };
}
const shownNames = (page) => page.get("JSON.stringify(VISIBLE.map((k) => (items().find((x) => keyOf(x) === k) || {}).name))");

test("double-clicked in the middle: the list as shown (searched, filtered, sorted), from that item", async () => {
  const page = await start((v) => {
    const by = (n) => v.footage.items.find((it) => it.name === n);
    by("C.MP4").rating = 5; by("B.MP4").rating = 3; by("E.MP4").pick = "reject";
  });
  // Originals, every clip found by a search, rejected ones hidden, most stars first
  page.get("S.view = 'sources'; S.path = []; S.q = 'mp4'; S.sel = {}; CULL.hide = true; CULL.starSort = true; renderAll()");
  assert.deepEqual(JSON.parse(shownNames(page)), ["C.MP4", "B.MP4", "A.MP4", "D.MP4"]);
  dbl(page, page.row("Day 1/B.MP4"));
  await page.quiet(40);
  const s = sent(page);
  assert.deepEqual(s.names, ["C.MP4", "B.MP4", "A.MP4", "D.MP4"]);
  assert.equal(s.at, 1);
  assert.equal(s.opened, "B.MP4");
  assert.deepEqual(s.titles, ["C", "B", "A", "D"], "names as the player shows them (no .MP4)");
  assert.equal(s.label, "Originals · “mp4” · filtered");
  assert.equal(page.calls("open_items").length, 0, "nothing opened by itself");

  // The filter changed afterwards: nothing sent (a snapshot); double-clicked again: the new list replaces it
  page.get("CULL.hide = false; S.q = ''; S.path = ['Day 2']; renderAll()");
  await page.quiet(40);
  assert.equal(page.calls("open_list").length, 1);
  dbl(page, page.row("Day 2/E.MP4"));
  await page.quiet(40);
  assert.deepEqual(sent(page), { names: ["D.MP4", "E.MP4"], titles: ["D", "E"], at: 1, opened: "E.MP4", label: "Originals › Day 2" });
});

test("moments: one whose original isn't found is left out; a clip's view: its moments only (no marks, no photos)", async () => {
  const page = await start((v) => { v.moments.items.find((m) => m.name === "Shark").original = null; });
  page.get("S.view = 'footage'; S.path = [AV_MARK + ALBUM_VIEWS.find((v) => v[0] === 'moments')[1]]; S.q = ''; S.sel = {}; renderAll()");
  const shown = JSON.parse(shownNames(page));
  assert.ok(shown.includes("Shark") && shown.includes("Turtle") && shown.includes("Ray"), "all three shown: " + shown);
  dbl(page, page.row("_Moments/Reef/Turtle.afplink.mp4"));
  await page.quiet(40);
  let s = sent(page);
  assert.deepEqual(s.names, shown.filter((n) => n !== "Shark").map((n) => n + ".afplink.mp4"));
  assert.equal(s.opened, "Turtle.afplink.mp4");
  assert.equal(s.titles[s.at], "Turtle", "a moment's own name, for the playlist");

  // A clip's marks, moments and photos: the moment plays among its moments and photos; the mark isn't in the list
  page.get("S.view = 'clip'; S.clip = 'Day 1/A.MP4'; S.path = []; buildClip(); renderAll()");
  const kinds = page.get("JSON.stringify(items().map((x) => x.kind))");
  assert.ok(/mark/.test(kinds) && /still/.test(kinds) && /moment/.test(kinds), kinds);
  dbl(page, page.$('#main [data-v^="i:m:"], #main .row[data-v*="Turtle"], #main .card[data-v*="Turtle"]') || page.row("_Moments/Reef/Turtle.afplink.mp4"));
  await page.quiet(40);
  s = sent(page);
  const want = JSON.parse(page.get("JSON.stringify(VISIBLE.map((k) => items().find((x) => keyOf(x) === k)).filter((x) => x && (x.kind === 'moment' || x.kind === 'still')).map((x) => x.path.split('/').pop()))"));
  assert.deepEqual(s.names, want);
  assert.ok(s.names.includes("Turtle.afplink.mp4") && want.some((n) => /\.(jpg|heic)$/i.test(n)), JSON.stringify(s.names));
  assert.equal(s.opened, "Turtle.afplink.mp4");
  // A mark: its clip, by itself (as before)
  const lists = page.calls("open_list").length;
  dbl(page, page.$('#main [data-v="i:k:0"]'));
  await page.quiet(40);
  assert.equal(page.calls("open_list").length, lists);
  assert.deepEqual(page.calls("open_items").pop().args.paths, ["/F/Day 1/A.MP4"]);
});

test("a photo double-clicked plays at its place in the list, a mark by itself; several selected: those, in the window's order", async () => {
  const page = await start();
  page.get("S.view = 'sources'; S.path = ['Day 1']; S.q = ''; S.sel = {}; renderAll()");
  // Several selected (C then A, as clicked), opened with Return: A, C as the window lists them, from the first
  page.select(["Day 1/C.MP4", "Day 1/A.MP4"]);
  page.get("openSelection()");
  await page.quiet(40);
  let s = sent(page);
  assert.deepEqual([s.names, s.at], [["A.MP4", "C.MP4"], 0]);

  // A photo: in the list of the photos shown, at its place (the player shows it as a still, its live part on hover)
  page.get("S.view = 'footage'; S.path = [AV_MARK + ALBUM_VIEWS.find((v) => v[0] === 'photos')[1]]; S.sel = {}; renderAll()");
  const before = page.calls("open_list").length, items_before = page.calls("open_items").length;
  dbl(page, page.$("#main [data-a='pick']"));
  await page.quiet(40);
  assert.equal(page.calls("open_list").length, before + 1);
  assert.equal(page.calls("open_items").length, items_before, "no photo by itself");
  s = sent(page);
  assert.ok(s.names.length >= 1 && s.names.every((n) => /\.(jpg|heic)$/i.test(n)), JSON.stringify(s.names));
  assert.match(s.opened, /\.(jpg|heic)$/i);
});

test("thousands shown: the list is all of them, the item at its place, made at once", async () => {
  const page = await start();
  page.get("S.view = 'sources'; S.path = ['Day 1']; S.q = ''; S.sel = {}; renderAll()");
  // The window's list as 5000 clips (as rendering them would leave VISIBLE), the list for the 3210th
  const r = JSON.parse(page.get(`(() => {
    const one = items()[0], big = [];
    for (let i = 0; i < 5000; i++) big.push(Object.assign({}, one, { key: "s:Day 1/C" + i + ".MP4", id: "Day 1/C" + i + ".MP4", name: "C" + i + ".MP4", path: "/F/Day 1/C" + i + ".MP4" }));
    DATA.sources.items = big; VISIBLE = big.slice().reverse().map(keyOf);
    const t0 = Date.now(), l = playList([big[3210]]);
    return JSON.stringify({ n: l.paths.length, at: l.at, opened: l.paths[l.at], first: l.paths[0], ms: Date.now() - t0 });
  })()`));
  assert.deepEqual([r.n, r.opened, r.at, r.first], [5000, "/F/Day 1/C3210.MP4", 5000 - 1 - 3210, "/F/Day 1/C4999.MP4"]);
  assert.ok(r.ms < 200, "made in " + r.ms + " ms");
});
