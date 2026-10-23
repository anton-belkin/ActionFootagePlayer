// SPDX-License-Identifier: GPL-3.0-or-later
//
// Gyroflow stabilization as a VapourSynth filter: `core.gyroflow.Stabilize(clip, project=...)`.
//
// Built for mpv's `vf=vapoursynth`, so a video with a .gyroflow project next to it, a project embedded in it,
// or motion data of its own (with a default preset) plays stabilized in realtime. Each plane of the YUV frame is stabilized on its own, at the clip's bit depth, the way
// Gyroflow's exporter does it (src/rendering/mod.rs) - no RGB conversion, so 10-bit D-Log M stays 10-bit.

use std::ffi::{ c_int, c_void, CStr, CString };
use std::ptr::{ null, null_mut };
use std::sync::{ Arc, Once, mpsc, atomic::{ AtomicBool, AtomicIsize, Ordering } };

use parking_lot::Mutex;
use vapoursynth4_sys::*;

use gyroflow_core::{ StabilizationManager, filesystem };

#[allow(dead_code)]
pub mod embedded;
pub mod probe;
pub mod project;
pub mod gpu_lut;
pub mod edges;
pub mod sphere;
use gyroflow_core::gpu::{ BufferDescription, BufferSource, Buffers };
pub use gyroflow_core::stabilization::Interpolation;
use gyroflow_core::stabilization::{ ComputeParams, Luma8, Luma16, PixelType, Stabilization };

// ---------------------------------------------------------------------------------------------------
// Logging

pub fn init_logging() {
    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        let level = match std::env::var("VS_GYROFLOW_LOG").unwrap_or_default().as_str() {
            "debug" => log::LevelFilter::Debug,
            "trace" => log::LevelFilter::Trace,
            _ => log::LevelFilter::Info,
        };
        if let Ok(file) = std::fs::OpenOptions::new().create(true).append(true).open(log_path()) {
            let config = simplelog::ConfigBuilder::new()
                // gyroflow-core is chatty at info level about things that don't matter for playback
                .add_filter_ignore_str("wgpu")
                .add_filter_ignore_str("naga")
                .build();
            let _ = simplelog::WriteLogger::init(level, config, file);
        }
    });
}

/// macOS: ~/Library/Logs; Windows: %LOCALAPPDATA%; elsewhere $XDG_STATE_HOME (~/.local/state)
pub fn log_path() -> std::path::PathBuf {
    use std::path::PathBuf;
    let env = |k: &str| std::env::var_os(k).filter(|v| !v.is_empty()).map(PathBuf::from);
    let dir = if cfg!(target_os = "macos") {
        env("HOME").map(|h| h.join("Library/Logs"))
    } else if cfg!(windows) {
        env("LOCALAPPDATA")
    } else {
        env("XDG_STATE_HOME").or_else(|| env("HOME").map(|h| h.join(".local/state")))
    };
    match dir.filter(|d| std::fs::create_dir_all(d).is_ok()) {
        Some(d) => d.join("vs-gyroflow.log"),
        None => std::env::temp_dir().join("vs-gyroflow.log"),
    }
}

/// std::fs::canonicalize, without the \\?\ Windows puts in front of a drive path: ffmpeg's messages, Gyroflow's
/// URLs and the paths printed for the player read better without it, and all of them take the plain path
pub fn canonicalize(path: impl AsRef<std::path::Path>) -> std::io::Result<std::path::PathBuf> {
    let p = std::fs::canonicalize(path)?;
    if cfg!(windows) {
        let s = p.to_string_lossy();
        if let Some(rest) = s.strip_prefix(r"\\?\") {
            if rest.as_bytes().get(1) == Some(&b':') { return Ok(std::path::PathBuf::from(rest)); }
        }
    }
    Ok(p)
}

// ---------------------------------------------------------------------------------------------------
// Engine: a loaded project and a worker thread that owns the per-plane GPU stabilizers.
//
// mpv reloads the VapourSynth script on every seek, so loading the project and setting up the GPU
// each time would stall every seek. Engines are cached for the life of the process instead, keyed by
// everything that would make them render differently.

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SampleKind { U8, U16 }

#[derive(Clone, Copy)]
pub struct PlaneJob {
    pub src: *const u8, pub src_stride: usize, pub src_w: usize, pub src_h: usize,
    pub dst: *mut u8,   pub dst_stride: usize, pub dst_w: usize, pub dst_h: usize,
}

