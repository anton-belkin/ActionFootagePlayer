// The project's settings: its locations, what this computer keeps for it
// One of the areas of the Library's commands: main.rs is the wiring (the state, main(), generate_handler!'s list)

use super::*;

// ---------- the project's settings: its locations, what this computer keeps for it ----------

fn new_id() -> String {
    // Random enough for a project's id: the time and this process, hashed
    let t = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0);
    let mut h: u64 = 0xcbf29ce484222325;
    for b in format!("{t}-{}-{:?}", std::process::id(), std::thread::current().id()).bytes() { h ^= b as u64; h = h.wrapping_mul(0x100000001b3); }
    let t2 = (t as u64).rotate_left(17) ^ h.rotate_right(9);
    format!("{:08x}-{:04x}-4{:03x}-a{:03x}-{:012x}", h >> 32, (h >> 16) & 0xffff, h & 0xfff, (t2 >> 52) & 0xfff, t2 & 0xffff_ffff_ffff)
}

// The project's id, made (and written) when it has none
pub(crate) fn project_id(root: &Path) -> Result<String, String> {
    if let Some(id) = read_doc(root).get("id").and_then(Value::as_str).filter(|i| !i.is_empty()) { return Ok(id.to_string()); }
    let id = new_id();
    update_doc(root, |d| { d.insert("id".into(), json!(id)); })?;
    Ok(id)
}

fn dir_size(dir: &Path) -> (u64, u64) {
    let (mut bytes, mut files) = (0, 0);
    let mut todo = vec![dir.to_path_buf()];
    while let Some(d) = todo.pop() {
        for e in std::fs::read_dir(&d).into_iter().flatten().flatten() {
            let Ok(m) = e.metadata() else { continue };
            if m.is_dir() { todo.push(e.path()); } else { bytes += m.len(); files += 1; }
        }
    }
    (bytes, files)
}

// This computer's caches: the player's (motion data, probes, GPU tables, the Library's probes) and the Library's
// pictures. They're kept by each file's path, for every project at once
fn caches(app: &tauri::AppHandle) -> Vec<(String, String, PathBuf)> {
    let mut out = Vec::new();
    let player = if cfg!(target_os = "macos") { std::env::var_os("HOME").map(|h| PathBuf::from(h).join("Library/Caches/ActionFootagePlayer")) }
        else if cfg!(windows) { std::env::var_os("LOCALAPPDATA").map(|h| PathBuf::from(h).join("ActionFootagePlayer/cache")) }
        else { std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".cache/ActionFootagePlayer")) };
    if let Some(p) = player {
        out.push(("motion".into(), "Motion data: gyro samples read from clips, so they open fast here".into(), p.join("motion")));
        out.push(("probe".into(), "What each clip is: read once per clip".into(), p.join("probe")));
        out.push(("library".into(), "What each clip is, for the Library's filters".into(), p.join("library")));
        out.push(("gpu".into(), "GPU tables: each clip's warp for its settings".into(), p.join("gpu")));
    }
    if let Ok(c) = app.path().app_cache_dir() { out.push(("thumbs".into(), "Pictures shown in the Library".into(), c.join("thumbs"))); }
    out
}

