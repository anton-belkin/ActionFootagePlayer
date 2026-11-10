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
       afp-catalog look <project> [--deep]  look at the folders and note what changed (what the Library does)
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
    let mut cat = project::open(&proj);
    let (roots, ours) = project::roots(&proj);
    match a[0].as_str() {
        "derived" => {
            let (hash, clip) = (opt("--hash"), opt("--clip"));
            let info = match load(&mut cat, &Want::Keys(Catalog::keys_for(hash.as_deref(), clip.as_deref()))) { Ok(i) => i, Err(e) => return fail(e.to_string()) };
            if info.empty && info.snapshot.is_none() { println!("{}", json!({ "catalog": false })); return ExitCode::SUCCESS; }
            let mut out = Vec::new();
            let mut notes = Vec::new();
            for (root, rel, f) in cat.derived(hash.as_deref(), clip.as_deref()) {
                let Some(r) = roots.iter().find(|r| r.id == root) else { continue };
                let path = r.dir.join(&rel);
                // Each one looked at: what's there now is what's said (and noted when it isn't what was on record)
                match observe(&ours, &root, &rel, &path) {
                    Ok(Some(now)) if now.size == f.size && now.mtime == f.mtime => { if let Some(v) = item(&roots, &root, &rel, &f) { out.push(v); } }
                    Ok(now) => {
                        if let Some(n) = &now {
                            let lh = n.meta.as_ref().and_then(|m| m["link"]["hash"].as_str()).map(str::to_string);
                            let lc = n.meta.as_ref().and_then(|m| m["link"]["clip"].as_str()).map(str::to_string);
                            if (hash.is_some() && lh == hash) || (lh.is_none() && clip.is_some() && lc == clip) { if let Some(v) = item(&roots, &root, &rel, n) { out.push(v); } }
                        }
                        notes.push((root, rel, now));
                    }
                    Err(_) => { if let Some(v) = item(&roots, &root, &rel, &f) { out.push(v); } }
                }
            }
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
            if let Err(e) = load(&mut cat, &Want::All) { return fail(e.to_string()); }
            let t = std::time::Instant::now();
            match cat.sweep(&roots, &ours, &Sweep { deep: a.iter().any(|x| x == "--deep"), ..Default::default() }) {
                Ok(r) => { println!("{}", json!({ "seconds": t.elapsed().as_secs_f64(), "folders": r.dirs, "listed": r.listed, "read": r.inspected, "changes": r.changes,
                                                  "errors": r.errors, "roots_not_there": r.skipped_roots })); ExitCode::SUCCESS }
                Err(e) => fail(e.to_string()),
            }
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
