// SPDX-License-Identifier: GPL-3.0-or-later
//
// The Library's culling and tags, on the page itself (culling.js with app.js) in jsdom, over tests/mock.js:
//   npm install jsdom (anywhere), then NODE_PATH=<its node_modules> node library/ui/tests/culling_test.js
// The Tags part's buttons for one, several and mixed selections, the tags in use, the new tag's suggestions, the keys,
// Hide rejected across the tabs, Most stars first, photos.
const fs = require("fs"), path = require("path"), vm = require("vm");
const { JSDOM } = require("jsdom");
const ui = path.join(__dirname, "..");
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
  run(path.join(ui, "culling.js"));
  run(path.join(ui, "app.js"));
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
  const clipKey = (i) => "i:" + M.clips[i].id;
  const select = (keys) => ev("S.sel = {}; " + JSON.stringify(keys) + ".forEach((k) => { S.sel[k] = true; }); S.anchor = " + JSON.stringify(keys[0] || null) + "; S.cursor = S.anchor; renderAll()");
  const click = (el) => el.dispatchEvent(new w.MouseEvent("click", { bubbles: true }));
  const key = (k, code, target, more) => { const e = new w.KeyboardEvent("keydown", Object.assign({ key: k, code: code || "", bubbles: true, cancelable: true }, more || {})); (target || doc.body).dispatchEvent(e); return e; };
  const type = (text) => { const i = q("#tagdraft"); i.focus(); i.value = text; i.dispatchEvent(new w.Event("input", { bubbles: true })); };
  const sugg = () => qa("#tagsugg button").map((b) => b.textContent);
  const lastCall = (cmd) => M.calls.filter((c) => c[0] === cmd).pop();

  // The Tags part, in Day 1 of Footage
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
  click(pick("reject"));
  expect("Reject: at once on the page", [pressed('[data-c="pick"][data-v="reject"]'), pressed('[data-c="pick"][data-v="accept"]')], ["true", "false"]);
  expect("…the rows too", ev('items().filter((it) => it.dir === "Day 1" && it.pick === "reject").length'), 4);
  await settle();
  expect("Reject: written for the four", [lastCall("set_pick")[1].pick, lastCall("set_pick")[1].items.length, M.clips.slice(0, 4).every((c) => c.pick === "reject")], ["reject", 4, true]);
  expect("…and the same after the view is read again", pressed('[data-c="pick"][data-v="reject"]'), "true");
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
  const tab = key("Tab", "Tab", q("#tagdraft"));
  expect("Tab: completed to the first", [q("#tagdraft").value, tab.defaultPrevented], ["Wreck", true]);
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
  ev('S.path = []; S.sel = { "f:Day 2": true }; renderAll()');
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
  click(q('#top [data-a="view"][data-v="moments"]')); await tick(60);
  expect("in Moments: still on, the rejected moment hidden", [hide().getAttribute("aria-pressed"), ev('VISIBLE.some((k) => k.includes("Dolphins far"))'), ev("VISIBLE.length")], ["true", false, 2]);
  click(q('#top [data-a="view"][data-v="stills"]')); await tick(60);
  expect("in Photos: the rejected photo hidden", ev('VISIBLE.some((k) => k.includes("Blurry"))'), false);
  expect("remembered on this computer", w.localStorage.getItem("afl.hideRejected"), "1");

  // Photos are culled and tagged like moments
  select(["i:_Stills/Reef 2.heic"]);
  expect("a photo: the buttons work", [pick("accept").disabled, star(1).disabled], [false, false]);
  key("p", "KeyP"); await settle();
  expect("a photo accepted", [lastCall("set_pick")[1].items[0].kind, M.stills[1].pick], ["still", "accept"]);
  select(["i:_Stills/Reef 2.heic"]);
  click(star(4)); await settle();
  expect("a photo rated", M.stills[1].rating, 4);
  click(tagb("Turtle")); await settle();
  expect("a photo tagged with a tag in use", M.stills[1].tags.includes("Turtle"), true);
  // Filters: Keep it? and Stars, photos too
  expect("filters: Keep it? and Stars in Photos", ev('allFacets().filter((f) => f.id === "pick" || f.id === "stars").map((f) => f.opts.length)'), [3, 6]);
  ev('setOpt("pick:a", true); renderAll()');
  expect("filter Accepted: the accepted photos", ev("VISIBLE.length"), 2);
  ev('S.filters = {}; renderAll()');

  // Most stars first
  click(hide());
  const sort = () => q('#top [data-c="starsort"]');
  click(sort());
  expect("Most stars first: by stars, the rest as they were", ev("VISIBLE").map((k) => k.replace("i:_Stills/", "")), ["Turtle 1.heic", "Reef 2.heic", "Blurry 3.heic"]);
  click(q('#top [data-a="view"][data-v="footage"]')); await tick(60);
  expect("…folders stay on top", ev("VISIBLE.slice(0, 2)"), ["f:Day 1", "f:Day 2"]);
  click(sort());

  // Exports: not culled
  click(q('#top [data-a="view"][data-v="exports"]')); await tick(60);
  expect("Exports: no buttons, no switches", [q('#side [data-c="pick"]'), q('#top [data-c="hide"]')], [null, null]);
  // A write that fails is said, and the view shows what's on disk
  click(q('#top [data-a="view"][data-v="footage"]')); await tick(60);
  ev('S.path = ["Day 1"]; renderAll()');
  M.handlers.set_pick = () => ["DJI: couldn't write"];
  select([clipKey(4)]); click(pick("accept")); await settle();
  expect("a failed write: said", /couldn't write/.test(q("#error").textContent) && !q("#error").hidden, true);
  expect("…and the clip as it is on disk", pressed('[data-c="pick"][data-v="accept"]'), "false");

  console.log(checks + " checks, " + failures + " failed");
  w.close();
  process.exit(failures ? 1 : 0);
})().catch((e) => { console.error(e); process.exit(1); });
