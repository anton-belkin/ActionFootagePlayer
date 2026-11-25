// The project: its file (read_doc, update_doc), opening it, the page's look at it
// One of the areas of the Library's commands: main.rs is the wiring (the state, main(), generate_handler!'s list)

use super::*;

// ---------- the project ----------

pub(crate) fn is_project(dir: &Path) -> bool {
    dir.join(PROJECT_FILE).is_file() || dir.join(LEGACY_DIR).is_dir()
}

// A project's folder from what was opened: its Project.afproject, the folder itself, or a folder in it
pub(crate) fn project_of(path: &Path) -> Option<PathBuf> {
    let path = std::path::absolute(path).unwrap_or_else(|_| path.to_path_buf());
    if path.is_file() {
        if path.file_name().is_some_and(|n| n.to_string_lossy().eq_ignore_ascii_case(PROJECT_FILE)) {
            return path.parent().map(Path::to_path_buf);
        }
        return path.parent().and_then(project_of);
    }
    let mut dir = Some(path.as_path());
    while let Some(d) = dir {
        if is_project(d) { return Some(d.to_path_buf()); }
        dir = d.parent();
    }
    None
}

pub(crate) fn read_doc(root: &Path) -> Value {
    afp_catalog::store::nas::read();
    std::fs::read_to_string(root.join(PROJECT_FILE)).ok()
        .and_then(|t| serde_json::from_str::<Value>(&t).ok())
        .filter(|v| v.is_object() && v.get("format").is_none_or(|f| f == FORMAT))
        .unwrap_or_else(|| json!({}))
}

// The doc read fresh, changed and written whole through a temporary file, what this doesn't know kept: the player
// writes it too
pub(crate) fn update_doc(root: &Path, change: impl FnOnce(&mut serde_json::Map<String, Value>)) -> Result<(), String> {
    let file = root.join(PROJECT_FILE);
    if file.is_file() && std::fs::read_to_string(&file).ok().and_then(|t| serde_json::from_str::<Value>(&t).ok())
        .is_some_and(|v| v.get("format").is_some_and(|f| f != FORMAT)) {
        return Err(format!("{} isn't Action Footage's", file.display()));
    }
    if let Some(e) = afp_catalog::project::too_new(root) { return Err(e); }
    let mut doc = read_doc(root);
    let map = doc.as_object_mut().ok_or("not an object")?;
    map.insert("format".into(), json!(FORMAT));
    // This Library's version (afp_catalog::project::VERSION), never lowered: older ones then refuse the project. Set
    // after the change, so no change can lower it (saving locations wrote 2 here, over the 4 it had been raised to)
    let v = map.get("version").and_then(Value::as_u64).unwrap_or(1).max(afp_catalog::project::VERSION);
    if !map.get("settings").is_some_and(Value::is_object) { map.insert("settings".into(), json!({})); }
    change(map);
    map.insert("version".into(), json!(v));
    let tmp = file.with_extension("afproject.tmp");
    std::fs::write(&tmp, serde_json::to_vec(&doc).map_err(|e| e.to_string())?).map_err(|e| format!("{}: {e}", tmp.display()))?;
    std::fs::rename(&tmp, &file).map_err(|e| format!("{}: {e}", file.display()))
}

#[derive(Serialize)]
pub(crate) struct ProjectInfo {
    path: String,
    name: String,
    // Before 0.9: the player moves the folder into Project.afproject when it next opens a clip of it
    pub(crate) legacy: bool,
    pub(crate) clips_flat: bool,
    // How clip previews are laid out in _Clips: tree, flat or trips (the player's workspace plugin, clips_layout)
    clips_layout: String,
    pub(crate) moments_alone: bool,
    // The library's tag stores (docs/design/tags-and-culling.md): "finder"
    tag_stores: Vec<String>,
}