struct Job {
    timestamp_us: i64,
    planes: Vec<PlaneJob>,
    limited_range: bool,
    edges: Option<edges::EdgeJob>,
    reply: mpsc::SyncSender<Result<(), String>>,
}
// The frame buffers stay alive until the reply arrives: get_frame blocks on it
unsafe impl Send for Job { }

/// Also used by gyroflow-export, so a video renders the same as it plays
pub struct Engine {
    key: String,
    tx: mpsc::Sender<Job>,
    pub render_size: (usize, usize),
    pub frame_offset_us: i64,
    /// What fills the parts the frame doesn't cover; off unless set
    pub edges: Arc<edges::Edges>,
}

impl Engine {
    /// `subsampling`: log2 of the chroma subsampling, (w, h)
    /// `project`: a .gyroflow project, or the video itself: its embedded project, or else its own motion data with
    /// Gyroflow's default preset and then `preset`
    /// `fov`: Gyroflow's FOV (Advanced) instead of the project's; above 1 the view is wider than the zoom covers
    pub fn load(key: String, project: &str, preset: Option<&str>, fov: Option<f64>, input_size: (usize, usize), num_planes: usize, subsampling: (usize, usize), output_override: Option<(usize, usize)>, kind: SampleKind, max_value: f32, is_yuv: bool, interpolation: Interpolation, workers: usize) -> Result<Engine, String> {
        let t = std::time::Instant::now();
        let stab = StabilizationManager::default();
        let url = filesystem::path_to_url(project);
        let project_json = if is_project_file(project) {
            stab.import_gyroflow_file(&url, true, |_| (), Arc::new(AtomicBool::new(false)), true)
                .map_err(|e| format!("Can't load {project}: {e:?}"))?;
            serde_json::from_slice(&std::fs::read(project).map_err(|e| e.to_string())?).ok()
        } else {
            let path = std::path::Path::new(project);
            let embedded = embedded::read(path).map_err(|e| format!("Can't read the project embedded in {project}: {e}"))?;
            // The camera's file, without the embedded project at its end (see embedded.rs)
            let mut file = std::fs::File::open(path).map_err(|e| format!("Can't open {project}: {e}"))?;
            let size = file.metadata().map_err(|e| e.to_string())?.len() as usize;
            let (mut content, size) = embedded::Content::new(&mut file, size);
            stab.load_video_file(&mut content, size, &url, None, true).map_err(|e| format!("Can't load {project}: {e:?}"))?;
            if let Some(obj) = embedded.as_ref() {
                let mut is_preset = false;
                let data = serde_json::to_vec(obj).map_err(|e| e.to_string())?;
                stab.import_gyroflow_data(&data, true, None, |_| (), Arc::new(AtomicBool::new(false)), &mut is_preset, true)
                    .map_err(|e| format!("Can't load the project embedded in {project}: {e:?}"))?;
            } else {
                if !stab.gyro.read().has_motion() {
                    return Err(format!("{project} has no project and no motion data"));
                }
                if let Some(preset) = preset {
                    stab.import_gyroflow_file(&filesystem::path_to_url(preset), true, |_| (), Arc::new(AtomicBool::new(false)), true)
                        .map_err(|e| format!("Can't load preset {preset}: {e:?}"))?;
                }
            }
            log::info!("{project}: {}", if embedded.is_some() { "embedded project" } else { "own motion data, default preset" });
            embedded
        };
        // Project gyro transforms (including LPF) are restored after the embedded motion data is loaded,
        // so apply them before smoothing (same as the frei0r plugin)
        stab.recompute_gyro();
        if let Some(fov) = fov {
            stab.set_fov(fov);
        }
        // In plugin mode the core leaves the output size to the host; the host here is a player, so render at
        // the project's output size (`output.output_width/height`), unless the script asks for another.
        // The core's own output_size is only the aspect, at the input's scale; the buffer sets the resolution.
        // Without a project there's no output size to keep: render at the size mpv hands over (scaled down from 8K)
        let render_size = output_override.or_else(|| match project_json.as_ref() {
            Some(json) => project_output_size(json),
            None => {
                let out = stab.params.read().output_size;
                let scale = (input_size.0 as f64 / out.0.max(1) as f64).min(input_size.1 as f64 / out.1.max(1) as f64).min(1.0);
                Some((((out.0 as f64 * scale) as usize) & !1, ((out.1 as f64 * scale) as usize) & !1))
            }
        });
        if let Some((w, h)) = render_size {
            stab.set_output_size(w, h);
        }
        stab.invalidate_smoothing();
        stab.recompute_blocking();

        let (size, output_size, frame_offset_us) = {
            let p = stab.params.read();
            (p.size, p.output_size, (p.frame_offset as f64 / p.fps * 1_000_000.0).round() as i64)
        };
        if size.0 == 0 || output_size.0 == 0 {
            return Err(format!("{project} has no video size, is it a Gyroflow project?"));
        }
        let render_size = render_size.unwrap_or(output_size);
        log::info!("Loaded {project} in {:.0} ms: {}x{} -> {}x{} (frame {}x{}), frame offset {} us", t.elapsed().as_secs_f64() * 1000.0, size.0, size.1, render_size.0, render_size.1, output_size.0, output_size.1, frame_offset_us);

        // Each frame is uploaded, rendered and read back in blocking calls, so a few workers keep the GPU
        // busy: one uploads while another renders. Each has its own GPU context.
        let plane_size = |(w, h): (usize, usize), p: usize| if p == 0 { (w, h) } else { (w >> subsampling.0, h >> subsampling.1) };
        let geometry: Vec<_> = (0..num_planes).map(|p| (plane_size(input_size, p), plane_size(render_size, p))).collect();

        let (tx, rx) = mpsc::channel::<Job>();
        let rx = Arc::new(Mutex::new(rx));
        let stab = Arc::new(stab);
        let config = WorkerConfig { kind, max_value, is_yuv, interpolation };
        let edges = Arc::new(edges::Edges::new(edges::EdgeGeometry::new(&geometry, kind, max_value)));
        // Each worker sets up its GPU context and renders one frame before playback starts: mpv waits for the
        // filter to be created, but the first frames already run against the clock, and dropped ~1 s of video
        let (ready_tx, ready_rx) = mpsc::channel::<()>();
        let workers = workers.max(1);
        for i in 0..workers {
            let (stab, rx, geometry, ready_tx, edges) = (stab.clone(), rx.clone(), geometry.clone(), ready_tx.clone(), edges.clone());
            std::thread::Builder::new().name(format!("gyroflow-vs-{i}")).spawn(move || {
                worker(&stab, &rx, config, &geometry, &edges, ready_tx);
            }).map_err(|e| e.to_string())?;
        }
        for _ in 0..workers { let _ = ready_rx.recv(); }
        log::info!("{workers} GPU workers ready after {:.0} ms", t.elapsed().as_secs_f64() * 1000.0);

        Ok(Engine { key, tx, render_size, frame_offset_us, edges })
    }

