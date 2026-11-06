// SPDX-License-Identifier: GPL-3.0-or-later
//
// The plugin's 360 mode (docs/360-cameras.md, "The plugin's part"; step 4 of docs/360-implementation-plan.md):
// `core.gyroflow.Sphere360(clip, video=...)`, where `clip` is a 360 camera's lens tracks joined side by side (mpv's
// lavfi-complex hstack, in track order). Out comes the sphere, equirectangular, in the camera's own axes at the middle
// of the frame: every lens mapped onto it by gyroflow-core's kernel (the fork's equirect output projection and RigLens:
// its pose, its offset at the stitching distance, its validity), with no smoothing, so only the rolling shutter is
// taken out, and the lenses blended by their feathers (rig.rs). Under it, ROWS rows more: the stamp (stamp.rs), the
// camera in the world and the heading modes' yaws (heading.rs), for the view shader (mpv/scripts/view360).
//
// Per frame and plane: the first lens is rendered straight into the output, each other one into a buffer of its own
// and blended into the output where it has a share. The shares depend only on the direction in the camera's axes, so
// they're worked out once per clip: per row, spans where the lens is alone (a copy) and where it's mixed (its share
// of what the lenses before it left, in 256ths).
//
// Each lens and plane is its own gyroflow-core Stabilization, set up for the lens's size (luma; a chroma plane maps
// onto it, as in the flat mode) and reading its part of the joined frame (the input buffer's rect); a few workers each
// have their GPU context and a set of them, so one uploads while another renders.

use std::ffi::{ c_int, c_void, CString };
use std::ptr::null;
use std::sync::{ Arc, mpsc };

use nalgebra::Vector3;
use parking_lot::Mutex;
use vapoursynth4_sys::*;

use gyroflow_core::{ StabilizationManager, filesystem, lens_profile::LensProfile };
use gyroflow_core::gpu::{ BufferDescription, BufferSource, Buffers };
use gyroflow_core::stabilization::{ ComputeParams, Interpolation, Luma8, Luma16, PixelType, RigLens, Stabilization };
use gyroflow_core::stabilization::distortion_models::DistortionModel;
use gyroflow_core::stabilization::projection::OutputProjection;
use gyroflow_core::stabilization_params::{ BackgroundMode, ReadoutDirection };

use crate::{ PlaneJob, SampleKind, rig::{ Rig, equirect_direction }, heading::{ World, Headings, flip, yaw }, stamp::{ self, Stamp } };

/// The stitching distance for a rig that states none (Insta360's don't), metres. The seam is right for things about
/// this far from the camera: on an X5 (31.9 mm between the lenses) a subject at 1 m is ~0.9° off at infinity, ~0.5°
/// at 2 m, and the horizon ~0.9° at 1 m. A stick or handlebars put the camera's surroundings at 0.5-2 m
pub const DEFAULT_DISTANCE: f64 = 2.0;
/// The sphere's width, its height half of it: 16 px a degree. An X5's lens has ~19 px a degree over its field (6912
/// across 360°), but on screen with the view shader (gpu-next) an M1 Pro played the sample X5 clip at 25 fps with no
/// frame dropped up to 6400 wide; 6912 dropped 17 frames in 15 s, 7680 ~140. 5760 leaves room
pub const DEFAULT_WIDTH: usize = 5760;

/// Where one lens is blended in, per row of a plane: copied where it's alone, mixed by its share elsewhere
struct Spans {
    rows: Vec<Vec<Span>>,
}

struct Span {
    x: usize,
    len: usize,
    /// Its share of each pixel in 256ths, 0 < a < 256; none where it's the whole of it (a copy)
    alpha: Option<Vec<u16>>,
}

impl Spans {
    /// Lens `lens`'s share (of what lenses 0..lens leave) per pixel of a plane `w` by `h` covering the sphere
    fn new(rig: &Rig, lens: usize, distance: f64, w: usize, h: usize) -> Spans {
        let threads = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4).min(16);
        let chunk = h.div_ceil(threads);
        let mut rows: Vec<Vec<Span>> = (0..h).map(|_| Vec::new()).collect();
        std::thread::scope(|s| {
            for (c, out) in rows.chunks_mut(chunk).enumerate() {
                s.spawn(move || {
                    for (r, spans) in out.iter_mut().enumerate() {
                        let y = c * chunk + r;
                        let mut alphas = Vec::with_capacity(w);
                        for x in 0..w {
                            let wts = rig.weights(equirect_direction(x as f64, y as f64, w, h), distance);
                            let before: f64 = wts[..lens].iter().sum();
                            let share = if wts[lens] > 0.0 { wts[lens] / (before + wts[lens]) } else { 0.0 };
                            alphas.push((share * 256.0).round() as u16);
                        }
                        let mut x = 0;
                        while x < w {
                            let a = alphas[x];
                            if a == 0 { x += 1; continue; }
                            let start = x;
                            let whole = a >= 256;
                            while x < w && alphas[x] != 0 && (alphas[x] >= 256) == whole { x += 1; }
                            spans.push(Span { x: start, len: x - start, alpha: (!whole).then(|| alphas[start..x].to_vec()) });
                        }
                    }
                });
            }
        });
        Spans { rows }
    }

    /// `src` (rows `src_stride` samples apart) into `dst`
    fn blend<T: Copy + Into<u32> + TryFrom<u32>>(&self, src: &[T], src_stride: usize, dst: &mut [T], dst_stride: usize) {
        for (y, spans) in self.rows.iter().enumerate() {
            let (s, d) = (&src[y * src_stride..], &mut dst[y * dst_stride..]);
            for span in spans {
                let range = span.x..span.x + span.len;
                match &span.alpha {
                    None => d[range.clone()].copy_from_slice(&s[range]),
                    Some(alpha) => for ((o, i), a) in d[range.clone()].iter_mut().zip(&s[range]).zip(alpha) {
                        let (o32, i32, a) = ((*o).into(), (*i).into(), *a as u32);
                        let v = (o32 * (256 - a) + i32 * a + 128) >> 8;
                        if let Ok(v) = T::try_from(v) { *o = v; }
                    },
                }
            }
        }
    }
}

/// How the motion data (gyroflow-core's, after its IMU orientation) turns into the rig's axes, where it doesn't match:
/// the gyro's turn and the accelerometer's, each q_rig_from_sensor in OpenCV axes.
///
/// Insta360 X4 and X5 (IMU orientation `yzX`): telemetry-parser's orientation assumes the IMU already turned into lens
/// 0's frame, which it does only for a clip with a crop window (a flat one, `insert_lens_profile`); a 360 file's isn't.
/// Found on the sample X5 clip, not derived, and to check on other X4/X5 clips:
/// - the gyro: half a turn about the vertical (x to -x, z to -z). Of the 24 axis turns, the one with which the gyro
///   alone and one vertical explain the horizon measured in the sphere every second (the sky's lower edge fitted with a
///   great circle): 5.4° rms over 23 s, the next 8.1°, the others 23° and more;
/// - the accelerometer: x to -y, y to -x, z to -z, a quarter turn from the gyro's. Gravity turned into the world by the
///   gyro stays the steadiest with it, and its mean is 2.9° from the vertical the horizon gives
fn imu_turn(rig: &Rig, imu_orientation: Option<&str>) -> (Option<nalgebra::UnitQuaternion<f64>>, Option<nalgebra::UnitQuaternion<f64>>) {
    match (rig.brand.as_str(), imu_orientation) {
        ("Insta360", Some("yzX")) => (axes("-x,y,-z"), axes("-y,-x,-z")),
        _ => (None, None),
    }
}

