// SPDX-License-Identifier: GPL-3.0-or-later
//
// Which project a clip belongs to, and where that project's files for it are (docs/moments-and-workspaces.md):
// the one Rust copy, used by gyroflow-export and its tools (vs-gyroflow/src/workspace.rs re-exports it), the Library
// (tags.rs's clip_own) and afp-catalog's own command line. The player's twin is mpv/scripts/workspace/workspace.lua;
// both are tested over tests/workspace_cases.txt (this file's test below, tests/workspace_test.lua): keep them in step.
//
// A project is a folder with `Project.afproject` in it. Its footage is in its locations (locations.rs), each placed at
// a point of the project's tree, in an order; without any, the project's own folder at the top (the trip folder, its
// footage under it). What belongs to a clip (its Gyroflow project, its own file) lives beside the clip, one per clip
// over all the locations. A clip finds its project in the folders above it: a Project.afproject, or `<name>.afplocation`
// files at the top of a location, which say where their project is (locations.rs's pointer_project: only the project
// of the pointer's id, CLAUDE.md #299). Before 0.10 the mark was a folder, `_ActionFootagePlayer/`, which still makes
// a project.

use crate::locations::{ self, Location, LOCATION_EXT, PROJECT_FILE };
use std::io::{ Read, Seek, SeekFrom };
use std::path::{ Path, PathBuf };

/// The folder it was before 0.10, which still marks a project
pub const LEGACY_DIR: &str = "_ActionFootagePlayer";

pub use crate::locations::relative;

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

/// The projects whose markers are in a folder: its Project.afproject (or the folder of before 0.10), then its
/// .afplocation files' projects (those that exist, of the pointer's id), in the order of their names
fn projects_in(dir: &Path, mounts: &[(String, PathBuf)]) -> Vec<PathBuf> {
    let mut out = Vec::new();
    crate::store::nas::stat();
    if dir.join(PROJECT_FILE).is_file() || dir.join(LEGACY_DIR).is_dir() { out.push(dir.to_path_buf()); }
    crate::store::nas::read();
    let mut names: Vec<String> = std::fs::read_dir(dir).into_iter().flatten().flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| !n.starts_with('.') && n.to_lowercase().ends_with(LOCATION_EXT)).collect();
    names.sort();
    for n in names {
        let Some(p) = locations::read_json(&dir.join(&n)) else { continue };
        let Some(where_) = locations::pointer_project(&p, dir, mounts) else { continue };
        if !out.iter().any(|o| same(o, &where_)) { out.push(where_); }
    }
    out
}

/// The projects the clip (an absolute path) belongs to, all found in the nearest folder above it, or its own, that
/// has any: its Project.afproject, its .afplocation files' projects. The search ends at the volume's top folder
/// (included: a card's project is on its top) or at `home` (not included: a project of everything in it would be no
/// trip). Empty: none
pub fn projects_of(clip: &Path, home: Option<&Path>, mounts: &[(String, PathBuf)]) -> Vec<PathBuf> {
    let mut dir = clip.parent();
    while let Some(d) = dir {
        if d.as_os_str().is_empty() { return vec![]; }
        if home.is_some_and(|h| same(d, h)) { return vec![]; }
        let found = projects_in(d, mounts);
        if !found.is_empty() { return found; }
        if is_volume_root(d) { return vec![]; }
        dir = d.parent();
    }
    vec![]
}

/// The first of them, None: none
pub fn project_root_in(clip: &Path, home: Option<&Path>, mounts: &[(String, PathBuf)]) -> Option<PathBuf> {
    projects_of(clip, home, mounts).into_iter().next()
}

/// The clip's project on this computer (the shares mounted now, the search ending at the home folder)
pub fn project_root(clip: &Path) -> Option<PathBuf> {
    project_root_in(clip, home().as_deref(), &locations::mounts())
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

fn is_file(f: &Path) -> bool {
    // (nas::stat: a share's slowness stood in for when measuring, nothing otherwise)
    crate::store::nas::stat();
    f.is_file()
}

/// The file at a path of the tree in each location that has it, in their order
pub fn files_at<'a>(tree: &str, locations: &'a [Location]) -> Vec<(PathBuf, &'a Location)> {
    locations.iter().filter_map(|l| locations::in_location(tree, l).filter(|f| is_file(f)).map(|f| (f, l))).collect()
}

