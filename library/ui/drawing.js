"use strict";
// Drawing: the top, the grid, the pictures, the filters, the side panel and its parts.
// One of the window's scripts that were app.js: index.html loads them in this order before app.js, as classic
// scripts sharing their top-level names, so each uses the others' when called (only earlier ones at load).

// ---------- drawing ----------

function renderTop() {
  const recent = S.recent.filter((r) => r.path !== S.root);
  $("top").innerHTML =
    '<div class="menuwrap"><button class="proj" type="button" data-a="menu" aria-haspopup="menu" aria-expanded="' + S.menu + '">' + ICON.folder + '<span class="ttl"><span>' + esc(S.root ? S.name : "Action Footage Library") + '</span></span>' + ICON.down + '</button>' +
    (S.menu ? '<div class="menu" role="menu">' +
      (S.root ? '<button type="button" role="menuitem" class="cur" data-a="closemenu"><span class="ck">' + ICON.check + '</span><span style="min-width:0"><span style="display:block">' + esc(S.name) + '</span><span class="sub">' + esc(S.root) + '</span></span></button>' : '') +
      recent.map((r) => recentRow(r, true)).join("") +
      '<hr><button type="button" role="menuitem" data-a="choose"><span class="ck"></span>Open a project…</button>' +
      startMenu() +
      (S.root ? '<hr><button type="button" role="menuitem" data-a="sheet" data-v="settings"><span class="ck"></span>Project settings…</button>' +
        '<button type="button" role="menuitem" data-a="takein"><span class="ck"></span>Take in another project…</button>' +
        '<button type="button" role="menuitem" data-a="moveproject"><span class="ck"></span>Move the project to…</button>' +
        '<button type="button" role="menuitem" data-a="exportjson"><span class="ck"></span>Export the library as JSON…</button>' +
        '<button type="button" role="menuitem" data-a="revealroot"><span class="ck"></span>' + (S.macos ? "Show in Finder" : "Show in Explorer") + '</button>' : '') +
      '</div>' : '') + '</div>' +
    (S.root ? '<div class="seg" role="tablist" aria-label="What to browse">' + VIEWS.map((v) => '<button type="button" role="tab" aria-selected="' + (S.view === v[0]) + '" class="' + (S.view === v[0] ? "on" : "") + '" data-a="view" data-v="' + v[0] + '">' + v[1] + '</button>').join("") + '</div>' +
      '<div class="searchwrap"><div class="search">' + ICON.search + activeTokens().map((t) => '<span class="tok"><span>' + esc(t.f.title + ": " + t.labels.join(", ")) + '</span><button type="button" data-a="tokrm" data-v="' + esc(t.f.id) + '" aria-label="Remove filter ' + esc(t.f.title) + '">' + ICON.x + '</button></span>').join("") +
      '<input id="q" type="search" value="' + esc(S.q) + '" placeholder="' + (nActive() ? "Add words or another filter" : "Search names, cameras, dates, tags, comments, or type a filter: dji, log…") + '" aria-label="Search" autocomplete="off" spellcheck="false"></div><div id="sugg"></div></div>' +
      '<button type="button" class="btn' + (S.filtersOpen ? " on" : "") + '" data-a="filters" aria-pressed="' + S.filtersOpen + '">' + ICON.filter + 'Filters' + (nActive() ? " · " + nActive() : "") + '</button>' +
      '<div class="seg" aria-label="Layout"><button type="button" class="' + (S.layout === "list" ? "on" : "") + '" data-a="layout" data-v="list" aria-label="List">' + ICON.list + '</button><button type="button" class="' + (S.layout === "grid" ? "on" : "") + '" data-a="layout" data-v="grid" aria-label="Grid">' + ICON.grid + '</button></div>' : "");
  cullTop();
}

function mosaic(path, filt) {
  const its = under(path).filter((x) => x.thumbPath && (!filt || matches(x))).slice(0, 4);
  let h = '<span class="mosaic">';
  for (let i = 0; i < 4; i++) h += its[i] ? '<i>' + itemImg(its[i]) + '</i>' : '<i></i>';
  return h + '<span class="fb">' + ICON.fsm + '</span></span>';
}
function img(path) {
  const t = THUMBS.get(path);
  return '<img data-thumb="' + esc(path) + '" alt=""' + (t ? ' src="' + t + '"' : (t === null ? ' hidden' : '')) + '>';
}

