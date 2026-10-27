// SPDX-License-Identifier: GPL-3.0-or-later
//
// Moves a Gyroflow project into the video it belongs to, so the clip carries it (see embedded.rs), and back out.
//
//   gyroflow-embed [--move] <video> [project.gyroflow]   store the project (default: the .gyroflow next to the video);
//                                                         --move deletes the .gyroflow once it's stored and read back
//   gyroflow-embed --extract [--preset <preset.gyroflow>] [--underwater yes|no|auto] <video> [project.gyroflow]
//                                                         write the stored project next to the video, to open it in Gyroflow.
//                                                         A video without one gets the project the plugin plays it with:
//                                                         its own motion data, Gyroflow's default preset, then --preset,
//                                                         with the lens under water as --underwater says (default auto:
//                                                         as the camera says, where it records it)
//   gyroflow-embed --remove <video>                      remove the stored project
//   gyroflow-embed --show <video>                        print the stored project

use std::path::{ Path, PathBuf };
use vs_gyroflow::{ embedded, project as motion_project, Underwater };

fn main() {
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    let mode = match args.first().map(String::as_str) {
        Some(m @ ("--move" | "--extract" | "--remove" | "--show")) => { let m = m.to_string(); args.remove(0); m },
        _ => String::new(),
    };
    let mut preset = None;
    let mut underwater = Underwater::Auto;
    if mode == "--extract" {
        if let Some(i) = args.iter().position(|a| a == "--preset") {
            if i + 1 < args.len() { preset = Some(args.remove(i + 1)); }
            args.remove(i);
        }
        if let Some(i) = args.iter().position(|a| a == "--underwater") {
            let value = (i + 1 < args.len()).then(|| args.remove(i + 1));
            args.remove(i);
            underwater = value.as_deref().and_then(Underwater::parse).unwrap_or_else(|| {
                eprintln!("--underwater takes yes, no or auto");
                std::process::exit(2);
            });
        }
    }
    if args.is_empty() || args.len() > 2 || args[0].starts_with("--") {
        eprintln!("usage: gyroflow-embed [--move|--extract [--preset <preset.gyroflow>] [--underwater yes|no|auto]|--remove|--show] <video> [project.gyroflow]");
        std::process::exit(2);
    }
    let video = PathBuf::from(&args[0]);
    let project = args.get(1).map(PathBuf::from).unwrap_or_else(|| video.with_extension("gyroflow"));
    if let Err(e) = run(&mode, &video, &project, preset.as_deref(), underwater) {
        eprintln!("gyroflow-embed: {e}");
        std::process::exit(1);
    }
}

fn run(mode: &str, video: &Path, project: &Path, preset: Option<&str>, underwater: Underwater) -> Result<(), String> {
    match mode {
        "" | "--move" => {
            let data = std::fs::read(project).map_err(|e| format!("{}: {e}", project.display()))?;
            let json: serde_json::Value = serde_json::from_slice(&data).map_err(|e| format!("{} is not a Gyroflow project: {e}", project.display()))?;
            if json.get("videofile").and_then(|x| x.as_str()).unwrap_or_default().is_empty() {
                return Err(format!("{} is a preset, not a project of a video", project.display()));
            }
            embedded::write(video, &data).map_err(|e| format!("{}: {e}", video.display()))?;
            if embedded::read_raw(video).map_err(|e| e.to_string())?.as_deref() != Some(&data[..]) {
                return Err(format!("{}: the stored project didn't read back the same", video.display()));
            }
            println!("Stored {} in {} ({} KB)", project.display(), video.display(), data.len() / 1024);
            if mode == "--move" {
                std::fs::remove_file(project).map_err(|e| format!("{}: {e}", project.display()))?;
                println!("Removed {}", project.display());
            }
        },
        "--extract" => {
            if project.exists() {
                return Err(format!("{} already exists", project.display()));
            }
            let data = match embedded::read(video).map_err(|e| format!("{}: {e}", video.display()))? {
                Some(obj) => serde_json::to_string_pretty(&obj).unwrap(),
                None => {
                    let video = vs_gyroflow::canonicalize(video).map_err(|e| format!("{}: {e}", video.display()))?;
                    let project_path = std::path::absolute(project).map_err(|e| format!("{}: {e}", project.display()))?;
                    let preset = preset.map(|p| vs_gyroflow::canonicalize(p).map_err(|e| format!("{p}: {e}"))).transpose()?;
                    let data = motion_project::from_motion_data(&video.to_string_lossy(), preset.as_ref().map(|p| p.to_string_lossy()).as_deref(), underwater, &project_path.to_string_lossy());
                    // On macOS the core makes the project file, empty, to bookmark the video relative to it
                    if data.is_err() && std::fs::metadata(project).is_ok_and(|m| m.len() == 0) {
                        let _ = std::fs::remove_file(project);
                    }
                    data?
                }
            };
            std::fs::write(project, data).map_err(|e| format!("{}: {e}", project.display()))?;
            println!("Wrote {}", project.display());
        },
        "--remove" => {
            let removed = embedded::remove(video).map_err(|e| format!("{}: {e}", video.display()))?;
            println!("{}", if removed { "Removed the project" } else { "No project in it" });
        },
        "--show" => {
            let obj = embedded::read(video).map_err(|e| format!("{}: {e}", video.display()))?
                .ok_or_else(|| format!("{} has no project in it", video.display()))?;
            println!("{}", serde_json::to_string_pretty(&obj).unwrap());
        },
        _ => unreachable!()
    }
    Ok(())
}
