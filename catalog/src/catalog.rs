// SPDX-License-Identifier: GPL-3.0-or-later
//
// A project's catalog, as one computer has it: read from the newest snapshot and the journals' lines after it,
// written to by appending to this computer's journal, folded into a new snapshot from time to time (the Library only).
// docs/library-management.md §9.

use crate::journal::{ self, segment_name, segment_number };
use crate::snapshot::{ self, bits_for, bucket, bucket_key, decode_part, encode_part, name_parts, part_name, Manifest };
use crate::state::{ digest, join, key, File, Rec, State, Version };
use crate::store::{ Item, Store };
use serde_json::{ json, Value };
use std::collections::{ BTreeMap, BTreeSet, HashMap, HashSet };
use std::io;
use std::path::{ Path, PathBuf };
use std::sync::Arc;

/// A segment grows to this, then the next is started
pub const ROTATE: u64 = 1 << 20;
/// Snapshots kept: older ones (and parts only they had, segments only they needed) go, two clean-ups after
pub const KEEP: usize = 3;

#[derive(Debug)]
pub enum Error {
    /// The disk couldn't be read or written (a lost connection): nothing was changed
    Io(io::Error),
    /// Lines this computer hasn't read were removed: a newer snapshot has them (read again from it)
    Gap(String),
    /// No snapshot could be read whole
    Damaged(String),
}

impl From<io::Error> for Error { fn from(e: io::Error) -> Self { Error::Io(e) } }

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self { Error::Io(e) => write!(f, "{e}"), Error::Gap(s) => write!(f, "gap: {s}"), Error::Damaged(s) => write!(f, "damaged: {s}") }
    }
}

/// What to read of the snapshot: all of it (the Library), or the parts these keys are in (the player: a clip's)
#[derive(Clone, Debug)]
pub enum Want { All, Keys(Vec<String>) }

/// What a load found
#[derive(Debug, Default, Clone)]
pub struct Loaded {
    pub snapshot: Option<String>,
    /// Snapshots passed over (damaged, or parts missing)
    pub passed: usize,
    pub parts_read: usize,
    pub parts_cached: usize,
    pub tail_lines: usize,
    /// Nothing there yet: a catalog to build
    pub empty: bool,
}

/// What this computer knows of its own journal (kept on its own disk)
#[derive(Debug, Default, Clone)]
struct Writer { seg: u64, size: u64, seq: u64, l: u64, broken: bool }

pub struct Catalog {
    pub shared: Arc<dyn Store>,
    pub local: Arc<dyn Store>,
    /// <project>/.afp/catalog
    pub dir: PathBuf,
    /// This computer's cache for this project (parts and snapshots read, its journal's state)
    pub cache: PathBuf,
    pub me: String,
    pub state: State,
    pub manifest: Option<(String, Manifest)>,
    /// Each computer's last line read
    pub applied: BTreeMap<String, u64>,
    /// Each computer's highest version read
    pub lamports: BTreeMap<String, u64>,
    pub lamport: u64,
    /// Lines read since the snapshot (when to fold them into a new one)
    pub tail_lines: usize,
    pub tail_bytes: u64,
    pub partial: bool,
    offsets: HashMap<PathBuf, u64>,
    /// Records read from the journals that change what there is (a file, its insides, a folder there or gone), not
    /// only what's known of a folder's listing: for the Library to read its views again only then
    pub seen_changes: u64,
    /// How long a file must have been not needed before a clean-up removes it (10 minutes)
    pub grace: std::time::Duration,
    /// Journal files that couldn't be read at the last reading (their computer's later lines wait)
    pub unreadable: Vec<String>,
    /// Seconds added to this computer's clock (tests: time passing)
    pub clock_ahead: u64,
    /// How long after a folder was listed because it changed this computer lists it once more (60 s): the Mac gives a
    /// share's folder's new time with a listing from its cache up to ~30 s old (seen on the NAS), and only a listing
    /// made after that is sure. Kept on this computer's disk (recheck.json): it's its cache that was behind
    pub recheck_after: std::time::Duration,
}

fn not_found(e: &io::Error) -> bool { e.kind() == io::ErrorKind::NotFound }

impl Catalog {
    pub fn new(shared: Arc<dyn Store>, local: Arc<dyn Store>, project: &Path, cache: &Path, me: &str) -> Catalog {
        Catalog { shared, local, dir: project.join(".afp").join("catalog"), cache: cache.to_path_buf(), me: me.to_string(), state: State::default(),
                  manifest: None, applied: BTreeMap::new(), lamports: BTreeMap::new(), lamport: 0, tail_lines: 0, tail_bytes: 0, partial: false,
                  offsets: HashMap::new(), grace: std::time::Duration::from_secs(600), clock_ahead: 0, seen_changes: 0, unreadable: Vec::new(),
                  recheck_after: std::time::Duration::from_secs(60) }
    }

    fn reset(&mut self) {
        self.state = State::default();
        self.manifest = None;
        self.applied.clear();
        self.lamports.clear();
        self.lamport = 0;
        self.tail_lines = 0;
        self.tail_bytes = 0;
        self.offsets.clear();
    }

