// SPDX-License-Identifier: GPL-3.0-or-later
//
// "Open at start" on the page (atstart.js with app.js) in jsdom, over mock.js:
//   NODE_PATH=<jsdom's node_modules> node library/ui-tests/atstart_test.js
// The app started with no file opens the project marked, skipping the start screen; offline, it shows on the start
// screen with Connect; the tick in the project's menu and on the start screen's rows; the start screen from the menu.
const fs = require("fs"), path = require("path"), vm = require("vm");
const { JSDOM } = require("jsdom");
const ui = path.join(__dirname, "..", "ui");
let checks = 0, failures = 0;
function expect(what, got, want) {
  checks++;
  const a = JSON.stringify(got), b = JSON.stringify(want);
  if (a !== b) { failures++; console.log("FAILED " + what + ": got " + a + ", wanted " + b); }
}
const tick = (ms) => new Promise((r) => setTimeout(r, ms || 0));
const pages = [];
// The page started over the stand-in with these options
async function page(opts) {
  const html = fs.readFileSync(path.join(ui, "index.html"), "utf8").replace(/<script[^>]*><\/script>/g, "");
  const dom = new JSDOM(html, { url: "http://localhost/", runScripts: "outside-only", pretendToBeVisual: true });
  const w = dom.window;
  pages.push(w);
  w.Element.prototype.scrollIntoView = function () {};
  if (!w.CSS) w.CSS = { escape: (x) => String(x).replace(/["\\]/g, "\\$&") };
  w.MOCK_OPTS = opts;
  const ctx = dom.getInternalVMContext();
  for (const f of [path.join(__dirname, "mock.js"), path.join(ui, "culling.js"), path.join(ui, "atstart.js"), path.join(ui, "app.js")]) new vm.Script(fs.readFileSync(f, "utf8"), { filename: f }).runInContext(ctx);
  await tick(150);
  const doc = w.document;
  return { w, doc, M: w.MOCK, ev: (c) => vm.runInContext(c, ctx), q: (s) => doc.querySelector(s), qa: (s) => Array.from(doc.querySelectorAll(s)),
    click: async (el) => { el.dispatchEvent(new w.MouseEvent("click", { bubbles: true })); await tick(80); } };
}
const EGYPT = "/Volumes/Trips/Egypt 2026", NORWAY = "/Volumes/Trips/Norway 2025", GARDEN = "/Users/me/Movies/Garden";

(async () => {
  // None marked: the start screen, each project with its tick, none on
  let p = await page({ project: null });
  expect("none marked: the start screen", [p.q("#empty").hidden, p.ev("S.root")], [false, null]);
  expect("each row has its tick, off", p.qa('#recentEmpty [data-s="start"]').map((b) => b.getAttribute("aria-checked")), ["false", "false", "false"]);
  expect("no line about it", p.q("#startHint").hidden, true);
  // Ticked: kept by the app, shown on; another ticked: moved
  await p.click(p.qa('#recentEmpty [data-s="start"]')[2]);
  expect("ticked: asked of the app", p.M.calls.filter((c) => c[0] === "set_start_project").pop()[1].path, GARDEN);
  expect("…shown, only that one", p.qa('#recentEmpty [data-s="start"]').map((b) => b.getAttribute("aria-checked")), ["false", "false", "true"]);
  expect("…and said", /Garden opens when the app starts/.test(p.q("#startHint").textContent), true);
  await p.click(p.qa('#recentEmpty [data-s="start"]')[0]);
  expect("another ticked: the one", p.qa('#recentEmpty [data-s="start"]').map((b) => b.getAttribute("aria-checked")), ["true", "false", "false"]);
  await p.click(p.qa('#recentEmpty [data-s="start"]')[0]);
  expect("its tick again: taken off", [p.M.atStart, p.qa('#recentEmpty [data-s="start"][aria-checked="true"]').length, p.q("#startHint").hidden], [null, 0, true]);
  expect("the project wasn't opened by ticking", p.ev("S.root"), null);

  // Started with no file, one marked and reachable: opened at once, no start screen
  p = await page({ project: null, atStart: GARDEN });
  expect("at start: opened at once", [p.ev("S.root"), p.q("#empty").hidden, p.M.calls.some((c) => c[0] === "open_at_start")], [GARDEN, true, true]);
  // Its menu: the tick on, and the way to the start screen
  await p.click(p.q('#top [data-a="menu"]'));
  const item = () => p.q('#top .menu [data-s="start"]');
  expect("menu: Open at start, ticked", [item().getAttribute("aria-checked"), item().textContent], ["true", "Open at start"]);
  expect("menu: Start screen", !!p.q('#top .menu [data-s="screen"]'), true);
  await p.click(p.q('#top .menu [data-s="screen"]'));
  expect("Start screen: the project closed, the list shown", [p.ev("S.root"), p.q("#empty").hidden, p.qa("#recentEmpty .rrow").length], [null, false, 3]);
  expect("…the marked one ticked there", p.qa('#recentEmpty [data-s="start"]').map((b) => b.getAttribute("aria-checked")), ["false", "false", "true"]);
  expect("…and it isn't opened again by itself", (await tick(700), p.ev("S.root")), null);
  // Taken off there
  await p.click(p.qa('#recentEmpty [data-s="start"]')[2]);
  expect("the tick taken off on the start screen", p.M.atStart, null);

  // Opened with a file (a double click on another Project.afproject): that one, the marked one left alone
  p = await page({ project: EGYPT, atStart: GARDEN });
  expect("a file opened: that project", [p.ev("S.root"), p.M.calls.some((c) => c[0] === "open_at_start")], [EGYPT, false]);
  await p.click(p.q('#top [data-a="menu"]'));
  expect("its menu: Open at start, not ticked", p.q('#top .menu [data-s="start"]').getAttribute("aria-checked"), "false");
  expect("…the marked one named in the list", p.qa("#top .menu .rrow").filter((r) => r.querySelector(".rstart")).map((r) => r.querySelector(".nm").textContent.trim()), ["Garden"]);
  await p.click(p.q('#top .menu [data-s="start"]'));
  expect("ticked in the menu: this one now", p.M.atStart, EGYPT);
  await p.click(p.q('#top [data-a="menu"]'));
  expect("…shown ticked", p.q('#top .menu [data-s="start"]').getAttribute("aria-checked"), "true");

  // The marked one offline (its share not mounted): the start screen, the row Offline with Connect and its tick
  p = await page({ project: null, atStart: NORWAY });
  expect("offline at start: the start screen", [p.ev("S.root"), p.q("#empty").hidden], [null, false]);
  const row = p.qa("#recentEmpty .rrow")[1];
  expect("its row: Offline, Connect, ticked", [/Offline/.test(row.textContent), !!row.querySelector('[data-a="connectrecent"]'), row.querySelector('[data-s="start"]').getAttribute("aria-checked")], [true, true, "true"]);
  expect("…and said", /Norway 2025 opens when the app starts, but it's offline now/.test(p.q("#startHint").textContent), true);

  console.log(checks + " checks, " + failures + " failed");
  for (const w of pages) w.close();
  process.exit(failures ? 1 : 0);
})().catch((e) => { console.error(e); process.exit(1); });