/// A turn given as the images of x, y and z, each an axis with its sign ("-y,-x,-z"); None for anything else
fn axes(v: &str) -> Option<nalgebra::UnitQuaternion<f64>> {
    let cols: Vec<Vector3<f64>> = v.split(',').map(|a| {
        let a = a.trim();
        let (sign, name) = if let Some(n) = a.strip_prefix('-') { (-1.0, n) } else { (1.0, a.trim_start_matches('+')) };
        match name { "x" => Some(Vector3::x() * sign), "y" => Some(Vector3::y() * sign), "z" => Some(Vector3::z() * sign), _ => None }
    }).collect::<Option<_>>()?;
    if cols.len() != 3 { return None; }
    let m = nalgebra::Matrix3::from_columns(&cols);
    // Not from_matrix: it iterates from the identity and stays there for a half turn
    (m.determinant() > 0.5).then(|| nalgebra::UnitQuaternion::from_rotation_matrix(&nalgebra::Rotation3::from_matrix_unchecked(m)))
}

/// What the stamp needs per frame: gyroflow-core's motion, turned into the world, and the headings
struct Stamps {
    stab: Arc<StabilizationManager>,
    world: World,
    headings: Headings,
    fps: f64,
    frame_offset_us: i64,
}

impl Stamps {
    fn at(&self, timestamp_us: i64) -> Stamp {
        let frame = (((timestamp_us - self.frame_offset_us) as f64) * self.fps / 1e6).round() as i64;
        let q = self.world.world_from_cam(frame as f64, &self.stab.gyro.read().org_quat_at_timestamp(timestamp_us as f64 / 1000.0));
        Stamp { time_us: timestamp_us, q_world_from_cam: [q.w, q.i, q.j, q.k], headings: self.headings.at(frame) }
    }
}

/// A 360 clip's motion for the stamp: gyroflow-core's (the gyro alone, no smoothing), the world it's leveled into, the
/// heading modes per frame (heading.rs), and what the modes found, for the player's panel (`report`)
pub struct Motion {
    pub stab: Arc<StabilizationManager>,
    pub world: World,
    pub headings: Headings,
    pub fps: f64,
    pub frame_count: usize,
    pub frame_offset_us: i64,
    pub report: serde_json::Value,
}

impl Motion {
    pub fn load(video: &str, rig: &Rig) -> Result<Motion, String> {
        // The motion, smoothed by nothing: the output camera is the camera at the middle of the frame
        let stab = StabilizationManager::default();
        {
            let url = filesystem::path_to_url(video);
            let mut file = std::fs::File::open(video).map_err(|e| format!("{video}: {e}"))?;
            let size = file.metadata().map_err(|e| e.to_string())?.len() as usize;
            stab.load_video_file(&mut file, size, &url, None, true).map_err(|e| format!("Can't load {video}: {e:?}"))?;
        }
        if !stab.gyro.read().has_motion() { return Err(format!("{video} has no motion data")); }
        // The motion in the rig's axes: gyroflow-core's camera is the rig's (RigLens), for the rolling shutter as much as
        // for the stamp
        let imu_orientation = stab.gyro.read().file_metadata.read().imu_orientation.clone();
        let (gyro_turn, acc_turn) = match std::env::var("AFP_360_IMU").ok() {
            // For checking another body (tools/sphere360/): the gyro's and the accelerometer's turns, each as the images
            // of x, y, z, "x,y,z;-y,-x,-z"
            Some(v) => { let mut it = v.split(';').map(axes); (it.next().flatten(), it.next().flatten()) }
            None => imu_turn(rig, imu_orientation.as_deref()),
        };
        {
            let mut g = stab.gyro.write();
            g.imu_transforms.imu_rotation = gyro_turn.map(|t| (flip() * t * flip()).to_rotation_matrix());
            g.imu_transforms.acc_rotation = acc_turn.or(gyro_turn).map(|t| (flip() * t * flip()).to_rotation_matrix());
        }
        // The gyro alone: the vertical comes from the accelerometer averaged over seconds (heading.rs), not within the
        // integration, which takes braking for a pitch
        stab.gyro.write().integration_method = 3;
        stab.recompute_gyro();
        stab.set_smoothing_method(0);
        stab.recompute_blocking();
        let (fps, frame_count, frame_offset_us) = {
            let p = stab.params.read();
            (p.fps, p.frame_count, (p.frame_offset as f64 / p.fps * 1_000_000.0).round() as i64)
        };

        // The world: gyroflow-core's, leveled frame by frame to the accelerometer averaged over seconds (heading.rs).
        // Per frame its samples turned into gyroflow-core's world (by their own time: the averaging is over seconds)
        let ups = {
            let gyro = stab.gyro.read();
            let md = gyro.file_metadata.read();
            let mut ups = vec![Vector3::zeros(); frame_count.max(1)];
            for x in gyro.raw_imu(&md) {
                let (Some(a), ts) = (x.accl, (x.timestamp_ms * 1000.0).round() as i64) else { continue };
                let Some((_, q)) = gyro.quaternions.range(..=ts).next_back() else { continue };
                let frame = (x.timestamp_ms / 1000.0 * fps).round();
                if frame >= 0.0 && (frame as usize) < ups.len() { ups[frame as usize] += q * Vector3::from(a); }
            }
            ups
        };
        if ups.iter().all(|u| u.norm() < 1e-9) { return Err(format!("{video}: no accelerometer to level the horizon with")); }
        {
            // Where the accelerometer says up, in the rig's axes: -y for a camera held upright, as most are
            let gyro = stab.gyro.read();
            let md = gyro.file_metadata.read();
            let mean = gyro.raw_imu(&md).iter().filter_map(|x| x.accl).fold(Vector3::zeros(), |s, a| s + flip() * Vector3::from(a));
            let tilt = mean.angle(&-Vector3::y()).to_degrees();
            log::info!("{video}: up in the rig's axes on average {:.3?}, {tilt:.0}° from the rig's own (IMU orientation {imu_orientation:?})", mean.normalize().as_slice());
        }
        let world = World::new(&ups, fps);
        let headings = {
            let gyro = stab.gyro.read();
            let yaws: Vec<(f64, f64)> = (0..frame_count).map(|i| {
                let us = (i as f64 * 1e6 / fps).round() as i64 + frame_offset_us;
                yaw(&world.world_from_cam(i as f64, &gyro.org_quat_at_timestamp(us as f64 / 1000.0)))
            }).collect();
            Headings::new(&yaws, fps)
        };

        let (headings, report) = travel_and_north(video, rig, &stab, &world, headings, fps, frame_count, frame_offset_us);
        Ok(Motion { stab: Arc::new(stab), world, headings, fps, frame_count, frame_offset_us, report })
    }

    /// The camera in the world at frame `i` (its middle)
    pub fn world_from_cam(&self, i: usize) -> nalgebra::UnitQuaternion<f64> {
        let us = (i as f64 * 1e6 / self.fps).round() as i64 + self.frame_offset_us;
        self.world.world_from_cam(i as f64, &self.stab.gyro.read().org_quat_at_timestamp(us as f64 / 1000.0))
    }
}

