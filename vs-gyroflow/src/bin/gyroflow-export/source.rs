// The clip: what the gyroflow plugin stabilizes it with, what ffprobe says of it; a file's own attributes.
// One of gyroflow-export's parts: main.rs has the uses and main(); each part's items are the program's.

use super::*;

// The Finder's own attributes of a file: its tags (and their colors), its label, its comment. A file replaced
// (--force: a moment updated, an export made again) keeps them, as it would if edited in place
#[cfg(target_os = "macos")]
const FINDER_ATTRIBUTES: &[&str] = &["com.apple.metadata:_kMDItemUserTags", "com.apple.FinderInfo", "com.apple.metadata:kMDItemFinderComment"];

#[cfg(target_os = "macos")]
pub(crate) fn copy_finder_attributes(from: &Path, to: &Path) {
    use std::os::unix::ffi::OsStrExt;
    let (Ok(from), Ok(to)) = (std::ffi::CString::new(from.as_os_str().as_bytes()), std::ffi::CString::new(to.as_os_str().as_bytes())) else { return };
    for name in FINDER_ATTRIBUTES {
        let name = std::ffi::CString::new(*name).unwrap();
        let mut buf = vec![0u8; 64 * 1024];
        let n = unsafe { libc::getxattr(from.as_ptr(), name.as_ptr(), buf.as_mut_ptr() as *mut libc::c_void, buf.len(), 0, 0) };
        if n <= 0 { continue; }
        unsafe { libc::setxattr(to.as_ptr(), name.as_ptr(), buf.as_ptr() as *const libc::c_void, n as usize, 0, 0); }
    }
}

#[cfg(not(target_os = "macos"))]
pub(crate) fn copy_finder_attributes(_from: &Path, _to: &Path) {}

pub(crate) fn file_name(path: &str) -> String {
    Path::new(path).file_name().map(|x| x.to_string_lossy().to_string()).unwrap_or_else(|| path.to_string())
}

pub(crate) enum Outcome { Done(PathBuf), Skipped(String) }

