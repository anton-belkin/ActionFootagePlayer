// Which project a clip belongs to, and where that project's files for it are (docs/design/moments-and-workspaces.md):
// afp-catalog's workspace.rs, the one Rust copy (the Library uses it too), with this computer's mounted shares read once
// per process. The player's twin is the workspace plugin's workspace.lua; both run tests/workspace_cases.txt.

use std::path::{ Path, PathBuf };

pub use afp_catalog::locations::{ Location, LOCATION_EXT, PROJECT_FILE };
pub use afp_catalog::workspace::*;

/// The shares this computer has mounted, read once (gyroflow-export runs for one job; the GPU table's server asks for
/// each request, and its clips are on the shares mounted when the player started it)
fn mounts() -> &'static [(String, PathBuf)] {
    static ONCE: std::sync::OnceLock<Vec<(String, PathBuf)>> = std::sync::OnceLock::new();
    ONCE.get_or_init(afp_catalog::locations::mounts)
}

/// The project's locations here, in their order (this computer's own first); a project without any: its own folder
pub fn locations(project: &Path) -> Vec<Location> {
    afp_catalog::locations::locations(project, mounts())
}

/// The clip's project, None: none
pub fn project_root(clip: &Path) -> Option<PathBuf> {
    project_root_in(clip, home().as_deref(), mounts())
}

/// The clip's own file and the other copies' (workspace::own_file over its project's locations; in none: beside it)
pub fn own_files(clip: &Path) -> (PathBuf, Vec<PathBuf>) {
    match project_root(clip) {
        Some(root) => own_file(clip, &locations(&root)),
        None => (sidecar_file(clip), Vec::new()),
    }
}

/// The clip's own file, the main one: read and written where the player and the Library read and write it. Kept a few
/// seconds per clip: the GPU table's server asks for each request (its lens), and finding it reads the folders above
/// the clip and the project's file, off a share
pub fn own_file_of(clip: &Path) -> PathBuf {
    use std::collections::HashMap;
    use std::time::{ Duration, Instant };
    static KEPT: std::sync::Mutex<Option<HashMap<PathBuf, (Instant, PathBuf)>>> = std::sync::Mutex::new(None);
    if let Some((t, f)) = KEPT.lock().unwrap().get_or_insert_with(HashMap::new).get(clip) {
        if t.elapsed() < Duration::from_secs(10) { return f.clone(); }
    }
    let f = own_files(clip).0;
    KEPT.lock().unwrap().get_or_insert_with(HashMap::new).insert(clip.to_path_buf(), (Instant::now(), f.clone()));
    f
}

#[cfg(test)]
mod tests {
    use super::*;

    /// What gyroflow-export reads from a clip's own file (a 360 clip's recorded view, the lens type the Library set)
    /// it reads where the player and the Library keep it: over the project's locations, not only beside the clip
    /// opened. Before, reframe.rs and lens_pick.rs read <clip>.afp.json beside it: a clip opened from a working copy
    /// had no recording and no lens type
    #[test]
    fn readers_find_the_own_file_over_the_locations() {
        let top = std::env::temp_dir().join(format!("afp-own-readers-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&top);
        let (p, work, archive) = (top.join("P"), top.join("Work"), top.join("Archive"));
        for d in [&p, &work, &archive] { std::fs::create_dir_all(d).unwrap(); }
        let id = format!("readers-test-{}", std::process::id());
        std::fs::write(p.join(PROJECT_FILE), format!(r#"{{"id":"{id}","locations":[{{"name":"Work","at":"","path":"../Work","copies":true}},{{"name":"Archive","at":"","path":"../Archive"}}]}}"#)).unwrap();
        for d in [&work, &archive] {
            std::fs::write(d.join(format!("P{LOCATION_EXT}")), format!(r#"{{"project":"../P","id":"{id}"}}"#)).unwrap();
            std::fs::write(d.join("A.insv"), b"").unwrap();
        }
        std::fs::write(archive.join("A.afp.json"), r#"{"source":{"lens_type":"wide"},"view360":{"recording":{"follows":"camera","parts":[{"samples":[[0,0,0,0,90],[1,10,0,0,90]]}]}}}"#).unwrap();
        let clip = work.join("A.insv");
        assert_eq!(own_file_of(&clip), archive.join("A.afp.json"));
        assert!(crate::reframe::Recording::read(&clip).is_ok(), "the recording in the archive's own file");
        assert_eq!(crate::lens_pick::chosen_type(&clip.to_string_lossy()).as_deref(), Some("wide"));
        let _ = std::fs::remove_dir_all(&top);
    }
}
