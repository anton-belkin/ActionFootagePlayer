"use strict";
// Culling and tags in the Library's window (docs/design/tags-and-culling.md; the app's side is src/tags.rs): the Tags part of
// the side panel, the keys, Hide rejected and Most stars first. Loaded before app.js, which calls in from a few places
// (tagsBody, matches, filtering, renderMain, renderTop, its keydown); what's here uses app.js's own (S, DATA, ctx,
// invoke, renderAll…) only when called, after both are loaded.
//
// On the selection's originals, moments and photos (never exports, marks or folders themselves: a folder stands for
// what's in it): Accept or Reject or neither, 1–5 stars or none, the tags used in the library as buttons (the most used
// first), and a field for a new one that suggests the ones in use while typing. A change shows at once and is written
// behind it (one at a time, in order); the view is read again when the last one is written.
const CULL = {
  // Hide rejected and Most stars first: across the tabs, and remembered on this computer
  hide: cullRemembered("afl.hideRejected") === "1", starSort: cullRemembered("afl.starSort") === "1",
  // The new tag's suggestions: shown, and the one the arrows are on (-1: none, Enter adds what's typed)
  suggOpen: false, sugg: -1,
  // Writes on their way, one after another
  pending: 0, chain: Promise.resolve(),
  // The project whose other views were asked for (the tags in use are counted over all of them), Rename… shown
  askedFor: null, rename: false,
  // The comment being typed: for which selection (`sig`), its items, the text, whether it's not saved yet, whether the
  // selection's comments differ (then it's saved only on Set for all), the timer that saves it
  note: null,
};
// A typed comment is saved this long after the last key (and at once when the field is left or the selection changes)
const NOTE_IDLE_MS = 700;
function cullRemembered(k) { try { return localStorage.getItem(k) || ""; } catch (_) { return ""; } }
function cullRemember(k, v) { try { localStorage.setItem(k, v); } catch (_) { /* */ } }
const CULL_KINDS = { clip: true, moment: true, still: true };
const SYS_TAG = "Deleted in Photos";
const ci = (a, b) => String(a).toLowerCase() === String(b).toLowerCase();
function stars(n) { return n ? "★".repeat(n) : ""; }
const hasTag = (it, t) => (it.tags || []).some((x) => ci(x, t));

// ---------- what's hidden, and the order ----------

// Hide rejected: an item rejected isn't shown (nor counted in a folder, nor taken by a batch on a folder)
function cullShown(it) { return !(CULL.hide && it.pick === "reject"); }
// …which is a filter like the others while something is hidden by it
function cullHiding() { return CULL.hide && items().some((it) => it.pick === "reject"); }
// Most stars first: the list's items by their stars, the folders left on top and equal ones as they were
function cullSort(list) {
  if (!CULL.starSort) return list;
  const at = new Map(list.map((it, i) => [it, i]));
  return list.sort((a, b) => (b.kind === "folder") - (a.kind === "folder") || (a.kind === "folder" ? 0 : (b.rating || 0) - (a.rating || 0)) || at.get(a) - at.get(b));
}
// Accepted / Rejected and the stars on a small tile that has no room for chips (Timeline's: albumviews.js)
function cullBadge(it) {
  const p = it.pick === "accept" ? '<b class="ok" title="Accepted">✓</b>' : it.pick === "reject" ? '<b class="no" title="Rejected">✕</b>' : "";
  return p || it.rating ? '<span class="cb">' + p + (it.rating ? '<b>' + stars(it.rating) + '</b>' : "") + '</span>' : "";
}
// The two switches, at the end of the top bar
function cullTop() {
  const top = $("top");
  if (!top || !S.root) return;
  const hidden = CULL.hide ? items().filter((it) => it.pick === "reject").length : 0;
  top.insertAdjacentHTML("beforeend",
    '<button type="button" class="btn cullsw' + (CULL.hide ? " on" : "") + '" data-c="hide" aria-pressed="' + CULL.hide + '" title="Rejected ones aren’t shown, in every tab">Hide rejected' + (hidden ? " · " + hidden : "") + '</button>' +
    '<button type="button" class="btn cullsw' + (CULL.starSort ? " on" : "") + '" data-c="starsort" aria-pressed="' + CULL.starSort + '" aria-label="Most stars first" title="Most stars first">★ first</button>');
}

