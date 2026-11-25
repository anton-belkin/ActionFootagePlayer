// Stabilize optically: the lens; Source settings
// One of the areas of the Library's commands: main.rs is the wiring (the state, main(), generate_handler!'s list)

use super::*;

// ---------- Stabilize optically: the lens ----------

/// The lens types to choose from for these clips (gyroflow-optical --lens-types: vs-gyroflow's lens_pick, per camera):
/// for each path asked, its camera and [[type, label]]; none for a camera without (GoPro: its lens is in the file)
#[tauri::command]
pub(crate) async fn lens_types(paths: Vec<String>) -> Result<Vec<Value>, String> {
    tauri::async_runtime::spawn_blocking(move || -> Result<Vec<Value>, String> {
        let tool = optical_tool()?;
        let mut cmd = Command::new(&tool);
        cmd.arg("--lens-types").args(&paths).stdin(Stdio::null()).stderr(Stdio::null());
        #[cfg(windows)]
        { use std::os::windows::process::CommandExt; cmd.creation_flags(0x0800_0000); }
        let out = cmd.output().map_err(|e| e.to_string())?;
        Ok(lens_types_of(&String::from_utf8_lossy(&out.stdout)))
    }).await.map_err(|e| e.to_string())?
}

/// gyroflow-optical --lens-types' lines: {"path", "camera", "types": [[type, label]...], "type" (chosen), "recorded", "mode"
/// (the camera's own number for it)}, those that are
pub(crate) fn lens_types_of(text: &str) -> Vec<Value> {
    text.lines().filter_map(|l| serde_json::from_str::<Value>(l).ok())
        .filter(|v| v["path"].is_string() && v["types"].is_array())
        .map(|v| json!({ "path": v["path"], "camera": v["camera"], "types": v["types"], "type": v["type"], "recorded": v["recorded"], "default": v["default"], "mode": v["mode"] })).collect()
}

/// A clip's camera and the lens profile the player takes for it (gyroflow-probe --lens, as the player's Source → Lens
/// shows it): {"camera", "lens" (a profile's label, "in the file", or "" for none), "types", "type", "recorded", "mode"}
#[tauri::command]
pub(crate) async fn lens_info(path: String) -> Result<Value, String> {
    tauri::async_runtime::spawn_blocking(move || -> Result<Value, String> {
        let probe = index::probe_tool(player().ok().as_deref()).ok_or("ActionFootagePlayer's gyroflow-probe isn't there")?;
        let mut cmd = Command::new(&probe);
        cmd.arg("--lens").arg(&path).stdin(Stdio::null()).stderr(Stdio::null());
        #[cfg(windows)]
        { use std::os::windows::process::CommandExt; cmd.creation_flags(0x0800_0000); }
        let out = cmd.output().map_err(|e| e.to_string())?;
        serde_json::from_slice(&out.stdout).map_err(|e| e.to_string())
    }).await.map_err(|e| e.to_string())?
}

/// The lens of the Gyroflow project beside this clip kept as the default for its camera, mode and size, in the user's
/// lens list (gyroflow-probe --save-lens: vs-gyroflow's lens_pick::save_default). Every clip of that camera is read
/// again: the ones of that mode and size, without a lens of their own, have it now. {"camera", "type", "type_label",
/// "size", "lens", "profile", "file", "again" (how many clips are read again)}
#[tauri::command]
pub(crate) async fn save_project_lens(probes: State<'_, index::SharedProbes>, path: String) -> Result<Value, String> {
    let probes = probes.inner().clone();
    tauri::async_runtime::spawn_blocking(move || -> Result<Value, String> {
        let probe = index::probe_tool(player().ok().as_deref()).ok_or("ActionFootagePlayer's gyroflow-probe isn't there")?;
        let before = index::lens_list_stamp();
        let mut cmd = Command::new(&probe);
        cmd.arg("--save-lens").arg(&path).stdin(Stdio::null()).stderr(Stdio::null());
        #[cfg(windows)]
        { use std::os::windows::process::CommandExt; cmd.creation_flags(0x0800_0000); }
        let out = cmd.output().map_err(|e| e.to_string())?;
        let mut v: Value = String::from_utf8_lossy(&out.stdout).lines().last().and_then(|l| serde_json::from_str(l).ok())
            .ok_or("This ActionFootagePlayer can't save a lens: update it")?;
        if let Some(e) = v["error"].as_str() { return Err(e.to_string()); }
        // Saved for any mode: the camera's clips of that size have it at once (their answers asked again behind)
        let whole = (v["type"].as_str() == Some("any")).then(|| (v["size"].as_str().unwrap_or("").to_string(),
            format!("{}, from your list: {}", v["type_label"].as_str().unwrap_or("Any mode"), v["lens"].as_str().unwrap_or("").replace('_', " "))));
        let again = probes.lock().unwrap().lens_saved_for(v["camera"].as_str().unwrap_or(""), &before, whole.as_ref().map(|(s, l)| (s.as_str(), l.as_str())));
        v["again"] = json!(again);
        Ok(v)
    }).await.map_err(|e| e.to_string())?
}

