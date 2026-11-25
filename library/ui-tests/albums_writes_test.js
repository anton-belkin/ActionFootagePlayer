"use strict";
// Album actions shown at once (../ui/albums.js, docs/design/library-responsiveness.md "Albums"): the page against the fake app
// answering as the app does (src/changed.rs's of_album: the links and items it made), each command slowed so what's
// on screen before it answers can be looked at.
//   cd library/ui-tests && npm install && node --test

const test = require("node:test");
const assert = require("node:assert/strict");
const { open, smallViews } = require("./page");
const { fake } = require("./fakeapp");

const A = "Day 1/A.MP4", B = "Day 1/B.MP4", D = "Day 2/D.MP4";
const TURTLE = "_Moments/Reef/Turtle.afplink.mp4";

const opened = [];
test.afterEach(() => { while (opened.length) opened.pop().close(); });
async function start(o) {
  const views = smallViews();
  const app = fake(views, { changed: true });
  const page = await open(Object.assign({ views }, o || {}, { commands: app.commands }));
  opened.push(page);
  app.attach(page);
  return { page, app, views };
}
const val = (page, expr) => JSON.parse(page.get("JSON.stringify(" + expr + ")"));
const unsaved = (page) => (page.$("#unsaved").hidden ? "" : page.$("#unsaved .lines").textContent);
// The views read since a mark (the point: none after an album action)
const scans = (page, mark) => page.since(mark).filter((c) => c.cmd === "scan" || c.cmd === "project").map((c) => c.cmd + " " + ((c.args || {}).view || ""));
// An album's items as Albums show them (the album open or not: DATA.footage holds every album's)
const albumItems = (page, album) => val(page, "DATA.footage.items.filter((x) => x.dir === " + JSON.stringify(album) + ").map((x) => ({ key: x.key, kind: x.kind, pending: !!x.pending, thumb: x.thumbPath, link: x.link_file || null, name: x.name }))");
// How many albums Originals says a clip is in (its "In an album" chip)
const inAlbums = (page, id) => page.get("(DATA.sources.items.find((x) => x.id === " + JSON.stringify(id) + ") || {}).albums || 0");
function originals(page, ids) {
  page.get("S.view = 'sources'; S.path = []; S.flat = true; renderAll()");
  page.select(ids, "i:s");
}
// Through the sheet, as a click does it: Add (or Move) to the album `name` (a new one, or one there is)
async function viaSheet(page, mode, name) {
  await page.get("askAlbum(" + JSON.stringify(mode) + ")");
  page.get("albumTyped(" + JSON.stringify(name) + ")");
  page.click('[data-a="albumgo"]');
}

test("Add to a new album: the clips are in it at once, with the pictures the page has; the app's links replace them, no view read", async () => {
  const { page, views } = await start({ delay: { album_add: 150 } });
  originals(page, [A, D]);
  const picA = page.get("DATA.sources.items.find((x) => x.id === " + JSON.stringify(A) + ").thumbPath");
  assert.equal(inAlbums(page, A), 1);
  const mark = page.mark();
  await viaSheet(page, "add", "Egypt");
  // At once: the album, its two items being written (each with its clip's picture), the sheet closed, the counts
  assert.equal(page.get("S.sheet"), null);
  assert.equal(page.calls("album_add")[0].took, null, "the app hasn't answered yet");
  let items = albumItems(page, "Egypt");
  assert.deepEqual(items.map((x) => [x.key, x.pending]), [["a:_Clips/Egypt/A.afplink.mp4", true], ["a:_Clips/Egypt/D.afplink.mp4", true]]);
  assert.equal(items[0].thumb, picA, "the clip's picture meanwhile");
  assert.equal(inAlbums(page, A), 2, "Originals' chip: In 2 albums");
  assert.ok(val(page, "albumList()").includes("Egypt"), "the album is among the user's albums at once");
  assert.equal(page.get("Writes.pending()"), 1, "closing the window would wait for it");
  // Written: the links as the app made them; nothing read, no second read from the catalog's notice
  await page.quiet(80);
  items = albumItems(page, "Egypt");
  assert.deepEqual(items.map((x) => [x.key, x.pending, x.link]), [["a:_Clips/Egypt/A.afplink.mp4", false, "/P/_Clips/Egypt/A.afplink.mp4"], ["a:_Clips/Egypt/D.afplink.mp4", false, "/P/_Clips/Egypt/D.afplink.mp4"]]);
  assert.equal(items[0].thumb, "/P/_Clips/Egypt/A.afplink.mp4", "its own picture: the link's (the thumbnail written into it)");
  assert.deepEqual(scans(page, mark), []);
  assert.ok(page.since(mark).some((c) => c.cmd === "changes_since"), "the catalog's notice was asked about…");
  assert.equal(inAlbums(page, A), 2);
  assert.equal(views.footage.links.filter((l) => l.rel.startsWith("_Clips/Egypt/")).length, 2);
  assert.equal(unsaved(page), "");
  assert.deepEqual(page.errors, []);
});

