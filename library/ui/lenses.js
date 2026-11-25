"use strict";
// The lens dialog (Source Settings › Lens › Choose lens…): the lenses of one camera mode, each with what Find lens and
// Estimate lens said of it on the clip (gyroflow-optical --lenses: vs-gyroflow's lens_find::choices), Find and Estimate
// run from here (Quick: Find stopped at the first lens that fits, on by default), and the one chosen used for the whole
// mode with the clips of it measured again. A sheet as Share and Add to an album are; the player has the same dialog
// (side panel, Source › Lens for the mode › Choose…). One click selects a lens.

// Quick (Find stopped at the first lens that fits): on unless switched off on this computer (app.js loads after this file)
function lensQuickOn() { if (S.lensQuick == null) S.lensQuick = remembered("afl.lensQuick", "1") === "1"; return S.lensQuick; }

// The selected clips a lens is chosen for: flat, of one mode, not with the lens in the file; the first is the one Find
// and Estimate run on
function lensSheetClips() {
  const flat = ctx().selItems.filter((it) => it.kind === "clip" && !it.is360 && !it.gyroflow && lensFrom(it) !== "file");
  const modes = new Set(flat.map((it) => (lensOf(it) || {}).mode_key || ""));
  return { flat, mode: modes.size === 1 ? Array.from(modes)[0] : "", modes: modes.size };
}

async function openLensSheet() {
  const c = lensSheetClips();
  if (!c.flat.length) return showError("Select clips to choose their lens");
  if (!c.mode) return showError("A lens is chosen for one camera mode at a time: select clips of one mode");
  S.lens = { mode: c.mode, path: c.flat[0].path, id: c.flat[0].id, name: c.flat[0].name, data: null, chosen: null, busy: false, after: null, seen: "" };
  S.sheet = "lens";
  renderSheet();
  await lensReload();
}

async function lensReload() {
  const l = S.lens;
  if (!l) return;
  try { l.data = await invoke("lens_choices", { path: l.path }); l.error = null; } catch (e) { l.error = String(e); }
  if (l.chosen == null && l.data) {
    const rows = lensRows(l.data);
    const i = rows.findIndex((r) => r.in_use);
    l.chosen = i >= 0 ? i : null;
  }
  if (S.sheet === "lens") renderSheet();
}

// Its rows in the dialog's order: what fits (an estimate first), what was tried and doesn't, what wasn't tried
function lensRows(d) {
  const all = (d && d.lenses) || [];
  const fits = (r) => r.fits || ["good", "single", "unchecked"].includes(r.verdict);
  const tried = (r) => r.verdict !== "untried";
  return all.filter((r) => tried(r) && fits(r)).concat(all.filter((r) => tried(r) && !fits(r)), all.filter((r) => !tried(r)));
}

const LENS_VERDICT = { good: ["ok", "fits"], single: ["ok", "fits, one part told"], unchecked: ["", "not checked"], loose: ["bad", "not reliable"], poor: ["bad", "doesn’t fit"], untried: ["", "not tried"] };

// The clips of the mode it would measure again (app.js's keptLensClips: the selected ones measured from the picture or
// with no motion data)
function lensMeasureCount() { const l = S.lens; return l ? keptLensClips(l.path, l.mode).length : 0; }

// The Find or Estimate of this dialog's clip in Tasks, the latest
function lensJob() {
  const l = S.lens;
  if (!l || l.after == null) return null;
  return (S.jobs || []).filter((j) => j.id > l.after && (j.kind === "lens-find" || j.kind === "lens-estimate")).slice(-1)[0] || null;
}

