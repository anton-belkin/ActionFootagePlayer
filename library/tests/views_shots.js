// Screenshots of Timeline and Map (library/ui) against the mock (mock.js + views_mock.js) in a headless Chrome: no
// window opens. With `perf`, the timings of a synthetic library of 10 000 clips in the browser instead.
//   PUPPETEER=<path to node_modules/puppeteer-core> node library/tests/views_shots.js <ui folder> <out folder> [dark] [perf]
// The map's pictures come from tile.openstreetmap.org when the network allows; the offline scene blocks them.
const path = require("path"), fs = require("fs");
const puppeteer = require(process.env.PUPPETEER);
const [ui, out, ...flags] = process.argv.slice(2);
const dark = flags.includes("dark"), perf = flags.includes("perf");
(async () => {
  fs.mkdirSync(out, { recursive: true });
  const browser = await puppeteer.launch({ executablePath: process.env.CHROME || "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome", headless: true });
  const page = await browser.newPage();
  await page.setViewport({ width: 1320, height: 820, deviceScaleFactor: perf ? 1 : 2 });
  await page.emulateMediaFeatures([{ name: "prefers-color-scheme", value: dark ? "dark" : "light" }]);
  await page.evaluateOnNewDocument(fs.readFileSync(path.join(__dirname, "mock.js"), "utf8"));
  await page.evaluateOnNewDocument("window.__VIEWS_MOCK__ = { big: " + (perf ? 10000 : 700) + " };");
  await page.evaluateOnNewDocument(fs.readFileSync(path.join(__dirname, "views_mock.js"), "utf8"));
  page.on("pageerror", (e) => console.log("page error: " + e.message));
  let block = false, tiles = 0;
  await page.setRequestInterception(true);
  page.on("request", (r) => { if (r.url().includes("tile.openstreetmap.org")) { if (block) return r.abort(); tiles++; } r.continue(); });
  await page.goto("file://" + path.resolve(ui, "index.html"));
  const wait = (ms = 250) => new Promise((r) => setTimeout(r, ms));
  const click = async (sel) => { const el = await page.$(sel); if (!el) { console.log("not there: " + sel); return false; } await el.click(); await wait(); return true; };
  const shot = async (name) => { await wait(500); await page.screenshot({ path: path.join(out, name + (dark ? "-dark" : "") + ".png") }); console.log(name); };
  await wait(1500);
  await click('[data-a="view"][data-v="footage"]'); await click('[data-a="layout"][data-v="grid"]');
  if (perf) {
    await click('[data-a="aview"][data-v="timeline"]'); await wait(1500);
    const r = await page.evaluate(async () => {
      const t = (f) => { const a = performance.now(); f(); return +(performance.now() - a).toFixed(1); };
      const frame = () => new Promise((res) => requestAnimationFrame(() => requestAnimationFrame(res)));
      const main = document.getElementById("main"), o = { items: avEverything().length };
      o.model_ms = t(() => tlModel(avEverything()));
      o.days_renderMain_ms = Math.min(t(renderMain), t(renderMain), t(renderMain));
      main.getBoundingClientRect(); o.days_layout_ms = t(() => main.getBoundingClientRect() && document.body.offsetHeight);
      o.days_cards_in_page = main.querySelectorAll(".card").length; o.days_height_px = main.scrollHeight;
      // Scrolling the whole timeline in 200 steps: each step's fill, and the frames it took
      const steps = [], h = main.scrollHeight - main.clientHeight;
      const t0 = performance.now();
      for (let i = 1; i <= 200; i++) { main.scrollTop = h * i / 200; const a = performance.now(); tlFill(false); document.body.offsetHeight; steps.push(performance.now() - a); await frame(); }
      steps.sort((a, b) => a - b);
      o.scroll_fill_ms = { median: +steps[100].toFixed(1), p95: +steps[190].toFixed(1), max: +steps[199].toFixed(1) }; o.scroll_200_steps_ms = Math.round(performance.now() - t0);
      const k = VISIBLE[5000]; S.sel = {}; S.sel[k] = true;
      o.select_renderAll_ms = t(renderAll);
      TL.level = "months"; o.months_renderMain_ms = t(renderMain); TL.level = "years"; o.years_renderMain_ms = t(renderMain); TL.level = "days";
      o.clusters_z3_ms = t(() => mapClusters(avEverything(), 3)); o.clusters_z12_ms = t(() => mapClusters(avEverything(), 12));
      return o;
    });
    console.log(JSON.stringify(r, null, 1));
    await click('[data-a="crumb"][data-v="0"]'); await click('[data-a="aview"][data-v="map"]'); await wait(3000);
    const m = await page.evaluate(() => { const a = performance.now(); mapPins(); return { pins_ms: +(performance.now() - a).toFixed(1), pins: document.querySelectorAll(".mappin").length }; });
    console.log(JSON.stringify(m));
    await browser.close(); return;
  }
  await shot("albums-top");
  await click('[data-a="aview"][data-v="timeline"]'); await wait(800);
  await shot("timeline-days");
  if (!(await page.$('#side .pane[data-tab="info"]'))) await click('[data-a="tab"][data-v="info"]');
  await page.evaluate(() => { const b = TL.layout.blocks.find((x) => x.type === "trip"); document.getElementById("main").scrollTop = document.getElementById("tlbody").offsetTop + b.top - 8; }); await wait(300);
  const at = await page.evaluate(() => { for (const e of document.querySelectorAll("#main .card.tl")) { const r = e.getBoundingClientRect(); if (r.top > 260) return [r.left + 40, r.top + 40]; } });
  await page.mouse.click(at[0], at[1]); await wait();
  await shot("timeline-trip-selected");
  await click('[data-a="tlevel"][data-v="months"]'); await shot("timeline-months");
  await click('[data-a="tlevel"][data-v="years"]'); await shot("timeline-years");
  await click('[data-a="tlevel"][data-v="days"]');
  await click('[data-a="crumb"][data-v="0"]');
  await click('[data-a="aview"][data-v="map"]'); await wait(4000);
  console.log("tiles asked: " + tiles + ", loaded: " + await page.evaluate(() => MAPV.ok) + ", failed: " + await page.evaluate(() => MAPV.bad));
  await shot("map");
  await page.evaluate(() => { MAPV.map.setView([28.2, 34.4], 9, { animate: false }); }); await wait(3000);
  await shot("map-zoomed");
  await click(".mappin"); await shot("map-place");
  await click('[data-a="crumb"][data-v="1"]');
  await click('[data-a="mapmode"][data-v="list"]'); await shot("map-places-list");
  // Offline: the tiles blocked, the map asked for again
  block = true;
  await click('[data-a="mapmode"][data-v="map"]');
  await page.evaluate(() => { MAPV.map.setView([46, 7], 7, { animate: false }); }); await wait(4000);
  console.log("offline: " + await page.evaluate(() => MAPV.offline));
  await shot("map-offline");
  await browser.close();
})();
