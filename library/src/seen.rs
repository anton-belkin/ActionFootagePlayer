// SPDX-License-Identifier: GPL-3.0-or-later
//
// The project's catalog in the Library (docs/design/library-management.md §9): what's in its folder and locations as last
// seen, shared with every computer through .afp/catalog/. The views read it instead of the disks (a file's size,
// time, link, sidecar: no question to the NAS for each); a thread keeps it up to date:
//
// - every 3 s: the journals' new lines (what the player, or another computer, wrote)
// - what this computer's system says changed (FSEvents): those folders looked at a second after it settles
// - every 5 minutes: a look at the project's own folders' times
// - at the start and every hour: the roots nobody has looked after within a day (no deep look by any computer, no
//   NAS watcher vouching for them: afp_catalog::care) looked at deep, slowly — what a script or an editor changed in
//   place, which leaves its folder's time alone
// - what the Library couldn't note at once (a share away): its folders looked at again soon, then less often
// - the journals folded into a new snapshot when they've grown, and what's no longer needed cleared
//
// It has two copies: one the views read (taken only briefly to read the journals' lines into it), one that looks and
// folds (minutes on a first look at a big project, which the views mustn't wait for).

use afp_catalog::inspect::Ours;
use afp_catalog::state::File;
use afp_catalog::{ project, Catalog, DiskStore, Error, Inspect, Root, Store, Sweep, Want };
use std::collections::{ BTreeSet, HashSet };
use std::path::{ Path, PathBuf };
use std::sync::atomic::{ AtomicBool, AtomicU64, Ordering };
use std::sync::mpsc::{ channel, Receiver, Sender };
use std::sync::{ Arc, Mutex, RwLock };
use std::time::{ Duration, Instant };

pub struct Seen {
    /// The thread that made it (a project's watch made again makes another)
    pub id: u64,
    pub root: PathBuf,
    pub roots: Vec<Root>,
    /// What's kept and read in them (the sweep's rules: note_now keeps to them too)
    pub ours: Ours,
    pub cat: Catalog,
    /// The page's count of changes, and when the last was: counted when the Library notes its own (note_now)
    pub bump: Option<(Arc<AtomicU64>, Arc<Mutex<Instant>>)>,
}

impl Seen {
    /// As a test has it: the project's roots and rules, no page to tell
    #[cfg(test)]
    pub fn of(root: &Path, cat: Catalog) -> Seen {
        let (roots, ours) = project::roots(root);
        Seen { id: 0, root: root.to_path_buf(), roots, ours, cat, bump: None }
    }
}

pub static SEEN: RwLock<Option<Seen>> = RwLock::new(None);

/// The project whose catalog is being read now (the views wait for it rather than walk the disks)
/// (with the thread's id: a thread stopped as the project is watched again doesn't end the new one's)
static STARTING: Mutex<Option<(PathBuf, u64)>> = Mutex::new(None);

/// Until the project's catalog is read (or found not to be there: then the views read the disks)
pub fn wait_ready(root: &Path, most: Duration) {
    let t = Instant::now();
    while t.elapsed() < most && STARTING.lock().unwrap().as_ref().is_some_and(|(r, _)| r == root) { std::thread::sleep(Duration::from_millis(50)); }
}

/// The catalog's thread running now (start()), and whether a view was read without its catalog since it started (none
/// yet, another's, the one of before the project was watched again): the views are read again once it's read only
/// then. Every view the page read after waiting for it (wait_ready) is as the catalog says, and a change counted
/// "can't say" there had the page read everything a second time, alongside its own first read (2026-11-24 journal)
static RUN: AtomicU64 = AtomicU64::new(0);
/// (the thread's id it's for: a thread stopped, still finishing, doesn't take the new one's)
static READ_WITHOUT: AtomicU64 = AtomicU64::new(0);

/// A view about to be read: noted if it's not from the catalog of this run (main.rs's scan, after wait_ready)
pub fn reading_view(root: &Path) {
    // Under SEEN's lock: the thread puts its catalog there under the same lock, then asks (ready_counts)
    let g = SEEN.read().unwrap_or_else(|e| e.into_inner());
    let run = RUN.load(Ordering::Relaxed);
    if !g.as_ref().is_some_and(|s| s.root == root && s.id == run) { READ_WITHOUT.store(run, Ordering::Relaxed); }
}

/// The catalog of thread `id` read: whether the views must be read again (one was read without it)
fn ready_counts(id: u64) -> bool { READ_WITHOUT.compare_exchange(id, 0, Ordering::Relaxed, Ordering::Relaxed).is_ok() }

fn started(root: &Path, id: u64, on: bool) {
    let mut g = STARTING.lock().unwrap();
    if on { *g = Some((root.to_path_buf(), id)); } else if g.as_ref().is_some_and(|(r, i)| r == root && *i == id) { *g = None; }
}

/// Tests that set SEEN, one at a time (it's the process's)
#[cfg(test)]
pub static TESTS: Mutex<()> = Mutex::new(());

/// As before the Library noted its own changes (2026-11-10 audit): the views as they were, for the audit's table
#[cfg(test)]
pub static OLD: AtomicBool = AtomicBool::new(false);
#[cfg(test)]
fn old() -> bool { OLD.load(Ordering::Relaxed) }
#[cfg(not(test))]
fn old() -> bool { false }
/// A test that has a player of its own to tell (e2e)
#[cfg(test)]
pub static TELL_PLAYERS: AtomicBool = AtomicBool::new(false);
#[cfg(test)]
fn tell_players_on() -> bool { TELL_PLAYERS.load(Ordering::Relaxed) }
#[cfg(not(test))]
fn tell_players_on() -> bool { true }

/// What the views can say from it: (root, path in it), when the catalog is read and has looked at that root
fn locate<'a>(s: &'a Seen, path: &Path) -> Option<(String, String)> {
    let (root, rel) = project::locate(&s.roots, path)?;
    s.cat.state.get(&root, "").is_some_and(|f| f.dir).then_some((root, rel))
}

/// None: the catalog can't say (not read yet, not a root it has looked at): ask the disk. Some(None): not there
pub fn file(path: &Path) -> Option<Option<File>> {
    let g = SEEN.read().ok()?;
    let s = g.as_ref()?;
    let (root, rel) = locate(s, path)?;
    Some(s.cat.state.get(&root, &rel).cloned())
}

pub fn is_file(path: &Path) -> bool {
    match file(path) { Some(f) => f.is_some_and(|f| !f.dir), None => path.is_file() }
}

/// A file's size (None: no such file), from the catalog when it has the place
pub fn size(path: &Path) -> Option<u64> {
    match file(path) {
        Some(f) => f.filter(|f| !f.dir).map(|f| f.size),
        None => std::fs::metadata(path).ok().filter(|m| m.is_file()).map(|m| m.len()),
    }
}

/// What's inside a file (its link, a sidecar's marks and tags), as last read
pub fn meta(path: &Path) -> Option<Option<serde_json::Value>> { file(path).map(|f| f.and_then(|f| f.meta)) }

pub fn seconds(ns: i64) -> i64 { ns.div_euclid(1_000_000_000) }

