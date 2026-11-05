// SPDX-License-Identifier: GPL-3.0-or-later
//
// A 3D LUT (.cube) applied to 10-bit YUV 4:2:0 frames on the GPU, for gyroflow-export. ffmpeg's lut3d runs on the
// CPU and, with the conversions to RGB and back, held a 4K export to ~20 fps; this keeps the frame where it is.
//
// The same math as mpv's shader (shaders/dlogm_*.glsl.in): the decoded YUV to RGB in the source's matrix and
// range, clamped, through the LUT with trilinear interpolation, then back to YUV in the target's matrix, limited
// range. Each invocation does a 4x2 block of luma and its 2x1 chroma, so every write is a whole 32-bit word;
// chroma is the average of the block's four pixels.
//
// Frames are planar, 16 bits per sample (ffmpeg's yuv420p10le), width a multiple of 4 and height of 2.
//
// With `measure` (an HLG output, for a Dolby Vision export's L1), the same pass measures the frame it writes as the
// player's mpv (tools/mpv-patches/0007) measures what it shows: each pixel back to RGB from its written 10-bit luma
// and its block's chroma, HLG to display light for 1000 nits by BT.2100's OOTF, max(R,G,B) in 12-bit PQ; each
// workgroup writes the sum, count, lowest and highest of its pixels, which the CPU adds up.

use std::sync::{ Arc, mpsc };

const SHADER: &str = r#"
struct Params {
    width: u32,           // luma
    height: u32,
    lut_size: u32,
    measure: u32,         // 1: measure the written frame for L1
    in_y_offset: f32,     // normalized Y' = (Y - in_y_offset) / in_y_scale, C = (C - 512) / in_c_scale
    in_y_scale: f32,
    in_c_scale: f32,
    _pad2: f32,
    in_kr: f32,           // the matrices, as Kr and Kb
    in_kb: f32,
    out_kr: f32,
    out_kb: f32,
}

@group(0) @binding(0) var<uniform> p: Params;
@group(0) @binding(1) var<storage, read_write> frame: array<u32>;
@group(0) @binding(2) var<storage, read> lut: array<f32>;
@group(0) @binding(3) var<storage, read_write> parts: array<vec4<f32>>;

var<workgroup> wg_sum: array<f32, 64>;
var<workgroup> wg_count: array<f32, 64>;
var<workgroup> wg_lo: array<f32, 64>;
var<workgroup> wg_hi: array<f32, 64>;

fn hlg_scene(e: f32) -> f32 {
    let x = clamp(e, 0.0, 1.0);
    return select((exp((x - 0.55991073) / 0.17883277) + 0.28466892) / 12.0, x * x / 3.0, x <= 0.5);
}

fn pq(nits: f32) -> f32 {
    let y = pow(clamp(nits / 10000.0, 0.0, 1.0), 0.1593017578125);
    return pow((0.8359375 + 18.8515625 * y) / (1.0 + 18.6875 * y), 78.84375);
}

// A written pixel's max(R,G,B) in 12-bit PQ: its limited-range 10-bit luma and chroma back to HLG RGB in the
// output's matrix, to display light for a 1000-nit display
fn measured(y10: f32, cb10: f32, cr10: f32) -> f32 {
    let yn = (y10 - 64.0) / 876.0;
    let r = yn + 2.0 * (1.0 - p.out_kr) * (cr10 - 512.0) / 896.0;
    let b = yn + 2.0 * (1.0 - p.out_kb) * (cb10 - 512.0) / 896.0;
    let g = (yn - p.out_kr * r - p.out_kb * b) / (1.0 - p.out_kr - p.out_kb);
    let e = clamp(vec3<f32>(r, g, b), vec3<f32>(0.0), vec3<f32>(1.0));
    let sc = vec3<f32>(hlg_scene(e.r), hlg_scene(e.g), hlg_scene(e.b));
    let ys = dot(sc, vec3<f32>(0.2627, 0.6780, 0.0593));
    var nits = 0.0;
    if (ys > 0.0) { nits = 1000.0 * pow(ys, 0.2) * max(sc.r, max(sc.g, sc.b)); }
    return pq(nits) * 4095.0;
}

