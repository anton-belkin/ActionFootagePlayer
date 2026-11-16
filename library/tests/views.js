// The Library's page in jsdom against the mock (mock.js + views_mock.js): Timeline and Map, the views inside Albums.
//   JSDOM=<path to node_modules/jsdom> node library/tests/views.js library/ui [big]
// With `big` (a number, 10000): the timings of a synthetic library of that many clips, no checks of the small one.
const path = require("path"), fs = require("fs");
const { JSDOM } = require(process.env.JSDOM);
const ui = process.argv[2], big = +process.argv[3] || 0;
const html = fs.readFileSync(path.join(ui, "index.html"), "utf8").replace(/<script src="[^"]+"><\/script>\s*/g, "");
const dom = new JSDOM(html, { runScripts: "outside-only", pretendToBeVisual: true, url: "http://localhost/" });
const w = dom.window;
w.CSS = { escape: (s) => String(s).replace(/[^a-zA-Z0-9_-]/g, "\\$&") };
w.Element.prototype.scrollIntoView = () => {};
const { calls } = require("./mock.js")(w);
require("./views_mock.js")(w, { big });
const errors = [];
w.addEventListener("error", (e) => errors.push(String(e.error && e.error.stack || e.message)));
const scripts = Array.from(fs.readFileSync(path.join(ui, "index.html"), "utf8").matchAll(/<script src="([^"]+)">/g)).map((m) => m[1]);
w.eval(scripts.map((s) => fs.readFileSync(path.join(ui, s), "utf8").replace(/^"use strict";/, "")).join("\n;\n") +
  "\n;window.__T = { S, DATA, AV, TL, MAPV, ALBUM_VIEWS, ctx, renderAll, renderMain, avEverything, tlModel, tlBlocks, mapClusters, mapPlaces, mapSel, avShown, deletable, visible: () => VISIBLE };");