    pub fn process(&self, timestamp_us: i64, planes: Vec<PlaneJob>, limited_range: bool) -> Result<(), String> {
        let (reply, result) = mpsc::sync_channel(1);
        let edges = match self.edges.mode() {
            edges::EdgeMode::Off => None,
            mode => Some(self.edges.prepare(mode, &planes)),
        };
        self.tx.send(Job { timestamp_us, planes, limited_range, edges, reply }).map_err(|_| "Gyroflow worker is gone".to_string())?;
        result.recv().map_err(|_| "Gyroflow worker is gone".to_string())?
    }
}

fn is_project_file(path: &str) -> bool {
    path.to_ascii_lowercase().ends_with(".gyroflow")
}

fn project_output_size(json: &serde_json::Value) -> Option<(usize, usize)> {
    let output = json.get("output")?;
    let w = output.get("output_width")?.as_u64()? as usize;
    let h = output.get("output_height")?.as_u64()? as usize;
    // Even, for the subsampled chroma planes
    (w >= 16 && h >= 16).then_some((w & !1, h & !1))
}

static ENGINES: Mutex<Vec<Arc<Engine>>> = Mutex::new(Vec::new());
const MAX_ENGINES: usize = 2;

fn get_engine(key: &str, load: impl FnOnce() -> Result<Engine, String>) -> Result<Arc<Engine>, String> {
    let mut engines = ENGINES.lock();
    if let Some(i) = engines.iter().position(|e| e.key == key) {
        let e = engines.remove(i);
        engines.push(e.clone());
        return Ok(e);
    }
    let e = Arc::new(load()?);
    engines.push(e.clone());
    // Dropping an engine drops its sender, which ends its worker and frees its GPU resources
    while engines.len() > MAX_ENGINES { engines.remove(0); }
    Ok(e)
}

