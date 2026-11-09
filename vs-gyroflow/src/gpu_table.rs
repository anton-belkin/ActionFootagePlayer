// SPDX-License-Identifier: GPL-3.0-or-later
//
// The GPU warp's table (docs/gpu-render.md, step 1: the spike): each frame's kernel parameters and its row rotations,
// from gyroflow-core as the filter gets them, packed for shaders/gyroflow/warp.glsl, which finds its frame by mpv's
// PTS. The rotations go as quaternion knots evenly over the frame's rows (nlerp between them in the shader), or one
// per row (Gyroflow's own nearest-row matrices). bin/gyroflow-gpu-table writes the shader file.

use gyroflow_core::stabilization::{ ComputeParams, FrameTransform };
use nalgebra::{ Matrix3, Rotation3, UnitQuaternion };
use crate::Underwater;

pub const TEMPLATE: &str = include_str!("../../shaders/gyroflow/warp.glsl");
/// The frame's parameters before its knots, in texels (warp.glsl's table layout)
pub const PARAM_TEXELS: usize = 16;

/// Gyroflow's lens models and digital lenses as GLSL (shaders/gyroflow/lens, copies of gyroflow-core's
/// distortion_models/*.glsl), by their ids. Sony's and the generic polynomial need mesh data the warp doesn't carry
const LENSES: &[(&str, &str)] = &[
    ("opencv_fisheye", include_str!("../../shaders/gyroflow/lens/opencv_fisheye.glsl")),
    ("opencv_standard", include_str!("../../shaders/gyroflow/lens/opencv_standard.glsl")),
    ("poly3", include_str!("../../shaders/gyroflow/lens/poly3.glsl")),
    ("poly5", include_str!("../../shaders/gyroflow/lens/poly5.glsl")),
    ("ptlens", include_str!("../../shaders/gyroflow/lens/ptlens.glsl")),
    ("gopro", include_str!("../../shaders/gyroflow/lens/gopro.glsl")),
    ("insta360", include_str!("../../shaders/gyroflow/lens/insta360.glsl")),
];
const DIGITAL_LENSES: &[(&str, &str)] = &[
    ("gopro_superview", include_str!("../../shaders/gyroflow/lens/gopro_superview.glsl")),
    ("gopro6_superview", include_str!("../../shaders/gyroflow/lens/gopro6_superview.glsl")),
    ("gopro_hyperview", include_str!("../../shaders/gyroflow/lens/gopro_hyperview.glsl")),
    ("gopro_warp", include_str!("../../shaders/gyroflow/lens/gopro_warp.glsl")),
    ("digital_stretch", include_str!("../../shaders/gyroflow/lens/digital_stretch.glsl")),
];

/// Why the warp can't render this clip as the VapourSynth path does, if it can't: the lens or digital lens it has no
/// GLSL for, a lens correction below 100 % (the root solve isn't ported), the equirect output, a horizontal rolling
/// shutter, IBIS/OIS, lens breathing or a mesh (Sony)
pub fn unsupported(clip: &Clip) -> Option<String> {
    let p = &clip.params;
    let lens = p.distortion_model.id();
    if !LENSES.iter().any(|(id, _)| *id == lens) { return Some(format!("lens model {lens}")); }
    if let Some(d) = &p.digital_lens {
        if !DIGITAL_LENSES.iter().any(|(id, _)| *id == d.id()) { return Some(format!("digital lens {}", d.id())); }
    }
    if p.lens_correction_amount < 1.0 { return Some(format!("lens correction {:.0} %", p.lens_correction_amount * 100.0)); }
    if p.output_projection == gyroflow_core::stabilization::projection::EQUIRECT { return Some("equirect output".into()); }
    if p.frame_readout_direction.is_horizontal() && clip.rows() > 1 { return Some("horizontal rolling shutter".into()); }
    let tr = clip.transform(clip.frame_count / 2, clip.output_size);
    if tr.matrices.iter().any(|m| m[9..14].iter().any(|v| *v != 0.0) || (m[14] > 0.0 && m[14] != 1.0)) { return Some("IBIS, OIS or lens breathing".into()); }
    if !tr.mesh_data.is_empty() { return Some("a lens mesh".into()); }
    None
}

