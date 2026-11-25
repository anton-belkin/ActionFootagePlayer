"use strict";
// A folder renamed (or moved) keeps its pictures (Anton, 2026-11-18: "when I rename a folder in Albums its four
// previews turn into grey squares", for a long time). The window shows the new name at once and the disk is renamed
// after (docs/design/library-responsiveness.md), so the pictures must come with the new paths: asked for before the disk has
// them, the app has no file there and the page kept "no picture" for good.
//   cd library/ui-tests && npm install && node --test rename_thumbs_test.js

const test = require("node:test");
const assert = require("node:assert/strict");
const { open, smallViews } = require("./page");
const { fake } = require("./fakeapp");

const opened = [];
test.afterEach(() => { while (opened.length) opened.pop().close(); });

// The app's `thumb` as it is: a picture only for a file that's on the disk (src/index.rs's thumb_bytes stats it
// first), the disk being the fake app's views
function onDisk(views, p) {
  const f = views.footage;
  if ((f.links || []).some((l) => l.path === p)) return true;
  if (f.items.some((it) => it.path === p || it.preview === p)) return true;
  return ["moments", "stills", "exports"].some((v) => views[v] && views[v].items.some((it) => it.path === p));
}
async function start(o) {
  const views = smallViews();
  const app = fake(views, { changed: true });
  const thumbs = [];
  app.commands.thumb = (a) => { thumbs.push(a.path); return onDisk(views, a.path) ? "data:image/jpeg;base64," + Buffer.from(a.path).toString("base64") : null; };
  const page = await open(Object.assign({ views }, o || {}, { commands: app.commands }));
  opened.push(page);
  app.attach(page);
  return { page, app, views, thumbs };
}
const val = (page, expr) => JSON.parse(page.get("JSON.stringify(" + expr + ")"));
// A folder's tile as drawn: its four places, each a picture shown (its file), hidden (no picture) or still to come
function tile(page, name) {
  return val(page, "(() => { const c = Array.from(document.querySelectorAll('#main .nm')).find((e) => e.textContent === " + JSON.stringify(name) + ");" +
    " if (!c) return null; const m = c.closest('[data-v]').querySelector('.mosaic'); if (!m) return null;" +
    " return Array.from(m.querySelectorAll('i')).map((i) => { const g = i.querySelector('img'); return !g ? 'empty' : g.hidden ? 'grey' : g.getAttribute('src') ? atob(g.getAttribute('src').split(',')[1]) : 'waiting'; }); })()");
}
const myAlbums = (page) => page.get("S.view = 'footage'; S.path = [AV_MARK + 'My albums']; S.flat = false; S.layout = 'grid'; renderAll()");

test("an album renamed: its tile keeps its four pictures at once, and after the disk is renamed; none made again", async () => {
  const { page, thumbs } = await start({ delay: { rename_album: 150 } });
  myAlbums(page);
  await page.quiet(60);
  const was = tile(page, "Day 1");
  assert.deepEqual(was, ["/P/_Clips/Day 1/A.afplink.mp4", "/P/_Clips/Day 1/B.afplink.mp4", "/P/_Clips/Day 1/C.afplink.mp4", "empty"]);
  const asked = thumbs.length;
  page.get("S.renaming = 'f:Day 1'; commitRename('Nile')");
  // At once, the disk not renamed yet
  assert.deepEqual(tile(page, "Nile"), was, "the same pictures right after the rename");
  await page.quiet(40);
  assert.deepEqual(tile(page, "Nile"), was, "…while the disk is being renamed");
  await page.quiet(250);
  assert.equal(page.calls("rename_album").length, 1);
  assert.ok(page.calls("rename_album")[0].took != null, "written");
  assert.deepEqual(tile(page, "Nile"), was, "…and once it is");
  assert.deepEqual(thumbs.slice(asked), [], "no picture asked again: the files are the same");
  // Inside the album: each item's own picture too
  page.get("S.path = ['Nile']; renderAll()");
  await page.quiet(40);
  const shown = val(page, "Array.from(document.querySelectorAll('#main img[data-thumb]')).map((g) => g.hidden ? 'grey' : g.getAttribute('src') ? 'shown' : 'waiting')");
  assert.deepEqual(shown, ["shown", "shown", "shown"]);
  assert.deepEqual(page.errors, []);
});

