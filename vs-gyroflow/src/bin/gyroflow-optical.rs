// SPDX-License-Identifier: GPL-3.0-or-later
//
// Stabilizes clips optically: Gyroflow measures the camera's motion from the picture, and the project it writes goes
// beside each clip, where the player plays with it (vs_gyroflow::optical says which clips, and how).
//
//   gyroflow-optical [--strength <%>] [--tracking klt|dis] <video>...   each in turn, how far on stderr
//   gyroflow-optical --check <video>...                                 what would be done, one JSON line each:
//                                                                       {"path", "stabilize": true, "lens"} or
//                                                                       {"path", "stabilize": false, "why"}
//   gyroflow-optical --batch <request.json>                             Action Footage Library's "Stabilize optically"
//
// The batch's request: {"clips": [paths], "status": <file>, "stop": <file>, "strength": <%, 100>, "tracking": "klt"}.
// It writes how far it is to "status" as the player's batches do (the Library's Tasks tab reads it): {"state":
// "looking"|"running"|"done"|"stopped"|"failed", "done", "total" (the clips stabilized, not the ones left out),
// "failed", "made", "fraction" (of the clip being analyzed), "current" (its name), "message", "failures": [[name,
// why]], "left_out": [[name, why]]}, and stops once the "stop" file appears. Gyroflow: AFP_GYROFLOW, else where it's
// installed (optical::gyroflow_cli).

use std::path::{ Path, PathBuf };
use std::time::Instant;
use serde_json::{ json, Value };
use vs_gyroflow::optical::{ self, Settings, Verdict };

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

fn run(clips: Vec<PathBuf>, settings: Settings, status_file: Option<PathBuf>, stop_file: Option<PathBuf>, work: PathBuf) -> i32 {
    // Absolute: Gyroflow runs in the work folder, and reads files by their URL
    let clips: Vec<PathBuf> = clips.into_iter().map(|c| std::path::absolute(&c).unwrap_or(c)).collect();
    let stopped = || stop_file.as_ref().is_some_and(|f| f.exists());
    let mut st = Status { file: status_file, v: json!({ "state": "looking", "done": 0, "total": 0, "failed": 0, "made": 0,
                                                         "failures": [], "left_out": [] }) };
    st.write();
    // Which clips: each one's metadata read (a second or so from a share)
    let mut todo: Vec<(PathBuf, Option<String>)> = Vec::new();
    let mut left_out: Vec<Value> = Vec::new();
    for c in &clips {
        if stopped() { st.set("state", json!("stopped")); st.write(); return 0; }
        st.set("current", json!(name_of(c)));
        st.write();
        match optical::verdict(c) {
            Verdict::Stabilize { lens, profile } => { eprintln!("{}: lens {lens}", c.display()); todo.push((c.clone(), profile)); }
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
    let gyroflow = match optical::gyroflow_cli() {
        Ok(g) => g,
        Err(e) => { eprintln!("{e}"); st.set("state", json!("failed")); st.set("message", json!(e)); st.write(); return 1; }
    };
    st.set("state", json!("running"));
    st.write();
    let mut failures: Vec<Value> = Vec::new();
    let (mut made, mut done) = (0, 0);
    for (clip, profile) in &todo {
        st.set("current", json!(name_of(clip)));
        st.set("fraction", json!(0.0));
        st.write();
        let started = Instant::now();
        let mut last = Instant::now();
        let result = optical::stabilize(&gyroflow, clip, profile.as_deref(), &settings, &work, &stopped, &mut |p| {
            st.set("fraction", json!(p));
            if last.elapsed().as_millis() >= 400 { st.write(); last = Instant::now(); }
            if st.file.is_none() { eprint!("\r{}: {:.0}%   ", name_of(clip), p * 100.0); }
        });
        let secs = started.elapsed().as_secs_f64();
        match result {
            Ok(p) => { made += 1; eprintln!("\r{}: {} in {secs:.0} s", clip.display(), p.display()); }
            Err(e) if e == "stopped" => {
                st.set("state", json!("stopped"));
                st.set("message", json!(format!("{made} stabilized before it stopped")));
                st.write();
                let _ = std::fs::remove_dir_all(&work);
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
    let _ = std::fs::remove_dir_all(&work);
    let mut said = vec![format!("{made} stabilized")];
    if !left_out.is_empty() { said.push(format!("{} left out", left_out.len())); }
    st.set("state", json!("done"));
    st.set("current", Value::Null);
    st.set("message", json!(said.join(" · ")));
    st.write();
    i32::from(!failures.is_empty())
}

fn main() {
    let _ = simplelog::TermLogger::init(log::LevelFilter::Error, simplelog::Config::default(), simplelog::TerminalMode::Stderr, simplelog::ColorChoice::Never);
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    let usage = || -> ! {
        eprintln!("usage: gyroflow-optical [--strength <%>] [--tracking klt|dis] <video>...\n       gyroflow-optical --check <video>...\n       gyroflow-optical --batch <request.json>");
        std::process::exit(2);
    };
    if args.is_empty() { usage(); }
    let work_default = std::env::temp_dir().join(format!("gyroflow-optical-{}", std::process::id()));
    match args[0].as_str() {
        "--check" => {
            for a in &args[1..] {
                let p = std::path::absolute(a).unwrap_or_else(|_| PathBuf::from(a));
                let line = match optical::verdict(&p) {
                    Verdict::Stabilize { lens, .. } => json!({ "path": a, "stabilize": true, "lens": lens }),
                    Verdict::LeaveOut(why) => json!({ "path": a, "stabilize": false, "why": why }),
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
            };
            let path = |k: &str| req[k].as_str().map(PathBuf::from);
            let work = path("status").and_then(|s| s.parent().map(|d| d.join("work"))).unwrap_or(work_default);
            std::process::exit(run(clips, settings, path("status"), path("stop"), work));
        }
        _ => {
            let mut settings = Settings::default();
            while args.first().is_some_and(|a| a.starts_with("--")) {
                let flag = args.remove(0);
                let Some(v) = (!args.is_empty()).then(|| args.remove(0)) else { usage() };
                match flag.as_str() {
                    "--strength" => settings.strength = v.parse::<f64>().unwrap_or_else(|_| usage()) / 100.0,
                    "--tracking" if v == "klt" || v == "dis" => settings.tracking = v,
                    _ => usage(),
                }
            }
            if args.is_empty() { usage(); }
            std::process::exit(run(args.into_iter().map(PathBuf::from).collect(), settings, None, None, work_default));
        }
    }
}