struct PlaneStab {
    stab: Stabilization,
    index: usize,
}

#[derive(Clone, Copy)]
struct WorkerConfig {
    kind: SampleKind,
    max_value: f32,
    is_yuv: bool,
    interpolation: Interpolation,
}

/// The project's parameters, with the background color in this plane's channel and range
fn plane_compute_params(mgr: &StabilizationManager, cfg: WorkerConfig, index: usize, limited_range: bool) -> ComputeParams {
    let mut compute_params = ComputeParams::from_manager(mgr);
    let channel = if cfg.is_yuv { [index] } else { [0] };
    compute_params.background = match cfg.kind {
        SampleKind::U8  => <Luma8  as PixelType>::from_rgb_color(compute_params.background, &channel, limited_range),
        SampleKind::U16 => <Luma16 as PixelType>::from_rgb_color(compute_params.background, &channel, limited_range),
    };
    compute_params
}

fn create_planes(mgr: &StabilizationManager, cfg: WorkerConfig, num_planes: usize, limited_range: bool) -> Vec<PlaneStab> {
    let (size, output_size, device) = {
        let p = mgr.params.read();
        (p.size, p.output_size, p.current_device as isize)
    };
    (0..num_planes).map(|index| {
        let mut stab = Stabilization::default();
        stab.interpolation = cfg.interpolation;
        // The wgpu cache is thread-local; this thread owns its stabilizers outright
        stab.share_wgpu_instances = false;
        stab.set_device(device);
        stab.init_size(size, output_size);
        stab.set_compute_params(plane_compute_params(mgr, cfg, index, limited_range));
        PlaneStab { stab, index }
    }).collect()
}

fn process_planes(planes: &mut [PlaneStab], cfg: WorkerConfig, timestamp_us: i64, jobs: &[PlaneJob]) -> Result<&'static str, String> {
    let mut backend = "";
    for (plane, pj) in planes.iter_mut().zip(jobs) {
        backend = match cfg.kind {
            SampleKind::U8  => process_plane::<Luma8>(plane, timestamp_us, cfg.max_value, pj)?,
            SampleKind::U16 => process_plane::<Luma16>(plane, timestamp_us, cfg.max_value, pj)?,
        };
    }
    Ok(backend)
}

/// `geometry`: per plane, (input size, output size)
fn worker(mgr: &StabilizationManager, rx: &Mutex<mpsc::Receiver<Job>>, cfg: WorkerConfig, geometry: &[((usize, usize), (usize, usize))], edges: &edges::Edges, ready: mpsc::Sender<()>) {
    // Video without a range property is almost always limited; rebuilt if a frame says otherwise
    let mut limited_range = cfg.is_yuv;
    let mut planes = create_planes(mgr, cfg, geometry.len(), limited_range);
    let mut edge_worker = edges::EdgeWorker::new(mgr, cfg, &edges.geometry, limited_range);
    if let Err(e) = edge_worker.warm_up(cfg) { log::error!("Edges warm-up: {e}"); }

    {
        let bytes = if cfg.kind == SampleKind::U8 { 1 } else { 2 };
        let mut buffers: Vec<(Vec<u8>, Vec<u8>)> = geometry.iter().map(|((w, h), (ow, oh))| (vec![0u8; w * h * bytes], vec![0u8; ow * oh * bytes])).collect();
        let jobs: Vec<PlaneJob> = geometry.iter().zip(buffers.iter_mut()).map(|(((w, h), (ow, oh)), (src, dst))| PlaneJob {
            src: src.as_ptr(), src_stride: w * bytes, src_w: *w, src_h: *h,
            dst: dst.as_mut_ptr(), dst_stride: ow * bytes, dst_w: *ow, dst_h: *oh,
        }).collect();
        match process_planes(&mut planes, cfg, 0, &jobs) {
            Ok(backend) => log::info!("Processing on {backend}, {:?}, max value {}", cfg.kind, cfg.max_value),
            Err(e) => log::error!("Warm-up: {e}"),
        }
    }
    let _ = ready.send(());

    let (mut frames, mut busy) = (0u32, std::time::Duration::ZERO);
    loop {
        // Hold the lock only while waiting, so the other worker takes the next frame
        let Ok(job) = rx.lock().recv() else { break };
        let t = std::time::Instant::now();
        if job.limited_range != limited_range {
            // Only the background color depends on the range; the GPU setup stays
            limited_range = job.limited_range;
            for plane in planes.iter_mut() {
                plane.stab.set_compute_params(plane_compute_params(mgr, cfg, plane.index, limited_range));
            }
        }
        let mut result = process_planes(&mut planes, cfg, job.timestamp_us, &job.planes).map(|_| ());
        if let (Ok(()), Some(e)) = (&result, &job.edges) {
            result = edge_worker.apply(edges, cfg, e, job.timestamp_us, &job.planes);
        }
        let _ = job.reply.send(result);

        frames += 1;
        busy += t.elapsed();
        if frames == 300 {
            log::info!("{:.1} ms per frame on the GPU worker", busy.as_secs_f64() * 1000.0 / frames as f64);
            (frames, busy) = (0, std::time::Duration::ZERO);
        }
    }
    log::debug!("Worker finished");
}

