// SPDX-License-Identifier: GPL-3.0-or-later
//
// The programs the plugin's tools run (ffmpeg, ffprobe): the app's own where it's in one, else where they're installed.

use std::path::{ Path, PathBuf };

/// A tool by its name: GYROFLOW_TOOLS's folder, else (Windows, Linux) beside this program, else (macOS) the app's
/// Contents/Helpers, then Homebrew's; else the bare name, for PATH
pub fn tool(name: &str) -> PathBuf {
    let name = format!("{name}{}", std::env::consts::EXE_SUFFIX);
    if let Some(p) = std::env::var_os("GYROFLOW_TOOLS").map(|d| Path::new(&d).join(&name)).filter(|p| p.is_file()) {
        return p;
    }
    let here = std::env::current_exe().ok().and_then(|e| Some(e.parent()?.join(&name))).filter(|p| p.is_file());
    if cfg!(windows) || cfg!(target_os = "linux") {
        if let Some(p) = here { return p; }
    }
    if cfg!(target_os = "macos") {
        // In the app: its own (Contents/Helpers), whoever runs this
        if let Some(p) = app_contents().map(|c| c.join("Helpers").join(&name)).filter(|p| p.is_file()) { return p; }
        for dir in ["/opt/homebrew/bin", "/usr/local/bin"] {
            let p = Path::new(dir).join(&name);
            if p.is_file() { return p; }
        }
    }
    PathBuf::from(name)
}

/// The app's Contents folder when this runs from inside ActionFootagePlayer.app (Contents/Resources/plugins/gyroflow/)
pub fn app_contents() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?.canonicalize().ok()?;
    exe.ancestors().find(|a| a.file_name().is_some_and(|n| n == "Contents") && a.join("Helpers").is_dir()).map(Path::to_path_buf)
}