    /// A file of the snapshot (immutable, named by its content): from this computer's cache, else from the share
    /// (kept in the cache). Checked either way. (bytes, whether it came from the cache)
    fn immutable(&self, sub: &str, name: &str, check: &dyn Fn(&[u8]) -> bool) -> io::Result<(Vec<u8>, bool)> {
        let cached = self.cache.join(sub).join(name);
        if let Ok(b) = self.local.read(&cached) {
            if check(&b) { return Ok((b, true)); }
            let _ = self.local.remove(&cached);
        }
        let b = self.shared.read(&self.dir.join(sub).join(name))?;
        if !check(&b) { return Err(io::Error::new(io::ErrorKind::InvalidData, format!("{sub}/{name} damaged"))); }
        if self.local.mkdirs(&self.cache.join(sub)).is_ok() { let _ = self.local.put(&cached, &b); }
        Ok((b, false))
    }

    pub fn snapshots(&self) -> io::Result<Vec<String>> {
        let mut names: Vec<String> = match self.shared.list(&self.dir.join("snap")) {
            Ok(l) => l.into_iter().map(|i| i.name).filter(|n| name_parts(n).is_some()).collect(),
            Err(e) if not_found(&e) => Vec::new(),
            Err(e) => return Err(e),
        };
        names.sort_by(|a, b| b.cmp(a));
        Ok(names)
    }

    fn read_manifest(&self, name: &str) -> io::Result<Manifest> {
        let (b, _) = self.immutable("snap", name, &|b| Manifest::from_bytes(name, b).is_ok())?;
        Manifest::from_bytes(name, &b).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
    }

    /// Read: the newest snapshot that reads whole (its parts that are wanted), then the journals after it
    pub fn load(&mut self, want: &Want) -> Result<Loaded, Error> {
        let mut info = Loaded::default();
        let names = self.snapshots()?;
        for name in &names {
            self.reset();
            let m = match self.read_manifest(name) {
                Ok(m) => m,
                Err(e) if e.kind() == io::ErrorKind::InvalidData || not_found(&e) => { info.passed += 1; continue }
                Err(e) => return Err(e.into()),
            };
            let wanted: Option<BTreeSet<u32>> = match want {
                Want::All => None,
                Want::Keys(keys) => Some(keys.iter().map(|k| bucket(k, m.bits)).collect()),
            };
            let mut ok = true;
            for (i, part) in m.parts.iter().enumerate() {
                if wanted.as_ref().is_some_and(|w| !w.contains(&(i as u32))) { continue; }
                match self.immutable("parts", part, &|b| part_name(b) == *part) {
                    Ok((b, cached)) => {
                        if cached { info.parts_cached += 1 } else { info.parts_read += 1 }
                        if decode_part(part, &b, &mut self.state).is_err() { ok = false; break; }
                    }
                    Err(e) if e.kind() == io::ErrorKind::InvalidData || not_found(&e) => { ok = false; break }
                    Err(e) => return Err(e.into()),
                }
            }
            if !ok { info.passed += 1; continue; }
            self.applied = m.covers.clone();
            self.lamports = m.lamports.clone();
            // Each journal read on from where the snapshot's reading ended (lines it has aren't read again)
            for (c, (seg, off)) in &m.at {
                self.offsets.insert(self.dir.join("journal").join(c).join(segment_name(*seg)), *off);
            }
            self.lamport = m.l;
            self.partial = wanted.is_some();
            self.manifest = Some((name.clone(), m));
            info.snapshot = Some(name.clone());
            info.tail_lines = self.read_tails()?;
            return Ok(info);
        }
        if !names.is_empty() { return Err(Error::Damaged(format!("none of {} snapshots reads whole", names.len()))); }
        // No snapshot yet: the journals from their first lines
        self.reset();
        self.partial = false;
        info.tail_lines = self.read_tails()?;
        info.empty = info.tail_lines == 0;
        Ok(info)
    }

    /// The journals' lines not read yet: how many
    pub fn refresh(&mut self) -> Result<usize, Error> { self.read_tails() }

    /// A record read from a journal into what's known, counted when it changes what there is
    fn take(&mut self, root: &str, rel: &str, l: u64, by: &crate::state::Id, f: Option<File>) {
        let k = key(root, rel);
        let before = self.state.recs.get(&k).map(|r| r.f.as_ref().map(|f| (f.dir, f.size, f.mtime, f.meta.clone())));
        let visible = |x: &Option<File>| x.as_ref().map(|f| if f.dir { (true, 0, 0, None) } else { (false, f.size, f.mtime, f.meta.clone()) });
        let now = visible(&f);
        if self.state.apply(k, Rec { v: Version { l, by: by.clone() }, f }) {
            let was = before.map(|b| b.map(|(d, s, m, x)| if d { (true, 0, 0, None) } else { (d, s, m, x) })).flatten();
            if was != now { self.seen_changes += 1; }
        }
    }

