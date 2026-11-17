// The catalog against the files, scenario by scenario (docs/catalog-safeguards.md): what's changed in the project's
// files, by our programs or by anything else (Finder, a script), is in the catalog within the time the report
// promises — at once where something watches, within a day where nothing does, now on demand — and the catalog
// writes nothing where nothing changed. Two computers on one simulated share: a Mac with the Library, and the NAS.

mod common;
use afp_catalog::care;
use afp_catalog::sim::{ Fs, Net, SimStore };
use afp_catalog::state::File;
use afp_catalog::store::{ Item, Stat };
use afp_catalog::{ Catalog, Inspect, Root, Store, Sweep, Want };
use common::*;
use serde_json::{ json, Value };
use std::io;
use std::path::{ Path, PathBuf };
use std::sync::{ Arc, Mutex };

const MINUTE: i64 = 60 * 1_000_000_000;

/// One computer's view of the shared files with its own precision of times: `extra` nanoseconds added to every time
/// it sees. The simulated disk's times are whole microseconds; a Mac over SMB sees a file's time in 100 ns steps
/// (…540500), the NAS natively in nanoseconds (…540523): the NAS is `extra: 23`, the Mac `extra: 0`.
#[derive(Clone)]
struct Prec { inner: SimStore, extra: i64 }

impl Prec {
    fn fix(&self, s: Stat) -> Stat { Stat { mtime: s.mtime + self.extra, ..s } }
}

impl Store for Prec {
    fn stat(&self, p: &Path) -> io::Result<Option<Stat>> { Ok(self.inner.stat(p)?.map(|s| self.fix(s))) }
    fn list(&self, p: &Path) -> io::Result<Vec<Item>> {
        Ok(self.inner.list(p)?.into_iter().map(|i| Item { stat: self.fix(i.stat), ..i }).collect())
    }
    fn read(&self, p: &Path) -> io::Result<Vec<u8>> { self.inner.read(p) }
    fn read_from(&self, p: &Path, o: u64) -> io::Result<Vec<u8>> { self.inner.read_from(p, o) }
    fn append(&self, p: &Path, d: &[u8]) -> io::Result<()> { self.inner.append(p, d) }
    fn put(&self, p: &Path, d: &[u8]) -> io::Result<()> { self.inner.put(p, d) }
    fn remove(&self, p: &Path) -> io::Result<()> { self.inner.remove(p) }
    fn mkdirs(&self, p: &Path) -> io::Result<()> { self.inner.mkdirs(p) }
    fn lock(&self, p: &Path) -> io::Result<Box<dyn std::any::Any>> { self.inner.lock(p) }
}

/// What's read from a file: a moment's link in the project's folder (as common's), and a clip's own file (.afp.json)
/// anywhere, kept whole; one that isn't JSON is an error, as afp_catalog::inspect::read has it. `old`: a program from
/// before "pick" (Accept / Reject) was kept, which drops it (the NAS Helper of 2026-11-17)
struct Reader { store: Arc<dyn Store>, old: bool }

impl Inspect for Reader {
    fn skip(&self, _r: &str, _rel: &str, name: &str, _d: bool) -> bool { name.starts_with('.') || name.starts_with('#') }
    fn wants(&self, root: &str, rel: &str) -> bool { (root == "p" && rel.ends_with(".mp4")) || rel.ends_with(".afp.json") }
    fn inspect(&self, path: &Path, _r: &str, rel: &str) -> io::Result<Option<Value>> {
        let b = self.store.read(path)?;
        if rel.ends_with(".mp4") { return Ok(read_meta(&b)); }
        let mut own = serde_json::from_slice::<Value>(&b).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e.to_string()))?;
        if self.old { if let Some(o) = own.as_object_mut() { o.remove("pick"); } }
        Ok(Some(json!({ "own": own })))
    }
}

struct Pc { cat: Catalog, store: Arc<dyn Store>, old: bool }

