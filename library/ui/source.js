"use strict";
// Source settings and the lens.
// One of the window's scripts that were app.js: index.html loads them in this order before app.js, as classic
// scripts sharing their top-level names, so each uses the others' when called (only earlier ones at load).

// ---------- Source settings ----------
// How each original is read, as a list of its source's attributes (Anton, 2026-11-24): each shows what it is now for
// the clips selected (their one value, or Mixed with how many have which) and is changed by choosing another and Save,
// for all of them at once. An attribute that doesn't apply to some of them (a lens type where the camera has none, the
// underwater colour for a log clip) is changed only for the ones it applies to. Kept in each clip's own file beside it
// (`source`, src/tags.rs set_source; the player's Source tab shows and changes the same): the turn, the log curve, the
// underwater colour, the lens type, the lens under water. A lens chosen for a mode is the player's own lens
// database's: every clip of that mode takes it. What's read from the clip itself (its camera, colour, lens, motion
// data) is the index's (gyroflow-probe --library, its lens_info too): nothing is read from the clip to show this.
// Stabilize optically and Find / Estimate lens are tasks, started at once from their attribute's row.
// (the log curves: registry.json's, window.AFP_LISTS)
const SRC_CURVES = [["auto", "From the clip"]].concat(window.AFP_LISTS.curves.filter((c) => c.log).map((c) => [c.id, c.name]));
const SRC_TURNS = [["0", "As is"], ["90", "90° ↻"], ["180", "180°"], ["270", "90° ↺"]];
const SRC_WATER = [["auto", "Auto: as the camera says"], ["yes", "Yes (a flat port)"], ["no", "No"]];
const SRC_UW = [["off", "Off"], ["on", "On"], ["bright", "On, brighter highlights"]];
const SRC_NAMES = { rotation: "Recorded turned", curve: "Log curve", uw: "Underwater color", uw_strength: "Strength", lens_type: "Lens type", underwater: "Under water", lens: "Lens" };
function srcClips(c) { return c.selItems.filter((it) => it.kind === "clip"); }
function srcOf(it) { const s = (it.own || {}).source; return s && typeof s === "object" ? s : {}; }
function lensTypeOf(it) { return srcOf(it).lens_type || ""; }
// Whether the clip's camera records when it's in the water (gyroflow-probe's "water", vs-gyroflow's water.rs): only
// then can the lens under water follow it (Auto); null: not read yet
function recordsWater(it) { const p = PROBES[it.path]; return p ? p.water === true : null; }
// The clip's lens as the player takes it (the index's lens_info: gyroflow-probe --lens without the candidates);
// null while it isn't read (or read by a player from before it)
function lensOf(it) { const p = PROBES[it.path]; return p && p.lens_info && typeof p.lens_info === "object" ? p.lens_info : null; }
function lensTypesOf(it) { const l = lensOf(it); return l && Array.isArray(l.types) ? l.types.map((t) => [t.id, t.label || t.id]) : []; }
// Where its lens comes from: project (its Gyroflow project's), file, own (the player's own lenses: chosen or
// estimated for its mode), list (the curated list), database (Gyroflow's, made for exactly its mode), "" none ready;
// null: not read yet
function lensFrom(it) {
  if (it.gyroflow) return "project";
  const l = lensOf(it);
  if (!l) return null;
  if (l.lens === "in the file") return "file";
  return l.from === "file" ? "file" : (l.lens ? l.from || "database" : "");
}
const LENS_FROM = { project: "The Gyroflow project’s", file: "In the file", clip: "Tried on this clip only", own: "The player’s own, for its mode", builtin: "Built into the player, for its mode", list: "From the curated list", database: "Gyroflow’s database", "": "No ready lens" };
function lensName(it) {
  const l = lensOf(it);
  if (it.gyroflow) return l && l.project && l.project.lens ? String(l.project.lens).replace(/_/g, " ") : "The Gyroflow project’s";
  if (!l) return null;
  if (l.lens === "in the file") return "From the file";
  if (l.lens) return String(l.lens).replace(/_/g, " ");
  const t = l.type || l.recorded || l.default;
  const lb = t ? (((l.types || []).find((x) => x.id === t) || {}).label || t) : "";
  const inside = [lb, l.mode != null ? "mode " + l.mode : ""].filter(Boolean).join(", ");
  return "No ready lens" + (inside ? " (" + inside + ")" : "");
}
// The underwater colour can play: a plain SDR clip (neither log nor HDR, no curve set by hand), as the dlogm plugin's uw_ok
function uwOk(it) { const s = srcOf(it); return !!it.probed && !it.colorGroup && (!s.curve || s.curve === "auto"); }
function uwKey(it) { const s = srcOf(it); return s.uw === true ? (s.uw_highlights === true ? "bright" : "on") : "off"; }
// Its motion data: gyro (in the file, played), pic (measured from the picture, .afp-motion), project (a Gyroflow
// project), cam (none: the camera stabilized it itself), none; null: not read yet
function motionKey(it) {
  if (it.gyroflow) return "project";
  // Measured without its lens's rolling shutter (a built-in lens's readout went unused before): to be measured again
  if (it.measured) return it.readoutLeftOut ? "picro" : "pic";
  if (it.motion === true) return "gyro";
  if (it.motion === false) return it.camStab ? "cam" : "none";
  return null;
}
const MOTION_TEXT = { gyro: "Gyro in the file", pic: "Measured from the picture", picro: "Measured without the rolling shutter: measure again", project: "A Gyroflow project", cam: "None: stabilized in the camera", none: "None" };
function tally(clips, f) { const m = new Map(); clips.forEach((it) => { const v = f(it); if (v == null) return; m.set(v, (m.get(v) || 0) + 1); }); return m; }
// A label inside a sentence: its first letter small, not a name's ("D-Log M", "Gyroflow’s database")
function lc(t) { t = String(t); return /^(Gyroflow|[A-Z][^a-z])/.test(t) ? t : t.charAt(0).toLowerCase() + t.slice(1); }
function tallyText(m, label) { return Array.from(m).map(([v, n]) => n + " " + (label ? label(v) : v)).join(" · "); }
const srcRow = (name, ctl, prov, cls, extra) => '<div class="attr' + (cls ? " " + cls : "") + '"><div class="an">' + esc(name) + '</div><div class="ac">' + ctl +
  (prov ? '<div class="prov">' + prov + '</div>' : "") + (extra || "") + '</div></div>';
