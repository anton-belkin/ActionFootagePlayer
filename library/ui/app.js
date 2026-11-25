"use strict";
// The Library's window (docs/design/library-management.md, the mockup it was settled on): a project's footage, moments,
// photos and exports browsed as folders; filters that narrow the folder you're in, or a search that lists every match;
// what's selected and the commands on it in one side panel, its parts chosen with a column of icons at its edge (the
// queue one of them). What it shows comes from the app
// (src/main.rs, src/index.rs): the project's files, and what each clip is (gyroflow-probe --library).

// ---------- doing ----------

async function addJob(kind) {
  const c = ctx();
  if (!c.any) return;
  const options = kind === "photos" ? { photo: S.photoKind }
    : kind === "clips" || kind === "clips-again" ? { layout: S.clipsLayout } : kind === "moments" ? { alone: S.alone } : {};
  let label = c.label;
  if (kind === "optical") {
    const op = opticalPlan(c, S.opticalRedo);
    if (!op.go.length) return showError("None of the selected clips needs it: they have motion data, a Gyroflow project, their motion measured already (Measure again), or no ready lens (Find lens).");
    options.clips = op.go;
    options.redo = S.opticalRedo;
    label = op.go.length + (op.go.length === 1 ? " clip" : " clips") + (S.opticalRedo && op.redo ? ", " + op.redo + " measured again" : "");
  }
  if (kind === "lens-find" || kind === "lens-estimate") {
    const clips = c.selItems.filter((it) => it.kind === "clip" && !it.is360 && lensFrom(it) !== "file");
    if (!clips.length) return showError(c.selItems.some((it) => lensFrom(it) === "file") ? "The lens is in the file: the camera wrote its own calibration of the clip" : "Select clips to find their lens");
    if (kind === "lens-estimate" && clips.length !== 1) return showError("Estimate lens is for one clip: select the one with the best view (the camera moving through a still scene)");
    options.clips = clips.map((it) => it.id);
    label = clips.length === 1 ? clips[0].name : clips.length + " clips";
  }
  try {
    await invoke("add_job", { kind, options, only: c.only, label });
    toast("In the queue: " + label);
    showTab("queue");
    renderSide();
  } catch (e) { showError(String(e)); }
}
function selectedPaths() {
  const c = ctx();
  return c.selItems.map((it) => it.path);
}

