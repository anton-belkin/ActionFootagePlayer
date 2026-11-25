// The GPU path: the player's warp shader run by ffmpeg's libplacebo filter.
// One of gyroflow-export's parts: main.rs has the uses and main(); each part's items are the program's.

use super::*;

/// The player's GPU path: up to this frame rate its interpolation is Lanczos, above it bicubic (mpv/scripts/gyroflow/
/// main.lua's INTERPOLATION_SLOW, INTERPOLATION_FAST, SLOW_MAX_FPS; gyroflow-gpu-table maps "bicubic" to bicubic9)
pub(crate) const PLAYER_SLOW_MAX_FPS: f64 = 31.0;

/// The player's GPU warp (gyroflow-gpu-table --player, gpu_table::Options::player) for this export's clip as the
/// engine renders it: its view at the level, the camera, the edges asked for, the fringes where some frame has the
/// lens under water, the translation compensation kept for it (--with-translation), the temporal denoise's passes
/// (`denoise` frames each side, 0 none) at warp.glsl's INTERP `interp`; the first frame rendered `first_frame`.
/// Always the RGB grid layout: the planes' (what the player uses where the colors aren't read apart) is within 66 dB
/// of it and slower in ffmpeg's filter. Returns the shader and its output size
#[allow(clippy::too_many_arguments)]
pub(crate) fn warp_shader(engine: &Engine, opts: &Options, key: &str, view: u8, camera: vs_gyroflow::Camera, fps: f64, first_frame: u64, edges: EdgeMode,
               interp: i32, denoise: usize) -> Result<(String, (usize, usize)), String> {
    use vs_gyroflow::gpu_table::{ self, Clip, Table };
    let index = engine.views.index(view, opts.level).min(engine.views.params.len() - 1);
    let params = camera.apply(&engine.views.params[index]);
    let clip = Clip { size: (params.width, params.height), output_size: (params.output_width, params.output_height), fps,
                      frame_count: params.frame_count.max(1), frame_offset_us: engine.frame_offset_us, params };
    if let Some(why) = gpu_table::unsupported(&clip) { return Err(format!("the GPU warp can't render it: {why}")); }
    let wet = opts.chromatic && (0..clip.frame_count).step_by(15).any(|f| clip.transform(f, clip.output_size).kernel_params.light_refraction_coefficient > 1.0);
    let edge_fill = match edges { EdgeMode::Off => None, _ => Some(opts.edge_settings.blur) };
    let sphere = edges == EdgeMode::Sphere && edge_fill.is_some();
    let mut o = gpu_table::Options::player(interp, wet, edge_fill, sphere, denoise > 0, first_frame as f64 / fps);
    o.mode = gpu_table::Mode::Grid;
    let table = Table::build(&clip, &o)?;
    let kept = || vs_gyroflow::translation::cached(key).filter(|v| v.len() == clip.frame_count);
    let trans = if opts.with_translation {
        Some(kept().ok_or("--with-translation: no translation compensation computed for this clip (gyroflow-export --translation)")?)
    } else if opts.translation_kept { kept() } else { None };
    if trans.is_some() { eprintln!("  with the sideways jerks taken out (Ctrl+j)"); }
    let mut text = gpu_table::shader_with(&clip, &o, &table, trans.as_deref());
    if denoise > 0 {
        // What the player sets for its clean frame (glsl-shader-opts), here the parameters' defaults
        for (name, v) in [("gyroflow_dn_radius", denoise as f64), ("gyroflow_dn_after", denoise as f64),
                          ("gyroflow_dn_strong", opts.denoise_strong as i32 as f64), ("gyroflow_dn_keep", opts.photo_grain as f64)] {
            text = gpu_table::set_param_default(&text, name, v)?;
        }
    }
    Ok((text, clip.output_size))
}

/// How the GPU path's ffmpeg decodes and encodes here, found once (gpu_setup)
#[derive(Clone)]
pub(crate) struct GpuSetup {
    /// Before the inputs: a device for the filter and the encoder
    global: Vec<String>,
    /// Before the video's input: its hardware decoder, its frames left on the GPU
    decode: Vec<String>,
    /// After the filter, before the encoder: nothing where the encoder takes the filter's frames as they are
    to_encoder: String,
    /// The encoder, where it isn't the export's own (Vulkan Video's HEVC)
    encoder: Option<&'static str>,
    label: String,
}

