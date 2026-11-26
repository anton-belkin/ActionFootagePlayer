// SPDX-License-Identifier: GPL-3.0-or-later
//
// The turn a camera's own stabilizer gave the picture, measured from the picture, for a camera that records its gyro but
// not that turn: GoPro's HyperSmooth on a HERO7 Black (docs/design/hero7-hypersmooth-iori.md). A HERO8 and later record it
// as IORI; telemetry-parser hands Gyroflow the camera's turn times it, the picture's own orientation, which is what
// a HERO12's clip is stabilized further with (in_camera.rs). A HERO7's gyro is the camera body's alone: stabilized with
// it, the picture would get HyperSmooth's correction a second time, the other way.
//
// What the picture says against the gyro is what Gyroflow's optical correction measures: the small rotation δ(t) that
// takes the motion data to the picture's own orientation, `q' = q · exp(δ)`, in the quaternions' frame. With the gyro
// the body's, δ is HyperSmooth's turn, IORI. It is fitted here as Gyroflow fits its correction (the same tracks and
// solver, optical_motion::solve_with), with three differences:
// - All of it is taken (strength 1): HyperSmooth's turn is of every frequency its margin lets through, down to a
//   fraction of a hertz.
// - It's fitted on top of a model of HyperSmooth (`model`): the body's path smoothed (σ 0.5 s) and 0.2 s late, which
//   matched the turn measured from a HERO7 clip's tracks with gain 1 (research_notes/dynamic_lens, part 2). The solver
//   pulls δ to zero where the picture doesn't pin it down (frames nothing was tracked in: open water, a fish filling the
//   frame); there the model stands in, and the clip plays about as recorded, instead of getting HyperSmooth's
//   correction back the other way.
// - Under water it's measured with the turn inside the housing (gyroflow/gyroflow#1302): the flat port refracts about
//   the body's axis, not the picture's. Which turn that is depends on what's measured, so it's measured again with the
//   turn found, in `ROUNDS` rounds.
//
// The result is an ordinary optical correction of the gyro's quaternions, kept in the clip's .afp-motion with the gyro
// kept (afp_motion.rs, `Header::in_camera_turn`). Played, it composes onto the gyro as any correction does, and the
// same turn goes to gyroflow-core as the clip's image orientations (`set_image_orientations`): what a HERO12 has from its
// file. From there the clip is a HERO12's: its crop share (in_camera.rs), the refraction inside the housing.

use std::sync::Arc;
use gyroflow_core::StabilizationManager;
use gyroflow_core::gyro_source::{ GyroSource, OpticalCorrection, OpticalCorrectionSettings, Quat64, TimeQuat };
use gyroflow_core::synchronization::optical_motion::{ self, OpticalMeasurements };
use nalgebra::Vector3;
use serde::{ Deserialize, Serialize };

/// HyperSmooth's model: the body's path smoothed by a Gaussian of this σ, s, and this late, s
pub const MODEL_SIGMA_S: f64 = 0.5;
pub const MODEL_DELAY_S: f64 = 0.2;
/// Measured again with the turn found, under water. Each round takes it part of the way (GX010564, the world's jitter at
/// the edges: 1 round 4.7 px, 3 rounds 3.5, 6 rounds 3.2, 10 rounds 3.1); a round is a fraction of a second
pub const ROUNDS: usize = 10;
/// What `FileMetadata::additional_data` says of a clip whose image orientations are the turn measured here
pub const MARK: &str = "afp_in_camera_turn";
/// The gyro integrated as Gyroflow does by default (VQF), not a file's own orientations: the body's, whatever the camera
pub const INTEGRATION: usize = 2;

/// The camera's stabilizer, when the clip's turn is measured on top of its gyro: a HERO7 Black's HyperSmooth. The
/// HERO8 and later record the turn (IORI), and the HERO12 on are stabilized from it (in_camera.rs); a HERO5's and 6's
/// EIS isn't tried
pub fn stabilizer_of(video: &std::path::Path) -> Option<String> {
    let path = video.to_string_lossy();
    let (maker, model) = crate::probe::camera(&path)?;
    if maker != "GoPro" || !measured_model(&model?) { return None; }
    crate::probe::in_camera_of(&path).filter(|h| !h.is_empty())
}