// ---------- the selection's state ----------

function cullItems(c) { return c.selItems.filter((it) => CULL_KINDS[it.kind]); }
// How many are accepted, rejected, undecided; the least and the most stars
function cullState(list) {
  const st = { n: list.length, accept: 0, reject: 0, none: 0, min: list.length ? 5 : 0, max: 0 };
  for (const it of list) {
    st[it.pick === "accept" || it.pick === "reject" ? it.pick : "none"]++;
    const r = it.rating || 0;
    if (r < st.min) st.min = r;
    if (r > st.max) st.max = r;
  }
  return st;
}
// A button's state for `k` of `n`: on all of it, on some, on none
const cullPart = (k, n) => (n && k === n ? "all" : k ? "some" : "none");
const cullPressed = (p) => (p === "all" ? "true" : p === "some" ? "mixed" : "false");
// The tags in use in the library, the most used first: over the originals, the moments and the photos read so far
function cullTagsInUse() {
  const by = new Map();
  const lists = [(typeof RAW_FOOTAGE !== "undefined" && RAW_FOOTAGE && RAW_FOOTAGE.items) || [], (DATA.moments && DATA.moments.items) || [], (DATA.stills && DATA.stills.items) || []];
  for (const list of lists) for (const it of list) for (const t of it.tags || []) {
    const k = t.toLowerCase(), e = by.get(k);
    if (e) e.n++; else by.set(k, { name: t, n: 1 });
  }
  return Array.from(by.values()).sort((a, b) => b.n - a.n || a.name.localeCompare(b.name));
}
// The views not read yet (the photos, mostly) asked for once per project, so their tags count too
function cullNeedViews() {
  if (!S.root || CULL.askedFor === S.root) return;
  CULL.askedFor = S.root;
  needViews(["footage", "moments", "stills"]).then(() => { if (S.tab === "tags") renderSide(); }).catch(() => {});
}
// What the field suggests for what's typed: the tags in use (then the ones used last on this computer) that start with
// it, then the ones that have it inside; not the ones all of the selection has already
function cullSuggestions(c) {
  const q = S.tagDraft.trim().toLowerCase();
  const list = cullItems(c || ctx());
  const names = cullTagsInUse().map((t) => t.name);
  for (const t of S.recentTags || []) if (!names.some((x) => ci(x, t))) names.push(t);
  const free = names.filter((t) => t !== SYS_TAG && !(list.length && list.every((it) => hasTag(it, t))));
  const starts = free.filter((t) => t.toLowerCase().startsWith(q)), inside = q ? free.filter((t) => !t.toLowerCase().startsWith(q) && t.toLowerCase().includes(q)) : [];
  return starts.concat(inside).slice(0, 8);
}

// ---------- the Tags part ----------

