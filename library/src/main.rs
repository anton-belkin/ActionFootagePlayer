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

mod index;

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
    map.insert("version".into(), json!(1));
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
#[tauri::command]
async fn scan(shared: State<'_, Shared>, probes: State<'_, index::SharedProbes>, view: String) -> Result<Value, String> {
    let root = shared.lock().unwrap().project.clone().ok_or("Open a project first")?;
    let flat = read_doc(&root).get("settings").and_then(|s| s.get("clips_flat")).and_then(Value::as_bool).unwrap_or(false);
    let probes = probes.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let out = index::scan(&root, &view, flat);
        let mut want: Vec<String> = Vec::new();
        for it in out["items"].as_array().into_iter().flatten() {
            match it["kind"].as_str() {
                Some("clip") => if let Some(p) = it["path"].as_str() { want.push(p.to_string()); },
                // A moment's original: what it is (its camera, its color) is the clip's
                Some("moment") => if let Some(c) = it["link"]["clip"].as_str() { want.push(root.join(c).display().to_string()); },
                _ => {}
            }
        }
        let known = { let mut p = probes.lock().unwrap(); p.want(&want); p.known(&want) };
        json!({ "root": root.display().to_string(), "view": view, "dirs": out["dirs"], "items": out["items"], "probes": known })
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

const KINDS: [&str; 5] = ["clips", "clips-again", "moments", "photos", "export"];

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
    std::thread::spawn(move || index::probe_worker(probe_worker));
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
        .invoke_handler(tauri::generate_handler![snapshot, project, engine, open_project, choose_project, add_job, cancel_job,
                                                 clear_finished, set_setting, reveal, answer_close, recent, scan, probes_since,
                                                 thumb, open_items, reveal_path])
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
