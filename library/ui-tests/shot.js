"use strict";
// A picture of the Library's page in a state, without a window: headless Chrome over ../ui with a mock of the app
// (page.js's small project) and a few lines that bring the state about. For the pictures a UI change's PR carries.
//
//   node shot.js <state> <out.png> [--ui <another ui folder, e.g. main's>]
//
// States: unsaved (a tag that couldn't be written), saving (changes still being written, in Tasks), closing (the
// window closed with changes waiting), tagged (a tag just added, the app not answered yet), catalog (Checks' catalog part
// after a check against the files).

const fs = require("fs");
const os = require("os");
const path = require("path");
const { execFileSync } = require("child_process");
const { smallViews } = require("./page");

const STATES = {
  // set_tags: how the mock answers it; then what's done once the project shows
  unsaved: { set_tags: { fail: "//nas/footage isn’t connected (No route to host)" }, does: "S.path = ['Day 1']; showTab('tags'); renderAll(); sel(['Day 1/A.MP4', 'Day 1/B.MP4']); tag('turtle');" },
  saving: { set_tags: { never: true }, does: "S.path = ['Day 1']; showTab('queue'); renderAll(); sel(['Day 1/B.MP4', 'Day 1/C.MP4']); S.tagDraft = 'turtle'; setTags(['turtle'], []);" },
  closing: { set_tags: { never: true }, does: "S.path = ['Day 1']; showTab('tags'); renderAll(); sel(['Day 1/B.MP4']); tag('turtle'); setTimeout(() => { window.closeWaits = true; closing(true); }, 700);" },
  // A list filtered to a tag, the tag taken off the clip selected: it leaves the list
  leaves: { set_tags: { apply: true }, first: "VIEWS.footage.items[1].tags = ['reef']; VIEWS.footage.items[2].tags = ['reef'];",
    does: "S.path = ['Day 1']; showTab('tags'); renderAll(); buildFacets(); S.filters = { 'tag:reef': true }; S.filtersOpen = true; renderAll(); sel(['Day 1/A.MP4']); setTimeout(() => document.querySelector('[data-a=\"untag\"][data-v=\"reef\"]').click(), 200);" },
  tagged: { set_tags: { never: true }, does: "S.path = ['Day 1']; showTab('tags'); renderAll(); sel(['Day 1/A.MP4', 'Day 1/B.MP4']); tag('turtle');" },
  // Checks' catalog part after "Read every file again" corrected two records, with one clip's own file not JSON
  catalog: { set_tags: { never: true }, care: { kind: "check", busy: false, changes: 2, corrected: ["/P/Day 1/A.afp.json", "/P/Generated/2026-11-01 Moments/Day 1/A m.mp4"],
    unreadable_now: ["/P/Day 1/C.afp.json"] }, does: "S.path = ['Day 1']; showTab('checks'); renderAll(); sel(['Day 1/A.MP4']);" },
};

function html(ui, state) {
  const st = STATES[state];
  if (!st) throw new Error("no such state: " + state + " (" + Object.keys(STATES).join(", ") + ")");
  const mock = `
    window.onerror = (m, f, l) => { if (!document.title.startsWith("ERR")) document.title = "ERR " + m + " " + f + ":" + l; };
    window.addEventListener("unhandledrejection", (e) => { document.title = "REJ " + e.reason; });
    const VIEWS = ${JSON.stringify(smallViews())}, ROOT = "/P", ST = ${JSON.stringify(st)};
    ${st.first || ""}
    const answers = {
      snapshot: () => ({ project: ROOT, jobs: [], close_asked: false, close_waits: !!window.closeWaits, macos: true, changes: 1, quiet_ms: 5000, catalog: "", missing: 0, moving: null }),
      project: () => ({ path: ROOT, name: "Red Sea 2025", legacy: false, clips_flat: false, clips_layout: "tree", moments_alone: false }),
      engine: () => "", recent: () => [], catalog_care: () => ST.care || {}, catalog_look: () => null, cached_view: () => null, probes_since: () => ({ version: 1, items: {}, pending: 0 }), thumb: () => null, missing_links: () => [],
      scan: (a) => JSON.parse(JSON.stringify(VIEWS[a.view] || { root: ROOT, view: a.view, dirs: [], items: [] })),
      set_tags: (a) => {
        if (ST.set_tags.never) return new Promise(() => {});
        if (ST.set_tags.fail) throw ST.set_tags.fail;
        // (written: a view read afterwards shows it)
        if (ST.set_tags.apply) for (const x of a.items) { const it = VIEWS.footage.items.find((c) => c.id === x.id); if (it) it.tags = (it.tags || []).filter((t) => !a.remove.includes(t)).concat(a.add); }
        return [];
      },
    };
    window.__TAURI__ = { core: { invoke: (cmd, a) => new Promise((ok, no) => setTimeout(() => { try { const f = answers[cmd]; Promise.resolve(f ? f(a || {}) : null).then(ok, no); } catch (e) { no(e); } }, cmd === "set_tags" ? 60 : 0)) },
      event: { listen: () => Promise.resolve(() => {}) }, webview: { getCurrentWebview: () => ({ onDragDropEvent: () => {} }) } };`;
  const does = `
    const sel = (ids) => { S.sel = {}; for (const id of ids) S.sel["i:" + id] = true; renderMain(); renderSide(); };
    const tag = (t) => { S.tagDraft = t; renderSide(); document.querySelector('[data-a="tag"]').click(); };
    const go = () => { if (!(S.root && DATA.footage && DATA.moments)) return setTimeout(go, 20); ${st.does} };
    setTimeout(go, 50);`;
  let page = fs.readFileSync(path.join(ui, "index.html"), "utf8");
  page = page.replace("<head>", '<head><base href="file://' + encodeURI(ui) + '/">');
  // (in a function of its own: the page's scripts have names of their own)
  page = page.replace(/<script src=/, () => "<script>(() => {" + mock + "})();</script>\n<script src=");
  return page.replace("</body>", "<script>" + does + "</script></body>");
}

if (require.main === module) {
  const [state, out] = process.argv.slice(2);
  const i = process.argv.indexOf("--ui");
  const ui = path.resolve(i > 0 ? process.argv[i + 1] : path.join(__dirname, "..", "ui"));
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), "afl-shot-"));
  const file = path.join(dir, "shot.html");
  fs.writeFileSync(file, html(ui, state));
  const chrome = process.env.CHROME || "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome";
  // Its own profile (the user's Chrome is left alone), no window
  const args = ["--headless=new", "--disable-gpu", "--hide-scrollbars", "--user-data-dir=" + path.join(dir, "profile"), "--allow-file-access-from-files",
    "--window-size=1320,860", "--virtual-time-budget=3000"];
  // SHOT_DEBUG: the page as it ended up, as text (a script that failed is in its title)
  if (process.env.SHOT_DEBUG) { console.log(execFileSync(chrome, args.concat(["--dump-dom", "file://" + file]), { stdio: ["ignore", "pipe", "ignore"] }).toString().slice(0, 3000)); process.exit(0); }
  execFileSync(chrome, args.concat(["--screenshot=" + path.resolve(out), "file://" + file]), { stdio: "ignore" });
  console.log(out);
}