test("an add that fails is taken back whole and said; the album made for it goes too", async () => {
  const { page, app } = await start({ delay: { album_add: 80 } });
  app.fail.album_add = () => "The share isn’t connected";
  originals(page, [A]);
  await viaSheet(page, "add", "Egypt");
  assert.equal(albumItems(page, "Egypt").length, 1);
  assert.equal(inAlbums(page, A), 2);
  await page.until(() => unsaved(page));
  assert.deepEqual(albumItems(page, "Egypt"), []);
  assert.ok(!val(page, "albumList()").includes("Egypt"));
  assert.equal(inAlbums(page, A), 1);
  assert.match(unsaved(page), /Not saved: adding A\.MP4 to the album Egypt\. The share isn’t connected/);
});

test("two adds of the same clip to the same album are one link, as the command's 'already'; a name taken gets ' 2' as the command gives it", async () => {
  const { page, views } = await start({ delay: { album_add: 60 } });
  originals(page, [A]);
  await viaSheet(page, "add", "Day 2");
  originals(page, [A]);
  await viaSheet(page, "add", "Day 2");
  assert.match(page.$("#toastMsg").textContent, /there already/);
  assert.equal(albumItems(page, "Day 2").filter((x) => x.name === "A.MP4").length, 1);
  // Another clip named A.MP4 (Day 2/A.MP4 is not there: one whose name is taken in the album, by a link of D renamed)
  views.footage.links.find((l) => l.clip === D).rel = "_Clips/Day 2/B.afplink.mp4";
  views.footage.links.find((l) => l.clip === D).path = "/P/_Clips/Day 2/B.afplink.mp4";
  await page.get("loadView('footage')"); await page.quiet(40);
  originals(page, [B]);
  await viaSheet(page, "add", "Day 2");
  const pending = albumItems(page, "Day 2").filter((x) => x.pending).map((x) => x.key);
  await page.quiet(80);
  const real = albumItems(page, "Day 2").filter((x) => x.name === "B.MP4" && x.key !== "a:_Clips/Day 2/B.afplink.mp4").map((x) => x.key);
  assert.deepEqual(pending, ["a:_Clips/Day 2/B 2.afplink.mp4"]);
  assert.deepEqual(real, ["a:_Clips/Day 2/B 2.afplink.mp4"], "the page's name is the command's");
  assert.equal(page.calls("album_add").length, 2, "the second add of A wasn't sent");
  assert.equal(unsaved(page), "");
});

test("a moment and a photo copied into an album: at once, each with its file's picture; a tag on a copy being written waits for it", async () => {
  const { page } = await start({ delay: { album_copy: 120, set_tags: 20 } });
  page.get("S.view = 'footage'; S.path = []; S.flat = true; renderAll()");
  page.select(["m:" + TURTLE, "p:_Stills/A_1.5s.jpg"], "i");
  // (a rendered photo isn't copied: moved; the moment is a link)
  const copies = val(page, "albumSel(ctx()).copies");
  assert.deepEqual(copies, ["/P/" + TURTLE]);
  const mark = page.mark();
  page.get("albumCopy('_Clips/Day 1', " + JSON.stringify(copies) + ")");
  const now = albumItems(page, "Day 1").filter((x) => x.kind === "moment");
  assert.deepEqual(now.map((x) => [x.key, x.pending, x.thumb]), [["m:_Clips/Day 1/Turtle.afplink.mp4", true, "/P/" + TURTLE]]);
  // Tagged while it's written: shown at once, written after it
  page.get("Writes.run('tags', [DATA.moments.items.find((x) => x.id === '_Clips/Day 1/Turtle.afplink.mp4')], { items: [{ kind: 'moment', id: '_Clips/Day 1/Turtle.afplink.mp4', path: '/P/_Clips/Day 1/Turtle.afplink.mp4' }], add: ['copy'], remove: [] })");
  assert.deepEqual(val(page, "DATA.moments.items.find((x) => x.id === '_Clips/Day 1/Turtle.afplink.mp4').tags"), ["copy"]);
  await page.quiet(80);
  const order = page.since(mark).map((c) => c.cmd).filter((c) => c === "album_copy" || c === "set_tags");
  assert.deepEqual(order, ["album_copy", "set_tags"], "the tag waited for the copy");
  const m = val(page, "DATA.moments.items.find((x) => x.id === '_Clips/Day 1/Turtle.afplink.mp4')");
  assert.deepEqual([m.pending || false, m.tags], [false, ["copy"]]);
  assert.ok(val(page, "DATA.moments.items.some((x) => x.id === " + JSON.stringify(TURTLE) + ")"), "the one copied stays");
  assert.deepEqual(scans(page, mark), []);
  assert.equal(unsaved(page), "");
});

