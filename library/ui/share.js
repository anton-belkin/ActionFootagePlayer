// Share (docs/design/albums-photos-share-concept.md, "Share replaces export"): one command on any selection in Albums and
// Originals (clips, moments, photos, mixed). Two choices: what (the selection) and where (a destination). The render
// of each item comes from the render cache, made first where the cache lacks it (library/src/share.rs, renders.rs);
// the page only says what's selected and shows how each item goes.
//
// Loaded before app.js, which calls it at the lines marked `share.js`.

// Where a selection can go. Another destination is a line here (and its arm in share.rs's deliver)
const SHARE_DESTS = [
  { id: "folder", label: "Save to a folder…", sub: "Each item rendered as the player shows it, then copied into a folder you choose, under its own name." },
];
// The export's settings (docs/design/export-settings.md), the same names and choices as the player's Output tab and Save
// frame (mpv/scripts/gyroflow/exportsettings.lua): a video's size, encoding and quality, a photo's size and quality.
// Each choice's first is the default, what was rendered before there were settings. Remembered on this computer.
// Both registry.json's (window.AFP_LISTS, put there by the app before the page's scripts): { key: [[value, label, note]] }
// and the settings each kind has
const EXPORT_CHOICES = Object.fromEntries(window.AFP_LISTS.export.settings.map((s) => [s.key, s.choices.map((c) => [c.value, c.label, c.note])]));
const EXPORT_LABELS = { size: "Size", encode: "Encoding", quality: "Quality" };
const EXPORT_KEYS = window.AFP_LISTS.export.kinds;
function shareSettings() {
  const out = {};
  for (const kind of ["video", "photo"]) {
    out[kind] = {};
    for (const k of EXPORT_KEYS[kind]) {
      const v = remembered("afl.share." + kind + "." + k, EXPORT_CHOICES[k][0][0]);
      out[kind][k] = EXPORT_CHOICES[k].some((c) => c[0] === v) ? v : EXPORT_CHOICES[k][0][0];
    }
  }
  return out;
}
// Whether a kind's settings are the defaults: then the renders the views know of are the ones Share would use
function shareDefault(settings, kind) { return EXPORT_KEYS[kind].every((k) => settings[kind][k] === EXPORT_CHOICES[k][0][0]); }
function shareSetting(kindKey, value) {
  const [kind, k] = String(kindKey).split(".");
  if (!EXPORT_KEYS[kind] || !EXPORT_KEYS[kind].includes(k) || !EXPORT_CHOICES[k].some((c) => c[0] === value)) return;
  remember("afl.share." + kind + "." + k, value);
  if (S.share) { S.share.settings = shareSettings(); S.share.sel = shareSel(S.share.list, S.share.look, S.share.settings); }
}
// A kind's settings as fields: a drop-down each (as the look is), the quality's note under them
function shareSettingFields(settings, kind) {
  let h = "";
  for (const k of EXPORT_KEYS[kind]) {
    const id = "share-" + kind + "-" + k;
    h += '<div class="field"><label class="sub" for="' + id + '">' + EXPORT_LABELS[k] + '</label><select id="' + id + '" data-a="sharesetting" data-v="' + kind + "." + k + '">' +
      EXPORT_CHOICES[k].map((c) => '<option value="' + c[0] + '"' + (settings[kind][k] === c[0] ? " selected" : "") + '>' + esc(c[1]) + (k === "size" ? " · " + esc(c[2]) : "") + '</option>').join("") + '</select></div>';
  }
  const q = EXPORT_CHOICES.quality.find((c) => c[0] === settings[kind].quality);
  // Quality encoding is said too (slower, bigger)
  const enc = kind === "video" && settings.video.encode === "quality" ? "Encoding: " + EXPORT_CHOICES.encode[1][2] + ". " : "";
  return h + '<span class="sub wrap sharenote">' + esc(enc + (q ? q[2] : "")) + '</span>';
}
const SHARE_STATE = { waiting: "Waiting for its render", rendering: "Rendering", copying: "Copying", copied: "Saved", failed: "Failed" };

