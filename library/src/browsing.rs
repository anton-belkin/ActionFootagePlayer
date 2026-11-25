// Browsing: the views, what changed in them, the probes, the pictures
// One of the areas of the Library's commands: main.rs is the wiring (the state, main(), generate_handler!'s list)

use super::*;

// ---------- browsing ----------

// A view of the open project (index.rs), its clips' probes asked for in the background
// The last look at a view of a project, kept in the app's cache folder: shown at once when the project opens, while
// it's looked at again
fn view_cache(app: &tauri::AppHandle, root: &Path, view: &str) -> Option<PathBuf> {
    let c = app.path().app_cache_dir().ok()?;
    let mut h: u64 = 0xcbf29ce484222325;
    for b in root.display().to_string().bytes() { h ^= b as u64; h = h.wrapping_mul(0x100000001b3); }
    Some(c.join("views").join(format!("{h:016x}-{view}.json")))
}

#[tauri::command]
pub(crate) async fn cached_view(app: tauri::AppHandle, shared: State<'_, Shared>, probes: State<'_, index::SharedProbes>, view: String) -> Result<Value, String> {
    let root = shared.lock().unwrap().project.clone().ok_or("Open a project first")?;
    let Some(file) = view_cache(&app, &root, &view) else { return Ok(Value::Null) };
    let probes = probes.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let Some(mut v) = locations::read_json(&file) else { return Value::Null };
        let want: Vec<String> = v["items"].as_array().into_iter().flatten().filter_map(|it| match it["kind"].as_str() {
            Some("clip") => it["path"].as_str().map(str::to_string),
            Some("moment") => it["original"].as_str().map(str::to_string),
            _ => None }).collect();
        v["probes"] = probes.lock().unwrap().known(&want);
        v["cached"] = json!(true);
        v
    }).await.map_err(|e| e.to_string())
}

#[tauri::command]
pub(crate) async fn scan(app: tauri::AppHandle, shared: State<'_, Shared>, probes: State<'_, index::SharedProbes>, view: String) -> Result<Value, String> {
    let root = shared.lock().unwrap().project.clone().ok_or("Open a project first")?;
    let cache_file = view_cache(&app, &root, &view);
    let probes = probes.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let where_ = journal::where_(&root);
        let flat = changed::flat_previews(&root);
        // The project's catalog being read (a second or two): waited for, not the disks walked meanwhile (minutes)
        let t = journal::now();
        seen::wait_ready(&root, std::time::Duration::from_secs(60));
        journal::step("catalog-wait", json!({ "d": journal::now() - t, "view": view, "where": where_ }));
        seen::reading_view(&root);
        let mut s = journal::span("scan", json!({ "view": view, "where": where_ }));
        let mut out = index::scan(&root, &view, flat);
        s.add("items", json!(out["items"].as_array().map(Vec::len)));
        drop(s);
        // Moments whose clip was found away from the path their link names: the path written into the link
        // (docs/design/library-management.md §8), so the next look, from any computer, finds it at once
        let tool = link_tool();
        let mut relinked = Vec::new();
        for it in out["items"].as_array_mut().into_iter().flatten() {
            let (Some(to), Some(file)) = (it["relink"].as_str().map(str::to_string), it["path"].as_str().map(PathBuf::from)) else { continue };
            let mut l = it["link"].clone();
            l["clip"] = json!(to);
            if write_link(tool.as_ref(), &file, &l.to_string()).is_ok() { it["link"] = l; relinked.push(file); }
            it.as_object_mut().unwrap().remove("relink");
        }
        // …and noted: the catalog would otherwise give the old link on the next look, and it'd be written again
        seen::note_now(&relinked);
        let mut want: Vec<String> = Vec::new();
        for it in out["items"].as_array().into_iter().flatten() {
            match it["kind"].as_str() {
                Some("clip") => if let Some(p) = it["path"].as_str() { want.push(p.to_string()); },
                // A moment's original: what it is (its camera, its color) is the clip's
                Some("moment") => if let Some(o) = it["original"].as_str() { want.push(o.to_string()); },
                _ => {}
            }
        }
        // An offline clip's answer as known from before; not asked for (its file isn't there to read)
        let off: std::collections::HashSet<&str> = out["items"].as_array().into_iter().flatten()
            .filter(|it| it["offline"].as_bool() == Some(true)).filter_map(|it| it["path"].as_str()).collect();
        let ask: Vec<String> = want.iter().filter(|p| !off.contains(p.as_str())).cloned().collect();
        let known = { let mut p = probes.lock().unwrap(); p.want(&ask); p.known(&want) };
        let mut v = out.clone();
        v["root"] = json!(root.display().to_string());
        v["view"] = json!(view);
        if let Some(f) = &cache_file {
            if let Some(d) = f.parent() { let _ = std::fs::create_dir_all(d); }
            let tmp = f.with_extension("tmp");
            if std::fs::write(&tmp, serde_json::to_vec(&v).unwrap()).is_ok() { let _ = std::fs::rename(&tmp, f); }
        }
        v["probes"] = known;
        v
    }).await.map_err(|e| e.to_string())
}

