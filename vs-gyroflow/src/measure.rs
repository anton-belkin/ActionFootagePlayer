// SPDX-License-Identifier: GPL-3.0-or-later
//
// A clip's motion measured from its picture by the player itself, kept in its <stem>.afp-motion (afp_motion.rs): no
// Gyroflow app, no project in between (docs/design/optical-without-gyroflow.md, step 2).
//
// The measuring is Gyroflow's own: gyroflow-core's OpticalMotionAnalysis, the code behind Gyroflow's "Analyze image
// optically", the same KLT tracking and the same fit (StabilizationManager::set_optical_measurements). Only the frames
// come from here: decoded by ffmpeg as Gyroflow's app decodes them for it (8-bit luma, at most 960 px wide, unturned),
// each at its frame's time in the clip.
//
// What the clip is measured with is decided as the player plays it: its lens as lens_pick.rs picks it (the curated type
// given to it, the lens in its file, Gyroflow's database), the rolling shutter's readout that comes with the lens, and
// the lens under water as its own file says (source.underwater; else as the camera recorded, water.rs). Motion data of
// the file's own is set aside: a clip measured here has none, or one the camera stabilized the picture with already.
// Except on a HERO7 Black with HyperSmooth on (in_camera_turn.rs): its gyro is the camera body's, and what's measured
// is the turn HyperSmooth gave the picture on top of it.

use std::io::Read;
use std::path::Path;
use std::process::{ Command, Stdio };
use std::sync::{ Arc, atomic::AtomicBool };
use gyroflow_core::{ filesystem, gyro_source::{ OpticalCorrectionSettings, TrackingMethod }, synchronization::optical_motion::OpticalMotionAnalysis };
use crate::afp_motion::{ self, ClipId, Header, Measured, MotionFile };

/// Width the frames are tracked at, as Gyroflow's app tracks them (rendering's analyze_optically)
pub const TRACKED_WIDTH: usize = 960;

/// How it's measured
#[derive(Debug, Clone)]
pub struct Settings {
    /// How much of the measured motion is taken, 0-1 (1: all of it, as there's no other)
    pub strength: f64,
    /// "klt" (corners, the default) or "dis" (dense optical flow, for little texture: sky, water; slower)
    pub tracking: String,
    /// A lens profile (Gyroflow's JSON) to measure with instead of the one the player picks, its readout (ms,
    /// Gyroflow's sign) and the light's refraction (1 in air, 1.33 in water) instead of the clip's own
    pub lens: Option<serde_json::Value>,
    pub readout_ms: Option<f64>,
    pub refraction: Option<f64>,
    /// The camera's own stabilizer's turn measured on top of its gyro (in_camera_turn.rs): None as the clip is
    /// (`in_camera_turn::stabilizer_of`), Some(true) whatever it is (a HERO12's, to check against its IORI)
    pub in_camera_turn: Option<bool>,
}

impl Default for Settings {
    fn default() -> Self { Settings { strength: 1.0, tracking: "klt".into(), lens: None, readout_ms: None, refraction: None, in_camera_turn: None } }
}

