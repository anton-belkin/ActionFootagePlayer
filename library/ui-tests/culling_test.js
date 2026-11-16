// SPDX-License-Identifier: GPL-3.0-or-later
//
// The Library's culling and tags, on the page itself (culling.js with app.js) in jsdom, over mock.js:
//   npm install jsdom (anywhere), then NODE_PATH=<its node_modules> node library/ui-tests/culling_test.js
// The Tags part's buttons for one, several and mixed selections, the tags in use, the new tag's suggestions, the keys,
// Hide rejected across the tabs, Most stars first, photos. Since the merge of the night of 2026-11-16 the tabs are
// Originals (the clips: where the page opens) and Albums (the clips' links, the moments, the photos, the exports):
// the clips are culled in Originals here, the moments and photos where Albums show them, and a clip's link in an album.
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
  // The page's own scripts, as index.html names them, in its order
  for (const m of fs.readFileSync(path.join(ui, "index.html"), "utf8").matchAll(/<script src="([^"]+)"><\/script>/g)) run(path.join(ui, m[1]));
  const doc = w.document, M = w.MOCK;
  const settle = async () => { for (let i = 0; i < 6; i++) { await tick(5); await ev("CULL.chain"); } };
  for (let i = 0; i < 100 && !ev("typeof RAW_FOOTAGE !== 'undefined' && RAW_FOOTAGE && DATA.footage && DATA.moments ? 1 : 0"); i++) await tick(10);
  await tick(50);
  const q = (s) => doc.querySelector(s), qa = (s) => Array.from(doc.querySelectorAll(s));
  const pane = () => q("#side .pane");
  const pressed = (sel) => { const e = q("#side " + sel); return e ? e.getAttribute("aria-pressed") : null; };
  const pick = (v) => q('#side [data-c="pick"][data-v="' + v + '"]');
  const star = (n) => q('#side [data-c="rate"][data-v="' + n + '"]');
  const tagb = (t) => q('#side [data-c="tag"][data-v="' + t + '"]');
  // A clip's key in Originals, and its link's in the album of its day
  const clipKey = (i) => "i:s:" + M.clips[i].id;
  const linkKey = (i) => "i:a:" + M.links[i].rel;
  const tab = (v) => q('#top [data-a="view"][data-v="' + v + '"]');
  const shownOf = (view, id) => ev("(DATA." + view + ".items.find((x) => x.id === " + JSON.stringify(id) + ") || {})");
  const scans = () => M.calls.filter((c) => c[0] === "scan").length;
  const select = (keys) => ev("S.sel = {}; " + JSON.stringify(keys) + ".forEach((k) => { S.sel[k] = true; }); S.anchor = " + JSON.stringify(keys[0] || null) + "; S.cursor = S.anchor; renderAll()");
  const click = (el) => el.dispatchEvent(new w.MouseEvent("click", { bubbles: true }));
  const key = (k, code, target, more) => { const e = new w.KeyboardEvent("keydown", Object.assign({ key: k, code: code || "", bubbles: true, cancelable: true }, more || {})); (target || doc.body).dispatchEvent(e); return e; };
  const type = (text) => { const i = q("#tagdraft"); i.focus(); i.value = text; i.dispatchEvent(new w.Event("input", { bubbles: true })); };
  const sugg = () => qa("#tagsugg button").map((b) => b.textContent);
  const lastCall = (cmd) => M.calls.filter((c) => c[0] === cmd).pop();

  // The Tags part, in Day 1 of Originals (the tab the page opens on)
  expect("the page opens on Originals, of the two tabs", [ev("S.view"), qa('#top [data-a="view"]').map((b) => b.textContent)], ["sources", ["Originals", "Albums"]]);
  ev('S.path = ["Day 1"]; showTab("tags"); renderAll()');
  await tick(60);
  expect("the page is up: Day 1's clips", ev("VISIBLE.length"), 5);

  // Nothing selected: everything in its place, greyed
  expect("none selected: Accept there, off", [!!pick("accept"), pick("accept").disabled, pressed('[data-c="pick"][data-v="accept"]')], [true, true, "false"]);
  expect("none selected: five stars and None", [qa('#side .star').length, !!star(0), star(3).disabled], [5, true, true]);
  expect("none selected: the tags in use listed", !!tagb("Reef") && tagb("Reef").disabled, true);
  expect("none selected: the field too", q("#tagdraft").disabled, true);

  // The tags in use over the whole library (the photos' too, asked for), the most used first
  const order = qa("#side .tagb").map((b) => b.dataset.v);
  expect("tags in use: most used first", order.slice(0, 3), ["Reef", "Turtle", "Wreck"]);
  expect("tags in use: one spelling of each", order.filter((t) => t.toLowerCase() === "reef").length, 1);
  expect("tags in use: the photos were read for it", M.calls.some((c) => c[0] === "scan" && c[1].view === "stills"), true);

  // Two accepted clips, 4 and 2 stars, Reef on both and Turtle on one
  select([clipKey(0), clipKey(1)]);
  expect("all accepted: Accept on", [pressed('[data-c="pick"][data-v="accept"]'), pick("accept").classList.contains("on")], ["true", true]);
  expect("all accepted: the others off", [pressed('[data-c="pick"][data-v="reject"]'), pressed('[data-c="pick"][data-v="none"]')], ["false", "false"]);
  expect("stars 4 and 2: two on, two some, one off", [1, 2, 3, 4, 5].map((n) => star(n).getAttribute("aria-pressed")), ["true", "true", "mixed", "mixed", "false"]);
  expect("stars: said mixed", /mixed: 2–4/.test(q("#side .stars .sub").textContent), true);
  expect("a tag on all: on; on one: some; on none: off", [tagb("Reef").getAttribute("aria-pressed"), tagb("Turtle").getAttribute("aria-pressed"), tagb("Wreck").getAttribute("aria-pressed")], ["true", "mixed", "false"]);
  expect("…and looks so", [tagb("Reef").classList.contains("on"), tagb("Turtle").classList.contains("some")], [true, true]);

  // Accepted, accepted, rejected, undecided
  select([0, 1, 2, 3].map(clipKey));
  expect("mixed: each of the three some", ["accept", "reject", "none"].map((v) => pick(v).getAttribute("aria-pressed")), ["mixed", "mixed", "mixed"]);
  expect("mixed: how many", pick("accept").textContent.replace(/\s+/g, " "), "✓ Accept 2");
  // Reject on a mixed selection: all of it, shown at once and written
  const scans0 = scans();
  click(pick("reject"));
  expect("Reject: at once on the page", [pressed('[data-c="pick"][data-v="reject"]'), pressed('[data-c="pick"][data-v="accept"]')], ["true", "false"]);
  expect("…the rows too", ev('items().filter((it) => it.dir === "Day 1" && it.pick === "reject").length'), 4);
  expect("…and the clips' links in their album", ev('DATA.footage.items.filter((it) => it.kind === "clip" && it.dir === "Day 1" && it.pick === "reject").length'), 4);
  await settle();
  // (writes.js: shown at once, written after; the page keeps its own patch, no view is read again)
  expect("Reject: no view read again for it", scans() - scans0, 0);
  expect("Reject: written for the four", [lastCall("set_pick")[1].pick, lastCall("set_pick")[1].items.length, M.clips.slice(0, 4).every((c) => c.pick === "reject")], ["reject", 4, true]);
  expect("…and the same once it's written", pressed('[data-c="pick"][data-v="reject"]'), "true");
  // Again: neither (as the player's X)
  click(pick("reject"));
  await settle();
  expect("Reject again: neither", [lastCall("set_pick")[1].pick, pressed('[data-c="pick"][data-v="none"]'), M.clips[0].pick], [null, "true", undefined]);
  click(pick("accept")); await settle();
  click(pick("none")); await settle();
  expect("Undecided: the way back", [lastCall("set_pick")[1].pick, M.clips[1].pick], [null, undefined]);

  // Stars
  click(star(3));
  expect("3 stars: at once", [1, 2, 3, 4, 5].map((n) => star(n).getAttribute("aria-pressed")), ["true", "true", "true", "false", "false"]);
  await settle();
  expect("3 stars: written", [lastCall("set_rating")[1].rating, M.clips[3].rating], [3, 3]);
  click(star(3)); await settle();
  expect("the same stars again: none", [lastCall("set_rating")[1].rating, M.clips[3].rating, pressed('[data-c="rate"][data-v="0"]')], [null, undefined, "true"]);
  click(star(5)); await settle();
  click(star(0)); await settle();
  expect("None: none", [lastCall("set_rating")[1].rating, M.clips[0].rating], [null, undefined]);

  // A tag in use: on part of the selection, a click puts it on the rest; on all, a click takes it off
  expect("Reef on three of four", tagb("Reef").getAttribute("aria-pressed"), "mixed");
  click(tagb("Reef")); await settle();
  expect("a tag on some: put on the ones without", [lastCall("set_tags")[1].add, lastCall("set_tags")[1].items.length, tagb("Reef").getAttribute("aria-pressed")], [["Reef"], 1, "true"]);
  expect("…the spelling an item had stays", M.clips[2].tags.includes("reef"), true);
  click(tagb("Reef")); await settle();
  expect("a tag on all: taken off all", [lastCall("set_tags")[1].remove, lastCall("set_tags")[1].items.length, M.clips.slice(0, 4).some((c) => c.tags.some((t) => t.toLowerCase() === "reef"))], [["Reef"], 4, false]);

  // The new tag's field: suggestions from the tags in use, never in the way of a new one
  type("tu");
  expect("typing: the ones in use that start so", sugg(), ["Turtle"]);
  expect("typing: none picked for you", q("#tagsugg .on"), null);
  key("ArrowDown", "ArrowDown", q("#tagdraft"));
  expect("Down: on the first", q("#tagsugg .on").textContent, "Turtle");
  key("Enter", "Enter", q("#tagdraft")); await settle();
  expect("Enter on a suggestion: that tag, on the ones without it", [lastCall("set_tags")[1].add, lastCall("set_tags")[1].items.length], [["Turtle"], 3]);
  expect("…the field emptied, the list closed, the focus kept", [q("#tagdraft").value, q("#tagsugg").hidden, doc.activeElement.id], ["", true, "tagdraft"]);
  type("Shark");
  expect("a new word: nothing suggested", sugg(), []);
  key("Enter", "Enter", q("#tagdraft")); await settle();
  expect("Enter: the new tag as typed", [lastCall("set_tags")[1].add, M.clips[0].tags.includes("Shark")], [["Shark"], true]);
  expect("…and it's a button now", !!tagb("Shark") && tagb("Shark").getAttribute("aria-pressed"), "true");
  type("wr");
  const tabKey = key("Tab", "Tab", q("#tagdraft"));
  expect("Tab: completed to the first", [q("#tagdraft").value, tabKey.defaultPrevented], ["Wreck", true]);
  key("Escape", "Escape", q("#tagdraft"));
  expect("Esc: the list closed, the selection kept", [q("#tagsugg").hidden, ev("Object.keys(S.sel).length")], [true, 4]);
  type("re");
  expect("typed inside a word too, the starts first", sugg().slice(0, 1), ["Reef"]);
  key("Enter", "Enter", q("#tagdraft")); await settle();
  expect("Enter with none picked: what was typed, not a suggestion", lastCall("set_tags")[1].add, ["re"]);
  type("DOLPHIN"); key("Enter", "Enter", q("#tagdraft")); await settle();
  expect("a tag in use typed in another case: the library's spelling", lastCall("set_tags")[1].add, ["Dolphin"]);
  // A letter typed in the field is a letter
  const before = M.calls.length;
  key("p", "KeyP", q("#tagdraft")); key("3", "Digit3", q("#tagdraft")); await settle();
  expect("P and 3 in the field: not culling", M.calls.slice(before).filter((c) => c[0] === "set_pick" || c[0] === "set_rating").length, 0);
  q("#tagdraft").blur();

  // The keys, with the list in front. One selected: the next one after
  select([clipKey(3)]);
  let e = key("p", "KeyP");
  expect("P: accepted at once, the next one selected", [e.defaultPrevented, ev('items().find((it) => it.id === ' + JSON.stringify(M.clips[3].id) + ').pick'), ev("Object.keys(S.sel)")], [true, "accept", [clipKey(4)]]);
  await settle();
  expect("P: written", M.clips[3].pick, "accept");
  key("3", "Digit3"); await settle();
  expect("3: stars; the last one keeps the selection", [M.clips[4].rating, ev("Object.keys(S.sel)")], [3, [clipKey(4)]]);
  key("0", "Digit0"); await settle();
  expect("0: no stars", M.clips[4].rating, undefined);
  select([clipKey(0)]);
  key("ч", "KeyX"); await settle();
  expect("X in a Cyrillic layout: rejected, the next selected", [M.clips[0].pick, ev("Object.keys(S.sel)")], ["reject", [clipKey(1)]]);
  select([clipKey(0)]);
  key("u", "KeyU"); await settle();
  expect("U: undecided", M.clips[0].pick, undefined);
  // Several selected: all of it, the selection stays
  select([clipKey(0), clipKey(1)]);
  key("x", "KeyX"); await settle();
  expect("X on two: both, still selected", [M.clips[0].pick, M.clips[1].pick, ev("Object.keys(S.sel).length")], ["reject", "reject", 2]);
  key("x", "KeyX"); await settle();
  expect("X again on two: neither", [M.clips[0].pick, M.clips[1].pick], [undefined, undefined]);
  // With ⌘ or Ctrl: not culling
  const n0 = M.calls.length;
  key("p", "KeyP", null, { metaKey: true }); await settle();
  expect("⌘P: not culling", M.calls.slice(n0).some((c) => c[0] === "set_pick"), false);
  // A folder selected: what's in it
  ev('S.path = [AV_MARK + "My albums"]; S.sel = { "f:Day 2": true }; renderAll()');
  key("4", "Digit4"); await settle();
  expect("a folder: its clips rated", [M.clips[5].rating, M.clips[6].rating, M.clips[7].rating], [4, 4, 4]);

  // Hide rejected: in the list, in the folders' counts, across the tabs
  ev('S.path = ["Day 1"]; S.sel = {}; renderAll()');
  select([clipKey(1)]); key("x", "KeyX"); await settle();
  const hide = () => q('#top [data-c="hide"]');
  expect("Hide rejected: a switch on top, off", hide().getAttribute("aria-pressed"), "false");
  click(hide());
  expect("Hide rejected: the rejected one gone from the list", [hide().getAttribute("aria-pressed"), ev("VISIBLE.indexOf(" + JSON.stringify(clipKey(1)) + ")"), ev("VISIBLE.length")], ["true", -1, 4]);
  expect("…said how many", /· 2$/.test(hide().textContent), true);
  // Rejecting one while they're hidden: it goes, the next one is selected
  select([clipKey(2)]); key("x", "KeyX");
  expect("X while hiding: gone, the next selected", [ev("VISIBLE.indexOf(" + JSON.stringify(clipKey(2)) + ")"), ev("Object.keys(S.sel)")], [-1, [clipKey(3)]]);
  await settle();
  // In Albums (the moments and the photos are there since the merge: at its top here, beside the two albums)
  const kindOf = (k) => ({ a: "clip", m: "moment", p: "still", e: "export" }[k.slice(2, 3)] || "folder");
  const visibleOf = (kind) => ev("VISIBLE").filter((k) => kindOf(k) === kind);
  click(tab("footage")); await tick(60);
  ev('S.path = [AV_MARK + "My albums"]; renderAll()'); // (what's at the albums' top is in My albums)
  expect("in Albums: still on, the rejected moment hidden", [hide().getAttribute("aria-pressed"), ev('VISIBLE.some((k) => k.includes("Dolphins far"))'), visibleOf("moment").length], ["true", false, 2]);
  expect("…and the rejected photo", [ev('VISIBLE.some((k) => k.includes("Blurry"))'), visibleOf("still").length], [false, 2]);
  ev('S.path = ["Day 1"]; renderAll()');
  expect("…and the rejected clips' links in their album", [ev("VISIBLE.indexOf(" + JSON.stringify(linkKey(1)) + ")"), ev("VISIBLE.indexOf(" + JSON.stringify(linkKey(2)) + ")"), ev("VISIBLE.length")], [-1, -1, 3]);
  ev('S.path = [AV_MARK + "My albums"]; renderAll()');
  expect("remembered on this computer", w.localStorage.getItem("afl.hideRejected"), "1");

  // Photos are culled and tagged like moments
  const REEF2 = "_Stills/Reef 2.heic";
  select(["i:p:" + REEF2]);
  expect("a photo: the buttons work", [pick("accept").disabled, star(1).disabled], [false, false]);
  let n1 = scans();
  key("p", "KeyP");
  expect("a photo accepted: at once, on the photo and where Albums show it", [shownOf("stills", REEF2).pick, ev('DATA.footage.items.find((x) => x.key === "p:' + REEF2 + '").pick')], ["accept", "accept"]);
  await settle();
  expect("a photo accepted", [lastCall("set_pick")[1].items[0].kind, M.stills[1].pick], ["still", "accept"]);
  select(["i:p:" + REEF2]);
  click(star(4)); await settle();
  expect("a photo rated", [M.stills[1].rating, ev('DATA.footage.items.find((x) => x.key === "p:' + REEF2 + '").rating')], [4, 4]);
  click(tagb("Turtle")); await settle();
  expect("a photo tagged with a tag in use", [M.stills[1].tags.includes("Turtle"), ev('DATA.footage.items.find((x) => x.key === "p:' + REEF2 + '").tags')], [true, ["Reef", "Turtle"]]);
  expect("…its row shows it", /✓ Accepted.*★★★★.*Turtle/.test(q('#main [data-v="i:p:' + REEF2 + '"] .chips').textContent), true);
  expect("…and no view was read again", scans() - n1, 0);
  // A moment, the same
  const WRECK = "_Moments/Wreck bow.afplink.mp4";
  select(["i:m:" + WRECK]);
  n1 = scans();
  key("p", "KeyP");
  // (one selected: the next one is selected after each key; the moment again)
  select(["i:m:" + WRECK]);
  key("2", "Digit2");
  select(["i:m:" + WRECK]);
  click(tagb("Reef"));
  expect("a moment accepted, rated, tagged: at once", [shownOf("moments", WRECK).pick, shownOf("moments", WRECK).rating, ev('DATA.footage.items.find((x) => x.key === "m:' + WRECK + '").tags')], ["accept", 2, ["Wreck", "Reef"]]);
  await settle();
  expect("…written", [M.moments[1].pick, M.moments[1].rating, M.moments[1].tags], ["accept", 2, ["Wreck", "Reef"]]);
  expect("…and no view was read again", scans() - n1, 0);
  // Filters: Keep it? and Stars, photos too
  expect("filters: Keep it? and Stars in Albums", ev('allFacets().filter((f) => f.id === "pick" || f.id === "stars").map((f) => f.opts.length)'), [3, 6]);
  ev('setOpt("pick:a", true); renderAll()');
  expect("filter Accepted: the accepted photos", visibleOf("still").length, 2);
  ev('S.filters = {}; renderAll()');

  // Most stars first
  click(hide());
  const sort = () => q('#top [data-c="starsort"]');
  click(sort());
  expect("Most stars first: by stars, the rest as they were", visibleOf("still").map((k) => k.replace("i:p:_Stills/", "")), ["Turtle 1.heic", "Reef 2.heic", "Blurry 3.heic"]);
  expect("…folders stay on top", ev("VISIBLE.slice(0, 2)"), ["f:Day 1", "f:Day 2"]);
  click(sort());

  // A clip selected in its album (its link): the clip itself is what's culled; it shows at once on the link and on
  // the clip in Originals, and no view is read again
  ev('S.path = ["Day 2"]; renderAll()');
  select([linkKey(6)]);
  n1 = scans();
  key("x", "KeyX");
  expect("X on a link: at once on it and in Originals, the next link selected", [ev('DATA.footage.items.find((x) => "i:" + x.key === ' + JSON.stringify(linkKey(6)) + ').pick'), shownOf("sources", M.clips[6].id).pick, ev("Object.keys(S.sel)")], ["reject", "reject", [linkKey(7)]]);
  select([linkKey(6)]);
  click(star(1)); click(tagb("Wreck"));
  expect("stars and a tag on a link: at once, both places", [shownOf("sources", M.clips[6].id).rating, shownOf("sources", M.clips[6].id).tags, ev('DATA.footage.items.find((x) => "i:" + x.key === ' + JSON.stringify(linkKey(6)) + ').tags')], [1, ["for family", "Wreck"], ["for family", "Wreck"]]);
  await settle();
  expect("…written for the clip", [lastCall("set_pick")[1].items[0], M.clips[6].pick, M.clips[6].rating, M.clips[6].tags], [{ kind: "clip", id: M.clips[6].id, path: M.clips[6].path }, "reject", 1, ["for family", "Wreck"]]);
  expect("…and no view was read again", scans() - n1, 0);
  click(tab("sources")); await tick(60);
  ev('S.path = ["Day 2"]; renderAll()');
  expect("…its row in Originals", /✕ Rejected.*★.*Wreck/.test(q('#main [data-v="' + w.CSS.escape(clipKey(6)) + '"] .chips').textContent), true);

  // Exports (Albums › Other folders › _Exports): not culled
  click(tab("footage")); await tick(60);
  ev('S.path = ["Other folders", "_Exports"]; renderAll()');
  select(["i:e:" + M.exports[0].id]);
  const n2 = M.calls.length;
  key("p", "KeyP"); key("3", "Digit3"); await settle();
  expect("an export: no buttons, the keys do nothing", [q('#side [data-c="pick"]'), /Exports aren’t kept/.test(pane().textContent), M.calls.slice(n2).filter((c) => /^set_/.test(c[0])).length], [null, true, 0]);
  // A write that fails is said (until it's read: the strip of what wasn't saved), and the clip shows what's on disk
  click(tab("sources")); await tick(60);
  ev('S.path = ["Day 1"]; renderAll()');
  M.handlers.set_pick = () => ["DJI: couldn't write"];
  select([clipKey(4)]); click(pick("accept")); await settle(); await tick(80);
  expect("a failed write: said", [q("#unsaved").hidden, /Not all saved: Accepted on .*couldn't write/.test(q("#unsaved").textContent)], [false, true]);
  expect("…and the clip as it is on disk", pressed('[data-c="pick"][data-v="accept"]'), "false");

  console.log(checks + " checks, " + failures + " failed");
  w.close();
  process.exit(failures ? 1 : 0);
})().catch((e) => { console.error(e); process.exit(1); });
