// SPDX-License-Identifier: GPL-3.0-or-later
//
// `gyroflow-export <video>...`: renders videos the way mpv plays them, stabilized and with the D-Log M LUT, in one
// pass: decode -> Gyroflow (the plugin's own engine, on the GPU) -> LUT -> encode. The source is decoded and
// encoded once, so there's no intermediate file to lose quality in.
//
// It picks the stabilization the same way the gyroflow mpv plugin does (a .gyroflow next to the video, a project
// embedded in it, or the clip's own motion data with the default preset) and the LUT from the color profile in
// the camera's metadata, as the dlogm plugin does. Clips there's nothing to stabilize with are skipped.
//
// Installed in the gyroflow plugin's folder (~~/scripts/gyroflow/), it takes the default preset from there and the
// LUTs from the dlogm plugin's (~~/scripts/dlogm/luts/); $GYROFLOW_DATA and $DLOGM_DATA, when set, come first,
// as for the plugins (Steady Player keeps your preset and DJI's LUT there, its own copy of the plugins being
// read-only).
//
// Output: <video's folder>/stabilized/<name>_stab.mov (with --from/--to, <name>_stab_<from>-<to>s.mov), 10-bit
//   D-Log M:        HEVC Main10, Rec.2100 HLG (luts/dlogm_hlg.cube), or Rec.709 with --sdr (DJI's LUT)
//   anything else:  HEVC Main10 in the source's own colors
//   --prores:       ProRes 422 HQ instead of HEVC
// Audio is copied as it is; the file gets the source's dates.
//
// Decoding and encoding are ffmpeg (Homebrew's), as child processes with raw frames over pipes; the LUT is ffmpeg's
// lut3d filter, in RGB, the same place mpv's shader applies it.
//
// For a player running it (the gyroflow plugin's export.lua): --progress writes the progress to a file twice a
// second; SIGTERM (or Ctrl+C) stops it, the ffmpegs too, and removes the unfinished file; SIGUSR1 puts it and the
// ffmpegs at background priority (macOS's lowest, CPU, disk and GPU: playback keeps its frame rate, the export
// nearly stops) and SIGUSR2 back.

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

Renders each video stabilized (and, for D-Log M, color-converted) into a stabilized/ folder next to it.

  --sdr            D-Log M to Rec.709 (DJI's LUT) instead of HLG
  --prores         ProRes 422 HQ instead of HEVC 10-bit
  --from <time>    start at this time (seconds, or m:ss)
  --to <time>      end at this time
  --out <folder>   write here instead of <video's folder>/stabilized
  --force          replace an existing output instead of skipping the video
  --interpolation  bilinear | bicubic | lanczos (default lanczos)
  --progress <file>  write the progress there as JSON, twice a second
  --background     start at background priority (SIGUSR1 / SIGUSR2 switch it later)";

struct Options {
    sdr: bool,
    prores: bool,
    from: Option<f64>,
    to: Option<f64>,
    out: Option<PathBuf>,
    force: bool,
    interpolation: Interpolation,
    progress: Progress,
}

fn main() {
    vs_gyroflow::init_logging();
    catch_signals();
    let mut opts = Options { sdr: false, prores: false, from: None, to: None, out: None, force: false,
                             interpolation: Interpolation::Lanczos4, progress: Progress(None) };
    let mut files = Vec::new();
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        let mut value = |name: &str| args.next().unwrap_or_else(|| fail(&format!("{name} needs a value")));
        match a.as_str() {
            "--sdr" => opts.sdr = true,
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
            "--progress" => opts.progress = Progress(Some(PathBuf::from(value("--progress")))),
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
        libc::SIGUSR1 => BACKGROUND.store(true, Relaxed),
        libc::SIGUSR2 => BACKGROUND.store(false, Relaxed),
        _ => STOP.store(sig, Relaxed),
    }
}

fn catch_signals() {
    for sig in [libc::SIGINT, libc::SIGTERM, libc::SIGHUP, libc::SIGUSR1, libc::SIGUSR2] {
        unsafe { libc::signal(sig, on_signal as extern "C" fn(libc::c_int) as libc::sighandler_t); }
    }
}

fn stopped() -> bool { STOP.load(Relaxed) != 0 }

// PRIO_DARWIN_BG, for these processes (0: this one). Set on the ffmpegs too: the decoding and encoding compete
// with the player's as much as the GPU work does
fn set_background(pids: &[u32], on: bool) {
    for &pid in pids {
        unsafe { libc::setpriority(libc::PRIO_DARWIN_PROCESS, pid as libc::id_t, if on { libc::PRIO_DARWIN_BG } else { 0 }); }
    }
}

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

fn mpv_dir() -> PathBuf {
    std::env::var_os("MPV_CONFIG_DIR").map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(std::env::var("HOME").unwrap_or_default()).join(".config/mpv"))
}

