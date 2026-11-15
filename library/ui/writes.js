"use strict";
// Screen first, disk after (docs/library-responsiveness.md): a change made in the Library's window shows at once, and
// is written in the background; one that couldn't be written is taken back, and the window says plainly what wasn't
// saved and why. This file is the mechanism, on its own (app.js is its host: `Writes.host`); a command joins it with a
// few lines (`Writes.define`), as tags, locations, Source settings, renaming and deleting do in app.js.
//
// How it's kept right:
//
// - What the disk has and what the window shows are two things. The host keeps each view as the app last read it
//   (`host.get(view)`: "confirmed"); what's shown is that with the changes still being written put over it, in the
//   order they were made (`Writes.over`). Nothing is undone by hand: a change that failed is simply no longer put
//   over, so what shows is what the disk has plus the changes after it. A later change is never undone by an earlier
//   one's failure.
// - One write at a time, in the order the changes were made (two commands on one clip's own file, or on the
//   project's file, would otherwise read it and write it over each other). The window never waits for the queue.
// - A change that is written becomes part of what the disk has (the same patch, on the confirmed view: `land`), or,
//   when the command says what its items are now (`Changed`, Path 2), those items are.
// - A view read from the disk while a change was being written may have been read before it landed. A change stays
//   put over every reading that started before it landed (`reading` / `read`), and is let go once those are in.
// - A change the app wrote only in part without saying which (a list of problems): stays shown until the views are
//   read again after it, then the disk decides.
// - A change from elsewhere (the player, another computer) for an item with a change of ours waiting: ours stays on
//   top until it lands, since it's put over whatever the disk says meanwhile.
// - A change made to something an earlier change made (a renamed moment tagged at once) and that earlier one fails:
//   it can't be done either, and is taken back with it, said so.
// - Closing the window with changes waiting: they're written first (`drained`); the app keeps the window until then.
//
// A patch must be the command's own rule (what it does to the item on disk), and doing it twice must be as doing it
// once: it is put over fresh readings again and again.