const srcVal = (t) => '<div class="val">' + t + '</div>';
const srcGroup = (t) => '<div class="agrp">' + esc(t) + '</div>';
const HAND = '<span class="hand">set</span>';
// One attribute chosen from a list. `on`: the clips it applies to; now(it): each one's value; label(v, one): a value's
// words (one: the only clip, for its own details). The first option is what they have now (Mixed: what each has,
// left as is); choosing another is a change, Save writes it. What's shown under it: where the value comes from, how many
// have which, and how many it isn't for
function srcChoice(id, name, choices, clips, on, now, label, o) {
  o = o || {};
  if (!on.length) return srcRow(name, '<select disabled><option>—</option></select>', esc(o.none || "Not for " + (clips.length === 1 ? "this clip" : "these clips")), "na");
  const m = tally(on, now);
  const one = m.size === 1 ? m.keys().next().value : null;
  const single = on.length === 1 ? on[0] : null;
  const draft = S.srcDraft[id];
  const head = one != null ? label(one, single) : "Mixed: leave as is";
  const opts = '<option value=""' + (draft ? "" : " selected") + '>' + esc(head) + '</option>' +
    choices.filter((x) => x[0] !== one).map((x) => '<option value="' + esc(x[0]) + '"' + (draft === x[0] ? " selected" : "") + (x[2] ? " disabled" : "") + '>' + esc(x[1]) + '</option>').join("");
  const sel = '<select id="src-' + id + '" data-a="srcset" data-v="' + id + '" aria-label="' + esc(name) + '">' + opts + '</select>';
  let prov = o.prov ? o.prov(one, m, single) : one != null ? (on.length > 1 ? "All " + on.length : "") : esc(tallyText(m, (v) => lc(label(v, null))));
  if (on.length < clips.length) prov += (prov ? " · " : "") + esc("not for " + (clips.length - on.length) + (o.notFor ? ": " + o.notFor : ""));
  const was = draft ? '<div class="was">Was: ' + esc(one != null ? label(one, single) : tallyText(m, (v) => lc(label(v, null)))) + '</div>' : "";
  return srcRow(name, sel, prov, (draft ? "chg" : one == null ? "mixed" : ""), (o.extra || "") + was);
}
// The lenses of a mode to choose from (gyroflow-probe --lens's "candidates": its camera's lenses, the ones made for
// exactly its mode first), asked once per mode when one mode's clips are selected
const LENS_CANDIDATES = {};
function lensCandidates(key, path) {
  if (!(key in LENS_CANDIDATES)) {
    LENS_CANDIDATES[key] = null;
    invoke("lens_info", { path }).then((r) => { LENS_CANDIDATES[key] = r && Array.isArray(r.candidates) ? r.candidates : []; }, () => { LENS_CANDIDATES[key] = []; })
      .finally(() => { if (S.tab === "source") renderSide(); });
  }
  return LENS_CANDIDATES[key];
}
// How many clips the project has of a mode (the index's lens_info), for what a lens chosen for it reaches
function clipsOfMode(key) { return ((RAW_FOOTAGE && RAW_FOOTAGE.items) || []).filter((it) => { const l = lensOf(it); return l && l.mode_key === key; }).length; }
function sourceBody(c) {
  const clips = srcClips(c);
  const key = clips.map((it) => it.id).join("\n");
  if (S.srcFor !== key) { S.srcFor = key; S.srcDraft = {}; }
  if (!clips.length) return '<p class="sub wrap">' + (c.any ? "No clips in the selection: source settings are the originals’." : "Select clips (or folders of them) to see how they’re read and change it: one, or many at once.") + '</p>';
  const n = clips.length, one = n === 1 ? clips[0] : null, d = S.srcDraft;
  const probeOf = (it) => PROBES[it.path];
  let h = '<div class="attrs">';

  // Camera: what recorded it (from the clip), and the turn it was recorded with
  h += srcGroup("Camera");
  const cams = tally(clips, (it) => it.cam || (it.probed ? "Unknown" : "Reading…"));
  h += srcRow("Camera", srcVal(cams.size === 1 ? esc(cams.keys().next().value) : "<i>Mixed</i>"),
    one ? esc([resText(probeOf(one)), one.is360 ? "360°" : one.probed ? "flat" : ""].filter(Boolean).join(" · ")) : cams.size > 1 ? esc(tallyText(cams)) : "All " + n);
  const turnLabel = (v) => (SRC_TURNS.find((x) => x[0] === v) || [v, v + "°"])[1];
  const gf = clips.filter((it) => it.gyroflow).length;
  h += srcChoice("rotation", "Recorded turned", SRC_TURNS, clips, clips, (it) => String(Number(srcOf(it).rotation) || 0), turnLabel, {
    prov: (v, m) => v != null ? (v === "0" ? (n > 1 ? "All " + n + " as recorded" : "As recorded") : "Set " + HAND + (n > 1 ? " for all " + n : "")) + (gf ? esc(" · " + gf + " with a Gyroflow project: its turn plays") : "")
      : esc(tallyText(m, (x) => lc(turnLabel(x)))) });

  // Color: as recorded, the log curve it's read with, and the underwater colour (the source's too: it takes out what
  // the camera put in, and plays in HDR)
  h += srcGroup("Color");
  const recText = (it) => it.colorGroup === "Log" ? "Log, " + it.color : it.colorGroup === "HDR" ? "HDR, " + it.color : it.probed ? "SDR" + (it.color && it.color !== "Normal" ? ", " + it.color : "") : "Reading…";
  const recs = tally(clips, recText);
  h += srcRow("Recorded", srcVal(recs.size === 1 ? esc(recs.keys().next().value) : "<i>Mixed</i>"), recs.size > 1 ? esc(tallyText(recs)) : n > 1 ? "All " + n : "");
  const curveLabel = (v, it) => v === "auto" ? "From the clip" + (it ? ": " + (it.colorGroup === "Log" ? it.color : "not log") : "") : (SRC_CURVES.find((x) => x[0] === v) || [v, v])[1];
  h += srcChoice("curve", "Log curve", SRC_CURVES, clips, clips, (it) => srcOf(it).curve || "auto", curveLabel, {
    prov: (v, m) => v != null ? (v === "auto" ? "As recorded" : "Set " + HAND) : esc(tallyText(m, (x) => lc(curveLabel(x, null)))) });
  const uwOn = clips.filter(uwOk);
  const strengths = tally(uwOn.filter((it) => uwKey(it) !== "off"), (it) => Math.round((typeof srcOf(it).uw_strength === "number" ? srcOf(it).uw_strength : 1) * 100));
  const uwNow = tally(uwOn, uwKey);
  const uwShown = d.uw || (uwNow.size === 1 ? uwNow.keys().next().value : null);
  let uwExtra = "";
  if (uwOn.length && uwShown && uwShown !== "off") {
    const st = d.uw_strength != null ? Math.round(d.uw_strength * 100) : strengths.size === 1 ? strengths.keys().next().value : strengths.size ? null : 100;
    uwExtra = '<div class="rng">Strength <input type="range" min="0" max="100" step="5" id="src-uwstrength" data-a="srcstrength" value="' + (st == null ? 100 : st) + '" aria-label="Strength"> <span>' + (st == null ? "Mixed" : st + " %") + '</span></div>' +
      (d.uw_strength != null ? '<div class="was">Strength was ' + esc(strengths.size ? tallyText(strengths, (x) => x + " %") : "100 %") + '</div>' : "");
  }
  h += srcChoice("uw", "Underwater color", SRC_UW, clips, uwOn, uwKey, (v) => (SRC_UW.find((x) => x[0] === v) || [v, v])[1], {
    notFor: "log or HDR (their curve’s own look)",
    none: (n === 1 ? "Not for this clip" : "Not for these clips") + ": log or HDR, the curve’s own look",
    prov: (v, m) => (v != null ? (v === "off" ? "Takes out the green and brown the camera put in under water" : "Plays in HDR: the color and the light stretched past SDR’s (HLG, Rec.2020)" + (srcOf(uwOn[0]).uw != null ? " " + HAND : ""))
      : esc(tallyText(m, (x) => (SRC_UW.find((y) => y[0] === x) || [x, x])[1]))),
    extra: uwExtra });

  // Lens: the lens it's corrected with (the index's), a lens for one mode chosen here (the player's own database),
  // the lens type where the camera has types, and the lens under water with a flat port
  h += srcGroup("Lens");
  const flat = clips.filter((it) => !it.is360);
  const reading = flat.filter((it) => lensFrom(it) == null).length;
  if (!flat.length) h += srcRow("Lens", srcVal("—"), "360°: its lenses are the rig’s", "na");
  else {
    const names = tally(flat, (it) => lensName(it) || "Reading…");
    const froms = tally(flat.filter((it) => lensFrom(it) != null), lensFrom);
    const modes = new Set(flat.map((it) => (lensOf(it) || {}).mode_key || ""));
    const modeKey = modes.size === 1 && !gf ? Array.from(modes)[0] : "";
    // The camera's own calibration in the file (a GoPro from the HERO11, a DJI): no lens chosen, found or estimated for
    // those (Anton, 2026-11-24)
    const inFile = flat.filter((it) => lensFrom(it) === "file").length;
    const findable = flat.length - inFile;
    let ctl, prov = "", extra = "";
    if (inFile && !findable) {
      ctl = srcVal(names.size === 1 ? esc(names.keys().next().value) : "<i>Mixed</i>");
      prov = esc(LENS_FROM.file + " · the camera’s own calibration of " + (flat.length === 1 ? "this clip" : "each clip") + ", which " + (flat.length === 1 ? "it plays" : "they play") + " with: no lens is chosen, found or estimated for it");
    } else {
      // The lens, where it comes from, and Choose lens… (lenses.js: the lens dialog, with Find and Estimate in it)
      ctl = srcVal(names.size === 1 ? esc(names.keys().next().value) : "<i>Mixed</i>");
      const assigned = flat.some((it) => lensFrom(it) === "own");
      const others = modeKey ? clipsOfMode(modeKey) : 0;
      prov = esc(names.size > 1 ? tallyText(froms, (x) => lc(LENS_FROM[x])) : froms.size === 1 ? LENS_FROM[froms.keys().next().value] : "") + (assigned ? " " + HAND : "") +
        (reading ? esc(" · " + reading + " not read yet") : gf ? esc(" · its Gyroflow project’s plays")
          : modes.size > 1 ? esc(" · a lens is chosen for one mode at a time: select clips of one mode")
          : froms.size === 1 && froms.has("clip") ? esc(" · its mode’s other clips keep theirs; Choose lens… makes it the mode’s, or forgets it")
          : esc(" · kept for every clip of " + modeKey + (others > flat.length ? " (" + others + " in this project)" : "")));
      if (one && one.gyroflow) extra += projectLensHtml(one);
      if (findable && !gf) extra += '<div class="btns"><button type="button" class="btn sm" data-a="lenssheet"' + (modeKey && !reading ? "" : " disabled") + '>Choose lens…</button></div>' +
        (inFile ? '<div class="prov">' + esc("Left out: " + inFile + (inFile === 1 ? " clip" : " clips") + " with the lens in the file") + '</div>' : "");
    }
    h += srcRow("Lens", ctl, prov, d.lens ? "chg" : names.size > 1 ? "mixed" : "", extra);
  }
  const typed = flat.filter((it) => lensTypesOf(it).length);
  const types = [];
  typed.forEach((it) => lensTypesOf(it).forEach((t) => { if (!types.some((x) => x[0] === t[0])) types.push(t); }));
  const typeLabel = (v, it) => {
    if (v !== "recorded") return (types.find((x) => x[0] === v) || [v, v])[1];
    const l = it && lensOf(it), r = l && (l.recorded || l.default);
    return "As recorded" + (r ? " (" + ((types.find((x) => x[0] === r) || [r, r])[1]) + ")" : "");
  };
  h += srcChoice("lens_type", "Lens type", [["recorded", "As the camera recorded"]].concat(types), clips, typed, (it) => lensTypeOf(it) || "recorded", typeLabel, {
    none: (n === 1 ? "This camera has" : "These cameras have") + " no lens types to choose", notFor: "no lens types",
    prov: (v, m) => v != null ? (v === "recorded" ? (typed.length > 1 ? "All " + typed.length + " as recorded" : "As recorded") : "Set " + HAND) : esc(tallyText(m, (x) => lc(typeLabel(x, null)))) });
  const rec = clips.filter((it) => recordsWater(it) === true).length;
  const waterLabel = (v, it) => v === "unset" ? "Not set: " + (it ? (recordsWater(it) ? "Auto" : "No") : !rec ? "No" : rec === n ? "Auto" : "Auto where the camera records it, else No") : (SRC_WATER.find((x) => x[0] === v) || [v, v])[1];
  h += srcChoice("underwater", "Under water", [["auto", rec ? (rec < n ? "Auto (" + rec + " of " + n + "; others No)" : SRC_WATER[0][1]) : "Auto: not for " + (n === 1 ? "this camera" : "these cameras"), !rec]].concat(SRC_WATER.slice(1)),
    clips, clips, (it) => srcOf(it).underwater || "unset", waterLabel, {
      prov: (v, m) => (v != null ? (v === "unset" ? "As the camera says" : "Set " + HAND) : esc(tallyText(m, (x) => lc(waterLabel(x, null))))) +
        esc(rec === n ? "" : rec ? " · " + (n - rec) + " of the cameras don’t record when they’re in the water" : (n === 1 ? " · the camera doesn’t" : " · the cameras don’t") + " record when " + (n === 1 ? "it’s" : "they’re") + " in the water") +
        (gf ? esc(" · " + gf + " with a Gyroflow project: its own plays") : "") });

  // Motion data: what the player stabilizes it with, and the task that measures it from the picture where there's
  // none it can use (Stabilize optically), or measures it again
  h += srcGroup("Motion data");
  const motions = tally(clips, motionKey);
  const op = opticalPlan(c);
  let mprov = "";
  if (one) {
    const p = probeOf(one) || {};
    mprov = one.measured ? "In " + esc(one.name.replace(/\.[^.]+$/, "")) + ".afp-motion" + (p.measured && p.measured.in_water ? ", measured in water" : ", measured in air") : "";
    if (one.readoutLeftOut) mprov += esc(" · measured without its lens’s rolling shutter (" + Number(one.readoutLeftOut).toFixed(1) + " ms): the edges wobble. Measure again");
    if (one.camStab) mprov += (mprov ? " · " : "") + esc("the camera stabilized it itself (" + one.camStab + ")" + (one.motion === false ? ": its gyro can’t be used" : ""));
    // Its own motion data plays (a HERO12+ with HyperSmooth on): a motion file measured before is left aside
    if (p.measured_aside) mprov += (mprov ? " · " : "") + esc(one.name.replace(/\.[^.]+$/, "") + ".afp-motion beside it is left aside: its own motion data plays");
  } else if (motions.size > 1) mprov = esc(tallyText(motions, (x) => lc(MOTION_TEXT[x])));
  else if (motions.size === 1) mprov = "All " + n;
  let mbtn = "";
  const go = op.go.length - op.redo;
  if (go > 0) mbtn += '<button type="button" class="btn sm" data-a="job" data-v="optical">Stabilize ' + (go === 1 ? (one ? "" : "1 clip ") : go + " clips ") + 'optically</button>';
  if (op.measured) mbtn += '<button type="button" class="btn sm" data-a="remeasure">Measure ' + (op.measured === 1 ? (one ? "" : "1 clip ") : op.measured + " clips ") + 'again</button>';
  const verb = (k, one1, many) => k === 1 ? one1 : many;
  const out = [[op.own, verb(op.own, "has gyro the player uses", "have gyro the player uses")], [op.project, verb(op.project, "has a Gyroflow project", "have a Gyroflow project")],
    [op.s360, verb(op.s360, "is 360°", "are 360°")], [op.lens, verb(op.lens, "needs a lens first: Find lens", "need a lens first: Find lens")]].filter((x) => x[0]);
  h += srcRow("Motion data", srcVal(motions.size === 1 ? esc(MOTION_TEXT[motions.keys().next().value]) : motions.size ? "<i>Mixed</i>" : "Reading…"), mprov,
    "", (mbtn ? '<div class="btns">' + mbtn + '</div>' : "") + (out.length && !one ? '<div class="prov">' + esc("Left out: " + out.map((x) => x[0] + " " + x[1]).join(", ")) + '</div>'
      : one && op.lens ? '<div class="prov">It needs a lens first: Find lens</div>' : "") +
      (mbtn ? '<div class="prov">Measured from the picture in the background (about a third of the clip’s length), kept beside each clip in its .afp-motion</div>' : ""));
  h += '</div>';

  // Save: the changes, for the clips each applies to
  const changes = srcChanges(clips);
  h += '<div class="savebar">' + (changes.length ? '<div class="what">' + esc(changes.join(" · ")) + '</div>' : "") +
    '<div class="btns"><button type="button" class="btn sm pri" data-a="srcapply"' + (changes.length ? "" : " disabled") + '>Save' + (changes.length ? (n === 1 ? " for the clip" : " for " + n + " clips") : "") + '</button>' +
    '<button type="button" class="btn sm" data-a="srcclear"' + (changes.length ? "" : " disabled") + '>Undo changes</button></div></div>';
  return h;
}
// What Save will do, in words: "Under water Yes, for all 6", "Lens type Wide, for 3 of 6"
function srcChanges(clips) {
  const d = S.srcDraft, n = clips.length, out = [];
  const forN = (k) => k === n ? (n === 1 ? "" : ", for all " + n) : ", for " + k + " of " + n;
  const lab = (list, v) => (list.find((x) => x[0] === v) || [v, v])[1];
  if (d.rotation) out.push("Recorded turned " + lab(SRC_TURNS, d.rotation) + forN(n));
  if (d.curve) out.push("Log curve " + lab(SRC_CURVES, d.curve) + forN(n));
  const uw = clips.filter(uwOk).length;
  if (d.uw) out.push("Underwater color " + lab(SRC_UW, d.uw) + forN(uw));
  if (d.uw_strength != null) out.push("strength " + Math.round(d.uw_strength * 100) + " %" + forN(uw));
  const typeName = (t) => { for (const it of clips) { const x = lensTypesOf(it).find((y) => y[0] === t); if (x) return x[1]; } return t; };
  if (d.lens_type) out.push("Lens type " + (d.lens_type === "recorded" ? "as recorded" : typeName(d.lens_type)) + forN(clips.filter((it) => lensTypesOf(it).some((t) => d.lens_type === "recorded" || t[0] === d.lens_type)).length));
  if (d.underwater) out.push("Under water " + lab(SRC_WATER, d.underwater).replace(/:.*$/, "") + forN(n));
  if (d.lens) {
    const key = (lensOf(clips[0]) || {}).mode_key || "the mode";
    out.push((d.lens === "-" ? "The lens chosen for " + key + " forgotten" : "Lens for every clip of " + key) + " (" + clipsOfMode(key) + " in this project)");
  }
  return out;
}
function srcDraftSet(k, v) { if (v) S.srcDraft[k] = v; else delete S.srcDraft[k]; renderSide(); }
async function applySource() {
  const clips = srcClips(ctx());
  const d = Object.assign({}, S.srcDraft);
  if (!clips.length || !Object.keys(d).length) return;
  let toNo = 0;
  const entries = clips.map((it) => {
    const s = {};
    if (d.curve) s.curve = d.curve;
    if (d.rotation) s.rotation = +d.rotation;
    if (d.underwater) {
      // Auto where the camera doesn't record the water: No (Anton, 2026-11-11), as the row says
      if (d.underwater === "auto" && recordsWater(it) !== true) { s.underwater = "no"; toNo++; } else s.underwater = d.underwater;
    }
    if (d.lens_type) {
      const types = lensTypesOf(it);
      if (d.lens_type === "recorded") { if (lensTypeOf(it)) s.lens_type = null; }
      else if (types.some((t) => t[0] === d.lens_type)) s.lens_type = d.lens_type;
    }
    // The underwater colour only where it can play (a plain SDR clip); with a curve chosen now, as that one is
    if (uwOk(it) && (!s.curve || s.curve === "auto")) {
      if (d.uw) { s.uw = d.uw !== "off"; if (d.uw !== "off") s.uw_highlights = d.uw === "bright"; }
      if (d.uw_strength != null) s.uw_strength = Math.round(d.uw_strength * 100) / 100;
    }
    return { clip: it.id, source: s };
  }).filter((e) => Object.keys(e.source).length);
  S.srcDraft = {};
  if (toNo) toast("Under water No for " + toNo + (toNo === 1 ? " clip (its camera doesn’t" : " clips (their cameras don’t") + " record it)");
  // The lens chosen for the mode: the player's own database's (every clip of the mode)
  if (d.lens) assignLens(clips.find((it) => !it.is360 && !it.gyroflow).path, d.lens);
  if (!entries.length) { renderSide(); return; }
  // Shown at once, written after (writes.js)
  await Writes.run("source", clips.filter((it) => entries.some((e) => e.clip === it.id)), { entries });
}

