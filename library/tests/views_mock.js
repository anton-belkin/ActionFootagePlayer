// What Timeline and Map need, laid over mock.js (which must be installed first): the mock's clips get locations (a
// clip's own file's `location`, as library/src/geo.rs writes it), the project gets moments and photos, and with
// `big: N` a synthetic library of N more clips over several years: trips of a few days at one place, single days at
// home, some clips without a date and some without a location. Deterministic (its own little generator).
// In a browser: window.__VIEWS_MOCK__ = { big: 10000 } before this file sets the size; in node, require(...)(window, { big }).
(function () {
  const PLACES = [
    [28.5091, 34.5136, { locality: "Dahab", region: "South Sinai", country: "Egypt", cc: "EG" }],
    [27.7333, 34.2500, { site: "Ras Muhammad National Park", locality: "Sharm El Sheikh", region: "South Sinai", country: "Egypt", cc: "EG" }],
    [45.9237, 6.8694, { locality: "Chamonix-Mont-Blanc", region: "Haute-Savoie", country: "France", cc: "FR" }],
    [46.2044, 6.1432, { locality: "Geneva", region: "Geneva", country: "Switzerland", cc: "CH" }],
    [46.0207, 7.7491, { locality: "Zermatt", region: "Valais", country: "Switzerland", cc: "CH" }],
    [36.3932, 25.4615, { locality: "Santorini", region: "South Aegean", country: "Greece", cc: "GR" }],
    [32.6669, -16.9241, { locality: "Funchal", region: "Madeira", country: "Portugal", cc: "PT" }],
    [64.1466, -21.9426, { locality: "Reykjavík", region: "Capital Region", country: "Iceland", cc: "IS" }],
    [-8.4095, 115.1889, { locality: "Ubud", region: "Bali", country: "Indonesia", cc: "ID" }],
    [4.1755, 73.5093, { locality: "Malé", region: "Kaafu Atoll", country: "Maldives", cc: "MV" }],
    [28.2916, -16.6291, { locality: "Adeje", region: "Canary Islands", country: "Spain", cc: "ES" }],
    [46.5197, 6.6323, { locality: "Lausanne", region: "Vaud", country: "Switzerland", cc: "CH" }],
  ];
  function install(w, opts) {
    opts = opts || w.__VIEWS_MOCK__ || {};
    // `places: true`: twelve more places in other countries, for long lists of countries and places (ui_fixes_shots.js)
    if (opts.places && PLACES.length === 12) [["Tromsø", "Troms", "Norway", 69.65, 18.96], ["Cairns", "Queensland", "Australia", -16.92, 145.77], ["Cape Town", "Western Cape", "South Africa", -33.92, 18.42],
      ["Queenstown", "Otago", "New Zealand", -45.03, 168.66], ["Banff", "Alberta", "Canada", 51.18, -115.57], ["Cusco", "Cusco", "Peru", -13.53, -71.97], ["Kyoto", "Kyoto", "Japan", 35.01, 135.77],
      ["Hội An", "Quảng Nam", "Vietnam", 15.88, 108.33], ["Zanzibar City", "Zanzibar", "Tanzania", -6.16, 39.19], ["Dubrovnik", "Dubrovnik-Neretva", "Croatia", 42.65, 18.09],
      ["Tulum", "Quintana Roo", "Mexico", 20.21, -87.46], ["Marrakesh", "Marrakesh-Safi", "Morocco", 31.63, -8.0]].forEach((p) => PLACES.push([p[3], p[4], { locality: p[0], region: p[1], country: p[2] }]));
    const inner = w.__TAURI__.core.invoke;
    const ROOT = "/p/Trips";
    const loc = (i, dlat, dlon, named) => { const p = PLACES[i]; const l = { lat: p[0] + (dlat || 0), lon: p[1] + (dlon || 0), source: "photos" }; if (named !== false) l.place = p[2]; return l; };
    // The mock's own clips: Egypt's day 1 in Dahab, day 2 at Ras Muhammad (one clip with no location), the Alps in
    // Chamonix (one with a point but no name looked up yet)
    const own = {
      "Egypt 2026/Day 1/DJI_20261020083012_0001_D.MP4": loc(0), "Egypt 2026/Day 1/DJI_20261020091544_0002_D.MP4": loc(0, 0.004, 0.002),
      "Egypt 2026/Day 1/DJI_20261020101201_0003_D.MP4": loc(0, -0.003, 0.001), "Egypt 2026/Day 2/GX010123.MP4": loc(1), "Egypt 2026/Day 2/GX010124.MP4": loc(1, 0.002, 0.003),
      "Alps 2026/DJI_20260214102233_0007_D.MP4": loc(2), "Alps 2026/DJI_20260214113020_0008_D.MP4": loc(2, 0.21, 0.37, false),
    };
    let seed = 12345;
    const rnd = () => { seed = (seed * 1103515245 + 12345) & 0x7fffffff; return seed / 0x7fffffff; };
    const big = [];
    if (opts.big) {
      const n = opts.big, start = Date.UTC(2020, 0, 5, 9) / 1000, span = 6.8 * 365 * 86400;
      let made = 0, t = start, trip = 0;
      while (made < n) {
        // A trip (most of the clips) or a day at home
        const isTrip = rnd() < 0.7, place = isTrip ? Math.floor(rnd() * PLACES.length) : 3, days = isTrip ? 3 + Math.floor(rnd() * 8) : 1;
        trip++;
        for (let d = 0; d < days && made < n; d++) {
          const clips = isTrip ? 8 + Math.floor(rnd() * 40) : 1 + Math.floor(rnd() * 6);
          for (let k = 0; k < clips && made < n; k++) {
            const created = Math.round(t + d * 86400 + k * 600 + rnd() * 300);
            const tree = "Big/" + String(trip).padStart(4, "0") + "/GX" + String(100000 + made).slice(1) + ".MP4";
            const r = rnd();
            big.push({ tree, created: r < 0.03 ? null : created, location: r > 0.85 ? null : loc(place, (rnd() - 0.5) * 0.08, (rnd() - 0.5) * 0.08, r > 0.1), duration: 20 + Math.floor(rnd() * 400) });
            made++;
          }
        }
        t += (days + 2 + rnd() * (span / (n / 28) / 86400)) * 86400;
      }
    }
    const moments = [
      { rel: "_Moments/Egypt/GX010123 0m12s.afplink.mp4", clip: "Egypt 2026/Day 2/GX010123.MP4", a: 12, b: 21 },
      { rel: "_Moments/Egypt/DJI_20261020083012_0001_D 0m05s.afplink.mp4", clip: "Egypt 2026/Day 1/DJI_20261020083012_0001_D.MP4", a: 5, b: 14 },
    ];
    const stills = [{ rel: "_Stills/Egypt/GX010124_8.5s.heic", form: "Photo", mtime: 1761150000 }, { rel: "_Stills/Alps/DJI_20260214102233_0007_D_3s.heic", form: "Live Photo", mtime: 1771070000 }];
    w.__TAURI__.core.invoke = async (cmd, args) => {
      const r = await inner(cmd, args);
      if (cmd !== "scan") return r;
      if (args.view === "footage") {
        for (const it of r.items) if (own[it.id]) it.own = Object.assign({}, it.own, { location: own[it.id] });
        const dirs = new Set(r.dirs);
        for (const b of big) {
          const path = "/Volumes/video/" + b.tree, dir = b.tree.split("/").slice(0, -1).join("/");
          dirs.add("Big"); dirs.add(dir);
          r.items.push({ id: b.tree, dir, name: b.tree.split("/").pop(), path, size: 9e8, mtime: 1761139722, kind: "clip", loc: "Archive", copies: [{ loc: "Archive", li: 0, rel: b.tree, path, offline: false }],
            own_files: [], gyroflow: false, preview: null, pdir: null, own: b.location ? { location: b.location } : undefined });
          r.probes.items[path] = { mode: "gyro", camera: "GoPro HERO12 Black", color: "normal", lens: "", movie: { duration: b.duration, created: b.created }, marks: { times: [] } };
        }
        r.dirs = Array.from(dirs).sort();
      } else if (args.view === "moments") {
        r.dirs = ["_Moments", "_Moments/Egypt"];
        r.items = moments.map((m) => ({ id: m.rel, dir: "_Moments/Egypt", name: m.rel.split("/").pop(), path: ROOT + "/" + m.rel, size: 21000, mtime: 1761139722, kind: "moment", renders: {},
          original: "/Volumes/video/" + m.clip, link: { v: 1, clip: m.clip, in: m.a, out: m.b, created: m.clip.includes("DJI_2026102008") ? "2026-10-20T08:30:12" : "2025-10-22T18:00:00Z" } }));
      } else if (args.view === "stills") {
        r.dirs = ["_Stills", "_Stills/Egypt", "_Stills/Alps"];
        r.items = stills.map((s) => ({ id: s.rel, dir: s.rel.split("/").slice(0, -1).join("/"), name: s.rel.split("/").pop(), path: ROOT + "/" + s.rel, size: 3.1e6, mtime: s.mtime, kind: "still", form: s.form }));
      }
      return r;
    };
    return { big };
  }
  if (typeof module !== "undefined" && module.exports) module.exports = install;
  else if (typeof window !== "undefined" && window.__TAURI__) install(window);
})();
