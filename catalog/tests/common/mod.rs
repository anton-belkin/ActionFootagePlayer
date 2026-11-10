// A project on a simulated NAS seen by several computers, and what the catalog should say about it
#![allow(dead_code)]

use afp_catalog::sim::{ Fs, Net, SimStore };
use afp_catalog::{ Catalog, Inspect, Root, Store };
use serde_json::{ json, Value };
use std::collections::BTreeMap;
use std::io;
use std::path::{ Path, PathBuf };
use std::sync::{ Arc, Mutex };

pub const PROJECT: &str = "/nas/proj";
pub const FOOTAGE: &str = "/nas/footage";

pub fn roots() -> Vec<Root> {
    vec![Root { id: "p".into(), dir: PathBuf::from(PROJECT), marker: Some("Project.afproject".into()), exclude: vec![] },
         Root { id: "f".into(), dir: PathBuf::from(FOOTAGE), marker: Some("footage.afplocation".into()), exclude: vec![] }]
}

pub struct Rng(pub u64);
impl Rng {
    pub fn next(&mut self) -> u64 { let mut x = self.0; x ^= x << 13; x ^= x >> 7; x ^= x << 17; self.0 = x; x }
    pub fn below(&mut self, n: u64) -> u64 { if n == 0 { 0 } else { self.next() % n } }
    pub fn pick<'a, T>(&mut self, v: &'a [T]) -> Option<&'a T> { if v.is_empty() { None } else { Some(&v[self.below(v.len() as u64) as usize]) } }
}

/// What's inside a moment: its link, as JSON (a real one is an MP4 with the link in a key)
pub fn moment_bytes(hash: &str, clip: &str, n: u64) -> Vec<u8> { json!({ "kind": "moment", "hash": hash, "clip": clip, "in": n }).to_string().into_bytes() }

pub fn read_meta(bytes: &[u8]) -> Option<Value> { serde_json::from_slice::<Value>(bytes).ok().map(|l| json!({ "link": l })) }

pub struct Insp { pub store: Arc<dyn Store> }

impl Inspect for Insp {
    fn skip(&self, _root: &str, _rel: &str, name: &str, _dir: bool) -> bool { name.starts_with('.') || name.starts_with('#') }
    fn wants(&self, root: &str, rel: &str) -> bool { root == "p" && rel.ends_with(".mp4") }
    fn inspect(&self, path: &Path, _root: &str, _rel: &str) -> io::Result<Option<Value>> { Ok(read_meta(&self.store.read(path)?)) }
}

pub struct Computer {
    pub name: String,
    pub net: Arc<Mutex<Net>>,
    pub shared: SimStore,
    pub local: SimStore,
    pub cat: Catalog,
}

impl Computer {
    pub fn new(fs: &Arc<Mutex<Fs>>, name: &str, seed: u64) -> Computer {
        let net = Net::new(seed);
        let shared = SimStore { fs: fs.clone(), net: net.clone() };
        let local = SimStore { fs: Fs::new(), net: Net::new(seed ^ 0x5555) };
        let mut cat = Catalog::new(Arc::new(shared.clone()), Arc::new(local.clone()), Path::new(PROJECT), Path::new("/cache/proj"), name);
        // Clean-ups at once here (a test's time doesn't pass); one test gives them their 10 minutes
        cat.grace = std::time::Duration::ZERO;
        Computer { name: name.into(), net, shared, local, cat }
    }
    pub fn insp(&self) -> Insp { Insp { store: Arc::new(self.shared.clone()) } }
    /// The program started again: what it had in memory gone, its own disk's cache kept
    pub fn restart(&mut self) {
        let grace = self.cat.grace;
        self.cat = Catalog::new(Arc::new(self.shared.clone()), Arc::new(self.local.clone()), Path::new(PROJECT), Path::new("/cache/proj"), &self.name);
        self.cat.grace = grace;
    }
    pub fn sweep(&mut self) -> Result<afp_catalog::Report, afp_catalog::Error> { let i = self.insp(); self.cat.sweep(&roots(), &i, &Default::default()) }
    pub fn deep(&mut self) -> Result<afp_catalog::Report, afp_catalog::Error> {
        let i = self.insp();
        self.cat.sweep(&roots(), &i, &afp_catalog::Sweep { deep: true, ..Default::default() })
    }
    pub fn counts(&self) -> afp_catalog::sim::Counts { self.net.lock().unwrap().counts.clone() }
    pub fn reset_counts(&self) { self.net.lock().unwrap().counts = Default::default(); }
    pub fn set(&self, fail: f64, lag: u32, down: bool) {
        let mut n = self.net.lock().unwrap();
        n.fail = fail; n.lag = lag; n.down = down;
        if lag == 0 { n.forget(); }
    }
}

/// (root, path) -> (folder, size, modified, meta) for files, folders with only their presence
pub type View = BTreeMap<(String, String), (bool, u64, i64, Option<Value>)>;

pub fn truth(fs: &Arc<Mutex<Fs>>) -> View {
    let fs = fs.lock().unwrap();
    let mut out = View::new();
    for r in roots() {
        if !fs.exists(&r.dir) { continue; }
        for (p, n) in fs.nodes.range(r.dir.clone()..).take_while(|(k, _)| k.starts_with(&r.dir)) {
            let rel = p.strip_prefix(&r.dir).unwrap().to_string_lossy().into_owned();
            if rel.is_empty() || rel.split('/').any(|c| c.starts_with('.') || c.starts_with('#')) { continue; }
            let meta = if r.id == "p" && rel.ends_with(".mp4") && !n.dir { read_meta(&n.data) } else { None };
            out.insert((r.id.clone(), rel), (n.dir, if n.dir { 0 } else { n.data.len() as u64 }, if n.dir { 0 } else { n.mtime }, meta));
        }
    }
    out
}

pub fn view(cat: &Catalog) -> View {
    cat.state.live().filter(|(_, rel, _)| !rel.is_empty())
        .map(|(root, rel, f)| ((root.to_string(), rel.to_string()), (f.dir, f.size, if f.dir { 0 } else { f.mtime }, if f.dir { None } else { f.meta.clone() }))).collect()
}

/// The differences, said briefly
pub fn diff(a: &View, b: &View) -> Vec<String> {
    let mut out = Vec::new();
    for (k, v) in a { match b.get(k) { None => out.push(format!("missing {k:?}")), Some(w) if w != v => out.push(format!("differs {k:?}: {v:?} vs {w:?}")), _ => {} } }
    for k in b.keys() { if !a.contains_key(k) { out.push(format!("extra {k:?}")); } }
    out
}

/// A project: `days` folders of `per` clips, a moment for every `every`th clip
pub fn project(fs: &Arc<Mutex<Fs>>, days: usize, per: usize, every: usize) {
    let mut f = fs.lock().unwrap();
    f.write(&Path::new(PROJECT).join("Project.afproject"), b"{\"format\":\"actionfootage-project\"}");
    f.write(&Path::new(FOOTAGE).join("footage.afplocation"), b"{}");
    for d in 0..days {
        for c in 0..per {
            let clip = format!("Day{d:03}/C{c:04}.mp4");
            f.write(&Path::new(FOOTAGE).join(&clip), format!("clip {d} {c} ......").as_bytes());
            if every > 0 && c % every == 0 {
                let m = format!("Generated/2026-11-{:02} Moments/Day{d:03}/C{c:04} m.mp4", 1 + d % 28);
                f.write(&Path::new(PROJECT).join(&m), &moment_bytes(&format!("h{d}x{c}"), &clip, c as u64));
            }
        }
    }
}
