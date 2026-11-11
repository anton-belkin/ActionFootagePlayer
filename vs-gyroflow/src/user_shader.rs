// SPDX-License-Identifier: GPL-3.0-or-later
//
// The player's mpv user shaders run without mpv, on wgpu, for gyroflow-export: so an export carries what the player
// draws over the stabilized frame, by the same shader code. Two of them so far:
//   - the water's color fringes taken out: mpv/scripts/gyroflow/chromatic.glsl, which reads the frame's lens geometry
//     from the marker the plugin writes into its last row of luma (dispersion.rs)
//   - the temporal denoise (Ctrl+p): the GPU path's, shaders/gyroflow/warp.glsl's dn_ passes (gpu_table::denoise_shader)
//
// libplacebo's hook format as those use it: PARAM (DYNAMIC or CONSTANT, float or int), TEXTURE (data in hex, or
// STORAGE), BUFFER (STORAGE, its VARs), and passes hooking LUMA or MAIN (LUMA's first, then MAIN's, each in file
// order) with BIND, SAVE, WIDTH and HEIGHT (RPN), WHEN, COMPUTE. What libplacebo declares around a pass is declared
// here: for each texture it binds NAME_raw, _pos, _size, _pt, _mul, _tex(), _texOff(), the parameters, its storage
// images and buffers; then naga's GLSL front end compiles it. That has no atomics: an atomicAdd on a buffer's uint
// array goes in as a call to a stub, made the atomic add in naga's IR (patch_atomics).
//
// Frames come and go as gyroflow-export has them, 10-bit YUV 4:2:0 (yuv420p10le). MAIN is the frame in RGB as
// libplacebo makes it (the source's matrix and range, not clamped; the chroma between its samples, which sit with
// the even columns and between the rows: MPEG-2's), in rgba16f as mpv's passes are; LUMA its luma as coded (code /
// 1023). After the passes MAIN goes back to YUV in the same matrix and range, each chroma sample the mean of its 2x2
// pixels (as gpu_lut.rs makes it).

use std::borrow::Cow;
use std::collections::HashMap;
use std::sync::mpsc;

use wgpu::naga;

#[derive(Clone, Copy, PartialEq, Debug)]
enum Kind { Float, Int }

#[derive(Clone, Debug)]
struct Param { name: String, kind: Kind, value: f64 }

#[derive(Clone, Debug)]
struct TextureDecl { name: String, size: (u32, u32), format: String, linear: bool, storage: bool, hex: String }

#[derive(Clone, Debug)]
struct BufferDecl { name: String, vars: Vec<String> }

#[derive(Clone, Debug, Default)]
struct PassDecl {
    hooks: Vec<String>,
    binds: Vec<String>,
    save: Option<String>,
    width: Option<String>,
    height: Option<String>,
    when: Option<String>,
    /// Block width and height, threads across and down
    compute: Option<[u32; 4]>,
    desc: String,
    body: String,
}

#[derive(Default)]
struct Parsed { params: Vec<Param>, textures: Vec<TextureDecl>, buffers: Vec<BufferDecl>, passes: Vec<PassDecl> }

/// One shader file's blocks into `out`
fn parse(text: &str, out: &mut Parsed) -> Result<(), String> {
    let text = text.replace("\r\n", "\n");
    let lines: Vec<&str> = text.split('\n').collect();
    let directive = |l: &str| l.trim_start().strip_prefix("//!").map(|d| d.trim().to_string());
    let starter = |d: &str| ["HOOK", "TEXTURE", "BUFFER", "PARAM"].contains(&d.split_whitespace().next().unwrap_or(""));
    let mut i = 0;
    while i < lines.len() && !directive(lines[i]).is_some_and(|d| starter(&d)) { i += 1; }
    while i < lines.len() {
        let first = directive(lines[i]).unwrap();
        let kind = first.split_whitespace().next().unwrap_or("").to_string();
        let mut dirs = vec![first];
        i += 1;
        // Its directives: up to a line that isn't one, or another block's (a pass may hook more than one stage)
        while i < lines.len() {
            let Some(d) = directive(lines[i]) else { break };
            if starter(&d) && !(kind == "HOOK" && d.starts_with("HOOK")) { break; }
            dirs.push(d);
            i += 1;
        }
        let start = i;
        while i < lines.len() && !directive(lines[i]).is_some_and(|d| starter(&d)) { i += 1; }
        let body = lines[start..i].join("\n");
        let arg = |d: &str| d.split_once(char::is_whitespace).map(|(_, a)| a.trim().to_string()).unwrap_or_default();
        let key = |d: &str| d.split_whitespace().next().unwrap_or("").to_string();
        match kind.as_str() {
            "PARAM" => {
                let mut p = Param { name: arg(&dirs[0]), kind: Kind::Float, value: 0.0 };
                for d in &dirs[1..] {
                    if key(d) == "TYPE" {
                        let t = arg(d);
                        if t.split_whitespace().any(|w| w == "int" || w == "uint") { p.kind = Kind::Int; }
                    }
                }
                p.value = body.trim().parse::<f64>().unwrap_or(0.0);
                out.params.push(p);
            }
            "TEXTURE" => {
                let mut t = TextureDecl { name: arg(&dirs[0]), size: (1, 1), format: "rgba16f".into(), linear: true, storage: false, hex: String::new() };
                for d in &dirs[1..] {
                    match key(d).as_str() {
                        "SIZE" => {
                            let v: Vec<u32> = arg(d).split_whitespace().filter_map(|x| x.parse().ok()).collect();
                            t.size = (*v.first().unwrap_or(&1), *v.get(1).unwrap_or(&1));
                        }
                        "FORMAT" => t.format = arg(d),
                        "FILTER" => t.linear = arg(d) != "NEAREST",
                        "STORAGE" => t.storage = true,
                        _ => {}
                    }
                }
                t.hex = body.chars().filter(|c| c.is_ascii_hexdigit()).collect();
                out.textures.push(t);
            }
            "BUFFER" => {
                let mut b = BufferDecl { name: arg(&dirs[0]), vars: Vec::new() };
                for d in &dirs[1..] {
                    if key(d) == "VAR" { b.vars.push(arg(d)); }
                }
                out.buffers.push(b);
            }
            _ => {
                let mut p = PassDecl { body, ..Default::default() };
                for d in &dirs {
                    let a = arg(d);
                    match key(d).as_str() {
                        "HOOK" => p.hooks.push(a),
                        "BIND" => p.binds.push(a),
                        "SAVE" => p.save = Some(a),
                        "WIDTH" => p.width = Some(a),
                        "HEIGHT" => p.height = Some(a),
                        "WHEN" => p.when = Some(a),
                        "DESC" => p.desc = a,
                        "COMPUTE" => {
                            let v: Vec<u32> = a.split_whitespace().filter_map(|x| x.parse().ok()).collect();
                            if v.len() < 2 { return Err(format!("COMPUTE {a}: two numbers at least")); }
                            p.compute = Some([v[0], v[1], *v.get(2).unwrap_or(&v[0]), *v.get(3).unwrap_or(&v[1])]);
                        }
                        _ => {}
                    }
                }
                out.passes.push(p);
            }
        }
    }
    Ok(())
}

