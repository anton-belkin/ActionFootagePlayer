// The project's own files moved to another folder
// One of the areas of the Library's commands: main.rs is the wiring (the state, main(), generate_handler!'s list)

use super::*;

// The project's own files moved to another folder (the SSD, say): Project.afproject and its _ folders. The clips and
// their own files stay where they are: the old folder becomes a location at the top, unless it is one already; the
// locations given relative to the project are written again for its new place
// The project's own files moved from `root` to `dest`: Project.afproject and its _ folders. The clips and their own
// files stay: the old folder becomes a location at the top unless the project had locations, whose paths relative to
// the project are written again for its new place. Problems writing the pointers are returned
pub(crate) fn move_to(root: &Path, dest: &Path, progress: &dyn Fn(Moving)) -> Result<Vec<String>, String> {
    if dest == root { return Ok(vec![]); }
    if dest.join(PROJECT_FILE).exists() { return Err(format!("{} has a project already", dest.display())); }
    if dest.starts_with(root) { return Err("Choose a folder outside the project's".into()); }
    let mounts = locations::mounts();
    let all: Vec<locations::Location> = locations::locations(root, &mounts).into_iter().filter(|l| !l.local).collect();
    let had = read_doc(root).get("locations").and_then(Value::as_array).is_some_and(|a| !a.is_empty());
    let list: Vec<Value> = if had {
        all.iter().map(|l| {
            let path = match (&l.dir, locations::network(&l.path)) { (Some(d), None) => locations::path_for(d, dest, &mounts), _ => l.path.clone() };
            json!({ "name": l.name, "at": l.at, "path": path, "copies": l.copies })
        }).collect()
    } else {
        vec![json!({ "name": root.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or("Footage".into()), "at": "",
                     "path": locations::path_for(root, dest, &mounts), "copies": false })]
    };
    project_id(root)?;
    std::fs::create_dir_all(dest).map_err(|e| e.to_string())?;
    let mut moved = Vec::new();
    // Its catalog (.afp) too: read at once in the new place (its folder looked at again first), not made again from
    // the disks (minutes on a share, the views reading the disks meanwhile)
    let names: Vec<&str> = [PROJECT_FILE, "_Moments", "_Clips", "_Stills", "_Exports", "_Renders", "_Cache", index::GENERATED, ".afp"].into_iter().filter(|n| root.join(n).exists()).collect();
    // On another disk they're copied: how much, for the page's bar (counted once a rename fails)
    let mut state = Moving { to: dest.display().to_string(), ..Default::default() };
    let mut counted = false;
    progress(state.clone());
    for name in &names {
        let from = root.join(name);
        let to = dest.join(name);
        if std::fs::rename(&from, &to).is_err() {
            if !counted {
                state.total = names.iter().map(|n| tree_size(&root.join(n))).sum();
                counted = true;
            }
            // Another disk: copied, then the old one removed
            if let Err(e) = copy_tree(&from, &to, &mut state, progress) {
                return Err(format!("couldn't move {}: {e} (moved so far: {})", from.display(), if moved.is_empty() { "nothing".to_string() } else { moved.join(", ") }));
            }
            let _ = if from.is_dir() { std::fs::remove_dir_all(&from) } else { std::fs::remove_file(&from) };
        }
        moved.push(name.to_string());
    }
    update_doc(dest, |d| { d.insert("locations".into(), json!(list)); })?;
    let after: Vec<locations::Location> = locations::locations(dest, &mounts).into_iter().filter(|l| !l.local).collect();
    // The pointers of before (the old folder's name) out, the new ones in
    for l in &all {
        if let Some(d) = &l.dir { if locations::pointer_name(root) != locations::pointer_name(dest) { let _ = std::fs::remove_file(d.join(locations::pointer_name(root))); } }
    }
    Ok(locations::write_pointers(dest, &after, &[], &mounts))
}

// The bytes of the files under `p` (or of `p`)
pub(crate) fn tree_size(p: &Path) -> u64 {
    let Ok(m) = std::fs::symlink_metadata(p) else { return 0 };
    if !m.is_dir() { return m.len(); }
    std::fs::read_dir(p).map(|rd| rd.flatten().map(|e| tree_size(&e.path())).sum()).unwrap_or(0)
}

