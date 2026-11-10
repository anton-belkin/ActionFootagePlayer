// SPDX-License-Identifier: GPL-3.0-or-later
//
// A project's catalog on this computer: its roots (the project's folder; its locations outside it), the rules of what's
// read in them, this computer's name and its cache for the project.

use crate::catalog::{ computer_id, machine_id, Catalog, Root };
use crate::inspect::Ours;
use crate::locations;
use crate::store::DiskStore;
use std::path::{ Path, PathBuf };
use std::sync::{ Arc, OnceLock };

/// (this computer's own files: its name; its caches)
pub fn base_dirs() -> (PathBuf, PathBuf) {
    // Tests: a home of their own
    if let Some(h) = std::env::var_os("AFP_HOME") { let h = PathBuf::from(h); return (h.join("data"), h.join("cache")); }
    let home = std::env::var_os("HOME").map(PathBuf::from).unwrap_or_default();
    if cfg!(target_os = "macos") {
        (home.join("Library/Application Support/ActionFootage"), home.join("Library/Caches/ActionFootage"))
    } else if cfg!(windows) {
        let app = std::env::var_os("APPDATA").map(PathBuf::from).unwrap_or_default();
        let local = std::env::var_os("LOCALAPPDATA").map(PathBuf::from).unwrap_or_else(|| app.clone());
        (app.join("ActionFootage"), local.join("ActionFootage"))
    } else {
        let cfg = std::env::var_os("XDG_CONFIG_HOME").map(PathBuf::from).unwrap_or_else(|| home.join(".config"));
        let cache = std::env::var_os("XDG_CACHE_HOME").map(PathBuf::from).unwrap_or_else(|| home.join(".cache"));
        (cfg.join("ActionFootage"), cache.join("ActionFootage"))
    }
}

fn host() -> String {
    if let Ok(h) = std::env::var("COMPUTERNAME") { return h; }
    if cfg!(target_os = "macos") {
        if let Ok(o) = std::process::Command::new("/usr/sbin/scutil").args(["--get", "ComputerName"]).output() {
            let h = String::from_utf8_lossy(&o.stdout).trim().to_string();
            if !h.is_empty() { return h; }
        }
    }
    std::fs::read_to_string("/etc/hostname").ok().map(|h| h.trim().to_string()).filter(|h| !h.is_empty())
        .or_else(|| std::process::Command::new("hostname").output().ok().map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string()))
        .unwrap_or_else(|| "computer".into())
}

/// This computer's name in the journals
pub fn me() -> String {
    static ME: OnceLock<String> = OnceLock::new();
    ME.get_or_init(|| computer_id(&base_dirs().0, &machine_id(), &host()).unwrap_or_else(|_| format!("{}-tmp", host()))).clone()
}

/// The project's id (Project.afproject), else one from its path
pub fn project_id(project: &Path) -> String {
    let doc = locations::read_json(&project.join(locations::PROJECT_FILE));
    doc.as_ref().and_then(|d| d.get("id")).and_then(|v| v.as_str()).filter(|i| !i.is_empty()).map(str::to_string)
        .unwrap_or_else(|| crate::snapshot::sha(project.to_string_lossy().as_bytes()))
}

/// The roots to look at, and what's read in them
pub fn roots(project: &Path) -> (Vec<Root>, Ours) {
    let mounts = locations::mounts();
    let locs = locations::locations(project, &mounts);
    let mut roots = vec![Root { id: "p".into(), dir: project.to_path_buf(), marker: Some(locations::PROJECT_FILE.into()), exclude: Vec::new() }];
    let mut footage = Vec::new();
    let marker = locations::pointer_name(project);
    for l in locs.iter().filter(|l| !l.local) {
        let Some(dir) = &l.dir else { continue };
        if let Ok(rel) = dir.strip_prefix(project) {
            footage.push(rel.components().map(|c| c.as_os_str().to_string_lossy().into_owned()).collect::<Vec<_>>().join("/"));
            continue;
        }
        if roots.iter().any(|r| r.dir == *dir) { continue; }
        // The project's folder inside this location: left out of it (it's the root "p")
        let exclude = project.strip_prefix(dir).ok().map(|r| r.components().map(|c| c.as_os_str().to_string_lossy().into_owned()).collect::<Vec<_>>().join("/"))
            .into_iter().collect();
        roots.push(Root { id: format!("l:{}", l.path), dir: dir.clone(), marker: Some(marker.clone()), exclude });
    }
    (roots, Ours { footage_in_project: footage })
}

/// A file's root and path in it (the longest root that has it, not where it's left out)
pub fn locate(roots: &[Root], path: &Path) -> Option<(String, String)> {
    roots.iter().filter_map(|r| {
        let rel = path.strip_prefix(&r.dir).ok()?;
        let rel = rel.components().map(|c| c.as_os_str().to_string_lossy().into_owned()).collect::<Vec<_>>().join("/");
        if r.exclude.iter().any(|x| rel == *x || rel.starts_with(&format!("{x}/"))) { return None; }
        Some((r.dir.components().count(), r.id.clone(), rel))
    }).max_by_key(|(n, _, _)| *n).map(|(_, id, rel)| (id, rel))
}

/// The project's catalog as this computer has it (not loaded yet)
pub fn open(project: &Path) -> Catalog {
    let cache = base_dirs().1.join("catalog").join(project_id(project));
    Catalog::new(Arc::new(DiskStore), Arc::new(DiskStore), project, &cache, &me())
}
