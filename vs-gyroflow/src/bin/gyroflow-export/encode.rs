// ffmpeg and the encoders: which one, its settings, the output's size and bit rate.
// One of gyroflow-export's parts: main.rs has the uses and main(); each part's items are the program's.

use super::*;

// Run without a console window of its own on Windows: mpv runs this one without a console, and each ffmpeg would
// open one
pub(crate) fn ffmpeg_command(name: &str) -> Command { ffmpeg_at(tool(name)) }

pub(crate) fn ffmpeg_at(program: PathBuf) -> Command {
    #[allow(unused_mut)]
    let mut cmd = Command::new(program);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    cmd
}

// The HEVC encoder ffmpeg writes the export with: how to set it up, and the pixel format it takes
pub(crate) struct Encoder {
    pub(crate) name: String,
    // Before the inputs (a device to open)
    pub(crate) global: Vec<String>,
    // The encoder's own options
    pub(crate) args: Vec<String>,
    pub(crate) pix_fmt: &'static str,
    // After the format filter (uploading to the device)
    pub(crate) upload: &'static str,
}

const VAAPI_DEVICE: &str = "/dev/dri/renderD128";

/// --resolution's value: Some(None) original, Some(Some(n)) the short side at most n pixels
pub(crate) fn parse_resolution(v: &str) -> Option<Option<usize>> {
    match v.trim().trim_end_matches('p') {
        "original" | "" => Some(None),
        "4k" | "4K" => Some(Some(2160)),
        n => n.parse::<usize>().ok().filter(|n| *n >= 16).map(Some),
    }
}

/// A frame of `size` with its short side at most `cap`: the same shape, sides even, never enlarged
pub(crate) fn capped(size: (usize, usize), cap: Option<usize>) -> (usize, usize) {
    let Some(cap) = cap else { return size };
    let short = size.0.min(size.1);
    if short <= cap { return size; }
    let k = cap as f64 / short as f64;
    let even = |v: usize| ((v as f64 * k).round() as usize).max(2) & !1;
    if size.0 <= size.1 { (cap & !1, even(size.1)) } else { (even(size.0), cap & !1) }
}

/// The bit rate an export is encoded at: the source's, for the output's size; --encode quality half as much again
pub(crate) fn export_bit_rate(source: Option<u64>, source_px: usize, out_px: usize, quality: bool) -> f64 {
    let r = source.unwrap_or(80_000_000) as f64 * out_px as f64 / source_px.max(1) as f64;
    if quality { r * 1.5 } else { r }
}

/// The encoder's own mode for --encode: on the GPU path (`gpu`), fast is VideoToolbox's real-time mode (82 fps for 4K
/// HEVC Main10 against 50, the same quality at the same bit rate: docs/design/export-gpu-path.md), the pipes keep each
/// encoder's default; quality is each one's quality mode on both
pub(crate) fn encode_mode_args(encoder: &str, quality: bool, gpu: bool) -> Vec<String> {
    let v = |a: &[&str]| a.iter().map(|x| x.to_string()).collect::<Vec<_>>();
    match (encoder, quality) {
        ("hevc_videotoolbox", false) if gpu => v(&["-realtime", "1", "-prio_speed", "1"]),
        ("hevc_videotoolbox", true) => v(&["-realtime", "0", "-prio_speed", "0"]),
        ("hevc_nvenc", true) => v(&["-preset", "p7", "-tune", "hq"]),
        ("libx265", true) => v(&["-preset", "slow"]),
        ("hevc_qsv", true) => v(&["-preset", "veryslow"]),
        ("hevc_amf", true) => v(&["-quality", "quality"]),
        _ => vec![],
    }
}

/// The pipes' scale of the stabilized frames to --resolution's size (empty: none), by --interpolation's filter
pub(crate) fn scale_filter(from: (usize, usize), to: (usize, usize), interp: Interpolation) -> String {
    if from == to { return String::new(); }
    let flags = match interp { Interpolation::Bilinear => "bilinear", Interpolation::Bicubic => "bicubic", _ => "lanczos" };
    format!("scale={}:{}:flags={flags}", to.0, to.1)
}