/// Every file and folder under `base` (as index.rs's walk: path in it, folder, size, modified seconds), folders kept
/// out by `skip` with what's in them
pub fn walk(base: &Path, skip: &dyn Fn(&str, &str) -> bool, max_depth: usize) -> Option<Vec<(String, bool, u64, i64)>> {
    let g = SEEN.read().ok()?;
    let s = g.as_ref()?;
    let (root, prefix) = locate(s, base)?;
    Some(walk_in(s, &root, &prefix, skip, max_depth))
}

/// As walk, for a location by its root's id ("l:" and its path), reachable or not: what the catalog last saw of it (a
/// share not mounted now shows as it was, offline). None: the catalog isn't read, or has never looked at it
pub fn walk_root(root: &str, skip: &dyn Fn(&str, &str) -> bool, max_depth: usize) -> Option<Vec<(String, bool, u64, i64)>> {
    let g = SEEN.read().ok()?;
    let s = g.as_ref()?;
    s.cat.state.get(root, "").is_some_and(|f| f.dir).then(|| walk_in(s, root, "", skip, max_depth))
}

/// A file as the catalog last saw it, by its root's id and path in it (a location offline). None: not known
pub fn file_in(root: &str, rel: &str) -> Option<File> {
    SEEN.read().ok()?.as_ref()?.cat.state.get(root, rel).cloned()
}

fn walk_in(s: &Seen, root: &str, prefix: &str, skip: &dyn Fn(&str, &str) -> bool, max_depth: usize) -> Vec<(String, bool, u64, i64)> {
    if !prefix.is_empty() && !s.cat.state.get(root, prefix).is_some_and(|f| f.dir) { return Vec::new(); }
    // As the disk's walk gives them: a folder's entries by name, folder after folder as they were found
    let mut kids: std::collections::HashMap<&str, Vec<(&str, &File)>> = std::collections::HashMap::new();
    for (rel, r) in s.cat.state.under(root, prefix) {
        let Some(f) = &r.f else { continue };
        let mine = if prefix.is_empty() { rel } else { &rel[prefix.len() + 1..] };
        kids.entry(afp_catalog::state::parent(mine)).or_default().push((mine, f));
    }
    // A file noted in a folder that was never listed (the player's moment in a run's new folder of Generated/, a folder
    // the Library made): the folder is there, as the catalog's own look takes it (State::children_index)
    let implied = File { dir: true, size: 0, mtime: -1, meta: None, digest: None };
    let mut dirs: HashSet<&str> = kids.values().flatten().filter(|(_, f)| f.dir).map(|(m, _)| *m).collect();
    let parents: Vec<&str> = if old() { Vec::new() } else { kids.keys().copied().collect() };
    for mut p in parents {
        while !p.is_empty() && dirs.insert(p) {
            let up = afp_catalog::state::parent(p);
            kids.entry(up).or_default().push((p, &implied));
            p = up;
        }
    }
    for v in kids.values_mut() { v.sort_by(|a, b| a.0.cmp(b.0)); }
    let mut out = Vec::new();
    let mut todo = std::collections::VecDeque::from([("", 0usize)]);
    while let Some((dir, depth)) = todo.pop_front() {
        for (mine, f) in kids.get(dir).map(|v| v.as_slice()).unwrap_or(&[]) {
            let name = mine.rsplit('/').next().unwrap_or(mine);
            if f.dir {
                if skip(mine, &name.to_lowercase()) { continue; }
                out.push((mine.to_string(), true, 0, seconds(f.mtime)));
                if depth + 1 < max_depth { todo.push_back((mine, depth + 1)); }
            } else {
                out.push((mine.to_string(), false, f.size, seconds(f.mtime)));
            }
        }
    }
    out
}

/// The names in a folder
pub fn names_in(dir: &Path) -> Option<Vec<String>> {
    let g = SEEN.read().ok()?;
    let s = g.as_ref()?;
    let (root, rel) = locate(s, dir)?;
    Some(s.cat.state.children(&root, &rel).into_iter().map(|(n, _)| n.to_string()).collect())
}

/// What the Library itself changed in the project's folder or its locations, told to the catalog at once (as the
/// player's `afp-catalog note` does), for the views, which read it, to show it when the page reads them again right
/// after: on a share the system's notice comes late or never, and a file written in place (a link, a clip's own file)
/// leaves its folder's time as it was, so the catalog's next look wouldn't even list it. `paths`: files written, made or
/// removed; folders made or removed; a folder renamed or moved by both its paths (what's in the new one listed, what was
/// in the old one gone). Their folders are touched for the other computers (each lists them again). The page's count
/// of changes goes up (it reads the views again, and the missing links are counted again)
pub fn note_now(paths: &[PathBuf]) {
    if paths.is_empty() || old() { return; }
    let (roots, ours) = {
        let Ok(g) = SEEN.read() else { return };
        let Some(s) = g.as_ref() else { return };
        (s.roots.clone(), Ours { footage_in_project: s.ours.footage_in_project.clone(), own_only: false })
    };
    // Read before the catalog is held (on a share each is a round trip or a few)
    let mut files: Vec<(String, String, PathBuf, afp_catalog::store::Stat)> = Vec::new();
    let mut entries: Vec<(String, String, Option<File>)> = Vec::new();
    let mut gone: Vec<(String, String)> = Vec::new();
    let mut touch: BTreeSet<PathBuf> = BTreeSet::new();
    let mut done: HashSet<(String, String)> = HashSet::new();
    for p in paths {
        let Some((root, rel)) = project::locate(&roots, p) else { continue };
        if rel.is_empty() || !done.insert((root.clone(), rel.clone())) { continue; }
        if let Some(d) = p.parent() { touch.insert(d.to_path_buf()); }
        match DiskStore.stat(p) {
            Ok(Some(st)) if kept(&ours, &root, &rel, st.dir) => {
                if st.dir { list_into(&ours, &root, &rel, p, &st, &mut files, &mut entries, 0); } else { files.push((root, rel, p.clone(), st)); }
            }
            Ok(Some(_)) => {}
            Ok(None) => gone.push((root, rel)),
            Err(_) => RETRY.lock().unwrap().push(p.clone()),
        }
    }
    // What's inside the files that counts (a link, a clip's own file), 8 at a time
    let next = std::sync::atomic::AtomicUsize::new(0);
    let read: Mutex<Vec<(String, String, Option<File>)>> = Mutex::new(Vec::new());
    std::thread::scope(|sc| for _ in 0..8.min(files.len()) { sc.spawn(|| loop {
        let i = next.fetch_add(1, Ordering::Relaxed);
        let Some((root, rel, path, st)) = files.get(i) else { break };
        let meta = if ours.wants(root, rel) { match ours.inspect(path, root, rel) {
            Ok(m) => m,
            Err(e) if e.kind() == std::io::ErrorKind::InvalidData => { UNREADABLE.lock().unwrap().insert(path.clone()); continue }
            Err(_) => { RETRY.lock().unwrap().push(path.clone()); continue }
        } } else { None };
        read.lock().unwrap().push((root.clone(), rel.clone(), Some(File { dir: false, size: st.size, mtime: st.mtime, meta, digest: None })));
    }); });
    entries.extend(read.into_inner().unwrap());
    for d in &touch {
        let mark = afp_catalog::store::temp_name(&d.join("touch"));
        if DiskStore.put(&mark, b"").is_ok() { let _ = DiskStore.remove(&mark); }
    }
    let Ok(mut g) = SEEN.write() else { return };
    let Some(s) = g.as_mut() else { return };
    // Gone: what was in it too (a folder)
    for (root, rel) in gone {
        for (r, rec) in s.cat.state.under(&root, &rel) { if rec.f.is_some() { entries.push((root.clone(), r.to_string(), None)); } }
        if s.cat.state.rec(&root, &rel).is_some() { entries.push((root, rel, None)); }
    }
    // The folders a new file is in, when the catalog has never seen them (made with it): to be listed at its next look
    let mut have: HashSet<(String, String)> = entries.iter().filter(|e| e.2.as_ref().is_some_and(|f| f.dir)).map(|e| (e.0.clone(), e.1.clone())).collect();
    let mut up: Vec<(String, String, Option<File>)> = Vec::new();
    for (root, rel, f) in &entries {
        if f.is_none() { continue; }
        let mut d = afp_catalog::state::parent(rel);
        while !d.is_empty() && !have.contains(&(root.clone(), d.to_string())) {
            have.insert((root.clone(), d.to_string()));
            if s.cat.state.get(root, d).is_some_and(|f| f.dir) { break; }
            up.push((root.clone(), d.to_string(), Some(File { dir: true, size: 0, mtime: -1, meta: None, digest: None })));
            d = afp_catalog::state::parent(d);
        }
    }
    entries.extend(up);
    if entries.is_empty() { return; }
    // Not written (a share away): looked at again by the thread, from the files as they are then. A newer program's
    // project isn't written to at all, again or not
    if let Err(Error::Io(_)) = s.cat.note(entries) { RETRY.lock().unwrap().extend(paths.iter().cloned()); }
    // (what the journals said meanwhile is read by note: it's in this change too)
    let more = paths_of(&s.roots, s.cat.take_seen());
    if let Some((changes, last)) = &s.bump { changed_what(changes, last, more.map(|mut m| { m.extend(paths.iter().cloned()); m })); }
    let root = s.root.clone();
    drop(g);
    // The player windows open here told at once (their clip's moments, its own file): live.rs
    let files: Vec<String> = paths.iter().map(|p| p.display().to_string()).collect();
    // (not from tests unless they ask: the player windows open on this computer aren't theirs)
    if tell_players_on() { afp_catalog::live::tell_players(&serde_json::json!({ "project": root.display().to_string(), "files": files })); }
}

