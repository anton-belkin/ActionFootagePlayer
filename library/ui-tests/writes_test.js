"use strict";
// Screen first, disk after (../ui/writes.js, docs/library-responsiveness.md): the page against a mock of the app.
//   cd library/ui-tests && npm install && node --test

const test = require("node:test");
const assert = require("node:assert/strict");
const { open, smallViews, sleep } = require("./page");
const { fake } = require("./fakeapp");

const A = "Day 1/A.MP4", B = "Day 1/B.MP4", C = "Day 1/C.MP4";
const TURTLE = "_Moments/Reef/Turtle.afplink.mp4";

// The pages a test opened, closed when it ends however it ends (an open one keeps the process going)
const opened = [];
test.afterEach(() => { while (opened.length) opened.pop().close(); });
async function start(o) {
  const views = smallViews();
  const app = fake(views);
  const page = await open(Object.assign({ views }, o || {}, { commands: Object.assign({}, app.commands, (o && o.commands) || {}) }));
  opened.push(page);
  app.attach(page);
  return { page, app, views };
}
// A value of the page's as this side's own (the page's arrays are its window's)
const val = (page, expr) => JSON.parse(page.get("JSON.stringify(" + expr + ")"));
// (an item's row where the page is, by its id: Originals and Albums key a clip differently, page.js's keyOf)
const row = (page, id) => page.row(id);
// Its tags as its row shows them (Originals' "In an album" is drawn as one more such chip: not a tag)
const chips = (page, id) => Array.from(row(page, id).querySelectorAll(".chips .chip.tag")).map((e) => e.textContent).filter((t) => !/^In (an album|\d+ albums)$/.test(t));
// An item as shown: a clip ("footage") as Originals has it, a moment, photo or export as its own scan has it…
const shown = (page, view, id) => page.get("(DATA." + (view === "footage" ? "sources" : view) + ".items.find((x) => x.id === " + JSON.stringify(id) + ") || null)");
// …and as Albums show it (a clip: its link in an album)
const inAlbums = (page, id) => page.get("(DATA.footage.items.find((x) => x.id === " + JSON.stringify(id) + ") || null)");
// Originals (the tab the page opens on; the clips' list that was Footage's), in Day 1, the Tags part open, these
// clips selected
function inDay1(page, ids, tab) {
  page.get("S.view = 'sources'; S.path = ['Day 1']; showTab(" + JSON.stringify(tab || "tags") + "); renderAll()");
  page.select(ids);
}
// The Tags part is culling.js's since the merge: the new tag's field and its Add; a tag in use is a button, a click
// on one all of the selection has takes it off
function addTag(page, tag) { page.type("#tagdraft", tag); page.click('#side [data-c="add"]'); }
function untag(page, tag) { page.click('#side [data-c="tag"][data-v="' + tag + '"]'); }
const unsaved = (page) => (page.$("#unsaved").hidden ? "" : page.$("#unsaved .lines").textContent);

test("a tag shows before the app has answered, and no view is read to show it", async () => {
  const { page } = await start({ delay: { set_tags: 150 } });
  inDay1(page, [A, B]);
  const mark = page.mark();
  addTag(page, "turtle");
  // At once: the command is still to be answered
  assert.deepEqual(chips(page, A), ["reef", "turtle"]);
  assert.deepEqual(chips(page, B), ["turtle"]);
  assert.equal(page.calls("set_tags")[0].took, null);
  assert.equal(page.since(mark).filter((c) => c.cmd === "scan").length, 0);
  // The Tags part too
  assert.match(page.$("#side .chips").textContent, /turtle/);
  await page.quiet(60);
  assert.deepEqual(chips(page, A), ["reef", "turtle"]);
  assert.equal(page.get("Writes.pending()"), 0);
  assert.equal(unsaved(page), "");
  assert.deepEqual(page.errors, []);
});

