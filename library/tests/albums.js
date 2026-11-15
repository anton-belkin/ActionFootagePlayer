// The Library's page in jsdom against a mock of the app (mock.js): Albums, and links made from a selection.
// By hand (jsdom isn't a dependency of the project: any copy of it):
//   JSDOM=<path to node_modules/jsdom> node library/tests/albums.js library/ui
const path = require("path"), fs = require("fs");
const { JSDOM } = require(process.env.JSDOM);
const ui = process.argv[2];
const html = fs.readFileSync(path.join(ui, "index.html"), "utf8").replace(/<script src="[^"]+"><\/script>\s*/g, "");
const dom = new JSDOM(html, { runScripts: "outside-only", pretendToBeVisual: true, url: "http://localhost/" });
const w = dom.window;
w.CSS = { escape: (s) => String(s).replace(/[^a-zA-Z0-9_-]/g, "\\$&") };
w.Element.prototype.scrollIntoView = () => {};
const { calls, state, ROOT } = require("./mock.js")(w);
const errors = [];
w.addEventListener("error", (e) => errors.push(String(e.error && e.error.stack || e.message)));
// The page's scripts, in index.html's order, as one (they share their top-level names, as in the page)
const scripts = Array.from(fs.readFileSync(path.join(ui, "index.html"), "utf8").matchAll(/<script src="([^"]+)">/g)).map((m) => m[1]);
w.eval(scripts.map((s) => fs.readFileSync(path.join(ui, s), "utf8").replace(/^"use strict";/, "")).join("\n;\n") +
  "\n;window.__T = { ALBUM_SOURCES, S, arrange, renderAll };");