/// Whether the catalog keeps it: not hidden, nor in a folder it leaves out (its own rules, the sweep's)
fn kept(ours: &Ours, root: &str, rel: &str, dir: bool) -> bool {
    let parts: Vec<&str> = rel.split('/').collect();
    (0..parts.len()).all(|i| {
        let sub = parts[..=i].join("/");
        let name = parts[i];
        !name.starts_with(".tmp-") && !ours.skip(root, &sub, name, i + 1 < parts.len() || dir)
    })
}

/// A folder the Library made or moved, and what's in it: its files to read, its folders listed (each with what was in
/// it when listed, as the catalog's own look records it: its next look needn't list them again)
#[allow(clippy::too_many_arguments)]
fn list_into(ours: &Ours, root: &str, rel: &str, path: &Path, st: &afp_catalog::store::Stat, files: &mut Vec<(String, String, PathBuf, afp_catalog::store::Stat)>,
             entries: &mut Vec<(String, String, Option<File>)>, depth: usize) {
    let Ok(items) = DiskStore.list(path) else { entries.push((root.into(), rel.into(), Some(File { dir: true, size: 0, mtime: -1, meta: None, digest: None }))); return };
    let mut now: Vec<(String, File)> = Vec::new();
    for it in items {
        let sub = afp_catalog::state::join(rel, &it.name);
        if !kept(ours, root, &sub, it.stat.dir) { continue; }
        if it.stat.dir {
            now.push((it.name.clone(), File { dir: true, size: 0, mtime: -1, meta: None, digest: None }));
            if depth < 16 { list_into(ours, root, &sub, &path.join(&it.name), &it.stat, files, entries, depth + 1); }
        } else {
            now.push((it.name.clone(), File { dir: false, size: it.stat.size, mtime: it.stat.mtime, meta: None, digest: None }));
            files.push((root.into(), sub, path.join(&it.name), it.stat));
        }
    }
    let digest = afp_catalog::state::digest(now.iter().map(|(n, f)| (n.as_str(), f)));
    entries.push((root.into(), rel.into(), Some(File { dir: true, size: 0, mtime: st.mtime, meta: None, digest: Some(digest) })));
}

/// The thread keeping the catalog up to date for one project; stopped when dropped
pub struct Runner {
    stop: Arc<AtomicBool>,
    /// A look at everything asked for, the footage too (the library mended, clips taken in, a batch done)
    pub everything: Arc<AtomicBool>,
    pub events: Sender<Vec<PathBuf>>,
    /// What it's doing, for the page ("" when nothing)
    pub status: Arc<Mutex<String>>,
    /// Links whose original isn't where they say (checks.rs's missing, from the catalog), counted on its own as the
    /// project changes; u64::MAX: not known
    pub missing: Arc<AtomicU64>,
    /// …and which they are (file, the path it names)
    pub missing_list: Arc<Mutex<Vec<serde_json::Value>>>,
    /// Asked for in Checks: every folder listed now (after a script changed files), or every file of ours read again
    /// (after a restore that kept times)
    pub deep_now: Arc<AtomicBool>,
    pub check_now: Arc<AtomicBool>,
    /// The last of those, as said in Checks: { kind, busy, changes, corrected, unreadable }
    pub care: Arc<Mutex<serde_json::Value>>,
}

impl Drop for Runner {
    fn drop(&mut self) { self.stop.store(true, Ordering::Relaxed); }
}

fn load(cat: &mut Catalog, want: &Want) -> Result<afp_catalog::Loaded, Error> {
    match cat.load(want) { Err(Error::Gap(_)) => cat.load(want), r => r }
}