// What the project is and how it's set: its own file read, nothing walked. How many previews, moments, photos and
// exports it has isn't asked of the disk here (a share took 10 s of every start for it): the catalog's views say
pub(crate) fn project_info(root: &Path) -> ProjectInfo {
    let doc = read_doc(root);
    let settings = doc.get("settings").cloned().unwrap_or(json!({}));
    let flag = |k: &str| settings.get(k).and_then(Value::as_bool).unwrap_or(false);
    ProjectInfo {
        path: root.display().to_string(),
        name: root.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| root.display().to_string()),
        legacy: !root.join(PROJECT_FILE).is_file(),
        clips_flat: flag("clips_flat"),
        clips_layout: settings.get("clips_layout").and_then(Value::as_str).filter(|l| ["tree", "flat", "trips"].contains(l)).map(str::to_string)
            .unwrap_or_else(|| if flag("clips_flat") { "flat".into() } else { "tree".into() }),
        moments_alone: flag("moments_alone"),
        tag_stores: locations::tag_stores(root),
    }
}

// ---------- the page's commands ----------

#[derive(Serialize)]
pub(crate) struct Snapshot {
    project: Option<String>,
    jobs: Vec<Job>,
    close_asked: bool,
    close_waits: bool,
    macos: bool,
    // Changes seen in the project's folder and locations, and how long since the last one (ms)
    changes: u64,
    quiet_ms: u64,
    // What the project's catalog is doing ("": nothing to say)
    catalog: String,
    // Links whose original isn't where they say (moments, clip previews, exports, photos), counted from the catalog;
    // null: not known
    missing: Option<u64>,
    moving: Option<Moving>,
}

#[tauri::command]
pub(crate) async fn snapshot(shared: State<'_, Shared>) -> Result<Snapshot, String> {
    // In the background: the window never waits on it (a share can take seconds)
    let shared = shared.inner().clone();
    tauri::async_runtime::spawn_blocking(move || -> Snapshot {
    let mut inner = shared.lock().unwrap();
    if inner.project.is_none() {
        if let Some(p) = inner.pending_open.pop_front() { inner.project = Some(p); }
    }
    // Watched: the project open, its locations as its file says now
    match inner.project.clone() {
        // Its files being moved (its catalog with them): watched again once they're in their new place
        Some(_) if inner.moving.is_some() => {}
        Some(root) => {
            let stamp = std::fs::metadata(root.join(PROJECT_FILE)).and_then(|m| m.modified()).ok();
            // The project's file written with its locations as they were (a setting, the player's lists): nothing to
            // watch again
            if let Some(w) = inner.watch.as_mut().filter(|w| w.root == root && w.stamp != stamp) {
                if locations_key(&root) == w.locs { w.stamp = stamp; }
            }
            // …or a location became reachable, or stopped being (a share mounted after the Library opened)
            if inner.watch.as_ref().is_none_or(|w| w.root != root || w.stamp != stamp || w.reach_changed.load(std::sync::atomic::Ordering::Relaxed)) {
                let before = inner.watch.as_ref().filter(|w| w.root == root).map(|w| w.changes.load(std::sync::atomic::Ordering::Relaxed) + 1);
                let moved = inner.moved_to.take().is_some_and(|m| m == root);
                let w = watch(&root, moved);
                // The same project watched again (its locations changed): counted on from where it was, and once more
                if let Some(n) = before { w.changes.store(n, std::sync::atomic::Ordering::Relaxed); }
                inner.watch = Some(w);
            }
        }
        None => inner.watch = None,
    }
    let catalog = seen::status(inner.watch.as_ref().map(|w| &w.runner));
    let missing = Some(seen::missing(inner.watch.as_ref().map(|w| &w.runner))).filter(|n| *n != u64::MAX);
    let (changes, quiet_ms) = inner.watch.as_ref().map(|w| (w.changes.load(std::sync::atomic::Ordering::Relaxed), w.last.lock().unwrap().elapsed().as_millis() as u64)).unwrap_or((0, 0));
    Snapshot {
        project: inner.project.as_ref().map(|p| p.display().to_string()),
        jobs: inner.jobs.clone(),
        close_asked: inner.close_asked,
        close_waits: inner.close_waits,
        macos: cfg!(target_os = "macos"),
        changes,
        quiet_ms,
        catalog,
        missing,
        moving: inner.moving.clone(),
    }
}).await.map_err(|e| e.to_string())
}