/// The clip's lens and digital lens functions, for warp.glsl's @LENS@
fn lens_code(clip: &Clip) -> String {
    let p = &clip.params;
    let find = |list: &[(&str, &'static str)], id: &str| list.iter().find(|(i, _)| *i == id).map(|(_, t)| *t);
    let mut code = find(LENSES, p.distortion_model.id()).unwrap_or(LENSES[0].1).to_string();
    if let Some(d) = p.digital_lens.as_ref().and_then(|d| find(DIGITAL_LENSES, d.id())) { code += "\n"; code += d; }
    // A lens file's comments may not hold directive marks (libplacebo reads them anywhere in a pass)
    code.replace("//!", "// !")
}
/// The widest texture the table may be (Vulkan's maxImageDimension2D on Apple and Intel GPUs)
pub const MAX_TEX: usize = 16384;

/// A clip's motion as the filter renders it: a project, or the video with its own motion data
pub struct Clip {
    pub params: ComputeParams,
    pub size: (usize, usize),
    pub output_size: (usize, usize),
    pub fps: f64,
    pub frame_count: usize,
    pub frame_offset_us: i64,
}

/// The player's choices for a clip, as main.lua hands them to the VapourSynth filter (gyroflow.vpy's user-data JSON),
/// plus the video's own size (`input_width`, `input_height`), which the filter learns from its input
pub fn from_player_json(j: &serde_json::Value) -> Result<(Clip, f64), String> {
    let loaded = PlayerClip::load(j)?;
    Ok((loaded.clip(j), j.get("time_offset").and_then(|v| v.as_f64()).unwrap_or(0.0)))
}

/// A clip loaded for the player with every view it has (lib.rs's Views: normal and wide, and per level for a clip
/// without a project), so the view is picked without loading it again (gyroflow-gpu-table --serve keeps these)
pub struct PlayerClip {
    pub views: std::sync::Arc<crate::Views>,
    /// Built tables, by view, the fringes taken out or not and the knots; whether the view's lens is under water
    tables: std::sync::Mutex<Vec<((usize, bool, usize), std::sync::Arc<Table>)>>,
    wet: std::sync::Mutex<Vec<(usize, bool)>>,
    pub size: (usize, usize),
    pub output_size: (usize, usize),
    pub fps: f64,
    pub frame_count: usize,
    pub frame_offset_us: i64,
}

/// The JSON's keys that change what PlayerClip::load computes; the rest (the view, the edges, the color fringes, the
/// denoise, the interpolation, the time offset) are the shader's, or a pick among the views
pub const PLAYER_GEOMETRY_KEYS: &[&str] = &["project", "preset", "fov", "level", "horizon_lock", "underwater", "trim_start", "trim_end", "alone",
                                            "rotation", "aspect", "projection", "lens", "input_width", "input_height"];

impl PlayerClip {
    pub fn load(j: &serde_json::Value) -> Result<PlayerClip, String> {
        use crate::{ Fov, Frame, LensMode, Level };
        let s = |k: &str| j.get(k).and_then(|v| v.as_str()).filter(|v| !v.is_empty());
        let f = |k: &str| j.get(k).and_then(|v| v.as_f64());
        let project = s("project").ok_or("no project")?;
        // Wide or not is a pick among the views: Fov::Project computes them all
        let fov = f("fov").filter(|v| *v > 0.0).map(|v| Fov::Set(v.clamp(0.1, 4.0))).unwrap_or(Fov::Project);
        let underwater = match s("underwater") { None => Underwater::Auto, Some(v) => Underwater::parse(v).ok_or(format!("underwater {v}"))? };
        let level = match s("level") { None => Level::Medium, Some(v) => Level::parse(v).ok_or(format!("level {v}"))? };
        let trim = match (f("trim_start"), f("trim_end")) { (Some(a), Some(b)) if b > a => Some((a, b)), _ => None };
        let alone = j.get("alone").and_then(|v| v.as_bool());
        let frame = Frame::new(j.get("rotation").and_then(|v| v.as_i64()).unwrap_or(0), s("aspect"), j.get("projection").and_then(|v| v.as_i64()))?;
        let lens = match s("lens") { None => LensMode::Stabilized, Some(v) => LensMode::parse(v).ok_or(format!("lens {v}"))? };
        let input = (f("input_width").unwrap_or(0.0) as usize, f("input_height").unwrap_or(0.0) as usize);
        // All of a clip's own motion data before it plays (lookahead 0): the table is written once
        let p = crate::prepare(project, s("preset"), fov, f("horizon_lock").map(|v| v.clamp(0.0, 100.0)), underwater, level, 0.0, input, None, trim, alone, frame, lens)?;
        let sp = p.stab.params.read();
        Ok(PlayerClip { views: p.views.clone(), tables: Default::default(), wet: Default::default(), size: sp.size, output_size: sp.output_size, fps: sp.fps, frame_count: sp.frame_count, frame_offset_us: p.frame_offset_us })
    }

    /// The index of the view the JSON asks for (wide, level) in `views`
    pub fn view(&self, j: &serde_json::Value) -> usize {
        let wide = j.get("wide").and_then(|v| v.as_bool().or(v.as_i64().map(|i| i != 0))).unwrap_or(false);
        let level = j.get("level").and_then(|v| v.as_str()).and_then(crate::Level::parse).unwrap_or(crate::Level::Medium);
        self.views.index(if wide { crate::edges::VIEW_WIDE } else { crate::edges::VIEW_NORMAL }, level).min(self.views.params.len() - 1)
    }

    /// Whether the lens is under water anywhere in the clip, in view `view` (every 15th frame), kept
    pub fn wet(&self, view: usize, clip: &Clip) -> bool {
        if let Some(w) = self.wet.lock().unwrap().iter().find(|(v, _)| *v == view).map(|(_, w)| *w) { return w; }
        let w = (0..clip.frame_count).step_by(15).any(|f| clip.transform(f, clip.output_size).kernel_params.light_refraction_coefficient > 1.0);
        self.wet.lock().unwrap().push((view, w));
        w
    }

    /// The table for `clip` (view `view`) with these options, built once (it depends on the view, the fringes and the
    /// knots; the whole clip's)
    pub fn table(&self, view: usize, clip: &Clip, opts: &Options) -> Result<std::sync::Arc<Table>, String> {
        let key = (view, opts.chromatic, opts.knots);
        if opts.range.is_some() { return Table::build(clip, opts).map(std::sync::Arc::new); }
        if let Some(t) = self.tables.lock().unwrap().iter().find(|(k, _)| *k == key).map(|(_, t)| t.clone()) { return Ok(t); }
        let t = std::sync::Arc::new(Table::build(clip, opts)?);
        let mut tables = self.tables.lock().unwrap();
        tables.push((key, t.clone()));
        if tables.len() > 4 { tables.remove(0); }
        Ok(t)
    }

    /// The clip in the view the JSON asks for (wide, level)
    pub fn clip(&self, j: &serde_json::Value) -> Clip {
        let index = self.view(j);
        Clip { params: self.views.params[index].clone(), size: self.size, output_size: self.output_size, fps: self.fps, frame_count: self.frame_count, frame_offset_us: self.frame_offset_us }
    }
}

impl Clip {
    /// Underwater as the project says, the project's output size, no horizon lock or FOV of the player's own: what
    /// the plugin does for a clip with a project, edges off. `readout_ms`: the rolling shutter's readout time instead
    /// of the project's
    pub fn load(project: &str, readout_ms: Option<f64>) -> Result<Clip, String> {
        let (stab, _, _) = crate::load_manager(project, None, None, Underwater::Project, 0.0, false)?;
        if let Some(ms) = readout_ms { stab.params.write().frame_readout_time = ms; }
        stab.invalidate_smoothing();
        stab.recompute_blocking();
        let params = ComputeParams::from_manager(&stab);
        let p = stab.params.read();
        Ok(Clip { size: p.size, output_size: p.output_size, fps: p.fps, frame_count: p.frame_count,
                  frame_offset_us: (p.frame_offset as f64 / p.fps * 1_000_000.0).round() as i64, params })
    }

    /// Frame `frame`'s transform, as the filter computes it for a frame shown at pts frame / fps (vs-gyroflow's
    /// Filter: the pts plus the frame offset), for an output buffer of `out` pixels
    pub fn transform(&self, frame: usize, out: (usize, usize)) -> FrameTransform {
        let ts_ms = ((frame as f64 * 1_000_000.0 / self.fps).round() as i64 + self.frame_offset_us) as f64 / 1000.0;
        let mut tr = FrameTransform::at_timestamp(&self.params, ts_ms, gyroflow_core::frame_at_timestamp(ts_ms, self.params.scaled_fps) as usize);
        let kp = &mut tr.kernel_params;
        (kp.width, kp.height) = (self.size.0 as i32, self.size.1 as i32);
        (kp.output_width, kp.output_height) = (self.output_size.0 as i32, self.output_size.1 as i32);
        kp.source_rect = [0, 0, self.size.0 as i32, self.size.1 as i32];
        kp.output_rect = [0, 0, out.0 as i32, out.1 as i32];
        tr
    }

    /// The rows of matrices a frame has: its height with a rolling shutter readout, else 1
    pub fn rows(&self) -> usize {
        if self.params.frame_readout_time.abs() > 0.0 { self.size.1 } else { 1 }
    }
}

/// How the passes are laid out (warp.glsl's header)
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Mode {
    /// One pass at MAIN, the geometry at every pixel
    Exact,
    /// The geometry on a coarse grid (a compute pass), then one sampling pass at MAIN
    Grid,
    /// The grid, then sampling passes at LUMA and CHROMA (libplacebo converts at the output's size)
    Planes,
}

/// The sphere trail's equirect width (its height half of it): 0.18° a texel
const SPHERE_W: usize = 2048;

/// The largest output the grid texture is made for
const GRID_MAX: (usize, usize) = (8192, 4608);

#[derive(Clone, Copy, Debug)]
pub struct Options {
    pub mode: Mode,
    /// The grid's step in output pixels
    pub grid_step: usize,
    /// The grid interpolated bilinearly (4 reads) rather than Catmull-Rom (16)
    pub grid_linear: bool,
    /// Planes: the chroma sampled bilinearly whatever the luma's interpolation
    pub chroma_bilinear: bool,
    /// Under water, red and blue each read at their own coordinate (the water's color fringes taken out); needs the
    /// RGB layouts (exact, grid)
    pub chromatic: bool,
    /// Red and blue bilinear under chromatic (else with the main interpolation)
    pub chromatic_fast: bool,
    /// The uncovered edges filled (the plugin's smooth fill), blurred over this fraction of the frame's height; None: the
    /// background color
    pub edge_fill: Option<f32>,
    /// The edges filled from what was last seen in each direction of the world (the plugin's sphere trail), the smooth
    /// fill where nothing was; needs the RGB layouts
    pub sphere: bool,
    /// A moment played as a video of its own: where it starts in the clip, seconds
    pub time_offset: f64,
    /// The temporal denoise after the warp (the clean frame while it plays: warp.glsl's dn_ passes)
    pub denoise: bool,
    /// Knots a frame; 0: every row
    pub knots: usize,
    /// warp.glsl's INTERP: 0 bilinear, 1 B-spline, 2 bicubic, 3 Lanczos4, 4 EWA
    pub interp: i32,
    pub shrink: bool,
    /// Frames in the table (the rest show the background)
    pub range: Option<(usize, usize)>,
}

pub struct Table {
    pub width: usize,
    pub height: usize,
    /// rgba32f texels, row by row
    pub data: Vec<f32>,
    pub block: usize,
    pub cols: usize,
    /// Quaternions a frame (knots, or rows)
    pub per_frame: usize,
}

impl Table {
    pub fn build(clip: &Clip, opts: &Options) -> Result<Table, String> {
        let per_frame = if opts.knots == 0 { clip.rows() } else { opts.knots.max(2) };
        let block = PARAM_TEXELS + per_frame;
        if block > MAX_TEX { return Err(format!("{block} texels a frame don't fit a {MAX_TEX}-wide texture")); }
        let frames = clip.frame_count.max(1);
        // As many frames a row as fit whatever the clip's length, and the frame count in the table (texel 9's w): the
        // shader's text is then the same for clips alike, and mpv compiles it once
        let cols = (MAX_TEX / block).max(1);
        let (width, height) = (cols * block, frames.div_ceil(cols));
        let mut data = vec![0f32; width * height * 4];
        let (first, last) = opts.range.unwrap_or((0, frames - 1));
        // Each frame's block on its own (a rolling shutter's transform is a matrix a row: ~0.2 ms a frame), on every core
        let block_of = |frame: usize| -> Vec<[f32; 4]> {
            let tr = clip.transform(frame, clip.output_size);
            let kp = &tr.kernel_params;
            let k = |i: usize| [kp.k[i * 4], kp.k[i * 4 + 1], kp.k[i * 4 + 2], kp.k[i * 4 + 3]];
            let dl = |i: usize| [kp.digital_lens_params[i * 4], kp.digital_lens_params[i * 4 + 1], kp.digital_lens_params[i * 4 + 2], kp.digital_lens_params[i * 4 + 3]];
            let mut texels: Vec<[f32; 4]> = vec![
                [kp.f[0], kp.f[1], kp.c[0], kp.c[1]],
                k(0), k(1), k(2), k(3), k(4), k(5),
                [kp.fov, kp.lens_correction_amount, kp.light_refraction_coefficient, kp.field_limit],
                [kp.translation2d[0], kp.translation2d[1], kp.matrix_count as f32, 1.0],
                [kp.translation3d[0], kp.translation3d[1], kp.translation3d[2], frames as f32],
                dl(0), dl(1), dl(2), dl(3),
                {
                    // Red's and blue's index over green's where the frame is under water and the fringes are taken out
                    let n = kp.light_refraction_coefficient as f64;
                    let k = if opts.chromatic && n > 1.0 && kp.lens_correction_amount >= 0.999 { crate::dispersion::water_ratios(n) } else { [1.0, 1.0] };
                    [kp.input_horizontal_stretch, kp.input_vertical_stretch, k[0], k[1]]
                },
                {
                    // The output camera in the world (the sphere trail): its quaternion
                    let ts_us = (frame as f64 * 1_000_000.0 / clip.fps).round() as i64 + clip.frame_offset_us;
                    let r: Matrix3<f64> = nalgebra::convert(crate::sphere::View::at(&clip.params, ts_us).rotation());
                    let q = UnitQuaternion::from_rotation_matrix(&Rotation3::from_matrix_unchecked(r));
                    [q.i as f32, q.j as f32, q.k as f32, q.w as f32]
                },
            ];
            debug_assert_eq!(texels.len(), PARAM_TEXELS);
            let quats = if opts.knots == 0 { row_quats(&tr.matrices) } else { knot_quats(&tr.matrices, per_frame) };
            texels.extend((0..per_frame).map(|i| quats[i.min(quats.len() - 1)]));
            texels
        };
        let list: Vec<usize> = (first..=last.min(frames - 1)).collect();
        let threads = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4).clamp(1, 8);
        let chunk = list.len().div_ceil(threads).max(1);
        let blocks: Vec<(usize, Vec<[f32; 4]>)> = std::thread::scope(|scope| {
            let handles: Vec<_> = list.chunks(chunk).map(|part| scope.spawn(|| part.iter().map(|&f| (f, block_of(f))).collect::<Vec<_>>())).collect();
            handles.into_iter().flat_map(|h| h.join().unwrap_or_default()).collect()
        });
        for (frame, texels) in blocks {
            let base = ((frame / cols) * width + (frame % cols) * block) * 4;
            for (i, v) in texels.iter().enumerate() { data[base + i * 4..base + i * 4 + 4].copy_from_slice(v); }
        }
        // Frame 0's block says how many frames there are, whatever the range
        data[9 * 4 + 3] = frames as f32;
        Ok(Table { width, height, data, block, cols, per_frame })
    }

    pub fn hex(&self) -> String {
        const DIGITS: &[u8; 16] = b"0123456789abcdef";
        let mut s = Vec::with_capacity(self.data.len() * 8);
        for v in &self.data {
            for b in v.to_le_bytes() { s.push(DIGITS[(b >> 4) as usize]); s.push(DIGITS[(b & 15) as usize]); }
        }
        String::from_utf8(s).unwrap()
    }
}

/// The clip's constants for warp.glsl
pub fn defines(clip: &Clip, opts: &Options, table: &Table) -> String {
    let bg = clip.params.background;
    format!(
        "#define SRC_W {}\n#define SRC_H {}\n#define OUT_W {}\n#define OUT_H {}\n#define FPS {:.9}\n#define COLS {}\n#define BLOCK {}\n#define KNOTS {}\n#define FULL_ROWS {}\n#define PROJ {}\n#define INTERP {}\n#define SHRINK {}\n#define BACKGROUND vec4({:.6}, {:.6}, {:.6}, 1.0)\n#define HAS_DIGITAL {}\n#define TIME_OFFSET {:.6}\n#define CHROMATIC {}\n#define CHROMATIC_FAST {}\n#define EDGE_FILL {}\n#define EDGE_BLUR {:.5}\n#define SPHERE_FILL {}",
        clip.size.0, clip.size.1, clip.output_size.0, clip.output_size.1, clip.fps, table.cols, table.block,
        table.per_frame.max(2), (opts.knots == 0) as i32, clip.params.output_projection.max(0), opts.interp, opts.shrink as i32,
        bg[0] / 255.0, bg[1] / 255.0, bg[2] / 255.0, clip.params.digital_lens.is_some() as i32, opts.time_offset,
        (opts.chromatic && opts.mode != Mode::Planes) as i32, opts.chromatic_fast as i32, opts.edge_fill.is_some() as i32, opts.edge_fill.unwrap_or(0.0),
        (opts.sphere && opts.edge_fill.is_some() && opts.mode == Mode::Grid) as i32) + &if opts.mode == Mode::Exact { String::new() } else { format!("\n#define GRID_STEP {}\n#define GRID_LINEAR {}", opts.grid_step, opts.grid_linear as i32) }
}

/// warp.glsl's parts: the file's head (with @PASSES@), the common code, and each pass's body by name
fn sections() -> (&'static str, &'static str, Vec<(&'static str, &'static str)>) {
    let common_at = TEMPLATE.find("// ==== common ====\n").expect("warp.glsl: no common section");
    let head = &TEMPLATE[..common_at];
    let rest = &TEMPLATE[common_at + "// ==== common ====\n".len()..];
    let mut parts = rest.split("// ==== pass ");
    let common = parts.next().unwrap();
    let passes = parts.map(|p| {
        let (name, body) = p.split_once(" ====\n").expect("warp.glsl: a pass without a name");
        (name, body)
    }).collect();
    (head, common, passes)
}

fn pass_body(name: &str) -> &'static str {
    sections().2.into_iter().find(|(n, _)| *n == name).map(|(_, b)| b).unwrap_or_else(|| panic!("warp.glsl: no pass {name}"))
}