// A file of one of the mpv plugins: in $<PLUGIN>_DATA when it's there, else in the scripts folder this binary is
// installed in (~~/scripts/gyroflow/), or in mpv's when it runs from elsewhere (a build)
fn plugin_file(plugin: &str, name: &str) -> PathBuf {
    if let Some(data) = std::env::var_os(format!("{}_DATA", plugin.to_uppercase())).filter(|d| !d.is_empty()) {
        let p = PathBuf::from(data).join(name);
        if p.is_file() { return p; }
    }
    let here = std::env::current_exe().ok().and_then(|e| e.canonicalize().ok())
        .and_then(|e| Some(e.parent()?.parent()?.to_path_buf()))
        .filter(|scripts| scripts.join("gyroflow").join("gyroflow-export").is_file());
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
    let out = Command::new(tool("ffprobe")).args(["-v", "error", "-select_streams", "v:0", "-show_entries",
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

// Homebrew's ffmpeg: a Finder Quick Action or mpv doesn't have /opt/homebrew/bin in its PATH
fn tool(name: &str) -> PathBuf {
    for dir in ["/opt/homebrew/bin", "/usr/local/bin"] {
        let p = Path::new(dir).join(name);
        if p.is_file() { return p; }
    }
    PathBuf::from(name)
}

fn lut(name: &str) -> Result<PathBuf, String> {
    let p = plugin_file("dlogm", &format!("luts/{name}"));
    if p.is_file() { Ok(p) } else { Err(format!("{} is missing (install the dlogm plugin: install.sh)", p.display())) }
}

// ffmpeg's filter syntax: quote the path for the option value, then for the filter graph
fn filter_path(p: &Path) -> String {
    let s = p.to_string_lossy().replace('\\', "\\\\").replace('\'', "\\'").replace(':', "\\:");
    format!("'{s}'")
}

fn export(file: &str, opts: &Options) -> Result<Outcome, String> {
    let path = std::fs::canonicalize(file).map_err(|e| e.to_string())?;
    let source = match find_source(&path) {
        Ok(s) => s,
        Err(why) => return Ok(Outcome::Skipped(why)),
    };
    let color = probe::color_profile(&path.to_string_lossy());
    let info = ffprobe(&path)?;

    let out_dir = opts.out.clone().unwrap_or_else(|| path.parent().unwrap_or(Path::new(".")).join("stabilized"));
    let stem = path.file_stem().map(|x| x.to_string_lossy().to_string()).unwrap_or_default();
    // A part of the video gets its range in the name, so it doesn't replace the whole one
    let range = match (opts.from, opts.to) {
        (None, None) => String::new(),
        (f, t) => format!("_{:.1}-{}s", f.unwrap_or(0.0), t.map(|t| format!("{t:.1}")).unwrap_or_else(|| "end".into())),
    };
    let stem = format!("{stem}_stab{range}");
    let out_path = out_dir.join(format!("{stem}.mov"));
    if out_path.exists() && !opts.force {
        return Ok(Outcome::Skipped(format!("{} exists (--force replaces it)", out_path.display())));
    }
    std::fs::create_dir_all(&out_dir).map_err(|e| format!("{}: {e}", out_dir.display()))?;
    let tmp_path = out_dir.join(format!(".{stem}.partial.mov"));

    // Color: the LUT works in RGB (mpv's shader too: it reads MAIN, the decoded RGB), from the source's BT.709
    // matrix and range, and the result is converted back to YUV in the target's matrix. On the GPU (gpu_lut.rs)
    // when the output size allows, else ffmpeg's lut3d on the CPU, at a third of the speed
    let in_range = if info.full_range { "pc" } else { "tv" };
    let mut lut_job = None;
    let (graph, tags, color_label) = if color == Some("dlogm") {
        let (cube, matrix, primaries, transfer, label) = if opts.sdr {
            (lut("dji-dlogm-rec709.cube")?, "bt709", "bt709", "bt709", "D-Log M -> Rec.709")
        } else {
            (lut("dlogm_hlg.cube")?, "bt2020", "bt2020", "arib-std-b67", "D-Log M -> HLG")
        };
        let graph = format!("scale=in_color_matrix=bt709:in_range={in_range},format=gbrp16le,lut3d=file={}:interp=tetrahedral,\
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
        (String::new(), tags, "colors as recorded")
    };

    let fps = info.fps.0 as f64 / info.fps.1 as f64;
    let from = opts.from.unwrap_or(0.0).max(0.0);
    let to = opts.to.unwrap_or(info.duration).min(info.duration.max(from));
    // With -ss before -i, ffmpeg starts at the first frame at or after `from`
    let first_frame = (from * fps - 1e-6).ceil().max(0.0) as u64;
    let total_frames = if opts.from.is_some() || opts.to.is_some() { ((to - from) * fps).round().max(1.0) as u64 } else { info.frames.max(1) };

    // The engine, as the plugin sets it up for mpv: 10-bit YUV 4:2:0, the project's output size
    let (w, h) = (info.width & !1, info.height & !1);
    let workers = 3;
    let key = format!("export|{}", path.display());
    let engine = Arc::new(Engine::load(key, &source.project, source.preset.as_deref(), (w, h), 3, (1, 1), None,
                                       SampleKind::U16, 1023.0, true, opts.interpolation, workers)?);
    let (ow, oh) = (engine.render_size.0 & !1, engine.render_size.1 & !1);

    let mut graph = graph;
    let gpu_lut = match lut_job {
        None => None,
        Some((cube, matrix, cpu_graph)) => {
            let out_matrix = gpu_lut::matrix_coefficients(matrix).unwrap();
            match GpuLut::new(&cube, ow, oh, gpu_lut::matrix_coefficients("bt709").unwrap(), info.full_range, out_matrix) {
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
    eprintln!("{}: {color_label} · {} · {}x{} -> {ow}x{oh} · {duration_label}", file_name(file), source.label, w, h);

    if stopped() { return Err("cancelled".into()); }

    // Decoder: raw 10-bit planar frames on stdout
    let mut dec_cmd = Command::new(tool("ffmpeg"));
    dec_cmd.args(["-nostdin", "-v", "error", "-hwaccel", "videotoolbox"]);
    if opts.from.is_some() { dec_cmd.args(["-ss", &format!("{from:.6}")]); }
    if opts.to.is_some() { dec_cmd.args(["-to", &format!("{to:.6}")]); }
    dec_cmd.arg("-i").arg(&path)
        .args(["-map", "0:v:0", "-vf", &format!("scale={w}:{h}"), "-f", "rawvideo", "-pix_fmt", "yuv420p10le", "-"])
        .stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped());
    let mut decoder = dec_cmd.spawn().map_err(|e| format!("ffmpeg: {e}"))?;

    // Encoder: the stabilized frames, audio and metadata from the source
    let bit_rate = info.bit_rate.unwrap_or(80_000_000) as f64 * (ow * oh) as f64 / (w * h) as f64;
    let mut enc_cmd = Command::new(tool("ffmpeg"));
    enc_cmd.args(["-nostdin", "-v", "error", "-y",
                  "-f", "rawvideo", "-pix_fmt", "yuv420p10le", "-s", &format!("{ow}x{oh}"),
                  "-framerate", &format!("{}/{}", info.fps.0, info.fps.1),
                  "-color_range", in_range, "-colorspace", "bt709", "-color_primaries", "bt709", "-color_trc", "bt709",
                  "-i", "-"]);
    if opts.from.is_some() { enc_cmd.args(["-ss", &format!("{from:.6}")]); }
    if opts.to.is_some() { enc_cmd.args(["-to", &format!("{to:.6}")]); }
    enc_cmd.arg("-i").arg(&path)
        .args(["-map", "0:v", "-map", "1:a?", "-map_metadata", "1", "-c:a", "copy"]);
    let pix_fmt = if opts.prores { "p210le" } else { "p010le" };
    // The frames carry the input's BT.709 tags through the filters, and the encoder writes the frames' tags, not
    // the -color_* options, so the target's are set on the frames
    let setparams = format!("setparams=color_primaries={}:color_trc={}:colorspace={}:range={}", tags[0], tags[1], tags[2], tags[3]);
    let graph = if graph.is_empty() { format!("format={pix_fmt},{setparams}") } else { format!("{graph},format={pix_fmt},{setparams}") };
    enc_cmd.args(["-vf", &graph]);
    if opts.prores {
        enc_cmd.args(["-c:v", "prores_videotoolbox", "-profile:v", "hq", "-tag:v", "apch"]);
    } else {
        enc_cmd.args(["-c:v", "hevc_videotoolbox", "-profile:v", "main10", "-tag:v", "hvc1", "-b:v", &format!("{}", bit_rate as u64)]);
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
    let frame_offset_us = engine.frame_offset_us;
    let gpu_threads: Vec<_> = (0..workers).map(|_| {
        let (frame_rx, done_tx, engine) = (frame_rx.clone(), done_tx.clone(), engine.clone());
        let full_range = info.full_range;
        let mut lut = gpu_lut.as_ref().map(|l| l.worker());
        std::thread::spawn(move || {
            loop {
                let Ok((n, src)) = frame_rx.lock().recv() else { break };
                let mut dst = vec![0u8; out_frame];
                // Frame times from the start of the file, as Gyroflow and mpv count them
                let timestamp_us = (((first_frame + n) as f64 / fps) * 1_000_000.0).round() as i64 + frame_offset_us;
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
                let result = engine.process(timestamp_us, planes, !full_range)
                    .and_then(|_| match lut.as_mut() { Some(l) => l.apply(&mut dst), None => Ok(()) })
                    .map(|_| dst);
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
    for (n, result) in done_rx {
        if stopped() { error = Some("cancelled".to_string()); break; }
        let want = BACKGROUND.load(Relaxed);
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
