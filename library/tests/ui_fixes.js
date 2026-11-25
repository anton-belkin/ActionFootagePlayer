// The four fixes from the first real try (docs/archive/library-ui-fixes.md), in jsdom against the mock (mock.js + views_mock.js
// with twelve more places): what doesn't need a real layout. The pinned bar's place on the screen and a tick under
// the pointer are checked in a headless Chrome by ui_fixes_shots.js.
//   JSDOM=<path to node_modules/jsdom> node library/tests/ui_fixes.js library/ui
const path = require("path"), fs = require("fs");
const { JSDOM } = require(process.env.JSDOM);
const ui = process.argv[2];
const html = fs.readFileSync(path.join(ui, "index.html"), "utf8").replace(/<script src="[^"]+"><\/script>\s*/g, "");
const dom = new JSDOM(html, { runScripts: "outside-only", pretendToBeVisual: true, url: "http://localhost/" });
const w = dom.window;
w.CSS = { escape: (s) => String(s).replace(/[^a-zA-Z0-9_-]/g, "\\$&") };
w.Element.prototype.scrollIntoView = () => {};
const { calls } = require("./mock.js")(w);
require("./views_mock.js")(w, { big: 700, places: true });
const errors = [];
w.addEventListener("error", (e) => errors.push(String(e.error && e.error.stack || e.message)));
const scripts = Array.from(fs.readFileSync(path.join(ui, "index.html"), "utf8").matchAll(/<script src="([^"]+)">/g)).map((m) => m[1]);
w.eval(scripts.map((s) => fs.readFileSync(path.join(ui, s), "utf8").replace(/^"use strict";/, "")).join("\n;\n") +
  "\n;window.__T = { S, DATA, ALBUM_VIEWS, ctx, renderAll, avEverything, visible: () => VISIBLE };");
