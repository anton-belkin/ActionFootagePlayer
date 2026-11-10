// SPDX-License-Identifier: GPL-3.0-or-later
//
// Optical stabilization of clips the player can't stabilize with their own motion data: Gyroflow measures the camera's
// motion from the picture (its "Analyze image optically", motion from the video alone), and the Gyroflow project it
// writes is put beside the clip, where the player looks for one and plays with it (the gyroflow mpv plugin's own check,
// docs/library-management.md "The clip's own file"). Action Footage Library's "Stabilize optically" runs it on the
// selected clips, one after another (bin/gyroflow-optical.rs).
//
// Which clips (`verdict`):
// - one with a Gyroflow project beside it already is left out: it was made there, perhaps by hand, and is never
//   overwritten (delete it to make it again);
// - one with motion data the player stabilizes it with (the probe's "default": gyro, recorded with the camera's own
//   stabilization off) is left out: the gyro is better than what the picture says;
// - a 360° clip is left out (the 360 mode plays it);
// - one whose lens isn't known is left out: without a lens profile Gyroflow can't turn the picture's motion into the
//   camera's. Known: the lens the file has (DJI, newer GoPros), else the profile Gyroflow's database has for the camera,
//   lens mode, resolution and frame rate (what Gyroflow itself loads for it);
// - the rest are stabilized: no motion data (DJI with EIS on, older cameras), or motion data the camera already
//   stabilized the picture with (GoPro's HyperSmooth, DJI's RockSteady): that motion data is set aside
//   (ignore_file_motion), since the picture no longer moves as the camera did.
//
// The engine is Gyroflow's own command line (the fork's, which has the optical analysis): `gyroflow <clip> --preset
// <p> --export-project 1`, the analysis asked for in the preset's synchronization (a preset's synchronization replaces
// the command line's, so --optical-correction alone is lost), the project written into a work folder first and moved
// beside the clip once it's whole.

use std::io::{ BufRead, BufReader };
use std::path::{ Path, PathBuf };
use std::process::{ Command, Stdio };
use std::sync::OnceLock;
use std::time::Duration;
use gyroflow_core::{ filesystem, util, camera_identifier::CameraIdentifier, lens_profile_database::LensProfileDatabase };
use gyroflow_core::telemetry_parser::{ Input, InputOptions, tags_impl::* };
use serde_json::{ json, Value };

/// What's done with a clip
#[derive(Debug, Clone, PartialEq)]
pub enum Verdict {
    /// Stabilized optically, with this lens: its label ("in the file", or the profile's name), and the profile itself
    /// to give Gyroflow when it wouldn't load it by itself (None: it does)
    Stabilize { lens: String, profile: Option<String> },
    /// Left out, and why (said to the user)
    LeaveOut(String),
}

/// A Gyroflow project beside the video already: `<stem>.gyroflow` (Gyroflow's own name for it) or `<name>.gyroflow`
pub fn own_project(video: &Path) -> Option<PathBuf> {
    let dir = video.parent()?;
    let name = video.file_name()?.to_string_lossy().into_owned();
    let stem = video.file_stem()?.to_string_lossy().into_owned();
    [dir.join(format!("{stem}.gyroflow")), dir.join(format!("{name}.gyroflow"))].into_iter().find(|p| p.is_file())
}

/// What the clip is, as far as this decides: read from its metadata (probe_only: the first samples)
#[derive(Debug, Clone, Default)]
pub struct Facts {
    /// The camera ("GoPro HERO12 Black")
    pub camera: String,
    /// The player stabilizes it with its own motion data (probe::probe's "default")
    pub own_motion: bool,
    /// Why not, when it doesn't (probe::probe's reason)
    pub reason: String,
    /// A 360° clip: stitched (spherical metadata) or a 360 camera's own file
    pub spherical: bool,
    /// The lens: (its label, the profile to give Gyroflow, None when Gyroflow loads it by itself); None: not known
    pub lens: Option<(String, Option<String>)>,
}