fn process_plane<T: PixelType>(plane: &mut PlaneStab, timestamp_us: i64, max_value: f32, pj: &PlaneJob) -> Result<&'static str, String> {
    // gyroflow-core takes mutable slices for both sides, but only writes the output
    let src = unsafe { std::slice::from_raw_parts_mut(pj.src as *mut u8, pj.src_stride * pj.src_h) };
    let dst = unsafe { std::slice::from_raw_parts_mut(pj.dst, pj.dst_stride * pj.dst_h) };
    let mut buffers = Buffers {
        input:  BufferDescription { size: (pj.src_w, pj.src_h, pj.src_stride), data: BufferSource::Cpu { buffer: src }, ..Default::default() },
        output: BufferDescription { size: (pj.dst_w, pj.dst_h, pj.dst_stride), data: BufferSource::Cpu { buffer: dst }, ..Default::default() },
    };

    let stab = &mut plane.stab;
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
    transform.kernel_params.plane_index = plane.index as i32;

    stab.process_pixels::<T>(timestamp_us, None, &mut buffers, Some(&transform))
        .map(|info| info.backend)
        .map_err(|e| format!("process_pixels: {e:?}"))
}

// ---------------------------------------------------------------------------------------------------
// Frame timestamps
//
// mpv's source clip restarts at frame 0 after every seek, so the frame number alone doesn't say where
// in the video a frame is. mpv doesn't set _AbsoluteTime, but it attaches its own `struct mp_image`
// as the binary _MP_IMAGE property, and that has the pts. Rather than hardcode the struct layout of one
// mpv version, the pts field is found from two neighbouring frames: the first double that advances by
// exactly the frame duration. The offset is shared by all instances (it's the same mpv build), so this
// happens once per process, on its first frame.

const PTS_UNKNOWN: isize = -1;
const PTS_UNAVAILABLE: isize = -2;
static MP_PTS_OFFSET: AtomicIsize = AtomicIsize::new(PTS_UNKNOWN);

fn read_f64(blob: &[u8], off: usize) -> f64 {
    f64::from_ne_bytes(blob[off..off + 8].try_into().unwrap())
}

fn find_pts_offset(a: &[u8], b: &[u8], expected_step: f64) -> Option<usize> {
    if a.len() != b.len() || a.len() < 16 { return None; }
    (0..=a.len() - 8).step_by(8).find(|&off| {
        let (x, y) = (read_f64(a, off), read_f64(b, off));
        x.is_finite() && y.is_finite() && x >= 0.0 && x < 1.0e6 && ((y - x) - expected_step).abs() < 1.0e-4
    })
}

unsafe fn mp_image(api: &VSAPI, props: *const VSMap) -> Option<&'static [u8]> {
    let mut err = VSMapPropertyError::Success;
    let ptr = unsafe { (api.mapGetData)(props, c"_MP_IMAGE".as_ptr(), 0, &mut err) };
    if err != VSMapPropertyError::Success || ptr.is_null() { return None; }
    let size = unsafe { (api.mapGetDataSize)(props, c"_MP_IMAGE".as_ptr(), 0, &mut err) };
    Some(unsafe { std::slice::from_raw_parts(ptr as *const u8, size.max(0) as usize) })
}

// ---------------------------------------------------------------------------------------------------
// The filter

struct Filter {
    node: *mut VSNode,
    engine: Arc<Engine>,
    out_size: (c_int, c_int),
    num_planes: usize,
    is_yuv: bool,
    start_time: f64,
    time_offset: f64,
    fallback_duration: f64,
}