const $ = (id) => w.document.getElementById(id);
const tick = (ms = 40) => new Promise((r) => setTimeout(r, ms));
const click = (el, o) => el.dispatchEvent(new w.MouseEvent("click", Object.assign({ bubbles: true, detail: 1 }, o || {})));
const dbl = (el) => el.dispatchEvent(new w.MouseEvent("dblclick", { bubbles: true }));
const by = (a, v) => w.document.querySelector('[data-a="' + a + '"]' + (v != null ? '[data-v="' + v.replace(/"/g, '\\"') + '"]' : ""));
const names = () => Array.from(w.document.querySelectorAll("#main .nm")).map((e) => e.textContent);
const tab = (v) => by("view", v);
const ok = (cond, what) => { if (!cond) { console.log("FAIL: " + what); process.exitCode = 1; } else console.log("ok: " + what); };
const last = (cmd) => calls.filter((c) => c[0] === cmd).pop();
(async () => {
  await tick(500);
  ok(scripts.join() === "albums.js,app.js", "the page loads albums.js before app.js");
  const tabs = Array.from(w.document.querySelectorAll('[data-a="view"]')).map((e) => e.textContent);
  ok(tabs[0] === "Originals" && tabs[1] === "Albums" && !tabs.includes("Footage"), "the tabs: " + tabs.join(", "));
  ok(tab("sources").classList.contains("on"), "it opens on Originals");

  // Albums: the user's albums at the top, the rest of the project's folder under Other folders
  click(tab("footage")); await tick(120);
  ok(names().join("|") === "Best of the year|Egypt|Other folders", "Albums' top: " + names().join("|"));
  ok(!$("main").textContent.includes("Without a clip preview"), "no folder of clips without a preview");
  dbl(by("pick", "f:Egypt")); await tick();
  ok(names().join("|") === "Dives|DJI_20261020083012_0001_D.MP4|GX010123.MP4", "an album: its albums, then its links as their clips: " + names().join("|"));
  dbl(by("pick", "f:Egypt/Dives")); await tick();
  ok(names().join("|") === "GX010124.MP4", "an album inside an album");
  click(by("crumb", "0")); await tick();
  dbl(by("pick", "f:Best of the year")); await tick();
  ok(names().includes("GX010123.MP4"), "a clip in two albums is an item of each");
  click(by("crumb", "0")); await tick();
  dbl(by("pick", "f:Other folders")); await tick();
  ok(names().join("|") === "Generated", "Generated, as before, under Other folders");

  // Originals: a selection added to a new album
  click(tab("sources")); await tick(120);
  dbl(by("pick", "f:Egypt 2026")); await tick(); dbl(by("pick", "f:Egypt 2026/Day 1")); await tick();
  ok(!by("albumadd"), "nothing selected: no Add to an album… on the bar");
  const row = (i) => w.document.querySelectorAll('#main [data-a="pick"]')[i];
  click(row(1)); await tick(); click(row(2), { metaKey: true }); await tick();
  ok(!!w.document.querySelector('.selbar [data-a="albumadd"]'), "clips selected: Add to an album… on the bar");
  click(w.document.querySelector('.selbar [data-a="albumadd"]')); await tick(60);
  const sheet = () => $("sheet").textContent;
  ok(sheet().includes("Add 2 clips to an album") && sheet().includes("The originals stay where they are"), "the sheet says what it does");
  const offered = Array.from(w.document.querySelectorAll(".albumrow")).map((e) => e.dataset.v);
  ok(offered.join("|") === "Best of the year|Egypt|Egypt/Dives", "your albums offered, not Other folders: " + offered.join("|"));
  ok($("albumgo").disabled, "no album chosen: the button waits");
  $("albumname").value = "Red Sea"; $("albumname").dispatchEvent(new w.Event("input", { bubbles: true })); await tick();
  ok(!$("albumgo").disabled && $("albumgo").textContent === "Add to Red Sea", "a new album's name: " + $("albumgo").textContent);
  click($("albumgo")); await tick(200);
  const add = last("album_add")[1];
  ok(add.album === "_Clips/Red Sea" && add.clips.length === 2, "album_add into _Clips/Red Sea: " + add.clips.length + " clips");
  const c1 = add.clips[0], c2 = add.clips[1];
  ok(c1.tree === "Egypt 2026/Day 1/DJI_20261020091544_0002_D.MP4" && c1.path === "/Volumes/video/" + c1.tree && c1.size === 1.2e9, "each clip by its path in the tree, its file and size");
  ok(c1.curve === "dlogm" && c1.created === 1761139722 + 5400, "its curve and its time, as known already (the probe's answer): " + c1.curve + ", " + c1.created);
  ok(c2.curve === "dlog", "a curve chosen in its Source settings wins: " + c2.curve);
  ok($("sheet").innerHTML === "" && $("toastMsg").textContent.includes("2 clips added to Red Sea in "), "done: " + $("toastMsg").textContent);
  ok(!calls.some((c) => c[0] === "add_job"), "no batch in the player's queue");
  ok($("main").textContent.includes("In an album"), "Originals marks the clips that are in an album");
  // The same again: already there
  click(row(1)); await tick();
  click(w.document.querySelector('.selbar [data-a="albumadd"]')); await tick(60);
  ok(by("albumpick", "Red Sea").classList.contains("on"), "the album used last is chosen");
  click($("albumgo")); await tick(200);
  ok($("toastMsg").textContent.includes("1 was there already"), "a clip already in the album: " + $("toastMsg").textContent);

  // Albums: the new album, its items moved and removed
  click(tab("footage")); await tick(150);
  ok(names().includes("Red Sea"), "the new album is in Albums");
  dbl(by("pick", "f:Red Sea")); await tick();
  ok(names().length === 2, "with its two links");
  const item = w.document.querySelector('#main [data-a="pick"]');
  click(item); await tick();
  ok(!!w.document.querySelector('.selbar [data-a="albummove"]'), "an item selected: Move to an album…");
  click(w.document.querySelector('.selbar [data-a="albummove"]')); await tick(60);
  ok(sheet().includes("Move 1 item to an album"), "the sheet to move");
  click(by("albumpick", "Egypt/Dives")); await tick();
  click($("albumgo")); await tick(200);
  const mv = last("album_move")[1];
  ok(mv.album === "_Clips/Egypt/Dives" && mv.files.length === 1 && mv.files[0] === ROOT + "/_Clips/Red Sea/DJI_20261020091544_0002_D.afplink.mp4", "album_move: the link's file into _Clips/Egypt/Dives");
  ok(names().length === 1, "it left this album");
  // Removed from the album: the link's file to the Trash, never the original
  click(w.document.querySelector('#main [data-a="pick"]')); await tick();
  w.document.querySelector('#main [data-a="pick"]').dispatchEvent(new w.MouseEvent("contextmenu", { bubbles: true, clientX: 10, clientY: 10 })); await tick();
  const menu = w.document.querySelector(".cmenu");
  ok(menu && menu.textContent.includes("Remove from the album") && menu.textContent.includes("Add to another album…") && menu.textContent.includes("Move to an album…"), "the menu: Remove from the album, Add to another album…, Move to an album…");
  click(menu.querySelector('[data-a="trash"]')); await tick(200);
  const del = last("delete_items")[1].items;
  ok(del.length === 1 && del[0].kind === "preview" && del[0].path.endsWith("_Clips/Red Sea/DJI_20261020101201_0003_D.afplink.mp4"), "delete_items: the link, not the clip: " + del[0].path);
  ok($("toastMsg").textContent.includes("the originals are untouched"), "said: " + $("toastMsg").textContent);
  ok(state.clips.length === 8, "every original still there");

  // A new album; no batch of clip previews anywhere
  click(by("crumb", "0")); await tick();
  ok(by("newdir").getAttribute("aria-label") === "New album", "New album in Albums");
  click(by("newdir")); await tick(200);
  ok(last("new_folder")[1].view === "footage" && last("new_folder")[1].parent === "_Clips", "made in _Clips");
  click(by("tab", "moments")); await tick();
  const side = $("side").textContent;
  ok(!side.includes("Make clip previews") && !$("clips_layout") && side.includes("Moments of the marks") && side.includes("Photos of the marks"), "the side panel: no clip previews batch, moments and photos kept");
  ok(!!w.document.querySelector('#side [data-a="albumadd"]'), "Add to an album… in the side panel too");

  // An album holding more than one kind: the filter by kind (moments and photos are to join the clips)
  w.__T.ALBUM_SOURCES.push(() => [{ kind: "moment", key: "m:x", id: "_Clips/Egypt/Turtle.afplink.mp4", rdir: "_Clips/Egypt", name: "Turtle", path: "/p/Trips/_Clips/Egypt/Turtle.afplink.mp4", sub: "", tags: [] }]);
  w.__T.S.filtersOpen = true; w.__T.arrange(); w.__T.renderAll();
  await tick();
  ok($("fpanel").textContent.includes("Kind") && !!w.document.querySelector('[data-v="knd:moment"]') && !!w.document.querySelector('[data-v="knd:clip"]'), "two kinds in Albums: a filter by kind");
  click(tab("sources")); await tick(120);
  ok(!$("fpanel").textContent.includes("Kind"), "not in Originals");
  ok(errors.length === 0, "no script errors" + (errors.length ? ": " + errors.join(" | ") : ""));
  process.exit(process.exitCode || 0);
})();
