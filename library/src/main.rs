// SPDX-License-Identifier: GPL-3.0-or-later
//
// Action Footage Library: a project's footage, moments, photos and exports, browsed as folders, searched and filtered
// (index.rs reads them from the project's files), and the commands on what's selected, made in the background: clip
// previews, moments of the marks, photos of the marks, exports of moments. One at a time, in a queue that shows how far
// each is and can stop it. It opens a project, the folder with Project.afproject in it (a double click on that file
// opens it here); what's opened plays in ActionFootagePlayer's own window.
//
// It plays and renders nothing itself: each batch is ActionFootagePlayer run with no window (the workspace plugin's
// batch mode, AFP_BATCH), which makes them as the player does, with its plugins and settings, and writes how far it
// is to a status file this reads. The project's settings for the batches (clip previews in _Clips/ itself, new
// moments stabilized on their own) are in Project.afproject, which the player reads too.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod checks;
mod index;
mod locations;
mod tags;
mod takein;

use serde::Serialize;
use serde_json::{ json, Value };
use std::collections::VecDeque;
use std::path::{ Path, PathBuf };
use std::process::{ Child, Command, Stdio };
use std::sync::{ Arc, Mutex };
use std::time::{ Duration, Instant };
use tauri::{ Manager, RunEvent, State, WindowEvent };

const PROJECT_FILE: &str = "Project.afproject";
const LEGACY_DIR: &str = "_ActionFootagePlayer";
const FORMAT: &str = "actionfootage-project";
// No status from the player this long after it starts: it didn't start the batch
const START_TIMEOUT: Duration = Duration::from_secs(40);
// A batch asked to stop that's still running this long after: stopped for good
const STOP_TIMEOUT: Duration = Duration::from_secs(20);

#[derive(Serialize, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "lowercase")]
enum JobState { Waiting, Starting, Looking, Running, Stopping, Done, Stopped, Failed }

impl JobState {
    fn finished(self) -> bool { matches!(self, JobState::Done | JobState::Stopped | JobState::Failed) }
    fn from_status(s: &str) -> Option<JobState> {
        Some(match s {
            "starting" => JobState::Starting,
            "looking" => JobState::Looking,
            "running" => JobState::Running,
            "stopping" => JobState::Stopping,
            "done" => JobState::Done,
            "stopped" => JobState::Stopped,
            "failed" => JobState::Failed,
            _ => return None,
        })
    }
}

#[derive(Serialize, Clone)]
struct Job {
    id: u64,
    kind: String,
    title: String,
    project: String,
    project_name: String,
    state: JobState,
    done: u64,
    total: u64,
    failed: u64,
    made: Option<u64>,
    fraction: Option<f64>,
    current: Option<String>,
    message: Option<String>,
    failures: Vec<(String, String)>,
    #[serde(skip)]
    request: Value,
}

// The batch running: the player, the files it talks through, and macOS's caffeinate keeping the Mac awake meanwhile
struct Running {
    id: u64,
    child: Child,
    dir: PathBuf,
    started: Instant,
    stop_asked: Option<Instant>,
    awake: Option<Child>,
}

#[derive(Default)]
struct Inner {
    project: Option<PathBuf>,
    jobs: Vec<Job>,
    running: Option<Running>,
    next_id: u64,
    // The window's close asked while batches are queued: the page asks whether to stop them
    close_asked: bool,
    // Projects opened before the page was ready (Finder's double click as the app starts)
    pending_open: VecDeque<PathBuf>,
}

type Shared = Arc<Mutex<Inner>>;

// ---------- this computer's list of projects opened ----------

fn recent_file(app: &tauri::AppHandle) -> Option<PathBuf> {
    app.path().app_config_dir().ok().map(|d| d.join("recent.json"))
}

fn recent_list(app: &tauri::AppHandle) -> Vec<String> {
    recent_file(app).and_then(|f| std::fs::read_to_string(f).ok()).and_then(|t| serde_json::from_str::<Vec<String>>(&t).ok()).unwrap_or_default()
}

fn remember(app: &tauri::AppHandle, root: &Path) {
    let Some(file) = recent_file(app) else { return };
    let p = root.display().to_string();
    let mut list = recent_list(app);
    list.retain(|x| x != &p);
    list.insert(0, p);
    list.truncate(10);
    if let Some(d) = file.parent() { let _ = std::fs::create_dir_all(d); }
    let _ = std::fs::write(file, serde_json::to_vec(&list).unwrap_or_default());
}

// ---------- the project ----------

fn is_project(dir: &Path) -> bool {
    dir.join(PROJECT_FILE).is_file() || dir.join(LEGACY_DIR).is_dir()
}

