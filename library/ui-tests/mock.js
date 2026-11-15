// SPDX-License-Identifier: GPL-3.0-or-later
//
// A stand-in for the app behind the Library's page (window.__TAURI__), for the page's tests (jsdom: culling_test.js; outside ui/, which the app bundles whole)
// and its pictures (headless Chrome over library/ui): a small project of clips, moments and photos, kept in memory;
// the commands the page calls answer from it and change it, so a view read again shows what was written. `MOCK.calls`
// lists what was called. Set window.MOCK_OPTS before it is loaded: { project: null } starts with no project open.
(function () {
  const ROOT = "/Volumes/Trips/Egypt 2026";
  const opts = window.MOCK_OPTS || {};
  const clip = (i, day, more) => Object.assign({ kind: "clip", id: day + "/DJI_2026110" + (1 + (i % 3)) + "10" + String(10 + i) + "00_00" + String(10 + i) + "_D.MP4", dir: day, pdir: "_Clips/" + day,
    name: "DJI_2026110" + (1 + (i % 3)) + "10" + String(10 + i) + "00_00" + String(10 + i) + "_D.MP4", size: 1.2e9 + i * 3e8, mtime: 1793870000 + i * 600, tags: [], own: {}, copies: [{ loc: 0 }] }, more || {});
  const clips = [
    clip(0, "Day 1", { tags: ["Turtle", "Reef"], pick: "accept", rating: 4 }),
    clip(1, "Day 1", { tags: ["Reef"], pick: "accept", rating: 2 }),
    clip(2, "Day 1", { tags: ["reef", "Dolphin"], pick: "reject" }),
    clip(3, "Day 1", { tags: [] }),
    clip(4, "Day 1", { tags: ["Wreck"], rating: 5 }),
    clip(5, "Day 2", { tags: ["Reef", "Wreck"], rating: 3, pick: "accept" }),
    clip(6, "Day 2", { tags: ["for family"] }),
    clip(7, "Day 2", { tags: [], pick: "reject" }),
  ];
  for (const c of clips) c.path = ROOT + "/" + c.id;
  const moment = (name, c, a, b, more) => Object.assign({ kind: "moment", id: "_Moments/" + name + ".afplink.mp4", dir: "_Moments", name, path: ROOT + "/_Moments/" + name + ".afplink.mp4", size: 4e6, mtime: 1793880000,
    tags: [], link: { v: 1, kind: "moment", clip: c.id, in: a, out: b }, original: c.path, renders: {} }, more || {});
  const moments = [moment("Turtle close", clips[0], 12, 31, { tags: ["Turtle", "for family"], pick: "accept", rating: 5 }), moment("Wreck bow", clips[4], 40, 58, { tags: ["Wreck"], rating: 3 }), moment("Dolphins far", clips[2], 5, 14, { pick: "reject" })];
  const still = (name, more) => Object.assign({ kind: "still", id: "_Stills/" + name, dir: "_Stills", name, path: ROOT + "/_Stills/" + name, size: 3e6, mtime: 1793881000, tags: [], form: "HEIC with gain map", link: { v: 1, kind: "still" } }, more || {});
  const stills = [still("Turtle 1.heic", { tags: ["Turtle"], rating: 5, pick: "accept" }), still("Reef 2.heic", { tags: ["Reef"] }), still("Blurry 3.heic", { pick: "reject" })];
  const probes = {};
  for (const c of clips) probes[c.path] = { camera: "DJI OsmoAction6", color: "dlogm", mode: "gyro", movie: { duration: 95, created: c.mtime } };
  const M = window.MOCK = {
    root: "project" in opts ? opts.project : ROOT, clips, moments, stills, calls: [], recentTags: ["Turtle", "Night dive"],
    recent: [{ path: ROOT, name: "Egypt 2026", here: true }, { path: "/Volumes/Trips/Norway 2025", name: "Norway 2025", here: false, share: "//nas.local/Trips", connect: true, names: ["nas.local"] },
      { path: "/Users/me/Movies/Garden", name: "Garden", here: true }],
    atStart: opts.atStart || null, settings: { tag_stores: [] },
    // What a command answers: replaced by a test to make it fail, wait…
    handlers: {},
  };
  const copy = (x) => JSON.parse(JSON.stringify(x));
  const find = (it) => (it.kind === "clip" ? clips : it.kind === "moment" ? moments : stills).find((x) => x.id === it.id);
  const base = {
    engine: () => "ok",
    snapshot: () => ({ macos: opts.macos !== false, project: M.root, jobs: [], changes: 0, quiet_ms: 5000, missing: 0, catalog: "", close_asked: false, moving: null }),
    project: () => (M.root ? { name: M.root.split("/").pop(), path: M.root, tag_stores: M.settings.tag_stores } : null),
    recent: () => copy(M.recent.map((r) => Object.assign({}, r, { at_start: r.path === M.atStart }))),
    recent_tags: () => M.recentTags.slice(),
    cached_view: () => null,
    scan: (a) => {
      const v = a.view;
      if (v === "footage") return { root: M.root, items: copy(clips), dirs: ["Day 1", "Day 2"], clips_dirs: ["_Clips/Day 1", "_Clips/Day 2"], locations: [{ name: "Trips", path: "." }], probes: { items: copy(probes), version: 1, pending: 0 } };
      if (v === "moments") return { root: M.root, items: copy(moments), dirs: ["_Moments"] };
      if (v === "stills") return { root: M.root, items: copy(stills), dirs: ["_Stills"] };
      return { root: M.root, items: [], dirs: [] };
    },
    probes_since: () => ({ items: {}, version: 1, pending: 0 }),
    missing_links: () => [],
    set_focus: () => null,
    thumb: () => null,
    set_pick: (a) => { for (const i of a.items) { const it = find(i); if (it) { if (a.pick) it.pick = a.pick; else delete it.pick; } } return []; },
    set_rating: (a) => { for (const i of a.items) { const it = find(i); if (it) { if (a.rating) it.rating = a.rating; else delete it.rating; } } return []; },
    set_tags: (a) => {
      const same = (x, y) => x.toLowerCase() === y.toLowerCase();
      for (const i of a.items) {
        const it = find(i);
        if (!it) continue;
        it.tags = it.tags.filter((t) => !a.remove.some((r) => same(r, t)));
        for (const t of a.add) if (!it.tags.some((x) => same(x, t))) it.tags.push(t);
      }
      for (const t of a.add) if (!M.recentTags.some((x) => same(x, t))) M.recentTags.unshift(t);
      return [];
    },
    open_project: (a) => { M.root = a.path; return a.path; },
    close_project: () => { M.root = null; return null; },
    start_project: () => M.atStart,
    set_start_project: (a) => { M.atStart = a.path || null; return M.atStart; },
    forget_recent: (a) => { M.recent = M.recent.filter((r) => r.path !== a.path); if (M.atStart === a.path) M.atStart = null; return null; },
  };
  window.__TAURI__ = {
    core: { invoke: (cmd, args) => { M.calls.push([cmd, copy(args || {})]); const f = M.handlers[cmd] || base[cmd]; return f ? Promise.resolve().then(() => f(args || {})) : Promise.reject("mock: no " + cmd); } },
    event: { listen: () => Promise.resolve(() => {}) },
    webview: { getCurrentWebview: () => ({ onDragDropEvent: () => {} }) },
  };
})();