impl Pc {
    fn new(fs: &Arc<Mutex<Fs>>, name: &str, extra: i64, old: bool) -> Pc {
        let store: Arc<dyn Store> = Arc::new(Prec { inner: SimStore { fs: fs.clone(), net: Net::new(7) }, extra });
        let local = SimStore { fs: Fs::new(), net: Net::new(9) };
        let mut cat = Catalog::new(store.clone(), Arc::new(local), Path::new(PROJECT), Path::new("/cache/proj"), name);
        cat.grace = std::time::Duration::ZERO;
        cat.recheck_after = std::time::Duration::ZERO;
        cat.load(&Want::All).unwrap();
        Pc { cat, store, old }
    }
    fn reader(&self) -> Reader { Reader { store: self.store.clone(), old: self.old } }
    /// A look at every root's folders' times (the NAS's watcher every 30 min, a Library's "everything")
    fn look(&mut self) -> afp_catalog::Report { let r = self.reader(); self.cat.sweep(&roots(), &r, &Sweep::default()).unwrap() }
    /// The Library's regular look: the project's own folder only, by folders' times (seen.rs, project::own_roots)
    fn look_own(&mut self) -> afp_catalog::Report {
        let r = self.reader();
        let own: Vec<Root> = roots().into_iter().filter(|r| r.id == "p").collect();
        self.cat.sweep(&own, &r, &Sweep::default()).unwrap()
    }
    /// The Library at its start and every hour: the roots nobody has looked after today, looked at deep
    fn look_after(&mut self) -> Option<afp_catalog::Report> { let r = self.reader(); care::look_after(&mut self.cat, &roots(), &r, None, &|_| {}).unwrap() }
    /// Checks' "Look at every folder again", afp-catalog look --deep
    fn deep(&mut self) -> afp_catalog::Report {
        let r = self.reader();
        let rep = self.cat.sweep(&roots(), &r, &Sweep { deep: true, ..Default::default() }).unwrap();
        care::mark_deep(&self.cat, &roots());
        rep
    }
    /// Checks' "Read every file again", afp-catalog check
    fn check(&mut self) -> afp_catalog::Report { let r = self.reader(); self.cat.sweep(&roots(), &r, &Sweep { reread: true, ..Default::default() }).unwrap() }
    /// The NAS's watcher starting: every folder listed, then its roots vouched for while it watches
    fn helper_start(&mut self) -> afp_catalog::Report {
        let rep = self.deep();
        care::watching(&self.cat, &roots(), true);
        rep
    }
    fn beat(&self) { care::watching(&self.cat, &roots(), false); }
    /// The NAS's watcher told by inotify that something in this folder changed
    fn told(&mut self, root: &str, dir: &str) -> afp_catalog::Report {
        let r = self.reader();
        self.cat.sweep(&roots(), &r, &Sweep { only: vec![(root.into(), dir.into())], ..Default::default() }).unwrap()
    }
    /// What the Library's note_now does after writing a file: the file read and noted, its folder touched
    fn note_now(&mut self, root: &str, rel: &str) {
        let dir = roots().into_iter().find(|r| r.id == root).unwrap().dir;
        let path = dir.join(rel);
        let st = self.store.stat(&path).unwrap().unwrap();
        let meta = self.reader().inspect(&path, root, rel).unwrap();
        self.cat.touch(path.parent().unwrap()).unwrap();
        self.cat.note(vec![(root.into(), rel.into(), Some(File { dir: false, size: st.size, mtime: st.mtime, meta, digest: None }))]).unwrap();
    }
    fn meta(&self, root: &str, rel: &str) -> Option<Value> { self.cat.state.get(root, rel).and_then(|f| f.meta.clone()) }
    fn has(&self, root: &str, rel: &str) -> bool { self.cat.state.get(root, rel).is_some() }
}

fn moments(fs: &Arc<Mutex<Fs>>) -> usize {
    fs.lock().unwrap().files_under(Path::new(PROJECT)).iter().filter(|p| p.to_string_lossy().ends_with(".mp4")).count()
}

fn footage(rel: &str) -> PathBuf { Path::new(FOOTAGE).join(rel) }

/// Time passing on the share (its clock: the files' times)
fn later(fs: &Arc<Mutex<Fs>>, ns: i64) { fs.lock().unwrap().tick += ns; }

// --- One precision of times (safeguard A) ---

