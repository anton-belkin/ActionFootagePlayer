// The four fixes from the first real try (docs/library-ui-fixes.md), in a headless Chrome against the mock (mock.js +
// views_mock.js with 700 more clips at 24 places): pictures, and the checks that need a real layout (jsdom has none):
//   - the bar above the list stays in place on a long list (Originals' search, an album, Timeline, a Map place's list),
//     and a Timeline day's header sticks below it, not under it;
//   - a filter ticked and unticked by two clicks at the same point of the screen (the row doesn't move);
//   - Filters' Location collapsed, a long list's "more", Albums' top as one list, a media-type album.
// By hand (puppeteer-core isn't a dependency of the project: any copy of it):
//   PUPPETEER=<path to node_modules/puppeteer-core> node library/tests/ui_fixes_shots.js <ui folder> <out folder> [dark]
// On an older page (a checkout's library/ui, for "before" pictures) a scene whose control isn't there is skipped and
// the checks say what fails there. CHROME: the browser (default: Google Chrome in /Applications). No window opens.
const path = require("path"), fs = require("fs");
const puppeteer = require(process.env.PUPPETEER);
const [ui, out, ...flags] = process.argv.slice(2);
const dark = flags.includes("dark");
let fails = 0;
const ok = (c, what) => { if (c) console.log("ok: " + what); else { fails++; console.log("FAIL: " + what); } };
(async () => {
  fs.mkdirSync(out, { recursive: true });
  const browser = await puppeteer.launch({ executablePath: process.env.CHROME || "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome", headless: true });
  const page = await browser.newPage();
  await page.setViewport({ width: 1320, height: 820, deviceScaleFactor: 2 });
  await page.emulateMediaFeatures([{ name: "prefers-color-scheme", value: dark ? "dark" : "light" }]);
  await page.evaluateOnNewDocument(fs.readFileSync(path.join(__dirname, "mock.js"), "utf8"));
  await page.evaluateOnNewDocument("window.__VIEWS_MOCK__ = { big: 700, places: true };");
  await page.evaluateOnNewDocument(fs.readFileSync(path.join(__dirname, "views_mock.js"), "utf8"));
  const errors = [];
  page.on("pageerror", (e) => { errors.push(e.message); console.log("page error: " + e.message); });
  // No map pictures needed here: the Places list
  await page.setRequestInterception(true);
  page.on("request", (r) => (r.url().includes("tile.openstreetmap.org") ? r.abort() : r.continue()));
  await page.goto("file://" + path.resolve(ui, "index.html"));
  const wait = (ms = 250) => new Promise((r) => setTimeout(r, ms));
  const click = async (sel) => { const el = await page.$(sel); if (!el) { console.log("not there: " + sel); return false; } await el.click(); await wait(); return true; };
  const dbl = async (sel) => { const done = await page.evaluate((q) => { const el = document.querySelector(q); if (el) el.dispatchEvent(new MouseEvent("dblclick", { bubbles: true })); return !!el; }, sel); await wait(); return done; };
  const shot = async (name) => { await wait(400); await page.screenshot({ path: path.join(out, name + (dark ? "-dark" : "") + ".png") }); console.log("picture: " + name); };
  const scrollMain = async (y) => { await page.evaluate((y) => { const m = document.getElementById("main"); m.scrollTop = y; m.dispatchEvent(new Event("scroll")); }, y); await wait(300); };
  // Where the bar is after a scroll: its selection bar (or the view's own bar) still in the window's top part
  const barSeen = () => page.evaluate(() => {
    const m = document.getElementById("main"), mr = m.getBoundingClientRect();
    const sb = m.querySelector(".selbar"), cr = m.querySelector(".crumbs, .scope");
    const r = (e) => e ? e.getBoundingClientRect() : null;
    const a = r(sb), b = r(cr);
    return { scrolled: m.scrollTop, bar: !!a && a.top >= mr.top - 1 && a.bottom <= mr.top + 160, crumbs: !!b && b.top >= mr.top - 1 && b.top <= mr.top + 60 };
  });
  await wait(1500);

  // 1. The pinned bar. Originals, a long search (every GX clip), as a list, scrolled far down
  await click('[data-a="view"][data-v="sources"]'); await click('[data-a="layout"][data-v="list"]');
  await page.click("#q"); await page.keyboard.type("GX0"); await wait(500);
  await page.evaluate(() => { document.activeElement.blur(); const s = document.getElementById("sugg"); if (s) s.innerHTML = ""; }); await wait(200);
  await scrollMain(4000);
  let b = await barSeen();
  ok(b.scrolled > 1000 && b.bar && b.crumbs, "Originals, a long list scrolled " + b.scrolled + " px: the bar above it stays in place");
  await shot("pinned-originals-search");
  await page.click("#q", { clickCount: 3 }); await page.keyboard.press("Backspace"); await wait(300);
  await click('[data-a="endsearch"]');

  // Timeline, Days, scrolled: the bar in place, and a day's header below it, not under it
  await click('[data-a="view"][data-v="footage"]'); await click('[data-a="layout"][data-v="grid"]');
  if (await click('[data-a="aview"][data-v="timeline"]')) {
    await click('[data-a="tlevel"][data-v="days"]'); await wait(500);
    for (const y of [2200, 2350, 5000]) {
      await scrollMain(y);
      b = await barSeen();
      const under = await page.evaluate(() => {
        const m = document.getElementById("main"), bar = m.querySelector(".mbar") || m.querySelector(".selbar");
        const edge = bar ? bar.getBoundingClientRect().bottom : m.getBoundingClientRect().top;
        // A header partly behind the bar: its top above the bar's bottom while its bottom is below the bar's top
        const top = bar ? bar.getBoundingClientRect().top : edge;
        return Array.from(m.querySelectorAll(".tlday header")).map((h) => h.getBoundingClientRect()).filter((r) => r.bottom > top + 1 && r.top < edge - 1).length;
      });
      ok(b.bar && b.crumbs, "Timeline scrolled to " + y + ": the bar stays in place");
      ok(under === 0, "Timeline scrolled to " + y + ": no day's header under the bar (" + under + ")");
    }
    await shot("pinned-timeline-days");
  }
  // A Map place's list, scrolled
  await click('[data-a="crumb"][data-v="0"]');
  if (await click('[data-a="aview"][data-v="map"]')) {
    await click('[data-a="mapmode"][data-v="list"]'); await wait(300);
    await click(".placerow"); await wait(500);
    await scrollMain(1500);
    b = await barSeen();
    ok(b.bar && b.crumbs, "a Map place's list scrolled: the bar stays in place");
    await shot("pinned-map-place");
  }

  // 4. Albums' top as one list (list and grid), a media-type album
  await click('[data-a="crumb"][data-v="0"]');
  await click('[data-a="layout"][data-v="list"]'); await scrollMain(0);
  await shot("albums-top-list");
  await click('[data-a="layout"][data-v="grid"]');
  await shot("albums-top-grid");
  // My albums, an album in it (the path: Albums › My albums › Egypt)
  if (await click('[data-a="aview"][data-v="mine"]')) {
    await shot("my-albums");
    await dbl('[data-a="pick"][data-v="f:Egypt"]');
    const crumbs = await page.evaluate(() => Array.from(document.querySelectorAll(".crumbs button:not(.icon)")).map((b) => b.textContent).join(" › "));
    ok(crumbs === "Albums › My albums › Egypt", "an album's path: " + crumbs);
    await shot("album-in-my-albums");
    await click('[data-a="crumb"][data-v="0"]');
  }
  if (await click('[data-a="aview"][data-v="videos"]')) {
    await scrollMain(3000);
    b = await barSeen();
    ok(b.bar && b.crumbs, "the media-type album Videos (" + (await page.evaluate(() => VISIBLE.length)) + " clips) scrolled: the bar stays in place");
    await shot("media-videos-scrolled");
    await click('[data-a="crumb"][data-v="0"]');
  }
  if (await click('[data-a="aview"][data-v="photos"]')) {
    await click('#main [data-a="pick"]');
    await shot("media-photos-selected");
    await click('[data-a="crumb"][data-v="0"]');
  }
  if (await click('[data-a="aview"][data-v="moments"]')) await shot("media-moments");
  await click('[data-a="crumb"][data-v="0"]');

  // 2. Filters in Originals: Location as it opens, a group opened, a long list's "more" and its box
  await click('[data-a="view"][data-v="sources"]'); await click('[data-a="layout"][data-v="grid"]');
  await click('[data-a="filters"]');
  await page.evaluate(() => { for (const id of ["src", "tf"]) { const b = document.querySelector('[data-a="fsec"][data-v="' + id + '"]'); if (b && b.getAttribute("aria-expanded") === "true") b.click(); } });
  await wait(200);
  await click('[data-a="fsec"][data-v="geo"]');
  await page.evaluate(() => { const s = document.querySelector('[data-a="fsec"][data-v="geo"]'); document.getElementById("fpanel").scrollTop = s.offsetTop - 60; });
  await shot("filters-location");
  const deep = await page.evaluate(() => document.querySelectorAll("#fpanel .opt.kid").length);
  ok(deep === 0, "Filters' Location opens with its nested lists collapsed (" + deep + " nested rows showing)");
  // (on the older page every group is open already, and there's no "more")
  await click('[data-a="fmore"][data-v="gr"]');
  if (await page.$('[data-a="fexp"][data-v="gr:Switzerland"][aria-expanded="false"]')) await click('[data-a="fexp"][data-v="gr:Switzerland"]');
  await page.evaluate(() => { const s = document.querySelector('[data-o="gr:Switzerland"]') || document.querySelector('input[data-v="gr:Switzerland"]'); if (s) document.getElementById("fpanel").scrollTop += s.getBoundingClientRect().top - 300; });
  await shot("filters-region-more-expanded");
  await click('[data-a="fmore"][data-v="gr"]');
  await page.evaluate(() => { const s = document.querySelector('[data-a="fsec"][data-v="geo"]'); document.getElementById("fpanel").scrollTop = s.offsetTop - 60; });
  if (await page.$('.ffind[data-f="gc"]')) {
    await page.focus('.ffind[data-f="gc"]'); await page.keyboard.type("an", { delay: 60 }); await wait(300);
    const r = await page.evaluate(() => ({ found: Array.from(document.querySelectorAll('#fpanel input[data-v^="gc:"]')).map((i) => i.dataset.v.slice(3)), typed: document.activeElement && document.activeElement.value }));
    const found = r.found;
    ok(found.length > 0 && found.every((n) => /an/i.test(n)) && r.typed === "an", "Country's box finds by name, the focus kept while typing (" + r.typed + "): " + found.join(", "));
    await shot("filters-country-find");
    await page.evaluate(() => { const b = document.querySelector('.ffind[data-f="gc"]'); b.value = ""; b.dispatchEvent(new Event("input", { bubbles: true })); }); await wait(300);
  }

  // 3. A tick doesn't move: tick and untick a row with two clicks at one point, with the panel scrolled
  // The first row of Country (the largest)
  const rowId = await page.evaluate(() => { const i = document.querySelector('#fpanel input[data-a="fopt"][data-v^="gc:"]'); return i ? i.dataset.v : ""; });
  const rowSel = '#fpanel input[data-a="fopt"][data-v="' + rowId + '"]';
  await page.evaluate((q) => { const i = document.querySelector(q); const p = document.getElementById("fpanel"); if (i) p.scrollTop += i.getBoundingClientRect().top - p.getBoundingClientRect().top - 260; }, rowSel);
  await wait(200);
  const at = await page.evaluate((q) => { const i = document.querySelector(q); if (!i) return null; const r = i.getBoundingClientRect(); return { x: r.left + r.width / 2, y: r.top + r.height / 2, rows: document.querySelectorAll("#fpanel .opt").length, st: document.getElementById("fpanel").scrollTop }; }, rowSel);
  if (!at) ok(false, "a row in Filters' Country");
  else {
    const under = () => page.evaluate((x, y) => { const e = document.elementFromPoint(x, y); const o = e && e.closest(".opt"); const i = o && o.querySelector('input[data-a="fopt"]'); return { id: i ? i.dataset.v : null, on: i ? i.checked : null, rows: document.querySelectorAll("#fpanel .opt").length, st: document.getElementById("fpanel").scrollTop }; }, at.x, at.y);
    await page.mouse.click(at.x, at.y); await wait(400);
    const one = await under();
    ok(one.id === rowId && one.on === true, "the first click ticks " + rowId + ", and its row is still under the pointer (" + one.id + ", " + one.on + ")");
    ok(one.rows === at.rows, "no row of the panel vanished or came with the tick (" + at.rows + " → " + one.rows + ")");
    ok(Math.abs(one.st - at.st) < 1, "the panel's scroll is kept (" + at.st + " → " + one.st + ")");
    await shot("filters-after-tick");
    await page.mouse.click(at.x, at.y); await wait(400);
    const two = await under();
    ok(two.id === rowId && two.on === false, "the second click at the same point unticks it (" + two.id + ", " + two.on + ")");
    // And a row of another list whose count the tick takes to 0 stays, greyed
    await page.mouse.click(at.x, at.y); await wait(400);
    const zero = await page.evaluate(() => Array.from(document.querySelectorAll("#fpanel .opt.zero input")).map((i) => i.dataset.v));
    ok(zero.length > 0, "rows the tick left with nothing stay, greyed: " + zero.slice(0, 4).join(", "));
    await page.mouse.click(at.x, at.y); await wait(300);
  }
  // A ticked value inside a collapsed group says so on the group's row (the first Region or Place group of two or more)
  for (const f of ["gr", "gl"]) if (await page.$('[data-a="fmore"][data-v="' + f + '"]') && (await page.evaluate((f) => document.querySelector('[data-a="fmore"][data-v="' + f + '"]').textContent !== "Fewer", f))) await click('[data-a="fmore"][data-v="' + f + '"]');
  const grp = await page.evaluate(() => {
    const tw = (g) => document.querySelector('#fpanel [data-a="fexp"][data-v="' + g + '"]');
    for (const g of Array.from(document.querySelectorAll('#fpanel [data-a="fexp"][data-v^="gr:"], #fpanel [data-a="fexp"][data-v^="gl:"]')).map((b) => b.dataset.v)) {
      if (tw(g).getAttribute("aria-expanded") !== "true") tw(g).click(); // (the panel is drawn again: found again)
      const kids = [];
      let n = tw(g).closest(".opt").nextElementSibling;
      while (n && n.classList.contains("kid")) { kids.push(n.querySelector("input").dataset.v); n = n.nextElementSibling; }
      if (kids.length > 1) return { g, kid: kids[0] };
      tw(g).click();
    }
    return null;
  });
  await wait(200);
  if (grp) {
    await click('#fpanel input[data-v="' + grp.kid + '"]');
    await click('[data-a="fexp"][data-v="' + grp.g + '"]');
    const said = await page.evaluate((g) => { const o = document.querySelector('.opt[data-o="' + g + '"] .onv'); const i = document.querySelector('#fpanel input[data-v="' + g + '"]'); return { text: o ? o.textContent : "", ind: !!(i && i.indeterminate) }; }, grp.g);
    ok(said.ind && said.text === grp.kid.split("|").pop(), "a collapsed group with a ticked value shows it: " + grp.g.slice(3) + " · " + said.text);
    await page.evaluate((g) => { const s = document.querySelector('.opt[data-o="' + g + '"]'); if (s) document.getElementById("fpanel").scrollTop += s.getBoundingClientRect().top - 300; }, grp.g);
    await shot("filters-group-ticked-collapsed");
  } else ok(false, "a Region or Place group of two or more, to tick one in");
  ok(!errors.length, "no page errors");
  await browser.close();
  console.log(fails ? fails + " failed" : "all passed");
  process.exitCode = fails ? 1 : 0;
})();