/// The GoPro models measured so: the HERO7 Black ("HERO7 Black" as its file names it, "GoPro " in front or not)
pub fn measured_model(model: &str) -> bool {
    let upper = model.to_ascii_uppercase();
    upper.contains("HERO7") && upper.contains("BLACK")
}

/// How the turn was measured, and how sure it is (the .afp-motion's header)
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Turn {
    /// The camera's stabilizer, as its file names it ("HyperSmooth (HS EIS)")
    pub stabilizer: String,
    /// How the gyro was integrated (gyroflow-core's integration_method): the correction is of those quaternions
    pub integration: usize,
    /// The model it was fitted on top of: σ and delay, s
    pub model_sigma_s: f64,
    pub model_delay_s: f64,
    /// The turn's size over the clip: rms and 95th percentile, degrees
    pub rms_deg: f64,
    pub p95_deg: f64,
    /// How far the picture took it from the model: rms, degrees
    pub from_picture_rms_deg: f64,
    /// Each round's largest change of the turn, degrees (the last one small: it converged)
    pub rounds_deg: Vec<f64>,
    /// The share of frames the picture measured (the rest: the model)
    pub measured_share: f64,
}

/// HyperSmooth's turn as its model has it, at each of the quaternions' times: `q(t)⁻¹ · S(t − delay)`, S the path
/// smoothed. Smoothed on a 5 ms grid (the gyro's 1 kHz would take ~1 s a minute of clip for nothing)
pub fn model(quats: &TimeQuat, sigma_s: f64, delay_s: f64) -> TimeQuat {
    let (Some((&first, _)), Some((&last, _))) = (quats.first_key_value(), quats.last_key_value()) else { return TimeQuat::new() };
    const STEP_US: i64 = 5000;
    let grid: Vec<Quat64> = (0..=((last - first) / STEP_US)).map(|i| quat_at(quats, first + i * STEP_US)).collect();
    let sigma = sigma_s * 1e6 / STEP_US as f64;
    let reach = (3.0 * sigma).ceil() as i64;
    let weights: Vec<f64> = (0..=reach).map(|d| (-0.5 * (d as f64 / sigma).powi(2)).exp()).collect();
    let n = grid.len() as i64;
    let smooth: Vec<Quat64> = (0..n).map(|i| {
        let c = grid[i as usize];
        let mut sum = nalgebra::Vector4::zeros();
        for j in (i - reach).max(0)..=(i + reach).min(n - 1) {
            let q = grid[j as usize];
            let v = q.as_ref().coords;
            // The same hemisphere as the middle one's: q and -q are the same turn
            let v = if v.dot(&c.as_ref().coords) < 0.0 { -v } else { v };
            sum += v * weights[(j - i).unsigned_abs() as usize];
        }
        Quat64::from_quaternion(nalgebra::Quaternion::from(sum))
    }).collect();
    let smooth_at = |t: i64| -> Quat64 {
        let u = ((t - first) as f64 / STEP_US as f64).clamp(0.0, (n - 1) as f64);
        let i = (u.floor() as usize).min(n as usize - 1);
        let j = (i + 1).min(n as usize - 1);
        smooth[i].nlerp(&smooth[j], u - i as f64)
    };
    let delay = (delay_s * 1e6).round() as i64;
    quats.iter().map(|(&t, q)| (t, q.inverse() * smooth_at(t - delay))).collect()
}

