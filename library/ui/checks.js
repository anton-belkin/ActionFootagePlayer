"use strict";
// Check, Find moved files, the catalog's care; the Tags part.
// One of the window's scripts that were app.js: index.html loads them in this order before app.js, as classic
// scripts sharing their top-level names, so each uses the others' when called (only earlier ones at load).

// Check and Find moved files, on the selection (library/src/checks.rs)
const KIND_TEXT = { own2: ["warn", "2 own files"], orphan: ["stale", "no clip"], twice: ["stale", "twice"], missing: ["warn", "original missing"] };
function issueText(i) {
  const short = (p) => String(p || "").split("/").slice(-2).join("/");
  if (i.kind === "own2") return i.tree + " has its own file in " + i.files.length + " places: " + i.files.map(short).join(", ") + ". The first location’s is used and saved; the others are only read: delete them.";
  if (i.kind === "orphan") return short(i.file) + " has no clip beside it. Find moved files puts it back by the clip’s fingerprint.";
  if (i.kind === "twice") return "The same clip at " + i.trees.join(" and ") + ". Delete one.";
  if (i.kind === "missing") return short(i.file) + ": its original isn’t at " + i.tree + ".";
  return JSON.stringify(i);
}
function checksBody(c) {
  let body = '<p>' + (c.any ? "Over " + esc(c.selName) + (S.view === "footage" && c.selFolders.length ? " (" + esc(c.label) + ", and the links in them)" : "") + "." : "Select what to check: clips or folders (" + (S.macos ? "⌘A" : "Ctrl+A") + ": all of them).") +
    (S.checking ? ' <span class="sub" style="display:inline">Checking: a big selection on a share takes a while.</span>' : "") + '</p>' +
    '<div class="btns"><button type="button" class="btn sm' + (S.checked ? " on" : "") + '" data-a="runcheck"' + (c.any && !S.checking ? "" : " disabled") + '>' + (S.checking ? '<span class="spin" aria-hidden="true"></span> Checking…' : "Check") + '</button>' +
    '<button type="button" class="btn sm" data-a="findmoved"' + (S.checked && S.checked.issues.some((i) => i.kind === "missing" || i.kind === "orphan") ? "" : " disabled") + '>' + (S.finding ? "Looking…" : "Find moved files") + '</button></div>';
  if (S.checked) {
    const list = S.checked.issues;
    body += '<div class="result"><b>' + (list.length ? list.length + " issue" + (list.length > 1 ? "s" : "") : "Nothing wrong") + '</b>' +
      list.slice(0, 40).map((i) => { const k = KIND_TEXT[i.kind] || ["", i.kind]; return '<div class="it"><span class="chip ' + k[0] + '">' + esc(k[1]) + '</span><span class="wrap">' + esc(issueText(i)) + '</span></div>'; }).join("") +
      (list.length > 40 ? '<span class="sub">…and ' + (list.length - 40) + ' more</span>' : "") + '</div>';
  }
  if (S.found) {
    const ok = S.found.filter((f) => f.found);
    body += '<div class="result"><b>Found ' + ok.length + ' of ' + S.found.length + '</b>' +
      S.found.map((f, i) => '<div class="it"><span class="mono">' + esc(f.issue.tree) +
        (f.found ? '<br>→ ' + esc(f.found.tree) + ' <span class="sub" style="display:inline">(' + esc(f.by) + ')</span>'
          : (f.candidates || []).length ? '<br><span class="sub" style="display:inline">' + f.candidates.length + ' of the same ' + esc(f.by) + ', choose:</span>' +
            f.candidates.map((c, j) => '<br><button type="button" class="btn sm" data-a="pickmoved" data-v="' + i + ':' + j + '">' + esc(c.tree) + '</button>').join("")
          : '<br>not found') + '</span></div>').join("") +
      (ok.length ? '<div class="btns"><button type="button" class="btn sm pri" data-a="applymoved">Update ' + ok.length + '</button></div>' : "") + '</div>';
  }
  return body;
}
// The catalog against the files (library/src/seen.rs, docs/design/catalog-safeguards.md): what the Library shows is kept as
// the files were last seen, and seen again as they change. On demand: every folder looked at again (after a script or
// Finder changed files while nothing was watching), or every file of ours read again (after a restore that kept times)
function catalogBody() {
  if (S.care === undefined) { S.care = null; refreshCare(); }
  const k = S.care || {};
  const busy = !!k.busy;
  const btn = (v, text) => '<button type="button" class="btn sm" data-a="carelook" data-v="' + v + '"' + (busy ? " disabled" : "") + '>' +
    (busy && k.kind === v ? '<span class="spin" aria-hidden="true"></span> ' : "") + text + '</button>';
  let body = '<p><b>The whole project’s catalog</b></p><span class="sub wrap">What the Library shows, as the files were last seen. Files changed by a script or in Finder while nothing was watching are seen within a day, or now:</span>' +
    '<div class="btns">' + btn("deep", "Look at every folder again") + btn("check", "Read every file again") + '</div>';
  if (k.error) body += '<div class="result"><b>Not done</b><span class="wrap">' + esc(k.error) + '</span></div>';
  else if (!busy && k.kind) {
    const fixed = (k.corrected || []).length;
    body += '<div class="result"><b>' + (k.kind === "check" ? (fixed ? "Corrected " + fixed + (fixed === 1 ? " file" : " files") : "The catalog was right") :
      (k.changes ? k.changes + " changes seen" : "Nothing new")) + '</b>' +
      (k.corrected || []).slice(0, 20).map((f) => '<div class="it"><span class="mono wrap">' + esc(f) + '</span></div>').join("") + '</div>';
  }
  const bad = k.unreadable_now || [];
  if (bad.length) {
    body += '<div class="result"><b>' + bad.length + (bad.length === 1 ? " clip’s own file isn’t what it should be" : " clips’ own files aren’t what they should be") + '</b>' +
      '<span class="sub wrap">Not JSON: written wrong, or still being written. Shown as they were until they read right.</span>' +
      bad.slice(0, 20).map((f) => '<div class="it"><span class="chip warn">unreadable</span><span class="mono wrap">' + esc(f) + '</span></div>').join("") + '</div>';
  }
  return body;
}
async function refreshCare() {
  try { S.care = await invoke("catalog_care"); } catch (e) { S.care = {}; }
  renderSide();
  if (S.care && S.care.busy) setTimeout(refreshCare, 1000);
}
async function careLook(v) {
  try { await invoke("catalog_look", { check: v === "check" }); } catch (e) { showError(String(e)); return; }
  S.care = { kind: v, busy: true }; renderSide();
  setTimeout(refreshCare, 1000);
}
// Culling and tags on the selection: culling.js's panel (Accept / Reject, stars, the tags in use, a new one)
function tagsBody(c) { return cullBody(c); }
const tagItems = (c) => c.selItems.map((it) => ({ kind: it.kind, id: it.id, path: it.path }));
async function setTags(add, remove) {
  const c = ctx();
  if (!c.any) return;
  // Shown at once, written after (writes.js)
  const items = c.selItems.filter((it) => Writes.VIEW_OF[it.kind]);
  Writes.run("tags", items, { items: items.map((it) => ({ kind: it.kind, id: it.id, path: it.path })), add, remove })
    .then(() => { if (add.length) invoke("recent_tags").then((r) => { S.recentTags = r; }, () => {}); });
}
async function renameTag() {
  const from = S.tagFrom.trim(), to = S.tagTo.trim();
  if (!from || !to) { toast("The tag, and its new name"); return; }
  try {
    const r = await invoke("rename_tag", { from, to });
    if (r.problems.length) showError(r.problems.join(" · "));
    toast(r.count ? "“" + from + "” → “" + to + "” on " + r.count + " item" + (r.count === 1 ? "" : "s") : "No item has “" + from + "”");
    S.tagFrom = ""; S.tagTo = "";
    S.recentTags = await invoke("recent_tags").catch(() => S.recentTags);
    // The other views read already: read again (the catalog was told of every file written)
    for (const v of ["stills", "exports"]) if (DATA[v] && v !== S.view) loadView(v).catch(() => {});
    await refresh();
  } catch (e) { showError(String(e)); }
}

