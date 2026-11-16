// Screenshots of the Library's page (library/ui) against the mock (mock.js), in a headless Chrome: no window opens.
// By hand (puppeteer-core isn't a dependency of the project: any copy of it):
//   PUPPETEER=<path to node_modules/puppeteer-core> node library/tests/shots.js <ui folder> <out folder> [dark]
// CHROME: the browser (default: Google Chrome in /Applications). The same scenes work on an older page (for "before"
// pictures: a checkout of its library/ui), where a scene's control isn't there the scene is skipped.
const path = require("path"), fs = require("fs");
const puppeteer = require(process.env.PUPPETEER);
const [ui, out, scheme] = process.argv.slice(2);
(async () => {
  fs.mkdirSync(out, { recursive: true });
  const browser = await puppeteer.launch({ executablePath: process.env.CHROME || "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome", headless: true });
  const page = await browser.newPage();
  await page.setViewport({ width: 1320, height: 820, deviceScaleFactor: 2 });
  await page.emulateMediaFeatures([{ name: "prefers-color-scheme", value: scheme === "dark" ? "dark" : "light" }]);
  await page.evaluateOnNewDocument(fs.readFileSync(path.join(__dirname, "mock.js"), "utf8"));
  page.on("pageerror", (e) => console.log("page error: " + e.message));
  await page.goto("file://" + path.resolve(ui, "index.html"));
  const wait = (ms = 250) => new Promise((r) => setTimeout(r, ms));
  const click = async (sel, o) => { const el = await page.$(sel); if (!el) return false; await el.click(o); await wait(); return true; };
  const dbl = async (sel) => { const done = await page.evaluate((q) => { const el = document.querySelector(q); if (el) el.dispatchEvent(new MouseEvent("dblclick", { bubbles: true })); return !!el; }, sel); await wait(); return done; };
  const shot = async (name) => { await wait(400); await page.screenshot({ path: path.join(out, name + ".png") }); console.log(name); };
  await wait(900);
  // The view that was Footage, as a grid: its top, an album, the side panel's Moments part
  await click('[data-a="view"][data-v="footage"]'); await click('[data-a="layout"][data-v="grid"]');
  await shot("albums-top");
  if (await dbl('[data-a="pick"][data-v="f:Egypt"]')) {
    await click('#main [data-a="pick"]:not([data-v^="f:"])');
    await click('[data-a="tab"][data-v="info"]');
    await shot("album");
  }
  await click('[data-a="crumb"][data-v="0"]');
  if (!(await page.$('#side .pane[data-tab="moments"]'))) await click('[data-a="tab"][data-v="moments"]');
  await shot("side-moments");
  // Moments, photos and exports. Before (the tabs they had): each tab. Now: items of albums
  for (const v of ["moments", "stills", "exports"]) {
    if (!(await click('[data-a="view"][data-v="' + v + '"]'))) continue;
    await click('[data-a="layout"][data-v="grid"]');
    await dbl('[data-a="pick"][data-v^="f:"]');
    await shot("tab-" + v);
  }
  await click('[data-a="view"][data-v="footage"]'); await click('[data-a="layout"][data-v="grid"]'); await click('[data-a="crumb"][data-v="0"]');
  const pickName = (name, meta) => page.evaluate((n, meta) => { const el = Array.from(document.querySelectorAll('#main [data-a="pick"]')).find((e) => e.querySelector(".nm") && e.querySelector(".nm").textContent === n); if (el) el.dispatchEvent(new MouseEvent("click", { bubbles: true, detail: 1, metaKey: !!meta })); return !!el; }, name, meta);
  if (await dbl('[data-a="pick"][data-v="f:Best of the year"]')) {
    if (await pickName("Turtle")) {
      await wait();
      await click('[data-a="tab"][data-v="export"]');
      await shot("album-mixed-moment");
      await pickName("GX010123_14.00s.afplink.jpg", true); await pickName("GX010123.MP4", true); await wait();
      await click('[data-a="tab"][data-v="info"]');
      await shot("album-mixed-selected");
      if (await click('.selbar [data-a="albumadd"]')) { await shot("add-items-to-album"); await click('[data-a="closesheet"]'); }
      await click('[data-a="filters"]');
      await shot("album-kind-filter");
      await click('[data-a="filters"]');
    }
    await click('[data-a="crumb"][data-v="0"]');
    if (await dbl('[data-a="pick"][data-v="f:Generated"]')) await shot("generated");
    await click('[data-a="crumb"][data-v="0"]');
  }
  // Originals: a selection, and Add to an album…
  await click('[data-a="view"][data-v="sources"]'); await click('[data-a="layout"][data-v="list"]');
  await dbl('[data-a="pick"][data-v="f:Egypt 2026"]'); await dbl('[data-a="pick"][data-v="f:Egypt 2026/Day 2"]');
  await click('#main [data-a="pick"]');
  await page.keyboard.down("Shift"); await click('#main [data-a="pick"]:last-child'); await page.keyboard.up("Shift");
  await shot("originals-selected");
  if (await click('.selbar [data-a="albumadd"]')) await shot("add-to-album");
  await browser.close();
})();