/// `skip`: the filter's first frames not exported (the denoise's frames before); `pad`: (the decoded frame, how many
/// times) repeated at the end, the denoise's frames after the clip's last
/// `pre`: filters before libplacebo (a turn), `crop`: the filter's own crop options (`:crop_w=…`); `shader` empty: none
pub(crate) struct GpuPlan { pub(crate) setup: GpuSetup, pub(crate) shader: String, pub(crate) pre: String, pub(crate) crop: String, pub(crate) first_decoded: u64, pub(crate) skip: u64, pub(crate) pad: Option<(u64, u64)> }

/// A clip turned (and cut) without the warp, on the GPU: the turn as filters on the decoder's frames (VideoToolbox's
/// transpose_vt on a Mac, Vulkan's transpose elsewhere; a 180 degree turn as two flips there), the cut as the
/// libplacebo filter's crop of the upright frame. `size`: the decoded frame's, as the sensor has it
pub(crate) fn gpu_turn(frame: &vs_gyroflow::Frame, size: (usize, usize)) -> (String, String) {
    let mac = cfg!(target_os = "macos");
    let pre = match (frame.rotation, mac) {
        (90, true) => "transpose_vt=dir=clock,", (180, true) => "transpose_vt=dir=reversal,", (270, true) => "transpose_vt=dir=cclock,",
        (90, false) => "transpose_vulkan=dir=clock,", (180, false) => "transpose_vulkan=dir=clock,transpose_vulkan=dir=clock,",
        (270, false) => "transpose_vulkan=dir=cclock,",
        _ => "",
    }.to_string();
    let (w, h) = frame.size(size);
    let crop = if (w, h) != frame.rotated(size) {
        let (x, y) = frame.offset(size);
        format!(":crop_x={x}:crop_y={y}:crop_w={w}:crop_h={h}")
    } else { String::new() };
    (pre, crop)
}

/// The GPU path's ffmpeg: AFP_GPU_FFMPEG, else the one the export runs. On a Mac it loads Vulkan (MoltenVK) at run
/// time: the app's Frameworks, else Homebrew's
fn gpu_ffmpeg() -> Command {
    let mut cmd = match std::env::var_os("AFP_GPU_FFMPEG") { Some(p) => ffmpeg_at(PathBuf::from(p)), None => ffmpeg_command("ffmpeg") };
    if cfg!(target_os = "macos") {
        let mut dirs: Vec<PathBuf> = std::env::var_os("DYLD_FALLBACK_LIBRARY_PATH").map(|v| std::env::split_paths(&v).collect()).unwrap_or_default();
        // The app's (its ffmpeg's libraries are in Frameworks, relinked; MoltenVK's manifest in Resources)
        let contents = app_contents().or_else(|| std::env::var_os("GYROFLOW_TOOLS").and_then(|t| Some(Path::new(&t).parent()?.to_path_buf())));
        if let Some(contents) = contents {
            dirs.push(contents.join("Frameworks"));
            // MoltenVK for the Vulkan loader, as macos/player.sh points mpv to it, when this runs without it
            let icd = contents.join("Resources/vulkan/icd.d/MoltenVK_icd.json");
            if std::env::var_os("VK_DRIVER_FILES").is_none() && icd.is_file() { cmd.env("VK_DRIVER_FILES", &icd).env("VK_ICD_FILENAMES", &icd); }
        }
        dirs.extend(["/opt/homebrew/lib", "/usr/local/lib", "/usr/lib"].map(PathBuf::from));
        if let Ok(v) = std::env::join_paths(dirs) { cmd.env("DYLD_FALLBACK_LIBRARY_PATH", v); }
    }
    cmd
}

fn gpu_test(global: &[&str], vf: &str, encoder: &str) -> Result<(), String> {
    let out = gpu_ffmpeg().args(["-nostdin", "-hide_banner", "-v", "error"]).args(global)
        .args(["-f", "lavfi", "-i", "color=black:s=320x180:r=30", "-frames:v", "3", "-vf", vf, "-c:v", encoder, "-f", "null", "-"])
        .stdin(Stdio::null()).stdout(Stdio::null()).output().map_err(|e| format!("ffmpeg: {e}"))?;
    if out.status.success() { Ok(()) } else { Err(String::from_utf8_lossy(&out.stderr).lines().last().unwrap_or("failed").trim().to_string()) }
}

