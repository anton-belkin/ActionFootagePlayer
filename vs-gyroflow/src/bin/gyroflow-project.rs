// SPDX-License-Identifier: GPL-3.0-or-later
//
// Writes the Gyroflow project the plugin plays a video with when it has none next to it (its own motion data,
// Gyroflow's default preset, then --preset), so the video opens in Gyroflow as it plays: Ctrl+e in mpv runs it.
//
//   gyroflow-project [--preset <preset.gyroflow>] [--underwater yes|no|auto] [--horizon-lock <%>] [--wide]
//                    <video> [project.gyroflow]
//
// The project goes next to the video by default, and an existing one is never overwritten. With the lens under water
// as --underwater says (default auto: as the camera says, where it records it), the horizon lock's amount (0-100,
// 0 off) instead of the preset's, and with --wide in the plugin's wider view: no zoom, at its FOV.

use std::path::{ Path, PathBuf };
use vs_gyroflow::{ project as motion_project, Underwater };

fn main() {
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    let usage = || -> ! {
        eprintln!("usage: gyroflow-project [--preset <preset.gyroflow>] [--underwater yes|no|auto] [--horizon-lock <%>] [--wide] <video> [project.gyroflow]");
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
        eprintln!("--underwater takes yes, no or auto");
        std::process::exit(2);
    })).unwrap_or(Underwater::Auto);
    let horizon_lock = value("--horizon-lock").map(|v| v.parse::<f64>().ok().filter(|v| (0.0..=100.0).contains(v)).unwrap_or_else(|| {
        eprintln!("--horizon-lock takes 0-100");
        std::process::exit(2);
    }));
    let wide = match args.iter().position(|a| a == "--wide") {
        Some(i) => { args.remove(i); true },
        None => false,
    };
    if args.is_empty() || args.len() > 2 || args.iter().any(|a| a.starts_with("--")) {
        usage();
    }
    let video = PathBuf::from(&args[0]);
    let project = args.get(1).map(PathBuf::from).unwrap_or_else(|| video.with_extension("gyroflow"));
    if let Err(e) = run(&video, &project, preset.as_deref(), underwater, horizon_lock, wide) {
        eprintln!("gyroflow-project: {e}");
        std::process::exit(1);
    }
}

fn run(video: &Path, project: &Path, preset: Option<&str>, underwater: Underwater, horizon_lock: Option<f64>, wide: bool) -> Result<(), String> {
    if project.exists() {
        return Err(format!("{} already exists", project.display()));
    }
    let video = vs_gyroflow::canonicalize(video).map_err(|e| format!("{}: {e}", video.display()))?;
    let project_path = std::path::absolute(project).map_err(|e| format!("{}: {e}", project.display()))?;
    let preset = preset.map(|p| vs_gyroflow::canonicalize(p).map_err(|e| format!("{p}: {e}"))).transpose()?;
    let data = motion_project::from_motion_data(&video.to_string_lossy(), preset.as_ref().map(|p| p.to_string_lossy()).as_deref(), underwater, horizon_lock, wide, &project_path.to_string_lossy());
    // On macOS the core makes the project file, empty, to bookmark the video relative to it
    if data.is_err() && std::fs::metadata(project).is_ok_and(|m| m.len() == 0) {
        let _ = std::fs::remove_file(project);
    }
    std::fs::write(project, data?).map_err(|e| format!("{}: {e}", project.display()))?;
    println!("Wrote {}", project.display());
    Ok(())
}