fn lut_at(i: vec3<u32>) -> vec3<f32> {
    let n = p.lut_size;
    let k = 3u * (i.x + n * (i.y + n * i.z));
    return vec3<f32>(lut[k], lut[k + 1u], lut[k + 2u]);
}

fn apply_lut(c: vec3<f32>) -> vec3<f32> {
    let pos = clamp(c, vec3<f32>(0.0), vec3<f32>(1.0)) * f32(p.lut_size - 1u);
    let i0 = vec3<u32>(floor(pos));
    let i1 = min(i0 + vec3<u32>(1u), vec3<u32>(p.lut_size - 1u));
    let f = pos - floor(pos);
    let c00 = mix(lut_at(vec3<u32>(i0.x, i0.y, i0.z)), lut_at(vec3<u32>(i1.x, i0.y, i0.z)), f.x);
    let c10 = mix(lut_at(vec3<u32>(i0.x, i1.y, i0.z)), lut_at(vec3<u32>(i1.x, i1.y, i0.z)), f.x);
    let c01 = mix(lut_at(vec3<u32>(i0.x, i0.y, i1.z)), lut_at(vec3<u32>(i1.x, i0.y, i1.z)), f.x);
    let c11 = mix(lut_at(vec3<u32>(i0.x, i1.y, i1.z)), lut_at(vec3<u32>(i1.x, i1.y, i1.z)), f.x);
    return mix(mix(c00, c10, f.y), mix(c01, c11, f.y), f.z);
}

fn to_rgb(y: f32, cb: f32, cr: f32) -> vec3<f32> {
    let r = y + 2.0 * (1.0 - p.in_kr) * cr;
    let b = y + 2.0 * (1.0 - p.in_kb) * cb;
    let g = (y - p.in_kr * r - p.in_kb * b) / (1.0 - p.in_kr - p.in_kb);
    return vec3<f32>(r, g, b);
}

fn luma(c: vec3<f32>) -> f32 {
    return p.out_kr * c.r + (1.0 - p.out_kr - p.out_kb) * c.g + p.out_kb * c.b;
}

fn sample(word: u32, high: bool) -> f32 {
    return f32(select(word & 0xffffu, word >> 16u, high));
}

fn pack(lo: f32, hi: f32) -> u32 {
    let a = u32(clamp(round(lo), 0.0, 1023.0));
    let b = u32(clamp(round(hi), 0.0, 1023.0));
    return a | (b << 16u);
}

