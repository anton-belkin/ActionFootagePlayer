"use strict";
// The tile of the grid (../ui/tiles.js), Anton's layout of 2026-11-18: no name, no time; under the picture only the
// albums the item is in (one, two, none; a long one in two lines with its middle taken out) and one tag icon; what the
// item is as an icon over the picture. A clip turned in its own file shows its picture turned (an album link's preview
// made before the turn too), and a photo link of it. The Tags part's list of tags isn't held to a fixed height.
//   cd library/ui-tests && npm install && npm test

const test = require("node:test");
const assert = require("node:assert/strict");
const fs = require("fs");
const path = require("path");
const { open, sleep } = require("./page");
const { tilesViews, LONG } = require("./tiles_shots");

const opened = [];
// (the pictures asked for still coming: let them before the window goes)
test.afterEach(async () => { for (const p of opened) await p.quiet(20).catch(() => {}); await sleep(50); while (opened.length) opened.pop().close(); });
async function start(views) {
  const page = await open({ views: views || tilesViews() });
  opened.push(page);
  // The moments and photos too (Albums shows them once read)
  page.get("needViews(['moments', 'stills'])");
  await page.until(() => page.get("!!(DATA.stills && DATA.moments)"), 5000, "the photos");
  return page;
}
const tile = (page, key) => page.document.querySelector('#main .tile[data-v="' + page.window.CSS.escape(key) + '"]');
const lines = (el) => Array.from(el.querySelectorAll(".cap .al")).map((x) => x.textContent);
const grid = (page, view, p) => { page.get("S.layout = 'grid'; S.view = " + JSON.stringify(view) + "; S.path = " + JSON.stringify(p) + "; S.sel = {}; renderAll()"); };

test("a tile: no name, no time; its albums under the picture, one tag icon, what it is over it", async () => {
  const page = await start();
  grid(page, "footage", ["Red Sea dives"]);
  const tiles = page.$$("#main .grid > .tile");
  assert.equal(tiles.length, 10, "the album's clips, moments and photos as tiles");
  for (const t of tiles) {
    assert.equal(t.querySelector(".nm"), null, "no name line");
    assert.equal(t.querySelector(".sub"), null, "no second line (camera, time)");
    assert.ok(!/DJI_|\.MP4|\.jpg|\.heic|Ray|Turtle/.test(t.querySelector(".cap").textContent), "no file name in the caption: " + t.querySelector(".cap").textContent);
    assert.ok(!/\d:\d\d/.test(t.querySelector(".cap").textContent), "no time in the caption");
  }
  // Two albums: both, a line each. One: it alone
  const two = tile(page, "i:a:_Clips/Red Sea dives/DJI_0003.afplink.mp4");
  assert.deepEqual(lines(two), ["Red Sea dives", "Wrecks"]);
  assert.deepEqual(lines(tile(page, "i:a:_Clips/Red Sea dives/DJI_0002.afplink.mp4")), ["Red Sea dives"]);
  // Tags: one icon, no names, no count
  const tagged = tile(page, "i:a:_Clips/Red Sea dives/DJI_0001.afplink.mp4");
  assert.equal(tagged.querySelectorAll(".cap .tg").length, 1, "one tag icon");
  assert.equal(tagged.querySelectorAll(".cap .tg svg").length, 1);
  assert.ok(!/reef|turtle/.test(tagged.querySelector(".cap").textContent), "no tag names");
  assert.equal(tagged.querySelector(".cap .tg").textContent, "", "no count");
  assert.equal(tile(page, "i:a:_Clips/Red Sea dives/DJI_0002.afplink.mp4").querySelector(".tg"), null, "untagged: no icon");
  // What it is: a moment, a photo, a Live Photo (a photo link too: live in the player); a clip none (a 360° one says 360°)
  const kind = (key) => { const k = tile(page, key).querySelector(".th .kd"); return k ? k.getAttribute("aria-label") || k.textContent : null; };
  assert.equal(kind("i:m:_Moments/Red Sea dives/Ray.afplink.mp4"), "Moment");
  assert.equal(kind("i:p:_Stills/Red Sea dives/DJI_0003_1.5s.afplink.jpg"), "Live photo link");
  assert.equal(tile(page, "i:p:_Stills/Red Sea dives/DJI_0003_1.5s.afplink.jpg").querySelector(".th .kd").innerHTML,
               tile(page, "i:p:_Stills/Red Sea dives/DJI_0001_2s.heic").querySelector(".th .kd").innerHTML, "the Live icon");
  assert.equal(kind("i:p:_Stills/Red Sea dives/DJI_0005_9s.jpg"), "Photo");
  assert.equal(kind("i:p:_Stills/Red Sea dives/DJI_0001_2s.heic"), "Live Photo");
  assert.equal(kind("i:a:_Clips/Red Sea dives/DJI_0002.afplink.mp4"), null);
  assert.equal(kind("i:a:_Clips/Red Sea dives/VID_0004.insv".replace(".insv", ".afplink.mp4")), "360°");
  for (const k of ["Moment", "Photo", "Live Photo"]) assert.ok(page.$('#main .kd[aria-label="' + k + '"] svg'), k + " is drawn as an icon");
  // The Live Photo's icon: a dot, a ring and a circle of 24 short rays
  assert.equal(page.$('#main .kd[aria-label="Live Photo"] svg').querySelectorAll("g path").length, 24);
  // Kept over the picture: Accepted / Rejected and the stars, the length (not on photos)
  assert.equal(tagged.querySelector(".th .cb .ok").textContent, "✓");
  assert.equal(tagged.querySelector(".th .cb b:last-child").textContent, "★★★★");
  assert.equal(tile(page, "i:a:_Clips/Red Sea dives/VID_0004.afplink.mp4").querySelector(".th .cb .no").textContent, "✕");
  assert.ok(tagged.querySelector(".th .d"), "a clip's length");
  assert.equal(tile(page, "i:p:_Stills/Red Sea dives/DJI_0001_2s.heic").querySelector(".th .d"), null, "no “Live” text: the icon says it");
  // Selected: as before
  page.click(tagged);
  assert.equal(tile(page, "i:a:_Clips/Red Sea dives/DJI_0001.afplink.mp4").getAttribute("aria-pressed"), "true");
});

