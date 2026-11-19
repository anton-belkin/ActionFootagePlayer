//! The responsiveness journal, the Library's part (docs/responsiveness-journal.md): one JSON-lines file a run in this
//! computer's log folder (`~/Library/Logs/ActionFootagePlayer/journal/library-<when>-<pid>.jsonl`), with the page's
//! records (ui/journal.js sends them every 2 s: `journal_write`) and the app's own steps (`step`, `span`). Written by a
//! thread of its own: a record costs the caller a `serde_json` line and a channel send, never a disk write. Nothing is
//! sent anywhere. Off with AFP_JOURNAL=0; elsewhere with AFP_JOURNAL_DIR.
//!
//! The clock (`now()`): ms, the process's monotonic clock (Instant) from one reading of the wall clock at start, so it
//! never jumps and lines up with the page's (performance.timeOrigin + now()) and the player's.
//!
//!   journal::init()                       once, first thing in main: the run's header and the start action
//!   journal::step("scan", json!({...}))   a point (fields' "d": how long it took, ms; then "w" is its start)
//!   let _s = journal::span("thumb", json!({...}));   a step timed until the guard goes (`.add(k, v)` on the way)
//!   journal::where_(path)                 "nas" or "local": what the record says about a path
//!   journal::handoff(action, paths)       the player told which Library action opened these (handoff.json)

use serde_json::{ json, Map, Value };
use std::io::Write;
use std::path::{ Path, PathBuf };
use std::sync::mpsc::{ channel, Sender };
use std::sync::{ Mutex, OnceLock };
use std::time::{ Duration, Instant, SystemTime, UNIX_EPOCH };

const FLUSH_EVERY: Duration = Duration::from_secs(2);
const FILE_CAP: u64 = 20 * 1024 * 1024;
const DIR_CAP: u64 = 200 * 1024 * 1024;
const KEEP_DAYS: u64 = 45;

struct Journal {
    anchor_ms: f64,
    t0: Instant,
    tx: Mutex<Sender<String>>,
    dir: PathBuf,
    file: PathBuf,
    start_a: String,
    start_w: f64,
}

static J: OnceLock<Option<Journal>> = OnceLock::new();

fn wall_ms() -> f64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs_f64() * 1000.0).unwrap_or(0.0)
}

/// The journal's folder: AFP_JOURNAL_DIR, else this computer's logs
pub fn dir() -> Option<PathBuf> {
    if let Some(d) = std::env::var_os("AFP_JOURNAL_DIR").filter(|d| !d.is_empty()) { return Some(PathBuf::from(d)); }
    if cfg!(target_os = "macos") {
        return std::env::var_os("HOME").map(|h| PathBuf::from(h).join("Library/Logs/ActionFootagePlayer/journal"));
    }
    if cfg!(windows) {
        return std::env::var_os("LOCALAPPDATA").map(|h| PathBuf::from(h).join("ActionFootagePlayer").join("Logs").join("journal"));
    }
    let state = std::env::var_os("XDG_STATE_HOME").filter(|d| !d.is_empty()).map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/state")))?;
    Some(state.join("ActionFootagePlayer/journal"))
}

/// The journal's clock now (ms, Unix time, monotonic)
pub fn now() -> f64 {
    match J.get() {
        Some(Some(j)) => j.anchor_ms + j.t0.elapsed().as_secs_f64() * 1000.0,
        _ => wall_ms(),
    }
}

fn send(line: String) {
    if let Some(Some(j)) = J.get() {
        if let Ok(tx) = j.tx.lock() { let _ = tx.send(line); }
    }
}

/// A record (its "s" is "app" when not given)
pub fn rec(mut r: Value) {
    if !matches!(J.get(), Some(Some(_))) { return; }
    if let Some(m) = r.as_object_mut() { m.entry("s").or_insert(json!("app")); }
    send(r.to_string());
}

/// A step of the action going on (the analysis finds which, by its time: the app's commands don't say)
pub fn step(name: &str, fields: Value) {
    if !matches!(J.get(), Some(Some(_))) { return; }
    let mut m = match fields { Value::Object(m) => m, _ => Map::new() };
    let mut w = now();
    if let Some(d) = m.get("d").and_then(Value::as_f64) { w -= d; }
    m.insert("k".into(), json!("step"));
    m.insert("n".into(), json!(name));
    m.insert("w".into(), json!(w));
    rec(Value::Object(m));
}