@compute @workgroup_size(8, 8, 1)
fn main(@builtin(global_invocation_id) id: vec3<u32>, @builtin(local_invocation_index) li: u32,
        @builtin(workgroup_id) wg: vec3<u32>, @builtin(num_workgroups) wgs: vec3<u32>) {
    let bx = id.x; // block of 4x2 luma
    let by = id.y;
    // No early return: the workgroup's barrier below must be reached by all of it
    let inside = bx * 4u < p.width && by * 2u < p.height;
    var m_sum = 0.0;
    var m_count = 0.0;
    var m_lo = 4096.0;
    var m_hi = 0.0;

    if (inside) {
        let wpr = p.width / 2u;            // luma words per row
        let cw = p.width / 2u;             // chroma width
        let cwpr = cw / 2u;                // chroma words per row
        let u_base = p.width * p.height / 2u;
        let v_base = u_base + cw * (p.height / 2u) / 2u;
        let c_index = by * cwpr + bx;

        let cu = frame[u_base + c_index];
        let cv = frame[v_base + c_index];
        let cb = array<f32, 2>((sample(cu, false) - 512.0) / p.in_c_scale, (sample(cu, true) - 512.0) / p.in_c_scale);
        let cr = array<f32, 2>((sample(cv, false) - 512.0) / p.in_c_scale, (sample(cv, true) - 512.0) / p.in_c_scale);

        var sum_b = array<f32, 2>(0.0, 0.0);
        var sum_r = array<f32, 2>(0.0, 0.0);
        var written_y = array<f32, 8>(0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0);
        for (var row = 0u; row < 2u; row++) {
            let base = (by * 2u + row) * wpr + bx * 2u;
            for (var w = 0u; w < 2u; w++) {
                let word = frame[base + w];
                var out_y = array<f32, 2>(0.0, 0.0);
                for (var h = 0u; h < 2u; h++) {
                    let y = (sample(word, h == 1u) - p.in_y_offset) / p.in_y_scale;
                    let c = apply_lut(clamp(to_rgb(y, cb[w], cr[w]), vec3<f32>(0.0), vec3<f32>(1.0)));
                    let yo = luma(c);
                    out_y[h] = 64.0 + 876.0 * yo;
                    sum_b[w] += (c.b - yo) / (2.0 * (1.0 - p.out_kb));
                    sum_r[w] += (c.r - yo) / (2.0 * (1.0 - p.out_kr));
                }
                let packed = pack(out_y[0], out_y[1]);
                frame[base + w] = packed;
                written_y[row * 4u + w * 2u] = sample(packed, false);
                written_y[row * 4u + w * 2u + 1u] = sample(packed, true);
            }
        }
        // Two chroma samples, each the average of its 2x2 luma pixels
        let pu = pack(512.0 + 896.0 * sum_b[0] / 4.0, 512.0 + 896.0 * sum_b[1] / 4.0);
        let pv = pack(512.0 + 896.0 * sum_r[0] / 4.0, 512.0 + 896.0 * sum_r[1] / 4.0);
        frame[u_base + c_index] = pu;
        frame[v_base + c_index] = pv;

        if (p.measure != 0u) {
            for (var i = 0u; i < 8u; i++) {
                let hi_half = (i % 4u) >= 2u; // which of the two chroma samples: the block's left or right 2x2
                let v = measured(written_y[i], sample(pu, hi_half), sample(pv, hi_half));
                m_sum += v;
                m_lo = min(m_lo, v);
                m_hi = max(m_hi, v);
            }
            m_count = 8.0;
        }
    }

    if (p.measure != 0u) {
        wg_sum[li] = m_sum;
        wg_count[li] = m_count;
        wg_lo[li] = m_lo;
        wg_hi[li] = m_hi;
        workgroupBarrier();
        if (li == 0u) {
            var r = vec4<f32>(0.0, 0.0, 4096.0, 0.0);
            for (var i = 0u; i < 64u; i++) {
                r = vec4<f32>(r.x + wg_sum[i], r.y + wg_count[i], min(r.z, wg_lo[i]), max(r.w, wg_hi[i]));
            }
            parts[wg.y * wgs.x + wg.x] = r;
        }
    }
}
"#;

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Params {
    width: u32, height: u32, lut_size: u32, measure: u32,
    in_y_offset: f32, in_y_scale: f32, in_c_scale: f32, _pad2: f32,
    in_kr: f32, in_kb: f32, out_kr: f32, out_kb: f32,
}

/// Kr and Kb of a YUV matrix, by ffmpeg's name for it
pub fn matrix_coefficients(name: &str) -> Option<(f32, f32)> {
    match name {
        "bt709" => Some((0.2126, 0.0722)),
        "bt2020" | "bt2020nc" => Some((0.2627, 0.0593)),
        "bt601" | "smpte170m" | "bt470bg" => Some((0.299, 0.114)),
        _ => None,
    }
}

/// Shared by the workers: the device, the pipeline and the LUT
pub struct GpuLut {
    device: wgpu::Device,
    queue: wgpu::Queue,
    pipeline: wgpu::ComputePipeline,
    layout: wgpu::BindGroupLayout,
    lut: wgpu::Buffer,
    params: Params,
    frame_size: u64,
    parts_size: u64,
}

