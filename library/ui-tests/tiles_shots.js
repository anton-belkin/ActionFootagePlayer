"use strict";
// Pictures of the Library's tiles (the grid of clips, moments and photos) and of the Tags part, without a window:
// headless Chrome over ../ui with a mock of the app. For the PR of the tile layout (docs/img/library-tiles/).
//
//   node tiles_shots.js <out folder> [--ui <another ui folder, e.g. main's>] [--prefix before-]
//
// The project (tilesViews): clips in an album of their own and in two, one clip turned by 90° in its own file
// (own.source.rotation), a 360° clip, moments, a photo link, a Live Photo, a portrait photo, tags, Accepted and stars.
// Each item's picture is drawn here with an arrow to the sky; the turned clip's was recorded with the camera on its
// side (the arrow points left), so whether its picture is shown turned is seen at once.

const fs = require("fs");
const os = require("os");
const path = require("path");
const { execFileSync } = require("child_process");

const ROOT = "/P";
const LONG = "Egypt 2025 — Dahab, the Blue Hole and the Canyon, second week";

function tilesViews() {
  const clip = (dir, name, extra) => Object.assign({ id: dir + "/" + name, dir, name, path: "/F/" + dir + "/" + name, size: 8, mtime: 1700000000, kind: "clip", loc: "Archive",
    copies: [{ loc: "Archive", li: 0, rel: dir + "/" + name, path: "/F/" + dir + "/" + name, offline: false }], own_files: [], gyroflow: false, tags: [],
    pdir: "_Clips/" + dir, preview: null }, extra || {});
  const footage = {
    root: ROOT, view: "footage", dirs: ["Day 1", "Day 2"], clips_dirs: ["_Clips", "_Clips/Red Sea dives", "_Clips/" + LONG, "_Clips/Wrecks"],
    locations: [{ name: "Archive", at: "", path: "../F", dir: "/F", copies: false, local: false, dirs: ["Day 1", "Day 2"], offline: false, connect: false, names: [] }],
    items: [
      clip("Day 1", "DJI_0001.MP4", { tags: ["reef", "turtle"], pick: "accept", rating: 4 }),
      clip("Day 1", "DJI_0002.MP4"),
      clip("Day 2", "DJI_0003.MP4", { own: { marks: [], view: null, source: { rotation: 90 }, location: null }, own_files: ["/F/Day 2/DJI_0003.afp.json"], rating: 2 }),
      clip("Day 2", "VID_0004.insv", { pick: "reject" }),
      clip("Day 2", "DJI_0005.MP4", { tags: ["wreck"] }),
      clip("Day 2", "DJI_0006.MP4"),
    ],
    probes: { version: 1, pending: 0, items: {} },
  };
  const albums = { "Day 1/DJI_0001.MP4": ["Red Sea dives", LONG], "Day 1/DJI_0002.MP4": ["Red Sea dives"], "Day 2/DJI_0003.MP4": ["Red Sea dives", "Wrecks"],
    "Day 2/VID_0004.insv": ["Red Sea dives"], "Day 2/DJI_0005.MP4": ["Red Sea dives", "Wrecks"], "Day 2/DJI_0006.MP4": [LONG] };
  footage.links = [];
  for (const it of footage.items) for (const a of albums[it.id]) {
    const rel = "_Clips/" + a + "/" + it.name.replace(/\.[^.]+$/, ".afplink.mp4");
    footage.links.push({ path: ROOT + "/" + rel, rel, clip: it.id, draft: false, size: 2100, mtime: 1700000050 });
  }
  for (const it of footage.items) footage.probes.items[it.path] = { camera: "DJI OsmoAction6", color: "dlogm", mode: "default", movie: { duration: 30 + it.name.length, created: 1735700000 + it.name.length * 600 }, marks: { times: [] },
    spherical: it.name.endsWith(".insv") || undefined };
  const moment = (dir, name, clipId, extra) => Object.assign({ id: "_Moments/" + dir + "/" + name + ".afplink.mp4", dir: "_Moments/" + dir, name, path: ROOT + "/_Moments/" + dir + "/" + name + ".afplink.mp4",
    size: 100, mtime: 1700000100, kind: "moment", tags: [], gyroflow: false, renders: {}, original: "/F/" + clipId, link: { v: 1, kind: "moment", clip: clipId, in: 1, out: 6, created: "2025-01-01T08:00:00Z" } }, extra || {});
  const moments = { root: ROOT, view: "moments", dirs: ["_Moments", "_Moments/Red Sea dives"], items: [moment("Red Sea dives", "Ray", "Day 1/DJI_0001.MP4", { tags: ["ray"] }), moment("Red Sea dives", "Turtle", "Day 1/DJI_0002.MP4")],
    probes: { version: 1, pending: 0, items: {} } };
  const still = (dir, name, clipId, extra) => Object.assign({ id: "_Stills/" + dir + "/" + name, dir: "_Stills/" + dir, name, path: ROOT + "/_Stills/" + dir + "/" + name, size: 50, mtime: 1700000200, kind: "still", tags: [],
    form: "Ultra HDR JPEG", link: { v: 1, kind: "photo", clip: clipId, t: 1.5, created: "2025-01-01T08:00:00Z" } }, extra || {});
  const stills = { root: ROOT, view: "stills", dirs: ["_Stills", "_Stills/Red Sea dives"], items: [
    still("Red Sea dives", "DJI_0003_1.5s.afplink.jpg", "Day 2/DJI_0003.MP4", { linked: true, form: "Photo link" }),
    still("Red Sea dives", "DJI_0001_2s.heic", "Day 1/DJI_0001.MP4", { form: "Live Photo", tags: ["turtle"], pick: "accept" }),
    still("Red Sea dives", "DJI_0005_9s.jpg", "Day 2/DJI_0005.MP4", { rating: 5 }),
  ], probes: { version: 1, pending: 0, items: {} } };
  const exports = { root: ROOT, view: "exports", dirs: [], items: [], probes: { version: 1, pending: 0, items: {} } };
  return { footage, moments, stills, exports };
}

