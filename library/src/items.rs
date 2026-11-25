// What's done to the items: take in, Quick Look, rename, delete, new folder, the library as JSON
// One of the areas of the Library's commands: main.rs is the wiring (the state, main(), generate_handler!'s list)

use super::*;

// ---------- Take in another project ----------

#[tauri::command]
pub(crate) fn choose_other(shared: State<Shared>) -> Result<Option<String>, String> {
    shared.lock().unwrap().project.clone().ok_or("Open a project first")?;
    Ok(rfd::FileDialog::new().set_title("The project to take in: its Project.afproject").add_filter("Action Footage project", &["afproject"])
        .pick_file().and_then(|f| f.parent().map(|d| d.display().to_string())))
}

#[tauri::command]
pub(crate) async fn take_in_plan(shared: State<'_, Shared>, other: String, place: String) -> Result<Value, String> {
    let root = shared.lock().unwrap().project.clone().ok_or("Open a project first")?;
    tauri::async_runtime::spawn_blocking(move || {
        let mut p = takein::plan(&root, Path::new(&other), &place)?;
        // Its clips also elsewhere in this project (the camera uploaded them to the inbox too, say)
        let mounts = locations::mounts();
        let theirs: Vec<locations::Location> = locations::locations(Path::new(&other), &mounts);
        let sel: Vec<String> = theirs.iter().filter_map(|l| l.dir.as_ref()).filter_map(|d| {
            locations::locations(&root, &mounts).iter().filter_map(|h| h.dir.as_ref().and_then(|hd| locations::relative(d, hd).map(|r| if h.at.is_empty() { r } else { format!("{}/{r}", h.at) }))).next()
        }).collect();
        if !sel.is_empty() {
            let c = checks::check(&root, &sel, &[]);
            p["issues"] = json!(c["issues"].as_array().map(|a| a.iter().filter(|i| i["kind"] == "twice" || i["kind"] == "own2").cloned().collect::<Vec<_>>()).unwrap_or_default());
        }
        Ok(p)
    }).await.map_err(|e| e.to_string())?
}

#[tauri::command]
pub(crate) async fn take_in_run(shared: State<'_, Shared>, other: String, place: String) -> Result<Value, String> {
    let root = shared.lock().unwrap().project.clone().ok_or("Open a project first")?;
    seen::look_at_everything(shared.lock().unwrap().watch.as_ref().map(|w| &w.runner));
    let tool = link_tool();
    tauri::async_runtime::spawn_blocking(move || {
        let relink = |file: &Path, link: &str| write_link(tool.as_ref(), file, link);
        takein::take_in(&root, Path::new(&other), &place, &relink, true)
    }).await.map_err(|e| e.to_string())?
}

// ---------- Quick Look, renaming ----------

// The system's Quick Look of these files (macOS): what Finder's space bar shows. A second call closes it
static QUICKLOOK: Mutex<Option<std::process::Child>> = Mutex::new(None);

#[tauri::command]
pub(crate) async fn quick_look(paths: Vec<String>) -> Result<bool, String> {
    // In the background: the window never waits on it (a share can take seconds)
    tauri::async_runtime::spawn_blocking(move || -> Result<bool, String> {
    let mut q = QUICKLOOK.lock().unwrap();
    if let Some(mut c) = q.take() {
        if c.try_wait().ok().flatten().is_none() { let _ = c.kill(); let _ = c.wait(); return Ok(false); }
    }
    if paths.is_empty() { return Ok(false); }
    if !cfg!(target_os = "macos") { return Err("Quick Look is macOS's: on this system, open the file instead".into()); }
    // The system's panel, by the app's own helper (afp-quicklook, beside it); qlmanage -p crashes on videos on macOS 27
    let helper = std::env::current_exe().ok().and_then(|e| e.parent().map(|d| d.join("afp-quicklook"))).filter(|h| h.is_file());
    let child = match helper {
        Some(h) => Command::new(h).args(&paths).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).spawn(),
        None => Command::new("/usr/bin/qlmanage").arg("-p").args(&paths).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).spawn(),
    }.map_err(|e| e.to_string())?;
    *q = Some(child);
    Ok(true)
}).await.map_err(|e| e.to_string())?
}

