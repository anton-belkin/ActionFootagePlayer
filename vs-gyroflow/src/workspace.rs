// Which project a clip belongs to, and where that project's files for it are (docs/moments-and-workspaces.md): the
// twin of the workspace mpv plugin's workspace.lua, for gyroflow-export run without the player (the Finder quick
// action, the command line). Both are tested over tests/workspace_cases.txt: keep the two in step.
//
// A project is a folder with `Project.afproject` in it. Its footage is in its locations (docs/library-management.md),
// each placed at a point of the project's tree, in an order; without any, the project's own folder at the top (the
// trip folder, its footage under it). What belongs to a clip (its Gyroflow project, its own file) lives beside the
// clip, one per clip over all the locations. A clip finds its project in the folders above it: a Project.afproject,
// or `<name>.afplocation` files at the top of a location, which say where their project is. Before 0.10 the mark was a
// folder, `_ActionFootagePlayer/`, which still makes a project.

use std::io::{ Read, Seek, SeekFrom };
use std::path::{ Path, PathBuf };

/// The file a project's state is kept in, which also marks a folder as one (Action Footage Library opens it)
pub const PROJECT_FILE: &str = "Project.afproject";
/// The folder it was before 0.10, which still marks a project
pub const LEGACY_DIR: &str = "_ActionFootagePlayer";

/// A volume's top folder, where looking further up for a project ends: a disk or a share as the system mounts it
/// (/Volumes/<name>, /media/<user>/<name>, /run/media/<user>/<name>, /mnt/<name>, C:\, \\nas\video), or /.
/// `windows`: the path is a Windows one
pub fn is_volume_root_str(dir: &str, windows: bool) -> bool {
    if dir == "/" { return true; }
    // Exactly `n` parts, none empty
    let parts = |rest: &str, seps: &[char], n: usize| { let v: Vec<&str> = rest.split(seps).collect(); v.len() == n && v.iter().all(|p| !p.is_empty()) };
    if windows {
        let b = dir.as_bytes();
        let drive = b.len() >= 2 && b[0].is_ascii_alphabetic() && b[1] == b':'
            && (b.len() == 2 || (b.len() == 3 && (b[2] == b'\\' || b[2] == b'/')));
        let seps = ['\\', '/'];
        let unc = (dir.starts_with("\\\\") || dir.starts_with("//"))
            && parts(dir[2..].strip_suffix(seps).unwrap_or(&dir[2..]), &seps, 2);
        return drive || unc;
    }
    let under = |prefix: &str, n: usize| dir.strip_prefix(prefix).is_some_and(|rest| parts(rest, &['/'], n));
    under("/Volumes/", 1) || under("/media/", 2) || under("/run/media/", 2) || under("/mnt/", 1)
}

pub fn is_volume_root(dir: &Path) -> bool {
    is_volume_root_str(&dir.to_string_lossy(), cfg!(windows))
}

fn same(a: &Path, b: &Path) -> bool {
    if cfg!(windows) { a.to_string_lossy().eq_ignore_ascii_case(&b.to_string_lossy()) } else { a == b }
}

/// The home folder, where looking up for a project ends (not included)
pub fn home() -> Option<PathBuf> {
    std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" }).filter(|h| !h.is_empty()).map(PathBuf::from)
}

/// The file at the top of a location that says which project it's in
pub const LOCATION_EXT: &str = ".afplocation";

fn read_json(path: &Path) -> Option<serde_json::Value> {
    std::fs::read_to_string(path).ok().and_then(|t| serde_json::from_str(&t).ok())
}

/// The shares this computer has mounted: ("//host/share", where). macOS: `mount`'s; Linux: /proc/mounts' cifs ones.
/// Windows opens \\host\share itself
pub fn mounts() -> Vec<(String, PathBuf)> {
    if let Some(m) = MOUNTS_FOR_TESTS.lock().unwrap().clone() { return m; }
    static ONCE: std::sync::OnceLock<Vec<(String, PathBuf)>> = std::sync::OnceLock::new();
    ONCE.get_or_init(|| {
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
            let unescape = |s: &str| s.replace("\\040", " ").replace("\\011", "\t");
            for line in t.lines() {
                let f: Vec<&str> = line.split(' ').collect();
                if f.len() > 2 && f[0].starts_with("//") { out.push((unescape(f[0]), PathBuf::from(unescape(f[1])))); }
            }
        }
        out
    }).clone()
}
static MOUNTS_FOR_TESTS: std::sync::Mutex<Option<Vec<(String, PathBuf)>>> = std::sync::Mutex::new(None);

