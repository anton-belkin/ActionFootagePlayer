// SPDX-License-Identifier: GPL-3.0-or-later
//
// `gyroflow-export <video>...`: renders videos the way mpv plays them, stabilized and with the log curve's LUT, in one
// pass: decode -> Gyroflow (the plugin's own engine, on the GPU) -> LUT -> encode. The source is decoded and
// encoded once, so there's no intermediate file to lose quality in.
//
// It picks the stabilization the same way the gyroflow mpv plugin does (a .gyroflow next to the video, a project
// embedded in it, or the clip's own motion data with the default preset) and the log curve from the camera's
// metadata, as the dlogm plugin does (DJI's ColorMode, GoPro's Protune color; --curve sets it). A log clip with
// nothing to stabilize it with, or with --no-stabilize, is color-converted only (no Gyroflow in the pipeline); a
// clip with neither is skipped, there being nothing to change.
//
// Installed in the gyroflow plugin's folder (~~/scripts/gyroflow/), it takes the default preset from there and the
// LUTs from the dlogm plugin's (~~/scripts/dlogm/luts/); $GYROFLOW_DATA and $DLOGM_DATA, when set, come first,
// as for the plugins (ActionFootagePlayer keeps your preset and DJI's LUT there, its own copy of the plugins being
// read-only).
//
// Output: <video's folder>/stabilized/<name>_stab.mov (with --from/--to, <name>_stab_<from>-<to>s.mov; color only,
// _color instead of _stab), 10-bit
//   log:            HEVC Main10, Rec.2100 HLG (luts/<curve>_hlg.cube); D-Log M in Rec.709 with --sdr (DJI's LUT)
//   anything else:  HEVC Main10 in the source's own colors
//   --prores:       ProRes 422 HQ instead of HEVC
// Audio is copied as it is; the file gets the source's dates.
//
// Decoding and encoding are ffmpeg (Homebrew's on macOS; on Windows and Linux the one next to this binary, else the
// one in PATH), as child processes with raw frames over pipes; the LUT is ffmpeg's lut3d filter, in RGB, the same
// place mpv's shader applies it. The encoder is VideoToolbox on macOS; elsewhere the first of NVENC, Quick Sync,
// AMF (Windows) and VAAPI (Linux) that encodes a test frame, else x265 on the CPU (--encoder picks one).
//
// For a player running it (the gyroflow plugin's export.lua): --progress writes the progress to a file twice a
// second; SIGTERM (or Ctrl+C) stops it, the ffmpegs too, and removes the unfinished file; SIGUSR1 puts it and the
// ffmpegs at background priority (macOS's lowest, CPU, disk and GPU: playback keeps its frame rate, the export
// nearly stops) and SIGUSR2 back. Without macOS's background priority (Linux, Windows), background is a few frames
// a second instead. Windows has no such signals: --control <file> takes the same as a word in a file, polled
// (stop, background, normal).

use std::io::{ Read, Write };
use std::path::{ Path, PathBuf };
use std::process::{ Command, Stdio };
use std::sync::{ Arc, mpsc };
use std::sync::atomic::{ AtomicBool, AtomicI32, Ordering::Relaxed };
use std::collections::BTreeMap;
use std::time::Instant;

use parking_lot::Mutex;
use vs_gyroflow::{ Engine, Interpolation, PlaneJob, SampleKind, probe, gpu_lut::{ self, GpuLut } };

const USAGE: &str = "usage: gyroflow-export [options] <video>...