// A project's folder from what was opened: its Project.afproject, the folder itself, or a folder in it
fn project_of(path: &Path) -> Option<PathBuf> {
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

fn read_doc(root: &Path) -> Value {
    std::fs::read_to_string(root.join(PROJECT_FILE)).ok()
        .and_then(|t| serde_json::from_str::<Value>(&t).ok())
        .filter(|v| v.is_object() && v.get("format").is_none_or(|f| f == FORMAT))
        .unwrap_or_else(|| json!({}))
}

// The doc read fresh, changed and written whole through a temporary file, what this doesn't know kept: the player
// writes it too
fn update_doc(root: &Path, change: impl FnOnce(&mut serde_json::Map<String, Value>)) -> Result<(), String> {
    let file = root.join(PROJECT_FILE);
    if file.is_file() && std::fs::read_to_string(&file).ok().and_then(|t| serde_json::from_str::<Value>(&t).ok())
        .is_some_and(|v| v.get("format").is_some_and(|f| f != FORMAT)) {
        return Err(format!("{} isn't Action Footage's", file.display()));
    }
    let mut doc = read_doc(root);
    let map = doc.as_object_mut().ok_or("not an object")?;
    map.insert("format".into(), json!(FORMAT));
    // 2 once it has locations (docs/library-management.md); the player reads both
    let v = map.get("version").and_then(Value::as_u64).unwrap_or(1).max(1);
    map.insert("version".into(), json!(v));
    if !map.get("settings").is_some_and(Value::is_object) { map.insert("settings".into(), json!({})); }
    change(map);
    let tmp = file.with_extension("afproject.tmp");
    std::fs::write(&tmp, serde_json::to_vec(&doc).map_err(|e| e.to_string())?).map_err(|e| format!("{}: {e}", tmp.display()))?;
    std::fs::rename(&tmp, &file).map_err(|e| format!("{}: {e}", file.display()))
}

fn count_files(dir: &Path, suffixes: &[&str], depth: u32) -> u64 {
    let Ok(entries) = std::fs::read_dir(dir) else { return 0 };
    let mut n = 0;
    for e in entries.flatten() {
        let name = e.file_name().to_string_lossy().to_lowercase();
        if name.starts_with('.') { continue; }
        let path = e.path();
        if path.is_dir() {
            if depth > 0 { n += count_files(&path, suffixes, depth - 1); }
        } else if suffixes.iter().any(|s| name.ends_with(s)) {
            n += 1;
        }
    }
    n
}

#[derive(Serialize)]
struct ProjectInfo {
    path: String,
    name: String,
    // Before 0.9: the player moves the folder into Project.afproject when it next opens a clip of it
    legacy: bool,
    clips_flat: bool,
    moments_alone: bool,
    clips_made: u64,
    clips_failed: Vec<(String, String)>,
    clip_previews: u64,
    moments: u64,
    stills: u64,
    exports: u64,
}

fn project_info(root: &Path) -> ProjectInfo {
    let doc = read_doc(root);
    let settings = doc.get("settings").cloned().unwrap_or(json!({}));
    let flag = |k: &str| settings.get(k).and_then(Value::as_bool).unwrap_or(false);
    let failed = doc.get("clips_failed").and_then(Value::as_array).map(|a| a.iter().filter_map(|p| {
        let p = p.as_array()?;
        Some((p.first()?.as_str()?.to_string(), p.get(1).and_then(Value::as_str).unwrap_or("").to_string()))
    }).collect()).unwrap_or_default();
    ProjectInfo {
        path: root.display().to_string(),
        name: root.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| root.display().to_string()),
        legacy: !root.join(PROJECT_FILE).is_file(),
        clips_flat: flag("clips_flat"),
        moments_alone: flag("moments_alone"),
        clips_made: doc.get("clips_made").and_then(Value::as_array).map_or(0, |a| a.len() as u64),
        clips_failed: failed,
        clip_previews: count_files(&root.join("_Clips"), &[".afplink.mp4"], 8),
        moments: count_files(&root.join("_Moments"), &[".afplink.mp4"], 8),
        stills: count_files(&root.join("_Stills"), &[".heic", ".jpg", ".jpeg"], 2),
        exports: count_files(&root.join("_Exports"), &[".mov", ".mp4"], 2),
    }
}

// ---------- the player, run without a window ----------

// Where ActionFootagePlayer is: AFL_PLAYER (an .app, or its executable), else beside this app, else the usual places
fn player() -> Result<PathBuf, String> {
    if let Some(p) = std::env::var_os("AFL_PLAYER").filter(|p| !p.is_empty()) {
        return Ok(PathBuf::from(p));
    }
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    if cfg!(target_os = "macos") {
        // ActionFootageLibrary.app/Contents/MacOS/ActionFootageLibrary -> the folder the app is in
        let mut places: Vec<PathBuf> = Vec::new();
        if let Some(dir) = exe.ancestors().nth(4) { places.push(dir.join("ActionFootagePlayer.app")); }
        places.push(PathBuf::from("/Applications/ActionFootagePlayer.app"));
        if let Some(home) = std::env::var_os("HOME") { places.push(PathBuf::from(home).join("Applications/ActionFootagePlayer.app")); }
        places.into_iter().find(|p| p.join("Contents/MacOS/ActionFootagePlayer").is_file())
            .ok_or_else(|| "ActionFootagePlayer isn't installed: it's what makes them. Put it in Applications, beside this app".into())
    } else if cfg!(windows) {
        let p = exe.parent().map(|d| d.join("ActionFootagePlayer.exe")).filter(|p| p.is_file());
        p.ok_or_else(|| "ActionFootagePlayer.exe isn't beside this app: install ActionFootagePlayer again".into())
    } else {
        Ok(PathBuf::from("mpv"))
    }
}

fn start_player(request: &Path, log: &Path) -> Result<Child, String> {
    let player = player()?;
    let mut cmd;
    if cfg!(windows) {
        cmd = Command::new(&player);
        cmd.arg("--batch").arg(request);
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
        }
    } else {
        let exe = if player.extension().is_some_and(|e| e == "app") { player.join("Contents/MacOS/ActionFootagePlayer") } else { player.clone() };
        cmd = Command::new(&exe);
        cmd.args(["--idle=yes", "--vo=null", "--ao=null", "--force-window=no", "--terminal=no"])
            .arg(format!("--log-file={}", log.display()));
    }
    cmd.env("AFP_BATCH", request).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null());
    cmd.spawn().map_err(|e| format!("Couldn't start {}: {e}", player.display()))
}

fn jobs_dir() -> PathBuf {
    std::env::temp_dir().join(format!("ActionFootageLibrary-{}", std::process::id()))
}

// The last error line of the player's log, for a batch that ended without saying how
fn log_error(log: &Path) -> Option<String> {
    let text = std::fs::read_to_string(log).ok()?;
    text.lines().rev().find(|l| l.contains("][e][") || l.contains("][f]["))
        .map(|l| l.split("] ").last().unwrap_or(l).trim().to_string())
}

fn start_job(inner: &mut Inner) {
    let Some(job) = inner.jobs.iter_mut().find(|j| j.state == JobState::Waiting) else { return };
    let dir = jobs_dir().join(job.id.to_string());
    let request = dir.join("request.json");
    let log = dir.join("log.txt");
    let mut req = job.request.clone();
    req["status"] = json!(dir.join("status.json"));
    req["stop"] = json!(dir.join("stop"));
    let started = std::fs::create_dir_all(&dir).map_err(|e| e.to_string())
        .and_then(|_| std::fs::write(&request, req.to_string()).map_err(|e| e.to_string()))
        .and_then(|_| start_player(&request, &log));
    match started {
        Ok(child) => {
            job.state = JobState::Starting;
            let awake = if cfg!(target_os = "macos") {
                Command::new("/usr/bin/caffeinate").args(["-i", "-w", &child.id().to_string()])
                    .stdout(Stdio::null()).stderr(Stdio::null()).spawn().ok()
            } else { None };
            inner.running = Some(Running { id: job.id, child, dir, started: Instant::now(), stop_asked: None, awake });
        }
        Err(e) => {
            job.state = JobState::Failed;
            job.message = Some(e);
        }
    }
}