async function runCheck() {
  const c = ctx();
  // Only what's selected (Anton, 2026-11-10: never the whole project on its own; ⌘A selects all)
  if (!c.any) { toast("Select what to check: clips or folders (" + (S.macos ? "⌘A" : "Ctrl+A") + ": all of them)"); return; }
  S.checking = true; S.found = null; renderSide();
  // Footage's folders are its previews': the previews in them checked too (their clips may be at 0: moved)
  const previewDirs = S.view === "footage" ? c.selFolders.filter((f) => f !== NOPREV && !f.startsWith(NOPREV + "/")).flatMap((f) => f === OTHER ? otherExpanded([f]) : [f]).flatMap((f) => albumRealDirs(f)) : [];
  try {
    // The moments selected one by one: each as a link to check (a folder's are in previewDirs)
    S.checked = await invoke("run_check", { sel: c.only, links: previewDirs.concat(c.direct.filter((it) => it.kind === "moment").map((it) => it.id)) });
  } catch (e) { showError(String(e)); }
  S.checking = false; renderSide();
}
async function findMoved() {
  if (!S.checked) return;
  S.finding = true; renderSide();
  try { S.found = await invoke("find_moved", { issues: S.checked.issues.filter((i) => i.kind === "missing" || i.kind === "orphan") }); } catch (e) { showError(String(e)); }
  S.finding = false; renderSide();
}
async function applyMoved() {
  try {
    const problems = await invoke("apply_moved", { found: (S.found || []).filter((f) => f.found) });
    toast(problems.length ? "Updated, but: " + problems.join(" · ") : "Updated: links and own files follow the clips");
    S.found = null; S.checked = null;
    await refresh();
  } catch (e) { showError(String(e)); }
}