test("a change that can't be written is taken back, and said until it's read", async () => {
  const { page, app } = await start({ delay: { set_tags: 80 } });
  app.fail.set_tags = () => "The share isn’t connected";
  inDay1(page, [A, B]);
  addTag(page, "turtle");
  assert.deepEqual(chips(page, B), ["turtle"]);
  await page.until(() => unsaved(page));
  assert.deepEqual(chips(page, A), ["reef"]);
  assert.deepEqual(chips(page, B), []);
  // Which change, on what, and why
  assert.match(unsaved(page), /Not saved: the tag “turtle” on A\.MP4, B\.MP4\. The share isn’t connected/);
  // It stays (the errors' own strip goes by itself after 9 s; this one is read first)
  await page.quiet(60);
  await sleep(300);
  assert.notEqual(unsaved(page), "");
  page.click('[data-a="unsavedok"]');
  assert.equal(unsaved(page), "");
});

test("quick changes to one item: written in the order made, a failed one doesn't undo a later one", async () => {
  const { page, app, views } = await start({ delay: { set_tags: 60 } });
  app.fail.set_tags = (a) => (a.add[0] === "first" ? "no" : null);
  inDay1(page, [B]);
  addTag(page, "first");
  addTag(page, "second");
  untag(page, "second");
  addTag(page, "third");
  assert.deepEqual(chips(page, B), ["first", "third"]);
  await page.until(() => page.get("Writes.pending()") === 0);
  await page.quiet(60);
  // The first wasn't written: only it is gone
  assert.deepEqual(chips(page, B), ["third"]);
  assert.deepEqual(views.footage.items.find((it) => it.id === B).tags, ["third"]);
  // One at a time, in order
  const calls = page.calls("set_tags");
  assert.deepEqual(calls.map((c) => c.args.add[0] || "-" + c.args.remove[0]), ["first", "second", "-second", "third"]);
  for (let i = 1; i < calls.length; i++) assert.ok(calls[i].at >= calls[i - 1].at + calls[i - 1].took, "the " + i + "th started before the one before it ended");
  assert.match(unsaved(page), /“first” on B\.MP4/);
  assert.doesNotMatch(unsaved(page), /third|second/);
});

test("a change from elsewhere while ours waits: ours stays on top, then both are there", async () => {
  const { page, views } = await start({ delay: { set_tags: 200 } });
  inDay1(page, [B]);
  addTag(page, "mine");
  // The player (or another computer) writes the clip's own file meanwhile, and the page reads the view again
  views.footage.items.find((it) => it.id === B).tags = ["theirs"];
  page.changed();
  await page.until(() => chips(page, B).includes("theirs"));
  assert.deepEqual(chips(page, B), ["theirs", "mine"]);
  assert.equal(page.get("Writes.pending()"), 1);
  await page.quiet(80);
  assert.deepEqual(chips(page, B), ["theirs", "mine"]);
  assert.deepEqual(views.footage.items.find((it) => it.id === B).tags, ["theirs", "mine"]);
});

test("the reading in flight when a change lands doesn't take it back", async () => {
  const views = smallViews();
  const app = fake(views);
  let before = null;
  const page = await open({ views, delay: { set_tags: 30, scan: 200 },
    // Each reading answers with the view as it was when it started
    atStart(cmd, a, rec) { if (cmd === "scan") rec.was = JSON.parse(JSON.stringify(views[a.view] || null)); },
    commands: Object.assign({}, app.commands, { scan: (a, rec) => rec.was || { root: "/P", view: a.view, dirs: [], items: [] } }) });
  opened.push(page);
  app.attach(page);
  inDay1(page, [B]);
  before = page.mark();
  page.get("loadView('footage').then(renderAll)");
  await sleep(5);
  addTag(page, "kept");
  await page.until(() => page.get("Writes.pending()") === 0);
  assert.deepEqual(chips(page, B), ["kept"]);
  // The reading that started before the change comes in now, without it
  await page.until(() => page.since(before).filter((c) => c.cmd === "scan" && c.args.view === "footage" && c.took !== null).length >= 1);
  await sleep(20);
  assert.deepEqual(chips(page, B), ["kept"]);
  await page.quiet(250);
  assert.deepEqual(chips(page, B), ["kept"]);
});

