// SPDX-License-Identifier: GPL-3.0-or-later
//
// The iPhone app's motion side. The same as the Mac player's GPU path (gyroflow-gpu-table --player): a clip loaded by
// vs-gyroflow with the player's JSON, its table (each frame's kernel parameters and row rotations), and warp.glsl's
// exact pass. The pass is wrapped here as a plain fragment shader (the source read from the video's two planes, with
// the log curve's LUT) and translated to Metal by naga, so the app compiles it with MTLDevice.makeLibrary(source:).

use std::ffi::{ CStr, CString, c_char };
use vs_gyroflow::gpu_table::{ self, Mode, Options, PlayerClip };

/// What the app binds (to_msl's map): texture(0) luma, texture(1) chroma, texture(2) the table, texture(3) the LUT,
/// sampler(0) linear, buffer(0) the uniforms
const WRAP_HEAD: &str = r#"#version 450
layout(set = 0, binding = 0) uniform texture2D afp_luma;
layout(set = 0, binding = 1) uniform texture2D afp_chroma;
layout(set = 0, binding = 2) uniform texture2D afp_table;
layout(set = 0, binding = 3) uniform texture3D afp_lut;
layout(set = 0, binding = 4) uniform sampler afp_smp;
layout(set = 0, binding = 5) uniform AfpUniforms {
    vec4 pts_log_stab;      // x: the frame's time, s; y: LUT on (1); z: stabilized (1); w: LUT size
    vec4 sizes;             // xy: target (output) size in pixels; zw: the source's size in pixels
    vec4 cam;               // the output camera (crop mode): quaternion
    vec4 cam_zoom_range;    // x: zoom; y: limited range (1); z: output (0 as it is, 1 HLG, 2 PQ -> linear EDR); w: headroom
    mat4 yuv;               // Y'CbCr (0..1, range removed) -> R'G'B', the clip's matrix
} afp;
layout(location = 0) in vec2 afp_pos;
layout(location = 0) out vec4 afp_out;

#define PTS afp.pts_log_stab.x
#define target_size afp.sizes.xy
#define HOOKED_pos afp_pos
#define HOOKED_size afp.sizes.zw
#define HOOKED_pt (vec2(1.0) / afp.sizes.zw)
#define HOOKED_tex(p) afp_src(p)
#define GYROFLOW_TABLE sampler2D(afp_table, afp_smp)
#define gyroflow_cam_x afp.cam.x
#define gyroflow_cam_y afp.cam.y
#define gyroflow_cam_z afp.cam.z
#define gyroflow_cam_w afp.cam.w
#define gyroflow_cam_zoom afp.cam_zoom_range.x

// The source at normalized p: the planes read, made R'G'B', through the log curve's LUT when it has one
vec4 afp_src(vec2 p)
{
    float y = texture(sampler2D(afp_luma, afp_smp), p).r;
    vec2 c = texture(sampler2D(afp_chroma, afp_smp), p).rg;
    if (afp.cam_zoom_range.y > 0.5) {
        y = (y * 65535.0 / 64.0 - 64.0) / 876.0;
        c = (c * 65535.0 / 64.0 - 512.0) / 896.0;
    } else {
        c = c - 0.5;
    }
    vec3 rgb = clamp((afp.yuv * vec4(y, c, 1.0)).rgb, 0.0, 1.0);
    if (afp.pts_log_stab.y > 0.5) {
        float n = afp.pts_log_stab.w;
        rgb = texture(sampler3D(afp_lut, afp_smp), rgb * ((n - 1.0) / n) + 0.5 / n).rgb;
    }
    return vec4(rgb, 1.0);
}

