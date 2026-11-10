// SPDX-License-Identifier: GPL-3.0-or-later
//
// A simulated disk for tests: one set of files (the NAS) seen by several computers, each through its own connection,
// which can lose operations (a write cut off midway, a file written but the answer lost, a temporary file left), go
// down altogether, and show folder listings and file sizes some operations old (as macOS's SMB client caches them).
// It counts what each computer asked for, to price it at a network's speed.
//
// Folder times act as on a real disk: a folder's time changes when a file or folder is made, removed or renamed in
// it, not when a file in it is written to, not when something deeper changes; a folder moved keeps its time.

use crate::store::{ Item, Stat, Store };
use std::collections::{ BTreeMap, HashMap };
use std::io;
use std::path::{ Path, PathBuf };
use std::sync::{ Arc, Mutex };

#[derive(Clone, Debug)]
pub struct Node { pub dir: bool, pub data: Vec<u8>, pub mtime: i64 }

#[derive(Default)]
pub struct Fs { pub nodes: BTreeMap<PathBuf, Node>, pub tick: i64 }

impl Fs {
    pub fn new() -> Arc<Mutex<Fs>> {
        let mut fs = Fs::default();
        fs.nodes.insert(PathBuf::from("/"), Node { dir: true, data: Vec::new(), mtime: 0 });
        Arc::new(Mutex::new(fs))
    }
    fn tick(&mut self) -> i64 { self.tick += 1000; self.tick }
    fn touch_parent(&mut self, p: &Path) {
        let t = self.tick();
        if let Some(d) = p.parent() { if let Some(n) = self.nodes.get_mut(d) { n.mtime = t; } }
    }
    pub fn mkdirs(&mut self, p: &Path) {
        let mut cur = PathBuf::from("/");
        for c in p.components().skip(1) {
            cur.push(c);
            if !self.nodes.contains_key(&cur) {
                let t = self.tick();
                self.nodes.insert(cur.clone(), Node { dir: true, data: Vec::new(), mtime: t });
                self.touch_parent(&cur);
            }
        }
    }
    /// A file written whole (made, or written over in place)
    pub fn write(&mut self, p: &Path, data: &[u8]) {
        if let Some(d) = p.parent() { self.mkdirs(d); }
        let t = self.tick();
        let new = !self.nodes.contains_key(p);
        self.nodes.insert(p.to_path_buf(), Node { dir: false, data: data.to_vec(), mtime: t });
        if new { self.touch_parent(p); }
    }
    pub fn append(&mut self, p: &Path, data: &[u8]) {
        if let Some(d) = p.parent() { self.mkdirs(d); }
        let t = self.tick();
        let new = !self.nodes.contains_key(p);
        let n = self.nodes.entry(p.to_path_buf()).or_insert(Node { dir: false, data: Vec::new(), mtime: t });
        n.data.extend_from_slice(data);
        n.mtime = t;
        if new { self.touch_parent(p); }
    }
    fn subtree(&self, p: &Path) -> Vec<PathBuf> {
        self.nodes.range(p.to_path_buf()..).take_while(|(k, _)| k.starts_with(p)).map(|(k, _)| k.clone()).collect()
    }
    pub fn remove(&mut self, p: &Path) -> bool {
        let all = self.subtree(p);
        if all.is_empty() { return false; }
        for k in all { self.nodes.remove(&k); }
        self.touch_parent(p);
        true
    }
    /// Moved (Finder): what's moved keeps its times; the two folders' change
    pub fn rename(&mut self, from: &Path, to: &Path) -> bool {
        if !self.nodes.contains_key(from) || self.nodes.contains_key(to) { return false; }
        if let Some(d) = to.parent() { self.mkdirs(d); }
        let all = self.subtree(from);
        for k in all {
            let n = self.nodes.remove(&k).unwrap();
            let rest = k.strip_prefix(from).unwrap();
            let dest = if rest.as_os_str().is_empty() { to.to_path_buf() } else { to.join(rest) };
            self.nodes.insert(dest, n);
        }
        self.touch_parent(from);
        self.touch_parent(to);
        true
    }
    /// Copied (Finder): files keep their times, folders made get new ones
    pub fn copy(&mut self, from: &Path, to: &Path) {
        for k in self.subtree(from) {
            let n = self.nodes[&k].clone();
            let rest = k.strip_prefix(from).unwrap();
            let dest = if rest.as_os_str().is_empty() { to.to_path_buf() } else { to.join(rest) };
            if n.dir { self.mkdirs(&dest); } else {
                if let Some(d) = dest.parent() { self.mkdirs(d); }
                let new = !self.nodes.contains_key(&dest);
                self.nodes.insert(dest.clone(), n);
                if new { self.touch_parent(&dest); }
            }
        }
    }
    pub fn exists(&self, p: &Path) -> bool { self.nodes.contains_key(p) }
    pub fn files_under(&self, p: &Path) -> Vec<PathBuf> {
        self.subtree(p).into_iter().filter(|k| k != p && !self.nodes[k].dir).collect()
    }
    fn list(&self, p: &Path) -> Option<Vec<Item>> {
        if !self.nodes.get(p)?.dir { return None; }
        Some(self.nodes.range(p.to_path_buf()..).skip(1).take_while(|(k, _)| k.starts_with(p)).filter(|(k, _)| k.parent() == Some(p))
            .map(|(k, n)| Item { name: k.file_name().unwrap().to_string_lossy().into_owned(), stat: Stat { dir: n.dir, size: n.data.len() as u64, mtime: n.mtime } })
            .collect())
    }
}

