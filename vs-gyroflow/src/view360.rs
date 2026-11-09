// SPDX-License-Identifier: GPL-3.0-or-later
//
// The 360 view on the GPU for exports (docs/360-cameras.md, "Exports"; step 7 of docs/360-implementation-plan.md): a
// sphere from the plugin's 360 mode (sphere360.rs) and its stamp, drawn as the player's view shader draws it
// (view360.wgsl, the same math as mpv/scripts/view360/view360.glsl), or as the whole sphere turned into the world and the
// chosen heading (projection 4, a stabilized 360 video). One plane at a time (luma, then each chroma plane at its own
// size), as the plugin renders; out comes the plane, normalized to 0-1.
//
// The tests render the same sphere and stamp through the player's own GLSL (its two passes, read from the file and
// compiled by naga's GLSL front end into a pipeline on the same device) and through the WGSL, and compare.

use std::collections::HashMap;
use parking_lot::Mutex;

/// What the view is: the frame's camera and heading (from its stamp), the head, the projection
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ViewParams {
    /// The camera at the middle of the frame in the world, Hamilton [w, x, y, z]
    pub q_world_from_cam: [f64; 4],
    /// The chosen heading's yaw in the world, radians
    pub heading_yaw: f64,
    /// The head, degrees: yaw right, pitch up, roll counterclockwise
    pub yaw: f64,
    pub pitch: f64,
    pub roll: f64,
    /// Degrees across a 16:9 picture of the output's height
    pub fov: f64,
    pub projection: Projection,
    /// 0 mono, 1 top-bottom, 2 left-right
    pub stereo: u32,
}

impl Default for ViewParams {
    fn default() -> Self {
        ViewParams { q_world_from_cam: [1.0, 0.0, 0.0, 0.0], heading_yaw: 0.0, yaw: 0.0, pitch: 0.0, roll: 0.0, fov: 100.0, projection: Projection::Auto, stereo: 0 }
    }
}

/// The output projections: the player's four, and the whole sphere
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Projection { Auto = 0, Rectilinear = 1, Stereographic = 2, Planet = 3, Equirect = 4 }