// The signal made light for an EDR layer in extended linear BT.2020: 1.0 the SDR white (203 nits, HLG's 75 %), the
// highlights rolled off into the screen's headroom. HLG: BT.2100's inverse OETF and its OOTF for a 1000 nit screen
float afp_hlg_inv(float e) { return e <= 0.5 ? e * e / 3.0 : (exp((e - 0.55991073) / 0.17883277) + 0.28466892) / 12.0; }
float afp_pq_eotf(float e)
{
    float p = pow(clamp(e, 0.0, 1.0), 1.0 / 78.84375);
    return 10000.0 * pow(max(p - 0.8359375, 0.0) / (18.8515625 - 18.6875 * p), 1.0 / 0.1593017578125);
}
vec4 afp_present(vec4 c)
{
    float mode = afp.cam_zoom_range.z;
    if (mode < 0.5) return c;
    vec3 nits;
    if (mode < 1.5) {
        vec3 s = vec3(afp_hlg_inv(c.r), afp_hlg_inv(c.g), afp_hlg_inv(c.b));
        float ys = dot(s, vec3(0.2627, 0.6780, 0.0593));
        nits = 1000.0 * pow(max(ys, 1e-6), 0.2) * s;
    } else {
        nits = vec3(afp_pq_eotf(c.r), afp_pq_eotf(c.g), afp_pq_eotf(c.b));
    }
    vec3 l = nits / 203.0;
    float h = max(afp.cam_zoom_range.w, 1.0);
    float m = max(max(l.r, l.g), l.b);
    float knee = h > 1.25 ? 0.75 * h : h;
    if (m > knee) {
        float r = h - knee;
        float to = r > 0.0 ? knee + r * (1.0 - exp(-(m - knee) / r)) : h;
        l *= to / m;
    }
    return vec4(l, 1.0);
}
"#;

const WRAP_TAIL: &str = r#"
void main()
{
    if (afp.pts_log_stab.z > 0.5) afp_out = afp_present(hook());
    else afp_out = afp_present(afp_src(afp_pos));
}
"#;

/// The plain pass (no motion data, or stabilization off): the source fitted to the output, through the LUT
const PLAIN: &str = "\nvoid main() { afp_out = afp_present(afp_src(afp_pos)); }\n";

pub struct Warp {
    pub msl: String,
    pub table: Vec<f32>,
    pub table_w: usize,
    pub table_h: usize,
    pub src: (usize, usize),
    pub out: (usize, usize),
    pub fps: f64,
    pub frames: usize,
    /// Why it plays unstabilized, when it does
    pub note: String,
}

/// The widest texture the table may be here: the simulator's Metal takes 8192 (an iPhone 16384)
const MAX_W: usize = 8192;

/// The table with fewer frames a row, so it's at most `max_w` texels wide (the shader's COLS follows table.cols)
fn repack(t: &gpu_table::Table, max_w: usize) -> gpu_table::Table {
    let cols = (max_w / t.block).max(1);
    if cols >= t.cols { return gpu_table::Table { width: t.width, height: t.height, data: t.data.clone(), block: t.block, cols: t.cols, per_frame: t.per_frame }; }
    let frames = t.height * t.cols;
    let (width, height) = (cols * t.block, frames.div_ceil(cols));
    let mut data = vec![0f32; width * height * 4];
    let n = t.block * 4;
    for f in 0..frames {
        let src = ((f / t.cols) * t.width + (f % t.cols) * t.block) * 4;
        let dst = ((f / cols) * width + (f % cols) * t.block) * 4;
        if src + n <= t.data.len() { data[dst..dst + n].copy_from_slice(&t.data[src..src + n]); }
    }
    gpu_table::Table { width, height, data, block: t.block, cols, per_frame: t.per_frame }
}

/// GLSL -> MSL
pub fn to_msl(glsl: &str) -> Result<String, String> {
    let mut front = naga::front::glsl::Frontend::default();
    let module = front.parse(&naga::front::glsl::Options::from(naga::ShaderStage::Fragment), glsl)
        .map_err(|e| format!("GLSL: {}", e.emit_to_string(glsl)))?;
    let info = naga::valid::Validator::new(naga::valid::ValidationFlags::all(), naga::valid::Capabilities::all())
        .validate(&module).map_err(|e| format!("validation: {}", e.emit_to_string(glsl)))?;
    use naga::back::msl::{ BindSamplerTarget, BindTarget, EntryPointResources };
    let mut resources = naga::back::msl::BindingMap::new();
    let rb = |binding| naga::ResourceBinding { group: 0, binding };
    for t in 0..4u8 { resources.insert(rb(t as u32), BindTarget { texture: Some(t), ..Default::default() }); }
    resources.insert(rb(4), BindTarget { sampler: Some(BindSamplerTarget::Resource(0)), ..Default::default() });
    resources.insert(rb(5), BindTarget { buffer: Some(0), ..Default::default() });
    let mut map = naga::back::msl::EntryPointResourceMap::new();
    for ep in &module.entry_points { map.insert(ep.name.clone(), EntryPointResources { resources: resources.clone(), ..Default::default() }); }
    let options = naga::back::msl::Options { lang_version: (2, 4), per_entry_point_map: map, fake_missing_bindings: false, ..Default::default() };
    let pipeline = naga::back::msl::PipelineOptions::default();
    let (msl, _) = naga::back::msl::write_string(&module, &info, &options, &pipeline).map_err(|e| format!("MSL: {e}"))?;
    Ok(msl)
}

