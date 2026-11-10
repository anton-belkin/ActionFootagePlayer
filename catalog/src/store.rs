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

/// What's written made to reach the disk: fsync (a share flushes it on the server). macOS's F_FULLFSYNC, which std's
/// sync uses, isn't supported over SMB ("Operation not supported"): plain fsync there, and a share that doesn't do that
/// either has had the data handed to it
fn flush(f: &std::fs::File) -> io::Result<()> {
    #[cfg(unix)]
    {
        use std::os::fd::AsRawFd;
        if unsafe { libc::fsync(f.as_raw_fd()) } == 0 { return Ok(()); }
        let e = io::Error::last_os_error();
        return match e.raw_os_error() { Some(libc::ENOTSUP) | Some(libc::EOPNOTSUPP) | Some(libc::EINVAL) => Ok(()), _ => Err(e) };
    }
    #[allow(unreachable_code)]
    f.sync_all()
}

/// macOS: a folder's entries with their kind, size and time in one go (getattrlistbulk, as Finder lists), not a stat
/// for each (over SMB each is a round trip of several messages: ~5 ms on a wired NAS)
#[cfg(target_os = "macos")]
mod bulk {
    use super::{ Item, Stat };
    use std::io;
    use std::os::unix::ffi::OsStrExt;
    use std::path::Path;

    fn u32_at(b: &[u8], i: usize) -> u32 { u32::from_ne_bytes([b[i], b[i + 1], b[i + 2], b[i + 3]]) }
    fn i64_at(b: &[u8], i: usize) -> i64 { let mut x = [0u8; 8]; x.copy_from_slice(&b[i..i + 8]); i64::from_ne_bytes(x) }

