"use strict";
// The features merged in the night of 2026-11-16, together: culling and tags (../ui/culling.js) on what Albums show
// (../ui/albums.js: a clip's link, a moment, a photo) and on Timeline's items (../ui/timeline.js), written the
// screen-first way (../ui/writes.js) and corrected item by item (../ui/changes.js): what's put on an item in one
// place shows at once in the other places the same item is, and no view is read again.
//   cd library/ui-tests && npm install && npm test

const test = require("node:test");
const assert = require("node:assert/strict");
const { open, smallViews, sleep } = require("./page");
const { fake } = require("./fakeapp");

const A = "Day 1/A.MP4", B = "Day 1/B.MP4", C = "Day 1/C.MP4", D = "Day 2/D.MP4";
const TURTLE = "_Moments/Reef/Turtle.afplink.mp4", SHARK = "_Moments/Reef/Shark.afplink.mp4", PHOTO = "_Stills/A_1.5s.jpg";
const linkOf = (id) => "a:_Clips/" + id.replace(/\.MP4$/, ".afplink.mp4");

const opened = [];
// (the new tag's field looks 150 ms after it lost the focus: let that pass before the window goes)
test.afterEach(async () => { await sleep(170); while (opened.length) opened.pop().close(); });
// `changed`: the commands answer with what they changed (the app as it is); false: with a list of problems (the page
// then keeps its own patch)
async function start(changed, o) {
  const views = smallViews();
  const app = fake(views, { changed });
  const page = await open(Object.assign({ views }, o || {}, { commands: Object.assign({}, app.commands, (o && o.commands) || {}) }));
  opened.push(page);
  app.attach(page);
  return { page, app, views };
}
const val = (page, expr) => JSON.parse(page.get("JSON.stringify(" + expr + ")"));
// What culling shows of an item: in a view's data, by a field
const cullOf = (page, list, field, v) => val(page, "(() => { const it = " + list + ".find((x) => x." + field + " === " + JSON.stringify(v) + "); return it ? [it.pick || null, it.rating || 0, it.tags] : null; })()");
const original = (page, id) => cullOf(page, "DATA.sources.items", "id", id);
const inAlbums = (page, key) => cullOf(page, "DATA.footage.items", "key", key);
const chipsOf = (page, id) => page.row(id).querySelector(".chips").textContent;
const scans = (page, mark) => page.since(mark).filter((c) => c.cmd === "scan").map((c) => c.args.view);
const unsaved = (page) => (page.$("#unsaved").hidden ? "" : page.$("#unsaved .lines").textContent);
function go(page, view, path, tab) {
  page.get("S.view = " + JSON.stringify(view) + "; S.path = " + JSON.stringify(path || []) + "; S.q = ''; S.flat = false; S.filters = {}; showTab(" + JSON.stringify(tab || "tags") + "); S.sel = {}; renderAll()");
}
const pick = (page, v) => page.click('#side [data-c="pick"][data-v="' + v + '"]');
const rate = (page, n) => page.click('#side [data-c="rate"][data-v="' + n + '"]');
function addTag(page, tag) { page.type("#tagdraft", tag); page.click('#side [data-c="add"]'); }
const key = (page, k, code) => page.key(k, { code });