test("written only in part: said so, and the disk decides what shows", async () => {
  const { page, app } = await start({ delay: { set_tags: 40 } });
  app.failItems = (cmd, id) => (id === B ? "/F/Day 1/B.afp.json: Permission denied" : null);
  inDay1(page, [A, B, C]);
  addTag(page, "turtle");
  await page.until(() => unsaved(page));
  assert.match(unsaved(page), /Not all saved: the tag “turtle” on A\.MP4, B\.MP4, C\.MP4\. .*B\.afp\.json: Permission denied/);
  await page.quiet(80);
  assert.deepEqual(chips(page, A), ["reef", "turtle"]);
  assert.deepEqual(chips(page, B), []);
  assert.deepEqual(chips(page, C), ["turtle"]);
  assert.equal(page.get("Writes.pending()"), 0);
});

test("a location by hand shows at once, its place when it's looked up; taken off at once", async () => {
  const { page, views } = await start({ delay: { set_locations: 120, parse_location: 60 } });
  inDay1(page, [A, B], "geo");
  page.type("#geohand", "27.7333, 34.25");
  page.click('[data-a="geohand"]');
  const it = () => shown(page, "footage", B);
  assert.equal(it().geo.lat, 27.7333);
  assert.equal(it().geo.source, "manual");
  assert.equal(page.calls("set_locations").length, 0);
  await page.until(() => it().geo.place && it().geo.place.country === "Egypt");
  await page.quiet(80);
  assert.equal(views.footage.items.find((x) => x.id === B).own.location.lat, 27.7333);
  assert.equal(it().country, "Egypt");
  // …and on its link in its album
  assert.equal(inAlbums(page, B).geo.lat, 27.7333);
  assert.match(page.text("#toastMsg"), /South Sinai, Egypt: set for 2 clips/);
  // Taken off
  page.get("renderAll()");
  page.click('#side [data-a="georemove"]');
  assert.equal(it().geo, null);
  await page.quiet(80);
  assert.equal(views.footage.items.find((x) => x.id === B).own.location, null);
  assert.equal(unsaved(page), "");
});

test("Source settings show at once", async () => {
  const { page, views } = await start({ delay: { set_source: 120 } });
  inDay1(page, [A, B], "source");
  page.get("srcDraftSet('rotation', '180')");
  page.click('[data-a="srcapply"]');
  assert.equal(shown(page, "footage", B).turned, true);
  assert.match(row(page, B).querySelector(".sub").textContent, /turned/);
  assert.equal(page.calls("set_source")[0].took, null);
  await page.quiet(80);
  assert.deepEqual(views.footage.items.find((x) => x.id === B).own.source, { rotation: 180 });
  assert.equal(shown(page, "footage", A).turned, true);
  assert.equal(inAlbums(page, A).turned, true);
});

// The moments, where they are since the merge (the Moments tab is gone): in Albums, the album Reef (_Moments/Reef)
async function inMoments(page) {
  page.click('[data-a="view"][data-v="footage"]');
  await page.until(() => page.get("S.view === 'footage' && !S.scanning.moments"));
  await page.quiet(30);
  page.get("S.path = ['Reef']; renderAll()");
}
function rename(page, id, name, kind) {
  page.select([id], kind);
  page.get("startRename()");
  page.$("#rename").value = name;
  page.key("Enter", {}, page.$("#rename"));
}
const names = (page) => page.$$("#main .nm").map((e) => e.textContent);