/// A lens of the clip's camera (gyroflow-probe --lens's "candidates" id) kept for the clip's whole mode in the player's
/// own lens database (gyroflow-probe --assign-lens: vs-gyroflow's lens_db.rs); None: the one assigned taken away
/// (--forget-lens). Every clip of that camera is read again. {"file", "mode", "lens"} or {"removed", "mode"}, and
/// "again" (how many clips are read again)
#[tauri::command]
pub(crate) async fn assign_lens(probes: State<'_, index::SharedProbes>, path: String, id: Option<String>) -> Result<Value, String> {
    let probes = probes.inner().clone();
    tauri::async_runtime::spawn_blocking(move || -> Result<Value, String> {
        let probe = index::probe_tool(player().ok().as_deref()).ok_or("ActionFootagePlayer's gyroflow-probe isn't there")?;
        let before = index::lens_list_stamp();
        let mut cmd = Command::new(&probe);
        match &id {
            Some(id) => { cmd.arg("--assign-lens").arg(&path).arg(id); }
            None => { cmd.arg("--forget-lens").arg(&path); }
        }
        cmd.stdin(Stdio::null()).stderr(Stdio::null());
        #[cfg(windows)]
        { use std::os::windows::process::CommandExt; cmd.creation_flags(0x0800_0000); }
        let out = cmd.output().map_err(|e| e.to_string())?;
        let mut v: Value = String::from_utf8_lossy(&out.stdout).lines().last().and_then(|l| serde_json::from_str(l).ok())
            .ok_or("This ActionFootagePlayer can't keep a lens for a mode: update it")?;
        if let Some(e) = v["error"].as_str() { return Err(e.to_string()); }
        let camera = probes.lock().unwrap().get(&path).and_then(|a| a["camera"].as_str().map(String::from)).unwrap_or_default();
        let again = if camera.is_empty() { 0 } else { probes.lock().unwrap().lens_saved_for(&camera, &before, None) };
        v["again"] = json!(again);
        Ok(v)
    }).await.map_err(|e| e.to_string())?
}

/// One of what Find lens / Estimate lens found for a clip ("0".., "estimate") kept for its whole mode in the player's
/// own lens database (gyroflow-optical --keep-lens: vs-gyroflow's lens_find::keep). The camera's clips are read again.
/// {"file", "mode", "lens", "again"}
#[tauri::command]
pub(crate) async fn keep_found_lens(probes: State<'_, index::SharedProbes>, path: String, n: String) -> Result<Value, String> {
    let probes = probes.inner().clone();
    tauri::async_runtime::spawn_blocking(move || -> Result<Value, String> {
        let tool = optical_tool()?;
        let before = index::lens_list_stamp();
        let mut cmd = Command::new(&tool);
        cmd.arg("--keep-lens").arg(&path).arg(&n).stdin(Stdio::null()).stderr(Stdio::null());
        #[cfg(windows)]
        { use std::os::windows::process::CommandExt; cmd.creation_flags(0x0800_0000); }
        let out = cmd.output().map_err(|e| e.to_string())?;
        let mut v: Value = String::from_utf8_lossy(&out.stdout).lines().last().and_then(|l| serde_json::from_str(l).ok())
            .ok_or("This ActionFootagePlayer can't keep a lens found: update it")?;
        if let Some(e) = v["error"].as_str() { return Err(e.to_string()); }
        let camera = probes.lock().unwrap().get(&path).and_then(|a| a["camera"].as_str().map(String::from)).unwrap_or_default();
        let again = if camera.is_empty() { 0 } else { probes.lock().unwrap().lens_saved_for(&camera, &before, None) };
        v["again"] = json!(again);
        Ok(v)
    }).await.map_err(|e| e.to_string())?
}