fn read_status(job: &mut Job, dir: &Path) -> bool {
    let Some(st) = std::fs::read_to_string(dir.join("status.json")).ok().and_then(|t| serde_json::from_str::<Value>(&t).ok()) else { return false };
    let num = |k: &str| st.get(k).and_then(Value::as_f64).map(|v| v as u64);
    if let Some(s) = st.get("state").and_then(Value::as_str).and_then(JobState::from_status) { job.state = s; }
    job.done = num("done").unwrap_or(job.done);
    job.total = num("total").unwrap_or(job.total);
    job.failed = num("failed").unwrap_or(job.failed);
    job.made = num("made").or(job.made);
    job.fraction = st.get("fraction").and_then(Value::as_f64);
    job.current = st.get("current").and_then(Value::as_str).map(str::to_string);
    if let Some(m) = st.get("message").and_then(Value::as_str) { job.message = Some(m.to_string()); }
    if let Some(f) = st.get("failures").and_then(Value::as_array) {
        job.failures = f.iter().filter_map(|p| {
            let p = p.as_array()?;
            Some((p.first()?.as_str()?.to_string(), p.get(1).and_then(Value::as_str).unwrap_or("").to_string()))
        }).collect();
    }
    true
}

fn tick(inner: &mut Inner) {
    if let Some(run) = inner.running.as_mut() {
        let id = run.id;
        let exited = run.child.try_wait().ok().flatten();
        let job = inner.jobs.iter_mut().find(|j| j.id == id);
        let mut finished = false;
        if let Some(job) = job {
            let has_status = read_status(job, &run.dir);
            if let Some(status) = exited {
                finished = true;
                if !job.state.finished() {
                    job.state = if run.stop_asked.is_some() { JobState::Stopped } else { JobState::Failed };
                    if job.state == JobState::Failed {
                        job.message = Some(log_error(&run.dir.join("log.txt"))
                            .unwrap_or_else(|| format!("ActionFootagePlayer stopped ({status})")));
                    }
                }
            } else if !has_status && run.started.elapsed() > START_TIMEOUT {
                let _ = run.child.kill();
                job.state = JobState::Failed;
                job.message = Some("ActionFootagePlayer didn't start the batch".into());
            } else if run.stop_asked.is_some_and(|t| t.elapsed() > STOP_TIMEOUT) {
                let _ = run.child.kill();
            }
        } else {
            finished = exited.is_some();
        }
        if finished {
            if let Some(mut run) = inner.running.take() {
                if let Some(mut a) = run.awake.take() { let _ = a.kill(); let _ = a.wait(); }
                let _ = run.child.wait();
            }
        }
    }
    if inner.running.is_none() { start_job(inner); }
}

fn stop_all(inner: &mut Inner) {
    inner.jobs.retain(|j| j.state != JobState::Waiting);
    if let Some(mut run) = inner.running.take() {
        let _ = std::fs::write(run.dir.join("stop"), b"");
        // Its exports' ffmpegs are stopped by the player: give it a moment, then end it
        for _ in 0..30 {
            if run.child.try_wait().ok().flatten().is_some() { break; }
            std::thread::sleep(Duration::from_millis(200));
        }
        let _ = run.child.kill();
        let _ = run.child.wait();
        if let Some(mut a) = run.awake.take() { let _ = a.kill(); }
    }
}

// ---------- the page's commands ----------

#[derive(Serialize)]
struct Snapshot {
    project: Option<String>,
    jobs: Vec<Job>,
    close_asked: bool,
    macos: bool,
}

#[tauri::command]
fn snapshot(shared: State<Shared>) -> Snapshot {
    let mut inner = shared.lock().unwrap();
    if inner.project.is_none() {
        if let Some(p) = inner.pending_open.pop_front() { inner.project = Some(p); }
    }
    Snapshot {
        project: inner.project.as_ref().map(|p| p.display().to_string()),
        jobs: inner.jobs.clone(),
        close_asked: inner.close_asked,
        macos: cfg!(target_os = "macos"),
    }
}

#[tauri::command]
fn project(shared: State<Shared>) -> Option<ProjectInfo> {
    let root = shared.lock().unwrap().project.clone()?;
    Some(project_info(&root))
}

#[tauri::command]
fn engine() -> Result<String, String> {
    player().map(|p| p.display().to_string())
}

fn open_path(app: &tauri::AppHandle, shared: &Shared, path: &Path) -> Result<String, String> {
    let root = project_of(path).ok_or_else(|| format!(
        "{} isn't in a project. A project is a trip folder with {PROJECT_FILE} in it: open a clip of it in \
         ActionFootagePlayer and initialize the project there (the side panel's Project tab)", path.display()))?;
    shared.lock().unwrap().project = Some(root.clone());
    remember(app, &root);
    Ok(root.display().to_string())
}

#[tauri::command]
fn open_project(app: tauri::AppHandle, shared: State<Shared>, path: String) -> Result<String, String> {
    open_path(&app, &shared, Path::new(&path))
}

#[derive(Serialize)]
struct Recent { path: String, name: String, here: bool }

// The projects this computer has opened, the last first: the ones still there
#[tauri::command]
fn recent(app: tauri::AppHandle) -> Vec<Recent> {
    recent_list(&app).into_iter().filter(|p| Path::new(p).join(PROJECT_FILE).is_file() || Path::new(p).join(LEGACY_DIR).is_dir())
        .map(|p| Recent { name: Path::new(&p).file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| p.clone()), here: true, path: p })
        .collect()
}

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
async fn cached_view(app: tauri::AppHandle, shared: State<'_, Shared>, probes: State<'_, index::SharedProbes>, view: String) -> Result<Value, String> {
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
async fn scan(app: tauri::AppHandle, shared: State<'_, Shared>, probes: State<'_, index::SharedProbes>, view: String) -> Result<Value, String> {
    let root = shared.lock().unwrap().project.clone().ok_or("Open a project first")?;
    let cache_file = view_cache(&app, &root, &view);
    let flat = read_doc(&root).get("settings").and_then(|s| s.get("clips_flat")).and_then(Value::as_bool).unwrap_or(false);
    let probes = probes.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let out = index::scan(&root, &view, flat);
        let mut want: Vec<String> = Vec::new();
        for it in out["items"].as_array().into_iter().flatten() {
            match it["kind"].as_str() {
                Some("clip") => if let Some(p) = it["path"].as_str() { want.push(p.to_string()); },
                // A moment's original: what it is (its camera, its color) is the clip's
                Some("moment") => if let Some(o) = it["original"].as_str() { want.push(o.to_string()); },
                _ => {}
            }
        }
        let known = { let mut p = probes.lock().unwrap(); p.want(&want); p.known(&want) };
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

#[tauri::command]
fn probes_since(probes: State<index::SharedProbes>, since: u64) -> Value {
    probes.lock().unwrap().since(since)
}

#[tauri::command]
async fn thumb(app: tauri::AppHandle, path: String) -> Result<Option<String>, String> {
    let cache = app.path().app_cache_dir().map_err(|e| e.to_string())?.join("thumbs");
    tauri::async_runtime::spawn_blocking(move || index::thumb(Path::new(&path), &cache)).await.map_err(|e| e.to_string())
}

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
fn project_id(root: &Path) -> Result<String, String> {
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
async fn project_settings(app: tauri::AppHandle, shared: State<'_, Shared>) -> Result<Value, String> {
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
                   "next_to_footage": doc.get("locations").and_then(Value::as_array).is_none_or(|a| a.is_empty()) }))
    }).await.map_err(|e| e.to_string())?
}