// What the gyroflow plugin stabilizes the video with
// `lens`: what's done with the lens and motion; a clip with no motion data has its lens alone corrected, whatever was asked
pub(crate) struct Source { pub(crate) project: String, pub(crate) preset: Option<String>, pub(crate) label: &'static str, pub(crate) lens: vs_gyroflow::LensMode }

// mpv's own default: %APPDATA%\mpv on Windows, $XDG_CONFIG_HOME/mpv (~/.config/mpv) elsewhere
fn mpv_dir() -> PathBuf {
    let env = |k: &str| std::env::var_os(k).filter(|v| !v.is_empty()).map(PathBuf::from);
    if let Some(dir) = env("MPV_CONFIG_DIR") { return dir; }
    if cfg!(windows) { return env("APPDATA").unwrap_or_default().join("mpv"); }
    if !cfg!(target_os = "macos") { if let Some(dir) = env("XDG_CONFIG_HOME") { return dir.join("mpv"); } }
    env("HOME").unwrap_or_default().join(".config/mpv")
}

// A file of one of the mpv plugins: in $<PLUGIN>_DATA when it's there, else in the scripts folder this binary is
// installed in (~~/scripts/gyroflow/), or in mpv's when it runs from elsewhere (a build)
pub(crate) fn plugin_file(plugin: &str, name: &str) -> PathBuf {
    if let Some(data) = std::env::var_os(format!("{}_DATA", plugin.to_uppercase())).filter(|d| !d.is_empty()) {
        let p = PathBuf::from(data).join(name);
        if p.is_file() { return p; }
    }
    let here = std::env::current_exe().ok().and_then(|e| vs_gyroflow::canonicalize(e).ok())
        .and_then(|e| Some(e.parent()?.parent()?.to_path_buf()))
        .filter(|scripts| scripts.join("gyroflow").join(format!("gyroflow-export{}", std::env::consts::EXE_SUFFIX)).is_file());
    here.unwrap_or_else(|| mpv_dir().join("scripts")).join(plugin).join(name)
}

// `project`: the one given (--project), else the video's own, as the workspace plugin finds it
// `preset`: the one the settings name (the player's), else the plugin's own
pub(crate) fn find_source(path: &Path, project: Option<&Path>, lens: vs_gyroflow::LensMode, preset: Option<&Path>) -> Result<Source, String> {
    let s = path.to_string_lossy().to_string();
    let default_preset = || preset.map(Path::to_path_buf).filter(|p| p.is_file()).unwrap_or_else(|| plugin_file("gyroflow", "default.gyroflow"));
    if let Some(p) = project {
        return Ok(Source { project: p.to_string_lossy().to_string(), preset: None, label: "project", lens });
    }
    if let Some(sidecar) = vs_gyroflow::workspace::gyroflow_project(path) {
        return Ok(Source { project: sidecar.to_string_lossy().to_string(), preset: None, label: "project", lens });
    }
    // Nothing to stabilize with: the lens alone when it has a lens profile and the lens is to be corrected
    let lens_alone = |why: String| -> Result<Source, String> {
        let has_lens = probe::camera_and_lens(&s).is_some_and(|(_, l)| !l.is_empty());
        if lens != vs_gyroflow::LensMode::Recorded && has_lens {
            Ok(Source { project: s.clone(), preset: None, label: "the lens alone", lens: vs_gyroflow::LensMode::Only })
        } else {
            Err(why)
        }
    };
    // Motion measured from the picture beside it (afp_motion.rs) and none of its own the player uses: stabilized with
    // it, as the player plays it (load_manager reads it; no horizon lock). Before, exports, moments and photos of such
    // a clip had the lens alone corrected (GOPR1585)
    let measured = vs_gyroflow::afp_motion::header_for(path).is_some_and(|h| h.is_ok());
    let info = match probe::probe(&s) {
        Ok(info) => info,
        Err(_) if measured => {
            let preset = default_preset();
            return Ok(Source { project: s.clone(), preset: preset.is_file().then(|| preset.to_string_lossy().to_string()), label: "motion measured from the picture", lens });
        }
        Err(e) => return lens_alone(e),
    };
    match info["mode"].as_str() {
        Some("default") => {
            let preset = default_preset();
            Ok(Source { project: s.clone(), preset: preset.is_file().then(|| preset.to_string_lossy().to_string()), label: "default preset", lens })
        }
        _ => lens_alone("no project or motion data".into()),
    }
}

pub(crate) struct VideoInfo {
    pub(crate) width: usize,
    pub(crate) height: usize,
    pub(crate) fps: (u64, u64),
    pub(crate) frames: u64,
    pub(crate) duration: f64,
    pub(crate) bit_rate: Option<u64>,
    pub(crate) full_range: bool,
    // The format's creation_time (UTC)
    pub(crate) created: Option<String>,
    // Color tags, ffmpeg names
    pub(crate) primaries: Option<String>,
    pub(crate) transfer: Option<String>,
    pub(crate) matrix: Option<String>,
}

pub(crate) fn ffprobe(path: &Path) -> Result<VideoInfo, String> {
    let out = ffmpeg_command("ffprobe").args(["-v", "error", "-select_streams", "v:0", "-show_entries",
        "stream=width,height,r_frame_rate,nb_frames,duration,bit_rate,pix_fmt,color_range,color_primaries,color_transfer,color_space:format=duration:format_tags=creation_time",
        "-of", "json"]).arg(path).output().map_err(|e| format!("ffprobe: {e}"))?;
    if !out.status.success() { return Err(format!("ffprobe: {}", String::from_utf8_lossy(&out.stderr).trim())); }
    let json: serde_json::Value = serde_json::from_slice(&out.stdout).map_err(|e| e.to_string())?;
    let st = &json["streams"][0];
    let num = |v: &serde_json::Value| v.as_str().and_then(|x| x.parse::<f64>().ok()).or_else(|| v.as_f64());
    let tag = |k: &str| st[k].as_str().filter(|x| !x.is_empty() && *x != "unknown").map(str::to_string);
    let fps = st["r_frame_rate"].as_str().and_then(|r| r.split_once('/'))
        .and_then(|(n, d)| Some((n.parse().ok()?, d.parse().ok()?))).filter(|&(n, d): &(u64, u64)| n > 0 && d > 0)
        .ok_or("no frame rate")?;
    let duration = num(&st["duration"]).or_else(|| num(&json["format"]["duration"])).unwrap_or(0.0);
    Ok(VideoInfo {
        width: st["width"].as_u64().ok_or("no video")? as usize,
        height: st["height"].as_u64().ok_or("no video")? as usize,
        fps,
        frames: num(&st["nb_frames"]).map(|x| x as u64).unwrap_or((duration * fps.0 as f64 / fps.1 as f64) as u64),
        duration,
        bit_rate: num(&st["bit_rate"]).map(|x| x as u64),
        // A JPEG pixel format is full range whatever the stream says: its frames are (the decoder's VUI flag), and mpv
        // decodes them so (old GoPros' H.264: yuvj420p in a stream tagged tv)
        full_range: st["color_range"].as_str() == Some("pc") || st["pix_fmt"].as_str().is_some_and(|f| f.starts_with("yuvj")),
        created: json["format"]["tags"]["creation_time"].as_str().map(str::to_string),
        primaries: tag("color_primaries"),
        transfer: tag("color_transfer"),
        matrix: tag("color_space"),
    })
}