// A name a file can have on every system
fn good_name(name: &str) -> Result<String, String> {
    let n = name.trim();
    if n.is_empty() || n == "." || n == ".." || n.starts_with('.') { return Err("That name can't be a file's".into()); }
    if n.chars().any(|c| matches!(c, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|') || c.is_control()) {
        return Err("A name can't have / \\ : * ? \" < > |".into());
    }
    Ok(n.to_string())
}

// Renamed in the project's own folders: a moment (its own Gyroflow project and renders with it), a photo (a Live
// Photo's video with it), an export, a folder of moments, clip previews, photos or exports (_Renders' with _Moments').
// Footage isn't renamed here: Finder renames it, and Find moved files then follows
#[tauri::command]
pub(crate) async fn rename_item(app: tauri::AppHandle, shared: State<'_, Shared>, probes: State<'_, index::SharedProbes>, view: String, kind: String, path: String, name: String) -> Result<changed::Changed, String> {
    // In the background: the window never waits on it (a share can take seconds)
    let shared = shared.inner().clone();
    let probes = probes.inner().clone();
    tauri::async_runtime::spawn_blocking(move || -> Result<changed::Changed, String> {
        let root = shared.lock().unwrap().project.clone().ok_or("Open a project first")?;
        let to = rename_in(&root, &view, &kind, &path, &name)?;
        thumbs_follow(&app, vec![(path.clone().into(), to.clone().into())]);
        // What it is now, and that the old name is gone (changed.rs); its new path is the answer
        Ok(changed::of_rename(&root, &view, &kind, Path::new(&path), &to).with_probes(&probes))
    }).await.map_err(|e| e.to_string())?
}

/// An album renamed: each of its folders (in _Clips, _Moments, _Stills: up to three) given the name, as one change.
/// When one can't be, the ones renamed already get their old name back, and nothing is changed (the window then
/// shows the album as it was, whole); the folders are read again after it, as for a folder renamed
#[tauri::command]
pub(crate) async fn rename_album(app: tauri::AppHandle, shared: State<'_, Shared>, paths: Vec<String>, name: String) -> Result<changed::Changed, String> {
    let shared = shared.inner().clone();
    tauri::async_runtime::spawn_blocking(move || -> Result<changed::Changed, String> {
        let root = shared.lock().unwrap().project.clone().ok_or("Open a project first")?;
        let to = rename_album_in(&root, &paths, &name)?;
        thumbs_follow(&app, paths.iter().map(PathBuf::from).zip(to.iter().map(PathBuf::from)).collect());
        Ok(changed::Changed { answer: json!(to), ..Default::default() }.reload_all())
    }).await.map_err(|e| e.to_string())?
}

/// Files or folders renamed or moved (from, to): the pictures the Library keeps of them follow (index::thumbs_follow),
/// in the background: the command answers meanwhile, and the window has them already (app.js's thumbFollows)
pub(crate) fn thumbs_follow(app: &tauri::AppHandle, moved: Vec<(PathBuf, PathBuf)>) {
    let Ok(dir) = app.path().app_cache_dir() else { return };
    let cache = dir.join("thumbs");
    std::thread::spawn(move || for (a, b) in moved { index::thumbs_follow(&cache, &a, &b); });
}

pub(crate) fn rename_album_in(root: &Path, paths: &[String], name: &str) -> Result<Vec<String>, String> {
    let mut done: Vec<(String, String)> = Vec::new();
    for p in paths {
        match rename_in(root, "footage", "folder", p, name) {
            Ok(to) => done.push((p.clone(), to)),
            Err(e) => {
                // Back as it was, the last first
                let was = |p: &str| Path::new(p).file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                let mut also = Vec::new();
                for (from, to) in done.iter().rev() { if let Err(b) = rename_in(root, "footage", "folder", to, &was(from)) { also.push(b); } }
                return Err(if also.is_empty() { e } else { format!("{e}; and couldn't be named back: {}", also.join(" · ")) });
            }
        }
    }
    Ok(done.into_iter().map(|(_, t)| t).collect())
}

pub(crate) fn rename_in(root: &Path, view: &str, kind: &str, path: &str, name: &str) -> Result<String, String> {
    let root = root.to_path_buf();
    let name = good_name(name)?;
    let from = PathBuf::from(&path);
    // The project's own files and folders, wherever they are in its folder (docs/design/library-management.md §8); among its
    // footage (a project next to it), only a link file (a moment or clip preview beside its clip)
    let footage = index::footage_dirs(&root);
    let linked = kind != "folder" && index::link(&from).is_some();
    if from == root || !from.starts_with(&root) || !(index::own_area(&root, &from, &footage) || linked) {
        return Err("Footage is renamed in Finder (Find moved files then updates what points at it). Here: moments, clip previews, photos, exports and their folders".into());
    }
    let dir = from.parent().ok_or("no folder")?.to_path_buf();
    let fname = from.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    // What's renamed, both its paths, told to the catalog once done (the views read it; on a share the system's notice
    // comes late or never)
    let touched: std::cell::RefCell<Vec<PathBuf>> = std::cell::RefCell::new(Vec::new());
    let mv = |a: &Path, b: &Path| -> Result<(), String> {
        afp_catalog::store::nas::stat();
        if b.exists() { return Err(format!("{} exists already", b.display())); }
        afp_catalog::store::nas::write();
        std::fs::rename(a, b).map_err(|e| format!("{}: {e}", a.display()))?;
        touched.borrow_mut().extend([a.to_path_buf(), b.to_path_buf()]);
        Ok(())
    };
    let renamed = (|| -> Result<String, String> {
    if kind == "folder" {
        let to = dir.join(&name);
        mv(&from, &to)?;
        // The moments' renders follow their folder (_Renders/<its path>, a folder of _Moments by its path there)
        if let Ok(rel) = from.strip_prefix(root.join("_Moments")).or_else(|_| from.strip_prefix(&root)) {
            let r = root.join("_Renders").join(rel);
            if r.is_dir() { let _ = mv(&r, &r.with_file_name(&name)); }
        }
        return Ok(to.display().to_string());
    }
    match kind {
        "moment" => {
            let ext = if fname.to_lowercase().ends_with(index::LINK_EXT) { fname[fname.len() - index::LINK_EXT.len()..].to_string() }
                else { fname.rfind('.').map(|i| fname[i..].to_string()).unwrap_or_default() };
            let old = fname[..fname.len() - ext.len()].to_string();
            let to = dir.join(format!("{name}{ext}"));
            mv(&from, &to)?;
            let g = dir.join(format!("{old}.gyroflow"));
            if g.is_file() { let _ = mv(&g, &dir.join(format!("{name}.gyroflow"))); }
            if let Ok(rel) = from.strip_prefix(&root) {
                let rel = rel.to_string_lossy().replace('\\', "/");
                for p in index::RENDER_PROFILES {
                    let r = index::render_path(&root, &rel, p);
                    if r.is_file() { let _ = mv(&r, &r.with_file_name(format!("{name}.{p}.mov"))); }
                }
            }
            Ok(to.display().to_string())
        }
        _ => {
            // A photo or an export: its extension kept (a Motion Photo's .MP.jpg whole), a Live Photo's video along
            let lower = fname.to_lowercase();
            let ext = if lower.ends_with(albums::PHOTO_EXT) { fname[fname.len() - albums::PHOTO_EXT.len()..].to_string() }
                else if lower.ends_with(".mp.jpg") { fname[fname.len() - 7..].to_string() } else { fname.rfind('.').map(|i| fname[i..].to_string()).unwrap_or_default() };
            let stem = &fname[..fname.len() - ext.len()];
            let to = dir.join(format!("{name}{ext}"));
            mv(&from, &to)?;
            if view == "stills" {
                for e in [".mov", ".MOV"] {
                    let v = dir.join(format!("{stem}{e}"));
                    if v.is_file() { let _ = mv(&v, &dir.join(format!("{name}{e}"))); }
                }
            }
            Ok(to.display().to_string())
        }
    }
    })();
    seen::note_now(&touched.borrow());
    renamed
}

#[cfg(test)]
pub(crate) static TRASH_REMOVES: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

// Where `file` goes when deleted: the system's Trash on a local disk, the share's own #recycle on a network one (a
// UGREEN or Synology share keeps what's deleted there; Finder can't put a network file in the Trash). Ok: where it went
pub(crate) fn to_trash(file: &Path) -> Result<String, String> {
    afp_catalog::store::nas::write();
    // Tests on a local disk: removed (Finder's Trash isn't theirs to fill)
    #[cfg(test)]
    if TRASH_REMOVES.load(std::sync::atomic::Ordering::Relaxed) {
        let r = if file.is_dir() { std::fs::remove_dir_all(file) } else { std::fs::remove_file(file) };
        return r.map(|_| "removed (test)".to_string()).map_err(|e| e.to_string());
    }
    if cfg!(target_os = "macos") {
        // The volume it's on and its kind, from mount: "//u@host/share on /Volumes/share (smbfs, ...)"
        let out = std::process::Command::new("/sbin/mount").output().map(|o| String::from_utf8_lossy(&o.stdout).into_owned()).unwrap_or_default();
        let mut best: Option<(PathBuf, String)> = None;
        for line in out.lines() {
            let Some((_, rest)) = line.split_once(" on ") else { continue };
            let Some((at, kind)) = rest.rsplit_once(" (") else { continue };
            let at = PathBuf::from(at);
            if file.starts_with(&at) && best.as_ref().is_none_or(|(b, _)| at.as_os_str().len() > b.as_os_str().len()) {
                best = Some((at, kind.split([',', ')']).next().unwrap_or("").trim().to_string()));
            }
        }
        let (at, kind) = best.unwrap_or((PathBuf::from("/"), "apfs".into()));
        if ["smbfs", "nfs", "afpfs", "webdav", "cifs"].contains(&kind.as_str()) {
            if !at.join("#recycle").is_dir() {
                return Err(format!("{}: this share has no recycle bin to put it in: delete it in Finder", file.display()));
            }
            let r = if file.is_dir() { std::fs::remove_dir_all(file) } else { std::fs::remove_file(file) };
            return r.map(|_| "the share's #recycle".to_string()).map_err(|e| format!("{}: {e}", file.display()));
        }
        let script = format!("tell application \"Finder\" to delete (POSIX file \"{}\" as alias)", file.display().to_string().replace('\\', "\\\\").replace('"', "\\\""));
        let ok = std::process::Command::new("/usr/bin/osascript").args(["-e", &script]).output().map(|o| o.status.success()).unwrap_or(false);
        return if ok && !file.exists() { Ok("the Trash".into()) } else { Err(format!("{}: Finder couldn't move it to the Trash", file.display())) };
    }
    if cfg!(target_os = "windows") {
        let what = if file.is_dir() { "DeleteDirectory" } else { "DeleteFile" };
        let cmd = format!("Add-Type -AssemblyName Microsoft.VisualBasic; [Microsoft.VisualBasic.FileIO.FileSystem]::{what}('{}','OnlyErrorDialogs','SendToRecycleBin')", file.display().to_string().replace('\'', "''"));
        let ok = std::process::Command::new("powershell").args(["-NoProfile", "-Command", &cmd]).output().map(|o| o.status.success()).unwrap_or(false);
        return if ok && !file.exists() { Ok("the Recycle Bin".into()) } else { Err(format!("{}: couldn't move it to the Recycle Bin", file.display())) };
    }
    Err("Delete it in the file manager".into())
}

// Deletes `items` ({kind, path}) of the project's own folders, to the Trash: a folder whole, a moment with its
// Gyroflow project and renders, a photo with its Live Photo video, a clip's preview (the clip itself stays: its
// footage is deleted in Finder), an export. What went where, and what couldn't
#[tauri::command]
pub(crate) async fn delete_items(shared: State<'_, Shared>, items: Vec<serde_json::Value>) -> Result<changed::Changed, String> {
    // In the background: the window never waits on it (a share can take seconds)
    let shared = shared.inner().clone();
    tauri::async_runtime::spawn_blocking(move || -> Result<changed::Changed, String> {
        let root = shared.lock().unwrap().project.clone().ok_or("Open a project first")?;
        // What's gone, and the clips whose previews went, as they are now (changed.rs); what went where is the answer
        let clips = changed::before_delete(&items);
        let answer = delete_in(&root, &items);
        Ok(changed::of_delete(&root, &items, &clips, answer))
    }).await.map_err(|e| e.to_string())?
}

pub(crate) fn delete_in(root: &Path, items: &[Value]) -> Value {
    let root = root.to_path_buf();
    let footage = index::footage_dirs(&root);
    let (mut done, mut problems) = (0usize, Vec::<String>::new());
    let mut went = std::collections::BTreeSet::new();
    // What went, told to the catalog at the end (the views read it: on a share the system's notice comes late or never)
    let mut gone: Vec<PathBuf> = Vec::new();
    for it in items {
        let path = PathBuf::from(it["path"].as_str().unwrap_or(""));
        let kind = it["kind"].as_str().unwrap_or("");
        // The project's own files and folders, wherever they are in its folder (docs/design/library-management.md §8); among
        // its footage, only a link file
        let linked = kind != "folder" && index::link(&path).is_some();
        if path == root || !path.starts_with(&root) || !(index::own_area(&root, &path, &footage) || linked) || (kind == "folder" && footage.iter().any(|d| d.starts_with(&path))) {
            problems.push(format!("{}: not the project's own. Footage is deleted in Finder", path.display()));
            continue;
        }
        if !path.exists() { continue; }
        let mut also: Vec<PathBuf> = Vec::new();
        let fname = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        let dir = path.parent().map(Path::to_path_buf).unwrap_or_default();
        if let Ok(rel) = path.strip_prefix(root.join("_Moments")).or_else(|_| path.strip_prefix(&root)) {
            let rel = rel.to_string_lossy().replace('\\', "/");
            if kind == "folder" { also.push(root.join("_Renders").join(&rel)); } else if kind == "moment" {
                let stem = fname.strip_suffix(index::LINK_EXT).or_else(|| fname.rsplit_once('.').map(|(s, _)| s)).unwrap_or(&fname);
                also.push(dir.join(format!("{stem}.gyroflow")));
                for p in index::RENDER_PROFILES { also.push(index::render_path(&root, &rel, p)); }
            }
        }
        if kind == "still" {
            if let Some(i) = fname.rfind('.') { for e in [".mov", ".MOV"] { also.push(dir.join(format!("{}{e}", &fname[..i]))); } }
        }
        match to_trash(&path) {
            Ok(w) => {
                done += 1;
                went.insert(w);
                gone.push(path.clone());
                for a in also.into_iter().filter(|a| a.exists()) { match to_trash(&a) { Ok(_) => gone.push(a), Err(e) => problems.push(e) } }
            }
            Err(e) => problems.push(e),
        }
    }
    seen::note_now(&gone);
    serde_json::json!({ "done": done, "to": went.into_iter().collect::<Vec<_>>(), "problems": problems })
}

// Everything the library keeps, written out in the open as one JSON file: the project's file and this computer's
// additions, the locations and their pointer files, and for each clip, moment, clip preview, render, photo and export
// what's kept in it or beside it (an MP4's keys, a photo's XMP link, a clip's own file, what the clip itself says).
// Nothing in the library is kept anywhere this doesn't show
pub(crate) fn library_json(root: &Path, probes: &index::SharedProbes) -> Value {
    let mounts = locations::mounts();
    let doc = read_doc(root);
    let name = locations::pointer_name(root);
    let local = doc.get("id").and_then(Value::as_str).and_then(locations::local_file).and_then(|f| locations::read_json(&f).map(|d| json!({ "file": f.display().to_string(), "content": d })));
    let locs: Vec<Value> = locations::locations(root, &mounts).iter().map(|l| json!({
        "name": l.name, "at": l.at, "path": l.path, "copies": l.copies, "this_computer_only": l.local,
        "here": l.dir.as_ref().map(|d| d.display().to_string()),
        "pointer_file": l.dir.as_ref().map(|d| d.join(&name)).filter(|f| f.is_file()).map(|f| json!({ "file": f.display().to_string(), "content": locations::read_json(&f) })),
    })).collect();
    // An MP4's own keys (ch.actionfootageplayer.*), as JSON where they are
    let keys = |p: &str| -> Value {
        let k = index::keys_and_cover(Path::new(p)).0;
        let mut o = serde_json::Map::new();
        let mut names: Vec<&String> = k.keys().filter(|n| n.starts_with("ch.actionfootageplayer")).collect();
        names.sort();
        for n in names { o.insert(n.clone(), serde_json::from_str(&k[n]).unwrap_or_else(|_| json!(k[n]))); }
        Value::Object(o)
    };
    let own_file = |p: &Path| json!({ "file": p.display().to_string(), "content": locations::read_json(p) });
    let flat = doc.get("settings").and_then(|s| s.get("clips_flat")).and_then(Value::as_bool).unwrap_or(false);
    let footage = index::scan(root, "footage", flat);
    let paths: Vec<String> = footage["items"].as_array().into_iter().flatten().filter_map(|it| it["path"].as_str().map(str::to_string)).collect();
    let known = probes.lock().unwrap().known(&paths);
    let clips: Vec<Value> = footage["items"].as_array().into_iter().flatten().map(|it| {
        let p = it["path"].as_str().unwrap_or("");
        json!({
            "tree": it["id"], "copies": it["copies"], "size": it["size"], "modified": it["mtime"],
            "own_files": it["own_files"].as_array().into_iter().flatten().filter_map(Value::as_str).map(|f| own_file(Path::new(f))).collect::<Vec<_>>(),
            "gyroflow_project": it["gyroflow"],
            "preview": it["preview"].as_str().map(|pv| json!({ "file": pv, "keys": keys(pv) })),
            "what_the_clip_says": known["items"].get(p).cloned().unwrap_or(Value::Null),
        })
    }).collect();
    let view = |v: &str| index::scan(root, v, flat)["items"].as_array().cloned().unwrap_or_default();
    let moments: Vec<Value> = view("moments").iter().map(|it| {
        let p = it["path"].as_str().unwrap_or("");
        let rel = it["id"].as_str().unwrap_or("");
        let renders: Vec<Value> = index::RENDER_PROFILES.iter().map(|pr| (pr, index::render_path(root, rel, pr))).filter(|(_, f)| f.is_file())
            .map(|(pr, f)| json!({ "profile": pr, "file": f.display().to_string(), "state": it["renders"][*pr], "keys": keys(&f.to_string_lossy()) })).collect();
        let g = Path::new(p).with_file_name(format!("{}.gyroflow", it["name"].as_str().unwrap_or("")));
        json!({ "file": p, "name": it["name"], "original": it["original"], "keys": keys(p), "renders": renders,
                "gyroflow_project": g.is_file().then(|| g.display().to_string()) })
    }).collect();
    let photo_tags = doc.get("photo_tags").cloned().unwrap_or(json!({}));
    let photos: Vec<Value> = view("stills").iter().map(|it| json!({
        "file": it["path"], "form": it["form"], "link_in_its_xmp": it["link"], "tags_in_its_xmp": it["path"].as_str().map(|p| index::photo_tags(Path::new(p))), "tags_in_the_project_file": photo_tags.get(it["id"].as_str().unwrap_or("")).cloned(),
    })).collect();
    let exports: Vec<Value> = view("exports").iter().map(|it| { let p = it["path"].as_str().unwrap_or(""); json!({ "file": p, "keys": keys(p) }) }).collect();
    json!({
        "format": "actionfootage-library-export", "version": 1,
        "about": "Everything Action Footage Library and ActionFootagePlayer keep about this library, in the open: what's in the files beside the clips, inside the moments, previews, renders, photos and exports, and in the project's own files. The app keeps nothing else.",
        "app": env!("CARGO_PKG_VERSION"),
        "exported_unix_time": std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0),
        "project": { "folder": root.display().to_string(), "file": root.join(locations::PROJECT_FILE).display().to_string(), "content": doc },
        "this_computer": local,
        "locations": locs,
        "clips": clips, "moments": moments, "photos": photos, "exports": exports,
    })
}