document.addEventListener("mousedown", (e) => { if (e.shiftKey && e.target.closest('[data-a="pick"]')) e.preventDefault(); });
document.addEventListener("click", async (e) => {
  const el = e.target.closest("[data-a]");
  if (CMENU) { const inMenu = el && el.closest(".cmenu"); closeMenu(); if (!inMenu) return; if (el.disabled) return; }
  if (el && el.tagName === "INPUT" && el.dataset.a === "loccopies") return;
  // Connect's list of names: closed by a click anywhere else
  if (S.via && !(el && el.closest(".split"))) { S.via = null; renderAll(); if (!el) return; }
  if (S.menu && !(el && el.closest(".menu")) && !(el && el.dataset.a === "menu")) { S.menu = false; if (!el) { renderTop(); renderSugg(); return; } }
  if (S.suggOpen && !e.target.closest(".searchwrap")) { S.suggOpen = false; renderSugg(); }
  if (!el) {
    if (e.target.closest("#main") && !e.target.closest(".row,.card,.crumbs,.scope,.selbar") && Object.keys(S.sel).length) { S.sel = {}; S.anchor = null; renderMain(); renderSide(); }
    return;
  }
  const a = el.dataset.a, v = el.dataset.v;
  if (el.tagName === "INPUT" || el.tagName === "SELECT") return;
  if (a.startsWith("geo")) return geoAct(a, v, el);
  if (a === "savelens") return saveProjectLens(v, el.dataset.cam);
  if (a === "keepfound") return keepFoundLens(v, el.dataset.n, el.dataset.m);
  switch (a) {
    case "menu": S.menu = !S.menu; if (S.menu) S.recent = await invoke("recent").catch(() => S.recent); break;
    case "closemenu": S.menu = false; break;
    // (another project: what's still being written of this one is written first, here and below)
    case "choose": S.menu = false; await Writes.drained(); await call("choose_project").catch(() => {}); return poll();
    case "openrecent": {
      // As it is now (a share connected by hand since the list was drawn)
      S.recent = await invoke("recent").catch(() => S.recent);
      const r = S.recent.find((x) => x.path === v);
      if (r && !r.here) { if (r.connect) return connectRecent(v); showError(r.name + " isn’t reachable: connect " + (r.share || "its disk") + " first"); return; }
      S.menu = false; await Writes.drained(); await call("open_project", { path: v }).catch(() => {}); return poll();
    }
    case "connectrecent": S.via = null; return connectRecent(v, el.dataset.h);
    case "connectvia": S.via = S.via === v ? null : v; renderAll(); return;
    case "forgetrecent": await invoke("forget_recent", { path: v }).catch(() => {}); S.recent = await invoke("recent").catch(() => S.recent); renderAll(); return;
    case "revealroot": S.menu = false; call("reveal", {}).catch(() => {}); break;
    case "view":
      S.view = v; S.path = []; S.sel = {}; S.anchor = null; S.filters = {}; S.q = ""; S.flat = false; S.renaming = null;
      // Read again each time (from the project's catalog: quick), what it had shown meanwhile: what changed while
      // another tab was open (a task's photos, exports) shows
      renderAll(); await loadView(v).catch(() => {});
      break;
    case "addsource": return addSource();
    case "filters": S.filtersOpen = !S.filtersOpen; S.ffreeze = null; break; // laid out afresh when it opens
    case "fmore": S.fmore[v] = !S.fmore[v]; break;
    case "fclear": S.filters = {}; break;
    case "fsec": S.fsec[v] = !S.fsec[v]; break;
    case "fexp": S.fexp[v] = S.fexp[v] !== true; break; // collapsed until opened, kept while the window is open
    case "tokrm": { const f = allFacets().find((x) => x.id === v); if (f) facetLeaves(f).forEach((l) => { delete S.filters[l.id]; }); break; }
    case "suggpick": setOpt(v, true); S.q = ""; S.suggOpen = false; S.sugg = -1; renderAll(); $("q").focus(); return;
    case "suggtext": S.suggOpen = false; renderSugg(); $("q").focus(); return;
    case "scope": S.scope = v; S.sel = {}; S.anchor = null; break;
    case "flat": S.flat = true; S.scope = "here"; S.sel = {}; S.anchor = null; break;
    case "endsearch": S.q = ""; S.flat = false; S.sel = {}; S.anchor = null; break;
    case "layout": S.layout = v; break;
    case "crumb": S.path = S.path.slice(0, +v); S.sel = {}; S.anchor = null; S.flat = false; break;
    case "pick": {
      if (e.detail > 1) return;
      if (e.target.id === "rename") return;
      const multi = e.metaKey || e.ctrlKey;
      if (e.shiftKey && S.anchor && VISIBLE.indexOf(S.anchor) >= 0) {
        const i = VISIBLE.indexOf(S.anchor), j = VISIBLE.indexOf(v), lo = Math.min(i, j), hi = Math.max(i, j);
        if (!multi) S.sel = {};
        VISIBLE.slice(lo, hi + 1).forEach((k) => { S.sel[k] = true; });
      } else if (multi) { if (S.sel[v]) delete S.sel[v]; else S.sel[v] = true; S.anchor = v; }
      else { S.sel = {}; S.sel[v] = true; S.anchor = v; }
      S.checked = null; S.found = null;
      renderMain(); renderSide(); return;
    }
    case "selall": S.sel = {}; VISIBLE.forEach((k) => { S.sel[k] = true; }); S.anchor = VISIBLE[0] || null; break;
    case "desel": S.sel = {}; S.anchor = null; break;
    // The part shown again: the panel closes to its column (as the player's side panel does)
    case "tab": if (S.tab === v && S.sideOpen) { S.sideOpen = false; remember("afl.side", ""); } else showTab(v); break;
    case "srcsettings": showTab("source"); break;
    case "srcapply": return applySource();
    case "remeasure": S.opticalRedo = true; try { await addJob("optical"); } finally { S.opticalRedo = false; } return;
    case "srcclear": S.srcDraft = {}; renderSide(); return;
    case "job": return addJob(v);
    case "connectloc": S.via = null; connectLocation(v, el.dataset.h); return;
    case "openit": openChosen(); return;
    case "revealit": { const c = ctx(); const first = c.selFolders.length ? (S.view === "sources" ? treeOnDisk(c.selFolders[0]) : folderOnDisk(c.selFolders[0])) : c.direct[0] && c.direct[0].path; if (first) call("reveal_path", { path: first }).catch(() => {}); return; }
    case "cancel": await call("cancel_job", { id: Number(v) }).catch(() => {}); return poll();
    case "clearjobs": await call("clear_finished").catch(() => {}); return poll();
    case "keep": await call("answer_close", { quit: false }).catch(() => {}); return poll();
    // (what's still being written is written first)
    case "quit": return Writes.drained().then(() => call("answer_close", { quit: true }).catch(() => {}));
    case "closenow": closingNow = false; return call("answer_close", { quit: true }).catch(() => {});
    case "unsavedok": UNSAVED.length = 0; renderUnsaved(); return;
    case "toastok": $("toast").hidden = true; return;
    case "sheet": return openSettings();
    case "takein": S.sheet = null; renderSheet(); return chooseTakeIn();
    case "takeplan": return planTakeIn(S.take.other);
    case "takerun": return runTakeIn();
    case "runcheck": return runCheck();
    case "carelook": return careLook(v);
    case "revealpath": { if (v) call("reveal_path", { path: v }).catch(() => {}); return; }
    case "showbroken": {
      // Where they are: the view that has them, filtered to them (across its folders); clip previews (not items of a
      // view) listed in the strip itself
      const list = S.missingList || [];
      // (moments, photos and exports are all in Albums)
      const view = list.some((x) => x.kind === "moment" || x.kind === "export" || /\.(heic|jpe?g)$/i.test(x.file)) ? "footage" : null;
      const previews = list.filter((x) => x.kind === "clip" || (!/\.(heic|jpe?g)$/i.test(x.file) && !["moment", "export"].includes(x.kind)));
      S.showBrokenList = !view || previews.length > 0;
      if (view) {
        S.view = view; S.path = []; S.flat = true; S.q = ""; S.sel = {};
        await needViews(["footage", "moments", "stills", "exports"]);
        buildFacets(); S.filters = { "lk:broken": true };
      }
      renderAll(); return;
    }
    case "mend": {
      S.mending = true; renderMain();
      try {
        const r = await invoke("mend");
        toast("Mended " + r.mended + " of " + r.missing + (r.left.length ? ": " + r.left.length + " to choose or not found, in Checks" : "") + (r.problems.length ? "; " + r.problems.length + " couldn’t be written" : ""));
        if (r.left.length) { S.found = r.left; S.checked = { issues: r.left.map((f) => f.issue) }; showTab("checks"); }
        refresh();
      } catch (er) { showError(String(er)); }
      S.mending = false; renderAll(); return;
    }
    case "back": return goHistory(-1);
    case "fwd": return goHistory(1);
    case "up": return goUp();
    case "newdir": return newFolder();
    case "enter": return openSelection();
    case "showclip": return showClip(v);
    case "gofootage": return goFootage(v);
    case "fcrumb": S.view = "sources"; S.path = v ? v.split("/") : []; S.sel = {}; S.anchor = null; S.filters = {}; if (!DATA.footage) await needViews(["footage"]); break;
    case "ql": return quickLook();
    case "renameit": { const c = ctx(); if (canRename(c)) startRename(); else toast(S.view === "sources" ? "Sources’ folders and clips are renamed in Finder" : "A clip is renamed in Finder: Find moved files then follows"); return; }
    case "trash": return deleteSelection();
    case "share": return askShare(); // share.js
    case "sharego": return shareGo(v);
    case "lenssheet": return openLensSheet(); // lenses.js
    case "lensfind": return lensRun("lens-find");
    case "lensestimate": return lensRun("lens-estimate");
    case "lenspick": if (S.lens) { S.lens.chosen = +v; renderSheet(); } return;
    case "lensuse": return lensUse();
    case "lenstry": return lensTry();
    case "lensforget": return lensForget();
    case "albumadd": return askAlbum("add");
    case "albummove": return askAlbum("move");
    case "albumout": return albumOut();
    case "photolinks": return photoLinks();
    case "albumpick": if (S.album) { S.album.to = v; S.album.name = ""; renderSheet(); } return;
    case "albumgo": return albumGo();
    case "info": showTab("info"); break;
    case "revealhere": { const p = hereOnDisk(); if (p) call("reveal_path", { path: p }).catch(() => {}); return; }
    case "opengf": { const it = ctx().selItems[0]; if (it) call("open_in_gyroflow", { paths: it.kind === "clip" ? (it.copies || []).map((x) => x.path) : [it.path] }).catch(() => {}); return; }
    case "removesource": return removeSources(ctx().selFolders);
    case "delrenders": {
      const c = ctx(); if (!c.any) return;
      // Shown at once, deleted after (writes.js): the renders of the moments and photo links selected
      const ms = c.selItems.filter((it) => it.kind === "moment" || (it.kind === "still" && it.linked));
      const done = await Writes.run("renders", ms, { moments: ms.map((it) => it.id), staleOnly: v === "stale" });
      if (done.answer) toast(done.answer.deleted + " render" + (done.answer.deleted === 1 ? "" : "s") + " deleted");
      return;
    }
    case "tag": { const t = S.tagDraft.trim(); if (!t) return; S.tagDraft = ""; return setTags([t], []); }
    case "untag": return setTags([], [v]);
    case "renametag": return renameTag();
    case "synctags": try { const r = await invoke("sync_tags", { items: null }); toast(r.came ? r.came + " item" + (r.came === 1 ? "" : "s") + " changed in Finder, taken in" : "In step with Finder (" + r.looked + " looked at)"); if (r.problems.length) showError(r.problems.join(" · ")); await refresh(); } catch (er) { showError(String(er)); } return;
    case "savetagstores": try { const r = await invoke("save_tag_stores", { stores: S.tagStores }); S.sheet = null; renderSheet(); toast("Tag stores saved" + (r.came ? ": " + r.came + " item" + (r.came === 1 ? "" : "s") + " taken in from Finder" : "")); if (r.problems.length) showError(r.problems.join(" · ")); await refresh(); } catch (er) { showError(String(er)); } return;
    case "findmoved": return findMoved();
    case "applymoved": return applyMoved();
    case "pickmoved": { const [i, j] = String(v).split(":").map(Number); const f = S.found && S.found[i]; if (f && f.candidates[j]) { f.found = f.candidates[j]; f.by = "chosen"; renderSide(); } return; }
    case "scrim": if (e.target !== el) return; S.sheet = null; renderSheet(); return;
    case "closesheet": S.sheet = null; renderSheet(); return;
    case "stab": S.stab = v; renderSheet(); return;
    case "locup": { const i = +v, l = S.locEdit; [l[i - 1], l[i]] = [l[i], l[i - 1]]; renderSheet(); return; }
    case "locdn": { const i = +v, l = S.locEdit; [l[i + 1], l[i]] = [l[i], l[i + 1]]; renderSheet(); return; }
    case "locrm": S.locEdit.splice(+v, 1); renderSheet(); return;
    case "locadd": { const r = await call("choose_location").catch(() => null); if (r) { S.locEdit.push({ name: r.name, at: "", path: r.path, copies: false, st: "Chosen: " + r.dir }); renderSheet(); } return; }
    case "locself": S.locEdit.unshift({ name: S.name, at: "", path: ".", copies: false, st: "The project folder itself" }); renderSheet(); return;
    case "savelocs": return saveLocations();
    case "localadd": { const d = await call("choose_local").catch(() => null); if (d) { S.localEdit.push({ name: "On this computer", at: "", path: d }); renderSheet(); } return; }
    case "localrm": S.localEdit.splice(+v, 1); renderSheet(); return;
    case "savelocal": try { const nr = $("neverrename"); await invoke("save_local", { list: S.localEdit.map((l) => ({ name: l.name, at: l.at, path: l.path })), neverRename: nr ? nr.checked : S.neverRename }); S.sheet = null; renderSheet(); toast("Saved on this computer"); await refresh(); } catch (er) { showError(String(er)); } return;
    case "clearcache": try { await invoke("clear_cache", { key: v }); toast("Cleared"); await openSettings(); } catch (er) { showError(String(er)); } return;
    case "clearrenders": try { const r = await invoke("clear_renders"); toast("Render cache cleared: " + sizeText(r.bytes) + " freed (" + r.files + (r.files === 1 ? " file" : " files") + ")"); await openSettings(); } catch (er) { showError(String(er)); } return;
    case "exportjson": {
      S.menu = false; renderTop();
      try { const f = await invoke("export_library"); if (f) { toast("Exported: " + f); call("reveal_path", { path: f }).catch(() => {}); } else $("toast").hidden = true; } catch (er) { showError(String(er)); }
      return;
    }
    case "moveproject": {
      S.menu = false; S.sheet = null; renderSheet(); renderTop();
      // The page polls how far it is meanwhile, more often than usual
      const fast = setInterval(poll, 250);
      // Its tasks wait meanwhile, then go on from the new place
      try { const r = await invoke("move_project"); if (r) { toast("The project is now in " + r); return poll(); } } catch (er) { showError(String(er)); }
      finally { clearInterval(fast); renderMoving(null); }
      return;
    }
    default: return;
  }
  renderAll();
});
document.addEventListener("dblclick", (e) => {
  const el = e.target.closest('[data-a="pick"]');
  if (!el) return;
  const v = el.dataset.v;
  if (v.startsWith("f:")) { S.q = ""; S.flat = false; S.path = v.slice(2).split("/"); S.sel = {}; S.anchor = null; renderAll(); $("main").scrollTop = 0; return; }
  if (S.renaming) return;
  const it = items().find((x) => keyOf(x) === v);
  if (it) { S.sel = {}; S.sel[v] = true; S.anchor = v; renderMain(); renderSide(); openItems([it.path], playList([it])); }
});
document.addEventListener("input", (e) => {
  const t = e.target;
  if (t.classList && t.classList.contains("ffind")) { S.ffind[t.dataset.f] = t.value; renderFilters(); }
  else if (t.id === "q") { S.q = t.value; S.suggOpen = true; S.sugg = -1; S.sel = {}; S.anchor = null; renderMain(); renderSide(); renderFilters(); renderSugg(); }
  else if (t.dataset.l) { S.locEdit[+t.dataset.i][t.dataset.l] = t.value; }
  else if (t.id === "hosts") { S.hostsText = t.value; }
  else if (t.id === "takeplace") { S.takePlace = t.value; }
  else if (t.id === "tagdraft") { S.tagDraft = t.value; }
  else if (t.id === "albumname") { albumTyped(t.value); }
  else if (t.id === "tagfrom") { S.tagFrom = t.value; }
  else if (t.id === "tagto") { S.tagTo = t.value; }
  else if (t.id === "geohand") { S.geoHand = t.value; }
  else if (t.id === "geosite") { S.geoSite = t.value; }
  else if (t.id === "geomax") { S.geoMax = +t.value; geoSlid(); }
});
document.addEventListener("change", (e) => {
  const el = e.target, a = el.dataset.a, v = el.dataset.v;
  if (!a) return;
  if (a === "loccopies") { S.locEdit[+v].copies = el.checked; return; }
  if (a === "neverrename") { S.neverRename = el.checked; return; }
  if (a === "tagstore") { S.tagStores = S.tagStores.filter((k) => k !== v).concat(el.checked ? [v] : []); return; }
  // The sheet of Photos' places: which one the clips too far get, and whether locations set already stay
  if (a === "geopick") { S.geoPick = el.value; geoSlid(); return; }
  if (a === "geokeep") { S.geoKeep = el.checked; geoSlid(); return; }
  if (a === "lensquick") { S.lensQuick = el.checked; remember("afl.lensQuick", el.checked ? "1" : "0"); return; }
  if (a === "sharesetting") { shareSetting(el.dataset.v, el.value); renderSheet(); renderSide(); return; }
  if (a === "sharelook") { if (S.share) { S.share.look = el.value === "SDR" ? "SDR" : "HDR"; S.share.sel = shareSel(S.share.list, S.share.look); remember("afl.share.look", S.share.look); renderSheet(); renderSide(); } return; }
  if (a === "fopt") setOpt(v, el.checked);
  else if (a === "pkind") S.photoKind = el.value;
  // Chosen for each run, remembered on this computer (docs/design/library-management.md §8), not in the project
  else if (a === "layout_set") { S.clipsLayout = el.value; remember("afl.layout", S.clipsLayout); return; }
  else if (a === "alone_set") { S.alone = el.checked; remember("afl.alone", S.alone ? "1" : ""); return; }
  else if (a === "srcstrength") { S.srcDraft.uw_strength = (+el.value || 0) / 100; renderSide(); return; }
  else if (a === "srcset") { srcDraftSet(v, el.value); return; }
  else if (a === "assignlens") { assignLens(v, el.value); return; }
  else if (a === "setting") { call("set_setting", { key: v, value: el.checked }).then(() => invoke("project")).then((i) => { S.info = i; renderSide(); }).catch(() => {}); return; }
  else return;
  renderAll();
});
document.addEventListener("keydown", (e) => {
  const typing = /^(INPUT|TEXTAREA|SELECT)$/.test(e.target.tagName);
  if ((e.target.id === "tagfrom" || e.target.id === "tagto") && e.key === "Enter") { e.preventDefault(); renameTag(); return; }
  if (e.target.id === "tagdraft" && cullDraftKeys(e)) return;
  if (e.target.id === "tagdraft" && e.key === "Enter") { e.preventDefault(); const t = S.tagDraft.trim(); if (t) { S.tagDraft = ""; setTags([t], []); } return; }
  if (e.target.id === "albumname" && e.key === "Enter") { e.preventDefault(); albumGo(); return; }
  if (e.target.id === "q") {
    const list = suggestions(), max = list.length;
    if (e.key === "ArrowDown" && S.suggOpen) { e.preventDefault(); S.sugg = Math.min(max, S.sugg + 1); renderSugg(); }
    else if (e.key === "ArrowUp" && S.suggOpen) { e.preventDefault(); S.sugg = Math.max(-1, S.sugg - 1); renderSugg(); }
    else if (e.key === "Enter") { e.preventDefault(); if (S.suggOpen && S.sugg >= 0 && S.sugg < max) { setOpt(list[S.sugg].id, true); S.q = ""; } S.suggOpen = false; S.sugg = -1; renderAll(); $("q").focus(); }
    else if (e.key === "Backspace" && e.target.value === "") { const t = activeTokens(); if (t.length) { facetLeaves(t[t.length - 1].f).forEach((l) => { delete S.filters[l.id]; }); renderAll(); $("q").focus(); } }
    else if (e.key === "Escape" && S.suggOpen) { S.suggOpen = false; renderSugg(); }
    return;
  }
  if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === "a" && !typing && S.root) { e.preventDefault(); S.sel = {}; VISIBLE.forEach((k) => { S.sel[k] = true; }); S.anchor = VISIBLE[0] || null; renderMain(); renderSide(); return; }
  if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === "f" && S.root) { e.preventDefault(); const q = $("q"); if (q) q.focus(); return; }
  if (CMENU && e.key === "Escape") { closeMenu(); return; }
  if (CMENU) closeMenu();
  if (e.key === "Escape" && S.sheet) { S.sheet = null; renderSheet(); return; }
  if (e.key === "Escape" && S.menu) { S.menu = false; renderTop(); return; }
  if (e.target.id === "rename") {
    if (e.key === "Enter") { e.preventDefault(); commitRename(e.target.value); }
    else if (e.key === "Escape") { e.preventDefault(); S.renaming = null; renderMain(); }
    return;
  }
  if (e.key === "Escape" && !typing && Object.keys(S.sel).length) { S.sel = {}; S.anchor = null; renderMain(); renderSide(); return; }
  if (typing || S.sheet || !S.root) return;
  if (cullKeys(e)) { e.preventDefault(); return; }
  if (listKeys(e)) e.preventDefault();
});

// A Project.afproject (or a folder of the project) dropped on the window
try {
  window.__TAURI__.webview.getCurrentWebview().onDragDropEvent(async (ev) => {
    if (ev.payload.type === "drop" && ev.payload.paths.length) {
      await Writes.drained();
      await call("open_project", { path: ev.payload.paths[0] }).catch(() => {});
      poll();
    }
  });
} catch (e) { /* no drops then */ }

(async () => {
  try { await invoke("engine"); } catch (e) { $("engine").textContent = String(e); $("engine").hidden = false; }
  S.recentTags = await invoke("recent_tags").catch(() => []);
  S.recent = await invoke("recent").catch(() => []);
  await poll();
  // Started with no file: the project marked Open at start (atstart.js)
  if (!S.root) await startOpen();
  if (!S.root) renderAll();
  setInterval(poll, 600);
  setInterval(pollProbes, 900);
})();
