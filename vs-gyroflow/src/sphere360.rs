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
/// The sphere's width, its height half of it: an X5's lens has ~19 px per degree over its field, 6912 across 360°
pub const DEFAULT_WIDTH: usize = 7680;

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

/// How the motion data's camera (gyroflow-core's, after its IMU orientation) turns into the rig's axes, where they
/// differ: q_rig_from_imu-camera, OpenCV axes.
///
/// Insta360 X4 and X5 (IMU orientation `yzX`): telemetry-parser's orientation assumes the IMU already turned into lens
/// 0's frame, which it does only for a clip with a crop window (a flat one, `insert_lens_profile`); a 360 file's isn't.
/// That turn is the vendor's lens 0 pose, a quarter turn about the lens's axis less its calibration residual, and the
/// rig's axes are that pose less the same residual (rig.rs `rotation`), so what's left is the quarter turn exactly. The
/// sample X5 clip: up read along +x before it, along -y (the camera upright) after
fn imu_turn(rig: &Rig, imu_orientation: Option<&str>) -> Option<nalgebra::UnitQuaternion<f64>> {
    match (rig.brand.as_str(), imu_orientation) {
        ("Insta360", Some("yzX")) => Some(nalgebra::UnitQuaternion::from_axis_angle(&Vector3::y_axis(), std::f64::consts::PI)
            * nalgebra::UnitQuaternion::from_axis_angle(&Vector3::z_axis(), -std::f64::consts::FRAC_PI_2)),
        _ => None,
    }
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
        let q = self.world.world_from_cam(&self.stab.gyro.read().org_quat_at_timestamp(timestamp_us as f64 / 1000.0));
        let frame = (((timestamp_us - self.frame_offset_us) as f64) * self.fps / 1e6).round() as i64;
        Stamp { time_us: timestamp_us, q_world_from_cam: [q.w, q.i, q.j, q.k], headings: self.headings.at(frame) }
    }
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
        if let Some(turn) = imu_turn(&rig, imu_orientation.as_deref()) {
            stab.gyro.write().imu_transforms.imu_rotation = Some((flip() * turn * flip()).to_rotation_matrix());
            stab.recompute_gyro();
        }
        stab.set_smoothing_method(0);
        stab.recompute_blocking();
        let (fps, frame_count, frame_offset_us) = {
            let p = stab.params.read();
            (p.fps, p.frame_count, (p.frame_offset as f64 / p.fps * 1_000_000.0).round() as i64)
        };

        // The world: gyroflow-core's own leveled to the accelerometer's mean (heading.rs)
        let up = {
            let gyro = stab.gyro.read();
            let md = gyro.file_metadata.read();
            let mut sum = Vector3::zeros();
            for x in gyro.raw_imu(&md) {
                let (Some(a), ts) = (x.accl, (x.timestamp_ms * 1000.0).round() as i64) else { continue };
                if let Some((_, q)) = gyro.quaternions.range(..=ts).next_back() { sum += q * Vector3::from(a); }
            }
            sum
        };
        if up.norm() < 1e-9 { return Err(format!("{video}: no accelerometer to level the horizon with")); }
        {
            // Where the accelerometer says up, in the rig's axes: -y for a camera held upright, as most are
            let gyro = stab.gyro.read();
            let md = gyro.file_metadata.read();
            let mean = gyro.raw_imu(&md).iter().filter_map(|x| x.accl).fold(Vector3::zeros(), |s, a| s + flip() * Vector3::from(a));
            let tilt = mean.angle(&-Vector3::y()).to_degrees();
            log::info!("{video}: up in the rig's axes on average {:.3?}, {tilt:.0}° from the rig's own (IMU orientation {imu_orientation:?})", mean.normalize().as_slice());
        }
        let world = World::new(up);
        let headings = {
            let gyro = stab.gyro.read();
            let yaws: Vec<(f64, f64)> = (0..frame_count).map(|i| {
                let us = (i as f64 * 1e6 / fps).round() as i64 + frame_offset_us;
                yaw(&world.world_from_cam(&gyro.org_quat_at_timestamp(us as f64 / 1000.0)))
            }).collect();
            Headings::new(&yaws, fps)
        };

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
        let stab = Arc::new(stab);
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
        Ok(Sphere360 { tx, shared, frame_offset_us, throughput: Mutex::new((0, std::time::Instant::now())) })
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

    /// The lens's part of `src` (the joined plane, `rect` in it) into `dst`
    fn render<T: PixelType>(&mut self, shared: &Shared, timestamp_us: i64, src: (*const u8, usize, usize, usize), rect: (usize, usize, usize, usize), dst: (*mut u8, usize, usize, usize)) -> Result<(), String> {
        let max_value = ((1u32 << shared.cfg.depth) - 1) as f32;
        let (sp, sstride, sw, sh) = src;
        let (dp, dstride, dw, dh) = dst;
        // gyroflow-core takes mutable slices for both sides, but only writes the output
        let s = unsafe { std::slice::from_raw_parts_mut(sp as *mut u8, sstride * sh) };
        let d = unsafe { std::slice::from_raw_parts_mut(dp, dstride * dh) };
        let mut buffers = Buffers {
            input: BufferDescription { size: (sw, sh, sstride), rect: Some(rect), data: BufferSource::Cpu { buffer: s }, ..Default::default() },
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
        if let Err(e) = frame(shared, &mut gpu, &mut scratch, 0, &planes) { log::error!("360 warm-up: {e}"); }
    }
    let _ = ready.send(());

    let (mut frames, mut busy) = (0u32, std::time::Duration::ZERO);
    loop {
        let Ok(job) = rx.lock().recv() else { break };
        let t = std::time::Instant::now();
        if job.limited_range != limited_range {
            limited_range = job.limited_range;
            for lp in gpu.iter_mut().flatten() { lp.set_range(shared, limited_range); }
        }
        let result = frame(shared, &mut gpu, &mut scratch, job.timestamp_us, &job.planes);
        let _ = job.reply.send(result);
        frames += 1;
        busy += t.elapsed();
        if frames == 300 {
            log::info!("360: {:.1} ms per frame on the GPU worker", busy.as_secs_f64() * 1000.0 / frames as f64);
            (frames, busy) = (0, std::time::Duration::ZERO);
        }
    }
}

