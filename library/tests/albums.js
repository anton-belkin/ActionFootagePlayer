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
  "\n;window.__T = { ALBUM_SOURCES, S, arrange, renderAll, showClip, commitRename };");
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
  ok(scripts.join() === "albums.js,share.js,app.js", "the page loads albums.js and share.js before app.js");
  const tabs = Array.from(w.document.querySelectorAll('[data-a="view"]')).map((e) => e.textContent);
  ok(tabs[0] === "Originals" && tabs[1] === "Albums" && !tabs.includes("Footage"), "the tabs: " + tabs.join(", "));
  ok(tab("sources").classList.contains("on"), "it opens on Originals");

  // Albums: the user's albums at the top, the rest of the project's folder under Other folders
  click(tab("footage")); await tick(120);
  ok(tabs.length === 2, "two tabs: Moments, Photos and Exports are gone");
  ok(names().join("|") === "Best of the year|Egypt|Generated|Other folders", "Albums' top: your albums, Generated, Other folders: " + names().join("|"));
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
  ok(names().join("|") === "_Exports", "what's elsewhere in the project's folder (exports) under Other folders: " + names().join("|"));

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

  // ---------- moments and photos are items of albums ----------
  const T = w.__T;
  const go = async (...path) => { click(tab("footage")); await tick(80); click(by("crumb", "0") || tab("footage")); await tick(); for (let i = 0; i < path.length; i++) { dbl(by("pick", "f:" + path.slice(0, i + 1).join("/"))); await tick(); } };
  const rowOf = (name) => Array.from(w.document.querySelectorAll('#main [data-a="pick"]')).find((e) => e.querySelector(".nm").textContent === name);
  const pick = async (name, o) => { click(rowOf(name), o); await tick(); return name; };
  const menuOf = async (name) => { const el = rowOf(name); el.dispatchEvent(new w.MouseEvent("contextmenu", { bubbles: true, clientX: 10, clientY: 10 })); await tick(); return w.document.querySelector(".cmenu"); };
  const bar = (a) => w.document.querySelector('.selbar [data-a="' + a + '"]');
  await go("Best of the year");
  ok(["GX010123.MP4", "Turtle", "GX010123_14.00s.afplink.jpg"].every((n) => names().includes(n)), "an album holds clips, moments and photos together (_Clips, _Moments and _Stills as one): " + names().join("|"));
  T.S.filtersOpen = true; T.renderAll(); await tick();
  ok($("fpanel").textContent.includes("Kind") && ["clip", "moment", "still"].every((k) => !!w.document.querySelector('[data-v="knd:' + k + '"]')), "the filter by kind: clips, moments, photos");
  click(w.document.querySelector('[data-v="knd:moment"]')); await tick();
  ok(names().join("|") === "Turtle", "Kind: Moments shows the album's moments: " + names().join("|"));
  click(w.document.querySelector('[data-v="knd:moment"]')); await tick();
  ok($("fpanel").textContent.includes("Renders") && $("fpanel").textContent.includes("Photo"), "the moments' Renders and the photos' kind among the filters");

  // A moment: its menu, its side panel
  let el = await pick("Turtle");
  let m = await menuOf(el);
  ok(["Open in the player", "Go to the footage", "Rename", "Move to Trash", "Add to another album…", "Move to an album…", "Take out of the album", "Info"].every((t) => m.textContent.includes(t)), "a moment's menu: " + m.textContent.replace(/\s+/g, " ").slice(0, 200));
  click(w.document.body); await tick(); // closes the menu
  el = await pick("Turtle");
  click(by("tab", "share")); await tick();
  ok($("side").textContent.includes("On 1 moment") && $("side").textContent.includes("rendered already") && !!w.document.querySelector('#side [data-a="share"]'), "Share in the side panel, of the moment selected (tests/share.js): " + $("side").textContent.replace(/\s+/g, " ").slice(0, 120));
  let job;
  // A clip and a moment selected: the clip's jobs take the clip, the moments' the moment
  await go("Best of the year");
  await pick("Turtle"); await pick("GX010123.MP4", { metaKey: true });
  // (the Share part is open still: a click on its tab would close the panel)
  ok($("side").textContent.includes("On 2 items") && $("side").textContent.includes("1 moment") && $("side").textContent.includes("1 clip"), "a mixed selection is items, shared as such: " + $("side").textContent.replace(/\s+/g, " ").slice(0, 60));
  click(by("tab", "moments")); await tick();
  click(w.document.querySelector('#side [data-a="job"][data-v="moments"]')); await tick(60);
  job = last("add_job")[1];
  ok(job.kind === "moments" && job.only.join() === "Egypt 2026/Day 2/GX010123.MP4", "Moments of the marks takes the clip only: " + JSON.stringify(job.only));

  // Add to another album: the clip by a new link, the moment and the photo link by a copy
  await go("Best of the year");
  await pick("Turtle"); await pick("GX010123_14.00s.afplink.jpg", { metaKey: true }); await pick("GX010123.MP4", { metaKey: true });
  click(bar("albumadd")); await tick(60);
  ok(sheet().includes("Add 3 items to an album") && sheet().includes("a copy of its link"), "the sheet for the three kinds");
  click(by("albumpick", "Egypt")); await tick(); click($("albumgo")); await tick(200);
  const cp = last("album_copy")[1];
  ok(cp.album === "_Clips/Egypt" && cp.files.length === 2 && cp.files[0].endsWith("Turtle.afplink.mp4") && cp.files[1].endsWith("GX010123_14.00s.afplink.jpg"), "album_copy: the moment's and the photo's link files into _Clips/Egypt");
  ok(last("album_add")[1].album === "_Clips/Egypt" && last("album_add")[1].clips.length === 1, "album_add: the clip");
  ok($("toastMsg").textContent.includes("added to Egypt"), "said: " + $("toastMsg").textContent);
  await go("Egypt");
  ok(names().includes("Turtle") && names().includes("GX010123_14.00s.afplink.jpg"), "they're in Egypt too");
  // Moved: every kind by its file
  await go("Best of the year");
  await pick("Turtle"); await pick("GX010123_14.00s.afplink.jpg", { metaKey: true });
  click(bar("albummove")); await tick(60);
  ok(sheet().includes("Move 2 items to an album"), "the sheet to move a moment and a photo");
  click(by("albumpick", "Egypt/Dives")); await tick(); click($("albumgo")); await tick(200);
  const mv2 = last("album_move")[1];
  ok(mv2.album === "_Clips/Egypt/Dives" && mv2.files.join("|") === ROOT + "/_Moments/Best of the year/Turtle.afplink.mp4|" + ROOT + "/_Clips/Best of the year/GX010123_14.00s.afplink.jpg", "album_move: the moment and the photo into _Clips/Egypt/Dives");
  await go("Egypt", "Egypt/Dives".split("/")[1]);
  ok(names().includes("Turtle") && names().includes("GX010123_14.00s.afplink.jpg") && names().includes("GX010124.MP4"), "Dives holds the three kinds: " + names().join("|"));
  // Taken out: the clip's link goes, the moment and the photo go back to Generated
  await pick("Turtle"); await pick("GX010123_14.00s.afplink.jpg", { metaKey: true }); await pick("GX010124.MP4", { metaKey: true });
  ok(!!bar("albumout"), "Take out of the album on the bar");
  const nDel = calls.filter((c) => c[0] === "delete_items").length;
  click(bar("albumout")); await tick(250);
  const outs = calls.filter((c) => c[0] === "album_move").slice(-2).map((c) => c[1]);
  ok(outs[0].album === "_Moments/Generated" && outs[0].files[0].endsWith("Dives/Turtle.afplink.mp4") && outs[1].album === "_Stills/Generated" && outs[1].files[0].endsWith(".afplink.jpg"), "taken out: the moment to _Moments/Generated, the photo to _Stills/Generated");
  const d2 = last("delete_items")[1].items;
  ok(calls.filter((c) => c[0] === "delete_items").length === nDel + 1 && d2.length === 1 && d2[0].kind === "preview" && d2[0].path.endsWith("GX010124.afplink.mp4"), "…and the clip's link removed");
  ok(state.moments.length === 4 && state.clips.length === 8, "nothing deleted but the link");

  // Generated: what's in no album, beside the albums; a rendered photo is a photo there
  await go("Generated");
  ok(["Turtle", "GX010124_3.00s.heic", "GX010125_7.50s.MP.jpg", "2026-11-10 14-32", "2026-11-10 14-32 Clip previews"].every((n) => names().includes(n)), "Generated: _Moments/Generated, _Stills/Generated and the project's own as one: " + names().join("|"));
  el = await pick("GX010124_3.00s.heic");
  m = await menuOf(el);
  ok(m.textContent.includes("Open") && !m.textContent.includes("Open in the player") && m.textContent.includes("Move to an album…") && !m.textContent.includes("Add to another album…") && !m.textContent.includes("Take out"), "a rendered photo: opened, moved, renamed, not copied: " + m.textContent.replace(/\s+/g, " ").slice(0, 160));
  click(w.document.body); await tick();
  // Renaming a photo: by its kind
  await pick("GX010124_3.00s.heic");
  w.document.dispatchEvent(new w.KeyboardEvent("keydown", { key: "Enter", bubbles: true })); await tick();
  const inp = w.document.querySelector("#main input.rn, #main input");
  ok(!!inp && inp.value === "GX010124_3.00s", "a photo is renamed in place: " + (inp && inp.value));
  if (inp) { inp.value = "Ray"; inp.dispatchEvent(new w.KeyboardEvent("keydown", { key: "Enter", bubbles: true })); await tick(200); }
  const rn = last("rename_item");
  ok(rn && rn[1].view === "stills" && rn[1].kind === "still" && rn[1].name === "Ray", "rename_item as a photo: " + JSON.stringify(rn && rn[1]));
  // Everything of a kind: the kind's filter over every album
  await go();
  click(w.document.querySelector('[data-v="knd:still"]')); await tick();
  if (by("flat")) { click(by("flat")); await tick(); }
  ok(names().filter((n) => /\.(jpg|heic)$/.test(n)).length === 4, "Kind: Photos across the albums lists every photo: " + names().join("|"));
  click(by("endsearch") || by("crumb", "0")); await tick();
  T.S.filters = {}; T.renderAll(); await tick();
  // Exports: under Other folders, with their menu
  await go("Other folders", "_Exports", "Generated");
  ok(names().join("|") === "Turtle.mp4", "an export, where it was saved: " + names().join("|"));
  el = await pick("Turtle.mp4");
  m = await menuOf(el);
  ok(["Open in the player", "Go to the footage", "Rename", "Move to Trash"].every((t) => m.textContent.includes(t)) && !m.textContent.includes("to an album"), "an export’s menu: " + m.textContent.replace(/\s+/g, " ").slice(0, 120));
  click(w.document.body); await tick();

  // Photos of the marks: links, made by the Library at once, in the album the clips were selected in
  await go("Egypt");
  await pick("GX010123.MP4");
  click(by("tab", "moments")); await tick();
  ok(!!w.document.querySelector('#side [data-a="photolinks"]') && !!w.document.querySelector('#side [data-a="job"][data-v="photos"]'), "Photos of the marks (links), and rendered now (the player's batch)");
  const nJobs = calls.filter((c) => c[0] === "add_job").length;
  click(w.document.querySelector('#side [data-a="photolinks"]')); await tick(250);
  const pa = last("photo_add")[1];
  ok(pa.album === "_Clips/Egypt" && pa.photos.length === 2 && pa.photos[0].t === 3 && pa.photos[0].tree === "Egypt 2026/Day 2/GX010123.MP4" && pa.photos[0].size === 3.9e9, "photo_add: a photo a mark, into the album: " + JSON.stringify(pa.photos.map((x) => x.t)));
  ok(calls.filter((c) => c[0] === "add_job").length === nJobs && $("toastMsg").textContent.includes("2 photos made"), "no batch in the player: " + $("toastMsg").textContent);
  ok(names().includes("GX010123_3.00s.afplink.jpg") && names().includes("GX010123_12.00s.afplink.jpg"), "the photos are in the album");
  await pick("GX010123.MP4");
  click(w.document.querySelector('#side [data-a="photolinks"]')); await tick(250);
  ok($("toastMsg").textContent.includes("No mark without a photo"), "again: each mark has its photo: " + $("toastMsg").textContent);
  // A clip's own view: its marks, moments and photos; a photo of a mark
  T.showClip("Egypt 2026/Day 2/GX010125.MP4"); await tick(200);
  ok(names().some((n) => n.startsWith("Mark 1")) && names().includes("GX010125_7.50s.MP.jpg"), "a clip's view: its marks and its photos: " + names().join("|"));
  el = await pick(names().find((n) => n.startsWith("Mark 1")));
  m = await menuOf(el);
  ok(m.textContent.includes("Make a photo of the mark"), "a mark's menu: Make a photo of the mark");
  click(m.querySelector('[data-a="photolinks"]')); await tick(250);
  ok(last("photo_add")[1].album === "_Stills/Generated" && last("photo_add")[1].photos[0].t === 3, "…a link in Generated");
  // An album renamed: each of its folders on disk
  await go();
  await pick("Best of the year");
  const before = calls.filter((c) => c[0] === "rename_item").length;
  T.S.renaming = "f:Best of the year"; await T.commitRename("Best"); await tick(100);
  const rns = calls.filter((c) => c[0] === "rename_item").slice(before).map((c) => c[1].path);
  ok(rns.join("|") === ROOT + "/_Clips/Best of the year", "an album's folders renamed (here the one that's left, in _Clips): " + rns.join("|"));
  click(tab("sources")); await tick(120);
  ok(!$("fpanel").textContent.includes("Kind"), "not in Originals");
  ok(errors.length === 0, "no script errors" + (errors.length ? ": " + errors.join(" | ") : ""));
  process.exit(process.exitCode || 0);
})();
