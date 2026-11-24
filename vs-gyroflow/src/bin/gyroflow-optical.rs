// SPDX-License-Identifier: GPL-3.0-or-later
//
// Stabilizes clips optically: their motion measured from the picture (Gyroflow's own analysis, run here:
// vs_gyroflow::measure) and kept in each clip's <stem>.afp-motion, which the player plays with as it plays gyro, without
// a Gyroflow project (vs_gyroflow::optical says which clips). No Gyroflow app is needed.
//
//   gyroflow-optical [--redo] [--strength <%>] [--tracking klt|dis] <video>...   each in turn, how far on stderr
//   gyroflow-optical --check [--redo] <video>...                        what would be done, one JSON line each:
//                                                                       {"path", "stabilize": true, "lens", "replaces"}
//                                                                       or {"path", "stabilize": false, "why"}
// --redo: clips with their motion measured already measured again (optical::decide)
//   gyroflow-optical --lens-types <video>...                            the lens types its camera has (lens_pick.rs), one
//                                                                       JSON line each: {"path", "camera", "types":
//                                                                       [[id, label]], "type" (chosen), "recorded",
//                                                                       "default", "mode" (the camera's number)}, for
//                                                                       the picker
//   gyroflow-optical --batch <request.json>                             Action Footage Library's "Stabilize optically"
//   gyroflow-optical --from-project [--force] <video>...                the motion a Gyroflow project beside each clip
//                                                                       measured from the picture, kept in the clip's own
//                                                                       <stem>.afp-motion (afp_motion.rs), so it plays
//                                                                       with it once the project is gone; one JSON line
//                                                                       each: {"path", "written", "lens", "in_water",
//                                                                       "frames"} or {"path", "error"}. --force: over a
//                                                                       motion file the clip has already
//   gyroflow-optical --show <video>...                                  the header of each clip's .afp-motion, one JSON
//                                                                       line each ({"path", "error"} without one)
//
// The batch's request: {"clips": [paths], "redo": false, "status": <file>, "stop": <file>, "strength": <%, 100>,
// "tracking": "klt"}.
// It writes how far it is to "status" as the player's batches do (the Library's Tasks tab reads it): {"state":
// "looking"|"running"|"done"|"stopped"|"failed", "done", "total" (the clips measured, not the ones left out),
// "failed", "made", "fraction" (of the clip being measured), "current" (its name), "message", "failures": [[name,
// why]], "left_out": [[name, why]], "made_paths": [the clips measured, for a run again to leave out]}, and stops
// once the "stop" file appears. The player measures the clip it plays with it too (its "measure motion").

use std::path::{ Path, PathBuf };
use std::time::Instant;
use serde_json::{ json, Value };
use vs_gyroflow::optical::{ self, Verdict };
use vs_gyroflow::measure::{ self, Settings };

fn name_of(p: &Path) -> String { p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| p.display().to_string()) }

struct Status {
    file: Option<PathBuf>,
    v: Value,
}

impl Status {
    fn write(&self) {
        let Some(f) = &self.file else { return };
        let tmp = f.with_extension("tmp");
        if std::fs::write(&tmp, self.v.to_string()).is_ok() { let _ = std::fs::rename(&tmp, f); }
    }
    fn set(&mut self, key: &str, value: Value) { self.v[key] = value; }
}