impl Filter {
    unsafe fn frame_duration(&self, api: &VSAPI, props: *const VSMap) -> f64 {
        let mut e1 = VSMapPropertyError::Success;
        let mut e2 = VSMapPropertyError::Success;
        let num = unsafe { (api.mapGetInt)(props, c"_DurationNum".as_ptr(), 0, &mut e1) };
        let den = unsafe { (api.mapGetInt)(props, c"_DurationDen".as_ptr(), 0, &mut e2) };
        if e1 == VSMapPropertyError::Success && e2 == VSMapPropertyError::Success && num > 0 && den > 0 {
            num as f64 / den as f64
        } else {
            self.fallback_duration
        }
    }

    /// Finds the pts in _MP_IMAGE from frame n and n + 1
    unsafe fn calibrate(&self, api: &VSAPI, props: *const VSMap, next_props: *const VSMap) {
        let found = match (unsafe { mp_image(api, props) }, unsafe { mp_image(api, next_props) }) {
            (Some(a), Some(b)) => find_pts_offset(a, b, unsafe { self.frame_duration(api, props) }),
            _ => None,
        };
        match found {
            Some(off) => log::info!("Found the pts in mpv's _MP_IMAGE at offset {off}"),
            None => log::warn!("No pts in the frame properties; timing by frame number, which is only right without seeking"),
        }
        let _ = MP_PTS_OFFSET.compare_exchange(PTS_UNKNOWN, found.map(|x| x as isize).unwrap_or(PTS_UNAVAILABLE), Ordering::Relaxed, Ordering::Relaxed);
    }

    /// Presentation time of frame `n`, in seconds
    unsafe fn frame_time(&self, n: c_int, api: &VSAPI, props: *const VSMap) -> f64 {
        let mut err = VSMapPropertyError::Success;
        let t = unsafe { (api.mapGetFloat)(props, c"_AbsoluteTime".as_ptr(), 0, &mut err) };
        if err == VSMapPropertyError::Success && t.is_finite() { return t; }

        let off = MP_PTS_OFFSET.load(Ordering::Relaxed);
        if off >= 0 {
            if let Some(blob) = unsafe { mp_image(api, props) }.filter(|b| off as usize + 8 <= b.len()) {
                let pts = read_f64(blob, off as usize);
                if pts.is_finite() && pts > -1.0e3 { return pts; }
            }
        }
        n as f64 * unsafe { self.frame_duration(api, props) }
    }
}

unsafe extern "system-unwind" fn filter_get_frame(n: c_int, reason: VSActivationReason, instance: *mut c_void, frame_data: *mut *mut c_void, ctx: *mut VSFrameContext, core: *mut VSCore, vsapi: *const VSAPI) -> *const VSFrame {
    let f = unsafe { &*(instance as *const Filter) };
    let api = unsafe { &*vsapi };
    match reason {
        VSActivationReason::Initial => unsafe {
            (api.requestFrameFilter)(n, f.node, ctx);
            if MP_PTS_OFFSET.load(Ordering::Relaxed) == PTS_UNKNOWN {
                // Remember that this request also has the next frame, for `calibrate`
                (api.requestFrameFilter)(n + 1, f.node, ctx);
                *frame_data = 1 as *mut c_void;
            }
            null()
        },
        VSActivationReason::AllFramesReady => unsafe {
            let src = (api.getFrameFilter)(n, f.node, ctx);
            let props = (api.getFramePropertiesRO)(src);

            if !(*frame_data).is_null() {
                let next = (api.getFrameFilter)(n + 1, f.node, ctx);
                f.calibrate(api, props, (api.getFramePropertiesRO)(next));
                (api.freeFrame)(next);
            }

            let time = f.frame_time(n, api, props);
            let timestamp_us = ((time - f.start_time + f.time_offset) * 1_000_000.0).round() as i64 + f.engine.frame_offset_us;

            let mut err = VSMapPropertyError::Success;
            let range = (api.mapGetInt)(props, c"_ColorRange".as_ptr(), 0, &mut err);
            // VapourSynth: 0 = full, 1 = limited. Video without the property is almost always limited
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
                    log::error!("Frame {n} at {timestamp_us} us: {e}");
                    (api.freeFrame)(dst);
                    let msg = CString::new(format!("Gyroflow: {e}")).unwrap_or_default();
                    (api.setFilterError)(msg.as_ptr(), ctx);
                    null()
                }
            }
        },
        _ => null()
    }
}