test("a tile in no album has nothing under its picture but its tag icon; one in two albums, both", async () => {
  const v = tilesViews();
  // A photo in _Stills itself (Albums' top, no album of the user's), tagged
  v.stills.items.push(Object.assign({}, v.stills.items[2], { id: "_Stills/Loose.jpg", dir: "_Stills", name: "Loose.jpg", path: "/P/_Stills/Loose.jpg", tags: ["x"] }));
  const page = await start(v);
  // (Albums › Photos: every photo, wherever it is)
  page.get("S.layout = 'grid'; albumViewGo('photos')");
  const t = tile(page, page.keyOf("_Stills/Loose.jpg"));
  assert.ok(t, "the photo in Albums › Photos");
  assert.deepEqual(lines(t), []);
  assert.equal(t.querySelectorAll(".cap .tg").length, 1);
  // Originals: a clip in no album
  page.get("RAW_FOOTAGE.links = RAW_FOOTAGE.links.filter((l) => l.clip !== 'Day 1/DJI_0002.MP4'); arrange()");
  grid(page, "sources", ["Day 1"]);
  assert.deepEqual(lines(tile(page, "i:s:Day 1/DJI_0002.MP4")), []);
  // …and one in two: a line each, the long one's middle out
  const both = lines(tile(page, "i:s:Day 1/DJI_0001.MP4"));
  assert.equal(both.length, 2);
  assert.ok(both[0].includes("…") && LONG.startsWith(both[0].split("…")[0]) && LONG.endsWith(both[0].split("…")[1]), both[0]);
  assert.equal(both[1], "Red Sea dives");
});

test("a long album's name: two lines at most, its beginning and its end shown, the middle taken out", async () => {
  const page = await start();
  // Fitted by a measure of its own (the tests have no canvas): 6.6 px a character
  const fit = (t, w) => JSON.parse(page.get("JSON.stringify(fitLines(" + JSON.stringify(t) + ", " + w + ", (s) => s.length * 6.6))"));
  assert.deepEqual(fit("Red Sea dives", 160), ["Red Sea dives"], "short: one line");
  assert.deepEqual(fit("Egypt 2025 — Dahab and the Canyon", 160), ["Egypt 2025 — Dahab and ", "the Canyon"].map((s) => s.trim()), "two lines when it fits in two");
  const two = fit(LONG, 160);
  assert.equal(two.length, 2);
  assert.ok(LONG.startsWith(two[0]), "the first line is the beginning: " + two[0]);
  assert.ok(two[1].startsWith("…") && LONG.endsWith(two[1].slice(1)), "the second is the end, after an ellipsis: " + two[1]);
  assert.ok(two[0].length + two[1].length - 1 < LONG.length, "the middle is out");
  for (const l of two) assert.ok(l.length * 6.6 <= 160, "each line fits: " + l);
  // Two albums: a line each, each shortened in its middle; more: “+N”
  const albums = (names, w) => JSON.parse(page.get("JSON.stringify(albumLines(" + JSON.stringify(names) + ", " + w + ", (s) => s.length * 6.6))"));
  const [a, b] = albums([LONG, "Wrecks"], 160);
  assert.ok(a.includes("…") && LONG.startsWith(a.split("…")[0]) && LONG.endsWith(a.split("…")[1]), "one line, middle out: " + a);
  assert.equal(b, "Wrecks");
  assert.deepEqual(albums(["A", "B", "C", "D"], 160), ["A", "B +2"]);
  // In the page: Originals' Day 2 tile of the clip in the long album alone, at the grid's width
  grid(page, "sources", ["Day 2"]);
  const t = tile(page, "i:s:Day 2/DJI_0006.MP4");
  const shown = lines(t);
  assert.equal(shown.length, 2, "two lines: " + JSON.stringify(shown));
  assert.ok(LONG.startsWith(shown[0]) && shown[1].startsWith("…") && LONG.endsWith(shown[1].slice(1)), JSON.stringify(shown));
  assert.equal(t.querySelector(".cap").getAttribute("title"), "In the album " + LONG.split("/").join(" › "), "the whole name on hover");
});