Renders each video stabilized (and, for log, color-converted) into a stabilized/ folder next to it. A log
video with nothing to stabilize it with is color-converted only.

  --curve <curve>  the log curve: dlogm, dlog, gplog, gplog2, ilog, or none (colors as recorded) instead of the
                   one the camera's metadata says
  --sdr            D-Log M to Rec.709 (DJI's LUT) instead of HLG
  --no-stabilize   the color conversion only, as a clip plays with the stabilization off
  --horizon-lock <%>  the horizon lock's amount (0-100, 0 off) instead of the project's or preset's
  --prores         ProRes 422 HQ instead of HEVC 10-bit
  --from <time>    start at this time (seconds, or m:ss)
  --to <time>      end at this time
  --out <folder>   write here instead of <video's folder>/stabilized
  --force          replace an existing output instead of skipping the video
  --interpolation  bilinear | bicubic | lanczos (default lanczos)
  --encoder <name> ffmpeg's HEVC encoder, e.g. hevc_nvenc, hevc_qsv, hevc_amf, hevc_vaapi, libx265
                   (default: VideoToolbox on macOS, else the first of these that works)
  --progress <file>  write the progress there as JSON, twice a second
  --background     start at background priority (SIGUSR1 / SIGUSR2 switch it later)
  --control <file> read stop / background / normal from this file while running (for Windows, which has no
                   SIGUSR1/SIGUSR2)";

// The log curves, as the dlogm plugin names them; luts/<id>_hlg.cube is each one's LUT to HLG
const CURVES: &[(&str, &str)] = &[("dlogm", "D-Log M"), ("dlog", "D-Log"), ("gplog", "GP-Log"), ("gplog2", "GP-Log2"), ("ilog", "I-Log")];

struct Options {
    // None: from the metadata; Some(None): none
    curve: Option<Option<&'static str>>,
    sdr: bool,
    no_stabilize: bool,
    horizon_lock: Option<f64>,
    prores: bool,
    from: Option<f64>,
    to: Option<f64>,
    out: Option<PathBuf>,
    force: bool,
    interpolation: Interpolation,
    encoder: Option<String>,
    progress: Progress,
    control: Option<PathBuf>,
}

fn main() {
    vs_gyroflow::init_logging();
    catch_signals();
    let mut opts = Options { curve: None, sdr: false, no_stabilize: false, horizon_lock: None, prores: false, from: None, to: None, out: None, force: false,
                             interpolation: Interpolation::Lanczos4, encoder: None, progress: Progress(None), control: None };
    let mut files = Vec::new();
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        let mut value = |name: &str| args.next().unwrap_or_else(|| fail(&format!("{name} needs a value")));
        match a.as_str() {
            "--curve" => opts.curve = Some(match value("--curve").as_str() {
                "none" => None,
                c => Some(CURVES.iter().find(|(id, _)| *id == c).map(|(id, _)| *id)
                    .unwrap_or_else(|| fail(&format!("unknown curve {c} (dlogm, dlog, gplog, gplog2, ilog, none)")))),
            }),
            "--sdr" => opts.sdr = true,
            "--no-stabilize" => opts.no_stabilize = true,
            "--horizon-lock" => opts.horizon_lock = Some(value("--horizon-lock").parse::<f64>().ok().filter(|v| (0.0..=100.0).contains(v))
                .unwrap_or_else(|| fail("--horizon-lock takes 0-100"))),
            "--prores" => opts.prores = true,
            "--from" => opts.from = Some(parse_time(&value("--from"))),
            "--to" => opts.to = Some(parse_time(&value("--to"))),
            "--out" => opts.out = Some(PathBuf::from(value("--out"))),
            "--force" => opts.force = true,
            "--interpolation" => opts.interpolation = match value("--interpolation").as_str() {
                "bilinear" => Interpolation::Bilinear,
                "bicubic" => Interpolation::Bicubic,
                "lanczos" => Interpolation::Lanczos4,
                other => fail(&format!("unknown interpolation {other}")),
            },
            "--encoder" => opts.encoder = Some(value("--encoder")),
            "--progress" => opts.progress = Progress(Some(PathBuf::from(value("--progress")))),
            "--control" => opts.control = Some(PathBuf::from(value("--control"))),
            "--background" => { BACKGROUND.store(true, Relaxed); set_background(&[0], true); }
            "-h" | "--help" => { println!("{USAGE}"); return; }
            _ if a.starts_with("--") => fail(&format!("unknown option {a}\n\n{USAGE}")),
            _ => files.push(a),
        }
    }
    if files.is_empty() { fail(USAGE); }
    if let (Some(f), Some(t)) = (opts.from, opts.to) { if t <= f { fail("--to must be after --from"); } }

    // The pid first: a player stops or reprioritizes it by it, from the start
    opts.progress.write(serde_json::json!({ "pid": std::process::id() }));
    let mut failed = 0;
    for file in &files {
        match export(file, &opts) {
            Ok(Outcome::Done(out)) => eprintln!("  -> {}", out.display()),
            Ok(Outcome::Skipped(why)) => eprintln!("{}: skipped, {why}", file_name(file)),
            Err(_) if stopped() => {
                eprintln!("{}: cancelled", file_name(file));
                std::process::exit(128 + STOP.load(Relaxed));
            }
            Err(e) => { eprintln!("{}: failed, {e}", file_name(file)); failed += 1; }
        }
    }
    if failed > 0 { std::process::exit(1); }
}

fn fail(msg: &str) -> ! {
    eprintln!("{msg}");
    std::process::exit(2);
}

// Set by the signal handlers: the signal that asked to stop, and the priority asked for
static STOP: AtomicI32 = AtomicI32::new(0);
static BACKGROUND: AtomicBool = AtomicBool::new(false);

extern "C" fn on_signal(sig: libc::c_int) {
    match sig {
        #[cfg(unix)]
        libc::SIGUSR1 => BACKGROUND.store(true, Relaxed),
        #[cfg(unix)]
        libc::SIGUSR2 => BACKGROUND.store(false, Relaxed),
        _ => STOP.store(sig, Relaxed),
    }
}

