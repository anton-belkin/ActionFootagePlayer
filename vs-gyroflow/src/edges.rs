// SPDX-License-Identifier: GPL-3.0-or-later
//
// What shows where the stabilized frame doesn't cover the output, when the project's zoom isn't enough:
//
//   off       the project's background color (as Gyroflow renders it)
//   original  the unstabilized frame behind, blurred
//   zoom      the stabilized frame scaled up behind (+50 % by default), blurred
//   trail     what was last drawn there in earlier frames, blurred; where nothing was yet, the zoom fill
//   sphere    what was last seen in that direction of the world (sphere.rs), blurred; where it wasn't seen yet,
//             the trail. It's written from a view wider than the output, so it has what the output crops away too
//
// Which output pixels the video covers comes from Gyroflow's own kernel: each frame it renders a white plane
// with a feathered border onto black, with the frame's transform, at 1/4 size. That's the same geometry as the
// picture (lens, rolling shutter, refraction), and the feather blends the picture into the fill instead of
// a hard seam. The fills are blurred anyway, so they're made on a coarse grid (~240x135): the stabilized
// frame at that size is rendered on the GPU too, from the source scaled down here. Only the blocks of the
// frame the mask says are uncovered are touched at full size.
//
// The mode and the blur can change while playing: mpv's script writes them to a control file, read here at
// most every 50 ms, so switching doesn't rebuild mpv's filter chain.
//
// In playback the sphere is written on a thread of its own (sphere_writer), from the newest frame it can take, and
// only while a GPU worker is free: the frames come first, and if they take all of the GPU the sphere is written less
// often, or not at all, and what it has fades into the screen trail. The frames read it every frame, by their own
// directions, so the fill stays where it belongs however old what's in it is. An export writes it with each frame.

use std::path::PathBuf;
use std::sync::{ Arc, OnceLock };
use std::sync::atomic::{ AtomicBool, AtomicU8, AtomicUsize, Ordering };
use std::time::{ Duration, Instant, SystemTime };

use parking_lot::{ Condvar, Mutex };
use gyroflow_core::StabilizationManager;
use gyroflow_core::stabilization::{ ComputeParams, Interpolation, Luma8, Luma16, PixelType, Stabilization };
use gyroflow_core::stabilization_params::BackgroundMode;

use crate::{ PlaneJob, PlaneStab, SampleKind, WorkerConfig, plane_compute_params, process_plane };
use crate::sphere::{ GridFrame, Sphere, View, WIDE, wide_params };

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum EdgeMode { Off = 0, Original = 1, Zoom = 2, Trail = 3, Sphere = 4 }

