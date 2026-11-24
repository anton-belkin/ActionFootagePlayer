"use strict";
// Back and Forward (history.js), as Safari's: the places opened, in order; Back to the one before, Forward again to the
// one Back left; a new place after Back drops the ones ahead; a place comes back as it was left (scroll, selection,
// search). The mouse's side buttons (button 3, 4), ⌘[ ⌘] and the bar's ‹ › all step through it.
//   cd library/ui-tests && npm install && npm test

const test = require("node:test");
const assert = require("node:assert/strict");
const { open } = require("./page");

const opened = [];
test.afterEach(() => { while (opened.length) opened.pop().close(); });
async function start() {
  const page = await open({ commands: { open_list: () => true, open_items: () => true } });
  opened.push(page);
  // jsdom has no scrolling: the list's scroll kept as a number
  let st = 0;
  Object.defineProperty(page.$("#main"), "scrollTop", { get: () => st, set: (v) => { st = Math.max(0, +v || 0); }, configurable: true });
  page.get("S.view = 'sources'; S.path = []; S.sel = {}; S.anchor = null; resetHistory(); renderAll()");
  return page;
}
const where = (page) => page.get("S.view + ':' + (S.view === 'clip' ? S.clip : S.path.join('/'))");
const selected = (page) => JSON.parse(page.get("JSON.stringify(Object.keys(S.sel))"));
const steps = (page) => JSON.parse(page.get("JSON.stringify(S.hist.map((e) => e.key.replace('\\u0000', ':')))"));
// A mouse's side button pressed and let go over the list: 3 is Back, 4 is Forward
function side(page, button) {
  const t = page.$("#main") || page.document.body;
  const down = new page.window.MouseEvent("mousedown", { bubbles: true, cancelable: true, button });
  const up = new page.window.MouseEvent("mouseup", { bubbles: true, cancelable: true, button });
  t.dispatchEvent(down); t.dispatchEvent(up);
  return { down: down.defaultPrevented, up: up.defaultPrevented };
}
const folder = (page, p) => page.$('#main [data-v="' + page.window.CSS.escape("f:" + p) + '"]');
// A folder opened by a double click (its row as drawn now: a click before it draws the list again)
const into = (page, p) => folder(page, p).dispatchEvent(new page.window.MouseEvent("dblclick", { bubbles: true, cancelable: true, detail: 2 }));

test("side buttons: back to the folder above (the one left selected), forward into it again", async () => {
  const page = await start();
  assert.equal(where(page), "sources:");
  into(page, "Day 1");
  assert.equal(where(page), "sources:Day 1");
  const back = side(page, 3);
  assert.deepEqual(back, { down: true, up: true }, "the webview's own handling of the buttons kept off");
  assert.equal(where(page), "sources:");
  assert.deepEqual(selected(page), ["f:Day 1"], "the folder one came out of, selected");
  side(page, 4);
  assert.equal(where(page), "sources:Day 1");
  assert.equal(page.$('[data-a="fwd"]').disabled, true, "nothing ahead");
  assert.equal(page.$('[data-a="back"]').disabled, false);
  // Middle and left buttons: no step
  side(page, 1); side(page, 0);
  assert.equal(where(page), "sources:Day 1");
});

test("a new place after Back drops the ones ahead (branching)", async () => {
  const page = await start();
  into(page, "Day 1");
  side(page, 3);
  into(page, "Day 2");
  assert.deepEqual(steps(page), ["sources:", "sources:Day 2"]);
  side(page, 4);
  assert.equal(where(page), "sources:Day 2", "Forward: nothing to go to");
  side(page, 3);
  assert.equal(where(page), "sources:");
  side(page, 3);
  assert.equal(where(page), "sources:", "Back at the first place: stays");
});

