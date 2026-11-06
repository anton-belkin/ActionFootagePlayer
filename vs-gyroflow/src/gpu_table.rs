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
pub const PARAM_TEXELS: usize = 5;
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

/// The largest output the grid texture is made for
const GRID_MAX: (usize, usize) = (8192, 4608);

#[derive(Clone, Copy, Debug)]
pub struct Options {
    pub mode: Mode,
    /// The grid's step in output pixels
    pub grid_step: usize,
    /// The grid interpolated bilinearly (4 reads) rather than Catmull-Rom (16)
    pub grid_linear: bool,
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
        let cols = (MAX_TEX / block).max(1).min(frames);
        let (width, height) = (cols * block, frames.div_ceil(cols));
        let mut data = vec![0f32; width * height * 4];
        let (first, last) = opts.range.unwrap_or((0, frames - 1));
        for frame in first..=last.min(frames - 1) {
            let tr = clip.transform(frame, clip.output_size);
            let kp = &tr.kernel_params;
            let base = ((frame / cols) * width + (frame % cols) * block) * 4;
            let texels: [[f32; 4]; PARAM_TEXELS] = [
                [kp.f[0], kp.f[1], kp.c[0], kp.c[1]],
                [kp.k[0], kp.k[1], kp.k[2], kp.k[3]],
                [kp.fov, kp.lens_correction_amount, kp.light_refraction_coefficient, kp.field_limit],
                [kp.translation2d[0], kp.translation2d[1], kp.matrix_count as f32, 1.0],
                [kp.translation3d[0], kp.translation3d[1], kp.translation3d[2], 0.0],
            ];
            for (i, v) in texels.iter().enumerate() { data[base + i * 4..base + i * 4 + 4].copy_from_slice(v); }
            let quats = if opts.knots == 0 { row_quats(&tr.matrices) } else { knot_quats(&tr.matrices, per_frame) };
            for i in 0..per_frame {
                let at = base + (PARAM_TEXELS + i) * 4;
                data[at..at + 4].copy_from_slice(&quats[i.min(quats.len() - 1)]);
            }
        }
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
        "#define SRC_W {}\n#define SRC_H {}\n#define OUT_W {}\n#define OUT_H {}\n#define FPS {:.9}\n#define FRAMES {}\n#define COLS {}\n#define BLOCK {}\n#define KNOTS {}\n#define FULL_ROWS {}\n#define PROJ {}\n#define INTERP {}\n#define SHRINK {}\n#define BACKGROUND vec4({:.6}, {:.6}, {:.6}, 1.0)",
        clip.size.0, clip.size.1, clip.output_size.0, clip.output_size.1, clip.fps, clip.frame_count.max(1), table.cols, table.block,
        table.per_frame.max(2), (opts.knots == 0) as i32, clip.params.output_projection.max(0), opts.interp, opts.shrink as i32,
        bg[0] / 255.0, bg[1] / 255.0, bg[2] / 255.0) + &if opts.mode == Mode::Exact { String::new() } else { format!("\n#define GRID_STEP {}\n#define GRID_LINEAR {}", opts.grid_step, opts.grid_linear as i32) }
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
fn hook_pass(directives: &[String], defines: &str, extra: &str, body: &str) -> String {
    let (_, common, _) = sections();
    format!("{}\n\n{defines}\n{extra}\n{common}\n{body}\n", directives.join("\n"))
}

/// The passes for `opts.mode`, without the file's head
pub fn passes(clip: &Clip, opts: &Options, table: &Table) -> String {
    let defs = defines(clip, opts, table);
    let d = |v: &[&str]| v.iter().map(|s| s.to_string()).collect::<Vec<_>>();
    let sample = |hook: &str, desc: &str, extra: &str| hook_pass(&d(&[&format!("//!HOOK {hook}"), "//!BIND HOOKED", "//!BIND GYROFLOW_TABLE", "//!BIND GYROFLOW_GRID",
        "//!BIND GYROFLOW_GRID_DONE", "//!WIDTH OUTPUT.w", "//!HEIGHT OUTPUT.h", "//!WHEN gyroflow_gpu_on", &format!("//!DESC {desc}")]), &defs, extra, pass_body("sample"));
    let grid = |hook: &str| {
        let s = opts.grid_step;
        let (gw, gh) = (GRID_MAX.0.div_ceil(s) + 4, GRID_MAX.1.div_ceil(s) + 4);
        format!("//!TEXTURE GYROFLOW_GRID\n//!SIZE {gw} {gh}\n//!FORMAT rgba32f\n//!STORAGE\n\n{}",
            hook_pass(&d(&[&format!("//!HOOK {hook}"), "//!BIND GYROFLOW_TABLE", "//!BIND GYROFLOW_GRID", "//!SAVE GYROFLOW_GRID_DONE",
                &format!("//!WIDTH OUTPUT.w {s} / 4 +"), &format!("//!HEIGHT OUTPUT.h {s} / 4 +"), "//!WHEN gyroflow_gpu_on", "//!COMPUTE 8 8", "//!DESC gyroflow grid"]),
                &defs, "#define GRID_PASS", pass_body("grid")))
    };
    match opts.mode {
        Mode::Exact => hook_pass(&d(&["//!HOOK MAIN", "//!BIND HOOKED", "//!BIND GYROFLOW_TABLE", "//!WIDTH OUTPUT.w", "//!HEIGHT OUTPUT.h",
            "//!WHEN gyroflow_gpu_on", "//!DESC gyroflow warp"]), &defs, "", pass_body("exact")),
        Mode::Grid => format!("{}\n{}", grid("MAIN"), sample("MAIN", "gyroflow warp", "")),
        Mode::Planes => format!("{}\n{}\n{}", grid("LUMA"),
            sample("LUMA", "gyroflow warp luma", "#undef BACKGROUND\n#define BACKGROUND vec4(0.0625)"),
            sample("CHROMA", "gyroflow warp chroma", "#undef BACKGROUND\n#define BACKGROUND vec4(0.5)\n#define PLANE_OFFSET vec2(0.25, 0.0)")),
    }
}