/// Along the way and north (heading.rs), from what the file records besides its motion (nav.rs), and the report for the
/// panel: per mode whether it's there, how sure, and why not
#[allow(clippy::too_many_arguments)]
fn travel_and_north(video: &str, rig: &Rig, stab: &StabilizationManager, world: &World, headings: Headings, fps: f64, frame_count: usize, frame_offset_us: i64) -> (Headings, serde_json::Value) {
    use std::f64::consts::{ PI, TAU };
    let telemetry = crate::nav::Telemetry::read(video).unwrap_or_default();
    let frame_s = |i: usize| (i as f64 * 1000.0 / fps + frame_offset_us as f64 / 1000.0) / 1000.0;
    let mut report = serde_json::json!({});
    let track: Vec<(f64, [f64; 2])> = crate::nav::local_track(&telemetry.gps, 5.0).iter().map(|(t, p, _)| (t / 1000.0, *p)).collect();
    let speeds: Vec<(f64, f64)> = telemetry.gps.iter().filter(|f| f.fix >= 2 && f.speed.is_finite()).map(|f| (f.t_ms / 1000.0, f.speed)).collect();
    let alignment = if track.len() >= 20 {
        // The accelerometer's specific force in the leveled world, at its own rate
        let gyro = stab.gyro.read();
        let md = gyro.file_metadata.read();
        let acc: Vec<(f64, Vector3<f64>)> = gyro.raw_imu(&md).iter().filter_map(|x| {
            let a = x.accl?;
            let (_, q) = gyro.quaternions.range(..=(x.timestamp_ms * 1000.0).round() as i64).next_back()?;
            let frame = (x.timestamp_ms / 1000.0 * fps).round().max(0.0);
            Some((x.timestamp_ms / 1000.0, world.level(frame) * (q * Vector3::from(a))))
        }).collect();
        crate::heading::align(&acc, &track)
    } else { None };
    let mut headings = headings;
    let moving_share = speeds.iter().filter(|s| s.1 >= 3.0).count() as f64 / speeds.len().max(1) as f64;
    report["travel"] = match (&alignment, track.len()) {
        (_, n) if n < 20 => serde_json::json!({ "available": false, "why": telemetry.why_no_gps(), "gps_records": telemetry.gps_records }),
        (None, _) => serde_json::json!({ "available": false, "why": "too little acceleration to tie the camera's turns to the GPS track (a straight road at a steady speed, or standing)" }),
        (Some(a), _) => {
            let tr = crate::heading::travel(a, &track, &speeds, headings.camera(), fps, frame_s);
            let ok = a.confidence() > 0.2 && !tr.is_empty();
            log::info!("{video}: along the way: θ {:.1}°, drift {:.2}°/min, GPS {:.1} s late ({}), ρ {:.2}, ±{:.1}°, {:.0} s of acceleration, scale {:.2}: confidence {:.2}",
                a.theta.to_degrees(), a.drift.to_degrees() * 60.0, a.lag, telemetry.gps_clock, a.rho, a.sigma.to_degrees(), a.excited_s, a.scale, a.confidence());
            if ok { headings = headings.with_travel(tr); }
            serde_json::json!({
                "available": ok, "confidence": a.confidence(), "sigma_deg": a.sigma.to_degrees(), "rho": a.rho, "lag_s": a.lag,
                "gps_clock": telemetry.gps_clock, "drift_deg_min": a.drift.to_degrees() * 60.0, "moving": moving_share,
                "why": if ok { serde_json::Value::Null } else { "the camera's turns don't match the GPS track well enough".into() },
            })
        }
    };
    // North: a compass where the camera has one (none of the Insta360s; telemetry.north stays empty), else the GPS
    // fit's true north
    let cams: Vec<nalgebra::UnitQuaternion<f64>> = if telemetry.north.is_empty() { Vec::new() } else {
        let gyro = stab.gyro.read();
        (0..frame_count).map(|i| world.world_from_cam(i as f64, &gyro.org_quat_at_timestamp(frame_s(i) * 1000.0))).collect()
    };
    let no_compass = if rig.brand == "Insta360" { "an Insta360 has no compass" } else { "no compass in this clip" };
    report["north"] = if !telemetry.north.is_empty() {
        let compass: Vec<(Vector3<f64>, f64)> = (0..frame_count).map(|i| {
            let t = frame_s(i) * 1000.0;
            let k = telemetry.north.partition_point(|n| n.0 < t).min(telemetry.north.len() - 1);
            let k = if k > 0 && (telemetry.north[k - 1].0 - t).abs() < (telemetry.north[k].0 - t).abs() { k - 1 } else { k };
            (telemetry.north[k].1, telemetry.north[k].2)
        }).collect();
        let n = crate::heading::north(&cams, &compass, fps);
        let confidence = n.iter().map(|x| x.1).sum::<f64>() / n.len().max(1) as f64;
        let ok = confidence > 0.1;
        if ok { headings = headings.with_north(n); }
        serde_json::json!({ "available": ok, "source": "compass", "confidence": confidence,
                            "why": if ok { serde_json::Value::Null } else { "the camera wasn't sure of its compass".into() } })
    } else if let Some(a) = alignment.as_ref().filter(|a| a.confidence() > 0.2) {
        let n: Vec<(f64, f64)> = (0..frame_count).map(|i| (-a.theta_at(frame_s(i)), a.confidence())).collect();
        headings = headings.with_north(n);
        serde_json::json!({ "available": true, "source": "gps", "confidence": a.confidence() })
    } else {
        serde_json::json!({ "available": false, "why": if track.len() < 20 { format!("{no_compass}, and no GPS track to find north by") }
                                                       else { format!("{no_compass}, and the GPS track doesn't tie the camera's turns to the world") } })
    };
    // In the world, held in the real world where north or the GPS fit say how the gyro's world turns
    let drift: Option<(Vec<f64>, &str)> = if let Some(n) = headings.north().filter(|n| n.iter().any(|x| x.1 > 0.1)) {
        let first = n[0].0;
        Some((n.iter().map(|x| (x.0 - first + PI).rem_euclid(TAU) - PI).collect(), if telemetry.north.is_empty() { "gps" } else { "north" }))
    } else { None };
    report["world"] = match &drift {
        Some((d, by)) => serde_json::json!({ "held_by": by, "drift_deg_min": d.last().map(|x| x.to_degrees() / (frame_count as f64 / fps) * 60.0) }),
        None => serde_json::json!({ "held_by": null }),
    };
    if let Some((d, _)) = drift { headings = headings.with_drift(d); }
    // Per second, for the panel: how sure each is then
    let seconds = (frame_count as f64 / fps).ceil() as usize;
    report["seconds"] = (0..seconds).map(|sec| {
        let i = ((sec as f64 + 0.5) * fps) as usize;
        let c = |v: Option<&[(f64, f64)]>| v.and_then(|v| v.get(i.min(v.len().saturating_sub(1)))).map(|x| (x.1 * 100.0).round() / 100.0).unwrap_or(0.0);
        serde_json::json!([c(headings.travel()), c(headings.north())])
    }).collect::<Vec<_>>().into();
    (headings, report)
}

struct Job {
    timestamp_us: i64,
    planes: Vec<PlaneJob>,
    limited_range: bool,
    reply: mpsc::SyncSender<Result<(), String>>,
}
// The frame buffers stay alive until the reply arrives: get_frame blocks on it
unsafe impl Send for Job { }

#[derive(Clone, Copy)]
struct Config {
    kind: SampleKind,
    depth: u32,
    is_yuv: bool,
    interpolation: Interpolation,
    subsampling: (usize, usize),
}

/// Everything the workers share
struct Shared {
    rig: Rig,
    /// Per lens, its parameters
    params: Vec<ComputeParams>,
    /// Per lens from the second, per plane kind (luma, chroma): where it's blended in
    spans: Vec<[Spans; 2]>,
    sphere: (usize, usize),
    stamps: Stamps,
    device: isize,
    cfg: Config,
}

/// A loaded 360 clip: its rig, motion and GPU workers. Cached for the life of the process, as the flat Engine is: mpv
/// makes the filter again on every seek
pub struct Sphere360 {
    tx: mpsc::Sender<Job>,
    shared: Arc<Shared>,
    pub frame_offset_us: i64,
    /// What the heading modes found (Motion's report)
    pub report: serde_json::Value,
    /// The clip's frame rate, as gyroflow-core has it
    pub fps: f64,
    throughput: Mutex<(u32, std::time::Instant)>,
}