fn catch_signals() {
    #[cfg(unix)]
    let signals = [libc::SIGINT, libc::SIGTERM, libc::SIGHUP, libc::SIGUSR1, libc::SIGUSR2];
    #[cfg(windows)]
    let signals = [libc::SIGINT, libc::SIGTERM];
    for sig in signals {
        unsafe { libc::signal(sig, on_signal as extern "C" fn(libc::c_int) as libc::sighandler_t); }
    }
}

fn stopped() -> bool { STOP.load(Relaxed) != 0 }

// --control: the word in the file, as the signals would set it
fn read_control(path: &Path) {
    match std::fs::read_to_string(path).unwrap_or_default().trim() {
        "stop" => STOP.store(libc::SIGTERM, Relaxed),
        "background" => BACKGROUND.store(true, Relaxed),
        "normal" => BACKGROUND.store(false, Relaxed),
        _ => {}
    }
}

// PRIO_DARWIN_BG, for these processes (0: this one). Set on the ffmpegs too: the decoding and encoding compete
// with the player's as much as the GPU work does
#[cfg(target_os = "macos")]
fn set_background(pids: &[u32], on: bool) {
    for &pid in pids {
        unsafe { libc::setpriority(libc::PRIO_DARWIN_PROCESS, pid as libc::id_t, if on { libc::PRIO_DARWIN_BG } else { 0 }); }
    }
}

// Nothing like it elsewhere: a raised nice value can't be lowered again without privileges on Linux, and the GPU
// and the video engines have no priority to set. The export loop slows itself down instead (THROTTLE)
#[cfg(not(target_os = "macos"))]
fn set_background(_pids: &[u32], _on: bool) {}

// Background without macOS's priority: at most a frame per this long, a few fps that leave the GPU to playback
const THROTTLE: std::time::Duration = std::time::Duration::from_millis(150);

// --progress: the file is replaced whole (written next to it, then renamed), so a reader never sees half of it
struct Progress(Option<PathBuf>);

impl Progress {
    fn write(&self, value: serde_json::Value) {
        let Some(path) = &self.0 else { return };
        let tmp = PathBuf::from(format!("{}.tmp", path.display()));
        if std::fs::write(&tmp, value.to_string()).is_ok() { let _ = std::fs::rename(&tmp, path); }
    }
}

fn parse_time(s: &str) -> f64 {
    s.split(':').try_fold(0.0, |acc, part| part.parse::<f64>().map(|v| acc * 60.0 + v))
        .unwrap_or_else(|_| fail(&format!("bad time {s} (seconds, or m:ss)")))
}

fn file_name(path: &str) -> String {
    Path::new(path).file_name().map(|x| x.to_string_lossy().to_string()).unwrap_or_else(|| path.to_string())
}

enum Outcome { Done(PathBuf), Skipped(String) }