    fn read_tails(&mut self) -> Result<usize, Error> {
        self.unreadable.clear();
        let jdir = self.dir.join("journal");
        let computers: Vec<String> = match self.shared.list(&jdir) {
            Ok(l) => l.into_iter().filter(|i| i.stat.dir && !i.name.starts_with('.')).map(|i| i.name).collect(),
            Err(e) if not_found(&e) => return Ok(0),
            Err(e) => return Err(e.into()),
        };
        let mut n = 0;
        for c in computers {
            let cdir = jdir.join(&c);
            let mut segs: Vec<u64> = match self.shared.list(&cdir) {
                Ok(l) => l.into_iter().filter_map(|i| segment_number(&i.name)).collect(),
                Err(e) if not_found(&e) => continue,
                Err(e) => return Err(e.into()),
            };
            segs.sort();
            let Some(&first) = segs.first() else { continue };
            let done = self.applied.get(&c).copied().unwrap_or(0);
            if first > done + 1 { return Err(Error::Gap(format!("{c}'s lines {}..{} are gone", done + 1, first - 1))); }
            let by = self.state.id(&c);
            let mut stuck = false;
            for (i, seg) in segs.iter().enumerate() {
                let done = self.applied.get(&c).copied().unwrap_or(0);
                if segs.get(i + 1).is_some_and(|next| *next <= done + 1) { continue; }
                let path = cdir.join(segment_name(*seg));
                let off = self.offsets.get(&path).copied().unwrap_or(0);
                let bytes = match self.shared.read_from(&path, off) {
                    Ok(b) => b,
                    // Removed since it was listed: a newer snapshot has its lines
                    Err(e) if not_found(&e) => return Err(Error::Gap(format!("{c}'s segment {seg} went while read"))),
                    // One file that can't be read now (seen on the NAS: the Mac's SMB client answering "Invalid
                    // argument" for a file another computer had just written, a healthy file there): this computer's
                    // lines from here on wait for the next read; the others' are read
                    Err(e) => { self.unreadable.push(format!("{}: {e}", path.display())); stuck = true; break }
                };
                let p = journal::parse(&bytes);
                for line in p.lines {
                    let last = self.applied.get(&c).copied().unwrap_or(0);
                    if line.seq <= last { continue; }
                    for (root, rel, f) in line.entries { self.take(&root, &rel, line.l, &by, f); }
                    self.applied.insert(c.clone(), line.seq);
                    let ml = self.lamports.entry(c.clone()).or_insert(0);
                    *ml = (*ml).max(line.l);
                    self.lamport = self.lamport.max(line.l);
                    self.tail_lines += 1;
                    n += 1;
                }
                self.tail_bytes += p.consumed as u64;
                self.offsets.insert(path, off + p.consumed as u64);
            }
            if stuck { continue; }
            // A segment started since the folder was listed: looked for by the name it would have (macOS keeps a
            // share's listings some 30 s; a file opened by its name is read as it is now)
            loop {
                let next = self.applied.get(&c).copied().unwrap_or(0) + 1;
                if segs.contains(&next) { break; }
                let path = cdir.join(segment_name(next));
                let Ok(bytes) = self.shared.read_from(&path, 0) else { break };
                segs.push(next);
                let p = journal::parse(&bytes);
                let before = self.applied.get(&c).copied().unwrap_or(0);
                for line in p.lines {
                    let last = self.applied.get(&c).copied().unwrap_or(0);
                    if line.seq <= last { continue; }
                    for (root, rel, f) in line.entries { self.take(&root, &rel, line.l, &by, f); }
                    self.applied.insert(c.clone(), line.seq);
                    let ml = self.lamports.entry(c.clone()).or_insert(0);
                    *ml = (*ml).max(line.l);
                    self.lamport = self.lamport.max(line.l);
                    self.tail_lines += 1;
                    n += 1;
                }
                self.tail_bytes += p.consumed as u64;
                self.offsets.insert(path, p.consumed as u64);
                if self.applied.get(&c).copied().unwrap_or(0) == before { break; }
            }
        }
        Ok(n)
    }

    // ---------- writing: this computer's journal ----------

    fn writer_file(&self) -> PathBuf { self.cache.join("writer.json") }

    fn read_writer(&self) -> Option<Writer> {
        let v: Value = serde_json::from_slice(&self.local.read(&self.writer_file()).ok()?).ok()?;
        Some(Writer { seg: v["seg"].as_u64()?, size: v["size"].as_u64()?, seq: v["seq"].as_u64()?, l: v["l"].as_u64()?, broken: v["broken"].as_bool().unwrap_or(false) })
    }

    fn save_writer(&self, w: &Writer) {
        let _ = self.local.mkdirs(&self.cache);
        let _ = self.local.put(&self.writer_file(), json!({ "seg": w.seg, "size": w.size, "seq": w.seq, "l": w.l, "broken": w.broken }).to_string().as_bytes());
    }

    /// This computer's journal as it is on the share, when what's kept here doesn't match it (or isn't there)
    fn recover(&self, kept: Option<Writer>) -> io::Result<(Writer, bool)> {
        let cdir = self.dir.join("journal").join(&self.me);
        let mut segs: Vec<u64> = match self.shared.list(&cdir) {
            Ok(l) => l.into_iter().filter_map(|i| segment_number(&i.name)).collect(),
            Err(e) if not_found(&e) => Vec::new(),
            Err(e) => return Err(e),
        };
        segs.sort();
        let mut w = kept.unwrap_or_default();
        w.l = w.l.max(*self.lamports.get(&self.me).unwrap_or(&0));
        w.seq = w.seq.max(*self.applied.get(&self.me).unwrap_or(&0));
        let Some(&last) = segs.last() else { w.seg = 0; w.size = 0; return Ok((w, true)) };
        let bytes = self.shared.read(&cdir.join(segment_name(last)))?;
        let p = journal::parse(&bytes);
        w.seq = w.seq.max(last.saturating_sub(1)).max(p.lines.iter().map(|l| l.seq).max().unwrap_or(0));
        w.l = w.l.max(p.lines.iter().map(|l| l.l).max().unwrap_or(0));
        w.seg = last;
        w.size = bytes.len() as u64;
        let fresh = p.torn || p.bad > 0 || p.computer.as_deref() != Some(self.me.as_str());
        Ok((w, fresh))
    }