/// The shader with no stabilization: the source through the LUT
pub fn plain() -> Result<String, String> { to_msl(&format!("{WRAP_HEAD}{PLAIN}")) }

/// The clip in `json` (the player's keys: project = the video or its .gyroflow, input_width/height, level,
/// horizon_lock, interpolation, time_offset, …) as a stabilized pass and its table
pub fn build(json: &str) -> Result<Warp, String> {
    let j: serde_json::Value = serde_json::from_str(json).map_err(|e| format!("json: {e}"))?;
    let loaded = PlayerClip::load(&j)?;
    let clip = loaded.clip(&j);
    if let Some(why) = gpu_table::unsupported(&clip) {
        return Ok(Warp { msl: plain()?, table: vec![0.0; 4], table_w: 1, table_h: 1, src: clip.size, out: clip.size, fps: clip.fps,
                         frames: clip.frame_count, note: format!("not stabilized on the GPU: {why}") });
    }
    let s = |k: &str| j.get(k).and_then(|v| v.as_str()).unwrap_or("");
    let interp = match s("interpolation") { "bilinear" => 0, "bspline" => 1, "bicubic" => 2, "lanczos" | "lanczos4" => 3, "ewa" => 4, _ => 5 };
    let opts = Options { mode: Mode::Exact, grid_step: 8, grid_linear: true, chroma_bilinear: false, chromatic: false, chromatic_fast: true,
                         edge_fill: None, sphere: false, time_offset: j.get("time_offset").and_then(|v| v.as_f64()).unwrap_or(0.0),
                         denoise: false, knots: 33, interp, shrink: false, range: None };
    let view = loaded.view(&j);
    let built = loaded.table(view, &clip, &opts)?;
    let table = repack(&built, MAX_W);
    let glsl = format!("{WRAP_HEAD}\n{}\n{WRAP_TAIL}", gpu_table::exact_glsl(&clip, &opts, &table));
    let msl = to_msl(&glsl).map_err(|e| { if std::env::var_os("AFP_DUMP").is_some() { let _ = std::fs::write("/tmp/afp-warp.glsl", &glsl); } e })?;
    Ok(Warp { msl, table: table.data.clone(), table_w: table.width, table_h: table.height, src: clip.size, out: clip.output_size,
              fps: clip.fps, frames: clip.frame_count, note: String::new() })
}

/// The mpv user shader for the clip, as `gyroflow-gpu-table --player <json> <out>` writes it for the Mac player (the
/// same JSON keys, the same reply), here in the app's process: iOS starts no programs. The table at most MAX_W wide
pub fn mpv_shader(json: &str, out: &str) -> serde_json::Value {
    let j: serde_json::Value = match serde_json::from_str(json) { Ok(j) => j, Err(e) => return serde_json::json!({ "error": format!("bad JSON: {e}") }) };
    let loaded = match PlayerClip::load(&j) { Ok(l) => l, Err(e) => return serde_json::json!({ "error": e }) };
    let clip = loaded.clip(&j);
    if let Some(why) = gpu_table::unsupported(&clip) { return serde_json::json!({ "unsupported": why }); }
    let s = |k: &str| j.get(k).and_then(|v| v.as_str()).unwrap_or("");
    let interp = match s("interpolation") { "bilinear" => 0, "lanczos" | "lanczos4" => 3, "ewa" => 4, "bspline" => 1, _ => 5 };
    let chromatic = j.get("gpu_chromatic").and_then(|v| v.as_bool()).unwrap_or(false);
    let view = loaded.view(&j);
    let wet = chromatic && loaded.wet(view, &clip);
    let edge_fill = match s("edges") { "smooth" | "sphere" => Some(j.get("edges_blur").and_then(|v| v.as_f64()).unwrap_or(2.0) as f32 / 100.0), _ => None };
    let sphere = s("edges") == "sphere" && j.get("gpu_sphere").and_then(|v| v.as_bool()).unwrap_or(true);
    let rgb = wet || sphere;
    let opts = Options { mode: if rgb { Mode::Grid } else { Mode::Planes }, grid_step: 8, grid_linear: true, chroma_bilinear: interp == 5,
                         chromatic: wet, chromatic_fast: true, edge_fill, sphere,
                         time_offset: j.get("time_offset").and_then(|v| v.as_f64()).unwrap_or(0.0),
                         denoise: j.get("gpu_denoise").and_then(|v| v.as_bool()).unwrap_or(false), knots: 33, interp, shrink: false, range: None };
    let built = match loaded.table(view, &clip, &opts) { Ok(t) => t, Err(e) => return serde_json::json!({ "error": e }) };
    let table = repack(&built, MAX_W);
    if let Err(e) = std::fs::write(out, gpu_table::shader(&clip, &opts, &table)) { return serde_json::json!({ "error": format!("{out}: {e}") }); }
    serde_json::json!({ "shader": out, "output": [clip.output_size.0, clip.output_size.1], "mode": if rgb { "grid" } else { "planes" },
                        "chromatic": wet, "sphere": sphere })
}

