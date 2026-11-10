// SPDX-License-Identifier: GPL-3.0-or-later
//
// What the catalog asks of a disk: a few operations, so that a simulated one (sim.rs) can stand for the NAS in tests,
// with its delays, stale listings and lost connections. Paths are full paths.
//
// Only these are relied on, as restic does on any storage: a file listed or looked at, read (from an offset), appended
// to by its one writer, written whole under a name no one else uses (a temporary file renamed to a new name: never
// over a file that's there, which SMB can't do safely), and removed. No locks on the share, no hard links, no clocks.

use std::io;
use std::path::{ Path, PathBuf };
use std::sync::atomic::{ AtomicU64, Ordering };

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Stat {
    pub dir: bool,
    pub size: u64,
    /// Modified, nanoseconds since 1970: compared only for being the same, never with a clock
    pub mtime: i64,
}

#[derive(Clone, Debug)]
pub struct Item {
    pub name: String,
    pub stat: Stat,
}

pub trait Store: Send + Sync {
    /// None: it isn't there (an error is an error: a lost connection is not a missing file)
    fn stat(&self, p: &Path) -> io::Result<Option<Stat>>;
    /// A folder's files and folders; NotFound when it isn't there
    fn list(&self, p: &Path) -> io::Result<Vec<Item>>;
    fn read(&self, p: &Path) -> io::Result<Vec<u8>>;
    /// From an offset to the end
    fn read_from(&self, p: &Path, offset: u64) -> io::Result<Vec<u8>>;
    /// Appended to (made when it isn't there): only by the file's one writer
    fn append(&self, p: &Path, data: &[u8]) -> io::Result<()>;
    /// Written whole: into a temporary file beside it, then renamed to its name. The name must be new (a content's
    /// hash): the rename never replaces another writer's file
    fn put(&self, p: &Path, data: &[u8]) -> io::Result<()>;
    fn remove(&self, p: &Path) -> io::Result<()>;
    fn mkdirs(&self, p: &Path) -> io::Result<()>;
    /// This computer's processes one at a time (a lock on a file on this computer's own disk)
    fn lock(&self, p: &Path) -> io::Result<Box<dyn std::any::Any>>;
}

static COUNTER: AtomicU64 = AtomicU64::new(0);

/// A temporary file's name beside `p`: hidden (walks leave it out), never the same twice
pub fn temp_name(p: &Path) -> PathBuf {
    let t = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0);
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let name = p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    p.with_file_name(format!(".tmp-{}-{:x}-{}-{name}", std::process::id(), t, n))
}

pub fn nanos(t: io::Result<std::time::SystemTime>) -> i64 {
    match t {
        Ok(t) => match t.duration_since(std::time::UNIX_EPOCH) {
            Ok(d) => d.as_nanos().min(i64::MAX as u128) as i64,
            Err(e) => -(e.duration().as_nanos().min(i64::MAX as u128) as i64),
        },
        Err(_) => 0,
    }
}

/// The real disks
pub struct DiskStore;

fn stat_of(m: &std::fs::Metadata) -> Stat {
    Stat { dir: m.is_dir(), size: if m.is_dir() { 0 } else { m.len() }, mtime: nanos(m.modified()) }
}

impl Store for DiskStore {
    fn stat(&self, p: &Path) -> io::Result<Option<Stat>> {
        match std::fs::metadata(p) {
            Ok(m) => Ok(Some(stat_of(&m))),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(e),
        }
    }
    fn list(&self, p: &Path) -> io::Result<Vec<Item>> {
        let mut out = Vec::new();
        for e in std::fs::read_dir(p)? {
            let e = e?;
            let name = e.file_name().to_string_lossy().into_owned();
            // A link is followed (a folder of footage linked in); one that leads nowhere is left out
            let m = match e.metadata() {
                Ok(m) if m.file_type().is_symlink() => match std::fs::metadata(e.path()) { Ok(m) => m, Err(_) => continue },
                Ok(m) => m,
                Err(e) if e.kind() == io::ErrorKind::NotFound => continue,
                Err(e) => return Err(e),
            };
            out.push(Item { name, stat: stat_of(&m) });
        }
        Ok(out)
    }
    fn read(&self, p: &Path) -> io::Result<Vec<u8>> { std::fs::read(p) }
    fn read_from(&self, p: &Path, offset: u64) -> io::Result<Vec<u8>> {
        use std::io::{ Read, Seek, SeekFrom };
        let mut f = std::fs::File::open(p)?;
        f.seek(SeekFrom::Start(offset))?;
        let mut out = Vec::new();
        f.read_to_end(&mut out)?;
        Ok(out)
    }
    fn append(&self, p: &Path, data: &[u8]) -> io::Result<()> {
        use std::io::Write;
        let mut f = std::fs::OpenOptions::new().create(true).append(true).open(p)?;
        f.write_all(data)?;
        f.sync_data()
    }
    fn put(&self, p: &Path, data: &[u8]) -> io::Result<()> {
        use std::io::Write;
        let tmp = temp_name(p);
        let r = (|| {
            let mut f = std::fs::File::create(&tmp)?;
            f.write_all(data)?;
            f.sync_all()?;
            drop(f);
            std::fs::rename(&tmp, p)
        })();
        if r.is_err() { let _ = std::fs::remove_file(&tmp); }
        r
    }
    fn remove(&self, p: &Path) -> io::Result<()> {
        match std::fs::remove_file(p) {
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
            r => r,
        }
    }
    fn mkdirs(&self, p: &Path) -> io::Result<()> { std::fs::create_dir_all(p) }
    fn lock(&self, p: &Path) -> io::Result<Box<dyn std::any::Any>> {
        if let Some(d) = p.parent() { std::fs::create_dir_all(d)?; }
        let f = std::fs::OpenOptions::new().create(true).truncate(false).write(true).open(p)?;
        f.lock()?;
        Ok(Box::new(f))
    }
}