/// One hook pass: its directives, the defines, the common code and its body
fn hook_pass(clip: &Clip, directives: &[String], defines: &str, extra: &str, body: &str) -> String {
    let (_, common, _) = sections();
    format!("{}\n\n{defines}\n{extra}\n{}\n{body}\n", directives.join("\n"), common.replace("@LENS@", &lens_code(clip)))
}

/// The denoise's ring of frames: 10 bits a channel (half of rgba16f's memory: 11 slots of 4K)
const FRAME_FORMAT: &str = "rgb10a2";

/// The smooth fill's grid: 135 cells down (edges.rs's, ~8 px a cell at 1080p), across as the output's shape has it
fn fill_grid(clip: &Clip) -> (usize, usize) {
    let h = 135;
    (((h as f64 * clip.output_size.0 as f64 / clip.output_size.1.max(1) as f64).round() as usize).max(1), h)
}

/// The passes for `opts.mode`, without the file's head
pub fn passes(clip: &Clip, opts: &Options, table: &Table) -> String {
    let defs = defines(clip, opts, table);
    let rb = opts.chromatic && opts.mode == Mode::Grid;
    let d = |v: &[&str]| v.iter().map(|s| s.to_string()).filter(|s| !s.is_empty()).collect::<Vec<_>>();
    let rb_bind = if rb { "//!BIND GYROFLOW_GRID_RB" } else { "" };
    let sphere = opts.sphere && opts.edge_fill.is_some() && opts.mode == Mode::Grid;
    let sphere_bind = if sphere { "//!BIND GYROFLOW_SPHERE" } else { "" };
    // The smooth fill's passes at a hook, ahead of the pass that draws it, and what that pass binds and defines
    let fill = |hook: &str, tag: &str, extra: &str, grid: bool| -> (String, String, String) {
        if opts.edge_fill.is_none() { return (String::new(), String::new(), String::new()); }
        let (fw, fh) = fill_grid(clip);
        let mut sizes = vec![(fw, fh)];
        while { let s = *sizes.last().unwrap(); s.0 > 1 || s.1 > 1 } {
            let s = *sizes.last().unwrap();
            sizes.push((s.0.div_ceil(2), s.1.div_ceil(2)));
        }
        let binds = |more: &[&str]| { let mut v = vec![format!("//!HOOK {hook}"), "//!BIND HOOKED".into(), "//!BIND GYROFLOW_TABLE".into()];
            if grid { v.push("//!BIND GYROFLOW_GRID".into()); if rb { v.push("//!BIND GYROFLOW_GRID_RB".into()); } v.push("//!BIND GYROFLOW_GRID_DONE".into()); }
            v.extend(more.iter().map(|s| s.to_string())); v };
        let io = |name: &str, prefix: &str| format!("#define {prefix}_RAW {name}_raw\n#define {prefix}_MUL {name}_mul\n#define {prefix}_SIZE {name}_size");
        let defs = format!("{defs}\n#define FILL_PASS\n#define FILL_W {fw}\n#define FILL_H {fh}");
        let pass = |more: Vec<String>, size: (usize, usize), desc: &str, ex: &str, body: &str| {
            let mut v = binds(&more.iter().map(|s| s.as_str()).collect::<Vec<_>>());
            v.extend([format!("//!WIDTH {}", size.0), format!("//!HEIGHT {}", size.1), "//!WHEN gyroflow_gpu_on".into(), format!("//!DESC gyroflow fill {desc}")]);
            hook_pass(clip, &v, &defs, &format!("{extra}\n{ex}"), pass_body(body))
        };
        let lo = |k: usize| format!("GF{tag}{k}");
        let up = |k: usize| format!("GP{tag}{k}");
        let mut out = vec![pass(vec![format!("//!SAVE {}", lo(0))], sizes[0], "frame", "", "fill_lo")];
        for k in 1..sizes.len() {
            out.push(pass(vec![format!("//!BIND {}", lo(k - 1)), format!("//!SAVE {}", lo(k))], sizes[k], "pull", &io(&lo(k - 1), "IN"), "fill_pull"));
        }
        let top = sizes.len() - 1;
        for k in (0..top).rev() {
            let coarse = if k + 1 == top { lo(top) } else { up(k + 1) };
            out.push(pass(vec![format!("//!BIND {}", lo(k)), format!("//!BIND {coarse}"), format!("//!SAVE {}", up(k))], sizes[k], "push",
                &format!("{}\n{}", io(&lo(k), "IN"), io(&coarse, "COARSE")), "fill_push"));
        }
        let first = if top == 0 { lo(0) } else { up(0) };
        let done = format!("GYROFLOW_FILL{tag}");
        out.push(pass(vec![format!("//!BIND {first}"), format!("//!SAVE GF{tag}H")], sizes[0], "blur", &format!("{}\n#define FILL_DIR ivec2(1, 0)", io(&first, "IN")), "fill_blur"));
        out.push(pass(vec![format!("//!BIND GF{tag}H"), format!("//!SAVE {done}")], sizes[0], "blur", &format!("{}\n#define FILL_DIR ivec2(0, 1)", io(&format!("GF{tag}H"), "IN")), "fill_blur"));
        (out.join("\n"), format!("//!BIND {done}"), format!("#define FILL_RAW {done}_raw\n#define FILL_MUL {done}_mul"))
    };
    let sample = |hook: &str, desc: &str, extra: &str, tag: &str| {
        let (passes, bind, fdefs) = fill(hook, tag, extra, true);
        format!("{passes}\n{}", hook_pass(clip, &d(&[&format!("//!HOOK {hook}"), "//!BIND HOOKED", "//!BIND GYROFLOW_TABLE", "//!BIND GYROFLOW_GRID", rb_bind, sphere_bind, &bind,
            "//!BIND GYROFLOW_GRID_DONE", "//!WIDTH OUTPUT.w", "//!HEIGHT OUTPUT.h", "//!WHEN gyroflow_gpu_on", &format!("//!DESC {desc}")]), &defs, &format!("{extra}\n{fdefs}"), pass_body("sample")))
    };
    let grid = |hook: &str| {
        let s = opts.grid_step;
        let (gw, gh) = (GRID_MAX.0.div_ceil(s) + 4, GRID_MAX.1.div_ceil(s) + 4);
        let texture = |name: &str| format!("//!TEXTURE {name}\n//!SIZE {gw} {gh}\n//!FORMAT rgba32f\n//!STORAGE\n\n");
        format!("{}{}{}", texture("GYROFLOW_GRID"), if rb { texture("GYROFLOW_GRID_RB") } else { String::new() },
            hook_pass(clip, &d(&[&format!("//!HOOK {hook}"), "//!BIND GYROFLOW_TABLE", "//!BIND GYROFLOW_GRID", rb_bind, "//!SAVE GYROFLOW_GRID_DONE",
                &format!("//!WIDTH OUTPUT.w {s} / 4 +"), &format!("//!HEIGHT OUTPUT.h {s} / 4 +"), "//!WHEN gyroflow_gpu_on", "//!COMPUTE 8 8", "//!DESC gyroflow grid"]),
                &defs, "#define GRID_PASS", pass_body("grid")))
    };
    let warp = match opts.mode {
        Mode::Exact => {
            let (passes, bind, fdefs) = fill("MAIN", "M", "", false);
            format!("{passes}\n{}", hook_pass(clip, &d(&["//!HOOK MAIN", "//!BIND HOOKED", "//!BIND GYROFLOW_TABLE", &bind, "//!WIDTH OUTPUT.w", "//!HEIGHT OUTPUT.h",
                "//!WHEN gyroflow_gpu_on", "//!DESC gyroflow warp"]), &defs, &fdefs, pass_body("exact")))
        }
        Mode::Grid => format!("{}\n{}{}", grid("MAIN"), sample("MAIN", "gyroflow warp", "", "M"), if sphere {
            // What the frame covers, written to the sphere by direction after the warp (before the looks, in the same
            // colors the fill reads back)
            format!("\n//!TEXTURE GYROFLOW_SPHERE\n//!SIZE {SPHERE_W} {}\n//!FORMAT rgba32f\n//!STORAGE\n\n{}", SPHERE_W / 2,
                hook_pass(clip, &d(&["//!HOOK MAIN", "//!BIND HOOKED", "//!BIND GYROFLOW_TABLE", "//!BIND GYROFLOW_GRID", rb_bind, "//!BIND GYROFLOW_SPHERE", "//!SAVE GYROFLOW_SPHERE_DONE",
                    "//!WIDTH OUTPUT.w 2 /", "//!HEIGHT OUTPUT.h 2 /", "//!WHEN gyroflow_gpu_on", "//!COMPUTE 8 8", "//!DESC gyroflow sphere"]), &defs, "#define SPHERE_PASS", pass_body("sphere")))
        } else { String::new() }),
        Mode::Planes => format!("{}\n{}\n{}", grid("LUMA"),
            sample("LUMA", "gyroflow warp luma", "#undef BACKGROUND\n#define BACKGROUND vec4(0.0625)\n#define PLANE_LUMA", "L"),
            sample("CHROMA", "gyroflow warp chroma", &format!("#undef BACKGROUND\n#define BACKGROUND vec4(0.5)\n#define PLANE_CHROMA\n#define PLANE_OFFSET vec2(0.25, 0.0){}",
                if opts.chromatic_bilinear_planes() { "\n#undef INTERP\n#define INTERP 0" } else { "" }), "C")),
    };
    if !opts.denoise { return warp; }
    // The temporal denoise after it, at MAIN (RGB in both layouts by then): the shared code binds what it reads
    let common: Vec<&str> = vec!["//!BIND GYROFLOW_TABLE", if opts.mode == Mode::Exact { "" } else { "//!BIND GYROFLOW_GRID" }, rb_bind, sphere_bind];
    let with = |mut v: Vec<&str>| { let at = 2.min(v.len()); for (k, c) in common.iter().enumerate() { v.insert(at + k, c); } d(&v) };
    // The shown frame and DN_RADIUS frames on each side of it: a ring of 2 x DN_RADIUS + 1 slots, two across (a
    // texture's height is at most 16384), each the size of the largest output it takes
    const RADIUS: usize = 5;
    const RING: usize = 2 * RADIUS + 1;
    const COLS: usize = 2;
    const SLOT: (usize, usize) = (3840, 2160);
    let rows = RING.div_ceil(COLS);
    let dn_defs = format!("{defs}\n#define DN_RADIUS {RADIUS}\n#define DN_RING {RING}\n#define DN_COLS {COLS}\n#define DN_SLOT_W {}\n#define DN_SLOT_H {}\n#define DN_PASS", SLOT.0, SLOT.1);
    let dn = |extra: &str, name: &str| format!("{extra}\n{}", pass_body("dn_common")) + pass_body(name);
    let state = format!("//!BUFFER GDN\n//!VAR uint gdn_hist[8192]\n//!VAR float gdn_sigma[32]\n//!VAR vec4 gdn_quat[{RING}]\n//!VAR vec4 gdn_cam[{RING}]\n//!VAR float gdn_zoom[{RING}]\n//!VAR float gdn_pts[{RING}]\n//!VAR vec2 gdn_size\n//!VAR int gdn_head\n//!VAR int gdn_epoch\n//!VAR uint gdn_magic\n//!STORAGE\n\n");
    let texture = |name: &str, w: usize, h: usize, fmt: &str| format!("//!TEXTURE {name}\n//!SIZE {w} {h}\n//!FORMAT {fmt}\n//!STORAGE\n\n");
    let nb = (SLOT.0.div_ceil(32) + 1, SLOT.1.div_ceil(32) + 1);
    let ring = ["//!BIND GDN_FRAMES", "//!BIND GDN_QUART", "//!BIND GDN"];
    let pass = |head: &[&str], tail: &[&str]| with(head.iter().chain(ring.iter()).chain(tail.iter()).copied().collect());
    // After the frames' average: the dark specks, then the guided filter's quarter-size statistics, its a and b, their
    // means, and the filter itself (warp.glsl's dn_clean)
    let q = |save: &str, desc: &str, binds: &[&str]| {
        let mut head = vec!["//!HOOK MAIN", "//!BIND HOOKED"];
        head.extend_from_slice(binds);
        let save = format!("//!SAVE {save}");
        let desc = format!("//!DESC gyroflow denoise {desc}");
        pass(&head, &[save.as_str(), "//!WIDTH HOOKED.w 4 /", "//!HEIGHT HOOKED.h 4 /", "//!COMPONENTS 4", "//!WHEN gyroflow_gpu_on", desc.as_str()])
    };
    let clean = [
        hook_pass(clip, &pass(&["//!HOOK MAIN", "//!BIND HOOKED"], &["//!WHEN gyroflow_gpu_on", "//!DESC gyroflow denoise specks"]),
            &dn_defs, "", &dn("#define DN_SPECKS", "dn_clean")),
        hook_pass(clip, &q("GDN_GF_LO", "filter quarter", &[]), &dn_defs, "", &dn("#define DN_GF_LO", "dn_clean")),
        hook_pass(clip, &q("GDN_GF_AY", "filter luma", &["//!BIND GDN_GF_LO"]), &dn_defs, "", &dn("#define DN_GF_AY", "dn_clean")),
        hook_pass(clip, &q("GDN_GF_AC", "filter color", &["//!BIND GDN_GF_LO"]), &dn_defs, "", &dn("#define DN_GF_AC", "dn_clean")),
        hook_pass(clip, &q("GDN_GF_MY", "filter luma means", &["//!BIND GDN_GF_AY"]), &dn_defs, "", &dn("#define DN_GF_MY", "dn_clean")),
        hook_pass(clip, &q("GDN_GF_MC", "filter color means", &["//!BIND GDN_GF_AC"]), &dn_defs, "", &dn("#define DN_GF_MC", "dn_clean")),
        hook_pass(clip, &pass(&["//!HOOK MAIN", "//!BIND HOOKED", "//!BIND GDN_GF_MY", "//!BIND GDN_GF_MC"], &["//!WHEN gyroflow_gpu_on", "//!DESC gyroflow denoise filter"]),
            &dn_defs, "", &dn("#define DN_GF_APPLY", "dn_clean")),
    ].join("\n");
    format!("{warp}\n{state}{}{}{}\n{}\n{}\n{}\n{}\n{}\n{}\n{clean}",
        texture("GDN_FRAMES", SLOT.0 * COLS, SLOT.1 * rows, FRAME_FORMAT), texture("GDN_QUART", SLOT.0 / 4 * COLS, SLOT.1 / 4 * rows, "rgba16f"),
        texture("GDN_VEC", nb.0, nb.1 * RING, "rgba32f"),
        hook_pass(clip, &with(vec!["//!HOOK MAIN", "//!BIND HOOKED", "//!SAVE GDN_CUR4", "//!WIDTH HOOKED.w 4 /", "//!HEIGHT HOOKED.h 4 /",
            "//!WHEN gyroflow_gpu_on", "//!DESC gyroflow denoise luma"]), &dn_defs, "", pass_body("dn_luma")),
        hook_pass(clip, &pass(&["//!HOOK MAIN", "//!BIND HOOKED", "//!BIND GDN_CUR4"], &["//!SAVE GDN_STORE_DONE", "//!WIDTH OUTPUT.w", "//!HEIGHT OUTPUT.h",
            "//!WHEN gyroflow_gpu_on", "//!COMPUTE 8 8", "//!DESC gyroflow denoise store"]), &dn_defs, "#define DN_COMPUTE", &dn("#define DN_STORE", "dn_store")),
        hook_pass(clip, &pass(&["//!HOOK MAIN", "//!BIND HOOKED", "//!BIND GDN_CUR4", "//!BIND GDN_VEC"], &["//!SAVE GDN_MOTION_DONE", "//!WIDTH OUTPUT.w 32 / 1 +",
            "//!HEIGHT OUTPUT.h 32 / 1 +", "//!WHEN gyroflow_gpu_on", "//!COMPUTE 8 8", "//!DESC gyroflow denoise motion"]),
            &dn_defs, "#define DN_COMPUTE", &dn("#define DN_MOTION", "dn_motion")),
        hook_pass(clip, &pass(&["//!HOOK MAIN", "//!BIND HOOKED", "//!BIND GDN_VEC"], &["//!SAVE GDN_NV", "//!WIDTH OUTPUT.w 32 / 1 +",
            &format!("//!HEIGHT OUTPUT.h 32 / 2 + {} *", 2 * RADIUS), "//!WHEN gyroflow_gpu_on", "//!DESC gyroflow denoise motion chained"]),
            &dn_defs, "#define DN_COMPUTE", &dn("#define DN_COMPOSE", "dn_compose")),
        hook_pass(clip, &pass(&["//!HOOK MAIN", "//!BIND HOOKED", "//!BIND GDN_NV"], &["//!WHEN gyroflow_gpu_on", "//!DESC gyroflow denoise"]),
            &dn_defs, "", &dn("#define DN_BLEND", "dn_blend")),
        hook_pass(clip, &pass(&["//!HOOK MAIN", "//!BIND HOOKED"], &["//!SAVE GDN_STATE_DONE", "//!WIDTH 16", "//!HEIGHT 1", "//!WHEN gyroflow_gpu_on",
            "//!COMPUTE 16 1", "//!DESC gyroflow denoise state"]), &dn_defs, "#define DN_COMPUTE", &dn("#define DN_STATE", "dn_state")))

}