fn network(path: &str) -> Option<(String, String, String)> {
    let b = path.as_bytes();
    if b.len() < 3 || !(b[0] == b'/' || b[0] == b'\\') || !(b[1] == b'/' || b[1] == b'\\') { return None; }
    let mut parts = path[2..].splitn(3, ['/', '\\']);
    let host = parts.next().filter(|h| !h.is_empty())?.to_string();
    let share = parts.next().filter(|s| !s.is_empty())?.to_string();
    Some((host, share, parts.next().unwrap_or("").to_string()))
}

fn host_key(h: &str) -> String {
    let h = h.to_lowercase();
    let h = h.rsplit_once('@').map(|(_, x)| x.to_string()).unwrap_or(h);
    let h = h.trim_end_matches('.');
    let h = h.strip_suffix("._smb._tcp.local").or_else(|| h.strip_suffix(".local")).unwrap_or(h);
    h.to_string()
}

/// Where a location's path is on this computer: a network path through the share mounted (`hosts`: other names of a
/// host), one relative to `base` (the project's folder), an absolute one as it is. None: not reachable here
pub fn place(path: &str, base: &Path, hosts: Option<&serde_json::Value>) -> Option<PathBuf> {
    if path.is_empty() { return None; }
    if let Some((host, share, rest)) = network(path) {
        if cfg!(windows) {
            let mut p = PathBuf::from(format!("\\\\{host}\\{share}"));
            for part in rest.split(['/', '\\']).filter(|p| !p.is_empty()) { p.push(part); }
            return Some(p);
        }
        let mut names = vec![host_key(&host)];
        if let Some(obj) = hosts.and_then(|h| h.as_object()) {
            for (k, list) in obj {
                let mut all = vec![k.clone()];
                all.extend(list.as_array().into_iter().flatten().filter_map(|x| x.as_str().map(str::to_string)));
                if all.iter().any(|x| host_key(x) == host_key(&host)) { names.extend(all.iter().map(|x| host_key(x))); }
            }
        }
        for (src, dir) in mounts() {
            let Some((mh, ms, _)) = network(&src) else { continue };
            if names.contains(&host_key(&mh)) && ms.eq_ignore_ascii_case(&share) {
                let mut p = dir.clone();
                for part in rest.split('/').filter(|p| !p.is_empty()) { p.push(part); }
                return Some(p);
            }
        }
        return None;
    }
    let p = Path::new(path);
    let joined = if p.is_absolute() { p.to_path_buf() } else { base.join(p) };
    // "." and ".." taken out, as the Lua's normalize does
    let mut out = PathBuf::new();
    for c in joined.components() {
        match c {
            std::path::Component::CurDir => {}
            std::path::Component::ParentDir => { out.pop(); }
            other => out.push(other.as_os_str()),
        }
    }
    Some(out)
}

/// A location of a project, on this computer
#[derive(Debug, Clone)]
pub struct Location {
    pub name: Option<String>,
    /// Its place in the project's tree: "" for the top, "Action 6"
    pub at: String,
    /// Where it is here; None: not reachable
    pub dir: Option<PathBuf>,
    pub copies: bool,
}

