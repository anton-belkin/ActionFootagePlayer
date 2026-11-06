// Which project a clip belongs to, and where that project's files for it are (docs/moments-and-workspaces.md): the
// twin of the workspace mpv plugin's workspace.lua, for gyroflow-export run without the player (the Finder quick
// action, the command line). Both are tested over tests/workspace_cases.txt: keep the two in step.
//
// A project is a folder with `Project.afproject` in it, the trip folder: its footage is under it, and the player's
// files for a clip sit at the clip's own path in it (its Gyroflow project, its marks), next to the clip. Before 0.10 the
// mark was a folder, `_ActionFootagePlayer/`, which still makes a project.

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

/// The project the clip (an absolute path) belongs to: the nearest folder above it, or its own, with the project's
/// file (or the folder of before) in it. The search ends at the volume's top folder (included: a card's project is on its top) or at `home` (not
/// included: a project of everything in it would be no trip). None: none
pub fn project_root_in(clip: &Path, home: Option<&Path>) -> Option<PathBuf> {
    let mut dir = clip.parent();
    while let Some(d) = dir {
        if d.as_os_str().is_empty() { return None; }
        if home.is_some_and(|h| same(d, h)) { return None; }
        if d.join(PROJECT_FILE).is_file() || d.join(LEGACY_DIR).is_dir() { return Some(d.to_path_buf()); }
        if is_volume_root(d) { return None; }
        dir = d.parent();
    }
    None
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
                clip = None;
            } else if let Some(rest) = line.strip_prefix("home ") {
                home = base.join(rest);
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
                if let Some(dir) = rel.strip_suffix('/') {
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
                    "relative" => project_root_in(clip, Some(&home)).and_then(|r| relative(clip, &r)),
                    "new" => Some(new_project_root(clip).display().to_string()),
                    "gyroflow" => gyroflow_project(clip).map(|p| p.display().to_string()),
                    "gyroflow-new" => Some(gyroflow_new(clip).display().to_string()),
                    "sidecar" => Some(sidecar_file(clip).display().to_string()),
                    _ => panic!("unknown expectation {key}"),
                };
                let want = if key == "relative" { Some(value.to_string()) } else { at(&base, value).map(|p| p.display().to_string()) };
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