#[tauri::command]
pub(crate) async fn project(shared: State<'_, Shared>) -> Result<Option<ProjectInfo>, String> {
    // In the background: the window never waits on it (a share can take seconds)
    let shared = shared.inner().clone();
    tauri::async_runtime::spawn_blocking(move || -> Option<ProjectInfo> {
    let root = shared.lock().unwrap().project.clone()?;
    let _s = journal::span("project_info", json!({ "where": journal::where_(&root) }));
    Some(project_info(&root))
}).await.map_err(|e| e.to_string())
}

#[tauri::command]
pub(crate) async fn engine() -> Result<String, String> {
    // In the background: the window never waits on it (a share can take seconds)
    tauri::async_runtime::spawn_blocking(move || -> Result<String, String> {
    player().map(|p| p.display().to_string())
}).await.map_err(|e| e.to_string())?
}

fn open_path(app: &tauri::AppHandle, shared: &Shared, path: &Path) -> Result<String, String> {
    let root = project_of(path).ok_or_else(|| format!(
        "{} isn't in a project. A project is a trip folder with {PROJECT_FILE} in it: open a clip of it in \
         ActionFootagePlayer and initialize the project there (the side panel's Project tab)", path.display()))?;
    usable(&root)?;
    shared.lock().unwrap().project = Some(root.clone());
    remember(app, &root);
    Ok(root.display().to_string())
}

#[tauri::command]
pub(crate) async fn open_project(app: tauri::AppHandle, shared: State<'_, Shared>, path: String) -> Result<String, String> {
    // In the background: the window never waits on it (a share can take seconds)
    let shared = shared.inner().clone();
    tauri::async_runtime::spawn_blocking(move || -> Result<String, String> {
    let _s = journal::span("open_project", json!({ "where": journal::where_(Path::new(&path)) }));
    open_path(&app, &shared, Path::new(&path))
}).await.map_err(|e| e.to_string())?
}

#[derive(Serialize)]
pub(crate) struct Recent {
    path: String,
    name: String,
    /// Reachable now (else offline: its share not mounted, its disk not plugged in)
    here: bool,
    /// Its share, for saying where it is ("//DISKSTATIONDXP/ssd")
    share: Option<String>,
    /// Can be connected from here (the Mac, its share known)
    connect: bool,
    /// The names it can be connected by (the NAS's), for Connect's list
    names: Vec<String>,
    /// The one the app opens when it's started with no file ("Open at start")
    at_start: bool,
}

// The projects this computer has opened, the last first: every one, those not reachable now too (offline)
#[tauri::command]
pub(crate) async fn recent(app: tauri::AppHandle) -> Result<Vec<Recent>, String> {
    // In the background: the window never waits on it (a share can take seconds)
    tauri::async_runtime::spawn_blocking(move || -> Vec<Recent> {
    let mounts = locations::mounts();
    let mut list = recent_list(&app);
    let kept: Vec<String> = list.iter().filter_map(|e| e.share.clone()).collect();
    let mut changed = false;
    let out = list.iter_mut().map(|e| {
        let here = recent_here(e, &mounts);
        // One kept before its share was (or mounted elsewhere now): its share noted, its path the one now
        if let Some(h) = &here {
            if let Some((s, r)) = share_of(h, &mounts) { if e.share.as_deref() != Some(s.as_str()) || e.rel != r { e.share = Some(s); e.rel = r; changed = true; } }
            let names = other_names(h, e.share.as_deref());
            if e.hosts != names { e.hosts = names; changed = true; }
            let hp = h.display().to_string();
            if e.path != hp { e.path = hp; changed = true; locations::note_here(h); }
        }
        // Its share not kept (a list of before): guessed, to connect it all the same
        let guess = if here.is_none() { guess_share(e, &mounts, &kept) } else { None };
        let known = guess.as_ref().map(|g| &g.0).unwrap_or(e);
        let share = known.share.as_deref().and_then(locations::network).map(|(h, n, _)| format!("//{}/{n}", h.rsplit_once('@').map(|x| x.1).unwrap_or(&h).trim_end_matches('.').trim_end_matches("._smb._tcp.local")));
        Recent { name: Path::new(&e.path).file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| e.path.clone()),
                 here: here.is_some(), connect: here.is_none() && cfg!(target_os = "macos") && known.share.is_some(), share, path: e.path.clone(),
                 names: match (&here, &guess) { (Some(_), _) => Vec::new(), (None, Some(g)) => g.1.clone(), (None, None) => recent_names(e) }, at_start: e.at_start }
    }).collect();
    if changed { write_recent(&app, &list); }
    out
}).await.map_err(|e| e.to_string())
}