impl Projection {
    pub fn from_name(s: &str) -> Option<Projection> {
        Some(match s {
            "auto" => Projection::Auto, "rectilinear" => Projection::Rectilinear, "stereographic" => Projection::Stereographic,
            "planet" | "little-planet" => Projection::Planet, "equirect" | "equirectangular" => Projection::Equirect,
            _ => return None,
        })
    }
    /// As the player names it
    pub fn name(self) -> &'static str {
        match self {
            Projection::Auto => "auto", Projection::Rectilinear => "rectilinear", Projection::Stereographic => "stereographic",
            Projection::Planet => "planet", Projection::Equirect => "equirect",
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Uniforms {
    q: [f32; 4],
    size: [f32; 2],
    out_size: [f32; 2],
    heading_yaw: f32,
    yaw: f32,
    pitch: f32,
    roll: f32,
    fov: f32,
    projection: i32,
    stereo: i32,
    stamp_rows: f32,
}

/// A plane of samples, `stride` apart per row
#[derive(Clone, Copy)]
pub enum Plane<'a> { U8(&'a [u8]), U16(&'a [u16]) }

struct Target {
    input: wgpu::Texture,
    output: wgpu::Texture,
    readback: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
    uniforms: wgpu::Buffer,
}

pub struct View360 {
    pub(crate) device: wgpu::Device,
    pub(crate) queue: wgpu::Queue,
    pipeline: wgpu::RenderPipeline,
    layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    targets: Mutex<HashMap<(usize, usize, bool, usize, usize), Target>>,
}

/// Rows of the readback padded to 256 bytes
fn padded(bytes: usize) -> usize { bytes.div_ceil(256) * 256 }

impl View360 {
    pub fn new() -> Result<View360, String> {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            force_fallback_adapter: false,
            compatible_surface: None,
            apply_limit_buckets: false,
        })).map_err(|e| format!("no GPU: {e}"))?;
        let features = adapter.features() & wgpu::Features::TEXTURE_FORMAT_16BIT_NORM;
        if !features.contains(wgpu::Features::TEXTURE_FORMAT_16BIT_NORM) {
            return Err("the GPU has no 16-bit normalized textures".into());
        }
        let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some("360 view"),
            required_features: features,
            required_limits: adapter.limits(),
            memory_hints: wgpu::MemoryHints::Performance,
            ..Default::default()
        })).map_err(|e| format!("GPU device: {e}"))?;
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("view360.wgsl"),
            source: wgpu::ShaderSource::Wgsl(include_str!("view360.wgsl").into()),
        });
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("360 view"),
            entries: &[
                wgpu::BindGroupLayoutEntry { binding: 0, visibility: wgpu::ShaderStages::FRAGMENT, ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Uniform, has_dynamic_offset: false, min_binding_size: None }, count: None },
                wgpu::BindGroupLayoutEntry { binding: 1, visibility: wgpu::ShaderStages::FRAGMENT, ty: wgpu::BindingType::Texture { sample_type: wgpu::TextureSampleType::Float { filterable: true }, view_dimension: wgpu::TextureViewDimension::D2, multisampled: false }, count: None },
                wgpu::BindGroupLayoutEntry { binding: 2, visibility: wgpu::ShaderStages::FRAGMENT, ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering), count: None },
            ],
        });
        let pipeline = render_pipeline(&device, &layout, &module, "vs", &module, "fs");
        let sampler = linear_sampler(&device);
        Ok(View360 { device, queue, pipeline, layout, sampler, targets: Mutex::new(HashMap::new()) })
    }

    /// Draws the view of a sphere plane `w` by `h` (`stamp_rows` of them under the sphere aren't: 0 for a plane with
    /// none) into `out` pixels: the plane's samples normalized, 0-1, row by row
    pub fn render(&self, src: Plane, w: usize, h: usize, stride: usize, stamp_rows: usize, out: (usize, usize), p: &ViewParams) -> Result<Vec<f32>, String> {
        let wide = matches!(src, Plane::U16(_));
        let key = (w, h, wide, out.0, out.1);
        let mut targets = self.targets.lock();
        let t = targets.entry(key).or_insert_with(|| self.target(w, h, wide, out));
        let u = Uniforms {
            q: p.q_world_from_cam.map(|c| c as f32),
            size: [w as f32, h as f32],
            out_size: [out.0 as f32, out.1 as f32],
            heading_yaw: p.heading_yaw as f32,
            yaw: p.yaw as f32, pitch: p.pitch as f32, roll: p.roll as f32, fov: p.fov as f32,
            projection: p.projection as i32,
            stereo: p.stereo as i32,
            stamp_rows: stamp_rows as f32,
        };
        self.queue.write_buffer(&t.uniforms, 0, bytemuck::bytes_of(&u));
        upload(&self.queue, &t.input, src, w, h, stride)?;
        draw(&self.device, &self.queue, &self.pipeline, &t.bind_group, &t.output, &t.readback, out)
    }

    fn target(&self, w: usize, h: usize, wide: bool, out: (usize, usize)) -> Target {
        let input = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("sphere"), size: wgpu::Extent3d { width: w as u32, height: h as u32, depth_or_array_layers: 1 },
            mip_level_count: 1, sample_count: 1, dimension: wgpu::TextureDimension::D2,
            format: if wide { wgpu::TextureFormat::R16Unorm } else { wgpu::TextureFormat::R8Unorm },
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST, view_formats: &[],
        });
        let (output, readback) = output_texture(&self.device, out);
        let uniforms = self.device.create_buffer(&wgpu::BufferDescriptor { label: Some("360 view"), size: std::mem::size_of::<Uniforms>() as u64, usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false });
        let view = input.create_view(&Default::default());
        let bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("360 view"), layout: &self.layout,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: uniforms.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::TextureView(&view) },
                wgpu::BindGroupEntry { binding: 2, resource: wgpu::BindingResource::Sampler(&self.sampler) },
            ],
        });
        Target { input, output, readback, bind_group, uniforms }
    }
}

fn linear_sampler(device: &wgpu::Device) -> wgpu::Sampler {
    device.create_sampler(&wgpu::SamplerDescriptor {
        label: Some("360 view"),
        address_mode_u: wgpu::AddressMode::ClampToEdge, address_mode_v: wgpu::AddressMode::ClampToEdge,
        mag_filter: wgpu::FilterMode::Linear, min_filter: wgpu::FilterMode::Linear,
        ..Default::default()
    })
}