function cullSuggHtml(c) {
  const list = CULL.suggOpen ? cullSuggestions(c) : [];
  if (CULL.sugg >= list.length) CULL.sugg = list.length - 1;
  return list.map((t, i) => '<button type="button" role="option" id="tagsugg' + i + '" class="' + (i === CULL.sugg ? "on" : "") + '" aria-selected="' + (i === CULL.sugg) + '" data-c="sugg" data-v="' + esc(t) + '">' + esc(t) + '</button>').join("");
}
function cullRenderSugg() {
  const el = $("tagsugg"), inp = $("tagdraft");
  if (!el) return;
  el.innerHTML = cullSuggHtml();
  el.hidden = !el.innerHTML;
  if (inp) {
    inp.setAttribute("aria-expanded", String(!el.hidden));
    if (CULL.sugg >= 0 && !el.hidden) inp.setAttribute("aria-activedescendant", "tagsugg" + CULL.sugg); else inp.removeAttribute("aria-activedescendant");
  }
}
// Up to this many tags in use: their list is never made smaller than it is (about three rows of chips at most)
const CULL_FEW_TAGS = 9;
function cullBody(c) {
  // Exports alone selected (in Albums → Other folders → _Exports)
  if (c.selItems.length && c.selItems.every((it) => it.kind === "export")) return '<p class="sub wrap">Exports aren’t kept, rejected, rated or tagged: they’re made of moments, which are. Select moments, clips or photos.</p>';
  cullNeedViews();
  const list = cullItems(c), st = cullState(list), dis = list.length ? "" : " disabled";
  const pick = (v, cls, text, key, n) => {
    const p = cullPart(n, st.n);
    return '<button type="button" class="pickb ' + cls + (p === "all" ? " on" : p === "some" ? " some" : "") + '" data-c="pick" data-v="' + v + '" aria-pressed="' + cullPressed(p) + '" title="' + text + " (" + key + ")" +
      (p === "some" ? ": " + n + " of " + st.n : "") + '"' + dis + '>' + text + (p === "some" ? ' <span class="n">' + n + '</span>' : "") + '</button>';
  };
  let h = '<div class="lab">Keep it?</div><div class="picks" role="group" aria-label="Keep it?">' +
    pick("accept", "acc", "✓ Accept", "P", st.accept) + pick("reject", "rej", "✕ Reject", "X", st.reject) + pick("none", "und", "Undecided", "U", st.none) + '</div>';
  // The stars: filled up to what all of it has, half-toned up to the most any of it has
  const mixed = st.n && st.min !== st.max;
  h += '<div class="lab">Stars</div><div class="stars" role="group" aria-label="Stars">' + [1, 2, 3, 4, 5].map((n) => {
    const p = n <= st.min ? "all" : n <= st.max ? "some" : "none";
    return '<button type="button" class="star' + (p === "all" ? " on" : p === "some" ? " some" : "") + '" data-c="rate" data-v="' + n + '" aria-pressed="' + cullPressed(p) + '" aria-label="' + n + ' star' + (n > 1 ? "s" : "") + '" title="' + n + " star" + (n > 1 ? "s" : "") + " (" + n + ")" + '"' + dis + '>' + (p === "none" ? "☆" : "★") + '</button>';
  }).join("") +
    '<button type="button" class="nostar' + (st.n && st.max === 0 ? " on" : "") + '" data-c="rate" data-v="0" aria-pressed="' + (st.n > 0 && st.max === 0) + '" title="No stars (0)"' + dis + '>None</button>' +
    '<span class="sub">' + (mixed ? "mixed: " + st.min + "–" + st.max : "") + '</span></div>';
  h += cullNoteHtml(c, list);
  // The tags in use, the most used first; the app's own only when the selection has it
  const used = cullTagsInUse().filter((t) => t.name !== SYS_TAG || list.some((it) => hasTag(it, t.name)));
  h += '<div class="lab">Tags</div><div class="chips culltags' + (used.length > CULL_FEW_TAGS ? " many" : "") + '" role="group" aria-label="Tags in use">' + (used.length ? used.map((t) => {
    const k = list.filter((it) => hasTag(it, t.name)).length, p = cullPart(k, st.n);
    return '<button type="button" class="chip tagb' + (t.name === SYS_TAG ? " sys" : "") + (p === "all" ? " on" : p === "some" ? " some" : "") + '" data-c="tag" data-v="' + esc(t.name) + '" aria-pressed="' + cullPressed(p) + '" title="' +
      esc(p === "all" ? "On all of the selection: a click takes it off" : p === "some" ? "On " + k + " of " + st.n + ": a click puts it on all" : "Used on " + t.n + " in the library: a click puts it on the selection") + '"' + dis + '>' + esc(t.name) + '</button>';
  }).join("") : '<span class="sub">No tags in this library yet</span>') + '</div>';
  const sugg = cullSuggHtml(c);
  h += '<div class="tagwrap"><div class="tagin"><input id="tagdraft" type="text" role="combobox" aria-autocomplete="list" aria-controls="tagsugg" aria-expanded="' + (sugg !== "") + '" autocomplete="off" spellcheck="false" value="' + esc(S.tagDraft) +
    '" placeholder="' + (list.length ? "A new tag for " + esc(c.label) : "A new tag") + '" aria-label="New tag"' + dis + '><button type="button" class="btn sm" data-c="add"' + dis + '>Add</button></div>' +
    '<div id="tagsugg" class="tagsugg" role="listbox" aria-label="Tags in use"' + (sugg ? "" : " hidden") + '>' + sugg + '</div></div>';
  h += '<span class="sub wrap">' + (list.length ? "" : "Select clips, moments or photos (or folders of them). ") + 'Keys, with the list in front: P accept, X reject, U undecided, 1–5 stars, 0 none; with one selected, the next one is selected after.</span>' +
    '<span class="sub wrap">Kept with each: an original’s in its own file, a moment’s in its link, a photo’s in its own metadata' + (S.tagStores.indexOf("finder") >= 0 ? "; and as Finder tags on the files (Project settings → Tags)" : "") + '.</span>';
  // A tag renamed over the whole project (to a name in use: the two become one)
  h += '<div class="btns"><button type="button" class="btn sm' + (CULL.rename ? " on" : "") + '" data-c="rename" aria-expanded="' + CULL.rename + '">Rename a tag…</button></div>';
  if (CULL.rename) {
    h += '<datalist id="tagnames">' + cullTagsInUse().map((t) => '<option value="' + esc(t.name) + '">').join("") + '</datalist>' +
      '<div class="tagin"><input id="tagfrom" type="text" list="tagnames" value="' + esc(S.tagFrom) + '" placeholder="Tag" aria-label="Tag to rename">' +
      '<input id="tagto" type="text" list="tagnames" value="' + esc(S.tagTo) + '" placeholder="New name" aria-label="Its new name"><button type="button" class="btn sm" data-a="renametag">Rename</button></div>' +
      '<span class="sub wrap">On originals, moments and photos of the whole project. To a name in use: the two become one.</span>';
  }
  return h;
}