// What the gyroflow plugin stabilizes the video with
struct Source { project: String, preset: Option<String>, label: &'static str }

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
fn plugin_file(plugin: &str, name: &str) -> PathBuf {
    if let Some(data) = std::env::var_os(format!("{}_DATA", plugin.to_uppercase())).filter(|d| !d.is_empty()) {
        let p = PathBuf::from(data).join(name);
        if p.is_file() { return p; }
    }
    let here = std::env::current_exe().ok().and_then(|e| vs_gyroflow::canonicalize(e).ok())
        .and_then(|e| Some(e.parent()?.parent()?.to_path_buf()))
        .filter(|scripts| scripts.join("gyroflow").join(format!("gyroflow-export{}", std::env::consts::EXE_SUFFIX)).is_file());
    here.unwrap_or_else(|| mpv_dir().join("scripts")).join(plugin).join(name)
}

fn find_source(path: &Path) -> Result<Source, String> {
    let s = path.to_string_lossy().to_string();
    let base = path.with_extension("");
    for sidecar in [base.with_extension("gyroflow"), PathBuf::from(format!("{s}.gyroflow"))] {
        if sidecar.is_file() {
            return Ok(Source { project: sidecar.to_string_lossy().to_string(), preset: None, label: "project" });
        }
    }
    let info = probe::probe(&s)?;
    match info["mode"].as_str() {
        Some("embedded") => Ok(Source { project: s, preset: None, label: "embedded project" }),
        Some("default") => {
            let preset = plugin_file("gyroflow", "default.gyroflow");
            Ok(Source { project: s, preset: preset.is_file().then(|| preset.to_string_lossy().to_string()), label: "default preset" })
        }
        _ => Err("no project or motion data".into()),
    }
}

struct VideoInfo {
    width: usize,
    height: usize,
    fps: (u64, u64),
    frames: u64,
    duration: f64,
    bit_rate: Option<u64>,
    full_range: bool,
    // Color tags, ffmpeg names
    primaries: Option<String>,
    transfer: Option<String>,
    matrix: Option<String>,
}

fn ffprobe(path: &Path) -> Result<VideoInfo, String> {
    let out = ffmpeg_command("ffprobe").args(["-v", "error", "-select_streams", "v:0", "-show_entries",
        "stream=width,height,r_frame_rate,nb_frames,duration,bit_rate,color_range,color_primaries,color_transfer,color_space:format=duration",
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
        full_range: st["color_range"].as_str() == Some("pc"),
        primaries: tag("color_primaries"),
        transfer: tag("color_transfer"),
        matrix: tag("color_space"),
    })
}

// Homebrew's ffmpeg: a Finder Quick Action or mpv doesn't have /opt/homebrew/bin in its PATH. On Windows, where
// ffmpeg is rarely in PATH, one copied next to this binary (the gyroflow plugin's folder) comes first. The one in
// GYROFLOW_TOOLS (ActionFootagePlayer.app's own) comes before all of them
fn tool(name: &str) -> PathBuf {
    let name = format!("{name}{}", std::env::consts::EXE_SUFFIX);
    if let Some(p) = std::env::var_os("GYROFLOW_TOOLS").map(|d| Path::new(&d).join(&name)).filter(|p| p.is_file()) {
        return p;
    }
    let here = std::env::current_exe().ok().and_then(|e| Some(e.parent()?.join(&name))).filter(|p| p.is_file());
    if cfg!(windows) || cfg!(target_os = "linux") {
        if let Some(p) = here { return p; }
    }
    if cfg!(target_os = "macos") {
        for dir in ["/opt/homebrew/bin", "/usr/local/bin"] {
            let p = Path::new(dir).join(&name);
            if p.is_file() { return p; }
        }
    }
    PathBuf::from(name)
}

// Run without a console window of its own on Windows: mpv runs this one without a console, and each ffmpeg would
// open one
fn ffmpeg_command(name: &str) -> Command {
    #[allow(unused_mut)]
    let mut cmd = Command::new(tool(name));
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    cmd
}

// The HEVC encoder ffmpeg writes the export with: how to set it up, and the pixel format it takes
struct Encoder {
    name: String,
    // Before the inputs (a device to open)
    global: Vec<String>,
    // The encoder's own options
    args: Vec<String>,
    pix_fmt: &'static str,
    // After the format filter (uploading to the device)
    upload: &'static str,
}

const VAAPI_DEVICE: &str = "/dev/dri/renderD128";

fn encoder(name: &str, prores: bool) -> Encoder {
    let v = |a: &[&str]| a.iter().map(|x| x.to_string()).collect::<Vec<_>>();
    let mut e = Encoder { name: name.to_string(), global: vec![], args: vec![], pix_fmt: "p010le", upload: "" };
    match name {
        "prores_videotoolbox" => { e.args = v(&["-profile:v", "hq"]); e.pix_fmt = "p210le"; }
        "prores_ks" => { e.args = v(&["-profile:v", "3", "-vendor", "apl0"]); e.pix_fmt = "yuv422p10le"; }
        "hevc_videotoolbox" | "hevc_qsv" | "hevc_amf" => e.args = v(&["-profile:v", "main10"]),
        "hevc_nvenc" => e.args = v(&["-profile:v", "main10", "-preset", "p5"]),
        "hevc_vaapi" => {
            e.global = v(&["-vaapi_device", VAAPI_DEVICE]);
            e.args = v(&["-profile:v", "main10"]);
            e.upload = ",hwupload";
        }
        "libx265" => {
            e.args = v(&["-profile:v", "main10", "-preset", "fast", "-x265-params", "log-level=error"]);
            e.pix_fmt = "yuv420p10le";
        }
        _ => e.pix_fmt = if prores { "yuv422p10le" } else { "yuv420p10le" },
    }
    e
}

// Whether ffmpeg encodes a 10-bit frame with it: an encoder can be built in without the hardware it needs
fn encoder_works(e: &Encoder) -> bool {
    if e.name == "hevc_vaapi" && !Path::new(VAAPI_DEVICE).exists() { return false; }
    ffmpeg_command("ffmpeg").args(["-nostdin", "-v", "error"]).args(&e.global)
        .args(["-f", "lavfi", "-i", "color=black:s=640x360:r=30", "-frames:v", "1",
               "-vf", &format!("format={}{}", e.pix_fmt, e.upload), "-c:v", &e.name]).args(&e.args)
        .args(["-f", "null", "-"])
        .stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null())
        .status().is_ok_and(|s| s.success())
}

// Whether ffmpeg decodes the video's first frame with -hwaccel auto. It falls back to the CPU when there's no
// hardware decoder, but stops when one fails to start (a D3D device on Windows, a VAAPI driver)
fn hwaccel_works(path: &Path) -> bool {
    ffmpeg_command("ffmpeg").args(["-nostdin", "-v", "error", "-hwaccel", "auto", "-i"]).arg(path)
        .args(["-map", "0:v:0", "-frames:v", "1", "-f", "null", "-"])
        .stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null())
        .status().is_ok_and(|s| s.success())
}