/// The decision, from what the clip is and whether it has a project already
pub fn decide(has_project: bool, facts: &Facts) -> Verdict {
    if has_project { return Verdict::LeaveOut("it has a Gyroflow project already (delete it to make a new one)".into()); }
    if facts.spherical { return Verdict::LeaveOut("a 360° clip: the player's 360 view plays it".into()); }
    if facts.own_motion { return Verdict::LeaveOut("it has motion data the player stabilizes it with".into()); }
    match &facts.lens {
        Some((lens, profile)) => Verdict::Stabilize { lens: lens.clone(), profile: profile.clone() },
        None => Verdict::LeaveOut(format!("{}: its lens isn't known (no lens profile for this camera and mode)",
            if facts.camera.is_empty() { "unknown camera" } else { &facts.camera })),
    }
}

/// Gyroflow's lens database, read once
fn lens_db() -> &'static LensProfileDatabase {
    static DB: OnceLock<LensProfileDatabase> = OnceLock::new();
    DB.get_or_init(|| { let mut db = LensProfileDatabase::default(); db.load_all(); db })
}

/// The lens Gyroflow corrects the clip with: the one in the file, else the one its database has for the camera's
/// identifier (Gyroflow loads it by itself on opening the clip, as it does here)
fn lens_of(input: &Input, width: usize, height: usize, fps: f64) -> Option<(String, Option<String>)> {
    let samples = input.samples.as_deref().unwrap_or_default();
    let in_file = samples.iter().filter_map(|s| s.tag_map.as_ref()?.get(&GroupId::Lens))
        .any(|m| m.contains_key(&TagId::Data) || m.contains_key(&TagId::Name));
    if in_file { return Some(("in the file".into(), None)); }
    let id = CameraIdentifier::from_telemetry_parser(input, width, height, fps).ok()?.get_identifier_for_autoload();
    (!id.is_empty() && lens_db().contains_id(&id)).then_some((id, None))
}

/// What the clip is (`Facts`); Err: it can't be read
pub fn facts(video: &Path) -> Result<Facts, String> {
    let path = video.to_string_lossy().into_owned();
    let url = filesystem::path_to_url(&path);
    let mut file = std::fs::File::open(video).map_err(|e| e.to_string())?;
    let size = file.metadata().map_err(|e| e.to_string())?.len() as usize;
    let meta = util::get_video_metadata(&mut file, size, &url).map_err(|e| format!("not a video Gyroflow reads: {e}"))?;
    let options = InputOptions { probe_only: true, ..Default::default() };
    let input = Input::from_stream_with_options(&mut file, size, &path, |_| (), std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)), options)
        .map_err(|e| e.to_string())?;
    let camera = format!("{} {}", input.camera_type(), input.camera_model().map(|s| s.as_str()).unwrap_or("")).trim().to_string();
    let lens = lens_of(&input, meta.width, meta.height, meta.fps);
    let (own_motion, reason) = match crate::probe::probe(&path) { Ok(_) => (true, String::new()), Err(e) => (false, e) };
    let spherical = crate::probe::spherical(&path).is_some() || crate::rig::read(&path).is_some();
    Ok(Facts { camera, own_motion, reason, spherical, lens })
}

/// The decision for a clip on the disk
pub fn verdict(video: &Path) -> Verdict {
    if !video.is_file() { return Verdict::LeaveOut("not found".into()); }
    let has_project = own_project(video).is_some();
    if has_project { return decide(true, &Facts::default()); }
    match facts(video) {
        Ok(f) => decide(false, &f),
        Err(e) => Verdict::LeaveOut(format!("couldn't be read: {e}")),
    }
}

// ---------- Gyroflow's command line ----------

/// How the analysis runs
#[derive(Debug, Clone)]
pub struct Settings {
    /// How much of the measured motion is taken, 0-1 (1: all of it, as there's no other)
    pub strength: f64,
    /// "klt" (corners, the default) or "dis" (dense optical flow: low texture such as sky or water, slower)
    pub tracking: String,
}

impl Default for Settings {
    fn default() -> Self { Settings { strength: 1.0, tracking: "klt".into() } }
}

