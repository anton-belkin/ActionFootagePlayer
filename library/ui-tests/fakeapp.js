"use strict";
// The app's changing commands, stood in for on the views a test has (page.js): each changes the views as the disk
// would change (src/tags.rs, src/geo.rs, src/main.rs's rename_in and delete_in), so that a view read afterwards shows
// it, and tells the page what the catalog tells it (`catalog-changed`). A test makes one fail (`fail`) or slow (page.js's
// `delay`).
//
//   const app = fake(views);  const page = await open({ views, commands: app.commands });  app.attach(page);
//   app.fail.set_tags = (args) => "the share is gone";     // every item fails: the command's own error
//   app.failItems = (cmd, id) => id.endsWith("B.MP4") && "B.MP4: read-only";   // some items fail: its problems

const VIEW_OF = { clip: "footage", moment: "moments", still: "stills", export: "exports" };

// `o.changed`: the commands answer as the app does since the pinpoint update (src/changed.rs's Changed: the items as
// they are now), and `changes_since` says which items the catalog's changes were; without it they answer as before
// (lists of problems), and the page reads its views again
function fake(views, o) {
  const app = { views, fail: {}, failItems: null, page: null, done: [], log: [], changed: !!(o && o.changed) };
  const find = (kind, id) => (views[VIEW_OF[kind]] || { items: [] }).items.find((it) => it.id === id);
  const copy = (v) => JSON.parse(JSON.stringify(v));
  // What a change was, for changes_since: [[view, id]…] (null: can't be said), and the files that went
  const told = (what, goneFiles) => {
    if (!app.page) return;
    app.log.push({ n: app.page.state.changes + 1, what: what === undefined ? null : what, gone: goneFiles || [] });
    app.page.changed();
  };
  // A Changed: these items as the views have them now (the ones not there: gone)
  const changedOf = (what, more) => {
    const c = Object.assign({ items: {}, gone: {}, gone_files: [], reload: [], probes: { version: 1, items: {}, pending: 0 }, problems: [], answer: null }, more || {});
    for (const [view, id] of what || []) {
      const it = (views[view] || { items: [] }).items.find((x) => x.id === id);
      const into = it ? c.items : c.gone;
      if (!(into[view] || (into[view] = [])).some((x) => (it ? x.id : x) === id)) into[view].push(it ? copy(it) : id);
    }
    return c;
  };
  const ALL = ["footage", "moments", "stills", "exports"];
  // As the command answers: the old way (`plain`), or what it changed
  const answer = (plain, what, more) => (app.changed ? changedOf(what, Object.assign({ answer: plain, problems: Array.isArray(plain) ? plain : (plain && plain.problems) || [] }, more || {})) : plain);
  // A change made elsewhere (the player, another computer), after the test changed `views` as the disk changed:
  // the catalog tells the page; `what` as for `told`
  app.elsewhere = (what, goneFiles) => told(what, goneFiles);
  const refuse = (cmd, args) => { const f = app.fail[cmd]; const why = f && f(args); if (why) throw why; };
  const bad = (cmd, id) => (app.failItems && app.failItems(cmd, id)) || null;
  const ownOf = (it) => { if (!it.own) it.own = { marks: null, view: null, source: null, location: null }; return it.own; };
  const tagsChanged = (old, add, remove) => {
    const list = (old || []).slice();
    for (let a of add) { a = a.trim(); if (a && !list.some((x) => x.toLowerCase() === a.toLowerCase())) list.push(a); }
    return list.filter((x) => !remove.some((r) => r.toLowerCase() === x.toLowerCase()));
  };
  const parent = (p) => (p.lastIndexOf("/") < 0 ? "" : p.slice(0, p.lastIndexOf("/")));
  // The folders of a view of the project's own files, from its items (and the ones made empty)
  const redirs = (v) => {
    const set = new Set(v.empty || []);
    for (const it of v.items) { let d = it.dir; while (d && !set.has(d)) { set.add(d); d = parent(d); } }
    v.dirs = Array.from(set).sort();
  };
  app.commands = {
    set_tags(a) {
      refuse("set_tags", a);
      const problems = [];
      for (const x of a.items) {
        const it = find(x.kind, x.id);
        if (!it) { if (VIEW_OF[x.kind]) problems.push(x.id + " isn't in the project's locations"); continue; }
        const why = bad("set_tags", x.id);
        if (why) { problems.push(why); continue; }
        it.tags = tagsChanged(it.tags, a.add, a.remove);
        if (it.kind === "moment" || it.kind === "export") { if (it.tags.length) it.link.tags = it.tags.slice(); else delete it.link.tags; }
        if (it.kind === "clip") ownOf(it);
      }
      app.done.push(["set_tags", a]);
      const what = a.items.filter((x) => VIEW_OF[x.kind]).map((x) => [VIEW_OF[x.kind], x.id]);
      told(what);
      return answer(problems, what);
    },
    set_locations(a) {
      refuse("set_locations", a);
      const problems = [];
      for (const e of a.entries) for (const id of e.ids) {
        const it = find("clip", id);
        if (!it) { problems.push(id + " isn't in the project's locations"); continue; }
        const why = bad("set_locations", id);
        if (why) { problems.push(why); continue; }
        if (e.remove) { if (it.own) it.own.location = null; continue; }
        const loc = { lat: Math.round(e.lat * 1e6) / 1e6, lon: Math.round(e.lon * 1e6) / 1e6, source: e.source || "manual", set: "2026-11-15T22:00:00Z" };
        const p = Object.assign({}, e.place || {});
        if (e.site && e.site.trim()) p.site = e.site.trim();
        if (Object.keys(p).length) loc.place = p;
        if (e.photo_gap_s != null) loc.photo_gap_s = Math.round(e.photo_gap_s);
        ownOf(it).location = loc;
      }
      app.done.push(["set_locations", a]);
      const what = a.entries.flatMap((e) => e.ids.map((id) => ["footage", id]));
      told(what);
      return answer(problems, what);
    },
    parse_location(a) {
      const m = /^\s*(-?\d+(?:\.\d+)?)\s*,\s*(-?\d+(?:\.\d+)?)\s*$/.exec(a.text || "");
      if (!m) throw "Give it as latitude, longitude: 27.7333, 34.25";
      return { lat: +m[1], lon: +m[2], place: { country: "Egypt", region: "South Sinai" }, place_text: "South Sinai, Egypt" };
    },
    set_source(a) {
      refuse("set_source", a);
      const problems = [];
      for (const e of a.entries) {
        const it = find("clip", e.clip);
        if (!it) { problems.push(e.clip + " isn't in the project's locations"); continue; }
        const why = bad("set_source", e.clip);
        if (why) { problems.push(why); continue; }
        const own = ownOf(it);
        const src = Object.assign({}, own.source || {});
        for (const k of Object.keys(e.source)) { if (e.source[k] === null) delete src[k]; else src[k] = e.source[k]; }
        own.source = Object.keys(src).length ? src : null;
      }
      app.done.push(["set_source", a]);
      const what = a.entries.map((e) => ["footage", e.clip]);
      told(what);
      return answer(problems, what);
    },
    rename_item(a) {
      refuse("rename_item", a);
      const v = views[a.view];
      const root = (views.footage || v).root;
      const rel = a.path.slice(root.length + 1);
      if (/[\/\\:*?"<>|]/.test(a.name)) throw "A name can't have / \\ : * ? \" < > |";
      if (a.kind === "folder") {
        const to = (parent(rel) ? parent(rel) + "/" : "") + a.name;
        if (v.dirs.includes(to)) throw root + "/" + to + " exists already";
        const move = (p) => (p === rel ? to : p.startsWith(rel + "/") ? to + p.slice(rel.length) : p);
        if (a.view === "footage") {
          // Footage's folders are its previews': the clips whose preview is in it follow
          v.clips_dirs = v.clips_dirs.map(move).sort();
          for (const it of v.items) if (it.preview && it.pdir != null && (it.pdir === rel || it.pdir.startsWith(rel + "/"))) { it.pdir = move(it.pdir); it.preview = root + "/" + it.pdir + "/" + it.preview.split("/").pop(); }
        } else {
          v.empty = (v.empty || []).map(move);
          for (const it of v.items) if (it.dir === rel || it.dir.startsWith(rel + "/")) { it.dir = move(it.dir); it.id = it.dir + "/" + it.id.split("/").pop(); it.path = root + "/" + it.id; }
          redirs(v);
          v.items.sort((x, y) => (x.id < y.id ? -1 : x.id > y.id ? 1 : 0));
        }
        app.done.push(["rename_item", a]);
        told(null);
        return answer(root + "/" + to, [], { reload: ALL });
      }
      const it = v.items.find((x) => x.path === a.path);
      if (!it) throw a.path + ": No such file or directory";
      const file = it.id.split("/").pop();
      const ext = /\.afplink\.mp4$/i.test(file) ? file.slice(-12) : /\.mp\.jpg$/i.test(file) ? file.slice(-7) : file.lastIndexOf(".") > 0 ? file.slice(file.lastIndexOf(".")) : "";
      const id = (it.dir ? it.dir + "/" : "") + a.name + ext;
      if (v.items.some((x) => x.id === id)) throw root + "/" + id + " exists already";
      const was = it.id;
      it.id = id; it.path = root + "/" + id; it.name = a.kind === "moment" ? a.name : a.name + ext;
      v.items.sort((x, y) => (x.id < y.id ? -1 : x.id > y.id ? 1 : 0));
      app.done.push(["rename_item", a]);
      const what = [[a.view, was], [a.view, id]];
      told(what);
      return answer(it.path, what);
    },
    delete_items(a) {
      refuse("delete_items", a);
      const problems = [];
      let done = 0;
      const what = [], files = [];
      let folders = false;
      for (const x of a.items) {
        const why = bad("delete_items", x.path);
        if (why) { problems.push(why); continue; }
        if (x.kind === "preview") {
          const it = views.footage.items.find((c) => c.preview === x.path);
          if (it) { it.preview = null; it.pdir = null; delete it.draft; done++; what.push(["footage", it.id]); files.push(x.path); }
          continue;
        }
        if (x.kind === "folder") {
          for (const name of ["moments", "stills", "exports"]) {
            const v = views[name]; if (!v) continue;
            const rel = x.path.slice(v.root.length + 1);
            const n = v.items.length;
            v.items = v.items.filter((it) => !(it.dir === rel || it.dir.startsWith(rel + "/")));
            v.empty = (v.empty || []).filter((d) => !(d === rel || d.startsWith(rel + "/")));
            if (n !== v.items.length || v.dirs.includes(rel)) { redirs(v); }
          }
          done++;
          folders = true;
          continue;
        }
        const v = views[VIEW_OF[x.kind]];
        const i = v ? v.items.findIndex((it) => it.path === x.path) : -1;
        if (v && x.path.startsWith(v.root + "/")) what.push([VIEW_OF[x.kind], x.path.slice(v.root.length + 1)]);
        if (i < 0) continue;
        v.items.splice(i, 1);
        files.push(x.path);
        done++;
      }
      app.done.push(["delete_items", a]);
      told(folders ? null : what, files);
      return answer({ done, to: done ? ["the Trash"] : [], problems }, folders ? [] : what, folders ? { reload: ALL } : null);
    },
  };
  // What changed since the page's count, as items; null when a change among them can't be said item by item
  app.commands.changes_since = (a) => {
    const upto = app.page ? app.page.state.changes : 0;
    if (!app.changed) return null;
    const list = app.log.filter((e) => e.n > a.since && e.n <= upto);
    if (list.length !== upto - a.since || list.some((e) => !e.what)) return { upto, changed: null };
    return { upto, changed: changedOf(list.flatMap((e) => e.what), { gone_files: list.flatMap((e) => e.gone) }) };
  };
  app.commands.view_items = (a) => changedOf(a.ids.map((id) => [a.view, id]));
  app.attach = (page) => { app.page = page; return app; };
  return app;
}

module.exports = { fake, VIEW_OF };