/// One frame: every lens of every plane, the blend, the stamp
fn frame(shared: &Shared, gpu: &mut [Vec<LensPlane>], scratch: &mut [Vec<Vec<u8>>], timestamp_us: i64, planes: &[PlaneJob]) -> Result<(), String> {
    let cfg = shared.cfg;
    for (p, pj) in planes.iter().enumerate() {
        let (ss, sv) = if p == 0 { (0, 0) } else { cfg.subsampling };
        let (w, h) = (shared.sphere.0 >> ss, shared.sphere.1 >> sv);
        if pj.dst_w < w || pj.dst_h < h { return Err(format!("plane {p}: the output {}x{} is smaller than the sphere {w}x{h}", pj.dst_w, pj.dst_h)); }
        for (l, lens_planes) in gpu.iter_mut().enumerate() {
            let r = shared.rig.source_rect(l);
            let rect = (r.0 >> ss, r.1 >> sv, r.2 >> ss, r.3 >> sv);
            let src = (pj.src, pj.src_stride, pj.src_w, pj.src_h);
            let bytes = if cfg.kind == SampleKind::U8 { 1 } else { 2 };
            let dst = if l == 0 { (pj.dst, pj.dst_stride, w, h) } else { (scratch[l][p].as_mut_ptr(), w * bytes, w, h) };
            let lp = &mut lens_planes[p];
            match cfg.kind {
                SampleKind::U8 => lp.render::<Luma8>(shared, timestamp_us, src, rect, dst)?,
                SampleKind::U16 => lp.render::<Luma16>(shared, timestamp_us, src, rect, dst)?,
            }
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
// The filter: `core.gyroflow.Sphere360(clip, video, width=7680, distance=0, interpolation="bicubic", workers=3,
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
    let interpolation = match unsafe { crate::map_get_str(api, input, c"interpolation") }.unwrap_or_else(|| "bicubic".into()).to_ascii_lowercase().as_str() {
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
        let engine = Sphere360::load(&clip, (jw, jh), 3, (SampleKind::U8, 8, true, (1, 1)), Interpolation::Bicubic, width, distance, 1).unwrap();
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