impl Sphere360 {
    /// `video`: the camera's file; `input_size`: the joined frame mpv hands over; `width`: the sphere's; `distance`:
    /// the stitching distance in metres (0: the rig's own, else DEFAULT_DISTANCE)
    pub fn load(video: &str, input_size: (usize, usize), num_planes: usize, cfg_in: (SampleKind, u32, bool, (usize, usize)), interpolation: Interpolation, width: usize, distance: f64, workers: usize) -> Result<Sphere360, String> {
        let t = std::time::Instant::now();
        let (kind, depth, is_yuv, subsampling) = cfg_in;
        let rig = crate::rig::read(video).ok_or_else(|| format!("{video}: not a 360 camera's file (no rig in its metadata)"))??;
        if rig.joined_size() != input_size {
            return Err(format!("{video}: {} tracks of {}x{} joined are {:?}, the clip is {:?} (the tracks side by side in order, unscaled)",
                rig.pictures, rig.picture.0, rig.picture.1, rig.joined_size(), input_size));
        }
        let distance = if distance > 0.0 { distance } else { rig.convergence.unwrap_or(DEFAULT_DISTANCE) };
        let sphere_w = (width.max(512) / 4) * 4;
        let sphere = (sphere_w, sphere_w / 2);

        let Motion { stab, world, headings, fps, frame_offset_us, report, .. } = Motion::load(video, &rig)?;

        // Per lens, its parameters
        let base = ComputeParams::from_manager(&stab);
        let params = rig.lenses.iter().map(|l| {
            let mut p = base.clone();
            let mut lens = LensProfile::from_value(l.lens_profile(&rig.brand, &rig.model)).map_err(|e| format!("lens {}: {e}", l.index))?;
            lens.init();
            p.lens = lens;
            p.distortion_model = DistortionModel::from_name("insta360");
            p.digital_lens = None;
            p.digital_lens_params = None;
            p.width = l.rect.2;
            p.height = l.rect.3;
            p.output_width = sphere.0;
            p.output_height = sphere.1;
            p.output_projection = OutputProjection::Equirect as i32;
            p.fov_scale = 1.0;
            p.fovs.clear();
            p.minimal_fovs.clear();
            p.adaptive_zoom_window = 0.0;
            p.lens_correction_amount = 1.0;
            p.video_rotation = 0.0;
            p.view_rotation = (0.0, 0.0, 0.0);
            p.additional_rotation = (0.0, 0.0, 0.0);
            p.additional_translation = (0.0, 0.0, 0.0);
            p.focal_lengths.clear();
            p.smoothed_focal_lengths.clear();
            p.focal_length_smoothing_enabled = false;
            p.lens_breathing_enabled = false;
            p.background_mode = BackgroundMode::from(0);
            p.background = nalgebra::Vector4::new(0.0, 0.0, 0.0, 0.0);
            let (ms, top) = l.readout.unwrap_or((0.0, true));
            p.frame_readout_time = ms;
            p.frame_readout_direction = if top { ReadoutDirection::TopToBottom } else { ReadoutDirection::BottomToTop };
            p.rig_lens = RigLens { rotation: l.rotation, translation: l.translation, stitching_distance: distance, validity: l.validity };
            Ok(p)
        }).collect::<Result<Vec<_>, String>>()?;

        let chroma = (sphere.0 >> subsampling.0, sphere.1 >> subsampling.1);
        let spans = (1..rig.lenses.len()).map(|i| [Spans::new(&rig, i, distance, sphere.0, sphere.1), Spans::new(&rig, i, distance, chroma.0, chroma.1)]).collect();
        log::info!("{video}: {} {} rig, {} lenses in {} tracks of {}x{}; the sphere {}x{} at a stitching distance of {distance} m; weights in {:.0} ms",
            rig.brand, rig.model, rig.lenses.len(), rig.pictures, rig.picture.0, rig.picture.1, sphere.0, sphere.1, t.elapsed().as_secs_f64() * 1000.0);

        let device = stab.params.read().current_device as isize;
        let cfg = Config { kind, depth, is_yuv, interpolation, subsampling };
        let shared = Arc::new(Shared { rig, params, spans, sphere, stamps: Stamps { stab, world, headings, fps, frame_offset_us }, device, cfg });

        let (tx, rx) = mpsc::channel::<Job>();
        let rx = Arc::new(Mutex::new(rx));
        let (ready_tx, ready_rx) = mpsc::channel::<()>();
        let workers = workers.max(1);
        for i in 0..workers {
            let (shared, rx, ready) = (shared.clone(), rx.clone(), ready_tx.clone());
            std::thread::Builder::new().name(format!("gyroflow-360-{i}")).spawn(move || worker(&shared, num_planes, &rx, ready)).map_err(|e| e.to_string())?;
        }
        for _ in 0..workers { let _ = ready_rx.recv(); }
        log::info!("{video}: 360 mode ready after {:.0} ms, {workers} GPU workers", t.elapsed().as_secs_f64() * 1000.0);
        Ok(Sphere360 { tx, shared, frame_offset_us, report, fps, throughput: Mutex::new((0, std::time::Instant::now())) })
    }

    /// The output's size: the sphere and the stamp's rows under it
    pub fn output_size(&self) -> (usize, usize) { (self.shared.sphere.0, self.shared.sphere.1 + stamp::ROWS) }

    pub fn process(&self, timestamp_us: i64, planes: Vec<PlaneJob>, limited_range: bool) -> Result<(), String> {
        let (reply, result) = mpsc::sync_channel(1);
        self.tx.send(Job { timestamp_us, planes, limited_range, reply }).map_err(|_| "Gyroflow 360 worker is gone".to_string())?;
        let r = result.recv().map_err(|_| "Gyroflow 360 worker is gone".to_string())?;
        let mut t = self.throughput.lock();
        t.0 += 1;
        if t.0 == 300 {
            log::info!("360: {:.1} frames per second", t.0 as f64 / t.1.elapsed().as_secs_f64());
            *t = (0, std::time::Instant::now());
        }
        r
    }
}

/// One lens of one plane on the GPU
struct LensPlane {
    stab: Stabilization,
    lens: usize,
    plane: usize,
    /// The bytes of the input and output buffers it was set up with: another size sets it up again (see PlaneStab)
    buffers: (usize, usize),
    limited_range: bool,
}

impl LensPlane {
    fn new(shared: &Shared, lens: usize, plane: usize, limited_range: bool) -> LensPlane {
        let p = &shared.params[lens];
        let mut stab = Stabilization::default();
        stab.interpolation = shared.cfg.interpolation;
        stab.share_wgpu_instances = false;
        stab.set_device(shared.device);
        // In the lens's (luma) pixels; a chroma plane's buffers map onto it
        stab.init_size((p.width, p.height), (p.output_width, p.output_height));
        let mut lp = LensPlane { stab, lens, plane, buffers: (0, 0), limited_range: !limited_range };
        lp.set_range(shared, limited_range);
        lp
    }

    /// The background (black, neutral chroma) in this plane's channel and range
    fn set_range(&mut self, shared: &Shared, limited_range: bool) {
        if self.limited_range == limited_range { return; }
        self.limited_range = limited_range;
        let mut p = shared.params[self.lens].clone();
        let channel = if shared.cfg.is_yuv { [self.plane] } else { [0] };
        p.background = match shared.cfg.kind {
            SampleKind::U8 => <Luma8 as PixelType>::from_rgb_color(p.background, &channel, limited_range),
            SampleKind::U16 => <Luma16 as PixelType>::from_rgb_color(p.background, &channel, limited_range),
        };
        self.stab.set_compute_params(p);
    }

