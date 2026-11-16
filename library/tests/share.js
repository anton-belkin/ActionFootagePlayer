// The page's Share (library/ui/share.js) in jsdom, against the mock (mock.js): the command on a selection of any
// kinds, the sheet (what, where), each item's progress and failures, the Tasks row.
//   JSDOM=<path to node_modules/jsdom> node library/tests/share.js library/ui
const path = require("path"), fs = require("fs");
const { JSDOM } = require(process.env.JSDOM);
const ui = process.argv[2];
const html = fs.readFileSync(path.join(ui, "index.html"), "utf8").replace(/<script src="[^"]+"><\/script>\s*/g, "");
const dom = new JSDOM(html, { runScripts: "outside-only", pretendToBeVisual: true, url: "http://localhost/" });
const w = dom.window;
w.CSS = { escape: (s) => String(s).replace(/[^a-zA-Z0-9_-]/g, "\\$&") };
w.Element.prototype.scrollIntoView = () => {};
const { calls, state } = require("./mock.js")(w);
const errors = [];
w.addEventListener("error", (e) => errors.push(String(e.error && e.error.stack || e.message)));
const scripts = Array.from(fs.readFileSync(path.join(ui, "index.html"), "utf8").matchAll(/<script src="([^"]+)">/g)).map((m) => m[1]);
w.eval(scripts.map((s) => fs.readFileSync(path.join(ui, s), "utf8").replace(/^"use strict";/, "")).join("\n;\n") +
  "\n;window.__T = { S, renderAll, poll, SHARE_DESTS, openSettings };");