/// libplacebo's size and condition expressions: RPN over numbers, NAME.w / NAME.h (NAME.width / .height), the
/// parameters, + - * / % > < = and ! (not)
fn rpn(expr: &str, size: &dyn Fn(&str) -> Option<(u32, u32)>, param: &dyn Fn(&str) -> Option<f64>) -> Result<f64, String> {
    let mut st: Vec<f64> = Vec::new();
    for tok in expr.split_whitespace() {
        let v = if let Ok(x) = tok.parse::<f64>() { x } else if let Some((n, c)) = tok.rsplit_once('.') {
            let s = size(n).ok_or_else(|| format!("{expr}: no texture {n}"))?;
            match c { "w" | "width" => s.0 as f64, "h" | "height" => s.1 as f64, _ => return Err(format!("{expr}: {tok}")) }
        } else if tok == "!" {
            let a = st.pop().ok_or_else(|| format!("{expr}: nothing for !"))?;
            if a == 0.0 { 1.0 } else { 0.0 }
        } else if ["+", "-", "*", "/", "%", ">", "<", "="].contains(&tok) {
            let b = st.pop().ok_or_else(|| format!("{expr}: nothing for {tok}"))?;
            let a = st.pop().ok_or_else(|| format!("{expr}: nothing for {tok}"))?;
            match tok {
                "+" => a + b, "-" => a - b, "*" => a * b, "/" => a / b, "%" => a % b,
                ">" => (a > b) as i32 as f64, "<" => (a < b) as i32 as f64, _ => (a == b) as i32 as f64,
            }
        } else {
            param(tok).ok_or_else(|| format!("{expr}: no parameter {tok}"))?
        };
        st.push(v);
    }
    st.pop().ok_or_else(|| format!("empty expression {expr:?}"))
}

/// A format name as libplacebo has it: wgpu's, and GLSL's image qualifier
fn format(name: &str) -> Option<(wgpu::TextureFormat, &'static str)> {
    use wgpu::TextureFormat as F;
    Some(match name {
        "rgba16f" => (F::Rgba16Float, "rgba16f"),
        "rgba32f" => (F::Rgba32Float, "rgba32f"),
        "rgb10a2" => (F::Rgb10a2Unorm, "rgb10_a2"),
        "r32f" => (F::R32Float, "r32f"),
        "rg32f" => (F::Rg32Float, "rg32f"),
        "r16f" => (F::R16Float, "r16f"),
        "rg16f" => (F::Rg16Float, "rg16f"),
        "rgba8" => (F::Rgba8Unorm, "rgba8"),
        _ => return None,
    })
}

/// A buffer's VAR declaration: its name, and whether it's an array of uint (one an atomicAdd can go to)
fn var_name(decl: &str) -> (String, bool) {
    let mut words = decl.split_whitespace();
    let ty = words.next().unwrap_or("");
    let rest: String = words.collect::<Vec<_>>().join("");
    let name = rest.split('[').next().unwrap_or("").trim_end_matches(';').to_string();
    (name, ty == "uint" && rest.contains('['))
}

/// `atomicAdd(name[index], value)` on a buffer's uint array as `afp_atomicAdd_name(index, value)`, which
/// patch_atomics makes the atomic add; whether there was one
fn rewrite_atomics(body: &str, arrays: &[String]) -> Result<(String, Vec<String>), String> {
    let mut out = String::with_capacity(body.len());
    let mut used = Vec::new();
    let mut rest = body;
    while let Some(at) = rest.find("atomicAdd(") {
        out.push_str(&rest[..at]);
        let args_start = at + "atomicAdd(".len();
        // The call's arguments, to its closing parenthesis
        let bytes = rest.as_bytes();
        let (mut depth, mut end, mut comma) = (1i32, None, None);
        for (k, &c) in bytes.iter().enumerate().skip(args_start) {
            match c {
                b'(' | b'[' => depth += 1,
                b')' | b']' => { depth -= 1; if depth == 0 { end = Some(k); break; } }
                b',' if depth == 1 && comma.is_none() => comma = Some(k),
                _ => {}
            }
        }
        let (end, comma) = (end.ok_or("atomicAdd without its )")?, comma.ok_or("atomicAdd without a value")?);
        let target = rest[args_start..comma].trim();
        let value = rest[comma + 1..end].trim();
        let (name, index) = target.split_once('[').ok_or_else(|| format!("atomicAdd({target}): not an array's element"))?;
        let name = name.trim();
        if !arrays.iter().any(|a| a == name) { return Err(format!("atomicAdd on {name}: only a buffer's uint array")); }
        let index = index.trim().strip_suffix(']').ok_or_else(|| format!("atomicAdd({target})"))?;
        out.push_str(&format!("afp_atomicAdd_{name}({index}, {value})"));
        if !used.iter().any(|u| u == name) { used.push(name.to_string()); }
        rest = &rest[end + 1..];
    }
    out.push_str(rest);
    Ok((out, used))
}

/// GLSL's barrier() as the workgroup's (with its storage and images), as GLSL means it: naga's GLSL front end makes it
/// every kind of barrier, a subgroup's too, which wgpu then refuses without the subgroup capability
fn patch_barriers(m: &mut naga::Module) {
    fn block(b: &mut naga::Block) {
        use naga::Statement as S;
        for s in b.iter_mut() {
            match s {
                S::ControlBarrier(f) | S::MemoryBarrier(f) => f.remove(naga::Barrier::SUB_GROUP),
                S::Block(x) => block(x),
                S::If { accept, reject, .. } => { block(accept); block(reject); }
                S::Loop { body, continuing, .. } => { block(body); block(continuing); }
                S::Switch { cases, .. } => for c in cases.iter_mut() { block(&mut c.body) },
                _ => {}
            }
        }
    }
    for (_, f) in m.functions.iter_mut() { block(&mut f.body); }
    for ep in m.entry_points.iter_mut() { block(&mut ep.function.body); }
}