    /// Records appended to this computer's journal as one line, then into what's known here. Err: not written
    /// (perhaps written: the next line won't be in the same segment, and a line read twice counts once)
    pub fn note(&mut self, entries: Vec<(String, String, Option<File>)>) -> Result<(), Error> {
        if entries.is_empty() { return Ok(()); }
        // One record a key in a line, the last said (a line's records all have its version)
        let mut seen = HashSet::new();
        let mut entries: Vec<_> = entries.into_iter().rev().filter(|(r, p, _)| seen.insert(key(r, p))).collect();
        entries.reverse();
        // What's on record read first: this line's version above every one written before it that can be read now
        // (a computer that hasn't read for a while would otherwise write below newer records, and lose to them)
        match self.read_tails() { Ok(_) | Err(Error::Gap(_)) => {} Err(e) => return Err(e) }
        let _ = self.local.mkdirs(&self.cache);
        let _guard = self.local.lock(&self.cache.join("writer.lock"))?;
        let cdir = self.dir.join("journal").join(&self.me);
        let kept = self.read_writer();
        let (mut w, mut fresh) = match &kept {
            Some(k) if !k.broken && k.seg > 0 => match self.shared.stat(&cdir.join(segment_name(k.seg)))? {
                Some(s) if s.size == k.size => (k.clone(), false),
                _ => self.recover(kept.clone())?,
            },
            _ => self.recover(kept.clone())?,
        };
        let seq = w.seq + 1;
        let l = self.lamport.max(w.l) + 1;
        let covered = self.manifest.as_ref().and_then(|(_, m)| m.covers.get(&self.me)).is_some_and(|c| *c >= w.seq);
        if w.size >= ROTATE || (covered && w.size > 64 * 1024) { fresh = true; }
        let mut data = Vec::new();
        if fresh { w.seg = seq; w.size = 0; data.extend(journal::header(&self.me, seq)); }
        data.extend(journal::line(seq, l, &entries));
        if fresh { self.shared.mkdirs(&cdir)?; }
        let r = self.shared.append(&cdir.join(segment_name(w.seg)), &data);
        w.seq = seq;
        w.l = l;
        match r {
            Err(e) => { w.broken = true; self.save_writer(&w); Err(e.into()) }
            Ok(()) => {
                w.size += data.len() as u64;
                w.broken = false;
                self.save_writer(&w);
                let by = self.state.id(&self.me);
                for (root, rel, f) in entries { self.state.apply(key(&root, &rel), Rec { v: Version { l, by: by.clone() }, f }); }
                self.lamport = l;
                let ml = self.lamports.entry(self.me.clone()).or_insert(0);
                *ml = (*ml).max(l);
                if self.applied.get(&self.me).copied().unwrap_or(0) + 1 == seq { self.applied.insert(self.me.clone(), seq); }
                Ok(())
            }
        }
    }

