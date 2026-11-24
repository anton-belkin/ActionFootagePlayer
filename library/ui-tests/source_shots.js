"use strict";
// Pictures of Source settings (the side panel's list of the source's attributes, its filters), without a window:
// headless Chrome over ../ui with a mock of the app. For the PR that made it a list (docs/img/library-source/).
//
//   node source_shots.js <out folder> [--ui <another ui folder, e.g. the one before>] [--prefix before-]
//
// The project: Day 1 with three Action 6 clips in D-Log M (gyro, the lens in the file, lens types), a GoPro HERO7 clip
// measured from the picture (HyperSmooth: its gyro can't be used) with the underwater colour on, one stabilized in the
// camera with a lens from Gyroflow's database, and a HERO4 clip with no gyro and no ready lens.
// States: one (the HERO7 clip, a change waiting for Save), many (all seven), none (nothing selected), filters (the
// Source filters, the clips with no motion data shown and selected), stabilize (the Stabilize part, before it moved),
// filelens (a HERO12 clip with its lens in the file), aside (the same once its own gyro plays), estimate (an estimate that fits only loosely).

const fs = require("fs");
const os = require("os");
const path = require("path");
const { execFileSync } = require("child_process");

const ROOT = "/P";
function views(aside) {
  const mk = (dir, name, extra) => Object.assign({ id: dir + "/" + name, dir, name, path: "/F/" + dir + "/" + name, size: 4.2e9, mtime: 1730000000, kind: "clip", loc: "Archive",
    copies: [{ loc: "Archive", li: 0, rel: dir + "/" + name, path: "/F/" + dir + "/" + name, offline: false }], own_files: [], gyroflow: false,
    pdir: "_Clips/" + dir, preview: ROOT + "/_Clips/" + dir + "/" + name.replace(/\.MP4$/, ".afplink.mp4"), tags: [] }, extra || {});
  const own = (source) => ({ own: { marks: [], view: null, source, location: null }, own_files: [] });
  const items = [
    mk("Day 1", "DJI_0087.MP4"), mk("Day 1", "DJI_0088.MP4", own({ underwater: "yes" })), mk("Day 1", "DJI_0091.MP4"),
    mk("Day 1", "GX010977.MP4", own({ underwater: "yes", uw: true, uw_highlights: false, uw_strength: 0.7 })), mk("Day 1", "GX010981.MP4"), mk("Day 1", "GOPR1585.MP4"),
    mk("Day 1", "GX010020.MP4", own({ underwater: "yes" })),
  ];
  const probes = {};
  const djiLens = { camera: "DJI OsmoAction6", lens: "in the file", from: "file", types: [{ id: "wide", label: "Wide" }, { id: "normal", label: "Normal" }, { id: "linear", label: "Linear" }], recorded: "wide", mode: 6,
    mode_key: "DJI OsmoAction6 · wide · 3840x2160 · 50 fps" };
  const h7Key = "GoPro HERO7 Black · wide · hseis · 3840x2160 · 29.97 fps";
  for (const it of items) {
    let p;
    if (it.name.startsWith("DJI")) p = { camera: "DJI OsmoAction6", color: "dlogm", mode: "default", water: true, stabilized_in_camera: false, width: 3840, height: 2160, fps: 50, lens_info: djiLens };
    else if (it.name.startsWith("GOPR")) p = { camera: "GoPro HERO4 Black", color: "normal", mode: null, lens: "", water: false, stabilized_in_camera: null, width: 2704, height: 1520, fps: 29.97,
      lens_info: { camera: "GoPro HERO4 Black", lens: "", from: "", types: [], mode_key: "GoPro HERO4 Black · medium · 2704x1520 · 29.97 fps" } };
    else if (it.name === "GX010977.MP4") p = { camera: "GoPro HERO7 Black", color: "normal", mode: "default", motion: "optical", measured: { in_water: true }, water: false, stabilized_in_camera: "HyperSmooth (HS EIS)",
      width: 3840, height: 2160, fps: 29.97, lens_info: { camera: "GoPro HERO7 Black", lens: "GoPro HERO7 Black Wide HS EIS 4K 29.97 fps, assigned", from: "own", types: [], mode_key: h7Key } };
    // A HERO12 with HyperSmooth AutoBoost, its lens in the file, measured from the picture once (Maldives GX010020)
    else if (it.name === "GX010020.MP4") p = { camera: "GoPro HERO12 Black", color: "normal", mode: "default", water: false,
      ...(aside ? { measured_aside: true } : { motion: "optical", measured: { in_water: true } }),
      stabilized_in_camera: "HyperSmooth (HS AutoBoost)", width: 3840, height: 2160, fps: 29.97,
      lens_info: { camera: "GoPro HERO12 Black", lens: "in the file", from: "file", types: [], mode_key: "GoPro HERO12 Black · linear · hsautoboost · 3840x2160 · 29.97 fps" } };
    else p = { camera: "GoPro HERO7 Black", color: "normal", mode: null, lens: "GoPro HERO7 Black 4k Wide HS EIS", water: false, stabilized_in_camera: "HyperSmooth (HS EIS)", width: 3840, height: 2160, fps: 29.97,
      lens_info: { camera: "GoPro HERO7 Black", lens: "GoPro HERO7 Black 4k 16:9 Wide HS EIS 3840x2160 29.97fps", from: "database", types: [], mode_key: h7Key } };
    probes[it.path] = Object.assign(p, { geotag: 7, movie: { duration: it.name.startsWith("DJI") ? 62 : 48, created: 1730000000 }, marks: { times: [] } });
  }
  const footage = { root: ROOT, view: "footage", dirs: ["Day 1"], clips_dirs: ["_Clips", "_Clips/Day 1"],
    locations: [{ name: "Archive", at: "", path: "../F", dir: "/F", copies: false, local: false, dirs: ["Day 1"], offline: false, connect: false, names: [] }],
    items, probes: { version: 1, pending: 0, items: probes } };
  footage.links = items.map((it) => ({ path: it.preview, rel: it.preview.slice(ROOT.length + 1), clip: it.id, draft: false, size: 2100, mtime: 1730000050 }));
  const empty = (v) => ({ root: ROOT, view: v, dirs: [], items: [], probes: { version: 1, pending: 0, items: {} } });
  return { footage, moments: empty("moments"), stills: empty("stills"), exports: empty("exports") };
}
// What the app answers about lenses: the lens types (before: asked per clip), the mode's lenses to choose from
const LENS_TYPES = (p) => p.includes("/DJI_") ? { path: p, camera: "DJI OsmoAction6", types: [["wide", "Wide"], ["normal", "Normal"], ["linear", "Linear"]], type: null, recorded: "wide", mode: 6 } : { path: p, camera: "GoPro", types: [] };
const LENS_INFO = (p) => p.includes("/GX010020") ? { camera: "GoPro HERO12 Black", lens: "in the file", from: "file", mode_key: "GoPro HERO12 Black · linear · hsautoboost · 3840x2160 · 29.97 fps",
  candidates: [{ id: "db:m", label: "GoPro_HERO12 Black_Max_NO-EIS_1080p_16by9_1920x1080-23.98fps", readout: false }] } : p.includes("/GX") ? { camera: "GoPro HERO7 Black", lens: "GoPro HERO7 Black Wide HS EIS 4K 29.97 fps, assigned", from: "own", mode_key: "GoPro HERO7 Black · wide · hseis · 3840x2160 · 29.97 fps",
  candidates: [{ id: "local:a", label: "GoPro HERO7 Black Wide HS EIS 4K 29.97 fps", local: true, exact: true }, { id: "db:b", label: "GoPro HERO7 Black 4k 16:9 Wide HS EIS 3840x2160 29.97fps (official)", readout: true, exact: true },
               { id: "db:c", label: "GoPro HERO7 Black 4k 16:9 Wide 59.94fps", readout: false }] } : {};