/// The shader file mpv loads
pub fn shader(clip: &Clip, opts: &Options, table: &Table) -> String {
    let (head, _, _) = sections();
    head.replace("@TABLE_W@", &table.width.to_string()).replace("@TABLE_H@", &table.height.to_string())
        .replace("@TABLE_HEX@", &table.hex()).replace("@PASSES@", &passes(clip, opts, table))
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
    let all = row_quats(m);
    let rows = all.len();
    (0..knots).map(|j| {
        let y = if rows > 1 { j as f64 * (rows - 1) as f64 / (knots - 1) as f64 } else { 0.0 };
        // Between two rows: their nlerp (a knot falls on a row only when (rows-1) divides evenly)
        let a = (y.floor() as usize).min(rows - 1);
        nlerp(all[a], all[(a + 1).min(rows - 1)], (y - y.floor()) as f32)
    }).collect()
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
    struct U { hooked_size: [f32; 2], out_size: [f32; 2], pts: f32, on: i32, _pad: [f32; 2] }

    const GRID_TEX: (u32, u32) = (600, 340);

    /// Pass `name` as a shader for naga: "grid" a compute shader, the others fragment shaders
    fn glsl(clip: &Clip, opts: &Options, table: &Table, name: &str, uv_out: bool) -> String {
        let compute = name == "grid";
        let mut defs = defines(clip, opts, table);
        if compute { defs += "\n#define GRID_PASS"; }
        if uv_out && std::env::var("AFP_GPU_STRICT").is_ok() { defs += "\n#define GRID_STRICT"; }
        if uv_out { defs += "\n#define UV_OUT 1\n#undef BACKGROUND\n#define BACKGROUND vec4(-1.0e6)"; }
        let head = format!("#version 450
{}
layout(set = 0, binding = 0, std140) uniform Params {{ vec2 hooked_size; vec2 out_size; float PTS; int gyroflow_gpu_on; }};
layout(set = 0, binding = 1) uniform texture2D hooked_t;
layout(set = 0, binding = 2) uniform sampler hooked_s;
layout(set = 0, binding = 3) uniform texture2D table_t;
layout(set = 0, binding = 4) uniform sampler table_s;
layout(set = 0, binding = 5, rgba32f) uniform {} image2D grid_img;
#define GYROFLOW_GRID grid_img
#define GYROFLOW_TABLE sampler2D(table_t, table_s)
#define target_size out_size
{}",
            if compute { "layout(local_size_x = 8, local_size_y = 8) in;" } else { "layout(location = 0) out vec4 o_color;" },
            if compute { "writeonly" } else { "readonly" },
            // The grid pass binds no HOOKED in mpv: none here either
            if compute { "" } else { "#define HOOKED_tex(pos) texture(sampler2D(hooked_t, hooked_s), pos)\n#define HOOKED_size hooked_size\n#define HOOKED_pt (1.0 / hooked_size)\n#define HOOKED_pos (gl_FragCoord.xy / out_size)\n" });
        let (_, common, _) = sections();
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
            wgpu::BindGroupLayoutEntry { binding: 5, visibility: vis, ty: wgpu::BindingType::StorageTexture { access, format: wgpu::TextureFormat::Rgba32Float, view_dimension: wgpu::TextureViewDimension::D2 }, count: None }] })
    }

    /// Each output pixel's (u, v) as the shader computes it, out.0 x out.1, for the frame at `pts`: the grid pass first
    /// when `grid` is given, then `fs` (a fragment pass's GLSL)
    fn render_uv(g: &Gpu, grid: Option<&str>, fs: &str, table: &Table, src_size: (usize, usize), out: (usize, usize), pts: f32) -> Vec<[f32; 4]> {
        let d = &g.device;
        let hooked = texture(g, 4, 4, Some(&[0.5; 64]), wgpu::TextureUsages::TEXTURE_BINDING);
        let table_tex = texture(g, table.width, table.height, Some(&table.data), wgpu::TextureUsages::TEXTURE_BINDING);
        let grid_tex = texture(g, GRID_TEX.0 as usize, GRID_TEX.1 as usize, None, wgpu::TextureUsages::STORAGE_BINDING);
        let u = U { hooked_size: [src_size.0 as f32, src_size.1 as f32], out_size: [out.0 as f32, out.1 as f32], pts, on: 1, _pad: [0.0; 2] };
        let buf = d.create_buffer(&wgpu::BufferDescriptor { label: None, size: std::mem::size_of::<U>() as u64, usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false });
        g.queue.write_buffer(&buf, 0, bytemuck::bytes_of(&u));
        let sampler = d.create_sampler(&wgpu::SamplerDescriptor::default());
        let group = |l: &wgpu::BindGroupLayout| d.create_bind_group(&wgpu::BindGroupDescriptor { label: None, layout: l, entries: &[
            wgpu::BindGroupEntry { binding: 0, resource: buf.as_entire_binding() },
            wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::TextureView(&hooked.create_view(&Default::default())) },
            wgpu::BindGroupEntry { binding: 2, resource: wgpu::BindingResource::Sampler(&sampler) },
            wgpu::BindGroupEntry { binding: 3, resource: wgpu::BindingResource::TextureView(&table_tex.create_view(&Default::default())) },
            wgpu::BindGroupEntry { binding: 4, resource: wgpu::BindingResource::Sampler(&sampler) },
            wgpu::BindGroupEntry { binding: 5, resource: wgpu::BindingResource::TextureView(&grid_tex.create_view(&Default::default())) }] });
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
    fn the_shader_maps_pixels_as_gyroflow_does() {
        let Ok(project) = std::env::var("AFP_GPU_CLIP") else { eprintln!("AFP_GPU_CLIP not set: skipped"); return; };
        let clip = Clip::load(&project, Some(10.0)).unwrap();
        let g = gpu();
        let out = (1920usize, 1080usize);
        let frames = [0usize, 600, 1049, 1181, clip.frame_count - 1];
        let base = Options { mode: Mode::Exact, grid_step: 16, grid_linear: false, knots: 33, interp: 2, shrink: false, range: None };
        // (label, options, every frame or only the fastest)
        let cases = [
            ("exact, 33 knots", base, true),
            ("exact, every row", Options { knots: 0, range: Some((frames[2], frames[2])), ..base }, false),
            ("grid 16, 33 knots", Options { mode: Mode::Grid, ..base }, true),
            ("grid 32, 33 knots", Options { mode: Mode::Grid, grid_step: 32, ..base }, false),
            ("grid 8 bilinear, 33 knots", Options { mode: Mode::Grid, grid_step: 8, grid_linear: true, ..base }, false),
        ];
        for (label, opts, all) in cases {
            // The full rows only for the frames compared: the whole clip would be 5 M texels
            let table = Table::build(&clip, &opts).unwrap();
            let (grid, fs) = match opts.mode {
                Mode::Exact => (None, glsl(&clip, &opts, &table, "exact", true)),
                _ => (Some(glsl(&clip, &opts, &table, "grid", true)), glsl(&clip, &opts, &table, "sample", true)),
            };
            for &frame in &frames {
                if !all && frame != frames[2] { continue; }
                let pts = (frame as f64 / clip.fps) as f32;
                let uv = render_uv(&g, grid.as_deref(), &fs, &table, clip.size, out, pts);
                // Gyroflow at the same pixels, into a buffer of the output's size (the filter's dst)
                let tr = clip.transform(frame, out);
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
                assert!(worst < 0.1, "frame {frame}, {label}: the shader is {worst} px off Gyroflow");
                assert!(mismatched < out.0, "frame {frame}: {mismatched} pixels the shader left out");
            }
        }
        // Every interpolation compiles in every pass (the picture, not the coordinates)
        let table = Table::build(&clip, &Options { range: Some((0, 0)), ..base }).unwrap();
        for interp in 0..5 {
            for shrink in [false, true] {
                for (mode, name) in [(Mode::Exact, "exact"), (Mode::Grid, "sample"), (Mode::Grid, "grid")] {
                    let opts = Options { mode, interp, shrink, ..base };
                    let src = glsl(&clip, &opts, &table, name, false);
                    let scope = g.device.push_error_scope(wgpu::ErrorFilter::Validation);
                    module(&g, &src, if name == "grid" { wgpu::naga::ShaderStage::Compute } else { wgpu::naga::ShaderStage::Fragment });
                    let err = pollster::block_on(scope.pop());
                    assert!(err.is_none(), "INTERP {interp}, SHRINK {shrink}, pass {name}: {err:?}");
                }
            }
        }
    }
}
