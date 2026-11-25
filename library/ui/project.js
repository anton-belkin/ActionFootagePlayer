"use strict";
// No project open (the recent ones); the project's settings, Take in.
// One of the window's scripts that were app.js: index.html loads them in this order before app.js, as classic
// scripts sharing their top-level names, so each uses the others' when called (only earlier ones at load).

function renderEmpty() {
  $("empty").hidden = !!S.root;
  $("bodyrow").hidden = !S.root;
  startHint();
  if (!S.root) $("recentEmpty").innerHTML = S.recent.length ? '<div class="lab" style="margin-top:12px">Opened before</div><div class="rlist">' + S.recent.map((r) => recentRow(r, false)).join("") + '</div>' : "";
}
// A project opened before: opened with a click; offline (its share not mounted, its disk not plugged in) marked so, with
// Connect when its share is known; taken off the list with ×
function recentRow(r, menu) {
  const busy = S.connecting["p:" + r.path];
  const where = r.here ? r.path : r.path + (r.share ? " · on " + r.share : "");
  return '<div class="rrow' + (r.here ? "" : " off") + '"><button type="button"' + (menu ? ' role="menuitem"' : "") + ' class="rmain" data-a="openrecent" data-v="' + esc(r.path) + '">' + (menu ? '<span class="ck"></span>' : "") +
    '<span style="min-width:0"><span class="nm" style="display:block">' + esc(r.name) + (r.here ? "" : ' <span class="chip warn">Offline</span>') + '</span><span class="sub">' + esc(where) + '</span></span></button>' +
    (!r.here && r.connect ? connectButton("connectrecent", r.path, busy, r.names) : "") + startRow(r, menu) +
    '<button type="button" class="rx" data-a="forgetrecent" data-v="' + esc(r.path) + '" title="Remove from the list" aria-label="Remove ' + esc(r.name) + ' from the list">' + ICON.x + '</button></div>';
}
// An offline project's share connected (connect_project), then the project opened from where it is now
async function connectRecent(path, host) {
  const key = "p:" + path;
  if (S.connecting[key]) return;
  S.connecting[key] = true; renderAll();
  try {
    const here = await invoke("connect_project", { path, host: host || null });
    S.menu = false;
    await Writes.drained();
    await call("open_project", { path: here }).catch(() => {});
  } catch (e) { showError(String(e)); }
  delete S.connecting[key];
  S.recent = await invoke("recent").catch(() => S.recent);
  renderAll();
  return poll();
}

// ---------- the project's settings ----------