/// The lenses to choose from for a clip's mode, with what Find and Estimate said of each on it (gyroflow-optical --lenses:
/// vs-gyroflow's lens_find::choices), for the lens dialog
#[tauri::command]
pub(crate) async fn lens_choices(path: String) -> Result<Value, String> {
    tauri::async_runtime::spawn_blocking(move || -> Result<Value, String> {
        let tool = optical_tool()?;
        let mut cmd = Command::new(&tool);
        cmd.arg("--lenses").arg(&path).stdin(Stdio::null()).stderr(Stdio::null());
        #[cfg(windows)]
        { use std::os::windows::process::CommandExt; cmd.creation_flags(0x0800_0000); }
        let out = cmd.output().map_err(|e| e.to_string())?;
        let v: Value = String::from_utf8_lossy(&out.stdout).lines().last().and_then(|l| serde_json::from_str(l).ok())
            .ok_or("This ActionFootagePlayer can't list a clip's lenses: update it")?;
        if let Some(e) = v["error"].as_str() { return Err(e.to_string()); }
        Ok(v)
    }).await.map_err(|e| e.to_string())?
}

/// A lens tried on one clip alone before its whole mode takes it (the lens dialog's Try on this clip): `how` "keep" (one
/// Find or Estimate found, `what` its n) or "assign" (one of the mode's, `what` its id), its profile from
/// gyroflow-optical --lens-for, kept in the clip's own file (`source.lens`, set_source: noted to the catalog); "forget"
/// takes it away. The clip's probe read again
#[tauri::command]
pub(crate) async fn try_lens(shared: State<'_, Shared>, probes: State<'_, index::SharedProbes>, clip: String, path: String, how: String, what: Option<String>) -> Result<Value, String> {
    let root = shared.lock().unwrap().project.clone().ok_or("Open a project first")?;
    let probes = probes.inner().clone();
    tauri::async_runtime::spawn_blocking(move || -> Result<Value, String> {
        let lens = if how == "forget" { Value::Null } else {
            let tool = optical_tool()?;
            let mut cmd = Command::new(&tool);
            cmd.arg("--lens-for").arg(&path).arg(&how).arg(what.as_deref().unwrap_or_default()).stdin(Stdio::null()).stderr(Stdio::null());
            #[cfg(windows)]
            { use std::os::windows::process::CommandExt; cmd.creation_flags(0x0800_0000); }
            let out = cmd.output().map_err(|e| e.to_string())?;
            let v: Value = String::from_utf8_lossy(&out.stdout).lines().last().and_then(|l| serde_json::from_str(l).ok())
                .ok_or("This ActionFootagePlayer can't try a lens on one clip: update it")?;
            if let Some(e) = v["error"].as_str() { return Err(e.to_string()); }
            v
        };
        let name = lens["name"].clone();
        let change: serde_json::Map<String, Value> = [("lens".to_string(), lens)].into_iter().collect();
        let problems = tags::set_source(&root, &[(clip.clone(), change)]);
        if let Some(p) = problems.first() { return Err(p.clone()); }
        probes.lock().unwrap().forget(&[path.clone()]);
        Ok(json!({ "lens": name }))
    }).await.map_err(|e| e.to_string())?
}