/// What changed since the page's count `since` (seen.rs counts, and tells the page: `catalog-changed`), as the items
/// it changed (changed.rs): { upto: the count now, changed: the items, or null when that can't be said and the views
/// are to be read again }
#[tauri::command]
pub(crate) async fn changes_since(shared: State<'_, Shared>, probes: State<'_, index::SharedProbes>, since: u64) -> Result<Value, String> {
    let (root, now) = { let g = shared.lock().unwrap(); (g.project.clone().ok_or("Open a project first")?, g.watch.as_ref().map(|w| w.changes.load(std::sync::atomic::Ordering::Relaxed)).unwrap_or(0)) };
    let probes = probes.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let changed = seen::since(since, now).map(seen::not_ours).and_then(|files| changed::of_files(&root, &files)).map(|c| c.with_probes(&probes));
        json!({ "upto": now, "changed": changed })
    }).await.map_err(|e| e.to_string())
}

/// These items of a view as they are now (changed.rs): what the page asks for when it knows which items to look at
/// again (a clip whose preview went)
#[tauri::command]
pub(crate) async fn view_items(shared: State<'_, Shared>, probes: State<'_, index::SharedProbes>, view: String, ids: Vec<String>) -> Result<changed::Changed, String> {
    let root = shared.lock().unwrap().project.clone().ok_or("Open a project first")?;
    let probes = probes.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let mut c = changed::Changed::default();
        c.add(&root, changed::flat_previews(&root), &view, &ids);
        c.with_probes(&probes)
    }).await.map_err(|e| e.to_string())
}

/// An offline location's share connected (the Sources banner's Connect): its clips show as soon as it's there
#[tauri::command]
pub(crate) async fn connect_location(shared: State<'_, Shared>, path: String, host: Option<String>) -> Result<(), String> {
    let shared = shared.inner().clone();
    tauri::async_runtime::spawn_blocking(move || -> Result<(), String> {
        // By the name chosen, else each of the NAS's names in turn
        let root = shared.lock().unwrap().project.clone().ok_or("Open a project first")?;
        let (h, _, _) = locations::network(&path).ok_or("Not on a share")?;
        let names = match host { Some(n) => vec![n], None => host_names(&h, read_doc(&root).get("hosts")) };
        mount_share(&path, &names)?;
        // Seen now, not at the next look
        if let Some(w) = shared.lock().unwrap().watch.as_ref() { w.reach_changed.store(true, std::sync::atomic::Ordering::Relaxed); }
        Ok(())
    }).await.map_err(|e| e.to_string())?
}

#[tauri::command]
pub(crate) async fn probes_since(probes: State<'_, index::SharedProbes>, since: u64) -> Result<Value, String> {
    // In the background: the window never waits on it (a share can take seconds)
    let probes = probes.inner().clone();
    tauri::async_runtime::spawn_blocking(move || -> Value {
    let mut p = probes.lock().unwrap();
    // A lens changed in the player (its lens database, not the project's files: no catalog change says so)
    p.lens_list_check();
    p.since(since)
}).await.map_err(|e| e.to_string())
}

#[tauri::command]
pub(crate) async fn thumb(app: tauri::AppHandle, path: String) -> Result<Option<String>, String> {
    let cache = app.path().app_cache_dir().map_err(|e| e.to_string())?.join("thumbs");
    tauri::async_runtime::spawn_blocking(move || {
        // Journal: a picture that took a while (made, or read from a share), not each one
        let t = journal::now();
        let r = index::thumb(Path::new(&path), &cache);
        let d = journal::now() - t;
        if d >= 20.0 { journal::step("thumb", json!({ "d": d, "where": journal::where_(Path::new(&path)), "made": r.is_some() })); }
        r
    }).await.map_err(|e| e.to_string())
}