// ---------- the comment ----------
//
// A few lines of free text on an original, a moment or a photo (what happens in it), kept with each where its tags are
// (src/tags.rs set_comment) and found by the search (app.js's textMatch). Typed, it's saved by itself: a moment after
// the last key, and at once when the field is left (another item clicked, another tab) or ⌘↩ (which also selects the
// next one, to go through a folder quickly). Several selected with the same comment (or none): what's typed goes to
// all of them, as for one. Several with different comments: the field starts empty and nothing is saved until Set for
// all is pressed (or ⌘↩); leaving the selection drops what was typed.

// A comment as the app keeps it (src/tags.rs comment_clean): lines' ends as \n, spaces at both ends off
function cullNoteClean(t) { return String(t == null ? "" : t).replace(/\r\n?/g, "\n").trim(); }
const cullNoteOf = (it) => it.comment || "";
// The selection's own: which items, in a word
function cullNoteSig(list) { return list.map((it) => it.kind + ":" + it.id).sort().join("\n"); }
// The comment of the items, when all of them have the same (none: ""); null when they differ
function cullNoteShared(list) {
  if (!list.length) return "";
  const first = cullNoteOf(list[0]);
  return list.every((it) => cullNoteOf(it) === first) ? first : null;
}
// An item as the page shows it now (the draft's copy may be older than a change written since)
function cullNoteNow(it) {
  const v = DATA[Writes.VIEW_OF[it.kind]];
  return (v && v.items && v.items.find((x) => x.id === it.id)) || it;
}
// The draft for this selection (made when typing starts), the one of another selection saved and let go first
function cullNoteFor(list) {
  const sig = cullNoteSig(list);
  if (CULL.note && CULL.note.sig !== sig) { cullNoteSave(); CULL.note = null; }
  if (!CULL.note && list.length) CULL.note = { sig, list: list.slice(), text: null, dirty: false, mixed: cullNoteShared(list) == null, timer: null };
  return CULL.note;
}
// The draft written (on all of its items that don't have it already): what's shown changes at once, the disk after.
// A draft over different comments only when `all` (Set for all, ⌘↩)
function cullNoteSave(all) {
  const n = CULL.note;
  if (!n) return;
  clearTimeout(n.timer); n.timer = null;
  if (!n.dirty || (n.mixed && !all)) return;
  n.dirty = false;
  const text = cullNoteClean(n.text);
  const targets = n.list.filter((it) => cullNoteOf(cullNoteNow(it)) !== text);
  // Saved over different ones: from now on they're the same
  n.mixed = false;
  if (targets.length) CULL.chain = Writes.run("comment", targets, { items: targets.map((it) => ({ kind: it.kind, id: it.id, path: it.path })), comment: text || null });
  cullNoteTell();
  return CULL.chain;
}
// The app told how many changes wait to be written, a comment being typed counted as one: closing the window (⌘Q
// too, which leaves the field without its focus going) then waits, and the draft is saved first (Writes.drained)
function cullNoteTell() {
  const n = CULL.note, typed = n && n.dirty && !n.mixed ? 1 : 0;
  invoke("writes_pending", { n: Writes.pending() + typed }).catch(() => {});
}
// (changes.js tells the app the writes alone as they end: told again after it while a comment is being typed)
Writes.onChange(() => { if (CULL.note && CULL.note.dirty && !CULL.note.mixed) setTimeout(cullNoteTell, 0); });
function cullNoteHtml(c, list) {
  // Not a draft of another selection: saved now, before this one's is shown
  if (CULL.note && CULL.note.sig !== cullNoteSig(list)) { cullNoteSave(); CULL.note = null; }
  const n = CULL.note, shared = cullNoteShared(list), mixed = n ? n.mixed : shared == null;
  const text = n && n.text != null ? n.text : shared || "";
  const one = list.length === 1, noun = one ? (KIND_NOUN[list[0].kind] || ["item"])[0] : "";
  const have = list.filter((it) => cullNoteOf(it)).length;
  const ph = !list.length ? "Select clips, moments or photos" : one ? "What happens in this " + noun + "…"
    : mixed ? "Different comments (" + have + " of " + list.length + " have one). Type one for all of them…" : "A comment for all " + list.length + "…";
  let h = '<div class="lab"><label for="cullnote">Comment</label></div>' +
    '<textarea id="cullnote" class="cullnote" rows="3" spellcheck="true" placeholder="' + esc(ph) + '" aria-describedby="cullnotehint"' + (list.length ? "" : " disabled") + '>' + esc(text) + '</textarea>';
  if (mixed && list.length) {
    h += '<div class="btns"><button type="button" class="btn sm" data-c="noteall"' + (n && n.dirty ? "" : " disabled") + '>Set for all ' + list.length + '</button></div>' +
      '<span id="cullnotehint" class="sub wrap">Replaces each one’s comment (empty: takes them off). Selecting something else drops what’s typed.</span>';
  } else if (list.length) {
    h += '<span id="cullnotehint" class="sub wrap">' + (one ? "Saved as you type. " : "Goes to all " + list.length + " selected, saved as you type. ") + (one ? "⌘↩ selects the next one, Esc goes back to the list." : "Esc goes back to the list.") + '</span>';
  }
  return h;
}
// ⌘↩ (Ctrl+Enter): saved (over different ones too), and with one item selected the next one selected, the field kept
// in front for its comment. Esc: saved (not over different ones: dropped), back to the list. true: taken
function cullNoteKeys(e) {
  if (e.key === "Enter" && (e.metaKey || e.ctrlKey)) {
    e.preventDefault();
    const c = ctx();
    cullNoteSave(true);
    const single = c.direct.length === 1 && !c.selFolders.length ? keyOf(c.direct[0]) : null;
    const at = single ? VISIBLE.indexOf(single) : -1;
    if (at >= 0 && at + 1 < VISIBLE.length) {
      moveTo(at + 1, false);
      const f = $("cullnote");
      if (f && !f.disabled) { f.focus(); f.setSelectionRange(f.value.length, f.value.length); }
    } else renderSide();
    return true;
  }
  if (e.key === "Escape") {
    e.preventDefault();
    if (!(CULL.note && CULL.note.mixed)) cullNoteSave();
    CULL.note = null;
    // (the field as it is now: a save may have drawn the panel again)
    const f = $("cullnote");
    if (f) f.blur();
    renderSide();
    return true;
  }
  return false;
}

