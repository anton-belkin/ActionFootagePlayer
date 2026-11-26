// SPDX-License-Identifier: GPL-3.0-or-later
//
// Stabilizes clips optically: their motion measured from the picture (Gyroflow's own analysis, run here:
// vs_gyroflow::measure) and kept in each clip's <stem>.afp-motion, which the player plays with as it plays gyro, without
// a Gyroflow project (vs_gyroflow::optical says which clips). No Gyroflow app is needed.
//
//   gyroflow-optical [--redo] [--strength <%>] [--tracking klt|dis] [--underwater yes|no] <video>...   each in turn, how
//                                                                       far on stderr. --underwater: the lens under water
//                                                                       or not, whatever the clip's own file or camera
//                                                                       says (a HERO7 says nothing)
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
//   gyroflow-optical --lenses <video>                                    the lenses to choose from for its mode, with what Find and
//                                                                         Estimate said of each on it (the lens dialogs)
//   gyroflow-optical --try-lens <video> keep <n> | assign <id> | forget   a lens tried on this clip alone (source.lens in its
//                                                                         own file, which wins over its mode's), or taken away
//   gyroflow-optical --lens-for <video> keep <n> | assign <id>          that lens as {name, how, from, profile} (the Library
//                                                                         writes it into the clip's own file itself)
//   gyroflow-optical --promote-lens <video>                              the lens tried on it made its whole mode's
//   gyroflow-optical --find-lens [--quick] <video>...                             each clip's lens found among the known ones of its
//                                                                       camera (vs_gyroflow::lens_find::find), one JSON
//                                                                       line each: lens_find::Kept, or {"path", "error"}
//   gyroflow-optical --estimate-lens <video>                            a lens and its readout estimated from the clip
//                                                                       (lens_find::estimate): lens_find::Kept
//   gyroflow-optical --keep-lens <video> <n>                            one of what was found for it ("0".., "estimate")
//                                                                       kept for its whole mode in the player's own lens
//                                                                       database: {"file", "mode", "lens"}
//
// The batch's request: {"clips": [paths], "redo": false, "status": <file>, "stop": <file>, "strength": <%, 100>,
// "tracking": "klt", "action": "measure" (the default) | "find-lens" | "estimate-lens", "quick": Find stopped at the first
// lens that fits}. Find lens and Estimate lens
// write what they found for each clip to the status's "results" ([lens_find::Kept with "path"], or {"path", "error"}).
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

/// What was found, without the profiles themselves (kept in the cache for --keep-lens): what's said of it
fn without_profiles(v: &mut Value) {
    if let Some(t) = v.get_mut("tried").and_then(Value::as_array_mut) { for x in t { if let Some(o) = x.as_object_mut() { o.remove("profile"); } } }
    if let Some(o) = v.get_mut("estimate").and_then(Value::as_object_mut) { o.remove("profile"); }
}