/// Started for a project: `changes` counted (and `last` set) each time what it knows changed, for the page to read it
/// again
/// `moved`: the project's own files were just moved here with its catalog (Move the project…), whose records of its
/// folder are of the old one: that folder looked at again before the views read it
pub fn start(root: &Path, changes: Arc<AtomicU64>, last: Arc<Mutex<Instant>>, moved: bool) -> Runner {
    let (tx, rx) = channel();
    let stop = Arc::new(AtomicBool::new(false));
    let status = Arc::new(Mutex::new(String::new()));
    let everything = Arc::new(AtomicBool::new(false));
    let missing = Arc::new(AtomicU64::new(u64::MAX));
    let missing_list = Arc::new(Mutex::new(Vec::new()));
    let deep_now = Arc::new(AtomicBool::new(false));
    let check_now = Arc::new(AtomicBool::new(false));
    let care = Arc::new(Mutex::new(serde_json::Value::Null));
    let r = Runner { stop: stop.clone(), everything: everything.clone(), missing: missing.clone(), missing_list: missing_list.clone(), events: tx, status: status.clone(),
                     deep_now: deep_now.clone(), check_now: check_now.clone(), care: care.clone() };
    let root = root.to_path_buf();
    static IDS: AtomicU64 = AtomicU64::new(1);
    let id = IDS.fetch_add(1, Ordering::Relaxed);
    RUN.store(id, Ordering::Relaxed);
    // A catalog there to read: the views wait for it from now (not from when the thread gets to it: a view asked for
    // meanwhile would read the disks)
    if root.join(".afp").join("catalog").join("snap").is_dir() { started(&root, id, true); }
    // (another project's counts, or this one's from before it was watched again: nothing can be said of them)
    LOG.lock().unwrap().clear();
    RETRY.lock().unwrap().clear();
    UNREADABLE.lock().unwrap().clear();
    let asked = Asked { deep: deep_now, check: check_now, care };
    std::thread::Builder::new().name("catalog".into()).spawn(move || run(id, root, moved, stop, everything, missing, missing_list, rx, status, changes, last, asked)).ok();
    r
}

/// What note_now couldn't write or read (a share away): looked at again by the project's thread, soon and then less
/// often. The files themselves are read again then, never what was seen of them before (docs/design/catalog-safeguards.md)
static RETRY: Mutex<Vec<PathBuf>> = Mutex::new(Vec::new());

/// Clips' own files that aren't what they should be (not JSON: written wrong by a script): their records kept as they
/// were; read again every few minutes, and noted once they read right. For Checks
static UNREADABLE: Mutex<BTreeSet<PathBuf>> = Mutex::new(BTreeSet::new());

/// Told each time what the catalog knows changed, with the count (main.rs: the page hears it at once, an event)
pub static ON_CHANGE: std::sync::OnceLock<Box<dyn Fn(u64) + Send + Sync>> = std::sync::OnceLock::new();

/// What changed at each of the page's counts of changes, the last few hundred: the files (full paths), or None when
/// that can't be said (the catalog read whole, a first look at a location). For the page to correct only those items
/// (changed.rs) instead of reading its views again
static LOG: Mutex<std::collections::VecDeque<(u64, Option<Vec<PathBuf>>)>> = Mutex::new(std::collections::VecDeque::new());

/// The files that changed after the page's count `since`, up to `now`; None: can't be said (a count with nothing
/// logged for it: the file system's own notice with no catalog, too long ago, too many): read the views again
/// Files the window's own commands just wrote and already answered for (albums.rs: the links and photos made, the
/// files moved), each with when: the catalog's notice of them asks the window for nothing (`not_ours`). Kept a
/// minute at most, so a change from elsewhere to the same file later is seen
static OURS: Mutex<Vec<(PathBuf, Instant)>> = Mutex::new(Vec::new());
pub fn ours(paths: &[PathBuf]) {
    let mut o = OURS.lock().unwrap();
    let now = Instant::now();
    o.retain(|(_, t)| now.duration_since(*t) < Duration::from_secs(60));
    o.extend(paths.iter().map(|p| (p.clone(), now)));
}
/// The files changed without the window's own (each let go once seen: the next change to it is someone else's)
pub fn not_ours(files: Vec<PathBuf>) -> Vec<PathBuf> {
    let mut o = OURS.lock().unwrap();
    if o.is_empty() { return files; }
    let now = Instant::now();
    o.retain(|(_, t)| now.duration_since(*t) < Duration::from_secs(60));
    files.into_iter().filter(|f| match o.iter().position(|(p, _)| p == f) { Some(i) => { o.remove(i); false } None => true }).collect()
}

pub fn since(since: u64, now: u64) -> Option<Vec<PathBuf>> {
    let log = LOG.lock().unwrap();
    let mut out: Vec<PathBuf> = Vec::new();
    let mut have = 0u64;
    for (n, what) in log.iter().filter(|(n, _)| *n > since && *n <= now) {
        let _ = n;
        have += 1;
        out.extend(what.as_ref()?.iter().cloned());
    }
    if have != now.saturating_sub(since) || out.len() > 400 { return None; }
    out.sort();
    out.dedup();
    Some(out)
}

fn changed(changes: &AtomicU64, last: &Mutex<Instant>) { changed_what(changes, last, None) }

/// The catalog's records that changed, as full paths
fn paths_of(roots: &[Root], keys: Option<Vec<(String, String)>>) -> Option<Vec<PathBuf>> {
    keys?.into_iter().map(|(root, rel)| roots.iter().find(|r| r.id == root).map(|r| rel.split('/').filter(|p| !p.is_empty()).fold(r.dir.clone(), |d, p| d.join(p)))).collect()
}

fn changed_what(changes: &AtomicU64, last: &Mutex<Instant>, what: Option<Vec<PathBuf>>) {
    // Logged under the count it gets (the lock held over both: counts are logged in order)
    let mut log = LOG.lock().unwrap();
    let n = changes.fetch_add(1, Ordering::Relaxed) + 1;
    log.push_back((n, what));
    while log.len() > 300 { log.pop_front(); }
    drop(log);
    *last.lock().unwrap() = Instant::now() - Duration::from_secs(1);
    if let Some(f) = ON_CHANGE.get() { f(n); }
}

/// Another program here noted what it wrote (the player, through afp-catalog: live.rs): the journals read now, not
/// at the thread's next turn (3 s)
pub fn read_now() {
    let Ok(mut g) = SEEN.write() else { return };
    let Some(s) = g.as_mut() else { return };
    let before = s.cat.seen_changes;
    if s.cat.refresh().is_ok() && s.cat.seen_changes != before {
        let what = paths_of(&s.roots, s.cat.take_seen());
        if let Some((changes, last)) = &s.bump { changed_what(changes, last, what); }
    }
}

fn set(status: &Mutex<String>, s: &str) { *status.lock().unwrap() = s.to_string(); }

/// What Checks asks of the thread, and where it says how it went
struct Asked { deep: Arc<AtomicBool>, check: Arc<AtomicBool>, care: Arc<Mutex<serde_json::Value>> }

/// A look's files that weren't what they should be: kept for Checks (full paths)
fn unreadable_from(roots: &[Root], rep: &afp_catalog::Report) {
    if rep.unreadable.is_empty() { return; }
    if let Some(p) = paths_of(roots, Some(rep.unreadable.clone())) { UNREADABLE.lock().unwrap().extend(p); }
}

/// The files that weren't what they should be, read again: the ones that read right now noted (and off the list)
fn unreadable_again() {
    let list: Vec<PathBuf> = UNREADABLE.lock().unwrap().iter().cloned().collect();
    let right: Vec<PathBuf> = list.into_iter().filter(|p| !matches!(afp_catalog::inspect::read(p), Err(e) if e.kind() == std::io::ErrorKind::InvalidData)).collect();
    if right.is_empty() { return; }
    { let mut u = UNREADABLE.lock().unwrap(); for p in &right { u.remove(p); } }
    note_now(&right);
}