#[tauri::command]
pub(crate) async fn project_settings(app: tauri::AppHandle, shared: State<'_, Shared>) -> Result<Value, String> {
    let root = shared.lock().unwrap().project.clone().ok_or("Open a project first")?;
    tauri::async_runtime::spawn_blocking(move || {
        let mounts = locations::mounts();
        let doc = read_doc(&root);
        let name = locations::pointer_name(&root);
        let all = locations::locations(&root, &mounts);
        let own: Vec<Value> = all.iter().filter(|l| !l.local && !(l.path == "." && doc.get("locations").is_none())).map(|l| json!({
            "name": l.name, "at": l.at, "path": l.path, "copies": l.copies, "dir": l.dir.as_ref().map(|d| d.display().to_string()),
            "reachable": l.dir.as_ref().is_some_and(|d| d.is_dir()),
            "pointer": l.dir.as_ref().is_some_and(|d| d == &root || d.join(&name).is_file()),
        })).collect();
        let local: Vec<Value> = all.iter().filter(|l| l.local).map(|l| {
            let (bytes, files) = l.dir.as_ref().map(|d| dir_size(d)).unwrap_or((0, 0));
            json!({ "name": l.name, "at": l.at, "path": l.path, "reachable": l.dir.as_ref().is_some_and(|d| d.is_dir()), "bytes": bytes, "files": files })
        }).collect();
        let caches: Vec<Value> = caches(&app).into_iter().map(|(k, what, dir)| { let (b, f) = dir_size(&dir); json!({ "key": k, "what": what, "path": dir.display().to_string(), "bytes": b, "files": f }) }).collect();
        Ok(json!({ "path": root.display().to_string(), "name": root.file_name().map(|n| n.to_string_lossy().into_owned()),
                   "id": doc.get("id"), "hosts": doc.get("hosts").cloned().unwrap_or(json!({})), "locations": own, "local": local, "caches": caches,
                   "never_rename": locations::never_rename(&root),
                   "tag_stores": locations::tag_stores(&root), "finder_here": cfg!(target_os = "macos"),
                   "generated_runs": doc.get("generated_runs").and_then(Value::as_bool).unwrap_or(false),
                   "next_to_footage": doc.get("locations").and_then(Value::as_array).is_none_or(|a| a.is_empty()),
                   "renders": renders::cache_size(&root) }))
    }).await.map_err(|e| e.to_string())?
}

// The project's locations written: into Project.afproject (none: the project is next to its footage again), and its
// .afplocation file into each of them (taken out of the ones removed)
#[tauri::command]
pub(crate) async fn save_locations(shared: State<'_, Shared>, list: Vec<Value>, hosts: Option<Value>, generated_runs: Option<bool>) -> Result<Vec<String>, String> {
    let root = shared.lock().unwrap().project.clone().ok_or("Open a project first")?;
    tauri::async_runtime::spawn_blocking(move || {
        let mounts = locations::mounts();
        let before: Vec<locations::Location> = locations::locations(&root, &mounts).into_iter().filter(|l| !l.local).collect();
        let clean: Vec<Value> = list.iter().filter(|l| l.get("path").and_then(Value::as_str).is_some_and(|p| !p.trim().is_empty())).map(|l| json!({
            "name": l.get("name").and_then(Value::as_str).unwrap_or("").trim(),
            "at": l.get("at").and_then(Value::as_str).unwrap_or("").trim().trim_matches('/'),
            "path": l.get("path").and_then(Value::as_str).unwrap_or("").trim(),
            "copies": l.get("copies").and_then(Value::as_bool).unwrap_or(false),
        })).collect();
        project_id(&root)?;
        update_doc(&root, |d| {
            if clean.is_empty() { d.remove("locations"); } else { d.insert("locations".into(), json!(clean)); }
            if let Some(h) = &hosts { if h.as_object().is_some_and(|o| !o.is_empty()) { d.insert("hosts".into(), h.clone()); } else { d.remove("hosts"); } }
            // A folder per run in Generated/ (the player reads it): written only when on
            match generated_runs { Some(true) => { d.insert("generated_runs".into(), json!(true)); } Some(false) => { d.remove("generated_runs"); } None => {} }
        })?;
        let after: Vec<locations::Location> = locations::locations(&root, &mounts).into_iter().filter(|l| !l.local).collect();
        Ok(locations::write_pointers(&root, &after, &before, &mounts))
    }).await.map_err(|e| e.to_string())?
}