/// A clip's own file over the project's locations, as `own_at` decides it
#[derive(Debug, Clone, PartialEq)]
pub struct Own {
    /// The main one: read and written
    pub file: PathBuf,
    /// The copy of the clip it's beside
    pub clip: PathBuf,
    /// The other copies' own files: read, never written
    pub others: Vec<PathBuf>,
    /// Found or where the first save writes it (beside the first copy that isn't copies only); false: every copy is a
    /// copies-only one or this computer's, `file` is beside the first of them
    pub settled: bool,
}

/// Where the own file of the clip at a path of the tree is, decided the same way on every computer
/// (docs/library-management.md §8): the copies in the library's locations in their order, then the
/// copies-only ones and this computer's own; the main file is beside the first copy that has one (its .afp.json, or
/// the .marks.json of before view settings); none has one: beside the first copy that isn't copies only (where the
/// first save writes it). None: the path is in no location here
pub fn own_at(tree: &str, locations: &[Location]) -> Option<Own> {
    let mut copies = files_at(tree, locations);
    // Stable: the locations' order within each
    copies.sort_by_key(|(_, l)| l.copies || l.local);
    let mut found: Vec<(PathBuf, PathBuf)> = copies.iter().map(|(f, _)| (sidecar_file(f), f.clone()))
        .filter(|(o, _)| is_file(o) || is_file(&old_sidecar_file(o))).collect();
    if !found.is_empty() {
        let (file, clip) = found.remove(0);
        return Some(Own { file, clip, others: found.into_iter().map(|(o, _)| o).collect(), settled: true });
    }
    let (clip, settled) = match copies.iter().find(|(_, l)| !(l.copies || l.local)) {
        Some((f, _)) => (f.clone(), true),
        None => (copies.first()?.0.clone(), false),
    };
    Some(Own { file: sidecar_file(&clip), clip, others: Vec::new(), settled })
}

