// How the catalog stays true to the files, and where it doesn't (docs/catalog-safeguards.md). Most of these tests
// document how the catalog behaves TODAY: each asserts a hole that is there, so that the report's evidence is
// re-runnable, and each says which assertion to turn round once the safeguard it argues for is built. The last one
// checks the project's version gate on a real folder.

mod common;
use afp_catalog::sim::{ Fs, Net, SimStore };
use afp_catalog::state::File;
use afp_catalog::store::{ Item, Stat };
use afp_catalog::{ Catalog, Inspect, Root, Store, Sweep, Want };
use common::*;
use serde_json::{ json, Value };
use std::io;
use std::path::{ Path, PathBuf };
use std::sync::{ Arc, Mutex };

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
/// anywhere, kept whole. `old`: a program from before "pick" (Accept / Reject) was kept, which drops it (the NAS
/// Helper of 2026-11-17)
struct Reader { store: Arc<dyn Store>, old: bool }

impl Inspect for Reader {
    fn skip(&self, _r: &str, _rel: &str, name: &str, _d: bool) -> bool { name.starts_with('.') || name.starts_with('#') }
    fn wants(&self, root: &str, rel: &str) -> bool { (root == "p" && rel.ends_with(".mp4")) || rel.ends_with(".afp.json") }
    fn inspect(&self, path: &Path, _r: &str, rel: &str) -> io::Result<Option<Value>> {
        let b = self.store.read(path)?;
        if rel.ends_with(".mp4") { return Ok(read_meta(&b)); }
        let Ok(mut own) = serde_json::from_slice::<Value>(&b) else { return Ok(None) };
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
    /// The Library's regular look and its daily deep look: the project's own folder only (seen.rs, project::own_roots)
    fn look_own(&mut self, deep: bool) -> afp_catalog::Report {
        let r = self.reader();
        let own: Vec<Root> = roots().into_iter().filter(|r| r.id == "p").collect();
        self.cat.sweep(&own, &r, &Sweep { deep, ..Default::default() }).unwrap()
    }
    fn deep(&mut self) -> afp_catalog::Report { let r = self.reader(); self.cat.sweep(&roots(), &r, &Sweep { deep: true, ..Default::default() }).unwrap() }
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
}

fn moments(fs: &Arc<Mutex<Fs>>) -> usize {
    fs.lock().unwrap().files_under(Path::new(PROJECT)).iter().filter(|p| p.to_string_lossy().ends_with(".mp4")).count()
}

/// Two computers that see the same files' times at different precisions re-read and re-note every file of ours at
/// every look, for ever: each look finds the other's records "changed". With one precision the second look reads
/// nothing. (Safeguard: one canonical precision for times in records; then turn `assert!(… > 0)` into `== 0`.)
#[test]
fn today_two_precisions_reread_everything_for_ever() {
    for (nas_extra, settles) in [(0, true), (23, false)] {
        let fs = Fs::new();
        project(&fs, 6, 10, 2);
        let n = moments(&fs);
        let mut mac = Pc::new(&fs, "mac", 0, false);
        let mut nas = Pc::new(&fs, "nas", nas_extra, false);
        mac.look();
        nas.look();
        for round in 0..4 {
            let (m, s) = (mac.look(), nas.look());
            if settles {
                assert_eq!((m.inspected, s.inspected, m.changes, s.changes), (0, 0, 0, 0), "round {round}");
            } else {
                // Every moment read again by both, and every folder's and file's record written again
                assert_eq!((m.inspected, s.inspected), (n, n), "round {round}");
                assert!(m.changes > n && s.changes > n, "round {round}: {} / {}", m.changes, s.changes);
            }
        }
    }
}

/// The 2026-11-17 channel: the Library writes a clip's own file and notes it; the NAS's watcher, told of the write,
/// lists the folder and — only because it sees the time with more digits — reads the file again and notes it again
/// (with whatever its program keeps). With one precision it notes nothing of the file.
#[test]
fn today_the_nas_renotes_each_file_a_mac_just_noted() {
    for (nas_extra, renoted) in [(0, false), (23, true)] {
        let fs = Fs::new();
        project(&fs, 2, 4, 0);
        let rel = "Day000/C0000.afp.json";
        fs.lock().unwrap().write(&Path::new(FOOTAGE).join(rel), br#"{"tags":["sun"]}"#);
        let mut mac = Pc::new(&fs, "mac", 0, false);
        let mut nas = Pc::new(&fs, "nas", nas_extra, false);
        mac.look();
        nas.look();
        fs.lock().unwrap().write(&Path::new(FOOTAGE).join(rel), br#"{"tags":["sun"],"pick":"reject"}"#);
        mac.note_now("f", rel);
        let r = nas.told("f", "Day000");
        assert_eq!(r.inspected, renoted as usize, "the NAS read the file again: {renoted}");
    }
}

/// …and a program that keeps less than the file holds then wipes the field from every view, for good where no
/// computer looks again: the Library's regular and daily deep looks are of the project's own folder, not of the
/// footage's locations where clips' own files are. With one precision the older program never touches the record.
/// (Today project::VERSION refuses such a program and own_kept carries unknown fields; the hole shown here is the
/// mechanism, which any future inspection difference would use again.)
#[test]
fn today_an_older_program_wipes_a_field_and_nothing_heals_it() {
    for nas_extra in [0, 23] {
        let fs = Fs::new();
        project(&fs, 2, 4, 0);
        let rel = "Day000/C0000.afp.json";
        let file = Path::new(FOOTAGE).join(rel);
        fs.lock().unwrap().write(&file, br#"{"tags":["sun"]}"#);
        let mut mac = Pc::new(&fs, "mac", 0, false);
        let mut nas = Pc::new(&fs, "nas", nas_extra, true);
        mac.look();
        nas.look();
        fs.lock().unwrap().write(&file, br#"{"tags":["sun"],"pick":"reject"}"#);
        mac.note_now("f", rel);
        nas.told("f", "Day000");
        // The Library's looks, regular and deep, many times; the NAS's every-30-minutes look too
        for _ in 0..3 { mac.look_own(false); mac.look_own(true); nas.look(); }
        mac.cat.refresh().unwrap();
        let pick = mac.meta("f", rel).and_then(|m| m["own"].get("pick").cloned());
        if nas_extra == 0 {
            assert_eq!(pick, Some(json!("reject")), "one precision: the older program never re-read it");
        } else {
            assert_eq!(pick, None, "today: wiped, and no look brings it back (the file still has it)");
            // What would: reading the file's insides again although its size and time are as on record
            assert!(String::from_utf8(fs.lock().unwrap().nodes[&file].data.clone()).unwrap().contains("reject"));
        }
    }
}

/// A file whose insides change while its size and time stay (a copy or a restore that keeps times, `touch -r`, a
/// rewrite within a coarse disk's tick) is never read again: not by a look, not by the deep look (it lists every
/// folder but reads only files whose size or time differ). (Safeguard: a periodic re-reading of contents; then turn
/// the last assertion round.)
#[test]
fn today_a_change_keeping_size_and_time_is_never_seen() {
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
    let r = a.look();
    assert!(r.listed >= 1, "its folder was listed (its time changed)");
    let d = a.deep();
    assert_eq!(d.inspected, 0, "the deep look lists every folder but reads nothing");
    assert!(d.listed > 5);
    assert_ne!(a.meta("p", rel), read_meta(&new), "today: the catalog keeps the old insides for good");
}

/// A file of a location written in place by something not ours (its folder's time stays) when the NAS's watcher
/// doesn't hear it (down, or the change made before it started): the NAS's 30-minute look and the Library's looks go
/// by folders' times and don't list it; only a deep look at the locations would, and no program does one. (Safeguard:
/// the periodic re-reading covering the locations' own files.)
#[test]
fn today_an_in_place_change_in_a_location_waits_for_ever_without_inotify() {
    let fs = Fs::new();
    project(&fs, 2, 4, 0);
    let rel = "Day001/C0002.afp.json";
    let file = Path::new(FOOTAGE).join(rel);
    fs.lock().unwrap().write(&file, br#"{"tags":[]}"#);
    let mut mac = Pc::new(&fs, "mac", 0, false);
    let mut nas = Pc::new(&fs, "nas", 0, false);
    mac.look();
    nas.look();
    fs.lock().unwrap().write(&file, br#"{"tags":["edited in a text editor"]}"#);
    for _ in 0..3 { nas.look(); mac.look_own(false); mac.look_own(true); }
    mac.cat.refresh().unwrap();
    assert_eq!(mac.meta("f", rel), Some(json!({ "own": { "tags": [] } })), "today: not seen");
    // A deep look over the locations would see it (its size and time changed)
    assert_eq!(nas.deep().inspected, 1);
    mac.cat.refresh().unwrap();
    assert_eq!(mac.meta("f", rel), Some(json!({ "own": { "tags": ["edited in a text editor"] } })));
}

/// A disk whose times are coarse (exFAT 10 ms, FAT 2 s) as a location: a file added to a folder in the same tick as
/// the folder's last listing leaves its time as recorded, and no look but a deep one lists it.
#[test]
fn today_a_coarse_folder_time_hides_a_new_file_until_a_deep_look() {
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
    fs.lock().unwrap().write(&Path::new(FOOTAGE).join("Day000/NEW1.mp4"), b"x");
    look(&mut cat, false);
    look(&mut cat, false);
    fs.lock().unwrap().write(&Path::new(FOOTAGE).join("Day000/NEW2.mp4"), b"y");
    for _ in 0..3 { look(&mut cat, false); }
    assert!(cat.state.get("f", "Day000/NEW1.mp4").is_some());
    assert!(cat.state.get("f", "Day000/NEW2.mp4").is_none(), "today: not seen by the looks");
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