    fn clock(&self) -> u64 { std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0) + self.clock_ahead }

    fn rechecks(&self) -> Vec<(String, String, u64)> {
        self.local.read(&self.cache.join("recheck.json")).ok().and_then(|b| serde_json::from_slice::<Vec<(String, String, u64)>>(&b).ok()).unwrap_or_default()
    }

    fn save_rechecks(&self, list: &[(String, String, u64)]) {
        let _ = self.local.mkdirs(&self.cache);
        let _ = self.local.put(&self.cache.join("recheck.json"), serde_json::to_string(list).unwrap().as_bytes());
    }

    /// Folders this computer is to list once more now (their time has come)
    pub fn due_rechecks(&self) -> Vec<(String, String)> {
        let t = self.clock();
        self.rechecks().into_iter().filter(|(_, _, due)| *due <= t).map(|(r, d, _)| (r, d)).collect()
    }

    /// A folder's time changed (a file made and removed in it): what was written into a file in place (a link, tags)
    /// leaves its folder's time as it was, and the next look wouldn't list it again
    pub fn touch(&self, dir: &Path) -> io::Result<()> {
        let mark = crate::store::temp_name(&dir.join("touch"));
        self.shared.put(&mark, b"")?;
        self.shared.remove(&mark)
    }

    // ---------- folding into a snapshot (the Library) ----------

    /// Lines read since the snapshot and their bytes: when to fold
    pub fn should_compact(&self) -> bool { self.tail_lines >= 2000 || self.tail_bytes >= 2 * ROTATE }

    /// A new snapshot of everything read (the whole catalog must have been loaded)
    pub fn compact(&mut self) -> Result<String, Error> {
        if self.partial { return Err(Error::Damaged("only part of the catalog is loaded".into())); }
        self.refresh()?;
        // Gone records every computer has written past can't be outdone by a line still to come: left out
        let min_l = self.lamports.values().copied().min().unwrap_or(0);
        let keep: Vec<(&String, &Rec)> = self.state.recs.iter().filter(|(_, r)| r.f.is_some() || r.v.l >= min_l).collect();
        let bits = bits_for(keep.len());
        let mut buckets: Vec<Vec<(&String, &Rec)>> = vec![Vec::new(); 1 << bits];
        for (k, r) in keep.iter().copied() {
            let (root, rel) = crate::state::split(k);
            buckets[bucket(&bucket_key(root, rel, r.f.as_ref()), bits) as usize].push((k, r));
        }
        let pdir = self.dir.join("parts");
        let there: HashSet<String> = match self.shared.list(&pdir) {
            Ok(l) => l.into_iter().map(|i| i.name).collect(),
            Err(e) if not_found(&e) => { self.shared.mkdirs(&pdir)?; HashSet::new() }
            Err(e) => return Err(e.into()),
        };
        let mut parts = Vec::new();
        for b in &buckets {
            let bytes = encode_part(b);
            let name = part_name(&bytes);
            if !there.contains(&name) { self.shared.put(&pdir.join(&name), &bytes)?; }
            if self.local.mkdirs(&self.cache.join("parts")).is_ok() { let _ = self.local.put(&self.cache.join("parts").join(&name), &bytes); }
            parts.push(name);
        }
        let gen = self.snapshots()?.iter().filter_map(|n| name_parts(n)).map(|(g, _)| g).max().unwrap_or(0) + 1;
        // Where its reading of each journal ended: the last segment it read, and how far
        let jdir = self.dir.join("journal");
        let mut at: BTreeMap<String, (u64, u64)> = BTreeMap::new();
        for (path, off) in &self.offsets {
            let (Some(c), Some(seg)) = (path.parent().and_then(|p| p.file_name()).map(|n| n.to_string_lossy().into_owned()),
                                        path.file_name().and_then(|n| segment_number(&n.to_string_lossy()))) else { continue };
            if path.parent().and_then(|p| p.parent()) != Some(jdir.as_path()) { continue; }
            if at.get(&c).is_none_or(|(s, _)| seg > *s) { at.insert(c, (seg, *off)); }
        }
        let m = Manifest { gen, by: self.me.clone(), covers: self.applied.clone(), lamports: self.lamports.clone(), l: self.lamport, bits, parts,
                           count: keep.len() as u64, at };
        let bytes = m.to_bytes();
        let name = m.name(&bytes);
        self.shared.mkdirs(&self.dir.join("snap"))?;
        self.shared.put(&self.dir.join("snap").join(&name), &bytes)?;
        // What was left out goes from here too
        let drop: Vec<String> = self.state.recs.iter().filter(|(_, r)| r.f.is_none() && r.v.l < min_l).map(|(k, _)| k.clone()).collect();
        for k in drop { self.state.recs.remove(&k); }
        self.manifest = Some((name.clone(), m));
        self.tail_lines = 0;
        self.tail_bytes = 0;
        Ok(name)
    }

    /// Clearing up: snapshots older than the newest KEEP, parts none of those has, temporary files left, this
    /// computer's segments those have read. Each only once seen at the clean-up before (a computer folding a snapshot
    /// meanwhile has its parts written before its list, and a reader on an older list is told to read again)
    pub fn clean(&mut self) -> Result<usize, Error> {
        // What's to go, and since when this computer has seen it so (its own clock, against itself only): it goes once
        // it has been so for `grace` — longer than another computer's listing can be behind (macOS: ~30 s), longer
        // than a snapshot takes to write its parts and then its list
        let state_file = self.cache.join("clean.json");
        let before: HashMap<String, u64> = self.local.read(&state_file).ok().and_then(|b| serde_json::from_slice::<HashMap<String, u64>>(&b).ok())
            .unwrap_or_default();
        let t = self.clock();
        let grace = self.grace.as_secs();
        let mut now: HashMap<String, u64> = HashMap::new();
        let mut removed = 0;
        let mut go = |shared: &Arc<dyn Store>, path: PathBuf, now: &mut HashMap<String, u64>| -> io::Result<()> {
            let k = path.display().to_string();
            match before.get(&k) {
                // A file another computer has open can't go now (SMB: busy): next time
                Some(&since) if since <= t && t - since >= grace => match shared.remove(&path) {
                    Ok(()) => removed += 1,
                    // …or one this computer may not remove (the NAS's watcher's, written as another user): left
                    Err(e) if e.kind() == io::ErrorKind::ResourceBusy || e.raw_os_error() == Some(16) || e.kind() == io::ErrorKind::PermissionDenied => { now.insert(k, since); }
                    Err(e) => return Err(e),
                },
                Some(&since) => { now.insert(k, since.min(t)); }
                None => { now.insert(k, t); }
            }
            Ok(())
        };
        let snaps = self.snapshots()?;
        let mut kept = Vec::new();
        for (i, n) in snaps.iter().enumerate() {
            if i < KEEP { kept.push(n.clone()); } else { go(&self.shared, self.dir.join("snap").join(n), &mut now)?; }
        }
        let mut manifests = Vec::new();
        for n in &kept {
            match self.read_manifest(n) { Ok(m) => manifests.push(m), Err(_) => { manifests.clear(); break } }
        }
        let tmp = |i: &Item| i.name.starts_with(".tmp-");
        for sub in ["snap", "parts"] {
            if let Ok(list) = self.shared.list(&self.dir.join(sub)) {
                for i in list.iter().filter(|i| tmp(i)) { go(&self.shared, self.dir.join(sub).join(&i.name), &mut now)?; }
            }
        }
        if manifests.len() == kept.len() && !kept.is_empty() {
            let used: HashSet<&String> = manifests.iter().flat_map(|m| m.parts.iter()).collect();
            if let Ok(list) = self.shared.list(&self.dir.join("parts")) {
                for i in list.iter().filter(|i| !tmp(i) && !used.contains(&i.name)) { go(&self.shared, self.dir.join("parts").join(&i.name), &mut now)?; }
            }
            // This computer's segments every kept snapshot has read
            let covered = manifests.iter().map(|m| m.covers.get(&self.me).copied().unwrap_or(0)).min().unwrap_or(0);
            let cdir = self.dir.join("journal").join(&self.me);
            if let Ok(list) = self.shared.list(&cdir) {
                let mut segs: Vec<u64> = list.iter().filter_map(|i| segment_number(&i.name)).collect();
                segs.sort();
                for w in segs.windows(2) {
                    if w[1] - 1 <= covered { go(&self.shared, cdir.join(segment_name(w[0])), &mut now)?; }
                }
            }
            // This computer's cache: parts no kept snapshot has
            if let Ok(list) = self.local.list(&self.cache.join("parts")) {
                for i in list.iter().filter(|i| !used.contains(&i.name)) { let _ = self.local.remove(&self.cache.join("parts").join(&i.name)); }
            }
        }
        let _ = self.local.mkdirs(&self.cache);
        let _ = self.local.put(&state_file, serde_json::to_string(&now).unwrap().as_bytes());
        Ok(removed)
    }

    // ---------- what it knows ----------

    /// The moments, clip previews, exports and photos of an original: by its fingerprint, or (links without one) its
    /// path in the project's tree
    pub fn derived(&self, hash: Option<&str>, clip: Option<&str>) -> Vec<(String, String, File)> {
        let mut out = Vec::new();
        for (root, rel, f) in self.state.live() {
            let Some(link) = f.meta.as_ref().and_then(|m| m.get("link")) else { continue };
            // By the original's path (originals don't move unless the library is mended after, which rewrites the
            // links), or its fingerprint when asked
            let lh = link.get("hash").and_then(Value::as_str).filter(|h| !h.is_empty());
            let hit = (clip.is_some() && link.get("clip").and_then(Value::as_str) == clip) || (hash.is_some() && lh == hash);
            if hit { out.push((root.to_string(), rel.to_string(), f.clone())); }
        }
        out
    }

    /// The keys to read for an original's derived files (Want::Keys)
    pub fn keys_for(hash: Option<&str>, clip: Option<&str>) -> Vec<String> {
        let mut k = Vec::new();
        if let Some(h) = hash { k.push(format!("h:{h}")); }
        if let Some(c) = clip { k.push(format!("c:{c}")); }
        k
    }

    // ---------- looking at the folders ----------

    /// Look at the roots' folders: a folder whose time or contents aren't what was written when it was last listed is
    /// listed again, and what changed in it written to the journal (records of new or changed files with what's
    /// inside them, gone ones as gone). A folder that couldn't be looked at is left as it was: an error isn't an
    /// empty folder. A root without its marker (a share not mounted, an empty folder where it was) isn't looked at
    pub fn sweep(&mut self, roots: &[Root], insp: &dyn Inspect, opts: &Sweep) -> Result<Report, Error> {
        let mut rep = Report::default();
        self.read_tails()?;
        // The catalog's own folder made first: made after the project's folder was looked at, it'd change its time
        let _ = self.shared.mkdirs(&self.dir.join("journal").join(&self.me));
        let batch = if opts.batch == 0 { 2000 } else { opts.batch };
        let mut pending: Vec<(String, String, Option<File>)> = Vec::new();
        let mut recheck_new: Vec<(String, String, u64)> = Vec::new();
        let mut recheck_done: Vec<(String, String)> = Vec::new();
        for root in roots {
            let starts: Vec<String> = if opts.only.is_empty() { vec![String::new()] } else {
                opts.only.iter().filter(|(r, _)| *r == root.id).map(|(_, d)| d.clone()).collect()
            };
            // Listed because they changed a while ago: listed again now
            let t = self.clock();
            // What's left out of this root changed (a source inside it taken out of the project): looked at whole, so
            // that what was left out comes in (its folders' times say nothing of it)
            let excl = json!(root.exclude);
            let excl_was = self.state.get(&root.id, "").and_then(|f| f.meta.as_ref()).and_then(|m| m.get("exclude").cloned()).unwrap_or(json!([]));
            let whole = excl_was != excl && self.state.get(&root.id, "").is_some();
            let due: HashSet<String> = self.rechecks().into_iter().filter(|(r, _, d)| *r == root.id && *d <= t).map(|(_, d, _)| d).collect();
            if starts.is_empty() { continue; }
            let available = matches!(self.shared.stat(&root.dir), Ok(Some(s)) if s.dir)
                && root.marker.as_ref().is_none_or(|m| matches!(self.shared.stat(&root.dir.join(m)), Ok(Some(_))));
            if !available { rep.skipped_roots.push(root.id.clone()); continue; }
            rep.roots += 1;
            let rid = root.id.clone();
            let mut idx = self.state.children_index(&rid);
            let mut stack: Vec<(String, bool)> = starts.into_iter().map(|d| (d, !opts.only.is_empty())).collect();
            while let Some((d, force)) = stack.pop() {
                rep.dirs += 1;
                if let Some(p) = opts.pause { std::thread::sleep(p); }
                let full = if d.is_empty() { root.dir.clone() } else { root.dir.join(&d) };
                let st = match self.shared.stat(&full) {
                    Ok(Some(s)) if s.dir => s,
                    Ok(_) => continue,
                    Err(_) => { rep.errors += 1; continue }
                };
                let known: Vec<(String, File)> = idx.get(&d).cloned().unwrap_or_default();
                let rec = self.state.get(&rid, &d).cloned();
                let dg = digest(known.iter().map(|(n, f)| (n.as_str(), f)));
                let differs = force || opts.deep || whole || match &rec {
                    Some(f) if f.dir => f.mtime != st.mtime || f.digest.as_deref() != Some(dg.as_str()),
                    _ => true,
                };
                let is_due = due.contains(&d);
                let changed = differs || is_due;
                if !changed {
                    for (n, f) in &known { if f.dir { stack.push((join(&d, n), false)); } }
                    continue;
                }
                let mut items = match self.shared.list(&full) {
                    Ok(i) => i,
                    Err(e) if not_found(&e) => continue,
                    Err(_) => { rep.errors += 1; continue }
                };
                rep.listed += 1;
                let keep = |i: &Item| !i.name.starts_with(".tmp-") && !insp.skip(&rid, &join(&d, &i.name), &i.name, i.stat.dir)
                    && !root.exclude.iter().any(|x| *x == join(&d, &i.name));
                items.retain(|i| keep(i));
                // A folder that had things in it, listed empty: listed again, and its root's marker looked for again
                if items.is_empty() && known.len() > 8 {
                    let again = self.shared.list(&full).map(|mut l| { l.retain(|i| keep(i)); l });
                    let marker = root.marker.as_ref().is_none_or(|m| matches!(self.shared.stat(&root.dir.join(m)), Ok(Some(_))));
                    match again { Ok(l) if marker => items = l, _ => { rep.errors += 1; continue } }
                }
                let known_map: HashMap<&str, &File> = known.iter().map(|(n, f)| (n.as_str(), f)).collect();
                let listed: HashSet<&str> = items.iter().map(|i| i.name.as_str()).collect();
                let mut out: Vec<(String, String, Option<File>)> = Vec::new();
                let mut now: Vec<(String, File)> = Vec::new();
                let mut failed = false;
                let mut gone_dirs: Vec<String> = Vec::new();
                // What's inside the new and changed files, read several at once (each a few round trips on a share)
                let to_read: Vec<&Item> = items.iter().filter(|it| !it.stat.dir && insp.wants(&rid, &join(&d, &it.name))
                    && !insp.leave(&rid, &join(&d, &it.name), false)
                    && !known_map.get(it.name.as_str()).is_some_and(|k| !k.dir && k.size == it.stat.size && k.mtime == it.stat.mtime)).collect();

                let read: HashMap<&str, io::Result<Option<Value>>> = {
                    let threads = to_read.len().clamp(1, 8);
                    let chunks: Vec<Vec<&Item>> = (0..threads).map(|t| to_read.iter().skip(t).step_by(threads).copied().collect()).collect();
                    let (rid, d, full) = (&rid, &d, &full);
                    std::thread::scope(|sc| {
                        let hs: Vec<_> = chunks.into_iter().map(|c| sc.spawn(move || c.into_iter().map(|it| (it.name.as_str(), insp.inspect(&full.join(&it.name), rid, &join(d, &it.name)))).collect::<Vec<_>>())).collect();
                        hs.into_iter().flat_map(|h| h.join().unwrap_or_default()).collect()
                    })
                };
                rep.inspected += read.len();
                let mut read = read;
                for it in &items {
                    let rel = join(&d, &it.name);
                    let old = known_map.get(it.name.as_str()).copied();
                    // Not looked at this time: as on record
                    if insp.leave(&rid, &rel, it.stat.dir) {
                        if let Some(k) = old { now.push((it.name.clone(), k.clone())); }
                        continue;
                    }
                    if it.stat.dir {
                        let f = match old {
                            Some(k) if k.dir => k.clone(),
                            _ => {
                                if old.is_some() { gone_dirs.push(rel.clone()); }
                                let f = File { dir: true, size: 0, mtime: -1, meta: None, digest: None };
                                out.push((rid.clone(), rel.clone(), Some(f.clone())));
                                f
                            }
                        };
                        now.push((it.name.clone(), f));
                        stack.push((rel, false));
                        continue;
                    }
                    if old.is_some_and(|k| k.dir) { gone_dirs.push(rel.clone()); }
                    let f = match old.filter(|k| !k.dir && k.size == it.stat.size && k.mtime == it.stat.mtime) {
                        Some(k) => k.clone(),
                        None => {
                            let meta = match read.remove(it.name.as_str()) {
                                None => None,
                                Some(Ok(m)) => m,
                                Some(Err(_)) => { failed = true; rep.errors += 1; continue }
                            };
                            let f = File { dir: false, size: it.stat.size, mtime: it.stat.mtime, meta, digest: None };
                            out.push((rid.clone(), rel.clone(), Some(f.clone())));
                            f
                        }
                    };
                    now.push((it.name.clone(), f));
                }
                for (n, k) in &known {
                    if listed.contains(n.as_str()) { continue; }
                    let rel = join(&d, n);
                    if insp.leave(&rid, &rel, k.dir) { now.push((n.clone(), k.clone())); continue; }
                    out.push((rid.clone(), rel.clone(), None));
                    if k.dir { gone_dirs.push(rel); }
                }
                for g in gone_dirs {
                    for (rel, r) in self.state.under(&rid, &g) {
                        if r.f.is_some() { out.push((rid.clone(), rel.to_string(), None)); }
                    }
                    idx.retain(|dir, _| dir != &g && !dir.starts_with(&format!("{g}/")));
                }
                // Changed: to be listed once more by this computer, a while later (not when it's that second listing,
                // nor a folder never listed before, as on a first look: nothing of it was seen to change)
                if differs && !is_due && rec.is_some() && !opts.deep { recheck_new.push((rid.clone(), d.clone(), t + self.recheck_after.as_secs())); }
                if is_due { recheck_done.push((rid.clone(), d.clone())); }
                let dir_rec = File { dir: true, size: 0, mtime: if failed { -1 } else { st.mtime },
                                     meta: (d.is_empty() && !root.exclude.is_empty()).then(|| json!({ "exclude": root.exclude })),
                                     digest: Some(digest(now.iter().map(|(n, f)| (n.as_str(), f)))) };
                if rec.as_ref() != Some(&dir_rec) { out.push((rid.clone(), d.clone(), Some(dir_rec))); }
                rep.changes += out.len();
                idx.insert(d.clone(), now);
                pending.extend(out);
                if pending.len() >= batch { self.note(std::mem::take(&mut pending))?; }
            }
        }
        if !pending.is_empty() { self.note(pending)?; }
        // Folders listed again are off the list; folders that changed go on it (once, the latest time)
        if !recheck_new.is_empty() || !recheck_done.is_empty() {
            let mut list: Vec<(String, String, u64)> = self.rechecks().into_iter()
                .filter(|(r, d, _)| !recheck_done.iter().any(|(r2, d2)| r == r2 && d == d2) && !recheck_new.iter().any(|(r2, d2, _)| r == r2 && d == d2)).collect();
            list.extend(recheck_new);
            self.save_rechecks(&list);
        }
        Ok(rep)
    }
}