// The bars above the list stay at the top of #main while it scrolls (CSS: .mbar is position: sticky); what sticks
// below them (Timeline's day headers) is put under their height, --mbar-h, measured here and again when they wrap
let PIN_OBS = null;
function pinBar(main) {
  const set = () => { const b = main.querySelector(":scope > .mbar"); main.style.setProperty("--mbar-h", (b ? b.offsetHeight : 0) + "px"); };
  set();
  if (!main.dataset.pinned) { // a line under the bar once the list is under it
    main.dataset.pinned = "1";
    main.addEventListener("scroll", () => main.classList.toggle("scrolled", main.scrollTop > 0), { passive: true });
  }
  main.classList.toggle("scrolled", main.scrollTop > 0);
  if (typeof ResizeObserver === "function") {
    if (!PIN_OBS) PIN_OBS = new ResizeObserver(set);
    PIN_OBS.disconnect();
    const b = main.querySelector(":scope > .mbar");
    if (b) PIN_OBS.observe(b);
  }
}
function renderMain() {
  const main = $("main");
  if (!S.root) { main.innerHTML = ""; return; }
  noteHistory();
  if (albumViewDraw(main)) { pinBar(main); return; } // albumviews.js: Timeline, Map
  // A media-type album (Videos, Moments, Photos: albumviews.js) is drawn as an album, of every item of its kind; My
  // albums as the folders of the user's albums (dirKey() is "" there)
  const av = albumViewOpen(), mine = albumMineOpen(), media = av && !mine ? av : null;
  const c = ctx(), srch = searching(), filt = filtering();
  const dir = dirKey();
  // Albums' top: My albums, Timeline, Map and the media types as rows of the list, then Generated and Other folders
  // (albumviews.js); the user's albums are in My albums
  const atTop = S.view === "footage" && !S.path.length && !srch;
  let hiddenFolders = 0;
  const folderRows = media ? [] : childDirs(dir).filter((d) => !(atTop || mine) || (mine ? userAlbum(d) : !userAlbum(d))).map((d) => ({ kind: "folder", path: d, name: d.split("/").pop(), dir }));
  const here = media ? items().slice() : folderRows.concat(atTop ? [] : items().filter((it) => it.dir === dir));
  const list = srch ? scopeItems().filter((it) => matches(it)) : !filt ? here : here.filter((it) => {
    if (it.kind !== "folder") return matches(it);
    const ok = under(it.path).some((x) => matches(x));
    if (!ok) hiddenFolders++;
    return ok;
  });
  cullSort(list);
  // The bars above the list (breadcrumbs, the search's or the filters' line, the selection's bar, the list's head):
  // one block pinned to the top while the list scrolls (pinBar); what only informs comes after it, and scrolls
  let bar = "", h = "";
  if (srch) {
    bar += '<div class="scope"><span class="sub" style="display:inline">Search in</span>' +
      (S.path.length ? '<div class="seg"><button type="button" class="' + (S.scope === "here" ? "on" : "") + '" data-a="scope" data-v="here">' + esc(c.folderName) + '</button><button type="button" class="' + (S.scope === "all" ? "on" : "") + '" data-a="scope" data-v="all">All of ' + c.viewName + '</button></div>' : '<b>All of ' + c.viewName + '</b>') +
      '<span class="sub" style="display:inline">' + list.length + ' found' + (S.q.trim() ? "" : " by the filters") + '</span><button type="button" class="btn sm" data-a="endsearch" style="margin-left:auto">Back to the folder</button></div>';
  } else if (S.view === "clip") {
    const cl = DATA.clip && DATA.clip.clip;
    const dirs = cl && cl.dir ? cl.dir.split("/") : [];
    bar += '<div class="crumbs">' + navButtons() + '<button type="button" data-a="fcrumb" data-v="">Originals</button>';
    dirs.forEach((p, i) => { bar += '<span aria-hidden="true">›</span><button type="button" data-a="fcrumb" data-v="' + esc(dirs.slice(0, i + 1).join("/")) + '">' + esc(p) + '</button>'; });
    bar += '<span aria-hidden="true">›</span><button type="button" class="here" data-a="gofootage" data-v="' + esc(S.clip) + '" title="Go to the clip in Originals">' + esc(clipName()) + '</button>' +
      '<span class="where">Its marks, moments and photos, in the order they are in the clip</span></div>';
  } else {
    bar += '<div class="crumbs">' + navButtons() + '<button type="button" class="' + (S.path.length ? "" : "here") + '" data-a="crumb" data-v="0">' + c.viewName + '</button>';
    // An album of the user's: in My albums (a place in the page: the album's path is its own)
    if (S.view === "footage" && !av && S.path.length && userAlbum(S.path[0])) bar += '<span aria-hidden="true">›</span><button type="button" data-a="aview" data-v="mine">My albums</button>';
    S.path.forEach((p, i) => { bar += '<span aria-hidden="true">›</span><button type="button" class="' + (i === S.path.length - 1 ? "here" : "") + '" data-a="crumb" data-v="' + (i + 1) + '">' + esc(p) + '</button>'; });
    bar += '<span class="where">' + esc(av ? albumViewWhere(av) : atTop ? "Your albums, the whole library by day and by place, each kind of item, and the folders new items land in" : S.view === "footage" ? (S.path.length ? "Links to the originals: opening one plays its original" : "Your albums: folders of links to the originals. New ones are made from a selection in Originals") : S.view === "sources" ? (S.path.length ? "The originals at " + S.path.join("/") : "The originals in the project’s tree (where each is: Sources settings)") : S.root + "/" + BASE[S.view].replace(/\/$/, "")) + '</span></div>';
    if (S.view === "sources" && !S.path.length) bar += '<div class="selbar"><button type="button" class="btn sm" data-a="addsource">Add a source…</button><button type="button" class="btn sm" data-a="sheet" data-v="settings">Sources settings…</button></div>';
    if (filt) {
      const all = under(dir).filter((x) => matches(x)).length;
      bar += '<div class="scope"><span class="chip fast">Filtered</span><span class="sub" style="display:inline">' + list.length + ' of ' + here.length + ' here' + (hiddenFolders ? " · " + hiddenFolders + " folder" + (hiddenFolders > 1 ? "s" : "") + " with no match hidden" : "") + '</span>' +
        '<button type="button" class="btn sm" data-a="flat" style="margin-left:auto">List every match below here (' + all + ')</button></div>';
    }
  }
  const scanView = S.view === "sources" ? "footage" : S.view;
  if (S.scanning[scanView] && S.cachedViews[scanView]) h += '<div class="indexing"><span class="spin" aria-hidden="true"></span> Looking at the disk again: what was there last time shows meanwhile</div>';
  if (S.missing > 0 || S.mending) h += '<div class="indexing">' + (S.mending ? '<span class="spin" aria-hidden="true"></span>Looking for the moved originals and mending the links…'
    : S.missing + (S.missing === 1 ? " moment, album link, export or photo points" : " moments, album links, exports and photos point") + ' at an original that isn’t where it was. <button type="button" class="btn sm" data-a="showbroken">Show</button> <button type="button" class="btn sm pri" data-a="mend">Find and mend</button>' +
      (S.showBrokenList ? '<div class="sub wrap" style="margin-top:6px">' + (S.missingList || []).map((x) => '<div><code>' + esc(x.file.replace(S.root + "/", "")) + '</code> → ' + esc(x.tree || "?") + ' <button type="button" class="btn sm" data-a="revealpath" data-v="' + esc(x.file) + '">' + (S.macos ? "Show in Finder" : "Show in Explorer") + '</button></div>').join("") + '</div>' : "")) + '</div>';
  // Locations not reachable now: their clips as last seen, marked offline; a share connected from here (the Mac)
  if (S.view === "footage" || S.view === "sources") {
    for (const l of S.locations.filter((x) => x.offline)) {
      const busy = S.connecting[l.path];
      h += '<div class="indexing">' + (busy ? '<span class="spin" aria-hidden="true"></span>' : '') + '<b>' + esc(l.name || l.path) + '</b> (' + esc(l.path) + ') is offline: its clips show as they were last seen, and play once it’s connected.' +
        (l.connect ? ' ' + connectButton("connectloc", l.path, busy, l.names) : "") + '</div>';
    }
  }
  if (probesPending && (S.view === "footage" || S.view === "sources")) h += '<div class="indexing">Reading what the clips are: ' + probesPending + ' to go. Filters fill in as they come.</div>';
  if (!DATA[S.view]) { main.innerHTML = '<div class="mbar">' + bar + '</div>' + h + '<div class="empty"><span class="spin" aria-hidden="true"></span> Scanning the project’s files…</div>'; VISIBLE = []; return; }
  VISIBLE = list.map(keyOf);
  const nSel = VISIBLE.filter((k) => S.sel[k]).length;
  if (list.length) bar += '<div class="selbar"><span class="sub" style="display:inline">' + (nSel ? nSel + " of " + list.length + " selected" : "None selected") + '</span><button type="button" class="btn sm" data-a="selall"' + (nSel === list.length ? " disabled" : "") + '>Select all</button><button type="button" class="btn sm" data-a="desel"' + (nSel ? "" : " disabled") + '>Select none</button>' + albumBar(c) + shareBar(c) + '</div>'; // albums.js, share.js
  const thumb = (it) => it.kind === "folder" ? mosaic(it.path, filt) : it.kind === "mark" ? '<span class="th mk" aria-hidden="true">' + ICON_MARK + '</span>' : '<span class="th">' + itemImg(it) + (it.dur ? '<span class="d">' + esc(it.dur) + '</span>' : "") + '</span>';
  const folderSub = (it) => { const all = under(it.path); const m = filt ? all.filter((x) => matches(x)).length : all.length; return filt && !srch ? m + " of " + all.length + " match" : plural(all.length); };
  const subOf = (it) => it.kind === "folder" ? folderSub(it) : it.sub + (srch ? " · in " + (it.dir || "the top folder") : "");
  const top = atTop ? albumTopRows(S.layout, filt) : null; // albumviews.js
  if (!list.length && !top) {
    if (S.view === "clip" && !srch && !filt) h += '<div class="empty">No marks, moments or photos of this clip yet. Mark it in the player (M), then make Moments or Photos of the marks.</div>'; else
    h += '<div class="empty">' + (srch ? "Nothing found. Search all of " + c.viewName + ", or remove a filter." : filt ? "Nothing in this folder matches. List every match below here, or remove a filter." : S.view === "footage" && !S.path.length ? "No albums yet. Select clips in Originals and choose Add to an album…" : S.view === "footage" ? "This album is empty. Select clips in Originals and choose Add to an album…" : "This folder is empty.") + '</div>';
  } else if (S.layout === "list") {
    const clipsV = S.view === "footage" || S.view === "sources";
    const cnt = clipsV ? "Marks · mom." : "";
    if (!top) bar += '<div class="head" aria-hidden="true"><span></span><span>Name</span><span class="hide-s">' + (clipsV ? "Recorded" : "Date") + '</span><span class="hide-s">Length</span><span class="hide-s">' + cnt + '</span><span class="hide-s">Status</span><span>' + (S.locations.length > 1 && S.view === "footage" ? "Plays from" : "Size") + '</span></div>';
    h += '<div class="list">' + (top || "");
    list.forEach((it, i) => {
      if (top) h += albumTopHead(list, i);
      const key = keyOf(it), sel = !!S.sel[key], f = it.kind === "folder";
      h += '<button type="button" class="row' + (sel ? " sel" : "") + (it.offline ? " off" : "") + (it.pending ? " pending" : "") + (it.pending ? '" title="Being written…' : cullNoteTip(it)) + '" data-a="pick" data-v="' + esc(key) + '" aria-pressed="' + sel + '">' + thumb(it) +
        '<span style="min-width:0">' + (S.renaming === key ? '<input id="rename" class="renamer" type="text" value="' + esc(renameStem(it)) + '" aria-label="New name">' : '<span class="nm">' + esc(it.name) + '</span>') + '<span class="sub">' + esc(subOf(it)) + '</span></span>' +
        '<span class="cell hide-s">' + (f ? "" : esc(it.date)) + '</span><span class="cell hide-s">' + (f ? "" : esc(it.dur)) + '</span>' +
        '<span class="cell hide-s">' + (it.kind === "clip" ? (it.marks ? it.marks + " · " + (it.moments || 0) : "—") : "") + '</span>' +
        '<span class="chips hide-s">' + chips(it) + '</span><span class="cell">' + (f ? "" : (S.locations.length > 1 && it.kind === "clip" ? esc(it.loc) : sizeText(it.size))) + '</span></button>';
    });
    h += '</div>';
  } else {
    h += '<div class="grid">' + (top || "");
    TILE.w = tileWidth(main); // tiles.js: the albums' text fitted to the columns
    tileWatch(main);
    list.forEach((it, i) => {
      if (top) h += albumTopHead(list, i);
      const key = keyOf(it), sel = !!S.sel[key];
      if (it.kind !== "folder" && it.kind !== "mark") {
        h += itemTile(it, key, { sel, w: TILE.w, renaming: S.renaming === key ? '<input id="rename" class="renamer" type="text" value="' + esc(renameStem(it)) + '" aria-label="New name">' : "" });
        return;
      }
      h += '<button type="button" class="card' + (sel ? " sel" : "") + (it.offline ? " off" : "") + (it.pending ? " pending" : "") + (it.pending ? '" title="Being written…' : cullNoteTip(it)) + '" data-a="pick" data-v="' + esc(key) + '" aria-pressed="' + sel + '">' + thumb(it) +
        (S.renaming === key ? '<input id="rename" class="renamer" type="text" value="' + esc(renameStem(it)) + '" aria-label="New name">' : '<span class="nm">' + esc(it.name) + '</span>') + '<span class="sub">' + esc(subOf(it)) + '</span><span class="chips">' + chips(it) + '</span></button>';
    });
    h += '</div>';
  }
  if (top && !list.length) h += '<div class="empty">' + (filt ? "No album matches. Remove a filter." : "Generated and Other folders are empty.") + '</div>';
  const scrollTop = main.scrollTop;
  main.innerHTML = '<div class="mbar">' + bar + '</div>' + h;
  main.scrollTop = scrollTop;
  pinBar(main);
  fillThumbs();
  const ren = $("rename");
  if (ren) { ren.focus(); ren.select(); }
}
function sizeText(n) {
  if (!(n > 0)) return "";
  const u = ["B", "KB", "MB", "GB", "TB"]; let i = 0;
  while (n >= 1000 && i < u.length - 1) { n /= 1000; i++; }
  return (n >= 10 || i === 0 ? Math.round(n) : n.toFixed(1)) + " " + u[i];
}
function chips(it) {
  const c = [];
  if (it.pick === "accept") c.push(["chip ok", "✓ Accepted"]);
  if (it.pick === "reject") c.push(["chip warn", "✕ Rejected"]);
  if (it.rating) c.push(["chip tag", stars(it.rating)]);
  for (const t of it.tags || []) c.push([t === "Deleted in Photos" ? "chip sys" : "chip tag", t]);
  if (it.kind === "folder") {
    // An album or a folder: its count is under its name
  } else if (it.kind === "clip") {
    if (it.offline) c.push(["chip warn", "Offline"]);
    if (it.conflict) c.push(["chip warn", it.own_conflict.length + " own files"]);
    if ((it.locs || []).length > 1) c.push(["chip fast", "Also in " + it.locs.slice(1).join(", ")]);
    if (S.view === "sources" && it.albums) c.push(["chip tag", it.albums === 1 ? "In an album" : "In " + it.albums + " albums"]);
    if (it.gyroflow) c.push(["chip ok", "Gyroflow project"]);
    if (it.motion === false) c.push(["chip", it.camStab ? "Stabilized in the camera" : "No motion data"]);
    if (it.is360) c.push(["chip fast", "360°"]);
  } else if (it.kind === "moment") {
    for (const p of ["HDR", "SDR"]) { const st = (it.renders || {})[p]; if (st) c.push(["chip " + (st === "current" ? "ok" : "stale"), p + (st === "current" ? "" : " stale")]); }
    if (it.gyroflow) c.push(["chip ok", "Own Gyroflow project"]);
    if (!it.link) c.push(["chip warn", "No link"]);
  } else if (it.kind === "mark") {
    if (!it.hasMoment) c.push(["chip", "No moment"]);
    if (!it.hasPhoto) c.push(["chip", "No photo"]);
  } else if (it.kind === "still") {
    c.push(["chip tag", it.form]);
  } else if (it.kind === "export") {
    c.push(["chip tag", it.look]);
  }
  return c.map((x) => '<span class="' + x[0] + '">' + esc(x[1]) + '</span>').join("");
}