/// Measures `video`'s motion from its picture. `progress`: the fraction done, now and then; `stop`: asked between
/// frames, true stops it ("cancelled")
pub fn measure(video: &Path, settings: &Settings, mut progress: impl FnMut(f64), stop: impl Fn() -> bool) -> Result<MotionFile, String> {
    let t0 = std::time::Instant::now();
    let path = video.to_string_lossy().into_owned();
    let url = filesystem::path_to_url(&path);
    let stab = crate::lens_pick::manager();
    // The camera's own stabilizer's turn measured on top of its gyro (in_camera_turn.rs), or the picture's motion alone
    let turn = match settings.in_camera_turn {
        None => crate::in_camera_turn::stabilizer_of(video),
        Some(true) => Some(crate::probe::in_camera_of(&path).unwrap_or_default()),
        Some(false) => None,
    };
    // The file as the plugin loads it: its metadata, the lens in it, the first seconds of any motion data it has; all of
    // it when the turn is measured against it
    let (m, size) = crate::motion::Motion::open(&path, &url, if turn.is_some() { 0.0 } else { 1.0 }).map_err(|e| format!("can't open it: {e}"))?;
    m.with_file(|file| stab.load_video_file(file, size, &url, None, true)).map_err(|e| format!("can't read it: {e:?}"))?;
    let mut pick = crate::lens_pick::apply(&stab, &path).map(|p| p.name);
    if let Some(lens) = &settings.lens {
        let mut l = stab.lens.write();
        l.load_from_json_value(lens);
        l.resolve_interpolations(&stab.lens_profile_db.read());
        pick = lens.get("name").and_then(|v| v.as_str()).map(String::from);
    }
    // A lens given without a readout of its own to measure with: the lens's (lens_pick::apply sets it for the one picked)
    let readout = settings.readout_ms.or_else(|| settings.lens.as_ref().and_then(|_| stab.lens.read().frame_readout_time));
    if let Some(r) = readout {
        use gyroflow_core::stabilization_params::ReadoutDirection;
        let mut p = stab.params.write();
        p.frame_readout_time = r;
        p.frame_readout_direction = if r < 0.0 { ReadoutDirection::BottomToTop } else { ReadoutDirection::TopToBottom };
    }
    if stab.lens.read().fisheye_params.camera_matrix.len() != 3 {
        return Err("its lens isn't known: without a lens profile the picture's motion can't be turned into the camera's".into());
    }
    stab.set_light_refraction_coefficient(settings.refraction.or_else(|| crate::water::refraction_for(video)).unwrap_or(1.0));
    if turn.is_some() {
        // The gyro is the body's, integrated as it plays (in_camera_turn::INTEGRATION), not the file's own orientations
        // if it has them; and the turn is the picture's, measured below
        stab.set_ignore_file_motion(false);
        if !stab.gyro.read().has_motion() { return Err("no gyro to measure the camera's own stabilizer's turn against".into()); }
        stab.set_integration_method(crate::in_camera_turn::INTEGRATION);
    } else {
        // The file's own motion data, if any, is of the picture before the camera stabilized it: set aside
        stab.set_ignore_file_motion(true);
    }
    *stab.optical_settings.write() = OpticalCorrectionSettings { strength: settings.strength.clamp(0.0, 1.0), per_frame: false };
    let tracking = TrackingMethod::from_name(&settings.tracking).ok_or_else(|| format!("tracking {:?}: klt or dis", settings.tracking))?;
    *stab.optical_tracking.write() = tracking;
    stab.recompute_undistortion();
    stab.recompute_gyro();

    let (w, h, fps, frames) = { let p = stab.params.read(); (p.size.0, p.size.1, p.fps, p.frame_count) };
    if w == 0 || h == 0 || fps <= 0.0 { return Err("no video size or frame rate".into()); }
    let mut analysis = OpticalMotionAnalysis::from_manager(&stab, Arc::new(AtomicBool::new(false)))?;
    let tw = w.min(TRACKED_WIDTH);
    let th = (((h as f64 * tw as f64 / w as f64) / 2.0).round() * 2.0) as usize;

    let mut cmd = Command::new(crate::tools::tool("ffmpeg"));
    cmd.args(["-nostdin", "-v", "error"]);
    if cfg!(target_os = "macos") { cmd.args(["-hwaccel", "videotoolbox"]); }
    // Every frame as the file has it (none doubled or dropped to a rate), unturned as Gyroflow takes it
    cmd.args(["-noautorotate", "-i"]).arg(video)
        .args(["-map", "0:v:0", "-fps_mode", "passthrough", "-vf", &format!("scale={tw}:{th}:flags=bicubic"), "-f", "rawvideo", "-pix_fmt", "gray", "-"])
        .stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped());
    #[cfg(windows)]
    { use std::os::windows::process::CommandExt; cmd.creation_flags(0x0800_0000); }
    let mut decoder = cmd.spawn().map_err(|e| format!("ffmpeg: {e}"))?;
    let mut out = decoder.stdout.take().ok_or("ffmpeg: no output")?;
    let mut frame = vec![0u8; tw * th];
    let mut n = 0usize;
    let mut last = std::time::Instant::now();
    loop {
        if stop() {
            let _ = decoder.kill();
            let _ = decoder.wait();
            return Err("cancelled".into());
        }
        if out.read_exact(&mut frame).is_err() { break; }
        // The frame's time in the clip, as the analysis numbers frames (ComputeParams::frame_at_timestamp)
        let timestamp_us = (n as f64 * 1_000_000.0 / fps).round() as i64;
        analysis.feed_frame(timestamp_us, tw as u32, th as u32, tw, &frame)?;
        n += 1;
        if last.elapsed().as_millis() >= 250 {
            last = std::time::Instant::now();
            let (ready, total) = analysis.progress();
            progress(ready as f64 / total.max(frames).max(1) as f64 * 0.98);
        }
    }
    let status = decoder.wait().map_err(|e| format!("ffmpeg: {e}"))?;
    if !status.success() || n == 0 {
        let mut err = String::new();
        if let Some(mut e) = decoder.stderr.take() { let _ = e.read_to_string(&mut err); }
        return Err(format!("ffmpeg couldn't decode it: {}", err.lines().last().unwrap_or("no frames")));
    }
    let decoded = t0.elapsed();
    let measurements = analysis.finish()?;
    let (correction, turn, port_axis) = match &turn {
        Some(stabilizer) => {
            let (c, t, axis) = crate::in_camera_turn::fit(&stab, &measurements, stabilizer, &stop)?;
            log::info!("{path}: {stabilizer}'s turn {:.2}° rms, {:.2}° from the picture, rounds {:?}; the port's axis from the bending in {:.0} % of the frames", t.rms_deg, t.from_picture_rms_deg, t.rounds_deg, t.tilt_share * 100.0);
            (c, Some(t), axis)
        }
        None => {
            stab.set_optical_measurements(measurements)?;
            (stab.gyro.read().optical_correction.as_deref().cloned().ok_or("the analysis gave no correction")?, None, None)
        }
    };
    progress(1.0);

    let lens = stab.lens.read().get_json_value().unwrap_or_default();
    let p = stab.params.read();
    let mut clip = ClipId::of(video)?;
    // The frame timing exactly as the analysis had it (its context has the frames' times in it)
    clip.width = w; clip.height = h; clip.fps = fps; clip.frames = frames; clip.duration_ms = p.duration_ms;
    let header = Header {
        clip,
        lens_name: pick.or_else(|| lens.get("name").and_then(|v| v.as_str()).map(String::from)).unwrap_or_else(|| "in the file".into()),
        lens,
        readout_ms: p.frame_readout_time,
        readout_direction: format!("{:?}", p.frame_readout_direction),
        refraction: p.light_refraction_coefficient,
        frame_offset: p.frame_offset as i64,
        offsets: stab.gyro.read().get_offsets().clone(),
        ignore_file_motion: turn.is_none(),
        in_camera_turn: turn,
        measured: Measured {
            how: "measured".into(),
            from: String::new(),
            at: std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or_default(),
            tool: format!("vs-gyroflow {}", env!("CARGO_PKG_VERSION")),
            tracking: correction.tracking.name().into(),
            strength: correction.settings.strength,
            per_frame: correction.settings.per_frame,
            frames: correction.frames,
            measured_frames: correction.measured_frames,
            rms_deg: correction.rms_deg,
        },
        ..Default::default()
    };
    drop(p);
    log::info!("{path}: motion measured from the picture: {n} frames at {tw}x{th} decoded and tracked in {:.1} s, fitted in {:.1} s; {} of {} frames measured",
        decoded.as_secs_f64(), (t0.elapsed() - decoded).as_secs_f64(), correction.measured_frames, correction.frames);
    MotionFile::new(header, correction)?.with_port_axis(port_axis)
}