/// A step timed from here until it's dropped
pub struct Span { name: &'static str, t0: f64, fields: Map<String, Value>, on: bool }
pub fn span(name: &'static str, fields: Value) -> Span {
    let on = matches!(J.get(), Some(Some(_)));
    Span { name, t0: if on { now() } else { 0.0 }, fields: match fields { Value::Object(m) => m, _ => Map::new() }, on }
}
impl Span {
    pub fn add(&mut self, k: &str, v: Value) { if self.on { self.fields.insert(k.into(), v); } }
}
impl Drop for Span {
    fn drop(&mut self) {
        if !self.on { return; }
        let mut m = std::mem::take(&mut self.fields);
        m.insert("k".into(), json!("step"));
        m.insert("n".into(), json!(self.name));
        m.insert("w".into(), json!(self.t0));
        m.insert("d".into(), json!(now() - self.t0));
        rec(Value::Object(m));
    }
}

/// The network shares mounted (macOS and Linux: mount's list, read once), longest first
fn shares() -> &'static Vec<String> {
    static S: OnceLock<Vec<String>> = OnceLock::new();
    S.get_or_init(|| {
        let mut list = Vec::new();
        if cfg!(target_os = "linux") {
            for l in std::fs::read_to_string("/proc/mounts").unwrap_or_default().lines() {
                let f: Vec<&str> = l.split(' ').collect();
                if f.len() > 2 && (f[2].starts_with("cifs") || f[2].starts_with("smb") || f[2].starts_with("nfs")) {
                    list.push(f[1].replace("\\040", " "));
                }
            }
        } else if cfg!(target_os = "macos") {
            let out = std::process::Command::new("/sbin/mount").output().map(|o| String::from_utf8_lossy(&o.stdout).into_owned()).unwrap_or_default();
            list = parse_mount(&out);
        }
        list.sort_by_key(|m| std::cmp::Reverse(m.len()));
        list
    })
}

/// macOS mount's lines: "//user@nas/share on /Volumes/share (smbfs, nodev, ...)": the network ones' mount points
pub fn parse_mount(out: &str) -> Vec<String> {
    out.lines().filter_map(|l| {
        let (_, rest) = l.split_once(" on ")?;
        let (at, fs) = rest.rsplit_once(" (")?;
        let fs = fs.split([',', ')']).next()?.trim();
        ["smbfs", "nfs", "afpfs", "webdav"].contains(&fs).then(|| at.to_string())
    }).collect()
}

/// "nas" for a path on a network share, else "local"
pub fn where_(path: &Path) -> &'static str { where_in(path, shares()) }
pub fn where_in(path: &Path, shares: &[String]) -> &'static str {
    let p = path.to_string_lossy();
    if cfg!(windows) && (p.starts_with("\\\\") || p.starts_with("//")) && !p.starts_with("\\\\?\\") { return "nas"; }
    if p.starts_with("\\\\?\\UNC\\") { return "nas"; }
    for m in shares {
        if p.starts_with(m.as_str()) && (p.len() == m.len() || p[m.len()..].starts_with('/')) { return "nas"; }
    }
    "local"
}

/// The player told which action opened these files: it takes it as it opens one (journal.lua's handoff)
pub fn handoff(action: Option<&str>, paths: &[String]) {
    let Some(Some(j)) = J.get() else { return };
    let Some(a) = action else { return };
    let body = json!({ "a": a, "w": now(), "paths": paths }).to_string();
    let _ = std::fs::create_dir_all(&j.dir);
    let tmp = j.dir.join("handoff.json.tmp");
    if std::fs::write(&tmp, body).is_ok() { let _ = std::fs::rename(&tmp, j.dir.join("handoff.json")); }
    step("handoff", json!({ "a": a, "files": paths.len() }));
}

/// The env a batch player gets: its records say which job of this run started it
pub fn parent_env(job: u64) -> Option<String> {
    match J.get() { Some(Some(_)) => Some(format!("L{}.job{}", std::process::id(), job)), _ => None }
}

/// The run's start action: its id and when the process started
pub fn start() -> Option<(String, f64)> {
    match J.get() { Some(Some(j)) => Some((j.start_a.clone(), j.start_w)), _ => None }
}

pub fn file() -> Option<PathBuf> { match J.get() { Some(Some(j)) => Some(j.file.clone()), _ => None } }