impl EdgeMode {
    pub fn parse(s: &str) -> Option<EdgeMode> {
        match s.trim().to_ascii_lowercase().as_str() {
            "off" | "background" | "" => Some(EdgeMode::Off),
            "original" => Some(EdgeMode::Original),
            "zoom" => Some(EdgeMode::Zoom),
            "trail" => Some(EdgeMode::Trail),
            "sphere" => Some(EdgeMode::Sphere),
            _ => None,
        }
    }
    fn from_u8(v: u8) -> EdgeMode {
        match v { 1 => EdgeMode::Original, 2 => EdgeMode::Zoom, 3 => EdgeMode::Trail, 4 => EdgeMode::Sphere, _ => EdgeMode::Off }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct EdgeSettings {
    /// Blur radius (sigma), as a fraction of the frame height
    pub blur: f32,
    /// How much the zoom fill scales the stabilized frame up
    pub zoom: f32,
}
impl Default for EdgeSettings {
    fn default() -> Self { EdgeSettings { blur: 0.02, zoom: 1.5 } }
}

// The mask's feather, as a fraction of the source height, and how far in it starts: the interpolation's taps
// past the picture's edge pull in the background, a few pixels in
const FEATHER: f32 = 0.03;
const INSET: f32 = 0.002;
// Older trail fades into the zoom fill, so stale content doesn't stay forever
const TRAIL_HALF_LIFE_S: f32 = 3.0;
// On the sphere content is where it belongs, so it can stay longer
const SPHERE_HALF_LIFE_S: f32 = 5.0;
// A jump in time this big is a seek: the trail starts over
const TRAIL_MAX_GAP_US: i64 = 300_000;
// Mean difference (of the range) between the frame and the trail where both are, above which it's a cut
const SCENE_CUT: f32 = 0.12;

/// Sizes, all luma-based: every buffer here covers the whole frame, so planes are sampled by normalized position
pub struct EdgeGeometry {
    pub kind: SampleKind,
    pub max_value: f32,
    /// Per plane: (input size, output size)
    pub planes: Vec<((usize, usize), (usize, usize))>,
    pub mask_src: (usize, usize),
    pub mask: (usize, usize),
    pub grid_src: (usize, usize),
    pub grid: (usize, usize),
    /// The sphere trail's wide view (sphere::wide_params), as fine as `grid` would be unzoomed
    pub wide: (usize, usize),
}

impl EdgeGeometry {
    pub fn new(planes: &[((usize, usize), (usize, usize))], kind: SampleKind, max_value: f32) -> EdgeGeometry {
        let ((sw, sh), (ow, oh)) = planes[0];
        let div = |(w, h): (usize, usize), f: usize| (w.div_ceil(f).max(2), h.div_ceil(f).max(2));
        // ~135 rows whatever the size: 4K is 16 pixels per cell
        let f = (oh / 135).max(4);
        let grid = div((ow, oh), f);
        EdgeGeometry {
            kind, max_value, planes: planes.to_vec(),
            mask_src: div((sw, sh), 4), mask: div((ow, oh), 4),
            grid_src: div((sw, sh), f), grid,
            wide: ((grid.0 as f64 * WIDE).round() as usize, (grid.1 as f64 * WIDE).round() as usize),
        }
    }
}

struct Control {
    path: Option<PathBuf>,
    modified: Option<SystemTime>,
    checked: Option<Instant>,
}

struct Trail {
    last_us: Option<i64>,
    weight: Vec<f32>,
    planes: Vec<Vec<f32>>,
}

/// The newest frame for the sphere writer, and whether it should stop
struct Feed {
    frame: Mutex<Option<(i64, Arc<Vec<Vec<f32>>>)>>,
    ready: Condvar,
    stop: AtomicBool,
}

/// Per engine: the mode, its settings, and the trail (shared by the workers, frames come in any order)
pub struct Edges {
    pub geometry: EdgeGeometry,
    mode: AtomicU8,
    control: Mutex<Control>,
    settings: Mutex<EdgeSettings>,
    trail: Mutex<Trail>,
    /// Made on the first frame written, as fine as the grid is then; read without a lock (sphere.rs)
    sphere: OnceLock<Sphere>,
    /// The time of the frame last written to the sphere; held while writing, so writers take turns
    sphere_written: Mutex<Option<i64>>,
    /// Whether the clip has frames the stabilized frame doesn't cover (Engine's `uncovered_frames`): the sphere trail is
    /// written only then, and every frame, to have what the camera saw when they show
    expected: AtomicBool,
    /// The view the frames are rendered in (VIEW_NORMAL, VIEW_WIDE: lib.rs's Views), as the control file says
    view: AtomicU8,
    /// Playback: the sphere is written by sphere_writer, when the GPU workers leave room for it
    realtime: AtomicBool,
    writer: AtomicBool,
    feed: Feed,
    /// Frames sent to the GPU workers and not back yet (Engine::process), and how many workers there are
    pub(crate) in_flight: AtomicUsize,
    workers: usize,
}

pub struct EdgeJob {
    mode: EdgeMode,
    settings: EdgeSettings,
    /// The source planes on the coarse grid; shared with the sphere writer
    src_lo: Arc<Vec<Vec<f32>>>,
}

impl Edges {
    pub fn new(geometry: EdgeGeometry, workers: usize) -> Edges {
        let n = geometry.grid.0 * geometry.grid.1;
        let trail = Trail { last_us: None, weight: vec![0.0; n], planes: vec![vec![0.0; n]; geometry.planes.len()] };
        Edges {
            geometry, mode: AtomicU8::new(EdgeMode::Off as u8),
            control: Mutex::new(Control { path: None, modified: None, checked: None }),
            settings: Mutex::new(EdgeSettings::default()), trail: Mutex::new(trail),
            sphere: OnceLock::new(), sphere_written: Mutex::new(None),
            expected: AtomicBool::new(true), view: AtomicU8::new(VIEW_NORMAL),
            realtime: AtomicBool::new(false), writer: AtomicBool::new(false),
            feed: Feed { frame: Mutex::new(None), ready: Condvar::new(), stop: AtomicBool::new(false) },
            in_flight: AtomicUsize::new(0), workers: workers.max(1),
        }
    }

    /// Whether some frame of the clip leaves edges (`set_expected`)
    pub fn expected(&self) -> bool {
        self.expected.load(Ordering::Relaxed)
    }

    pub fn set_expected(&self, expected: bool) {
        self.expected.store(expected, Ordering::Relaxed);
    }

    /// Playback (the VapourSynth filter): the sphere written on its own thread, as the GPU has room. Off, as in an
    /// export, it's written with each frame
    pub fn set_realtime(&self, realtime: bool) {
        self.realtime.store(realtime, Ordering::Relaxed);
    }

    /// Ends the sphere writer (Engine's drop)
    pub(crate) fn stop(&self) {
        let _frame = self.feed.frame.lock();
        self.feed.stop.store(true, Ordering::Relaxed);
        self.feed.ready.notify_all();
    }

    /// The frame for the sphere writer, in place of one it hasn't taken yet: only the newest counts. Not an older one
    /// finished after it, unless it's a seek back
    fn post(&self, timestamp_us: i64, src_lo: &Arc<Vec<Vec<f32>>>) {
        let mut frame = self.feed.frame.lock();
        if frame.as_ref().is_some_and(|(t, _)| timestamp_us < *t && *t - timestamp_us < TRAIL_MAX_GAP_US) { return; }
        *frame = Some((timestamp_us, src_lo.clone()));
        self.feed.ready.notify_one();
    }

    /// The wide view's frame onto the sphere; starts it over at a seek, or where it no longer matches the frame (a cut,
    /// or a jump the gyro didn't see)
    fn write_sphere(&self, frame: &GridFrame, timestamp_us: i64) {
        let mut last = self.sphere_written.lock();
        let sphere = self.sphere.get_or_init(|| {
            let s = Sphere::new(frame.view.cells_per_radian(frame.grid.0), frame.planes.len(), SPHERE_HALF_LIFE_S);
            log::debug!("Edges: sphere of 6 x {0}x{0}", s.n);
            s
        });
        let now = timestamp_us as f32 / 1e6;
        match *last {
            None => sphere.reset(now),
            Some(l) if (timestamp_us - l).abs() > TRAIL_MAX_GAP_US => sphere.reset(now),
            // Another worker already wrote a newer frame
            Some(l) if timestamp_us <= l => return,
            Some(_) => {
                if sphere.mismatch(frame, now).is_some_and(|d| d > SCENE_CUT * self.geometry.max_value) {
                    log::debug!("Edges: the sphere doesn't match the frame at {timestamp_us} us, reset");
                    sphere.reset(now);
                }
            }
        }
        sphere.write(frame, now);
        *last = Some(timestamp_us);
    }

    /// From the filter's arguments; the control file, when there is one, overrides `mode` (and the blur and the view,
    /// if it has them)
    pub fn configure(&self, mode: EdgeMode, settings: EdgeSettings, view: u8, control: Option<PathBuf>) {
        self.mode.store(mode as u8, Ordering::Relaxed);
        self.view.store(view, Ordering::Relaxed);
        *self.settings.lock() = settings;
        *self.control.lock() = Control { path: control, modified: None, checked: None };
        self.trail.lock().last_us = None;
        *self.sphere_written.lock() = None;
        self.check_control(true);
        log::debug!("Edges: {:?}, {settings:?}", self.mode());
    }

    pub fn mode(&self) -> EdgeMode {
        self.check_control(false);
        EdgeMode::from_u8(self.mode.load(Ordering::Relaxed))
    }

    pub fn view(&self) -> u8 {
        self.check_control(false);
        self.view.load(Ordering::Relaxed)
    }

    pub fn set_view(&self, view: u8) {
        self.view.store(view, Ordering::Relaxed);
    }

    fn check_control(&self, now: bool) {
        // Another worker is already looking
        let Some(mut c) = self.control.try_lock() else { return };
        let Some(path) = c.path.clone() else { return };
        if !now && c.checked.is_some_and(|t| t.elapsed().as_millis() < 50) { return; }
        c.checked = Some(Instant::now());
        let Ok(modified) = std::fs::metadata(&path).and_then(|m| m.modified()) else { return };
        if c.modified == Some(modified) { return; }
        c.modified = Some(modified);
        let text = std::fs::read_to_string(&path).unwrap_or_default();
        let (mode, blur, view) = parse_control(&text);
        match mode {
            Some(mode) => {
                if self.mode.swap(mode as u8, Ordering::Relaxed) != mode as u8 {
                    log::info!("Edges: {mode:?}");
                }
            }
            None => log::warn!("Edges: unknown mode {text:?} in {path:?}"),
        }
        if let Some(view) = view {
            if self.view.swap(view, Ordering::Relaxed) != view {
                log::info!("View: {}", if view == VIEW_WIDE { "wide" } else { "normal" });
                // The screen trail is in the output's pixels, which now show another FOV: it starts over. The sphere is in
                // the world's directions, and stays
                self.trail.lock().last_us = None;
            }
        }
        if let Some(blur) = blur {
            let mut s = self.settings.lock();
            if s.blur != blur {
                s.blur = blur;
                log::info!("Edges: blur {:.1} %", blur * 100.0);
            }
        }
    }

    /// Runs on the caller's thread, not the GPU worker's: the source scaled down to the coarse grid
    pub fn prepare(&self, mode: EdgeMode, planes: &[PlaneJob]) -> EdgeJob {
        let g = &self.geometry;
        let t = Instant::now();
        let src_lo = Arc::new(planes.iter().map(|pj| unsafe {
            match g.kind {
                SampleKind::U8  => downscale::<u8>(pj.src, pj.src_stride, (pj.src_w, pj.src_h), g.grid_src),
                SampleKind::U16 => downscale::<u16>(pj.src, pj.src_stride, (pj.src_w, pj.src_h), g.grid_src),
            }
        }).collect());
        log::trace!("Edges: source scaled down in {:.2} ms", t.elapsed().as_secs_f64() * 1000.0);
        EdgeJob { mode, settings: *self.settings.lock(), src_lo }
    }
}

/// The views (lib.rs's Views): the project's, and the wider one
pub const VIEW_NORMAL: u8 = 0;
pub const VIEW_WIDE: u8 = 1;

/// The control file: the mode, then optional `key=value` settings (`blur` in % of the frame height, as the
/// filter's `edges_blur`; `view`, normal or wide). A file with only the mode in it leaves the settings as the filter
/// was given them
fn parse_control(text: &str) -> (Option<EdgeMode>, Option<f32>, Option<u8>) {
    let mut words = text.split_whitespace();
    let mode = EdgeMode::parse(words.next().unwrap_or(""));
    let (mut blur, mut view) = (None, None);
    for w in words {
        match w.split_once('=') {
            Some(("blur", v)) => match v.parse::<f32>() {
                Ok(v) if v.is_finite() => blur = Some((v / 100.0).clamp(0.0, 0.2)),
                _ => log::warn!("Edges: bad blur {v:?} in the control file"),
            },
            Some(("view", "normal")) => view = Some(VIEW_NORMAL),
            Some(("view", "wide")) => view = Some(VIEW_WIDE),
            _ => log::warn!("Edges: unknown setting {w:?} in the control file"),
        }
    }
    (mode, blur, view)
}

// ---------------------------------------------------------------------------------------------------
// Samples

pub trait Sample: Copy + bytemuck::Pod {
    fn to_f32(self) -> f32;
    fn from_f32(v: f32) -> Self;
}
impl Sample for u8 {
    #[inline] fn to_f32(self) -> f32 { self as f32 }
    #[inline] fn from_f32(v: f32) -> Self { (v + 0.5).clamp(0.0, 255.0) as u8 }
}
impl Sample for u16 {
    #[inline] fn to_f32(self) -> f32 { self as f32 }
    #[inline] fn from_f32(v: f32) -> Self { (v + 0.5).clamp(0.0, 65535.0) as u16 }
}

unsafe fn row<'a, T: Sample>(ptr: *const u8, stride: usize, y: usize, w: usize) -> &'a [T] {
    unsafe { std::slice::from_raw_parts(ptr.add(y * stride) as *const T, w) }
}

/// Box average into `to` cells. Every other row is enough for what gets blurred anyway, and halves the reading
unsafe fn downscale<T: Sample>(ptr: *const u8, stride: usize, (w, h): (usize, usize), (gw, gh): (usize, usize)) -> Vec<f32> {
    let mut out = vec![0.0f32; gw * gh];
    let xb: Vec<usize> = (0..=gw).map(|i| i * w / gw).collect();
    for j in 0..gh {
        let (y0, y1) = (j * h / gh, ((j + 1) * h / gh).max(j * h / gh + 1));
        let cell = &mut out[j * gw..(j + 1) * gw];
        let mut rows = 0;
        for y in (y0..y1).step_by(2) {
            let r = unsafe { row::<T>(ptr, stride, y, w) };
            for (i, c) in cell.iter_mut().enumerate() {
                *c += r[xb[i]..xb[i + 1].max(xb[i] + 1).min(w)].iter().map(|v| v.to_f32()).sum::<f32>();
            }
            rows += 1;
        }
        for (i, c) in cell.iter_mut().enumerate() {
            *c /= (rows * (xb[i + 1] - xb[i]).max(1)) as f32;
        }
    }
    out
}

#[inline]
fn bilinear(grid: &[f32], (gw, gh): (usize, usize), x: f32, y: f32) -> f32 {
    let x = x.clamp(0.0, (gw - 1) as f32);
    let y = y.clamp(0.0, (gh - 1) as f32);
    let (x0, y0) = (x as usize, y as usize);
    let (x1, y1) = ((x0 + 1).min(gw - 1), (y0 + 1).min(gh - 1));
    let (fx, fy) = (x - x0 as f32, y - y0 as f32);
    let a = grid[y0 * gw + x0] + (grid[y0 * gw + x1] - grid[y0 * gw + x0]) * fx;
    let b = grid[y1 * gw + x0] + (grid[y1 * gw + x1] - grid[y1 * gw + x0]) * fx;
    a + (b - a) * fy
}

/// `src` (covering the whole frame) on `to`, `scale` times bigger around the middle
fn resample(src: &[f32], from: (usize, usize), to: (usize, usize), scale: (f32, f32)) -> Vec<f32> {
    let mut out = vec![0.0f32; to.0 * to.1];
    for j in 0..to.1 {
        let v = 0.5 + ((j as f32 + 0.5) / to.1 as f32 - 0.5) / scale.1;
        for i in 0..to.0 {
            let u = 0.5 + ((i as f32 + 0.5) / to.0 as f32 - 0.5) / scale.0;
            out[j * to.0 + i] = bilinear(src, from, u * from.0 as f32 - 0.5, v * from.1 as f32 - 0.5);
        }
    }
    out
}

/// Three box passes each way, about a Gaussian of `sigma` cells; edges clamped
fn blur(grid: &mut [f32], (w, h): (usize, usize), sigma: f32) {
    let r = (((4.0 * sigma * sigma + 1.0).sqrt() - 1.0) / 2.0).round() as usize;
    if r < 1 { return; }
    // Box of 2r+1 along `line`, through `padded` (the line with its ends repeated r+1 times)
    fn pass(line: &mut [f32], padded: &mut Vec<f32>, r: usize) {
        let n = line.len();
        padded.clear();
        padded.extend(std::iter::repeat_n(line[0], r + 1));
        padded.extend_from_slice(line);
        padded.extend(std::iter::repeat_n(line[n - 1], r + 1));
        let norm = 1.0 / (2 * r + 1) as f32;
        let mut sum: f32 = padded[1..2 * r + 2].iter().sum();
        for i in 0..n {
            line[i] = sum * norm;
            sum += padded[i + 2 * r + 2] - padded[i + 1];
        }
    }
    let mut padded = Vec::with_capacity(w.max(h) + 2 * r + 2);
    let mut column = vec![0.0f32; h];
    for _ in 0..3 {
        for row in grid.chunks_exact_mut(w) { pass(row, &mut padded, r); }
    }
    for x in 0..w {
        for (y, c) in column.iter_mut().enumerate() { *c = grid[y * w + x]; }
        for _ in 0..3 { pass(&mut column, &mut padded, r); }
        for (y, c) in column.iter().enumerate() { grid[y * w + x] = *c; }
    }
}

/// The mask's source: white, with a border that ramps from black, in `(w, h)` pixels
fn mask_source((w, h): (usize, usize)) -> Vec<u8> {
    let inset = INSET * h as f32;
    let feather = (FEATHER * h as f32).max(1.0);
    let ramp = |d: f32| ((d - inset) / feather).clamp(0.0, 1.0);
    let mut out = vec![0u8; w * h];
    for y in 0..h {
        let dy = ramp((y as f32 + 0.5).min(h as f32 - y as f32 - 0.5));
        for x in 0..w {
            let dx = ramp((x as f32 + 0.5).min(w as f32 - x as f32 - 0.5));
            out[y * w + x] = (dx.min(dy) * 255.0 + 0.5) as u8;
        }
    }
    out
}

// ---------------------------------------------------------------------------------------------------
// Per GPU worker

struct GpuBuffers {
    /// u16 storage, so 16-bit samples are aligned; 8-bit ones use the bytes
    input: Vec<u16>,
    output: Vec<u16>,
    size_in: (usize, usize),
    size_out: (usize, usize),
    bytes: usize,
}
impl GpuBuffers {
    fn new(size_in: (usize, usize), size_out: (usize, usize), bytes: usize) -> GpuBuffers {
        let n = |(w, h): (usize, usize)| (w * h * bytes).div_ceil(2);
        GpuBuffers { input: vec![0; n(size_in)], output: vec![0; n(size_out)], size_in, size_out, bytes }
    }
    fn job(&mut self) -> PlaneJob {
        PlaneJob {
            src: self.input.as_ptr() as *const u8, src_stride: self.size_in.0 * self.bytes, src_w: self.size_in.0, src_h: self.size_in.1,
            dst: self.output.as_mut_ptr() as *mut u8, dst_stride: self.size_out.0 * self.bytes, dst_w: self.size_out.0, dst_h: self.size_out.1,
        }
    }
    fn input_of<T: Sample>(&mut self) -> &mut [T] {
        let n = self.size_in.0 * self.size_in.1;
        &mut bytemuck::cast_slice_mut::<u16, T>(&mut self.input)[..n]
    }
    fn output_of<T: Sample>(&self) -> &[T] {
        let n = self.size_out.0 * self.size_out.1;
        &bytemuck::cast_slice::<u16, T>(&self.output)[..n]
    }
}

/// What the GPU stabilizers of the edges are made from, taken from the project once
struct StabSetup {
    size: (usize, usize),
    output_size: (usize, usize),
    device: isize,
}
impl StabSetup {
    fn new(mgr: &StabilizationManager) -> StabSetup {
        let p = mgr.params.read();
        StabSetup { size: p.size, output_size: p.output_size, device: p.current_device as isize }
    }
    fn stab(&self, params: ComputeParams, index: usize) -> PlaneStab {
        let mut stab = Stabilization::default();
        stab.interpolation = Interpolation::Bilinear;
        stab.share_wgpu_instances = false;
        stab.set_device(self.device);
        stab.init_size(self.size, self.output_size);
        stab.set_compute_params(params);
        PlaneStab::new(stab, index, self.device)
    }
}

/// The mask's parameters: white on black, nothing else
fn mask_params(base: &ComputeParams, cfg: WorkerConfig) -> ComputeParams {
    let mut params = plane_compute_params(base, cfg, 0, false);
    params.background.fill(0.0);
    params.background_mode = BackgroundMode::SolidColor;
    params
}

/// A plane's parameters on the coarse grid: edges repeated rather than background, so it has no hole of its own
fn grid_params(base: &ComputeParams, cfg: WorkerConfig, index: usize, limited_range: bool) -> ComputeParams {
    let mut params = plane_compute_params(base, cfg, index, limited_range);
    params.background_mode = BackgroundMode::RepeatPixels;
    params
}

fn mask_buf(g: &EdgeGeometry, size: (usize, usize)) -> GpuBuffers {
    let mut buf = GpuBuffers::new(g.mask_src, size, 1);
    buf.input_of::<u8>().copy_from_slice(&mask_source(g.mask_src));
    buf
}

fn grid_bufs(g: &EdgeGeometry, size: (usize, usize)) -> Vec<GpuBuffers> {
    let bytes = if g.kind == SampleKind::U8 { 1 } else { 2 };
    (0..g.planes.len()).map(|_| GpuBuffers::new(g.grid_src, size, bytes)).collect()
}

/// The sphere trail's wide view (sphere::wide_params), that it's written from, with GPU stabilizers of its own: the
/// frame on a grid as fine as the output's would be unzoomed, and what the frame covers there (the mask rendered at
/// the grid's size is coverage enough)
struct WideView {
    mask: PlaneStab,
    mask_buf: GpuBuffers,
    planes: Vec<PlaneStab>,
    bufs: Vec<GpuBuffers>,
    /// For the directions it's written by
    params: ComputeParams,
}

/// What a WideView is made from, so it's made only when the sphere is written there
struct WideSetup {
    setup: StabSetup,
    mask: ComputeParams,
    planes: Vec<ComputeParams>,
    params: ComputeParams,
}

impl WideSetup {
    /// `base`: either view's parameters (lib.rs's Views): its own wide view has no zoom and a FOV of its own either way
    fn new(mgr: &StabilizationManager, base: &ComputeParams, cfg: WorkerConfig, g: &EdgeGeometry, limited_range: bool) -> WideSetup {
        WideSetup {
            setup: StabSetup::new(mgr),
            mask: wide_params(mask_params(base, cfg)),
            planes: (0..g.planes.len()).map(|index| wide_params(grid_params(base, cfg, index, limited_range))).collect(),
            params: wide_params(base.clone()),
        }
    }

    fn view(&self, g: &EdgeGeometry) -> WideView {
        WideView {
            mask: self.setup.stab(self.mask.clone(), 0), mask_buf: mask_buf(g, g.wide),
            planes: self.planes.iter().enumerate().map(|(index, p)| self.setup.stab(p.clone(), index)).collect(), bufs: grid_bufs(g, g.wide),
            params: self.params.clone(),
        }
    }
}

impl WideView {
    /// The frame and its coverage, on the wide grid
    fn render<T: Sample, P: PixelType>(&mut self, src_lo: &[Vec<f32>], timestamp_us: i64, max_value: f32) -> Result<(Vec<Vec<f32>>, Vec<f32>), String> {
        let planes = render_grid::<T, P>(&mut self.planes, &mut self.bufs, src_lo, timestamp_us, max_value)?;
        process_plane::<Luma8>(&mut self.mask, timestamp_us, 255.0, &self.mask_buf.job())?;
        let coverage = self.mask_buf.output_of::<u8>().iter().map(|&v| v as f32 / 255.0).collect();
        Ok((planes, coverage))
    }

    fn write(&self, edges: &Edges, planes: &[Vec<f32>], coverage: &[f32], timestamp_us: i64) {
        let view = View::at(&self.params, timestamp_us);
        edges.write_sphere(&GridFrame { view: &view, grid: edges.geometry.wide, planes, coverage }, timestamp_us);
    }
}

pub(crate) struct EdgeWorker {
    mask: PlaneStab,
    mask_buf: GpuBuffers,
    /// The stabilized frame on the coarse grid, per plane
    lo: Vec<PlaneStab>,
    lo_buf: Vec<GpuBuffers>,
    /// Where the sphere is written with each frame (an export), made on the first
    wide_setup: WideSetup,
    wide: Option<WideView>,
    /// The view's parameters, for the directions the sphere trail is read by
    params: ComputeParams,
    /// ms spent: mask, low-res frames, fill, composite; and frames
    timing: ([f64; 4], u32),
}

impl EdgeWorker {
    /// `base`: the view's parameters (lib.rs's Views)
    pub(crate) fn new(mgr: &StabilizationManager, base: &ComputeParams, cfg: WorkerConfig, g: &EdgeGeometry, limited_range: bool) -> EdgeWorker {
        let setup = StabSetup::new(mgr);
        EdgeWorker {
            mask: setup.stab(mask_params(base, cfg), 0), mask_buf: mask_buf(g, g.mask),
            lo: (0..g.planes.len()).map(|index| setup.stab(grid_params(base, cfg, index, limited_range), index)).collect(), lo_buf: grid_bufs(g, g.grid),
            wide_setup: WideSetup::new(mgr, base, cfg, g, limited_range), wide: None,
            params: base.clone(),
            timing: ([0.0; 4], 0),
        }
    }

    /// Another view (or range): what the output covers, and the frame on the grid, as it renders now. The sphere's
    /// own wide view is the same for both
    pub(crate) fn set_view(&mut self, base: &ComputeParams, cfg: WorkerConfig, limited_range: bool) {
        self.mask.stab.set_compute_params(mask_params(base, cfg));
        for plane in self.lo.iter_mut() {
            plane.stab.set_compute_params(grid_params(base, cfg, plane.index, limited_range));
        }
        self.params = base.clone();
    }

    /// Sets up the GPU side before playback, like the planes' own warm-up
    pub(crate) fn warm_up(&mut self, cfg: WorkerConfig) -> Result<(), String> {
        process_plane::<Luma8>(&mut self.mask, 0, 255.0, &self.mask_buf.job())?;
        for (plane, buf) in self.lo.iter_mut().zip(self.lo_buf.iter_mut()) {
            let job = buf.job();
            match cfg.kind {
                SampleKind::U8  => process_plane::<Luma8>(plane, 0, cfg.max_value, &job)?,
                SampleKind::U16 => process_plane::<Luma16>(plane, 0, cfg.max_value, &job)?,
            };
        }
        Ok(())
    }

    /// After the planes are stabilized: fills what the frame doesn't cover, in the output planes
    pub(crate) fn apply(&mut self, edges: &Edges, cfg: WorkerConfig, job: &EdgeJob, timestamp_us: i64, planes: &[PlaneJob]) -> Result<(), String> {
        match cfg.kind {
            SampleKind::U8  => self.apply_t::<u8, Luma8>(edges, cfg, job, timestamp_us, planes),
            SampleKind::U16 => self.apply_t::<u16, Luma16>(edges, cfg, job, timestamp_us, planes),
        }
    }

    fn apply_t<T: Sample, P: PixelType>(&mut self, edges: &Edges, cfg: WorkerConfig, job: &EdgeJob, timestamp_us: i64, planes: &[PlaneJob]) -> Result<(), String> {
        let g = &edges.geometry;
        let mut t = Instant::now();
        let mut lap = |i: usize, timing: &mut ([f64; 4], u32)| { timing.0[i] += t.elapsed().as_secs_f64() * 1000.0; t = Instant::now(); };
        process_plane::<Luma8>(&mut self.mask, timestamp_us, 255.0, &self.mask_buf.job())?;
        lap(0, &mut self.timing);
        let blocks = Blocks::new(self.mask_buf.output_of::<u8>(), g.mask);

        // In a clip with edges the sphere is written every frame, edges or not: what the camera sees while the output
        // covers is what fills the edges later. A clip without any doesn't need it, and it costs a fifth of 4K60.
        // In playback the sphere writer does it, as the GPU has room
        if job.mode == EdgeMode::Sphere && edges.expected.load(Ordering::Relaxed) {
            if edges.realtime.load(Ordering::Relaxed) && edges.writer.load(Ordering::Relaxed) {
                edges.post(timestamp_us, &job.src_lo);
            } else {
                let wide = self.wide.get_or_insert_with(|| self.wide_setup.view(g));
                let (planes, coverage) = wide.render::<T, P>(&job.src_lo, timestamp_us, cfg.max_value)?;
                lap(1, &mut self.timing);
                wide.write(edges, &planes, &coverage, timestamp_us);
                lap(2, &mut self.timing);
            }
        }

        let mask = self.mask_buf.output_of::<u8>();
        log::trace!("Edges: {} of {} mask blocks to fill at {timestamp_us} us, mask mean {:.1}", blocks.flags.iter().filter(|&&b| b).count(), blocks.flags.len(),
                    mask.iter().map(|&v| v as f32).sum::<f32>() / mask.len() as f32);
        if !blocks.any {
            self.count_frame();
            return Ok(());
        }

        let lo = if job.mode != EdgeMode::Original {
            render_grid::<T, P>(&mut self.lo, &mut self.lo_buf, &job.src_lo, timestamp_us, cfg.max_value)?
        } else {
            Vec::new()
        };
        lap(1, &mut self.timing);

        let zoom = |lo: &[f32]| resample(lo, g.grid, g.grid, (job.settings.zoom.max(1.0), job.settings.zoom.max(1.0)));
        let mut fills: Vec<Vec<f32>> = match job.mode {
            EdgeMode::Off => return Ok(()),
            EdgeMode::Original => {
                // Cover the output with the source, aspect kept
                let ((sw, sh), (ow, oh)) = g.planes[0];
                let k = (ow as f32 / sw as f32).max(oh as f32 / sh as f32);
                let scale = (k * sw as f32 / ow as f32, k * sh as f32 / oh as f32);
                job.src_lo.iter().map(|s| resample(s, g.grid_src, g.grid, scale)).collect()
            }
            EdgeMode::Zoom => lo.iter().map(|l| zoom(l)).collect(),
            EdgeMode::Trail | EdgeMode::Sphere => {
                let coverage = cell_coverage(mask, g.mask, g.grid);
                let mut fills: Vec<Vec<f32>> = {
                    let mut trail = edges.trail.lock();
                    update_trail(&mut trail, &lo, &coverage, timestamp_us, g.max_value);
                    lo.iter().zip(&trail.planes).map(|(l, t)| {
                        zoom(l).iter().zip(t).zip(&trail.weight).map(|((z, t), w)| t * w + z * (1.0 - w)).collect()
                    }).collect()
                };
                if job.mode == EdgeMode::Sphere {
                    let view = View::at(&self.params, timestamp_us);
                    if let Some(sphere) = edges.sphere.get() {
                        read_sphere(sphere, &view, g.grid, &coverage, timestamp_us, &mut fills);
                    }
                }
                fills
            }
        };
        let sigma = job.settings.blur * g.grid.1 as f32;
        for f in fills.iter_mut() { blur(f, g.grid, sigma); }
        lap(2, &mut self.timing);

        for (pj, fill) in planes.iter().zip(&fills) {
            unsafe { composite::<T>(pj, mask, g.mask, &blocks, fill, g.grid) };
        }
        lap(3, &mut self.timing);
        self.count_frame();
        Ok(())
    }

    fn count_frame(&mut self) {
        self.timing.1 += 1;
        if self.timing.1 == 300 {
            let ms = self.timing.0.map(|v| v / 300.0);
            log::debug!("Edges per frame: mask {:.2} ms, low-res frames {:.2}, fill {:.2}, composite {:.2}", ms[0], ms[1], ms[2], ms[3]);
            self.timing = ([0.0; 4], 0);
        }
    }
}

/// Playback's sphere writer, on a thread of its own: the newest frame posted, through the wide view, onto the sphere.
/// Only while some GPU worker has no frame: the frames come first, and when they keep all the workers busy the sphere
/// waits, written less often or not at all. Its GPU stabilizers are made on the first frame
pub(crate) fn sphere_writer(edges: Arc<Edges>, mgr: Arc<StabilizationManager>, base: ComputeParams, cfg: WorkerConfig) {
    let setup = WideSetup::new(&mgr, &base, cfg, &edges.geometry, cfg.is_yuv);
    drop(mgr);
    edges.writer.store(true, Ordering::Relaxed);
    let stop = || edges.feed.stop.load(Ordering::Relaxed);
    let mut wide: Option<WideView> = None;
    let (mut written, mut waited, mut since) = (0u32, Duration::ZERO, Instant::now());
    loop {
        {
            let mut frame = edges.feed.frame.lock();
            while frame.is_none() && !stop() { edges.feed.ready.wait(&mut frame); }
        }
        let t = Instant::now();
        while edges.in_flight.load(Ordering::Relaxed) >= edges.workers && !stop() {
            std::thread::sleep(Duration::from_millis(1));
        }
        if stop() { break; }
        waited += t.elapsed();
        // The newest by now
        let Some((timestamp_us, src_lo)) = edges.feed.frame.lock().take() else { continue };
        let wide = wide.get_or_insert_with(|| setup.view(&edges.geometry));
        let rendered = match cfg.kind {
            SampleKind::U8  => wide.render::<u8, Luma8>(&src_lo, timestamp_us, cfg.max_value),
            SampleKind::U16 => wide.render::<u16, Luma16>(&src_lo, timestamp_us, cfg.max_value),
        };
        match rendered {
            Ok((planes, coverage)) => wide.write(&edges, &planes, &coverage, timestamp_us),
            Err(e) => log::error!("Sphere writer: {e}"),
        }
        written += 1;
        let elapsed = since.elapsed();
        if elapsed >= Duration::from_secs(5) {
            log::debug!("Sphere written {:.1} times a second, waiting for a free GPU worker {:.0} % of the time", written as f64 / elapsed.as_secs_f64(), waited.as_secs_f64() / elapsed.as_secs_f64() * 100.0);
            (written, waited, since) = (0, Duration::ZERO, Instant::now());
        }
    }
    log::debug!("Sphere writer finished");
}

/// The source on the coarse grid through each plane's stabilizer, back as f32
fn render_grid<T: Sample, P: PixelType>(stabs: &mut [PlaneStab], bufs: &mut [GpuBuffers], src_lo: &[Vec<f32>], timestamp_us: i64, max_value: f32) -> Result<Vec<Vec<f32>>, String> {
    stabs.iter_mut().zip(bufs.iter_mut()).zip(src_lo).map(|((plane, buf), src)| {
        for (d, s) in buf.input_of::<T>().iter_mut().zip(src) { *d = T::from_f32(*s); }
        process_plane::<P>(plane, timestamp_us, max_value, &buf.job())?;
        Ok(buf.output_of::<T>().iter().map(|v| v.to_f32()).collect())
    }).collect()
}

/// How much of each grid cell the frame covers, from the mask
fn cell_coverage(mask: &[u8], (mw, mh): (usize, usize), (gw, gh): (usize, usize)) -> Vec<f32> {
    let mut out = vec![0.0f32; gw * gh];
    for j in 0..gh {
        let (y0, y1) = (j * mh / gh, ((j + 1) * mh / gh).max(j * mh / gh + 1).min(mh));
        for i in 0..gw {
            let (x0, x1) = (i * mw / gw, ((i + 1) * mw / gw).max(i * mw / gw + 1).min(mw));
            let sum: u32 = (y0..y1).map(|y| mask[y * mw + x0..y * mw + x1].iter().map(|&v| v as u32).sum::<u32>()).sum();
            out[j * gw + i] = sum as f32 / (255 * (y1 - y0) * (x1 - x0)) as f32;
        }
    }
    out
}

fn update_trail(trail: &mut Trail, lo: &[Vec<f32>], coverage: &[f32], timestamp_us: i64, max_value: f32) {
    let reset = |trail: &mut Trail| trail.weight.iter_mut().for_each(|w| *w = 0.0);
    let dt = match trail.last_us {
        None => { reset(trail); 0 }
        Some(last) if (timestamp_us - last).abs() > TRAIL_MAX_GAP_US => { reset(trail); 0 }
        // A frame another worker finished after a newer one: the trail already has newer content
        Some(last) if timestamp_us <= last => return,
        Some(last) => timestamp_us - last,
    };
    // A cut: the frame and the trail differ where both have content
    let (mut diff, mut n) = (0.0f32, 0usize);
    for ((c, w), (s, t)) in coverage.iter().zip(&trail.weight).zip(lo[0].iter().zip(&trail.planes[0])) {
        if *c > 0.95 && *w > 0.5 { diff += (s - t).abs(); n += 1; }
    }
    if n > coverage.len() / 20 && diff / n as f32 > SCENE_CUT * max_value {
        log::debug!("Edges: scene cut at {timestamp_us} us, trail reset");
        reset(trail);
    }
    let decay = 0.5f32.powf(dt as f32 / 1e6 / TRAIL_HALF_LIFE_S);
    for (i, &c) in coverage.iter().enumerate() {
        let old = trail.weight[i] * decay * (1.0 - c);
        let w = c + old;
        if w > 1e-4 {
            for (t, l) in trail.planes.iter_mut().zip(lo) {
                t[i] = (l[i] * c + t[i] * old) / w;
            }
        }
        trail.weight[i] = w;
    }
    trail.last_us = Some(timestamp_us);
}

/// Reads the sphere over `fills` (the screen trail) where the frame doesn't cover, by how much it has in each cell's
/// direction. Where it covers, the fill is the frame anyway: the screen trail has just taken it
fn read_sphere(sphere: &Sphere, view: &View, (gw, gh): (usize, usize), coverage: &[f32], timestamp_us: i64, fills: &mut [Vec<f32>]) {
    let fade = sphere.fade(timestamp_us as f32 / 1e6);
    let mut values = vec![0.0f32; fills.len()];
    for j in 0..gh {
        for i in 0..gw {
            let k = j * gw + i;
            if coverage[k] >= 0.999 { continue; }
            let w = sphere.sample(&view.dir(i as f32 / gw as f32, j as f32 / gh as f32), fade, &mut values).min(1.0);
            for (f, v) in fills.iter_mut().zip(&values) { f[k] = v * w + f[k] * (1.0 - w); }
        }
    }
}

/// Which 8x8 blocks of the mask aren't fully covered, grown by a block (bilinear sampling reaches into the next)
struct Blocks {
    any: bool,
    size: (usize, usize),
    flags: Vec<bool>,
}
const BLOCK: usize = 8;
impl Blocks {
    fn new(mask: &[u8], (mw, mh): (usize, usize)) -> Blocks {
        let (bw, bh) = (mw.div_ceil(BLOCK), mh.div_ceil(BLOCK));
        let mut open = vec![false; bw * bh];
        for y in 0..mh {
            for (x, &v) in mask[y * mw..(y + 1) * mw].iter().enumerate() {
                if v < 255 { open[(y / BLOCK) * bw + x / BLOCK] = true; }
            }
        }
        let mut flags = vec![false; bw * bh];
        for by in 0..bh {
            for bx in 0..bw {
                if !open[by * bw + bx] { continue; }
                for y in by.saturating_sub(1)..(by + 2).min(bh) {
                    for x in bx.saturating_sub(1)..(bx + 2).min(bw) { flags[y * bw + x] = true; }
                }
            }
        }
        Blocks { any: open.iter().any(|&b| b), size: (bw, bh), flags }
    }
}

/// Where each output column falls on a coarser buffer: the two neighbours and the weight of the second
fn taps(n: usize, to: usize) -> Vec<(u32, u32, f32)> {
    (0..n).map(|x| {
        let v = ((x as f32 + 0.5) * to as f32 / n as f32 - 0.5).clamp(0.0, (to - 1) as f32);
        let i = v as usize;
        (i as u32, (i + 1).min(to - 1) as u32, v - i as f32)
    }).collect()
}

const THREADS: usize = 4;

/// out = picture * mask + fill * (1 - mask), in the blocks that need it; rows split over a few threads
unsafe fn composite<T: Sample>(pj: &PlaneJob, mask: &[u8], (mw, mh): (usize, usize), blocks: &Blocks, fill: &[f32], (gw, gh): (usize, usize)) {
    let (dw, dh) = (pj.dst_w, pj.dst_h);
    let (mx, gx) = (taps(dw, mw), taps(dw, gw));
    let (my, gy) = (taps(dh, mh), taps(dh, gh));
    // Per block row, the runs of blocks to fill, in output columns
    let (bw, bh) = blocks.size;
    let spans: Vec<Vec<(usize, usize)>> = (0..bh).map(|by| {
        let mut out: Vec<(usize, usize)> = Vec::new();
        for bx in (0..bw).filter(|&bx| blocks.flags[by * bw + bx]) {
            let x0 = bx * BLOCK * dw / mw;
            let x1 = ((bx + 1) * BLOCK * dw).div_ceil(mw).min(dw);
            match out.last_mut() {
                Some(last) if last.1 >= x0 => last.1 = x1,
                _ => out.push((x0, x1)),
            }
        }
        out
    }).collect();
    let (dst, stride) = (pj.dst as usize, pj.dst_stride);
    let rows_per = dh.div_ceil(THREADS);
    std::thread::scope(|scope| {
        for k in 0..THREADS {
            let (mx, gx, my, gy, spans) = (&mx, &gx, &my, &gy, &spans);
            scope.spawn(move || {
                let mut mrow = vec![0.0f32; mw];
                let mut frow = vec![0.0f32; gw];
                for y in k * rows_per..((k + 1) * rows_per).min(dh) {
                    let (m0, m1, mf) = my[y];
                    let row_spans = &spans[(m0 as usize / BLOCK).min(bh - 1)];
                    if row_spans.is_empty() { continue; }
                    let (a, b) = (&mask[m0 as usize * mw..][..mw], &mask[m1 as usize * mw..][..mw]);
                    let (g0, g1, gf) = gy[y];
                    let (fa, fb) = (&fill[g0 as usize * gw..][..gw], &fill[g1 as usize * gw..][..gw]);
                    for (f, (p, q)) in frow.iter_mut().zip(fa.iter().zip(fb)) { *f = p + (q - p) * gf; }
                    let out = unsafe { std::slice::from_raw_parts_mut((dst + y * stride) as *mut T, dw) };
                    for &(x0, x1) in row_spans {
                        let (c0, c1) = (mx[x0].0 as usize, mx[x1 - 1].1 as usize + 1);
                        for c in c0..c1 {
                            mrow[c] = (a[c] as f32 + (b[c] as f32 - a[c] as f32) * mf) * (1.0 / 255.0);
                        }
                        for x in x0..x1 {
                            let (i, j, t) = mx[x];
                            let m = mrow[i as usize] + (mrow[j as usize] - mrow[i as usize]) * t;
                            if m >= 0.999 { continue; }
                            let (i, j, t) = gx[x];
                            let f = frow[i as usize] + (frow[j as usize] - frow[i as usize]) * t;
                            let s = out[x].to_f32();
                            out[x] = T::from_f32(f + (s - f) * m);
                        }
                    }
                }
            });
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn control_file() {
        assert_eq!(parse_control("sphere"), (Some(EdgeMode::Sphere), None, None));
        assert_eq!(parse_control("trail\n"), (Some(EdgeMode::Trail), None, None));
        assert_eq!(parse_control("sphere blur=3.5"), (Some(EdgeMode::Sphere), Some(3.5 / 100.0), None));
        assert_eq!(parse_control("zoom blur=50"), (Some(EdgeMode::Zoom), Some(0.2), None));
        assert_eq!(parse_control("zoom blur=x"), (Some(EdgeMode::Zoom), None, None));
        assert_eq!(parse_control(""), (Some(EdgeMode::Off), None, None));
        assert_eq!(parse_control("nope blur=1"), (None, Some(1.0 / 100.0), None));
        assert_eq!(parse_control("sphere blur=2 view=wide"), (Some(EdgeMode::Sphere), Some(0.02), Some(VIEW_WIDE)));
        assert_eq!(parse_control("off view=normal"), (Some(EdgeMode::Off), None, Some(VIEW_NORMAL)));
        assert_eq!(parse_control("off view=sideways"), (Some(EdgeMode::Off), None, None));
    }
}