/// The quaternion at `t` (µs), between the two around it (nlerp), held past the ends
fn quat_at(quats: &TimeQuat, t: i64) -> Quat64 {
    let before = quats.range(..=t).next_back();
    let after = quats.range(t..).next();
    match (before, after) {
        (Some((&a, qa)), Some((&b, qb))) if b > a => qa.nlerp(qb, (t - a) as f64 / (b - a) as f64),
        (Some((_, q)), _) | (None, Some((_, q))) => *q,
        (None, None) => Quat64::identity(),
    }
}

/// The turn a correction stands for, at each of the quaternions' times: `exp(δ(t))`. What the player gives
/// gyroflow-core as the clip's image orientations, and what the estimate's context is taken with: one function, so the two
/// are the same to the last bit
pub fn turn_of(correction: &OpticalCorrection, quats: &TimeQuat) -> TimeQuat {
    quats.keys().map(|&t| (t, Quat64::from_scaled_axis(correction.at(t as f64)))).collect()
}

/// The clip's image orientations set to `turn` (gyroflow-core's FileMetadata is shared by every copy of the motion data,
/// so a copy of its own), and marked as measured here (`MARK`)
pub fn set_turn(gyro: &mut GyroSource, turn: TimeQuat) {
    let mut md = gyro.file_metadata.read().clone();
    md.image_orientations = Some(turn);
    if !md.additional_data.is_object() { md.additional_data = serde_json::json!({}); }
    if let Some(o) = md.additional_data.as_object_mut() { o.insert(MARK.into(), true.into()); }
    gyro.file_metadata = md.into();
}

/// A clip playing with a correction measured here: its image orientations are the correction's turn. Before the
/// correction is set (StabilizationManager::set_optical_correction), whose context is taken with them
pub fn set_image_orientations(stab: &StabilizationManager, correction: &OpticalCorrection) {
    let mut gyro = stab.gyro.write();
    let turn = turn_of(correction, &gyro.quaternions);
    set_turn(&mut gyro, turn);
}

/// Whether the clip's image orientations are a turn measured here (`set_turn`)
pub fn measured_here(stab: &StabilizationManager) -> bool {
    stab.gyro.read().file_metadata.read().additional_data.get(MARK).and_then(|v| v.as_bool()) == Some(true)
}