    /// The lens's part of `src` (the joined plane: its pointer, stride, height; `rect` in it) into `dst`. Only that part
    /// goes to the GPU: its rows from where it starts, the joined plane's stride apart (a texture upload by rows reads
    /// the last row only as far as its width)
    fn render<T: PixelType>(&mut self, shared: &Shared, timestamp_us: i64, src: (*const u8, usize, usize), rect: (usize, usize, usize, usize), dst: (*mut u8, usize, usize, usize)) -> Result<(), String> {
        let max_value = ((1u32 << shared.cfg.depth) - 1) as f32;
        let bytes = if shared.cfg.kind == SampleKind::U8 { 1 } else { 2 };
        let (sp, sstride, sh) = src;
        let (dp, dstride, dw, dh) = dst;
        if rect.1 + rect.3 > sh || (rect.0 + rect.2) * bytes > sstride { return Err(format!("the lens's part {rect:?} is outside the input")); }
        let offset = rect.1 * sstride + rect.0 * bytes;
        // gyroflow-core takes mutable slices for both sides, but only writes the output
        let s = unsafe { std::slice::from_raw_parts_mut(sp.add(offset) as *mut u8, sstride * sh - offset) };
        let d = unsafe { std::slice::from_raw_parts_mut(dp, dstride * dh) };
        let (sw, sh) = (rect.2, rect.3);
        let mut buffers = Buffers {
            input: BufferDescription { size: (sw, sh, sstride), data: BufferSource::Cpu { buffer: s }, ..Default::default() },
            output: BufferDescription { size: (dw, dh, dstride), data: BufferSource::Cpu { buffer: d }, ..Default::default() },
        };
        let sizes = (sstride * sh, dstride * dh);
        if self.buffers != sizes {
            if self.buffers != (0, 0) { self.stab.set_device(shared.device); }
            self.buffers = sizes;
        }
        let stab = &mut self.stab;
        if stab.initialized_backend.is_none() || stab.pending_device_change.is_some() {
            stab.ensure_ready_for_processing::<T>(timestamp_us, None, &mut buffers);
            stab.stab_data.clear();
        }
        let mut transform = stab.get_frame_transform_at::<T>(timestamp_us, None, &buffers);
        transform.kernel_params.pixel_value_limit = max_value;
        transform.kernel_params.max_pixel_value = max_value;
        if stab.initialized_backend.is_wgpu() && T::wgpu_format().map(|x| x.2).unwrap_or_default() {
            transform.kernel_params.pixel_value_limit = 1.0;
            transform.kernel_params.max_pixel_value = 1.0;
        }
        transform.kernel_params.plane_index = self.plane as i32;
        stab.process_pixels::<T>(timestamp_us, None, &mut buffers, Some(&transform)).map(|_| ()).map_err(|e| format!("process_pixels: {e:?}"))
    }
}

fn worker(shared: &Shared, num_planes: usize, rx: &Mutex<mpsc::Receiver<Job>>, ready: mpsc::Sender<()>) {
    let cfg = shared.cfg;
    let lenses = shared.rig.lenses.len();
    let bytes = if cfg.kind == SampleKind::U8 { 1 } else { 2 };
    let plane_size = |(w, h): (usize, usize), p: usize| if p == 0 { (w, h) } else { (w >> cfg.subsampling.0, h >> cfg.subsampling.1) };
    let joined = shared.rig.joined_size();
    let mut limited_range = cfg.is_yuv;
    let mut gpu: Vec<Vec<LensPlane>> = (0..lenses).map(|l| (0..num_planes).map(|p| LensPlane::new(shared, l, p, limited_range)).collect()).collect();
    // Each lens after the first renders here, then is blended into the output
    let mut scratch: Vec<Vec<Vec<u8>>> = (0..lenses).map(|l| if l == 0 { Vec::new() } else {
        (0..num_planes).map(|p| { let (w, h) = plane_size(shared.sphere, p); vec![0u8; w * h * bytes] }).collect()
    }).collect();

    // Set up on a frame of the sizes that come, before playback: mpv waits for the filter, not for its first frames
    {
        let stride = |w: usize| (w * bytes).div_ceil(32) * 32;
        let out = (shared.sphere.0, shared.sphere.1 + stamp::ROWS);
        let mut bufs: Vec<(Vec<u8>, Vec<u8>)> = (0..num_planes).map(|p| {
            let (i, o) = (plane_size(joined, p), plane_size(out, p));
            (vec![0u8; stride(i.0) * i.1], vec![0u8; stride(o.0) * o.1])
        }).collect();
        let planes: Vec<PlaneJob> = (0..num_planes).zip(bufs.iter_mut()).map(|(p, (s, d))| {
            let (i, o) = (plane_size(joined, p), plane_size(out, p));
            PlaneJob { src: s.as_ptr(), src_stride: stride(i.0), src_w: i.0, src_h: i.1, dst: d.as_mut_ptr(), dst_stride: stride(o.0), dst_w: o.0, dst_h: o.1 }
        }).collect();
        if let Err(e) = frame(shared, &mut gpu, &mut scratch, 0, &planes, &mut [0.0; 3]) { log::error!("360 warm-up: {e}"); }
    }
    let _ = ready.send(());

    let (mut frames, mut busy) = (0u32, std::time::Duration::ZERO);
    let mut times = [0.0f64; 3];
    loop {
        let Ok(job) = rx.lock().recv() else { break };
        let t = std::time::Instant::now();
        if job.limited_range != limited_range {
            limited_range = job.limited_range;
            for lp in gpu.iter_mut().flatten() { lp.set_range(shared, limited_range); }
        }
        let result = frame(shared, &mut gpu, &mut scratch, job.timestamp_us, &job.planes, &mut times);
        let _ = job.reply.send(result);
        frames += 1;
        busy += t.elapsed();
        if frames == 300 {
            let ms = |v: f64| v * 1000.0 / frames as f64;
            log::info!("360: {:.1} ms per frame on the GPU worker: the first lens {:.1}, the others {:.1}, their blend {:.1}",
                       ms(busy.as_secs_f64()), ms(times[0]), ms(times[1]), ms(times[2]));
            (frames, busy, times) = (0, std::time::Duration::ZERO, [0.0; 3]);
        }
    }
}

/// One frame: every lens of every plane, the blend, the stamp
fn frame(shared: &Shared, gpu: &mut [Vec<LensPlane>], scratch: &mut [Vec<Vec<u8>>], timestamp_us: i64, planes: &[PlaneJob], times: &mut [f64; 3]) -> Result<(), String> {
    let cfg = shared.cfg;
    for (p, pj) in planes.iter().enumerate() {
        let (ss, sv) = if p == 0 { (0, 0) } else { cfg.subsampling };
        let (w, h) = (shared.sphere.0 >> ss, shared.sphere.1 >> sv);
        if pj.dst_w < w || pj.dst_h < h { return Err(format!("plane {p}: the output {}x{} is smaller than the sphere {w}x{h}", pj.dst_w, pj.dst_h)); }
        for (l, lens_planes) in gpu.iter_mut().enumerate() {
            let r = shared.rig.source_rect(l);
            let rect = (r.0 >> ss, r.1 >> sv, r.2 >> ss, r.3 >> sv);
            let src = (pj.src, pj.src_stride, pj.src_h);
            let bytes = if cfg.kind == SampleKind::U8 { 1 } else { 2 };
            let dst = if l == 0 { (pj.dst, pj.dst_stride, w, h) } else { (scratch[l][p].as_mut_ptr(), w * bytes, w, h) };
            let lp = &mut lens_planes[p];
            let t = std::time::Instant::now();
            match cfg.kind {
                SampleKind::U8 => lp.render::<Luma8>(shared, timestamp_us, src, rect, dst)?,
                SampleKind::U16 => lp.render::<Luma16>(shared, timestamp_us, src, rect, dst)?,
            }
            times[(l > 0) as usize] += t.elapsed().as_secs_f64();
            let t = std::time::Instant::now();
            if l > 0 {
                let spans = &shared.spans[l - 1][(p > 0) as usize];
                match cfg.kind {
                    SampleKind::U8 => {
                        let out = unsafe { std::slice::from_raw_parts_mut(pj.dst, pj.dst_stride * h) };
                        spans.blend::<u8>(&scratch[l][p], w, out, pj.dst_stride);
                    }
                    SampleKind::U16 => {
                        let out = unsafe { std::slice::from_raw_parts_mut(pj.dst as *mut u16, pj.dst_stride / 2 * h) };
                        let s: &[u16] = bytemuck::cast_slice(&scratch[l][p]);
                        spans.blend::<u16>(s, w, out, pj.dst_stride / 2);
                    }
                }
            }
            times[2] += t.elapsed().as_secs_f64();
        }
        // Under the sphere: the stamp's rows, black, and neutral chroma under them
        let fill = if p == 0 { if cfg.is_yuv { 16u32 << (cfg.depth - 8) } else { 0 } } else { 128u32 << (cfg.depth - 8) };
        unsafe {
            match cfg.kind {
                SampleKind::U8 => std::ptr::write_bytes(pj.dst.add(h * pj.dst_stride), fill as u8, (pj.dst_h - h) * pj.dst_stride),
                SampleKind::U16 => std::slice::from_raw_parts_mut(pj.dst.add(h * pj.dst_stride) as *mut u16, (pj.dst_h - h) * pj.dst_stride / 2).fill(fill as u16),
            }
        }
    }
    let luma = planes.first().ok_or("no planes")?;
    let s = shared.stamps.at(timestamp_us);
    let written = unsafe {
        match cfg.kind {
            SampleKind::U8 => stamp::write(std::slice::from_raw_parts_mut(luma.dst, luma.dst_stride * luma.dst_h), luma.dst_stride, luma.dst_w, luma.dst_h, cfg.depth, &s),
            SampleKind::U16 => stamp::write(std::slice::from_raw_parts_mut(luma.dst as *mut u16, luma.dst_stride / 2 * luma.dst_h), luma.dst_stride / 2, luma.dst_w, luma.dst_h, cfg.depth, &s),
        }
    };
    if !written { return Err("the frame is too small for the stamp".into()); }
    Ok(())
}