/// mpv_shader for C: the reply as JSON; free with afp_string_free
#[unsafe(no_mangle)]
pub extern "C" fn afp_mpv_shader(json: *const c_char, out: *const c_char) -> *mut c_char {
    let get = |p: *const c_char| if p.is_null() { String::new() } else { unsafe { CStr::from_ptr(p) }.to_string_lossy().into_owned() };
    let (json, out) = (get(json), get(out));
    let reply = std::panic::catch_unwind(|| mpv_shader(&json, &out)).unwrap_or_else(|_| serde_json::json!({ "error": "the motion library panicked" }));
    cstr(&reply.to_string())
}

// ---- C interface (ios/afp-motion/include/afp_motion.h) ----

#[repr(C)]
pub struct AfpWarp {
    /// The Metal source, or null with `error` set
    pub msl: *mut c_char,
    pub error: *mut c_char,
    pub note: *mut c_char,
    /// rgba32f texels, table_w x table_h
    pub table: *mut f32,
    pub table_w: u32,
    pub table_h: u32,
    pub src_w: u32,
    pub src_h: u32,
    pub out_w: u32,
    pub out_h: u32,
    pub fps: f64,
    pub frames: u32,
}

fn cstr(s: &str) -> *mut c_char { CString::new(s.replace('\0', " ")).unwrap().into_raw() }

/// Builds the warp for the player's JSON. Never null; free with afp_warp_free
#[unsafe(no_mangle)]
pub extern "C" fn afp_warp_build(json: *const c_char) -> *mut AfpWarp {
    let json = if json.is_null() { "" } else { unsafe { CStr::from_ptr(json) }.to_str().unwrap_or("") };
    let result = std::panic::catch_unwind(|| build(json)).unwrap_or_else(|_| Err("the motion library panicked".into()));
    let w = match result {
        Ok(w) => {
            let mut table = w.table.into_boxed_slice();
            let ptr = table.as_mut_ptr();
            std::mem::forget(table);
            AfpWarp { msl: cstr(&w.msl), error: std::ptr::null_mut(), note: cstr(&w.note), table: ptr, table_w: w.table_w as u32, table_h: w.table_h as u32,
                      src_w: w.src.0 as u32, src_h: w.src.1 as u32, out_w: w.out.0 as u32, out_h: w.out.1 as u32, fps: w.fps, frames: w.frames as u32 }
        }
        Err(e) => AfpWarp { msl: std::ptr::null_mut(), error: cstr(&e), note: std::ptr::null_mut(), table: std::ptr::null_mut(), table_w: 0, table_h: 0,
                            src_w: 0, src_h: 0, out_w: 0, out_h: 0, fps: 0.0, frames: 0 },
    };
    Box::into_raw(Box::new(w))
}

/// The plain shader (no stabilization), Metal source; free with afp_string_free
#[unsafe(no_mangle)]
pub extern "C" fn afp_plain_msl() -> *mut c_char {
    match plain() { Ok(s) => cstr(&s), Err(e) => cstr(&format!("//error {e}")) }
}

#[unsafe(no_mangle)]
pub extern "C" fn afp_string_free(s: *mut c_char) { if !s.is_null() { drop(unsafe { CString::from_raw(s) }); } }

#[unsafe(no_mangle)]
pub extern "C" fn afp_warp_free(w: *mut AfpWarp) {
    if w.is_null() { return; }
    let w = unsafe { Box::from_raw(w) };
    afp_string_free(w.msl);
    afp_string_free(w.error);
    afp_string_free(w.note);
    if !w.table.is_null() {
        let n = w.table_w as usize * w.table_h as usize * 4;
        drop(unsafe { Box::from_raw(std::ptr::slice_from_raw_parts_mut(w.table, n)) });
    }
}
