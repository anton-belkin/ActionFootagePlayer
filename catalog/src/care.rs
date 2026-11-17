// SPDX-License-Identifier: GPL-3.0-or-later
//
// Who has looked after each of the project's roots, and when (docs/catalog-safeguards.md): so that a file changed in
// place by something not ours (a script, an editor: its folder's time stays as it was) is seen within a day, by
// whoever is about, and nobody lists every folder again when somebody already has.
//
// .afp/catalog/care/<computer>/<root's key>.deep   a line appended at each deep look of that root (every folder
//                                                  listed): its time is when
// .afp/catalog/care/<computer>/<root's key>.watch  the NAS's watcher, told by the system of every change there:
//                                                  made new right after its deep look at its start, a line appended
//                                                  every few minutes while it watches
//
// A root is looked after when some computer looked at it deep within a day, or watches it now (its .watch written to
// within BEAT_FRESH: it was made after a deep look, and the watcher hasn't stopped since). The times compared are the
// share's own, all of them (the files' times, and a file made there now): no computer's clock against another's.

use crate::catalog::{ Catalog, Root };
use std::path::PathBuf;

/// A deep look counts for this long
pub const DAY_NS: i64 = 86_400 * 1_000_000_000;
/// A watcher counts while its last line is this young (it writes one every BEAT_EVERY)
pub const BEAT_FRESH_NS: i64 = 15 * 60 * 1_000_000_000;
pub const BEAT_EVERY: std::time::Duration = std::time::Duration::from_secs(5 * 60);
/// A watcher's file started again once it has grown to this (a line every 5 minutes: ~9 KB a day)
const WATCH_MAX: u64 = 64 * 1024;

/// A root's name in these files: the same on every computer (its id is its location's path in the project's file)
pub fn key(root: &str) -> String { crate::snapshot::sha(root.as_bytes())[..16].to_string() }

fn dir(cat: &Catalog) -> PathBuf { cat.dir.join("care") }
fn mine(cat: &Catalog) -> PathBuf { dir(cat).join(&cat.me) }

/// The share's "now", in its own clock: a file made there, its time read, removed. None: it couldn't be written
pub fn share_now(cat: &Catalog) -> Option<i64> {
    let d = mine(cat);
    cat.shared.mkdirs(&d).ok()?;
    let p = crate::store::temp_name(&d.join("now"));
    cat.shared.append(&p, b"").ok()?;
    let t = cat.shared.stat(&p).ok().flatten().map(|s| s.mtime);
    let _ = cat.shared.remove(&p);
    t
}

fn line(root: &Root) -> Vec<u8> { format!("{}\n", serde_json::json!({ "root": root.id })).into_bytes() }

/// These roots were just looked at deep (every folder listed): said for every computer
pub fn mark_deep(cat: &Catalog, roots: &[Root]) {
    let d = mine(cat);
    if cat.shared.mkdirs(&d).is_err() { return; }
    for r in roots { let _ = cat.shared.append(&d.join(format!("{}.deep", key(&r.id))), &line(r)); }
}

/// The watcher watches these roots: `start` right after its deep look (its files made new: what was before it isn't
/// vouched for), else a line every BEAT_EVERY while it goes on
pub fn watching(cat: &Catalog, roots: &[Root], start: bool) {
    let d = mine(cat);
    if cat.shared.mkdirs(&d).is_err() { return; }
    for r in roots {
        let p = d.join(format!("{}.watch", key(&r.id)));
        let big = matches!(cat.shared.stat(&p), Ok(Some(s)) if s.size >= WATCH_MAX);
        if start || big { let _ = cat.shared.remove(&p); }
        let _ = cat.shared.append(&p, &line(r));
    }
}

/// The watcher stops watching (it's told to stop, or its roots changed): its files removed, so nobody counts on it
pub fn stop_watching(cat: &Catalog, roots: &[Root]) {
    for r in roots { let _ = cat.shared.remove(&mine(cat).join(format!("{}.watch", key(&r.id)))); }
}

/// Of these roots, the ones nobody has looked after within a day: to be looked at deep. Every one of them when the
/// share can't say (its "now" not written): looking is what's safe
pub fn uncovered(cat: &Catalog, roots: &[Root]) -> Vec<Root> {
    let Some(now) = share_now(cat) else { return roots.to_vec() };
    let mut deep: std::collections::HashMap<String, i64> = Default::default();
    let mut watch: std::collections::HashMap<String, i64> = Default::default();
    for c in cat.shared.list(&dir(cat)).unwrap_or_default().into_iter().filter(|c| c.stat.dir) {
        for f in cat.shared.list(&dir(cat).join(&c.name)).unwrap_or_default() {
            let Some((k, kind)) = f.name.rsplit_once('.') else { continue };
            let m = match kind { "deep" => &mut deep, "watch" => &mut watch, _ => continue };
            let t = m.entry(k.to_string()).or_insert(i64::MIN);
            *t = (*t).max(f.stat.mtime);
        }
    }
    roots.iter().filter(|r| {
        let k = key(&r.id);
        let looked = deep.get(&k).is_some_and(|t| now - t < DAY_NS);
        let watched = watch.get(&k).is_some_and(|t| now - t < BEAT_FRESH_NS);
        !looked && !watched
    }).cloned().collect()
}

/// What a computer that keeps the catalog does at its start and then every hour or so (the Library; afp-catalog look
/// --care): the roots nobody has looked after within a day looked at deep, and said. None: all of them were looked
/// after (nothing listed)
/// (`starting`: told how many roots are to be looked at, before the look: for a status line)
pub fn look_after(cat: &mut Catalog, roots: &[Root], insp: &dyn crate::Inspect, pause: Option<std::time::Duration>, starting: &dyn Fn(usize)) -> Result<Option<crate::Report>, crate::Error> {
    let todo = uncovered(cat, roots);
    if todo.is_empty() { return Ok(None); }
    starting(todo.len());
    let rep = cat.sweep(&todo, insp, &crate::Sweep { deep: true, pause, ..Default::default() })?;
    let looked: Vec<Root> = todo.into_iter().filter(|r| !rep.skipped_roots.contains(&r.id)).collect();
    mark_deep(cat, &looked);
    Ok(Some(rep))
}