const DAY = "S.path = ['Day 1']; S.q = ''; S.flat = false;";
const STATES = {
  one: DAY + "showTab('source'); renderAll(); sel(['Day 1/GX010977.MP4']); setTimeout(() => { if (typeof srcChanges === 'function') { S.srcDraft = { uw: 'bright' }; renderSide(); } }, 400);",
  many: DAY + "showTab('source'); renderAll(); sel(['Day 1/DJI_0087.MP4','Day 1/DJI_0088.MP4','Day 1/DJI_0091.MP4','Day 1/GX010977.MP4','Day 1/GX010981.MP4','Day 1/GOPR1585.MP4']);",
  none: DAY + "showTab('source'); renderAll();",
  filters: DAY + "S.filtersOpen = true; S.fsec = { src: true, tf: false }; showTab('source'); buildFacets(); S.filters = { 'mot:cam': true, 'mot:none': true, 'mot:n': true }; renderAll(); sel(['Day 1/GX010981.MP4','Day 1/GOPR1585.MP4']);",
  stabilize: DAY + "showTab('stabilize'); renderAll(); sel(['Day 1/GX010981.MP4','Day 1/GOPR1585.MP4']);",
  // The lens in the file (GX010020): no lens to choose, find or estimate for it
  filelens: DAY + "showTab('source'); renderAll(); sel(['Day 1/GX010020.MP4']);",
  // The same clip once its own gyro plays (with #201: a HERO12+ with HyperSmooth on): its .afp-motion left aside
  aside: DAY + "showTab('source'); renderAll(); sel(['Day 1/GX010020.MP4']);",
  // An estimate that fits only loosely (GX010020's before the fix): not reliable, not kept
  estimate: DAY + "window.SHOT_JOBS = [window.ESTIMATE]; S.jobs = window.SHOT_JOBS; showTab('source'); renderAll(); sel(['Day 1/GOPR1585.MP4']);",
};
// What Estimate lens found for GX010020 (its lensfind cache): started from a Max Lens Mod fisheye, fits loosely
const ESTIMATE = { id: 7, kind: "lens-estimate", title: "Estimate lens · GOPR1585.MP4", project: ROOT, project_name: "Red Sea 2025", state: "done", fraction: 1,
  results: [{ path: "/F/Day 1/GOPR1585.MP4", mode: "GoPro HERO4 Black · medium · 2704x1520 · 29.97 fps", tried: [], accepted: null, doubts: [],
    estimate: { n: "estimate", name: "Estimated from this clip (started from GoPro_HERO12 Black_Max_NO-EIS_1080p_16by9_1920x1080-23.98fps (scaled))", verdict: "loose", fits: false,
      refraction: 1.33, medium_sure: false, readout_ms: 9.48, readout_measured: true, moved: 12.9, limit: 21.3 } }] };