const ICON_UP = '<svg width="10" height="10" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.6" aria-hidden="true"><path d="M6 15l6-6 6 6"/></svg>';
const ICON_DN = '<svg width="10" height="10" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.6" aria-hidden="true"><path d="M6 9l6 6 6-6"/></svg>';
function hostsToText(h) { return Object.keys(h || {}).map((k) => k + " = " + (h[k] || []).join(", ")).join("\n"); }
function textToHosts(t) {
  const out = {};
  for (const line of String(t || "").split("\n")) {
    const m = /^\s*([^=\s]+)\s*=\s*(.*)$/.exec(line);
    if (m) out[m[1]] = m[2].split(",").map((x) => x.trim()).filter(Boolean);
  }
  return out;
}
async function openSettings() {
  S.menu = false;
  renderTop();
  const st = await call("project_settings").catch(() => null);
  if (!st) return;
  S.settings = st;
  const ptr = st.name + ".afplocation";
  S.locEdit = st.locations.map((l) => ({ name: l.name, at: l.at, path: l.path, copies: l.copies,
    st: (l.reachable ? "Reachable" : "Not reachable here") + (l.pointer ? " · " + ptr + " in it" : l.reachable ? " · its " + ptr + " is written on Save" : "") }));
  S.neverRename = !!st.never_rename;
  S.tagStores = (st.tag_stores || []).slice();
  S.localEdit = st.local.map((l) => ({ name: l.name, at: l.at, path: l.path, bytes: l.bytes, files: l.files, reachable: l.reachable }));
  S.hostsText = hostsToText(st.hosts);
  S.sheet = "settings";
  renderSheet();
}
function renderTake() {
  const t = S.take;
  const counts = t.counts || {};
  const what = Object.keys(counts).filter((k) => counts[k]).map((k) => counts[k] + " in " + k).join(", ") || "nothing in its own folders";
  let h = '<div class="scrim" data-a="scrim"><div class="sheet" role="dialog" aria-modal="true" aria-labelledby="takeTitle">' +
    '<header><h2 id="takeTitle">Take in “' + esc(t.name) + '”</h2><span class="sub">Into ' + esc(S.name) + ', once. Nothing stays in sync afterwards.</span></header><div class="sb">' +
    '<div class="field"><span class="sub">Project</span><span class="mono">' + esc(t.other) + '/Project.afproject</span></div><div class="result">';
  for (const l of t.landings) {
    h += '<div class="it">' + (l.inside ? '<span class="chip ok">inside</span><span class="wrap">' + esc(l.name || "Its folder") + ' is already in this project, at ' + esc(l.at_here || "the top") + '/. No location added, no footage moved.</span>'
      : l.dir ? '<span class="chip fast">added</span><span class="wrap">' + esc(l.name) + ' becomes a location of this project at ' + esc(l.at_here || "?") + '/ (' + esc(l.path_here) + ').</span>'
      : '<span class="chip warn">not here</span><span class="wrap">' + esc(l.name) + ' (' + esc(l.path_here) + ') isn’t reachable from this computer: it’s added as it is.</span>') + '</div>';
  }
  h += '<div class="it"><span class="wrap">' + esc(what) + ' move into this project’s, under ' + esc(t.place || "?") + '/; their links get “' + esc(t.place) + '/” in front. The clips’ own files stay beside the clips.</span></div>';
  for (const i of t.issues || []) { const k = KIND_TEXT[i.kind] || ["", i.kind]; h += '<div class="it"><span class="chip ' + k[0] + '">' + esc(k[1]) + '</span><span class="wrap">' + esc(issueText(i)) + '</span></div>'; }
  h += '<div class="it"><span class="sub wrap">Its Project.afproject then goes to the Trash.</span></div></div>';
  if (t.landings.some((l) => !l.inside && l.dir)) {
    h += '<div class="field"><label class="sub" for="takeplace">Its other locations go at</label><span class="tagin"><input id="takeplace" type="text" value="' + esc(S.takePlace) + '" placeholder="2025/Red Sea"><button type="button" class="btn sm" data-a="takeplan">Show</button></span></div>';
  }
  h += '</div><footer><button type="button" class="btn" data-a="takein">Choose another…</button><span class="sp"></span><button type="button" class="btn" data-a="closesheet">Cancel</button><button type="button" class="btn pri" data-a="takerun"' + (t.needs_place || S.taking ? " disabled" : "") + '>' + (S.taking ? "Taking in…" : "Take in") + '</button></footer>';
  return h + '</div></div>';
}
async function chooseTakeIn() {
  S.menu = false; renderTop();
  const other = await call("choose_other").catch(() => null);
  if (!other) return;
  S.takePlace = "";
  await planTakeIn(other);
}
async function planTakeIn(other) {
  try { S.take = await invoke("take_in_plan", { other, place: S.takePlace }); S.sheet = "take"; renderSheet(); } catch (e) { showError(String(e)); }
}
async function runTakeIn() {
  if (!S.take) return;
  S.taking = true; renderSheet();
  try {
    const r = await invoke("take_in_run", { other: S.take.other, place: S.takePlace });
    S.sheet = null; S.take = null;
    toast("Taken in: " + r.moved + " files under " + r.place + "/" + (r.added ? ", " + r.added + " location" + (r.added > 1 ? "s" : "") + " added" : "") + "; its project file " + r.project_file + (r.problems.length ? ". But: " + r.problems.join(" · ") : ""));
    await refresh();
  } catch (e) { showError(String(e)); }
  S.taking = false; renderSheet();
}
function renderSheet() {
  const el = $("sheet");
  if (S.sheet === "take" && S.take) { el.innerHTML = renderTake(); return; }
  if (S.sheet === "album") { el.innerHTML = renderAlbumSheet(); return; } // albums.js
  if (S.sheet === "share") { el.innerHTML = renderShareSheet(); return; } // share.js
  if (S.sheet === "lens") { el.innerHTML = renderLensSheet(); return; } // lenses.js
  if (S.sheet === "geo") { el.innerHTML = renderGeo(); return; }
  if (S.sheet === "geoask") { el.innerHTML = renderGeoAsk(); return; }
  if (S.sheet === "geodenied") { el.innerHTML = renderGeoDenied(); return; }
  if (!S.sheet || !S.settings) { el.innerHTML = ""; return; }
  const st = S.settings;
  const tabs = [["loc", "Locations"], ["tags", "Tags"], ["mac", S.macos ? "On this Mac" : "On this computer"], ["gen", "General"]];
  let h = '<div class="scrim" data-a="scrim"><div class="sheet" role="dialog" aria-modal="true" aria-labelledby="sheetTitle">' +
    '<header><h2 id="sheetTitle">' + esc(st.name) + ': project settings</h2><span class="mono">' + esc(st.path) + '/Project.afproject</span></header>' +
    '<div class="tabs" role="tablist" style="margin-top:8px">' + tabs.map((t) => '<button type="button" role="tab" class="' + (S.stab === t[0] ? "on" : "") + '" data-a="stab" data-v="' + t[0] + '">' + t[1] + '</button>').join("") + '</div><div class="sb">';
  if (S.stab === "loc") {
    h += '<p class="sub wrap">Where the footage is. The first that has a clip wins. Each is a network path (//nas/share/folder) or a path relative to the project folder: the same on every computer. “In the tree” is where its folders show in the project.' +
      (S.locEdit.length ? "" : " With none, the project is next to its footage: its own folder.") + '</p>';
    if (S.locEdit.length) h += '<div class="lhead" aria-hidden="true"><span></span><span>Name</span><span>In the tree</span><span>Where</span><span>Copies only</span><span></span></div>';
    S.locEdit.forEach((l, i) => {
      h += '<div class="lrow"><span class="mv"><button type="button" data-a="locup" data-v="' + i + '" aria-label="Move ' + esc(l.name || "location") + ' up"' + (i === 0 ? " disabled" : "") + '>' + ICON_UP + '</button><button type="button" data-a="locdn" data-v="' + i + '" aria-label="Move ' + esc(l.name || "location") + ' down"' + (i === S.locEdit.length - 1 ? " disabled" : "") + '>' + ICON_DN + '</button></span>' +
        '<input type="text" id="locname-' + i + '" data-l="name" data-i="' + i + '" value="' + esc(l.name) + '" aria-label="Name" placeholder="Name">' +
        '<input type="text" id="locat-' + i + '" data-l="at" data-i="' + i + '" value="' + esc(l.at) + '" aria-label="In the tree" placeholder="/ (the top)">' +
        '<input type="text" id="locpath-' + i + '" data-l="path" data-i="' + i + '" value="' + esc(l.path) + '" aria-label="Where" placeholder="//nas/share/folder" class="mono">' +
        '<label class="switch" style="min-height:30px"><input type="checkbox" id="loccp-' + i + '" data-a="loccopies" data-v="' + i + '"' + (l.copies ? " checked" : "") + '><span class="sub" style="display:inline">copies</span></label>' +
        '<button type="button" class="rm" data-a="locrm" data-v="' + i + '" aria-label="Remove ' + esc(l.name || "location") + '">' + ICON.x + '</button>' +
        '<span class="st">' + esc(l.st || "") + '</span></div>';
    });
    h += '<div class="btns"><button type="button" class="btn sm" data-a="locadd">Add a location…</button>' + (st.next_to_footage ? '<button type="button" class="btn sm" data-a="locself">Add the project folder</button>' : "") + '</div>' +
      '<p class="sub wrap">Saving writes them into Project.afproject, and a ' + esc(st.name) + '.afplocation file at the top of each, so a clip opened from Finder finds this project. A location taken out loses its file; nothing else in it changes.</p></div>' +
      '<footer><span class="sp"></span><button type="button" class="btn" data-a="closesheet">Cancel</button><button type="button" class="btn pri" data-a="savelocs">Save</button></footer>';
  } else if (S.stab === "tags") {
    // The tag stores (docs/design/tags-and-culling.md): the library's own always; the others synced both ways when on, a
    // choice of this computer's (kept in its own file for the project, never in Project.afproject)
    const on = (k) => S.tagStores.indexOf(k) >= 0;
    const row = (k, name, what, can) => '<label class="switch"><input type="checkbox" data-a="tagstore" data-v="' + k + '"' + (k === "library" || on(k) ? " checked" : "") + (k === "library" || !can ? " disabled" : "") + '><span class="wrap"><b>' + esc(name) + '</b> · ' + what + '</span></label>';
    h += '<p class="sub wrap">Accept or Reject, 1–5 stars and your own tags, on originals, moments and stills (never clip previews or exports). The library keeps them, the same for every computer. The stores turned on here get them too, and what’s changed there comes back (both ways). Which stores are on is this computer’s choice: it’s kept on this computer, not in Project.afproject.</p>' +
      '<div class="lab">Where tags are kept</div>' +
      row("library", "The library", "the truth: an original’s own file, a moment’s link, a still’s XMP keywords", true) +
      row("finder", "Finder tags", S.settings.finder_here ? "on the files themselves, as Finder and Spotlight show them: Accept (green), Reject (red), ★★★, and your tags. An attribute beside the video, never the video" : "macOS only: set on a Mac", !!S.settings.finder_here) +
      row("xmp", "XMP sidecars", "clip.xmp beside each original, for Lightroom, Bridge, digiKam, darktable (not yet)", false) +
      row("explorer", "Windows Explorer", "Explorer keeps an MP4’s tags inside the video: not for originals; moments later (not yet)", false) +
      row("synology", "Synology Photos", "through its web API, with an account (not yet)", false) +
      row("qnap", "QNAP QuMagie, UGREEN Photos", "to find out: their own databases, no API known (not yet)", false) +
      '<div class="btns"><button type="button" class="btn sm" data-a="synctags"' + (on("finder") ? "" : " disabled") + '>Sync tags now</button></div>' +
      '<p class="sub wrap">Sync runs on every change, when the player opens an item, and when this Mac sees a file’s Finder tags change. Changes made on another computer to files on the NAS come in when an item is opened or with Sync tags now. Conflicts: tags from both sides; Accept/Reject and stars from the side that changed (both: the later).</p></div>' +
      '<footer><span class="sp"></span><button type="button" class="btn" data-a="closesheet">Cancel</button><button type="button" class="btn pri" data-a="savetagstores">Save</button></footer>';
  } else if (S.stab === "mac") {
    h += '<p class="sub wrap">Kept only on this computer, never part of the library: delete any of it and this computer simply reads the library again.</p>' +
      '<div class="lab">Local copies of clips</div>';
    if (!S.localEdit.length) h += '<p class="sub wrap">None. A folder here with copies of some clips, at their places in the tree, plays before the library’s.</p>';
    S.localEdit.forEach((l, i) => {
      h += '<div class="kv"><span style="min-width:0"><span class="nm">' + esc(l.path) + '</span><span class="sub">In the tree at ' + esc(l.at ? l.at + "/" : "the top") + (l.reachable === false ? " · not there now" : "") + '</span></span><span class="num">' + (l.files != null ? l.files + " files · " + sizeText(l.bytes) : "") + '</span><button type="button" class="btn sm" data-a="localrm" data-v="' + i + '">Remove</button></div>';
    });
    h += '<div class="btns"><button type="button" class="btn sm" data-a="localadd">Add a local copy…</button></div>' +
      '<div class="lab">Finding moved clips</div>' +
      '<label class="switch"><input type="checkbox" id="neverrename" data-a="neverrename"' + (S.neverRename ? " checked" : "") + '><span class="wrap">I never rename my footage: a clip that moved is found by its name too, not by its size and recording time alone. A fingerprint, where a link has one, decides either way.</span></label>' +
      '<div class="lab">Caches on this computer, for all projects</div><div class="kv">';
    for (const c of st.caches) h += '<span style="min-width:0"><span class="nm">' + esc(c.key) + '</span><span class="sub">' + esc(c.what) + '</span></span><span class="num">' + c.files + " files · " + sizeText(c.bytes) + '</span><button type="button" class="btn sm" data-a="clearcache" data-v="' + esc(c.key) + '"' + (c.files ? "" : " disabled") + '>Clear</button>';
    h += '</div></div><footer><span class="sp"></span><button type="button" class="btn" data-a="closesheet">Close</button><button type="button" class="btn pri" data-a="savelocal">Save</button></footer>';
  } else {
    h += '<div class="field"><span class="sub">Project folder</span><span class="mono">' + esc(st.path) + '</span></div>' +
      '<div class="field"><span class="sub">Its id</span><span class="mono">' + esc(st.id || "made when it’s first needed") + '</span></div>' +
      '<div class="field" style="align-items:start"><label class="sub" for="hosts">The NAS’s other names</label><textarea id="hosts" rows="3" class="mono" style="border:1px solid var(--line);border-radius:7px;padding:6px 8px;background:var(--surface);color:var(--fg)" placeholder="DISKSTATIONDXP = web.nas, 192.168.50.186">' + esc(S.hostsText) + '</textarea></div>' +
      '<p class="sub wrap">One line per NAS: the name its locations use, then the other names it answers to. Any of them matches a share mounted under that name.</p>' +
      '<div class="lab">What’s made</div>' +
      '<label class="switch"><input type="checkbox" id="genruns"' + (st.generated_runs ? " checked" : "") + '><span class="wrap">A folder per run in Generated: each run’s clip previews, moments, photos and exports in a folder of their own, named by its date and time (what’s saved by hand: one a day). Off: all in Generated itself (_Clips/Generated, _Moments/Generated, _Stills/Generated, _Exports/Generated).</span></label>' +
      // The render cache (_Renders, in the project: Share's renders, made again when they're needed). Cleared only here
      (st.renders ? '<div class="lab">Renders</div><div class="kv" style="grid-template-columns:minmax(0,1fr) auto auto;padding:8px 10px;border:1px solid var(--line2);border-radius:8px"><span style="min-width:0"><span class="nm">_Renders</span><span class="sub">Share’s renders, copied at once next time; cleared, they’re made again when shared' + (st.renders.others ? " · " + st.renders.others + (st.renders.others === 1 ? " other file there is kept" : " other files there are kept") : "") + '</span></span><span class="num" style="white-space:nowrap">' + st.renders.files + " files · " + sizeText(st.renders.bytes) + '</span><button type="button" class="btn sm" data-a="clearrenders"' + (st.renders.files ? "" : " disabled") + '>Clear the render cache</button></div>' : "") +
      '<div class="btns"><button type="button" class="btn sm" data-a="moveproject">Move the project to…</button></div>' +
      '<p class="sub wrap">Moves Project.afproject and the project’s _ folders to another folder (the SSD, say). The clips and their own files stay where they are; this folder becomes a location.</p></div>' +
      '<footer><span class="sp"></span><button type="button" class="btn" data-a="closesheet">Cancel</button><button type="button" class="btn pri" data-a="savelocs">Save</button></footer>';
  }
  el.innerHTML = h + '</div></div>';
}
async function saveLocations() {
  const hosts = textToHosts(S.hostsText);
  try {
    const gr = $("genruns");
    const problems = await invoke("save_locations", { list: S.locEdit.map((l) => ({ name: l.name, at: l.at, path: l.path, copies: !!l.copies })), hosts, generatedRuns: gr ? gr.checked : null });
    S.sheet = null; renderSheet();
    toast(problems && problems.length ? "Saved. " + problems.join(" · ") : "Saved in Project.afproject; each location has its pointer file");
    await refresh();
  } catch (e) { showError(String(e)); }
}