// A picture: a colour of its own and an arrow pointing to the sky. `o.tall`: a portrait one; `o.side`: recorded with the
// camera on its side (the arrow points left: turned by 90° clockwise it points up)
function picture(seed, o) {
  o = o || {};
  let h = 0;
  for (const c of seed) h = (h * 31 + c.charCodeAt(0)) >>> 0;
  const hue = h % 360, w = o.tall ? 90 : 160, ht = o.tall ? 160 : 90;
  const arrow = `<path d="M80 16 l14 18h-8v40h-12v-40h-8z" fill="#fff" fill-opacity=".85"/>`;
  const svg = `<svg xmlns="http://www.w3.org/2000/svg" width="${w * 4}" height="${ht * 4}" viewBox="0 0 ${w} ${ht}"><defs><linearGradient id="g" x1="0" y1="0" x2="${o.side ? 1 : 0}" y2="${o.side ? 0 : 1}">` +
    `<stop offset="0" stop-color="hsl(${hue},55%,62%)"/><stop offset="1" stop-color="hsl(${(hue + 40) % 360},60%,28%)"/></linearGradient></defs><rect width="${w}" height="${ht}" fill="url(#g)"/>` +
    `<g transform="translate(${w / 2 - 80} ${ht / 2 - 45})${o.side ? " rotate(-90 80 45)" : ""}">${arrow}</g></svg>`;
  return "data:image/svg+xml;base64," + Buffer.from(svg).toString("base64");
}

const STATES = {
  // An album holding every kind: clips (one turned, one 360°, in one or two albums), moments, a photo link, a Live Photo, a portrait photo
  album: "S.layout = 'grid'; S.view = 'footage'; S.path = ['Red Sea dives']; renderAll();",
  // Originals as tiles: the clips with every album they're in
  originals: "S.layout = 'grid'; S.view = 'sources'; S.path = ['Day 2']; renderAll();",
  // Timeline's tiles
  timeline: "S.layout = 'grid'; albumViewGo('timeline');",
  // The Tags part with the tags in use, the side panel tall
  tags: "S.layout = 'grid'; S.view = 'footage'; S.path = ['Red Sea dives']; showTab('tags'); renderAll(); sel(['a:_Clips/Red Sea dives/DJI_0001.afplink.mp4']);",
};

