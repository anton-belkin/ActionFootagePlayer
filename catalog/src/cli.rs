// SPDX-License-Identifier: GPL-3.0-or-later
//
// afp-catalog: the project's catalog from the command line — what the player asks of it (a clip's moments, the
// project's own files of a kind, what it wrote noted), and what the Library does (looking at the folders, folding a
// snapshot), to see and try it without the app. docs/library-management.md §9.

use crate::catalog::{ Catalog, Error, Sweep, Want };
use crate::inspect::{ self, Ours };
use crate::project;
use crate::state::File;
use crate::store::{ DiskStore, Store };
use crate::Inspect;
use serde_json::{ json, Value };
use std::collections::BTreeSet;
use std::path::{ Path, PathBuf };
use std::process::ExitCode;

const USAGE: &str = "usage: afp-catalog derived <project> [--hash <fingerprint>] [--clip <path in the tree>]
                                          a clip's moments, clip previews, exports and photos, each looked at (JSON)
       afp-catalog list <project> [--kind moment|clip|export|photo]
                                          the project's own files with a link (JSON)
       afp-catalog note <project> <file>...  files written or removed: their folders touched, noted in this computer's journal
                                          (project -: found from where each file is)
       afp-catalog status <project>       the catalog: its snapshot, journals, size
       afp-catalog look <project> [--own] [--every <s>] [--deep]
                                          look at the folders and note what changed (--own: the project's own files,
                                          what the player asks for; folds the journals when they've grown)
       afp-catalog watch <project> [--share <name>=<path>]... [--every <s>]
                                          keep the catalog, on the NAS (Docker): told by the system what changes
       afp-catalog fold <project>         a new snapshot from the journals, and clearing up (what the Library does)";

fn load(cat: &mut Catalog, want: &Want) -> Result<crate::Loaded, Error> {
    match cat.load(want) {
        Err(Error::Gap(_)) => cat.load(want),
        r => r,
    }
}

fn kind_of(rel: &str, link: &Value) -> String {
    let lower = rel.to_lowercase();
    if inspect::is_photo(&lower) { return "photo".into(); }
    link.get("kind").and_then(Value::as_str).unwrap_or("moment").to_string()
}

fn item(roots: &[crate::Root], root: &str, rel: &str, f: &File) -> Option<Value> {
    let r = roots.iter().find(|r| r.id == root)?;
    let link = f.meta.as_ref()?.get("link")?.clone();
    if link.is_null() { return None; }
    let mut v = json!({ "path": r.dir.join(rel).display().to_string(), "root": root, "rel": rel, "size": f.size, "link": link, "kind": kind_of(rel, &link) });
    if let Some(t) = f.meta.as_ref().and_then(|m| m.get("tags")) { v["tags"] = t.clone(); }
    Some(v)
}

/// What a file is now, as a record (None: not there, its folder listed without it; Err: couldn't tell)
fn observe(ours: &Ours, root: &str, rel: &str, path: &Path) -> std::io::Result<Option<File>> {
    match DiskStore.stat(path)? {
        Some(s) => {
            let meta = if !s.dir && ours.wants(root, rel) { ours.inspect(path, root, rel)? } else { None };
            Ok(Some(File { dir: s.dir, size: s.size, mtime: s.mtime, meta, digest: None }))
        }
        None => {
            let parent = path.parent().ok_or_else(|| std::io::Error::other("no folder"))?;
            let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
            if DiskStore.list(parent)?.iter().any(|i| i.name == name) { return Err(std::io::Error::other("listed but not there")); }
            Ok(None)
        }
    }
}

/// The project a file is in: a Project.afproject above it, else the project its location's .afplocation names
pub fn project_for(file: &Path) -> Option<PathBuf> {
    let mounts = crate::locations::mounts();
    let mut dir = file.parent();
    while let Some(d) = dir {
        if d.join(crate::locations::PROJECT_FILE).is_file() { return Some(d.to_path_buf()); }
        for e in std::fs::read_dir(d).into_iter().flatten().flatten() {
            let n = e.file_name().to_string_lossy().into_owned();
            if !n.ends_with(crate::locations::LOCATION_EXT) { continue; }
            let Some(v) = crate::locations::read_json(&e.path()) else { continue };
            let Some(p) = v.get("project").and_then(Value::as_str).and_then(|p| crate::locations::place(p, d, None, &mounts)) else { continue };
            if p.join(crate::locations::PROJECT_FILE).is_file() { return Some(p); }
        }
        dir = d.parent();
    }
    None
}

/// At low priority (a look in the background mustn't slow what's playing): nice, and on a Mac its disk and network
/// throttled as a background task
fn background() {
    #[cfg(unix)]
    unsafe {
        libc::setpriority(libc::PRIO_PROCESS, 0, 10);
        #[cfg(target_os = "macos")]
        libc::setpriority(libc::PRIO_DARWIN_PROCESS, 0, libc::PRIO_DARWIN_BG);
    }
}

/// afp-catalog watch: told by the system, looking only where it said
fn watch(proj: &Path, mut cat: Catalog, roots: &[crate::Root], ours: &Ours, every: u64) -> ExitCode {
    use notify::Watcher;
    use std::time::{ Duration, Instant };
    let (tx, rx) = std::sync::mpsc::channel::<notify::Result<notify::Event>>();
    let mut watcher = notify::recommended_watcher(move |e| { let _ = tx.send(e); }).ok();
    let mut told = false;
    if let Some(w) = watcher.as_mut() {
        for r in roots { match w.watch(&r.dir, notify::RecursiveMode::Recursive) {
            Ok(()) => told = true,
            Err(e) => eprintln!("afp-catalog: {} not watched ({e}): looked at every {every} s", r.dir.display()),
        } }
    }
    if !told { eprintln!("afp-catalog: no notices from the system: every folder's time looked at every {every} s"); }
    let mut loaded = false;
    let mut everything = true;
    let mut last_all = Instant::now();
    let mut pending: std::collections::BTreeSet<(String, String)> = Default::default();
    let mut last_event = Instant::now();
    loop {
        if !loaded {
            match load(&mut cat, &Want::All) {
                Ok(_) => loaded = true,
                Err(Error::Damaged(_)) => { cat = project::open(proj); loaded = true; }
                Err(e) => { eprintln!("afp-catalog: {e}"); std::thread::sleep(Duration::from_secs(10)); continue }
            }
        }
        // What the system said: the folders it named (ours, in .afp, aside)
        while let Ok(e) = rx.try_recv() {
            match e {
                Ok(e) => {
                    if e.need_rescan() { everything = true; }
                    if matches!(e.kind, notify::EventKind::Access(_)) { continue; }
                    for p in e.paths {
                        if p.components().any(|c| c.as_os_str().to_string_lossy().starts_with('.')) { continue; }
                        if let Some((r, rel)) = project::locate(roots, &p) {
                            pending.insert((r.clone(), crate::state::parent(&rel).to_string()));
                            pending.insert((r, rel));
                        }
                    }
                    last_event = Instant::now();
                }
                Err(_) => everything = true,
            }
        }
        if !told && last_all.elapsed() >= Duration::from_secs(every.min(30)) { everything = true; }
        if last_all.elapsed() >= Duration::from_secs(every) { everything = true; }
        let r = if everything {
            everything = false;
            last_all = Instant::now();
            pending.clear();
            Some(cat.sweep(roots, ours, &Sweep::default()))
        } else if !pending.is_empty() && last_event.elapsed() >= Duration::from_secs(1) {
            let only: Vec<(String, String)> = std::mem::take(&mut pending).into_iter().collect();
            Some(cat.sweep(roots, ours, &Sweep { only, ..Default::default() }))
        } else {
            // Folders that changed, listed once more a minute later
            let due = cat.due_rechecks();
            (!due.is_empty()).then(|| cat.sweep(roots, ours, &Sweep { only: due, ..Default::default() }))
        };
        match r {
            Some(Ok(rep)) => {
                if rep.changes > 0 { eprintln!("afp-catalog: {} changes ({} folders listed, {} read)", rep.changes, rep.listed, rep.inspected); }
                if cat.should_compact() || cat.manifest.is_none() {
                    match cat.compact() { Ok(n) => { let removed = cat.clean().unwrap_or(0); eprintln!("afp-catalog: folded into {n}, {removed} removed"); } Err(e) => eprintln!("afp-catalog: fold: {e}") }
                }
            }
            Some(Err(Error::Gap(_))) => loaded = false,
            Some(Err(e)) => eprintln!("afp-catalog: {e}"),
            None => {}
        }
        std::thread::sleep(Duration::from_millis(250));
    }
}

pub fn main() -> ExitCode {
    let a: Vec<String> = std::env::args().skip(1).collect();
    if a.len() < 2 || a.iter().any(|x| x == "-h" || x == "--help") { println!("{USAGE}"); return if a.len() < 2 { ExitCode::from(1) } else { ExitCode::SUCCESS }; }
    // note -: each file's project found from where it is
    if a[0] == "note" && a[1] == "-" {
        let mut by: std::collections::BTreeMap<PathBuf, Vec<String>> = Default::default();
        for f in &a[2..] {
            let path = std::path::absolute(f).unwrap_or_else(|_| PathBuf::from(f));
            match project_for(&path) { Some(p) => by.entry(p).or_default().push(f.clone()), None => eprintln!("afp-catalog: {f}: in no project") }
        }
        let me = std::env::current_exe().ok();
        let mut ok = true;
        for (p, files) in by {
            let Some(me) = &me else { break };
            let st = std::process::Command::new(me).arg("note").arg(&p).args(&files).status();
            ok &= st.is_ok_and(|s| s.success());
        }
        return if ok { ExitCode::SUCCESS } else { ExitCode::from(1) };
    }
    let proj = PathBuf::from(&a[1]);
    let opt = |name: &str| a.iter().position(|x| x == name).and_then(|i| a.get(i + 1)).cloned();
    let fail = |e: String| { eprintln!("afp-catalog: {e}"); ExitCode::from(1) };
    if !proj.join(crate::locations::PROJECT_FILE).is_file() { return fail(format!("{} isn't a project's folder", proj.display())); }
    // AFP_CATALOG_TIMES=1: how long each step took, on stderr
    let t0 = std::time::Instant::now();
    let times = std::env::var_os("AFP_CATALOG_TIMES").is_some();
    let lap = |what: &str| if times { eprintln!("{what}: {:.1} ms", t0.elapsed().as_secs_f64() * 1000.0); };
    let mut cat = project::open(&proj);
    lap("opened (this computer's name)");
    let (roots, ours) = project::roots(&proj);
    lap("roots (locations, mounts)");
    match a[0].as_str() {
        "derived" => {
            let (hash, clip) = (opt("--hash"), opt("--clip"));
            let info = match load(&mut cat, &Want::Keys(Catalog::keys_for(hash.as_deref(), clip.as_deref()))) { Ok(i) => i, Err(e) => return fail(e.to_string()) };
            lap("loaded");
            if info.empty && info.snapshot.is_none() { println!("{}", json!({ "catalog": false })); return ExitCode::SUCCESS; }
            let mut out = Vec::new();
            let mut notes = Vec::new();
            for (root, rel, f) in cat.derived(hash.as_deref(), clip.as_deref()) {
                let Some(r) = roots.iter().find(|r| r.id == root) else { continue };
                let path = r.dir.join(&rel);
                // Each one looked at: what's there now is what's said (and noted when it isn't what was on record)
                match observe(&ours, &root, &rel, &path) {
                    Ok(Some(now)) if now.size == f.size && now.mtime == f.mtime => { if let Some(v) = item(&roots, &root, &rel, &f) { out.push(v); } }
                    // Not there: kept, said missing (moved in Finder, or removed: the look the player asks for next tells)
                    Ok(None) => { if let Some(mut v) = item(&roots, &root, &rel, &f) { v["missing"] = json!(true); out.push(v); } }
                    Ok(Some(n)) => {
                        let lh = n.meta.as_ref().and_then(|m| m["link"]["hash"].as_str()).map(str::to_string);
                        let lc = n.meta.as_ref().and_then(|m| m["link"]["clip"].as_str()).map(str::to_string);
                        if (clip.is_some() && lc == clip) || (hash.is_some() && lh == hash) { if let Some(v) = item(&roots, &root, &rel, &n) { out.push(v); } }
                        notes.push((root, rel, Some(n)));
                    }
                    Err(_) => { if let Some(v) = item(&roots, &root, &rel, &f) { out.push(v); } }
                }
            }
            lap("looked at");
            println!("{}", json!({ "catalog": true, "items": out }));
            if !notes.is_empty() { let _ = cat.note(notes); }
            ExitCode::SUCCESS
        }
        "list" => {
            let kind = opt("--kind");
            let info = match load(&mut cat, &Want::All) { Ok(i) => i, Err(e) => return fail(e.to_string()) };
            if info.empty && info.snapshot.is_none() { println!("{}", json!({ "catalog": false })); return ExitCode::SUCCESS; }
            let out: Vec<Value> = cat.state.live().filter_map(|(root, rel, f)| item(&roots, root, rel, f))
                .filter(|v| kind.as_deref().is_none_or(|k| v["kind"] == k)).collect();
            println!("{}", json!({ "catalog": true, "items": out }));
            ExitCode::SUCCESS
        }
        "note" => {
            let mut entries = Vec::new();
            let mut dirs: BTreeSet<PathBuf> = BTreeSet::new();
            for f in &a[2..] {
                let path = std::path::absolute(f).unwrap_or_else(|_| PathBuf::from(f));
                let Some((root, rel)) = project::locate(&roots, &path) else { eprintln!("afp-catalog: {f}: not in the project's folder or its locations"); continue };
                match observe(&ours, &root, &rel, &path) {
                    Ok(now) => {
                        if let Some(d) = path.parent() { dirs.insert(d.to_path_buf()); }
                        entries.push((root, rel, now));
                    }
                    Err(e) => eprintln!("afp-catalog: {f}: {e}"),
                }
            }
            for d in &dirs { let _ = cat.touch(d); }
            if let Err(e) = load(&mut cat, &Want::Keys(Vec::new())) { if !matches!(e, Error::Damaged(_)) { return fail(e.to_string()); } }
            match cat.note(entries) { Ok(()) => ExitCode::SUCCESS, Err(e) => fail(e.to_string()) }
        }
        "status" => {
            let info = match load(&mut cat, &Want::All) { Ok(i) => i, Err(e) => return fail(e.to_string()) };
            let m = cat.manifest.as_ref().map(|(n, m)| json!({ "name": n, "generation": m.gen, "by": m.by, "parts": m.parts.len(), "records": m.count, "covers": m.covers }));
            println!("{}", serde_json::to_string_pretty(&json!({ "me": cat.me, "snapshot": m, "read": { "parts": info.parts_read, "parts_cached": info.parts_cached, "passed": info.passed },
                "journal_lines_after_it": cat.tail_lines, "journal_bytes_after_it": cat.tail_bytes, "applied": cat.applied, "records_now": cat.state.len_live(),
                "roots": roots.iter().map(|r| json!({ "id": r.id, "dir": r.dir.display().to_string() })).collect::<Vec<_>>() })).unwrap());
            ExitCode::SUCCESS
        }
        "look" => {
            // --own: the project's own files only (what the player asks for); --every <s>: not again sooner on this
            // computer (another look here meanwhile, by anyone, counts); one at a time on this computer
            background();
            let own = a.iter().any(|x| x == "--own");
            let every: u64 = opt("--every").and_then(|x| x.parse().ok()).unwrap_or(0);
            let (roots, ours) = if own { project::own_roots(&proj) } else { (roots, ours) };
            let _ = std::fs::create_dir_all(&cat.cache);
            let Ok(lockf) = std::fs::OpenOptions::new().create(true).truncate(false).write(true).open(cat.cache.join("look.lock")) else { return fail("no lock".into()) };
            if lockf.try_lock().is_err() { println!("{}", json!({ "busy": true })); return ExitCode::SUCCESS; }
            let stamp = cat.cache.join(if own { "look-own.json" } else { "look.json" });
            let now_s = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
            let last = std::fs::read_to_string(&stamp).ok().and_then(|t| t.trim().parse::<u64>().ok()).unwrap_or(0);
            if every > 0 && now_s.saturating_sub(last) < every { println!("{}", json!({ "skipped": true, "seconds_ago": now_s.saturating_sub(last) })); return ExitCode::SUCCESS; }
            if let Err(e) = load(&mut cat, &Want::All) { if !matches!(e, Error::Damaged(_)) { return fail(e.to_string()); } cat = project::open(&proj); }
            let t = std::time::Instant::now();
            match cat.sweep(&roots, &ours, &Sweep { deep: a.iter().any(|x| x == "--deep"), ..Default::default() }) {
                Ok(r) => {
                    let _ = std::fs::write(&stamp, now_s.to_string());
                    // The journals folded when they've grown (in the background, at low priority)
                    let folded = if cat.should_compact() || cat.manifest.is_none() { cat.compact().ok().map(|n| { let _ = cat.clean(); n }) } else { None };
                    println!("{}", json!({ "seconds": t.elapsed().as_secs_f64(), "folders": r.dirs, "listed": r.listed, "read": r.inspected, "changes": r.changes,
                                           "errors": r.errors, "roots_not_there": r.skipped_roots, "folded": folded }));
                    ExitCode::SUCCESS
                }
                Err(e) => fail(e.to_string()),
            }
        }
        "watch" => {
            // The NAS's own watcher (docs §9): at the start every folder's time looked at; then the system says what
            // changes (inotify, as Jellyfin is told), and those folders are looked at a second after it settles; a
            // look at every folder's time again when the system says it lost count, and every --every s (1800) for
            // safety. --share <name>=<path>: where a share the locations name is here
            let every: u64 = opt("--every").and_then(|x| x.parse().ok()).unwrap_or(1800);
            let shares: Vec<(String, PathBuf)> = a.windows(2).filter(|w| w[0] == "--share").filter_map(|w| w[1].split_once('=').map(|(n, p)| (n.to_string(), PathBuf::from(p)))).collect();
            let (roots, ours) = project::roots_with(&proj, &shares);
            eprintln!("afp-catalog: watching {} as {} ({} roots: {})", proj.display(), cat.me, roots.len(), roots.iter().map(|r| r.dir.display().to_string()).collect::<Vec<_>>().join(", "));
            watch(&proj, cat, &roots, &ours, every)
        }
        "fold" => {
            if let Err(e) = load(&mut cat, &Want::All) { return fail(e.to_string()); }
            match cat.compact() {
                Ok(n) => { let removed = cat.clean().unwrap_or(0); println!("{}", json!({ "snapshot": n, "removed": removed })); ExitCode::SUCCESS }
                Err(e) => fail(e.to_string()),
            }
        }
        _ => { eprintln!("{USAGE}"); ExitCode::from(1) }
    }
}