/// The clip's own file (the main one) and the other copies', over the locations; the clip in none of them, or each
/// copy of it in a copies-only location or this computer's with none: beside the clip
pub fn own_file(clip: &Path, locations: &[Location]) -> (PathBuf, Vec<PathBuf>) {
    match tree_path(clip, locations).and_then(|tree| own_at(&tree, locations)) {
        Some(o) if o.settled => (o.file, o.others),
        _ => (sidecar_file(clip), Vec::new()),
    }
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

/// The clip's own file, its marks and the view it's saved to play in: <name>.afp.json next to it (sidecar.lua's)
pub fn sidecar_file(clip: &Path) -> PathBuf {
    clip.with_extension("afp.json")
}

/// The name it had before view settings, marks only: <name>.marks.json (beside an own file: its old name)
pub fn old_sidecar_file(own: &Path) -> PathBuf {
    own.with_extension("").with_extension("marks.json")
}

/// How much of each end of a file its fingerprint reads
pub const FINGERPRINT_PART: u64 = 1 << 20;

/// A file's fingerprint, kept in a link with the original's path (docs/moments-and-workspaces.md, "The link
/// inside"), to find the original again once it's been moved or renamed: SHA-256 (hex) of its first MiB followed by
/// its last MiB, the two overlapping in a file under 2 MiB. Reads 2 MiB, not a 4 GB clip off a NAS; the link's size and
/// creation time go with it. The same as `(head -c 1048576 f; tail -c 1048576 f) | shasum -a 256`
pub fn fingerprint(path: &Path) -> std::io::Result<String> {
    use sha2::{ Digest, Sha256 };
    crate::store::nas::read();
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
    Ok(hex(&hash.finalize()))
}

/// SHA-256 of some bytes, in hex
pub fn sha256_hex(data: &[u8]) -> String {
    use sha2::{ Digest, Sha256 };
    hex(&Sha256::digest(data))
}

fn hex(b: &[u8]) -> String { b.iter().map(|b| format!("{b:02x}")).collect() }

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::locations::PROJECTS_DIR_FOR_TESTS;

    #[test]
    fn fingerprints() {
        let dir = std::env::temp_dir().join(format!("afp-fp-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let (small, big) = (dir.join("small"), dir.join("big"));
        std::fs::write(&small, b"abc").unwrap();
        // "abcabc"
        assert_eq!(fingerprint(&small).unwrap(), "bbb59da3af939f7af5f360f2ceb80a496e3bae1cd87dde426db0ae40677e1c2c");
        let data: Vec<u8> = (0..3u32 << 20).map(|i| (i * 7 % 251) as u8).collect();
        std::fs::write(&big, &data).unwrap();
        let mut ends = data[..1 << 20].to_vec();
        ends.extend_from_slice(&data[data.len() - (1 << 20)..]);
        let want = sha256_hex(&ends);
        assert_eq!(fingerprint(&big).unwrap(), want);
        // The middle doesn't count, an end does
        let mut changed = data.clone();
        changed[3 << 19] ^= 1;
        std::fs::write(&big, &changed).unwrap();
        assert_eq!(fingerprint(&big).unwrap(), want);
        changed[0] ^= 1;
        std::fs::write(&big, &changed).unwrap();
        assert_ne!(fingerprint(&big).unwrap(), want);
        let link = with_fingerprint("{\"v\":1}", &small);
        assert!(link.contains("\"hash\":\"bbb59da3"));
        assert_eq!(with_fingerprint("{\"hash\":\"x\"}", &small), "{\"hash\":\"x\"}");
        // SHA-256's own test vectors
        assert_eq!(sha256_hex(b""), "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855");
        assert_eq!(sha256_hex(b"abc"), "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad");
        let _ = std::fs::remove_dir_all(&dir);
    }

    // tests/workspace_cases.txt, as its comment says, on a temporary folder: everything the cases say, as the player's
    // workspace.lua is checked against them
    #[test]
    fn workspace_cases() {
        let text = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../tests/workspace_cases.txt")).unwrap();
        let top = std::env::temp_dir().join(format!("catalog-workspace-cases-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&top);
        let (mut failures, mut checks, mut n) = (Vec::new(), 0, 0);
        let (mut base, mut home, mut name, mut in_tree) = (PathBuf::new(), PathBuf::new(), String::new(), false);
        let mut mounts: Vec<(String, PathBuf)> = Vec::new();
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
                mounts.clear();
                PROJECTS_DIR_FOR_TESTS.with(|d| *d.borrow_mut() = Some(base.join("no records")));
                clip = None;
            } else if let Some(rest) = line.strip_prefix("home ") {
                home = base.join(rest);
            } else if let Some(rest) = line.strip_prefix("known ") {
                let d = base.join(rest);
                PROJECTS_DIR_FOR_TESTS.with(|x| *x.borrow_mut() = Some(d));
            } else if let Some(rest) = line.strip_prefix("mount ") {
                let (src, dir) = rest.split_once(' ').unwrap();
                mounts.push((src.to_string(), base.join(dir)));
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
                if let Some((file, text)) = rel.split_once(" = ") {
                    let p = base.join(file);
                    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
                    std::fs::write(p, text.replace("@/", &format!("{}/", base.display()))).unwrap();
                } else if let Some(dir) = rel.strip_suffix('/') {
                    std::fs::create_dir_all(base.join(dir)).unwrap();
                } else {
                    let p = base.join(rel);
                    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
                    std::fs::write(p, b"").unwrap();
                }
            } else if let Some(clip) = &clip {
                let (key, value) = line.trim().split_once(' ').unwrap();
                let root = || project_root_in(clip, Some(&home), &mounts);
                let locs = || root().map(|r| locations::locations(&r, &mounts)).unwrap_or_default();
                let got: Option<String> = match key {
                    "project" => root().map(|p| p.display().to_string()),
                    "relative" => root().and_then(|r| tree_path(clip, &locations::locations(&r, &mounts))),
                    "projects" => Some(projects_of(clip, Some(&home), &mounts).iter().map(|p| relative(p, &base).unwrap_or_default()).collect::<Vec<_>>().join("|")),
                    "original" => {
                        // the tree path is up to the file's extension, the file after it
                        let tree = value.split_inclusive(".MP4").next().unwrap_or("").trim();
                        files_at(tree, &locs()).first().map(|(f, _)| f.display().to_string())
                    }
                    "own" => Some(own_file(clip, &locs()).0.display().to_string()),
                    "others" => Some(own_file(clip, &locs()).1.iter().map(|p| p.display().to_string()).collect::<Vec<_>>().join("|")),
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
        PROJECTS_DIR_FOR_TESTS.with(|d| *d.borrow_mut() = None);
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
        // As the player's sidecar.lua names it: the last extension of the name replaced, a name without one kept whole
        assert_eq!(sidecar_file(Path::new("/a.b/VID_1_00_005.insv")), PathBuf::from("/a.b/VID_1_00_005.afp.json"));
        assert_eq!(sidecar_file(Path::new("clip")), PathBuf::from("clip.afp.json"));
        assert_eq!(old_sidecar_file(Path::new("/x/A.afp.json")), PathBuf::from("/x/A.marks.json"));
    }

    /// This computer's own location (a copy on its disk) comes last, as a copies-only one does: the library's copy's
    /// own file is the main one, as the player's workspace.lua decides it (its `here` locations). Before this module,
    /// gyroflow-export didn't read this computer's locations at all, and the Library didn't count a .marks.json
    #[test]
    fn this_computers_copy_and_an_old_marks_file() {
        let top = std::env::temp_dir().join(format!("afp-own-local-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&top);
        let (p, archive, mine, records) = (top.join("SSD/P"), top.join("HDD/Footage"), top.join("Local/Copy"), top.join("records"));
        for d in [&p, &archive, &mine, &records] { std::fs::create_dir_all(d).unwrap(); }
        std::fs::write(p.join(PROJECT_FILE), r#"{"id":"p-7","locations":[{"name":"Archive","at":"","path":"../../HDD/Footage"}]}"#).unwrap();
        std::fs::write(records.join("p-7.json"), format!(r#"{{"locations":[{{"name":"Mine","at":"","path":"{}"}}]}}"#, mine.display())).unwrap();
        PROJECTS_DIR_FOR_TESTS.with(|d| *d.borrow_mut() = Some(records.clone()));
        let locs = locations::locations(&p, &[]);
        assert!(locs[0].local, "this computer's own first in the list");
        for d in [&archive, &mine] { std::fs::write(d.join("A.MP4"), b"").unwrap(); }
        // Neither has an own file: the archive's is where the first save writes it, whichever copy is opened
        assert_eq!(own_file(&mine.join("A.MP4"), &locs), (archive.join("A.afp.json"), vec![]));
        assert_eq!(own_at("A.MP4", &locs).unwrap(), Own { file: archive.join("A.afp.json"), clip: archive.join("A.MP4"), others: vec![], settled: true });
        // Only this computer's copy has one: it's the main one
        std::fs::write(mine.join("A.afp.json"), b"{}").unwrap();
        assert_eq!(own_file(&archive.join("A.MP4"), &locs), (mine.join("A.afp.json"), vec![]));
        // The archive's, of before view settings (.marks.json): the archive's is the main one, this computer's read only
        std::fs::write(archive.join("A.marks.json"), b"{}").unwrap();
        assert_eq!(own_file(&mine.join("A.MP4"), &locs), (archive.join("A.afp.json"), vec![mine.join("A.afp.json")]));
        assert_eq!(own_at("A.MP4", &locs).unwrap().clip, archive.join("A.MP4"));
        // A clip only in this computer's copy: beside it, not settled
        std::fs::write(mine.join("B.MP4"), b"").unwrap();
        assert_eq!(own_at("B.MP4", &locs).unwrap(), Own { file: mine.join("B.afp.json"), clip: mine.join("B.MP4"), others: vec![], settled: false });
        assert_eq!(own_file(&mine.join("B.MP4"), &locs).0, mine.join("B.afp.json"));
        assert!(own_at("C.MP4", &locs).is_none(), "in no location");
        PROJECTS_DIR_FOR_TESTS.with(|d| *d.borrow_mut() = None);
        let _ = std::fs::remove_dir_all(&top);
    }
}
