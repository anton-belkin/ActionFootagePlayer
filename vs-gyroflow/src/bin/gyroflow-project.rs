// SPDX-License-Identifier: GPL-3.0-or-later
//
// Writes the Gyroflow project the plugin plays a video with when it has none next to it (its own motion data,
// Gyroflow's default preset, then --preset), so the video opens in Gyroflow as it plays: Ctrl+e in mpv runs it.
//
//   gyroflow-project [--preset <preset.gyroflow>] [--underwater yes|no|auto|project] [--horizon-lock <%>]
//                    [--level weak|medium|strong] [--wide] <video> [project.gyroflow]
//
// For a moment of the player's (docs/moments-and-workspaces.md), a project of its own:
//   --trim <in>,<out>   its part (seconds of the video) as the project's trim range, which Gyroflow opens at
//   --alone yes|no      smoothed within it only (Gyroflow's trim_range_only), or as part of the whole video
//   --copy <project>    a copy of the video's own project (its settings, sync, lens) instead of a new one
//   --edit              <project.gyroflow> alone, changed in place: --trim, --alone, --horizon-lock (the moment's
//                       ends, its switch, its horizon kept)
//   --print-trim        <project.gyroflow>: its first trim range, "<in>,<out>" in seconds (nothing without one): a
//                       moment's ends changed in Gyroflow, which the player takes
//
// The project goes next to the video by default, and an existing one is never overwritten. With the lens under water
// as --underwater says (default auto: as the camera says, where it records it; project: as the preset says), the horizon lock's amount (0-100,
// 0 off) instead of the preset's, at the level it plays in (--level, default medium: its crop and smoothness), and
// with --wide in the plugin's wider view: no zoom, at its FOV. The motion data goes into the project.

use std::path::{ Path, PathBuf };
use vs_gyroflow::{ project as motion_project, Level, Underwater };

fn main() {
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    let usage = || -> ! {
        eprintln!("usage: gyroflow-project [--preset <preset.gyroflow>] [--underwater yes|no|auto|project] [--horizon-lock <%>] [--level weak|medium|strong] [--wide] [--trim <in>,<out>] [--alone yes|no] [--copy <project>] <video> [project.gyroflow]\n       gyroflow-project --edit [--trim <in>,<out>] [--alone yes|no] <project.gyroflow>");
        std::process::exit(2);
    };
    let mut value = |name: &str| -> Option<String> {
        let i = args.iter().position(|a| a == name)?;
        let v = (i + 1 < args.len()).then(|| args.remove(i + 1));
        args.remove(i);
        Some(v.unwrap_or_else(|| usage()))
    };
    let preset = value("--preset");
    let underwater = value("--underwater").map(|v| Underwater::parse(&v).unwrap_or_else(|| {
        eprintln!("--underwater takes yes, no, auto or project");
        std::process::exit(2);
    })).unwrap_or(Underwater::Auto);
    let horizon_lock = value("--horizon-lock").map(|v| v.parse::<f64>().ok().filter(|v| (0.0..=100.0).contains(v)).unwrap_or_else(|| {
        eprintln!("--horizon-lock takes 0-100");
        std::process::exit(2);
    }));
    let level = value("--level").map(|v| Level::parse(&v).unwrap_or_else(|| {
        eprintln!("--level takes weak, medium or strong");
        std::process::exit(2);
    })).unwrap_or(Level::Medium);
    let trim = value("--trim").map(|v| {
        let t: Vec<f64> = v.split(',').filter_map(|x| x.trim().parse().ok()).collect();
        if t.len() != 2 || t[1] <= t[0] || t[0] < 0.0 { eprintln!("--trim takes <in>,<out>, seconds"); std::process::exit(2); }
        (t[0], t[1])
    });
    let alone = value("--alone").map(|v| match v.as_str() {
        "yes" => true, "no" => false,
        _ => { eprintln!("--alone takes yes or no"); std::process::exit(2); }
    });
    let copy = value("--copy");
    let mut flag = |name: &str| match args.iter().position(|a| a == name) {
        Some(i) => { args.remove(i); true },
        None => false,
    };
    let wide = flag("--wide");
    let edit = flag("--edit");
    if flag("--print-trim") {
        if args.len() != 1 { usage(); }
        let json: Option<serde_json::Value> = std::fs::read_to_string(&args[0]).ok().and_then(|t| serde_json::from_str(&t).ok());
        let range = json.as_ref().and_then(|j| j.get("trim_ranges_ms")?.as_array()?.first()?.as_array().cloned());
        if let Some(r) = range.filter(|r| r.len() == 2) {
            if let (Some(a), Some(b)) = (r[0].as_f64(), r[1].as_f64()) {
                if b > a && a >= 0.0 { println!("{:.3},{:.3}", a / 1000.0, b / 1000.0); }
            }
        }
        return;
    }
    if edit {
        if args.len() != 1 || args[0].starts_with("--") { usage(); }
        let project = PathBuf::from(&args[0]);
        let result = std::fs::read_to_string(&project).map_err(|e| format!("{}: {e}", project.display()))
            .and_then(|text| adjust(&text, trim, alone, horizon_lock))
            .and_then(|text| write_whole(&project, &text));
        if let Err(e) = result {
            eprintln!("gyroflow-project: {e}");
            std::process::exit(1);
        }
        return;
    }
    if args.is_empty() || args.len() > 2 || args.iter().any(|a| a.starts_with("--")) {
        usage();
    }
    let video = PathBuf::from(&args[0]);
    let project = args.get(1).map(PathBuf::from).unwrap_or_else(|| video.with_extension("gyroflow"));
    let result = match &copy {
        // The video's own project, copied with the moment's trim: it then lives apart from it
        Some(src) => if project.exists() { Err(format!("{} already exists", project.display())) } else {
            std::fs::read_to_string(src).map_err(|e| format!("{src}: {e}"))
                .and_then(|text| adjust(&text, trim, alone, horizon_lock))
                .and_then(|text| write_whole(&project, &text))
        },
        None => run(&video, &project, preset.as_deref(), underwater, horizon_lock, level, wide, trim, alone),
    };
    if let Err(e) = result {
        eprintln!("gyroflow-project: {e}");
        std::process::exit(1);
    }
}