/// The project's locations here, in their order; a project without any: its own folder at the top
pub fn locations(project: &Path) -> Vec<Location> {
    let doc = read_json(&project.join(PROJECT_FILE)).unwrap_or(serde_json::Value::Null);
    let list: Vec<Location> = doc.get("locations").and_then(|l| l.as_array()).into_iter().flatten().filter(|l| l.is_object()).map(|l| Location {
        name: l.get("name").and_then(|n| n.as_str()).map(str::to_string),
        at: l.get("at").and_then(|a| a.as_str()).unwrap_or("").trim_matches('/').to_string(),
        dir: l.get("path").and_then(|p| p.as_str()).and_then(|p| place(p, project, doc.get("hosts"))),
        copies: l.get("copies").and_then(|c| c.as_bool()).unwrap_or(false),
    }).collect();
    if list.is_empty() { vec![Location { name: None, at: String::new(), dir: Some(project.to_path_buf()), copies: false }] } else { list }
}

/// The clip's path in the project's tree: the location holding it (the deepest, when one is in another), its place
/// plus the path under it ("Action 6/A.MP4", with "/")
pub fn tree_path(clip: &Path, locations: &[Location]) -> Option<String> {
    let mut best: Option<(usize, String)> = None;
    for l in locations {
        let Some(dir) = &l.dir else { continue };
        if let Some(rel) = relative(clip, dir) {
            let depth = dir.components().count();
            if best.as_ref().is_none_or(|(d, _)| depth > *d) {
                best = Some((depth, if l.at.is_empty() { rel } else { format!("{}/{rel}", l.at) }));
            }
        }
    }
    best.map(|(_, t)| t)
}

/// The file at a path of the tree in each location that has it, in their order
pub fn files_at<'a>(tree: &str, locations: &'a [Location]) -> Vec<(PathBuf, &'a Location)> {
    let mut out = Vec::new();
    for l in locations {
        let Some(dir) = &l.dir else { continue };
        let rel = if l.at.is_empty() { Some(tree) } else { tree.strip_prefix(&format!("{}/", l.at)) };
        let Some(rel) = rel else { continue };
        let mut f = dir.clone();
        for part in rel.split('/') { f.push(part); }
        if f.is_file() { out.push((f, l)); }
    }
    out
}

/// Where the clip's own file is over the locations, decided the same way on every computer (docs/library-management.md
/// §8): the copies in the library's locations in their order, then the copies-only ones; the main file is beside the
/// first copy that has one; none has one: beside the first copy that isn't copies only (where the first save writes
/// it). Second: the other copies' files, read never written
pub fn own_file(clip: &Path, locations: &[Location]) -> (PathBuf, Vec<PathBuf>) {
    let Some(tree) = tree_path(clip, locations) else { return (sidecar_file(clip), Vec::new()) };
    let mut copies = files_at(&tree, locations);
    copies.sort_by_key(|(_, l)| l.copies);
    let mut found: Vec<PathBuf> = copies.iter().map(|(f, _)| sidecar_file(f))
        .filter(|o| o.is_file() || o.with_extension("").with_extension("marks.json").is_file()).collect();
    let home = copies.iter().find(|(_, l)| !l.copies).map(|(f, _)| sidecar_file(f));
    if found.is_empty() { return (home.unwrap_or_else(|| sidecar_file(clip)), Vec::new()) }
    let main = found.remove(0);
    (main, found)
}

fn projects_in(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    if dir.join(PROJECT_FILE).is_file() || dir.join(LEGACY_DIR).is_dir() { out.push(dir.to_path_buf()); }
    let mut names: Vec<String> = std::fs::read_dir(dir).into_iter().flatten().flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| !n.starts_with('.') && n.to_lowercase().ends_with(LOCATION_EXT)).collect();
    names.sort();
    for n in names {
        let Some(p) = read_json(&dir.join(&n)) else { continue };
        let Some(where_) = p.get("project").and_then(|x| x.as_str()).and_then(|x| place(x, dir, p.get("hosts"))) else { continue };
        if where_.join(PROJECT_FILE).is_file() && !out.iter().any(|o| same(o, &where_)) { out.push(where_); }
    }
    out
}