fn run(clips: Vec<PathBuf>, redo: bool, settings: Settings, status_file: Option<PathBuf>, stop_file: Option<PathBuf>) -> i32 {
    // Absolute: the clips are read by their URL
    let clips: Vec<PathBuf> = clips.into_iter().map(|c| std::path::absolute(&c).unwrap_or(c)).collect();
    let stopped = || stop_file.as_ref().is_some_and(|f| f.exists());
    let mut st = Status { file: status_file, v: json!({ "state": "looking", "done": 0, "total": 0, "failed": 0, "made": 0,
                                                         "failures": [], "left_out": [] }) };
    st.write();
    // Which clips: each one's metadata read (a second or so from a share)
    let mut todo: Vec<PathBuf> = Vec::new();
    let mut left_out: Vec<Value> = Vec::new();
    for c in &clips {
        if stopped() { st.set("state", json!("stopped")); st.write(); return 0; }
        st.set("current", json!(name_of(c)));
        st.write();
        match optical::verdict(c, redo) {
            Verdict::Stabilize { lens, .. } => { eprintln!("{}: lens {lens}", c.display()); todo.push(c.clone()); }
            Verdict::LeaveOut(why) => { eprintln!("{}: left out, {why}", c.display()); left_out.push(json!([c.display().to_string(), why])); }
        }
    }
    st.set("left_out", json!(left_out));
    st.set("total", json!(todo.len()));
    st.set("current", Value::Null);
    if todo.is_empty() {
        st.set("state", json!("done"));
        st.set("message", json!(format!("Nothing to stabilize: {} left out", left_out.len())));
        st.write();
        return 0;
    }
    st.set("state", json!("running"));
    st.write();
    let mut failures: Vec<Value> = Vec::new();
    let (mut made, mut done) = (0, 0);
    let mut made_paths: Vec<Value> = Vec::new();
    for clip in &todo {
        st.set("current", json!(name_of(clip)));
        st.set("fraction", json!(0.0));
        st.write();
        let started = Instant::now();
        let mut last = Instant::now();
        let result = measure::measure_into_file(clip, &settings, |p| {
            st.set("fraction", json!(p));
            if last.elapsed().as_millis() >= 400 { st.write(); last = Instant::now(); }
            if st.file.is_none() { eprint!("\r{}: {:.0}%   ", name_of(clip), p * 100.0); }
        }, &stopped).map(|_| vs_gyroflow::afp_motion::path_for(clip));
        let secs = started.elapsed().as_secs_f64();
        match result {
            Ok(p) => {
                made += 1;
                made_paths.push(json!(clip.display().to_string()));
                st.set("made_paths", json!(made_paths));
                eprintln!("\r{}: {} in {secs:.0} s", clip.display(), p.display());
            }
            Err(e) if e == "cancelled" => {
                st.set("state", json!("stopped"));
                st.set("message", json!(if made == 0 { "Stopped before one was measured".to_string() } else { format!("{made} measured before it stopped") }));
                st.write();
                return 0;
            }
            Err(e) => { eprintln!("\r{}: {e}", clip.display()); failures.push(json!([clip.display().to_string(), e])); }
        }
        done += 1;
        st.set("done", json!(done));
        st.set("made", json!(made));
        st.set("failed", json!(failures.len()));
        st.set("failures", json!(failures));
        st.set("fraction", Value::Null);
        st.write();
    }
    let mut said = vec![format!("{made} stabilized")];
    if !left_out.is_empty() { said.push(format!("{} left out", left_out.len())); }
    st.set("state", json!("done"));
    st.set("current", Value::Null);
    st.set("message", json!(said.join(" · ")));
    st.write();
    i32::from(!failures.is_empty())
}

/// The motion the Gyroflow project beside `video` measured from the picture, written to its .afp-motion
fn motion_from_project(video: &Path, force: bool) -> Result<Value, String> {
    use vs_gyroflow::afp_motion;
    if !video.is_file() { return Err("not found".into()); }
    let project = optical::own_project(video).ok_or("no Gyroflow project beside it")?;
    let out = afp_motion::path_for(video);
    if out.exists() && !force { return Err(format!("{} is there already (--force writes over it)", name_of(&out))); }
    let doc: Value = serde_json::from_slice(&std::fs::read(&project).map_err(|e| format!("{}: {e}", project.display()))?)
        .map_err(|e| format!("{}: {e}", project.display()))?;
    let m = afp_motion::from_project(&doc, &project, video)?;
    m.write(&out)?;
    let h = &m.header;
    Ok(json!({ "written": out.display().to_string(), "lens": h.lens_name, "in_water": h.in_water(), "readout_ms": h.readout_ms,
               "frames": h.measured.frames, "measured_frames": h.measured.measured_frames }))
}