fn pick_encoder(opts: &Options) -> Result<Encoder, String> {
    static PICKED: std::sync::OnceLock<Option<String>> = std::sync::OnceLock::new();
    if let Some(name) = &opts.encoder { return Ok(encoder(name, opts.prores)); }
    if cfg!(target_os = "macos") {
        return Ok(encoder(if opts.prores { "prores_videotoolbox" } else { "hevc_videotoolbox" }, opts.prores));
    }
    if opts.prores { return Ok(encoder("prores_ks", true)); }
    let candidates: &[&str] = if cfg!(windows) { &["hevc_nvenc", "hevc_qsv", "hevc_amf", "libx265"] }
                              else { &["hevc_nvenc", "hevc_vaapi", "hevc_qsv", "libx265"] };
    let picked = PICKED.get_or_init(|| candidates.iter().find(|&&n| encoder_works(&encoder(n, false))).map(|n| n.to_string()));
    match picked {
        Some(name) => Ok(encoder(name, false)),
        None => Err("ffmpeg has no HEVC encoder that works here (hevc_nvenc, hevc_qsv, hevc_amf, hevc_vaapi, libx265)".into()),
    }
}

fn lut(name: &str) -> Result<PathBuf, String> {
    let p = plugin_file("dlogm", &format!("luts/{name}"));
    if p.is_file() { Ok(p) } else { Err(format!("{} is missing (install the dlogm plugin: install.sh)", p.display())) }
}

// The YUV matrix the video is decoded with, as mpv does: the one it's tagged with, else BT.709 (DJI tags D-Log M
// BT.709; a GoPro's or Insta360's 10-bit may be tagged BT.2020). By the names of ffmpeg's scale and gpu_lut
fn yuv_matrix(tag: Option<&str>) -> &'static str {
    match tag {
        Some("bt2020nc" | "bt2020c") => "bt2020",
        Some("smpte170m" | "bt470bg") => "bt601",
        _ => "bt709",
    }
}

// ffmpeg's filter syntax: quote the path for the option value, then for the filter graph
fn filter_path(p: &Path) -> String {
    let s = p.to_string_lossy().replace('\\', "\\\\").replace('\'', "\\'").replace(':', "\\:");
    format!("'{s}'")
}