fn render_pipeline(device: &wgpu::Device, layout: &wgpu::BindGroupLayout, vs: &wgpu::ShaderModule, vs_entry: &str, fs: &wgpu::ShaderModule, fs_entry: &str) -> wgpu::RenderPipeline {
    let pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor { label: None, bind_group_layouts: &[Some(layout)], immediate_size: 0 });
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("360 view"),
        layout: Some(&pl),
        vertex: wgpu::VertexState { module: vs, entry_point: Some(vs_entry), buffers: &[], compilation_options: Default::default() },
        fragment: Some(wgpu::FragmentState {
            module: fs, entry_point: Some(fs_entry), compilation_options: Default::default(),
            targets: &[Some(wgpu::ColorTargetState { format: wgpu::TextureFormat::R32Float, blend: None, write_mask: wgpu::ColorWrites::ALL })],
        }),
        primitive: Default::default(),
        depth_stencil: None,
        multisample: Default::default(),
        multiview_mask: None,
        cache: None,
    })
}

fn output_texture(device: &wgpu::Device, out: (usize, usize)) -> (wgpu::Texture, wgpu::Buffer) {
    let output = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("360 view out"), size: wgpu::Extent3d { width: out.0 as u32, height: out.1 as u32, depth_or_array_layers: 1 },
        mip_level_count: 1, sample_count: 1, dimension: wgpu::TextureDimension::D2, format: wgpu::TextureFormat::R32Float,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC, view_formats: &[],
    });
    let readback = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("360 view readback"), size: (padded(out.0 * 4) * out.1) as u64,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ, mapped_at_creation: false,
    });
    (output, readback)
}

fn upload(queue: &wgpu::Queue, tex: &wgpu::Texture, src: Plane, w: usize, h: usize, stride: usize) -> Result<(), String> {
    let (bytes, sample): (&[u8], usize) = match src {
        Plane::U8(s) => (s, 1),
        Plane::U16(s) => (bytemuck::cast_slice(s), 2),
    };
    if bytes.len() < (stride * (h - 1) + w) * sample { return Err(format!("a plane of {} bytes for {w}x{h}, stride {stride}", bytes.len())); }
    queue.write_texture(
        wgpu::TexelCopyTextureInfo { texture: tex, mip_level: 0, origin: wgpu::Origin3d::ZERO, aspect: wgpu::TextureAspect::All },
        bytes,
        wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some((stride * sample) as u32), rows_per_image: Some(h as u32) },
        wgpu::Extent3d { width: w as u32, height: h as u32, depth_or_array_layers: 1 },
    );
    Ok(())
}

