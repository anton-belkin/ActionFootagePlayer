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

use afp_catalog::state::File;
use afp_catalog::{ project, Catalog, Error, Root, Sweep, Want };
use std::collections::BTreeSet;
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
    pub cat: Catalog,
}

pub static SEEN: RwLock<Option<Seen>> = RwLock::new(None);

/// Tests that set SEEN, one at a time (it's the process's)
#[cfg(test)]
pub static TESTS: Mutex<()> = Mutex::new(());

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

/// The thread keeping the catalog up to date for one project; stopped when dropped
pub struct Runner {
    stop: Arc<AtomicBool>,
    /// A look at everything asked for, the footage too (the library mended, clips taken in, a batch done)
    pub everything: Arc<AtomicBool>,
    pub events: Sender<Vec<PathBuf>>,
    /// What it's doing, for the page ("" when nothing)
    pub status: Arc<Mutex<String>>,
}

impl Drop for Runner {
    fn drop(&mut self) { self.stop.store(true, Ordering::Relaxed); }
}

fn load(cat: &mut Catalog, want: &Want) -> Result<afp_catalog::Loaded, Error> {
    match cat.load(want) { Err(Error::Gap(_)) => cat.load(want), r => r }
}

/// Started for a project: `changes` counted (and `last` set) each time what it knows changed, for the page to read it
/// again
pub fn start(root: &Path, changes: Arc<AtomicU64>, last: Arc<Mutex<Instant>>) -> Runner {
    let (tx, rx) = channel();
    let stop = Arc::new(AtomicBool::new(false));
    let status = Arc::new(Mutex::new(String::new()));
    let everything = Arc::new(AtomicBool::new(false));
    let r = Runner { stop: stop.clone(), everything: everything.clone(), events: tx, status: status.clone() };
    let root = root.to_path_buf();
    static IDS: AtomicU64 = AtomicU64::new(1);
    let id = IDS.fetch_add(1, Ordering::Relaxed);
    std::thread::Builder::new().name("catalog".into()).spawn(move || run(id, root, stop, everything, rx, status, changes, last)).ok();
    r
}

fn changed(changes: &AtomicU64, last: &Mutex<Instant>) {
    changes.fetch_add(1, Ordering::Relaxed);
    *last.lock().unwrap() = Instant::now() - Duration::from_secs(1);
}

fn set(status: &Mutex<String>, s: &str) { *status.lock().unwrap() = s.to_string(); }

fn run(id: u64, root: PathBuf, stop: Arc<AtomicBool>, everything: Arc<AtomicBool>, rx: Receiver<Vec<PathBuf>>, status: Arc<Mutex<String>>, changes: Arc<AtomicU64>, last: Arc<Mutex<Instant>>) {
    // Everything: the first look, a mended library, clips taken in, a batch done. Regularly only the project's own
    // files: originals move only when the library is mended after (docs §9)
    let (roots, ours) = project::roots(&root);
    let (own_roots, own) = project::own_roots(&root);
    let mut worker = project::open(&root);
    // Read; nothing there yet: built (the first look at every folder, every file of ours read)
    loop {
        if stop.load(Ordering::Relaxed) { return; }
        set(&status, "Reading the project's catalog…");
        match load(&mut worker, &Want::All) {
            Ok(info) if info.empty && info.snapshot.is_none() => {
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
        set(&status, "The project's catalog can't be read now (the share?): trying again…");
        for _ in 0..40 { if stop.load(Ordering::Relaxed) { return; } std::thread::sleep(Duration::from_millis(250)); }
    }
    let mut reader = project::open(&root);
    if load(&mut reader, &Want::All).is_err() { reader = project::open(&root); }
    *SEEN.write().unwrap() = Some(Seen { id, root: root.clone(), roots: roots.clone(), cat: reader });
    set(&status, "");
    changed(&changes, &last);
    // A source the catalog has never looked at (just added): looked at now (until then the views ask the disk for each
    // of its files, which on a share takes minutes); the others' views read from the catalog meanwhile
    let new: Vec<afp_catalog::Root> = roots.iter().filter(|r| worker.state.get(&r.id, "").is_none()).cloned().collect();
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

pub fn is_on(root: &Path) -> bool { SEEN.read().ok().is_some_and(|g| g.as_ref().is_some_and(|s| s.root == root)) }