/// Clips' own files that aren't what they should be, as last seen
pub fn unreadable() -> Vec<String> { UNREADABLE.lock().unwrap().iter().map(|p| p.display().to_string()).collect() }

#[allow(clippy::too_many_arguments)]
fn run(id: u64, root: PathBuf, moved: bool, stop: Arc<AtomicBool>, everything: Arc<AtomicBool>, missing: Arc<AtomicU64>, missing_list: Arc<Mutex<Vec<serde_json::Value>>>, rx: Receiver<Vec<PathBuf>>, status: Arc<Mutex<String>>, changes: Arc<AtomicU64>, last: Arc<Mutex<Instant>>, asked: Asked) {
    // Everything: the first look, a mended library, clips taken in, a batch done. Regularly only the project's own
    // files: originals move only when the library is mended after (docs §9)
    // A catalog there to read: the views wait for it (none yet: they read the disks while it's made)
    // (STARTING set by start(), before the thread)
    let (roots, ours) = project::roots(&root);
    let (own_roots, own) = project::own_roots(&root);
    let mut worker = project::open(&root);
    // Read; nothing there yet: built (the first look at every folder, every file of ours read)
    loop {
        if stop.load(Ordering::Relaxed) { started(&root, id, false); return; }
        set(&status, "Reading the project's catalog…");
        match load(&mut worker, &Want::All) {
            Ok(info) if info.empty && info.snapshot.is_none() => {
                started(&root, id, false);
                set(&status, "Making the project's catalog (a first look at every folder)…");
                if worker.sweep(&roots, &ours, &Sweep::default()).is_ok() { let _ = worker.compact(); break; }
            }
            Ok(_) => break,
            // None of its snapshots reads whole: made again from the files
            Err(Error::Damaged(_)) => {
                set(&status, "Making the project's catalog again (it was damaged)…");
                worker = project::open(&root);
                if worker.sweep(&roots, &ours, &Sweep::default()).is_ok() && worker.compact().is_ok() { continue; }
            }
            Err(_) => {}
        }
        started(&root, id, false);
        set(&status, "The project's catalog can't be read now (the share?): trying again…");
        for _ in 0..40 { if stop.load(Ordering::Relaxed) { started(&root, id, false); return; } std::thread::sleep(Duration::from_millis(250)); }
    }
    // Moved here: what it says of the project's folder is of the folder it was in (its footage, now a location): looked
    // at again first, the views waiting meanwhile (its own folders only: seconds)
    if moved {
        set(&status, "Looking at the project's new folder…");
        let here: Vec<Root> = roots.iter().filter(|r| r.id == "p").cloned().collect();
        let _ = worker.sweep(&here, &ours, &Sweep::default());
    }
    let mut reader = project::open(&root);
    if load(&mut reader, &Want::All).is_err() { reader = project::open(&root); }
    let mine = Ours { footage_in_project: ours.footage_in_project.clone(), own_only: false };
    *SEEN.write().unwrap() = Some(Seen { id, root: root.clone(), roots: roots.clone(), ours: mine, cat: reader, bump: Some((changes.clone(), last.clone())) });
    started(&root, id, false);
    set(&status, "");
    // The views read meanwhile without it (the disks, or the catalog of before): read again. Those that waited for it
    // are as it says: nothing to read again
    // (a view that saw the catalog of before stored that before the thread could put its own in SEEN: under the lock)
    if ready_counts(id) { changed(&changes, &last); }
    // A source the catalog has never looked at (just added): looked at now (until then the views ask the disk for each
    // of its files, which on a share takes minutes); the others' views read from the catalog meanwhile
    // (or one whose sources inside it changed: one taken out of the project comes in under it)
    let new: Vec<afp_catalog::Root> = roots.iter().filter(|r| match worker.state.get(&r.id, "") {
        None => true,
        Some(f) => f.meta.as_ref().and_then(|m| m.get("exclude").cloned()).unwrap_or(serde_json::json!([])) != serde_json::json!(r.exclude),
    }).cloned().collect();
    if !new.is_empty() {
        set(&status, "Looking at the new source…");
        if worker.sweep(&new, &ours, &Sweep::default()).is_ok() {
            let mut g = SEEN.write().unwrap();
            if let Some(s) = g.as_mut().filter(|s| s.id == id) { let _ = s.cat.refresh(); }
            drop(g);
            changed(&changes, &last);
        }
        set(&status, "");
    }
    // Links whose original isn't there: counted now, and again a few seconds after what the catalog knows changed
    let count_missing = |missing: &AtomicU64| {
        let found = crate::checks::missing(&root);
        let n = found.as_ref().map(|v| v.len() as u64).unwrap_or(u64::MAX);
        *missing_list.lock().unwrap() = found.unwrap_or_default().into_iter()
            .map(|i| serde_json::json!({ "file": i["file"], "tree": i["tree"], "kind": i["link"]["kind"] })).collect();
        missing.store(n, Ordering::Relaxed);
    };
    count_missing(&missing);
    let mut counted_at = changes.load(Ordering::Relaxed);
    let mut counted_when = Instant::now();
    // Looked after: at the start, then every hour (afp_catalog::care decides whether anything needs looking at)
    let mut last_care = Instant::now() - Duration::from_secs(10_000);
    // What couldn't be noted: when it's looked at again, and how long it waits after a failure
    let mut retry: Vec<(String, String)> = Vec::new();
    let mut retry_at = Instant::now();
    let mut retry_wait = Duration::from_secs(5);
    let mut last_read = Instant::now();
    let mut last_look = Instant::now() - Duration::from_secs(10_000);
    let mut pending: BTreeSet<(String, String)> = BTreeSet::new();
    let mut last_event = Instant::now();
    let refresh = |changes: &AtomicU64, last: &Mutex<Instant>| {
        let mut g = SEEN.write().unwrap();
        let Some(s) = g.as_mut().filter(|s| s.id == id) else { return };
        // The views read again only when what there is changed (not a folder's listing noted again)
        let before = s.cat.seen_changes;
        match s.cat.refresh() {
            Ok(_) if s.cat.seen_changes != before => { let what = paths_of(&s.roots, s.cat.take_seen()); changed_what(changes, last, what) }
            Ok(_) | Err(Error::Io(_)) => {}
            Err(_) => { let mut c = project::open(&root); if load(&mut c, &Want::All).is_ok() { s.cat = c; changed(changes, last); } }
        }
    };
    while !stop.load(Ordering::Relaxed) {
        std::thread::sleep(Duration::from_millis(250));
        while let Ok(paths) = rx.try_recv() {
            for p in paths {
                if let Some((r, rel)) = project::locate(&roots, &p) {
                    pending.insert((r.clone(), afp_catalog::state::parent(&rel).to_string()));
                    pending.insert((r, rel));
                }
            }
            last_event = Instant::now();
        }
        if last_read.elapsed() >= Duration::from_secs(3) { refresh(&changes, &last); last_read = Instant::now(); }
        // …or asked for again (after mending), or not known yet
        let unknown = missing.load(Ordering::Relaxed) == u64::MAX && counted_when.elapsed() >= Duration::from_secs(2);
        if unknown || changes.load(Ordering::Relaxed) != counted_at && counted_when.elapsed() >= Duration::from_secs(5) {
            counted_at = changes.load(Ordering::Relaxed);
            counted_when = Instant::now();
            count_missing(&missing);
        }
        // What the Library couldn't note: its folders (and the files) looked at again, from the files as they are now
        for p in std::mem::take(&mut *RETRY.lock().unwrap()) {
            if let Some((r, rel)) = project::locate(&roots, &p) {
                let pair = (r.clone(), afp_catalog::state::parent(&rel).to_string());
                if !retry.contains(&pair) { retry.push(pair); }
                if !retry.contains(&(r.clone(), rel.clone())) { retry.push((r, rel)); }
            }
        }
        if !retry.is_empty() && Instant::now() >= retry_at {
            let only = std::mem::take(&mut retry);
            match worker.sweep(&roots, &ours, &Sweep { only: only.clone(), ..Default::default() }) {
                Ok(rep) => { unreadable_from(&roots, &rep); refresh(&changes, &last); retry_wait = Duration::from_secs(5); }
                // The share still away: again later, waiting longer each time (up to 5 minutes); the regular looks
                // see it anyway once it's back (the folders' times were changed when it was written)
                Err(Error::Io(_)) => { retry = only; retry_at = Instant::now() + retry_wait; retry_wait = (retry_wait * 2).min(Duration::from_secs(300)); }
                // A newer program's project: nothing more written
                Err(_) => {}
            }
        }
        // What this computer changed: its folders looked at once it settles
        if !pending.is_empty() && last_event.elapsed() >= Duration::from_secs(1) {
            let only: Vec<(String, String)> = std::mem::take(&mut pending).into_iter().collect();
            if let Ok(rep) = worker.sweep(&roots, &ours, &Sweep { only, ..Default::default() }) { unreadable_from(&roots, &rep); refresh(&changes, &last); }
        }
        // Folders that changed, listed once more a minute later (the Mac's listing then is sure)
        let due = worker.due_rechecks();
        if !due.is_empty() && last_look.elapsed() < Duration::from_secs(300) {
            if worker.sweep(&roots, &ours, &Sweep { only: due, ..Default::default() }).is_ok() { refresh(&changes, &last); }
        }
        if everything.swap(false, Ordering::Relaxed) {
            set(&status, "Looking at the project's footage…");
            if worker.sweep(&roots, &ours, &Sweep::default()).is_ok() { refresh(&changes, &last); }
            set(&status, "");
        }
        if last_look.elapsed() >= Duration::from_secs(300) {
            if let Ok(rep) = worker.sweep(&own_roots, &own, &Sweep::default()) { unreadable_from(&roots, &rep); refresh(&changes, &last); }
            unreadable_again();
            last_look = Instant::now();
        }
        // Asked for in Checks: every folder listed now, or every file of ours read again (both said for the other
        // computers: none of them needs to look today)
        let deep = asked.deep.swap(false, Ordering::Relaxed);
        let check = asked.check.swap(false, Ordering::Relaxed);
        if deep || check {
            let kind = if check { "check" } else { "deep" };
            *asked.care.lock().unwrap() = serde_json::json!({ "kind": kind, "busy": true });
            set(&status, if check { "Reading every file of ours again, against the catalog…" } else { "Looking at every folder again…" });
            let r = worker.sweep(&roots, &ours, &Sweep { deep, reread: check, ..Default::default() });
            set(&status, "");
            *asked.care.lock().unwrap() = match r {
                Ok(rep) => {
                    afp_catalog::care::mark_deep(&worker, &roots.iter().filter(|x| !rep.skipped_roots.contains(&x.id)).cloned().collect::<Vec<_>>());
                    unreadable_from(&roots, &rep);
                    refresh(&changes, &last);
                    let p = |k: &[(String, String)]| paths_of(&roots, Some(k.to_vec())).unwrap_or_default().iter().map(|p| p.display().to_string()).collect::<Vec<_>>();
                    serde_json::json!({ "kind": kind, "busy": false, "changes": rep.changes, "read": rep.inspected, "folders": rep.dirs,
                                        "corrected": p(&rep.corrected), "unreadable": p(&rep.unreadable), "not_there": rep.skipped_roots })
                }
                Err(e) => serde_json::json!({ "kind": kind, "busy": false, "error": e.to_string() }),
            };
            last_care = Instant::now();
        }
        if worker.should_compact() {
            set(&status, "Folding the project's catalog…");
            if worker.compact().is_ok() { let _ = worker.clean(); }
            set(&status, "");
        }
        // Looked after: the roots nobody has looked at deep within a day, nor watches now (the NAS's watcher), looked
        // at deep, slowly; nothing when somebody has (afp_catalog::care)
        if last_care.elapsed() >= Duration::from_secs(3600) {
            last_care = Instant::now();
            let st = status.clone();
            let r = afp_catalog::care::look_after(&mut worker, &roots, &ours, Some(Duration::from_millis(20)),
                &|n| set(&st, &format!("Looking at every folder of {n} {} nobody has looked after today…", if n == 1 { "source" } else { "sources" })));
            set(&status, "");
            if let Ok(Some(rep)) = r { unreadable_from(&roots, &rep); refresh(&changes, &last); }
        }
    }
    let mut g = SEEN.write().unwrap();
    if g.as_ref().is_some_and(|s| s.id == id) { *g = None; }
}

