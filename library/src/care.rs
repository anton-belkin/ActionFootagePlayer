// Check, Find moved files, Mend; the renders' cache
// One of the areas of the Library's commands: main.rs is the wiring (the state, main(), generate_handler!'s list)

use super::*;

// ---------- Check, Find moved files ----------

// The player's afp-cut, which writes a key of an MP4 again in place, the file's other keys kept (--set-key)
pub(crate) fn link_tool() -> Option<PathBuf> {
    let player = player().ok()?;
    let probe = index::probe_tool(Some(&player))?;
    let name = if cfg!(windows) { "afp-cut.exe" } else { "afp-cut" };
    Some(probe.with_file_name(name)).filter(|p| p.is_file())
}

// A moment's, clip preview's or export's link written again (its original moved, tags changed)
pub(crate) fn write_link(tool: Option<&PathBuf>, file: &Path, link: &str) -> Result<(), String> {
    // A photo's link is in its XMP: afp-meta writes it (afp-cut writes an MP4's key)
    let lower = file.to_string_lossy().to_lowercase();
    if [".heic", ".heif", ".jpg", ".jpeg"].iter().any(|e| lower.ends_with(e)) {
        let meta = meta_tool().ok_or("ActionFootagePlayer's afp-meta isn't there to write a photo's link")?;
        let o = Command::new(meta).arg(file).arg("--link").arg(link).output().map_err(|e| e.to_string())?;
        return if o.status.success() { Ok(()) } else { Err(String::from_utf8_lossy(&o.stderr).trim().to_string()) };
    }
    let tool = tool.ok_or("ActionFootagePlayer's afp-cut isn't there to write the links")?;
    let o = Command::new(tool).arg("--set-key").arg(file).arg("ch.actionfootageplayer.link").arg(link).output().map_err(|e| e.to_string())?;
    if o.status.success() { Ok(()) } else { Err(String::from_utf8_lossy(&o.stderr).trim().to_string()) }
}

// The player's afp-meta, which writes a photo's own metadata (its tags in its XMP; a HEIC's through afp-photo)
pub(crate) fn meta_tool() -> Option<PathBuf> {
    let name = if cfg!(windows) { "afp-meta.exe" } else { "afp-meta" };
    link_tool().map(|t| t.with_file_name(name)).filter(|p| p.is_file())
}

pub(crate) fn write_photo_tags(tool: Option<&PathBuf>, file: &Path, tags: &[String]) -> Result<(), String> {
    let tool = tool.ok_or("ActionFootagePlayer's afp-meta isn't there to write a photo's tags")?;
    let o = Command::new(tool).arg(file).arg("--tags").arg(serde_json::to_string(tags).unwrap()).output().map_err(|e| e.to_string())?;
    if o.status.success() { Ok(()) } else { Err(String::from_utf8_lossy(&o.stderr).trim().to_string()) }
}

// Check, on the selection: `sel` paths of the footage tree, `links` link files (moments, exports) by their path in the
// project
#[tauri::command]
pub(crate) async fn run_check(shared: State<'_, Shared>, sel: Vec<String>, links: Vec<String>) -> Result<Value, String> {
    let root = shared.lock().unwrap().project.clone().ok_or("Open a project first")?;
    tauri::async_runtime::spawn_blocking(move || checks::check(&root, &sel, &links)).await.map_err(|e| e.to_string())
}

#[tauri::command]
pub(crate) async fn find_moved(shared: State<'_, Shared>, issues: Vec<Value>) -> Result<Value, String> {
    let root = shared.lock().unwrap().project.clone().ok_or("Open a project first")?;
    // The library being mended: the catalog looks at the footage too
    seen::look_at_everything(shared.lock().unwrap().watch.as_ref().map(|w| &w.runner));
    tauri::async_runtime::spawn_blocking(move || checks::find_moved(&root, &issues)).await.map_err(|e| e.to_string())
}

