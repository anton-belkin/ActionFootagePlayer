// SPDX-License-Identifier: GPL-3.0-or-later
//
// What the catalog knows: for each file and folder seen (a root — the project's folder, or a location — and its path
// in it), what it is (size, modified, what's inside it that counts, for a folder what was in it when it was last
// looked at), or that it's gone; and which observation that is (a version).
//
// Versions are Lamport clocks: a computer gives what it writes a number above every one it has read, the computer's
// name breaking ties. A record replaces another only with a higher version. So whatever order the journals are read
// in, whoever reads them, the same records win: every computer that has read the same ends up knowing the same.

use serde_json::{ json, Value };
use sha2::{ Digest, Sha256 };
use std::collections::{ BTreeMap, HashMap };
use std::sync::Arc;

pub type Id = Arc<str>;

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Version {
    pub l: u64,
    pub by: Id,
}

#[derive(Clone, Debug, PartialEq)]
pub struct File {
    pub dir: bool,
    pub size: u64,
    /// As the disk has it (nanoseconds); for a folder, its time when it was listed (-1: to be listed again)
    pub mtime: i64,
    /// What's inside it that counts (a moment's link, a sidecar's marks and tags…): the inspector's
    pub meta: Option<Value>,
    /// A folder: what was in it when it was listed (names, and the files' sizes and times), hashed
    pub digest: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Rec {
    pub v: Version,
    /// None: gone
    pub f: Option<File>,
}

/// A record's key: root, a zero byte, path in the root ("" is the root itself). Sorted, a folder's files follow it
pub fn key(root: &str, rel: &str) -> String { format!("{root}\0{rel}") }

pub fn split(k: &str) -> (&str, &str) { k.split_once('\0').unwrap_or((k, "")) }

pub fn parent(rel: &str) -> &str { rel.rsplit_once('/').map(|(d, _)| d).unwrap_or("") }

pub fn name(rel: &str) -> &str { rel.rsplit_once('/').map(|(_, n)| n).unwrap_or(rel) }

pub fn join(dir: &str, name: &str) -> String { if dir.is_empty() { name.to_string() } else { format!("{dir}/{name}") } }

#[derive(Default, Clone)]
pub struct State {
    pub recs: BTreeMap<String, Rec>,
    ids: HashMap<String, Id>,
}

/// A folder's digest, from what's in it: (name, folder, size, modified)
pub fn digest<'a>(children: impl Iterator<Item = (&'a str, &'a File)>) -> String {
    let mut lines: Vec<String> = children.map(|(n, f)| if f.dir { format!("{n}/") } else { format!("{n}\t{}\t{}", f.size, f.mtime) }).collect();
    lines.sort();
    let mut h = Sha256::new();
    for l in &lines { h.update(l.as_bytes()); h.update(b"\n"); }
    hex(&h.finalize()[..8])
}

pub fn hex(b: &[u8]) -> String { b.iter().map(|x| format!("{x:02x}")).collect() }

impl State {
    pub fn id(&mut self, s: &str) -> Id {
        if let Some(i) = self.ids.get(s) { return i.clone(); }
        let i: Id = Arc::from(s);
        self.ids.insert(s.to_string(), i.clone());
        i
    }

    /// The record replaces what's there only when its version is higher: true when it did
    pub fn apply(&mut self, k: String, r: Rec) -> bool {
        match self.recs.get(&k) {
            Some(old) if old.v >= r.v => false,
            _ => { self.recs.insert(k, r); true }
        }
    }

    pub fn get(&self, root: &str, rel: &str) -> Option<&File> { self.recs.get(&key(root, rel)).and_then(|r| r.f.as_ref()) }

    pub fn rec(&self, root: &str, rel: &str) -> Option<&Rec> { self.recs.get(&key(root, rel)) }

    /// Everything under a folder (records, gone ones too), the folder itself left out
    pub fn under<'a>(&'a self, root: &str, dir: &str) -> impl Iterator<Item = (&'a str, &'a Rec)> + 'a {
        let prefix = if dir.is_empty() { format!("{root}\0") } else { format!("{root}\0{dir}/") };
        let start = prefix.clone();
        self.recs.range(start..).take_while(move |(k, _)| k.starts_with(&prefix)).filter_map(|(k, r)| {
            let (_, rel) = split(k);
            (!rel.is_empty()).then_some((rel, r))
        })
    }

    /// What's in a folder now (not what's in its folders)
    pub fn children<'a>(&'a self, root: &str, dir: &str) -> Vec<(&'a str, &'a File)> {
        let d = dir.to_string();
        self.under(root, dir).filter(|(rel, _)| parent(rel) == d).filter_map(|(rel, r)| r.f.as_ref().map(|f| (name(rel), f))).collect()
    }

    /// Every root's folders and what's in each (now): built once for a look at all of them
    pub fn children_index(&self, root: &str) -> HashMap<String, Vec<(String, File)>> {
        let mut out: HashMap<String, Vec<(String, File)>> = HashMap::new();
        let mut dirs: std::collections::HashSet<String> = std::collections::HashSet::new();
        for (rel, r) in self.under(root, "") {
            if let Some(f) = &r.f {
                out.entry(parent(rel).to_string()).or_default().push((name(rel).to_string(), f.clone()));
                if f.dir { dirs.insert(rel.to_string()); }
            }
        }
        // A file noted in a folder that was never listed (the player's moment in a folder made since): the folder is
        // there for the look, to be listed (and its file found gone when the folder is)
        let mut implied: Vec<String> = Vec::new();
        for (rel, r) in self.under(root, "") {
            if r.f.is_none() { continue; }
            let mut p = parent(rel);
            while !p.is_empty() && !dirs.contains(p) {
                dirs.insert(p.to_string());
                implied.push(p.to_string());
                p = parent(p);
            }
        }
        for p in implied {
            out.entry(parent(&p).to_string()).or_default().push((name(&p).to_string(), File { dir: true, size: 0, mtime: -1, meta: None, digest: None }));
        }
        out
    }

    pub fn live(&self) -> impl Iterator<Item = (&str, &str, &File)> {
        self.recs.iter().filter_map(|(k, r)| r.f.as_ref().map(|f| { let (root, rel) = split(k); (root, rel, f) }))
    }

    pub fn len_live(&self) -> usize { self.recs.values().filter(|r| r.f.is_some()).count() }
}

// ---------- as written: in the journals' lines and the snapshots' parts ----------

pub fn entry_json(root: &str, rel: &str, f: Option<&File>) -> Value {
    let mut v = json!({ "r": root, "p": rel });
    match f {
        None => { v["g"] = json!(1); }
        Some(f) => {
            if f.dir { v["d"] = json!(1); } else { v["s"] = json!(f.size); }
            v["m"] = json!(f.mtime);
            if let Some(x) = &f.meta { v["x"] = x.clone(); }
            if let Some(h) = &f.digest { v["h"] = json!(h); }
        }
    }
    v
}

pub fn entry_from(v: &Value) -> Option<(String, String, Option<File>)> {
    let root = v.get("r")?.as_str()?.to_string();
    let rel = v.get("p")?.as_str()?.to_string();
    if v.get("g").is_some() { return Some((root, rel, None)); }
    let dir = v.get("d").is_some();
    Some((root, rel, Some(File {
        dir,
        size: v.get("s").and_then(Value::as_u64).unwrap_or(0),
        mtime: v.get("m")?.as_i64()?,
        meta: v.get("x").cloned(),
        digest: v.get("h").and_then(Value::as_str).map(str::to_string),
    })))
}