fn draw(device: &wgpu::Device, queue: &wgpu::Queue, pipeline: &wgpu::RenderPipeline, bind_group: &wgpu::BindGroup, output: &wgpu::Texture, readback: &wgpu::Buffer, out: (usize, usize)) -> Result<Vec<f32>, String> {
    let view = output.create_view(&Default::default());
    let mut enc = device.create_command_encoder(&Default::default());
    {
        let mut pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("360 view"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment { view: &view, depth_slice: None, resolve_target: None, ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color::BLACK), store: wgpu::StoreOp::Store } })],
            ..Default::default()
        });
        pass.set_pipeline(pipeline);
        pass.set_bind_group(0, bind_group, &[]);
        pass.draw(0..3, 0..1);
    }
    let row = padded(out.0 * 4);
    enc.copy_texture_to_buffer(
        wgpu::TexelCopyTextureInfo { texture: output, mip_level: 0, origin: wgpu::Origin3d::ZERO, aspect: wgpu::TextureAspect::All },
        wgpu::TexelCopyBufferInfo { buffer: readback, layout: wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(row as u32), rows_per_image: Some(out.1 as u32) } },
        wgpu::Extent3d { width: out.0 as u32, height: out.1 as u32, depth_or_array_layers: 1 },
    );
    queue.submit([enc.finish()]);
    let slice = readback.slice(..);
    let (tx, rx) = std::sync::mpsc::channel();
    slice.map_async(wgpu::MapMode::Read, move |r| { let _ = tx.send(r); });
    device.poll(wgpu::PollType::wait_indefinitely()).map_err(|e| format!("GPU: {e}"))?;
    rx.recv().map_err(|_| "GPU readback lost")?.map_err(|e| format!("GPU readback: {e}"))?;
    let mut values = Vec::with_capacity(out.0 * out.1);
    {
        let data = slice.get_mapped_range().map_err(|e| format!("GPU readback: {e:?}"))?;
        for y in 0..out.1 {
            values.extend_from_slice(bytemuck::cast_slice(&data[y * row..y * row + out.0 * 4]));
        }
    }
    readback.unmap();
    Ok(values)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::stamp::{ self, Stamp, HeadingStamp };

    /// A sphere to look at: smooth gradients and sharp lines, so filtering and placement both show; `w` by `w/2`,
    /// 8-bit, with `rows` more rows under it (for the stamp)
    fn sphere(w: usize, rows: usize) -> Vec<u8> { sphere_with(w, rows, true) }

    fn sphere_with(w: usize, rows: usize, lines: bool) -> Vec<u8> {
        let h = w / 2;
        let mut out = vec![0u8; w * (h + rows)];
        for y in 0..h {
            for x in 0..w {
                let lon = (x as f64 + 0.5) / w as f64 * std::f64::consts::TAU;
                let lat = (0.5 - (y as f64 + 0.5) / h as f64) * std::f64::consts::PI;
                let mut v = 128.0 + 60.0 * (3.0 * lon).sin() * lat.cos() + 40.0 * (5.0 * lat).sin();
                if lines && (x / 16 + y / 16) % 7 == 0 { v = 250.0; }
                if lines && (x % 64 == 0 || y % 64 == 0) { v = 10.0; }
                out[y * w + x] = v.clamp(0.0, 255.0) as u8;
            }
        }
        out
    }

    fn quat(axis: [f64; 3], deg: f64) -> [f64; 4] {
        let a = nalgebra::Vector3::new(axis[0], axis[1], axis[2]).normalize();
        let q = nalgebra::UnitQuaternion::from_axis_angle(&nalgebra::Unit::new_normalize(a), deg.to_radians());
        [q.w, q.i, q.j, q.k]
    }

    /// The player's GLSL (mpv/scripts/view360/view360.glsl) as two fragment shaders for naga: the LUMA pass that reads
    /// the stamp, and the view pass. What libplacebo provides (the textures, their sizes, the params) is declared
    /// around each pass's text as it is in the file
    fn glsl_passes() -> (String, String) {
        let file = include_str!("../../mpv/scripts/view360/view360.glsl");
        let pass = |desc: &str| -> String {
            let at = file.find(&format!("//!DESC {desc}")).unwrap_or_else(|| panic!("no pass {desc}"));
            let body = &file[at..];
            let body = &body[body.find('\n').unwrap() + 1..];
            body[..body.find("//!HOOK").unwrap_or(body.len())].to_string()
        };
        let params = "layout(set = 0, binding = 0, std140) uniform Params {
    vec2 hooked_size; vec2 out_size;
    float view360_yaw; float view360_pitch; float view360_roll; float view360_fov;
    int view360_projection; int view360_heading; int view360_stereo; int view360_on;
};
layout(set = 0, binding = 1) uniform texture2D hooked_t;
layout(set = 0, binding = 2) uniform sampler hooked_s;
layout(set = 0, binding = 3) uniform texture2D stamp_t;
layout(set = 0, binding = 4) uniform sampler stamp_s;
layout(location = 0) out vec4 o_color;
#define HOOKED_raw sampler2D(hooked_t, hooked_s)
#define HOOKED_tex(pos) texture(sampler2D(hooked_t, hooked_s), pos)
#define HOOKED_size hooked_size
#define HOOKED_mul 1.0
#define STAMP360_raw sampler2D(stamp_t, stamp_s)
#define STAMP360_mul 1.0
#define target_size out_size
#define HOOKED_pos (gl_FragCoord.xy / out_size)
";
        let wrap = |body: String| format!("#version 450\n{params}\n{body}\nvoid main() {{ o_color = hook(); }}\n");
        (wrap(pass("360 stamp")), wrap(pass("360 view")))
    }

    #[repr(C)]
    #[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
    struct GlslParams {
        hooked_size: [f32; 2], out_size: [f32; 2],
        yaw: f32, pitch: f32, roll: f32, fov: f32,
        projection: i32, heading: i32, stereo: i32, on: i32,
    }

    /// The GLSL's two passes on `v`'s device: the stamp pass over the sphere's luma with its stamp rows (34 x 1), then
    /// the view pass from both
    struct Glsl { stamp: wgpu::RenderPipeline, view: wgpu::RenderPipeline, layout: wgpu::BindGroupLayout, sampler: wgpu::Sampler, nearest: wgpu::Sampler }

    impl Glsl {
        fn new(v: &View360) -> Glsl {
            let (stamp_src, view_src) = glsl_passes();
            let module = |src: &str, label: &str| v.device.create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some(label),
                source: wgpu::ShaderSource::Glsl { shader: src.into(), stage: wgpu::naga::ShaderStage::Fragment, defines: &[] },
            });
            let vs = v.device.create_shader_module(wgpu::ShaderModuleDescriptor { label: Some("vs"), source: wgpu::ShaderSource::Wgsl(include_str!("view360.wgsl").into()) });
            let tex = |b: u32, filterable: bool| wgpu::BindGroupLayoutEntry { binding: b, visibility: wgpu::ShaderStages::FRAGMENT, ty: wgpu::BindingType::Texture { sample_type: wgpu::TextureSampleType::Float { filterable }, view_dimension: wgpu::TextureViewDimension::D2, multisampled: false }, count: None };
            let layout = v.device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("glsl"),
                entries: &[
                    wgpu::BindGroupLayoutEntry { binding: 0, visibility: wgpu::ShaderStages::FRAGMENT, ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Uniform, has_dynamic_offset: false, min_binding_size: None }, count: None },
                    tex(1, true),
                    wgpu::BindGroupLayoutEntry { binding: 2, visibility: wgpu::ShaderStages::FRAGMENT, ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering), count: None },
                    tex(3, false),
                    wgpu::BindGroupLayoutEntry { binding: 4, visibility: wgpu::ShaderStages::FRAGMENT, ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::NonFiltering), count: None },
                ],
            });
            let stamp = render_pipeline(&v.device, &layout, &vs, "vs", &module(&stamp_src, "glsl stamp"), "main");
            let view = render_pipeline(&v.device, &layout, &vs, "vs", &module(&view_src, "glsl view"), "main");
            let nearest = v.device.create_sampler(&wgpu::SamplerDescriptor::default());
            Glsl { stamp, view, layout, sampler: linear_sampler(&v.device), nearest }
        }

        /// The view of an 8-bit luma plane `w` by `h` (the stamp in its last rows)
        fn render(&self, v: &View360, luma: &[u8], w: usize, h: usize, out: (usize, usize), p: &ViewParams, heading: i32) -> (Vec<f32>, Vec<f32>) {
            let d = &v.device;
            let input = d.create_texture(&wgpu::TextureDescriptor {
                label: None, size: wgpu::Extent3d { width: w as u32, height: h as u32, depth_or_array_layers: 1 }, mip_level_count: 1, sample_count: 1,
                dimension: wgpu::TextureDimension::D2, format: wgpu::TextureFormat::R8Unorm, usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST, view_formats: &[],
            });
            upload(&v.queue, &input, Plane::U8(luma), w, h, w).unwrap();
            let (stamp_tex, stamp_back) = output_texture(d, (34, 1));
            let stamp_tex_sampled = d.create_texture(&wgpu::TextureDescriptor {
                label: None, size: wgpu::Extent3d { width: 34, height: 1, depth_or_array_layers: 1 }, mip_level_count: 1, sample_count: 1,
                dimension: wgpu::TextureDimension::D2, format: wgpu::TextureFormat::R32Float,
                usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC, view_formats: &[],
            });
            let _ = stamp_tex;
            let (out_tex, out_back) = output_texture(d, out);
            let group = |size: (usize, usize), stamp: &wgpu::Texture| {
                let params = GlslParams {
                    hooked_size: [w as f32, h as f32], out_size: [size.0 as f32, size.1 as f32],
                    yaw: p.yaw as f32, pitch: p.pitch as f32, roll: p.roll as f32, fov: p.fov as f32,
                    projection: p.projection as i32, heading, stereo: p.stereo as i32, on: 1,
                };
                let buf = d.create_buffer(&wgpu::BufferDescriptor { label: None, size: std::mem::size_of::<GlslParams>() as u64, usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false });
                v.queue.write_buffer(&buf, 0, bytemuck::bytes_of(&params));
                d.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: None, layout: &self.layout,
                    entries: &[
                        wgpu::BindGroupEntry { binding: 0, resource: buf.as_entire_binding() },
                        wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::TextureView(&input.create_view(&Default::default())) },
                        wgpu::BindGroupEntry { binding: 2, resource: wgpu::BindingResource::Sampler(&self.sampler) },
                        wgpu::BindGroupEntry { binding: 3, resource: wgpu::BindingResource::TextureView(&stamp.create_view(&Default::default())) },
                        wgpu::BindGroupEntry { binding: 4, resource: wgpu::BindingResource::Sampler(&self.nearest) },
                    ],
                })
            };
            // The stamp pass reads no STAMP360: bind any texture there
            let dummy = d.create_texture(&wgpu::TextureDescriptor {
                label: None, size: wgpu::Extent3d { width: 34, height: 1, depth_or_array_layers: 1 }, mip_level_count: 1, sample_count: 1,
                dimension: wgpu::TextureDimension::D2, format: wgpu::TextureFormat::R32Float, usage: wgpu::TextureUsages::TEXTURE_BINDING, view_formats: &[],
            });
            let stamp = draw(d, &v.queue, &self.stamp, &group((34, 1), &dummy), &stamp_tex_sampled, &stamp_back, (34, 1)).unwrap();
            let view = draw(d, &v.queue, &self.view, &group(out, &stamp_tex_sampled), &out_tex, &out_back, out).unwrap();
            (stamp, view)
        }
    }

    fn compare(a: &[f32], b: &[f32]) -> (f32, f64) {
        let worst = a.iter().zip(b).map(|(x, y)| (x - y).abs()).fold(0.0f32, f32::max);
        let over = a.iter().zip(b).filter(|(x, y)| (*x - *y).abs() > 0.5 / 255.0).count() as f64 / a.len() as f64;
        (worst, over)
    }

    #[test]
    fn the_wgsl_draws_what_the_players_glsl_does() {
        let v = View360::new().unwrap();
        let glsl = Glsl::new(&v);
        let (w, rows) = (1024usize, stamp::ROWS);
        let h = w / 2 + rows;
        let mut luma = sphere(w, rows);
        let st = Stamp {
            time_us: 1_234_567,
            q_world_from_cam: quat([0.3, -1.0, 0.2], 37.0),
            headings: [
                HeadingStamp { yaw: 0.4, confidence: 1.0 }, HeadingStamp { yaw: -2.1, confidence: 1.0 },
                HeadingStamp { yaw: 2.9, confidence: 0.8 }, HeadingStamp { yaw: -0.7, confidence: 0.0 },
            ],
        };
        assert!(stamp::write(&mut luma, w, w, h, 8, &st));
        // What a reader gets back, as the export reads it
        let read = stamp::read(&luma, w, w, h, 8).unwrap().to_stamp();
        let out = (480usize, 270usize);
        let cases: &[(Projection, f64, f64, f64, f64, i32)] = &[
            (Projection::Auto, 0.0, 0.0, 0.0, 100.0, 0),
            (Projection::Auto, 60.0, 20.0, 10.0, 70.0, 1),
            (Projection::Rectilinear, -150.0, -35.0, 0.0, 90.0, 2),
            (Projection::Stereographic, 170.0, 80.0, -30.0, 200.0, 3),
            (Projection::Auto, 0.0, 0.0, 0.0, 140.0, 1),
            (Projection::Planet, 30.0, 0.0, 0.0, 250.0, 0),
            (Projection::Rectilinear, 10.0, 0.0, 0.0, 20.0, 2),
        ];
        for &(projection, yaw, pitch, roll, fov, heading) in cases {
            let p = ViewParams { q_world_from_cam: read.q_world_from_cam, heading_yaw: read.headings[heading as usize].yaw, yaw, pitch, roll, fov, projection, stereo: 0 };
            let (stamp_texels, from_glsl) = glsl.render(&v, &luma, w, h, out, &p, heading);
            assert_eq!(stamp_texels[0], 1.0, "the GLSL read no stamp");
            let from_wgsl = v.render(Plane::U8(&luma), w, h, w, rows, out, &p).unwrap();
            let (worst, over) = compare(&from_glsl, &from_wgsl);
            eprintln!("{projection:?} yaw {yaw} pitch {pitch} roll {roll} fov {fov} heading {heading}: worst {:.2}/255, {:.4} % over half a code", worst * 255.0, over * 100.0);
            assert!(worst < 2.0 / 255.0 && over < 0.001, "{projection:?} {yaw} {pitch} {fov}: worst {}/255, {} % over half a code", worst * 255.0, over * 100.0);
            // And it isn't all one value
            let mean = from_wgsl.iter().sum::<f32>() / from_wgsl.len() as f32;
            assert!(from_wgsl.iter().any(|x| (x - mean).abs() > 0.1));
            // The comparison can tell: another mode's heading, or a degree more of yaw, shows
            let other = ViewParams { heading_yaw: read.headings[(heading as usize + 1) % 4].yaw, ..p };
            assert!(compare(&from_glsl, &v.render(Plane::U8(&luma), w, h, w, rows, out, &other).unwrap()).1 > 0.05);
            let off = ViewParams { yaw: yaw + 1.0, ..p };
            assert!(compare(&from_glsl, &v.render(Plane::U8(&luma), w, h, w, rows, out, &off).unwrap()).1 > 0.01);
        }
    }

    #[test]
    fn the_equirect_turned_by_nothing_is_the_sphere() {
        // The camera level and the heading 0: the 360 output is its input, but for the Mitchell filter's slight blur
        // (it isn't interpolating: 1/18, 16/18, 1/18 at the samples), on a smooth sphere
        let v = View360::new().unwrap();
        let w = 512usize;
        let s = sphere_with(w, 0, false);
        let out = v.render(Plane::U8(&s), w, w / 2, w, 0, (w, w / 2), &ViewParams { projection: Projection::Equirect, ..Default::default() }).unwrap();
        let worst = s.iter().zip(&out).enumerate().filter(|(i, _)| { let y = i / w; y > 2 && y < w / 2 - 3 })
            .map(|(_, (a, b))| (*a as f32 / 255.0 - b).abs()).fold(0.0f32, f32::max);
        assert!(worst < 1.0 / 255.0, "{}/255", worst * 255.0);
    }

    #[test]
    fn the_equirect_is_turned_into_the_world_and_the_heading() {
        // A camera turned 90° to the right (yaw) and the heading -30°: a point straight ahead of the camera lands
        // 90 - (-30) = 120° right of the output's middle. One bright pixel's way
        let v = View360::new().unwrap();
        let (w, h) = (720usize, 360usize);
        let mut s = vec![20u8; w * h];
        // Straight ahead in the camera: longitude 0, the middle
        for dy in 0..4 { for dx in 0..4 { s[(h / 2 - 2 + dy) * w + w / 2 - 2 + dx] = 250; } }
        let q = quat([0.0, 1.0, 0.0], 90.0);
        let out = v.render(Plane::U8(&s), w, h, w, 0, (w, h), &ViewParams { q_world_from_cam: q, heading_yaw: (-30f64).to_radians(), projection: Projection::Equirect, ..Default::default() }).unwrap();
        let (at, _) = out.iter().enumerate().fold((0, 0.0f32), |b, (i, v)| if *v > b.1 { (i, *v) } else { b });
        let (x, y) = ((at % w) as f64, (at / w) as f64);
        let lon = (x + 0.5) / w as f64 * 360.0 - 180.0;
        assert!((lon - 120.0).abs() < 1.5 && (y - h as f64 / 2.0).abs() < 2.0, "lon {lon} y {y}");
    }
}