function html(ui, state, extraTags) {
  const views = tilesViews();
  if (extraTags) views.footage.items[1].tags = Array.from({ length: extraTags }, (_, i) => ["reef", "turtle", "wreck", "ray", "night dive", "shark", "Blue Hole", "Canyon", "coral", "school of fish", "drift", "safety stop"][i % 12] + (i >= 12 ? " " + i : ""));
  const pics = {};
  const side = (id) => ({ side: id === "Day 2/DJI_0003.MP4" });
  for (const it of views.footage.items) pics[it.path] = picture(it.id, side(it.id));
  for (const l of views.footage.links) pics[l.path] = picture(l.clip, side(l.clip));
  for (const it of views.moments.items) pics[it.path] = picture(it.id);
  for (const it of views.stills.items) pics[it.path] = picture(it.id, { tall: it.name.endsWith("9s.jpg") });
  // the photo link's picture is its clip's frame, as recorded (landscape)
  pics[views.stills.items[0].path] = picture("Day 2/DJI_0003.MP4", { side: true });
  const mock = `
    window.onerror = (m, f, l) => { if (!document.title.startsWith("ERR")) document.title = "ERR " + m + " " + f + ":" + l; };
    const VIEWS = ${JSON.stringify(views)}, ROOT = "${ROOT}", PICS = ${JSON.stringify(pics)};
    try { localStorage.clear(); } catch (_) {}
    const answers = {
      snapshot: () => ({ project: ROOT, jobs: [], close_asked: false, close_waits: false, macos: true, changes: 1, quiet_ms: 5000, catalog: "", missing: 0, moving: null }),
      project: () => ({ path: ROOT, name: "Red Sea 2025", legacy: false, clips_flat: false, clips_layout: "tree", moments_alone: false }),
      engine: () => "", recent: () => [], recent_tags: () => [], catalog_care: () => ({}), catalog_look: () => null, cached_view: () => null, probes_since: () => ({ version: 1, items: {}, pending: 0 }),
      thumb: (a) => PICS[a.path] || null, missing_links: () => [], places_of: (a) => (a.points || []).map(() => null), mini_map: () => null, lens_types: () => [], lens_info: () => ({}),
      scan: (a) => JSON.parse(JSON.stringify(VIEWS[a.view] || { root: ROOT, view: a.view, dirs: [], items: [] })),
    };
    window.__TAURI__ = { core: { invoke: (cmd, a) => new Promise((ok, no) => setTimeout(() => { try { const f = answers[cmd]; Promise.resolve(f ? f(a || {}) : null).then(ok, no); } catch (e) { no(e); } }, 0)) },
      event: { listen: () => Promise.resolve(() => {}) }, webview: { getCurrentWebview: () => ({ onDragDropEvent: () => {} }) } };`;
  const does = `
    const sel = (keys) => { S.sel = {}; for (const k of keys) S.sel[k] = true; renderMain(); renderSide(); };
    const go = () => { if (!(S.root && DATA.footage && DATA.moments && DATA.stills)) { if (S.root && !DATA.stills) needViews(["moments", "stills"]).catch(() => {}); return setTimeout(go, 20); } ${STATES[state]}
      setTimeout(() => { const t = document.querySelector(".culltags"); const p = document.querySelector(".pane"); if (t) document.title = "TAGS " + t.scrollHeight + "/" + t.clientHeight + " PANE " + p.scrollHeight + "/" + p.clientHeight; }, 400); };
    setTimeout(go, 50);`;
  let page = fs.readFileSync(path.join(ui, "index.html"), "utf8");
  page = page.replace("<head>", '<head><base href="file://' + encodeURI(ui) + '/">');
  page = page.replace(/<script src=/, () => "<script>" + require("./lists").script() + "(() => {" + mock + "})();</script>\n<script src=");
  return page.replace("</body>", "<script>" + does + "</script></body>");
}

function shoot(ui, state, out, o) {
  o = o || {};
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), "afl-tiles-"));
  const file = path.join(dir, "shot.html");
  fs.writeFileSync(file, html(ui, state, o.tags));
  const chrome = process.env.CHROME || "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome";
  const args = ["--headless=new", "--disable-gpu", "--user-data-dir=" + path.join(dir, "profile"), "--allow-file-access-from-files",
    "--window-size=" + (o.size || "1320,860"), "--virtual-time-budget=4000"].concat(o.scrollbars ? [] : ["--hide-scrollbars"]);
  if (o.dom) return execFileSync(chrome, args.concat(["--dump-dom", "file://" + file]), { stdio: ["ignore", "pipe", "ignore"] }).toString();
  // (headless Chrome now and then quits at start: once more)
  for (let i = 0; ; i++) {
    try { execFileSync(chrome, args.concat(["--screenshot=" + path.resolve(out), "file://" + file]), { stdio: "ignore" }); return out; } catch (e) { if (i >= 2) throw e; }
  }
}

if (require.main === module) {
  const argv = process.argv.slice(2);
  const opt = (k, d) => { const i = argv.indexOf(k); return i >= 0 ? argv[i + 1] : d; };
  const outDir = path.resolve(argv[0]);
  const ui = path.resolve(opt("--ui", path.join(__dirname, "..", "ui")));
  const pre = opt("--prefix", "");
  fs.mkdirSync(outDir, { recursive: true });
  for (const s of ["album", "originals", "timeline"]) console.log(shoot(ui, s, path.join(outDir, pre + s + ".png")));
  // The Tags part: a few tags, in a tall window; the scroll measured in the title
  console.log(shoot(ui, "tags", path.join(outDir, pre + "tags.png"), { tags: 30, scrollbars: true, size: "1320,1000" }));
  const dom = shoot(ui, "tags", null, { tags: 30, scrollbars: true, size: "1320,1000", dom: true });
  console.log((dom.match(/<title>([^<]*)<\/title>/) || [])[1]);
}

module.exports = { tilesViews, html, shoot, LONG };