// A folder chosen for a location: where it is here and how the project keeps it (network path, or relative)
#[tauri::command]
pub(crate) fn choose_location(shared: State<Shared>) -> Result<Option<Value>, String> {
    let root = shared.lock().unwrap().project.clone().ok_or("Open a project first")?;
    let Some(dir) = rfd::FileDialog::new().set_title("A folder of the project's footage").pick_folder() else { return Ok(None) };
    let mounts = locations::mounts();
    Ok(Some(json!({ "dir": dir.display().to_string(), "path": locations::path_for(&dir, &root, &mounts),
                    "name": dir.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default() })))
}

// This computer's own locations for the project (copies on its own disk), kept on it, by the project's id
#[tauri::command]
pub(crate) fn choose_local(shared: State<Shared>) -> Result<Option<String>, String> {
    shared.lock().unwrap().project.clone().ok_or("Open a project first")?;
    Ok(rfd::FileDialog::new().set_title("A folder on this computer with copies of the project's clips").pick_folder().map(|d| d.display().to_string()))
}

#[tauri::command]
pub(crate) async fn save_local(shared: State<'_, Shared>, list: Vec<Value>, never_rename: Option<bool>) -> Result<(), String> {
    // In the background: the window never waits on it (a share can take seconds)
    let shared = shared.inner().clone();
    tauri::async_runtime::spawn_blocking(move || -> Result<(), String> {
    let root = shared.lock().unwrap().project.clone().ok_or("Open a project first")?;
    let id = project_id(&root)?;
    let file = locations::local_file(&id).ok_or("No place for this computer's files")?;
    let clean: Vec<Value> = list.iter().filter_map(|l| {
        let p = l.get("path").and_then(Value::as_str)?.trim();
        (!p.is_empty()).then(|| json!({ "name": l.get("name").and_then(Value::as_str).unwrap_or("On this computer"), "at": l.get("at").and_then(Value::as_str).unwrap_or("").trim_matches('/'), "path": p }))
    }).collect();
    if let Some(d) = file.parent() { std::fs::create_dir_all(d).map_err(|e| e.to_string())?; }
    // "I never rename my footage": kept, unless this save says otherwise
    let never_rename = never_rename.unwrap_or_else(|| locations::never_rename(&root));
    // The tag stores chosen on this computer (save_tag_stores) stay
    let stores = locations::read_json(&file).map(|d| locations::tag_stores_in(&d)).unwrap_or_default();
    // Kept with nothing else in it too: it says where the project is here (locations::note_here)
    let mut doc = json!({ "project": root.display().to_string(), "locations": clean });
    if never_rename { doc["never_rename"] = json!(true); }
    if !stores.is_empty() { doc["tag_stores"] = json!(stores); }
    std::fs::write(&file, serde_json::to_vec_pretty(&doc).unwrap()).map_err(|e| e.to_string())
}).await.map_err(|e| e.to_string())?
}

// The project's render cache (_Renders) emptied: what Share made there, never anything else. Only on the user's Clear
// (Project settings, General); nothing cleans it by itself. { files, bytes } freed, { kept }
#[tauri::command]
pub(crate) async fn clear_renders(shared: State<'_, Shared>) -> Result<Value, String> {
    let root = shared.lock().unwrap().project.clone().ok_or("Open a project first")?;
    tauri::async_runtime::spawn_blocking(move || {
        let (freed, problems) = renders::clear(&root);
        if problems.is_empty() { Ok(freed) } else { Err(format!("Couldn't remove {} of them: {}", problems.len(), problems[0])) }
    }).await.map_err(|e| e.to_string())?
}

#[tauri::command]
pub(crate) async fn clear_cache(app: tauri::AppHandle, key: String) -> Result<(), String> {
    // In the background: the window never waits on it (a share can take seconds)
    tauri::async_runtime::spawn_blocking(move || -> Result<(), String> {
    let (_, _, dir) = caches(&app).into_iter().find(|(k, _, _)| *k == key).ok_or("no such cache")?;
    if dir.is_dir() { std::fs::remove_dir_all(&dir).map_err(|e| e.to_string())?; }
    Ok(())
}).await.map_err(|e| e.to_string())?
}