// `from` (a file or a folder) copied to `to`, the bytes counted into `state` as they're written (a render can be
// gigabytes over the network: counted in pieces, not per file), each file's permissions and modification time kept
pub(crate) fn copy_tree(from: &Path, to: &Path, state: &mut Moving, progress: &dyn Fn(Moving)) -> Result<(), String> {
    use std::io::{ Read, Write };
    let m = std::fs::symlink_metadata(from).map_err(|e| e.to_string())?;
    if m.is_dir() {
        std::fs::create_dir_all(to).map_err(|e| format!("{}: {e}", to.display()))?;
        let mut entries: Vec<_> = std::fs::read_dir(from).map_err(|e| format!("{}: {e}", from.display()))?.flatten().map(|e| e.path()).collect();
        entries.sort();
        for e in entries {
            let Some(name) = e.file_name() else { continue };
            copy_tree(&e, &to.join(name), state, progress)?;
        }
        return Ok(());
    }
    if m.file_type().is_symlink() {
        #[cfg(unix)]
        { let target = std::fs::read_link(from).map_err(|e| e.to_string())?; return std::os::unix::fs::symlink(target, to).map_err(|e| format!("{}: {e}", to.display())); }
        #[cfg(not(unix))]
        { return Ok(()); }
    }
    state.current = from.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    progress(state.clone());
    let mut src = std::fs::File::open(from).map_err(|e| format!("{}: {e}", from.display()))?;
    let mut dst = std::fs::File::create(to).map_err(|e| format!("{}: {e}", to.display()))?;
    let mut buf = vec![0u8; 4 << 20];
    let mut last = Instant::now();
    loop {
        let n = src.read(&mut buf).map_err(|e| format!("{}: {e}", from.display()))?;
        if n == 0 { break; }
        dst.write_all(&buf[..n]).map_err(|e| format!("{}: {e}", to.display()))?;
        state.done += n as u64;
        if last.elapsed() > Duration::from_millis(200) { progress(state.clone()); last = Instant::now(); }
    }
    dst.flush().map_err(|e| e.to_string())?;
    let _ = dst.set_permissions(m.permissions());
    if let Ok(t) = m.modified() { let _ = dst.set_modified(t); }
    drop(dst);
    progress(state.clone());
    Ok(())
}

// Asked where, then moved in the background: the page polls snapshot's `moving` for its bar meanwhile. Its tasks don't
// stop it (Anton, 2026-11-10: nothing waits on the task list): the one running is stopped after what it's making and
// they all wait, the files move, and they run again from the new place (the running one from the start: what it made
// is found there)
#[tauri::command]
pub(crate) async fn move_project(app: tauri::AppHandle, shared: State<'_, Shared>) -> Result<Option<String>, String> {
    let root = shared.lock().unwrap().project.clone().ok_or("Open a project first")?;
    if shared.lock().unwrap().moving.is_some() { return Err("The project is being moved already".into()); }
    let Some(dest) = rfd::AsyncFileDialog::new().set_title("Move the project's own files to this folder").pick_folder().await else { return Ok(None) };
    let dest = dest.path().to_path_buf();
    let s = shared.inner().clone();
    shared.lock().unwrap().moving = Some(Moving { to: dest.display().to_string(), ..Default::default() });
    let (r, d) = (root.clone(), dest.clone());
    let old = root.display().to_string();
    let done = tauri::async_runtime::spawn_blocking(move || {
        {
            let mut inner = s.lock().unwrap();
            inner.held = Some(old.clone());
            let stop = inner.running.as_ref().is_some_and(|r| inner.jobs.iter().any(|j| j.id == r.id && j.project == old));
            if stop {
                let run = inner.running.as_mut().unwrap();
                let _ = std::fs::write(run.dir.join("stop"), b"");
                run.stop_asked = Some(Instant::now());
                run.yielding = true;
            }
        }
        // Until it has ended (tick kills it after STOP_TIMEOUT)
        loop {
            {
                let mut inner = s.lock().unwrap();
                tick(&mut inner);
                let ours = inner.running.as_ref().is_some_and(|r| inner.jobs.iter().any(|j| j.id == r.id && j.project == old));
                if !ours { break; }
            }
            std::thread::sleep(Duration::from_millis(200));
        }
        // Its catalog's thread stopped: its files move along
        s.lock().unwrap().watch = None;
        let report = |m: Moving| { s.lock().unwrap().moving = Some(m); };
        let moved = move_to(&r, &d, &report);
        let mut inner = s.lock().unwrap();
        inner.held = None;
        if moved.is_ok() {
            let new = d.display().to_string();
            for j in inner.jobs.iter_mut().filter(|j| j.project == old && !j.state.finished()) {
                j.project = new.clone();
                j.request["project"] = json!(new);
                // Paths it was kept to, in the project's folder, move with it (Stabilize optically's clips: those of a
                // location inside the project's folder)
                for key in ["only", "clips"] {
                    if let Some(only) = j.request.get_mut(key).and_then(Value::as_array_mut) {
                        for o in only.iter_mut() {
                            // Under the folder itself, not one whose name begins the same ("Trip" and "Trip 2")
                            let rest = o.as_str().and_then(|x| x.strip_prefix(&old)).filter(|r| r.is_empty() || r.starts_with('/') || r.starts_with('\\'));
                            if let Some(rest) = rest { *o = json!(format!("{new}{rest}")); }
                        }
                    }
                }
            }
            inner.project = Some(d.clone());
            inner.moved_to = Some(d.clone());
        }
        tick(&mut inner);
        moved
    }).await.map_err(|e| e.to_string());
    shared.lock().unwrap().moving = None;
    let problems = done??;
    shared.lock().unwrap().project = Some(dest.clone());
    remember(&app, &dest);
    Ok(Some(if problems.is_empty() { dest.display().to_string() } else { format!("{} ({})", dest.display(), problems.join("; ")) }))
}