/// The stubs' calls (rewrite_atomics) made atomic adds: the buffer's member they name an array of atomic<u32>, each
/// call `atomicAdd(&member[index], value)`
fn patch_atomics(m: &mut naga::Module) -> Result<(), String> {
    use naga::{ Expression, Handle, Scalar, Span, Statement, Type, TypeInner };
    let stubs: Vec<(Handle<naga::Function>, String)> = m.functions.iter()
        .filter_map(|(h, f)| Some((h, f.name.as_deref()?.strip_prefix("afp_atomicAdd_")?.to_string()))).collect();
    if stubs.is_empty() { return Ok(()); }
    // Each stub: the global and its member's index
    let mut targets: HashMap<Handle<naga::Function>, (Handle<naga::GlobalVariable>, u32)> = HashMap::new();
    for (stub, member) in &stubs {
        let found = m.global_variables.iter().find_map(|(gv, var)| match &m.types[var.ty].inner {
            TypeInner::Struct { members, .. } => members.iter().position(|mm| mm.name.as_deref() == Some(member)).map(|i| (gv, i as u32)),
            _ => None,
        }).ok_or_else(|| format!("atomicAdd: no buffer has {member}"))?;
        targets.insert(*stub, found);
    }
    // The members' types: array<u32> -> array<atomic<u32>> (the same layout)
    let mut done: Vec<(Handle<naga::GlobalVariable>, u32)> = Vec::new();
    for &(gv, idx) in targets.values() {
        if done.contains(&(gv, idx)) { continue; }
        done.push((gv, idx));
        let ty = m.types[m.global_variables[gv].ty].clone();
        let TypeInner::Struct { mut members, span } = ty.inner else { unreachable!() };
        let (size, stride) = match m.types[members[idx as usize].ty].inner {
            TypeInner::Array { base, size, stride } if m.types[base].inner == TypeInner::Scalar(Scalar::U32) => (size, stride),
            _ => return Err(format!("atomicAdd: {} isn't an array of uint", members[idx as usize].name.clone().unwrap_or_default())),
        };
        let atomic = m.types.insert(Type { name: None, inner: TypeInner::Atomic(Scalar::U32) }, Span::UNDEFINED);
        let array = m.types.insert(Type { name: None, inner: TypeInner::Array { base: atomic, size, stride } }, Span::UNDEFINED);
        members[idx as usize].ty = array;
        let new = m.types.insert(Type { name: ty.name.clone(), inner: TypeInner::Struct { members, span } }, Span::UNDEFINED);
        m.global_variables[gv].ty = new;
    }
    fn patch_block(block: &mut naga::Block, exprs: &mut naga::Arena<Expression>, targets: &HashMap<Handle<naga::Function>, (Handle<naga::GlobalVariable>, u32)>) -> Result<(), String> {
        let mut k = 0;
        while k < block.len() {
            let replace = match &mut block[k] {
                Statement::Call { function, arguments, result } if targets.contains_key(function) => {
                    if result.is_some() || arguments.len() != 2 { return Err("atomicAdd: only as a statement".into()); }
                    let (gv, idx) = targets[function];
                    let (index, value) = (arguments[0], arguments[1]);
                    let global = exprs.append(Expression::GlobalVariable(gv), Span::UNDEFINED);
                    let before = exprs.len();
                    let member = exprs.append(Expression::AccessIndex { base: global, index: idx }, Span::UNDEFINED);
                    let element = exprs.append(Expression::Access { base: member, index }, Span::UNDEFINED);
                    let range = exprs.range_from(before);
                    Some(naga::Block::from_vec(vec![Statement::Emit(range),
                        Statement::Atomic { pointer: element, fun: naga::AtomicFunction::Add, value, result: None }]))
                }
                Statement::Block(b) => { patch_block(b, exprs, targets)?; None }
                Statement::If { accept, reject, .. } => { patch_block(accept, exprs, targets)?; patch_block(reject, exprs, targets)?; None }
                Statement::Loop { body, continuing, .. } => { patch_block(body, exprs, targets)?; patch_block(continuing, exprs, targets)?; None }
                Statement::Switch { cases, .. } => { for c in cases.iter_mut() { patch_block(&mut c.body, exprs, targets)?; } None }
                _ => None,
            };
            match replace {
                Some(b) => { let n = b.len(); block.splice(k..k + 1, b); k += n; }
                None => k += 1,
            }
        }
        Ok(())
    }
    let handles: Vec<_> = m.functions.iter().map(|(h, _)| h).collect();
    for h in handles {
        let f = &mut m.functions[h];
        patch_block(&mut f.body, &mut f.expressions, &targets)?;
    }
    for ep in m.entry_points.iter_mut() {
        patch_block(&mut ep.function.body, &mut ep.function.expressions, &targets)?;
    }
    Ok(())
}

// ---------------------------------------------------------------------------------------------------
// The frame in and out

const CONVERT: &str = r#"
struct C {
    w: u32, h: u32, full: u32, words: u32,
    kr: f32, kb: f32, _a: f32, _b: f32,
}
@group(0) @binding(0) var<uniform> c: C;

fn y_range() -> vec2<f32> { return select(vec2<f32>(64.0, 876.0), vec2<f32>(0.0, 1023.0), c.full == 1u); }
fn c_scale() -> f32 { return select(896.0, 1023.0, c.full == 1u); }
"#;

const UNPACK: &str = r#"
@group(0) @binding(1) var<storage, read> src: array<u32>;
@group(0) @binding(2) var luma: texture_storage_2d<rgba16float, write>;
@group(0) @binding(3) var rgb: texture_storage_2d<rgba16float, write>;

fn s(i: u32) -> f32 {
    let w = src[i >> 1u];
    return f32(select(w & 0xffffu, w >> 16u, (i & 1u) == 1u));
}

@compute @workgroup_size(8, 8, 1)
fn main(@builtin(global_invocation_id) id: vec3<u32>) {
    if (id.x >= c.w || id.y >= c.h) { return; }
    let y = s(id.y * c.w + id.x);
    let cw = c.w / 2u;
    let ch = c.h / 2u;
    let ub = c.w * c.h;
    let vb = ub + cw * ch;
    // The chroma's samples sit with the even columns, between the rows
    let cx = f32(id.x) * 0.5;
    let cy = (f32(id.y) - 0.5) * 0.5;
    let x0 = floor(cx);
    let y0 = floor(cy);
    let f = vec2<f32>(cx - x0, cy - y0);
    let xa = u32(clamp(x0, 0.0, f32(cw - 1u)));
    let xb = u32(clamp(x0 + 1.0, 0.0, f32(cw - 1u)));
    let ya = u32(clamp(y0, 0.0, f32(ch - 1u)));
    let yb = u32(clamp(y0 + 1.0, 0.0, f32(ch - 1u)));
    let u = mix(mix(s(ub + ya * cw + xa), s(ub + ya * cw + xb), f.x), mix(s(ub + yb * cw + xa), s(ub + yb * cw + xb), f.x), f.y);
    let v = mix(mix(s(vb + ya * cw + xa), s(vb + ya * cw + xb), f.x), mix(s(vb + yb * cw + xa), s(vb + yb * cw + xb), f.x), f.y);
    let o = y_range();
    let yn = (y - o.x) / o.y;
    let cb = (u - 512.0) / c_scale();
    let cr = (v - 512.0) / c_scale();
    let r = yn + 2.0 * (1.0 - c.kr) * cr;
    let b = yn + 2.0 * (1.0 - c.kb) * cb;
    let g = (yn - c.kr * r - c.kb * b) / (1.0 - c.kr - c.kb);
    textureStore(luma, vec2<i32>(id.xy), vec4<f32>(y / 1023.0, 0.0, 0.0, 1.0));
    textureStore(rgb, vec2<i32>(id.xy), vec4<f32>(r, g, b, 1.0));
}
"#;

const TO_YUV: &str = r#"
@group(0) @binding(1) var res: texture_2d<f32>;
@group(0) @binding(2) var<storage, read_write> samples: array<u32>;

fn code(v: f32) -> u32 { return u32(clamp(round(v), 0.0, 1023.0)); }

// A chroma sample each: its 2x2 pixels' luma, and the chroma of their mean
@compute @workgroup_size(8, 8, 1)
fn main(@builtin(global_invocation_id) id: vec3<u32>) {
    let cw = c.w / 2u;
    let ch = c.h / 2u;
    if (id.x >= cw || id.y >= ch) { return; }
    let o = y_range();
    var sb = 0.0;
    var sr = 0.0;
    for (var k = 0u; k < 4u; k++) {
        let x = id.x * 2u + (k & 1u);
        let y = id.y * 2u + (k >> 1u);
        let p = textureLoad(res, vec2<i32>(i32(x), i32(y)), 0).rgb;
        let yv = c.kr * p.r + (1.0 - c.kr - c.kb) * p.g + c.kb * p.b;
        samples[y * c.w + x] = code(o.x + o.y * yv);
        sb += (p.b - yv) / (2.0 * (1.0 - c.kb));
        sr += (p.r - yv) / (2.0 * (1.0 - c.kr));
    }
    let ub = c.w * c.h;
    samples[ub + id.y * cw + id.x] = code(512.0 + c_scale() * sb * 0.25);
    samples[ub + cw * ch + id.y * cw + id.x] = code(512.0 + c_scale() * sr * 0.25);
}
"#;

const PACK: &str = r#"
@group(0) @binding(1) var<storage, read> samples: array<u32>;
@group(0) @binding(2) var<storage, read_write> words: array<u32>;

@compute @workgroup_size(256, 1, 1)
fn main(@builtin(global_invocation_id) id: vec3<u32>) {
    let i = id.x + id.y * 65535u * 256u;
    if (i >= c.words) { return; }
    words[i] = samples[2u * i] | (samples[2u * i + 1u] << 16u);
}
"#;

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct ConvertParams { w: u32, h: u32, full: u32, words: u32, kr: f32, kb: f32, _a: f32, _b: f32 }

// ---------------------------------------------------------------------------------------------------