/// A look at everything, the footage too, asked of the project's thread
pub fn look_at_everything(r: Option<&Runner>) { if let Some(r) = r { r.everything.store(true, Ordering::Relaxed); } }

/// Every folder listed now (Checks: "Look at every folder again"), or every file of ours read again ("Check every file
/// against the catalog")
pub fn look_deep(r: Option<&Runner>, check: bool) {
    let Some(r) = r else { return };
    *r.care.lock().unwrap() = serde_json::json!({ "kind": if check { "check" } else { "deep" }, "busy": true });
    if check { r.check_now.store(true, Ordering::Relaxed); } else { r.deep_now.store(true, Ordering::Relaxed); }
}

/// How the last of those went, and the files that aren't what they should be
pub fn care(r: Option<&Runner>) -> serde_json::Value {
    let mut v = r.map(|r| r.care.lock().unwrap().clone()).unwrap_or(serde_json::Value::Null);
    if v.is_null() { v = serde_json::json!({}); }
    v["unreadable_now"] = serde_json::json!(unreadable());
    v
}

/// What the page shows of it
pub fn status(r: Option<&Runner>) -> String { r.map(|r| r.status.lock().unwrap().clone()).unwrap_or_default() }

/// How many links' originals aren't where they say (u64::MAX: not known yet)
pub fn missing(r: Option<&Runner>) -> u64 { r.map(|r| r.missing.load(Ordering::Relaxed)).unwrap_or(u64::MAX) }

/// Which links' originals aren't where they say
pub fn missing_list(r: Option<&Runner>) -> Vec<serde_json::Value> { r.map(|r| r.missing_list.lock().unwrap().clone()).unwrap_or_default() }