// ---------- changes ----------

// Accept / Reject and the stars as changes writes.js makes (shown at once, written after, taken back with a message
// when they couldn't be); the tags' own kind is changes.js's. Each does to an item what src/tags.rs does on the disk
Writes.define("pick", {
  what: (a, op) => (a.pick === "accept" ? "Accepted on " : a.pick === "reject" ? "Rejected on " : "taking Accepted / Rejected off ") + namesText(op.targets.map((t) => t.name)),
  item(it, a) { if (a.pick) it.pick = a.pick; else delete it.pick; },
  send: (a) => ["set_pick", { items: a.items, pick: a.pick }],
  problems: (r) => r,
});
Writes.define("rating", {
  what: (a, op) => (a.rating ? "the stars of " : "taking the stars off ") + namesText(op.targets.map((t) => t.name)),
  item(it, a) { if (a.rating) it.rating = a.rating; else delete it.rating; },
  send: (a) => ["set_rating", { items: a.items, rating: a.rating }],
  problems: (r) => r,
});
Writes.define("comment", {
  what: (a, op) => (a.comment ? "the comment of " : "taking the comment off ") + namesText(op.targets.map((t) => t.name)),
  item(it, a) { if (a.comment) it.comment = a.comment; else delete it.comment; if ((it.kind === "moment" || it.kind === "still") && it.link) { if (a.comment) it.link.comment = a.comment; else delete it.link.comment; } },
  send: (a) => ["set_comment", { items: a.items, comment: a.comment }],
  problems: (r) => r,
});
// A comment as a tile's tooltip (the tile itself stays as it is): the rest of a title attribute, after its quote
function cullNoteTip(it) { return it.comment ? '" title="' + esc(it.comment) : ""; }
// A change on `list`, shown at once and written behind: { pick: "accept" | "reject" | null }, { rating: 1–5 | null },
// { add: [tags], remove: [tags] }. No view is read again: the command answers with the items as they are now
function cullApply(list, change) {
  list = list.filter((it) => Writes.VIEW_OF[it.kind]);
  if (!list.length) return;
  const items = list.map((it) => ({ kind: it.kind, id: it.id, path: it.path }));
  const done = "pick" in change ? Writes.run("pick", list, { items, pick: change.pick }) : "rating" in change ? Writes.run("rating", list, { items, rating: change.rating })
    : Writes.run("tags", list, { items, add: change.add || [], remove: change.remove || [] });
  CULL.chain = done.then(async () => { if (change.add && change.add.length) S.recentTags = await invoke("recent_tags").catch(() => S.recentTags); });
  return CULL.chain;
}
// Accept or Reject on the selection; what all of it is already, again: neither (as the player's P and X)
function cullPick(v, c) {
  const list = cullItems(c || ctx()), st = cullState(list);
  const pick = v === "none" || (st.n && st[v] === st.n) ? null : v;
  return cullApply(list, { pick });
}
// Stars on the selection; the stars all of it has already, again: none
function cullRate(n, c) {
  const list = cullItems(c || ctx()), st = cullState(list);
  return cullApply(list, { rating: !n || (st.n && st.min === n && st.max === n) ? null : n });
}
// A tag: on all of the selection already: off; else on all of it. In the spelling the library has it
function cullTag(t, c, onlyAdd) {
  t = String(t).split(/\s+/).filter(Boolean).join(" ");
  if (!t) return;
  const known = cullTagsInUse().find((x) => ci(x.name, t));
  if (known) t = known.name;
  const list = cullItems(c || ctx());
  if (!onlyAdd && list.length && list.every((it) => hasTag(it, t))) return cullApply(list, { remove: [t] });
  return cullApply(list.filter((it) => !hasTag(it, t)), { add: [t] });
}
// What's typed (or the suggestion the arrows are on) added
function cullAddDraft(t) {
  t = (t == null ? S.tagDraft : t).trim();
  S.tagDraft = ""; CULL.suggOpen = false; CULL.sugg = -1;
  if (t) cullTag(t, null, true);
  renderSide();
  const inp = $("tagdraft");
  if (inp) inp.focus();
}