test("a clip turned in its own file: its pictures shown turned, its album links' previews and a photo link of it too", async () => {
  const page = await start();
  const cls = (key) => { const i = tile(page, key).querySelector(".th img"); return i ? i.getAttribute("class") || "" : null; };
  grid(page, "footage", ["Red Sea dives"]);
  // DJI_0003 is turned 90° (own.source.rotation): its link's preview, made as the camera recorded it, is turned
  assert.equal(cls("i:a:_Clips/Red Sea dives/DJI_0003.afplink.mp4"), "t90");
  assert.equal(cls("i:a:_Clips/Red Sea dives/DJI_0002.afplink.mp4"), "", "a clip not turned: as it is");
  assert.equal(cls("i:p:_Stills/Red Sea dives/DJI_0003_1.5s.afplink.jpg"), "t90", "the photo link's picture is its clip's frame");
  assert.equal(cls("i:m:_Moments/Red Sea dives/Ray.afplink.mp4"), "", "a moment was made turned: as it is");
  // Turned again (180, then 270, then back): shown as it is now, without a picture made again
  const thumbs = page.calls("thumb").length;
  for (const [r, want] of [[180, "t180"], [270, "t270"], [0, ""]]) {
    page.get("RAW_FOOTAGE.items.find((c) => c.id === 'Day 2/DJI_0003.MP4').own.source.rotation = " + r + "; arrange(); renderMain()");
    assert.equal(cls("i:a:_Clips/Red Sea dives/DJI_0003.afplink.mp4"), want, "turned " + r);
  }
  assert.equal(page.calls("thumb").length, thumbs, "no picture asked for again");
  // Originals, both layouts, and Info
  page.get("RAW_FOOTAGE.items.find((c) => c.id === 'Day 2/DJI_0003.MP4').own.source.rotation = 90; arrange()");
  grid(page, "sources", ["Day 2"]);
  assert.equal(cls("i:s:Day 2/DJI_0003.MP4"), "t90");
  page.get("S.layout = 'list'; renderMain()");
  assert.equal(page.row("s:Day 2/DJI_0003.MP4").querySelector(".th img").getAttribute("class"), "t90", "the list's picture too");
  page.select("s:Day 2/DJI_0003.MP4");
  page.get("showTab('info'); renderSide()");
  assert.equal(page.$("#side .ith img").getAttribute("class"), "t90", "Info's picture too");
  // The CSS: a quarter turn shown whole in the tile (its sides swapped, fitted), half a turn upside down
  const css = fs.readFileSync(path.join(__dirname, "..", "ui", "style.css"), "utf8");
  assert.match(css, /img\.t90\{transform:translate\(-50%,-50%\) rotate\(90deg\)\}/);
  assert.match(css, /img\.t90,img\.t270\{[^}]*width:100cqh !important;height:100cqw !important/);
  assert.match(css, /\.tile \.th img\.t90,\.tile \.th img\.t270[^{]*\{object-fit:contain\}/);
  assert.match(css, /img\.t180\{transform:rotate\(180deg\)\}/);
});

test("the Tags part: the tags in use aren't held to a fixed height", async () => {
  const v = tilesViews();
  const page = await start(v);
  page.get("S.view = 'footage'; S.path = ['Red Sea dives']; showTab('tags'); renderAll()");
  const list = () => page.$("#side .culltags");
  assert.ok(list(), "the list");
  assert.ok(!list().classList.contains("many"), "a few tags: never made smaller than they are");
  // Many tags: the list may give way (and scroll) only when the panel has no room left for it
  page.get("RAW_FOOTAGE.items[1].tags = Array.from({ length: 30 }, (_, i) => 'tag ' + i); renderSide()");
  assert.ok(list().classList.contains("many"));
  const css = fs.readFileSync(path.join(__dirname, "..", "ui", "style.css"), "utf8");
  assert.ok(!/\.culltags\{[^}]*max-height/.test(css), "no fixed height on the list");
  assert.match(css, /\.pane\[data-tab="tags"\] > \.secb\{flex:1 1 auto;min-height:0\}/);
  assert.match(css, /\.culltags\.many\{flex:0 1 auto;min-height:84px\}/);
});