const Writes = (() => {
  const VIEW_OF = { clip: "footage", moment: "moments", still: "stills", export: "exports" };
  const kinds = {};
  let ops = [];              // made and not let go yet, in the order made
  let busy = false;          // one being written
  let seq = 0;               // counts readings started and writes ended, to tell which came first
  const readings = new Set();  // views being read from the disk: { view, at }
  const waiters = [];
  const listeners = [];
  const W = { host: null, VIEW_OF };

  const clone = (v) => (v == null ? v : JSON.parse(JSON.stringify(v)));
  const active = (o) => o.state === "queued" || o.state === "writing";
  function tell() { for (const f of listeners) f(); if (!W.pending()) while (waiters.length) waiters.shift()(); }

  // A view's data to change without touching the one it's made from: the lists copied, an item copied when first
  // asked for. What a kind's `patch` gets
  function draft(view, data) {
    let index = null;
    const own = new Set();
    const d = {
      view, data: Object.assign({}, data, { items: (data.items || []).slice() }),
      // The item with that id, to change; null: not in this view
      item(id) {
        if (!index) { index = new Map(); d.data.items.forEach((it, i) => index.set(it.id, i)); }
        const i = index.get(id);
        if (i === undefined) return null;
        if (!own.has(d.data.items[i])) { d.data.items[i] = clone(d.data.items[i]); own.add(d.data.items[i]); }
        return d.data.items[i];
      },
      // Every item `test` takes, to change
      where(test) { return d.data.items.filter(test).map((it) => d.item(it.id)); },
      remove(test) { d.data.items = d.data.items.filter((it) => !test(it)); index = null; },
      // An item whose id or folder changed: its id as it is now, and put where a reading of the disk would have it
      // among the others of its folder (by name)
      moved(it) {
        const list = d.data.items.filter((x) => x !== it);
        let at = -1, last = -1;
        list.forEach((x, i) => { if (x.dir === it.dir) { last = i; if (at < 0 && x.id > it.id) at = i; } });
        list.splice(at >= 0 ? at : last + 1 || list.length, 0, it);
        d.data.items = list; index = null;
      },
      // A list of folders of the view (`dirs`, Footage's `clips_dirs`) changed
      list(name, f) { d.data[name] = f((d.data[name] || []).slice()); },
    };
    return d;
  }

  function patched(view, data, list) {
    if (!data) return data;
    list = list.filter((o) => o.views.includes(view));
    if (!list.length) return data;
    const d = draft(view, data);
    for (const o of list) {
      const k = kinds[o.kind];
      if (k.patch) k.patch(d, o.args, o);
      else for (const t of o.targets) { if (t.view !== view) continue; const it = d.item(t.id); if (it) k.item(it, o.args, o); }
    }
    return d.data;
  }

  // What's shown of a view: what the disk has, with the changes not let go yet over it
  W.over = (view, data) => patched(view, data, ops);

  // A command joins: { item(it, args) | patch(draft, args, op), send(args, op) -> [command, its arguments] |
  // run(op, invoke) -> its answer, what(args) -> "the tag “reef”" for the message, views (all it may touch, when not
  // only its items'), problems(answer) -> what couldn't be done (strings), made(args) -> ["view:id"] it makes }
  W.define = (name, kind) => { kinds[name] = kind; };

  // A change made: shown at once, written when its turn comes. `items`: the items it's for, as shown ({ kind, id,
  // name }); a promise of the change once it's settled ({ ok: written, answer: the command's, args })
  W.run = (name, items, args, extra) => {
    const k = kinds[name];
    const targets = (items || []).filter((it) => VIEW_OF[it.kind]).map((it) => ({ view: VIEW_OF[it.kind], id: it.id, name: it.name || it.id }));
    const views = Array.from(new Set((k.views || []).concat(targets.map((t) => t.view))));
    if (!views.length) return Promise.resolve({ ok: true, args });
    const op = Object.assign({ kind: name, args, targets, views, state: "queued", ok: false, until: new Set(), needs: new Set(), made: k.made ? k.made(args) : [] }, extra || {});
    // (a kind that learns more of its change while it's written shows it again: op.show)
    op.show = () => { if (ops.includes(op)) W.host.show(op.views); };
    const done = new Promise((r) => { op.done = () => r(op); });
    ops.push(op);
    W.host.show(views);
    tell();
    pump();
    return done;
  };

  function failed(op, why) {
    ops = ops.filter((o) => o !== op);
    op.state = "failed";
    const k = kinds[op.kind];
    W.host.failed({ what: k.what ? k.what(op.args, op) : op.kind, names: op.targets.map((t) => t.name), why: String(why) });
    op.done();
    // What was made by it (a new name) and changed since: can't be done either
    if (op.made.length) for (const o of ops.slice()) {
      if (o.state === "queued" && o.targets.some((t) => op.made.includes(t.view + ":" + t.id))) failed(o, "it follows a change that wasn't saved");
    }
  }

  async function pump() {
    if (busy) return;
    const op = ops.find((o) => o.state === "queued");
    if (!op) return;
    busy = true;
    op.state = "writing";
    const k = kinds[op.kind];
    let answer, error = null;
    try {
      if (k.run) answer = await k.run(op, W.host.invoke);
      else { const [cmd, a] = k.send(op.args, op); answer = await W.host.invoke(cmd, a); }
    } catch (e) { error = e; }
    busy = false;
    op.ended = ++seq;
    if (error !== null) {
      // Not written: what shows goes back to what the disk has (with the later changes), and the disk is read again
      // (a command that failed halfway)
      failed(op, error);
      W.host.show(op.views);
      W.host.reload(op.views);
    } else {
      op.answer = answer;
      const problems = (k.problems ? k.problems(answer, op) : []) || [];
      if (problems.length) {
        // Written in part, which part not said: shown as changed until the views are read again, then as the disk has it
        op.state = "unsure";
        op.needs = new Set(op.views);
        const kk = kinds[op.kind];
        W.host.failed({ what: kk.what ? kk.what(op.args, op) : op.kind, names: op.targets.map((t) => t.name), why: problems.join(" · "), partly: true });
        op.done();
        W.host.reload(op.views);
      } else {
        land(op);
        op.ok = true;
        op.done();
      }
    }
    tell();
    pump();
  }

  // Written: part of what the disk has from now on; kept over the readings that started before it was
  function land(op) {
    op.state = "landed";
    for (const v of op.views) { const data = W.host.get(v); if (data) W.host.set(v, patched(v, data, [op])); }
    op.until = new Set(Array.from(readings).filter((r) => op.views.includes(r.view)));
    if (!op.until.size) ops = ops.filter((o) => o !== op);
  }

  // A view being read from the disk (the host says so before it asks): what `read` is given back with the answer
  W.reading = (view) => { const r = { view, at: ++seq }; readings.add(r); return r; };
  // …and its answer: with what was written since the reading started put into it, to be kept as what the disk has
  W.read = (r, data) => {
    readings.delete(r);
    for (const op of ops.slice()) {
      if (op.state === "landed" && op.until.has(r)) {
        if (data) data = patched(r.view, data, [op]);
        op.until.delete(r);
        if (!op.until.size) ops = ops.filter((o) => o !== op);
      } else if (op.state === "unsure" && data && r.at > op.ended && op.needs.has(r.view)) {
        op.needs.delete(r.view);
        if (!op.needs.size) ops = ops.filter((o) => o !== op);
      }
    }
    return data;
  };

  // How many changes are still to be written
  W.pending = () => ops.filter(active).length;
  // Whether an item (as shown) has a change still to be written
  W.waiting = (view, id) => ops.some((o) => active(o) && o.targets.some((t) => t.view === view && t.id === id));
  // Resolved once nothing is waiting to be written
  W.drained = () => (W.pending() ? new Promise((r) => waiters.push(r)) : Promise.resolve());
  W.onChange = (f) => { listeners.push(f); };
  // Another project opened: the change being written goes on (the app has it), the ones still waiting can't be
  // written any more (they were the other project's) and are said so; nothing of them is shown over the new one
  W.reset = () => {
    for (const o of ops.slice()) {
      if (o.state === "queued") failed(o, "another project was opened before it was written");
      else if (o.state !== "writing") o.done();
    }
    ops = ops.filter((o) => o.state === "writing");
    for (const o of ops) { o.views = []; o.made = []; }
    readings.clear();
    tell();
  };

  return W;
})();