unsafe extern "system-unwind" fn filter_free(instance: *mut c_void, _core: *mut VSCore, vsapi: *const VSAPI) {
    let f = unsafe { Box::from_raw(instance as *mut Filter) };
    unsafe { ((*vsapi).freeNode)(f.node) };
}

unsafe fn map_get_str(api: &VSAPI, map: *const VSMap, key: &CStr) -> Option<String> {
    let mut err = VSMapPropertyError::Success;
    let ptr = unsafe { (api.mapGetData)(map, key.as_ptr(), 0, &mut err) };
    if err != VSMapPropertyError::Success || ptr.is_null() { return None; }
    Some(unsafe { CStr::from_ptr(ptr) }.to_string_lossy().into_owned())
}
unsafe fn map_get_f64(api: &VSAPI, map: *const VSMap, key: &CStr) -> Option<f64> {
    let mut err = VSMapPropertyError::Success;
    let v = unsafe { (api.mapGetFloat)(map, key.as_ptr(), 0, &mut err) };
    (err == VSMapPropertyError::Success).then_some(v)
}
unsafe fn map_get_i64(api: &VSAPI, map: *const VSMap, key: &CStr) -> Option<i64> {
    let mut err = VSMapPropertyError::Success;
    let v = unsafe { (api.mapGetInt)(map, key.as_ptr(), 0, &mut err) };
    (err == VSMapPropertyError::Success).then_some(v)
}