// ---------------------------------------------------------------------------------------------------
// The filter: `core.gyroflow.Sphere360(clip, video, width=5760, distance=0, interpolation="bilinear", workers=3,
// start_time=0, time_offset=0)`

static ENGINES: Mutex<Vec<(String, Arc<Sphere360>)>> = Mutex::new(Vec::new());

struct Filter {
    node: *mut VSNode,
    engine: Arc<Sphere360>,
    out_size: (c_int, c_int),
    num_planes: usize,
    is_yuv: bool,
    start_time: f64,
    time_offset: f64,
    fallback_duration: f64,
}

unsafe extern "system-unwind" fn get_frame(n: c_int, reason: VSActivationReason, instance: *mut c_void, frame_data: *mut *mut c_void, ctx: *mut VSFrameContext, core: *mut VSCore, vsapi: *const VSAPI) -> *const VSFrame {
    let f = unsafe { &*(instance as *const Filter) };
    let api = unsafe { &*vsapi };
    match reason {
        VSActivationReason::Initial => unsafe {
            (api.requestFrameFilter)(n, f.node, ctx);
            if crate::pts_unknown() {
                (api.requestFrameFilter)(n + 1, f.node, ctx);
                *frame_data = 1 as *mut c_void;
            }
            null()
        },
        VSActivationReason::AllFramesReady => unsafe {
            let src = (api.getFrameFilter)(n, f.node, ctx);
            let props = (api.getFramePropertiesRO)(src);
            let duration = crate::frame_duration(api, props, f.fallback_duration);
            if !(*frame_data).is_null() {
                let next = (api.getFrameFilter)(n + 1, f.node, ctx);
                crate::calibrate_pts(api, props, (api.getFramePropertiesRO)(next), duration);
                (api.freeFrame)(next);
            }
            let time = crate::frame_pts(api, props, n, duration);
            let timestamp_us = ((time - f.start_time + f.time_offset) * 1_000_000.0).round() as i64 + f.engine.frame_offset_us;
            let mut err = VSMapPropertyError::Success;
            let range = (api.mapGetInt)(props, c"_ColorRange".as_ptr(), 0, &mut err);
            let limited_range = if err == VSMapPropertyError::Success { range == 1 } else { f.is_yuv };

            let format = (api.getVideoFrameFormat)(src);
            let dst = (api.newVideoFrame)(format, f.out_size.0, f.out_size.1, src, core);
            let planes = (0..f.num_planes as c_int).map(|p| PlaneJob {
                src: (api.getReadPtr)(src, p), src_stride: (api.getStride)(src, p) as usize,
                src_w: (api.getFrameWidth)(src, p) as usize, src_h: (api.getFrameHeight)(src, p) as usize,
                dst: (api.getWritePtr)(dst, p), dst_stride: (api.getStride)(dst, p) as usize,
                dst_w: (api.getFrameWidth)(dst, p) as usize, dst_h: (api.getFrameHeight)(dst, p) as usize,
            }).collect();
            let result = f.engine.process(timestamp_us, planes, limited_range);
            (api.freeFrame)(src);
            match result {
                Ok(()) => dst,
                Err(e) => {
                    log::error!("360 frame {n} at {timestamp_us} us: {e}");
                    (api.freeFrame)(dst);
                    let msg = CString::new(format!("Gyroflow 360: {e}")).unwrap_or_default();
                    (api.setFilterError)(msg.as_ptr(), ctx);
                    null()
                }
            }
        },
        _ => null(),
    }
}

unsafe extern "system-unwind" fn free(instance: *mut c_void, _core: *mut VSCore, vsapi: *const VSAPI) {
    let f = unsafe { Box::from_raw(instance as *mut Filter) };
    unsafe { ((*vsapi).freeNode)(f.node) };
}