fn main() {
    let _ = simplelog::TermLogger::init(log::LevelFilter::Error, simplelog::Config::default(), simplelog::TerminalMode::Stderr, simplelog::ColorChoice::Never);
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    let usage = || -> ! {
        eprintln!("usage: gyroflow-optical [--redo] [--strength <%>] [--tracking klt|dis] <video>...\n       gyroflow-optical --check [--redo] <video>...\n       gyroflow-optical --lens-types <video>...\n       gyroflow-optical --batch <request.json>\n       gyroflow-optical --from-project [--force] <video>...\n       gyroflow-optical --show <video>...");
        std::process::exit(2);
    };
    if args.is_empty() { usage(); }
    match args[0].as_str() {
        "--check" => {
            let redo = args.get(1).is_some_and(|a| a == "--redo");
            for a in &args[if redo { 2 } else { 1 }..] {
                let p = std::path::absolute(a).unwrap_or_else(|_| PathBuf::from(a));
                let line = match optical::verdict(&p, redo) {
                    Verdict::Stabilize { lens, replaces, .. } => json!({ "path": a, "stabilize": true, "lens": lens, "replaces": replaces }),
                    Verdict::LeaveOut(why) => json!({ "path": a, "stabilize": false, "why": why }),
                };
                println!("{line}");
            }
        }
        "--from-project" => {
            let force = args.get(1).is_some_and(|a| a == "--force");
            let mut failed = false;
            for a in &args[if force { 2 } else { 1 }..] {
                let p = std::path::absolute(a).unwrap_or_else(|_| PathBuf::from(a));
                let line = match motion_from_project(&p, force) {
                    Ok(v) => { let mut v = v; v["path"] = json!(a); v }
                    Err(e) => { failed = true; json!({ "path": a, "error": e }) }
                };
                println!("{line}");
            }
            std::process::exit(if failed { 1 } else { 0 });
        }
        "--show" => {
            for a in &args[1..] {
                let p = std::path::absolute(a).unwrap_or_else(|_| PathBuf::from(a));
                let line = match vs_gyroflow::afp_motion::header_for(&p) {
                    Some(Ok(h)) => { let mut v = serde_json::to_value(h).unwrap_or_default(); v["path"] = json!(a); v }
                    Some(Err(e)) => json!({ "path": a, "error": e }),
                    None => json!({ "path": a, "error": "no .afp-motion beside it" }),
                };
                println!("{line}");
            }
        }
        "--lens-types" => {
            for a in &args[1..] {
                let p = std::path::absolute(a).unwrap_or_else(|_| PathBuf::from(a));
                let line = match optical::lens_types(&p) {
                    Ok(l) => json!({ "path": a, "camera": l.camera, "types": l.types, "type": l.chosen, "recorded": l.recorded, "default": l.default, "mode": l.mode }),
                    Err(e) => json!({ "path": a, "error": e, "types": [] }),
                };
                println!("{line}");
            }
        }
        "--batch" => {
            let Some(req_file) = args.get(1) else { usage() };
            let req: Value = match std::fs::read_to_string(req_file).ok().and_then(|t| serde_json::from_str(&t).ok()) {
                Some(v) => v,
                None => { eprintln!("{req_file}: not a request"); std::process::exit(2); }
            };
            let clips: Vec<PathBuf> = req["clips"].as_array().into_iter().flatten().filter_map(Value::as_str).map(PathBuf::from).collect();
            let settings = Settings {
                strength: req["strength"].as_f64().map(|s| s / 100.0).unwrap_or(1.0),
                tracking: req["tracking"].as_str().filter(|t| ["klt", "dis"].contains(t)).unwrap_or("klt").to_string(),
                ..Default::default()
            };
            let path = |k: &str| req[k].as_str().map(PathBuf::from);
            std::process::exit(run(clips, req["redo"].as_bool().unwrap_or(false), settings, path("status"), path("stop")));
        }
        _ => {
            let mut settings = Settings::default();
            let mut redo = false;
            while args.first().is_some_and(|a| a.starts_with("--")) {
                let flag = args.remove(0);
                if flag == "--redo" { redo = true; continue; }
                let Some(v) = (!args.is_empty()).then(|| args.remove(0)) else { usage() };
                match flag.as_str() {
                    "--strength" => settings.strength = v.parse::<f64>().unwrap_or_else(|_| usage()) / 100.0,
                    "--tracking" if v == "klt" || v == "dis" => settings.tracking = v,
                    _ => usage(),
                }
            }
            if args.is_empty() { usage(); }
            std::process::exit(run(args.into_iter().map(PathBuf::from).collect(), redo, settings, None, None));
        }
    }
}