/// Whether this ffmpeg runs the GPU path, and how: on a Mac VideoToolbox's frames into the libplacebo filter and out
/// of it with no copies (tools/export-gpu's build of ffmpeg, patched); elsewhere Vulkan's (Vulkan Video's decoder and
/// HEVC encoder where the GPU's driver has them, else a copy each way)
pub(crate) fn gpu_setup(video: &Path) -> Result<GpuSetup, String> {
    static FOUND: std::sync::OnceLock<Result<GpuSetup, String>> = std::sync::OnceLock::new();
    let base = FOUND.get_or_init(|| {
        let help = gpu_ffmpeg().args(["-hide_banner", "-h", "filter=libplacebo"]).stdin(Stdio::null()).stderr(Stdio::null()).output()
            .map_err(|e| format!("ffmpeg: {e}"))?;
        if !String::from_utf8_lossy(&help.stdout).contains("custom_shader_path") { return Err("this ffmpeg has no libplacebo filter".into()); }
        let s = |v: &[&str]| v.iter().map(|x| x.to_string()).collect::<Vec<_>>();
        if cfg!(target_os = "macos") {
            gpu_test(&["-init_hw_device", "videotoolbox=vt", "-filter_hw_device", "vt"], "format=p010le,hwupload,libplacebo=format=p010le", "hevc_videotoolbox")
                .map_err(|e| format!("this ffmpeg's libplacebo filter doesn't take VideoToolbox frames (tools/export-gpu's build does): {e}"))?;
            return Ok(GpuSetup { global: vec![], decode: s(&["-hwaccel", "videotoolbox", "-hwaccel_output_format", "videotoolbox_vld"]),
                                 to_encoder: String::new(), encoder: None, label: "VideoToolbox -> Vulkan (MoltenVK) -> VideoToolbox, no copies".into() });
        }
        let vk = ["-init_hw_device", "vulkan=vk", "-filter_hw_device", "vk"];
        gpu_test(&vk, "format=p010le,hwupload,libplacebo=format=p010le", "rawvideo").map_err(|e| format!("no Vulkan device for the libplacebo filter: {e}"))?;
        let direct = gpu_test(&vk, "format=p010le,hwupload,libplacebo=format=p010le", "hevc_vulkan").is_ok();
        Ok(GpuSetup { global: s(&vk), decode: vec![], to_encoder: if direct { String::new() } else { ",hwdownload,format=p010le".into() },
                      encoder: direct.then_some("hevc_vulkan"),
                      label: if direct { "Vulkan -> Vulkan Video's HEVC encoder".into() } else { "Vulkan, a copy to the encoder".into() } })
    });
    let mut setup = base.clone()?;
    if !cfg!(target_os = "macos") {
        // The video decoded by Vulkan Video where the driver can, its frames staying there; else decoded as it can
        // be and uploaded (a copy)
        let decodes = gpu_ffmpeg().args(["-nostdin", "-v", "error"]).args(&setup.global).args(["-hwaccel", "vulkan", "-hwaccel_output_format", "vulkan", "-i"])
            .arg(video).args(["-map", "0:v:0", "-frames:v", "1", "-f", "null", "-"])
            .stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).status().is_ok_and(|s| s.success());
        if decodes {
            setup.decode = ["-hwaccel", "vulkan", "-hwaccel_output_format", "vulkan"].map(String::from).to_vec();
            setup.label = format!("Vulkan Video's decoder -> {}", setup.label);
        } else {
            setup.label = format!("decoded, uploaded (a copy) -> {}", setup.label);
        }
    }
    Ok(setup)
}

/// What export() hands the GPU path
pub(crate) struct GpuJob<'a> {
    pub(crate) path: &'a Path, pub(crate) info: &'a VideoInfo, pub(crate) opts: &'a Options, pub(crate) tags: &'a [String], pub(crate) enc: &'a Encoder, pub(crate) prores: bool,
    pub(crate) size: (usize, usize), pub(crate) fps: f64, pub(crate) out_rate: (u64, u64),
    /// Where the decoder starts (s) and stops (s, None: the end); whether it seeks at all
    pub(crate) seek: f64, pub(crate) until: Option<f64>, pub(crate) from_set: bool,
    /// The export's own start (the audio's) and end, as asked
    pub(crate) video_start: f64, pub(crate) to: f64, pub(crate) opt_from: Option<f64>, pub(crate) opt_to: Option<f64>,
    pub(crate) total_frames: u64, pub(crate) first_frame: u64,
    pub(crate) tmp_path: &'a Path, pub(crate) out_path: &'a Path, pub(crate) in_range: &'a str,
    pub(crate) dv_rpus: Option<Vec<Vec<u8>>>, pub(crate) dv_rpu: Option<&'a PathBuf>, pub(crate) started: Instant,
}