pub unsafe extern "system-unwind" fn create(input: *const VSMap, out: *mut VSMap, _user_data: *mut c_void, core: *mut VSCore, vsapi: *const VSAPI) {
    crate::init_logging();
    let api = unsafe { &*vsapi };
    let mut err = VSMapPropertyError::Success;
    let node = unsafe { (api.mapGetNode)(input, c"clip".as_ptr(), 0, &mut err) };
    let fail = |msg: String| unsafe { crate::set_error(api, out, node, "Sphere360", msg) };
    let vi = unsafe { (*(api.getVideoInfo)(node)).clone() };
    let fmt = vi.format.clone();
    let is_yuv = fmt.color_family == VSColorFamily::YUV;
    if !(is_yuv || fmt.color_family == VSColorFamily::Gray) || fmt.sample_type != VSSampleType::Integer || fmt.bytes_per_sample > 2 || vi.width == 0 {
        return fail("only constant-size YUV or Gray clips with 8-16 bit integer samples".into());
    }
    let Some(video) = (unsafe { crate::map_get_str(api, input, c"video") }) else { return fail("video is required".into()) };
    // Bilinear by default: the kernel's time is its taps where a lens has picture (bicubic's 16 took an X5's frame at
    // 7680 to ~120 ms a GPU worker, past what 25 fps leaves; bilinear's 4, ~70), and the view shader enlarges the
    // sphere at most FOVs anyway
    let interpolation = match unsafe { crate::map_get_str(api, input, c"interpolation") }.unwrap_or_else(|| "bilinear".into()).to_ascii_lowercase().as_str() {
        "bilinear" => Interpolation::Bilinear,
        "bicubic" => Interpolation::Bicubic,
        "lanczos" | "lanczos4" => Interpolation::Lanczos4,
        other => return fail(format!("unknown interpolation {other} (bilinear, bicubic, lanczos)")),
    };
    let width = unsafe { crate::map_get_i64(api, input, c"width") }.filter(|w| *w > 0).map(|w| w as usize).unwrap_or(DEFAULT_WIDTH).clamp(512, 16384);
    let distance = unsafe { crate::map_get_f64(api, input, c"distance") }.filter(|d| d.is_finite() && *d >= 0.0).unwrap_or(0.0);
    let workers = unsafe { crate::map_get_i64(api, input, c"workers") }.unwrap_or(3).clamp(1, 6) as usize;
    let kind = if fmt.bytes_per_sample == 1 { SampleKind::U8 } else { SampleKind::U16 };
    let input_size = (vi.width as usize, vi.height as usize);
    let version = std::fs::metadata(&video).map(|m| format!("{:?}/{}", m.modified().ok(), m.len())).unwrap_or_default();
    let key = format!("{video}|{version}|{input_size:?}|{:?}{}|{}|{}|{width}|{distance}|{interpolation:?}|{workers}", fmt.color_family, fmt.bits_per_sample, fmt.sub_sampling_w * 10 + fmt.sub_sampling_h, fmt.num_planes);
    let engine = {
        let mut engines = ENGINES.lock();
        match engines.iter().find(|e| e.0 == key) {
            Some(e) => e.1.clone(),
            None => {
                // One 360 clip at a time: an 8K one's GPU buffers are big
                engines.clear();
                match Sphere360::load(&video, input_size, fmt.num_planes as usize, (kind, fmt.bits_per_sample as u32, is_yuv, (fmt.sub_sampling_w as usize, fmt.sub_sampling_h as usize)), interpolation, width, distance, workers) {
                    Ok(e) => { let e = Arc::new(e); engines.push((key, e.clone())); e }
                    Err(e) => { drop(engines); return fail(e); }
                }
            }
        }
    };
    let (w, h) = engine.output_size();
    let mut out_vi = vi.clone();
    out_vi.width = w as c_int;
    out_vi.height = h as c_int;
    let fallback_duration = if vi.fps_num > 0 && vi.fps_den > 0 { vi.fps_den as f64 / vi.fps_num as f64 } else { 1.0 / 30.0 };
    let filter = Box::new(Filter {
        node, engine, out_size: (out_vi.width, out_vi.height), num_planes: fmt.num_planes as usize, is_yuv,
        start_time: unsafe { crate::map_get_f64(api, input, c"start_time") }.unwrap_or(0.0),
        time_offset: unsafe { crate::map_get_f64(api, input, c"time_offset") }.unwrap_or(0.0),
        fallback_duration,
    });
    let deps = [VSFilterDependency { source: node, request_pattern: VSRequestPattern::StrictSpatial }];
    unsafe {
        (api.createVideoFilter)(out, c"Sphere360".as_ptr(), &out_vi, get_frame, Some(free), VSFilterMode::Parallel,
                                deps.as_ptr(), deps.len() as c_int, Box::into_raw(filter) as *mut c_void, core);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_spans_blend_each_lens_by_its_share() {
        let rig = Rig::new(&crate::rig::tests::x5(), (3840, 3840)).unwrap();
        let (w, h) = (720, 360);
        let spans = Spans::new(&rig, 1, 2.0, w, h);
        // Lens 0 (all 10) and lens 1 (all 200): where lens 1 has the whole of it, 200; lens 0 alone, 10; the band between
        let mut out = vec![10u8; w * h];
        spans.blend::<u8>(&vec![200u8; w * h], w, &mut out, w);
        for y in [10, 180, 350] {
            for x in 0..w {
                let wts = rig.weights(equirect_direction(x as f64, y as f64, w, h), 2.0);
                let want = 10.0 * wts[0] + 200.0 * wts[1];
                assert!((out[y * w + x] as f64 - want).abs() <= 1.5, "{x} {y}: {} against {want}", out[y * w + x]);
            }
        }
        // The equator's middle is lens 0's (its axis), its ends lens 1's
        assert_eq!(out[180 * w + w / 2], 10);
        assert_eq!(out[180 * w], 200);
        // 16 bits too
        let mut out16 = vec![1000u16; w * h];
        spans.blend::<u16>(&vec![60000u16; w * h], w, &mut out16, w);
        assert_eq!(out16[180 * w], 60000);
        assert_eq!(out16[180 * w + w / 2], 1000);
    }

    /// A real clip's frames through the 360 mode, for looking at: `AFP_360_CLIP` (a 360 camera's file), `AFP_360_OUT` (a
    /// folder), `AFP_360_TIMES` (seconds, comma separated; 1 by default), `AFP_360_WIDTH`, `AFP_360_DISTANCE`. ffmpeg
    /// joins the tracks as mpv's lavfi-complex does; each frame's sphere goes to `sphere-<t>.png`, its stamp to the log
    #[test]
    #[ignore]
    fn a_clip_into_the_sphere() {
        crate::init_logging();
        let clip = std::env::var("AFP_360_CLIP").expect("AFP_360_CLIP");
        let out = std::path::PathBuf::from(std::env::var("AFP_360_OUT").expect("AFP_360_OUT"));
        let times: Vec<f64> = std::env::var("AFP_360_TIMES").unwrap_or("1".into()).split(',').map(|t| t.trim().parse().unwrap()).collect();
        let width = std::env::var("AFP_360_WIDTH").ok().and_then(|w| w.parse().ok()).unwrap_or(DEFAULT_WIDTH);
        let distance = std::env::var("AFP_360_DISTANCE").ok().and_then(|w| w.parse().ok()).unwrap_or(0.0);
        let rig = crate::rig::read(&clip).unwrap().unwrap();
        let (jw, jh) = rig.joined_size();
        let t = std::time::Instant::now();
        let engine = Sphere360::load(&clip, (jw, jh), 3, (SampleKind::U8, 8, true, (1, 1)), Interpolation::Bilinear, width, distance, 1).unwrap();
        eprintln!("loaded in {:.0} ms", t.elapsed().as_secs_f64() * 1000.0);
        let (ow, oh) = engine.output_size();
        let inputs: String = (0..rig.pictures).map(|i| format!("[0:v:{i}]")).collect();
        for time in times {
            let raw = out.join(format!("joined-{time}.yuv"));
            let status = std::process::Command::new("ffmpeg").args(["-v", "error", "-y", "-ss", &format!("{time}"), "-i", &clip, "-filter_complex",
                &format!("{inputs}hstack=inputs={},format=yuvj420p", rig.pictures), "-frames:v", "1", "-f", "rawvideo"]).arg(&raw).status().unwrap();
            assert!(status.success());
            let src = std::fs::read(&raw).unwrap();
            let _ = std::fs::remove_file(&raw);
            assert_eq!(src.len(), jw * jh * 3 / 2);
            let mut dst = vec![0u8; ow * oh * 3 / 2];
            let (sy, sc) = (jw * jh, jw * jh / 4);
            let (dy, dc) = (ow * oh, ow * oh / 4);
            let planes = vec![
                PlaneJob { src: src.as_ptr(), src_stride: jw, src_w: jw, src_h: jh, dst: dst.as_mut_ptr(), dst_stride: ow, dst_w: ow, dst_h: oh },
                PlaneJob { src: src[sy..].as_ptr(), src_stride: jw / 2, src_w: jw / 2, src_h: jh / 2, dst: dst[dy..].as_mut_ptr(), dst_stride: ow / 2, dst_w: ow / 2, dst_h: oh / 2 },
                PlaneJob { src: src[sy + sc..].as_ptr(), src_stride: jw / 2, src_w: jw / 2, src_h: jh / 2, dst: dst[dy + dc..].as_mut_ptr(), dst_stride: ow / 2, dst_w: ow / 2, dst_h: oh / 2 },
            ];
            let ts = (time * 1e6).round() as i64 + engine.frame_offset_us;
            let t = std::time::Instant::now();
            engine.process(ts, planes, false).unwrap();
            eprintln!("{time} s: rendered in {:.0} ms", t.elapsed().as_secs_f64() * 1000.0);
            let st = engine.shared.stamps.at(ts);
            let qq = nalgebra::UnitQuaternion::from_quaternion(nalgebra::Quaternion::new(st.q_world_from_cam[0], st.q_world_from_cam[1], st.q_world_from_cam[2], st.q_world_from_cam[3]));
            let up = qq.inverse() * -Vector3::y();
            // The camera's up in the rig's axes, for tools/sphere360/horizon.py
            eprintln!("UP {time} {:.4} {:.4} {:.4}", up.x, up.y, up.z);
            let fixed = stamp::read(&dst[..dy], ow, ow, oh, 8).unwrap();
            let s = fixed.to_stamp();
            let q = nalgebra::UnitQuaternion::from_quaternion(nalgebra::Quaternion::new(s.q_world_from_cam[0], s.q_world_from_cam[1], s.q_world_from_cam[2], s.q_world_from_cam[3]));
            let (r, p, y) = q.euler_angles();
            eprintln!("{time} s: stamp time {} us, q {:?} (euler {:.1} {:.1} {:.1}), headings {:?}", s.time_us, s.q_world_from_cam,
                      r.to_degrees(), p.to_degrees(), y.to_degrees(), s.headings.map(|h| (h.yaw.to_degrees().round(), h.confidence)));
            // The view shader's composition with the head straight ahead, over the whole sphere: each direction of the
            // world turned to the camera's heading, into this frame's camera, looked up (nearest pixel, luma). The horizon
            // should come out straight across the middle
            {
                let heading = nalgebra::UnitQuaternion::from_axis_angle(&Vector3::y_axis(), s.headings[1].yaw);
                let cam_from_view = q.inverse() * heading;
                let (lw, lh) = (2048usize, 1024usize);
                let mut level = vec![0u8; lw * lh];
                for y in 0..lh {
                    for x in 0..lw {
                        let c = cam_from_view * equirect_direction(x as f64, y as f64, lw, lh);
                        let lon = c.x.atan2(c.z);
                        let lat = (-c.y).asin();
                        let sx = (((lon / std::f64::consts::TAU + 0.5) * ow as f64) as usize).min(ow - 1);
                        let sy = (((0.5 - lat / std::f64::consts::PI) * (oh - stamp::ROWS) as f64) as usize).min(oh - stamp::ROWS - 1);
                        level[y * lw + x] = dst[sy * ow + sx];
                    }
                }
                let pgm = out.join(format!("level-{time}.pgm"));
                let mut f = format!("P5 {lw} {lh} 255\n").into_bytes();
                f.extend_from_slice(&level);
                std::fs::write(&pgm, f).unwrap();
                let status = std::process::Command::new("ffmpeg").args(["-v", "error", "-y", "-i"]).arg(&pgm).arg(out.join(format!("level-{time}.png"))).status().unwrap();
                assert!(status.success());
                let _ = std::fs::remove_file(&pgm);
            }
            let yuv = out.join(format!("sphere-{time}.yuv"));
            std::fs::write(&yuv, &dst).unwrap();
            let png = out.join(format!("sphere-{time}.png"));
            let status = std::process::Command::new("ffmpeg").args(["-v", "error", "-y", "-f", "rawvideo", "-pix_fmt", "yuvj420p", "-s", &format!("{ow}x{oh}"), "-i"])
                .arg(&yuv).args(["-vf", "scale=2048:-1", "-frames:v", "1"]).arg(&png).status().unwrap();
            assert!(status.success());
            let _ = std::fs::remove_file(&yuv);
        }
    }
}

#[cfg(test)]
mod imu_frames {
    use nalgebra::{ Matrix3, UnitQuaternion, Vector3 };
    /// Which turn of the accelerometer makes it agree with the gyro: the gravity it reads, turned into the world by the
    /// gyro alone, the steadiest over the clip
    #[test]
    #[ignore]
    fn accelerometer_against_gyro() {
        let clip = std::env::var("AFP_360_CLIP").expect("AFP_360_CLIP");
        let stab = gyroflow_core::StabilizationManager::default();
        let mut file = std::fs::File::open(&clip).unwrap();
        let size = file.metadata().unwrap().len() as usize;
        stab.load_video_file(&mut file, size, &gyroflow_core::filesystem::path_to_url(&clip), None, true).unwrap();
        stab.gyro.write().integration_method = 3;
        stab.recompute_gyro();
        let gyro = stab.gyro.read();
        let md = gyro.file_metadata.read();
        let samples: Vec<(UnitQuaternion<f64>, Vector3<f64>, f64)> = gyro.raw_imu(&md).iter().filter_map(|x| {
            let a = x.accl?;
            let ts = (x.timestamp_ms * 1000.0).round() as i64;
            let (_, q) = gyro.quaternions.range(..=ts).next_back()?;
            Some((*q, Vector3::from(a), x.timestamp_ms))
        }).collect();
        let axes = [Vector3::x(), Vector3::y(), Vector3::z(), -Vector3::x(), -Vector3::y(), -Vector3::z()];
        let mut results = Vec::new();
        for a in &axes { for b in &axes {
            let c = a.cross(b);
            if c.norm() < 0.5 { continue; }
            let m = Matrix3::from_columns(&[*a, *b, c]);
            // Half-second bins of the world's specific force, their mean resultant length
            let mut bins: std::collections::BTreeMap<i64, Vector3<f64>> = Default::default();
            for (q, acc, t) in &samples { *bins.entry((t / 500.0).floor() as i64).or_insert(Vector3::zeros()) += q * (m * acc); }
            let units: Vec<Vector3<f64>> = bins.values().filter(|v| v.norm() > 0.0).map(|v| v.normalize()).collect();
            let r = (units.iter().sum::<Vector3<f64>>() / units.len() as f64).norm();
            results.push((r, format!("{a:?} {b:?}").replace("[[", "").replace("]]", "")));
        }}
        results.sort_by(|a, b| b.0.total_cmp(&a.0));
        for (r, m) in results.iter().take(6) { eprintln!("ACCM R={r:.4} x->{m}"); }
    }

    /// Per second, the gyro alone's camera (OpenCV axes into its world, a matrix row by row) and the accelerometer's
    /// direction (OpenCV axes), with no turn of either: tools/sphere360/horizon.py matches them with the horizon
    #[test]
    #[ignore]
    fn gyro_by_second() {
        let clip = std::env::var("AFP_360_CLIP").expect("AFP_360_CLIP");
        let stab = gyroflow_core::StabilizationManager::default();
        let mut file = std::fs::File::open(&clip).unwrap();
        let size = file.metadata().unwrap().len() as usize;
        stab.load_video_file(&mut file, size, &gyroflow_core::filesystem::path_to_url(&clip), None, true).unwrap();
        stab.gyro.write().integration_method = 3;
        stab.recompute_gyro();
        let gyro = stab.gyro.read();
        let f = crate::heading::flip();
        let md = gyro.file_metadata.read();
        let seconds = (gyro.duration_ms / 1000.0).floor() as i64;
        for t in 1..seconds {
            let mut s = nalgebra::Vector3::zeros();
            for x in gyro.raw_imu(&md) {
                if let Some(a) = x.accl { if (x.timestamp_ms / 1000.0 - t as f64).abs() < 0.4 { s += f * nalgebra::Vector3::from(a); } }
            }
            let s = s.normalize();
            eprintln!("ACC {t} {:.4} {:.4} {:.4}", s.x, s.y, s.z);
        }
        for t in 1..seconds {
            let q = gyro.org_quat_at_timestamp(t as f64 * 1000.0) * f;
            let m = q.to_rotation_matrix();
            let m = m.matrix();
            eprintln!("GYRO {t} {}", (0..3).flat_map(|r| (0..3).map(move |c| (r, c))).map(|(r, c)| format!("{:.6}", m[(r, c)])).collect::<Vec<_>>().join(" "));
        }
    }
}