test("a copy that fails takes the tag made on it back with it", async () => {
  const { page, app } = await start({ delay: { album_copy: 80 } });
  app.fail.album_copy = () => "Read-only";
  page.get("albumCopy('_Clips/Day 1', ['/P/" + TURTLE + "'])");
  page.get("Writes.run('tags', [{ kind: 'moment', id: '_Clips/Day 1/Turtle.afplink.mp4', name: 'Turtle' }], { items: [{ kind: 'moment', id: '_Clips/Day 1/Turtle.afplink.mp4' }], add: ['copy'], remove: [] })");
  await page.until(() => /Read-only/.test(unsaved(page)));
  await page.quiet(60);
  assert.ok(!val(page, "DATA.moments.items.some((x) => x.id === '_Clips/Day 1/Turtle.afplink.mp4')"));
  assert.match(unsaved(page), /copying Turtle into the album Day 1\. Read-only/);
  assert.match(unsaved(page), /it follows a change that wasn.t saved/);
  assert.equal(page.calls("set_tags").length, 0);
});

test("Move to an album: a clip's link and a moment move at once, in the album and out of the old one; nothing read", async () => {
  const { page } = await start({ delay: { album_move: 120 } });
  page.get("S.view = 'footage'; S.path = []; S.flat = true; renderAll()");
  page.select(["a:_Clips/Day 1/A.afplink.mp4", "m:" + TURTLE], "i");
  const mark = page.mark();
  await viaSheet(page, "move", "Best");
  const best = albumItems(page, "Best");
  assert.deepEqual(best.map((x) => [x.key, x.pending]).sort(), [["a:_Clips/Best/A.afplink.mp4", true], ["m:_Clips/Best/Turtle.afplink.mp4", true]]);
  assert.ok(!albumItems(page, "Day 1").some((x) => x.name === "A.MP4"), "gone from Day 1 at once");
  assert.ok(!albumItems(page, "Reef").some((x) => x.name === "Turtle"));
  assert.equal(inAlbums(page, A), 1, "still in one album");
  // A move again of what's being written: refused, said
  page.select(["a:_Clips/Best/A.afplink.mp4"], "i");
  await page.get("askAlbum('move')");
  assert.equal(page.get("S.sheet"), null);
  assert.match(page.$("#toastMsg").textContent, /still being written/);
  await page.quiet(80);
  assert.deepEqual(albumItems(page, "Best").map((x) => [x.key, x.pending, x.link]).sort(), [["a:_Clips/Best/A.afplink.mp4", false, "/P/_Clips/Best/A.afplink.mp4"], ["m:_Clips/Best/Turtle.afplink.mp4", false, null]]);
  assert.deepEqual(scans(page, mark), []);
  assert.equal(unsaved(page), "");
});

test("Take out of the album: links and moments at once, the moment in Generated", async () => {
  const { page } = await start({ delay: { album_move: 100, delete_items: 100 } });
  page.get("S.view = 'footage'; S.path = ['Reef']; renderAll()");
  page.select(["m:" + TURTLE], "i");
  page.get("albumOut()");
  assert.ok(!albumItems(page, "Reef").some((x) => x.name === "Turtle"));
  assert.deepEqual(albumItems(page, "Generated").map((x) => x.key), ["m:_Moments/Generated/Turtle.afplink.mp4"]);
  await page.quiet(60);
  assert.deepEqual(albumItems(page, "Generated").map((x) => [x.key, x.pending]), [["m:_Moments/Generated/Turtle.afplink.mp4", false]]);
  assert.equal(unsaved(page), "");
});

test("Photos of the marks: the photo links show at once with the clip's picture, then as made; nothing read", async () => {
  const { page } = await start({ delay: { photo_add: 150 } });
  const mark = page.mark();
  // A clip's marks without photos: D's two (its probe says so)
  page.get("PROBES['/F/" + D + "'].marks = { times: [3, 12] }");
  originals(page, [D]);
  const picD = page.get("DATA.sources.items.find((x) => x.id === " + JSON.stringify(D) + ").thumbPath");
  await page.get("photoLinks()");
  assert.deepEqual(val(page, "DATA.stills.items.filter((x) => x.pending).map((x) => [x.id, x.thumbPath])"), [["_Stills/Generated/D_12.00s.afplink.jpg", picD], ["_Stills/Generated/D_3.00s.afplink.jpg", picD]]);
  await page.quiet(80);
  const real = val(page, "DATA.stills.items.filter((x) => x.id.startsWith('_Stills/Generated/D_')).map((x) => [x.id, !!x.pending, x.thumbPath])");
  assert.deepEqual(real, [["_Stills/Generated/D_12.00s.afplink.jpg", false, "/P/_Stills/Generated/D_12.00s.afplink.jpg"], ["_Stills/Generated/D_3.00s.afplink.jpg", false, "/P/_Stills/Generated/D_3.00s.afplink.jpg"]]);
  assert.deepEqual(scans(page, mark), []);
});