/// "Open at start" put on a project of the list (None: taken off): the one the app opens when it's started with no
/// file. Kept with the list, in the app's own data on this computer
#[tauri::command]
pub(crate) async fn set_start_project(app: tauri::AppHandle, path: Option<String>) -> Result<(), String> {
    let mut list = recent_list(&app);
    if !set_start(&mut list, path.as_deref()) { return Err("That project isn't in the list of those opened here".into()); }
    write_recent(&app, &list);
    Ok(())
}

/// The app started with no file (the page asks once, when it's up and no project is open: a Project.afproject double
/// clicked has opened by then): the project marked "Open at start" opened, when it's reachable. Its path; None: none
/// marked, one opened already, or it's offline (the start screen then shows it so, with Connect)
#[tauri::command]
pub(crate) async fn open_at_start(app: tauri::AppHandle, shared: State<'_, Shared>) -> Result<Option<String>, String> {
    let shared = shared.inner().clone();
    tauri::async_runtime::spawn_blocking(move || -> Option<String> {
        if shared.lock().unwrap().project.is_some() { return None; }
        let list = recent_list(&app);
        let root = recent_here(start_entry(&list)?, &locations::mounts())?;
        usable(&root).map_err(|e| eprintln!("{e}")).ok()?;
        let mut inner = shared.lock().unwrap();
        // One opened meanwhile (a double click) stays
        if inner.project.is_some() { return None; }
        inner.project = Some(root.clone());
        drop(inner);
        locations::note_here(&root);
        Some(root.display().to_string())
    }).await.map_err(|e| e.to_string())
}

/// The project closed: the start screen, with the projects opened before (tasks queued go on)
#[tauri::command]
pub(crate) fn close_project(shared: State<Shared>) {
    let mut inner = shared.lock().unwrap();
    inner.project = None;
    inner.pending_open.clear();
}

/// A project taken off the list of those opened before (a test one, one moved away)
#[tauri::command]
pub(crate) async fn forget_recent(app: tauri::AppHandle, path: String) -> Result<(), String> {
    let mut list = recent_list(&app);
    list.retain(|e| e.path != path);
    write_recent(&app, &list);
    Ok(())
}

/// An offline project's share connected: where the project is then
#[tauri::command]
pub(crate) async fn connect_project(app: tauri::AppHandle, path: String, host: Option<String>) -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || -> Result<String, String> {
        let list = recent_list(&app);
        let kept: Vec<String> = list.iter().filter_map(|e| e.share.clone()).collect();
        let e = list.into_iter().find(|e| e.path == path).ok_or("It isn't in the list")?;
        // Its share as kept, else guessed from its path and the NAS known here
        let (e, names) = match guess_share(&e, &locations::mounts(), &kept) { Some(g) => g, None => { let n = recent_names(&e); (e, n) } };
        let share = e.share.clone().ok_or("Its share isn't known: connect it in Finder, then open it")?;
        // By the name chosen, else each of the NAS's names in turn
        mount_share(&share, &host.map(|h| vec![h]).unwrap_or(names))?;
        recent_here(&e, &locations::mounts()).map(|p| p.display().to_string())
            .ok_or_else(|| format!("{share} is connected, but {} isn't on it", e.path))
    }).await.map_err(|e| e.to_string())?
}