/// Counted again soon (after mending)
pub fn recount(r: Option<&Runner>) { if let Some(r) = r { r.missing.store(u64::MAX, Ordering::Relaxed); } }

pub fn is_on(root: &Path) -> bool { SEEN.read().ok().is_some_and(|g| g.as_ref().is_some_and(|s| s.root == root)) }

#[cfg(test)]
mod tests {
    use super::*;

    fn project(name: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!("afl-seen-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("Day 1")).unwrap();
        std::fs::write(root.join("Day 1/A.MP4"), b"x").unwrap();
        std::fs::write(root.join("Project.afproject"), r#"{"id":"seen-test","locations":[{"name":"Here","at":"","path":"."}]}"#).unwrap();
        std::fs::write(root.join("Day 1/A.afp.json"), br#"{"format":"actionfootageplayer-clip","version":1,"tags":["kept"]}"#).unwrap();
        let root = root.canonicalize().unwrap();
        let (roots, ours) = project::roots(&root);
        let store: Arc<dyn Store> = Arc::new(DiskStore);
        let mut cat = Catalog::new(store.clone(), store, &root, &root.join(".cache"), "test");
        cat.load(&Want::All).unwrap();
        cat.sweep(&roots, &ours, &Default::default()).unwrap();
        *SEEN.write().unwrap() = Some(Seen::of(&root, cat));
        root
    }

    /// The project opened: the views that waited for its catalog aren't read again once it's read (the 2026-11-24
    /// journal: every start read moments and footage twice, side by side); a view read before it (the disks, the
    /// catalog of before) is
    #[test]
    fn the_views_read_again_at_the_start_only_when_read_without_its_catalog() {
        let _one = TESTS.lock().unwrap_or_else(|e| e.into_inner());
        // (a project with no catalog yet: project() above makes one, but no snapshot)
        let root = std::env::temp_dir().join(format!("afl-seen-ready-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("Day 1")).unwrap();
        std::fs::write(root.join("Day 1/A.MP4"), b"x").unwrap();
        std::fs::write(root.join("Project.afproject"), r#"{"id":"seen-ready","locations":[{"name":"Here","at":"","path":"."}]}"#).unwrap();
        let root = root.canonicalize().unwrap();
        // (a home of its own: the catalog's cache of this computer not touched, nor another test's used)
        std::env::set_var("AFP_HOME", root.join(".home"));
        *SEEN.write().unwrap() = None;
        let ready = |r: &Runner| {
            let t = Instant::now();
            while !(SEEN.read().unwrap().as_ref().is_some_and(|s| s.root == root && s.id == RUN.load(Ordering::Relaxed)) && status(Some(r)).is_empty())
                && t.elapsed() < Duration::from_secs(30) { std::thread::sleep(Duration::from_millis(20)); }
            assert!(status(Some(r)).is_empty());
            std::thread::sleep(Duration::from_millis(300));
        };
        let counter = || (Arc::new(AtomicU64::new(0)), Arc::new(Mutex::new(Instant::now())));
        // No catalog yet: made, the view read from the disks meanwhile, so read again
        let (c, l) = counter();
        let r = start(&root, c.clone(), l, false);
        reading_view(&root);
        ready(&r);
        assert!(root.join(".afp/catalog/snap").is_dir(), "the catalog made");
        assert!(c.load(Ordering::Relaxed) >= 1, "read without it: read again");
        drop(r);
        // There: the view waits for it, as main.rs's scan does, and isn't read again
        let (c, l) = counter();
        let r = start(&root, c.clone(), l, false);
        wait_ready(&root, Duration::from_secs(30));
        reading_view(&root);
        ready(&r);
        assert_eq!(c.load(Ordering::Relaxed), 0, "read from it: nothing to read again");
        drop(r);
        // Watched again (its locations changed): a view read from the catalog of before is read again
        let (c, l) = counter();
        let r = start(&root, c.clone(), l, false);
        reading_view(&root);
        ready(&r);
        assert!(c.load(Ordering::Relaxed) >= 1, "read from the catalog of before: read again");
        drop(r);
        *SEEN.write().unwrap() = None;
        let _ = std::fs::remove_dir_all(&root);
    }

    fn tags(path: &Path) -> serde_json::Value { meta(path).flatten().map(|m| m["own"]["tags"].clone()).unwrap_or_default() }

    /// A clip's own file written wrong by a script, noted by the Library: its tags aren't wiped, it's listed for
    /// Checks; written right (in place), it's noted at the thread's next turn and off the list
    #[test]
    fn a_broken_own_file_is_kept_said_and_taken_once_right() {
        let _one = TESTS.lock().unwrap_or_else(|e| e.into_inner());
        UNREADABLE.lock().unwrap().clear();
        let root = project("broken");
        let own = root.join("Day 1/A.afp.json");
        assert_eq!(tags(&own), serde_json::json!(["kept"]));
        std::fs::write(&own, br#"{"tags":["half"#).unwrap();
        note_now(std::slice::from_ref(&own));
        assert_eq!(tags(&own), serde_json::json!(["kept"]), "its record as it was");
        assert_eq!(unreadable(), vec![own.display().to_string()]);
        unreadable_again();
        assert_eq!(unreadable().len(), 1, "still broken: still said");
        std::fs::write(&own, br#"{"format":"actionfootageplayer-clip","version":1,"tags":["kept","fixed"]}"#).unwrap();
        unreadable_again();
        assert!(unreadable().is_empty());
        assert_eq!(tags(&own), serde_json::json!(["kept", "fixed"]));
        *SEEN.write().unwrap() = None;
        let _ = std::fs::remove_dir_all(&root);
    }

    /// What note_now couldn't write (the catalog's journal not writable: a share away) is kept to be looked at again,
    /// by the files' paths — never by what was seen of them then
    #[test]
    fn a_note_that_couldnt_be_written_is_kept_to_look_at_again() {
        let _one = TESTS.lock().unwrap_or_else(|e| e.into_inner());
        RETRY.lock().unwrap().clear();
        let root = project("retry");
        let own = root.join("Day 1/A.afp.json");
        std::fs::write(&own, br#"{"tags":["new"]}"#).unwrap();
        // The journal's folder made unwritable, as a share gone away is
        let journal = root.join(".afp/catalog/journal");
        let mut p = std::fs::metadata(&journal).unwrap().permissions();
        #[cfg(unix)] { use std::os::unix::fs::PermissionsExt; p.set_mode(0o500); }
        std::fs::set_permissions(&journal, p.clone()).unwrap();
        for e in std::fs::read_dir(&journal).unwrap().flatten() {
            let mut q = std::fs::metadata(e.path()).unwrap().permissions();
            #[cfg(unix)] { use std::os::unix::fs::PermissionsExt; q.set_mode(0o500); }
            std::fs::set_permissions(e.path(), q).unwrap();
            for f in std::fs::read_dir(e.path()).unwrap().flatten() {
                let mut r = std::fs::metadata(f.path()).unwrap().permissions();
                r.set_readonly(true);
                std::fs::set_permissions(f.path(), r).unwrap();
            }
        }
        // (the writer's own record of where it is, on this computer, forgotten: it must find the segment again)
        let _ = std::fs::remove_file(root.join(".cache/writer.json"));
        note_now(std::slice::from_ref(&own));
        let kept = RETRY.lock().unwrap().clone();
        #[cfg(unix)] { use std::os::unix::fs::PermissionsExt;
            for e in walkdir(&journal) { let mut q = std::fs::metadata(&e).unwrap().permissions(); q.set_mode(0o755); std::fs::set_permissions(&e, q).unwrap(); } }
        assert!(kept.contains(&own), "kept to look at again: {kept:?}");
        *SEEN.write().unwrap() = None;
        let _ = std::fs::remove_dir_all(&root);
    }

    fn walkdir(d: &Path) -> Vec<PathBuf> {
        let mut out = vec![d.to_path_buf()];
        for e in std::fs::read_dir(d).into_iter().flatten().flatten() {
            if e.path().is_dir() { out.extend(walkdir(&e.path())); } else { out.push(e.path()); }
        }
        out
    }
}

// The footage view of a real project, from its catalog and from the disks, compared (by hand:
// AFL_SEEN=<project> cargo test --release -- --ignored footage_from_catalog --nocapture)
#[cfg(test)]
mod real {
    #[test]
    #[ignore]
    fn footage_from_catalog() {
        let Some(root) = std::env::var_os("AFL_SEEN").map(PathBuf::from) else { return };
        use super::*;
        let _one = TESTS.lock().unwrap_or_else(|e| e.into_inner());
        let count = |v: &serde_json::Value| (v["items"].as_array().map(|a| a.len()).unwrap_or(0), v["dirs"].as_array().map(|a| a.len()).unwrap_or(0),
                                             v["clips_dirs"].as_array().map(|a| a.len()).unwrap_or(0));
        let t = Instant::now();
        let files = crate::index::scan(&root, "footage", false);
        println!("from the disks: {:?} items/dirs/clips_dirs in {:.1} s", count(&files), t.elapsed().as_secs_f64());
        let (roots, _) = project::roots(&root);
        let mut cat = project::open(&root);
        cat.load(&Want::All).unwrap();
        for r in &roots { println!("root {} at {} record {:?} exclude {:?}", r.id, r.dir.display(), cat.state.get(&r.id, "").map(|f| f.mtime), r.exclude); }
        *SEEN.write().unwrap() = Some(Seen::of(&root, cat));
        let t = Instant::now();
        let seen = crate::index::scan(&root, "footage", false);
        println!("from the catalog: {:?} in {:.2} s", count(&seen), t.elapsed().as_secs_f64());
        let ids = |v: &serde_json::Value| v["items"].as_array().into_iter().flatten().map(|i| (i["id"].as_str().unwrap_or("").to_string(), i["pdir"].to_string(), i["path"].as_str().unwrap_or("").to_string())).collect::<std::collections::BTreeSet<_>>();
        let (a, b) = (ids(&files), ids(&seen));
        for x in a.difference(&b).take(5) { println!("only from the disks: {x:?}"); }
        for x in b.difference(&a).take(5) { println!("only from the catalog: {x:?}"); }
        *SEEN.write().unwrap() = None;
    }

    /// The moments' view of a real project, as a Library just started reads it (a process of its own: nothing kept in
    /// memory), its catalog read first (not timed). Reads only.
    ///   AFL_SEEN=<project> cargo test --release -- --ignored moments_from_catalog --nocapture
    #[test]
    #[ignore]
    fn moments_from_catalog() {
        let Some(root) = std::env::var_os("AFL_SEEN").map(PathBuf::from) else { return };
        use super::*;
        let _one = TESTS.lock().unwrap_or_else(|e| e.into_inner());
        let mut cat = project::open(&root);
        cat.load(&Want::All).unwrap();
        *SEEN.write().unwrap() = Some(Seen::of(&root, cat));
        for v in ["moments", "footage"] {
            let t = Instant::now();
            let out = crate::index::scan(&root, v, false);
            println!("{v}: {} items in {:.0} ms", out["items"].as_array().map(Vec::len).unwrap_or(0), t.elapsed().as_secs_f64() * 1000.0);
        }
        // Each moment's renders as the view finds them from the catalog, and as the disk says (renders::Ctx fresh)
        let (fresh, view) = (crate::renders::Ctx::new(&root, true), crate::renders::Ctx::new(&root, false));
        let (mut same, mut differ) = (0, Vec::new());
        for it in crate::index::scan(&root, "moments", false)["items"].as_array().into_iter().flatten() {
            let Ok(item) = crate::renders::item(&root, "moment", it["id"].as_str().unwrap_or("")) else { continue };
            for p in ["HDR", "SDR"] {
                let profile = crate::renders::profile(p).unwrap();
                let (a, b) = (crate::renders::find(&fresh, &item, profile), crate::renders::find(&view, &item, profile));
                if (a.recipe.clone(), a.file.clone(), a.stale.clone()) == (b.recipe.clone(), b.file.clone(), b.stale.clone()) { same += 1 } else { differ.push((item.rel.clone(), p, a.recipe, b.recipe)); }
            }
        }
        println!("renders found the same from the catalog as from the disk: {same}, not: {}", differ.len());
        for d in differ.iter().take(3) { println!("  {d:?}"); }
        *SEEN.write().unwrap() = None;
    }

    #[test]
    #[ignore]
    fn check_from_catalog() {
        let Some(root) = std::env::var_os("AFL_SEEN").map(PathBuf::from) else { return };
        use super::*;
        let _one = TESTS.lock().unwrap_or_else(|e| e.into_inner());
        let mut cat = project::open(&root);
        cat.load(&Want::All).unwrap();
        *SEEN.write().unwrap() = Some(Seen::of(&root, cat));
        let t = Instant::now();
        let m = crate::checks::missing(&root);
        println!("missing on its own (from the catalog): {:?} in {:.2} s", m.as_ref().map(|v| v.len()), t.elapsed().as_secs_f64());
        let t = Instant::now();
        let all: Vec<String> = crate::checks::footage(&crate::locations::locations(&root, &crate::locations::mounts())).0.iter().map(|c| c.tree.clone()).collect();
        let r = crate::checks::check(&root, &all, &[]);
        let issues = r["issues"].as_array().cloned().unwrap_or_default();
        let mut kinds = std::collections::BTreeMap::new();
        for i in &issues { *kinds.entry(i["kind"].as_str().unwrap_or("").to_string()).or_insert(0) += 1; }
        println!("check, whole project: {} clips, issues {kinds:?} in {:.1} s", r["clips"], t.elapsed().as_secs_f64());
        let t = Instant::now();
        let missing: Vec<serde_json::Value> = issues.into_iter().filter(|i| i["kind"] == "missing").take(20).collect();
        let f = crate::checks::find_moved(&root, &missing);
        let found = f.as_array().map(|a| a.iter().filter(|x| !x["found"].is_null()).count()).unwrap_or(0);
        println!("find moved, 20 of them: {found} found in {:.1} s; e.g. {}", t.elapsed().as_secs_f64(), f[0]["found"]["tree"]);
        *SEEN.write().unwrap() = None;
    }
}