/// The projects the clip (an absolute path) belongs to, all found in the nearest folder above it, or its own, that
/// has any: its Project.afproject, its .afplocation files' projects. The search ends at the volume's top folder
/// (included: a card's project is on its top) or at `home` (not included: a project of everything in it would be no
/// trip). Empty: none
pub fn projects_of(clip: &Path, home: Option<&Path>) -> Vec<PathBuf> {
    let mut dir = clip.parent();
    while let Some(d) = dir {
        if d.as_os_str().is_empty() { return vec![]; }
        if home.is_some_and(|h| same(d, h)) { return vec![]; }
        let found = projects_in(d);
        if !found.is_empty() { return found; }
        if is_volume_root(d) { return vec![]; }
        dir = d.parent();
    }
    vec![]
}

/// The first of them, None: none
pub fn project_root_in(clip: &Path, home: Option<&Path>) -> Option<PathBuf> {
    projects_of(clip, home).into_iter().next()
}

pub fn project_root(clip: &Path) -> Option<PathBuf> {
    project_root_in(clip, home().as_deref())
}

/// Where a project for the clip would be made at the first save: the clip's folder, or the folder above DCIM for a
/// clip in a camera's own tree (DCIM/100GOPRO, DCIM/DJI_001), the card or a copy of it
pub fn new_project_root(clip: &Path) -> PathBuf {
    let dir = clip.parent().unwrap_or(Path::new("."));
    let mut up = Some(dir);
    for _ in 0..2 {
        let Some(u) = up else { break };
        if u.file_name().is_some_and(|n| n.to_string_lossy().eq_ignore_ascii_case("DCIM")) {
            return u.parent().unwrap_or(u).to_path_buf();
        }
        up = u.parent();
    }
    dir.to_path_buf()
}

/// The clip's path relative to its project's folder ("Day 1/A.MP4", with "/" on every system), None when it isn't
/// under it
pub fn relative(clip: &Path, root: &Path) -> Option<String> {
    let rel = clip.strip_prefix(root).ok()?;
    let parts: Vec<String> = rel.components().map(|c| c.as_os_str().to_string_lossy().to_string()).collect();
    (!parts.is_empty()).then(|| parts.join("/"))
}

/// The clip's Gyroflow project, the paths it may have, in the order looked at: <name>.gyroflow and
/// <name.ext>.gyroflow, next to the clip (Gyroflow's own two ways of naming it)
pub fn gyroflow_projects(clip: &Path) -> [PathBuf; 2] {
    let mut long = clip.as_os_str().to_owned();
    long.push(".gyroflow");
    [clip.with_extension("gyroflow"), PathBuf::from(long)]
}

/// The first of them that exists, None: none
pub fn gyroflow_project(clip: &Path) -> Option<PathBuf> {
    gyroflow_projects(clip).into_iter().find(|p| p.is_file())
}

/// Where a new Gyroflow project for the clip goes
pub fn gyroflow_new(clip: &Path) -> PathBuf {
    clip.with_extension("gyroflow")
}

/// The clip's own file, its marks and the view it's saved to play in: <name>.afp.json next to it
pub fn sidecar_file(clip: &Path) -> PathBuf {
    clip.with_extension("afp.json")
}