/// The GPU path: one ffmpeg decoding the video (its frames left on the GPU), running the player's shaders in the
/// libplacebo filter (`plan.shader`: the warp, its edges, fringes and denoise, then the look, as the player has them)
/// and encoding, with the audio and metadata from the source as the pipes' encoder takes them. Its output, progress,
/// stop, background, Dolby Vision, link and dates as export()'s
pub(crate) fn export_gpu(job: GpuJob, plan: GpuPlan) -> Result<Outcome, String> {
    let GpuJob { path, info, opts, tags, enc, prores, size: (w, h), fps, out_rate, seek, until, from_set, video_start, to, opt_from, opt_to,
                 total_frames, first_frame, tmp_path, out_path, in_range, dv_rpus, dv_rpu, started } = job;
    eprintln!("  on the GPU: {}", plan.setup.label);
    // The shaders' file, next to the output, gone with the export (AFP_GPU_KEEP keeps it)
    let shader_path = tmp_path.with_extension("glsl");
    std::fs::write(&shader_path, &plan.shader).map_err(|e| format!("{}: {e}", shader_path.display()))?;
    let _shader = (std::env::var_os("AFP_GPU_KEEP").is_none()).then(|| TempFile(shader_path.clone()));

    let encoder = plan.setup.encoder.unwrap_or(&enc.name);
    let pix_fmt = if prores { "p210le" } else { "p010le" };
    let range = if tags[3] == "pc" { "pc" } else { "tv" };
    // The frames tagged as what the shaders' output is: libplacebo then passes MAIN through (as the dlogm plugin tags
    // the video for mpv); the matrix and range stay the source's, for its conversion to RGB
    let mut vf = plan.pad.map(|(at, n)| format!("loop=loop={n}:size=1:start={at},")).unwrap_or_default();
    let shader_opt = if plan.shader.is_empty() { String::new() } else { format!(":custom_shader_path={}", filter_path(&shader_path)) };
    vf += &format!("{}setparams=color_primaries={}:color_trc={},libplacebo=w={w}:h={h}:format={pix_fmt}:colorspace={}:color_primaries={}:color_trc={}:\
                          range={range}:dithering=none{}{shader_opt}", plan.pre, tags[0], tags[1], tags[2], tags[0], tags[1], plan.crop);
    // The denoise shows a frame `denoise` frames behind the newest: its first ones are the frames before the export's
    if plan.skip > 0 { vf += &format!(",trim=start_frame={}", plan.skip); }
    vf += ",setpts=PTS-STARTPTS";
    vf += &plan.setup.to_encoder;
    let _ = (in_range, plan.first_decoded);

    let mut cmd = gpu_ffmpeg();
    cmd.args(["-nostdin", "-hide_banner", "-v", "error", "-nostats", "-progress", "pipe:2", "-stats_period", "0.5", "-y"]).args(&plan.setup.global).args(&plan.setup.decode);
    if from_set || opt_from.is_some() { cmd.args(["-ss", &format!("{seek:.6}")]); }
    if let Some(u) = until { cmd.args(["-to", &format!("{u:.6}")]); }
    cmd.args(["-noautorotate", "-i"]).arg(path);
    // The audio and metadata from the source, from the export's first frame's time, as the pipes' encoder has them
    if dv_rpus.is_none() {
        if opt_from.is_some() { cmd.args(["-ss", &format!("{video_start:.6}")]); }
        if opt_to.is_some() { cmd.args(["-to", &format!("{to:.6}")]); }
        cmd.arg("-i").arg(path).args(["-map", "0:v:0", "-map", "1:a?", "-map_metadata", "1", "-c:a", "copy"]);
    } else {
        cmd.args(["-map", "0:v:0"]);
    }
    cmd.args(["-frames:v", &total_frames.to_string(), "-vf", &vf, "-c:v", encoder]);
    if plan.setup.encoder.is_none() { cmd.args(&enc.args); } else { cmd.args(["-profile:v", "main10"]); }
    // --encode: VideoToolbox's real-time mode by default, its quality mode with quality (encode_mode_args)
    if !prores { cmd.args(encode_mode_args(encoder, opts.encode_quality, true)); }
    let bit_rate = export_bit_rate(info.bit_rate, info.width * info.height, w * h, opts.encode_quality);
    if prores { cmd.args(["-tag:v", "apch"]); } else { cmd.args(["-tag:v", "hvc1", "-b:v", &format!("{}", bit_rate as u64)]); }
    cmd.args(["-color_primaries", &tags[0], "-color_trc", &tags[1], "-colorspace", &tags[2], "-color_range", &tags[3]]);
    let moov_size = 512 * 1024 + total_frames * 64;
    if dv_rpus.is_none() && opts.link.is_some() {
        cmd.args(["-moov_size", &moov_size.to_string(), "-f", "mov"]).arg(tmp_path).stdout(Stdio::null());
    } else if dv_rpus.is_none() {
        cmd.args(["-movflags", "+faststart", "-f", "mov"]).arg(tmp_path).stdout(Stdio::null());
    } else {
        cmd.args(["-bf", "0", "-f", "hevc", "-"]).stdout(Stdio::piped());
    }
    cmd.stdin(Stdio::null()).stderr(Stdio::piped());
    if std::env::var_os("AFP_TIMES").is_some() { eprintln!("  {cmd:?}"); }
    let mut child = cmd.spawn().map_err(|e| format!("ffmpeg: {e}"))?;

    // Dolby Vision: the encoder's HEVC, its RPUs replaced with ours, into a MOV with the source's audio and metadata
    let color = [ "-color_primaries", &tags[0], "-color_trc", &tags[1], "-colorspace", &tags[2], "-color_range", &tags[3]];
    let mut muxer = None;
    let mut rewriter = None;
    if let Some(rpus) = dv_rpus {
        let mut mux_cmd = ffmpeg_command("ffmpeg");
        mux_cmd.args(["-nostdin", "-v", "error", "-y", "-f", "hevc", "-r", &format!("{}/{}", out_rate.0, out_rate.1), "-i", "-"]);
        if opt_from.is_some() { mux_cmd.args(["-ss", &format!("{video_start:.6}")]); }
        if opt_to.is_some() { mux_cmd.args(["-to", &format!("{to:.6}")]); }
        mux_cmd.arg("-i").arg(path)
            .args(["-map", "0:v", "-map", "1:a?", "-map_metadata", "1", "-c", "copy", "-tag:v", "hvc1"]).args(color)
            .args(["-moov_size", &moov_size.to_string(), "-f", "mov"]).arg(tmp_path)
            .stdin(Stdio::piped()).stdout(Stdio::null()).stderr(Stdio::piped());
        let mut mux = mux_cmd.spawn().map_err(|e| format!("ffmpeg: {e}"))?;
        let mut mux_in = std::io::BufWriter::with_capacity(1 << 20, mux.stdin.take().unwrap());
        let mut enc_out = child.stdout.take().unwrap();
        rewriter = Some(std::thread::spawn(move || -> Result<u64, String> {
            let mut injector = RpuInjector::new(rpus, first_frame);
            let mut buf = vec![0u8; 1 << 20];
            loop {
                let n = enc_out.read(&mut buf).map_err(|e| format!("reading the encoder: {e}"))?;
                if n == 0 { break; }
                injector.feed(&buf[..n], &mut mux_in).map_err(|e| format!("muxer stopped: {e}"))?;
            }
            injector.finish(&mut mux_in).and_then(|_| mux_in.flush()).map_err(|e| format!("muxer stopped: {e}"))?;
            Ok(injector.frames)
        }));
        muxer = Some(mux);
    }

    // ffmpeg's progress (frame=) and its errors, from its stderr
    let done = Arc::new(std::sync::atomic::AtomicU64::new(0));
    let errors = Arc::new(Mutex::new(String::new()));
    let reader = {
        let (done, errors, err) = (done.clone(), errors.clone(), child.stderr.take().unwrap());
        std::thread::spawn(move || {
            use std::io::BufRead;
            for line in std::io::BufReader::new(err).lines().map_while(Result::ok) {
                if let Some(n) = line.strip_prefix("frame=") { if let Ok(n) = n.trim().parse() { done.store(n, Relaxed); } continue; }
                if line.contains('=') && !line.contains(' ') { continue; }
                let mut e = errors.lock();
                if e.len() < 4000 { e.push_str(&line); e.push('\n'); }
            }
        })
    };
    let (mut last_report, mut last_control, mut reported, mut rate) = (Instant::now(), Instant::now(), 0u64, 0.0f64);
    let mut background = None;
    let status = loop {
        if let Some(control) = &opts.control {
            if last_control.elapsed().as_millis() >= 250 { read_control(control); last_control = Instant::now(); }
        }
        if stopped() {
            let _ = child.kill();
            if let Some(m) = muxer.as_mut() { let _ = m.kill(); }
            break None;
        }
        let want = BACKGROUND.load(Relaxed);
        if background != Some(want) {
            let mut pids = vec![0, child.id()];
            if let Some(m) = &muxer { pids.push(m.id()); }
            set_background(&pids, want);
            background = Some(want);
        }
        if let Some(s) = child.try_wait().map_err(|e| e.to_string())? { break Some(s); }
        if last_report.elapsed().as_millis() >= 500 {
            let d = done.load(Relaxed);
            let now = d.saturating_sub(reported) as f64 / last_report.elapsed().as_secs_f64();
            rate = if reported == 0 { now } else { rate * 0.7 + now * 0.3 };
            last_report = Instant::now();
            reported = d;
            let pct = (d as f64 / total_frames as f64 * 100.0).min(100.0);
            let eta = ((total_frames as f64 - d as f64).max(0.0) / rate.max(1e-3)).min(359_999.0) as u64;
            eprint!("\r  {pct:3.0}%  {rate:4.0} fps  ETA {}:{:02}   ", eta / 60, eta % 60);
            opts.progress.write(serde_json::json!({
                "pid": std::process::id(), "done": d, "total": total_frames, "fps": rate, "eta": eta, "background": want, "partial": tmp_path,
            }));
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    };
    let _ = reader.join();
    eprint!("\r{:60}\r", "");
    let rewritten = rewriter.map(|r| r.join().map_err(|_| "the RPU rewriter panicked".to_string()));
    let mux_status = match muxer.as_mut() { Some(m) => Some(m.wait().map_err(|e| e.to_string())?), None => None };
    let mux_err = muxer.as_mut().and_then(|m| m.stderr.take()).map(|mut e| { let mut s = String::new(); let _ = e.read_to_string(&mut s); s }).unwrap_or_default();
    let frames = done.load(Relaxed);
    let result = (|| -> Result<u64, String> {
        let Some(status) = status else { return Err("cancelled".into()) };
        if !status.success() { return Err(format!("ffmpeg (GPU path): {}", errors.lock().trim())); }
        if frames == 0 { return Err("no frames encoded".into()); }
        if let Some(r) = rewritten {
            let n = r??;
            if !mux_status.is_some_and(|s| s.success()) { return Err(format!("muxing: {}", mux_err.trim())); }
            if n != frames { return Err(format!("Dolby Vision: {n} pictures for {frames} frames")); }
            let (compatibility, profile) = if tags[1] == "smpte2084" { (1, "8.1") } else { (4, "8.4") };
            let level = dv_mux::patch_dvvc(tmp_path, fps, compatibility)?;
            eprintln!("  Dolby Vision {profile}, level {level}: {n} RPUs from {}", dv_rpu.map_or(String::new(), |p| p.display().to_string()));
        }
        if let Some(l) = &opts.link { vs_gyroflow::mp4meta::write_keys(tmp_path, &[(vs_gyroflow::mp4meta::LINK_KEY, l.as_str())])?; }
        Ok(frames)
    })();
    let frames = match result {
        Ok(f) => f,
        Err(e) => { let _ = std::fs::remove_file(tmp_path); return Err(e); }
    };
    // The source's dates, as export() sets them
    let modified = opts.dates.map(|d| d.system_time()).or_else(|| std::fs::metadata(path).and_then(|m| m.modified()).ok());
    if let (Some(modified), Ok(f)) = (modified, std::fs::File::options().write(true).open(tmp_path)) { let _ = f.set_modified(modified); }
    if out_path.exists() { copy_finder_attributes(out_path, tmp_path); }
    std::fs::rename(tmp_path, out_path).map_err(|e| e.to_string())?;
    let secs = started.elapsed().as_secs_f64();
    eprintln!("  {frames} frames in {:.0}:{:02.0} ({:.0} fps)", (secs / 60.0).floor(), secs % 60.0, frames as f64 / secs.max(1e-3));
    Ok(Outcome::Done(out_path.to_path_buf()))
}