/// When this process started (ms, the journal's clock): macOS's proc_pid_rusage, Linux's /proc
fn process_start(anchor_now: f64) -> Option<f64> {
    #[cfg(target_os = "macos")]
    {
        #[repr(C)] struct Timebase { numer: u32, denom: u32 }
        unsafe extern "C" {
            fn proc_pid_rusage(pid: i32, flavor: i32, buffer: *mut u64) -> i32;
            fn mach_absolute_time() -> u64;
            fn mach_timebase_info(info: *mut Timebase) -> i32;
        }
        let mut buf = [0u64; 12];
        // rusage_info_v0: a 16-byte uuid, 8 u64, then ri_proc_start_abstime
        if unsafe { proc_pid_rusage(std::process::id() as i32, 0, buf.as_mut_ptr()) } != 0 { return None; }
        let mut tb = Timebase { numer: 0, denom: 0 };
        unsafe { mach_timebase_info(&mut tb) };
        let now_abs = unsafe { mach_absolute_time() };
        if tb.denom == 0 || buf[10] == 0 || buf[10] > now_abs { return None; }
        let ago_ms = (now_abs - buf[10]) as f64 * tb.numer as f64 / tb.denom as f64 / 1e6;
        return Some(anchor_now - ago_ms);
    }
    #[cfg(target_os = "linux")]
    {
        let stat = std::fs::read_to_string("/proc/self/stat").ok()?;
        let rest = stat.rsplit_once(") ")?.1;
        let ticks: f64 = rest.split(' ').nth(19)?.parse().ok()?;
        let up: f64 = std::fs::read_to_string("/proc/uptime").ok()?.split(' ').next()?.parse().ok()?;
        return Some(anchor_now - (up - ticks / 100.0) * 1000.0);
    }
    #[allow(unreachable_code)]
    { let _ = anchor_now; None }
}

/// Once, first thing in main
pub fn init() {
    J.get_or_init(|| {
        if std::env::var("AFP_JOURNAL").is_ok_and(|v| v == "0") { return None; }
        let dir = dir()?;
        let t0 = Instant::now();
        let anchor_ms = wall_ms();
        let started = process_start(anchor_ms);
        let pid = std::process::id();
        let stamp = stamp((started.unwrap_or(anchor_ms) / 1000.0) as i64);
        let file = dir.join(format!("library-{stamp}-{pid}.jsonl"));
        let (tx, rx) = channel::<String>();
        let (wdir, wfile) = (dir.clone(), file.clone());
        std::thread::Builder::new().name("journal".into()).spawn(move || writer(rx, wdir, wfile)).ok()?;
        let start_a = format!("L{pid}.start");
        Some(Journal { anchor_ms, t0, tx: Mutex::new(tx), dir, file, start_a, start_w: started.unwrap_or(anchor_ms) })
    });
    let Some((a, w)) = start() else { return };
    rec(json!({ "k": "run", "w": now(), "app": "library", "v": 1, "pid": std::process::id(), "start": w,
                "version": env!("CARGO_PKG_VERSION"), "platform": std::env::consts::OS }));
    rec(json!({ "k": "act", "a": a, "n": "library/start", "w": w, "via": "core" }));
}

/// yyyymmdd-hhmmss (UTC) of a Unix time, for the file's name
pub fn stamp(secs: i64) -> String {
    let days = secs.div_euclid(86400);
    let s = secs.rem_euclid(86400);
    // Civil from days (Howard Hinnant's algorithm)
    let z = days + 719468;
    let era = z.div_euclid(146097);
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    format!("{y:04}{m:02}{d:02}-{:02}{:02}{:02}", s / 3600, s % 3600 / 60, s % 60)
}

// The writer: lines gathered, written every FLUSH_EVERY in one go; the folder kept small once, 20 s in
fn writer(rx: std::sync::mpsc::Receiver<String>, dir: PathBuf, file: PathBuf) {
    let mut pending: Vec<String> = Vec::new();
    let mut written = 0u64;
    let mut capped = false;
    let mut pruned = false;
    let born = Instant::now();
    let mut flush_ms = 0f64;
    let mut flushes = 0u64;
    let mut records = 0u64;
    loop {
        let deadline = Instant::now() + FLUSH_EVERY;
        let mut closed = false;
        loop {
            let left = deadline.saturating_duration_since(Instant::now());
            match rx.recv_timeout(left) {
                Ok(l) => { pending.push(l); records += 1; }
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => break,
                Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => { closed = true; break; }
            }
        }
        if !pending.is_empty() && !capped {
            let t = Instant::now();
            let _ = std::fs::create_dir_all(&dir);
            if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(&file) {
                let mut text = pending.join("\n");
                text.push('\n');
                if f.write_all(text.as_bytes()).is_ok() { written += text.len() as u64; }
                if written > FILE_CAP {
                    capped = true;
                    let _ = writeln!(f, "{}", json!({ "k": "capped", "w": now(), "s": "app", "bytes": written }));
                }
            }
            pending.clear();
            let ms = t.elapsed().as_secs_f64() * 1000.0;
            flush_ms += ms;
            flushes += 1;
            if flushes % 30 == 0 {
                pending.push(json!({ "k": "cost", "w": now(), "s": "app", "records": records, "bytes": written, "flushes": flushes, "flush_ms": flush_ms }).to_string());
            }
        }
        if !pruned && born.elapsed() > Duration::from_secs(20) {
            pruned = true;
            prune(&dir, &file, DIR_CAP, KEEP_DAYS);
        }
        if closed { return; }
    }
}