#[cfg(test)]
mod tests {
    #[test]
    fn fingerprints() {
        let dir = std::env::temp_dir().join(format!("afp-fp-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let (small, big) = (dir.join("small"), dir.join("big"));
        std::fs::write(&small, b"abc").unwrap();
        // "abcabc"
        assert_eq!(super::fingerprint(&small).unwrap(), "bbb59da3af939f7af5f360f2ceb80a496e3bae1cd87dde426db0ae40677e1c2c");
        let data: Vec<u8> = (0..3u32 << 20).map(|i| (i * 7 % 251) as u8).collect();
        std::fs::write(&big, &data).unwrap();
        let mut ends = data[..1 << 20].to_vec();
        ends.extend_from_slice(&data[data.len() - (1 << 20)..]);
        use sha2::Digest;
        let want: String = sha2::Sha256::digest(&ends).iter().map(|b| format!("{b:02x}")).collect();
        assert_eq!(super::fingerprint(&big).unwrap(), want);
        // The middle doesn't count, an end does
        let mut changed = data.clone();
        changed[3 << 19] ^= 1;
        std::fs::write(&big, &changed).unwrap();
        assert_eq!(super::fingerprint(&big).unwrap(), want);
        changed[0] ^= 1;
        std::fs::write(&big, &changed).unwrap();
        assert_ne!(super::fingerprint(&big).unwrap(), want);
        let link = super::with_fingerprint("{\"v\":1}", &small);
        assert!(link.contains("\"hash\":\"bbb59da3"));
        assert_eq!(super::with_fingerprint("{\"hash\":\"x\"}", &small), "{\"hash\":\"x\"}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    use super::*;

    // tests/workspace_cases.txt, as its comment says, on a temporary folder
    #[test]
    fn cases() {
        let text = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../tests/workspace_cases.txt")).unwrap();
        let top = std::env::temp_dir().join(format!("workspace-cases-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&top);
        let mut failures = Vec::new();
        let mut checks = 0;
        let mut n = 0;
        let mut base = PathBuf::new();
        let mut home = PathBuf::new();
        let mut name = String::new();
        let mut in_tree = false;
        let mut clip: Option<PathBuf> = None;
        let at = |base: &Path, rel: &str| -> Option<PathBuf> { (rel != "-").then(|| base.join(rel)) };
        for line in text.lines() {
            if line.starts_with('#') || line.trim().is_empty() { continue; }
            if let Some(rest) = line.strip_prefix("case ") {
                n += 1;
                let dir = top.join(n.to_string());
                std::fs::create_dir_all(&dir).unwrap();
                base = dir.canonicalize().unwrap();
                home = base.clone();
                name = rest.to_string();
                in_tree = false;
                *MOUNTS_FOR_TESTS.lock().unwrap() = None;
                clip = None;
            } else if let Some(rest) = line.strip_prefix("home ") {
                home = base.join(rest);
            } else if let Some(rest) = line.strip_prefix("mount ") {
                let (src, dir) = rest.split_once(' ').unwrap();
                MOUNTS_FOR_TESTS.lock().unwrap().get_or_insert_with(Vec::new).push((src.to_string(), base.join(dir)));
            } else if line == "tree" {
                in_tree = true;
            } else if let Some(rest) = line.strip_prefix("clip ") {
                in_tree = false;
                clip = Some(base.join(rest));
            } else if let Some(rest) = line.strip_prefix("volume ") {
                let (system, rest) = rest.split_once(' ').unwrap();
                let (path, answer) = rest.rsplit_once(' ').unwrap();
                checks += 1;
                if is_volume_root_str(path, system == "windows") != (answer == "yes") {
                    failures.push(format!("{name}: {system} {path}: want {answer}"));
                }
            } else if in_tree {
                let rel = line.trim();
                if let Some((name, text)) = rel.split_once(" = ") {
                    let p = base.join(name);
                    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
                    std::fs::write(p, text).unwrap();
                } else if let Some(dir) = rel.strip_suffix('/') {
                    std::fs::create_dir_all(base.join(dir)).unwrap();
                } else {
                    let p = base.join(rel);
                    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
                    std::fs::write(p, b"").unwrap();
                }
            } else if let Some(clip) = &clip {
                let (key, value) = line.trim().split_once(' ').unwrap();
                let got: Option<String> = match key {
                    "project" => project_root_in(clip, Some(&home)).map(|p| p.display().to_string()),
                    "relative" => project_root_in(clip, Some(&home)).and_then(|r| tree_path(clip, &locations(&r))),
                    "projects" => Some(projects_of(clip, Some(&home)).iter().map(|p| relative(p, &base).unwrap_or_default()).collect::<Vec<_>>().join("|")),
                    "original" => {
                        let (tree, _) = value.rsplit_once(' ').map(|(t, _)| (t, ())).unwrap();
                        // the tree path is up to the file's extension, the file after it
                        let tree = value.split_inclusive(".MP4").next().unwrap_or(tree).trim();
                        let locs = project_root_in(clip, Some(&home)).map(|r| locations(&r)).unwrap_or_default();
                        files_at(tree, &locs).first().map(|(f, _)| f.display().to_string())
                    }
                    "own" => {
                        let locs = project_root_in(clip, Some(&home)).map(|r| locations(&r)).unwrap_or_default();
                        Some(own_file(clip, &locs).0.display().to_string())
                    }
                    "others" => {
                        let locs = project_root_in(clip, Some(&home)).map(|r| locations(&r)).unwrap_or_default();
                        Some(own_file(clip, &locs).1.iter().map(|p| p.display().to_string()).collect::<Vec<_>>().join("|"))
                    }
                    "new" => Some(new_project_root(clip).display().to_string()),
                    "gyroflow" => gyroflow_project(clip).map(|p| p.display().to_string()),
                    "gyroflow-new" => Some(gyroflow_new(clip).display().to_string()),
                    "sidecar" => Some(sidecar_file(clip).display().to_string()),
                    _ => panic!("unknown expectation {key}"),
                };
                let want = match key {
                    "relative" | "projects" => Some(value.to_string()),
                    "others" if value == "-" => Some(String::new()),
                    "others" => Some(value.split('|').filter_map(|v| at(&base, v)).map(|p| p.display().to_string()).collect::<Vec<_>>().join("|")),
                    "original" => at(&base, value.split_inclusive(".MP4").nth(1).unwrap_or("").trim()).map(|p| p.display().to_string()),
                    _ => at(&base, value).map(|p| p.display().to_string()),
                };
                checks += 1;
                if got != want { failures.push(format!("{name}: {} {key}: got {got:?}, want {want:?}", clip.display())); }
            }
        }
        let _ = std::fs::remove_dir_all(&top);
        assert!(checks >= 45, "only {checks} checks read");
        assert!(failures.is_empty(), "{} of {checks} failed:\n{}", failures.len(), failures.join("\n"));
    }

    #[test]
    fn paths() {
        assert_eq!(relative(Path::new("/a/b/Day 1/A.MP4"), Path::new("/a/b")).as_deref(), Some("Day 1/A.MP4"));
        assert_eq!(relative(Path::new("/a/bc/A.MP4"), Path::new("/a/b")), None);
        assert_eq!(gyroflow_new(Path::new("/a.b/clip")), PathBuf::from("/a.b/clip.gyroflow"));
        assert_eq!(gyroflow_projects(Path::new("/x/A.MP4"))[1], PathBuf::from("/x/A.MP4.gyroflow"));
    }
}

/// How much of each end of a file its fingerprint reads
pub const FINGERPRINT_PART: u64 = 1 << 20;

/// A file's fingerprint, kept in a link with the original's path (docs/moments-and-workspaces.md, "The link inside"),
/// to find the original again once it's been moved or renamed: SHA-256 (hex) of its first MiB followed by its last
/// MiB, the two overlapping in a file under 2 MiB. Reads 2 MiB, not a 4 GB clip off a NAS; the link's size and
/// creation time go with it. The same as `(head -c 1048576 f; tail -c 1048576 f) | shasum -a 256`
pub fn fingerprint(path: &Path) -> std::io::Result<String> {
    use sha2::{ Digest, Sha256 };
    let mut f = std::fs::File::open(path)?;
    let size = f.metadata()?.len();
    let part = size.min(FINGERPRINT_PART) as usize;
    let mut buf = vec![0u8; part];
    let mut hash = Sha256::new();
    f.read_exact(&mut buf)?;
    hash.update(&buf);
    f.seek(SeekFrom::Start(size - part as u64))?;
    f.read_exact(&mut buf)?;
    hash.update(&buf);
    Ok(hash.finalize().iter().map(|b| format!("{b:02x}")).collect())
}

/// A link's JSON with the original's fingerprint, `hash`, when it has none (`clip`: the original)
pub fn with_fingerprint(link: &str, clip: &Path) -> String {
    match serde_json::from_str::<serde_json::Value>(link) {
        Ok(serde_json::Value::Object(mut o)) if !o.contains_key("hash") => match fingerprint(clip) {
            Ok(h) => { o.insert("hash".into(), h.into()); serde_json::Value::Object(o).to_string() }
            Err(_) => link.to_string(),
        },
        _ => link.to_string(),
    }
}