/// The preset that asks for the analysis: in its synchronization, which replaces the command line's
pub fn preset(s: &Settings) -> Value {
    json!({ "version": 2, "synchronization": {
        "do_optical_correction": true,
        "ignore_file_motion": true,
        "optical_correction_strength": s.strength.clamp(0.0, 1.0),
        "optical_tracking": s.tracking,
    } })
}

/// Gyroflow's executable: AFP_GYROFLOW, else where it's installed (macOS: the app, by its place or its bundle id;
/// Windows: the app .gyroflow files open with, then the usual places; Linux: `gyroflow` in PATH, then /opt)
pub fn gyroflow_cli() -> Result<PathBuf, String> {
    if let Some(p) = std::env::var_os("AFP_GYROFLOW").filter(|p| !p.is_empty()) {
        let p = PathBuf::from(p);
        let p = if p.extension().is_some_and(|e| e == "app") { p.join("Contents/MacOS/gyroflow") } else { p };
        return if p.is_file() { Ok(p) } else { Err(format!("AFP_GYROFLOW: {} isn't there", p.display())) };
    }
    let mut places: Vec<PathBuf> = Vec::new();
    let home = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE")).map(PathBuf::from);
    if cfg!(target_os = "macos") {
        places.push(PathBuf::from("/Applications/Gyroflow.app"));
        if let Some(h) = &home { places.push(h.join("Applications/Gyroflow.app")); }
        // Anywhere else: Spotlight, by its bundle id
        if let Ok(o) = Command::new("/usr/bin/mdfind").arg("kMDItemCFBundleIdentifier == 'xyz.gyroflow'").stderr(Stdio::null()).output() {
            places.extend(String::from_utf8_lossy(&o.stdout).lines().filter(|l| !l.is_empty()).map(PathBuf::from));
        }
        places = places.into_iter().map(|p| p.join("Contents/MacOS/gyroflow")).collect();
    } else if cfg!(windows) {
        if let Some(p) = windows_association() { places.push(p); }
        for var in ["ProgramFiles", "LOCALAPPDATA"] {
            if let Some(d) = std::env::var_os(var) {
                places.push(PathBuf::from(&d).join("Gyroflow/gyroflow.exe"));
                places.push(PathBuf::from(&d).join("Programs/Gyroflow/gyroflow.exe"));
            }
        }
        places.extend(in_path("gyroflow.exe"));
    } else {
        places.extend(in_path("gyroflow"));
        places.push(PathBuf::from("/opt/Gyroflow/gyroflow"));
        if let Some(h) = &home { places.push(h.join(".local/bin/gyroflow")); }
    }
    places.into_iter().find(|p| p.is_file())
        .ok_or_else(|| "Gyroflow isn't installed (it's what measures the motion): install it, or set AFP_GYROFLOW to its executable".into())
}

fn in_path(exe: &str) -> Vec<PathBuf> {
    std::env::var_os("PATH").map(|p| std::env::split_paths(&p).map(|d| d.join(exe)).collect()).unwrap_or_default()
}

/// The program .gyroflow files open with (Gyroflow's installer sets it): `assoc` and `ftype`, as cmd tells them
fn windows_association() -> Option<PathBuf> {
    if !cfg!(windows) { return None; }
    let run = |args: &[&str]| -> Option<String> {
        let mut cmd = Command::new("cmd");
        cmd.arg("/d").arg("/c").args(args).stdin(Stdio::null()).stderr(Stdio::null());
        #[cfg(windows)]
        { use std::os::windows::process::CommandExt; cmd.creation_flags(0x0800_0000); }
        let o = cmd.output().ok()?;
        Some(String::from_utf8_lossy(&o.stdout).trim().to_string())
    };
    let progid = run(&["assoc", ".gyroflow"])?.split_once('=')?.1.trim().to_string();
    let command = run(&["ftype", &progid])?.split_once('=')?.1.trim().to_string();
    executable_of_command(&command)
}