/// `measure`, written to the clip's .afp-motion
pub fn measure_into_file(video: &Path, settings: &Settings, progress: impl FnMut(f64), stop: impl Fn() -> bool) -> Result<MotionFile, String> {
    let m = measure(video, settings, progress, stop)?;
    m.write(&afp_motion::path_for(video))?;
    Ok(m)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A clip with a Gyroflow project of motion measured from the picture beside it (AFP_MEASURE_CLIP=<video>): its
    /// motion measured here, and compared with the project's (Gyroflow's app measured it, the same code on frames it
    /// decoded itself): the camera's orientation, frame by frame
    #[test]
    #[cfg(unix)]
    fn measured_like_gyroflow() {
        let Some(video) = std::env::var_os("AFP_MEASURE_CLIP").map(std::path::PathBuf::from) else { return };
        let _ = simplelog::TermLogger::init(log::LevelFilter::Info, simplelog::Config::default(), simplelog::TerminalMode::Stderr, simplelog::ColorChoice::Never);
        // The project's lens, readout and refraction: the same as Gyroflow measured with
        let project = crate::optical::own_project(&video);
        let doc: Option<serde_json::Value> = project.as_ref().and_then(|p| serde_json::from_slice(&std::fs::read(p).ok()?).ok());
        let settings = match &doc {
            Some(d) => Settings { lens: d.get("calibration_data").cloned(), readout_ms: d.pointer("/stabilization/frame_readout_time").and_then(|v| v.as_f64()),
                                  refraction: d.get("light_refraction_coefficient").and_then(|v| v.as_f64()), ..Default::default() },
            None => Settings::default(),
        };
        let t = std::time::Instant::now();
        let m = measure(&video, &settings, |_| (), || false).unwrap();
        eprintln!("measured in {:.1} s: {} of {} frames, lens {}, refraction {}, readout {} ms", t.elapsed().as_secs_f64(),
            m.header.measured.measured_frames, m.header.measured.frames, m.header.lens_name, m.header.refraction, m.header.readout_ms);
        let Some(project) = project else { return };
        let dir = std::env::temp_dir().join(format!("afp-measure-clip-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let link = dir.join(video.file_name().unwrap());
        let _ = std::fs::remove_file(&link);
        std::os::unix::fs::symlink(&video, &link).unwrap();
        m.write(&afp_motion::path_for(&link)).unwrap();
        let (a, _, _) = crate::load_manager(&project.to_string_lossy(), None, Some(0.0), crate::Underwater::Project, 0.0, false, None).unwrap();
        let (b, _, _) = crate::load_manager(&link.to_string_lossy(), None, Some(0.0), crate::Underwater::Auto, 0.0, false, None).unwrap();
        let (ga, gb) = (a.gyro.read(), b.gyro.read());
        eprintln!("correction applies: project {} / measured here {}", ga.optical_correction_applied, gb.optical_correction_applied);
        assert!(gb.optical_correction_applied);
        // Frame to frame, as the stabilization sees it: how the turn between two frames differs
        let fps = a.params.read().fps;
        let frames = a.params.read().frame_count;
        let at = |g: &gyroflow_core::gyro_source::GyroSource, f: usize| g.org_quat_at_timestamp(f as f64 * 1000.0 / fps);
        let mut d: Vec<f64> = (1..frames).map(|f| {
            let ra = at(&ga, f - 1).inverse() * at(&ga, f);
            let rb = at(&gb, f - 1).inverse() * at(&gb, f);
            ra.angle_to(&rb).to_degrees()
        }).collect();
        let total: Vec<f64> = (0..frames).map(|f| at(&ga, f).angle_to(&at(&gb, f)).to_degrees()).collect();
        // Per axis: the frame-to-frame turn as a rotation vector, Gyroflow's against this one's, for the first frames
        for f in [10usize, 50, 100, 150, 200] {
            if f >= frames { break; }
            let ra = (at(&ga, f - 1).inverse() * at(&ga, f)).scaled_axis() * 57.2958;
            let rb = (at(&gb, f - 1).inverse() * at(&gb, f)).scaled_axis() * 57.2958;
            eprintln!("frame {f}: Gyroflow {:+.3} {:+.3} {:+.3}  here {:+.3} {:+.3} {:+.3}", ra.x, ra.y, ra.z, rb.x, rb.y, rb.z);
        }
        for lag in -3i64..=3 {
            let mut e: Vec<f64> = (4..frames.saturating_sub(4)).map(|f| {
                let g = f as i64 + lag;
                let ra = at(&ga, f - 1).inverse() * at(&ga, f);
                let rb = at(&gb, (g - 1) as usize).inverse() * at(&gb, g as usize);
                ra.angle_to(&rb).to_degrees()
            }).collect();
            e.sort_by(|x, y| x.partial_cmp(y).unwrap());
            let (lo, hi) = (e[..e.len() / 2].to_vec(), e[e.len() / 2..].to_vec());
            eprintln!("lag {lag:+}: median {:.4}° (first half of the frames sorted {:.4}, second {:.4})", e[e.len() / 2], lo[lo.len() / 2], hi[hi.len() / 2]);
        }
        let series = |g: &gyroflow_core::gyro_source::GyroSource| (1..frames).map(|f| (at(g, f - 1).inverse() * at(g, f)).scaled_axis().y * 57.2958).collect::<Vec<f64>>();
        let (sa, sb) = (series(&ga), series(&gb));
        for f in (100..frames.min(200)).step_by(5) { eprintln!("  y {f}: {:+.3} {:+.3}", sa[f - 1], sb[f - 1]); }
        eprintln!("orientations: project {} from {:?} to {:?}, here {} from {:?} to {:?}", ga.quaternions.len(), ga.quaternions.keys().next(), ga.quaternions.keys().last(),
            gb.quaternions.len(), gb.quaternions.keys().next(), gb.quaternions.keys().last());
        d.sort_by(|x, y| x.partial_cmp(y).unwrap());
        let turn: f64 = (1..frames).map(|f| (at(&ga, f - 1).inverse() * at(&ga, f)).angle().to_degrees()).sum::<f64>() / frames as f64;
        eprintln!("frame-to-frame turn: Gyroflow's {:.4}°/frame on average; difference median {:.4}°, 95 % {:.4}°, max {:.4}°; drift over the clip max {:.3}°",
            turn, d[d.len() / 2], d[d.len() * 95 / 100], d[d.len() - 1], total.iter().cloned().fold(0.0, f64::max));
        drop((ga, gb));
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