// Stabilize optically (vs-gyroflow's optical.rs and measure.rs, run by gyroflow-optical): the camera's motion measured
// from the picture for clips whose motion data the player can't use, kept beside each clip in <clip>.afp-motion. Which
// of the selected clips, as far as the index tells; the job reads each one again (its lens) and lists what it left out.
// `redo`: the ones measured already are measured again (Measure again)
function opticalPlan(c, redo) {
  const out = { go: [], paths: [], all: [], own: 0, project: 0, measured: 0, s360: 0, lens: 0, redo: 0 };
  for (const it of c.selItems) {
    if (it.kind !== "clip") continue;
    const p = PROBES[it.path];
    out.all.push(it.path);
    if (it.is360) { out.s360++; continue; }
    // A Gyroflow project plays whatever is measured here
    if (it.gyroflow) { out.project++; continue; }
    if (p && p.mode === "default" && p.motion !== "optical") { out.own++; continue; }
    if (p && p.motion === "optical") { out.measured++; if (!redo) continue; out.redo++; }
    else if (p && p.mode == null && p.lens === "" && lensFrom(it) === "") { out.lens++; continue; }
    out.go.push(it.id);
    out.paths.push(it.path);
  }
  if (!redo) out.redo = 0;
  return out;
}
// The lens of a clip's Gyroflow project (the index's lens_info "project") and the button that keeps it in the user's
// lens list as the default for the clip's camera, mode and size: every clip of them without a lens of its own takes it
const LENS_SAVING = {};   // a clip's path -> true while its project's lens is being saved
function lensKeyText(p, cam) {
  return [cam || p.camera, p.type_label, String(p.size || "").replace("x", " × ")].filter(Boolean).join(" · ");
}
function projectLensHtml(it) {
  if (!it.gyroflow) return "";
  const r = lensOf(it);
  const p = r && r.project;
  if (!p) return "";
  if (!p.lens) return '<div class="prov">' + esc(p.error || "") + '</div>';
  if (p.error) return '<div class="prov">Can’t be kept as a default: ' + esc(p.error) + '</div>';
  if (p.saved) return '<div class="prov">The default for ' + esc(lensKeyText(p, it.cam)) + '</div>';
  const busy = LENS_SAVING[it.path];
  return '<div class="btns lensave"><button type="button" class="btn sm" data-a="savelens" data-v="' + esc(it.path) + '" data-cam="' + esc(it.cam || "") + '"' + (busy ? " disabled" : "") +
    ' title="Kept in your lens list: every clip of this camera, mode and size without a lens of its own (in the file, a lens type chosen, a Gyroflow project) takes it">' +
    (busy ? '<span class="spin" aria-hidden="true"></span> Saving…' : "Save as the default for " + esc(lensKeyText(p, it.cam))) + '</button></div>';
}
// A lens chosen for the clip's whole mode kept in the player's own lens database (gyroflow-probe --assign-lens): every
// clip of the same camera, mode, in-camera stabilization, size and frame rate takes it; the index reads them again
const LENS_ASSIGNING = {};
async function assignLens(path, id) {
  if (!id || LENS_ASSIGNING[path]) return;
  LENS_ASSIGNING[path] = true; renderSide();
  try {
    const r = await invoke("assign_lens", { path, id: id === "-" ? null : id });
    for (const k of Object.keys(LENS_CANDIDATES)) delete LENS_CANDIDATES[k];
    toast((id === "-" ? "Lens taken away from " : String(r.lens || "The lens").replace(/_/g, " ") + " kept for ") + (r.mode || "the mode") + (r.again ? ": " + r.again + (r.again === 1 ? " clip" : " clips") + " of the camera read again" : ""));
  } catch (e) { showError("The lens couldn’t be kept: " + e); }
  finally { delete LENS_ASSIGNING[path]; renderSide(); }
}
async function saveProjectLens(path, cam) {
  if (LENS_SAVING[path]) return;
  LENS_SAVING[path] = true; renderSide();
  try {
    const r = await invoke("save_project_lens", { path });
    for (const k of Object.keys(LENS_CANDIDATES)) delete LENS_CANDIDATES[k];
    toast(String(r.lens || "The lens").replace(/_/g, " ") + " is the default for " + lensKeyText(r, cam) + (r.again ? ": " + r.again + (r.again === 1 ? " clip" : " clips") + " of the camera read again" : ""));
  } catch (e) { showError("The lens couldn’t be saved: " + e); }
  finally { delete LENS_SAVING[path]; renderSide(); }
}
// What Find lens / Estimate lens found (vs-gyroflow's lens_find.rs, run by gyroflow-optical: Gyroflow's
// lens_from_video), from the latest of these tasks of the project, with a button that keeps one for the clip's whole
// mode (the player's own lens database: every clip of the same camera, mode, size and frame rate takes it)
const LENS_KEEPING = {};
function lensFindResults(only) {
  const job = only || (S.jobs || []).filter((j) => (j.kind === "lens-find" || j.kind === "lens-estimate") && j.results).slice(-1)[0];
  if (!job || !job.results) return "";
  const name = (p) => String(p || "").split("/").pop();
  const verdictText = { good: "fits", single: "fits (one part could tell)", loose: "not reliable: fits only loosely", poor: "doesn’t fit", unchecked: "not checked on another part" };
  // Kept only when it fits (or an estimate no other part of a short clip could judge): a loose fit isn't reliable
  const keepBtn = (r, t) => {
    if (!(t.fits || t.verdict === "unchecked")) return "";
    const k = r.path + "#" + t.n;
    // Kept and the clips measured again with it, in one go (GOPR1585: kept, then Measure again forgotten)
    return '<button type="button" class="btn sm pri" data-a="keepfound" data-v="' + esc(r.path) + '" data-n="' + esc(t.n) + '" data-m="' + esc(r.mode || "") + '"' + (LENS_KEEPING[k] ? " disabled" : "") + '>' + (LENS_KEEPING[k] ? "Keeping…" : "Use for this mode and measure again") + '</button>';
  };
  const lensLine = (r, t) => '<div class="lensfound"><span class="blk">' + esc(String(t.name).replace(/_/g, " ")) + '</span><span class="sub wrap blk">' +
    esc([verdictText[t.verdict] || t.verdict, t.refraction > 1 ? "under water" + (t.medium_sure ? "" : "?") : "in air" + (t.medium_sure ? "" : "?"),
         t.readout_ms ? "readout " + Math.abs(t.readout_ms).toFixed(1) + " ms" + (t.readout_measured ? " (measured)" : "") : ""].filter(Boolean).join(" · ")) + '</span>' + keepBtn(r, t) + '</div>';
  let h = '<div class="lensresults"><span class="sub wrap">' + esc(job.title + (job.state === "done" ? "" : " · " + job.state)) + '</span>';
  for (const r of job.results) {
    h += '<div class="isec"><b>' + esc(name(r.path)) + '</b>' + (r.mode ? '<span class="sub wrap blk">' + esc(r.mode) + '</span>' : "");
    if (r.error) { h += '<span class="sub wrap err">' + esc(r.error) + '</span></div>'; continue; }
    if (r.estimate && job.kind === "lens-estimate") h += lensLine(r, r.estimate) + (r.doubts && r.doubts.length ? '<span class="sub wrap">' + esc(r.doubts.join(" · ")) + '</span>' : "");
    else {
      const fit = (r.tried || []).filter((t) => t.fits);
      if (!fit.length) h += '<span class="sub wrap">No known lens fits' + (r.fragments_with_scene < 2 ? " (little of the clip could tell)" : "") + ': Estimate lens from a clip of this mode</span>';
      // The one lens_from_video took first, then the others that fit
      fit.sort((a, b) => (b.n === r.accepted) - (a.n === r.accepted));
      for (const t of fit) h += lensLine(r, t);
    }
    h += '</div>';
  }
  return h + '</div>';
}
async function keepFoundLens(path, n, mode) {
  const k = path + "#" + n;
  if (LENS_KEEPING[k]) return;
  LENS_KEEPING[k] = true; renderSide();
  try {
    const r = await invoke("keep_found_lens", { path, n });
    for (const x of Object.keys(LENS_CANDIDATES)) delete LENS_CANDIDATES[x];
    toast(String(r.lens || "The lens").replace(/_/g, " ") + " kept for " + (r.mode || "the mode") + (r.again ? ": " + r.again + (r.again === 1 ? " clip" : " clips") + " of the camera read again" : ""));
    await measureWithKept(path, r.mode || mode);
  } catch (e) { showError("The lens couldn’t be kept: " + e); }
  finally { delete LENS_KEEPING[k]; renderSide(); }
}
// The clips of the mode a lens was just kept for, measured again with it: the selected ones of that mode (else the one
// it was found on) that are measured from the picture, or have no motion data at all. Not clips with gyro (waiting for
// a ready lens: they play from their gyro now), 360° ones or ones with a Gyroflow project
function keptLensClips(path, mode) {
  const all = (DATA.footage && DATA.footage.items) || [];
  const ofMode = (it) => it.kind === "clip" && !it.is360 && !it.gyroflow && (lensOf(it) || {}).mode_key === mode;
  const measurable = (it) => { const p = PROBES[it.path] || {}; return it.measured || (p.mode == null && !/no ready lens/.test(String(p.reason || ""))); };
  const sel = ctx().selItems.filter(ofMode);
  return (sel.length ? sel : all.filter((it) => it.path === path)).filter(measurable);
}
async function measureWithKept(path, mode, except) {
  const clips = keptLensClips(path, mode).filter((it) => it.path !== except);
  if (!clips.length) return;
  const label = (clips.length === 1 ? clips[0].name : clips.length + " clips") + ", with the lens kept";
  await invoke("add_job", { kind: "optical", options: { clips: clips.map((it) => it.id), redo: true }, only: clips.map((it) => it.id), label });
  toast("Measuring again: " + label);
}
