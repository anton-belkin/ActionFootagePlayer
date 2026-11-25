// Album actions shown at once (docs/design/library-responsiveness.md "Albums"), pictured in a headless Chrome against the mock
// (mock.js), the album commands slowed to 4 s as a NAS might take them: what the window shows 0.5 s after the click on
// Add, then after the command answered. On an older page (a checkout's library/ui) the same clicks give the "before".
//   PUPPETEER=<path to node_modules/puppeteer-core> node library/tests/albums_pending_shots.js <ui folder> <out folder> [dark]
const path = require("path"), fs = require("fs");
const puppeteer = require(process.env.PUPPETEER);
const [ui, out, ...flags] = process.argv.slice(2);
const dark = flags.includes("dark");
(async () => {
  fs.mkdirSync(out, { recursive: true });
  const browser = await puppeteer.launch({ executablePath: process.env.CHROME || "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome", headless: true });
  const page = await browser.newPage();
  await page.setViewport({ width: 1320, height: 820, deviceScaleFactor: 2 });
  await page.emulateMediaFeatures([{ name: "prefers-color-scheme", value: dark ? "dark" : "light" }]);
  await page.evaluateOnNewDocument(fs.readFileSync(path.join(__dirname, "mock.js"), "utf8"));
  // The album commands take 4 s
  await page.evaluateOnNewDocument(`(() => { const core = window.__TAURI__.core, inv = core.invoke;
    core.invoke = async (cmd, a) => { if (/^(album_|photo_add|rename_album)/.test(cmd)) await new Promise((r) => setTimeout(r, 4000)); return inv(cmd, a); }; })();`);
  page.on("pageerror", (e) => console.log("page error: " + e.message));
  await page.goto("file://" + path.resolve(ui, "index.html"));
  const wait = (ms = 250) => new Promise((r) => setTimeout(r, ms));
  const shot = async (name) => { await page.screenshot({ path: path.join(out, name + (dark ? "-dark" : "") + ".png") }); console.log("picture: " + name); };
  await wait(1500);
  // Originals, Day 1, two clips selected → Add to an album… → a new album, Red Sea → Add
  await page.evaluate(() => { S.view = "sources"; S.path = ["Egypt 2026", "Day 1"]; S.layout = "grid"; renderAll(); });
  await wait(300);
  await page.evaluate(() => { const keys = items().slice(0, 2).map((it) => keyOf(it)); S.sel = {}; for (const k of keys) S.sel[k] = true; renderAll(); });
  await page.evaluate(() => askAlbum("add")); await wait(300);
  await page.evaluate(() => albumTyped("Red Sea")); await wait(100);
  await page.click("#albumgo");
  await wait(500);
  await shot("album-add-0.5s-originals");
  // The album itself, 0.5 s later still: Albums › My albums › Red Sea
  await page.evaluate(() => { if (albumList().includes("Red Sea")) { S.view = "footage"; S.path = ["Red Sea"]; S.sel = {}; renderAll(); } });
  await wait(300);
  await shot("album-add-1s-album");
  await wait(4500);
  await page.evaluate(() => { if (albumList().includes("Red Sea")) { S.view = "footage"; S.path = ["Red Sea"]; S.sel = {}; renderAll(); } });
  await wait(500);
  await shot("album-add-5s-album");
  await browser.close();
})().catch((e) => { console.error(e); process.exit(1); });