#[tauri::command]
pub(crate) async fn export_library(shared: State<'_, Shared>, probes: State<'_, index::SharedProbes>) -> Result<Option<String>, String> {
    let root = shared.lock().unwrap().project.clone().ok_or("Open a project first")?;
    let base = root.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| "Library".into());
    let Some(file) = rfd::AsyncFileDialog::new().set_title("Export the library as JSON").set_file_name(format!("{base} library.json"))
        .add_filter("JSON", &["json"]).save_file().await else { return Ok(None) };
    let file = file.path().to_path_buf();
    let probes = probes.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let v = library_json(&root, &probes);
        std::fs::write(&file, serde_json::to_vec_pretty(&v).unwrap()).map_err(|e| format!("{}: {e}", file.display()))?;
        Ok(Some(file.display().to_string()))
    }).await.map_err(|e| e.to_string())?
}

// A clip's or moment's Gyroflow project opened in Gyroflow (the system's app for .gyroflow): `paths`, the clip's
// copies or the moment's file; its project beside one of them (<name>.gyroflow or <name.ext>.gyroflow)
#[tauri::command]
pub(crate) async fn open_in_gyroflow(paths: Vec<String>) -> Result<(), String> {
    // In the background: the window never waits on it (a share can take seconds)
    tauri::async_runtime::spawn_blocking(move || -> Result<(), String> {
    for p in &paths {
        let p = PathBuf::from(p);
        let name = p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        let mut cands = vec![p.with_extension("gyroflow"), PathBuf::from(format!("{}.gyroflow", p.display()))];
        if let Some(stem) = name.strip_suffix(index::LINK_EXT) { cands.insert(0, p.with_file_name(format!("{stem}.gyroflow"))); }
        if let Some(g) = cands.into_iter().find(|c| c.is_file()) {
            let r = if cfg!(target_os = "macos") { Command::new("open").arg(&g).status() }
                else if cfg!(windows) { Command::new("cmd").args(["/d", "/c", "start", ""]).arg(&g).status() }
                else { Command::new("xdg-open").arg(&g).status() };
            return match r { Ok(s) if s.success() => Ok(()), _ => Err(format!("Couldn't open {}: is Gyroflow installed?", g.display())) };
        }
    }
    Err("It has no Gyroflow project".into())
}).await.map_err(|e| e.to_string())?
}

