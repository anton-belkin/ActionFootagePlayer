// SPDX-License-Identifier: GPL-3.0-or-later
//
// A project's locations (docs/library-management.md): where its footage is, each placed at a point of the project's
// tree, in an order, the first that has a clip winning. The twin of vs-gyroflow's workspace.rs and the workspace
// plugin's workspace.lua for the Library, which doesn't link the Gyroflow engine: keep the three in step (their shared
// cases are tests/workspace_cases.txt; this one's own tests are below).
//
// What one computer adds (copies of footage on its own disk) is kept on it, by the project's id, in
// <data>/ActionFootage/projects/<id>.json ({ "locations": [...] }): looked at before the project's own.

use serde_json::{ json, Value };
use std::path::{ Path, PathBuf };

pub const PROJECT_FILE: &str = "Project.afproject";
pub const LOCATION_EXT: &str = ".afplocation";

#[derive(Debug, Clone, serde::Serialize)]
pub struct Location {
    pub name: String,
    /// Its place in the project's tree: "" for the top
    pub at: String,
    /// As written in the project: a network path, or relative to the project's folder
    pub path: String,
    /// Where it is here; None: not reachable now
    pub dir: Option<PathBuf>,
    pub copies: bool,
    /// One of this computer's own (not the library's)
    pub local: bool,
}

/// The shares this computer has mounted: ("//host/share", where)
pub fn mounts() -> Vec<(String, PathBuf)> {
    let mut out = Vec::new();
    if cfg!(target_os = "macos") {
        if let Ok(o) = std::process::Command::new("/sbin/mount").output() {
            for line in String::from_utf8_lossy(&o.stdout).lines() {
                if let Some((src, rest)) = line.split_once(" on ") {
                    if let Some((dir, _)) = rest.rsplit_once(" (") {
                        if src.starts_with("//") { out.push((src.to_string(), PathBuf::from(dir))); }
                    }
                }
            }
        }
    } else if let Ok(t) = std::fs::read_to_string("/proc/mounts") {
        for line in t.lines() {
            let f: Vec<&str> = line.split(' ').collect();
            if f.len() > 2 && f[0].starts_with("//") { out.push((f[0].replace("\\040", " "), PathBuf::from(f[1].replace("\\040", " ")))); }
        }
    }
    out
}

pub fn network(path: &str) -> Option<(String, String, String)> {
    let b = path.as_bytes();
    if b.len() < 3 || !(b[0] == b'/' || b[0] == b'\\') || !(b[1] == b'/' || b[1] == b'\\') { return None; }
    let mut parts = path[2..].splitn(3, ['/', '\\']);
    let host = parts.next().filter(|h| !h.is_empty())?.to_string();
    let share = parts.next().filter(|s| !s.is_empty())?.to_string();
    Some((host, share, parts.next().unwrap_or("").trim_matches(['/', '\\']).to_string()))
}

pub fn host_key(h: &str) -> String {
    let h = h.to_lowercase();
    let h = h.rsplit_once('@').map(|(_, x)| x.to_string()).unwrap_or(h);
    let h = h.trim_end_matches('.');
    h.strip_suffix("._smb._tcp.local").or_else(|| h.strip_suffix(".local")).unwrap_or(h).to_string()
}

fn clean(p: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for c in p.components() {
        match c {
            std::path::Component::CurDir => {}
            std::path::Component::ParentDir => { out.pop(); }
            other => out.push(other.as_os_str()),
        }
    }
    out
}