// The project's locations written: into Project.afproject (none: the project is next to its footage again), and its
// .afplocation file into each of them (taken out of the ones removed)
#[tauri::command]
async fn save_locations(shared: State<'_, Shared>, list: Vec<Value>, hosts: Option<Value>) -> Result<Vec<String>, String> {
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
            if clean.is_empty() { d.remove("locations"); } else { d.insert("locations".into(), json!(clean)); d.insert("version".into(), json!(2)); }
            if let Some(h) = &hosts { if h.as_object().is_some_and(|o| !o.is_empty()) { d.insert("hosts".into(), h.clone()); } else { d.remove("hosts"); } }
        })?;
        let after: Vec<locations::Location> = locations::locations(&root, &mounts).into_iter().filter(|l| !l.local).collect();
        Ok(locations::write_pointers(&root, &after, &before, &mounts))
    }).await.map_err(|e| e.to_string())?
}

// A folder chosen for a location: where it is here and how the project keeps it (network path, or relative)
#[tauri::command]
fn choose_location(shared: State<Shared>) -> Result<Option<Value>, String> {
    let root = shared.lock().unwrap().project.clone().ok_or("Open a project first")?;
    let Some(dir) = rfd::FileDialog::new().set_title("A folder of the project's footage").pick_folder() else { return Ok(None) };
    let mounts = locations::mounts();
    Ok(Some(json!({ "dir": dir.display().to_string(), "path": locations::path_for(&dir, &root, &mounts),
                    "name": dir.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default() })))
}

// This computer's own locations for the project (copies on its own disk), kept on it, by the project's id
#[tauri::command]
fn choose_local(shared: State<Shared>) -> Result<Option<String>, String> {
    shared.lock().unwrap().project.clone().ok_or("Open a project first")?;
    Ok(rfd::FileDialog::new().set_title("A folder on this computer with copies of the project's clips").pick_folder().map(|d| d.display().to_string()))
}

#[tauri::command]
fn save_local(shared: State<Shared>, list: Vec<Value>) -> Result<(), String> {
    let root = shared.lock().unwrap().project.clone().ok_or("Open a project first")?;
    let id = project_id(&root)?;
    let file = locations::local_file(&id).ok_or("No place for this computer's files")?;
    let clean: Vec<Value> = list.iter().filter_map(|l| {
        let p = l.get("path").and_then(Value::as_str)?.trim();
        (!p.is_empty()).then(|| json!({ "name": l.get("name").and_then(Value::as_str).unwrap_or("On this computer"), "at": l.get("at").and_then(Value::as_str).unwrap_or("").trim_matches('/'), "path": p }))
    }).collect();
    if let Some(d) = file.parent() { std::fs::create_dir_all(d).map_err(|e| e.to_string())?; }
    if clean.is_empty() { let _ = std::fs::remove_file(&file); return Ok(()); }
    std::fs::write(&file, serde_json::to_vec_pretty(&json!({ "project": root.display().to_string(), "locations": clean })).unwrap()).map_err(|e| e.to_string())
}

#[tauri::command]
fn clear_cache(app: tauri::AppHandle, key: String) -> Result<(), String> {
    let (_, _, dir) = caches(&app).into_iter().find(|(k, _, _)| *k == key).ok_or("no such cache")?;
    if dir.is_dir() { std::fs::remove_dir_all(&dir).map_err(|e| e.to_string())?; }
    Ok(())
}

// The project's own files moved to another folder (the SSD, say): Project.afproject and its _ folders. The clips and
// their own files stay where they are: the old folder becomes a location at the top, unless it is one already; the
// locations given relative to the project are written again for its new place
// The project's own files moved from `root` to `dest`: Project.afproject and its _ folders. The clips and their own
// files stay: the old folder becomes a location at the top unless the project had locations, whose paths relative to
// the project are written again for its new place. Problems writing the pointers are returned
fn move_to(root: &Path, dest: &Path) -> Result<Vec<String>, String> {
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
    for name in [PROJECT_FILE, "_Moments", "_Clips", "_Stills", "_Exports", "_Renders", "_Cache"] {
        let from = root.join(name);
        if !from.exists() { continue; }
        let to = dest.join(name);
        if std::fs::rename(&from, &to).is_err() {
            // Another disk: copied, then the old one removed
            let ok = Command::new(if cfg!(windows) { "xcopy" } else { "/bin/cp" }).args(if cfg!(windows) { vec!["/E", "/I", "/H", "/K"] } else { vec!["-Rp"] })
                .arg(&from).arg(&to).status().map(|s| s.success()).unwrap_or(false);
            if !ok { return Err(format!("couldn't move {} (moved so far: {})", from.display(), moved.join(", "))); }
            let _ = if from.is_dir() { std::fs::remove_dir_all(&from) } else { std::fs::remove_file(&from) };
        }
        moved.push(name.to_string());
    }
    update_doc(dest, |d| { d.insert("locations".into(), json!(list)); d.insert("version".into(), json!(2)); })?;
    let after: Vec<locations::Location> = locations::locations(dest, &mounts).into_iter().filter(|l| !l.local).collect();
    // The pointers of before (the old folder's name) out, the new ones in
    for l in &all {
        if let Some(d) = &l.dir { if locations::pointer_name(root) != locations::pointer_name(dest) { let _ = std::fs::remove_file(d.join(locations::pointer_name(root))); } }
    }
    Ok(locations::write_pointers(dest, &after, &[], &mounts))
}