impl Options {
    fn chromatic_bilinear_planes(&self) -> bool { self.chroma_bilinear }
}

/// The shader file mpv loads
pub fn shader(clip: &Clip, opts: &Options, table: &Table) -> String {
    let (head, _, _) = sections();
    head.replace("@TABLE_W@", &table.width.to_string()).replace("@TABLE_H@", &table.height.to_string())
        .replace("@TABLE_HEX@", &table.hex()).replace("@PASSES@", &passes(clip, opts, table))
}

/// The GPU path's 360 mode: a 360 camera's sphere shader (warp.glsl's sphere360 and stamp360 passes) for its lens
/// tracks joined side by side, from what Sphere360 would render it with (sphere360::prepare): per frame each lens's
/// block (its parameters and knots, as a flat clip's) and the stamp's levels after them; each lens's share of every
/// direction (rig.rs's weights) as a texture of a tenth of the sphere's size. Returns the shader and its output's size
/// (the sphere and the stamp's two rows)
pub fn sphere_shader(prep: &crate::sphere360::Prepared360, time_offset: f64) -> Result<(String, (usize, usize)), String> {
    // The stamp's codes at 10 bits (34 of them, in 9 texels), drawn under the sphere as gray, as Sphere360 draws them
    // into the luma: view360.glsl reads them from the RGB sphere (its MAIN stamp pass, gyroflow_stamp360)
    const STAMP_DEPTH: u32 = 10;
    const STAMP_TEXELS: usize = 9;
    let n = prep.rig.lenses.len();
    if n == 0 || n > 4 { return Err(format!("{n} lenses: the sphere shader takes 1 to 4")); }
    let opts = Options { mode: Mode::Exact, grid_step: 8, grid_linear: false, chroma_bilinear: false, chromatic: false, chromatic_fast: false, edge_fill: None,
                         sphere: false, time_offset, denoise: false, knots: 33, interp: 0, shrink: false, range: None };
    let clips: Vec<Clip> = prep.params.iter().zip(&prep.rig.lenses).map(|(p, l)| Clip {
        params: p.clone(), size: (l.rect.2, l.rect.3), output_size: prep.sphere, fps: prep.fps, frame_count: prep.frame_count, frame_offset_us: prep.frame_offset_us,
    }).collect();
    for c in &clips {
        let tr = c.transform(c.frame_count / 2, c.output_size);
        if !tr.mesh_data.is_empty() { return Err("a lens mesh".into()); }
    }
    let tables = clips.iter().map(|c| Table::build(c, &opts)).collect::<Result<Vec<_>, _>>()?;
    let lens_block = tables[0].block;
    let block = n * lens_block + STAMP_TEXELS;
    let frames = prep.frame_count.max(1);
    let cols = (MAX_TEX / block).max(1);
    let (width, height) = (cols * block, frames.div_ceil(cols));
    let mut data = vec![0f32; width * height * 4];
    for f in 0..frames {
        let base = ((f / cols) * width + (f % cols) * block) * 4;
        for (l, t) in tables.iter().enumerate() {
            let src = ((f / t.cols) * t.width + (f % t.cols) * t.block) * 4;
            data[base + l * lens_block * 4..base + (l + 1) * lens_block * 4].copy_from_slice(&t.data[src..src + lens_block * 4]);
        }
        let at = base + n * lens_block * 4;
        for (i, c) in prep.stamp_codes(f, STAMP_DEPTH).into_iter().enumerate() { data[at + i] = c as f32 / ((1u32 << STAMP_DEPTH) - 1) as f32; }
    }
    let table = Table { width, height, data, block, cols, per_frame: opts.knots };

    // Each lens's share, at the middle of each texel (a tenth of the sphere each way), in 255ths
    let (ww, wh) = ((prep.sphere.0 / 10).max(4), (prep.sphere.1 / 10).max(2));
    let mut weights = vec![0u8; ww * wh * 4];
    let (sx, sy) = (prep.sphere.0 as f64 / ww as f64, prep.sphere.1 as f64 / wh as f64);
    for y in 0..wh {
        for x in 0..ww {
            let d = crate::rig::equirect_direction((x as f64 + 0.5) * sx - 0.5, (y as f64 + 0.5) * sy - 0.5, prep.sphere.0, prep.sphere.1);
            for (l, v) in prep.rig.weights(d, prep.distance).into_iter().enumerate().take(4) { weights[(y * ww + x) * 4 + l] = (v * 255.0).round() as u8; }
        }
    }
    let hex: String = weights.iter().map(|b| format!("{b:02x}")).collect();

    let clip = &clips[0];
    let defs = defines(clip, &opts, &table) + &format!("\n#define LENSES {n}\n#define LENS_BLOCK {lens_block}\n#define STAMP_BASE {}\nconst vec2 LENS_OFFSET[{n}] = vec2[]({});",
        n * lens_block, prep.rig.lenses.iter().map(|l| format!("vec2({}.0, {}.0)", l.input as usize * prep.rig.picture.0 + l.rect.0, l.rect.1)).collect::<Vec<_>>().join(", "));
    let (sw, sh) = (prep.sphere.0, prep.sphere.1 + 2);
    let d = |v: &[&str]| v.iter().map(|s| s.to_string()).collect::<Vec<_>>();
    let passes = format!("//!TEXTURE GYROFLOW_WEIGHTS\n//!SIZE {ww} {wh}\n//!FORMAT rgba8\n//!FILTER LINEAR\n{hex}\n\n{}",
        hook_pass(clip, &d(&["//!HOOK MAIN", "//!BIND HOOKED", "//!BIND GYROFLOW_TABLE", "//!BIND GYROFLOW_WEIGHTS", &format!("//!WIDTH {sw}"), &format!("//!HEIGHT {sh}"),
                            "//!WHEN gyroflow_gpu_on", "//!DESC gyroflow 360 sphere"]), &defs, &format!("#undef target_size\n#define target_size vec2({}.0, {}.0)", prep.sphere.0, prep.sphere.1), pass_body("sphere360")));
    let (head, _, _) = sections();
    let text = head.replace("@TABLE_W@", &table.width.to_string()).replace("@TABLE_H@", &table.height.to_string())
        .replace("@TABLE_HEX@", &table.hex()).replace("@PASSES@", &passes);
    Ok((text, (sw, sh)))
}