/// One worker's buffers: a frame is uploaded, converted in place and read back
pub struct GpuLutWorker {
    lut: Arc<GpuLut>,
    frame: wgpu::Buffer,
    staging: wgpu::Buffer,
    parts: wgpu::Buffer,
    parts_staging: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
}

fn read_cube(path: &std::path::Path) -> Result<(u32, Vec<f32>), String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let mut size = 0u32;
    let mut data = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') { continue; }
        if let Some(v) = line.strip_prefix("LUT_3D_SIZE") {
            size = v.trim().parse().map_err(|_| format!("{}: bad LUT_3D_SIZE", path.display()))?;
            continue;
        }
        if line.starts_with(|c: char| c.is_ascii_alphabetic()) {
            // TITLE, DOMAIN_MIN/MAX (0..1 is all that's used here)
            if line.starts_with("DOMAIN") && line.split_whitespace().skip(1).any(|v| v != "0" && v != "1" && v != "0.0" && v != "1.0") {
                return Err(format!("{}: only a 0..1 domain is supported", path.display()));
            }
            continue;
        }
        for v in line.split_whitespace() {
            data.push(v.parse::<f32>().map_err(|_| format!("{}: bad value {v}", path.display()))?);
        }
    }
    if size < 2 || data.len() != (size * size * size * 3) as usize {
        return Err(format!("{}: not a 3D .cube LUT", path.display()));
    }
    Ok((size, data))
}

/// The NBCU look's LUT, as one: a log curve's LUT to HLG (`log`), then NBCUniversal's HLG -> SDR (`nbcu`, a Type III
/// LUT: the signal's 0..1 is (64 + 876 x) / 1023 of its domain, in and out), at the log LUT's grid points, written
/// to `out` as a .cube with plain 0..1 Rec.709, clipped. The same as playback's two shaders (shaders/nbcu_sdr.glsl.in):
/// NBCU's knee by tetrahedral interpolation, its highlights above 100 % clipped
pub fn compose_nbcu(log: &std::path::Path, nbcu: &std::path::Path, out: &std::path::Path) -> Result<(), String> {
    let (n, log_data) = read_cube(log)?;
    let (m, nbcu_data) = read_cube(nbcu)?;
    let (lo, span) = (64.0 / 1023.0, 876.0 / 1023.0);
    let mut text = format!("# {} then {}, by gyroflow-export\nLUT_3D_SIZE {n}\n", file_name(log), file_name(nbcu));
    for v in log_data.chunks(3) {
        let o = tetrahedral(m as usize, &nbcu_data, [v[0] * span + lo, v[1] * span + lo, v[2] * span + lo]);
        let c = |x: f32| ((x - lo) / span).clamp(0.0, 1.0);
        text += &format!("{:.6} {:.6} {:.6}\n", c(o[0]), c(o[1]), c(o[2]));
    }
    std::fs::write(out, text).map_err(|e| format!("{}: {e}", out.display()))
}

fn file_name(p: &std::path::Path) -> String {
    p.file_name().map_or_else(String::new, |f| f.to_string_lossy().to_string())
}

/// A 3D LUT (red fastest) at `rgb` (0..1), by tetrahedral interpolation
fn tetrahedral(n: usize, lut: &[f32], rgb: [f32; 3]) -> [f32; 3] {
    let p = rgb.map(|v| v.clamp(0.0, 1.0) * (n - 1) as f32);
    let i = p.map(|x| (x as usize).min(n - 2));
    let f = [p[0] - i[0] as f32, p[1] - i[1] as f32, p[2] - i[2] as f32];
    let at = |c: [usize; 3]| {
        let k = ((i[0] + c[0]) + (i[1] + c[1]) * n + (i[2] + c[2]) * n * n) * 3;
        [lut[k], lut[k + 1], lut[k + 2]]
    };
    // From the cell's 000 corner to its 111, one axis at a time, the largest fraction first
    let mut axes = [0, 1, 2];
    axes.sort_by(|&a, &b| f[b].total_cmp(&f[a]));
    let mut corner = [0; 3];
    let mut prev = at(corner);
    let mut out = prev;
    for &axis in &axes {
        corner[axis] = 1;
        let cur = at(corner);
        for c in 0..3 { out[c] += f[axis] * (cur[c] - prev[c]); }
        prev = cur;
    }
    out
}