// ---------- keys ----------

// The player's keys, on the selection, with the list in front (not while typing: app.js's keydown): P, X, U, 1–5, 0.
// With one item selected, the next one in the list is selected after, as culling goes. true: taken
function cullKeys(e) {
  if (e.metaKey || e.ctrlKey || e.altKey) return false;
  const code = e.code || "";
  // The letters by where the key is (any layout: З, Ч, Г), the digits by what they type or where they are
  const letter = code === "KeyP" || (!code && /^[pP]$/.test(e.key)) ? "accept" : code === "KeyX" || (!code && /^[xX]$/.test(e.key)) ? "reject" : code === "KeyU" || (!code && /^[uU]$/.test(e.key)) ? "none" : null;
  const digit = /^[0-5]$/.test(e.key) ? +e.key : /^(Digit|Numpad)[0-5]$/.test(code) && !e.shiftKey ? +code.slice(-1) : null;
  if (letter == null && digit == null) return false;
  const c = ctx(), list = cullItems(c);
  if (!list.length) return false;
  // One item selected: the one after it, by its place in the list as it is before the change
  const single = c.direct.length === 1 && !c.selFolders.length ? keyOf(c.direct[0]) : null;
  const next = single && VISIBLE.indexOf(single) >= 0 ? VISIBLE[VISIBLE.indexOf(single) + 1] : null;
  if (letter != null) cullPick(letter, c); else cullRate(digit, c);
  if (single) {
    if (next && VISIBLE.indexOf(next) >= 0) moveTo(VISIBLE.indexOf(next), false);
    // The last one, hidden by its own rejection: nothing selected
    else if (VISIBLE.indexOf(single) < 0) { S.sel = {}; S.anchor = null; renderMain(); renderSide(); }
  }
  return true;
}
// The new tag's field: Down and Up through the suggestions, Enter adds the one they're on (none: what's typed, a new
// tag), Tab completes to it (to the first when on none), Esc closes them. true: taken
function cullDraftKeys(e) {
  const list = CULL.suggOpen ? cullSuggestions() : [];
  if (e.key === "ArrowDown") {
    e.preventDefault();
    if (!CULL.suggOpen) { CULL.suggOpen = true; CULL.sugg = -1; } else CULL.sugg = Math.min(list.length - 1, CULL.sugg + 1);
    cullRenderSugg(); return true;
  }
  if (e.key === "ArrowUp" && CULL.suggOpen) { e.preventDefault(); CULL.sugg = Math.max(-1, CULL.sugg - 1); cullRenderSugg(); return true; }
  if (e.key === "Tab" && !e.shiftKey && list.length && S.tagDraft.trim()) {
    const t = list[CULL.sugg >= 0 ? CULL.sugg : 0];
    if (ci(t, S.tagDraft.trim())) return false;
    e.preventDefault();
    S.tagDraft = t; e.target.value = t; CULL.sugg = -1;
    cullRenderSugg(); return true;
  }
  if (e.key === "Enter") { e.preventDefault(); cullAddDraft(CULL.sugg >= 0 && list[CULL.sugg] ? list[CULL.sugg] : null); return true; }
  if (e.key === "Escape" && CULL.suggOpen) { e.preventDefault(); e.stopPropagation(); CULL.suggOpen = false; CULL.sugg = -1; cullRenderSugg(); return true; }
  return false;
}

