// Timeline: a view inside Albums (albumviews.js) of everything in the library in time order, the newest first, as
// Years, Months or Days. A day's header has its date and the places of its items; days in a row at one place read as
// a trip, with its own line above them. What has no date is in one group at the end.
//
// Dates and places are what the page has already (an item's `when` and `geo`, app.js's deriveItems / deriveGeo): no
// clip is read for this view. Days is drawn by what's on screen: each block (a month's line, a trip's, a day with its
// rows of tiles) has a height known from the tiles' fixed size, so only the blocks near the window are in the page.

const TL = { tileW: 168, tileH: 146, gap: 8, pad: 14, dayH: 38, tripH: 34, monthH: 48, over: 700,
  level: ["years", "months", "days"].includes(avRemembered("afl.timeline", "days")) ? avRemembered("afl.timeline", "days") : "days",
  model: null, jump: null, range: "", bound: false };
const TL_MONTHS = ["January", "February", "March", "April", "May", "June", "July", "August", "September", "October", "November", "December"];

ALBUM_VIEWS.push(["timeline", "Timeline", drawTimeline, avEverything]);

function tlDayKey(d) { return d.getFullYear() + "-" + String(d.getMonth() + 1).padStart(2, "0") + "-" + String(d.getDate()).padStart(2, "0"); }
function tlDayText(d) { return d.getDate() + " " + MONTHS[d.getMonth()] + " " + d.getFullYear(); }
// "10–16 Oct 2026", "28 Sep – 3 Oct 2026", "30 Dec 2025 – 2 Jan 2026"
function tlSpanText(a, b) {
  if (a.getFullYear() !== b.getFullYear()) return tlDayText(a) + " – " + tlDayText(b);
  if (a.getMonth() !== b.getMonth()) return a.getDate() + " " + MONTHS[a.getMonth()] + " – " + tlDayText(b);
  return a.getDate() + "–" + tlDayText(b);
}
// The library by time: days (the newest first), each with its items (the earliest first) and places; the trips; the
// months and years; and what has no date. Pure: from the items alone
function tlModel(list) {
  const dated = [], undated = [];
  for (const it of list) { if (it.when && !isNaN(it.when)) dated.push(it); else undated.push(it); }
  dated.sort((a, b) => a.when - b.when);
  const days = [];
  let cur = null;
  for (const it of dated) {
    const k = tlDayKey(it.when);
    if (!cur || cur.key !== k) { cur = { key: k, date: new Date(it.when.getFullYear(), it.when.getMonth(), it.when.getDate()), items: [], trip: null }; days.push(cur); }
    cur.items.push(it);
  }
  for (const d of days) {
    d.places = avPlaces(d.items);
    const r = avPlaces(d.items, avRegion);
    d.region = r.length ? r[0][0] : null;
  }
  // Trips: days one after another (a day without anything between them allowed) at one wider place, two days or more.
  // A day whose place isn't known goes with the days around it
  const trips = [];
  let run = null;
  const close = () => { if (run && run.days.length > 1 && run.region) { const t = { days: run.days, region: run.region }; trips.push(t); for (const d of run.days) d.trip = t; } run = null; };
  for (const d of days) {
    const near = run && (d.date - run.days[run.days.length - 1].date) <= 2 * 86400e3 + 3600e3;
    if (run && near && (!d.region || !run.region || d.region === run.region)) { run.days.push(d); if (!run.region) run.region = d.region; }
    else { close(); run = { days: [d], region: d.region }; }
  }
  close();
  for (const t of trips) {
    // A trip ends with its last day that has a place (a day after it with none isn't the trip's)
    while (t.days.length && !t.days[t.days.length - 1].region) t.days.pop().trip = null;
    t.items = t.days.reduce((n, d) => n + d.items.length, 0);
    t.places = avPlaces([].concat(...t.days.map((d) => d.items)));
    t.from = t.days[0].date; t.to = t.days[t.days.length - 1].date;
    if (t.days.length < 2) { for (const d of t.days) d.trip = null; t.dead = true; }
  }
  days.reverse();
  const months = [], years = [];
  for (const d of days) {
    const mk = d.key.slice(0, 7), yk = d.key.slice(0, 4);
    let m = months[months.length - 1];
    if (!m || m.key !== mk) { m = { key: mk, year: +yk, month: d.date.getMonth(), days: [], n: 0 }; months.push(m); }
    m.days.push(d); m.n += d.items.length;
    let y = years[years.length - 1];
    if (!y || y.key !== yk) { y = { key: yk, months: [], n: 0 }; years.push(y); }
    if (y.months[y.months.length - 1] !== m) y.months.push(m);
    y.n += d.items.length;
  }
  const cover = (ds) => { for (const d of ds) for (const it of d.items) if (it.thumbPath) return it.thumbPath; return null; };
  for (const m of months) { m.places = avPlaces([].concat(...m.days.map((d) => d.items))); m.cover = cover(m.days); }
  for (const y of years) { y.places = avPlaces([].concat(...y.months.map((m) => [].concat(...m.days.map((d) => d.items))))); y.cover = cover([].concat(...y.months.map((m) => m.days))); }
  return { days, months, years, trips: trips.filter((t) => !t.dead), undated, n: list.length };
}
// Days as blocks from the top down, each with its place: { type, top, h, … }. `cols`: tiles in a row
function tlBlocks(model, cols) {
  const blocks = [];
  let top = 0;
  const push = (b) => { b.top = top; top += b.h; blocks.push(b); };
  const rowsH = (n) => Math.ceil(n / cols) * (TL.tileH + TL.gap);
  for (const m of model.months) {
    push({ type: "month", h: TL.monthH, id: "m:" + m.key, title: TL_MONTHS[m.month] + " " + m.year, n: m.n });
    for (const d of m.days) {
      const t = d.trip;
      if (t && t.days[t.days.length - 1] === d) push({ type: "trip", h: TL.tripH, trip: t });
      push({ type: "day", h: TL.dayH + rowsH(d.items.length), day: d, id: "d:" + d.key });
    }
  }
  if (model.undated.length) {
    push({ type: "month", h: TL.monthH, id: "m:none", title: "Without a date", n: model.undated.length });
    push({ type: "day", h: TL.dayH + rowsH(model.undated.length), day: { key: "none", items: model.undated, places: [], trip: null }, id: "d:none" });
  }
  return { blocks, height: top };
}
function tlCols(main) {
  const w = main.clientWidth || 780;
  return Math.max(1, Math.floor((w - 2 * TL.pad + TL.gap) / (TL.tileW + TL.gap)));
}
function tlTile(it) {
  const t = it.when && !isNaN(it.when) ? String(it.when.getHours()).padStart(2, "0") + ":" + String(it.when.getMinutes()).padStart(2, "0") : "";
  const what = it.kind === "clip" ? (it.cam || "") : it.kind === "moment" ? "Moment" : it.kind === "still" ? (it.form || "Photo") : "";
  return avCard(it, " tl", [t, what].filter(Boolean).join(" · "));
}
function tlBlockHtml(b) {
  const at = 'style="top:' + b.top + 'px;height:' + b.h + 'px"';
  if (b.type === "month") return '<div class="tlb tlmonth" ' + at + ' data-b="' + b.id + '"><h3>' + esc(b.title) + '</h3><span class="sub">' + avCount(b.n, "item", "items") + '</span></div>';
  if (b.type === "trip") {
    const t = b.trip;
    return '<div class="tlb tltrip" ' + at + '><b>' + esc(avNames(t.places, 2) || t.region) + '</b><span class="sub">' + esc(tlSpanText(t.from, t.to)) + ' · ' + t.days.length + ' days · ' + avCount(t.items, "item", "items") + '</span></div>';
  }
  const d = b.day;
  const title = d.key === "none" ? "No date known" : tlDayText(d.date) + (d.places.length ? " · " + avNames(d.places, 3) : "");
  const sub = d.key === "none" ? "The camera wrote none, or the file hasn’t been read yet" : avKinds(d.items);
  return '<section class="tlb tlday' + (d.trip ? " intrip" : "") + '" ' + at + ' data-b="' + b.id + '" aria-label="' + esc(title) + '"><header><b>' + esc(title) + '</b><span class="sub">' + esc(sub) + '</span></header>' +
    '<div class="tlgrid">' + d.items.map(tlTile).join("") + '</div></section>';
}
// The blocks near the window, into the page
function tlFill(force) {
  const main = $("main"), body = $("tlbody"), L = TL.layout;
  if (!body || !L) return;
  const y0 = main.scrollTop - body.offsetTop - TL.over, y1 = main.scrollTop - body.offsetTop + (main.clientHeight || 800) + TL.over;
  const bs = L.blocks;
  let lo = 0, hi = bs.length;
  while (lo < hi) { const mid = (lo + hi) >> 1; if (bs[mid].top + bs[mid].h < y0) lo = mid + 1; else hi = mid; }
  let end = lo;
  while (end < bs.length && bs[end].top < y1) end++;
  const range = lo + "-" + end;
  if (!force && range === TL.range) return;
  TL.range = range;
  let h = "";
  for (let i = lo; i < end; i++) h += tlBlockHtml(bs[i]);
  body.innerHTML = h;
  fillThumbs();
}
function tlCover(a, v, cover, title, sub) {
  return '<button type="button" class="card tlcover" data-a="' + a + '" data-v="' + esc(v) + '"><span class="th">' + (cover ? img(cover) : "") + '</span><span class="nm">' + esc(title) + '</span><span class="sub">' + esc(sub) + '</span></button>';
}
function drawTimeline(main) {
  const v = albumViewOpen(), c = ctx();
  const all = avEverything(), list = avShown(all);
  const sig = S.q + "\u0000" + Object.keys(S.filters).filter((k) => S.filters[k]).sort().join(",");
  if (!TL.model || TL.model.src !== all || TL.model.sig !== sig) { TL.model = tlModel(list); TL.model.src = all; TL.model.sig = sig; }
  const m = TL.model;
  const ready = !!DATA.sources;
  VISIBLE = TL.level === "days" ? [].concat(...m.days.map((d) => d.items.map(keyOf)), m.undated.map(keyOf)) : [];
  let h = avCrumbs(v, null, "Every original, moment and photo by when it was recorded, the newest first");
  h += '<div class="selbar"><div class="seg" role="tablist" aria-label="Timeline by">' + [["years", "Years"], ["months", "Months"], ["days", "Days"]].map((x) =>
    '<button type="button" role="tab" aria-selected="' + (TL.level === x[0]) + '" class="' + (TL.level === x[0] ? "on" : "") + '" data-a="tlevel" data-v="' + x[0] + '">' + x[1] + '</button>').join("") + '</div>' +
    '<span class="sub" style="display:inline">' + avCount(m.n, "item", "items") + (list.length !== all.length ? " of " + all.length.toLocaleString("en-US") + " match" : "") + (m.trips.length ? " · " + avCount(m.trips.length, "trip", "trips") : "") + '</span>' +
    (TL.level === "days" && m.n ? avSelBar(m.n, c) : "") + '</div>';
  if (probesPending) h += '<div class="indexing">Reading what the clips are: ' + probesPending + ' to go. A clip gets its day once it’s read.</div>';
  const st = main.scrollTop;
  if (!ready) { main.innerHTML = h + '<div class="empty"><span class="spin" aria-hidden="true"></span> Scanning the project’s files…</div>'; return; }
  if (!m.n) { main.innerHTML = h + '<div class="empty">' + (all.length ? "Nothing matches. Remove a filter or the search." : "Nothing in the library yet.") + '</div>'; return; }
  if (TL.level === "years") {
    h += '<div class="grid tlcovers">' + m.years.map((y) => tlCover("tlgo", "y:" + y.key, y.cover, y.key, avCount(y.n, "item", "items") + (y.places.length ? " · " + avNames(y.places, 3) : ""))).join("") +
      (m.undated.length ? tlCover("tlgo", "m:none", (m.undated.find((it) => it.thumbPath) || {}).thumbPath, "Without a date", avCount(m.undated.length, "item", "items")) : "") + '</div>';
    main.innerHTML = h; main.scrollTop = st; fillThumbs(); return;
  }
  if (TL.level === "months") {
    for (const y of m.years) {
      h += '<div class="tlyear" id="tly-' + y.key + '"><h3>' + y.key + '</h3><span class="sub">' + avCount(y.n, "item", "items") + '</span></div><div class="grid tlcovers">' +
        y.months.map((mo) => tlCover("tlgo", "m:" + mo.key, mo.cover, TL_MONTHS[mo.month], avCount(mo.n, "item", "items") + (mo.places.length ? " · " + avNames(mo.places, 2) : ""))).join("") + '</div>';
    }
    if (m.undated.length) h += '<div class="tlyear"><h3>Without a date</h3></div><div class="grid tlcovers">' + tlCover("tlgo", "m:none", (m.undated.find((it) => it.thumbPath) || {}).thumbPath, "No date known", avCount(m.undated.length, "item", "items")) + '</div>';
    main.innerHTML = h; main.scrollTop = st; fillThumbs();
    if (TL.jump) { const el = $("tly-" + TL.jump.slice(2)); TL.jump = null; if (el) main.scrollTop = el.offsetTop - 4; }
    return;
  }
  TL.layout = tlBlocks(m, tlCols(main));
  main.innerHTML = h + '<div id="tlbody" class="tlbody" style="height:' + TL.layout.height + 'px"></div>';
  main.scrollTop = st;
  if (TL.jump) {
    const b = TL.layout.blocks.find((x) => x.id === TL.jump);
    TL.jump = null;
    if (b) main.scrollTop = $("tlbody").offsetTop + b.top;
  }
  tlFill(true);
  if (!TL.bound) {
    TL.bound = true;
    let waiting = false;
    main.addEventListener("scroll", () => {
      if (waiting || !$("tlbody")) return;
      waiting = true;
      (window.requestAnimationFrame || setTimeout)(() => { waiting = false; tlFill(false); });
    }, { passive: true });
    window.addEventListener("resize", () => { if ($("tlbody")) renderMain(); });
  }
}
// The view's own buttons (albumviews.js's click handler): true when taken
function timelineClick(a, v) {
  if (a === "tlevel") { TL.level = v; remember("afl.timeline", v); S.sel = {}; S.anchor = null; renderAll(); $("main").scrollTop = 0; return true; }
  if (a === "tlgo") {
    TL.level = v.charAt(0) === "y" ? "months" : "days";
    TL.jump = v;
    remember("afl.timeline", TL.level);
    $("main").scrollTop = 0;
    renderAll();
    return true;
  }
  return false;
}