test("a place comes back as it was left: its selection and scroll; selecting is not a step", async () => {
  const page = await start();
  into(page, "Day 1");
  page.click(page.row("Day 1/B.MP4"));
  page.click(page.row("Day 1/C.MP4"));
  assert.deepEqual(steps(page), ["sources:", "sources:Day 1"], "clicks on items add no steps");
  page.$("#main").scrollTop = 140;
  page.click('[data-a="crumb"][data-v="0"]');
  assert.equal(where(page), "sources:");
  assert.equal(page.$("#main").scrollTop, 140, "(jsdom: the number stays until set)");
  page.$("#main").scrollTop = 0;
  side(page, 3);
  assert.equal(where(page), "sources:Day 1");
  assert.deepEqual(selected(page), [page.keyOf("Day 1/C.MP4")], "what was selected there");
  assert.equal(page.$("#main").scrollTop, 140, "where it was scrolled to");
  assert.ok(page.row("Day 1/C.MP4").classList.contains("sel"), "shown selected");
});

test("tabs and a search: Back to Originals brings its search back; Forward to Albums", async () => {
  const page = await start();
  page.type("#q", "mp4");
  assert.deepEqual(steps(page), ["sources:"], "typing a search is not a step");
  page.click('[data-a="view"][data-v="footage"]');
  await page.quiet(30);
  assert.equal(where(page), "footage:");
  assert.equal(page.get("S.q"), "");
  side(page, 3);
  assert.equal(where(page), "sources:");
  assert.equal(page.get("S.q"), "mp4");
  assert.equal(page.$("#q").value, "mp4", "the search field shows it");
  assert.ok(page.$$("#main [data-a=pick]").length >= 5, "its results");
  side(page, 4);
  assert.equal(where(page), "footage:");
  assert.equal(page.get("S.q"), "");
});

test("a clip's own folder is a place: Back returns to the folder it was opened from", async () => {
  const page = await start();
  into(page, "Day 1");
  await page.get("showClip('Day 1/A.MP4')");
  assert.equal(where(page), "clip:Day 1/A.MP4");
  side(page, 3);
  assert.equal(where(page), "sources:Day 1");
  side(page, 4);
  assert.equal(where(page), "clip:Day 1/A.MP4");
});

test("⌘[ ⌘]: from the list and from the search field, on any keyboard layout; ⇧⌘[ isn't Back", async () => {
  const page = await start();
  into(page, "Day 1");
  assert.equal(where(page), "sources:Day 1");
  page.key("[", { metaKey: true, shiftKey: true, code: "BracketLeft" });
  assert.equal(where(page), "sources:Day 1");
  page.key("[", { metaKey: true, code: "BracketLeft" });
  assert.equal(where(page), "sources:");
  // A Russian layout: the key's letter is х, its place is [
  page.key("ъ", { metaKey: true, code: "BracketRight" });
  assert.equal(where(page), "sources:Day 1");
  page.key("[", { metaKey: true, code: "BracketLeft" }, page.$("#q"));
  assert.equal(where(page), "sources:", "from the search field too");
});

test("not while a sheet or the window's question is up; a project opened anew starts a history of its own", async () => {
  const page = await start();
  into(page, "Day 1");
  page.get("S.sheet = 'settings'");
  side(page, 3);
  assert.equal(where(page), "sources:Day 1");
  page.get("S.sheet = null; $('ask').hidden = false");
  side(page, 3);
  assert.equal(where(page), "sources:Day 1");
  page.get("$('ask').hidden = true");
  side(page, 3);
  assert.equal(where(page), "sources:");
  await page.get("loadProject()");
  await page.quiet(30);
  assert.deepEqual(steps(page), ["sources:"]);
});

test("what was selected and has gone since is let go when its place comes back", async () => {
  const page = await start();
  into(page, "Day 1");
  page.click(page.row("Day 1/B.MP4"));
  page.click('[data-a="crumb"][data-v="0"]');
  // B gone from the disk meanwhile
  page.views.footage.items = page.views.footage.items.filter((it) => it.name !== "B.MP4");
  await page.get("loadView('footage')");
  side(page, 3);
  assert.equal(where(page), "sources:Day 1");
  assert.deepEqual(selected(page), []);
});