test("a moment renamed shows at once, where its name puts it; one that can't be goes back", async () => {
  const { page, app, views } = await start({ delay: { rename_item: 120 } });
  await inMoments(page);
  assert.deepEqual(names(page), ["Ray", "Shark", "Turtle"]);
  rename(page, TURTLE, "Angel");
  assert.ok(names(page).includes("Angel") && !names(page).includes("Turtle"));
  assert.equal(page.calls("rename_item")[0].took, null);
  assert.deepEqual(page.calls("rename_item")[0].args, { view: "moments", kind: "moment", path: "/P/_Moments/Reef/Turtle.afplink.mp4", name: "Angel" });
  await page.quiet(80);
  assert.ok(views.moments.items.some((x) => x.id === "_Moments/Reef/Angel.afplink.mp4"));
  // As a reading of the disk orders them
  assert.deepEqual(names(page), views.moments.items.map((x) => x.name));
  // Refused by the disk: back, and said
  app.fail.rename_item = () => "/P/_Moments/Reef/Ray.afplink.mp4: Permission denied";
  rename(page, "_Moments/Reef/Ray.afplink.mp4", "Manta");
  assert.ok(names(page).includes("Manta"));
  await page.until(() => unsaved(page));
  assert.ok(names(page).includes("Ray") && !names(page).includes("Manta"));
  assert.match(unsaved(page), /Not saved: renaming Ray to Manta\. .*Permission denied/);
  // A name that's taken, or no file's: said before anything shows, nothing asked of the app
  const n = page.calls("rename_item").length;
  rename(page, "_Moments/Reef/Ray.afplink.mp4", "Shark");
  rename(page, "_Moments/Reef/Ray.afplink.mp4", "a/b");
  assert.equal(page.calls("rename_item").length, n);
  assert.ok(names(page).includes("Ray"));
});

test("a change to something just renamed, when the rename fails: taken back with it, not sent", async () => {
  const { page, app } = await start({ delay: { rename_item: 100, set_tags: 30 } });
  app.fail.rename_item = () => "Permission denied";
  await inMoments(page);
  rename(page, TURTLE, "Angel");
  page.get("showTab('tags')");
  page.select(["_Moments/Reef/Angel.afplink.mp4"]);
  addTag(page, "best");
  assert.match(row(page, "_Moments/Reef/Angel.afplink.mp4").textContent, /best/);
  await page.until(() => page.get("Writes.pending()") === 0);
  await page.quiet(60);
  assert.equal(page.calls("set_tags").length, 0);
  assert.ok(names(page).includes("Turtle"));
  assert.doesNotMatch(row(page, TURTLE).textContent, /best/);
  assert.match(unsaved(page), /renaming Turtle to Angel/);
  assert.match(unsaved(page), /the tag “best” on Angel\. it follows a change that wasn’t saved|the tag “best” on Angel/);
});

test("an album renamed: what's in it follows at once, its moments and its clips' links", async () => {
  const { page, views } = await start({ delay: { rename_item: 120 } });
  await inMoments(page);
  page.get("S.path = [AV_MARK + 'My albums']; renderAll()"); // the user's albums are in My albums
  const albums = () => val(page, "DATA.footage.dirs").filter((d) => !d.startsWith("Other folders"));
  assert.deepEqual(albums(), ["Day 1", "Day 2", "Reef"]);
  // An album of moments: its folder in _Moments
  rename(page, "Reef", "Sea", "f");
  assert.deepEqual(albums(), ["Day 1", "Day 2", "Sea"]);
  assert.deepEqual(val(page, "DATA.moments.dirs"), ["Sea"]);
  assert.equal(shown(page, "moments", "_Moments/Sea/Turtle.afplink.mp4").path, "/P/_Moments/Sea/Turtle.afplink.mp4");
  assert.equal(page.get("DATA.footage.items.find((x) => x.key === 'm:_Moments/Sea/Turtle.afplink.mp4').dir"), "Sea");
  // (asked from Albums: the folder is named by its path)
  assert.deepEqual(page.calls("rename_item")[0].args, { view: "footage", kind: "folder", path: "/P/_Moments/Reef", name: "Sea" });
  await page.quiet(80);
  assert.deepEqual(albums(), ["Day 1", "Day 2", "Sea"]);
  assert.deepEqual(views.moments.dirs, ["_Moments", "_Moments/Sea"]);
  // An album of clips: its folder in _Clips, the links in it. The clip itself stays where it is in Originals
  rename(page, "Day 1", "First day", "f");
  assert.deepEqual(albums(), ["Day 2", "First day", "Sea"]);
  assert.equal(inAlbums(page, A).dir, "First day");
  assert.equal(inAlbums(page, A).key, "a:_Clips/First day/A.afplink.mp4");
  assert.equal(inAlbums(page, A).link_file, "/P/_Clips/First day/A.afplink.mp4");
  assert.equal(shown(page, "footage", A).dir, "Day 1");
  await page.quiet(80);
  assert.equal(views.footage.links.find((l) => l.clip === A).rel, "_Clips/First day/A.afplink.mp4");
  assert.equal(inAlbums(page, A).dir, "First day");
  assert.equal(inAlbums(page, A).link_file, "/P/_Clips/First day/A.afplink.mp4");
  assert.equal(unsaved(page), "");
});

