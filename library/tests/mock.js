// A mock of the app for the Library's page (library/ui): a small project with clips, albums and probes, and the
// commands the page calls. For the page's tests in jsdom (albums.js here) and for screenshots in a headless browser
// (shots.js): in a browser it installs itself as window.__TAURI__ before the page's scripts; in node,
// require("./mock.js")(window) does, and returns { calls, state }.
(function () {
  function install(w) {
    const ROOT = "/p/Trips";
    const calls = [];
    const clip = (tree, o) => Object.assign({ id: tree, dir: tree.split("/").slice(0, -1).join("/"), name: tree.split("/").pop(), path: "/Volumes/video/" + tree, size: 1.2e9, mtime: 1761139722,
      kind: "clip", loc: "Archive", copies: [{ loc: "Archive", li: 0, rel: tree, path: "/Volumes/video/" + tree, offline: false }], own_files: [], gyroflow: false, preview: null, pdir: null }, o || {});
    const state = {
      jobs: [],
      clips: [
        clip("Egypt 2026/Day 1/DJI_20261020083012_0001_D.MP4", { thumb: "/Volumes/video/Egypt 2026/Day 1/DJI_20261020083012_0001_D.LRF" }),
        clip("Egypt 2026/Day 1/DJI_20261020091544_0002_D.MP4"),
        clip("Egypt 2026/Day 1/DJI_20261020101201_0003_D.MP4", { own: { source: { curve: "dlog" } }, own_files: ["/Volumes/video/Egypt 2026/Day 1/DJI_20261020101201_0003_D.afp.json"] }),
        clip("Egypt 2026/Day 2/GX010123.MP4", { size: 3.9e9 }),
        clip("Egypt 2026/Day 2/GX010124.MP4", { size: 2.1e9 }),
        clip("Egypt 2026/Day 2/GX010125.MP4"),
        clip("Alps 2026/DJI_20260214102233_0007_D.MP4"),
        clip("Alps 2026/DJI_20260214113020_0008_D.MP4"),
      ],
      // Clip links: [its path in the project's folder, its clip]
      links: [
        ["_Clips/Egypt/DJI_20261020083012_0001_D.afplink.mp4", "Egypt 2026/Day 1/DJI_20261020083012_0001_D.MP4"],
        ["_Clips/Egypt/GX010123.afplink.mp4", "Egypt 2026/Day 2/GX010123.MP4"],
        ["_Clips/Egypt/Dives/GX010124.afplink.mp4", "Egypt 2026/Day 2/GX010124.MP4"],
        ["_Clips/Best of the year/GX010123.afplink.mp4", "Egypt 2026/Day 2/GX010123.MP4"],
        ["_Clips/Best of the year/DJI_20260214102233_0007_D.afplink.mp4", "Alps 2026/DJI_20260214102233_0007_D.MP4"],
        ["Generated/2026-11-10 14-32 Clip previews/DJI_20260214113020_0008_D.afplink.mp4", "Alps 2026/DJI_20260214113020_0008_D.MP4"],
      ],
      // Moments, photos (links and rendered ones) and exports: [its path in the project's folder, its link, more]
      moments: [
        ["_Moments/Best of the year/Turtle.afplink.mp4", { clip: "Egypt 2026/Day 2/GX010123.MP4", in: 12, out: 19.5, created: "2025-10-22T17:58:42.000000Z", snap: 1 }, { renders: { HDR: "current" } }],
        ["_Moments/Generated/2026-11-10 14-32/GX010124 0.03.afplink.mp4", { clip: "Egypt 2026/Day 2/GX010124.MP4", in: 3, out: 9, created: "2025-10-22T19:28:42.000000Z" }, {}],
        ["_Moments/Generated/2026-11-10 14-32/GX010124 0.12.afplink.mp4", { clip: "Egypt 2026/Day 2/GX010124.MP4", in: 12, out: 20, created: "2025-10-22T19:28:42.000000Z" }, { renders: { HDR: "stale" } }],
      ],
      stills: [
        ["_Clips/Best of the year/GX010123_14.00s.afplink.jpg", { clip: "Egypt 2026/Day 2/GX010123.MP4", t: 14, created: "2025-10-22T17:58:42.000000Z" }, { form: "Photo link", linked: true, renders: { Photo: "current" } }],
        ["_Stills/Generated/GX010124_3.00s.heic", { clip: "Egypt 2026/Day 2/GX010124.MP4", t: 3 }, { form: "Live Photo", linked: false }],
        ["_Stills/Generated/GX010125_7.50s.MP.jpg", { clip: "Egypt 2026/Day 2/GX010125.MP4", t: 7.5 }, { form: "Motion Photo", linked: false }],
      ],
      exports: [
        ["_Exports/Generated/Turtle.mp4", { clip: "Egypt 2026/Day 2/GX010123.MP4", in: 12, out: 19.5, look: "dv" }, {}],
      ],
      dirs: ["_Clips", "_Clips/Egypt", "_Clips/Egypt/Dives", "_Clips/Best of the year", "Generated", "Generated/2026-11-10 14-32 Clip previews"],
    };
    const probe = (c, i) => {
      const dji = c.name.startsWith("DJI");
      return { mode: "gyro", camera: dji ? "DJI OsmoAction6" : "GoPro HERO12 Black", color: dji ? "dlogm" : "normal", lens: "", movie: { duration: 40 + i * 37, created: 1761139722 + i * 5400 }, marks: { times: i % 2 ? [3, 12] : [] } };
    };
    const probes = () => { const items = {}; state.clips.forEach((c, i) => { items[c.path] = probe(c, i); }); return { version: 1, items, pending: 0 }; };
    const parent = (r) => r.split("/").slice(0, -1).join("/");
    const footage = () => {
      const first = {};
      for (const [rel, c] of state.links) if (!(c in first)) first[c] = rel;
      const dirs = new Set();
      for (const c of state.clips) { const parts = c.dir.split("/"); parts.forEach((_, i) => dirs.add(parts.slice(0, i + 1).join("/"))); }
      const clips_dirs = new Set(state.dirs);
      for (const [rel] of state.links) { let d = parent(rel); while (d) { clips_dirs.add(d); d = parent(d); } }
      return {
        root: ROOT, view: "footage", dirs: Array.from(dirs).sort(), clips_dirs: Array.from(clips_dirs).sort(),
        items: state.clips.map((c) => Object.assign({}, c, { preview: first[c.id] ? ROOT + "/" + first[c.id] : null, pdir: first[c.id] ? parent(first[c.id]) : null })),
        links: state.links.map(([rel, c]) => ({ path: ROOT + "/" + rel, rel, clip: c, draft: false, size: 21000, mtime: 1761139722 })),
        locations: [{ name: "Archive", at: "", path: "//NAS/video", dir: "/Volumes/video", copies: false, local: false, dirs: [], offline: false, connect: false, names: [] }],
        probes: probes(),
      };
    };
    // The moments', photos' and exports' scans: an item a file, as index.rs gives them
    const own = (view) => {
      const list = state[view], kind = { moments: "moment", stills: "still", exports: "export" }[view];
      const dirs = new Set();
      for (const [rel] of list) { let d = parent(rel); while (d) { dirs.add(d); d = parent(d); } }
      return { root: ROOT, view, dirs: Array.from(dirs).sort(), probes: probes(), items: list.map(([rel, link, more]) => Object.assign({
        id: rel, dir: parent(rel), name: kind === "moment" ? rel.split("/").pop().replace(/\.afplink\.mp4$/, "") : rel.split("/").pop(), path: ROOT + "/" + rel, size: kind === "export" ? 8.4e7 : 2.4e5, mtime: 1762784000,
        kind, tags: [], link: Object.assign({ v: 1, kind: kind === "still" ? "photo" : kind, size: 3.9e9, roots: [ROOT] }, link) },
        kind === "moment" ? { gyroflow: false, renders: {}, original: "/Volumes/video/" + link.clip } : {}, more)) };
    };
    const everything = () => state.moments.concat(state.stills, state.exports);
    // A picture for each file: a gradient from its name, flat and grey for an original that's log (as Finder draws it)
    const picture = (path) => {
      let h = 0;
      for (const ch of path.split("/").pop().replace(/\.[^.]*$/, "").replace(".afplink", "")) h = (h * 31 + ch.charCodeAt(0)) % 360;
      const link = path.includes(".afplink."), log = /DJI_/.test(path) && !link;
      const sat = log ? 12 : 62, a = "hsl(" + h + "," + sat + "%," + (log ? 55 : 46) + "%)", b = "hsl(" + ((h + 50) % 360) + "," + sat + "%," + (log ? 42 : 24) + "%)";
      const svg = '<svg xmlns="http://www.w3.org/2000/svg" width="320" height="180"><defs><linearGradient id="g" x1="0" y1="0" x2="1" y2="1"><stop offset="0" stop-color="' + a + '"/><stop offset="1" stop-color="' + b + '"/></linearGradient></defs>' +
        '<rect width="320" height="180" fill="url(#g)"/><circle cx="' + (60 + h % 200) + '" cy="' + (50 + h % 80) + '" r="34" fill="rgba(255,255,255,.22)"/><path d="M0 180 L90 110 L160 150 L230 90 L320 140 L320 180z" fill="rgba(0,0,0,.28)"/></svg>';
      return "data:image/svg+xml;base64," + (typeof btoa === "function" ? btoa(svg) : Buffer.from(svg).toString("base64"));
    };
    const invoke = async (cmd, args) => {
      calls.push([cmd, args]);
      switch (cmd) {
        case "snapshot": return { project: ROOT, jobs: state.jobs, close_asked: false, macos: true, changes: 1, quiet_ms: 5000, catalog: "", missing: 0, moving: null };
        case "project": return { path: ROOT, name: "Trips", counts: {}, settings: {} };
        case "recent": return [];
        case "project_settings": return { path: ROOT, name: "Trips", id: "t", hosts: {}, locations: [], local: [], caches: [], never_rename: false, tag_stores: ["library"],
                                          finder_here: true, generated_runs: false, next_to_footage: true,
                                          renders: state.rendersCleared ? { files: 0, bytes: 0, others: 1 } : { files: 3, bytes: 52428800, others: 1 } };
        case "clear_renders": state.rendersCleared = true; return { files: 3, bytes: 52428800, kept: 1 };
        case "engine": return "ok";
        case "cached_view": return null;
        case "scan": return args.view === "footage" ? footage() : own(args.view);
        case "probes_since": return { version: 1, items: {}, pending: 0 };
        case "thumb": return picture(args.path);
        case "album_add": {
          const made = [];
          let already = 0;
          for (const c of args.clips) {
            if (state.links.some(([rel, t]) => t === c.tree && parent(rel) === args.album)) { already++; continue; }
            const rel = args.album + "/" + c.tree.split("/").pop().replace(/\.[^.]+$/, "") + ".afplink.mp4";
            state.links.push([rel, c.tree]);
            made.push(ROOT + "/" + rel);
          }
          return { made, already, album: args.album, failed: [], ms: 12 * made.length };
        }
        case "album_move": {
          let moved = 0;
          for (const f of args.files) {
            const l = state.links.find(([rel]) => ROOT + "/" + rel === f);
            if (l) { l[0] = args.album + "/" + l[0].split("/").pop(); moved++; }
            const o = everything().find(([rel]) => ROOT + "/" + rel === f);
            if (o) { o[0] = args.album + "/" + o[0].split("/").pop(); moved++; }
          }
          if (!state.dirs.includes(args.album)) state.dirs.push(args.album);
          return { moved, album: args.album, problems: [] };
        }
        case "album_copy": {
          let copied = 0;
          for (const f of args.files) for (const view of ["moments", "stills"]) {
            const o = state[view].find(([rel]) => ROOT + "/" + rel === f);
            if (o) { state[view].push([args.album + "/" + o[0].split("/").pop(), o[1], o[2]]); copied++; }
          }
          return { copied, album: args.album, problems: [] };
        }
        case "photo_add": {
          const made = [];
          for (const ph of args.photos) {
            const rel = args.album + "/" + ph.tree.split("/").pop().replace(/\.[^.]+$/, "") + "_" + ph.t.toFixed(2) + "s.afplink.jpg";
            state.stills.push([rel, { clip: ph.tree, t: ph.t }, { form: "Photo link", linked: true }]);
            made.push(ROOT + "/" + rel);
          }
          return { made, already: 0, album: args.album, failed: [], ms: 180 * made.length };
        }
        case "rename_item": {
          const o = everything().find(([rel]) => ROOT + "/" + rel === args.path);
          if (o) o[0] = parent(o[0]) + "/" + args.name + o[0].slice(o[0].lastIndexOf("/") + 1).replace(/^.*?(\.afplink\.(mp4|jpg)|\.MP\.jpg|\.[^.]+)$/, "$1");
          return ROOT + "/" + (o ? o[0] : "");
        }
        // Share (share.rs): what the cache has is saved at once, the rest waits for the player; state.shareFolder is
        // the folder "chosen" (null: the dialog cancelled), state.shareEnd(name) ends the run, `name` failing
        case "share": {
          if (state.shareFolder === null) return null;
          const folder = args.folder || state.shareFolder || "/Users/me/Desktop/For Anna";
          const cached = (it) => it.kind === "file" || (it.kind === "moment" && ((state.moments.find(([rel]) => rel === it.path) || [])[2] || {}).renders || {})[args.look] === "current"
            || (it.kind === "photo" && (((state.stills.find(([rel]) => rel === it.path) || [])[2] || {}).renders || {}).Photo === "current");
          const items = args.items.map((it) => cached(it) ? { name: it.name, kind: it.kind, state: "copied", message: null, saved: folder + "/" + it.name } : { name: it.name, kind: it.kind, state: "waiting", message: null, saved: null });
          const first = items.find((i) => i.state === "waiting");
          if (first) first.state = "rendering";
          const made = items.filter((i) => i.state === "copied").length;
          state.jobs.push({ id: state.jobs.length + 1, kind: "share", title: "Share · " + items.length + " items to " + folder.split("/").pop(), project: ROOT, project_name: "Trips",
            state: first ? "running" : "done", done: made, total: items.length, failed: 0, made, fraction: first ? 0.4 : null, current: first ? first.name : null, message: null, failures: [], left_out: [],
            share: { dest: args.dest, folder, items } });
          return folder;
        }
        case "add_job": state.jobs.push({ id: state.jobs.length + 1, kind: args.kind, label: args.label, state: "queued" }); return null;
        case "delete_items": {
          const gone = (rel) => args.items.some((x) => x.path === ROOT + "/" + rel || (x.kind === "folder" && (ROOT + "/" + rel).startsWith(x.path + "/")));
          const n = () => state.links.length + everything().length;
          const before = n();
          state.links = state.links.filter(([rel]) => !gone(rel));
          for (const view of ["moments", "stills", "exports"]) state[view] = state[view].filter(([rel]) => !gone(rel));
          state.dirs = state.dirs.filter((d) => !args.items.some((x) => x.kind === "folder" && (ROOT + "/" + d === x.path || (ROOT + "/" + d).startsWith(x.path + "/"))));
          return { done: before - n() || args.items.length, to: ["the Trash"], problems: [] };
        }
        case "new_folder": { const d = (args.parent ? args.parent + "/" : "") + "untitled folder"; state.dirs.push(d); return d; }
        default: return null;
      }
    };
    state.shareEnd = (failing) => {
      const j = state.jobs.filter((x) => x.kind === "share").pop();
      for (const it of j.share.items) if (it.state !== "copied") { if (it.name === failing) { it.state = "failed"; it.message = "its original isn’t found"; } else { it.state = "copied"; it.saved = j.share.folder + "/" + it.name; } }
      j.made = j.share.items.filter((i) => i.state === "copied").length; j.failed = j.share.items.length - j.made; j.done = j.total; j.state = "done"; j.fraction = null; j.current = null;
      j.failures = j.share.items.filter((i) => i.state === "failed").map((i) => [i.name, i.message]);
    };
    w.__TAURI__ = { state, core: { invoke }, event: { listen: () => {} }, webview: { getCurrentWebview: () => ({ onDragDropEvent: () => {} }) } };
    return { calls, state, ROOT };
  }
  if (typeof module !== "undefined" && module.exports) module.exports = install;
  else if (typeof window !== "undefined" && !window.__TAURI__) window.__MOCK__ = install(window);
})();