#[tauri::command]
fn move_project(app: tauri::AppHandle, shared: State<Shared>) -> Result<Option<String>, String> {
    let root = shared.lock().unwrap().project.clone().ok_or("Open a project first")?;
    if busy(&shared) { return Err("Jobs are in the queue: move the project when they're done".into()); }
    let Some(dest) = rfd::FileDialog::new().set_title("Move the project's own files to this folder").pick_folder() else { return Ok(None) };
    let problems = move_to(&root, &dest)?;
    shared.lock().unwrap().project = Some(dest.clone());
    remember(&app, &dest);
    Ok(Some(if problems.is_empty() { dest.display().to_string() } else { format!("{} ({})", dest.display(), problems.join("; ")) }))
}

// ---------- Check, Find moved files ----------

// The player's afp-cut, which writes a key of an MP4 again in place, the file's other keys kept (--set-key)
fn link_tool() -> Option<PathBuf> {
    let player = player().ok()?;
    let probe = index::probe_tool(Some(&player))?;
    let name = if cfg!(windows) { "afp-cut.exe" } else { "afp-cut" };
    Some(probe.with_file_name(name)).filter(|p| p.is_file())
}

// A moment's, clip preview's or export's link written again (its original moved, tags changed)
fn write_link(tool: Option<&PathBuf>, file: &Path, link: &str) -> Result<(), String> {
    let tool = tool.ok_or("ActionFootagePlayer's afp-cut isn't there to write the links")?;
    let o = Command::new(tool).arg("--set-key").arg(file).arg("ch.actionfootageplayer.link").arg(link).output().map_err(|e| e.to_string())?;
    if o.status.success() { Ok(()) } else { Err(String::from_utf8_lossy(&o.stderr).trim().to_string()) }
}

// Check, on the selection: `sel` paths of the footage tree, `links` link files (moments, exports) by their path in the
// project
#[tauri::command]
async fn run_check(shared: State<'_, Shared>, sel: Vec<String>, links: Vec<String>) -> Result<Value, String> {
    let root = shared.lock().unwrap().project.clone().ok_or("Open a project first")?;
    tauri::async_runtime::spawn_blocking(move || checks::check(&root, &sel, &links)).await.map_err(|e| e.to_string())
}

#[tauri::command]
async fn find_moved(shared: State<'_, Shared>, issues: Vec<Value>) -> Result<Value, String> {
    let root = shared.lock().unwrap().project.clone().ok_or("Open a project first")?;
    tauri::async_runtime::spawn_blocking(move || checks::find_moved(&root, &issues)).await.map_err(|e| e.to_string())
}

#[tauri::command]
async fn apply_moved(shared: State<'_, Shared>, found: Vec<Value>) -> Result<Vec<String>, String> {
    let root = shared.lock().unwrap().project.clone().ok_or("Open a project first")?;
    if busy(&shared) { return Err("Jobs are in the queue: fix the paths when they're done".into()); }
    let tool = link_tool();
    tauri::async_runtime::spawn_blocking(move || {
        let relink = |file: &Path, link: &str| write_link(tool.as_ref(), file, link);
        checks::apply(&root, &found, &relink)
    }).await.map_err(|e| e.to_string())
}

// ---------- renders ----------

#[tauri::command]
fn delete_renders(shared: State<Shared>, moments: Vec<String>, stale_only: bool) -> Result<Value, String> {
    let root = shared.lock().unwrap().project.clone().ok_or("Open a project first")?;
    if busy(&shared) { return Err("Jobs are in the queue: delete renders when they're done".into()); }
    let (n, problems) = index::delete_renders(&root, &moments, stale_only);
    Ok(json!({ "deleted": n, "problems": problems }))
}

// ---------- tags ----------

#[tauri::command]
async fn set_tags(shared: State<'_, Shared>, items: Vec<Value>, add: Vec<String>, remove: Vec<String>) -> Result<Vec<String>, String> {
    let root = shared.lock().unwrap().project.clone().ok_or("Open a project first")?;
    let tool = link_tool();
    tauri::async_runtime::spawn_blocking(move || tags::set(&root, &items, &add, &remove, &|f, l| write_link(tool.as_ref(), f, l)))
        .await.map_err(|e| e.to_string())
}

// ---------- Take in another project ----------

#[tauri::command]
fn choose_other(shared: State<Shared>) -> Result<Option<String>, String> {
    shared.lock().unwrap().project.clone().ok_or("Open a project first")?;
    Ok(rfd::FileDialog::new().set_title("The project to take in: its Project.afproject").add_filter("Action Footage project", &["afproject"])
        .pick_file().and_then(|f| f.parent().map(|d| d.display().to_string())))
}