test("deleted: gone at once; what couldn't be comes back", async () => {
  const { page, app, views } = await start({ delay: { delete_items: 120 } });
  await inMoments(page);
  page.select([TURTLE]);
  page.get("deleteSelection()");
  assert.ok(!names(page).includes("Turtle"));
  assert.equal(page.calls("delete_items")[0].took, null);
  // Its clip's count of moments, at once too
  assert.equal(shown(page, "footage", A).moments, 0);
  assert.equal(inAlbums(page, A).moments, 0);
  await page.quiet(80);
  assert.ok(!views.moments.items.some((x) => x.id === TURTLE));
  assert.match(page.text("#toastMsg"), /1 moved to the Trash/);
  app.fail.delete_items = () => "Finder couldn’t move it to the Trash";
  page.select(["_Moments/Reef/Ray.afplink.mp4"]);
  page.get("deleteSelection()");
  assert.ok(!names(page).includes("Ray"));
  await page.until(() => unsaved(page));
  assert.ok(names(page).includes("Ray"));
  assert.match(unsaved(page), /Not saved: deleting Ray\. Finder couldn’t/);
});

test("changes still being written: said in Tasks after half a second, the app told how many", async () => {
  const { page } = await start({ delay: { set_tags: 900 } });
  inDay1(page, [B]);
  addTag(page, "slow");
  await sleep(100);
  const badge = () => page.$('.rb[data-v="queue"] .count');
  assert.equal(badge().hidden, true);
  assert.deepEqual(page.calls("writes_pending").map((c) => c.args.n), [1]);
  await page.until(() => !badge().hidden, 2000);
  assert.equal(badge().textContent, "1");
  page.get("showTab('queue'); renderSide()");
  assert.match(page.text("#qtab"), /Saving changes/);
  await page.until(() => page.get("Writes.pending()") === 0, 3000);
  await sleep(30);
  assert.equal(badge().hidden, true);
  assert.deepEqual(page.calls("writes_pending").map((c) => c.args.n), [1, 0]);
});

test("the window closed with changes waiting: they're written, then it closes", async () => {
  const { page } = await start({ delay: { set_tags: 300 } });
  inDay1(page, [B]);
  addTag(page, "last");
  // The app kept the window (writes pending) and says so
  page.state.close_waits = true;
  page.emit("close-waits");
  assert.equal(page.$("#closing").hidden, false);
  assert.match(page.text("#closingText"), /1 change is still being written/);
  assert.equal(page.calls("answer_close").length, 0);
  await page.until(() => page.calls("answer_close").length === 1, 3000);
  assert.deepEqual(page.calls("answer_close")[0].args, { quit: true });
  assert.ok(page.calls("set_tags")[0].took !== null);
});

test("another project opened with changes waiting: the ones not sent are said, not written into it", async () => {
  const { page } = await start({ delay: { set_tags: 150 } });
  inDay1(page, [B]);
  addTag(page, "one");
  addTag(page, "two");
  page.get("loadProject()");
  await page.until(() => unsaved(page));
  await page.quiet(200);
  assert.equal(page.calls("set_tags").length, 1);
  assert.match(unsaved(page), /“two” on B\.MP4\. another project was opened/);
});
