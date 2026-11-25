// Tags, picks, ratings, comments
// One of the areas of the Library's commands: main.rs is the wiring (the state, main(), generate_handler!'s list)

use super::*;

// ---------- tags ----------

#[tauri::command]
pub(crate) async fn set_tags(shared: State<'_, Shared>, probes: State<'_, index::SharedProbes>, items: Vec<Value>, add: Vec<String>, remove: Vec<String>) -> Result<changed::Changed, String> {
    let root = shared.lock().unwrap().project.clone().ok_or("Open a project first")?;
    let tool = link_tool();
    let meta = meta_tool();
    let probes = probes.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        tags::remember(&add);
        let problems = tags::set(&root, &items, &add, &remove, &|f, l| write_link(tool.as_ref(), f, l), &|f, t| write_photo_tags(meta.as_ref(), f, t));
        // The items as they are now (changed.rs): the page corrects those, and reads no view again
        changed::of_items(&root, &items, problems).with_probes(&probes)
    }).await.map_err(|e| e.to_string())
}

// 1–5 stars on the selection (None: none), kept where its tags are (stills: designed). Answers with the items as they
// are now, as set_tags does
#[tauri::command]
pub(crate) async fn set_rating(shared: State<'_, Shared>, probes: State<'_, index::SharedProbes>, items: Vec<Value>, rating: Option<u8>) -> Result<changed::Changed, String> {
    let root = shared.lock().unwrap().project.clone().ok_or("Open a project first")?;
    let tool = link_tool();
    let meta = meta_tool();
    let probes = probes.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let problems = tags::apply(&root, &items, &tags::Op::Rating(rating), &|f, l| write_link(tool.as_ref(), f, l),
                                   &|f, t| write_photo_tags(meta.as_ref(), f, t), &tags::Stores::of(&root));
        changed::of_items(&root, &items, problems).with_probes(&probes)
    }).await.map_err(|e| e.to_string())
}

// Accept or Reject on the selection (None: neither; docs/design/tags-and-culling.md)
#[tauri::command]
pub(crate) async fn set_pick(shared: State<'_, Shared>, probes: State<'_, index::SharedProbes>, items: Vec<Value>, pick: Option<String>) -> Result<changed::Changed, String> {
    let root = shared.lock().unwrap().project.clone().ok_or("Open a project first")?;
    let tool = link_tool();
    let meta = meta_tool();
    let probes = probes.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let problems = tags::apply(&root, &items, &tags::Op::Pick(pick.as_deref()), &|f, l| write_link(tool.as_ref(), f, l),
                                   &|f, t| write_photo_tags(meta.as_ref(), f, t), &tags::Stores::of(&root));
        changed::of_items(&root, &items, problems).with_probes(&probes)
    }).await.map_err(|e| e.to_string())
}

// The free-text comment of the selection (docs/design/tags-and-culling.md, "The comment"): the same text on each of `items`,
// None or empty: taken out
#[tauri::command]
pub(crate) async fn set_comment(shared: State<'_, Shared>, probes: State<'_, index::SharedProbes>, items: Vec<Value>, comment: Option<String>) -> Result<changed::Changed, String> {
    let root = shared.lock().unwrap().project.clone().ok_or("Open a project first")?;
    let tool = link_tool();
    let probes = probes.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let problems = tags::set_comment(&root, &items, comment.as_deref(), &|f, l| write_link(tool.as_ref(), f, l));
        changed::of_items(&root, &items, problems).with_probes(&probes)
    }).await.map_err(|e| e.to_string())
}

// The selection (none: every original and moment) synced with the library's tag stores: what changed in Finder comes in
#[tauri::command]
pub(crate) async fn sync_tags(shared: State<'_, Shared>, items: Option<Vec<Value>>) -> Result<Value, String> {
    let root = shared.lock().unwrap().project.clone().ok_or("Open a project first")?;
    let tool = link_tool();
    tauri::async_runtime::spawn_blocking(move || {
        let items = items.filter(|i| !i.is_empty()).unwrap_or_else(|| tags::every_item(&root));
        let (came, problems) = tags::sync(&root, &items, &|f, l| write_link(tool.as_ref(), f, l), &tags::Stores::of(&root));
        json!({ "looked": items.len(), "came": came, "problems": problems })
    }).await.map_err(|e| e.to_string())
}

// The tag stores this computer syncs (Project settings → Tags): written into this computer's own file for the project
// (never Project.afproject: it keeps only the locations), its other choices kept; then everything synced once
#[tauri::command]
pub(crate) async fn save_tag_stores(shared: State<'_, Shared>, stores: Vec<String>) -> Result<Value, String> {
    let root = shared.lock().unwrap().project.clone().ok_or("Open a project first")?;
    let tool = link_tool();
    tauri::async_runtime::spawn_blocking(move || {
        let id = project_id(&root)?;
        let file = locations::local_file(&id).ok_or("No place for this computer's files")?;
        write_tag_stores(&file, &root, &stores)?;
        let items = tags::every_item(&root);
        let (came, problems) = tags::sync(&root, &items, &|f, l| write_link(tool.as_ref(), f, l), &tags::Stores::of(&root));
        Ok(json!({ "looked": items.len(), "came": came, "problems": problems }))
    }).await.map_err(|e| e.to_string())?
}

/// `stores` (the ones known) into this computer's file for the project, what else it keeps left as it is; a file left
/// with nothing of this computer's is removed
pub(crate) fn write_tag_stores(file: &Path, root: &Path, stores: &[String]) -> Result<(), String> {
    let list: Vec<&String> = stores.iter().filter(|s| *s == "finder").collect();
    let mut doc = locations::read_json(file).filter(Value::is_object).unwrap_or_else(|| json!({ "project": root.display().to_string(), "locations": [] }));
    let o = doc.as_object_mut().unwrap();
    if list.is_empty() { o.remove("tag_stores"); } else { o.insert("tag_stores".into(), json!(list)); }
    // Kept with nothing else in it too: it says where the project is here (locations::note_here)
    o.insert("project".into(), json!(root.display().to_string()));
    if let Some(d) = file.parent() { std::fs::create_dir_all(d).map_err(|e| e.to_string())?; }
    std::fs::write(file, serde_json::to_vec_pretty(&doc).unwrap()).map_err(|e| e.to_string())
}

// A tag renamed everywhere in the project (every view's items that have it); to a name in use: merged into it
#[tauri::command]
pub(crate) async fn rename_tag(shared: State<'_, Shared>, from: String, to: String) -> Result<Value, String> {
    let root = shared.lock().unwrap().project.clone().ok_or("Open a project first")?;
    let (from, to) = (from.trim().to_string(), to.split_whitespace().collect::<Vec<_>>().join(" "));
    if from.is_empty() || to.is_empty() { return Err("A tag and its new name".into()); }
    let tool = link_tool();
    let meta = meta_tool();
    tauri::async_runtime::spawn_blocking(move || {
        let items = tags::with_tag(&root, &from);
        let problems = tags::apply(&root, &items, &tags::Op::Rename { from: &from, to: &to }, &|f, l| write_link(tool.as_ref(), f, l),
                                   &|f, t| write_photo_tags(meta.as_ref(), f, t), &tags::Stores::of(&root));
        tags::remember(std::slice::from_ref(&to));
        json!({ "count": items.len(), "problems": problems })
    }).await.map_err(|e| e.to_string())
}

// The tags used last on this computer (the player's K writes them too)
#[tauri::command]
pub(crate) fn recent_tags() -> Vec<String> { tags::recent() }