/// Each row's 3x3 (row-major, `M·d`) as a quaternion (x, y, z, w), signs kept continuous along the rows
pub fn row_quats(m: &[[f32; 16]]) -> Vec<[f32; 4]> {
    let mut out: Vec<[f32; 4]> = Vec::with_capacity(m.len());
    for r in m {
        let mat = Matrix3::new(r[0] as f64, r[1] as f64, r[2] as f64, r[3] as f64, r[4] as f64, r[5] as f64, r[6] as f64, r[7] as f64, r[8] as f64);
        let q = UnitQuaternion::from_rotation_matrix(&Rotation3::from_matrix_unchecked(mat));
        let mut v = [q.i as f32, q.j as f32, q.k as f32, q.w as f32];
        if let Some(p) = out.last() {
            if p[0] * v[0] + p[1] * v[1] + p[2] * v[2] + p[3] * v[3] < 0.0 { v = v.map(|x| -x); }
        }
        out.push(v);
    }
    out
}

/// `knots` quaternions evenly over the rows, knot j at row j·(rows−1)/(knots−1)
pub fn knot_quats(m: &[[f32; 16]], knots: usize) -> Vec<[f32; 4]> {
    let rows = m.len();
    let at = |j: usize| { let y = if rows > 1 { j as f64 * (rows - 1) as f64 / (knots - 1) as f64 } else { 0.0 }; ((y.floor() as usize).min(rows - 1), y) };
    // Only the rows around the knots made quaternions (row_quats' sign kept from one to the next: the rows between turn
    // a frame by a fraction of a degree, so the signs come out as with every row)
    let mut wanted: Vec<usize> = (0..knots).flat_map(|j| { let (a, _) = at(j); [a, (a + 1).min(rows - 1)] }).collect();
    wanted.dedup();
    let picked: Vec<[f32; 16]> = wanted.iter().map(|&r| m[r]).collect();
    let quats = row_quats(&picked);
    let q = |r: usize| quats[wanted.iter().position(|&w| w == r).unwrap_or(0)];
    (0..knots).map(|j| {
        let (a, y) = at(j);
        // Between two rows: their nlerp (a knot falls on a row only when (rows-1) divides evenly)
        nlerp(q(a), q((a + 1).min(rows - 1)), (y - y.floor()) as f32)
    }).collect()
}