unsafe extern "system-unwind" fn stabilize_create(input: *const VSMap, out: *mut VSMap, _user_data: *mut c_void, core: *mut VSCore, vsapi: *const VSAPI) {
    init_logging();
    let api = unsafe { &*vsapi };
    let fail = |node: *mut VSNode, msg: String| {
        log::error!("{msg}");
        let msg = CString::new(format!("Stabilize: {msg}")).unwrap_or_default();
        unsafe {
            (api.mapSetError)(out, msg.as_ptr());
            if !node.is_null() { (api.freeNode)(node); }
        }
    };

    let mut err = VSMapPropertyError::Success;
    let node = unsafe { (api.mapGetNode)(input, c"clip".as_ptr(), 0, &mut err) };
    let vi = unsafe { (*(api.getVideoInfo)(node)).clone() };
    let fmt = vi.format.clone();

    let is_yuv = fmt.color_family == VSColorFamily::YUV;
    if !(is_yuv || fmt.color_family == VSColorFamily::Gray) || fmt.sample_type != VSSampleType::Integer || fmt.bytes_per_sample > 2 || vi.width == 0 {
        return fail(node, "only constant-size YUV or Gray clips with 8-16 bit integer samples are supported".into());
    }
    let kind = if fmt.bytes_per_sample == 1 { SampleKind::U8 } else { SampleKind::U16 };
    let max_value = ((1u32 << fmt.bits_per_sample) - 1) as f32;

    let Some(project) = (unsafe { map_get_str(api, input, c"project") }) else {
        return fail(node, "project is required".into());
    };
    let start_time  = unsafe { map_get_f64(api, input, c"start_time") }.unwrap_or(0.0);
    let time_offset = unsafe { map_get_f64(api, input, c"time_offset") }.unwrap_or(0.0);
    let interpolation_name = unsafe { map_get_str(api, input, c"interpolation") }.unwrap_or_else(|| "bicubic".into());
    let interpolation = match interpolation_name.to_ascii_lowercase().as_str() {
        "bilinear" => Interpolation::Bilinear,
        "bicubic"  => Interpolation::Bicubic,
        "lanczos" | "lanczos4" => Interpolation::Lanczos4,
        other => return fail(node, format!("unknown interpolation {other} (bilinear, bicubic, lanczos)")),
    };
    let workers = unsafe { map_get_i64(api, input, c"workers") }.unwrap_or(3).clamp(1, 6) as usize;
    // Chroma planes need the size to be a multiple of the subsampling
    let align = |v: i64, ss: c_int| (v as usize) & !((1usize << ss) - 1);
    let output_override = match (unsafe { map_get_i64(api, input, c"width") }, unsafe { map_get_i64(api, input, c"height") }) {
        (Some(w), Some(h)) if w > 0 && h > 0 => Some((align(w, fmt.sub_sampling_w), align(h, fmt.sub_sampling_h))),
        (None, None) => None,
        _ => return fail(node, "width and height go together".into()),
    };

    let preset = unsafe { map_get_str(api, input, c"preset") }.filter(|p| !p.is_empty());
    let fov = unsafe { map_get_f64(api, input, c"fov") }.filter(|v| *v > 0.0).map(|v| v.clamp(0.1, 4.0));
    // Saving a project into a video keeps the video's modification time, so the size tells the versions apart
    let version = |path: &str| std::fs::metadata(path).map(|m| format!("{:?}/{}", m.modified().ok(), m.len())).unwrap_or_default();
    let mtime = format!("{}|{preset:?}|{}", version(&project), preset.as_deref().map(version).unwrap_or_default());
    let key = format!("{project}|{mtime}|{}x{}|{:?}{}|{output_override:?}|{interpolation_name}|{workers}|{fov:?}|{}", vi.width, vi.height, fmt.color_family, fmt.bits_per_sample, fmt.sub_sampling_w * 10 + fmt.sub_sampling_h);
    let engine = match get_engine(&key, || Engine::load(key.clone(), &project, preset.as_deref(), fov, (vi.width as usize, vi.height as usize), fmt.num_planes as usize, (fmt.sub_sampling_w as usize, fmt.sub_sampling_h as usize), output_override, kind, max_value, is_yuv, interpolation, workers)) {
        Ok(e) => e,
        Err(e) => return fail(node, e),
    };

    let edge_mode = unsafe { map_get_str(api, input, c"edges") }.unwrap_or_default();
    let Some(edge_mode) = edges::EdgeMode::parse(&edge_mode) else {
        return fail(node, format!("unknown edges {edge_mode} (off, original, zoom, trail, sphere)"));
    };
    let mut edge_settings = edges::EdgeSettings::default();
    if let Some(v) = unsafe { map_get_f64(api, input, c"edges_blur") } { edge_settings.blur = (v as f32 / 100.0).clamp(0.0, 0.2); }
    if let Some(v) = unsafe { map_get_f64(api, input, c"edges_zoom") } { edge_settings.zoom = (v as f32).clamp(1.0, 4.0); }
    if let Some(v) = unsafe { map_get_i64(api, input, c"edges_expected") } { edge_settings.expected = v != 0; }
    let edge_control = unsafe { map_get_str(api, input, c"edges_control") }.filter(|p| !p.is_empty()).map(std::path::PathBuf::from);
    // Also starts the trail over: mpv reloads the script on every seek
    engine.edges.configure(edge_mode, edge_settings, edge_control);

    let mut out_vi = vi.clone();
    out_vi.width  = align(engine.render_size.0 as i64, fmt.sub_sampling_w) as c_int;
    out_vi.height = align(engine.render_size.1 as i64, fmt.sub_sampling_h) as c_int;

    let fallback_duration = if vi.fps_num > 0 && vi.fps_den > 0 { vi.fps_den as f64 / vi.fps_num as f64 } else { 1.0 / 30.0 };
    let filter = Box::new(Filter {
        node, engine, out_size: (out_vi.width, out_vi.height), num_planes: fmt.num_planes as usize, is_yuv,
        start_time, time_offset, fallback_duration,
    });
    let deps = [VSFilterDependency { source: node, request_pattern: VSRequestPattern::StrictSpatial }];
    unsafe {
        (api.createVideoFilter)(out, c"Stabilize".as_ptr(), &out_vi, filter_get_frame, Some(filter_free), VSFilterMode::Parallel,
                                deps.as_ptr(), deps.len() as c_int, Box::into_raw(filter) as *mut c_void, core);
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "system-unwind" fn VapourSynthPluginInit2(plugin: *mut VSPlugin, vspapi: *const VSPLUGINAPI) {
    let api = unsafe { &*vspapi };
    unsafe {
        (api.configPlugin)(c"xyz.gyroflow.vapoursynth".as_ptr(), c"gyroflow".as_ptr(), c"Gyroflow video stabilization".as_ptr(),
                           vs_make_version(0, 1), vs_make_version(4, 0), 0, plugin);
        (api.registerFunction)(c"Stabilize".as_ptr(),
                               c"clip:vnode;project:data;preset:data:opt;start_time:float:opt;time_offset:float:opt;width:int:opt;height:int:opt;interpolation:data:opt;workers:int:opt;fov:float:opt;edges:data:opt;edges_blur:float:opt;edges_zoom:float:opt;edges_expected:int:opt;edges_control:data:opt;".as_ptr(),
                               c"clip:vnode;".as_ptr(), stabilize_create, null_mut(), plugin);
    }
}