test("a picture asked for while the disk isn't renamed yet isn't kept as none", async () => {
  // (a picture the page never had: the new path asked at once, the app has no file there yet)
  const { page } = await start({ delay: { rename_album: 150 } });
  myAlbums(page);
  await page.quiet(40);
  page.get("THUMBS.clear()");
  page.get("S.renaming = 'f:Day 1'; commitRename('Nile')");
  await page.quiet(300);
  assert.deepEqual(tile(page, "Nile"), ["/P/_Clips/Nile/A.afplink.mp4", "/P/_Clips/Nile/B.afplink.mp4", "/P/_Clips/Nile/C.afplink.mp4", "empty"]);
  assert.deepEqual(page.errors, []);
});

test("a folder with folders in it renamed: the inner album's pictures follow too", async () => {
  const { page, app, views } = await start({ delay: { rename_album: 150 } });
  // Day 1/Dives: an album in the album
  await page.get("albumAdd('_Clips/Day 1/Dives', [DATA.sources.items.find((x) => x.id === 'Day 2/D.MP4'), DATA.sources.items.find((x) => x.id === 'Day 2/E.MP4')])");
  await page.quiet(120);
  myAlbums(page);
  page.get("S.path = ['Day 1']; renderAll()");
  await page.quiet(60);
  const inner = tile(page, "Dives");
  assert.deepEqual(inner, ["/P/_Clips/Day 1/Dives/D.afplink.mp4", "/P/_Clips/Day 1/Dives/E.afplink.mp4", "empty", "empty"]);
  myAlbums(page);
  await page.quiet(40);
  page.get("S.renaming = 'f:Day 1'; commitRename('Nile')");
  page.get("S.path = ['Nile']; renderAll()");
  assert.deepEqual(tile(page, "Dives"), inner, "at once");
  await page.quiet(300);
  assert.ok(views.footage.clips_dirs.includes("_Clips/Nile/Dives"), "the disk renamed");
  assert.deepEqual(tile(page, "Dives"), inner, "after");
  assert.equal(app.done.filter((d) => d[0] === "rename_item").length, 1);
  assert.deepEqual(page.errors, []);
});

test("clips moved into another album: their pictures come along, none asked again", async () => {
  const { page, thumbs, views } = await start({ delay: { album_move: 150 } });
  myAlbums(page);
  await page.quiet(60);
  const was = tile(page, "Day 1");
  const asked = thumbs.length;
  page.get("albumMove('_Clips/Day 2', ['/P/_Clips/Day 1/A.afplink.mp4', '/P/_Clips/Day 1/B.afplink.mp4'], 'moving')");
  page.get("renderAll()");
  const shownIn = () => val(page, "(() => { S.path = ['Day 2']; renderAll(); return Array.from(document.querySelectorAll('#main img[data-thumb]')).map((g) => g.hidden ? 'grey' : g.getAttribute('src') ? atob(g.getAttribute('src').split(',')[1]) : 'waiting'); })()");
  assert.deepEqual(shownIn(page).sort(), [was[0], was[1], "/P/_Clips/Day 2/D.afplink.mp4", "/P/_Clips/Day 2/E.afplink.mp4"].sort(), "at once");
  await page.quiet(250);
  assert.ok(views.footage.links.some((l) => l.path === "/P/_Clips/Day 2/A.afplink.mp4"), "moved on the disk");
  assert.deepEqual(shownIn(page).sort(), [was[0], was[1], "/P/_Clips/Day 2/D.afplink.mp4", "/P/_Clips/Day 2/E.afplink.mp4"].sort(), "after");
  assert.deepEqual(thumbs.slice(asked), []);
  assert.deepEqual(page.errors, []);
});