/// Where a location's path is on this computer, None: not reachable
pub fn place(path: &str, base: &Path, hosts: Option<&Value>, mounts: &[(String, PathBuf)]) -> Option<PathBuf> {
    if path.is_empty() { return None; }
    if let Some((host, share, rest)) = network(path) {
        if cfg!(windows) {
            let mut p = PathBuf::from(format!("\\\\{host}\\{share}"));
            for part in rest.split(['/', '\\']).filter(|p| !p.is_empty()) { p.push(part); }
            return Some(p);
        }
        let mut names = vec![host_key(&host)];
        if let Some(obj) = hosts.and_then(Value::as_object) {
            for (k, list) in obj {
                let mut all = vec![k.clone()];
                all.extend(list.as_array().into_iter().flatten().filter_map(|x| x.as_str().map(str::to_string)));
                if all.iter().any(|x| host_key(x) == host_key(&host)) { names.extend(all.iter().map(|x| host_key(x))); }
            }
        }
        for (src, dir) in mounts {
            let Some((mh, ms, _)) = network(src) else { continue };
            if names.contains(&host_key(&mh)) && ms.eq_ignore_ascii_case(&share) {
                let mut p = dir.clone();
                for part in rest.split('/').filter(|p| !p.is_empty()) { p.push(part); }
                return Some(p);
            }
        }
        return None;
    }
    let p = Path::new(path);
    Some(clean(&if p.is_absolute() { p.to_path_buf() } else { base.join(p) }))
}

/// A folder of this computer as a location's path: a network path when it's on a mounted share (the same on every
/// computer), one relative to the project's folder when it's on the same disk as the project, else as it is
pub fn path_for(dir: &Path, project: &Path, mounts: &[(String, PathBuf)]) -> String {
    let mut best: Option<(usize, String)> = None;
    for (src, mdir) in mounts {
        if let Ok(rest) = dir.strip_prefix(mdir) {
            let Some((host, share, _)) = network(src) else { continue };
            let host = src.trim_start_matches('/').split('/').next().map(|h| h.rsplit_once('@').map(|(_, x)| x).unwrap_or(h)).unwrap_or(&host);
            let host = host.strip_suffix("._smb._tcp.local").or_else(|| host.strip_suffix(".local")).unwrap_or(host);
            let rest: Vec<String> = rest.components().map(|c| c.as_os_str().to_string_lossy().into_owned()).collect();
            let n = mdir.components().count();
            if best.as_ref().is_none_or(|(b, _)| n > *b) {
                best = Some((n, format!("//{host}/{share}{}", rest.iter().map(|r| format!("/{r}")).collect::<String>())));
            }
        }
    }
    if let Some((_, p)) = best { return p; }
    // On the project's own disk: relative to it
    let (a, b): (Vec<_>, Vec<_>) = (dir.components().collect(), project.components().collect());
    let common = a.iter().zip(b.iter()).take_while(|(x, y)| x == y).count();
    if common > 1 {
        let mut parts: Vec<String> = vec!["..".into(); b.len() - common];
        parts.extend(a[common..].iter().map(|c| c.as_os_str().to_string_lossy().into_owned()));
        return if parts.is_empty() { ".".into() } else { parts.join("/") };
    }
    dir.display().to_string()
}

pub fn read_json(path: &Path) -> Option<Value> {
    std::fs::read_to_string(path).ok().and_then(|t| serde_json::from_str(&t).ok())
}

/// This computer's own file for a project (its id): <data>/ActionFootage/projects/<id>.json
pub fn local_file(id: &str) -> Option<PathBuf> {
    let base = if cfg!(target_os = "macos") {
        PathBuf::from(std::env::var_os("HOME")?).join("Library/Application Support")
    } else if cfg!(windows) {
        PathBuf::from(std::env::var_os("APPDATA")?)
    } else {
        std::env::var_os("XDG_CONFIG_HOME").map(PathBuf::from).or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))?
    };
    (!id.is_empty() && id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')).then(|| base.join("ActionFootage/projects").join(format!("{id}.json")))
}

/// "I never rename my footage" (Project settings, this computer's choices, kept in its own file): a clip's name counts
/// when it's looked for (checks.rs's find_clip)
pub fn never_rename(project: &Path) -> bool {
    let doc = read_json(&project.join(PROJECT_FILE)).unwrap_or(Value::Null);
    doc.get("id").and_then(Value::as_str).and_then(local_file).and_then(|f| read_json(&f))
        .is_some_and(|l| l.get("never_rename").and_then(Value::as_bool) == Some(true))
}