function renderLensSheet() {
  const l = S.lens;
  if (!l) return "";
  const d = l.data;
  const job = lensJob();
  const running = job && !["done", "stopped", "failed"].includes(job.state);
  const n = clipsOfMode(l.mode);
  let h = '<div class="scrim" data-a="scrim"><div class="sheet lenssheet" role="dialog" aria-modal="true" aria-labelledby="lensTitle">' +
    '<header><h2 id="lensTitle">Lens for ' + esc(l.mode) + '</h2><span class="sub wrap">' +
    esc("Kept for every clip of this mode" + (n ? ": " + n + " in this project" : "") + ".") +
    (d && d.now ? " Now: <b>" + esc(String(d.now.name || "no ready lens").replace(/_/g, " ")) + "</b>" + esc((d.now.readout_ms ? ", readout " + d.now.readout_ms.toFixed(1) + " ms" : "") + ".") : "") + '</span></header><div class="sb">';
  h += '<div class="lensbar"><button type="button" class="btn sm" data-a="lensfind"' + (running ? " disabled" : "") + '>Find lens</button>' +
    '<button type="button" class="btn sm" data-a="lensestimate"' + (running ? " disabled" : "") + '>Estimate lens</button>' +
    '<label class="switch"><input type="checkbox" data-a="lensquick"' + (lensQuickOn() ? " checked" : "") + '> Quick: stop at the first lens that fits</label></div>';
  if (running) {
    const pct = Math.round(((job.done || 0) + (job.fraction || 0)) / Math.max(1, job.total || 1) * 100);
    h += '<div class="lensprog"><div class="bar2"><i style="width:' + pct + '%"></i></div><span class="sub">' + esc((job.kind === "lens-find" ? "Finding" : "Estimating") + " on " + l.name + " · " + pct + " %") + '</span></div>';
  } else {
    h += '<span class="sub wrap">' + esc("Find tries the camera’s known lenses on " + l.name + " and measures their readout; Estimate makes a lens from it.") + '</span>';
  }
  if (job && job.state === "failed" && job.message) h += '<span class="sub wrap err">' + esc(job.message) + '</span>';
  if (l.error) h += '<span class="sub wrap err">' + esc(l.error) + '</span>';
  if (!d) h += '<span class="sub">Reading its lenses…</span>';
  else {
    const rows = lensRows(d);
    const f = d.found;
    const fits = (r) => r.fits || ["good", "single", "unchecked"].includes(r.verdict);
    const groups = [["Fits this clip", rows.filter((r) => r.verdict !== "untried" && fits(r))], ["Tried, doesn’t fit", rows.filter((r) => r.verdict !== "untried" && !fits(r))],
      [f && f.quick && f.untried ? "Not tried (Quick stopped at the first that fits)" : "Not tried", rows.filter((r) => r.verdict === "untried")]];
    for (const [title, list] of groups) {
      if (!list.length) continue;
      h += '<div class="lab">' + esc(title) + '</div><div class="albumlist lensrows" role="radiogroup">';
      for (const r of list) {
        const i = rows.indexOf(r);
        const v = LENS_VERDICT[r.verdict] || ["", r.verdict];
        const about = [r.source, r.readout_ms ? "readout " + r.readout_ms.toFixed(1) + " ms" + (r.readout_measured ? ", measured on this clip" : "") : "no readout",
          r.limit ? "moved " + r.moved.toFixed(1) + " of " + r.limit.toFixed(0) + " px" : "", r.refraction > 1 ? "under water" : ""].filter(Boolean).join(" · ");
        h += '<button type="button" class="albumrow' + (l.chosen === i ? " on" : "") + '" role="radio" aria-checked="' + (l.chosen === i) + '" data-a="lenspick" data-v="' + i + '">' +
          '<i class="rad"></i><span><b>' + esc(String(r.name).replace(/_/g, " ")) + '</b><span class="sub wrap">' + esc(about) + '</span></span>' +
          (r.in_use ? '<span class="pill">In use</span>' : "") + '<span class="pill ' + v[0] + '">' + esc(v[1]) + '</span></button>';
      }
      h += '</div>';
    }
    if (!rows.length) h += '<span class="sub wrap">No lens of this camera to choose from: Estimate lens makes one from the clip.</span>';
  }
  const chosen = d && l.chosen != null ? lensRows(d)[l.chosen] : null;
  const m = lensMeasureCount();
  const canUse = chosen && chosen.use && !l.busy && !running;
  const forget = !!(d && d.now && d.now.from === "own");
  h += '</div><footer>' + (forget ? '<button type="button" class="btn" data-a="lensforget"' + (l.busy ? " disabled" : "") + '>Forget the lens chosen for this mode</button>' : "") +
    '<span class="sp"></span><button type="button" class="btn" data-a="closesheet">Cancel</button>' +
    '<button type="button" class="btn pri" data-a="lensuse"' + (canUse ? "" : " disabled") + '>' + (l.busy ? "Keeping…" : m ? "Use and measure " + (m === 1 ? "the clip" : m + " clips") + " again" : "Use for this mode") + '</button></footer>';
  return h + '</div></div>';
}

async function lensRun(kind) {
  const l = S.lens;
  if (!l) return;
  const ids = kind === "lens-estimate" ? [l.id] : lensSheetClips().flat.filter((it) => (lensOf(it) || {}).mode_key === l.mode).map((it) => it.id);
  l.after = (S.jobs || []).reduce((m, j) => Math.max(m, j.id), 0);
  try {
    await invoke("add_job", { kind, options: { clips: ids, quick: lensQuickOn() }, only: ids, label: ids.length === 1 ? l.name : ids.length + " clips" });
  } catch (e) { l.after = null; showError(String(e)); }
  renderSheet();
}

async function lensUse() {
  const l = S.lens;
  const r = l && l.data ? lensRows(l.data)[l.chosen] : null;
  if (!r || !r.use || l.busy) return;
  l.busy = true; renderSheet();
  try {
    const out = r.use.keep != null ? await invoke("keep_found_lens", { path: l.path, n: String(r.use.keep) }) : await invoke("assign_lens", { path: l.path, id: r.use.assign });
    for (const x of Object.keys(LENS_CANDIDATES)) delete LENS_CANDIDATES[x];
    toast(String(out.lens || r.name).replace(/_/g, " ") + " is the lens for " + (out.mode || l.mode));
    await measureWithKept(l.path, l.mode);
    S.sheet = null; S.lens = null;
  } catch (e) { showError("The lens couldn’t be kept: " + e); l.busy = false; }
  renderSheet(); renderSide();
}

async function lensForget() {
  const l = S.lens;
  if (!l || l.busy) return;
  l.busy = true; renderSheet();
  try { await invoke("assign_lens", { path: l.path, id: null }); toast("The lens chosen for " + l.mode + " is forgotten"); l.chosen = null; }
  catch (e) { showError(String(e)); }
  l.busy = false;
  await lensReload();
  renderSide();
}

// The queue was read again (app.js's poll): what Find or Estimate found so far, as it comes
function lensTick() {
  const l = S.lens;
  if (S.sheet !== "lens" || !l) return;
  const job = lensJob();
  if (!job) return;
  const sig = job.state + ":" + JSON.stringify((job.results || []).map((r) => [(r.tried || []).length, !!r.estimate]));
  if (sig !== l.seen) { l.seen = sig; lensReload(); }
  else renderSheet();
}
