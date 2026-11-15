"use strict";
// "Open at start" (src/main.rs: set_start_project, open_at_start, close_project): one project of the list of those
// opened here is the one the app opens when it's started with no file, skipping the start screen. The choice is this
// computer's, kept in the app's own data (with the list), never in the project. A tick on the project's menu (for the
// one open) and on each row of the start screen's list sets it; ticking another moves it; the tick again takes it off.
// The start screen stays a menu item away. Loaded before app.js, which calls startMenu, startRow and startOpen.

// The project menu's part for it: the tick for the project open, and the way to the start screen
function startMenu() {
  if (!S.root) return "";
  const on = S.recent.some((r) => r.path === S.root && r.at_start);
  return '<button type="button" role="menuitemcheckbox" aria-checked="' + on + '" data-s="start" data-v="' + esc(on ? "" : S.root) + '" title="When the app is started with no file, this project opens at once. Kept on this computer.">' +
    '<span class="ck">' + (on ? ICON.check : "") + '</span>Open at start</button>' +
    '<button type="button" role="menuitem" data-s="screen"><span class="ck"></span>Start screen</button>';
}
// A row of the list of projects opened before: on the start screen its tick; in the menu only a word on the one marked
function startRow(r, menu) {
  if (menu) return r.at_start ? '<span class="chip fast rstart">At start</span>' : "";
  return '<button type="button" role="checkbox" aria-checked="' + !!r.at_start + '" class="rstart' + (r.at_start ? " on" : "") + '" data-s="start" data-v="' + esc(r.at_start ? "" : r.path) + '" title="When the app is started with no file, this project opens at once' +
    (r.here ? "" : " (once its share is connected; until then it shows here, offline)") + '. Kept on this computer.">' + '<span class="box" aria-hidden="true">' + (r.at_start ? ICON.check : "") + '</span>Open at start</button>';
}
// The start screen's line about it
function startHint() {
  const el = $("startHint");
  if (!el) return;
  const r = S.recent.find((x) => x.at_start);
  el.hidden = !r;
  el.textContent = !r ? "" : r.here ? r.name + " opens when the app starts. Untick it below to start here instead."
    : r.name + " opens when the app starts, but it's offline now" + (r.connect ? ": connect its share below." : ".");
}
// The app started with no file: the one marked, when it's reachable (asked once, as the page comes up)
async function startOpen() {
  const p = await invoke("open_at_start").catch(() => null);
  if (p) await poll();
  startHint();
  return p;
}
// The tick: on that project (v: its path), or off (v empty)
async function startSet(path) {
  try { await invoke("set_start_project", { path: path || null }); } catch (e) { showError(String(e)); }
  S.recent = await invoke("recent").catch(() => S.recent);
  renderAll();
  startHint();
}
document.addEventListener("click", async (e) => {
  const el = e.target.closest ? e.target.closest("[data-s]") : null;
  if (!el) return;
  e.stopPropagation();
  if (el.dataset.s === "start") return startSet(el.dataset.v);
  if (el.dataset.s === "screen") {
    S.menu = false;
    await invoke("close_project").catch((er) => showError(String(er)));
    S.recent = await invoke("recent").catch(() => S.recent);
    await poll();
    renderAll();
    startHint();
  }
});