/// Fits HyperSmooth's turn to the tracks `m0` (measured against the gyro the manager has, its quaternions as they
/// are), and returns it as an optical correction of those quaternions, its context taken as the player will play it.
/// The manager is left with the turn as its image orientations
pub fn fit(stab: &StabilizationManager, m0: &OpticalMeasurements, stabilizer: &str, cancelled: impl Fn() -> bool) -> Result<(OpticalCorrection, Turn), String> {
    let gyro0 = stab.gyro.read().clone();
    if gyro0.quaternions.len() < 2 { return Err("no motion data to measure the turn against".into()); }
    if gyro0.segment_ranges().is_some_and(|s| !s.whole) { return Err("a video cut from recordings: not measured".into()); }
    let quats = gyro0.quaternions.clone();
    let water = { let p = stab.params.read(); p.light_refraction_coefficient > 0.0 && p.light_refraction_coefficient != 1.0 };
    let settings = OpticalCorrectionSettings { strength: 1.0, per_frame: false };
    let mut params = optical_motion::solver_params(&settings, m0.scaled_fps);
    // A fish or a diver crossing the frame moves its tracks its own way: let the fit set them aside
    params.robust_passes = 2;

    let model = model(&quats, MODEL_SIGMA_S, MODEL_DELAY_S);
    let mut turn = model.clone();
    let mut rounds_deg = Vec::new();
    let mut measured = (0, 0);
    // In air one round: the turn doesn't change what's measured
    for _ in 0..if water { ROUNDS } else { 1 } {
        let mut g = gyro0.clone();
        g.quaternions = Arc::new(quats.iter().map(|(t, q)| (*t, q * turn[t])).collect());
        set_turn(&mut g, turn.clone());
        let m = m0.remeasure_against(g, false, &cancelled)?;
        measured = (m.measured_frames, m.frames);
        let c = optical_motion::solve_with(&m, &settings, &params)?;
        let mut change = 0f64;
        for (t, r) in turn.iter_mut() {
            let d = c.at(*t as f64);
            change = change.max(d.norm());
            *r *= Quat64::from_scaled_axis(d);
        }
        rounds_deg.push(change.to_degrees());
    }

    // The turn as a correction of the gyro's own quaternions: control points on the solver's spacing, from the turn
    // there (a cubic B-spline at a knot is (c[j-1] + 4 c[j] + c[j+1]) / 6 of its points: undone to first order)
    let (first, last) = (*quats.keys().next().unwrap(), *quats.keys().next_back().unwrap());
    let spacing = params.spacing_us;
    let start_us = first as f64 - spacing;
    let count = ((last as f64 - start_us) / spacing).ceil() as usize + 3;
    let v: Vec<Vector3<f64>> = (0..count).map(|j| quat_at(&turn, (start_us + j as f64 * spacing).round() as i64).scaled_axis()).collect();
    let coeffs: Vec<[f32; 3]> = (0..count).map(|j| {
        let (a, b) = (v[j.saturating_sub(1)], v[(j + 1).min(count - 1)]);
        let c = v[j] - (a - 2.0 * v[j] + b) / 6.0;
        [c.x as f32, c.y as f32, c.z as f32]
    }).collect();
    let mut correction = OpticalCorrection {
        enabled: true,
        settings,
        start_us,
        spacing_us: spacing,
        coeffs,
        quats_checksum: m0.quats_checksum,
        context_checksum: 0,
        video_base: Vec::new(),
        tracking: m0.tracking,
        frames: measured.1,
        measured_frames: measured.0,
        rms_deg: 0.0,
        parts: Vec::new(),
    };
    // As it will be read: its points stored as 16-bit integers
    let stored = gyroflow_core::util::compress_to_base91_cbor(&correction).ok_or("the correction couldn't be encoded")?;
    correction = gyroflow_core::util::decompress_from_base91_cbor(&stored).map_err(|e| format!("the correction can't be read back: {e:?}"))?;

    // The context the player will play it in: with its turn as the image orientations
    set_image_orientations(stab, &correction);
    let measurement = optical_motion::measurement_params(stab);
    correction.context_checksum = optical_motion::context_checksum(&measurement);

    let played = turn_of(&correction, &quats);
    let mut angles: Vec<f64> = played.values().map(|q| q.angle().to_degrees()).collect();
    angles.sort_by(|a, b| a.total_cmp(b));
    let rms = |it: &mut dyn Iterator<Item = f64>| { let (s, n) = it.fold((0.0, 0usize), |(s, n), a| (s + a * a, n + 1)); (s / n.max(1) as f64).sqrt() };
    let from_picture = rms(&mut played.iter().map(|(t, q)| model[t].angle_to(q).to_degrees()));
    correction.rms_deg = rms(&mut angles.iter().copied());
    let info = Turn {
        stabilizer: stabilizer.to_string(),
        integration: gyro0.integration_method,
        model_sigma_s: MODEL_SIGMA_S,
        model_delay_s: MODEL_DELAY_S,
        rms_deg: correction.rms_deg,
        p95_deg: angles.get(angles.len() * 95 / 100).copied().unwrap_or_default(),
        from_picture_rms_deg: from_picture,
        rounds_deg,
        measured_share: measured.0 as f64 / measured.1.max(1) as f64,
    };
    Ok((correction, info))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn path(f: impl Fn(f64) -> Vector3<f64>, seconds: f64) -> TimeQuat {
        (0..(seconds * 1000.0) as i64).map(|ms| (ms * 1000, Quat64::from_scaled_axis(f(ms as f64 / 1000.0)))).collect()
    }

    #[test]
    fn the_model_takes_out_the_shake_and_lets_the_pan_through() {
        // A slow pan with a 6 Hz shake on it: the picture's path is the pan, late by the delay
        let pan = |t: f64| Vector3::new(0.0, 0.2 * t, 0.0);
        let shake = |t: f64| Vector3::new(0.02 * (2.0 * std::f64::consts::PI * 6.0 * t).sin(), 0.0, 0.0);
        let q = path(|t| pan(t) + shake(t), 6.0);
        let m = model(&q, MODEL_SIGMA_S, MODEL_DELAY_S);
        for t in [2.0f64, 3.0, 4.0] {
            let k = (t * 1e6) as i64;
            let picture = q[&k] * m[&k];
            let expected = Quat64::from_scaled_axis(pan(t - MODEL_DELAY_S));
            let err = picture.angle_to(&expected).to_degrees();
            assert!(err < 0.01, "at {t} s the picture is {err}° off the pan");
        }
    }

    #[test]
    fn a_correction_stands_for_its_turn() {
        // A turn as control points, as `fit` makes them, read back as the correction applies it
        let turn = path(|t| Vector3::new(0.01 * (3.0 * t).sin(), -0.02 * (1.3 * t).cos(), 0.005), 2.0);
        let spacing: f64 = 1e6 / 60.0 / 6.0;
        let start_us = -spacing;
        let count: usize = (2e6 / spacing).ceil() as usize + 3;
        let v: Vec<Vector3<f64>> = (0..count).map(|j| quat_at(&turn, (start_us + j as f64 * spacing).round() as i64).scaled_axis()).collect();
        let coeffs = (0..count).map(|j| { let (a, b) = (v[j.saturating_sub(1)], v[(j + 1).min(count - 1)]); let c = v[j] - (a - 2.0 * v[j] + b) / 6.0; [c.x as f32, c.y as f32, c.z as f32] }).collect();
        let c = OpticalCorrection { enabled: true, start_us, spacing_us: spacing, coeffs, ..Default::default() };
        let back = turn_of(&c, &turn);
        let worst = turn.iter().map(|(t, q)| q.angle_to(&back[t]).to_degrees()).fold(0f64, f64::max);
        // 32-bit points and the spline undone to first order: a thousandth of a degree, hundredths of a pixel at 4K
        assert!(worst < 0.002, "{worst}°");
    }

    /// The check a HERO7 can't have: a HERO12 records HyperSmooth's turn (IORI). Measured here as if it didn't (its gyro
    /// integrated, its IORI set aside), the turn found against the one it recorded, frame to frame and with the slow part
    /// (under 0.5 Hz, which the picture pins down only loosely) taken out of both. Clips separated by ';', in water unless
    /// AFP_TURN_WATER says another index (1: air):
    /// AFP_TURN_CLIP=<GX….MP4> cargo test --release --lib a_hero12_recorded -- --ignored --nocapture
    #[test]
    #[ignore]
    fn the_turn_measured_is_the_one_a_hero12_recorded() {
        let Ok(clips) = std::env::var("AFP_TURN_CLIP") else { return };
        let n: f64 = std::env::var("AFP_TURN_WATER").ok().and_then(|v| v.parse().ok()).unwrap_or(1.33);
        let _ = simplelog::TermLogger::init(log::LevelFilter::Info, simplelog::Config::default(), simplelog::TerminalMode::Stderr, simplelog::ColorChoice::Never);
        for clip in clips.split(';') {
            let (stab, _, _) = crate::load_manager(clip, None, None, crate::Underwater::Auto, 0.0, false, None).unwrap();
            let truth = stab.gyro.read().file_metadata.read().image_orientations.clone().expect("the clip recorded no IORI");
            drop(stab);
            let settings = crate::measure::Settings { refraction: Some(n), in_camera_turn: Some(true), ..Default::default() };
            let t0 = std::time::Instant::now();
            let m = crate::measure::measure(std::path::Path::new(clip), &settings, |_| (), || false).unwrap();
            let turn = m.header.in_camera_turn.clone().unwrap();
            eprintln!("{clip}: measured in {:.0} s: {turn:?}", t0.elapsed().as_secs_f64());
            // Every 10 ms of the recorded turn, the measured one at the same time, as rotation vectors (degrees)
            let times: Vec<i64> = truth.keys().copied().collect::<Vec<_>>().chunks(10).map(|c| c[0]).collect();
            let rec: Vec<Vector3<f64>> = times.iter().map(|t| truth[t].scaled_axis().map(f64::to_degrees)).collect();
            let got: Vec<Vector3<f64>> = times.iter().map(|t| m.correction.at(*t as f64).map(f64::to_degrees)).collect();
            // The slow part: a 2 s moving average (under ~0.5 Hz)
            let fast = |v: &[Vector3<f64>]| -> Vec<Vector3<f64>> {
                let k = 100usize;
                (0..v.len()).map(|i| { let (a, b) = (i.saturating_sub(k), (i + k + 1).min(v.len())); v[i] - v[a..b].iter().sum::<Vector3<f64>>() / (b - a) as f64 }).collect()
            };
            for (label, a, b) in [("all", rec.clone(), got.clone()), ("above 0.5 Hz", fast(&rec), fast(&got))] {
                let len = a.len() as f64;
                for axis in 0..3 {
                    let (x, y): (Vec<f64>, Vec<f64>) = a.iter().zip(&b).map(|(p, q)| (p[axis], q[axis])).unzip();
                    let mean = |v: &[f64]| v.iter().sum::<f64>() / len;
                    let (mx, my) = (mean(&x), mean(&y));
                    let sxy: f64 = x.iter().zip(&y).map(|(p, q)| (p - mx) * (q - my)).sum();
                    let sxx: f64 = x.iter().map(|p| (p - mx).powi(2)).sum();
                    let syy: f64 = y.iter().map(|q| (q - my).powi(2)).sum();
                    let err = (x.iter().zip(&y).map(|(p, q)| (p - q).powi(2)).sum::<f64>() / len).sqrt();
                    eprintln!("  {label}, axis {axis}: recorded {:.3}° rms, measured {:.3}° rms, apart {err:.3}° rms, correlation {:.3}, scale {:.3}",
                        (sxx / len).sqrt(), (syy / len).sqrt(), sxy / (sxx * syy).sqrt().max(1e-12), sxy / sxx.max(1e-12));
                }
            }
        }
    }

    /// How steady the world stays in each way of playing a clip that recorded its turn (a HERO12), the measure gyroflow#1302
    /// was judged by: KLT tracks' world rays (the clip's orientation times each point's ray, refraction and all), their
    /// second difference over 4 frames, median px at the centre, the middle and the edges. As the HERO12 plays (its own
    /// CORI·IORI), as a HERO7 would (its turn measured here), and measured from the picture alone (Stabilize optically
    /// before this). Tracks: f32 rows of id, frame, x, y in the clip's pixels (research_notes/dynamic_lens):
    /// AFP_TURN_STEADY=<clip.mp4>|<tracks.f32> cargo test --release --lib world_stays -- --ignored --nocapture
    #[test]
    #[ignore]
    fn how_still_the_world_stays() {
        use gyroflow_core::stabilization::{ ComputeParams, FrameTransform, undistort_points_for_optical_motion };
        let Ok(spec) = std::env::var("AFP_TURN_STEADY") else { return };
        let (clip, tracks) = spec.split_once('|').expect("<clip>|<tracks.f32>");
        let n = 1.33;
        let raw = std::fs::read(tracks).unwrap();
        let v: Vec<f32> = raw.chunks_exact(4).map(|b| f32::from_le_bytes(b.try_into().unwrap())).collect();
        let mut by_frame: std::collections::BTreeMap<usize, Vec<(u32, f32, f32)>> = Default::default();
        for c in v.chunks_exact(4) { by_frame.entry(c[1] as usize).or_default().push((c[0] as u32, c[2], c[3])); }
        let video = std::path::Path::new(clip);
        let settings = |turn: bool| crate::measure::Settings { refraction: Some(n), in_camera_turn: Some(turn), ..Default::default() };
        let estimate = crate::measure::measure(video, &settings(true), |_| (), || false).unwrap();
        let picture_alone = crate::measure::measure(video, &settings(false), |_| (), || false).unwrap();
        if let Ok(dump) = std::env::var("AFP_TURN_DUMP") {
            // The turn at each frame's time, radians (x y z), for the research's scripts
            let fps = estimate.header.clip.fps;
            let rows: String = (0..estimate.header.clip.frames).map(|f| { let d = estimate.correction.at(f as f64 * 1e6 / fps); format!("{} {} {}\n", d.x, d.y, d.z) }).collect();
            std::fs::write(dump, rows).unwrap();
        }
        let manager = |m: Option<&crate::afp_motion::MotionFile>| {
            let (stab, _, _) = crate::load_manager(clip, None, None, crate::Underwater::Auto, 0.0, false, None).unwrap();
            stab.set_light_refraction_coefficient(n);
            if let Some(m) = m {
                // As it plays: the correction applies (the motion data and context it was measured in)
                let applied = crate::afp_motion::apply(&stab, m).unwrap();
                assert!(applied.in_context, "the measured motion doesn't apply as it plays");
            }
            stab.recompute_gyro();
            stab
        };
        let flip = |r: (f32, f32, f32)| Vector3::new(r.0 as f64, -(r.1 as f64), -(r.2 as f64));
        for (label, stab) in [("as the clip's own motion data plays", manager(None)), ("as a HERO7 (turn measured)", manager(Some(&estimate))), ("the picture alone", manager(Some(&picture_alone)))] {
            let p = ComputeParams::from_manager(&stab);
            let (w, h) = (p.width as u32, p.height as u32);
            let fps = stab.params.read().fps;
            let f_px = FrameTransform::get_lens_data_at_timestamp(&p, 0.0, false).0[(0, 0)] * n;
            let mut per_track: std::collections::BTreeMap<u32, Vec<(usize, f64, Vector3<f64>)>> = Default::default();
            for (&fr, pts) in &by_frame {
                let ts = fr as f64 * 1000.0 / fps;
                let xy: Vec<(f32, f32)> = pts.iter().map(|p| (p.1, p.2)).collect();
                let rays = undistort_points_for_optical_motion(&xy, ts, fr, &p, (w, h));
                let q = { let g = p.gyro.read(); let lookup = g.lookup_for_frame(fr); g.org_quat_at(ts, &lookup) };
                for (i, pt) in pts.iter().enumerate() {
                    let Some(r) = rays[i] else { continue };
                    let rr = (((pt.1 as f64 - w as f64 / 2.0) / (w as f64 / 2.0)).hypot((pt.2 as f64 - h as f64 / 2.0) / (h as f64 / 2.0))) / 2f64.sqrt();
                    per_track.entry(pt.0).or_default().push((fr, rr, q * flip(r).normalize()));
                }
            }
            let k = 4usize;
            let mut z: [Vec<f64>; 3] = Default::default();
            for obs in per_track.values() {
                for i in k..obs.len().saturating_sub(k) {
                    let (a, b, c) = (&obs[i - k], &obs[i], &obs[i + k]);
                    if b.0 - a.0 != k || c.0 - b.0 != k { continue; }
                    let zi = if b.1 < 0.3 { 0 } else if b.1 < 0.7 { 1 } else { 2 };
                    z[zi].push(b.2.angle(&(a.2 + c.2).normalize()) * f_px);
                }
            }
            let med = |v: &mut Vec<f64>| { if v.is_empty() { return f64::NAN; } v.sort_by(|a, b| a.total_cmp(b)); v[v.len() / 2] };
            let [mut c, mut m, mut e] = z;
            eprintln!("{label}: the world's second difference over {k} frames, median px: centre {:.2} / middle {:.2} / edges {:.2}", med(&mut c), med(&mut m), med(&mut e));
        }
    }
}