const T = w.__T;
const $ = (id) => w.document.getElementById(id);
const tick = (ms = 40) => new Promise((r) => setTimeout(r, ms));
const click = (el, o) => el.dispatchEvent(new w.MouseEvent("click", Object.assign({ bubbles: true, detail: 1 }, o || {})));
const dbl = (el) => el.dispatchEvent(new w.MouseEvent("dblclick", { bubbles: true }));
const by = (a, v) => w.document.querySelector('[data-a="' + a + '"]' + (v != null ? '[data-v="' + v.replace(/"/g, '\\"') + '"]' : ""));
const names = () => Array.from(w.document.querySelectorAll("#main .nm")).map((e) => e.textContent);
const text = () => $("main").textContent;
const ok = (cond, what) => { if (!cond) { console.log("FAIL: " + what); process.exitCode = 1; } else console.log("ok: " + what); };
const ms = (f) => { const t = process.hrtime.bigint(); f(); return Number(process.hrtime.bigint() - t) / 1e6; };

async function bigRun() {
  await tick(1500);
  click(by("view", "footage")); await tick(300);
  click(by("aview", "timeline")); await tick(600);
  const all = T.avEverything();
  console.log("items: " + all.length);
  let m;
  console.log("tlModel (sort, days, trips, months, years): " + ms(() => { m = T.tlModel(all); }).toFixed(1) + " ms; " + m.days.length + " days, " + m.trips.length + " trips, " + m.months.length + " months, " + m.undated.length + " without a date");
  console.log("tlBlocks: " + ms(() => T.tlBlocks(m, 6)).toFixed(2) + " ms");
  for (const level of ["days", "months", "years"]) {
    click(by("tlevel", level)); await tick(50);
    const t = [0, 0, 0].map(() => ms(() => T.renderMain()));
    console.log("Timeline, " + level + ": renderMain " + Math.min(...t).toFixed(1) + " ms, " + $("main").querySelectorAll(".card").length + " cards in the page (of " + m.n + " items), main's HTML " + Math.round($("main").innerHTML.length / 1024) + " KB");
  }
  click(by("tlevel", "days")); await tick(50);
  const one = T.visible()[5000];
  T.S.sel = {}; T.S.sel[one] = true;
  console.log("Timeline, days: a selection drawn (renderAll) " + ms(() => T.renderAll()).toFixed(1) + " ms");
  click(by("crumb", "0")); await tick(50);
  click(by("aview", "map")); await tick(100);
  console.log("mapClusters at zoom 3: " + ms(() => T.mapClusters(all, 3)).toFixed(1) + " ms (" + T.mapClusters(all, 3).length + " pins); at zoom 12: " + ms(() => T.mapClusters(all, 12)).toFixed(1) + " ms (" + T.mapClusters(all, 12).length + " pins)");
  click(by("mapmode", "list")); await tick(50);
  console.log("Map, places: renderMain " + ms(() => T.renderMain()).toFixed(1) + " ms, " + w.document.querySelectorAll(".placerow").length + " places");
  click(w.document.querySelector(".placerow")); await tick(50);
  console.log("Map, the fullest place: renderMain " + ms(() => T.renderMain()).toFixed(1) + " ms, " + $("main").querySelectorAll(".card").length + " cards of " + T.visible().length);
  if (errors.length) { console.log("FAIL: page errors: " + errors.join("\n")); process.exitCode = 1; }
}

(async () => {
  if (big) { await bigRun(); process.exit(process.exitCode || 0); }
  await tick(500);
  ok(scripts.filter((x) => ["albums.js", "albumviews.js", "timeline.js", "map.js", "app.js"].includes(x)).join() === "albums.js,albumviews.js,timeline.js,map.js,app.js" && scripts[scripts.length - 1] === "app.js", "the page's scripts: " + scripts.join(", "));
  click(by("view", "footage")); await tick(120);
  ok(T.ALBUM_VIEWS.map((v) => v[1]).join() === "Timeline,Map", "ALBUM_VIEWS: Timeline, Map");
  ok(!!by("aview", "timeline") && !!by("aview", "map"), "Albums' top has the two views above the albums");
  ok(names().includes("Egypt") && names().includes("Other folders"), "…and the albums as before");

  // ----- Timeline -----
  click(by("aview", "timeline")); await tick(300);
  ok(T.S.view === "footage" && T.S.path.length === 1 && T.S.path[0].slice(1) === "Timeline", "Timeline is a place inside Albums");
  ok(calls.some((c) => c[0] === "scan" && c[1].view === "moments") && calls.some((c) => c[0] === "scan" && c[1].view === "stills"), "moments and photos are read for it");
  const all = T.avEverything();
  ok(all.filter((x) => x.kind === "clip").length === 8 && all.filter((x) => x.kind === "moment").length === 2 && all.filter((x) => x.kind === "still").length === 2, "everything: 8 originals, 2 moments, 2 photos");
  const heads = () => Array.from(w.document.querySelectorAll(".tlday header b")).map((e) => e.textContent);
  ok(heads()[0].startsWith("20 Oct 2026 · Dahab"), "a day's header has its date and place: " + heads()[0]);
  ok(heads().some((h) => h === "14 Feb 2026 · Chamonix-Mont-Blanc"), "a point with no name isn't in a header that has a named place: " + heads().find((h) => h.startsWith("14 Feb")));
  ok(heads().some((h) => /Ras Muhammad|Sharm El Sheikh/.test(h)), "the other day's place: " + heads().join(" | "));
  const months = Array.from(w.document.querySelectorAll(".tlmonth h3")).map((e) => e.textContent);
  ok(months[0] === "October 2026" && months.includes("February 2026"), "months, the newest first: " + months.join(", "));
  const order = T.visible().map((k) => all.find((x) => "i:" + (x.key || x.id) === k)).filter((x) => x.when && x.kind === "clip" && x.name.startsWith("DJI_20261020")).map((x) => x.name.slice(12, 18));
  ok(order.join() === "083012,091544,101201", "a day's items in the order they were recorded");
  ok(text().includes("Moment") && Array.from(w.document.querySelectorAll("#main .card.tl")).length === 12, "moments and photos are among the tiles (12 tiles)");
  // The model: trips, and what has no date
  const mk = (day, place, n, o) => Array.from({ length: n || 1 }, (_, i) => Object.assign({ kind: "clip", id: day + "-" + i + (place || ""), name: "c", when: day ? new Date(day + "T10:0" + i + ":00") : null,
    geo: place ? { lat: 28.5, lon: 34.5, place: { locality: place, region: "South Sinai", country: "Egypt" } } : null }, o || {}));
  const m = T.tlModel([].concat(mk("2026-10-10", "Dahab", 3), mk("2026-10-11", "Dahab", 2), mk("2026-10-12", null, 1), mk("2026-10-13", "Sharm El Sheikh", 2), mk("2026-10-20", "Dahab", 1), mk("2026-03-01", "Dahab", 1), mk(null, null, 2)));
  ok(m.trips.length === 1 && m.trips[0].days.length === 4 && m.trips[0].items === 8, "days in a row at one wider place are one trip (a day with no place between them included): " + m.trips.map((t) => t.days.length + " days").join());
  ok(m.trips[0].places[0][0] === "Dahab" && m.trips[0].from.getDate() === 10 && m.trips[0].to.getDate() === 13, "the trip's places and dates");
  ok(m.days[0].key === "2026-10-20" && !m.days[0].trip && m.days[m.days.length - 1].key === "2026-03-01", "a day a week later isn't the trip's; days are the newest first");
  ok(m.undated.length === 2 && m.years.length === 1 && m.months.length === 2 && m.months[0].n === 9, "what has no date is apart; months and years counted");
  const bl = T.tlBlocks(m, 2).blocks;
  ok(bl[bl.length - 2].title === "Without a date" && bl[bl.length - 1].day.key === "none", "the group without a date is the last, and named");
  ok(bl.filter((b) => b.type === "trip").length === 1 && bl[bl.findIndex((b) => b.type === "trip") + 1].day.key === "2026-10-13", "a trip's line is above its newest day");
  ok(bl.every((b, i) => !i || b.top === bl[i - 1].top + bl[i - 1].h), "blocks are one under another");
  // Selection, the side panel, opening, the menu
  const tile = Array.from(w.document.querySelectorAll("#main .card.tl")).find((e) => e.textContent.includes("GX010123.MP4"));
  click(tile); await tick();
  let c = T.ctx();
  ok(c.selItems.length === 1 && c.selItems[0].kind === "clip" && c.only.join() === "Egypt 2026/Day 2/GX010123.MP4", "a click selects the clip, as its path in the tree: " + c.only.join());
  ok($("side").textContent.includes("GX010123.MP4"), "the side panel is about it");
  ok(T.deletable(c).length === 0, "Delete takes nothing of an original here (not its album link either)");
  dbl(w.document.querySelector('#main .card.tl[aria-pressed="true"]')); await tick();
  const opened = calls.filter((x) => x[0] === "open_items").pop();
  ok(opened && JSON.stringify(opened[1]).includes("/Volumes/video/Egypt 2026/Day 2/GX010123.MP4"), "a double click opens it in the player");
  const tiles = () => Array.from(w.document.querySelectorAll("#main .card.tl"));
  click(tiles()[0]); click(tiles()[3], { shiftKey: true }); await tick();
  ok(Object.keys(T.S.sel).length === 4, "shift-click selects the run between, across days");
  tiles()[0].dispatchEvent(new w.MouseEvent("contextmenu", { bubbles: true, clientX: 10, clientY: 10 })); await tick();
  const menu = w.document.querySelector(".cmenu");
  ok(menu && menu.textContent.includes("Open in the player") && menu.textContent.includes("Add to an album…"), "the context menu: " + (menu ? menu.textContent.slice(0, 60) : "none"));
  // (share.js: Share… on what's selected here, as in any album)
  ok(menu && menu.textContent.includes("Share…") && !!w.document.querySelector('#main [data-a="share"]'), "Share… in Timeline: the menu and the bar");
  click(w.document.body); await tick();
  // Months and Years
  click(by("tlevel", "months")); await tick();
  ok(names().join("|") === "October|February|October" && w.document.querySelectorAll(".tlyear h3").length === 2, "Months: a cover a month under its year: " + names().join("|"));
  click(by("tlevel", "years")); await tick();
  ok(names().join("|") === "2026|2025" && text().includes("Dahab"), "Years: a cover a year with its places: " + names().join("|"));
  click(by("tlgo", "y:2026")); await tick();
  ok(T.TL.level === "months", "a year opens its months");
  click(by("tlgo", "m:2026-02")); await tick();
  ok(T.TL.level === "days" && heads().length > 0, "a month opens its days");
  // The search narrows it
  T.S.q = "GX0101"; T.renderAll(); await tick();
  ok(w.document.querySelectorAll("#main .card.tl").length === 5 && text().includes("5 items of 12 match"), "the search narrows the timeline");
  T.S.q = ""; T.renderAll();

  // ----- Map -----
  click(by("crumb", "0")); await tick();
  ok(T.S.path.length === 0 && !!by("aview", "map"), "the breadcrumb goes back to Albums");
  click(by("aview", "map")); await tick(100);
  ok(text().includes("11 items at 4 places") && text().includes("1 item without a location"), "the map's line: " + (text().match(/\d+ items at \d+ places[^L]*/) || [""])[0]);
  ok(!!$("mapbox"), "the map's box is there (Leaflet itself isn't loaded in jsdom)");
  const cl3 = T.mapClusters(all, 3), cl12 = T.mapClusters(all, 12);
  ok(cl3.length === 2 && cl3.map((x) => x.items.length).sort().join() === "3,8", "far out: Egypt is one pin and the Alps another: " + cl3.map((x) => x.items.length).join());
  ok(cl12.length > cl3.length && cl12.reduce((n, x) => n + x.items.length, 0) === 11, "closer in the pins split, every item in one: " + cl12.map((x) => x.items.length).join());
  const egypt = cl3.find((x) => x.items.length === 8);
  ok(Math.abs(egypt.lat - 28.2) < 0.4 && Math.abs(egypt.lon - 34.4) < 0.3, "a pin is at the middle of its items");
  // A pin's click (the map's button) lists its items in the ordinary grid
  T.S.path = [T.S.path[0], egypt.id]; T.renderAll(); await tick();
  ok(w.document.querySelectorAll("#main .grid .card").length === 8 && text().includes("Dahab and 1 more place"), "a pin's items in the grid, named by its places");
  click(w.document.querySelector("#main .grid .card")); await tick();
  ok(T.ctx().selItems.length === 1, "an item there is selected as anywhere");
  click(by("crumb", "1")); await tick();
  // Offline: the places as a list
  T.MAPV.offline = true; T.renderMain(); await tick();
  const rows = Array.from(w.document.querySelectorAll(".placerow"));
  ok(rows.length === 4 && rows[0].textContent.includes("Dahab") && rows.some((r) => r.textContent.includes("46.1° N, 7.2° E")) && text().includes("can’t be reached"), "offline: a list of places with counts: " + rows.map((r) => r.querySelector(".nm").textContent + " " + r.querySelector(".cnt").textContent).join(" | "));
  click(rows[0]); await tick();
  ok(w.document.querySelectorAll("#main .grid .card").length === 4 && names().some((n) => n.includes("0m05s")), "a place lists its items (a moment is where its clip is)");
  click(by("crumb", "1")); await tick();
  click(by("mapplace", "none")); await tick();
  ok(names().join("|") === "GX010125.MP4", "what has no location is listed: " + names().join("|"));
  click(by("crumb", "1")); await tick();
  click(by("mapretry")); await tick();
  ok(!T.MAPV.offline && !!$("mapbox"), "Try the map again goes back to the map");
  ok(!errors.length, "no page errors" + (errors.length ? ": " + errors.join("\n") : ""));
  process.exit(process.exitCode || 0);
})();