#[derive(Default, Debug, Clone)]
pub struct Counts {
    pub stats: u64, pub lists: u64, pub listed: u64, pub reads: u64, pub read_bytes: u64, pub writes: u64, pub written_bytes: u64,
    pub removes: u64, pub failures: u64,
}

impl Counts {
    pub fn round_trips(&self) -> u64 { self.stats + self.lists * 3 + self.reads * 3 + self.writes * 4 + self.removes * 2 }
    /// Seconds at a round trip (ms) and a speed (MB/s): a listing's entries, reads and writes counted in bytes
    pub fn seconds(&self, rtt_ms: f64, mb_s: f64) -> f64 {
        self.round_trips() as f64 * rtt_ms / 1000.0 + (self.read_bytes + self.written_bytes + self.listed * 120) as f64 / (mb_s * 1e6)
    }
}

pub struct Net {
    pub rng: u64,
    /// Chance an operation fails (0..1)
    pub fail: f64,
    /// Chance a write fails (on top of `fail`)
    pub wfail: f64,
    pub down: bool,
    /// Files this computer's connection can't read (an SMB client's "Invalid argument" for one file)
    pub bad: Vec<PathBuf>,
    /// Operations a listing or a size stays as it was seen
    pub lag: u32,
    cache: HashMap<PathBuf, (Option<Vec<Item>>, u32)>,
    stat_cache: HashMap<PathBuf, (Option<Stat>, u32)>,
    pub counts: Counts,
}

impl Net {
    pub fn new(seed: u64) -> Arc<Mutex<Net>> {
        Arc::new(Mutex::new(Net { rng: seed.max(1), fail: 0.0, wfail: 0.0, down: false, bad: Vec::new(), lag: 0, cache: HashMap::new(), stat_cache: HashMap::new(), counts: Counts::default() }))
    }
    pub fn rand(&mut self) -> u64 { let mut x = self.rng; x ^= x << 13; x ^= x >> 7; x ^= x << 17; self.rng = x; x }
    pub fn chance(&mut self, p: f64) -> bool { p > 0.0 && (self.rand() % 1_000_000) as f64 / 1e6 < p }
    fn fails(&mut self) -> bool {
        let f = self.down || { let p = self.fail; self.chance(p) };
        if f { self.counts.failures += 1; }
        f
    }
    pub fn forget(&mut self) { self.cache.clear(); self.stat_cache.clear(); }
    /// Sizes and times seen fresh, listings still as they were (what a client caching listings longer does)
    pub fn forget_stats(&mut self) { self.stat_cache.clear(); }
}

fn lost() -> io::Error { io::Error::new(io::ErrorKind::ConnectionAborted, "connection lost") }

/// One computer's connection to the simulated files
#[derive(Clone)]
pub struct SimStore { pub fs: Arc<Mutex<Fs>>, pub net: Arc<Mutex<Net>> }