// The pictures: from the app, a few at a time, each set where it shows once it's there; what's on screen first
let thumbsBusy = 0;
const thumbsWanted = [];
const thumbsAsked = new Set(); // asked, answer still to come
function fillThumbs() {
  for (const el of document.querySelectorAll("img[data-thumb]")) {
    const p = el.getAttribute("data-thumb");
    if (THUMBS.has(p)) continue;
    if (!thumbsAsked.has(p) && thumbsWanted.indexOf(p) < 0) thumbsWanted.push(p);
  }
  pumpThumbs();
}
// The next to ask: the first wanted one on screen (or just below or above it), else the first wanted
function nextThumb() {
  const want = new Set(thumbsWanted);
  const h = window.innerHeight;
  for (const el of document.querySelectorAll("img[data-thumb]")) {
    const p = el.getAttribute("data-thumb");
    if (!want.has(p)) continue;
    const r = (el.hidden ? el.parentElement : el).getBoundingClientRect();
    if (r.bottom > -300 && r.top < h + 300 && (r.width || r.height)) { thumbsWanted.splice(thumbsWanted.indexOf(p), 1); return p; }
  }
  return thumbsWanted.shift();
}
let thumbsWaitWrites = false;
function thumbsAfterWrites() {
  if (thumbsWaitWrites) return;
  thumbsWaitWrites = true;
  Writes.drained().then(() => { thumbsWaitWrites = false; fillThumbs(); });
}
function pumpThumbs() {
  while (thumbsBusy < 6 && thumbsWanted.length) {
    const p = nextThumb();
    if (THUMBS.has(p) || thumbsAsked.has(p)) continue;
    thumbsBusy++;
    thumbsAsked.add(p);
    invoke("thumb", { path: p }).then((u) => u || null, () => null).then((u) => {
      // None while a change is still being written: perhaps a new name the disk hasn't got yet (a renamed folder's
      // file). Not kept as none: asked again once the changes are written
      if (!u && typeof Writes === "object" && Writes.pending()) { thumbsAfterWrites(); return; }
      THUMBS.set(p, u);
      for (const el of document.querySelectorAll('img[data-thumb="' + CSS.escape(p) + '"]')) { if (u) { el.src = u; el.hidden = false; } else el.hidden = true; }
    }).finally(() => { thumbsBusy--; thumbsAsked.delete(p); pumpThumbs(); });
  }
}