impl GpuLut {
    /// `in_full_range`: the decoded frames' range; the output is always limited range. `measure`: each frame's L1
    /// too (an HLG output), from apply
    pub fn new(cube: &std::path::Path, width: usize, height: usize, in_matrix: (f32, f32), in_full_range: bool, out_matrix: (f32, f32), measure: bool) -> Result<Arc<GpuLut>, String> {
        if width % 4 != 0 || height % 2 != 0 {
            return Err(format!("{width}x{height}: the GPU LUT needs a width divisible by 4"));
        }
        let (lut_size, lut_data) = read_cube(cube)?;

        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            force_fallback_adapter: false,
            compatible_surface: None,
            apply_limit_buckets: false,
        })).map_err(|e| format!("no GPU: {e}"))?;
        let frame_size = (width * height * 3) as u64; // 4:2:0, 2 bytes per sample
        let limits = adapter.limits();
        if frame_size > limits.max_storage_buffer_binding_size as u64 || frame_size > limits.max_buffer_size {
            return Err(format!("{width}x{height} is too big for one GPU buffer"));
        }
        let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some("gyroflow-export LUT"),
            required_features: wgpu::Features::empty(),
            required_limits: limits,
            memory_hints: wgpu::MemoryHints::Performance,
            trace: wgpu::Trace::Off,
            experimental_features: wgpu::ExperimentalFeatures::disabled(),
        })).map_err(|e| format!("GPU device: {e}"))?;

        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor { label: None, source: wgpu::ShaderSource::Wgsl(SHADER.into()) });
        let entry = |binding, ty| wgpu::BindGroupLayoutEntry { binding, visibility: wgpu::ShaderStages::COMPUTE, ty, count: None };
        let storage = |read_only| wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Storage { read_only }, has_dynamic_offset: false, min_binding_size: None };
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: None,
            entries: &[
                entry(0, wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Uniform, has_dynamic_offset: false, min_binding_size: None }),
                entry(1, storage(false)),
                entry(2, storage(true)),
                entry(3, storage(false)),
            ],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor { label: None, bind_group_layouts: &[Some(&layout)], immediate_size: 0 });
        let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: None, layout: Some(&pipeline_layout), module: &shader, entry_point: Some("main"),
            compilation_options: Default::default(), cache: Default::default(),
        });

        let lut = device.create_buffer(&wgpu::BufferDescriptor { label: None, size: (lut_data.len() * 4) as u64, usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false });
        queue.write_buffer(&lut, 0, bytemuck::cast_slice(&lut_data));

        let (in_y_offset, in_y_scale, in_c_scale) = if in_full_range { (0.0, 1023.0, 1023.0) } else { (64.0, 876.0, 896.0) };
        let params = Params {
            width: width as u32, height: height as u32, lut_size, measure: measure as u32,
            in_y_offset, in_y_scale, in_c_scale, _pad2: 0.0,
            in_kr: in_matrix.0, in_kb: in_matrix.1, out_kr: out_matrix.0, out_kb: out_matrix.1,
        };
        let groups = ((width / 4) as u64).div_ceil(8) * ((height / 2) as u64).div_ceil(8);
        Ok(Arc::new(GpuLut { device, queue, pipeline, layout, lut, params, frame_size, parts_size: groups * 16 }))
    }

    pub fn worker(self: &Arc<Self>) -> GpuLutWorker {
        let d = &self.device;
        let frame = d.create_buffer(&wgpu::BufferDescriptor { label: None, size: self.frame_size, usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::COPY_SRC, mapped_at_creation: false });
        let staging = d.create_buffer(&wgpu::BufferDescriptor { label: None, size: self.frame_size, usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false });
        let parts = d.create_buffer(&wgpu::BufferDescriptor { label: None, size: self.parts_size, usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC, mapped_at_creation: false });
        let parts_staging = d.create_buffer(&wgpu::BufferDescriptor { label: None, size: self.parts_size, usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false });
        let params = d.create_buffer(&wgpu::BufferDescriptor { label: None, size: std::mem::size_of::<Params>() as u64, usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false });
        self.queue.write_buffer(&params, 0, bytemuck::bytes_of(&self.params));
        let bind_group = d.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &self.layout,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: params.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 1, resource: frame.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 2, resource: self.lut.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 3, resource: parts.as_entire_binding() },
            ],
        });
        GpuLutWorker { lut: self.clone(), frame, staging, parts, parts_staging, bind_group }
    }
}