/// Two computers that see the same files' times at different precisions (the NAS in ns, a Mac in 100 ns) settle like
/// two that see them alike: after the first looks, a look reads nothing and writes nothing. (Before the cut each look
/// found the other's records "changed", and both re-read and re-noted every file for ever.)
#[test]
fn two_precisions_settle_as_one() {
    for nas_extra in [0, 23] {
        let fs = Fs::new();
        project(&fs, 6, 10, 2);
        assert!(moments(&fs) > 0);
        let mut mac = Pc::new(&fs, "mac", 0, false);
        let mut nas = Pc::new(&fs, "nas", nas_extra, false);
        mac.look();
        nas.look();
        for round in 0..4 {
            let (m, s) = (mac.look(), nas.look());
            assert_eq!((m.inspected, s.inspected, m.changes, s.changes), (0, 0, 0, 0), "NAS +{nas_extra} ns, round {round}");
        }
    }
}

/// The Library writes a clip's own file and notes it; the NAS's watcher, told of the write, lists the folder and
/// leaves the file alone: it's on record as it is (the door of 2026-11-17 was this re-read, by a program that kept less).
#[test]
fn the_nas_leaves_alone_a_file_the_mac_just_noted() {
    for nas_extra in [0, 23] {
        let fs = Fs::new();
        project(&fs, 2, 4, 0);
        let rel = "Day000/C0000.afp.json";
        fs.lock().unwrap().write(&footage(rel), br#"{"tags":["sun"]}"#);
        let mut mac = Pc::new(&fs, "mac", 0, false);
        let mut nas = Pc::new(&fs, "nas", nas_extra, false);
        mac.look();
        nas.look();
        fs.lock().unwrap().write(&footage(rel), br#"{"tags":["sun"],"pick":"reject"}"#);
        mac.note_now("f", rel);
        let r = nas.told("f", "Day000");
        assert_eq!(r.inspected, 0, "NAS +{nas_extra} ns read it again");
    }
}

/// …so an older program on the NAS (one that drops a field) never gets to rewrite the record, and Accept / Reject
/// stays in every view, through every look of both computers.
#[test]
fn an_older_reader_never_rewrites_what_the_mac_noted() {
    for nas_extra in [0, 23] {
        let fs = Fs::new();
        project(&fs, 2, 4, 0);
        let rel = "Day000/C0000.afp.json";
        fs.lock().unwrap().write(&footage(rel), br#"{"tags":["sun"]}"#);
        let mut mac = Pc::new(&fs, "mac", 0, false);
        let mut nas = Pc::new(&fs, "nas", nas_extra, true);
        mac.look();
        nas.look();
        fs.lock().unwrap().write(&footage(rel), br#"{"tags":["sun"],"pick":"reject"}"#);
        mac.note_now("f", rel);
        nas.told("f", "Day000");
        for _ in 0..3 { mac.look_own(); nas.look(); }
        mac.cat.refresh().unwrap();
        assert_eq!(mac.meta("f", rel).and_then(|m| m["own"].get("pick").cloned()), Some(json!("reject")), "NAS +{nas_extra} ns");
    }
}

// --- Changes made in Finder or by a script (files only, nothing noted) ---

/// Finder, with the NAS's watcher running: a clip moved to another folder with its own file, one renamed, one deleted,
/// a whole folder moved. Each is seen as soon as the watcher is told, and the moved clip's own file keeps its tags at
/// its new place.
#[test]
fn finder_moves_renames_and_deletes_are_seen_where_the_nas_watches() {
    let fs = Fs::new();
    project(&fs, 3, 4, 0);
    fs.lock().unwrap().write(&footage("Day000/C0000.afp.json"), br#"{"tags":["reef"]}"#);
    let mut nas = Pc::new(&fs, "nas", 0, false);
    nas.helper_start();
    {
        let mut f = fs.lock().unwrap();
        f.rename(&footage("Day000/C0000.mp4"), &footage("Sorted/C0000.mp4"));
        f.rename(&footage("Day000/C0000.afp.json"), &footage("Sorted/C0000.afp.json"));
        f.rename(&footage("Day000/C0001.mp4"), &footage("Day000/Dive one.mp4"));
        f.remove(&footage("Day000/C0002.mp4"));
        f.rename(&footage("Day002"), &footage("2026 Egypt/Day002"));
    }
    for d in ["Day000", "Sorted", "", "2026 Egypt"] { nas.told("f", d); }
    assert_eq!(nas.meta("f", "Sorted/C0000.afp.json"), Some(json!({ "own": { "tags": ["reef"] } })));
    assert!(!nas.has("f", "Day000/C0000.afp.json") && !nas.has("f", "Day000/C0000.mp4") && nas.has("f", "Sorted/C0000.mp4"));
    assert!(nas.has("f", "Day000/Dive one.mp4") && !nas.has("f", "Day000/C0001.mp4"));
    assert!(!nas.has("f", "Day000/C0002.mp4"));
    assert!(nas.has("f", "2026 Egypt/Day002/C0003.mp4") && !nas.has("f", "Day002/C0003.mp4"));
}

/// The same in Finder with no watcher anywhere (a USB disk, the Helper not installed): the Library's look at
/// everything by folders' times sees all of it (moves, renames and deletions change folders' times).
#[test]
fn finder_changes_are_seen_by_the_librarys_look_without_a_watcher() {
    let fs = Fs::new();
    project(&fs, 3, 4, 0);
    fs.lock().unwrap().write(&footage("Day000/C0000.afp.json"), br#"{"tags":["reef"]}"#);
    let mut mac = Pc::new(&fs, "mac", 0, false);
    assert!(mac.look_after().is_some(), "nobody has looked after them yet: looked at");
    {
        let mut f = fs.lock().unwrap();
        f.rename(&footage("Day000/C0000.mp4"), &footage("Sorted/C0000.mp4"));
        f.rename(&footage("Day000/C0000.afp.json"), &footage("Sorted/C0000.afp.json"));
        f.remove(&footage("Day000/C0002.mp4"));
        f.rename(&footage("Day002"), &footage("2026 Egypt/Day002"));
    }
    mac.look();
    assert_eq!(mac.meta("f", "Sorted/C0000.afp.json"), Some(json!({ "own": { "tags": ["reef"] } })));
    assert!(!mac.has("f", "Day000/C0000.mp4") && mac.has("f", "Sorted/C0000.mp4"));
    assert!(!mac.has("f", "Day000/C0002.mp4") && mac.has("f", "2026 Egypt/Day002/C0003.mp4") && !mac.has("f", "Day002/C0003.mp4"));
}

/// A script that rewrites clips' own files in place (Python's open('w'): the folder's time stays) with nothing
/// watching: not seen by the looks that go by folders' times, seen by the next deep look — on demand at once, else
/// within a day, by whoever is about first.
#[test]
fn a_script_editing_in_place_is_seen_within_a_day_or_at_once_on_demand() {
    let fs = Fs::new();
    project(&fs, 2, 4, 0);
    let rel = "Day001/C0002.afp.json";
    fs.lock().unwrap().write(&footage(rel), br#"{"tags":[]}"#);
    let mut mac = Pc::new(&fs, "mac", 0, false);
    mac.look_after();
    fs.lock().unwrap().write(&footage(rel), br#"{"tags":["edited by a script"]}"#);
    mac.look();
    mac.look_own();
    assert!(mac.look_after().is_none(), "looked after today already: nothing listed");
    assert_eq!(mac.meta("f", rel), Some(json!({ "own": { "tags": [] } })), "not seen yet: its folder's time is as it was");
    // A day later, the Library's hourly look after
    later(&fs, 24 * 60 * MINUTE);
    let r = mac.look_after().expect("a day without anyone looking: looked at deep");
    assert!(r.listed >= 3 && r.inspected == 1, "every folder listed, only the changed file read: {r:?}");
    assert_eq!(mac.meta("f", rel), Some(json!({ "own": { "tags": ["edited by a script"] } })));
    // …or at once: Checks' "Look at every folder again" (afp-catalog look --deep)
    fs.lock().unwrap().write(&footage(rel), br#"{"tags":["again"]}"#);
    assert_eq!(mac.deep().inspected, 1);
    assert_eq!(mac.meta("f", rel), Some(json!({ "own": { "tags": ["again"] } })));
}

/// A deep look by one computer spares the others that day: a second Mac's Library opening looks at nothing.
#[test]
fn one_computers_deep_look_spares_the_others() {
    let fs = Fs::new();
    project(&fs, 3, 4, 2);
    let mut a = Pc::new(&fs, "mac-a", 0, false);
    let mut b = Pc::new(&fs, "mac-b", 0, false);
    a.look_after().expect("looked at");
    later(&fs, 23 * 60 * MINUTE);
    assert!(b.look_after().is_none(), "looked after by the other Mac within the day");
    later(&fs, 2 * 60 * MINUTE);
    assert!(b.look_after().is_some(), "over a day: this one looks");
    assert!(a.look_after().is_none(), "…and spares the first");
}

/// The NAS's watcher vouches for its roots while it runs, however long, with no deep look of its own: the Library
/// never looks deep then. Stopped, it stops vouching within 15 minutes, and the Library looks.
#[test]
fn a_watching_helper_spares_the_library_and_a_stopped_one_doesnt() {
    let fs = Fs::new();
    project(&fs, 2, 4, 2);
    let mut nas = Pc::new(&fs, "nas", 23, false);
    let mut mac = Pc::new(&fs, "mac", 0, false);
    nas.helper_start();
    // Two days of it watching, a line every 5 minutes; the Library opened now and then
    for i in 0..(2 * 24 * 12) {
        later(&fs, 5 * MINUTE);
        nas.beat();
        if i % 50 == 0 { assert!(mac.look_after().is_none(), "watched: nothing for the Library to look at ({i})"); }
    }
    // The Helper stops (the NAS off, its container stopped)
    later(&fs, 20 * MINUTE);
    assert!(mac.look_after().is_some(), "no longer watched: the Library looks");
}

/// The Helper starting again lists every folder: what was changed in place while it was down is seen.
#[test]
fn the_helper_at_its_start_sees_what_changed_while_it_was_down() {
    let fs = Fs::new();
    project(&fs, 2, 4, 0);
    let rel = "Day000/C0001.afp.json";
    fs.lock().unwrap().write(&footage(rel), br#"{"tags":[]}"#);
    let mut nas = Pc::new(&fs, "nas", 23, false);
    nas.helper_start();
    // Down; meanwhile a script edits in place
    fs.lock().unwrap().write(&footage(rel), br#"{"tags":["while it was down"]}"#);
    nas.look();
    assert_eq!(nas.meta("f", rel), Some(json!({ "own": { "tags": [] } })), "a look by folders' times doesn't see it");
    nas.helper_start();
    assert_eq!(nas.meta("f", rel), Some(json!({ "own": { "tags": ["while it was down"] } })));
}

/// A clip's own file written wrong (not JSON: a script's bug, or caught half-written) keeps its record as it was —
/// the tags aren't wiped — and is said; once it reads right it's noted, even written in place, by the next look of
/// its folder (which is listed again until then).
#[test]
fn a_broken_own_file_keeps_its_record_until_it_reads_right() {
    let fs = Fs::new();
    project(&fs, 2, 4, 0);
    let rel = "Day000/C0003.afp.json";
    fs.lock().unwrap().write(&footage(rel), br#"{"tags":["kept"]}"#);
    let mut nas = Pc::new(&fs, "nas", 0, false);
    nas.helper_start();
    fs.lock().unwrap().write(&footage(rel), br#"{"tags":["half"#);
    let r = nas.told("f", "Day000");
    assert_eq!(r.unreadable, vec![("f".to_string(), rel.to_string())]);
    assert_eq!(nas.meta("f", rel), Some(json!({ "own": { "tags": ["kept"] } })), "its record kept");
    // Still broken: said again, still kept
    assert_eq!(nas.look().unreadable.len(), 1);
    // Written right, in place (its folder's time stays): the next look reads it
    fs.lock().unwrap().write(&footage(rel), br#"{"tags":["kept","fixed"]}"#);
    let r = nas.look();
    assert!(r.unreadable.is_empty());
    assert_eq!(nas.meta("f", rel), Some(json!({ "own": { "tags": ["kept", "fixed"] } })));
    assert_eq!(nas.look().changes, 0, "and settled");
}

/// A moment deleted in Finder (into the NAS's #recycle) goes from the catalog at the Library's regular look of the
/// project's folder; put back, it's read again as new.
#[test]
fn a_moment_deleted_and_put_back_in_finder() {
    let fs = Fs::new();
    project(&fs, 2, 4, 2);
    let rel = "Generated/2026-11-01 Moments/Day000/C0000 m.mp4";
    let mut mac = Pc::new(&fs, "mac", 0, false);
    mac.look();
    assert!(mac.has("p", rel));
    let bin = Path::new(PROJECT).join("#recycle/C0000 m.mp4");
    fs.lock().unwrap().rename(&Path::new(PROJECT).join(rel), &bin);
    mac.look_own();
    assert!(!mac.has("p", rel), "deleted: gone from the catalog");
    fs.lock().unwrap().rename(&bin, &Path::new(PROJECT).join(rel));
    mac.look_own();
    assert_eq!(mac.meta("p", rel), read_meta(&moment_bytes("h0x0", "Day000/C0000.mp4", 0)));
}

// --- Out of what the looks can see, on demand: the check against the files ---

/// A file whose insides change while its size and time stay (a restore from a backup that keeps times) is out of what
/// the looks can see — the deep look lists every folder and reads nothing. The check against the files (Checks' "Read
/// every file again", afp-catalog check) reads every file of ours, corrects the record, says which; a second check
/// finds nothing and writes nothing.
#[test]
fn the_check_against_the_files_corrects_a_restore_that_kept_times() {
    let fs = Fs::new();
    project(&fs, 2, 4, 2);
    let mut a = Pc::new(&fs, "a", 0, false);
    a.look();
    let rel = "Generated/2026-11-01 Moments/Day000/C0000 m.mp4";
    let path = Path::new(PROJECT).join(rel);
    let new = moment_bytes("hzz9", "Day000/C0009.mp4", 0);
    {
        // Restored from a backup, times kept: put in by a rename (its folder's time changes), same size, same time
        let mut f = fs.lock().unwrap();
        let old = f.nodes[&path].clone();
        assert_eq!(old.data.len(), new.len());
        let tmp = path.with_file_name("restore.tmp");
        f.write(&tmp, &new);
        f.nodes.get_mut(&tmp).unwrap().mtime = old.mtime;
        f.remove(&path);
        f.rename(&tmp, &path);
    }
    a.look();
    assert_eq!(a.deep().inspected, 0, "the looks, deep too, read only files whose size or time changed");
    assert_ne!(a.meta("p", rel), read_meta(&new), "so the restore isn't seen by them");
    let c = a.check();
    assert_eq!(c.corrected, vec![("p".to_string(), rel.to_string())]);
    assert_eq!(a.meta("p", rel), read_meta(&new));
    let c = a.check();
    assert!(c.corrected.is_empty() && c.changes == 0 && c.inspected > 0, "nothing to correct, nothing written: {c:?}");
}

// --- Limits kept ---

/// A disk whose times are coarse (exFAT 10 ms, FAT 2 s) as a location: a file added to a folder in the same tick as
/// the folder's last listing leaves its time as recorded, and only a deep look lists it — within a day, by the looks
/// after (cards are taken in, not kept as locations: this is unlikely).
#[test]
fn a_coarse_folder_time_hides_a_new_file_until_a_deep_look() {
    #[derive(Clone)]
    struct Coarse(SimStore);
    impl Coarse { fn fix(s: Stat) -> Stat { Stat { mtime: s.mtime - s.mtime.rem_euclid(10_000_000), ..s } } }
    impl Store for Coarse {
        fn stat(&self, p: &Path) -> io::Result<Option<Stat>> { Ok(self.0.stat(p)?.map(Coarse::fix)) }
        fn list(&self, p: &Path) -> io::Result<Vec<Item>> { Ok(self.0.list(p)?.into_iter().map(|i| Item { stat: Coarse::fix(i.stat), ..i }).collect()) }
        fn read(&self, p: &Path) -> io::Result<Vec<u8>> { self.0.read(p) }
        fn read_from(&self, p: &Path, o: u64) -> io::Result<Vec<u8>> { self.0.read_from(p, o) }
        fn append(&self, p: &Path, d: &[u8]) -> io::Result<()> { self.0.append(p, d) }
        fn put(&self, p: &Path, d: &[u8]) -> io::Result<()> { self.0.put(p, d) }
        fn remove(&self, p: &Path) -> io::Result<()> { self.0.remove(p) }
        fn mkdirs(&self, p: &Path) -> io::Result<()> { self.0.mkdirs(p) }
        fn lock(&self, p: &Path) -> io::Result<Box<dyn std::any::Any>> { self.0.lock(p) }
    }
    let fs = Fs::new();
    project(&fs, 2, 4, 0);
    let store: Arc<dyn Store> = Arc::new(Coarse(SimStore { fs: fs.clone(), net: Net::new(3) }));
    let mut cat = Catalog::new(store.clone(), Arc::new(SimStore { fs: Fs::new(), net: Net::new(4) }), Path::new(PROJECT), Path::new("/c"), "a");
    cat.recheck_after = std::time::Duration::ZERO;
    cat.load(&Want::All).unwrap();
    let rd = Reader { store: store.clone(), old: false };
    let look = |cat: &mut Catalog, deep: bool| cat.sweep(&roots(), &rd, &Sweep { deep, ..Default::default() }).unwrap();
    // Start the folder's coarse tick afresh, then: listed (and listed again, the recheck), then a clip added
    { let mut f = fs.lock().unwrap(); f.tick = (f.tick / 10_000_000 + 1) * 10_000_000; }
    fs.lock().unwrap().write(&footage("Day000/NEW1.mp4"), b"x");
    look(&mut cat, false);
    look(&mut cat, false);
    fs.lock().unwrap().write(&footage("Day000/NEW2.mp4"), b"y");
    for _ in 0..3 { look(&mut cat, false); }
    assert!(cat.state.get("f", "Day000/NEW1.mp4").is_some());
    assert!(cat.state.get("f", "Day000/NEW2.mp4").is_none(), "not seen by the looks by folders' times");
    look(&mut cat, true);
    assert!(cat.state.get("f", "Day000/NEW2.mp4").is_some(), "the deep look sees it");
}

/// The version gate is asked at each write, not once: a catalog already open (the NAS's watcher running) stops
/// writing the moment the project is raised past what it knows, and a look's note is refused too.
#[test]
fn the_version_gate_holds_for_a_catalog_already_open() {
    use afp_catalog::DiskStore;
    let dir: PathBuf = std::env::temp_dir().join(format!("afp-gate-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("_Moments")).unwrap();
    let pf = dir.join("Project.afproject");
    let write_version = |v: u64| std::fs::write(&pf, json!({ "format": "actionfootage-project", "version": v }).to_string()).unwrap();
    write_version(afp_catalog::project::VERSION);
    let mut cat = Catalog::new(Arc::new(DiskStore), Arc::new(DiskStore), &dir, &dir.join("cache"), "nas");
    cat.load(&Want::All).unwrap();
    let rec = |n: &str| vec![("p".to_string(), format!("_Moments/{n}"), Some(File { dir: false, size: 1, mtime: 1, meta: None, digest: None }))];
    cat.note(rec("a.mp4")).unwrap();
    // Another program, newer, raises the project
    write_version(afp_catalog::project::VERSION + 1);
    assert!(matches!(cat.note(rec("b.mp4")), Err(afp_catalog::Error::TooNew(_))));
    std::fs::write(dir.join("_Moments/c.mp4"), b"c").unwrap();
    let root = Root { id: "p".into(), dir: dir.clone(), marker: Some("Project.afproject".into()), exclude: vec![] };
    let rd = Reader { store: Arc::new(DiskStore), old: true };
    assert!(matches!(cat.sweep(&[root], &rd, &Sweep::default()), Err(afp_catalog::Error::TooNew(_))));
    assert!(matches!(cat.compact(), Err(afp_catalog::Error::TooNew(_))));
    let _ = std::fs::remove_dir_all(&dir);
}

/// The real disk: a clip's own file that isn't JSON is an error for the catalog, not a clip without tags
#[test]
fn a_sidecar_that_isnt_json_is_an_error_on_the_real_disk() {
    let dir: PathBuf = std::env::temp_dir().join(format!("afp-bad-own-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let f = dir.join("A.afp.json");
    std::fs::write(&f, br#"{"tags":["x""#).unwrap();
    assert_eq!(afp_catalog::inspect::read(&f).unwrap_err().kind(), io::ErrorKind::InvalidData);
    std::fs::write(&f, br#"{"tags":["x"]}"#).unwrap();
    assert_eq!(afp_catalog::inspect::read(&f).unwrap().unwrap()["own"]["tags"], json!(["x"]));
    let _ = std::fs::remove_dir_all(&dir);
}