/// What runs: the shader files, in the order mpv would load them
pub struct Config {
    pub sources: Vec<String>,
    /// The frame's size (MAIN's, LUMA's and the output's)
    pub size: (usize, usize),
    /// Defines put before every pass (a shader's own RT, say)
    pub prelude: String,
    /// A TEXTURE's data (rgba32f texels, row by row) in place of its hex, by name
    pub data: Vec<(String, Vec<f32>)>,
    /// The YUV matrix (Kr, Kb) and range of the frames
    pub matrix: (f32, f32),
    pub full_range: bool,
    /// The parameters' values in place of their defaults
    pub params: Vec<(String, f64)>,
}

struct Tex {
    /// Kept with its view
    _texture: wgpu::Texture,
    view: wgpu::TextureView,
    size: (u32, u32),
    format: wgpu::TextureFormat,
    /// The image qualifier, for a storage texture
    glsl: &'static str,
    storage: bool,
    filterable: bool,
    linear: bool,
}

enum Pipeline { Render(wgpu::RenderPipeline), Compute(wgpu::ComputePipeline, (u32, u32)) }

/// A pass as it runs: its pipeline, its bindings, where it writes
struct Step {
    pipeline: Pipeline,
    group: wgpu::BindGroup,
    /// A fragment pass's target
    target: Option<String>,
}

pub struct Shaders {
    device: wgpu::Device,
    queue: wgpu::Queue,
    parsed: Parsed,
    prelude: String,
    size: (u32, u32),
    params: Vec<Param>,
    uniforms: wgpu::Buffer,
    textures: HashMap<String, Tex>,
    buffers: HashMap<String, wgpu::Buffer>,
    linear: wgpu::Sampler,
    nearest: wgpu::Sampler,
    /// The passes as planned for the conditions they were planned with (WHEN): the steps, and MAIN's texture after them
    plan: Option<(Vec<bool>, Vec<Step>, String)>,
    compiled: HashMap<usize, Pipeline>,
    convert: wgpu::Buffer,
    frame_in: wgpu::Buffer,
    samples: wgpu::Buffer,
    frame_out: wgpu::Buffer,
    staging: wgpu::Buffer,
    unpack: (wgpu::ComputePipeline, wgpu::BindGroup),
    to_yuv: wgpu::ComputePipeline,
    pack: (wgpu::ComputePipeline, wgpu::BindGroup),
    frame_bytes: u64,
    /// GPU time per frame, for the log
    pub millis: f64,
}

fn scoped<T>(device: &wgpu::Device, what: &str, f: impl FnOnce() -> T) -> Result<T, String> {
    let scope = device.push_error_scope(wgpu::ErrorFilter::Validation);
    let v = f();
    match pollster::block_on(scope.pop()) {
        Some(e) => Err(format!("{what}: {e}")),
        None => Ok(v),
    }
}