impl Store for SimStore {
    fn stat(&self, p: &Path) -> io::Result<Option<Stat>> {
        let mut net = self.net.lock().unwrap();
        net.counts.stats += 1;
        if net.fails() { return Err(lost()); }
        if let Some((s, left)) = net.stat_cache.get_mut(p) { if *left > 0 { *left -= 1; return Ok(s.clone()); } }
        let fs = self.fs.lock().unwrap();
        let s = fs.nodes.get(p).map(|n| Stat { dir: n.dir, size: if n.dir { 0 } else { n.data.len() as u64 }, mtime: n.mtime });
        let lag = net.lag;
        if lag > 0 { net.stat_cache.insert(p.to_path_buf(), (s.clone(), lag)); }
        Ok(s)
    }
    fn list(&self, p: &Path) -> io::Result<Vec<Item>> {
        let mut net = self.net.lock().unwrap();
        net.counts.lists += 1;
        if net.fails() { return Err(lost()); }
        let cached = match net.cache.get_mut(p) { Some((l, left)) if *left > 0 => { *left -= 1; Some(l.clone()) } _ => None };
        let l = match cached { Some(l) => l, None => {
            let l = self.fs.lock().unwrap().list(p);
            let lag = net.lag;
            if lag > 0 { net.cache.insert(p.to_path_buf(), (l.clone(), lag)); }
            l
        } };
        match l {
            Some(l) => { net.counts.listed += l.len() as u64; Ok(l) }
            None => Err(io::Error::new(io::ErrorKind::NotFound, "no such folder")),
        }
    }
    fn read(&self, p: &Path) -> io::Result<Vec<u8>> { self.read_from(p, 0) }
    fn read_from(&self, p: &Path, offset: u64) -> io::Result<Vec<u8>> {
        let mut net = self.net.lock().unwrap();
        net.counts.reads += 1;
        if net.fails() { return Err(lost()); }
        if net.bad.iter().any(|b| b == p) { return Err(io::Error::from_raw_os_error(22)); }
        let fs = self.fs.lock().unwrap();
        match fs.nodes.get(p) {
            Some(n) if !n.dir => { let b = n.data.get(offset as usize..).unwrap_or(&[]).to_vec(); net.counts.read_bytes += b.len() as u64; Ok(b) }
            _ => Err(io::Error::new(io::ErrorKind::NotFound, "no such file")),
        }
    }
    fn append(&self, p: &Path, data: &[u8]) -> io::Result<()> {
        let mut net = self.net.lock().unwrap();
        net.counts.writes += 1;
        net.counts.written_bytes += data.len() as u64;
        if net.down { net.counts.failures += 1; return Err(lost()); }
        if net.fails() || { let p = net.wfail; net.chance(p) } {
            // Cut off: some of it written, or all of it with the answer lost
            let cut = (net.rand() % (data.len() as u64 + 1)) as usize;
            self.fs.lock().unwrap().append(p, &data[..cut]);
            return Err(lost());
        }
        self.fs.lock().unwrap().append(p, data);
        Ok(())
    }
    fn put(&self, p: &Path, data: &[u8]) -> io::Result<()> {
        let mut net = self.net.lock().unwrap();
        net.counts.writes += 1;
        net.counts.written_bytes += data.len() as u64;
        if net.down { net.counts.failures += 1; return Err(lost()); }
        let tmp = crate::store::temp_name(p);
        if net.fails() || { let p = net.wfail; net.chance(p) } {
            match net.rand() % 3 {
                0 => {}
                1 => { let cut = (net.rand() % (data.len() as u64 + 1)) as usize; self.fs.lock().unwrap().write(&tmp, &data[..cut]); }
                _ => { let mut fs = self.fs.lock().unwrap(); fs.write(&tmp, data); fs.rename(&tmp, p); }
            }
            return Err(lost());
        }
        let mut fs = self.fs.lock().unwrap();
        if fs.exists(p) { fs.remove(p); }
        fs.write(&tmp, data);
        fs.rename(&tmp, p);
        Ok(())
    }
    fn remove(&self, p: &Path) -> io::Result<()> {
        let mut net = self.net.lock().unwrap();
        net.counts.removes += 1;
        if net.fails() {
            if net.rand() % 2 == 0 { self.fs.lock().unwrap().remove(p); }
            return Err(lost());
        }
        self.fs.lock().unwrap().remove(p);
        Ok(())
    }
    fn mkdirs(&self, p: &Path) -> io::Result<()> {
        let mut net = self.net.lock().unwrap();
        if net.fails() { return Err(lost()); }
        self.fs.lock().unwrap().mkdirs(p);
        Ok(())
    }
    fn lock(&self, _p: &Path) -> io::Result<Box<dyn std::any::Any>> { Ok(Box::new(())) }
}