const T = w.__T, d = w.document;
const $ = (id) => d.getElementById(id);
const tick = (ms = 40) => new Promise((r) => setTimeout(r, ms));
const click = (el, o) => el.dispatchEvent(new w.MouseEvent("click", Object.assign({ bubbles: true, detail: 1 }, o || {})));
const by = (a, v) => d.querySelector('[data-a="' + a + '"]' + (v != null ? '[data-v="' + String(v).replace(/"/g, '\\"') + '"]' : ""));
const names = () => Array.from(d.querySelectorAll("#main .nm")).map((e) => e.textContent);
let fails = 0, n = 0;
const ok = (c, what) => { n++; if (c) console.log("ok: " + what); else { fails++; console.log("FAIL: " + what); } };
// The Filters panel's rows, in order: [id, count, greyed]
const frows = () => Array.from(d.querySelectorAll("#fpanel .opt")).map((o) => [o.dataset.o, +o.querySelector(".ct").textContent, o.classList.contains("zero")]);
const pinned = (where) => {
  const m = $("main"), bar = m.firstElementChild;
  ok(!!bar && bar.classList.contains("mbar") && !!bar.querySelector(".crumbs, .scope") && m.style.getPropertyValue("--mbar-h") !== "",
    where + ": the bars above the list are one pinned block, first in the scrolling box (" + (bar ? bar.className : "none") + ")");
};

(async () => {
  await tick(600);
  // ----- 1. The pinned bar, in every view -----
  click(by("view", "sources")); await tick(150);
  pinned("Originals");
  ok(!!$("main").querySelector(".mbar .selbar"), "Originals: the selection's bar is in it");
  click(by("layout", "list")); await tick(60);
  ok(!!$("main").querySelector(".mbar .head"), "Originals as a list: the list's head is in it");
  click(by("view", "footage")); await tick(200);
  pinned("Albums' top");
  click(by("aview", "timeline")); await tick(300);
  pinned("Timeline");
  ok(!!$("main").querySelector(".mbar .seg [data-a=tlevel]"), "Timeline: its Years / Months / Days are in it");
  click(by("crumb", "0")); await tick(80);
  click(by("aview", "map")); await tick(150);
  click(by("mapmode", "list")); await tick(80);
  click(d.querySelector(".placerow")); await tick(120);
  pinned("a Map place's list");
  click(by("crumb", "0")); await tick(80);
  T.S.path = ["Egypt"]; T.renderAll(); await tick(80);
  pinned("an album");
  click(by("crumb", "0")); await tick(80);

  // ----- 4. Albums' top: one list -----
  click(by("layout", "list")); await tick(60);
  const top = names();
  ok(top.join("|") === "My albums|Timeline|Map|Videos|Moments|Photos|Live Photos|Generated|Other folders", "Albums' top, one list, My albums first: " + top.join("|"));
  const heads = Array.from(d.querySelectorAll("#main .list > .lgroup")).map((e) => e.textContent);
  ok(heads.join("|") === "The whole library|Media types|Folders", "its groups: " + heads.join("|"));
  ok(d.querySelectorAll("#main .list > .row.avrow").length === 7 && !d.querySelector(".avtiles"), "My albums, Timeline, Map and the media types are rows of the list's own look, no tiles");
  ok(!d.querySelector("#main .mbar .head"), "no column head over rows that have no columns");
  ok(d.querySelector('[data-a="aview"][data-v="mine"] .sub').textContent.startsWith("3 albums"), "My albums says how many: " + d.querySelector('[data-a="aview"][data-v="mine"] .sub').textContent);
  // My albums: one level, the albums inside as they were; an album's path; Up; Back
  click(by("aview", "mine")); await tick(80);
  ok(names().join("|") === "Alps|Best of the year|Egypt", "My albums: the user's albums: " + names().join("|"));
  pinned("My albums");
  d.querySelector('#main [data-a="pick"][data-v="f:Egypt"]').dispatchEvent(new w.MouseEvent("dblclick", { bubbles: true })); await tick(80);
  const crumbs = Array.from(d.querySelectorAll(".crumbs button:not(.icon)")).map((b) => b.textContent).join(" › ");
  ok(crumbs === "Albums › My albums › Egypt" && T.S.path.join("/") === "Egypt", "an album: " + crumbs + ", its own path " + T.S.path.join("/"));
  click(by("up")); await tick(60);
  ok(T.S.path[0].slice(1) === "My albums" && T.S.sel["f:Egypt"], "Up: My albums, Egypt selected");
  click(by("back")); await tick(60);
  ok(T.S.path.join("/") === "Egypt", "Back: the album again");
  click(by("aview", "mine")); await tick(60);
  ok(T.S.path[0].slice(1) === "My albums", "the path's My albums opens it");
  click(by("crumb", "0")); await tick(60);
  ok(!names().includes("360° videos"), "360° videos: not offered when the library has none");
  click(by("layout", "grid")); await tick(60);
  ok(d.querySelectorAll("#main .grid > .card.avrow").length === 7 && d.querySelectorAll("#main .grid > .lgroup").length === 3, "…and cards of the grid's own look in the grid");
  const allClips = T.DATA.sources.items.length;
  ok(d.querySelector('[data-a="aview"][data-v="videos"] .sub').textContent === allClips + " clips", "Videos says how many: " + d.querySelector('[data-a="aview"][data-v="videos"] .sub').textContent);

  // A media-type album: every item of the kind, selection, the bars, the menu, culling, filters
  const scansBefore = calls.filter((c) => c[0] === "scan").length;
  click(by("aview", "videos")); await tick(300);
  ok(T.S.path.length === 1 && T.S.path[0].slice(1) === "Videos", "Videos opens as a place in Albums: " + Array.from(d.querySelectorAll(".crumbs button")).map((b) => b.textContent).join(" › "));
  ok(T.visible().length === allClips, "Videos lists every original: " + T.visible().length + " of " + allClips);
  pinned("the media-type album Videos");
  const newScans = calls.filter((c) => c[0] === "scan").slice(scansBefore).map((c) => c[1].view);
  ok(newScans.every((v) => ["footage", "moments", "stills"].includes(v)), "no scan of its own (what it read: " + (newScans.join(", ") || "nothing") + ")");
  const first = d.querySelector('#main [data-a="pick"]');
  click(first); await tick(60);
  const c = T.ctx();
  ok(c.selItems.length === 1 && c.selItems[0].kind === "clip", "an item is selected as in any album");
  ok(!!d.querySelector('.mbar .selbar [data-a="albumadd"]') && !!d.querySelector('.mbar .selbar [data-a="share"]'), "its bar has Add to an album… and Share…");
  ok(!d.querySelector('[data-a="albumout"]'), "…and no Take out of the album (it's no album of files)");
  d.querySelector('#main [data-a="pick"].sel').dispatchEvent(new w.MouseEvent("contextmenu", { bubbles: true, clientX: 10, clientY: 10 })); await tick(60);
  const menu = Array.from(d.querySelectorAll(".cmenu button, [role=menu] button")).map((b) => b.textContent);
  ok(menu.some((t) => /album/i.test(t)) && menu.some((t) => /Share/.test(t)), "the menu: " + menu.join(" | "));
  d.body.dispatchEvent(new w.KeyboardEvent("keydown", { key: "Escape", bubbles: true })); await tick(30);
  d.body.dispatchEvent(new w.KeyboardEvent("keydown", { key: "p", bubbles: true })); await tick(200);
  ok(calls.some((c) => c[0] === "set_pick" && c[1].pick === "accept"), "P accepts it (culling)");
  click(by("filters")); await tick(80);
  ok(!!d.querySelector('#fpanel [data-a="fsec"][data-v="src"]') && frows().some((r) => r[0].startsWith("cam:")), "the filters are the clips' (Camera…)");
  click(by("filters")); await tick(40);
  for (const [id, label, kind, nn] of [["moments", "Moments", "moment", 2], ["photos", "Photos", "still", 2], ["livephotos", "Live Photos", "still", 1]]) {
    click(by("crumb", "0")); await tick(60);
    click(by("aview", id)); await tick(200);
    const its = T.visible().map((k) => T.avEverything().find((x) => "i:" + (x.key || x.id) === k));
    ok(its.length === nn && its.every((x) => x && x.kind === kind), label + ": every " + kind + " of the library (" + its.length + ")");
  }
  click(by("crumb", "0")); await tick(60);

  // ----- 2 + 3. Filters: collapsed groups, holding still while ticking -----
  click(by("view", "sources")); await tick(150);
  click(by("filters")); await tick(80);
  if (by("fsec", "geo").getAttribute("aria-expanded") !== "true") click(by("fsec", "geo")); await tick(60);
  ok(!d.querySelector("#fpanel .opt.kid"), "Location opens with every nested list collapsed");
  const parents = Array.from(d.querySelectorAll('#fpanel .opt [data-a="fexp"]'));
  ok(parents.length > 0 && parents.every((b) => b.getAttribute("aria-expanded") === "false"), "…its groups say so (" + parents.length + " collapsed)");
  ok(frows().filter((r) => r[0].startsWith("gr:")).every((r) => r[1] > 0), "…with their counts on the collapsed rows");
  const gc = frows().filter((r) => r[0].startsWith("gc:"));
  ok(gc.length === 6 && !!by("fmore", "gc") && !!d.querySelector('.ffind[data-f="gc"]'), "a long list (Country) shows its first 6, \"" + (by("fmore", "gc") || {}).textContent + "\" and a box to find in it");
  ok(gc.every((r, i) => !i || r[1] <= gc[i - 1][1]), "…the largest first: " + gc.map((r) => r[0].slice(3) + " " + r[1]).join(", "));
  // Opened on purpose, kept while the window is open (closing and opening the panel, another view and back)
  const grp = "gl:Egypt|South Sinai";
  click(by("fmore", "gl")); await tick(40);
  click(by("fexp", grp)); await tick(40);
  const kids = frows().filter((r) => r[0].startsWith(grp + "|"));
  ok(kids.length === 2, "a group opened shows its values: " + kids.map((r) => r[0].split("|").pop()).join(", "));
  click(by("filters")); await tick(30); click(by("filters")); await tick(60);
  click(by("view", "footage")); await tick(100); click(by("view", "sources")); await tick(150);
  ok(by("fexp", grp) && by("fexp", grp).getAttribute("aria-expanded") === "true", "…and stays open after the panel is closed and the view changed");
  // A value ticked inside a collapsed group: the group says which
  click(by("fopt", kids[0][0])); await tick(80);
  click(by("fexp", grp)); await tick(40);
  const row = d.querySelector('.opt[data-o="' + grp + '"]');
  ok(row.querySelector("input").indeterminate && row.querySelector(".onv") && row.querySelector(".onv").textContent === kids[0][0].split("|").pop(), "a collapsed group with a ticked value: half-ticked, and names it (" + (row.querySelector(".onv") || {}).textContent + ")");
  click(by("fopt", kids[0][0]) || row.querySelector("input")); await tick(80);
  if (T.S.filters[kids[0][0]]) { T.S.filters = {}; T.renderAll(); await tick(60); }
  // A tick: the same rows, in the same order, counts changed in place, the ones gone to 0 greyed; untick: as before
  const before = frows();
  const target = before.find((r) => r[0].startsWith("gc:") && r[0] !== "gc:none");
  click(by("fopt", target[0])); await tick(120);
  const after = frows();
  ok(T.S.filters[target[0]] === true, "ticked: " + target[0]);
  ok(after.map((r) => r[0]).join() === before.map((r) => r[0]).join(), "after the tick: the same " + after.length + " rows in the same order");
  ok(after.some((r, i) => r[1] !== before[i][1]), "…their counts changed in place");
  ok(after.some((r) => r[2] && r[1] === 0 && !before.find((b) => b[0] === r[0])[2]), "…rows the tick left with nothing stay, greyed (" + after.filter((r) => r[2]).map((r) => r[0]).slice(0, 3).join(", ") + ")");
  click(by("fopt", target[0])); await tick(120);
  ok(!T.S.filters[target[0]] && JSON.stringify(frows()) === JSON.stringify(before), "unticked: the panel is as it was");
  // Reopened while ticked: laid out again (what has nothing to show is left out then)
  click(by("fopt", target[0])); await tick(100);
  click(by("filters")); await tick(30); click(by("filters")); await tick(80);
  ok(frows().length < after.length && frows().every((r) => r[1] > 0 || T.S.filters[r[0]]), "reopened: laid out afresh, rows with nothing to show left out (" + after.length + " → " + frows().length + ")");
  // Finding in a long list
  click(by("fopt", target[0])); await tick(60);
  const box = d.querySelector('.ffind[data-f="gc"]');
  box.focus(); box.value = "an"; box.dispatchEvent(new w.Event("input", { bubbles: true })); await tick(60);
  const found = frows().filter((r) => r[0].startsWith("gc:")).map((r) => r[0].slice(3));
  ok(found.length > 0 && found.every((x) => /an/i.test(x)) && d.activeElement && d.activeElement.classList.contains("ffind"), "the box finds by name, the focus stays in it: " + found.join(", "));

  ok(!errors.length, "no script errors" + (errors.length ? ": " + errors.join("\n") : ""));
  console.log(n + " checks, " + fails + " failed");
  process.exit(fails ? 1 : 0);
})();
