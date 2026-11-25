"use strict";
// Pictures of an album's tile around a rename (rename_thumbs_test.js), for the PR: headless Chrome over ../ui (or an
// older copy, --ui), the fake app (fakeapp.js) answering on page.js's small project, its `thumb` a picture only for a
// file that's on the fake disk, as the app's is. Each file's picture has its own colour, so the same file is the same
// picture under its old and new names.
//
//   node rename_shots.js <state> <out.png> [--ui <older ui folder>] [--dark]
//
// States: start (My albums, before anything), renaming (Day 1 renamed to Nile, the disk not renamed yet), renamed (the
// same, the disk renamed: the write done), inside (the album Nile open after it)

const fs = require("fs");
const os = require("os");
const path = require("path");
const { execFileSync } = require("child_process");
const { smallViews } = require("./page");

const STATES = {
  start: { rename: false },
  renaming: { rename: true, never: true },
  renamed: { rename: true },
  inside: { rename: true, open: "Nile" },
};

function html(ui, state, dark) {
  const st = STATES[state];
  if (!st) throw new Error("no such state: " + state + " (" + Object.keys(STATES).join(", ") + ")");
  const fake = fs.readFileSync(path.join(__dirname, "fakeapp.js"), "utf8");
  const mock = `
    window.onerror = (m, f, l) => { if (!document.title.startsWith("ERR")) document.title = "ERR " + m + " " + f + ":" + l; };
    window.addEventListener("unhandledrejection", (e) => { document.title = "REJ " + e.reason; });
    const module = { exports: {} };
    ${fake}
    const VIEWS = ${JSON.stringify(smallViews())}, ROOT = "/P", ST = ${JSON.stringify(st)};
    const app = module.exports.fake(VIEWS, { changed: true });
    const state = { changes: 1 };
    app.attach({ state, changed: () => { state.changes++; } });
    const onDisk = (p) => VIEWS.footage.links.some((l) => l.path === p) || VIEWS.footage.items.some((it) => it.path === p || it.preview === p) ||
      ["moments", "stills", "exports"].some((v) => VIEWS[v].items.some((it) => it.path === p));
    // A picture of each file: a sea of its own colour (by the file's name, so a renamed folder's file is the same)
    const pic = (p) => {
      const n = p.split("/").pop(), h = { A: 190, B: 165, C: 210, D: 30, E: 120 }[n.charAt(0)] || 260;
      const svg = '<svg xmlns="http://www.w3.org/2000/svg" width="320" height="180"><defs><linearGradient id="g" x1="0" y1="0" x2="0" y2="1">' +
        '<stop offset="0" stop-color="hsl(' + h + ',70%,62%)"/><stop offset="1" stop-color="hsl(' + (h + 20) + ',75%,22%)"/></linearGradient></defs>' +
        '<rect width="320" height="180" fill="url(#g)"/><circle cx="' + (60 + (h % 200)) + '" cy="110" r="34" fill="hsla(' + (h + 180) + ',60%,70%,.55)"/>' +
        '<text x="14" y="164" font-family="-apple-system,Helvetica" font-size="26" fill="white" opacity=".85">' + n.charAt(0) + '</text></svg>';
      return "data:image/svg+xml;base64," + btoa(svg);
    };
    const answers = Object.assign({
      snapshot: () => ({ project: ROOT, jobs: [], close_asked: false, macos: true, changes: state.changes, quiet_ms: 5000, catalog: "", missing: 0, moving: null }),
      project: () => ({ path: ROOT, name: "Red Sea 2025", legacy: false, clips_flat: false, clips_layout: "tree", moments_alone: false }),
      engine: () => "", recent: () => [], recent_tags: () => [], catalog_care: () => ({}), catalog_look: () => null, cached_view: () => null,
      probes_since: () => ({ version: 1, items: {}, pending: 0 }), missing_links: () => [], set_focus: () => null, lens_types: () => [], lens_info: () => ({}),
      places_of: (a) => (a.points || []).map(() => null), mini_map: () => null,
      scan: (a) => JSON.parse(JSON.stringify(VIEWS[a.view] || { root: ROOT, view: a.view, dirs: [], items: [] })),
    }, app.commands, {
      thumb: (a) => (onDisk(a.path) ? pic(a.path) : null),
    });
    const slow = { rename_album: 700, thumb: 30 };
    window.__TAURI__ = { core: { invoke: (cmd, a) => {
        if (cmd === "rename_album" && ST.never) return new Promise(() => {});
        return new Promise((ok, no) => setTimeout(() => { try { const f = answers[cmd]; Promise.resolve(f ? f(a || {}) : null).then(ok, no); } catch (e) { no(e); } }, slow[cmd] || 0));
      } },
      event: { listen: () => Promise.resolve(() => {}) }, webview: { getCurrentWebview: () => ({ onDragDropEvent: () => {} }) } };
    window.ST = ST;
    try { localStorage.clear(); } catch (_) {}`;
  const does = `
    const go = () => {
      if (!(S.root && DATA.footage)) return setTimeout(go, 20);
      S.view = "footage"; S.path = [AV_MARK + "My albums"]; S.flat = false; S.layout = "grid"; renderAll();
      if (ST.rename) setTimeout(() => {
        S.renaming = "f:Day 1"; commitRename("Nile");
        if (ST.open) setTimeout(() => { S.path = [ST.open]; renderAll(); }, 1200);
      }, 600);
    };
    setTimeout(go, 50);`;
  let page = fs.readFileSync(path.join(ui, "index.html"), "utf8");
  page = page.replace("<head>", '<head><base href="file://' + encodeURI(ui) + '/">' + (dark ? '<meta name="color-scheme" content="dark">' : ""));
  page = page.replace(/<script src=/, () => "<script>" + require("./lists").script() + "(() => {" + mock + "})();</script>\n<script src=");
  return page.replace("</body>", "<script>" + does + "</script></body>");
}

if (require.main === module) {
  const [state, out] = process.argv.slice(2);
  const i = process.argv.indexOf("--ui");
  const ui = path.resolve(i > 0 ? process.argv[i + 1] : path.join(__dirname, "..", "ui"));
  const dark = process.argv.includes("--dark");
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), "afl-shot-"));
  const file = path.join(dir, "shot.html");
  fs.writeFileSync(file, html(ui, state, dark));
  const chrome = process.env.CHROME || "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome";
  // Its own profile (the user's Chrome is left alone), no window
  const args = ["--headless=new", "--disable-gpu", "--hide-scrollbars", "--user-data-dir=" + path.join(dir, "profile"), "--allow-file-access-from-files",
    "--window-size=1100,620", "--virtual-time-budget=3500"].concat(dark ? ["--force-dark-mode", "--blink-settings=preferredColorScheme=0"] : []);
  if (process.env.SHOT_DEBUG) { console.log(execFileSync(chrome, args.concat(["--dump-dom", "file://" + file]), { stdio: ["ignore", "pipe", "ignore"] }).toString().slice(0, 4000)); process.exit(0); }
  execFileSync(chrome, args.concat(["--screenshot=" + path.resolve(out), "file://" + file]), { stdio: "ignore" });
  fs.rmSync(dir, { recursive: true, force: true });
  console.log(out);
}

module.exports = { html };