/// Files older than keep_days go, then the oldest until the folder's under cap; `keep` (this run's) stays
pub fn prune(dir: &Path, keep: &Path, cap: u64, keep_days: u64) {
    let Ok(rd) = std::fs::read_dir(dir) else { return };
    let cutoff = SystemTime::now() - Duration::from_secs(keep_days * 86400);
    let mut files: Vec<(SystemTime, u64, PathBuf)> = Vec::new();
    for e in rd.flatten() {
        let p = e.path();
        if p.extension().is_none_or(|x| x != "jsonl") || p == keep { continue; }
        let Ok(m) = e.metadata() else { continue };
        let t = m.modified().unwrap_or(SystemTime::now());
        if t < cutoff { let _ = std::fs::remove_file(&p); continue; }
        files.push((t, m.len(), p));
    }
    let mut total: u64 = files.iter().map(|f| f.1).sum::<u64>() + std::fs::metadata(keep).map(|m| m.len()).unwrap_or(0);
    files.sort();
    for (_, size, p) in files {
        if total <= cap { break; }
        if std::fs::remove_file(&p).is_ok() { total -= size; }
    }
}

// ---- the page's side (ui/journal.js)

/// The page's records, as lines already (JSON objects): checked to be objects, then written with the app's
#[tauri::command]
pub fn journal_write(lines: Vec<String>) {
    for l in lines {
        if l.len() < 64 * 1024 && l.starts_with('{') && l.ends_with('}') { send(l); }
    }
}

/// What the page needs to start: this process's id, the start action
#[tauri::command]
pub fn journal_hello() -> Value {
    match start() {
        Some((a, w)) => json!({ "pid": std::process::id(), "start_a": a, "start_w": w, "file": file() }),
        None => json!({ "pid": std::process::id(), "off": true }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stamps() {
        assert_eq!(stamp(0), "19700101-000000");
        assert_eq!(stamp(1_795_104_368), "20261119-160608");
        assert_eq!(stamp(951_782_400), "20000229-000000");
    }

    #[test]
    fn mount_lines() {
        let out = "/dev/disk3s1s1 on / (apfs, sealed, local, read-only, journaled)\n\
                   //claude@web.nas/DJI_Inbox on /Volumes/DJI_Inbox (smbfs, nodev, nosuid, mounted by antonbelkin)\n\
                   /dev/disk5s1 on /Volumes/My SSD (apfs, local, nodev)\n\
                   nas:/video on /Volumes/video share (nfs)\n";
        let mut list = parse_mount(out);
        assert_eq!(list, vec!["/Volumes/DJI_Inbox".to_string(), "/Volumes/video share".to_string()]);
        list.sort_by_key(|m| std::cmp::Reverse(m.len()));
        assert_eq!(where_in(Path::new("/Volumes/DJI_Inbox/a.MP4"), &list), "nas");
        assert_eq!(where_in(Path::new("/Volumes/DJI_Inbox"), &list), "nas");
        assert_eq!(where_in(Path::new("/Volumes/DJI_Inbox2/a.MP4"), &list), "local");
        assert_eq!(where_in(Path::new("/Volumes/video share/x"), &list), "nas");
        assert_eq!(where_in(Path::new("/Users/a/x"), &list), "local");
    }

    #[test]
    fn prunes_old_and_over_cap() {
        let dir = std::env::temp_dir().join(format!("afp-journal-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        for (i, size) in [(1, 400u64), (2, 400), (3, 400)] {
            let p = dir.join(format!("player-{i}.jsonl"));
            std::fs::write(&p, vec![b'x'; size as usize]).unwrap();
            // Older first: 1 is oldest
            let t = SystemTime::now() - Duration::from_secs(100 - i);
            std::fs::File::options().write(true).open(&p).unwrap().set_modified(t).unwrap();
        }
        let old = dir.join("player-0.jsonl");
        std::fs::write(&old, b"x").unwrap();
        std::fs::File::options().write(true).open(&old).unwrap().set_modified(SystemTime::now() - Duration::from_secs(50 * 86400)).unwrap();
        std::fs::write(dir.join("notes.txt"), b"keep").unwrap();
        let keep = dir.join("library-now.jsonl");
        std::fs::write(&keep, vec![b'y'; 300]).unwrap();
        prune(&dir, &keep, 1200, 45);
        assert!(!old.exists(), "older than 45 days");
        assert!(!dir.join("player-1.jsonl").exists(), "the oldest, over the cap");
        assert!(dir.join("player-2.jsonl").exists() && dir.join("player-3.jsonl").exists());
        assert!(keep.exists() && dir.join("notes.txt").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