// A file dialog for Project.afproject (a synchronous command: it runs on the main thread, as the dialog must)
#[tauri::command]
pub(crate) fn choose_project(app: tauri::AppHandle, shared: State<Shared>) -> Result<Option<String>, String> {
    let picked = rfd::FileDialog::new().set_title("Open a project: its Project.afproject")
        .add_filter("Action Footage project", &["afproject"]).pick_file();
    match picked {
        Some(p) => open_path(&app, &shared, &p).map(Some),
        None => Ok(None),
    }
}

#[tauri::command]
pub(crate) async fn set_setting(shared: State<'_, Shared>, key: String, value: Value) -> Result<(), String> {
    // In the background: the window never waits on it (a share can take seconds)
    let shared = shared.inner().clone();
    tauri::async_runtime::spawn_blocking(move || -> Result<(), String> {
    let root = shared.lock().unwrap().project.clone().ok_or("Open a project first")?;
    match (key.as_str(), &value) {
        ("clips_flat" | "moments_alone", Value::Bool(_)) => update_doc(&root, |doc| { doc["settings"][&key] = value.clone(); }),
        // The layout, and clips_flat as it says (what players before it read)
        ("clips_layout", Value::String(l)) if ["tree", "flat", "trips"].contains(&l.as_str()) =>
            update_doc(&root, |doc| { doc["settings"]["clips_layout"] = value.clone(); doc["settings"]["clips_flat"] = json!(l == "flat"); }),
        _ => Err(format!("no such setting: {key} = {value}")),
    }
}).await.map_err(|e| e.to_string())?
}

#[tauri::command]
pub(crate) async fn reveal(shared: State<'_, Shared>, what: Option<String>) -> Result<(), String> {
    // In the background: the window never waits on it (a share can take seconds)
    let shared = shared.inner().clone();
    tauri::async_runtime::spawn_blocking(move || -> () {
    let Some(root) = shared.lock().unwrap().project.clone() else { return };
    let path = match what.as_deref() {
        Some(sub @ ("_Clips" | "_Moments" | "_Stills" | "_Exports" | "Generated")) if root.join(sub).is_dir() => root.join(sub),
        _ => root,
    };
    let _ = if cfg!(target_os = "macos") { Command::new("open").arg(&path).spawn() }
        else if cfg!(windows) { Command::new("explorer").arg(&path).spawn() }
        else { Command::new("xdg-open").arg(&path).spawn() };
}).await.map_err(|e| e.to_string())
}

/// A project this Library may work with: Err for one of a newer version (afp_catalog::project::VERSION), which it
/// would spoil (its catalog and files written without what it doesn't know); one of an older version is raised to
/// this one's, so older Libraries leave it alone from now on
pub(crate) fn usable(root: &Path) -> Result<(), String> {
    if let Some(e) = afp_catalog::project::too_new(root) { return Err(e); }
    afp_catalog::project::raise(root).map_err(|e| format!("{}: {e}", root.join(PROJECT_FILE).display()))
}

pub(crate) fn opened(shared: &Shared, app: &tauri::AppHandle, path: PathBuf) {
    match project_of(&path) {
        Some(root) if usable(&root).is_err() => eprintln!("{}", usable(&root).unwrap_err()),
        Some(root) => {
            remember(app, &root);
            let mut inner = shared.lock().unwrap();
            inner.project = Some(root.clone());
            inner.pending_open.push_back(root);
        }
        None => eprintln!("{} isn't in a project", path.display()),
    }
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.unminimize();
        let _ = w.show();
        let _ = w.set_focus();
    }
}