function optRow(id, label, count, checked, ind, isParent, exp, isKid, onNames) {
  const cid = "fo-" + id.replace(/[^a-z0-9]/gi, "_");
  return '<div class="opt' + (isKid ? " kid" : "") + (count ? "" : " zero") + '" data-o="' + esc(id) + '">' +
    (isParent ? '<button type="button" class="tw" data-a="fexp" data-v="' + esc(id) + '" aria-expanded="' + exp + '" aria-label="' + (exp ? "Collapse " : "Expand ") + esc(label) + '">' + ICON.chev + '</button>' : "<span></span>") +
    '<input type="checkbox" id="' + cid + '" data-a="fopt" data-v="' + esc(id) + '"' + (checked ? " checked" : "") + (ind ? ' data-ind="1"' : "") + '>' +
    '<label for="' + cid + '"><span class="lb">' + esc(label) + (onNames ? '<span class="onv" title="' + esc(onNames) + '">' + esc(onNames) + '</span>' : "") + '</span></label><span class="ct">' + count + '</span></div>';
}
// The panel holds still while it's used: which rows show, in which order, is decided when it opens (or the view, the
// folder or the search changes) and kept while things are ticked: a row whose count falls to 0 stays, greyed, the
// counts change in place. A long list (more than FMORE_AT rows) is ordered by its counts then, shows its first FTOP
// rows and "N more", and a box to find a value in it. Nested lists (Region in a country, a place in a region)
// are collapsed until opened (S.fexp: kept while the window is open); a collapsed one says what's ticked in it.
const FTOP = 6, FMORE_AT = 8;
// What the layout is made for: the view, the folder, the search, and the items themselves (a scan or the clips' probes
// coming in lay it out again; a tick doesn't)
const ITEMS_ID = new WeakMap();
let itemsIds = 0;
function itemsId(list) { if (!ITEMS_ID.has(list)) ITEMS_ID.set(list, ++itemsIds); return ITEMS_ID.get(list); }
function filtersKey() { return [S.view, dirKey(), S.q.trim(), S.scope, S.flat, probesVersion, itemsId(items())].join("\u0000"); }
function renderFilters() {
  const el = $("fpanel");
  el.hidden = !S.root || !S.filtersOpen;
  if (el.hidden) { el.innerHTML = ""; S.ffreeze = null; return; }
  const srch = searching();
  const base = srch ? scopeItems().filter(textMatch) : under(dirKey());
  const where = srch ? (S.scope === "all" || !S.path.length ? "all of " + ctx().viewName : S.path[S.path.length - 1] + " and below") : (S.path.length ? S.path[S.path.length - 1] + " and its folders" : "all of " + ctx().viewName);
  const key = filtersKey();
  // Made again: the rows to show (count > 0 or ticked), each long list's order and its first rows
  const fresh = !S.ffreeze || S.ffreeze.key !== key;
  const fz = fresh ? (S.ffreeze = { key, known: new Set(), shown: new Set(), order: {}, top: {} }) : S.ffreeze;
  let h = '<div class="fh"><b>Filters</b><button type="button" data-a="fclear"' + (nActive() ? "" : " disabled") + '>Clear all</button></div>' +
    '<div class="rule">Ticks in one list: any of them. Different lists: all of them. ' + (srch ? "Applied to the search." : "Applied to the folder you’re in.") + ' Numbers: how many match in ' + esc(where) + '.</div>';
  let hidden = 0;
  for (const sx of FACETS) {
    const open = !!S.fsec[sx.id];
    const n = sx.facets.reduce((a, f) => a + facetLeaves(f).filter((l) => S.filters[l.id]).length, 0);
    let body = "";
    for (const f of sx.facets) {
      const others = base.filter((it) => matches(it, f));
      const cnt = (fns) => others.filter((it) => fns.some((fn) => fn(it))).length;
      // Whether a row is shown: as decided when the panel was laid out; one that wasn't there then, by its count
      const show = (id, c) => {
        if (fresh) { fz.known.add(id); if (c > 0 || S.filters[id]) fz.shown.add(id); }
        else if (!fz.known.has(id)) { fz.known.add(id); if (c > 0 || S.filters[id]) fz.shown.add(id); }
        return fz.shown.has(id) || !!S.filters[id];
      };
      const rowsOf = [];
      for (const o of f.opts) {
        if (o.kids) {
          const onKids = o.kids.filter((k) => S.filters[k.id]);
          const kids = o.kids.map((k) => ({ k, c: cnt([k.fn]) })).filter((x) => show(x.k.id, x.c));
          hidden += o.kids.length - kids.length;
          const c = cnt(o.kids.map((k) => k.fn));
          if (!kids.length && !show(o.id, c)) { hidden++; continue; }
          rowsOf.push({ o, c, kids, onKids });
        } else {
          const c = cnt([o.fn]);
          if (!show(o.id, c)) { hidden++; continue; }
          rowsOf.push({ o, c });
        }
      }
      if (!rowsOf.length) continue;
      // A long list: by its counts, the largest first, as it was when laid out; its first rows, then "N more"
      const long = rowsOf.length > FMORE_AT;
      if (long && (fresh || !fz.order[f.id])) {
        // ("No location", "Unknown": last, whatever their counts)
        const odd = (x) => /:(none|\?)$/.test(x.o.id) ? 1 : 0;
        const byCount = rowsOf.slice().sort((x, y) => odd(x) - odd(y) || y.c - x.c);
        fz.order[f.id] = byCount.map((x) => x.o.id);
        fz.top[f.id] = new Set(byCount.filter((x, i) => i < FTOP || S.filters[x.o.id] || (x.onKids && x.onKids.length)).map((x) => x.o.id));
      }
      let rows = rowsOf;
      if (long) {
        const ord = fz.order[f.id], at = (x) => { const i = ord.indexOf(x.o.id); return i < 0 ? ord.length : i; };
        rows = rowsOf.slice().sort((x, y) => at(x) - at(y));
      }
      const find = (S.ffind[f.id] || "").trim().toLowerCase();
      const hit = (t) => !find || String(t).toLowerCase().indexOf(find) >= 0;
      const all = long && S.fmore[f.id];
      let out = "", more = 0;
      for (const x of rows) {
        const kidHits = x.kids ? x.kids.filter((k) => hit(k.k.label)) : null;
        if (find && !hit(x.o.label) && !(kidHits && kidHits.length)) continue;
        if (long && !all && !find && !fz.top[f.id].has(x.o.id)) { more++; continue; }
        if (x.kids) {
          const onN = x.onKids.length, kidsShown = find && !hit(x.o.label) ? kidHits : x.kids;
          const exp = S.fexp[x.o.id] === true || (!!find && kidsShown.length > 0 && !hit(x.o.label));
          out += optRow(x.o.id, x.o.label, x.c, onN === x.o.kids.length, onN > 0 && onN < x.o.kids.length, x.kids.length > 0, exp, false,
            !exp && onN && onN < x.o.kids.length ? x.onKids.map((k) => k.label).join(", ") : "");
          if (exp) kidsShown.forEach((k) => { out += optRow(k.k.id, k.k.label, k.c, !!S.filters[k.k.id], false, false, false, true); });
        } else out += optRow(x.o.id, x.o.label, x.c, !!S.filters[x.o.id], false, false, false, false);
      }
      const findBox = long || find ? '<input type="search" class="ffind" data-f="' + esc(f.id) + '" value="' + esc(S.ffind[f.id] || "") + '" placeholder="Find a ' + esc(f.title.toLowerCase()) + '…" aria-label="Find in ' + esc(f.title) + '" autocomplete="off" spellcheck="false">' : "";
      if (find && !out) out = '<div class="fnone">None called that</div>';
      if (more) out += '<button type="button" class="fmore" data-a="fmore" data-v="' + esc(f.id) + '">' + more + ' more…</button>';
      else if (all && !find) out += '<button type="button" class="fmore" data-a="fmore" data-v="' + esc(f.id) + '">Fewer</button>';
      body += (sx.facets.length > 1 ? '<div class="ftitle">' + esc(f.title) + '</div>' : "") + findBox + out;
    }
    if (!body && !n) continue;
    h += '<div class="fsec"><button type="button" class="fsech" data-a="fsec" data-v="' + sx.id + '" aria-expanded="' + open + '">' + ICON.chev + esc(sx.title) + (n ? '<span class="n">' + n + ' on</span>' : "") + '</button>';
    if (open) h += '<div class="fbody">' + body + '</div>';
    h += '</div>';
  }
  if (hidden) h += '<div class="rule" style="padding-top:10px">' + hidden + " filter" + (hidden > 1 ? "s" : "") + " with nothing to show here hidden.</div>";
  // Drawn again in place: the scroll kept, and the focus where it was (a tick, the box being typed in)
  const st = el.scrollTop, act = document.activeElement;
  const q = (t) => '"' + String(t).replace(/["\\]/g, "\\$&") + '"';
  const focus = act && act !== el && el.contains(act) ? (act.dataset.f ? ".ffind[data-f=" + q(act.dataset.f) + "]" : act.dataset.a && act.dataset.v != null ? "[data-a=" + q(act.dataset.a) + "][data-v=" + q(act.dataset.v) + "]" : null) : null;
  const caret = act && act.classList && act.classList.contains("ffind") ? act.selectionStart : null;
  el.innerHTML = h;
  el.scrollTop = st;
  el.querySelectorAll('input[data-ind="1"]').forEach((i) => { i.indeterminate = true; });
  if (focus) { const n = el.querySelector(focus); if (n) { n.focus({ preventScroll: true }); if (caret != null && n.setSelectionRange) n.setSelectionRange(caret, caret); } }
}
function suggestions() {
  const q = S.q.trim().toLowerCase();
  if (!q) return [];
  const out = [];
  for (const f of allFacets()) for (const o of f.opts) {
    const list = o.kids ? [o].concat(o.kids.map((k) => Object.assign({ parent: o }, k))) : [o];
    for (const x of list) {
      const path = f.title + (x.parent ? " › " + x.parent.label : "");
      const on = x.kids ? x.kids.every((k) => S.filters[k.id]) : S.filters[x.id];
      if ((x.label + " " + path).toLowerCase().indexOf(q) >= 0 && !on) out.push({ id: x.id, label: x.label, path });
    }
  }
  return out.slice(0, 7);
}
function renderSugg() {
  const el = $("sugg");
  if (!el) return;
  const q = S.q.trim();
  if (!S.suggOpen || !q) { el.innerHTML = ""; return; }
  const list = suggestions();
  let h = '<div class="sugg" role="listbox" aria-label="Suggestions">';
  if (list.length) { h += '<div class="sh">Filters</div>'; list.forEach((x, i) => { h += '<button type="button" role="option" class="' + (S.sugg === i ? "hi" : "") + '" data-a="suggpick" data-v="' + esc(x.id) + '"><span>' + esc(x.label) + '</span><span class="path">' + esc(x.path) + '</span></button>'; }); }
  h += '<div class="sh">Words</div><button type="button" role="option" class="' + (S.sugg === list.length ? "hi" : "") + '" data-a="suggtext"><span>Names, cameras and dates with “' + esc(q) + '”</span></button></div>';
  el.innerHTML = h;
}

// What the Library does in the background besides the batches (Anton, 2026-11-10: every task in one place)
function backgroundTasks() {
  const t = [];
  if (S.catalogStatus) t.push(["The project’s catalog", S.catalogStatus]);
  const reading = Object.keys(S.scanning || {}).filter((v) => S.scanning[v]);
  if (reading.length) t.push(["Reading the project", reading.map((v) => ({ footage: "footage", moments: "moments", stills: "photos", exports: "exports" })[v] || v).join(", ")]);
  if (probesPending) t.push(["Reading what the clips are", probesPending + " to go"]);
  if (S.checking) t.push(["Checking", "what you selected"]);
  if (S.finding) t.push(["Finding moved files", "by their names, sizes, fingerprints"]);
  if (S.mending) t.push(["Mending links", "finding the moved originals, writing the links"]);
  if (writesSlow) t.push(["Saving changes", Writes.pending() + " to go"]);
  return t;
}
function queueHtml() {
  const fin = (j) => ["done", "stopped", "failed"].includes(j.state);
  const live = S.jobs.filter((j) => !fin(j)), done = S.jobs.filter(fin).slice().reverse();
  const bg = backgroundTasks();
  let h = '<div class="q">';
  for (const [title, what] of bg) h += '<div class="job"><div class="t"><b><span class="spin" aria-hidden="true"></span>' + esc(title) + '</b></div><span class="sub wrap">' + esc(what) + '</span></div>';
  if (!live.length && !bg.length) h += '<p class="sub wrap">Nothing running. Commands on what you select land here, with what the Library does on its own.</p>';
  for (const j of live) {
    let pct = j.total > 0 ? Math.min(100, ((j.done + (j.fraction || 0)) / j.total) * 100) : 0;
    const what = j.state === "waiting" ? "waiting" : j.state === "starting" ? "starting the player" : j.state === "looking" ? (j.kind === "optical" ? "reading the clips" : "looking through the project") : j.state === "stopping" ? "stopping" : (j.total > 0 ? j.done + " of " + j.total : "");
    const other = j.project !== S.root ? " · " + j.project_name : "";
    h += '<div class="job"><div class="t"><b>' + esc(j.title + other) + '</b><span class="pill">' + esc(j.state) + '</span></div>' +
      (j.state === "waiting" ? "" : '<div class="bar2"><i style="width:' + pct.toFixed(1) + '%"></i></div>') +
      '<div class="t"><span class="sub" style="flex:1">' + esc([what, j.current].filter(Boolean).join(" · ")) + '</span><button type="button" class="btn sm" data-a="cancel" data-v="' + j.id + '">' + (j.state === "waiting" ? "Remove" : "Stop") + '</button></div>' + shareJobDetails(j) + '</div>'; // share.js
  }
  if (done.length) {
    h += '<div class="qh" style="display:flex;align-items:center">Done<button type="button" class="btn sm" data-a="clearjobs" style="margin-left:auto">Clear</button></div>';
    for (const j of done) {
      const parts = [];
      if (j.made != null) parts.push(j.made + (j.share ? " saved" : " made"));
      if (j.failed > 0) parts.push(j.failed + " failed");
      if (j.message) parts.push(j.message);
      const other = j.project !== S.root ? " · " + j.project_name : "";
      h += '<div class="job"><div class="t"><b>' + esc(j.title + other) + '</b><span class="pill">' + esc(j.state) + '</span></div><span class="sub wrap' + (j.state === "failed" ? " err" : "") + '">' + esc(parts.join(" · ")) + '</span>' +
        ((j.failures || []).length && !j.share ? '<details><summary>' + j.failures.length + " failure" + (j.failures.length > 1 ? "s" : "") + '</summary><ul>' + j.failures.map((f) => "<li><code>" + esc(f[0]) + "</code>: " + esc(f[1]) + "</li>").join("") + "</ul></details>" : "") +
        ((j.left_out || []).length ? '<details><summary>' + j.left_out.length + " left out" + '</summary><ul>' + j.left_out.map((f) => "<li><code>" + esc(f[0].split(/[\\/]/).pop()) + "</code>: " + esc(f[1]) + "</li>").join("") + "</ul></details>" : "") +
        ((j.kind === "lens-find" || j.kind === "lens-estimate") ? lensFindResults(j) : "") + shareJobDetails(j) + '</div>';
    }
  }
  h += '<p class="sub wrap">One job at a time, by ActionFootagePlayer without a window. Closing the Library asks first.</p></div>';
  return h;
}
function activeCount() { return S.jobs.filter((j) => !["done", "stopped", "failed"].includes(j.state)).length + backgroundTasks().length; }

// One part of the side panel: its title, and what it acts on
function partHead(title, c, acts) {
  const what = !acts ? "" : c.any ? "On " + c.selName + (c.selFolders.length ? " (" + countText(c.selItems) + ", everything inside included)" : "")
    : "Nothing selected: select " + NOUN[S.view][1] + " or whole folders. Settings can be set beforehand.";
  return '<div class="ph"><h2>' + esc(title) + '</h2>' + (what ? '<span class="sub wrap">' + esc(what) + '</span>' : "") + '</div>';
}
function part(body) { return '<div class="secb">' + body + '</div>'; }
function note(text) { return '<div class="secb"><p class="sub wrap">' + esc(text) + '</p></div>'; }
// One clip: its moments and photos; one moment, photo or export: its clip in Footage
function clipButton(c) {
  if (c.direct.length !== 1 || c.selFolders.length) return "";
  const it = c.direct[0];
  if (it.kind === "clip") return '<button type="button" class="btn sm" data-a="showclip" data-v="' + esc(it.id) + '">Marks, moments and photos</button>';
  if (it.kind === "mark") return '<button type="button" class="btn sm" data-a="gofootage" data-v="' + esc(S.clip) + '">Go to the footage</button>';
  const id = clipOf(it);
  return id ? '<button type="button" class="btn sm" data-a="gofootage" data-v="' + esc(id) + '">Go to the footage</button>' : "";
}

// The side panel's parts, chosen with the column of icons at its right edge, as the player's side panel has them (its
// rail): what's selected and the commands on it at the top, the Library's own work at the bottom. Clicking the part
// shown again leaves the column alone (Anton, 2026-11-11). Opening, selecting and showing in Finder are in the list's
// own menu (a right-click), not here
const ICON_TAB = {
  info: '<circle cx="12" cy="12" r="8.5"/><path d="M12 11v5.5M12 7.6v.1"/>',
  source: '<rect x="3.5" y="5.5" width="17" height="13" rx="2.5"/><path d="M10 9l5 3-5 3z"/>',
  tags: '<path d="M3.5 12.2V4.5h7.7l9 9-7.7 7.7z"/><circle cx="8" cy="9" r="1.3"/>',
  geo: '<path d="M12 21s-6.5-6.2-6.5-11a6.5 6.5 0 0 1 13 0c0 4.8-6.5 11-6.5 11z"/><circle cx="12" cy="10" r="2.3"/>',
  moments: '<rect x="3.5" y="4.5" width="17" height="15" rx="2"/><path d="M7.5 4.5v15M16.5 4.5v15M3.5 9h4M3.5 15h4M16.5 9h4M16.5 15h4"/>',
  share: '<path d="M12 15V3.5M7.5 8L12 3.5 16.5 8M5 12.5V20h14v-7.5"/>',
  queue: '<path d="M9 6.5h11M9 12h11M9 17.5h11M4 5l2.5 1.5L4 8M4 10.5l2.5 1.5L4 13.5M4 16l2.5 1.5L4 19"/>',
  checks: '<path d="M12 3l7.5 3v5.5c0 4.6-3.2 8.2-7.5 9.5-4.3-1.3-7.5-4.9-7.5-9.5V6z"/><path d="M8.5 12l2.5 2.5 4.5-5"/>',
};
const TABS_TOP = [["info", "Info"], ["source", "Source Settings"], ["tags", "Tags"], ["geo", "Location"], ["moments", "Moments & Photos"], ["share", "Share"]];
const TABS_LOW = [["queue", "Tasks"], ["checks", "Checks"]];
const TAB_IDS = TABS_TOP.concat(TABS_LOW).map((t) => t[0]);
// Whether a part has anything for the view (its icon dimmed where it hasn't: the part then says where that is)
function tabFits(id) {
  const clips = S.view === "footage" || S.view === "sources";
  if (id === "source" || id === "geo" || id === "moments") return clips;
  if (id === "share") return S.view === "footage" || S.view === "sources" || S.view === "clip";
  if (id === "checks") return clips;
  return true;
}
function railButton(t) {
  const on = S.sideOpen && S.tab === t[0];
  const n = t[0] === "queue" ? activeCount() : 0;
  return '<button type="button" role="tab" class="rb' + (on ? " on" : "") + (tabFits(t[0]) ? "" : " na") + '" data-a="tab" data-v="' + t[0] + '" aria-selected="' + on + '" aria-label="' + esc(t[1]) + '" title="' + esc(t[1]) + '">' +
    '<svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">' + ICON_TAB[t[0]] + '</svg>' +
    (t[0] === "queue" ? '<span class="count"' + (n ? "" : " hidden") + '>' + n + '</span>' : "") + '</button>';
}
// A part shown (the panel opened if only the column was), remembered on this computer
function showTab(id) {
  S.tab = id; S.sideOpen = true;
  remember("afl.tab", id); remember("afl.side", "1");
}
function sideBody(c) {
  const clipsView = S.view === "footage" || S.view === "sources";
  const title = TABS_TOP.concat(TABS_LOW).find((t) => t[0] === S.tab)[1];
  switch (S.tab) {
    case "info": return partHead(title, c, false) + '<div class="itab">' + infoHtml(c) + '</div>';
    case "queue": return partHead(title, c, false) + '<div id="qtab">' + queueHtml() + '</div>';
    case "source": return partHead(title, c, true) + (clipsView ? sourceBody(c) : note("Source settings are the originals’: how each is read (its turn, its colour, its lens, its motion data). Select clips in Albums or Originals."));
    case "tags": return partHead(title, c, true) + part(tagsBody(c));
    case "geo": return partHead(title, c, true) + (clipsView ? part(locationBody(c)) : note("A location is the original’s: select clips in Albums or Originals."));
    case "moments":
      if (!clipsView) return partHead(title, c, true) + note(S.view === "clip" ? "Moments and photos of this clip’s marks are made on the clip in Originals." : "Moments and photos are made from the clips: select them in Albums or Originals.");
      return partHead(title, c, true) + part(momentsBody(c));
    // Of anything selected: clips, moments, photos, mixed (share.js)
    case "share": return partHead(title, c, true) + shareBody(c);
    case "checks":
      return partHead(title, c, true) + (clipsView ? part(checksBody(c)) : note("Checks look at clips and moments: in Albums or Originals.")) + '<div class="secb whole">' + catalogBody() + '</div>';
  }
  return "";
}
function renderSide() {
  const side = $("side");
  side.hidden = !S.root;
  if (!S.root) return;
  const c = ctx();
  // Stabilize optically is Source settings' now (its motion data's row)
  if (S.tab === "stabilize") S.tab = "source";
  if (TAB_IDS.indexOf(S.tab) < 0) S.tab = "info";
  side.classList.toggle("closed", !S.sideOpen);
  const rail = '<nav class="rail" role="tablist" aria-orientation="vertical" aria-label="Side panel">' + TABS_TOP.map(railButton).join("") + '<span class="sp"></span>' + TABS_LOW.map(railButton).join("") + '</nav>';
  const old = side.querySelector(".pane");
  const keep = old && old.dataset.tab === S.tab ? old.scrollTop : 0;
  // A field of the panel being typed in keeps its focus and caret across the re-drawing (the comment, a new tag)
  const act = document.activeElement, typed = act && act.id && side.contains(act) && /^(INPUT|TEXTAREA)$/.test(act.tagName)
    ? { id: act.id, a: act.selectionStart, b: act.selectionEnd, top: act.scrollTop } : null;
  side.innerHTML = (S.sideOpen ? '<div class="pane" role="tabpanel" data-tab="' + S.tab + '">' + sideBody(c) + '</div>' : "") + rail;
  const pane = side.querySelector(".pane");
  if (!pane) return;
  pane.scrollTop = keep;
  if (typed) { const e = $(typed.id); if (e && !e.disabled) { e.focus(); try { if (typed.a != null) e.setSelectionRange(typed.a, typed.b); e.scrollTop = typed.top; } catch (_) { /* */ } } }
  if (S.tab === "info") { fillThumbs(); fillMaps(); return; }
  // Commands act only on what's selected: greyed out with nothing (their settings can still be set)
  // (a part of the whole project's, Checks' catalog: never waits for a selection)
  if (!c.any && S.tab !== "queue") pane.querySelectorAll('.secb:not(.whole) .btn').forEach((x) => { x.disabled = true; });
}
function momentsBody(c) {
  return '<p>For ' + esc(c.any ? c.label : "what you select") + '.</p>' +
    '<div class="btns"><button type="button" class="btn sm" data-a="albumadd">Add to an album…</button></div>' +
    '<span class="sub wrap">A link to each selected clip in an album of yours, with its thumbnail: made at once, the original stays where it is.</span>' +
    '<div class="btns"><button type="button" class="btn sm" data-a="job" data-v="moments">Moments of the marks</button></div>' +
    '<label class="switch"><input type="checkbox" id="moments_alone" data-a="alone_set"' + (S.alone ? " checked" : "") + '>Stabilized on their own, not as part of the clip</label>' +
    '<div class="field"><label class="sub" for="pkind">Photos as</label><select id="pkind" data-a="pkind"><option value="no"' + (S.photoKind === "no" ? " selected" : "") + '>Stills</option>' +
    (S.macos ? '<option value="live"' + (S.photoKind === "live" ? " selected" : "") + '>Live Photos</option>' : "") + '<option value="motion"' + (S.photoKind === "motion" ? " selected" : "") + '>Motion Photos</option></select></div>' +
    '<div class="btns"><button type="button" class="btn sm" data-a="photolinks">Photos of the marks</button><button type="button" class="btn sm" data-a="job" data-v="photos">…rendered now</button></div>' +
    '<span class="sub wrap">Of each mark without one anywhere in the project. A photo is a link to its frame with a small picture, made at once; it’s rendered when it’s exported. Rendered now: the full file, by the player, in the kind chosen above. Both land in Generated, in Albums.</span>';
}
// What the Library exports: moments, rendered and exported as the player's Shift+E does (the clips' photos are made in
// Moments & Previews)

// The folders shown now, on disk (Anton, 2026-11-10): the clip previews' pictures are made there first. Footage: the
// folders of its empty previews under the folder open; Originals: the clips' own folders there
let FOCUS_KEY = null;
function sendFocus() {
  if (S.view !== "footage" && S.view !== "sources") return;
  const path = S.path.join("/");
  const d = DATA[S.view];
  const key = S.view + "|" + path + "|" + (d && d.items ? d.items.length : 0);
  if (key === FOCUS_KEY) return;
  FOCUS_KEY = key;
  const dirs = new Set();
  for (const it of under(path)) {
    if (it.kind !== "clip") continue;
    const p = S.view === "footage" ? (it.draft ? it.preview : null) : it.path;
    if (p) dirs.add(p.replace(/[\\/][^\\/]*$/, ""));
    if (dirs.size >= 50) break;
  }
  invoke("set_focus", { dirs: Array.from(dirs) }).catch(() => { /* not wanted: the order only */ });
}
function renderAll() {
  const a = document.activeElement, id = a && a.id, pos = a && a.selectionStart;
  buildFacets();
  renderEmpty(); renderTop(); renderMain(); renderSide(); renderFilters(); renderSugg();
  sendFocus();
  if (id) { const e = $(id); if (e) { e.focus(); try { if (pos != null) e.setSelectionRange(pos, pos); } catch (_) { /* */ } } }
}
function renderQueueOnly() {
  if (S.tab === "queue") { const b = $("qtab"); if (b) b.innerHTML = queueHtml(); }
  const n = activeCount();
  document.querySelectorAll(".count").forEach((e) => { e.textContent = n; e.hidden = !n; });
}