const $ = (id) => w.document.getElementById(id);
const tick = (ms = 40) => new Promise((r) => setTimeout(r, ms));
const click = (el, o) => el.dispatchEvent(new w.MouseEvent("click", Object.assign({ bubbles: true, detail: 1 }, o || {})));
const dbl = (el) => el.dispatchEvent(new w.MouseEvent("dblclick", { bubbles: true }));
const by = (a, v) => w.document.querySelector('[data-a="' + a + '"]' + (v != null ? '[data-v="' + v.replace(/"/g, '\\"') + '"]' : ""));
const ok = (cond, what) => { if (!cond) { console.log("FAIL: " + what); process.exitCode = 1; } else console.log("ok: " + what); };
const last = (cmd) => calls.filter((c) => c[0] === cmd).pop();
const tab = (v) => by("view", v);
const rowOf = (name) => Array.from(w.document.querySelectorAll('#main [data-a="pick"]')).find((e) => e.querySelector(".nm").textContent === name);
const pick = async (name, o) => { click(rowOf(name), o); await tick(); };
const sheet = () => $("sheet").textContent;

(async () => {
  await tick(500);
  const T = w.__T;
  ok(T.SHARE_DESTS.length === 1 && T.SHARE_DESTS[0].id === "folder", "one destination for now: a folder");
  ok(!w.document.querySelector('[data-a="tab"][data-v="export"]') && !!w.document.querySelector('[data-a="tab"][data-v="share"]'), "the side panel has Share, not Export");

  // Albums: a mixed selection (a moment rendered already, a photo link, a clip, a rendered photo's neighbours)
  click(tab("footage")); await tick(120);
  ok(!by("share"), "nothing selected: no Share… on the bar");
  click(by("aview", "mine")); await tick();
  dbl(by("pick", "f:Best of the year")); await tick();
  await pick("Turtle");
  ok(!!w.document.querySelector('.selbar [data-a="share"]'), "a moment selected: Share… on the bar");
  click(by("tab", "share")); await tick();
  const side = () => $("side").textContent;
  ok(side().includes("1 moment") && side().includes("rendered already") && !side().includes("Export") && !side().includes("_Exports"), "the Share part: what's selected and what the cache has; no Export, no _Exports");
  ok(!w.document.querySelector('#side [data-a="job"][data-v="render"]') && !w.document.querySelector('#side [data-a="job"][data-v="export"]'), "no Render or Export job offered");
  ok(!by("delrenders", "all").disabled && by("delrenders", "stale").disabled, "its render can be deleted; none is stale");
  await pick("GX010123_14.00s.afplink.jpg", { metaKey: true }); await pick("GX010123.MP4", { metaKey: true });
  ok(side().includes("3 items") && side().includes("1 photo") && side().includes("1 clip"), "a mixed selection: each kind counted");
  // The context menu has it too
  rowOf("Turtle").dispatchEvent(new w.MouseEvent("contextmenu", { bubbles: true, clientX: 10, clientY: 10 })); await tick();
  ok(!!w.document.querySelector('.cmenu [data-a="share"]'), "Share… in the context menu");
  click(w.document.querySelector('.cmenu [data-a="share"]')); await tick(60);
  ok(sheet().includes("Share 3 items") && sheet().includes("The originals stay as they are"), "the sheet: what's shared");
  ok(sheet().includes("Save to a folder") && w.document.querySelectorAll('#sheet [data-a="sharego"]').length === 1, "…and where: Save to a folder, the one destination");
  ok(!!$("sharelook") && $("sharelook").value === "HDR", "videos among them: their look, HDR first");
  ok(sheet().includes("1 moment") && sheet().includes("rendered already"), "what the cache has is said before anything is made");
  // In SDR the moment has no render
  $("sharelook").value = "SDR"; $("sharelook").dispatchEvent(new w.Event("change", { bubbles: true })); await tick();
  ok($("sharelook").value === "SDR" && /1 moment\s*to render/.test(sheet().replace(/(\d+ moment)/, "$1 ")), "SDR is another render: to render");
  $("sharelook").value = "HDR"; $("sharelook").dispatchEvent(new w.Event("change", { bubbles: true })); await tick();
  // The folder's dialog cancelled: the sheet stays as it was, nothing queued
  state.shareFolder = null;
  click(by("sharego", "folder")); await tick(80);
  ok(sheet().includes("Share 3 items") && !by("sharego", "folder").disabled && !state.jobs.some((j) => j.kind === "share"), "the folder not chosen: back to the sheet, nothing started");
  // Chosen
  state.shareFolder = "/Users/me/Desktop/For Anna";
  click(by("sharego", "folder")); await tick(120);
  const a = last("share")[1];
  ok(a.dest === "folder" && a.look === "HDR" && a.folder === undefined, "share is asked for with the destination and the look; the folder is the system dialog's");
  ok(JSON.stringify(a.items.map((i) => i.kind + ":" + i.path)) === JSON.stringify(["clip:Egypt 2026/Day 2/GX010123.MP4", "moment:_Moments/Best of the year/Turtle.afplink.mp4", "photo:_Clips/Best of the year/GX010123_14.00s.afplink.jpg"])
    || a.items.length === 3 && a.items.some((i) => i.kind === "clip" && i.path === "Egypt 2026/Day 2/GX010123.MP4") && a.items.some((i) => i.kind === "moment" && i.path === "_Moments/Best of the year/Turtle.afplink.mp4") && a.items.some((i) => i.kind === "photo" && i.path === "_Clips/Best of the year/GX010123_14.00s.afplink.jpg"),
    "its items: the clip by its path in the tree, the moment and the photo link by theirs in the project: " + a.items.map((i) => i.kind + ":" + i.path).join(" | "));
  ok(sheet().includes("Saving to For Anna"), "the sheet follows: Saving to <the folder>");
  const rows = () => Array.from(w.document.querySelectorAll("#sheet .shareitem")).map((e) => e.querySelector(".nm").textContent + "=" + e.querySelector(".pill").textContent);
  ok(rows().length === 3 && rows().includes("Turtle=Saved") && rows().includes("GX010123_14.00s.afplink.jpg=Saved") && rows().includes("GX010123.MP4=Rendering"), "each item's state: " + rows().join(" | "));
  ok($("sharesum").textContent.includes("2 of 3 saved · rendering, 40%"), "how far it is: " + $("sharesum").textContent);
  ok(by("revealpath", "/Users/me/Desktop/For Anna") && sheet().includes("it goes on in Tasks"), "the folder can be shown; closing leaves it running");
  // The run ends, one failing
  state.shareEnd("GX010123.MP4"); await T.poll(); await tick();
  ok(sheet().includes("Saved to For Anna") && $("sharesum").textContent.includes("2 saved, 1 failed"), "done: " + $("sharesum").textContent);
  ok(rows().includes("GX010123.MP4=Failed") && w.document.querySelector("#sheet .shareitem.failed .why").textContent.includes("original"), "the failure is on its item, with why");
  click(by("closesheet")); await tick();
  ok(!sheet(), "the sheet closes");
  // Tasks: the share's row, its items
  click(by("tab", "queue")); await tick();
  const q = $("side").textContent;
  ok(q.includes("Share · 3 items to For Anna") && q.includes("2 saved") && q.includes("1 failed") && !!w.document.querySelector("#side details.sharejob[open]"), "Tasks has the share, its failed item open");

  // Rendered photos and exports are copied as they are
  click(by("crumb", "0")); await tick(); dbl(by("pick", "f:Generated")); await tick();
  const heic = Array.from(w.document.querySelectorAll("#main .nm")).map((e) => e.textContent).find((n) => /\.heic$/.test(n));
  if (heic) {
    await pick(heic); click(w.document.querySelector('.selbar [data-a="share"]')); await tick(60);
    ok(sheet().includes("copied as they are") && !$("sharelook"), "a rendered photo: copied as it is, no look to choose");
    click(by("sharego", "folder")); await tick(120);
    ok(last("share")[1].items[0].kind === "file" && /\.heic$/.test(last("share")[1].items[0].path) && sheet().includes("Saved to For Anna"), "…by its file; saved at once");
    click(by("closesheet")); await tick();
  } else ok(false, "a rendered photo in Generated");

  // Originals: clips, whole
  click(tab("sources")); await tick(120);
  dbl(by("pick", "f:Egypt 2026")); await tick(); dbl(by("pick", "f:Egypt 2026/Day 1")); await tick();
  const row = (i) => w.document.querySelectorAll('#main [data-a="pick"]')[i];
  click(row(0)); await tick(); click(row(1), { metaKey: true }); await tick();
  ok(!!w.document.querySelector('.selbar [data-a="share"]'), "Originals: Share… on clips");
  click(w.document.querySelector('.selbar [data-a="share"]')); await tick(60);
  ok(sheet().includes("Share 2 items") && sheet().includes("2 clips") && sheet().includes("whole"), "clips are shared whole");
  click(by("closesheet")); await tick();
  // The Renders filter knows photo links
  click(tab("footage")); await tick(120); click(by("aview", "mine")); await tick(); dbl(by("pick", "f:Best of the year")); await tick();
  T.S.filtersOpen = true; T.renderAll(); await tick();
  ok($("fpanel").textContent.includes("Renders"), "the Renders filter is there for an album with a photo link and a moment");
  // Project settings, General: the render cache's room and its Clear (never by itself)
  await T.openSettings(); await tick(60);
  click(by("stab", "gen")); await tick();
  ok(sheet().includes("_Renders") && sheet().includes("3 files · 52 MB") && sheet().includes("1 other file there is kept"), "settings: the render cache's room, what Clear leaves");
  ok(!!by("clearrenders") && !by("clearrenders").disabled, "…and Clear the render cache");
  click(by("clearrenders")); await tick(120);
  ok(!!last("clear_renders") && /Render cache cleared: 52 MB freed \(3 files\)/.test($("toast").textContent), "cleared: how much it freed (" + $("toast").textContent + ")");
  ok(by("clearrenders") && by("clearrenders").disabled && sheet().includes("0 files"), "…and the sheet shows it empty");
  click(by("closesheet")); await tick();
  ok(errors.length === 0, "no script errors" + (errors.length ? ": " + errors.join(" | ") : ""));
  process.exit(process.exitCode || 0);
})();