/// A root of the catalog: the project's folder, or a location
#[derive(Clone, Debug)]
pub struct Root {
    pub id: String,
    pub dir: PathBuf,
    /// A file that must be at its top for it to be looked at (Project.afproject; a location's .afplocation)
    pub marker: Option<String>,
    /// Paths in it left out (the project's own folder, inside a location around it)
    pub exclude: Vec<String>,
}

/// What a sweep keeps and reads: the Library's and afp-catalog's rules
pub trait Inspect: Sync {
    /// Left out of the catalog (and what's in it, a folder)
    fn skip(&self, root: &str, rel: &str, name: &str, dir: bool) -> bool;
    /// Whether what's inside it counts
    fn wants(&self, root: &str, rel: &str) -> bool;
    fn inspect(&self, path: &Path, root: &str, rel: &str) -> io::Result<Option<Value>>;
    /// Not looked at this time, its record kept as it is, whatever the listing says (the footage, when only the
    /// project's own files are looked at)
    fn leave(&self, _root: &str, _rel: &str, _dir: bool) -> bool { false }
}

#[derive(Default, Clone)]
pub struct Sweep {
    /// Every folder listed, whatever its time (once a day: changes inside a file that left its folder's time alone)
    pub deep: bool,
    /// Only these folders (root, path), listed whatever their time (what this computer's system said changed)
    pub only: Vec<(String, String)>,
    pub batch: usize,
    /// Between folders (a look in the background, not to load the NAS)
    pub pause: Option<std::time::Duration>,
}