// ---------- the page's events (its own attribute, data-c: app.js's handlers pass them by) ----------

document.addEventListener("click", (e) => {
  const el = e.target.closest ? e.target.closest("[data-c]") : null;
  if (!el || el.disabled) return;
  const a = el.dataset.c, v = el.dataset.v;
  switch (a) {
    case "pick": cullPick(v); return;
    case "rate": cullRate(+v); return;
    case "tag": cullTag(v); return;
    case "add": cullAddDraft(); return;
    case "sugg": cullAddDraft(v); return;
    case "noteall": cullNoteSave(true); renderSide(); return;
    case "rename": CULL.rename = !CULL.rename; renderSide(); return;
    case "hide": CULL.hide = !CULL.hide; cullRemember("afl.hideRejected", CULL.hide ? "1" : ""); renderAll(); return;
    case "starsort": CULL.starSort = !CULL.starSort; cullRemember("afl.starSort", CULL.starSort ? "1" : ""); renderAll(); return;
  }
});
// A suggestion pressed: the field keeps the focus (the click then adds it)
document.addEventListener("mousedown", (e) => { if (e.target.closest && e.target.closest("#tagsugg")) e.preventDefault(); });
// Typing in the field: the suggestions follow (app.js's own listener keeps what's typed, before this one or after)
// The comment typed: kept as a draft at once, saved a moment after the last key (over different ones: only on Set for all)
document.addEventListener("input", (e) => {
  if (e.target.id !== "cullnote") return;
  const n = cullNoteFor(cullItems(ctx()));
  if (!n) return;
  const was = n.dirty;
  n.text = e.target.value; n.dirty = true;
  clearTimeout(n.timer);
  if (!was && !n.mixed) cullNoteTell();
  if (n.mixed) { const b = document.querySelector('#side [data-c="noteall"]'); if (b) b.disabled = false; return; }
  n.timer = setTimeout(() => { if (CULL.note === n) cullNoteSave(); }, NOTE_IDLE_MS);
});
// Left (another item clicked, another tab, the window): saved at once, and the draft let go (what's shown is then the
// items' own again); a draft over different comments stays until the selection changes
document.addEventListener("focusout", (e) => {
  if (e.target.id !== "cullnote") return;
  cullNoteSave();
  // Not a re-drawing of the panel (which puts the focus back at once): the draft let go
  setTimeout(() => {
    const a = document.activeElement;
    if ((!a || a.id !== "cullnote") && CULL.note && !CULL.note.dirty) CULL.note = null;
  }, 0);
});
document.addEventListener("keydown", (e) => { if (e.target.id === "cullnote" && cullNoteKeys(e)) e.stopImmediatePropagation(); });
// The window closed, another project opened: a comment still being typed is written first
{
  const drained = Writes.drained;
  Writes.drained = () => { cullNoteSave(); return drained(); };
}
document.addEventListener("input", (e) => {
  if (e.target.id !== "tagdraft") return;
  S.tagDraft = e.target.value;
  CULL.suggOpen = S.tagDraft.trim() !== ""; CULL.sugg = -1;
  cullRenderSugg();
});
document.addEventListener("focusout", (e) => {
  if (e.target.id !== "tagdraft" || !CULL.suggOpen) return;
  // Left for good (not a re-drawing of the panel, which puts the focus back)
  setTimeout(() => { if (!document.activeElement || document.activeElement.id !== "tagdraft") { CULL.suggOpen = false; CULL.sugg = -1; cullRenderSugg(); } }, 150);
});