// A new folder in a view's own folder (_Clips, _Moments, _Stills, _Exports), inside `parent` (its path there): its
// path there, named "untitled folder" (" 2"… when taken), for renaming at once
#[tauri::command]
pub(crate) async fn new_folder(shared: State<'_, Shared>, view: String, parent: String, name: Option<String>) -> Result<String, String> {
    // In the background: the window never waits on it (a share can take seconds)
    let shared = shared.inner().clone();
    tauri::async_runtime::spawn_blocking(move || -> Result<String, String> {
        let root = shared.lock().unwrap().project.clone().ok_or("Open a project first")?;
        new_folder_named(&root, &view, &parent, name.as_deref())
    }).await.map_err(|e| e.to_string())?
}

pub(crate) fn new_folder_in(root: &Path, view: &str, parent: &str) -> Result<String, String> { new_folder_named(root, view, parent, None) }

/// …named as the window already shows it (screen first: it showed the folder before asking), when that name is free
/// and one a file can have; else "untitled folder" as before. Its path in the project's folder is the answer
fn new_folder_named(root: &Path, view: &str, parent: &str, want: Option<&str>) -> Result<String, String> {
    let root = root.to_path_buf();
    // In the project's folder (docs/design/library-management.md §8: its own files are wherever one puts them)
    if !matches!(view, "footage" | "moments" | "stills" | "exports") { return Err("Folders of the sources are made in Finder".into()); }
    let mut dir = root.clone();
    for p in parent.split('/').filter(|p| !p.is_empty() && *p != "..") { dir.push(p); }
    let mut name = "untitled folder".to_string();
    let mut n = 1;
    while dir.join(&name).exists() { n += 1; name = format!("untitled folder {n}"); }
    if let Some(w) = want.and_then(|w| good_name(w).ok()).filter(|w| !dir.join(w).exists()) { name = w; }
    std::fs::create_dir_all(dir.join(&name)).map_err(|e| e.to_string())?;
    // In the catalog at once: the views list folders from it
    seen::note_now(&[dir.join(&name)]);
    Ok(if parent.is_empty() { name } else { format!("{parent}/{name}") })
}