#[tauri::command]
pub(crate) async fn apply_moved(shared: State<'_, Shared>, found: Vec<Value>) -> Result<Vec<String>, String> {
    if shared.lock().unwrap().project.is_none() { return Err("Open a project first".into()); }
    seen::look_at_everything(shared.lock().unwrap().watch.as_ref().map(|w| &w.runner));
    let tool = link_tool();
    tauri::async_runtime::spawn_blocking(move || {
        let relink = |file: &Path, link: &str| write_link(tool.as_ref(), file, link);
        checks::apply(&found, &relink)
    }).await.map_err(|e| e.to_string())
}

// Checks' catalog part: every folder looked at again now (after a script changed files in place), or every file of ours
// read again against the catalog (after a restore that kept times); and how the last of those went
#[tauri::command]
pub(crate) async fn catalog_look(shared: State<'_, Shared>, check: bool) -> Result<(), String> {
    let g = shared.lock().unwrap();
    let w = g.watch.as_ref().ok_or("Open a project first")?;
    seen::look_deep(Some(&w.runner), check);
    Ok(())
}

#[tauri::command]
pub(crate) async fn catalog_care(shared: State<'_, Shared>) -> Result<Value, String> {
    Ok(seen::care(shared.lock().unwrap().watch.as_ref().map(|w| &w.runner)))
}

// The links whose original isn't where they say, as last counted: [{ file, tree, kind }]
#[tauri::command]
pub(crate) async fn missing_links(shared: State<'_, Shared>) -> Result<Vec<Value>, String> {
    Ok(seen::missing_list(shared.lock().unwrap().watch.as_ref().map(|w| &w.runner)))
}

// Mending on one click (Anton, 2026-11-10): every link whose original isn't where it says, looked for by what it is
// (its fingerprint…), the ones found surely written again; the ones with several to choose from given back
#[tauri::command]
pub(crate) async fn mend(shared: State<'_, Shared>) -> Result<Value, String> {
    let root = shared.lock().unwrap().project.clone().ok_or("Open a project first")?;
    let tool = link_tool();
    let out = tauri::async_runtime::spawn_blocking(move || {
        let issues = checks::missing(&root).ok_or("A source of the project isn't reachable now: its clips would all look gone")?;
        let found = checks::find_moved(&root, &issues);
        let all = found.as_array().cloned().unwrap_or_default();
        let (sure, left): (Vec<Value>, Vec<Value>) = all.into_iter().partition(|f| !f["found"].is_null());
        let relink = |file: &Path, link: &str| write_link(tool.as_ref(), file, link);
        let problems = checks::apply(&sure, &relink);
        Ok::<Value, String>(json!({ "missing": issues.len(), "mended": sure.len() - problems.len(), "problems": problems, "left": left }))
    }).await.map_err(|e| e.to_string())??;
    let g = shared.lock().unwrap();
    seen::recount(g.watch.as_ref().map(|w| &w.runner));
    seen::look_at_everything(g.watch.as_ref().map(|w| &w.runner));
    Ok(out)
}

// ---------- renders ----------

#[tauri::command]
pub(crate) async fn delete_renders(shared: State<'_, Shared>, moments: Vec<String>, stale_only: bool) -> Result<changed::Changed, String> {
    // In the background: the window never waits on it (a share can take seconds)
    let shared = shared.inner().clone();
    tauri::async_runtime::spawn_blocking(move || -> Result<changed::Changed, String> {
    let root = shared.lock().unwrap().project.clone().ok_or("Open a project first")?;
    let (n, problems) = renders::delete(&root, &moments, stale_only);
    // The moments and photo links as they are now (changed.rs): the page corrects those, and reads no view again
    let mut c = changed::Changed { answer: json!({ "deleted": n, "problems": problems }), problems, ..Default::default() };
    let (ms, ps): (Vec<String>, Vec<String>) = moments.iter().cloned().partition(|id| id.to_lowercase().ends_with(index::LINK_EXT));
    let flat = changed::flat_previews(&root);
    if !ms.is_empty() { c.add(&root, flat, "moments", &ms); }
    if !ps.is_empty() { c.add(&root, flat, "stills", &ps); }
    Ok(c)
}).await.map_err(|e| e.to_string())?
}