/// Crop mode's camera as warp.glsl turns the output ray by it (gyroflow_cam_x..w): gyroflow-core's view rotation in
/// its matrices' axes (y and z turned), which each row's rotation is multiplied by on the right
pub fn camera_quat(cam: &crate::Camera) -> UnitQuaternion<f64> {
    let mut p = ComputeParams::default();
    p.view_rotation = (cam.yaw, cam.pitch, cam.roll);
    let v = gyroflow_core::stabilization::FrameTransform::view_rotation(&p);
    UnitQuaternion::from_quaternion(nalgebra::Quaternion::new(v.w, v.i, -v.j, -v.k))
}

pub fn nlerp(a: [f32; 4], b: [f32; 4], f: f32) -> [f32; 4] {
    let q = [0, 1, 2, 3].map(|i| a[i] + (b[i] - a[i]) * f);
    let l = q.iter().map(|x| x * x).sum::<f32>().sqrt();
    q.map(|x| x / l)
}

/// The rows' matrices rebuilt from `knots` knots as the shader does it (nlerp between the two knots around the row);
/// the other entries (IBIS, OIS, breathing) as they were
pub fn from_knots(m: &[[f32; 16]], knots: usize) -> Vec<[f32; 16]> {
    let k = knot_quats(m, knots);
    let rows = m.len();
    m.iter().enumerate().map(|(y, orig)| {
        let t = if rows > 1 { y as f32 / (rows - 1) as f32 * (knots - 1) as f32 } else { 0.0 };
        let j = (t as usize).min(knots - 2);
        let q = nlerp(k[j], k[j + 1], t - j as f32);
        let uq = UnitQuaternion::from_quaternion(nalgebra::Quaternion::new(q[3] as f64, q[0] as f64, q[1] as f64, q[2] as f64));
        let r = uq.to_rotation_matrix();
        let mut out = *orig;
        for i in 0..3 { for c in 0..3 { out[i * 3 + c] = r.matrix()[(i, c)] as f32; } }
        out
    }).collect()
}

#[cfg(test)]
mod tests {
    // warp.glsl against gyroflow-core's CPU kernel: the shader's passes (through naga's GLSL front end, on this
    // machine's GPU, no window) write each output pixel's source coordinate, compared with
    // Stabilization::undistort_coord at every pixel. On a real clip: AFP_GPU_CLIP=<project.gyroflow> cargo test
    // --release gpu_table -- --nocapture (skipped without it). A synthetic readout time, so the rolling shutter's path
    // runs too. What libplacebo declares is declared here around each pass's text, as view360.rs's test does.
    use super::*;
    use gyroflow_core::stabilization::Stabilization;
    use nalgebra::Vector2;

