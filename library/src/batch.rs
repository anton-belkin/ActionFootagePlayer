// The player run without a window: the queue of batches, each one's status, Share's runs
// One of the areas of the Library's commands: main.rs is the wiring (the state, main(), generate_handler!'s list)

use super::*;

// ---------- the player, run without a window ----------

// Where ActionFootagePlayer is: AFL_PLAYER (an .app, or its executable), else beside this app, else the usual places
/// The player's ffmpeg (a frame of a camera's proxy for a thumbnail)
pub fn player_ffmpeg() -> Option<PathBuf> {
    let p = player().ok()?;
    let f = if cfg!(target_os = "macos") { p.join("Contents/Helpers/ffmpeg") } else { p.with_file_name(if cfg!(windows) { "ffmpeg.exe" } else { "ffmpeg" }) };
    f.is_file().then_some(f)
}

pub(crate) fn player() -> Result<PathBuf, String> {
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

fn start_player(request: &Path, log: &Path, job: u64) -> Result<Child, String> {
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
    // Its journal says which job of this run it is
    if let Some(parent) = journal::parent_env(job) { cmd.env("AFP_JOURNAL_PARENT", parent); }
    cmd.spawn().map_err(|e| format!("Couldn't start {}: {e}", player.display()))
}

/// gyroflow-optical (vs-gyroflow's: Stabilize optically), beside the player's gyroflow-probe; AFL_OPTICAL first
pub(crate) fn optical_tool() -> Result<PathBuf, String> {
    if let Some(p) = std::env::var_os("AFL_OPTICAL").filter(|p| !p.is_empty()) { return Ok(PathBuf::from(p)); }
    let probe = index::probe_tool(player().ok().as_deref()).ok_or("ActionFootagePlayer's gyroflow-probe isn't there: install ActionFootagePlayer again")?;
    let tool = probe.with_file_name(if cfg!(windows) { "gyroflow-optical.exe" } else { "gyroflow-optical" });
    if tool.is_file() { Ok(tool) } else { Err("This ActionFootagePlayer has no gyroflow-optical: update it".into()) }
}

// Stabilize optically: vs-gyroflow's gyroflow-optical, the request as the player's batches have it, its stderr the log
fn start_optical(request: &Path, log: &Path) -> Result<Child, String> {
    let tool = optical_tool()?;
    let log = std::fs::File::create(log).map_err(|e| e.to_string())?;
    let mut cmd = Command::new(&tool);
    cmd.arg("--batch").arg(request).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::from(log));
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    cmd.spawn().map_err(|e| format!("Couldn't start {}: {e}", tool.display()))
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
    let held = inner.held.clone();
    let Some(job) = inner.jobs.iter_mut().find(|j| j.state == JobState::Waiting && Some(&j.project) != held.as_ref()) else { return };
    let dir = jobs_dir().join(job.id.to_string());
    let request = dir.join("request.json");
    let log = dir.join("log.txt");
    let mut req = job.request.clone();
    req["status"] = json!(dir.join("status.json"));
    req["stop"] = json!(dir.join("stop"));
    // The folders the Library shows now, to make first (set_focus keeps them)
    req["focus"] = json!(jobs_dir().join("focus.json"));
    // A job run again (one that stepped aside): what its last run said isn't this one's
    let _ = std::fs::remove_file(dir.join("status.json"));
    let _ = std::fs::remove_file(dir.join("stop"));
    let started = std::fs::create_dir_all(&dir).map_err(|e| e.to_string())
        .and_then(|_| std::fs::write(&request, req.to_string()).map_err(|e| e.to_string()))
        .and_then(|_| if job.kind == "optical" || job.kind.starts_with("lens-") { start_optical(&request, &log) } else { start_player(&request, &log, job.id) });
    journal::step("job-start", json!({ "job": job.id, "kind": job.kind, "ok": started.is_ok(), "waited_ms": None::<f64> }));
    match started {
        Ok(child) => {
            job.state = JobState::Starting;
            let awake = if cfg!(target_os = "macos") {
                Command::new("/usr/bin/caffeinate").args(["-i", "-w", &child.id().to_string()])
                    .stdout(Stdio::null()).stderr(Stdio::null()).spawn().ok()
            } else { None };
            inner.running = Some(Running { id: job.id, child, dir, started: Instant::now(), stop_asked: None, awake, yielding: false });
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
    if let Some(f) = st.get("made_paths").and_then(Value::as_array) {
        job.made_paths = f.iter().filter_map(Value::as_str).map(str::to_string).collect();
    }
    // A share's: what the player would have rendered each with by itself
    if let (Some(s), Some(u)) = (job.share.as_mut(), st.get("used").and_then(Value::as_array)) { s.used = u.clone(); }
    if let Some(f) = st.get("left_out").and_then(Value::as_array) {
        job.left_out = f.iter().filter_map(|p| {
            let p = p.as_array()?;
            Some((p.first()?.as_str()?.to_string(), p.get(1).and_then(Value::as_str).unwrap_or("").to_string()))
        }).collect();
    }
    if let Some(r) = st.get("results") { job.results = Some(r.clone()); }
    true
}

pub(crate) fn tick(inner: &mut Inner) {
    // Clip previews' pictures, made for as long as it takes, step aside for any other job: stopped after the one being
    // made, then queued again after it (the ones left are found again by their links)
    if let Some(run) = inner.running.as_mut() {
        let finishing = inner.jobs.iter().any(|j| j.id == run.id && j.kind == "clips-finish");
        if finishing && run.stop_asked.is_none() && inner.jobs.iter().any(|j| j.state == JobState::Waiting && j.kind != "clips-finish") {
            let _ = std::fs::write(run.dir.join("stop"), b"");
            run.stop_asked = Some(Instant::now());
            run.yielding = true;
        }
    }
    if let Some(run) = inner.running.as_mut() {
        let id = run.id;
        let exited = run.child.try_wait().ok().flatten();
        let job = inner.jobs.iter_mut().find(|j| j.id == id);
        let mut finished = false;
        if let Some(job) = job {
            let had = job.made_paths.len();
            let has_status = read_status(job, &run.dir);
            // Share: the item being rendered now
            if let (Some(s), Some(now)) = (job.share.as_mut(), job.current.clone()) {
                for it in s.items.iter_mut().filter(|i| i.state == "waiting" || i.state == "rendering") {
                    it.state = if it.name == now { "rendering" } else { "waiting" }.into();
                }
            }
            // Stabilize optically: each clip's motion file, told to the catalog as it lands, and the clip's probe asked
            // again (the views read them: the clip's motion data and badge change then; gyroflow-optical tells no one)
            let made_now = job.made_paths.len().saturating_sub(had);
            if made_now > 0 {
                let made = job.made_paths[had..].to_vec();
                inner.measured.extend(made.iter().cloned());
                std::thread::spawn(move || seen::note_now(&optical_files(&made)));
            }
            if let Some(status) = exited {
                finished = true;
                if !job.state.finished() {
                    job.state = if run.stop_asked.is_some() { JobState::Stopped } else { JobState::Failed };
                    if job.state == JobState::Failed {
                        let log = run.dir.join("log.txt");
                        job.message = Some(if job.kind == "optical" || job.kind.starts_with("lens-") {
                            std::fs::read_to_string(&log).ok().and_then(|t| t.lines().rev().find(|l| !l.trim().is_empty()).map(str::to_string))
                                .unwrap_or_else(|| format!("gyroflow-optical stopped ({status})"))
                        } else {
                            log_error(&log).unwrap_or_else(|| format!("ActionFootagePlayer stopped ({status})"))
                        });
                    }
                }
            } else if !has_status && run.started.elapsed() > START_TIMEOUT {
                let _ = run.child.kill();
                job.state = JobState::Failed;
                job.message = Some(if job.kind == "optical" || job.kind.starts_with("lens-") { "gyroflow-optical didn't start".into() } else { "ActionFootagePlayer didn't start the batch".into() });
            } else if run.stop_asked.is_some_and(|t| t.elapsed() > STOP_TIMEOUT) {
                let _ = run.child.kill();
            }
        } else {
            finished = exited.is_some();
        }
        if finished {
            if run.yielding {
                if let Some(i) = inner.jobs.iter().position(|j| j.id == id) {
                    let mut job = inner.jobs.remove(i);
                    job.state = JobState::Waiting;
                    (job.done, job.total, job.failed, job.made, job.fraction, job.current, job.message) = (0, 0, 0, None, None, None, None);
                    job.failures.clear();
                    job.left_out.clear();
                    // Stabilize optically again: the clips it made already aren't made again
                    if job.kind == "optical" && !job.made_paths.is_empty() {
                        let made = std::mem::take(&mut job.made_paths);
                        if let Some(c) = job.request.get_mut("clips").and_then(Value::as_array_mut) { c.retain(|p| !p.as_str().is_some_and(|p| made.iter().any(|m| m == p))); }
                    }
                    inner.jobs.push(job);
                }
            }
            // A share's renders are made (or not): the worker hands them over (share_collect), the job done only then
            if let Some(job) = inner.jobs.iter_mut().find(|j| j.id == id && j.share.is_some()) {
                if let Some(s) = job.share.as_mut() { s.ended = true; }
            }
            // What a batch made, and the clips it looked at, into the catalog: a look at everything
            seen::look_at_everything(inner.watch.as_ref().map(|w| &w.runner));
            if let Some(mut run) = inner.running.take() {
                if let Some(mut a) = run.awake.take() { let _ = a.kill(); let _ = a.wait(); }
                let _ = run.child.wait();
            }
        }
    }
    if inner.running.is_none() { start_job(inner); }
}

/// The Gyroflow projects gyroflow-optical put beside these clips (<stem>.gyroflow, or <name.ext>.gyroflow when that's
/// the one it replaced)
/// The motion files Stabilize optically wrote beside these clips (vs-gyroflow's afp_motion.rs: <stem>.afp-motion)
pub(crate) fn optical_files(clips: &[String]) -> Vec<PathBuf> {
    clips.iter().map(|c| Path::new(c).with_extension("afp-motion")).filter(|g| g.is_file()).collect()
}

pub(crate) fn stop_all(inner: &mut Inner) {
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

const KINDS: [&str; 10] = ["clips", "clips-again", "clips-finish", "moments", "photos", "export", "render", "optical", "lens-find", "lens-estimate"];

#[tauri::command]
pub(crate) async fn add_job(shared: State<'_, Shared>, kind: String, options: Option<Value>, only: Option<Vec<String>>, label: Option<String>, journal: Option<String>) -> Result<(), String> {
    journal::step("job-added", json!({ "a": journal, "kind": kind, "items": only.as_ref().map(Vec::len) }));
    // In the background: the window never waits on it (a share can take seconds)
    let shared = shared.inner().clone();
    tauri::async_runtime::spawn_blocking(move || -> Result<(), String> {
    if !KINDS.contains(&kind.as_str()) { return Err(format!("no such batch: {kind}")); }
    let mut inner = shared.lock().unwrap();
    let root = inner.project.clone().ok_or("Open a project first")?;
    let options = options.unwrap_or(json!({}));
    let photo = options.get("photo").and_then(Value::as_str).unwrap_or("no").to_string();
    let sdr = options.get("sdr").and_then(Value::as_bool).unwrap_or(false);
    let title = match kind.as_str() {
        "clips" => "Clip previews (empty, to arrange now)".to_string(),
        "clips-finish" => "Clip previews: their pictures".to_string(),
        "clips-again" => "Clip previews of the removed clips".to_string(),
        "moments" => "Moments of all marks".to_string(),
        "photos" => format!("Photos of all marks ({})", match photo.as_str() { "live" => "Live Photos", "motion" => "Motion Photos", _ => "stills" }),
        "optical" => "Stabilize optically".to_string(),
        "lens-find" => "Find lens".to_string(),
        "lens-estimate" => "Estimate lens".to_string(),
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
    // Chosen for this run (docs/design/library-management.md §8): how clip previews are laid out in the run's folder, and
    // whether new moments are stabilized on their own
    // Clip previews (Anton, 2026-11-10): empty ones at once (a dark frame with the link inside, all in a few seconds), to
    // arrange in Footage right away; then a job making their pictures in their places, the folder in view first, out of
    // the way of any other job
    if kind == "clips" { request["draft"] = json!(true); }
    if kind == "clips" || kind == "clips-again" {
        let layout = options.get("layout").and_then(Value::as_str).filter(|l| ["tree", "flat", "trips"].contains(l)).unwrap_or("tree");
        request["layout"] = json!(layout);
    }
    if kind == "moments" { request["alone"] = json!(options.get("alone").and_then(Value::as_bool).unwrap_or(false)); }
    // The clips, by their paths in the project's tree: each one's file is the copy its own files are beside (the main
    // one, tags::clip_own), where its Gyroflow project goes. Redo: those with a project made again
    if kind == "optical" || kind.starts_with("lens-") {
        let trees: Vec<&str> = options.get("clips").and_then(Value::as_array).into_iter().flatten().filter_map(Value::as_str).collect();
        let clips: Vec<String> = trees.iter().filter_map(|t| tags::clip_own(&root, t).ok()).map(|(_, clip)| clip.display().to_string()).collect();
        if clips.is_empty() { return Err("Select clips".into()); }
        // Estimate lens: one clip (an estimate is of one stretch of one clip)
        if kind == "lens-estimate" && clips.len() != 1 { return Err("Estimate lens is for one clip at a time".into()); }
        request["clips"] = json!(clips);
        request["redo"] = json!(options.get("redo").and_then(Value::as_bool).unwrap_or(false));
        if kind.starts_with("lens-") { request["action"] = json!(kind.trim_start_matches("lens-").to_string() + "-lens"); }
        // Find stopped at the first lens that fits (the lens dialog's Quick, on by default)
        if kind == "lens-find" { request["quick"] = json!(options.get("quick").and_then(Value::as_bool).unwrap_or(true)); }
    }
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
        failures: Vec::new(), left_out: Vec::new(), made_paths: Vec::new(), request, share: None, results: None,
    };
    let follow = (job.kind == "clips").then(|| (job.project.clone(), job.project_name.clone()));
    inner.jobs.push(job);
    if let Some((project, project_name)) = follow {
        if !inner.jobs.iter().any(|j| !j.state.finished() && j.project == project && j.kind == "clips-finish") {
            inner.next_id += 1;
            let request = json!({ "kind": "clips-finish", "project": project });
            let id = inner.next_id;
            inner.jobs.push(Job { id, kind: "clips-finish".into(), title: "Clip previews: their pictures".into(), project_name, project, state: JobState::Waiting,
                                  done: 0, total: 0, failed: 0, made: None, fraction: None, current: None, message: None, failures: Vec::new(), left_out: Vec::new(), made_paths: Vec::new(), request, share: None, results: None });
        }
    }
    tick(&mut inner);
    Ok(())
}).await.map_err(|e| e.to_string())?
}

// ---------- Share (share.rs) ----------

// A share whose renders the player has made: each handed to the destination, away from the lock (a copy to a share
// takes its time), then the job says how it went
pub(crate) fn share_collect(shared: &Shared) {
    let ended: Vec<(u64, share::Share, Vec<(String, String)>, bool)> = {
        let mut inner = shared.lock().unwrap();
        // (ended, or never run: the player isn't installed, the job was taken out of the queue's turn)
        let pending = |s: &share::Share| s.items.iter().any(|i| i.state == "waiting" || i.state == "rendering");
        inner.jobs.iter_mut().filter(|j| j.share.as_ref().is_some_and(|s| s.ended || (j.state.finished() && pending(s)))).map(|j| {
            let s = j.share.as_mut().unwrap();
            s.ended = false;
            (j.id, s.clone(), j.failures.clone(), j.state == JobState::Stopped)
        }).collect()
    };
    // afp-cut, beside the player's gyroflow-probe: a moment with nothing to change is its part cut as it is
    if ended.is_empty() { return; }
    let cut = index::probe_tool(player().ok().as_deref())
        .map(|p| p.with_file_name(if cfg!(windows) { "afp-cut.exe" } else { "afp-cut" })).filter(|p| p.is_file());
    for (id, mut s, failures, stopped) in ended {
        let written = share::collect(&mut s, &failures, stopped, cut.as_deref(), player_ffmpeg().as_deref());
        seen::note_now(&written);
        let mut inner = shared.lock().unwrap();
        if let Some(j) = inner.jobs.iter_mut().find(|j| j.id == id) { share_sum(j, s); }
    }
}

// A share's job as its items are now: how many are saved, which failed and why
fn share_sum(job: &mut Job, s: share::Share) {
    let left = s.items.iter().filter(|i| !matches!(i.state.as_str(), "copied" | "failed")).count();
    job.total = s.items.len() as u64;
    job.failed = s.items.iter().filter(|i| i.state == "failed").count() as u64;
    job.done = job.total - left as u64;
    job.made = Some(s.items.iter().filter(|i| i.state == "copied").count() as u64);
    job.failures = s.items.iter().filter(|i| i.state == "failed").map(|i| (i.name.clone(), i.message.clone().unwrap_or_default())).collect();
    if left == 0 {
        // (stopped, or the player failed and said why: as it is)
        if !matches!(job.state, JobState::Stopped | JobState::Failed) {
            job.state = if job.made == Some(0) && job.failed > 0 { JobState::Failed } else { JobState::Done };
            job.message = (job.state == JobState::Failed).then(|| "Nothing was saved".to_string());
        }
        job.fraction = None;
        job.current = None;
    }
    job.share = Some(s);
}

// Share: the selection (`items`: { kind: moment | photo | clip | file, path, name }) to a destination. For now one,
// "folder": each item's render copied into `folder` (asked for here when not given) under a readable name. What the
// render cache has is copied at once; the rest is rendered first by the player, as a job in the queue, and copied when
// it's made. The folder chosen, or nothing when the choice was cancelled
#[tauri::command]
pub(crate) async fn share(shared: State<'_, Shared>, items: Vec<Value>, dest: String, folder: Option<String>, look: Option<String>, settings: Option<Value>) -> Result<Option<String>, String> {
    if !share::DESTINATIONS.iter().any(|(id, _)| *id == dest) { return Err(format!("no such destination: {dest}")); }
    if items.is_empty() { return Err("Select what to share".into()); }
    let folder = match folder.filter(|f| !f.is_empty()) {
        Some(f) => PathBuf::from(f),
        None => match rfd::AsyncFileDialog::new().set_title("Save to a folder").pick_folder().await { Some(f) => f.path().to_path_buf(), None => return Ok(None) },
    };
    let shared = shared.inner().clone();
    tauri::async_runtime::spawn_blocking(move || -> Result<Option<String>, String> {
        let root = shared.lock().unwrap().project.clone().ok_or("Open a project first")?;
        let look = look.unwrap_or_else(|| "HDR".into());
        // Shown in the queue at once: looking at the cache, then copying what it has
        let id = {
            let mut inner = shared.lock().unwrap();
            inner.next_id += 1;
            let n = items.len();
            let title = format!("Share · {} to {}", if n == 1 { "1 item".to_string() } else { format!("{n} items") },
                                folder.file_name().map(|f| f.to_string_lossy().into_owned()).unwrap_or_else(|| folder.display().to_string()));
            let id = inner.next_id;
            inner.jobs.push(Job { id, kind: "share".into(), title, project_name: root.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default(),
                                  project: root.display().to_string(), state: JobState::Looking, done: 0, total: n as u64, failed: 0, made: None, fraction: None,
                                  current: None, message: None, failures: Vec::new(), left_out: Vec::new(), made_paths: Vec::new(), request: Value::Null,
                                  share: Some(share::Share { dest: dest.clone(), folder: folder.display().to_string(), items: Vec::new(), ..Default::default() }), results: None });
            id
        };
        // The export's settings (the sheet's: size, encoding, quality), else the defaults
        let (planned, asked) = share::plan(&root, &items, &look, &settings.unwrap_or(Value::Null));
        let mut s = share::Share { dest: dest.clone(), folder: folder.display().to_string(), items: planned, ..Default::default() };
        let show = |s: &share::Share, request: Option<Value>| {
            let mut inner = shared.lock().unwrap();
            if let Some(j) = inner.jobs.iter_mut().find(|j| j.id == id) {
                share_sum(j, s.clone());
                // What the cache lacks: the player's to make, in its turn
                if let Some(r) = request { j.request = r; j.state = JobState::Waiting; }
            }
            tick(&mut inner);
        };
        show(&s, None);
        let mut written = Vec::new();
        for i in 0..s.items.len() {
            if s.items[i].state != "copying" { continue; }
            written.extend(share::deliver(&dest, &folder, &mut s.items[i]));
            show(&s, None);
        }
        seen::note_now(&written);
        let request = (!asked.is_empty()).then(|| json!({ "kind": "share", "project": root.display().to_string(), "items": asked }));
        show(&s, request);
        Ok(Some(folder.display().to_string()))
    }).await.map_err(|e| e.to_string())?
}

// The folders the Library shows now (absolute, on disk): a clip previews' pictures job makes theirs first
#[tauri::command]
pub(crate) fn set_focus(dirs: Vec<String>) -> Result<(), String> {
    let file = jobs_dir().join("focus.json");
    let _ = std::fs::create_dir_all(jobs_dir());
    let tmp = file.with_extension("json.tmp");
    std::fs::write(&tmp, json!({ "dirs": dirs }).to_string()).map_err(|e| e.to_string())?;
    std::fs::rename(&tmp, &file).map_err(|e| e.to_string())
}

#[tauri::command]
pub(crate) async fn cancel_job(shared: State<'_, Shared>, id: u64) -> Result<(), String> {
    // In the background: the window never waits on it (a share can take seconds)
    let shared = shared.inner().clone();
    tauri::async_runtime::spawn_blocking(move || -> () {
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
}).await.map_err(|e| e.to_string())
}

#[tauri::command]
pub(crate) async fn clear_finished(shared: State<'_, Shared>) -> Result<(), String> {
    // In the background: the window never waits on it (a share can take seconds)
    let shared = shared.inner().clone();
    tauri::async_runtime::spawn_blocking(move || -> () {
    shared.lock().unwrap().jobs.retain(|j| !j.state.finished());
}).await.map_err(|e| e.to_string())
}