function page(ui, does, name) {
  const mock = `
    window.onerror = (m, f, l) => { if (!document.title.startsWith("ERR")) document.title = "ERR " + m + " " + f + ":" + l; };
    const VIEWS = ${JSON.stringify(views(name === "aside"))}, ROOT = "${ROOT}";
    window.ESTIMATE = ${JSON.stringify(ESTIMATE)};
    const LT = ${LENS_TYPES.toString()}, LI = ${LENS_INFO.toString()};
    const answers = {
      snapshot: () => ({ project: ROOT, jobs: window.SHOT_JOBS || [], close_asked: false, close_waits: false, macos: true, changes: 1, quiet_ms: 5000, catalog: "", missing: 0, moving: null }),
      project: () => ({ path: ROOT, name: "Red Sea 2025", legacy: false, clips_flat: false, clips_layout: "tree", moments_alone: false }),
      lens_info: (a) => LI(a.path || ""), lens_types: (a) => (a.paths || []).map(LT),
      engine: () => "", recent: () => [], recent_tags: () => [], catalog_care: () => ({}), catalog_look: () => null, cached_view: () => null, probes_since: () => ({ version: 1, items: {}, pending: 0 }), thumb: () => null, missing_links: () => [],
      scan: (a) => JSON.parse(JSON.stringify(VIEWS[a.view] || { root: ROOT, view: a.view, dirs: [], items: [] })),
    };
    window.__TAURI__ = { core: { invoke: (cmd, a) => new Promise((ok, no) => setTimeout(() => { try { const f = answers[cmd]; Promise.resolve(f ? f(a || {}) : null).then(ok, no); } catch (e) { no(e); } }, 0)) },
      event: { listen: () => Promise.resolve(() => {}) }, webview: { getCurrentWebview: () => ({ onDragDropEvent: () => {} }) } };`;
  const run = `
    const sel = (ids) => { S.sel = {}; for (const it of (DATA[S.view] || {}).items || []) if (ids.includes(it.id)) S.sel["i:" + (it.key || it.id)] = true; renderMain(); renderSide(); };
    const go = () => { if (!(S.root && DATA.footage && DATA.moments)) return setTimeout(go, 20); ${does} };
    setTimeout(go, 50);`;
  let html = fs.readFileSync(path.join(ui, "index.html"), "utf8");
  html = html.replace("<head>", '<head><base href="file://' + encodeURI(ui) + '/">');
  html = html.replace(/<script src=/, () => "<script>(() => {" + mock + "})();</script>\n<script src=");
  return html.replace("</body>", "<script>" + run + "</script></body>");
}

if (require.main === module) {
  const args = process.argv.slice(2);
  const opt = (k) => { const i = args.indexOf(k); return i >= 0 ? args.splice(i, 2)[1] : null; };
  const ui = path.resolve(opt("--ui") || path.join(__dirname, "..", "ui"));
  const prefix = opt("--prefix") || "";
  const [out, ...want] = args;
  fs.mkdirSync(out, { recursive: true });
  const chrome = process.env.CHROME || "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome";
  for (const name of want.length ? want : Object.keys(STATES)) {
    const dir = fs.mkdtempSync(path.join(os.tmpdir(), "afl-src-"));
    const file = path.join(dir, "p.html");
    fs.writeFileSync(file, page(ui, STATES[name], name));
    // Its own profile (the user's Chrome is left alone), no window; tall enough for the whole list
    const args2 = ["--headless=new", "--disable-gpu", "--hide-scrollbars", "--user-data-dir=" + path.join(dir, "profile"), "--allow-file-access-from-files",
      "--window-size=1320," + (name === "many" ? 1260 : 1140), "--virtual-time-budget=3000"];
    if (process.env.SHOT_DEBUG) { console.log(name, execFileSync(chrome, args2.concat(["--dump-dom", "file://" + file]), { stdio: ["ignore", "pipe", "ignore"] }).toString().match(/<title>[^<]*<\/title>/)[0]); continue; }
    execFileSync(chrome, args2.concat(["--screenshot=" + path.join(path.resolve(out), prefix + name + ".png"), "file://" + file]), { stdio: "ignore" });
    console.log(prefix + name);
  }
}
module.exports = { views };