#[derive(Default, Debug, Clone)]
pub struct Report {
    pub roots: usize,
    pub skipped_roots: Vec<String>,
    pub dirs: usize,
    pub listed: usize,
    pub inspected: usize,
    pub changes: usize,
    pub errors: usize,
}

/// This computer's name in the journals: made once, kept on its own disk with the machine's id it was made on (a
/// home folder copied to another computer makes it a new one there, not two computers writing one journal)
pub fn computer_id(data: &Path, machine: &str, host: &str) -> io::Result<String> {
    let file = data.join("computer.json");
    if let Ok(t) = std::fs::read_to_string(&file) {
        if let Ok(v) = serde_json::from_str::<Value>(&t) {
            if v["machine"].as_str() == Some(machine) { if let Some(id) = v["id"].as_str() { return Ok(id.to_string()); } }
        }
    }
    let slug: String = host.chars().map(|c| if c.is_ascii_alphanumeric() { c.to_ascii_lowercase() } else { '-' }).collect::<String>()
        .split('-').filter(|s| !s.is_empty()).collect::<Vec<_>>().join("-");
    let slug: String = slug.chars().take(24).collect();
    let r = snapshot::sha(format!("{machine}{}{:?}", std::process::id(), std::time::SystemTime::now()).as_bytes());
    let id = format!("{}-{}", if slug.is_empty() { "computer" } else { &slug }, &r[..8]);
    std::fs::create_dir_all(data)?;
    std::fs::write(&file, json!({ "id": id, "machine": machine, "host": host }).to_string())?;
    Ok(id)
}