/// The lens tried on a clip made its whole mode's (gyroflow-optical --promote-lens: the player's own lens database), and
/// taken away from the clip's own file (the mode's is the same now). The camera's clips are read again
#[tauri::command]
pub(crate) async fn promote_lens(shared: State<'_, Shared>, probes: State<'_, index::SharedProbes>, clip: String, path: String) -> Result<Value, String> {
    let root = shared.lock().unwrap().project.clone().ok_or("Open a project first")?;
    let probes = probes.inner().clone();
    tauri::async_runtime::spawn_blocking(move || -> Result<Value, String> {
        let tool = optical_tool()?;
        let before = index::lens_list_stamp();
        let mut cmd = Command::new(&tool);
        cmd.arg("--promote-lens").arg(&path).stdin(Stdio::null()).stderr(Stdio::null());
        #[cfg(windows)]
        { use std::os::windows::process::CommandExt; cmd.creation_flags(0x0800_0000); }
        let out = cmd.output().map_err(|e| e.to_string())?;
        let mut v: Value = String::from_utf8_lossy(&out.stdout).lines().last().and_then(|l| serde_json::from_str(l).ok())
            .ok_or("This ActionFootagePlayer can't give a clip's lens to its mode: update it")?;
        if let Some(e) = v["error"].as_str() { return Err(e.to_string()); }
        let change: serde_json::Map<String, Value> = [("lens".to_string(), Value::Null)].into_iter().collect();
        let problems = tags::set_source(&root, &[(clip, change)]);
        if let Some(p) = problems.first() { return Err(p.clone()); }
        let camera = probes.lock().unwrap().get(&path).and_then(|a| a["camera"].as_str().map(String::from)).unwrap_or_default();
        probes.lock().unwrap().forget(&[path.clone()]);
        let again = if camera.is_empty() { 0 } else { probes.lock().unwrap().lens_saved_for(&camera, &before, None) };
        v["again"] = json!(again);
        Ok(v)
    }).await.map_err(|e| e.to_string())?
}

/// The lens type of these clips (their paths in the tree), kept in each one's own file; None: as the camera recorded
#[tauri::command]
pub(crate) async fn set_lens_type(shared: State<'_, Shared>, probes: State<'_, index::SharedProbes>, clips: Vec<String>, lens_type: Option<String>) -> Result<Vec<String>, String> {
    let root = shared.lock().unwrap().project.clone().ok_or("Open a project first")?;
    if lens_type.as_deref().is_some_and(|t| t.is_empty() || !t.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')) {
        return Err("no such lens type".into());
    }
    let probes = probes.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let problems = tags::set_lens_type(&root, &clips, lens_type.as_deref());
        // What the index knows about their lens was worked out with the type they had: read again
        let paths: Vec<String> = clips.iter().filter_map(|t| tags::clip_own(&root, t).ok()).map(|(_, c)| c.display().to_string()).collect();
        probes.lock().unwrap().forget(&paths);
        problems
    }).await.map_err(|e| e.to_string())
}

/// The Source settings of clips (the Source Settings tab, Batch set Source settings): [{ "clip": its path in the
/// tree, "source": { key: value, or null to take it out } }], each clip's own; only the keys given are written
#[tauri::command]
pub(crate) async fn set_source(shared: State<'_, Shared>, probes: State<'_, index::SharedProbes>, entries: Vec<Value>) -> Result<changed::Changed, String> {
    let root = shared.lock().unwrap().project.clone().ok_or("Open a project first")?;
    let list: Vec<(String, serde_json::Map<String, Value>)> = entries.iter().filter_map(|e| Some((e["clip"].as_str()?.to_string(), e["source"].as_object()?.clone())))
        .filter(|(_, s)| !s.is_empty()).collect();
    let probes = probes.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let problems = tags::set_source(&root, &list);
        // A lens type given or taken away changes the clip's lens: what the index knows of it is read again
        let typed: Vec<String> = list.iter().filter(|(_, s)| s.contains_key("lens_type")).filter_map(|(t, _)| tags::clip_own(&root, t).ok())
            .map(|(_, c)| c.display().to_string()).collect();
        if !typed.is_empty() { probes.lock().unwrap().forget(&typed); }
        // The clips as they are now (changed.rs)
        changed::of_clips(&root, &list.iter().map(|(t, _)| t.clone()).collect::<Vec<_>>(), problems).with_probes(&probes)
    }).await.map_err(|e| e.to_string())
}