/// Find lens / Estimate lens on the Library's selection, how far on the status as Stabilize optically writes it, what
/// was found in its "results"
fn lens_batch(clips: Vec<PathBuf>, estimate: bool, quick: bool, status_file: Option<PathBuf>, stop_file: Option<PathBuf>) -> i32 {
    let clips: Vec<PathBuf> = clips.into_iter().map(|c| std::path::absolute(&c).unwrap_or(c)).collect();
    let stopped = || stop_file.as_ref().is_some_and(|f| f.exists());
    let mut st = Status { file: status_file, v: json!({ "state": "running", "done": 0, "total": clips.len(), "failed": 0, "made": 0,
                                                         "failures": [], "left_out": [], "results": [] }) };
    st.write();
    let (mut results, mut failures, mut found): (Vec<Value>, Vec<Value>, usize) = (Vec::new(), Vec::new(), 0);
    for (i, c) in clips.iter().enumerate() {
        if stopped() { st.set("state", json!("stopped")); st.write(); return 0; }
        st.set("current", json!(name_of(c)));
        st.set("fraction", json!(0.0));
        st.write();
        let last = std::sync::Mutex::new(Instant::now());
        let status = std::sync::Mutex::new(&mut st);
        let progress = |p: f64| {
            let mut l = last.lock().unwrap();
            if l.elapsed().as_millis() >= 400 { let mut s = status.lock().unwrap(); s.set("fraction", json!(p)); s.write(); *l = Instant::now(); }
        };
        let stop = || stopped();
        // What's found so far, as each lens is tried: this clip's partial result after the finished ones
        let done_so_far = results.clone();
        let partial = |k: &vs_gyroflow::lens_find::Kept| {
            let mut v = serde_json::to_value(k).unwrap_or_default();
            without_profiles(&mut v);
            v["path"] = json!(c.display().to_string());
            v["partial"] = json!(true);
            let mut all = done_so_far.clone();
            all.push(v);
            let mut s = status.lock().unwrap();
            s.set("results", json!(all));
            s.write();
        };
        let r = if estimate { vs_gyroflow::lens_find::estimate(c, &progress, &stop) } else { vs_gyroflow::lens_find::find(c, quick, &progress, &stop, &partial) };
        drop(status);
        match r {
            Ok(k) => {
                if estimate && k.estimate.is_some() || !estimate && k.accepted.is_some() { found += 1; }
                let mut v = serde_json::to_value(k).unwrap_or_default();
                without_profiles(&mut v);
                v["path"] = json!(c.display().to_string());
                results.push(v);
            }
            Err(e) if e == "cancelled" => { st.set("state", json!("stopped")); st.write(); return 0; }
            Err(e) => { failures.push(json!([c.display().to_string(), e.clone()])); results.push(json!({ "path": c.display().to_string(), "error": e })); }
        }
        st.set("done", json!(i + 1));
        st.set("made", json!(found));
        st.set("failed", json!(failures.len()));
        st.set("failures", json!(failures));
        st.set("results", json!(results));
        st.set("fraction", Value::Null);
        st.write();
    }
    st.set("state", json!("done"));
    st.set("current", Value::Null);
    st.set("message", json!(if estimate { format!("{found} estimated") } else { format!("a lens fits {found} of {}", clips.len()) }));
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
        eprintln!("usage: gyroflow-optical [--redo] [--strength <%>] [--tracking klt|dis] [--underwater yes|no] <video>...\n       gyroflow-optical --check [--redo] <video>...\n       gyroflow-optical --lens-types <video>...\n       gyroflow-optical --batch <request.json>\n       gyroflow-optical --from-project [--force] <video>...\n       gyroflow-optical --show <video>...\n       gyroflow-optical --find-lens [--quick] <video>...\n       gyroflow-optical --lenses <video>\n       gyroflow-optical --try-lens <video> keep <n> | assign <id> | forget\n       gyroflow-optical --lens-for <video> keep <n> | assign <id>\n       gyroflow-optical --promote-lens <video>\n       gyroflow-optical --estimate-lens <video>\n       gyroflow-optical --keep-lens <video> <n>");
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
        "--find-lens" | "--estimate-lens" => {
            let estimate = args[0] == "--estimate-lens";
            // --quick: stopped at the first lens that fits (the Library's and the player's default)
            let quick = args.iter().any(|a| a == "--quick");
            let mut failed = false;
            for a in args[1..].iter().filter(|a| *a != "--quick") {
                let p = std::path::absolute(a).unwrap_or_else(|_| PathBuf::from(a));
                let r = if estimate { vs_gyroflow::lens_find::estimate(&p, &|_| (), &|| false) } else { vs_gyroflow::lens_find::find(&p, quick, &|_| (), &|| false, &|_| ()) };
                let line = match r {
                    Ok(k) => { let mut v = serde_json::to_value(k).unwrap_or_default(); without_profiles(&mut v); v["path"] = json!(a); v }
                    Err(e) => { failed = true; json!({ "path": a, "error": e }) }
                };
                println!("{line}");
            }
            std::process::exit(i32::from(failed));
        }
        "--lenses" => {
            let Some(a) = args.get(1) else { usage() };
            let p = std::path::absolute(a).unwrap_or_else(|_| PathBuf::from(a));
            let out = vs_gyroflow::lens_find::choices(&p).unwrap_or_else(|e| json!({ "path": a, "error": e }));
            println!("{out}");
            std::process::exit(if out["error"].is_null() { 0 } else { 1 });
        }
        // A lens of the dialogs as its profile (the Library writes it into the clip's own file), tried on the clip alone
        // (written here: the player's), or the one tried made the mode's
        "--lens-for" | "--try-lens" => {
            let (Some(a), Some(how)) = (args.get(1), args.get(2)) else { usage() };
            let p = std::path::absolute(a).unwrap_or_else(|_| PathBuf::from(a));
            let out = if args[0] == "--try-lens" && how == "forget" {
                match vs_gyroflow::lens_pick::set_clip_lens(&p.to_string_lossy(), None) { Ok(f) => json!({ "file": f.display().to_string(), "forgotten": true }), Err(e) => json!({ "error": e }) }
            } else {
                let Some(what) = args.get(3) else { usage() };
                if args[0] == "--try-lens" {
                    match vs_gyroflow::lens_find::try_on_clip(&p, how, what) { Ok((f, name)) => json!({ "file": f.display().to_string(), "lens": name }), Err(e) => json!({ "error": e }) }
                } else {
                    match vs_gyroflow::lens_find::lens_to_use(&p, how, what) {
                        Ok((profile, name, how, from)) => {
                            let mut prof: Value = profile.get_json().ok().and_then(|j| serde_json::from_str(&j).ok()).unwrap_or_default();
                            if prof["calibrator_version"].as_str().unwrap_or_default().is_empty() { prof["calibrator_version"] = json!(format!("Action Footage Player, {how}")); }
                            json!({ "name": name, "how": how, "from": from, "profile": prof })
                        }
                        Err(e) => json!({ "error": e }),
                    }
                }
            };
            println!("{out}");
            std::process::exit(if out["error"].is_null() { 0 } else { 1 });
        }
        "--promote-lens" => {
            let Some(a) = args.get(1) else { usage() };
            let p = std::path::absolute(a).unwrap_or_else(|_| PathBuf::from(a));
            let out = match vs_gyroflow::lens_find::promote(&p) {
                Ok((file, mode, lens)) => json!({ "file": file.display().to_string(), "mode": mode, "lens": lens }),
                Err(e) => json!({ "error": e }),
            };
            println!("{out}");
            std::process::exit(if out["error"].is_null() { 0 } else { 1 });
        }
        "--keep-lens" => {
            let (Some(a), Some(n)) = (args.get(1), args.get(2)) else { usage() };
            let p = std::path::absolute(a).unwrap_or_else(|_| PathBuf::from(a));
            let out = match vs_gyroflow::lens_find::keep(&p, n) {
                Ok((file, mode, lens)) => json!({ "file": file.display().to_string(), "mode": mode, "lens": lens }),
                Err(e) => json!({ "error": e }),
            };
            println!("{out}");
            std::process::exit(if out["error"].is_null() { 0 } else { 1 });
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
            match req["action"].as_str().unwrap_or("measure") {
                "find-lens" => std::process::exit(lens_batch(clips, false, req["quick"].as_bool().unwrap_or(false), path("status"), path("stop"))),
                "estimate-lens" => std::process::exit(lens_batch(clips, true, false, path("status"), path("stop"))),
                _ => std::process::exit(run(clips, req["redo"].as_bool().unwrap_or(false), settings, path("status"), path("stop"))),
            }
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
                    "--underwater" if v == "yes" => settings.refraction = Some(vs_gyroflow::water::WATER_REFRACTION),
                    "--underwater" if v == "no" => settings.refraction = Some(1.0),
                    _ => usage(),
                }
            }
            if args.is_empty() { usage(); }
            std::process::exit(run(args.into_iter().map(PathBuf::from).collect(), redo, settings, None, None));
        }
    }
}
