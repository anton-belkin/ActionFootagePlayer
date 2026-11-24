// SPDX-License-Identifier: GPL-3.0-or-later
//
// The comment on the Tags part (culling.js) and the search by it (app.js's textMatch), on the page in jsdom over
// mock.js: npm test here, or node comment_test.js. What's typed is saved by itself (a moment after the last key, at
// once when the field is left or the selection changes, ⌘↩ with the next one selected), never onto the item selected
// next; several with the same comment get it all; several with different ones only on Set for all. The search finds it
// by words and parts of words, Russian and English, without case, ё as е, every word typed.
const fs = require("fs"), path = require("path"), vm = require("vm");
const { JSDOM } = require("jsdom");
const ui = path.join(__dirname, "..", "ui");
let checks = 0, failures = 0;
function expect(what, got, want) {
  checks++;
  const a = JSON.stringify(got), b = JSON.stringify(want);
  if (a !== b) { failures++; console.log("FAILED " + what + ": got " + a + ", wanted " + b); }
}
const tick = (ms) => new Promise((r) => setTimeout(r, ms || 0));

(async () => {
  const html = fs.readFileSync(path.join(ui, "index.html"), "utf8").replace(/<script[^>]*><\/script>/g, "");
  const dom = new JSDOM(html, { url: "http://localhost/", runScripts: "outside-only", pretendToBeVisual: true });
  const w = dom.window;
  w.Element.prototype.scrollIntoView = function () {};
  if (!w.CSS) w.CSS = { escape: (s) => String(s).replace(/["\\]/g, "\\$&") };
  const ctx = dom.getInternalVMContext();
  const run = (file) => new vm.Script(fs.readFileSync(file, "utf8"), { filename: file }).runInContext(ctx);
  const ev = (code) => vm.runInContext(code, ctx);
  run(path.join(__dirname, "mock.js"));
  for (const m of fs.readFileSync(path.join(ui, "index.html"), "utf8").matchAll(/<script src="([^"]+)"><\/script>/g)) run(path.join(ui, m[1]));
  const doc = w.document, M = w.MOCK;
  const settle = async () => { for (let i = 0; i < 6; i++) { await tick(5); await ev("CULL.chain"); } };
  for (let i = 0; i < 100 && !ev("typeof RAW_FOOTAGE !== 'undefined' && RAW_FOOTAGE && DATA.footage ? 1 : 0"); i++) await tick(10);
  await tick(50);
  const q = (s) => doc.querySelector(s);
  const clipKey = (i) => "i:s:" + M.clips[i].id;
  const select = (keys) => ev("S.sel = {}; " + JSON.stringify(keys) + ".forEach((k) => { S.sel[k] = true; }); S.anchor = " + JSON.stringify(keys[0] || null) + "; S.cursor = S.anchor; renderAll()");
  const click = (el) => el.dispatchEvent(new w.MouseEvent("click", { bubbles: true }));
  const note = () => q("#cullnote");
  const type = (text) => { const f = note(); f.focus(); f.value = text; f.dispatchEvent(new w.Event("input", { bubbles: true })); };
  const keyIn = (k, more) => { const e = new w.KeyboardEvent("keydown", Object.assign({ key: k, bubbles: true, cancelable: true }, more || {})); note().dispatchEvent(e); return e; };
  const comments = () => M.calls.filter((c) => c[0] === "set_comment").map((c) => [c[1].items.map((i) => M.clips.findIndex((x) => x.id === i.id)), c[1].comment]);
  const shown = (i) => ev("(DATA.footage.items.find((x) => x.id === " + JSON.stringify(M.clips[i].id) + ") || {}).comment") || null;

  ev('S.path = ["Day 1"]; showTab("tags"); renderAll()');
  await tick(60);

  // One clip: an empty field that says what it's for, between the stars and the tags
  select([clipKey(0)]);
  expect("one clip: the field, empty, its placeholder", [note().value, note().getAttribute("placeholder"), note().disabled], ["", "What happens in this clip…", false]);
  const labs = Array.from(doc.querySelectorAll("#side .lab")).map((l) => l.textContent);
  expect("…after the stars, before the tags", labs.slice(0, 4), ["Keep it?", "Stars", "Comment", "Tags"]);

  // Typed: nothing written while typing; the app told one change waits; written a moment after the last key
  type("Черепаха");
  await tick(100);
  expect("typing: not written yet", comments(), []);
  expect("…the app told one change waits (closing waits for it)", M.calls.filter((c) => c[0] === "writes_pending").map((c) => c[1].n).slice(-1), [1]);
  type("Черепаха ест медузу");
  await tick(900); await settle();
  expect("a moment after the last key: written once, the whole text", comments(), [[[0], "Черепаха ест медузу"]]);
  expect("…shown on the clip", shown(0), "Черепаха ест медузу");
  expect("…the field keeps the focus and the text", [doc.activeElement && doc.activeElement.id, note().value], ["cullnote", "Черепаха ест медузу"]);

  // The panel drawn again while typing (an answer from the app, the probes): the text as typed, a space at its end too,
  // the focus and the caret kept; WebKit's focusout on the old field doesn't let the draft go
  type("Черепаха ест медузу ");
  note().setSelectionRange(4, 4);
  note().dispatchEvent(new w.FocusEvent("focusout", { bubbles: true }));
  ev("renderSide()");
  await tick(5);
  expect("drawn again while typing: text, focus, caret kept", [note().value, doc.activeElement && doc.activeElement.id, note().selectionStart], ["Черепаха ест медузу ", "cullnote", 4]);
  ev("renderAll()");
  expect("…by renderAll too", [note().value, doc.activeElement && doc.activeElement.id], ["Черепаха ест медузу ", "cullnote"]);

  // Typed, then another clip selected at once (a click: the field loses its focus first) — saved to the first clip,
  // not to the one selected
  type("Черепаха ест медузу, потом скат");
  note().blur();
  select([clipKey(1)]);
  await settle();
  expect("left for another clip: saved at once, to the clip it was typed for", comments().slice(-1), [[[0], "Черепаха ест медузу, потом скат"]]);
  expect("…the next clip's field is its own (empty)", [note().value, shown(1)], ["", null]);
  // The same without the field losing its focus first (the selection changed by the app): still saved, to its clip
  type("Риф");
  select([clipKey(2)]);
  await settle();
  expect("selection changed under the field: saved to its clip", comments().slice(-1), [[[1], "Риф"]]);
  expect("…and not onto the one selected now", shown(2), null);

  // ⌘↩: saved, the next clip selected, the field in front for it
  select([clipKey(2)]);
  type("Дельфины далеко");
  keyIn("Enter", { metaKey: true });
  await settle();
  expect("⌘↩: saved", comments().slice(-1), [[[2], "Дельфины далеко"]]);
  expect("…the next clip selected, the field focused for it", [ev("Object.keys(S.sel)"), doc.activeElement && doc.activeElement.id], [[clipKey(3)], "cullnote"]);
  // Enter alone is a new line, not a save
  type("Первая строка");
  const plain = keyIn("Enter");
  expect("Enter alone: a new line (not taken)", plain.defaultPrevented, false);
  // Esc: saved, back to the list
  keyIn("Escape");
  await settle();
  expect("Esc: saved, the field left", [comments().slice(-1), doc.activeElement && doc.activeElement.id !== "cullnote"], [[[[3], "Первая строка"]], true]);

  // Emptied: taken off
  select([clipKey(3)]);
  type("   ");
  await tick(900); await settle();
  expect("emptied: taken off", [comments().slice(-1), shown(3)], [[[[3], null]], null]);

  // Two with the same (none): typed for both
  select([clipKey(3), clipKey(4)]);
  expect("two without: a comment for both", note().getAttribute("placeholder"), "A comment for all 2…");
  type("Wreck at dusk");
  await tick(900); await settle();
  expect("…written to both", comments().slice(-1), [[[3, 4], "Wreck at dusk"]]);
  // Two with different ones: shown empty, said so, nothing written until Set for all
  select([clipKey(0), clipKey(1)]);
  expect("different: empty, said, Set for all off", [note().value, /Different comments \(2 of 2 have one\)/.test(note().getAttribute("placeholder")), q('#side [data-c="noteall"]').disabled], ["", true, true]);
  const before = comments().length;
  type("Риф с черепахой");
  await tick(900); await settle();
  expect("…typing writes nothing", comments().length, before);
  expect("…Set for all on once typed", q('#side [data-c="noteall"]').disabled, false);
  note().blur(); await settle();
  expect("…nor leaving the field", comments().length, before);
  click(q('#side [data-c="noteall"]'));
  await settle();
  expect("Set for all: written to both", comments().slice(-1), [[[0, 1], "Риф с черепахой"]]);
  expect("…both have it now, shown as theirs", [shown(0), shown(1), note().value], ["Риф с черепахой", "Риф с черепахой", "Риф с черепахой"]);
  // Different, typed, another selection: dropped
  select([clipKey(1), clipKey(2)]);
  const b2 = comments().length;
  type("не то");
  select([clipKey(4)]);
  await tick(900); await settle();
  expect("different, typed, then another selection: dropped", [comments().length, shown(1), shown(2)], [b2, "Риф с черепахой", "Дельфины далеко"]);

  // The comment as the tile's tooltip
  expect("the row's tooltip is its comment", q('#main [data-v="' + w.CSS.escape(clipKey(2)) + '"]').getAttribute("title"), "Дельфины далеко");
  ev("S.layout = 'grid'; renderAll()");
  const tile = q('#main [data-v="' + w.CSS.escape(clipKey(2)) + '"]');
  expect("…and the grid tile's (tiles.js), the tile otherwise as it was", [tile.classList.contains("tile"), tile.getAttribute("title"), tile.textContent.includes("Дельфины")], [true, "Дельфины далеко", false]);
  ev("S.layout = 'list'; renderAll()");

  // The search, over the whole of Originals
  const found = (text) => { ev("S.path = []; S.sel = {}; S.q = " + JSON.stringify(text) + "; renderAll()"); return ev("VISIBLE.slice()").map((k) => M.clips.findIndex((c) => "i:s:" + c.id === k)).filter((i) => i >= 0).sort(); };
  expect("search: a word of a comment, any case", found("ДЕЛЬФИНЫ"), [2]);
  expect("…a part of a word", found("дельф"), [2]);
  expect("…several words: all of them, in any order", found("черепахой риф"), [0, 1]);
  expect("…a word in the comment and a tag", found("черепах turtle"), [0]);
  expect("…a word nowhere: nothing", found("черепах акула"), []);
  expect("…ё as е, both ways", [found("ежик"), found("мурёна")], [[6], [6]]);
  expect("…English too", found("wreck dusk"), [3, 4]);
  expect("…what it found before still found (names)", found("_D.MP4").length, M.clips.length);
  ev('S.q = ""; renderAll()');

  console.log(checks + " checks, " + failures + " failed");
  w.close();
  process.exit(failures ? 1 : 0);
})().catch((e) => { console.error(e); process.exit(1); });