test("an album of three folders renamed: one change, shown whole; when its second folder can't be renamed, back whole", async () => {
  const { page, app, views } = await start({ delay: { rename_album: 100 } });
  // Egypt in _Clips, _Moments and _Stills
  await page.get("albumAdd('_Clips/Egypt', [DATA.sources.items.find((x) => x.id === " + JSON.stringify(A) + ")])");
  page.get("albumCopy('_Moments/Egypt', ['/P/" + TURTLE + "'])");
  await page.quiet(150);
  views.stills.items.push(Object.assign({}, views.stills.items[0], { id: "_Stills/Egypt/A_1.5s.jpg", dir: "_Stills/Egypt", path: "/P/_Stills/Egypt/A_1.5s.jpg" }));
  views.stills.dirs.push("_Stills/Egypt");
  await page.get("loadView('stills')"); await page.quiet(40);
  assert.deepEqual(val(page, "ALBUM_REAL.Egypt"), ["_Clips/Egypt", "_Moments/Egypt", "_Stills/Egypt"]);
  app.fail.rename_item = (a) => a.path.includes("/_Moments/") && "Permission denied";
  page.get("S.view = 'footage'; S.path = [AV_MARK + 'My albums']; S.flat = false; renderAll()");
  page.get("S.renaming = 'f:Egypt'; commitRename('Nile')");
  // At once: one album Nile with all three folders' items
  assert.deepEqual(val(page, "albumList()").filter((d) => d === "Egypt" || d === "Nile"), ["Nile"]);
  assert.equal(albumItems(page, "Nile").length, 3);
  await page.until(() => unsaved(page));
  await page.quiet(80);
  assert.deepEqual(val(page, "albumList()").filter((d) => d === "Egypt" || d === "Nile"), ["Egypt"], "back whole: never two albums");
  assert.equal(albumItems(page, "Egypt").length, 3);
  assert.match(unsaved(page), /Not saved: renaming the album Egypt to Nile\. Permission denied/);
  assert.ok(views.footage.clips_dirs.includes("_Clips/Egypt") && !views.footage.clips_dirs.includes("_Clips/Nile"), "the disk named back too");
  // …and when all three can be
  app.fail.rename_item = null;
  page.get("S.renaming = 'f:Egypt'; commitRename('Nile')");
  await page.quiet(150);
  assert.deepEqual(val(page, "ALBUM_REAL.Nile"), ["_Clips/Nile", "_Moments/Nile", "_Stills/Nile"]);
  assert.equal(page.calls("rename_album").length, 2);
});

test("New album: shown at once, ready to be named; the name typed is written after it's made", async () => {
  const { page } = await start({ delay: { new_folder: 100, rename_album: 30 } });
  page.get("S.view = 'footage'; S.path = []; renderAll()");
  page.get("newFolder()");
  assert.ok(val(page, "albumList()").includes("untitled folder"));
  assert.equal(page.get("S.renaming"), "f:untitled folder");
  page.get("commitRename('Iceland')");
  assert.ok(val(page, "albumList()").includes("Iceland"));
  await page.quiet(80);
  assert.deepEqual(page.calls().filter((c) => c.cmd === "new_folder" || c.cmd === "rename_album").map((c) => c.cmd), ["new_folder", "rename_album"]);
  assert.deepEqual(page.calls("new_folder")[0].args, { view: "footage", parent: "_Clips", name: "untitled folder" });
  assert.ok(val(page, "albumList()").includes("Iceland") && !val(page, "albumList()").includes("untitled folder"));
  assert.equal(unsaved(page), "");
});

test("closing the window while an album write is pending waits for it", async () => {
  const { page } = await start({ delay: { album_add: 150 } });
  originals(page, [A]);
  await viaSheet(page, "add", "Egypt");
  page.emit("close-waits");
  assert.equal(page.$("#closing").hidden, false);
  assert.match(page.$("#closingText").textContent, /1 change is still being written/);
  assert.equal(page.calls("answer_close").length, 0);
  await page.quiet(80);
  assert.equal(page.calls("answer_close").length, 1);
});