impl GpuLutWorker {
    /// Converts `frame` (planar 4:2:0, 16-bit samples) in place; measuring, also returns its L1: the lowest, average
    /// and highest max(R,G,B) in 12-bit PQ
    pub fn apply(&mut self, frame: &mut [u8]) -> Result<Option<[f64; 3]>, String> {
        let g = &*self.lut;
        if frame.len() as u64 != g.frame_size {
            return Err(format!("frame is {} bytes, the LUT was set up for {}", frame.len(), g.frame_size));
        }
        let measure = g.params.measure != 0;
        g.queue.write_buffer(&self.frame, 0, frame);
        let mut encoder = g.device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
        {
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor { label: None, timestamp_writes: None });
            pass.set_pipeline(&g.pipeline);
            pass.set_bind_group(0, Some(&self.bind_group), &[]);
            pass.dispatch_workgroups((g.params.width / 4).div_ceil(8), (g.params.height / 2).div_ceil(8), 1);
        }
        encoder.copy_buffer_to_buffer(&self.frame, 0, &self.staging, 0, g.frame_size);
        if measure { encoder.copy_buffer_to_buffer(&self.parts, 0, &self.parts_staging, 0, g.parts_size); }
        g.queue.submit(Some(encoder.finish()));

        let slice = self.staging.slice(..);
        let (tx, rx) = mpsc::channel();
        let tx2 = tx.clone();
        slice.map_async(wgpu::MapMode::Read, move |r| { let _ = tx.send(r); });
        let parts_slice = self.parts_staging.slice(..);
        if measure { parts_slice.map_async(wgpu::MapMode::Read, move |r| { let _ = tx2.send(r); }); }
        let _ = g.device.poll(wgpu::PollType::Wait { submission_index: None, timeout: None });
        for _ in 0..(1 + measure as usize) {
            rx.recv().map_err(|_| "GPU readback lost".to_string())?.map_err(|e| format!("GPU readback: {e}"))?;
        }
        {
            let data = slice.get_mapped_range().map_err(|e| format!("GPU readback: {e:?}"))?;
            frame.copy_from_slice(&data);
        }
        self.staging.unmap();
        if !measure { return Ok(None); }

        let (mut sum, mut count, mut lo, mut hi) = (0.0f64, 0.0f64, 4096.0f64, 0.0f64);
        {
            let data = parts_slice.get_mapped_range().map_err(|e| format!("GPU readback: {e:?}"))?;
            for part in bytemuck::cast_slice::<u8, [f32; 4]>(&data) {
                if part[1] <= 0.0 { continue; }
                sum += part[0] as f64;
                count += part[1] as f64;
                lo = lo.min(part[2] as f64);
                hi = hi.max(part[3] as f64);
            }
        }
        self.parts_staging.unmap();
        Ok((count > 0.0).then(|| [lo, sum / count, hi]))
    }
}
