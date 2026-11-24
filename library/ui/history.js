"use strict";
// Back and Forward, as Safari has them (Anton, 2026-11-24): the places one has been (a tab, a folder or album, an
// album's view, a clip's own folder) in the order they were opened; Back returns to the one before, Forward goes again
// to the one Back left; a new place opened after Back drops the ones ahead. A place comes back as it was left: its
// scroll, what was selected in it, its search and filters. Choosing, searching and filtering inside a place are not
// steps of their own. Its buttons are the bar's ‹ ›; the mouse's side buttons (button 3 Back, 4 Forward: WebKit on the
// Mac and WebView2 on Windows both hand them to the page), ⌘[ ⌘] on the Mac and Alt+← Alt+→ on Windows do the same.
// (app.js: renderMain notes the place it draws; S.hist, S.histAt)

const HIST_MAX = 200;
function place() { return S.view + "\u0000" + (S.view === "clip" ? S.clip : S.path.join("/")); }
// What a place looks like now, kept in its step (its scroll only as it's left: the list may be someone else's by then)
function histKeep(e) {
  e.sel = Object.keys(S.sel); e.anchor = S.anchor; e.cursor = S.cursor;
  e.q = S.q; e.flat = S.flat; e.scope = S.scope; e.filters = Object.assign({}, S.filters);
}
function noteHistory() {
  const here = place();
  const cur = S.hist[S.histAt];
  if (cur && cur.key === here) { histKeep(cur); return; }
  // The list still shows the place left: its scroll
  if (cur) cur.scroll = $("main").scrollTop;
  S.hist = S.hist.slice(0, S.histAt + 1);
  const e = { key: here, scroll: 0 };
  histKeep(e);
  S.hist.push(e);
  if (S.hist.length > HIST_MAX) S.hist.shift();
  S.histAt = S.hist.length - 1;
}
// Another project: a history of its own
function resetHistory() { S.hist = []; S.histAt = -1; }
function canGoHistory(step) { const i = S.histAt + step; return i >= 0 && i < S.hist.length; }
function goHistory(step) {
  if (!canGoHistory(step)) return false;
  const cur = S.hist[S.histAt];
  if (cur && cur.key === place()) { histKeep(cur); cur.scroll = $("main").scrollTop; }
  S.histAt += step;
  const e = S.hist[S.histAt];
  const [view, path] = e.key.split("\u0000");
  const left = S.path.slice(), fromView = S.view;
  if (view === "clip") { S.view = view; S.clip = path; S.path = []; } else { S.view = view; S.path = path ? path.split("/") : []; }
  S.sel = {}; (e.sel || []).forEach((k) => { S.sel[k] = true; });
  S.anchor = e.anchor || null; S.cursor = e.cursor || null;
  S.q = e.q || ""; S.flat = !!e.flat; S.scope = e.scope || "here"; S.filters = Object.assign({}, e.filters || {});
  S.renaming = null; S.suggOpen = false;
  // Back to a folder above with nothing selected there: the one left, selected (as Finder has it)
  if (!Object.keys(S.sel).length && view === fromView && left.length > S.path.length && S.path.every((p, k) => p === left[k])) {
    const k = "f:" + left.slice(0, S.path.length + 1).join("/"); S.sel[k] = true; S.anchor = k;
  }
  const back = () => { if (S.hist[S.histAt] === e) $("main").scrollTop = e.scroll || 0; };
  if (view === "clip") {
    if (DATA.moments && DATA.stills) buildClip();
    else needViews(["footage", "moments", "stills"]).then(() => { if (S.view === "clip" && S.clip === path) { buildClip(); renderAll(); back(); } });
  } else if (!(view === "sources" || view === "footage" ? RAW_FOOTAGE : DATA[view])) loadView(view).then(() => { renderAll(); back(); }, () => {});
  renderAll();
  back();
  // What was selected and is gone since: let go
  if (view === "clip" || !albumViewOpen()) {
    const shown = new Set(VISIBLE), gone = Object.keys(S.sel).filter((k) => !shown.has(k));
    if (VISIBLE.length && gone.length) { gone.forEach((k) => delete S.sel[k]); if (S.anchor && !S.sel[S.anchor]) S.anchor = null; renderMain(); renderSide(); back(); }
  }
  return true;
}
// Back and Forward from outside the list (a mouse's side button, a key): not while a sheet or question is up
function historyFree() {
  if (!S.root || S.sheet) return false;
  for (const id of ["ask", "closing", "moving"]) { const el = $(id); if (el && !el.hidden) return false; }
  return true;
}
function historyStep(step) {
  if (!historyFree()) return;
  if (typeof closeMenu === "function") closeMenu();
  goHistory(step);
}
// The side buttons: taken on mouseup (as WebView2 navigates on it: kept from doing so), their mousedown kept from
// anything else (a selection, a drag)
document.addEventListener("mousedown", (e) => { if (e.button === 3 || e.button === 4) e.preventDefault(); });
document.addEventListener("mouseup", (e) => {
  if (e.button !== 3 && e.button !== 4) return;
  e.preventDefault();
  historyStep(e.button === 3 ? -1 : 1);
});
document.addEventListener("auxclick", (e) => { if (e.button === 3 || e.button === 4) e.preventDefault(); });
// ⌘[ ⌘] (Mac), Alt+← Alt+→ (Windows): in the search field too, as Safari's; not while a name is being typed in place
document.addEventListener("keydown", (e) => {
  if (e.target && e.target.id === "rename") return;
  const mac = S.macos;
  const back = mac ? e.metaKey && !e.shiftKey && !e.altKey && !e.ctrlKey && (e.key === "[" || e.code === "BracketLeft") : e.altKey && !e.ctrlKey && !e.metaKey && e.key === "ArrowLeft";
  const fwd = mac ? e.metaKey && !e.shiftKey && !e.altKey && !e.ctrlKey && (e.key === "]" || e.code === "BracketRight") : e.altKey && !e.ctrlKey && !e.metaKey && e.key === "ArrowRight";
  if (!back && !fwd) return;
  if (!historyFree()) return;
  e.preventDefault();
  e.stopImmediatePropagation();
  historyStep(back ? -1 : 1);
});