/// The tag stores the library syncs besides its own (docs/tags-and-culling.md): Project.afproject's `tag_stores`
/// ("finder"), the library's choice, the same from every computer
pub fn tag_stores(project: &Path) -> Vec<String> {
    read_json(&project.join(PROJECT_FILE)).and_then(|d| d.get("tag_stores").cloned()).and_then(|v| serde_json::from_value::<Vec<String>>(v).ok())
        .unwrap_or_default().into_iter().filter(|s| s == "finder").collect()
}

/// The tags used last on this computer, the player's and the Library's (<data>/ActionFootage/tags-recent.json)
pub fn recent_tags_file() -> Option<PathBuf> {
    local_file("x").and_then(|f| f.parent().and_then(Path::parent).map(|d| d.join("tags-recent.json")))
}

fn from_json(list: Option<&Value>, project: &Path, hosts: Option<&Value>, mounts: &[(String, PathBuf)], local: bool) -> Vec<Location> {
    list.and_then(Value::as_array).into_iter().flatten().filter(|l| l.is_object()).map(|l| {
        let path = l.get("path").and_then(Value::as_str).unwrap_or("").to_string();
        Location {
            name: l.get("name").and_then(Value::as_str).unwrap_or("").to_string(),
            at: l.get("at").and_then(Value::as_str).unwrap_or("").trim_matches('/').to_string(),
            dir: place(&path, project, hosts, mounts),
            path,
            copies: local || l.get("copies").and_then(Value::as_bool).unwrap_or(false),
            local,
        }
    }).collect()
}

/// The project's locations here: this computer's own first, then the library's in their order; a project without
/// any: its own folder at the top
pub fn locations(project: &Path, mounts: &[(String, PathBuf)]) -> Vec<Location> {
    let doc = read_json(&project.join(PROJECT_FILE)).unwrap_or(Value::Null);
    let mut list = Vec::new();
    if let Some(f) = doc.get("id").and_then(Value::as_str).and_then(local_file) {
        if let Some(l) = read_json(&f) { list.extend(from_json(l.get("locations"), project, doc.get("hosts"), mounts, true)); }
    }
    let own = from_json(doc.get("locations"), project, doc.get("hosts"), mounts, false);
    if own.is_empty() {
        list.push(Location { name: "Project folder".into(), at: String::new(), path: ".".into(), dir: Some(project.to_path_buf()), copies: false, local: false });
    } else {
        list.extend(own);
    }
    list
}

#[allow(dead_code)]
pub fn relative(file: &Path, dir: &Path) -> Option<String> {
    let rel = file.strip_prefix(dir).ok()?;
    let parts: Vec<String> = rel.components().map(|c| c.as_os_str().to_string_lossy().to_string()).collect();
    (!parts.is_empty()).then(|| parts.join("/"))
}

/// A tree path's file in a location, when it's in that location's part of the tree
pub fn in_location(tree: &str, l: &Location) -> Option<PathBuf> {
    let dir = l.dir.as_ref()?;
    let rel = if l.at.is_empty() { Some(tree) } else { tree.strip_prefix(&format!("{}/", l.at)) }?;
    let mut f = dir.clone();
    for part in rel.split('/') { f.push(part); }
    Some(f)
}

/// The ids written in a location's .afplocation files: which projects use it
pub fn pointer_name(project: &Path) -> String {
    format!("{}{LOCATION_EXT}", project.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| "Project".into()))
}