// What Share is asked for, of the selected items: a clip whole, a moment, a photo link (rendered), or a file that is
// its own pixels already (a rendered photo, an export: copied as it is). Marks and folders' own rows aren't items
function shareItems(list) {
  const out = [];
  for (const it of list) {
    if (it.kind === "clip") out.push({ kind: "clip", path: it.id, name: it.name });
    else if (it.kind === "moment") out.push({ kind: "moment", path: it.id, name: it.name });
    else if (it.kind === "still") out.push(it.linked ? { kind: "photo", path: it.id, name: it.name } : { kind: "file", path: it.path, name: it.name });
    else if (it.kind === "export") out.push({ kind: "file", path: it.path, name: it.name });
  }
  return out;
}
// The selection as Share sees it: how many of each kind, and how many of them the cache has in this look (a clip's
// render is looked for when it's shared: the views don't work out each clip's recipe)
function shareSel(list, look, settings) {
  settings = settings || shareSettings();
  // Other settings than the defaults: other renders, which the views don't know of
  const vid = shareDefault(settings, "video"), pho = shareDefault(settings, "photo");
  const count = { clip: 0, moment: 0, photo: 0, file: 0 }, cached = { moment: 0, photo: 0 };
  for (const it of list) {
    if (it.kind === "clip") count.clip++;
    else if (it.kind === "moment") { count.moment++; if (vid && (it.renders || {})[look] === "current") cached.moment++; }
    else if (it.kind === "still" && it.linked) { count.photo++; if (pho && (it.renders || {}).Photo === "current") cached.photo++; }
    else if (it.kind === "still" || it.kind === "export") count.file++;
  }
  return { count, cached, n: count.clip + count.moment + count.photo + count.file, videos: count.clip + count.moment };
}
function shareLines(s) {
  const n = (k, one, many) => s.count[k] + " " + (s.count[k] === 1 ? one : many);
  const lines = [];
  if (s.count.moment) lines.push([n("moment", "moment", "moments"), s.cached.moment === s.count.moment ? "rendered already" : s.cached.moment ? s.cached.moment + " rendered already, " + (s.count.moment - s.cached.moment) + " to render" : "to render"]);
  if (s.count.photo) lines.push([n("photo", "photo", "photos"), s.cached.photo === s.count.photo ? "rendered already" : s.cached.photo ? s.cached.photo + " rendered already, " + (s.count.photo - s.cached.photo) + " to render" : "to render"]);
  if (s.count.clip) lines.push([n("clip", "clip", "clips"), "whole, rendered unless it was before"]);
  if (s.count.file) lines.push([n("file", "rendered photo or export", "rendered photos and exports"), "copied as they are"]);
  return lines;
}
function shareLook() { return remembered("afl.share.look", "HDR") === "SDR" ? "SDR" : "HDR"; }

// The context menu's line and the button on the bar above the list
function shareMenu(c, b) { return shareItems(c.selItems).length ? b("share", "Share…") : ""; }
function shareBar(c) { return shareItems(c.selItems).length ? '<button type="button" class="btn sm" data-a="share">Share…</button>' : ""; }

// The side panel's Share part: what's selected, the command, and the renders kept for it
function shareBody(c) {
  const s = shareSel(c.selItems, shareLook());
  if (!s.n) return note(c.any ? "Nothing here to share: select clips, moments or photos." : "Select clips, moments or photos (any mix), in Albums or Originals: each is rendered as the player shows it and saved where you choose.");
  const kept = c.selItems.filter((it) => (it.kind === "moment" || (it.kind === "still" && it.linked)) && Object.keys(it.renders || {}).length);
  const stale = kept.filter((it) => Object.values(it.renders).includes("stale"));
  return part('<div class="lab">Share · ' + s.n + (s.n === 1 ? " item" : " items") + '</div>' +
      '<div class="result">' + shareLines(s).map((l) => '<div class="it"><b>' + esc(l[0]) + '</b><span>' + esc(l[1]) + '</span></div>').join("") + '</div>' +
      '<div class="btns"><button type="button" class="btn sm pri" data-a="share">Share…</button></div>' +
      '<span class="sub wrap">The originals are never changed. What’s rendered once is kept and used again while nothing about its picture changes.</span>') +
    part('<div class="lab">Renders kept · ' + kept.length + ' of the selected</div>' +
      '<div class="btns"><button type="button" class="btn sm" data-a="delrenders" data-v="stale"' + (stale.length ? "" : " disabled") + '>Delete stale</button>' +
      '<button type="button" class="btn sm" data-a="delrenders" data-v="all"' + (kept.length ? "" : " disabled") + '>Delete renders</button></div>' +
      '<span class="sub wrap">In _Renders/ of the project, each named by what it’s made from, so an item moved, renamed or copied keeps its render. Deleting renders loses only the time to make them again.</span>');
}