pub(crate) fn encoder(name: &str, prores: bool) -> Encoder {
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
pub(crate) fn hwaccel_works(path: &Path) -> bool {
    ffmpeg_command("ffmpeg").args(["-nostdin", "-v", "error", "-hwaccel", "auto", "-i"]).arg(path)
        .args(["-map", "0:v:0", "-frames:v", "1", "-f", "null", "-"])
        .stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null())
        .status().is_ok_and(|s| s.success())
}

pub(crate) fn pick_encoder(opts: &Options, prores: bool) -> Result<Encoder, String> {
    static PICKED: std::sync::OnceLock<Option<String>> = std::sync::OnceLock::new();
    if let Some(name) = &opts.encoder { return Ok(encoder(name, prores)); }
    if cfg!(target_os = "macos") {
        return Ok(encoder(if prores { "prores_videotoolbox" } else { "hevc_videotoolbox" }, prores));
    }
    if prores { return Ok(encoder("prores_ks", true)); }
    let candidates: &[&str] = if cfg!(windows) { &["hevc_nvenc", "hevc_qsv", "hevc_amf", "libx265"] }
                              else { &["hevc_nvenc", "hevc_vaapi", "hevc_qsv", "libx265"] };
    let picked = PICKED.get_or_init(|| candidates.iter().find(|&&n| encoder_works(&encoder(n, false))).map(|n| n.to_string()));
    match picked {
        Some(name) => Ok(encoder(name, false)),
        None => Err("ffmpeg has no HEVC encoder that works here (hevc_nvenc, hevc_qsv, hevc_amf, hevc_vaapi, libx265)".into()),
    }
}

// Where an export goes by default: its project's _Exports/ (docs/design/moments-and-workspaces.md), else stabilized/ next to
// the video. A project is never made here: the player initializes one where its user chooses
pub(crate) fn default_out_dir(video: &Path) -> PathBuf {
    match vs_gyroflow::workspace::project_root(video) {
        Some(root) => root.join("_Exports"),
        None => video.parent().unwrap_or(Path::new(".")).join("stabilized"),
    }
}

// A preview's H.264 encoder: x264 first, which spends next to nothing on a frame repeated (some 40 bytes a frame,
// VideoToolbox some KB), then the hardware ones. One keyframe: the first frame is most of the file (100-200 KB at
// 960x540 for a camera's frame, its noise included), and a keyframe every 10 s, x264's default, made it 1 MB for 30 s
pub(crate) fn pick_h264() -> Result<Encoder, String> {
    static PICKED: std::sync::OnceLock<Option<String>> = std::sync::OnceLock::new();
    let candidates = ["libx264", "h264_videotoolbox", "h264_nvenc", "h264_qsv", "h264_amf", "h264_mf"];
    // VideoToolbox is on every Mac: taken without the test encode, which costs 0.4-0.9 s a preview (a VideoToolbox
    // session of its own, slower while the player plays), most of a moment saved from its picture
    let picked = PICKED.get_or_init(|| candidates.iter().copied()
        .filter(|n| *n != "h264_videotoolbox" || cfg!(target_os = "macos"))
        .find(|n| *n == "h264_videotoolbox" || encoder_works(&h264(n))).map(str::to_string));
    picked.as_deref().map(h264).ok_or_else(|| format!("ffmpeg has no H.264 encoder that works here ({})", candidates.join(", ")))
}

fn h264(name: &str) -> Encoder {
    let v = |a: &[&str]| a.iter().map(|x| x.to_string()).collect::<Vec<_>>();
    // For the first video stream only: the cover art is the second, copied
    let args = match name {
        "libx264" => v(&["-preset:v:0", "medium", "-crf:v:0", "26", "-g:v:0", "100000", "-profile:v:0", "high"]),
        _ => v(&["-b:v:0", "150k", "-g:v:0", "100000"]),
    };
    Encoder { name: name.to_string(), global: vec![], args, pix_fmt: "yuv420p", upload: "" }
}

// The size of a preview: the longer side 960 at most, even
pub(crate) fn preview_size(w: usize, h: usize) -> (usize, usize) {
    let scale = (960.0 / w.max(h) as f64).min(1.0);
    let even = |x: f64| ((x / 2.0).round() as usize * 2).max(2);
    (even(w as f64 * scale), even(h as f64 * scale))
}