    /// None: not done this way (a link among the entries is followed by the slow way)
    pub fn list(p: &Path) -> Option<io::Result<Vec<Item>>> {
        let c = std::ffi::CString::new(p.as_os_str().as_bytes()).ok()?;
        let fd = unsafe { libc::open(c.as_ptr(), libc::O_RDONLY | libc::O_DIRECTORY | libc::O_CLOEXEC) };
        if fd < 0 { return Some(Err(io::Error::last_os_error())); }
        struct Fd(i32);
        impl Drop for Fd { fn drop(&mut self) { unsafe { libc::close(self.0); } } }
        let _fd = Fd(fd);
        let mut al: libc::attrlist = unsafe { std::mem::zeroed() };
        al.bitmapcount = libc::ATTR_BIT_MAP_COUNT;
        al.commonattr = libc::ATTR_CMN_RETURNED_ATTRS | libc::ATTR_CMN_NAME | libc::ATTR_CMN_OBJTYPE | libc::ATTR_CMN_MODTIME;
        al.fileattr = libc::ATTR_FILE_DATALENGTH;
        let mut buf = vec![0u8; 256 * 1024];
        let mut out = Vec::new();
        loop {
            let n = unsafe { libc::getattrlistbulk(fd, &mut al as *mut _ as *mut libc::c_void, buf.as_mut_ptr() as *mut libc::c_void, buf.len(),
                                                   libc::FSOPT_PACK_INVAL_ATTRS as u64) };
            if n < 0 { return Some(Err(io::Error::last_os_error())); }
            if n == 0 { break; }
            let mut at = 0usize;
            for _ in 0..n {
                let len = u32_at(&buf, at) as usize;
                if len == 0 || at + len > buf.len() { return Some(Err(io::Error::new(io::ErrorKind::InvalidData, "getattrlistbulk"))); }
                let e = &buf[at..at + len];
                // length, the attributes returned (5 words), then name, kind, time, size in that order
                let mut o = 4 + 20;
                let name_off = u32_at(e, o) as i32 as isize;
                let name_len = u32_at(e, o + 4) as usize;
                let ns = (o as isize + name_off) as usize;
                let name = String::from_utf8_lossy(&e[ns..ns + name_len.saturating_sub(1)]).into_owned();
                o += 8;
                let kind = u32_at(e, o);
                o += 4;
                let mtime = i64_at(e, o).saturating_mul(1_000_000_000).saturating_add(i64_at(e, o + 8));
                o += 16;
                // A folder's entry has no file attributes
                let size = if o + 8 <= e.len() { i64_at(e, o).max(0) as u64 } else { 0 };
                at += len;
                const VREG: u32 = 1;
                const VDIR: u32 = 2;
                const VLNK: u32 = 5;
                match kind {
                    VREG => out.push(Item { name, stat: Stat { dir: false, size, mtime } }),
                    VDIR => out.push(Item { name, stat: Stat { dir: true, size: 0, mtime } }),
                    VLNK => match std::fs::metadata(p.join(&name)) {
                        Ok(m) => out.push(Item { name, stat: super::stat_of(&m) }),
                        Err(_) => {}
                    },
                    _ => {}
                }
            }
        }
        Some(Ok(out))
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
        #[cfg(target_os = "macos")]
        { if let Some(r) = bulk::list(p) { return r; } }
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
        flush(&f)
    }
    fn put(&self, p: &Path, data: &[u8]) -> io::Result<()> {
        use std::io::Write;
        let tmp = temp_name(p);
        let r = (|| {
            let mut f = std::fs::File::create(&tmp)?;
            f.write_all(data)?;
            flush(&f)?;
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

/// A store that counts what's asked of it (for the rig, and to say what a look cost)
pub struct Counted<S: Store> { pub inner: S, pub counts: std::sync::Mutex<[u64; 8]> }

impl<S: Store> Counted<S> {
    pub fn new(inner: S) -> Self { Counted { inner, counts: std::sync::Mutex::new([0; 8]) } }
    fn add(&self, i: usize, n: u64) { self.counts.lock().unwrap()[i] += n; }
    /// stats, lists, entries listed, reads, bytes read, writes, bytes written, removes
    pub fn take(&self) -> [u64; 8] { std::mem::take(&mut *self.counts.lock().unwrap()) }
}

impl<S: Store> Store for Counted<S> {
    fn stat(&self, p: &Path) -> io::Result<Option<Stat>> { self.add(0, 1); self.inner.stat(p) }
    fn list(&self, p: &Path) -> io::Result<Vec<Item>> { self.add(1, 1); let r = self.inner.list(p)?; self.add(2, r.len() as u64); Ok(r) }
    fn read(&self, p: &Path) -> io::Result<Vec<u8>> { self.add(3, 1); let r = self.inner.read(p)?; self.add(4, r.len() as u64); Ok(r) }
    fn read_from(&self, p: &Path, o: u64) -> io::Result<Vec<u8>> { self.add(3, 1); let r = self.inner.read_from(p, o)?; self.add(4, r.len() as u64); Ok(r) }
    fn append(&self, p: &Path, d: &[u8]) -> io::Result<()> { self.add(5, 1); self.add(6, d.len() as u64); self.inner.append(p, d) }
    fn put(&self, p: &Path, d: &[u8]) -> io::Result<()> { self.add(5, 1); self.add(6, d.len() as u64); self.inner.put(p, d) }
    fn remove(&self, p: &Path) -> io::Result<()> { self.add(7, 1); self.inner.remove(p) }
    fn mkdirs(&self, p: &Path) -> io::Result<()> { self.inner.mkdirs(p) }
    fn lock(&self, p: &Path) -> io::Result<Box<dyn std::any::Any>> { self.inner.lock(p) }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn a_listing_has_the_sizes_and_times_a_stat_has() {
        let d = std::env::temp_dir().join(format!("afp-cat-list-{}", std::process::id()));
        std::fs::create_dir_all(d.join("sub")).unwrap();
        std::fs::write(d.join("a.mp4"), b"12345").unwrap();
        std::fs::write(d.join("Ünïcode name.json"), b"{}").unwrap();
        #[cfg(unix)] std::os::unix::fs::symlink(d.join("a.mp4"), d.join("link.mp4")).unwrap();
        let mut l = DiskStore.list(&d).unwrap();
        l.sort_by(|a, b| a.name.cmp(&b.name));
        for i in &l {
            let s = DiskStore.stat(&d.join(&i.name)).unwrap().unwrap();
            assert_eq!(i.stat, s, "{}", i.name);
        }
        assert_eq!(l.len(), if cfg!(unix) { 4 } else { 3 });
        std::fs::remove_dir_all(&d).unwrap();
    }
}
