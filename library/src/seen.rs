// SPDX-License-Identifier: GPL-3.0-or-later
//
// The project's catalog in the Library (docs/library-management.md §9): what's in its folder and locations as last
// seen, shared with every computer through .afp/catalog/. The views read it instead of the disks (a file's size,
// time, link, sidecar: no question to the NAS for each); a thread keeps it up to date:
//
// - every 3 s: the journals' new lines (what the player, or another computer, wrote)
// - what this computer's system says changed (FSEvents): those folders looked at a second after it settles
// - every 5 minutes: a look at every folder's time; once a day the deep look, slowly
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
static STARTING: Mutex<Option<PathBuf>> = Mutex::new(None);

/// Until the project's catalog is read (or found not to be there: then the views read the disks)
pub fn wait_ready(root: &Path, most: Duration) {
    let t = Instant::now();
    while t.elapsed() < most && STARTING.lock().unwrap().as_deref() == Some(root) { std::thread::sleep(Duration::from_millis(50)); }
}

fn started(root: &Path, on: bool) {
    let mut g = STARTING.lock().unwrap();
    if on { *g = Some(root.to_path_buf()); } else if g.as_deref() == Some(root) { *g = None; }
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
    if !prefix.is_empty() && !s.cat.state.get(&root, &prefix).is_some_and(|f| f.dir) { return Some(Vec::new()); }
    // As the disk's walk gives them: a folder's entries by name, folder after folder as they were found
    let mut kids: std::collections::HashMap<&str, Vec<(&str, &File)>> = std::collections::HashMap::new();
    for (rel, r) in s.cat.state.under(&root, &prefix) {
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
    Some(out)
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
            Err(_) => {}
        }
    }
    // What's inside the files that counts (a link, a clip's own file), 8 at a time
    let next = std::sync::atomic::AtomicUsize::new(0);
    let read: Mutex<Vec<(String, String, Option<File>)>> = Mutex::new(Vec::new());
    std::thread::scope(|sc| for _ in 0..8.min(files.len()) { sc.spawn(|| loop {
        let i = next.fetch_add(1, Ordering::Relaxed);
        let Some((root, rel, path, st)) = files.get(i) else { break };
        let meta = if ours.wants(root, rel) { match ours.inspect(path, root, rel) { Ok(m) => m, Err(_) => continue } } else { None };
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
    let _ = s.cat.note(entries);
    if let Some((changes, last)) = &s.bump { changed(changes, last); }
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
    let r = Runner { stop: stop.clone(), everything: everything.clone(), missing: missing.clone(), missing_list: missing_list.clone(), events: tx, status: status.clone() };
    let root = root.to_path_buf();
    static IDS: AtomicU64 = AtomicU64::new(1);
    let id = IDS.fetch_add(1, Ordering::Relaxed);
    std::thread::Builder::new().name("catalog".into()).spawn(move || run(id, root, moved, stop, everything, missing, missing_list, rx, status, changes, last)).ok();
    r
}

fn changed(changes: &AtomicU64, last: &Mutex<Instant>) {
    changes.fetch_add(1, Ordering::Relaxed);
    *last.lock().unwrap() = Instant::now() - Duration::from_secs(1);
}

fn set(status: &Mutex<String>, s: &str) { *status.lock().unwrap() = s.to_string(); }

#[allow(clippy::too_many_arguments)]
fn run(id: u64, root: PathBuf, moved: bool, stop: Arc<AtomicBool>, everything: Arc<AtomicBool>, missing: Arc<AtomicU64>, missing_list: Arc<Mutex<Vec<serde_json::Value>>>, rx: Receiver<Vec<PathBuf>>, status: Arc<Mutex<String>>, changes: Arc<AtomicU64>, last: Arc<Mutex<Instant>>) {
    // Everything: the first look, a mended library, clips taken in, a batch done. Regularly only the project's own
    // files: originals move only when the library is mended after (docs §9)
    // A catalog there to read: the views wait for it (none yet: they read the disks while it's made)
    let has = root.join(".afp").join("catalog").join("snap").is_dir();
    if has { started(&root, true); }
    let (roots, ours) = project::roots(&root);
    let (own_roots, own) = project::own_roots(&root);
    let mut worker = project::open(&root);
    // Read; nothing there yet: built (the first look at every folder, every file of ours read)
    loop {
        if stop.load(Ordering::Relaxed) { started(&root, false); return; }
        set(&status, "Reading the project's catalog…");
        match load(&mut worker, &Want::All) {
            Ok(info) if info.empty && info.snapshot.is_none() => {
                started(&root, false);
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
        started(&root, false);
        set(&status, "The project's catalog can't be read now (the share?): trying again…");
        for _ in 0..40 { if stop.load(Ordering::Relaxed) { started(&root, false); return; } std::thread::sleep(Duration::from_millis(250)); }
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
    started(&root, false);
    set(&status, "");
    changed(&changes, &last);
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
    let deep_file = worker.cache.join("deep.json");
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
            Ok(_) if s.cat.seen_changes != before => changed(changes, last),
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
        // What this computer changed: its folders looked at once it settles
        if !pending.is_empty() && last_event.elapsed() >= Duration::from_secs(1) {
            let only: Vec<(String, String)> = std::mem::take(&mut pending).into_iter().collect();
            if worker.sweep(&roots, &ours, &Sweep { only, ..Default::default() }).is_ok() { refresh(&changes, &last); }
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
            if worker.sweep(&own_roots, &own, &Sweep::default()).is_ok() { refresh(&changes, &last); }
            last_look = Instant::now();
        }
        if worker.should_compact() {
            set(&status, "Folding the project's catalog…");
            if worker.compact().is_ok() { let _ = worker.clean(); }
            set(&status, "");
        }
        // The deep look, once a day (this computer's clock: only how long since it last did)
        let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
        let before = std::fs::read_to_string(&deep_file).ok().and_then(|t| t.trim().parse::<u64>().ok()).unwrap_or(0);
        if now.saturating_sub(before) > 86_400 && last_look.elapsed() < Duration::from_secs(290) {
            if worker.sweep(&own_roots, &own, &Sweep { deep: true, pause: Some(Duration::from_millis(20)), ..Default::default() }).is_ok() {
                let _ = std::fs::write(&deep_file, now.to_string());
                refresh(&changes, &last);
            }
        }
    }
    let mut g = SEEN.write().unwrap();
    if g.as_ref().is_some_and(|s| s.id == id) { *g = None; }
}

/// A look at everything, the footage too, asked of the project's thread
pub fn look_at_everything(r: Option<&Runner>) { if let Some(r) = r { r.everything.store(true, Ordering::Relaxed); } }

/// What the page shows of it
pub fn status(r: Option<&Runner>) -> String { r.map(|r| r.status.lock().unwrap().clone()).unwrap_or_default() }

/// How many links' originals aren't where they say (u64::MAX: not known yet)
pub fn missing(r: Option<&Runner>) -> u64 { r.map(|r| r.missing.load(Ordering::Relaxed)).unwrap_or(u64::MAX) }

/// Which links' originals aren't where they say
pub fn missing_list(r: Option<&Runner>) -> Vec<serde_json::Value> { r.map(|r| r.missing_list.lock().unwrap().clone()).unwrap_or_default() }

/// Counted again soon (after mending)
pub fn recount(r: Option<&Runner>) { if let Some(r) = r { r.missing.store(u64::MAX, Ordering::Relaxed); } }

pub fn is_on(root: &Path) -> bool { SEEN.read().ok().is_some_and(|g| g.as_ref().is_some_and(|s| s.root == root)) }

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