#[tauri::command]
async fn take_in_plan(shared: State<'_, Shared>, other: String, place: String) -> Result<Value, String> {
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
async fn take_in_run(shared: State<'_, Shared>, other: String, place: String) -> Result<Value, String> {
    let root = shared.lock().unwrap().project.clone().ok_or("Open a project first")?;
    if busy(&shared) { return Err("Jobs are in the queue: take it in when they're done".into()); }
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
fn quick_look(paths: Vec<String>) -> Result<bool, String> {
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
fn rename_item(shared: State<Shared>, view: String, kind: String, path: String, name: String) -> Result<String, String> {
    let root = shared.lock().unwrap().project.clone().ok_or("Open a project first")?;
    if busy(&shared) { return Err("Jobs are in the queue: rename when they're done".into()); }
    let name = good_name(&name)?;
    let from = PathBuf::from(&path);
    if !from.starts_with(&root) || !["_Moments", "_Clips", "_Stills", "_Exports"].iter().any(|d| from.starts_with(root.join(d))) {
        return Err("Footage is renamed in Finder (Find moved files then updates what points at it). Here: moments, clip previews, photos, exports and their folders".into());
    }
    let dir = from.parent().ok_or("no folder")?.to_path_buf();
    let fname = from.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let mv = |a: &Path, b: &Path| -> Result<(), String> {
        if b.exists() { return Err(format!("{} exists already", b.display())); }
        std::fs::rename(a, b).map_err(|e| format!("{}: {e}", a.display()))
    };
    if kind == "folder" {
        let to = dir.join(&name);
        mv(&from, &to)?;
        // The moments' renders follow their folder
        if let Ok(rel) = from.strip_prefix(root.join("_Moments")) {
            let r = root.join("_Renders").join(rel);
            if r.is_dir() { let _ = std::fs::rename(&r, r.with_file_name(&name)); }
        }
        return Ok(to.display().to_string());
    }
    match kind.as_str() {
        "moment" => {
            let old = fname.strip_suffix(index::LINK_EXT).unwrap_or(&fname).to_string();
            let to = dir.join(format!("{name}{}", index::LINK_EXT));
            mv(&from, &to)?;
            let g = dir.join(format!("{old}.gyroflow"));
            if g.is_file() { let _ = std::fs::rename(&g, dir.join(format!("{name}.gyroflow"))); }
            if let Ok(rel) = from.strip_prefix(root.join("_Moments")) {
                let rel = rel.to_string_lossy().replace('\\', "/");
                for p in index::RENDER_PROFILES {
                    let r = index::render_path(&root, &rel, p);
                    if r.is_file() { let _ = std::fs::rename(&r, r.with_file_name(format!("{name}.{p}.mov"))); }
                }
            }
            Ok(to.display().to_string())
        }
        _ => {
            // A photo or an export: its extension kept (a Motion Photo's .MP.jpg whole), a Live Photo's video along
            let lower = fname.to_lowercase();
            let ext = if lower.ends_with(".mp.jpg") { fname[fname.len() - 7..].to_string() } else { fname.rfind('.').map(|i| fname[i..].to_string()).unwrap_or_default() };
            let stem = &fname[..fname.len() - ext.len()];
            let to = dir.join(format!("{name}{ext}"));
            mv(&from, &to)?;
            if view == "stills" {
                for e in [".mov", ".MOV"] {
                    let v = dir.join(format!("{stem}{e}"));
                    if v.is_file() { let _ = std::fs::rename(&v, dir.join(format!("{name}{e}"))); }
                }
            }
            Ok(to.display().to_string())
        }
    }
}

// A clip's or moment's Gyroflow project opened in Gyroflow (the system's app for .gyroflow): `paths`, the clip's
// copies or the moment's file; its project beside one of them (<name>.gyroflow or <name.ext>.gyroflow)
#[tauri::command]
fn open_in_gyroflow(paths: Vec<String>) -> Result<(), String> {
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
}

// A new folder in a view's own folder (_Clips, _Moments, _Stills, _Exports), inside `parent` (its path there): its
// path there, named "untitled folder" (" 2"… when taken), for renaming at once
#[tauri::command]
fn new_folder(shared: State<Shared>, view: String, parent: String) -> Result<String, String> {
    let root = shared.lock().unwrap().project.clone().ok_or("Open a project first")?;
    let base = match view.as_str() { "footage" => "_Clips", "moments" => "_Moments", "stills" => "_Stills", "exports" => "_Exports",
                                     _ => return Err("Folders of the sources are made in Finder".into()) };
    let mut dir = root.join(base);
    for p in parent.split('/').filter(|p| !p.is_empty() && *p != "..") { dir.push(p); }
    let mut name = "untitled folder".to_string();
    let mut n = 1;
    while dir.join(&name).exists() { n += 1; name = format!("untitled folder {n}"); }
    std::fs::create_dir_all(dir.join(&name)).map_err(|e| e.to_string())?;
    Ok(if parent.is_empty() { name } else { format!("{parent}/{name}") })
}

// What's selected, opened: videos and moments in the player (one window, as a playlist), photos as the system opens them
#[tauri::command]
fn open_items(paths: Vec<String>) -> Result<(), String> {
    if paths.is_empty() { return Ok(()); }
    let (photos, videos): (Vec<&String>, Vec<&String>) = paths.iter().partition(|p| {
        let l = p.to_lowercase();
        [".heic", ".jpg", ".jpeg", ".png"].iter().any(|e| l.ends_with(e))
    });
    let player = player()?;
    if !videos.is_empty() {
        if cfg!(target_os = "macos") {
            Command::new("open").arg("-a").arg(&player).args(&videos).spawn().map_err(|e| e.to_string())?;
        } else {
            Command::new(&player).args(&videos).spawn().map_err(|e| e.to_string())?;
        }
    }
    for p in photos {
        let _ = if cfg!(target_os = "macos") { Command::new("open").arg(p).spawn() }
            else if cfg!(windows) { Command::new("explorer").arg(p).spawn() }
            else { Command::new("xdg-open").arg(p).spawn() };
    }
    Ok(())
}

#[tauri::command]
fn reveal_path(path: String) {
    let p = PathBuf::from(&path);
    let _ = if cfg!(target_os = "macos") { Command::new("open").arg("-R").arg(&p).spawn() }
        else if cfg!(windows) { Command::new("explorer").arg(format!("/select,{}", p.display())).spawn() }
        else { Command::new("xdg-open").arg(p.parent().unwrap_or(&p)).spawn() };
}

// A file dialog for Project.afproject (a synchronous command: it runs on the main thread, as the dialog must)
#[tauri::command]
fn choose_project(app: tauri::AppHandle, shared: State<Shared>) -> Result<Option<String>, String> {
    let picked = rfd::FileDialog::new().set_title("Open a project: its Project.afproject")
        .add_filter("Action Footage project", &["afproject"]).pick_file();
    match picked {
        Some(p) => open_path(&app, &shared, &p).map(Some),
        None => Ok(None),
    }
}

const KINDS: [&str; 6] = ["clips", "clips-again", "moments", "photos", "export", "render"];

#[tauri::command]
fn add_job(shared: State<Shared>, kind: String, options: Option<Value>, only: Option<Vec<String>>, label: Option<String>) -> Result<(), String> {
    if !KINDS.contains(&kind.as_str()) { return Err(format!("no such batch: {kind}")); }
    let mut inner = shared.lock().unwrap();
    let root = inner.project.clone().ok_or("Open a project first")?;
    let options = options.unwrap_or(json!({}));
    let photo = options.get("photo").and_then(Value::as_str).unwrap_or("no").to_string();
    let sdr = options.get("sdr").and_then(Value::as_bool).unwrap_or(false);
    let title = match kind.as_str() {
        "clips" => "Clip previews".to_string(),
        "clips-again" => "Clip previews of the removed clips".to_string(),
        "moments" => "Moments of all marks".to_string(),
        "photos" => format!("Photos of all marks ({})", match photo.as_str() { "live" => "Live Photos", "motion" => "Motion Photos", _ => "stills" }),
        "render" => format!("Render ({})", options.get("profiles").and_then(Value::as_array).map(|a| a.iter().filter_map(Value::as_str).collect::<Vec<_>>().join(", ")).unwrap_or_else(|| "HDR".into())),
        _ => format!("Export all moments ({})", if sdr { "SDR" } else { "HDR" }),
    };
    // Kept to what's selected: "Clip previews · 3 clips"
    let only = only.filter(|o| !o.is_empty());
    let title = match (&only, label) {
        (Some(_), Some(l)) => format!("{} · {l}", title.replace(" of all marks", " of the marks").replace("Export all moments", "Export")),
        _ => title,
    };
    let project = root.display().to_string();
    if inner.jobs.iter().any(|j| !j.state.finished() && j.project == project && j.title == title) {
        return Err(format!("{title} is in the queue already"));
    }
    let mut request = json!({ "kind": kind, "project": project });
    if let Some(o) = &only { request["only"] = json!(o); }
    if kind == "photos" { request["photo"] = json!(photo); }
    if kind == "export" { request["sdr"] = json!(sdr); }
    if kind == "render" {
        let profiles: Vec<String> = options.get("profiles").and_then(Value::as_array).into_iter().flatten().filter_map(Value::as_str)
            .filter(|p| index::RENDER_PROFILES.contains(p)).map(str::to_string).collect();
        if profiles.is_empty() { return Err("Tick a profile".into()); }
        request["profiles"] = json!(profiles);
    }
    inner.next_id += 1;
    let job = Job {
        id: inner.next_id, kind, title, project_name: root.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default(),
        project, state: JobState::Waiting, done: 0, total: 0, failed: 0, made: None, fraction: None, current: None, message: None,
        failures: Vec::new(), request,
    };
    inner.jobs.push(job);
    tick(&mut inner);
    Ok(())
}

#[tauri::command]
fn cancel_job(shared: State<Shared>, id: u64) {
    let mut inner = shared.lock().unwrap();
    if inner.running.as_ref().is_some_and(|r| r.id == id) {
        let run = inner.running.as_mut().unwrap();
        if run.stop_asked.is_none() {
            run.stop_asked = Some(Instant::now());
            let _ = std::fs::write(run.dir.join("stop"), b"");
        }
        if let Some(j) = inner.jobs.iter_mut().find(|j| j.id == id) { j.state = JobState::Stopping; }
    } else {
        inner.jobs.retain(|j| !(j.id == id && j.state == JobState::Waiting));
    }
}

#[tauri::command]
fn clear_finished(shared: State<Shared>) {
    shared.lock().unwrap().jobs.retain(|j| !j.state.finished());
}

#[tauri::command]
fn set_setting(shared: State<Shared>, key: String, value: bool) -> Result<(), String> {
    if key != "clips_flat" && key != "moments_alone" { return Err(format!("no such setting: {key}")); }
    let root = shared.lock().unwrap().project.clone().ok_or("Open a project first")?;
    update_doc(&root, |doc| { doc["settings"][&key] = json!(value); })
}

#[tauri::command]
fn reveal(shared: State<Shared>, what: Option<String>) {
    let Some(root) = shared.lock().unwrap().project.clone() else { return };
    let path = match what.as_deref() {
        Some(sub @ ("_Clips" | "_Moments" | "_Stills" | "_Exports")) if root.join(sub).is_dir() => root.join(sub),
        _ => root,
    };
    let _ = if cfg!(target_os = "macos") { Command::new("open").arg(&path).spawn() }
        else if cfg!(windows) { Command::new("explorer").arg(&path).spawn() }
        else { Command::new("xdg-open").arg(&path).spawn() };
}

// The window's close, answered: stop the batches and quit, or keep them going (the window stays)
#[tauri::command]
fn answer_close(app: tauri::AppHandle, shared: State<Shared>, quit: bool) {
    let mut inner = shared.lock().unwrap();
    inner.close_asked = false;
    if quit {
        stop_all(&mut inner);
        drop(inner);
        app.exit(0);
    }
}

fn busy(shared: &Shared) -> bool {
    shared.lock().unwrap().jobs.iter().any(|j| !j.state.finished())
}

fn opened(shared: &Shared, app: &tauri::AppHandle, path: PathBuf) {
    match project_of(&path) {
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

fn main() {
    let shared: Shared = Arc::new(Mutex::new(Inner::default()));
    // A project given on the command line (Explorer's double click)
    for arg in std::env::args_os().skip(1) {
        let p = PathBuf::from(&arg);
        if !arg.to_string_lossy().starts_with('-') {
            if let Some(root) = project_of(&p) { shared.lock().unwrap().project = Some(root); }
        }
    }
    let probes: index::SharedProbes = Arc::new(Mutex::new(index::Probes::default()));
    probes.lock().unwrap().tool = index::probe_tool(player().ok().as_deref());
    let probe_worker = probes.clone();
    // What's known of the clips is kept between runs: the app's cache folder, found once Tauri is up (setup below)
    let keep_probes: Arc<Mutex<Option<PathBuf>>> = Arc::new(Mutex::new(None));
    let keep_for_worker = keep_probes.clone();
    std::thread::spawn(move || {
        let keep = loop {
            if let Some(k) = keep_for_worker.lock().unwrap().clone() { break k; }
            std::thread::sleep(Duration::from_millis(100));
        };
        index::probe_worker(probe_worker, Some(keep))
    });
    let probes_for_setup = probes.clone();
    let worker = shared.clone();
    std::thread::spawn(move || loop {
        std::thread::sleep(Duration::from_millis(300));
        tick(&mut worker.lock().unwrap());
    });

    let for_instance = shared.clone();
    let for_window = shared.clone();
    let for_run = shared.clone();
    let app = tauri::Builder::default()
        // A second one started (a project double-clicked in Explorer while this runs): its project opens here
        .plugin(tauri_plugin_single_instance::init(move |app, argv, _cwd| {
            if let Some(p) = argv.iter().skip(1).find(|a| !a.starts_with('-')) {
                opened(&for_instance, app, PathBuf::from(p));
            }
        }))
        .manage(shared)
        .manage(probes)
        .setup(move |app| {
            if let Ok(c) = app.path().app_cache_dir() {
                let f = c.join("probes.json");
                probes_for_setup.lock().unwrap().load(&f);
                *keep_probes.lock().unwrap() = Some(f);
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![snapshot, project, engine, open_project, choose_project, add_job, cancel_job,
                                                 clear_finished, set_setting, reveal, answer_close, recent, scan, probes_since,
                                                 thumb, open_items, reveal_path, cached_view, quick_look, rename_item, open_in_gyroflow, new_folder, project_settings, save_locations, choose_location,
                                                 choose_local, save_local, clear_cache, move_project, run_check, find_moved, apply_moved,
                                                 choose_other, take_in_plan, take_in_run, set_tags, delete_renders])
        .on_window_event(move |window, event| {
            // Closed while batches are queued: the page asks first
            if let WindowEvent::CloseRequested { api, .. } = event {
                if busy(&for_window) {
                    api.prevent_close();
                    for_window.lock().unwrap().close_asked = true;
                    let _ = window.set_focus();
                }
            }
        })
        .build(tauri::generate_context!())
        .expect("couldn't start Action Footage Library");
    app.run(move |app, event| {
        match event {
            // Finder's double click on a Project.afproject (and files dropped on the app's icon)
            #[cfg(target_os = "macos")]
            RunEvent::Opened { urls } => {
                for url in urls {
                    if let Ok(path) = url.to_file_path() { opened(&for_run, app, path); }
                }
            }
            RunEvent::Exit => stop_all(&mut for_run.lock().unwrap()),
            _ => {}
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("afl-test-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn finds_the_project() {
        let root = temp("find");
        std::fs::create_dir_all(root.join("Day 1")).unwrap();
        std::fs::write(root.join("Day 1/A.MP4"), b"").unwrap();
        assert_eq!(project_of(&root), None);
        std::fs::write(root.join(PROJECT_FILE), b"{}").unwrap();
        assert_eq!(project_of(&root.join(PROJECT_FILE)), Some(root.clone()));
        assert_eq!(project_of(&root), Some(root.clone()));
        assert_eq!(project_of(&root.join("Day 1")), Some(root.clone()));
        assert_eq!(project_of(&root.join("Day 1/A.MP4")), Some(root.clone()));
        let old = temp("legacy");
        std::fs::create_dir_all(old.join(LEGACY_DIR)).unwrap();
        assert_eq!(project_of(&old), Some(old.clone()));
    }

    #[test]
    fn settings_keep_the_rest() {
        let root = temp("settings");
        // As the player's Lua writes it: an empty table is a JSON array
        std::fs::write(root.join(PROJECT_FILE), br#"{"format":"actionfootage-project","version":1,"settings":[],"clips_made":["a\t1"],"mine":2}"#).unwrap();
        update_doc(&root, |d| { d["settings"]["clips_flat"] = json!(true); }).unwrap();
        let doc = read_doc(&root);
        assert_eq!(doc["settings"]["clips_flat"], json!(true));
        assert_eq!(doc["clips_made"][0], json!("a\t1"));
        assert_eq!(doc["mine"], json!(2));
        let info = project_info(&root);
        assert!(info.clips_flat && !info.moments_alone && !info.legacy);
        assert_eq!(info.clips_made, 1);
        // Someone else's file of that name is left alone
        std::fs::write(root.join(PROJECT_FILE), br#"{"format":"other"}"#).unwrap();
        assert!(update_doc(&root, |_| {}).is_err());
    }

    #[test]
    fn moving_the_project_to_the_ssd() {
        let top = temp("move");
        let trip = top.join("HDD/Trip");
        std::fs::create_dir_all(trip.join("Day 1")).unwrap();
        std::fs::create_dir_all(trip.join("_Moments")).unwrap();
        std::fs::write(trip.join("Day 1/A.MP4"), b"x").unwrap();
        std::fs::write(trip.join("Day 1/A.afp.json"), b"{}").unwrap();
        std::fs::write(trip.join("_Moments/M.afplink.mp4"), b"x").unwrap();
        std::fs::write(trip.join(PROJECT_FILE), br#"{"format":"actionfootage-project","version":1,"settings":{"clips_flat":true}}"#).unwrap();
        let dest = top.join("SSD/Projects/Trip");
        assert!(move_to(&trip, &dest).unwrap().is_empty());
        // The project's own files moved, the clip and its own file stayed
        assert!(dest.join(PROJECT_FILE).is_file() && dest.join("_Moments/M.afplink.mp4").is_file());
        assert!(!trip.join(PROJECT_FILE).exists() && trip.join("Day 1/A.afp.json").is_file());
        let doc = read_doc(&dest);
        assert_eq!(doc["settings"]["clips_flat"], json!(true));
        assert_eq!(doc["locations"][0]["path"], json!("../../../HDD/Trip"));
        assert!(doc["id"].as_str().is_some_and(|i| i.len() == 36));
        // The old folder points at the project
        let ptr = locations::read_json(&trip.join("Trip.afplocation")).unwrap();
        assert_eq!(locations::place(ptr["project"].as_str().unwrap(), &trip, None, &[]), Some(dest.clone()));
        let v = index::scan(&dest, "footage", false);
        assert_eq!(v["items"][0]["id"], json!("Day 1/A.MP4"));
        // Moved again: its locations follow
        let dest2 = top.join("SSD/Other/Trip");
        move_to(&dest, &dest2).unwrap();
        assert_eq!(read_doc(&dest2)["locations"][0]["path"], json!("../../../HDD/Trip"));
        assert!(move_to(&dest2, &dest2.join("x")).is_err());
    }

    // The whole way, with the player itself: AFL_PLAYER (an ActionFootagePlayer.app) and AFL_TEST_TRIP (a copy of a
    // trip with Project.afproject in it). Without them it's skipped
    #[test]
    fn a_batch_with_the_player() {
        let (Some(_), Some(trip)) = (std::env::var_os("AFL_PLAYER"), std::env::var_os("AFL_TEST_TRIP")) else { return };
        let shared: Shared = Arc::new(Mutex::new(Inner::default()));
        shared.lock().unwrap().project = Some(PathBuf::from(&trip));
        let mut inner = shared.lock().unwrap();
        inner.next_id = 1;
        inner.jobs.push(Job {
            id: 1, kind: "clips-again".into(), title: "t".into(), project: trip.to_string_lossy().into(), project_name: "trip".into(),
            state: JobState::Waiting, done: 0, total: 0, failed: 0, made: None, fraction: None, current: None, message: None,
            failures: vec![], request: json!({ "kind": "clips-again", "project": trip.to_string_lossy() }),
        });
        let t0 = Instant::now();
        // Done once the player has said so and quit
        while !(inner.jobs[0].state.finished() && inner.running.is_none()) && t0.elapsed() < Duration::from_secs(120) {
            tick(&mut inner);
            drop(inner);
            std::thread::sleep(Duration::from_millis(300));
            inner = shared.lock().unwrap();
        }
        let j = &inner.jobs[0];
        assert_eq!(j.state, JobState::Done, "{:?}", j.message);
        assert!(inner.running.is_none());
    }
}