// A project's JSON with a trim range (seconds of the video), "only within it" and the horizon lock's amount set; the
// rest as it is
fn adjust(text: &str, trim: Option<(f64, f64)>, alone: Option<bool>, horizon_lock: Option<f64>) -> Result<String, String> {
    if trim.is_none() && alone.is_none() && horizon_lock.is_none() { return Ok(text.to_string()); }
    let mut json: serde_json::Value = serde_json::from_str(text).map_err(|e| format!("not a Gyroflow project: {e}"))?;
    let obj = json.as_object_mut().ok_or("not a Gyroflow project")?;
    if let Some((a, b)) = trim {
        obj.remove("trim_start");
        obj.remove("trim_end");
        obj.remove("trim_ranges");
        obj.insert("trim_ranges_ms".into(), serde_json::json!([[(a * 1000.0).round(), (b * 1000.0).round()]]));
    }
    if let Some(amount) = horizon_lock {
        let stab = obj.entry("stabilization").or_insert_with(|| serde_json::json!({}));
        stab.as_object_mut().ok_or("a broken stabilization")?.insert("horizon_lock_amount".into(), serde_json::json!(amount));
    }
    if let Some(alone) = alone {
        let stab = obj.entry("stabilization").or_insert_with(|| serde_json::json!({}));
        let params = stab.as_object_mut().ok_or("a broken stabilization")?.entry("smoothing_params").or_insert_with(|| serde_json::json!([]));
        let list = params.as_array_mut().ok_or("broken smoothing_params")?;
        let value = if alone { 1.0 } else { 0.0 };
        match list.iter_mut().find(|p| p.get("name").and_then(|n| n.as_str()) == Some("trim_range_only")) {
            Some(p) => { p["value"] = serde_json::json!(value); }
            None => list.push(serde_json::json!({ "name": "trim_range_only", "value": value })),
        }
    }
    serde_json::to_string_pretty(&json).map_err(|e| e.to_string())
}

// Written next to it, then renamed over it
fn write_whole(path: &Path, text: &str) -> Result<(), String> {
    let tmp = PathBuf::from(format!("{}.tmp", path.display()));
    std::fs::write(&tmp, text).map_err(|e| format!("{}: {e}", tmp.display()))?;
    std::fs::rename(&tmp, path).map_err(|e| format!("{}: {e}", path.display()))?;
    println!("Wrote {}", path.display());
    Ok(())
}

fn run(video: &Path, project: &Path, preset: Option<&str>, underwater: Underwater, horizon_lock: Option<f64>, level: Level, wide: bool,
       trim: Option<(f64, f64)>, alone: Option<bool>) -> Result<(), String> {
    if project.exists() {
        return Err(format!("{} already exists", project.display()));
    }
    let video = vs_gyroflow::canonicalize(video).map_err(|e| format!("{}: {e}", video.display()))?;
    let project_path = std::path::absolute(project).map_err(|e| format!("{}: {e}", project.display()))?;
    let preset = preset.map(|p| vs_gyroflow::canonicalize(p).map_err(|e| format!("{p}: {e}"))).transpose()?;
    let data = motion_project::from_motion_data(&video.to_string_lossy(), preset.as_ref().map(|p| p.to_string_lossy()).as_deref(), underwater, horizon_lock, level, wide, &project_path.to_string_lossy());
    // On macOS the core makes the project file, empty, to bookmark the video relative to it
    if data.is_err() && std::fs::metadata(project).is_ok_and(|m| m.len() == 0) {
        let _ = std::fs::remove_file(project);
    }
    let data = adjust(&data?, trim, alone, None)?;
    std::fs::write(project, data).map_err(|e| format!("{}: {e}", project.display()))?;
    println!("Wrote {}", project.display());
    Ok(())
}