for (const changed of [true, false]) {
  const how = changed ? "the app says what it changed" : "the app answers with problems only";

  test("a clip selected in its album: Accept, stars and a tag show at once on its link and in Originals, nothing read again (" + how + ")", async () => {
    const { page, views } = await start(changed, { delay: { set_pick: 60, set_rating: 60, set_tags: 60 } });
    go(page, "footage", ["Day 1"]);
    page.select([linkOf(B)]);
    assert.deepEqual(val(page, "Object.keys(S.sel)"), ["i:" + linkOf(B)]);
    const mark = page.mark();
    pick(page, "accept"); rate(page, 4); addTag(page, "night dive");
    // At once: none of the three is answered yet
    assert.equal(page.calls("set_pick")[0].took, null);
    assert.equal(page.calls("set_rating").length + page.calls("set_tags").length, 0);
    assert.deepEqual(inAlbums(page, linkOf(B)), ["accept", 4, ["night dive"]]);
    assert.deepEqual(original(page, B), ["accept", 4, ["night dive"]]);
    assert.match(chipsOf(page, linkOf(B)), /✓ Accepted★★★★night dive/);
    assert.equal(page.$('#side [data-c="pick"][data-v="accept"]').getAttribute("aria-pressed"), "true");
    // The clip's own: asked for the clip, not for its link
    await page.until(() => page.get("Writes.pending()") === 0);
    await page.quiet(60);
    assert.deepEqual(page.calls("set_pick")[0].args, { items: [{ kind: "clip", id: B, path: "/F/" + B }], pick: "accept" });
    assert.deepEqual([views.footage.items[1].pick, views.footage.items[1].rating, views.footage.items[1].tags], ["accept", 4, ["night dive"]]);
    assert.deepEqual(inAlbums(page, linkOf(B)), ["accept", 4, ["night dive"]]);
    assert.deepEqual(original(page, B), ["accept", 4, ["night dive"]]);
    // (an app that doesn't say what it changed: the catalog's change reads the views again, as before the pinpoint update)
    if (changed) assert.deepEqual(scans(page, mark), []);
    // …and its row in Originals
    go(page, "sources", ["Day 1"]);
    assert.match(chipsOf(page, B), /✓ Accepted★★★★night dive/);
    // The other clips of the album: untouched
    assert.deepEqual(inAlbums(page, linkOf(C)), [null, 0, []]);
    assert.equal(unsaved(page), "");
    assert.deepEqual(page.errors, []);
  });

  test("a moment and a photo selected in Albums: the same, on the moment, the photo and where Albums show them (" + how + ")", async () => {
    const { page, views } = await start(changed, { delay: { set_pick: 40, set_rating: 40, set_tags: 40 } });
    go(page, "footage", ["Reef"]);
    page.select([TURTLE]);
    const mark = page.mark();
    pick(page, "reject"); rate(page, 2); addTag(page, "blurry");
    assert.deepEqual(inAlbums(page, "m:" + TURTLE), ["reject", 2, ["blurry"]]);
    assert.deepEqual(cullOf(page, "DATA.moments.items", "id", TURTLE), ["reject", 2, ["blurry"]]);
    assert.match(chipsOf(page, TURTLE), /✕ Rejected★★blurry/);
    // A photo (in _Stills itself: at Albums' top) and a moment together, by the list of every item
    page.get("S.path = []; S.flat = true; renderAll()");
    page.select([PHOTO, SHARK]);
    pick(page, "accept"); rate(page, 5); addTag(page, "print");
    assert.deepEqual(inAlbums(page, "p:" + PHOTO), ["accept", 5, ["print"]]);
    assert.deepEqual(cullOf(page, "DATA.stills.items", "id", PHOTO), ["accept", 5, ["print"]]);
    assert.deepEqual(inAlbums(page, "m:" + SHARK), ["accept", 5, ["print"]]);
    assert.match(chipsOf(page, PHOTO), /✓ Accepted★★★★★print/);
    await page.until(() => page.get("Writes.pending()") === 0);
    await page.quiet(60);
    assert.deepEqual(page.calls("set_pick").map((c) => c.args.items.map((x) => x.kind + " " + x.id).sort()), [["moment " + TURTLE], ["moment " + SHARK, "still " + PHOTO]]);
    const m = views.moments.items.find((x) => x.id === TURTLE), ph = views.stills.items.find((x) => x.id === PHOTO);
    assert.deepEqual([m.pick, m.rating, m.tags, ph.pick, ph.rating, ph.tags], ["reject", 2, ["blurry"], "accept", 5, ["print"]]);
    assert.deepEqual(inAlbums(page, "m:" + TURTLE), ["reject", 2, ["blurry"]]);
    assert.deepEqual(inAlbums(page, "p:" + PHOTO), ["accept", 5, ["print"]]);
    if (changed) assert.deepEqual(scans(page, mark), []);
    // The moment in its clip's own view too
    await page.get("showClip(" + JSON.stringify(A) + ")");
    assert.deepEqual(cullOf(page, "DATA.clip.items", "key", "m:" + TURTLE), ["reject", 2, ["blurry"]]);
    assert.equal(unsaved(page), "");
    assert.deepEqual(page.errors, []);
  });

  test("Timeline: P, X and the digits on its items, a clip's showing in Originals and in its album too (" + how + ")", async () => {
    const { page, views } = await start(changed, { delay: { set_pick: 40, set_rating: 40 } });
    page.click('[data-a="view"][data-v="footage"]');
    await page.quiet(40);
    // An album's tile: one click marks it and opens nothing, as with a folder; a double click opens it
    page.click('[data-a="aview"][data-v="timeline"]');
    await page.quiet(40);
    assert.equal(page.get("S.path.length"), 0);
    assert.ok(page.get(`document.querySelector('[data-a="aview"][data-v="timeline"]').classList.contains('sel')`));
    page.dblclick('[data-a="aview"][data-v="timeline"]');
    await page.quiet(40);
    assert.equal(page.get("S.path[0].slice(1)"), "Timeline");
    // Enter on the tile (a click with no mouse behind it) opens it too
    page.get("S.path = []; renderAll()");
    page.click('[data-a="aview"][data-v="timeline"]', { detail: 0 });
    await page.quiet(40);
    assert.equal(page.get("S.path[0].slice(1)"), "Timeline");
    page.get("TL.level = 'days'; showTab('tags'); renderAll()");
    // Its items: every original once (Originals' own items), the moments, the photos
    const visible = val(page, "VISIBLE");
    assert.ok(visible.includes("i:s:" + B) && visible.includes("i:" + TURTLE) && visible.includes("i:" + PHOTO), visible.join(" "));
    const tl = (id) => cullOf(page, "items()", "id", id);
    const mark = page.mark();
    // A clip: P, then 3 (one selected: the next one is selected after each)
    page.select([B]);
    key(page, "p", "KeyP");
    assert.deepEqual(tl(B), ["accept", 0, []]);
    assert.deepEqual(original(page, B), ["accept", 0, []]);
    assert.deepEqual(inAlbums(page, linkOf(B)), ["accept", 0, []]);
    // (one selected: the next one of the day is selected after)
    assert.deepEqual(val(page, "Object.keys(S.sel)"), [visible[visible.indexOf("i:s:" + B) + 1]]);
    page.select([B]);
    // (a day's cards carry no chips: what the clip is now is said by the Tags part)
    assert.equal(page.$('#side [data-c="pick"][data-v="accept"]').getAttribute("aria-pressed"), "true");
    key(page, "3", "Digit3");
    assert.deepEqual(tl(B), ["accept", 3, []]);
    // A moment: X; a photo: 5. Two clips together: X on both, the selection stays
    page.select([TURTLE]); key(page, "x", "KeyX");
    page.select([PHOTO]); key(page, "5", "Digit5");
    page.select([C, D]); key(page, "x", "KeyX");
    assert.deepEqual(tl(TURTLE), ["reject", 0, []]);
    assert.deepEqual(inAlbums(page, "m:" + TURTLE), ["reject", 0, []]);
    assert.deepEqual(tl(PHOTO), [null, 5, []]);
    assert.deepEqual(inAlbums(page, "p:" + PHOTO), [null, 5, []]);
    assert.deepEqual([tl(C)[0], tl(D)[0], original(page, D)[0], inAlbums(page, linkOf(D))[0]], ["reject", "reject", "reject", "reject"]);
    assert.equal(val(page, "Object.keys(S.sel)").length, 2);
    await page.until(() => page.get("Writes.pending()") === 0);
    await page.quiet(60);
    const disk = (v, id) => { const it = views[v].items.find((x) => x.id === id); return [it.pick || null, it.rating || 0]; };
    assert.deepEqual([disk("footage", B), disk("moments", TURTLE), disk("stills", PHOTO), disk("footage", C), disk("footage", D)], [["accept", 3], ["reject", 0], [null, 5], ["reject", 0], ["reject", 0]]);
    assert.deepEqual([tl(B), tl(TURTLE), tl(PHOTO)], [["accept", 3, []], ["reject", 0, []], [null, 5, []]]);
    if (changed) assert.deepEqual(scans(page, mark), []);
    // Hide rejected takes them out of Timeline too
    page.click('#top [data-c="hide"]');
    const left = val(page, "VISIBLE");
    assert.ok(!left.includes("i:s:" + C) && !left.includes("i:s:" + D) && !left.includes("i:" + TURTLE) && left.includes("i:s:" + B), left.join(" "));
    page.click('#top [data-c="hide"]');
    assert.equal(unsaved(page), "");
    assert.deepEqual(page.errors, []);
  });

  test("Take out of the album, on a clip's link: gone from the album at once, the clip stays in Originals (" + how + ")", async () => {
    const { page, app, views } = await start(changed, { delay: { delete_items: 120 } });
    go(page, "footage", ["Day 1"], "info");
    page.select([linkOf(B)]);
    const names = () => page.$$("#main .nm").map((e) => e.textContent);
    assert.deepEqual(names(), ["A.MP4", "B.MP4", "C.MP4"]);
    page.click('#main [data-a="albumout"]');
    // At once: the app hasn't answered
    assert.equal(page.calls("delete_items")[0].took, null);
    assert.deepEqual(page.calls("delete_items")[0].args, { items: [{ kind: "preview", path: "/P/_Clips/Day 1/B.afplink.mp4" }] });
    assert.deepEqual(names(), ["A.MP4", "C.MP4"]);
    assert.equal(inAlbums(page, linkOf(B)), null);
    assert.deepEqual(original(page, B), [null, 0, []]);
    assert.equal(page.get("DATA.sources.items.find((x) => x.id === " + JSON.stringify(B) + ").albums"), 0);
    await page.until(() => page.get("Writes.pending()") === 0);
    await page.quiet(80);
    assert.ok(!views.footage.links.some((l) => l.clip === B));
    assert.deepEqual(names(), ["A.MP4", "C.MP4"]);
    assert.match(page.text("#toastMsg"), /1 item taken out of the album: the originals are untouched/);
    // In Originals: there, in no album now
    go(page, "sources", ["Day 1"], "info");
    assert.deepEqual(names(), ["A.MP4", "B.MP4", "C.MP4"]);
    assert.doesNotMatch(chipsOf(page, B), /In an album/);
    assert.match(chipsOf(page, A), /In an album/);
    assert.equal(unsaved(page), "");
    // One that can't be taken out comes back, said so
    app.fail.delete_items = () => "Finder couldn’t move it to the Trash";
    go(page, "footage", ["Day 1"], "info");
    page.select([linkOf(C)]);
    page.click('#main [data-a="albumout"]');
    assert.deepEqual(names(), ["A.MP4"]);
    await page.until(() => unsaved(page));
    await page.quiet(80);
    assert.deepEqual(names(), ["A.MP4", "C.MP4"]);
    assert.match(unsaved(page), /Not saved: deleting the link of C\.MP4\. Finder couldn’t/);
    assert.deepEqual(page.errors, []);
  });
}
