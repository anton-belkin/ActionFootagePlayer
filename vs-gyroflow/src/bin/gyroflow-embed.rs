// SPDX-License-Identifier: GPL-3.0-or-later
//
// Moves a Gyroflow project into the video it belongs to, so the clip carries it (see embedded.rs), and back out.
//
//   gyroflow-embed [--move] <video> [project.gyroflow]   store the project (default: the .gyroflow next to the video);
//                                                         --move deletes the .gyroflow once it's stored and read back
//   gyroflow-embed --extract <video> [project.gyroflow]  write the stored project next to the video, to open it in Gyroflow
//   gyroflow-embed --remove <video>                      remove the stored project
//   gyroflow-embed --show <video>                        print the stored project

#[path = "../embedded.rs"]
#[allow(dead_code)]
mod embedded;

use std::path::{ Path, PathBuf };

fn main() {
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    let mode = match args.first().map(String::as_str) {
        Some(m @ ("--move" | "--extract" | "--remove" | "--show")) => { let m = m.to_string(); args.remove(0); m },
        _ => String::new(),
    };
    if args.is_empty() || args.len() > 2 || args[0].starts_with("--") {
        eprintln!("usage: gyroflow-embed [--move|--extract|--remove|--show] <video> [project.gyroflow]");
        std::process::exit(2);
    }
    let video = PathBuf::from(&args[0]);
    let project = args.get(1).map(PathBuf::from).unwrap_or_else(|| video.with_extension("gyroflow"));
    if let Err(e) = run(&mode, &video, &project) {
        eprintln!("gyroflow-embed: {e}");
        std::process::exit(1);
    }
}

fn run(mode: &str, video: &Path, project: &Path) -> Result<(), String> {
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
            let obj = embedded::read(video).map_err(|e| format!("{}: {e}", video.display()))?
                .ok_or_else(|| format!("{} has no project in it", video.display()))?;
            if project.exists() {
                return Err(format!("{} already exists", project.display()));
            }
            std::fs::write(project, serde_json::to_string_pretty(&obj).unwrap()).map_err(|e| format!("{}: {e}", project.display()))?;
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