impl Shaders {
    pub fn new(cfg: Config) -> Result<Shaders, String> {
        let (w, h) = (cfg.size.0 as u32, cfg.size.1 as u32);
        if w % 2 != 0 || h % 2 != 0 || w == 0 || h == 0 { return Err(format!("{w}x{h}: even sizes only")); }
        let mut parsed = Parsed::default();
        for s in &cfg.sources { parse(s, &mut parsed)?; }
        let mut params = parsed.params.clone();
        for (n, v) in &cfg.params {
            match params.iter_mut().find(|p| &p.name == n) {
                Some(p) => p.value = *v,
                None => return Err(format!("no parameter {n}")),
            }
        }

        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            force_fallback_adapter: false,
            compatible_surface: None,
            apply_limit_buckets: false,
        })).map_err(|e| format!("no GPU: {e}"))?;
        let wanted = wgpu::Features::TEXTURE_ADAPTER_SPECIFIC_FORMAT_FEATURES | wgpu::Features::FLOAT32_FILTERABLE;
        let features = adapter.features() & wanted;
        let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some("gyroflow-export shaders"),
            required_features: features,
            required_limits: adapter.limits(),
            memory_hints: wgpu::MemoryHints::Performance,
            trace: wgpu::Trace::Off,
            experimental_features: wgpu::ExperimentalFeatures::disabled(),
        })).map_err(|e| format!("GPU device: {e}"))?;
        let max = device.limits().max_texture_dimension_2d;
        let rw = |f: wgpu::TextureFormat| features.contains(wgpu::Features::TEXTURE_ADAPTER_SPECIFIC_FORMAT_FEATURES)
            && adapter.get_texture_format_features(f).flags.contains(wgpu::TextureFormatFeatureFlags::STORAGE_READ_WRITE);
        let float32_filterable = features.contains(wgpu::Features::FLOAT32_FILTERABLE);

        let mut textures = HashMap::new();
        let make = |name: &str, size: (u32, u32), format: wgpu::TextureFormat, usage: wgpu::TextureUsages| {
            device.create_texture(&wgpu::TextureDescriptor {
                label: Some(name), size: wgpu::Extent3d { width: size.0, height: size.1, depth_or_array_layers: 1 }, mip_level_count: 1,
                sample_count: 1, dimension: wgpu::TextureDimension::D2, format, usage, view_formats: &[],
            })
        };
        // The shaders' own textures: data, or storage (read and written by the passes)
        for t in &parsed.textures {
            if t.size.0 > max || t.size.1 > max { return Err(format!("{}: {}x{} is more than this GPU's {max}", t.name, t.size.0, t.size.1)); }
            let (mut format, mut glsl) = format(&t.format).ok_or_else(|| format!("{}: format {}", t.name, t.format))?;
            if t.storage && !rw(format) {
                // The denoise's ring is rgb10a2 in mpv, half of rgba16f's memory; where wgpu can't read and write that
                // in a shader, rgba16f
                if rw(wgpu::TextureFormat::Rgba16Float) { (format, glsl) = (wgpu::TextureFormat::Rgba16Float, "rgba16f"); }
                else { return Err(format!("{}: no read-write storage textures on this GPU", t.name)); }
            }
            let usage = wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST | if t.storage { wgpu::TextureUsages::STORAGE_BINDING } else { wgpu::TextureUsages::empty() };
            let texture = make(&t.name, t.size, format, usage);
            if !t.storage {
                let data: Vec<u8> = match cfg.data.iter().find(|(n, _)| *n == t.name) {
                    Some((_, d)) => bytemuck::cast_slice(d).to_vec(),
                    None => (0..t.hex.len() / 2).map(|i| u8::from_str_radix(&t.hex[2 * i..2 * i + 2], 16).unwrap_or(0)).collect(),
                };
                let bpp = format.block_copy_size(None).unwrap_or(16) as usize;
                let need = t.size.0 as usize * t.size.1 as usize * bpp;
                if data.len() < need { return Err(format!("{}: {} bytes of data for {need}", t.name, data.len())); }
                queue.write_texture(wgpu::TexelCopyTextureInfo { texture: &texture, mip_level: 0, origin: wgpu::Origin3d::ZERO, aspect: wgpu::TextureAspect::All },
                    &data[..need], wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(t.size.0 * bpp as u32), rows_per_image: Some(t.size.1) },
                    wgpu::Extent3d { width: t.size.0, height: t.size.1, depth_or_array_layers: 1 });
            }
            let filterable = format != wgpu::TextureFormat::Rgba32Float && format != wgpu::TextureFormat::R32Float && format != wgpu::TextureFormat::Rg32Float || float32_filterable;
            let view = texture.create_view(&Default::default());
            textures.insert(t.name.clone(), Tex { _texture: texture, view, size: t.size, format, glsl, storage: t.storage, filterable, linear: t.linear });
        }
        // The stages' textures: written by the frame's conversion, read by the passes
        for name in ["LUMA", "MAIN"] {
            let format = wgpu::TextureFormat::Rgba16Float;
            let texture = make(name, (w, h), format, wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::STORAGE_BINDING);
            let view = texture.create_view(&Default::default());
            textures.insert(format!("afp_{name}"), Tex { _texture: texture, view, size: (w, h), format, glsl: "rgba16f", storage: false, filterable: true, linear: true });
        }
        let mut buffers = HashMap::new();
        for b in &parsed.buffers {
            let size = std430_size(&b.vars)?;
            let buffer = device.create_buffer(&wgpu::BufferDescriptor { label: Some(&b.name), size, usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false });
            buffers.insert(b.name.clone(), buffer);
        }
        let uniforms = device.create_buffer(&wgpu::BufferDescriptor { label: None, size: (params.len().max(1) * 16) as u64, usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false });
        let sampler = |f: wgpu::FilterMode| device.create_sampler(&wgpu::SamplerDescriptor {
            address_mode_u: wgpu::AddressMode::ClampToEdge, address_mode_v: wgpu::AddressMode::ClampToEdge,
            mag_filter: f, min_filter: f, ..Default::default()
        });
        let (linear, nearest) = (sampler(wgpu::FilterMode::Linear), sampler(wgpu::FilterMode::Nearest));

        // The frame's conversions
        let frame_bytes = (w as u64) * (h as u64) * 3;
        let buffer = |size: u64, usage: wgpu::BufferUsages| device.create_buffer(&wgpu::BufferDescriptor { label: None, size, usage, mapped_at_creation: false });
        let frame_in = buffer(frame_bytes, wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST);
        let samples = buffer(frame_bytes * 2, wgpu::BufferUsages::STORAGE);
        let frame_out = buffer(frame_bytes, wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC);
        let staging = buffer(frame_bytes, wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST);
        let convert = buffer(std::mem::size_of::<ConvertParams>() as u64, wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST);
        queue.write_buffer(&convert, 0, bytemuck::bytes_of(&ConvertParams { w, h, full: cfg.full_range as u32, words: (frame_bytes / 4) as u32,
                                                                              kr: cfg.matrix.0, kb: cfg.matrix.1, _a: 0.0, _b: 0.0 }));
        let compute = |src: &str| -> Result<wgpu::ComputePipeline, String> {
            let module = scoped(&device, "conversion shader", || device.create_shader_module(wgpu::ShaderModuleDescriptor { label: None, source: wgpu::ShaderSource::Wgsl(format!("{CONVERT}{src}").into()) }))?;
            scoped(&device, "conversion pipeline", || device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                label: None, layout: None, module: &module, entry_point: Some("main"), compilation_options: Default::default(), cache: None }))
        };
        let (unpack, to_yuv, pack) = (compute(UNPACK)?, compute(TO_YUV)?, compute(PACK)?);
        let group = |p: &wgpu::ComputePipeline, entries: &[wgpu::BindingResource]| device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None, layout: &p.get_bind_group_layout(0),
            entries: &entries.iter().enumerate().map(|(i, r)| wgpu::BindGroupEntry { binding: i as u32, resource: r.clone() }).collect::<Vec<_>>(),
        });
        let unpack_group = group(&unpack, &[convert.as_entire_binding(), frame_in.as_entire_binding(),
            wgpu::BindingResource::TextureView(&textures["afp_LUMA"].view), wgpu::BindingResource::TextureView(&textures["afp_MAIN"].view)]);
        let pack_group = group(&pack, &[convert.as_entire_binding(), samples.as_entire_binding(), frame_out.as_entire_binding()]);

        let mut s = Shaders {
            device, queue, parsed, prelude: cfg.prelude, size: (w, h), params, uniforms, textures, buffers, linear, nearest,
            plan: None, compiled: HashMap::new(), convert, frame_in, samples, frame_out, staging,
            unpack: (unpack, unpack_group), to_yuv, pack: (pack, pack_group), frame_bytes, millis: 0.0,
        };
        s.replan()?;
        Ok(s)
    }

    /// A parameter's value (the shaders' DYNAMIC ones change from the next frame on); false if there's none of that name
    pub fn set(&mut self, name: &str, value: f64) -> bool {
        match self.params.iter_mut().find(|p| p.name == name) {
            Some(p) => { p.value = value; true }
            None => false,
        }
    }

    fn param(&self, name: &str) -> Option<f64> { self.params.iter().find(|p| p.name == name).map(|p| p.value) }

    /// Which passes run (their WHEN) with the parameters as they are
    fn conditions(&self) -> Result<Vec<bool>, String> {
        let none = |_: &str| None;
        self.parsed.passes.iter().map(|p| match &p.when {
            None => Ok(true),
            Some(e) => rpn(e, &none, &|n| self.param(n)).map(|v| v != 0.0),
        }).collect()
    }

    /// The steps for the passes that run, their textures and bindings (pipelines compiled once)
    fn replan(&mut self) -> Result<(), String> {
        let on = self.conditions()?;
        let (w, h) = self.size;
        let mut steps = Vec::new();
        // Where each stage's texture is now, and the size of every texture by name
        let mut stage: HashMap<&str, String> = HashMap::from([("LUMA", "afp_LUMA".to_string()), ("MAIN", "afp_MAIN".to_string())]);
        let mut saved: HashMap<String, String> = HashMap::new();
        let mut order: Vec<usize> = Vec::new();
        for st in ["LUMA", "MAIN"] {
            for (i, p) in self.parsed.passes.iter().enumerate() {
                if on[i] && p.hooks.iter().any(|h| h == st) { order.push(i); }
            }
        }
        for i in order {
            let p = self.parsed.passes[i].clone();
            let st = if p.hooks.iter().any(|h| h == "LUMA") { "LUMA" } else { "MAIN" };
            if !p.hooks.iter().all(|h| h == "LUMA" || h == "MAIN") { return Err(format!("{}: hooks {:?} (LUMA and MAIN only)", p.desc, p.hooks)); }
            let hooked = stage[st].clone();
            let resolve = |n: &str| -> Option<String> {
                match n {
                    "HOOKED" => Some(hooked.clone()),
                    "MAIN" | "LUMA" => Some(stage[n].clone()),
                    _ => saved.get(n).cloned().or_else(|| self.textures.contains_key(n).then(|| n.to_string())),
                }
            };
            let size_of = |n: &str| -> Option<(u32, u32)> { if n == "OUTPUT" { return Some((w, h)); } resolve(n).and_then(|t| self.textures.get(&t)).map(|t| t.size) };
            let hsize = self.textures[&hooked].size;
            let ow = match &p.width { Some(e) => rpn(e, &size_of, &|n| self.param(n))?, None => hsize.0 as f64 };
            let oh = match &p.height { Some(e) => rpn(e, &size_of, &|n| self.param(n))?, None => hsize.1 as f64 };
            let out = ((ow.round() as u32).max(1), (oh.round() as u32).max(1));

            // What it binds, in order: (name in the shader, texture or buffer)
            let mut binds: Vec<(String, Bound)> = Vec::new();
            for b in &p.binds {
                if binds.iter().any(|(n, _)| n == b) { continue; }
                if let Some(buf) = self.buffers.get(b) { let _ = buf; binds.push((b.clone(), Bound::Buffer)); continue; }
                let t = resolve(b).ok_or_else(|| format!("{}: nothing to bind as {b}", p.desc))?;
                let tex = &self.textures[&t];
                binds.push((b.clone(), if tex.storage { Bound::Image(t) } else { Bound::Sampled(t) }));
            }
            if !self.compiled.contains_key(&i) {
                let pipeline = self.compile(&p, &binds, out)?;
                self.compiled.insert(i, pipeline);
            }
            // Its target: a texture of its own (saved, or the stage's from now on)
            let target = format!("afp_pass{i}");
            if !self.textures.contains_key(&target) {
                let usage = wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::RENDER_ATTACHMENT;
                let size = if p.compute.is_some() { (1, 1) } else { out };
                let format = wgpu::TextureFormat::Rgba16Float;
                let texture = self.device.create_texture(&wgpu::TextureDescriptor {
                    label: Some(&target), size: wgpu::Extent3d { width: size.0, height: size.1, depth_or_array_layers: 1 }, mip_level_count: 1,
                    sample_count: 1, dimension: wgpu::TextureDimension::D2, format, usage, view_formats: &[],
                });
                let view = texture.create_view(&Default::default());
                self.textures.insert(target.clone(), Tex { _texture: texture, view, size, format, glsl: "rgba16f", storage: false, filterable: true, linear: true });
            }
            let pipeline = match &self.compiled[&i] {
                Pipeline::Render(r) => Pipeline::Render(r.clone()),
                Pipeline::Compute(c, _) => {
                    let [bw, bh, _, _] = p.compute.unwrap();
                    Pipeline::Compute(c.clone(), (out.0.div_ceil(bw), out.1.div_ceil(bh)))
                }
            };
            let layout = match &pipeline { Pipeline::Render(r) => r.get_bind_group_layout(0), Pipeline::Compute(c, _) => c.get_bind_group_layout(0) };
            let mut entries = vec![wgpu::BindGroupEntry { binding: 0, resource: self.uniforms.as_entire_binding() }];
            let mut b = 1;
            for (name, bound) in &binds {
                match bound {
                    Bound::Buffer => { entries.push(wgpu::BindGroupEntry { binding: b, resource: self.buffers[name].as_entire_binding() }); b += 1; }
                    Bound::Image(t) => { entries.push(wgpu::BindGroupEntry { binding: b, resource: wgpu::BindingResource::TextureView(&self.textures[t].view) }); b += 1; }
                    Bound::Sampled(t) => {
                        let tex = &self.textures[t];
                        let sampler = if tex.linear && tex.filterable { &self.linear } else { &self.nearest };
                        entries.push(wgpu::BindGroupEntry { binding: b, resource: wgpu::BindingResource::TextureView(&tex.view) });
                        entries.push(wgpu::BindGroupEntry { binding: b + 1, resource: wgpu::BindingResource::Sampler(sampler) });
                        b += 2;
                    }
                }
            }
            let group = scoped(&self.device, &p.desc, || self.device.create_bind_group(&wgpu::BindGroupDescriptor { label: None, layout: &layout, entries: &entries }))?;
            let is_fragment = p.compute.is_none();
            steps.push(Step { pipeline, group, target: is_fragment.then(|| target.clone()) });
            match &p.save {
                Some(n) if n != st => { saved.insert(n.clone(), target); }
                _ => { if is_fragment { stage.insert(st, target); } }
            }
        }
        let main = stage["MAIN"].clone();
        self.plan = Some((on, steps, main));
        Ok(())
    }

    /// A pass as GLSL for naga, with what libplacebo declares around it, compiled
    fn compile(&self, p: &PassDecl, binds: &[(String, Bound)], out: (u32, u32)) -> Result<Pipeline, String> {
        let (w, h) = self.size;
        let compute = p.compute;
        let mut g = String::from("#version 450\n");
        if let Some([_, _, tw, th]) = compute { g += &format!("layout(local_size_x = {tw}, local_size_y = {th}) in;\n"); }
        g += &format!("layout(set = 0, binding = 0, std140) uniform AfpParams {{ vec4 afp_p[{}]; }};\n", self.params.len().max(1));
        for (i, par) in self.params.iter().enumerate() {
            g += &match par.kind { Kind::Float => format!("#define {} (afp_p[{i}].x)\n", par.name), Kind::Int => format!("#define {} int(afp_p[{i}].x)\n", par.name) };
        }
        g += &format!("#define target_size vec2({w}.0, {h}.0)\n#define input_size vec2({w}.0, {h}.0)\n#define tex_offset vec2(0.0)\n");
        let pos = if compute.is_some() { format!("((vec2(gl_GlobalInvocationID.xy) + 0.5) / vec2({}.0, {}.0))", out.0, out.1) }
                  else { format!("(gl_FragCoord.xy / vec2({}.0, {}.0))", out.0, out.1) };
        let mut b = 1;
        let mut atomic_arrays = Vec::new();
        for (name, bound) in binds {
            match bound {
                Bound::Buffer => {
                    let decl = self.parsed.buffers.iter().find(|x| &x.name == name).unwrap();
                    g += &format!("layout(set = 0, binding = {b}, std430) buffer AfpBuffer{b} {{\n");
                    for v in &decl.vars {
                        g += &format!("    {};\n", v.trim_end_matches(';'));
                        let (n, uint_array) = var_name(v);
                        if uint_array { atomic_arrays.push(n); }
                    }
                    g += "};\n";
                    b += 1;
                }
                Bound::Image(t) => {
                    g += &format!("layout(set = 0, binding = {b}, {}) uniform image2D {name};\n", self.textures[t].glsl);
                    b += 1;
                }
                Bound::Sampled(t) => {
                    let tex = &self.textures[t];
                    let (tw, th) = tex.size;
                    g += &format!("layout(set = 0, binding = {b}) uniform texture2D afp_t_{name};\nlayout(set = 0, binding = {}) uniform sampler afp_s_{name};\n", b + 1);
                    g += &format!("#define {name}_raw sampler2D(afp_t_{name}, afp_s_{name})\n#define {name}_size vec2({tw}.0, {th}.0)\n#define {name}_pt vec2({:.12}, {:.12})\n\
                                   #define {name}_mul 1.0\n#define {name}_rot mat2(1.0, 0.0, 0.0, 1.0)\n#define {name}_off vec2(0.0)\n#define {name}_pos {pos}\n\
                                   #define {name}_tex(pos) ({name}_mul * texture({name}_raw, pos))\n#define {name}_texOff(off) {name}_tex({name}_pos + {name}_pt * vec2(off))\n",
                                  1.0 / tw as f64, 1.0 / th as f64);
                    // A //!TEXTURE of the shader's own is its sampler by its name, as libplacebo binds it
                    if self.parsed.textures.iter().any(|x| &x.name == t) { g += &format!("#define {name} {name}_raw\n"); }
                    b += 2;
                }
            }
        }
        let body = p.body.replace("memoryBarrierBuffer();", "").replace("memoryBarrierImage();", "").replace("memoryBarrierShared();", "").replace("groupMemoryBarrier();", "");
        let (body, used) = rewrite_atomics(&body, &atomic_arrays)?;
        for n in &used { g += &format!("void afp_atomicAdd_{n}(uint i, uint v) {{ }}\n"); }
        if compute.is_none() { g += "layout(location = 0) out vec4 afp_out;\n"; }
        g += &self.prelude;
        g += "\n";
        g += &body;
        g += if compute.is_some() { "\nvoid main() { hook(); }\n" } else { "\nvoid main() { afp_out = hook(); }\n" };

        let stage = if compute.is_some() { naga::ShaderStage::Compute } else { naga::ShaderStage::Fragment };
        let mut module = naga::front::glsl::Frontend::default().parse(&naga::front::glsl::Options::from(stage), &g)
            .map_err(|e| format!("{}: {}", p.desc, e.emit_to_string(&g)))?;
        patch_atomics(&mut module).map_err(|e| format!("{}: {e}", p.desc))?;
        patch_barriers(&mut module);

        // The layout: the parameters, then what it binds
        let vis = if compute.is_some() { wgpu::ShaderStages::COMPUTE } else { wgpu::ShaderStages::FRAGMENT };
        let mut entries = vec![wgpu::BindGroupLayoutEntry { binding: 0, visibility: vis, ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Uniform, has_dynamic_offset: false, min_binding_size: None }, count: None }];
        let mut b = 1;
        for (_, bound) in binds {
            match bound {
                Bound::Buffer => {
                    entries.push(wgpu::BindGroupLayoutEntry { binding: b, visibility: vis, ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Storage { read_only: false }, has_dynamic_offset: false, min_binding_size: None }, count: None });
                    b += 1;
                }
                Bound::Image(t) => {
                    entries.push(wgpu::BindGroupLayoutEntry { binding: b, visibility: vis, ty: wgpu::BindingType::StorageTexture { access: wgpu::StorageTextureAccess::ReadWrite, format: self.textures[t].format, view_dimension: wgpu::TextureViewDimension::D2 }, count: None });
                    b += 1;
                }
                Bound::Sampled(t) => {
                    let tex = &self.textures[t];
                    let filtering = tex.linear && tex.filterable;
                    entries.push(wgpu::BindGroupLayoutEntry { binding: b, visibility: vis, ty: wgpu::BindingType::Texture { sample_type: wgpu::TextureSampleType::Float { filterable: tex.filterable }, view_dimension: wgpu::TextureViewDimension::D2, multisampled: false }, count: None });
                    entries.push(wgpu::BindGroupLayoutEntry { binding: b + 1, visibility: vis, ty: wgpu::BindingType::Sampler(if filtering { wgpu::SamplerBindingType::Filtering } else { wgpu::SamplerBindingType::NonFiltering }), count: None });
                    b += 2;
                }
            }
        }
        let d = &self.device;
        let layout = d.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor { label: None, entries: &entries });
        let pl = d.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor { label: None, bind_group_layouts: &[Some(&layout)], immediate_size: 0 });
        let shader = scoped(d, &p.desc, || d.create_shader_module(wgpu::ShaderModuleDescriptor { label: Some(&p.desc), source: wgpu::ShaderSource::Naga(Cow::Owned(module)) }))?;
        if compute.is_some() {
            let c = scoped(d, &p.desc, || d.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                label: Some(&p.desc), layout: Some(&pl), module: &shader, entry_point: Some("main"), compilation_options: Default::default(), cache: None }))?;
            Ok(Pipeline::Compute(c, (0, 0)))
        } else {
            let vs = d.create_shader_module(wgpu::ShaderModuleDescriptor { label: Some("afp fullscreen"), source: wgpu::ShaderSource::Wgsl(FULLSCREEN.into()) });
            let r = scoped(d, &p.desc, || d.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(&p.desc), layout: Some(&pl),
                vertex: wgpu::VertexState { module: &vs, entry_point: Some("vs"), buffers: &[], compilation_options: Default::default() },
                fragment: Some(wgpu::FragmentState { module: &shader, entry_point: Some("main"), targets: &[Some(wgpu::ColorTargetState { format: wgpu::TextureFormat::Rgba16Float, blend: None, write_mask: wgpu::ColorWrites::ALL })], compilation_options: Default::default() }),
                primitive: Default::default(), depth_stencil: None, multisample: Default::default(), multiview_mask: None, cache: None,
            }))?;
            Ok(Pipeline::Render(r))
        }
    }

    /// One frame (yuv420p10le, the size set up) through the passes; the result in the same format. The shaders' state
    /// (the denoise's ring) carries over from frame to frame
    pub fn render(&mut self, frame: &[u8]) -> Result<Vec<u8>, String> {
        let t = std::time::Instant::now();
        if frame.len() as u64 != self.frame_bytes { return Err(format!("frame is {} bytes, set up for {}", frame.len(), self.frame_bytes)); }
        if self.plan.as_ref().map(|p| &p.0) != Some(&self.conditions()?) { self.replan()?; }
        let values: Vec<[f32; 4]> = if self.params.is_empty() { vec![[0.0; 4]] } else { self.params.iter().map(|p| [p.value as f32, 0.0, 0.0, 0.0]).collect() };
        self.queue.write_buffer(&self.uniforms, 0, bytemuck::cast_slice(&values));
        self.queue.write_buffer(&self.frame_in, 0, frame);
        let (w, h) = self.size;
        let d = &self.device;
        let (_, steps, main) = self.plan.as_ref().unwrap();
        let to_yuv_group = d.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None, layout: &self.to_yuv.get_bind_group_layout(0),
            entries: &[wgpu::BindGroupEntry { binding: 0, resource: self.convert.as_entire_binding() },
                       wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::TextureView(&self.textures[main].view) },
                       wgpu::BindGroupEntry { binding: 2, resource: self.samples.as_entire_binding() }],
        });
        let mut enc = d.create_command_encoder(&Default::default());
        {
            let mut pass = enc.begin_compute_pass(&Default::default());
            pass.set_pipeline(&self.unpack.0);
            pass.set_bind_group(0, &self.unpack.1, &[]);
            pass.dispatch_workgroups(w.div_ceil(8), h.div_ceil(8), 1);
        }
        for s in steps {
            match &s.pipeline {
                Pipeline::Compute(c, groups) => {
                    let mut pass = enc.begin_compute_pass(&Default::default());
                    pass.set_pipeline(c);
                    pass.set_bind_group(0, &s.group, &[]);
                    pass.dispatch_workgroups(groups.0, groups.1, 1);
                }
                Pipeline::Render(r) => {
                    let view = &self.textures[s.target.as_ref().unwrap()].view;
                    let mut pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                        label: None,
                        color_attachments: &[Some(wgpu::RenderPassColorAttachment { view, depth_slice: None, resolve_target: None,
                            ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color::BLACK), store: wgpu::StoreOp::Store } })],
                        ..Default::default()
                    });
                    pass.set_pipeline(r);
                    pass.set_bind_group(0, &s.group, &[]);
                    pass.draw(0..3, 0..1);
                }
            }
        }
        {
            let mut pass = enc.begin_compute_pass(&Default::default());
            pass.set_pipeline(&self.to_yuv);
            pass.set_bind_group(0, &to_yuv_group, &[]);
            pass.dispatch_workgroups((w / 2).div_ceil(8), (h / 2).div_ceil(8), 1);
        }
        {
            let words = (self.frame_bytes / 4) as u32;
            let groups = words.div_ceil(256);
            let mut pass = enc.begin_compute_pass(&Default::default());
            pass.set_pipeline(&self.pack.0);
            pass.set_bind_group(0, &self.pack.1, &[]);
            pass.dispatch_workgroups(groups.min(65535), groups.div_ceil(65535), 1);
        }
        enc.copy_buffer_to_buffer(&self.frame_out, 0, &self.staging, 0, self.frame_bytes);
        self.queue.submit([enc.finish()]);
        let slice = self.staging.slice(..);
        let (tx, rx) = mpsc::channel();
        slice.map_async(wgpu::MapMode::Read, move |r| { let _ = tx.send(r); });
        let _ = d.poll(wgpu::PollType::Wait { submission_index: None, timeout: None });
        rx.recv().map_err(|_| "GPU readback lost".to_string())?.map_err(|e| format!("GPU readback: {e}"))?;
        let out = slice.get_mapped_range().map_err(|e| format!("GPU readback: {e:?}"))?.to_vec();
        self.staging.unmap();
        self.millis = t.elapsed().as_secs_f64() * 1000.0;
        Ok(out)
    }
}