// The sheet: what, then where
function askShare() {
  const c = ctx();
  const items = shareItems(c.selItems);
  if (!items.length) return;
  S.share = { items, sel: shareSel(c.selItems, shareLook()), list: c.selItems, look: shareLook(), settings: shareSettings(), busy: false, folder: null, after: 0 };
  S.sheet = "share";
  renderSheet();
}
function shareJob() {
  const s = S.share;
  if (!s || !s.folder) return null;
  return S.jobs.filter((j) => j.kind === "share" && j.share && j.id > s.after && j.share.folder === s.folder).pop() || null;
}
function renderShareSheet() {
  const s = S.share;
  if (!s) return "";
  const n = s.items.length;
  let h = '<div class="scrim" data-a="scrim"><div class="sheet narrow sharesheet" role="dialog" aria-modal="true" aria-labelledby="shareTitle">';
  if (!s.folder) {
    h += '<header><h2 id="shareTitle">Share ' + (n === 1 ? esc(s.items[0].name) : n + " items") + '</h2>' +
      '<span class="sub wrap">Each is rendered as the player shows it: stabilized, in its look, with its crop. The originals stay as they are.</span></header><div class="sb">' +
      '<div class="result">' + shareLines(s.sel).map((l) => '<div class="it"><b>' + esc(l[0]) + '</b><span>' + esc(l[1]) + '</span></div>').join("") + '</div>';
    const st = s.settings || shareSettings();
    if (s.sel.videos) {
      if (s.sel.count.photo) h += '<div class="lab">Videos</div>';
      h += '<div class="field"><label class="sub" for="sharelook">Look</label><select id="sharelook" data-a="sharelook">' +
        '<option value="HDR"' + (s.look === "HDR" ? " selected" : "") + '>HDR, as an iPhone records (HLG, Dolby Vision)</option>' +
        '<option value="SDR"' + (s.look === "SDR" ? " selected" : "") + '>SDR, for messengers and the web</option></select></div>';
    }
    // The export's settings: the videos' (size, encoding, quality: after their look), the photos' (size, quality)
    if (s.sel.videos) h += shareSettingFields(st, "video");
    if (s.sel.count.photo) h += (s.sel.videos ? '<div class="lab">Photos</div>' : "") + shareSettingFields(st, "photo");
    h += '<div class="lab">Where</div><div class="albumlist sharedests">' + SHARE_DESTS.map((d) =>
      '<button type="button" class="albumrow sharedest" data-a="sharego" data-v="' + d.id + '"' + (s.busy ? " disabled" : "") + '><span><b>' + esc(d.label) + '</b><span class="sub wrap">' + esc(d.sub) + '</span></span></button>').join("") + '</div>' +
      '</div><footer><span class="sp"></span><button type="button" class="btn" data-a="closesheet">Cancel</button></footer>';
    return h + '</div></div>';
  }
  // Chosen: how each item goes
  const j = shareJob();
  const rows = j ? j.share.items : s.items.map((it) => ({ name: it.name, state: "waiting" }));
  const left = rows.filter((r) => r.state !== "copied" && r.state !== "failed").length;
  const saved = rows.filter((r) => r.state === "copied").length, failed = rows.filter((r) => r.state === "failed").length;
  const where = s.folder.split(/[\\/]/).filter(Boolean).pop() || s.folder;
  const sum = !j ? "Looking at what’s rendered already…" : left ? saved + " of " + rows.length + " saved" + (j.state === "waiting" ? " · the rest waits for its turn in Tasks" : j.state === "starting" ? " · starting the player" : j.fraction != null ? " · rendering, " + Math.round(j.fraction * 100) + "%" : "")
    : saved + " saved" + (failed ? ", " + failed + " failed" : "") + (j.state === "stopped" ? " (stopped)" : "");
  h += '<header><h2 id="shareTitle">' + (left || !j ? "Saving to " : "Saved to ") + esc(where) + '</h2><span class="sub wrap" id="sharesum">' + esc(sum) + '</span></header><div class="sb">' +
    '<div class="shareitems" role="list">' + rows.map((r) => '<div class="shareitem ' + esc(r.state) + '" role="listitem"><span class="nm">' + esc(r.name) + '</span>' +
      '<span class="pill ' + (r.state === "copied" ? "ok" : r.state === "failed" ? "bad" : "") + '">' + esc(SHARE_STATE[r.state] || r.state) + '</span>' +
      (r.message ? '<span class="sub wrap why">' + esc(r.message) + '</span>' : "") + '</div>').join("") + '</div>' +
    (j && j.state === "failed" && j.message ? '<span class="sub wrap err">' + esc(j.message) + '</span>' : "") +
    '</div><footer><button type="button" class="btn" data-a="revealpath" data-v="' + esc(s.folder) + '">Show the folder</button><span class="sp"></span>' +
    '<button type="button" class="btn pri" data-a="closesheet">' + (left ? "Close (it goes on in Tasks)" : "Done") + '</button></footer>';
  return h + '</div></div>';
}
async function shareGo(dest) {
  const s = S.share;
  if (!s || s.busy) return;
  s.busy = true;
  s.after = S.jobs.reduce((m, j) => Math.max(m, j.id), 0);
  renderSheet();
  try {
    // The folder is asked for by the system's dialog; nothing chosen: back to the sheet
    const folder = await invoke("share", { items: s.items, dest, look: s.look, settings: s.settings || shareSettings() });
    s.busy = false;
    if (!folder) { renderSheet(); return; }
    s.folder = folder;
    renderSheet();
    await poll();
  } catch (e) { s.busy = false; renderSheet(); showError(String(e)); }
}
// The queue was read again (loading.js's poll): the sheet follows its job
function shareTick() { if (S.sheet === "share" && S.share && S.share.folder) renderSheet(); }

// A share's job in Tasks: each item and how it went
function shareJobDetails(j) {
  if (!j.share || !j.share.items.length) return "";
  const rows = j.share.items;
  const failed = rows.filter((r) => r.state === "failed").length;
  return '<details class="sharejob"' + (failed ? " open" : "") + '><summary>' + rows.length + (rows.length === 1 ? " item" : " items") + " to " + esc(j.share.folder) + '</summary><ul>' +
    rows.map((r) => "<li><code>" + esc(r.name) + "</code>: " + esc(SHARE_STATE[r.state] || r.state) + (r.message ? " (" + esc(r.message) + ")" : "") + "</li>").join("") + "</ul></details>";
}