/// Writes the project's .afplocation file at the top of each of its reachable locations (not this computer's own),
/// and removes it from `gone` (locations taken out): so that a clip opened from Finder finds the project
pub fn write_pointers(project: &Path, list: &[Location], gone: &[Location], mounts: &[(String, PathBuf)]) -> Vec<String> {
    let mut problems = Vec::new();
    let doc = read_json(&project.join(PROJECT_FILE)).unwrap_or(Value::Null);
    let name = pointer_name(project);
    for l in list.iter().filter(|l| !l.local) {
        let Some(dir) = &l.dir else { problems.push(format!("{}: not reachable, its {name} wasn't written", l.name)); continue };
        if dir == project { continue; }
        let pointer = json!({ "format": "actionfootage-location", "version": 1, "project": path_for(project, dir, mounts),
                              "id": doc.get("id"), "at": l.at, "name": l.name });
        let file = dir.join(&name);
        let tmp = dir.join(format!("{name}.tmp"));
        if let Err(e) = std::fs::write(&tmp, serde_json::to_vec_pretty(&pointer).unwrap()).and_then(|_| std::fs::rename(&tmp, &file)) {
            problems.push(format!("{}: {e}", file.display()));
        }
    }
    for l in gone.iter().filter(|l| !l.local) {
        if let Some(dir) = &l.dir {
            if list.iter().any(|x| x.dir.as_ref() == Some(dir)) { continue; }
            let _ = std::fs::remove_file(dir.join(&name));
        }
    }
    problems
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn network_paths() {
        assert_eq!(network("//DISKSTATIONDXP/video/Footage/x"), Some(("DISKSTATIONDXP".into(), "video".into(), "Footage/x".into())));
        assert_eq!(network("\\\\nas\\video"), Some(("nas".into(), "video".into(), "".into())));
        assert_eq!(network("/Volumes/video"), None);
        assert_eq!(host_key("antonadmin@DISKSTATIONDXP._smb._tcp.local"), "diskstationdxp");
        let m = vec![("//antonadmin@DISKSTATIONDXP._smb._tcp.local/video".to_string(), PathBuf::from("/Volumes/video"))];
        if !cfg!(windows) {
            assert_eq!(place("//diskstationdxp/video/Footage/2026", Path::new("/p"), None, &m), Some(PathBuf::from("/Volumes/video/Footage/2026")));
            let hosts = json!({ "DISKSTATIONDXP": ["web.nas"] });
            assert_eq!(place("//web.nas/VIDEO", Path::new("/p"), Some(&hosts), &m), Some(PathBuf::from("/Volumes/video")));
            assert_eq!(place("//other/video", Path::new("/p"), None, &m), None);
            assert_eq!(place("../Footage", Path::new("/Volumes/ssd/P"), None, &m), Some(PathBuf::from("/Volumes/ssd/Footage")));
            assert_eq!(path_for(Path::new("/Volumes/video/Footage"), Path::new("/Volumes/ssd/P"), &m), "//DISKSTATIONDXP/video/Footage");
            assert_eq!(path_for(Path::new("/Users/a/Trips/F"), Path::new("/Users/a/Trips/P"), &[]), "../F");
            assert_eq!(path_for(Path::new("/Users/a/P"), Path::new("/Users/a/P"), &[]), ".");
        }
    }

    #[test]
    fn locations_and_pointers() {
        let top = std::env::temp_dir().join(format!("afl-locs-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&top);
        let (p, a, w) = (top.join("SSD/P"), top.join("HDD/Footage"), top.join("SSD/Work"));
        for d in [&p, &a, &w] { std::fs::create_dir_all(d).unwrap(); }
        std::fs::write(p.join(PROJECT_FILE), r#"{"locations":[{"name":"Working","at":"","path":"../Work","copies":true},{"name":"Archive","at":"","path":"../../HDD/Footage"},{"name":"Gone","at":"x","path":"//nowhere/share"}]}"#).unwrap();
        let l = locations(&p, &[]);
        assert_eq!(l.len(), 3);
        assert_eq!(l[0].dir.as_deref(), Some(w.as_path()));
        assert!(l[0].copies);
        assert_eq!(l[1].dir.as_deref(), Some(a.as_path()));
        assert!(l[2].dir.is_none());
        let problems = write_pointers(&p, &l, &[], &[]);
        assert_eq!(problems.len(), 1, "{problems:?}");
        let ptr = read_json(&a.join("P.afplocation")).unwrap();
        assert_eq!(ptr["project"], json!("../../SSD/P"));
        assert_eq!(place(ptr["project"].as_str().unwrap(), &a, None, &[]), Some(p.clone()));
        assert_eq!(in_location("Day 1/A.MP4", &l[1]), Some(a.join("Day 1/A.MP4")));
        write_pointers(&p, &l[..1], &l[1..2], &[]);
        assert!(!a.join("P.afplocation").exists());
        let _ = std::fs::remove_dir_all(&top);
    }
}