#[derive(Clone, Debug)]
enum Bound { Buffer, Image(String), Sampled(String) }

const FULLSCREEN: &str = "@vertex fn vs(@builtin(vertex_index) i: u32) -> @builtin(position) vec4<f32> {
    let p = vec2<f32>(f32((i << 1u) & 2u), f32(i & 2u));
    return vec4<f32>(p * 2.0 - 1.0, 0.0, 1.0);
}";

/// A buffer's size by std430's rules, for its VARs (scalars, vec2, vec4 and their arrays), with room
fn std430_size(vars: &[String]) -> Result<u64, String> {
    let mut at = 0u64;
    for v in vars {
        let mut words = v.split_whitespace();
        let ty = words.next().unwrap_or("");
        let rest: String = words.collect::<Vec<_>>().join("");
        let (size, align): (u64, u64) = match ty {
            "float" | "int" | "uint" | "bool" => (4, 4),
            "vec2" | "ivec2" | "uvec2" => (8, 8),
            "vec3" | "ivec3" | "uvec3" => (12, 16),
            "vec4" | "ivec4" | "uvec4" => (16, 16),
            "mat4" => (64, 16),
            _ => return Err(format!("buffer variable {v}: type {ty}")),
        };
        let count = match rest.split_once('[') {
            Some((_, n)) => n.trim_end_matches(';').trim_end_matches(']').parse::<u64>().map_err(|_| format!("buffer variable {v}: its length"))?,
            None => 1,
        };
        let stride = if count > 1 || rest.contains('[') { size.max(align).div_ceil(align) * align } else { size };
        at = at.div_ceil(align) * align + stride * count;
    }
    Ok(at.div_ceil(16) * 16 + 256)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame(w: usize, h: usize, f: impl Fn(usize, usize, usize) -> u16) -> Vec<u8> {
        // Y, then U, then V: f(plane, x, y)
        let mut v = Vec::with_capacity(w * h * 3);
        for (p, (pw, ph)) in [(w, h), (w / 2, h / 2), (w / 2, h / 2)].into_iter().enumerate() {
            for y in 0..ph { for x in 0..pw { v.extend_from_slice(&f(p, x, y).to_le_bytes()); } }
        }
        v
    }
    fn samples(b: &[u8]) -> Vec<u16> { b.chunks_exact(2).map(|c| u16::from_le_bytes([c[0], c[1]])).collect() }

    #[test]
    fn rpn_sizes() {
        let size = |n: &str| (n == "HOOKED").then_some((3840, 2160));
        let param = |n: &str| (n == "scale").then_some(0.5);
        assert_eq!(rpn("HOOKED.w 4 /", &size, &param).unwrap(), 960.0);
        assert_eq!(rpn("HOOKED.h 32 / 2 + 10 *", &size, &param).unwrap(), (2160.0 / 32.0 + 2.0) * 10.0);
        assert_eq!(rpn("HOOKED.width scale *", &size, &param).unwrap(), 1920.0);
        assert_eq!(rpn("scale 0 > !", &size, &param).unwrap(), 0.0);
        assert!(rpn("NOPE.w", &size, &param).is_err());
    }

    #[test]
    fn atomics_rewritten() {
        let (b, used) = rewrite_atomics("if (x) { atomicAdd(gdn_hist[bin * 256u + f(a[1], 2)], 1u); }", &["gdn_hist".into()]).unwrap();
        assert_eq!(b, "if (x) { afp_atomicAdd_gdn_hist(bin * 256u + f(a[1], 2), 1u); }");
        assert_eq!(used, vec!["gdn_hist".to_string()]);
        assert!(rewrite_atomics("atomicAdd(other[0], 1u);", &["gdn_hist".into()]).is_err());
        assert_eq!(std430_size(&["uint h[8192]".into(), "float s[32]".into(), "vec4 q[11]".into(), "vec2 size".into(), "int head".into()]).unwrap(),
                   ((32768 + 128 + 176 + 8 + 4) as u64).div_ceil(16) * 16 + 256);
    }

    /// A frame through no passes comes back as it went in, within a code (the round trip through RGB): luma exactly
    /// where the chroma is flat
    #[test]
    fn round_trip() {
        let (w, h) = (64, 32);
        let input = frame(w, h, |p, x, y| if p == 0 { 64 + ((x * 13 + y * 7) % 876) as u16 } else { 512 + if p == 1 { 40 } else { -30i32 as u16 } });
        let mut s = Shaders::new(Config { sources: vec![], size: (w, h), prelude: String::new(), data: vec![], matrix: (0.2126, 0.0722), full_range: false, params: vec![] }).unwrap();
        let out = s.render(&input).unwrap();
        let (a, b) = (samples(&input), samples(&out));
        let worst = a.iter().zip(&b).map(|(x, y)| (*x as i32 - *y as i32).abs()).max().unwrap();
        assert!(worst <= 1, "{worst}");
    }

    /// The hook format's parts: a parameter, a LUMA pass saved for a MAIN pass, a quarter-size pass, a compute pass
    /// with a storage texture and a buffer's atomic counter, read back by the last pass
    #[test]
    fn passes_run_as_libplacebo_runs_them() {
        let src = "
//!PARAM gain
//!TYPE DYNAMIC float
0.5

//!BUFFER COUNT
//!VAR uint count[4]
//!VAR float last
//!STORAGE

//!TEXTURE SCRATCH
//!SIZE 8 8
//!FORMAT rgba16f
//!STORAGE

//!HOOK LUMA
//!BIND HOOKED
//!SAVE LEVEL
//!WIDTH 1
//!HEIGHT 1
vec4 hook() { return vec4(texelFetch(HOOKED_raw, ivec2(0, 0), 0).x * HOOKED_mul); }

//!HOOK MAIN
//!BIND HOOKED
//!BIND COUNT
//!BIND SCRATCH
//!SAVE COUNTED
//!WIDTH HOOKED.w
//!HEIGHT HOOKED.h
//!COMPUTE 8 8
void hook() {
    ivec2 j = ivec2(gl_GlobalInvocationID.xy);
    if (j.x < int(HOOKED_size.x) && j.y < int(HOOKED_size.y)) atomicAdd(count[1], 1u);
    if (j == ivec2(3, 2)) imageStore(SCRATCH, ivec2(1, 1), vec4(0.25));
}

//!HOOK MAIN
//!BIND HOOKED
//!BIND LEVEL
//!BIND COUNT
//!BIND SCRATCH
//!WHEN gain 0 >
vec4 hook() {
    float n = float(count[1]) / (HOOKED_size.x * HOOKED_size.y);
    vec3 c = HOOKED_tex(HOOKED_pos).rgb;
    return vec4(c * gain + vec3(LEVEL_tex(vec2(0.5)).x * 0.0 + imageLoad(SCRATCH, ivec2(1, 1)).x * n), 1.0);
}
";
        let (w, h) = (32, 16);
        // Grey: Y 502 (mid, limited range), no chroma
        let input = frame(w, h, |p, _, _| if p == 0 { 502 } else { 512 });
        let mut s = Shaders::new(Config { sources: vec![src.into()], size: (w, h), prelude: String::new(), data: vec![], matrix: (0.2126, 0.0722), full_range: false, params: vec![] }).unwrap();
        let out = samples(&s.render(&input).unwrap());
        // 0.5 * 0.5 + 0.25 * (count / pixels: 1 on the first frame)
        let want = 64.0 + 876.0 * (0.5 * 0.5 + 0.25);
        assert!((out[0] as f64 - want).abs() <= 1.0, "{} vs {want}", out[0]);
        // The counter carries over: 2 on the second frame
        let out = samples(&s.render(&input).unwrap());
        let want = 64.0 + 876.0 * (0.25 + 0.25 * 2.0);
        assert!((out[0] as f64 - want).abs() <= 1.0, "{} vs {want}", out[0]);
        // WHEN off: the frame as it is
        assert!(s.set("gain", 0.0));
        let out = samples(&s.render(&input).unwrap());
        assert!((out[0] as i32 - 502).abs() <= 1, "{}", out[0]);
    }
}
