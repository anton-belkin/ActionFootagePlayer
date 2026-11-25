"use strict";
// The app's changing commands, stood in for on the views a test has (page.js): each changes the views as the disk
// would change (src/tags.rs, src/geo.rs, src/items.rs's rename_in and delete_in), so that a view read afterwards shows
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
  const app = { views, fail: {}, failItems: null, page: null, done: [], log: [], changed: !!(o && o.changed),
    // (`o.albums: false`: the album commands answer as before they said what they made, for measuring the older page)
    albums: !(o && o.albums === false) && !!(o && o.changed) };
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
  // A clip's link in an album gone (the scan's `links`): the clip's `preview` is then its next link's, or none
  const linkGone = (path) => {
    const f = views.footage, l = (f.links || []).find((x) => x.path === path);
    if (!l) return null;
    f.links = f.links.filter((x) => x !== l);
    const it = f.items.find((c) => c.id === l.clip);
    if (it && it.preview === path) {
      const next = f.links.find((x) => x.clip === it.id);
      it.preview = next ? next.path : null; it.pdir = next ? parent(next.rel) : null; delete it.draft;
    }
    return it || null;
  };
  // Accept / Reject and the stars (src/tags.rs), as set_tags: on originals, moments and photos
  const cull = (cmd, a, set) => {
    refuse(cmd, a);
    const problems = [];
    for (const x of a.items) {
      const it = find(x.kind, x.id);
      if (!it) { if (VIEW_OF[x.kind]) problems.push(x.id + " isn't in the project's locations"); continue; }
      const why = bad(cmd, x.id);
      if (why) { problems.push(why); continue; }
      set(it);
      if (it.kind === "clip") ownOf(it);
    }
    app.done.push([cmd, a]);
    const what = a.items.filter((x) => VIEW_OF[x.kind]).map((x) => [VIEW_OF[x.kind], x.id]);
    told(what);
    return answer(problems, what);
  };
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
    set_pick(a) { return cull("set_pick", a, (it) => { if (a.pick) it.pick = a.pick; else delete it.pick; }); },
    set_rating(a) { return cull("set_rating", a, (it) => { if (a.rating) it.rating = a.rating; else delete it.rating; }); },
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
      // (a moment, photo or export renamed from Albums: by its kind; a folder: by its path)
      const v = views[VIEW_OF[a.kind]] || views[a.view] || views.footage;
      const root = (views.footage || v).root;
      const rel = a.path.slice(root.length + 1);
      if (/[\/\\:*?"<>|]/.test(a.name)) throw "A name can't have / \\ : * ? \" < > |";
      if (a.kind === "folder") {
        // A folder of the project's, by its path (src/items.rs's rename_in: whatever the view asked from): what's in
        // it follows in every view. Footage: the clips' links in it (and the clips whose preview is one of them)
        const to = (parent(rel) ? parent(rel) + "/" : "") + a.name;
        const taken = (x) => x && ((x.dirs || []).includes(to) || (x.clips_dirs || []).includes(to));
        if (ALL.some((n) => taken(views[n]))) throw root + "/" + to + " exists already";
        const move = (p) => (p === rel ? to : p.startsWith(rel + "/") ? to + p.slice(rel.length) : p);
        const inside = (p) => p != null && (p === rel || p.startsWith(rel + "/"));
        for (const n of ALL) {
          const x = views[n];
          if (!x) continue;
          if (n === "footage") {
            x.clips_dirs = (x.clips_dirs || []).map(move).sort();
            for (const l of x.links || []) if (inside(parent(l.rel))) { l.rel = move(parent(l.rel)) + "/" + l.rel.split("/").pop(); l.path = root + "/" + l.rel; }
            for (const it of x.items) if (it.preview && inside(it.pdir)) { it.pdir = move(it.pdir); it.preview = root + "/" + it.pdir + "/" + it.preview.split("/").pop(); }
          } else {
            x.empty = (x.empty || []).map(move);
            let any = (x.dirs || []).some(inside);
            for (const it of x.items) if (inside(it.dir)) { it.dir = move(it.dir); it.id = it.dir + "/" + it.id.split("/").pop(); it.path = root + "/" + it.id; any = true; }
            if (any) { x.dirs = Array.from(new Set((x.dirs || []).map(move))); redirs(x); x.items.sort((p, q) => (p.id < q.id ? -1 : p.id > q.id ? 1 : 0)); }
          }
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
      let folders = false, links = false;
      for (const x of a.items) {
        const why = bad("delete_items", x.path);
        if (why) { problems.push(why); continue; }
        if (x.kind === "preview") {
          // A clip's link in an album: the link goes, the clip stays (in Originals, and in its other albums)
          const it = linkGone(x.path);
          if (it) { done++; what.push(["footage", it.id]); files.push(x.path); links = true; }
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
          // …and the clips' links in it
          const f = views.footage, frel = x.path.slice(f.root.length + 1);
          for (const l of (f.links || []).slice()) if (parent(l.rel) === frel || parent(l.rel).startsWith(frel + "/")) linkGone(l.path);
          f.clips_dirs = (f.clips_dirs || []).filter((d) => !(d === frel || d.startsWith(frel + "/")));
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
      // (which links an album has left isn't said item by item: Footage is read again, docs/archive/night-handover-responsive.md)
      return answer({ done, to: done ? ["the Trash"] : [], problems }, folders ? [] : what, folders ? { reload: ALL } : links ? { reload: ["footage"] } : null);
    },
  };
  // ---------- albums (src/albums.rs) ----------
  // The names a folder of the project's folder has (every view's files there), lower-cased
  const namesIn = (dir) => {
    const out = new Set();
    for (const l of views.footage.links || []) if (parent(l.rel) === dir) out.add(l.rel.split("/").pop().toLowerCase());
    for (const n of ["moments", "stills", "exports"]) for (const it of (views[n] || { items: [] }).items) if (it.dir === dir) out.add(it.id.split("/").pop().toLowerCase());
    return out;
  };
  const free = (names, stem, ext) => { let name = stem + ext, n = 2; while (names.has(name.toLowerCase())) name = stem + " " + n++ + ext; names.add(name.toLowerCase()); return name; };
  const parts = (file) => { const l = file.toLowerCase(); const n = l.endsWith(".afplink.mp4") || l.endsWith(".afplink.jpg") ? 12 : l.endsWith(".mp.jpg") ? 7 : file.length - file.lastIndexOf("."); return [file.slice(0, file.length - n), file.slice(file.length - n)]; };
  const rootDir = () => views.footage.root;
  const listDir = (dir) => { const f = views.footage; f.clips_dirs = f.clips_dirs || []; for (let d = dir; d && !f.clips_dirs.includes(d); d = parent(d)) f.clips_dirs.push(d); f.clips_dirs.sort(); };
  const linkOf = (rel, clip) => ({ path: rootDir() + "/" + rel, rel, clip, draft: false, size: 20480, mtime: 1763300000 });
  // What an album command says it made (src/changed.rs's of_album), or its old answer; the catalog told of its own
  // files: the app's notice of them asks for nothing now (seen.rs's ours), before it made Footage be read again
  const albumAnswer = (plain, what, links, linksGone) => {
    told(app.albums ? [] : null);
    return app.albums ? changedOf(what, { answer: plain, problems: plain.problems || (plain.failed || []).map((f) => f[0] + ": " + f[1]), links: links || [], links_gone: linksGone || [] }) : plain;
  };
  app.commands.album_add = (a) => {
    refuse("album_add", a);
    const f = views.footage, names = namesIn(a.album);
    const made = [], links = [], what = [];
    let already = 0;
    for (const c of a.clips) {
      if ((f.links || []).some((l) => l.clip === c.tree && parent(l.rel) === a.album)) { already++; continue; }
      const rel = a.album + "/" + free(names, c.tree.split("/").pop().replace(/\.[^.]+$/, ""), ".afplink.mp4");
      const l = linkOf(rel, c.tree);
      (f.links = f.links || []).push(l);
      links.push(l); made.push(l.path); what.push(["footage", c.tree]);
      const it = f.items.find((x) => x.id === c.tree);
      if (it && !it.preview) { it.preview = l.path; it.pdir = a.album; }
    }
    if (made.length) listDir(a.album);
    app.done.push(["album_add", a]);
    return albumAnswer({ made, already, album: a.album, failed: [], ms: 5 * made.length }, what, links);
  };
  app.commands.album_copy = (a) => {
    refuse("album_copy", a);
    const names = namesIn(a.album), what = [];
    let copied = 0;
    for (const file of a.files) for (const n of ["moments", "stills"]) {
      const v = views[n], it = v && v.items.find((x) => x.path === file);
      if (!it || it.dir === a.album) continue;
      const [stem, ext] = parts(file.split("/").pop());
      const id = a.album + "/" + free(names, stem, ext);
      const c = Object.assign(copy(it), { id, dir: a.album, path: rootDir() + "/" + id, name: it.kind === "moment" ? parts(id.split("/").pop())[0] : id.split("/").pop() });
      v.items.push(c); v.items.sort((x, y) => (x.id < y.id ? -1 : x.id > y.id ? 1 : 0)); redirs(v);
      what.push([n, id]); copied++;
    }
    app.done.push(["album_copy", a]);
    return albumAnswer({ copied, album: a.album, problems: [] }, what);
  };
  app.commands.album_move = (a) => {
    refuse("album_move", a);
    const f = views.footage, names = namesIn(a.album), what = [], links = [], gone = [];
    let moved = 0;
    for (const file of a.files) {
      const l = (f.links || []).find((x) => x.path === file);
      const [stem, ext] = parts(file.split("/").pop());
      if (l) {
        if (parent(l.rel) === a.album) continue;
        const rel = a.album + "/" + free(names, stem, ext);
        l.rel = rel; l.path = rootDir() + "/" + rel;
        const it = f.items.find((x) => x.id === l.clip);
        if (it && it.preview === file) { it.preview = l.path; it.pdir = a.album; }
        listDir(a.album);
        links.push(copy(l)); gone.push(file); what.push(["footage", l.clip]); moved++;
        continue;
      }
      for (const n of ["moments", "stills"]) {
        const v = views[n], it = v && v.items.find((x) => x.path === file);
        if (!it || it.dir === a.album) continue;
        const was = it.id;
        it.id = a.album + "/" + free(names, stem, ext); it.dir = a.album; it.path = rootDir() + "/" + it.id;
        it.name = it.kind === "moment" ? parts(it.id.split("/").pop())[0] : it.id.split("/").pop();
        v.items.sort((x, y) => (x.id < y.id ? -1 : x.id > y.id ? 1 : 0)); redirs(v);
        what.push([n, was], [n, it.id]); moved++;
      }
    }
    app.done.push(["album_move", a]);
    return albumAnswer({ moved, album: a.album, problems: [] }, what, links, gone);
  };
  app.commands.photo_add = (a) => {
    refuse("photo_add", a);
    const v = views.stills, names = namesIn(a.album), made = [], what = [];
    let already = 0;
    for (const ph of a.photos) {
      if (v.items.some((x) => x.dir === a.album && x.link && x.link.clip === ph.tree && Math.abs((+x.link.t || 0) - ph.t) < 0.05)) { already++; continue; }
      const id = a.album + "/" + free(names, ph.tree.split("/").pop().replace(/\.[^.]+$/, "") + "_" + ph.t.toFixed(2) + "s", ".afplink.jpg");
      v.items.push({ kind: "still", id, dir: a.album, name: id.split("/").pop(), path: rootDir() + "/" + id, size: 30000, mtime: 1763300000, tags: [], linked: true, form: "Photo link",
        link: { v: 1, kind: "photo", clip: ph.tree, t: Math.round(ph.t * 1000) / 1000, size: ph.size || 0 } });
      made.push(rootDir() + "/" + id); what.push(["stills", id]);
    }
    v.items.sort((x, y) => (x.id < y.id ? -1 : x.id > y.id ? 1 : 0)); redirs(v);
    app.done.push(["photo_add", a]);
    return albumAnswer({ made, already, album: a.album, failed: [], ms: 180 * made.length }, what);
  };
  app.commands.new_folder = (a) => {
    refuse("new_folder", a);
    const dir = (n) => (a.parent ? a.parent + "/" : "") + n;
    const taken = (n) => ALL.some((v) => views[v] && ((views[v].dirs || []).includes(dir(n)) || (views[v].clips_dirs || []).includes(dir(n)))) || namesIn(a.parent).has(n.toLowerCase());
    let name = "untitled folder", k = 1;
    while (taken(name)) name = "untitled folder " + ++k;
    if (a.name && !taken(a.name)) name = a.name;
    if (a.view === "footage") listDir(dir(name)); else { const v = views[a.view]; v.empty = (v.empty || []).concat([dir(name)]); redirs(v); }
    app.done.push(["new_folder", a]);
    told(null);
    return dir(name);
  };
  // An album's folders renamed as one (src/items.rs's rename_album): the ones done named back when one can't be
  app.commands.rename_album = (a) => {
    refuse("rename_album", a);
    const done = [];
    try {
      for (const p of a.paths) { app.commands.rename_item({ view: "footage", kind: "folder", path: p, name: a.name }); done.push(p); }
    } catch (e) {
      for (const p of done.reverse()) app.commands.rename_item({ view: "footage", kind: "folder", path: parent(p) + "/" + a.name, name: p.split("/").pop() });
      throw e;
    }
    const to = a.paths.map((p) => parent(p) + "/" + a.name);
    return app.changed ? changedOf([], { answer: to, reload: ALL }) : to;
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