/// The executable of a command line as Windows keeps them: `"C:\Program Files\Gyroflow\gyroflow.exe" "%1"`
pub fn executable_of_command(command: &str) -> Option<PathBuf> {
    let c = command.trim();
    let exe = if let Some(rest) = c.strip_prefix('"') { rest.split('"').next()? } else { c.split_whitespace().next()? };
    (!exe.is_empty()).then(|| PathBuf::from(exe))
}

/// "[69a8ba86] Synchronization progress: 22.8%" -> 0.228
pub fn progress_of(line: &str) -> Option<f64> {
    let rest = line.split("Synchronization progress: ").nth(1)?;
    rest.trim().trim_end_matches('%').parse::<f64>().ok().map(|p| (p / 100.0).clamp(0.0, 1.0))
}

/// The last error Gyroflow logged, said plainly ("Optical analysis failed: Not enough of the image could be tracked")
pub fn error_of(log: &str) -> Option<String> {
    let line = log.lines().rev().find(|l| l.contains("[ERROR]") && !l.contains("bookmark"))?;
    let msg = line.split("Error: ").last().unwrap_or(line).trim();
    let msg = msg.strip_prefix("An error occurred: ").unwrap_or(msg);
    Some(msg.split("] ").last().unwrap_or(msg).trim().to_string())
}

/// A project Gyroflow wrote with the analysis in it (the fork's gyro_source.optical_correction); Gyroflow without it
/// writes the project with nothing measured
pub fn check_project(text: &str) -> Result<(), String> {
    let v: Value = serde_json::from_str(text).map_err(|e| format!("Gyroflow wrote a project that isn't JSON: {e}"))?;
    let measured = v.pointer("/gyro_source/optical_correction").is_some_and(|o| !o.is_null() && o != &json!("") && o != &json!({}));
    if measured { Ok(()) } else {
        Err("Gyroflow measured nothing: this Gyroflow has no optical analysis (it needs ActionFootagePlayer's Gyroflow build)".into())
    }
}

/// Puts the project Gyroflow made beside the video as `<stem>.gyroflow`: copied next to it under a temporary name,
/// then renamed, never over one that's there (made meanwhile)
pub fn place(made: &Path, video: &Path) -> Result<PathBuf, String> {
    let dir = video.parent().ok_or("the clip has no folder")?;
    let stem = video.file_stem().ok_or("the clip has no name")?.to_string_lossy().into_owned();
    let dest = dir.join(format!("{stem}.gyroflow"));
    if own_project(video).is_some() { return Err("a Gyroflow project appeared beside it meanwhile: left as it is".into()); }
    let part = dir.join(format!(".{stem}.gyroflow.part"));
    std::fs::copy(made, &part).map_err(|e| format!("couldn't write beside the clip ({}): {e}", dir.display()))?;
    // A plain rename replaces; hard_link doesn't: where links aren't possible (some shares), checked just before
    let linked = std::fs::hard_link(&part, &dest);
    let result = match linked {
        Ok(()) => Ok(dest.clone()),
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => Err("a Gyroflow project appeared beside it meanwhile: left as it is".into()),
        Err(_) if !dest.exists() => std::fs::rename(&part, &dest).map(|_| dest.clone()).map_err(|e| format!("couldn't write {}: {e}", dest.display())),
        Err(_) => Err("a Gyroflow project appeared beside it meanwhile: left as it is".into()),
    };
    let _ = std::fs::remove_file(&part);
    result
}