fn export(file: &str, opts: &Options) -> Result<Outcome, String> {
    let path = vs_gyroflow::canonicalize(file).map_err(|e| e.to_string())?;
    let curve = opts.curve.unwrap_or_else(|| probe::log_curve(&path.to_string_lossy()));
    // Nothing to stabilize with, or --no-stabilize: the color conversion alone, if there's one
    let source = if opts.no_stabilize { Err("stabilization off".to_string()) } else { find_source(&path) };
    let source = match (source, curve) {
        (Ok(s), _) => Some(s),
        (Err(_), Some(_)) => None,
        (Err(why), None) => return Ok(Outcome::Skipped(format!("{why}, and not log: nothing to change"))),
    };
    let info = ffprobe(&path)?;

    let out_dir = opts.out.clone().unwrap_or_else(|| path.parent().unwrap_or(Path::new(".")).join("stabilized"));
    let stem = path.file_stem().map(|x| x.to_string_lossy().to_string()).unwrap_or_default();
    // A part of the video gets its range in the name, so it doesn't replace the whole one
    let range = match (opts.from, opts.to) {
        (None, None) => String::new(),
        (f, t) => format!("_{:.1}-{}s", f.unwrap_or(0.0), t.map(|t| format!("{t:.1}")).unwrap_or_else(|| "end".into())),
    };
    let stem = format!("{stem}_{}{range}", if source.is_some() { "stab" } else { "color" });
    let out_path = out_dir.join(format!("{stem}.mov"));
    if out_path.exists() && !opts.force {
        return Ok(Outcome::Skipped(format!("{} exists (--force replaces it)", out_path.display())));
    }
    std::fs::create_dir_all(&out_dir).map_err(|e| format!("{}: {e}", out_dir.display()))?;
    let tmp_path = out_dir.join(format!(".{stem}.partial.mov"));

    // Color: the LUT works in RGB (mpv's shader too: it reads MAIN, the decoded RGB), from the source's matrix and
    // range, and the result is converted back to YUV in the target's matrix. On the GPU (gpu_lut.rs) when the
    // output size allows, else ffmpeg's lut3d on the CPU, at a third of the speed. Only D-Log M has a Rec.709 LUT:
    // the others are HLG whatever --sdr says, as they play
    let in_range = if info.full_range { "pc" } else { "tv" };
    let in_matrix = yuv_matrix(info.matrix.as_deref());
    let mut lut_job = None;
    let (graph, tags, color_label) = if let Some(curve) = curve {
        let name = CURVES.iter().find(|(id, _)| *id == curve).map_or(curve, |(_, name)| *name);
        let (cube, matrix, primaries, transfer, label) = if opts.sdr && curve == "dlogm" {
            (lut("dji-dlogm-rec709.cube")?, "bt709", "bt709", "bt709", format!("{name} -> Rec.709"))
        } else {
            let no_sdr = if opts.sdr { " (no Rec.709 LUT for it)" } else { "" };
            (lut(&format!("{curve}_hlg.cube"))?, "bt2020", "bt2020", "arib-std-b67", format!("{name} -> HLG{no_sdr}"))
        };
        let graph = format!("scale=in_color_matrix={in_matrix}:in_range={in_range},format=gbrp16le,lut3d=file={}:interp=tetrahedral,\
                             scale=out_color_matrix={matrix}:out_range=tv", filter_path(&cube));
        lut_job = Some((cube, matrix, graph));
        let space = if matrix == "bt2020" { "bt2020nc" } else { "bt709" };
        (String::new(), vec![primaries.to_string(), transfer.to_string(), space.to_string(), "tv".to_string()], label)
    } else {
        let tags = vec![
            info.primaries.clone().unwrap_or_else(|| "bt709".into()),
            info.transfer.clone().unwrap_or_else(|| "bt709".into()),
            info.matrix.clone().unwrap_or_else(|| "bt709".into()),
            in_range.to_string(),
        ];
        (String::new(), tags, "colors as recorded".to_string())
    };

    let fps = info.fps.0 as f64 / info.fps.1 as f64;
    let from = opts.from.unwrap_or(0.0).max(0.0);
    let to = opts.to.unwrap_or(info.duration).min(info.duration.max(from));
    // With -ss before -i, ffmpeg starts at the first frame at or after `from`
    let first_frame = (from * fps - 1e-6).ceil().max(0.0) as u64;
    let total_frames = if opts.from.is_some() || opts.to.is_some() { ((to - from) * fps).round().max(1.0) as u64 } else { info.frames.max(1) };

    // The engine, as the plugin sets it up for mpv: 10-bit YUV 4:2:0, the project's output size. Color only: none,
    // the frames go to the LUT as they are decoded
    let (w, h) = (info.width & !1, info.height & !1);
    let workers = 3;
    let key = format!("export|{}", path.display());
    let engine = match &source {
        Some(source) => Some(Arc::new(Engine::load(key, &source.project, source.preset.as_deref(), None, opts.horizon_lock, (w, h), 3, (1, 1), None,
                                                   SampleKind::U16, 1023.0, true, opts.interpolation, workers)?)),
        None => None,
    };
    let (ow, oh) = engine.as_ref().map_or((w, h), |e| (e.render_size.0 & !1, e.render_size.1 & !1));

    let mut graph = graph;
    let gpu_lut = match lut_job {
        None => None,
        Some((cube, matrix, cpu_graph)) => {
            let out_matrix = gpu_lut::matrix_coefficients(matrix).unwrap();
            match GpuLut::new(&cube, ow, oh, gpu_lut::matrix_coefficients(in_matrix).unwrap(), info.full_range, out_matrix) {
                Ok(l) => Some(l),
                Err(e) => {
                    eprintln!("  LUT on the CPU (slower): {e}");
                    graph = cpu_graph;
                    None
                }
            }
        }
    };

    let duration_label = { let t = (to - from).max(0.0) as u64; format!("{}:{:02}", t / 60, t % 60) };
    let source_label = source.as_ref().map_or("color only", |s| s.label);
    eprintln!("{}: {color_label} · {source_label} · {}x{} -> {ow}x{oh} · {duration_label}", file_name(file), w, h);

    if stopped() { return Err("cancelled".into()); }
    let enc = pick_encoder(opts)?;
    if !cfg!(target_os = "macos") { eprintln!("  encoder: {}", enc.name); }

    // Decoder: raw 10-bit planar frames on stdout. Elsewhere than on macOS, whatever hardware decoder ffmpeg finds,
    // else the CPU
    let hwaccel = if cfg!(target_os = "macos") { Some("videotoolbox") } else if hwaccel_works(&path) { Some("auto") } else { None };
    let mut dec_cmd = ffmpeg_command("ffmpeg");
    dec_cmd.args(["-nostdin", "-v", "error"]);
    if let Some(h) = hwaccel { dec_cmd.args(["-hwaccel", h]); }
    if opts.from.is_some() { dec_cmd.args(["-ss", &format!("{from:.6}")]); }
    if opts.to.is_some() { dec_cmd.args(["-to", &format!("{to:.6}")]); }
    dec_cmd.arg("-i").arg(&path)
        .args(["-map", "0:v:0", "-vf", &format!("scale={w}:{h}"), "-f", "rawvideo", "-pix_fmt", "yuv420p10le", "-"])
        .stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped());
    let mut decoder = dec_cmd.spawn().map_err(|e| format!("ffmpeg: {e}"))?;

    // Encoder: the stabilized frames, audio and metadata from the source
    let bit_rate = info.bit_rate.unwrap_or(80_000_000) as f64 * (ow * oh) as f64 / (w * h) as f64;
    let mut enc_cmd = ffmpeg_command("ffmpeg");
    enc_cmd.args(["-nostdin", "-v", "error", "-y"]).args(&enc.global);
    enc_cmd.args([
                  "-f", "rawvideo", "-pix_fmt", "yuv420p10le", "-s", &format!("{ow}x{oh}"),
                  "-framerate", &format!("{}/{}", info.fps.0, info.fps.1),
                  "-color_range", in_range, "-colorspace", "bt709", "-color_primaries", "bt709", "-color_trc", "bt709",
                  "-i", "-"]);
    if opts.from.is_some() { enc_cmd.args(["-ss", &format!("{from:.6}")]); }
    if opts.to.is_some() { enc_cmd.args(["-to", &format!("{to:.6}")]); }
    enc_cmd.arg("-i").arg(&path)
        .args(["-map", "0:v", "-map", "1:a?", "-map_metadata", "1", "-c:a", "copy"]);
    let pix_fmt = enc.pix_fmt;
    // The frames carry the input's BT.709 tags through the filters, and the encoder writes the frames' tags, not
    // the -color_* options, so the target's are set on the frames
    let setparams = format!("setparams=color_primaries={}:color_trc={}:colorspace={}:range={}", tags[0], tags[1], tags[2], tags[3]);
    let graph = if graph.is_empty() { format!("format={pix_fmt},{setparams}") } else { format!("{graph},format={pix_fmt},{setparams}") };
    enc_cmd.args(["-vf", &format!("{graph}{}", enc.upload)]);
    enc_cmd.args(["-c:v", &enc.name]).args(&enc.args);
    if opts.prores {
        enc_cmd.args(["-tag:v", "apch"]);
    } else {
        enc_cmd.args(["-tag:v", "hvc1", "-b:v", &format!("{}", bit_rate as u64)]);
    }
    enc_cmd.args(["-color_primaries", &tags[0], "-color_trc", &tags[1], "-colorspace", &tags[2], "-color_range", &tags[3],
                  "-movflags", "+faststart", "-f", "mov"])
        .arg(&tmp_path)
        .stdin(Stdio::piped()).stdout(Stdio::null()).stderr(Stdio::piped());
    let mut encoder = enc_cmd.spawn().map_err(|e| format!("ffmpeg: {e}"))?;

    let in_frame = w * h * 3; // 4:2:0, 2 bytes per sample
    let out_frame = ow * oh * 3;
    let plane_dims = |(w, h): (usize, usize)| [(w, h), (w / 2, h / 2), (w / 2, h / 2)];

    // Reader -> GPU workers -> writer, in order. A few frames in flight keep the GPU and both ffmpegs busy
    let in_flight = workers + 2;
    let (frame_tx, frame_rx) = mpsc::sync_channel::<(u64, Vec<u8>)>(in_flight);
    let (done_tx, done_rx) = mpsc::sync_channel::<(u64, Result<Vec<u8>, String>)>(in_flight);
    let mut dec_out = decoder.stdout.take().unwrap();
    let reader = std::thread::spawn(move || -> Result<u64, String> {
        let mut n = 0;
        loop {
            let mut buf = vec![0u8; in_frame];
            match dec_out.read_exact(&mut buf) {
                Ok(()) => {}
                Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => return Ok(n),
                Err(e) => return Err(format!("reading decoded frames: {e}")),
            }
            if frame_tx.send((n, buf)).is_err() { return Ok(n); }
            n += 1;
        }
    });
    let frame_rx = Arc::new(Mutex::new(frame_rx));
    let gpu_threads: Vec<_> = (0..workers).map(|_| {
        let (frame_rx, done_tx, engine) = (frame_rx.clone(), done_tx.clone(), engine.clone());
        let full_range = info.full_range;
        let mut lut = gpu_lut.as_ref().map(|l| l.worker());
        std::thread::spawn(move || {
            loop {
                let Ok((n, src)) = frame_rx.lock().recv() else { break };
                let frame = match &engine {
                    None => Ok(src),
                    Some(engine) => {
                        let mut dst = vec![0u8; out_frame];
                        // Frame times from the start of the file, as Gyroflow and mpv count them
                        let timestamp_us = (((first_frame + n) as f64 / fps) * 1_000_000.0).round() as i64 + engine.frame_offset_us;
                        let mut planes = Vec::with_capacity(3);
                        let (mut so, mut doff) = (0, 0);
                        for ((sw, sh), (dw, dh)) in plane_dims((w, h)).into_iter().zip(plane_dims((ow, oh))) {
                            planes.push(PlaneJob {
                                src: src[so..].as_ptr(), src_stride: sw * 2, src_w: sw, src_h: sh,
                                dst: dst[doff..].as_mut_ptr(), dst_stride: dw * 2, dst_w: dw, dst_h: dh,
                            });
                            so += sw * sh * 2;
                            doff += dw * dh * 2;
                        }
                        engine.process(timestamp_us, planes, !full_range).map(|_| dst)
                    }
                };
                let result = frame.and_then(|mut f| match lut.as_mut() { Some(l) => l.apply(&mut f).map(|_| f), None => Ok(f) });
                if done_tx.send((n, result)).is_err() { break; }
            }
        })
    }).collect();
    drop(done_tx);

    let mut enc_in = encoder.stdin.take().unwrap();
    let mut pending = BTreeMap::new();
    let mut next = 0u64;
    let started = Instant::now();
    let mut last_report = Instant::now();
    let (mut reported, mut rate) = (0u64, 0.0);
    let mut background = None;
    let mut error = None;
    let mut last_control = Instant::now();
    for (n, result) in done_rx {
        if let Some(control) = &opts.control {
            if last_control.elapsed().as_millis() >= 250 { read_control(control); last_control = Instant::now(); }
        }
        if stopped() { error = Some("cancelled".to_string()); break; }
        let want = BACKGROUND.load(Relaxed);
        if want && !cfg!(target_os = "macos") { std::thread::sleep(THROTTLE); }
        if background != Some(want) {
            set_background(&[0, decoder.id(), encoder.id()], want);
            background = Some(want);
        }
        match result {
            Ok(buf) => { pending.insert(n, buf); }
            Err(e) => { error = Some(format!("frame {n}: {e}")); break; }
        }
        while let Some(buf) = pending.remove(&next) {
            if let Err(e) = enc_in.write_all(&buf) { error = Some(format!("encoder stopped: {e}")); break; }
            next += 1;
        }
        if error.is_some() { break; }
        if last_report.elapsed().as_millis() >= 500 {
            // The recent rate, not the average: it changes with the priority
            let now = (next - reported) as f64 / last_report.elapsed().as_secs_f64();
            rate = if reported == 0 { now } else { rate * 0.7 + now * 0.3 };
            last_report = Instant::now();
            reported = next;
            let done = next as f64;
            let pct = (done / total_frames as f64 * 100.0).min(100.0);
            let eta = ((total_frames as f64 - done).max(0.0) / rate.max(1e-3)).min(359_999.0) as u64;
            eprint!("\r  {pct:3.0}%  {rate:4.0} fps  ETA {}:{:02}   ", eta / 60, eta % 60);
            opts.progress.write(serde_json::json!({
                "pid": std::process::id(), "done": next, "total": total_frames, "fps": rate, "eta": eta,
                "background": want, "partial": tmp_path,
            }));
        }
    }
    drop(enc_in);
    if error.is_some() || stopped() {
        let _ = decoder.kill();
    }
    // Cancelled: no point in the encoder finishing the file (the decoder may have stopped first, on Ctrl+C)
    if stopped() {
        let _ = encoder.kill();
    }
    for t in gpu_threads { let _ = t.join(); }
    let read = reader.join().map_err(|_| "reader panicked".to_string())?;
    let dec_status = decoder.wait().map_err(|e| e.to_string())?;
    let enc_status = encoder.wait().map_err(|e| e.to_string())?;
    let stderr_of = |child: Option<std::process::ChildStderr>| {
        let mut s = String::new();
        if let Some(mut e) = child { let _ = e.read_to_string(&mut s); }
        s.trim().to_string()
    };
    let dec_err = stderr_of(decoder.stderr.take());
    let enc_err = stderr_of(encoder.stderr.take());
    eprint!("\r{:60}\r", "");

    let result = (|| {
        if stopped() { return Err("cancelled".to_string()); }
        if let Some(e) = error { return Err(e); }
        let read = read?;
        if !dec_status.success() { return Err(format!("decoding: {dec_err}")); }
        if !enc_status.success() { return Err(format!("encoding: {enc_err}")); }
        if read == 0 { return Err("no frames decoded".into()); }
        Ok(read)
    })();
    let frames = match result {
        Ok(f) => f,
        Err(e) => { let _ = std::fs::remove_file(&tmp_path); return Err(e); }
    };

    // The source's dates, so the clip sorts where it was shot
    if let Ok(meta) = std::fs::metadata(&path) {
        if let (Ok(modified), Ok(f)) = (meta.modified(), std::fs::File::options().write(true).open(&tmp_path)) {
            let _ = f.set_modified(modified);
        }
    }
    std::fs::rename(&tmp_path, &out_path).map_err(|e| e.to_string())?;
    let secs = started.elapsed().as_secs_f64();
    eprintln!("  {frames} frames in {:.0}:{:02.0} ({:.0} fps)", (secs / 60.0).floor(), secs % 60.0, frames as f64 / secs.max(1e-3));
    Ok(Outcome::Done(out_path))
}