/// The machine's own id (not a copied home folder's): macOS's platform UUID, Windows' MachineGuid, Linux's machine-id
pub fn machine_id() -> String {
    // macOS: the host's UUID by a system call (ioreg, a process, costs 0.1–0.3 s)
    #[cfg(target_os = "macos")]
    {
        let mut uuid = [0u8; 16];
        let wait = libc::timespec { tv_sec: 1, tv_nsec: 0 };
        if unsafe { libc::gethostuuid(uuid.as_mut_ptr(), &wait) } == 0 {
            return uuid.iter().map(|b| format!("{b:02X}")).collect();
        }
    }
    if cfg!(target_os = "macos") {
        if let Ok(o) = std::process::Command::new("/usr/sbin/ioreg").args(["-rd1", "-c", "IOPlatformExpertDevice"]).output() {
            let t = String::from_utf8_lossy(&o.stdout);
            if let Some(l) = t.lines().find(|l| l.contains("IOPlatformUUID")) {
                if let Some(u) = l.rsplit('"').nth(1) { return u.to_string(); }
            }
        }
    } else if cfg!(windows) {
        if let Ok(o) = std::process::Command::new("reg").args(["query", r"HKLM\SOFTWARE\Microsoft\Cryptography", "/v", "MachineGuid"]).output() {
            if let Some(g) = String::from_utf8_lossy(&o.stdout).split_whitespace().last() { return g.to_string(); }
        }
    } else if let Ok(t) = std::fs::read_to_string("/etc/machine-id") {
        return t.trim().to_string();
    }
    String::new()
}