    const VS: &str = "@vertex fn vs(@builtin(vertex_index) i: u32) -> @builtin(position) vec4<f32> {
        let p = vec2<f32>(f32((i << 1u) & 2u), f32(i & 2u));
        return vec4<f32>(p * 2.0 - 1.0, 0.0, 1.0);
    }";

    #[repr(C)]
    #[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
    struct U { hooked_size: [f32; 2], out_size: [f32; 2], pts: f32, on: i32, cam: [f32; 4], zoom: f32, _pad: f32 }

    const GRID_TEX: (u32, u32) = (600, 340);

    /// Pass `name` as a shader for naga: "grid" a compute shader, the others fragment shaders
    fn glsl(clip: &Clip, opts: &Options, table: &Table, name: &str, uv_out: bool) -> String {
        let compute = name == "grid" || name == "sphere";
        let mut defs = defines(clip, opts, table);
        if name == "grid" { defs += "\n#define GRID_PASS"; }
        if name == "sphere" { defs += "\n#define SPHERE_PASS"; }
        // The smooth fill's textures (its passes' own, and the grid they leave), as one more texture
        if name.starts_with("fill") { defs += "\n#define FILL_PASS\n#define FILL_W 240\n#define FILL_H 135\n#define FILL_DIR ivec2(1, 0)"; }
        if uv_out && std::env::var("AFP_GPU_STRICT").is_ok() { defs += "\n#define GRID_STRICT"; }
        if uv_out { defs += "\n#define UV_OUT 1\n#undef BACKGROUND\n#define BACKGROUND vec4(-1.0e6)"; }
        let head = format!("#version 450
{}
layout(set = 0, binding = 0, std140) uniform Params {{ vec2 hooked_size; vec2 out_size; float PTS; int gyroflow_gpu_on; float gyroflow_cam_x; float gyroflow_cam_y; float gyroflow_cam_z; float gyroflow_cam_w; float gyroflow_cam_zoom; }};
layout(set = 0, binding = 1) uniform texture2D hooked_t;
layout(set = 0, binding = 2) uniform sampler hooked_s;
layout(set = 0, binding = 3) uniform texture2D table_t;
layout(set = 0, binding = 4) uniform sampler table_s;
layout(set = 0, binding = 5, rgba32f) uniform {} image2D grid_img;
layout(set = 0, binding = 6, rgba32f) uniform {} image2D grid_rb_img;
layout(set = 0, binding = 7, rgba32f) uniform image2D sphere_img;
#define GYROFLOW_SPHERE sphere_img
#define GYROFLOW_GRID grid_img
#define GYROFLOW_GRID_RB grid_rb_img
#define GYROFLOW_TABLE sampler2D(table_t, table_s)
#define target_size out_size
layout(set = 0, binding = 8) uniform texture2D fill_t;
layout(set = 0, binding = 9) uniform sampler fill_s;
#define FILL_RAW sampler2D(fill_t, fill_s)
#define FILL_MUL 1.0
#define IN_RAW sampler2D(fill_t, fill_s)
#define IN_MUL 1.0
#define IN_SIZE vec2(240.0, 135.0)
#define COARSE_RAW sampler2D(fill_t, fill_s)
#define COARSE_MUL 1.0
#define COARSE_SIZE vec2(120.0, 68.0)
{}",
            if compute { "layout(local_size_x = 8, local_size_y = 8) in;" } else { "layout(location = 0) out vec4 o_color;" },
            if name == "grid" { "writeonly" } else { "readonly" },
            if name == "grid" { "writeonly" } else { "readonly" },
            // The grid pass binds no HOOKED in mpv: none here either
            if name == "grid" { "" } else { "#define HOOKED_raw sampler2D(hooked_t, hooked_s)\n#define HOOKED_mul 1.0\n#define HOOKED_tex(pos) texture(sampler2D(hooked_t, hooked_s), pos)\n#define HOOKED_size hooked_size\n#define HOOKED_pt (1.0 / hooked_size)\n#define HOOKED_pos (gl_FragCoord.xy / out_size)\n" });
        let (_, common, _) = sections();
        let common = common.replace("@LENS@", &lens_code(clip));
        let main = if compute { "void main() { hook(); }" } else { "void main() { o_color = hook(); }" };
        format!("{head}\n{defs}\n{common}\n{}\n{main}\n", pass_body(name))
    }

    struct Gpu { device: wgpu::Device, queue: wgpu::Queue }

    fn gpu() -> Gpu {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default())).expect("no GPU");
        let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor { required_limits: adapter.limits(), ..Default::default() })).expect("no device");
        Gpu { device, queue }
    }

    fn texture(g: &Gpu, w: usize, h: usize, data: Option<&[f32]>, usage: wgpu::TextureUsages) -> wgpu::Texture {
        let t = g.device.create_texture(&wgpu::TextureDescriptor {
            label: None, size: wgpu::Extent3d { width: w as u32, height: h as u32, depth_or_array_layers: 1 }, mip_level_count: 1, sample_count: 1,
            dimension: wgpu::TextureDimension::D2, format: wgpu::TextureFormat::Rgba32Float, usage: usage | wgpu::TextureUsages::COPY_DST, view_formats: &[],
        });
        if let Some(data) = data {
            g.queue.write_texture(wgpu::TexelCopyTextureInfo { texture: &t, mip_level: 0, origin: wgpu::Origin3d::ZERO, aspect: wgpu::TextureAspect::All },
                bytemuck::cast_slice(data), wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some((w * 16) as u32), rows_per_image: Some(h as u32) },
                wgpu::Extent3d { width: w as u32, height: h as u32, depth_or_array_layers: 1 });
        }
        t
    }

    fn module(g: &Gpu, src: &str, stage: wgpu::naga::ShaderStage) -> wgpu::ShaderModule {
        g.device.create_shader_module(wgpu::ShaderModuleDescriptor { label: Some("warp"), source: wgpu::ShaderSource::Glsl { shader: src.into(), stage, defines: &[] } })
    }

    fn layout(g: &Gpu, vis: wgpu::ShaderStages, access: wgpu::StorageTextureAccess) -> wgpu::BindGroupLayout {
        let tex = |b: u32| wgpu::BindGroupLayoutEntry { binding: b, visibility: vis, ty: wgpu::BindingType::Texture { sample_type: wgpu::TextureSampleType::Float { filterable: false }, view_dimension: wgpu::TextureViewDimension::D2, multisampled: false }, count: None };
        let smp = |b: u32| wgpu::BindGroupLayoutEntry { binding: b, visibility: vis, ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::NonFiltering), count: None };
        g.device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor { label: None, entries: &[
            wgpu::BindGroupLayoutEntry { binding: 0, visibility: vis, ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Uniform, has_dynamic_offset: false, min_binding_size: None }, count: None },
            tex(1), smp(2), tex(3), smp(4),
            wgpu::BindGroupLayoutEntry { binding: 5, visibility: vis, ty: wgpu::BindingType::StorageTexture { access, format: wgpu::TextureFormat::Rgba32Float, view_dimension: wgpu::TextureViewDimension::D2 }, count: None },
            wgpu::BindGroupLayoutEntry { binding: 6, visibility: vis, ty: wgpu::BindingType::StorageTexture { access, format: wgpu::TextureFormat::Rgba32Float, view_dimension: wgpu::TextureViewDimension::D2 }, count: None }] })
    }

    /// Each output pixel's (u, v) as the shader computes it, out.0 x out.1, for the frame at `pts`: the grid pass first
    /// when `grid` is given, then `fs` (a fragment pass's GLSL)
    fn render_uv(g: &Gpu, grid: Option<&str>, fs: &str, table: &Table, src_size: (usize, usize), out: (usize, usize), pts: f32, cam: Option<&crate::Camera>) -> Vec<[f32; 4]> {
        let d = &g.device;
        let hooked = texture(g, 4, 4, Some(&[0.5; 64]), wgpu::TextureUsages::TEXTURE_BINDING);
        let table_tex = texture(g, table.width, table.height, Some(&table.data), wgpu::TextureUsages::TEXTURE_BINDING);
        let grid_tex = texture(g, GRID_TEX.0 as usize, GRID_TEX.1 as usize, None, wgpu::TextureUsages::STORAGE_BINDING);
        let grid_rb_tex = texture(g, GRID_TEX.0 as usize, GRID_TEX.1 as usize, None, wgpu::TextureUsages::STORAGE_BINDING);
        let q = cam.map(camera_quat).unwrap_or_else(UnitQuaternion::identity);
        let u = U { hooked_size: [src_size.0 as f32, src_size.1 as f32], out_size: [out.0 as f32, out.1 as f32], pts, on: 1,
                    cam: [q.i as f32, q.j as f32, q.k as f32, q.w as f32], zoom: cam.map(|c| c.zoom as f32).unwrap_or(1.0), _pad: 0.0 };
        let buf = d.create_buffer(&wgpu::BufferDescriptor { label: None, size: std::mem::size_of::<U>() as u64, usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false });
        g.queue.write_buffer(&buf, 0, bytemuck::bytes_of(&u));
        let sampler = d.create_sampler(&wgpu::SamplerDescriptor::default());
        let group = |l: &wgpu::BindGroupLayout| d.create_bind_group(&wgpu::BindGroupDescriptor { label: None, layout: l, entries: &[
            wgpu::BindGroupEntry { binding: 0, resource: buf.as_entire_binding() },
            wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::TextureView(&hooked.create_view(&Default::default())) },
            wgpu::BindGroupEntry { binding: 2, resource: wgpu::BindingResource::Sampler(&sampler) },
            wgpu::BindGroupEntry { binding: 3, resource: wgpu::BindingResource::TextureView(&table_tex.create_view(&Default::default())) },
            wgpu::BindGroupEntry { binding: 4, resource: wgpu::BindingResource::Sampler(&sampler) },
            wgpu::BindGroupEntry { binding: 5, resource: wgpu::BindingResource::TextureView(&grid_tex.create_view(&Default::default())) },
            wgpu::BindGroupEntry { binding: 6, resource: wgpu::BindingResource::TextureView(&grid_rb_tex.create_view(&Default::default())) }] });
        let mut enc = d.create_command_encoder(&Default::default());
        if let Some(src) = grid {
            let l = layout(g, wgpu::ShaderStages::COMPUTE, wgpu::StorageTextureAccess::WriteOnly);
            let pl = d.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor { label: None, bind_group_layouts: &[Some(&l)], immediate_size: 0 });
            let cs = module(g, src, wgpu::naga::ShaderStage::Compute);
            let p = d.create_compute_pipeline(&wgpu::ComputePipelineDescriptor { label: None, layout: Some(&pl), module: &cs, entry_point: Some("main"), compilation_options: Default::default(), cache: None });
            let mut pass = enc.begin_compute_pass(&Default::default());
            pass.set_pipeline(&p);
            pass.set_bind_group(0, &group(&l), &[]);
            pass.dispatch_workgroups(GRID_TEX.0.div_ceil(8), GRID_TEX.1.div_ceil(8), 1);
        }
        let l = layout(g, wgpu::ShaderStages::FRAGMENT, wgpu::StorageTextureAccess::ReadOnly);
        let pl = d.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor { label: None, bind_group_layouts: &[Some(&l)], immediate_size: 0 });
        let vs = d.create_shader_module(wgpu::ShaderModuleDescriptor { label: Some("vs"), source: wgpu::ShaderSource::Wgsl(VS.into()) });
        let fsm = module(g, fs, wgpu::naga::ShaderStage::Fragment);
        let pipeline = d.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: None, layout: Some(&pl),
            vertex: wgpu::VertexState { module: &vs, entry_point: Some("vs"), buffers: &[], compilation_options: Default::default() },
            fragment: Some(wgpu::FragmentState { module: &fsm, entry_point: Some("main"), targets: &[Some(wgpu::ColorTargetState { format: wgpu::TextureFormat::Rgba32Float, blend: None, write_mask: wgpu::ColorWrites::ALL })], compilation_options: Default::default() }),
            primitive: Default::default(), depth_stencil: None, multisample: Default::default(), multiview_mask: None, cache: None,
        });
        let target = d.create_texture(&wgpu::TextureDescriptor {
            label: None, size: wgpu::Extent3d { width: out.0 as u32, height: out.1 as u32, depth_or_array_layers: 1 }, mip_level_count: 1, sample_count: 1,
            dimension: wgpu::TextureDimension::D2, format: wgpu::TextureFormat::Rgba32Float, usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC, view_formats: &[],
        });
        let row = (out.0 * 16).div_ceil(256) * 256;
        let back = d.create_buffer(&wgpu::BufferDescriptor { label: None, size: (row * out.1) as u64, usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ, mapped_at_creation: false });
        {
            let view = target.create_view(&Default::default());
            let mut pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: None,
                color_attachments: &[Some(wgpu::RenderPassColorAttachment { view: &view, depth_slice: None, resolve_target: None, ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color::BLACK), store: wgpu::StoreOp::Store } })],
                ..Default::default()
            });
            pass.set_pipeline(&pipeline);
            pass.set_bind_group(0, &group(&l), &[]);
            pass.draw(0..3, 0..1);
        }
        enc.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo { texture: &target, mip_level: 0, origin: wgpu::Origin3d::ZERO, aspect: wgpu::TextureAspect::All },
            wgpu::TexelCopyBufferInfo { buffer: &back, layout: wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(row as u32), rows_per_image: Some(out.1 as u32) } },
            wgpu::Extent3d { width: out.0 as u32, height: out.1 as u32, depth_or_array_layers: 1 });
        g.queue.submit([enc.finish()]);
        let slice = back.slice(..);
        slice.map_async(wgpu::MapMode::Read, |_| ());
        d.poll(wgpu::PollType::wait_indefinitely()).unwrap();
        let data = slice.get_mapped_range().unwrap();
        (0..out.1).flat_map(|y| bytemuck::cast_slice::<u8, [f32; 4]>(&data[y * row..y * row + out.0 * 16]).to_vec()).collect()
    }

    #[test]
    fn the_camera_is_a_rotation_after_the_rows() {
        // Crop mode's camera (view_rotation): each row's rotation with it is the row's without it times C, C the view
        // rotation in the matrices' axes (y and z turned: i V i^-1), so the shader turns the output ray by C first;
        // the output camera in the world (the sphere trail's) times C too; the zoom is the FOV's
        let Ok(projects) = std::env::var("AFP_GPU_CLIP") else { eprintln!("AFP_GPU_CLIP not set: skipped"); return; };
        let project = projects.split(';').next().unwrap();
        let clip = Clip::load(project, Some(10.0)).unwrap();
        let cam = crate::Camera { yaw: 12.0, pitch: -7.0, roll: 4.0, zoom: 1.25 };
        let turned = Clip { params: cam.apply(&clip.params), size: clip.size, output_size: clip.output_size, fps: clip.fps, frame_count: clip.frame_count, frame_offset_us: clip.frame_offset_us };
        let c = camera_quat(&cam);
        let q = |m: &[f32; 16]| {
            let mat = Matrix3::new(m[0] as f64, m[1] as f64, m[2] as f64, m[3] as f64, m[4] as f64, m[5] as f64, m[6] as f64, m[7] as f64, m[8] as f64);
            UnitQuaternion::from_rotation_matrix(&Rotation3::from_matrix_unchecked(mat))
        };
        for f in [0usize, clip.frame_count / 2] {
            let (a, b) = (clip.transform(f, clip.output_size), turned.transform(f, turned.output_size));
            assert!((b.kernel_params.fov / a.kernel_params.fov - 1.25).abs() < 1e-4, "fov {} -> {}", a.kernel_params.fov, b.kernel_params.fov);
            for r in [0usize, a.matrices.len() / 2, a.matrices.len() - 1] {
                let err = (q(&a.matrices[r]) * c).angle_to(&q(&b.matrices[r])).to_degrees();
                assert!(err < 0.001, "frame {f} row {r}: {err}°");
            }
            let ts = (f as f64 * 1e6 / clip.fps) as i64 + clip.frame_offset_us;
            let w = |p: &ComputeParams| { let m: Matrix3<f64> = nalgebra::convert(crate::sphere::View::at(p, ts).rotation()); UnitQuaternion::from_rotation_matrix(&Rotation3::from_matrix_unchecked(m)) };
            let err = (w(&clip.params) * c).angle_to(&w(&turned.params)).to_degrees();
            assert!(err < 0.001, "frame {f}: the world's {err}°");
        }
    }

    #[test]
    fn the_shader_maps_pixels_as_gyroflow_does() {
        let Ok(projects) = std::env::var("AFP_GPU_CLIP") else { eprintln!("AFP_GPU_CLIP not set: skipped"); return; };
        let g = gpu();
        // Clips separated by ';' (a project, or a video with its own motion data)
        for project in projects.split(';') {
        let clip = Clip::load(project, Some(10.0)).unwrap();
        eprintln!("{project}: lens {}{}", clip.params.distortion_model.id(), clip.params.digital_lens.as_ref().map(|d| format!(" + {}", d.id())).unwrap_or_default());
        assert_eq!(unsupported(&clip), None);
        let out = (1920usize, 1080usize);
        let n = clip.frame_count;
        let mut frames: Vec<usize> = [0usize, 600, 1049, 1181, n / 2, n - 1].into_iter().filter(|f| *f < n).collect();
        frames.dedup();
        let fast = if n > 1049 { 1049 } else { n / 2 };
        let base = Options { mode: Mode::Exact, grid_step: 16, grid_linear: false, chroma_bilinear: false, chromatic: false, chromatic_fast: false, edge_fill: None, sphere: false, time_offset: 0.0, denoise: false, knots: 33, interp: 2, shrink: false, range: None };
        // (label, options, every frame or only the fastest)
        let cam = crate::Camera { yaw: 12.0, pitch: -7.0, roll: 4.0, zoom: 1.25 };
        let turned = Clip { params: cam.apply(&clip.params), size: clip.size, output_size: clip.output_size, fps: clip.fps, frame_count: clip.frame_count, frame_offset_us: clip.frame_offset_us };
        let cases = [
            ("exact, 33 knots", base, true),
            ("exact, every row", Options { knots: 0, range: Some((fast, fast)), ..base }, false),
            ("grid 16, 33 knots", Options { mode: Mode::Grid, ..base }, true),
            ("grid 32, 33 knots", Options { mode: Mode::Grid, grid_step: 32, ..base }, false),
            ("grid 8 bilinear, 33 knots", Options { mode: Mode::Grid, grid_step: 8, grid_linear: true, ..base }, false),
        ];
        // Crop mode's camera, as shader parameters on the table without it, against Gyroflow with it
        let with_camera = [("exact, 33 knots, a camera", base), ("grid 8 bilinear, 33 knots, a camera", Options { mode: Mode::Grid, grid_step: 8, grid_linear: true, ..base })];
        let cases = cases.into_iter().map(|(l, o, a)| (l, o, a, false)).chain(with_camera.into_iter().map(|(l, o)| (l, o, false, true)));
        for (label, opts, all, camera) in cases {
            // The full rows only for the frames compared: the whole clip would be 5 M texels
            let table = Table::build(&clip, &opts).unwrap();
            let reference = if camera { &turned } else { &clip };
            let (grid, fs) = match opts.mode {
                Mode::Exact => (None, glsl(&clip, &opts, &table, "exact", true)),
                _ => (Some(glsl(&clip, &opts, &table, "grid", true)), glsl(&clip, &opts, &table, "sample", true)),
            };
            for &frame in &frames {
                if !all && frame != fast { continue; }
                let pts = (frame as f64 / clip.fps) as f32;
                let uv = render_uv(&g, grid.as_deref(), &fs, &table, clip.size, out, pts, camera.then_some(&cam));
                // Gyroflow at the same pixels, into a buffer of the output's size (the filter's dst)
                let tr = reference.transform(frame, out);
                let kp = &tr.kernel_params;
                let out_c = Vector2::new(kp.output_width as f32 / 2.0, kp.output_height as f32 / 2.0);
                let f = gyroflow_core::stabilization::projection::output_focal(kp);
                let out_f = Vector2::new(f.0, f.1);
                let (mut worst, mut sum, mut n, mut mismatched) = (0f32, 0f64, 0usize, 0usize);
                for y in 0..out.1 {
                    for x in 0..out.0 {
                        let s = uv[y * out.0 + x];
                        let c = Stabilization::undistort_coord(Vector2::new(x as f32, y as f32), kp, &tr.matrices, &clip.params.distortion_model, None, &[], &out_c, &out_f);
                        let inside = |p: Vector2<f32>| p.x >= 0.0 && p.y >= 0.0 && p.x <= clip.size.0 as f32 - 1.0 && p.y <= clip.size.1 as f32 - 1.0;
                        if let Some(c) = c.filter(|c| inside(*c)) {
                            if s[0] < -1000.0 { mismatched += 1; continue; }
                            let e = (Vector2::new(s[0], s[1]) - c).norm();
                            worst = worst.max(e);
                            sum += e as f64;
                            n += 1;
                        }
                    }
                }
                eprintln!("frame {frame}, {label}: {n} pixels, worst {worst:.4} px, mean {:.5} px, {mismatched} the shader left out", sum / n.max(1) as f64);
                assert!(n > out.0 * out.1 / 2, "frame {frame}: too few pixels compared");
                // The bilinear grid with a camera zoomed out (wider than the clip, more of the lens's curve): up to 0.2 px
                assert!(worst < if camera && opts.grid_linear { 0.25 } else { 0.1 }, "frame {frame}, {label}: the shader is {worst} px off Gyroflow");
                assert!(mismatched < out.0, "frame {frame}: {mismatched} pixels the shader left out");
            }
        }
        // Every interpolation compiles in every pass (the picture, not the coordinates)
        let table = Table::build(&clip, &Options { range: Some((0, 0)), ..base }).unwrap();
        for interp in 0..6 {
            for shrink in [false, true] {
                for (mode, name, chromatic, fill, plane) in [(Mode::Exact, "exact", false, None, ""), (Mode::Grid, "sample", false, None, ""), (Mode::Grid, "grid", false, None, ""),
                    (Mode::Exact, "exact", true, Some(0.02), ""), (Mode::Grid, "sample", true, Some(0.02), ""), (Mode::Grid, "grid", true, Some(0.02), ""),
                    (Mode::Planes, "sample", false, Some(0.02), "#define PLANE_LUMA\n"),
                    (Mode::Grid, "sample", true, Some(0.019), "SPHERE"), (Mode::Grid, "sphere", true, Some(0.019), "SPHERE"), (Mode::Planes, "sample", false, Some(0.02), "#define PLANE_CHROMA\n#define PLANE_OFFSET vec2(0.25, 0.0)\n"),
                    (Mode::Grid, "fill_lo", true, Some(0.02), ""), (Mode::Planes, "fill_lo", false, Some(0.02), "#define PLANE_CHROMA\n#define PLANE_OFFSET vec2(0.25, 0.0)\n"),
                    (Mode::Exact, "fill_lo", false, Some(0.02), ""), (Mode::Grid, "fill_pull", false, Some(0.02), ""), (Mode::Grid, "fill_push", false, Some(0.02), ""),
                    (Mode::Grid, "fill_blur", false, Some(0.02), "")] {
                    let opts = Options { mode, interp, shrink, chromatic, edge_fill: fill, sphere: plane == "SPHERE", grid_linear: interp % 2 == 0, ..base };
                    let plane = if plane == "SPHERE" { "" } else { plane };
                    let src = glsl(&clip, &opts, &table, name, false).replacen("#version 450\n", &format!("#version 450\n{plane}"), 1);
                    let scope = g.device.push_error_scope(wgpu::ErrorFilter::Validation);
                    module(&g, &src, if name == "grid" || name == "sphere" { wgpu::naga::ShaderStage::Compute } else { wgpu::naga::ShaderStage::Fragment });
                    let err = pollster::block_on(scope.pop());
                    assert!(err.is_none(), "INTERP {interp}, SHRINK {shrink}, pass {name}: {err:?}");
                }
            }
        }
        }
    }
}