/// Runs Gyroflow on one clip and puts its project beside it. `stop` is asked about twice a second (true: Gyroflow
/// is ended, nothing written); `progress` hears how far the analysis is (0-1)
pub fn stabilize(gyroflow: &Path, video: &Path, profile: Option<&str>, settings: &Settings, work: &Path,
                 stop: &dyn Fn() -> bool, progress: &mut dyn FnMut(f64)) -> Result<PathBuf, String> {
    let _ = std::fs::remove_dir_all(work);
    std::fs::create_dir_all(work).map_err(|e| e.to_string())?;
    let stem = video.file_stem().ok_or("the clip has no name")?.to_string_lossy().into_owned();
    let preset_file = work.join("preset.gyroflow");
    std::fs::write(&preset_file, preset(settings).to_string()).map_err(|e| e.to_string())?;
    // Gyroflow's output options are JSON with ' for " (it swaps them): the work folder's path has neither
    let out = work.join(format!("{stem}.mp4"));
    let out_params = json!({ "output_path": out.to_string_lossy() }).to_string();
    let mut cmd = Command::new(gyroflow);
    cmd.arg(video);
    if let Some(p) = profile {
        let f = work.join("lens.json");
        std::fs::write(&f, p).map_err(|e| e.to_string())?;
        cmd.arg(f);
    }
    let log_file = work.join("gyroflow.log");
    let log = std::fs::File::create(&log_file).map_err(|e| e.to_string())?;
    cmd.arg("--preset").arg(&preset_file).args(["--export-project", "1", "--stdout-progress", "-p", &out_params])
        .current_dir(work).stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::from(log));
    #[cfg(windows)]
    { use std::os::windows::process::CommandExt; cmd.creation_flags(0x0800_0000); }
    let mut child = cmd.spawn().map_err(|e| format!("couldn't start Gyroflow ({}): {e}", gyroflow.display()))?;
    // Its progress, read on a thread of its own: the stop is looked at meanwhile
    let (tx, rx) = std::sync::mpsc::channel::<f64>();
    let stdout = child.stdout.take();
    let reader = std::thread::spawn(move || {
        if let Some(out) = stdout {
            for line in BufReader::new(out).lines().map_while(Result::ok) {
                if let Some(p) = progress_of(&line) { let _ = tx.send(p); }
            }
        }
    });
    let status = loop {
        while let Ok(p) = rx.try_recv() { progress(p); }
        if let Some(s) = child.try_wait().map_err(|e| e.to_string())? { break s; }
        if stop() {
            let _ = child.kill();
            let _ = child.wait();
            let _ = reader.join();
            return Err("stopped".into());
        }
        std::thread::sleep(Duration::from_millis(500));
    };
    let _ = reader.join();
    while let Ok(p) = rx.try_recv() { progress(p); }
    let made = work.join(format!("{stem}.gyroflow"));
    let text = std::fs::read_to_string(&made).map_err(|_| {
        let log = std::fs::read_to_string(&log_file).unwrap_or_default();
        error_of(&log).unwrap_or_else(|| format!("Gyroflow wrote no project ({status})"))
    })?;
    check_project(&text)?;
    place(&made, video)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn facts(own_motion: bool, lens: Option<&str>) -> Facts {
        Facts { camera: "GoPro HERO12 Black".into(), own_motion, lens: lens.map(|l| (l.to_string(), None)), ..Default::default() }
    }

    #[test]
    fn decides() {
        assert!(matches!(decide(true, &facts(false, Some("in the file"))), Verdict::LeaveOut(r) if r.contains("project already")));
        assert!(matches!(decide(false, &facts(true, Some("in the file"))), Verdict::LeaveOut(r) if r.contains("motion data")));
        assert!(matches!(decide(false, &facts(false, None)), Verdict::LeaveOut(r) if r.contains("GoPro HERO12 Black: its lens isn't known")));
        assert_eq!(decide(false, &facts(false, Some("in the file"))), Verdict::Stabilize { lens: "in the file".into(), profile: None });
        let mut f = facts(false, Some("in the file"));
        f.spherical = true;
        assert!(matches!(decide(false, &f), Verdict::LeaveOut(r) if r.contains("360")));
    }

    #[test]
    fn reads_gyroflow() {
        assert_eq!(progress_of("[69a8ba86] Synchronization progress: 22.8%"), Some(0.228));
        assert_eq!(progress_of("[0ce85b03] Rendering completed: x"), None);
        let log = "14:37:22 [ERROR] Failed to create bookmark: The file couldn't be opened.\n\
                   14:37:24 [ERROR] [69a8ba86] Error: An error occurred: Optical analysis failed: Not enough of the image could be tracked\n\
                   14:37:24 [ERROR] Failed to create bookmark: x\n";
        assert_eq!(error_of(log).as_deref(), Some("Optical analysis failed: Not enough of the image could be tracked"));
        assert_eq!(executable_of_command(r#""C:\Program Files\Gyroflow\gyroflow.exe" "%1""#), Some(PathBuf::from(r"C:\Program Files\Gyroflow\gyroflow.exe")));
        assert_eq!(executable_of_command(r#"C:\Gyroflow\gyroflow.exe "%1""#), Some(PathBuf::from(r"C:\Gyroflow\gyroflow.exe")));
        assert!(check_project(r#"{"gyro_source":{"optical_correction":"AAAA"}}"#).is_ok());
        assert!(check_project(r#"{"gyro_source":{"optical_correction_enabled":true}}"#).is_err());
        let p = preset(&Settings::default());
        assert_eq!(p["synchronization"]["do_optical_correction"], json!(true));
        assert_eq!(p["synchronization"]["ignore_file_motion"], json!(true));
    }

    #[test]
    fn places_beside_never_over() {
        let dir = std::env::temp_dir().join(format!("afp-optical-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("work")).unwrap();
        let video = dir.join("GX010973.MP4");
        std::fs::write(&video, b"video").unwrap();
        let made = dir.join("work/GX010973.gyroflow");
        std::fs::write(&made, b"{\"a\":1}").unwrap();
        assert!(own_project(&video).is_none());
        let placed = place(&made, &video).unwrap();
        assert_eq!(placed, dir.join("GX010973.gyroflow"));
        assert_eq!(std::fs::read(&placed).unwrap(), b"{\"a\":1}");
        assert_eq!(own_project(&video), Some(placed.clone()));
        // There already: left as it is
        std::fs::write(&made, b"{\"b\":2}").unwrap();
        assert!(place(&made, &video).is_err());
        assert_eq!(std::fs::read(&placed).unwrap(), b"{\"a\":1}");
        assert!(!dir.join(".GX010973.gyroflow.part").exists());
        assert!(matches!(verdict(&video), Verdict::LeaveOut(r) if r.contains("project already")));
        assert!(matches!(verdict(&dir.join("nope.MP4")), Verdict::LeaveOut(r) if r == "not found"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    // A project gyroflow-optical made, played as the player plays it (load_manager): the motion Gyroflow measured is
    // there, and it's smoothed. AFP_OPTICAL_PROJECT=<clip>.gyroflow cargo test --release --lib optical -- --ignored
    #[test]
    #[ignore]
    fn plays_with_the_project() {
        let Some(project) = std::env::var_os("AFP_OPTICAL_PROJECT") else { return };
        let project = project.to_string_lossy().into_owned();
        let (stab, _, _) = crate::load_manager(&project, None, None, crate::water::Underwater::Project, 0.0, false).unwrap();
        stab.recompute_blocking();
        let g = stab.gyro.read();
        assert!(g.optical_correction.is_some(), "the measured motion isn't in the player's engine");
        eprintln!("ignores the file's motion: {}, optical correction on: {:?}", g.ignores_file_motion(), g.optical_correction.as_ref().map(|c| c.enabled));
        assert!(g.ignores_file_motion(), "the camera's own motion data is used: it's of the picture before the camera stabilized it");
        let q: Vec<_> = g.quaternions.values().cloned().collect();
        let sm: Vec<_> = g.smoothed_quaternions.values().cloned().collect();
        assert!(q.len() > 10 && q.len() == sm.len());
        let travel: f64 = q.windows(2).map(|w| w[0].angle_to(&w[1])).sum::<f64>().to_degrees();
        let removed = (q.iter().zip(&sm).map(|(a, b)| a.angle_to(b).powi(2)).sum::<f64>() / q.len() as f64).sqrt().to_degrees();
        eprintln!("{project}: {} orientations, the camera turned {travel:.1}° along the way, {removed:.2}° rms taken out by smoothing", q.len());
        assert!(travel > 0.1 && removed > 0.01);
    }
}
