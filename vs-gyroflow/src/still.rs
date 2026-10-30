// SPDX-License-Identifier: GPL-3.0-or-later
//
// The paused frame without its noise, from the frames on both sides of it: `core.gyroflow.Still(clip, time=...)`.
//
// At 60 fps the eye averages the noise over time by itself; a still frame keeps all of it, and a pause has time to
// spare. So the frame the video is paused on (`time`, mpv's pts) is rebuilt from it and its `radius` neighbours
// before and after, each moved onto it by the motion found between them, on the GPU (wgpu):
//
//   1. motion: per block of the frame (16x16 at full size), where it is in each neighbour; coarse to fine over a
//      pyramid of the luma (each level half the size: the coarser level's vectors of the block and of the blocks
//      around it as the candidates, then a search around the best), then to a quarter pixel at full size. A
//      candidate has to beat the prediction by a share of its cost, or flat noisy ground would follow the noise
//   2. the noise: how much a pixel differs from where it is in the frames just before and after, by brightness
//      (the noise of log footage depends on it), for luma and each color
//   3. each pixel: the average of it and its neighbours' pixels where they are, each weighted by how much the patch
//      around it (9x9, every other pixel) differs there beyond the noise. Where it moved in a way the vectors don't
//      follow (or appeared), the patches differ and the frame keeps its own pixel, smoothed among alike ones around
//      it: no ghosts, no trails. Its vector is the blend of the surrounding blocks', or one of theirs where that fits
//      clearly better, so the edges of moving things keep theirs
//   4. specks: a small spot far from what all the neighbours agree on and from the pixels around it (a dark or
//      bright speck, a particle in the water) takes their value, with its fringe; the average would keep it, as it
//      matches none of them
//   5. some of the luma's grain kept (`luma_keep`), so it doesn't look smeared; the color's noise goes
//
// The color uses the luma's vectors and weights, and its own patches. The stabilized frame's marker row
// (dispersion.rs) is left as it is. Every other frame passes through untouched: in mpv the filter sits after
// Stabilize only while the gyroflow plugin's still.lua seeks to the paused frame again, and comes out on the next
// seek.

use std::ffi::{ c_int, c_void, CString };
use std::ptr::null;
use std::sync::{ Arc, mpsc };
use std::time::Instant;

use parking_lot::Mutex;
use vapoursynth4_sys::*;

/// The neighbours on each side at most (the shaders' MAXN is twice this)
pub const MAX_RADIUS: usize = 7;
/// Brightness bins of the noise measurement, over the 10-bit range
const BINS: usize = 16;
/// Steps of the measured differences, half a 10-bit code each
const STEPS: usize = 256;

#[derive(Clone, Copy, Debug)]
pub struct Settings {
    /// How hard the weights are: 1 as tuned, more averages more of what differs
    pub luma_strength: f32,
    pub chroma_strength: f32,
    /// How much of the luma's own noise stays (0..1)
    pub luma_keep: f32,
    pub chroma_keep: f32,
    /// Specks replaced (0..1)
    pub specks: f32,
}

impl Default for Settings {
    fn default() -> Self {
        Settings { luma_strength: 1.5, chroma_strength: 1.0, luma_keep: 0.25, chroma_keep: 0.0, specks: 1.0 }
    }
}

/// One plane of a frame as it is in memory
#[derive(Clone, Copy)]
pub struct Plane<'a> {
    pub data: &'a [u8],
    pub stride: usize,
    pub width: usize,
    pub height: usize,
}

/// A frame: 1 (gray) or 3 planes, chroma subsampled by `sub` (shifts), `bytes` per sample (1 or 2), values up to
/// `max_value`
pub struct Frame<'a> {
    pub planes: Vec<Plane<'a>>,
}

pub struct Output {
    /// Each plane, row by row without padding
    pub planes: Vec<Vec<u16>>,
    /// The noise measured (10-bit codes) at mid brightness: luma, cb, cr
    pub sigma: [f32; 3],
    pub millis: f64,
}

// ---------------------------------------------------------------------------------------------------
// Shaders

const COMMON: &str = r#"
struct P {
    w: u32, h: u32, cw: u32, ch: u32,
    frames: u32, sub_x: u32, sub_y: u32, max_value: u32,
    sw: u32, sh: u32, lw: u32, lh: u32,
    bw: u32, bh: u32, cbw: u32, cbh: u32,
    search: i32, step: u32, bs: u32, win: u32,
    fit: u32, _c: u32, _d: u32, _e: u32,
    to_code: f32, from_code: f32, luma_keep: f32, chroma_keep: f32,
    luma_strength: f32, chroma_strength: f32, specks: f32, lambda: f32,
    sigma: array<vec4<f32>, 16>,
}
@group(0) @binding(0) var<uniform> p: P;

const MAXN: u32 = 14u;

// The noise (10-bit codes) at a brightness, for luma (0), cb (1), cr (2)
fn sigma_at(level: f32, c: u32) -> f32 {
    let f = clamp(level / 64.0 - 0.5, 0.0, 15.0);
    let i = u32(floor(f));
    let t = f - floor(f);
    return mix(p.sigma[i][c], p.sigma[min(i + 1u, 15u)][c], t);
}
"#;

// Level-0 luma (the frames' own texture) or a level of the pyramid (a buffer), for the passes that only load pixels
const FETCH_TEXTURE: &str = r#"
@group(0) @binding(1) var src: texture_2d_array<f32>;
fn fetch(l: u32, x: i32, y: i32) -> f32 {
    let c = vec2<i32>(clamp(x, 0, i32(p.sw) - 1), clamp(y, 0, i32(p.sh) - 1));
    return textureLoad(src, c, i32(l), 0).r * p.to_code;
}
"#;
const FETCH_BUFFER: &str = r#"
@group(0) @binding(1) var<storage, read> src: array<f32>;
fn fetch(l: u32, x: i32, y: i32) -> f32 {
    let cx = u32(clamp(x, 0, i32(p.sw) - 1));
    let cy = u32(clamp(y, 0, i32(p.sh) - 1));
    return src[(l * p.sh + cy) * p.sw + cx];
}
"#;

// The next level: each pixel the mean of 2x2
const DOWN: &str = r#"
@group(0) @binding(2) var<storage, read_write> dst: array<f32>;
@compute @workgroup_size(8, 8, 1)
fn main(@builtin(global_invocation_id) id: vec3<u32>) {
    if (id.x >= p.lw || id.y >= p.lh) { return; }
    let x = i32(id.x) * 2;
    let y = i32(id.y) * 2;
    let v = fetch(id.z, x, y) + fetch(id.z, x + 1, y) + fetch(id.z, x, y + 1) + fetch(id.z, x + 1, y + 1);
    dst[(id.z * p.lh + id.y) * p.lw + id.x] = v * 0.25;
}
"#;

// Block matching at one level: for each block of the frame (layer 0; bs pixels, 8 on the coarser levels and 16 at
// full size) and each neighbour (layer id.z + 1), the offset (whole pixels of this level) where its win x win window
// around the block matches best. At full size (fit), then between pixels where the costs either side say it is
const MOTION: &str = r#"
@group(0) @binding(2) var<storage, read> coarse: array<vec2<f32>>;
@group(0) @binding(3) var<storage, read_write> vecs: array<vec2<f32>>;

fn cost(j: u32, bx: i32, by: i32, v: vec2<i32>) -> f32 {
    var s = 0.0;
    var n = 0.0;
    let x0 = bx * i32(p.bs) + (i32(p.bs) - i32(p.win)) / 2;
    let y0 = by * i32(p.bs) + (i32(p.bs) - i32(p.win)) / 2;
    let st = i32(p.step);
    let win = i32(p.win);
    for (var dy = 0; dy < win; dy += st) {
        for (var dx = 0; dx < win; dx += st) {
            if (p.fit != 0u) {
                s += abs(box(0u, x0 + dx, y0 + dy) - box(j, x0 + dx + v.x, y0 + dy + v.y));
            } else {
                s += abs(fetch(0u, x0 + dx, y0 + dy) - fetch(j, x0 + dx + v.x, y0 + dy + v.y));
            }
            n += 1.0;
        }
    }
    return s / n;
}

// At full size, 2x2 pixels at a time: half the noise to line up by mistake, and the window's every pixel
fn box(l: u32, x: i32, y: i32) -> f32 {
    return 0.25 * (fetch(l, x, y) + fetch(l, x + 1, y) + fetch(l, x, y + 1) + fetch(l, x + 1, y + 1));
}

@compute @workgroup_size(8, 8, 1)
fn main(@builtin(global_invocation_id) id: vec3<u32>) {
    if (id.x >= p.bw || id.y >= p.bh) { return; }
    let j = id.z + 1u;
    let bx = i32(id.x);
    let by = i32(id.y);
    // On flat, noisy ground every offset matches about as well, and the best one is where the noise happens to line
    // up: averaging frames lined up that way keeps the noise. So a candidate has to beat the prediction by a share of
    // its cost (lambda per pixel off it), not just by a hair
    var best = vec2<i32>(0, 0);
    var best_cost = cost(j, bx, by, best);
    if (p.cbw > 0u) {
        // The coarser level's vector of this block and of the four around it, at this level's scale
        var offs = array<vec2<i32>, 5>(vec2<i32>(0, 0), vec2<i32>(-1, 0), vec2<i32>(1, 0), vec2<i32>(0, -1), vec2<i32>(0, 1));
        let cmax = vec2<i32>(i32(p.cbw) - 1, i32(p.cbh) - 1);
        // The coarser level's blocks are 8 of its pixels, 16 of this level's
        let cb = vec2<i32>(bx, by) * i32(p.bs) / 16;
        for (var k = 0; k < 5; k++) {
            let q = clamp(cb + offs[k], vec2<i32>(0, 0), cmax);
            let v = vec2<i32>(round(coarse[(id.z * p.cbh + u32(q.y)) * p.cbw + u32(q.x)] * 2.0));
            let c = cost(j, bx, by, v);
            // The block's own prediction first; the others (and no motion) only when clearly better
            if (k == 0) { best_cost = min(best_cost * (1.0 + p.lambda), c); if (c <= best_cost) { best = v; } }
            else if (c < best_cost * (1.0 - 0.5 * p.lambda)) { best_cost = c; best = v; }
        }
    }
    let center = best;
    let center_cost = best_cost;
    for (var dy = -p.search; dy <= p.search; dy++) {
        for (var dx = -p.search; dx <= p.search; dx++) {
            if (dx == 0 && dy == 0) { continue; }
            let v = center + vec2<i32>(dx, dy);
            let c = cost(j, bx, by, v) + p.lambda * center_cost * f32(abs(dx) + abs(dy));
            if (c < best_cost) { best_cost = c; best = v; }
        }
    }
    var out = vec2<f32>(best);
    if (p.fit != 0u) {
        let c0 = cost(j, bx, by, best);
        out.x += between(cost(j, bx, by, best - vec2<i32>(1, 0)), c0, cost(j, bx, by, best + vec2<i32>(1, 0)));
        out.y += between(cost(j, bx, by, best - vec2<i32>(0, 1)), c0, cost(j, bx, by, best + vec2<i32>(0, 1)));
    }
    vecs[(id.z * p.bh + id.y) * p.bw + id.x] = out;
}

// Where between -1 and +1 the minimum is, from the costs there (a, b) and at 0 (c): absolute differences make a V,
// so the two lines of equal slope through them
fn between(a: f32, c: f32, b: f32) -> f32 {
    if (a < c || b < c) { return 0.0; }
    if (a >= b) { return clamp(0.5 * (a - b) / max(a - c, 1.0e-6), -0.5, 0.5); }
    return clamp(0.5 * (a - b) / max(b - c, 1.0e-6), -0.5, 0.5);
}
"#;

// Shared by the passes that sample the full-size frames between pixels
const SAMPLED: &str = r#"
@group(0) @binding(1) var luma: texture_2d_array<f32>;
@group(0) @binding(2) var samp: sampler;
@group(0) @binding(3) var<storage, read> vecs: array<vec2<f32>>;

// Luma (codes) at a pixel, and between pixels (x, y: the pixel's own coordinates, its center at +0.5)
fn load_y(l: u32, x: i32, y: i32) -> f32 {
    let c = vec2<i32>(clamp(x, 0, i32(p.w) - 1), clamp(y, 0, i32(p.h) - 1));
    return textureLoad(luma, c, i32(l), 0).r * p.to_code;
}
fn at_y(l: u32, x: f32, y: f32) -> f32 {
    let uv = vec2<f32>(x + 0.5, y + 0.5) / vec2<f32>(f32(p.w), f32(p.h));
    return textureSampleLevel(luma, samp, uv, i32(l), 0.0).r * p.to_code;
}
const BS: f32 = 16.0;
fn block_vec(n: u32, bx: i32, by: i32) -> vec2<f32> {
    let b = clamp(vec2<i32>(bx, by), vec2<i32>(0, 0), vec2<i32>(i32(p.bw) - 1, i32(p.bh) - 1));
    return vecs[(n * p.bh + u32(b.y)) * p.bw + u32(b.x)];
}
// The candidates for pixel (x, y) and neighbour n: the four surrounding blocks' vectors blended, then each of them
fn candidates(n: u32, x: i32, y: i32) -> array<vec2<f32>, 5> {
    let f = (vec2<f32>(f32(x), f32(y)) + 0.5 - 0.5 * BS) / BS;
    let i = vec2<i32>(floor(f));
    let t = f - floor(f);
    let v00 = block_vec(n, i.x, i.y);
    let v10 = block_vec(n, i.x + 1, i.y);
    let v01 = block_vec(n, i.x, i.y + 1);
    let v11 = block_vec(n, i.x + 1, i.y + 1);
    return array<vec2<f32>, 5>(mix(mix(v00, v10, t.x), mix(v01, v11, t.x), t.y), v00, v10, v01, v11);
}
fn inside(x: f32, y: f32) -> bool {
    return x >= -0.5 && y >= -0.5 && x <= f32(p.w) - 0.5 && y <= f32(p.h) - 0.5;
}
// The 5x5 patch of the frame around (x, y), and of neighbour n where it moved by v: the mean squared difference
fn patch_ref(x: i32, y: i32) -> array<f32, 25> {
    var r: array<f32, 25>;
    for (var k = 0; k < 25; k++) { r[k] = load_y(0u, x + k % 5 - 2, y + k / 5 - 2); }
    return r;
}
// The vector of the candidates whose 3x3 patch fits best; the blend where the blocks around move alike. Another one
// only where it fits better by more than the noise could make it (s: the noise): picking by a hair would pick the
// vector where the noise lines up, and averaging that keeps the noise
fn pick(n: u32, x: i32, y: i32, r: ptr<function, array<f32, 25>>, s: f32) -> vec2<f32> {
    var cands = candidates(n, x, y);
    var spread = 0.0;
    for (var c = 1; c < 5; c++) { spread = max(spread, length(cands[c] - cands[0])); }
    if (spread < 0.35) { return cands[0]; }
    var best = cands[0];
    var best_d = 1.0e30;
    for (var c = 0; c < 5; c++) {
        let v = cands[c];
        var d = 0.0;
        for (var k = 0; k < 9; k++) {
            let dx = k % 3 - 1;
            let dy = k / 3 - 1;
            let e = (*r)[(dy + 2) * 5 + dx + 2] - at_y(n + 1u, f32(x + dx) + v.x, f32(y + dy) + v.y);
            d += e * e;
        }
        if (c == 0) { d -= 18.0 * s * s; }
        if (d < best_d) { best_d = d; best = v; }
    }
    return best;
}
// The patch the weights compare: 9x9 pixels, every other one. The noise is coarse grained (HEVC keeps little finer
// than ~4 pixels), so a small patch's distance swings with it, and the weights with it from pixel to pixel: blotches
fn patch_wide(x: i32, y: i32) -> array<f32, 25> {
    var r: array<f32, 25>;
    for (var k = 0; k < 25; k++) { r[k] = load_y(0u, x + 2 * (k % 5 - 2), y + 2 * (k / 5 - 2)); }
    return r;
}
fn patch_distance(n: u32, x: i32, y: i32, v: vec2<f32>, r: ptr<function, array<f32, 25>>) -> f32 {
    var d = 0.0;
    for (var k = 0; k < 25; k++) {
        let e = (*r)[k] - at_y(n + 1u, f32(x + 2 * (k % 5 - 2)) + v.x, f32(y + 2 * (k / 5 - 2)) + v.y);
        d += e * e;
    }
    return d / 25.0;
}
// How much a neighbour counts, by how far its patch is from the frame's, in units of the noise's (2 sigma^2 when
// they're the same but for the noise)
fn weight(ratio: f32, strength: f32) -> f32 {
    return 1.0 - smoothstep(1.0 * strength, 3.0 * strength, ratio);
}
fn median(v: ptr<function, array<f32, 14>>, n: u32) -> f32 {
    for (var i = 1u; i < n; i++) {
        let x = (*v)[i];
        var j = i;
        while (j > 0u && (*v)[j - 1u] > x) { (*v)[j] = (*v)[j - 1u]; j--; }
        (*v)[j] = x;
    }
    if (n % 2u == 1u) { return (*v)[n / 2u]; }
    return 0.5 * ((*v)[n / 2u - 1u] + (*v)[n / 2u]);
}
"#;

// To the quarter pixel at full size: the fitted vector, or a quarter of a pixel off it each way, whichever fits best
// (the block 2x2 pixels at a time)
const REFINE: &str = r#"
@group(0) @binding(4) var<storage, read_write> out_vecs: array<vec2<f32>>;

fn cost(j: u32, bx: i32, by: i32, v: vec2<f32>) -> f32 {
    var s = 0.0;
    let b = i32(BS);
    let x0 = bx * b;
    let y0 = by * b;
    for (var dy = 0; dy < b; dy += 2) {
        for (var dx = 0; dx < b; dx += 2) {
            // Between 2x2 pixels: their mean, as the motion search compares them
            let px = f32(x0 + dx) + 0.5;
            let py = f32(y0 + dy) + 0.5;
            s += abs(at_y(0u, px, py) - at_y(j, px + v.x, py + v.y));
        }
    }
    return s;
}

@compute @workgroup_size(8, 8, 1)
fn main(@builtin(global_invocation_id) id: vec3<u32>) {
    if (id.x >= p.bw || id.y >= p.bh) { return; }
    let i = (id.z * p.bh + id.y) * p.bw + id.x;
    let bx = i32(id.x);
    let by = i32(id.y);
    var best = vecs[i];
    var best_cost = cost(id.z + 1u, bx, by, best);
    var dirs = array<vec2<f32>, 4>(vec2<f32>(-0.25, 0.0), vec2<f32>(0.25, 0.0), vec2<f32>(0.0, -0.25), vec2<f32>(0.0, 0.25));
    let center = best;
    for (var k = 0; k < 4; k++) {
        let v = center + dirs[k];
        let c = cost(id.z + 1u, bx, by, v);
        if (c < best_cost) { best_cost = c; best = v; }
    }
    out_vecs[i] = best;
}
"#;

// The noise: |frame - neighbour| where it moved (the nearest whole pixel: sampling between pixels would average the
// neighbour's noise down), for the neighbours just before and after, counted by brightness. Every other pixel each way
const NOISE: &str = r#"
@group(0) @binding(4) var chroma: texture_2d_array<f32>;
@group(0) @binding(5) var<storage, read_write> hist: array<atomic<u32>>;

fn count(c: u32, bin: u32, d: f32) {
    let s = min(u32(d * 2.0), 255u);
    atomicAdd(&hist[(c * 16u + bin) * 256u + s], 1u);
}

@compute @workgroup_size(8, 8, 1)
fn main(@builtin(global_invocation_id) id: vec3<u32>) {
    let x = i32(id.x) * 2;
    let y = i32(id.y) * 2;
    if (x >= i32(p.w) || y >= i32(p.h)) { return; }
    let n = id.z;
    var cands = candidates(n, x, y);
    let v = cands[0];
    let q = vec2<i32>(round(vec2<f32>(f32(x), f32(y)) + v));
    if (q.x < 0 || q.y < 0 || q.x >= i32(p.w) || q.y >= i32(p.h)) { return; }
    let y0 = load_y(0u, x, y);
    let bin = min(u32(max(y0, 0.0) / 64.0), 15u);
    count(0u, bin, abs(y0 - load_y(n + 1u, q.x, q.y)));
    if (p.cw > 0u) {
        let c0 = vec2<i32>(x >> p.sub_x, y >> p.sub_y);
        let c1 = vec2<i32>(round(vec2<f32>(c0) + v / vec2<f32>(f32(1u << p.sub_x), f32(1u << p.sub_y))));
        if (c1.x < 0 || c1.y < 0 || c1.x >= i32(p.cw) || c1.y >= i32(p.ch)) { return; }
        let l = i32(n + 1u) * 2;
        count(1u, bin, abs(textureLoad(chroma, c0, 0, 0).r - textureLoad(chroma, c1, l, 0).r) * p.to_code);
        count(2u, bin, abs(textureLoad(chroma, c0, 1, 0).r - textureLoad(chroma, c1, l + 1, 0).r) * p.to_code);
    }
}
"#;

// The average (den_y), what the neighbours agree on (cons_y) and how much the pixel is a speck against it (speck)
const FUSE_LUMA: &str = r#"
@group(0) @binding(4) var<storage, read_write> den_y: array<f32>;
@group(0) @binding(5) var<storage, read_write> cons_y: array<f32>;
@group(0) @binding(6) var<storage, read_write> speck: array<f32>;

@compute @workgroup_size(8, 8, 1)
fn main(@builtin(global_invocation_id) id: vec3<u32>) {
    if (id.x >= p.w || id.y >= p.h) { return; }
    let x = i32(id.x);
    let y = i32(id.y);
    var r = patch_ref(x, y);
    var mean = 0.0;
    for (var k = 0; k < 25; k++) { mean += r[k]; }
    let s = sigma_at(mean / 25.0, 0u);
    let y0 = r[12];
    var wide = patch_wide(x, y);
    var acc = y0;
    var wsum = 1.0;
    var vals: array<f32, 14>;
    var nv = 0u;
    for (var n = 0u; n + 1u < p.frames; n++) {
        let v = pick(n, x, y, &r, s);
        if (!inside(f32(x) + v.x, f32(y) + v.y)) { continue; }
        let ratio = patch_distance(n, x, y, v, &wide) / (2.0 * s * s);
        let val = at_y(n + 1u, f32(x) + v.x, f32(y) + v.y);
        let w = weight(ratio, p.luma_strength);
        acc += w * val;
        wsum += w;
        vals[nv] = val;
        nv++;
    }
    var den = acc / wsum;
    // Where few neighbours count (something moved in a way they don't follow, motion blur), the pixel would keep its
    // noise among smooth ones around it, which looks mottled: there, some of the frame's own patch, alike pixels only
    var bs = 0.0;
    var bw = 0.0;
    for (var k = 0; k < 25; k++) {
        let e = (r[k] - y0) / (2.0 * s);
        let w = exp(-0.5 * e * e);
        bs += w * r[k];
        bw += w;
    }
    let alone = 1.0 - (wsum - 1.0) / max(f32(nv), 1.0);
    den = mix(den, mix(den, bs / bw, 0.6), alone);
    var cons = den;
    var sp = 0.0;
    if (nv >= 2u && p.specks > 0.0) {
        // What the neighbours agree on, and how much they do
        let med = median(&vals, nv);
        var devs: array<f32, 14>;
        for (var i = 0u; i < nv; i++) { devs[i] = abs(vals[i] - med); }
        let spread = median(&devs, nv) * 1.4826;
        sp = smoothstep(3.5 * s, 6.0 * s, abs(y0 - med)) * (1.0 - smoothstep(1.5 * s, 3.0 * s, spread)) * p.specks;
        // and stands out from the pixels around it in the frame itself, the same way: a moving edge doesn't
        var ring: array<f32, 14>;
        var nr = 0u;
        for (var k = 0; k < 25; k++) {
            let dx = k % 5 - 2;
            let dy = k / 5 - 2;
            if (max(abs(dx), abs(dy)) == 2 && nr < 14u) { ring[nr] = r[k]; nr++; }
        }
        let around = median(&ring, nr);
        sp *= smoothstep(2.0 * s, 4.0 * s, (y0 - around) * sign(y0 - med));
        // Where they agree, the mean of those that do; specks fade into the pixels around them, which take it too
        if (spread < 3.0 * s) {
            var cs = 0.0;
            var cn = 0.0;
            let lim = 2.5 * max(s, spread);
            for (var i = 0u; i < nv; i++) {
                if (abs(vals[i] - med) <= lim) { cs += vals[i]; cn += 1.0; }
            }
            cons = cs / max(cn, 1.0);
        }
    }
    let i = id.y * p.w + id.x;
    // VS_GYROFLOW_STILL_DEBUG=1: the share of the neighbours that count, 2: the specks, as the picture
    if (p._c == 1u) { den = 64.0 + 876.0 * (wsum - 1.0) / max(f32(nv), 1.0); cons = den; sp = 0.0; }
    if (p._c == 2u) { den = 64.0 + 876.0 * sp; cons = den; sp = 0.0; }
    den_y[i] = den;
    cons_y[i] = cons;
    speck[i] = sp;
}
"#;

// Specks are small: where many pixels around stand out from what the neighbours agree on, it's something that moved
// in a way the vectors don't follow (a fin's edge the frames around show as water), which stays as it is
const SPECK_SIZE: &str = r#"
@group(0) @binding(1) var<storage, read> speck: array<f32>;
@group(0) @binding(2) var<storage, read_write> speck_small: array<f32>;

@compute @workgroup_size(8, 8, 1)
fn main(@builtin(global_invocation_id) id: vec3<u32>) {
    if (id.x >= p.w || id.y >= p.h) { return; }
    let i = id.y * p.w + id.x;
    let sp = speck[i];
    if (sp <= 0.0) { speck_small[i] = 0.0; return; }
    var count = 0.0;
    for (var k = 0; k < 81; k++) {
        let x = clamp(i32(id.x) + k % 9 - 4, 0, i32(p.w) - 1);
        let y = clamp(i32(id.y) + k / 9 - 4, 0, i32(p.h) - 1);
        count += select(0.0, 1.0, speck[u32(y) * p.w + u32(x)] > 0.2);
    }
    speck_small[i] = sp * (1.0 - smoothstep(6.0, 14.0, count));
}
"#;

// The luma put together: a speck's pixels and the ones around it (its fringe doesn't stand out from the noise, and
// would stay as a ring) take what the neighbours agree on; elsewhere the average, with some of the grain back
const SPECKS: &str = r#"
@group(0) @binding(1) var luma: texture_2d_array<f32>;
@group(0) @binding(2) var<storage, read> den_y: array<f32>;
@group(0) @binding(3) var<storage, read> cons_y: array<f32>;
@group(0) @binding(4) var<storage, read> speck: array<f32>;
@group(0) @binding(5) var<storage, read_write> out_y: array<u32>;
@group(0) @binding(6) var<storage, read_write> speck_wide: array<f32>;

@compute @workgroup_size(8, 8, 1)
fn main(@builtin(global_invocation_id) id: vec3<u32>) {
    if (id.x >= p.w || id.y >= p.h) { return; }
    var sp = 0.0;
    for (var k = 0; k < 25; k++) {
        let x = clamp(i32(id.x) + k % 5 - 2, 0, i32(p.w) - 1);
        let y = clamp(i32(id.y) + k / 5 - 2, 0, i32(p.h) - 1);
        // Full within a pixel of it, fading out two away
        let far = max(abs(k % 5 - 2), abs(k / 5 - 2));
        sp = max(sp, speck[u32(y) * p.w + u32(x)] * select(1.0, 0.5, far == 2));
    }
    let i = id.y * p.w + id.x;
    let y0 = textureLoad(luma, vec2<i32>(id.xy), 0, 0).r * p.to_code;
    let den = mix(den_y[i], cons_y[i], sp);
    let out = den + p.luma_keep * (1.0 - sp) * (y0 - den);
    out_y[i] = u32(clamp(round(out * p.from_code), 0.0, f32(p.max_value)));
    speck_wide[i] = sp;
}
"#;

const FUSE_CHROMA: &str = r#"
@group(0) @binding(4) var chroma: texture_2d_array<f32>;
@group(0) @binding(5) var<storage, read> speck: array<f32>;
@group(0) @binding(6) var<storage, read_write> out_c: array<u32>;

fn load_c(l: i32, x: i32, y: i32) -> f32 {
    let c = vec2<i32>(clamp(x, 0, i32(p.cw) - 1), clamp(y, 0, i32(p.ch) - 1));
    return textureLoad(chroma, c, l, 0).r * p.to_code;
}
fn at_c(l: i32, x: f32, y: f32) -> f32 {
    let uv = vec2<f32>(x + 0.5, y + 0.5) / vec2<f32>(f32(p.cw), f32(p.ch));
    return textureSampleLevel(chroma, samp, uv, l, 0.0).r * p.to_code;
}

@compute @workgroup_size(8, 8, 1)
fn main(@builtin(global_invocation_id) id: vec3<u32>) {
    if (id.x >= p.cw || id.y >= p.ch) { return; }
    let cx = i32(id.x);
    let cy = i32(id.y);
    // The luma pixel at the chroma sample's corner: its vector and its luma weight
    let x = min(cx << p.sub_x, i32(p.w) - 1);
    let y = min(cy << p.sub_y, i32(p.h) - 1);
    let scale = vec2<f32>(f32(1u << p.sub_x), f32(1u << p.sub_y));
    var r = patch_ref(x, y);
    var mean = 0.0;
    for (var k = 0; k < 25; k++) { mean += r[k]; }
    mean /= 25.0;
    var wide = patch_wide(x, y);
    let s = sigma_at(mean, 0u);
    let su = sigma_at(mean, 1u);
    let sv = sigma_at(mean, 2u);
    var ru: array<f32, 9>;
    var rv: array<f32, 9>;
    for (var k = 0; k < 9; k++) {
        ru[k] = load_c(0, cx + k % 3 - 1, cy + k / 3 - 1);
        rv[k] = load_c(1, cx + k % 3 - 1, cy + k / 3 - 1);
    }
    let u0 = ru[4];
    let v0 = rv[4];
    var acc_u = u0;
    var acc_v = v0;
    var wsum = 1.0;
    var us: array<f32, 14>;
    var vs: array<f32, 14>;
    var nv = 0u;
    for (var n = 0u; n + 1u < p.frames; n++) {
        let v = pick(n, x, y, &r, s);
        if (!inside(f32(x) + v.x, f32(y) + v.y)) { continue; }
        // Color is smooth: a luma patch that's a little off still has the right color, so a laxer luma weight
        let wy = weight(patch_distance(n, x, y, v, &wide) / (2.0 * s * s), 1.5 * p.chroma_strength);
        let vc = v / scale;
        let l = i32(n + 1u) * 2;
        var d = 0.0;
        for (var k = 0; k < 9; k++) {
            let px = f32(cx + k % 3 - 1) + vc.x;
            let py = f32(cy + k / 3 - 1) + vc.y;
            let eu = ru[k] - at_c(l, px, py);
            let ev = rv[k] - at_c(l + 1, px, py);
            d += eu * eu / (2.0 * su * su) + ev * ev / (2.0 * sv * sv);
        }
        let w = wy * weight(d / 18.0, p.chroma_strength);
        let un = at_c(l, f32(cx) + vc.x, f32(cy) + vc.y);
        let vn = at_c(l + 1, f32(cx) + vc.x, f32(cy) + vc.y);
        acc_u += w * un;
        acc_v += w * vn;
        wsum += w;
        us[nv] = un;
        vs[nv] = vn;
        nv++;
    }
    var du = acc_u / wsum;
    var dv = acc_v / wsum;
    // A speck in the luma takes the color the neighbours agree on there too
    var sp = 0.0;
    for (var k = 0; k < 4; k++) {
        let lx = min(x + (k % 2) * i32(p.sub_x), i32(p.w) - 1);
        let ly = min(y + (k / 2) * i32(p.sub_y), i32(p.h) - 1);
        sp = max(sp, speck[u32(ly) * p.w + u32(lx)]);
    }
    if (sp > 0.0 && nv >= 2u) {
        du = mix(du, median(&us, nv), sp);
        dv = mix(dv, median(&vs, nv), sp);
    }
    let ou = du + p.chroma_keep * (1.0 - sp) * (u0 - du);
    let ov = dv + p.chroma_keep * (1.0 - sp) * (v0 - dv);
    let i = id.y * p.cw + id.x;
    let mx = f32(p.max_value);
    out_c[i] = u32(clamp(round(ou * p.from_code), 0.0, mx));
    out_c[p.cw * p.ch + i] = u32(clamp(round(ov * p.from_code), 0.0, mx));
}
"#;

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Params {
    w: u32, h: u32, cw: u32, ch: u32,
    frames: u32, sub_x: u32, sub_y: u32, max_value: u32,
    sw: u32, sh: u32, lw: u32, lh: u32,
    bw: u32, bh: u32, cbw: u32, cbh: u32,
    search: i32, step: u32, bs: u32, win: u32,
    fit: u32, _c: u32, _d: u32, _e: u32,
    to_code: f32, from_code: f32, luma_keep: f32, chroma_keep: f32,
    luma_strength: f32, chroma_strength: f32, specks: f32, lambda: f32,
    sigma: [[f32; 4]; BINS],
}

// ---------------------------------------------------------------------------------------------------
// The GPU, set up once per process (the device and the shaders: a few hundred ms the first time)

struct Gpu {
    device: wgpu::Device,
    queue: wgpu::Queue,
    sampler: wgpu::Sampler,
    down_texture: wgpu::ComputePipeline,
    down_buffer: wgpu::ComputePipeline,
    motion_texture: wgpu::ComputePipeline,
    motion_buffer: wgpu::ComputePipeline,
    refine: wgpu::ComputePipeline,
    noise: wgpu::ComputePipeline,
    fuse_luma: wgpu::ComputePipeline,
    specks: wgpu::ComputePipeline,
    speck_size: wgpu::ComputePipeline,
    fuse_chroma: wgpu::ComputePipeline,
}

static GPU: Mutex<Option<Result<Arc<Gpu>, String>>> = Mutex::new(None);

fn gpu() -> Result<Arc<Gpu>, String> {
    let mut g = GPU.lock();
    if g.is_none() { *g = Some(Gpu::new().map(Arc::new)); }
    g.as_ref().unwrap().clone()
}

/// Sets the GPU up in the background, so the first paused frame doesn't wait for it
pub fn warm_up() {
    if GPU.lock().is_some() { return; }
    std::thread::spawn(|| { let _ = gpu(); });
}

/// The same, `after` seconds from now and at low priority: Stabilize asks for it as a clip starts (with the clean still
/// frame on), when the first seconds belong to getting playback going. Once per process
pub fn warm_up_later(after: f64) {
    static ASKED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
    if ASKED.swap(true, std::sync::atomic::Ordering::Relaxed) { return; }
    std::thread::spawn(move || {
        #[cfg(target_os = "macos")]
        unsafe { libc::pthread_set_qos_class_self_np(libc::qos_class_t::QOS_CLASS_UTILITY, 0); }
        std::thread::sleep(std::time::Duration::from_secs_f64(after.max(0.0)));
        if GPU.lock().is_none() { let _ = gpu(); }
    });
}

impl Gpu {
    fn new() -> Result<Gpu, String> {
        let t = Instant::now();
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            force_fallback_adapter: false,
            compatible_surface: None,
            apply_limit_buckets: false,
        })).map_err(|e| format!("no GPU: {e}"))?;
        // 16-bit samples filtered between pixels in hardware, and the pyramid's 32-bit floats
        let features = wgpu::Features::TEXTURE_FORMAT_16BIT_NORM | wgpu::Features::FLOAT32_FILTERABLE;
        if !adapter.features().contains(features) {
            return Err(format!("the GPU lacks {:?}", features - adapter.features()));
        }
        let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some("gyroflow still"),
            required_features: features,
            required_limits: adapter.limits(),
            memory_hints: wgpu::MemoryHints::Performance,
            trace: wgpu::Trace::Off,
            experimental_features: wgpu::ExperimentalFeatures::disabled(),
        })).map_err(|e| format!("GPU device: {e}"))?;
        device.on_uncaptured_error(Arc::new(|e| log::error!("Still: GPU: {e}")));
        let scope = device.push_error_scope(wgpu::ErrorFilter::Validation);
        let pipeline = |parts: &[&str]| {
            let source: String = parts.concat();
            let module = device.create_shader_module(wgpu::ShaderModuleDescriptor { label: None, source: wgpu::ShaderSource::Wgsl(source.into()) });
            device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                label: None, layout: None, module: &module, entry_point: Some("main"),
                compilation_options: Default::default(), cache: Default::default(),
            })
        };
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let gpu = Gpu {
            down_texture: pipeline(&[COMMON, FETCH_TEXTURE, DOWN]),
            down_buffer: pipeline(&[COMMON, FETCH_BUFFER, DOWN]),
            motion_texture: pipeline(&[COMMON, FETCH_TEXTURE, MOTION]),
            motion_buffer: pipeline(&[COMMON, FETCH_BUFFER, MOTION]),
            refine: pipeline(&[COMMON, SAMPLED, REFINE]),
            noise: pipeline(&[COMMON, SAMPLED, NOISE]),
            fuse_luma: pipeline(&[COMMON, SAMPLED, FUSE_LUMA]),
            specks: pipeline(&[COMMON, SPECKS]),
            speck_size: pipeline(&[COMMON, SPECK_SIZE]),
            fuse_chroma: pipeline(&[COMMON, SAMPLED, FUSE_CHROMA]),
            device, queue, sampler,
        };
        if let Some(e) = pollster::block_on(scope.pop()) { return Err(format!("shaders: {e}")); }
        log::info!("Still: GPU set up in {} ms", t.elapsed().as_millis());
        Ok(gpu)
    }

    fn buffer(&self, size: u64, usage: wgpu::BufferUsages) -> wgpu::Buffer {
        self.device.create_buffer(&wgpu::BufferDescriptor { label: None, size: size.max(16), usage, mapped_at_creation: false })
    }

    fn uniform(&self, params: &Params) -> wgpu::Buffer {
        let b = self.buffer(std::mem::size_of::<Params>() as u64, wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST);
        self.queue.write_buffer(&b, 0, bytemuck::bytes_of(params));
        b
    }

    /// `resources` by binding; a pass that doesn't use one of SAMPLED's leaves it out (the layout is the shader's own)
    fn dispatch(&self, encoder: &mut wgpu::CommandEncoder, pipeline: &wgpu::ComputePipeline, resources: &[wgpu::BindingResource], skip: &[u32], groups: (u32, u32, u32)) {
        let layout = pipeline.get_bind_group_layout(0);
        let entries: Vec<_> = resources.iter().enumerate().filter(|(i, _)| !skip.contains(&(*i as u32)))
            .map(|(i, r)| wgpu::BindGroupEntry { binding: i as u32, resource: r.clone() }).collect();
        let bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor { label: None, layout: &layout, entries: &entries });
        let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor { label: None, timestamp_writes: None });
        pass.set_pipeline(pipeline);
        pass.set_bind_group(0, Some(&bind_group), &[]);
        pass.dispatch_workgroups(groups.0.max(1), groups.1.max(1), groups.2.max(1));
    }

    /// With VS_GYROFLOW_STILL_PROFILE set: what's recorded so far run and timed, as `name`
    fn profile(&self, encoder: &mut wgpu::CommandEncoder, name: &str, since: &mut Instant) {
        if std::env::var_os("VS_GYROFLOW_STILL_PROFILE").is_none() { return; }
        let done = std::mem::replace(encoder, self.device.create_command_encoder(&Default::default()));
        self.submit_and_wait(done);
        log::info!("Still: {name} {:.1} ms", since.elapsed().as_secs_f64() * 1000.0);
        *since = Instant::now();
    }

    fn submit_and_wait(&self, encoder: wgpu::CommandEncoder) {
        self.queue.submit(Some(encoder.finish()));
        let _ = self.device.poll(wgpu::PollType::Wait { submission_index: None, timeout: None });
    }

    fn read(&self, buffer: &wgpu::Buffer, size: u64) -> Result<Vec<u8>, String> {
        let staging = self.buffer(size, wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST);
        let mut encoder = self.device.create_command_encoder(&Default::default());
        encoder.copy_buffer_to_buffer(buffer, 0, &staging, 0, size);
        self.queue.submit(Some(encoder.finish()));
        let slice = staging.slice(..size);
        let (tx, rx) = mpsc::channel();
        slice.map_async(wgpu::MapMode::Read, move |r| { let _ = tx.send(r); });
        let _ = self.device.poll(wgpu::PollType::Wait { submission_index: None, timeout: None });
        rx.recv().map_err(|_| "GPU readback lost".to_string())?.map_err(|e| format!("GPU readback: {e}"))?;
        let data = slice.get_mapped_range().map_err(|e| format!("GPU readback: {e:?}"))?.to_vec();
        staging.unmap();
        Ok(data)
    }

    /// A 2D array texture of `layers` planes, as they are in memory (`bytes` per sample: 1 or 2)
    fn planes_texture(&self, planes: &[Plane], bytes: usize) -> wgpu::Texture {
        let (w, h) = (planes[0].width as u32, planes[0].height as u32);
        let texture = self.device.create_texture(&wgpu::TextureDescriptor {
            label: None,
            size: wgpu::Extent3d { width: w, height: h, depth_or_array_layers: planes.len() as u32 },
            mip_level_count: 1, sample_count: 1, dimension: wgpu::TextureDimension::D2,
            format: if bytes == 1 { wgpu::TextureFormat::R8Unorm } else { wgpu::TextureFormat::R16Unorm },
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        for (layer, plane) in planes.iter().enumerate() {
            let len = plane.stride * (plane.height - 1) + plane.width * bytes;
            self.queue.write_texture(
                wgpu::TexelCopyTextureInfo { texture: &texture, mip_level: 0, origin: wgpu::Origin3d { x: 0, y: 0, z: layer as u32 }, aspect: wgpu::TextureAspect::All },
                &plane.data[..len],
                wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(plane.stride as u32), rows_per_image: Some(plane.height as u32) },
                wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
            );
        }
        texture
    }
}

// ---------------------------------------------------------------------------------------------------
// The denoising

/// The noise (codes) by brightness from the histograms. The pixels where something moved in a way the vectors don't
/// follow differ by a lot more than the noise, so the lower quartile, not the median: |a - b| of two noisy samples is
/// under 0.4506 sigma for a quarter of them (sqrt(2) times the normal quantile at 0.625). Bins with too few pixels take
/// their neighbours'
fn sigma_from(hist: &[u32]) -> [[f32; 4]; BINS] {
    let mut sigma = [[0.0f32; 4]; BINS];
    for c in 0..3 {
        let mut known: Vec<Option<f32>> = (0..BINS).map(|b| {
            let h = &hist[(c * BINS + b) * STEPS..(c * BINS + b + 1) * STEPS];
            let total: u64 = h.iter().map(|&v| v as u64).sum();
            if total < 400 { return None; }
            let mut seen = 0u64;
            for (i, &v) in h.iter().enumerate() {
                if (seen + v as u64) * 4 >= total {
                    let into = (total as f32 / 4.0 - seen as f32) / v.max(1) as f32;
                    let quartile = (i as f32 + into) * 0.5;
                    return Some((quartile / 0.4506).max(0.3));
                }
                seen += v as u64;
            }
            None
        }).collect();
        if known.iter().all(|v| v.is_none()) { known = vec![Some(4.0); BINS]; }
        for b in 0..BINS {
            let v = known[b].or_else(|| (1..BINS).find_map(|d| {
                let lo = b.checked_sub(d).and_then(|i| known[i]);
                let hi = known.get(b + d).copied().flatten();
                lo.or(hi)
            })).unwrap_or(4.0);
            sigma[b][c] = v;
        }
    }
    sigma
}

/// The bindings the passes on the full-size frames start with (SAMPLED)
fn sampled<'a>(luma: &'a wgpu::TextureView, sampler: &'a wgpu::Sampler, u: &'a wgpu::Buffer, vecs: &'a wgpu::Buffer) -> [wgpu::BindingResource<'a>; 4] {
    [u.as_entire_binding(), wgpu::BindingResource::TextureView(luma), wgpu::BindingResource::Sampler(sampler), vecs.as_entire_binding()]
}

/// `frames[0]` without its noise, from it and its neighbours `frames[1..]` (nearest first). All the same size and
/// format: `bytes` per sample, values up to `max_value`, chroma subsampled by (`sub_x`, `sub_y`)
pub fn denoise(frames: &[Frame], bytes: usize, max_value: u32, sub: (u32, u32), settings: &Settings) -> Result<Output, String> {
    if frames.len() < 2 { return Err("no neighbours".into()); }
    let gpu = gpu()?;
    let validation = gpu.device.push_error_scope(wgpu::ErrorFilter::Validation);
    let memory = gpu.device.push_error_scope(wgpu::ErrorFilter::OutOfMemory);
    let result = denoise_on(&gpu, frames, bytes, max_value, sub, settings);
    if let Some(e) = pollster::block_on(memory.pop()) { return Err(format!("GPU: {e}")); }
    if let Some(e) = pollster::block_on(validation.pop()) { return Err(format!("GPU: {e}")); }
    result
}

fn denoise_on(g: &Gpu, frames: &[Frame], bytes: usize, max_value: u32, sub: (u32, u32), settings: &Settings) -> Result<Output, String> {
    let start = Instant::now();
    let frames = &frames[..frames.len().min(1 + 2 * MAX_RADIUS)];
    let f = frames.len() as u32;
    let luma: Vec<Plane> = frames.iter().map(|fr| fr.planes[0]).collect();
    let (w, h) = (luma[0].width as u32, luma[0].height as u32);
    let color = frames[0].planes.len() >= 3;
    let (cw, ch) = if color { (frames[0].planes[1].width as u32, frames[0].planes[1].height as u32) } else { (0, 0) };
    let to_code = (if bytes == 1 { 255.0 } else { 65535.0 }) * 1023.0 / max_value as f32;
    let from_code = max_value as f32 / 1023.0;

    let luma_tex = g.planes_texture(&luma, bytes);
    let chroma: Vec<Plane> = if color { frames.iter().flat_map(|fr| [fr.planes[1], fr.planes[2]]).collect() } else { vec![] };
    // Gray: a texture of one tiny layer, never read
    let chroma_tex = if color { g.planes_texture(&chroma, bytes) } else { g.planes_texture(&[Plane { data: &[0, 0], stride: 2, width: 1, height: 1 }], 2) };
    let luma_view = luma_tex.create_view(&wgpu::TextureViewDescriptor { dimension: Some(wgpu::TextureViewDimension::D2Array), ..Default::default() });
    let chroma_view = chroma_tex.create_view(&wgpu::TextureViewDescriptor { dimension: Some(wgpu::TextureViewDimension::D2Array), ..Default::default() });

    let base = Params {
        w, h, cw, ch, frames: f, sub_x: sub.0, sub_y: sub.1, max_value,
        sw: w, sh: h, lw: w, lh: h, bw: 0, bh: 0, cbw: 0, cbh: 0,
        search: 0, step: 1, bs: 16, win: 16, fit: 0, _d: 0, _e: 0,
        _c: std::env::var("VS_GYROFLOW_STILL_DEBUG").ok().and_then(|v| v.parse().ok()).unwrap_or(0),
        to_code, from_code, luma_keep: settings.luma_keep.clamp(0.0, 1.0), chroma_keep: settings.chroma_keep.clamp(0.0, 1.0),
        luma_strength: settings.luma_strength.max(0.05), chroma_strength: settings.chroma_strength.max(0.05), specks: settings.specks.clamp(0.0, 1.0),
        lambda: 0.2, sigma: [[4.0; 4]; BINS],
    };
    let storage = wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC | wgpu::BufferUsages::COPY_DST;
    let groups = |x: u32, y: u32, z: u32| (x.div_ceil(8), y.div_ceil(8), z);

    // The pyramid: level 1 on down to one whose width is no less than 160
    let mut sizes = vec![(w, h)];
    while sizes.last().unwrap().0 / 2 >= 160 && sizes.len() < 6 {
        let (lw, lh) = *sizes.last().unwrap();
        sizes.push((lw / 2, lh / 2));
    }
    let mut encoder = g.device.create_command_encoder(&Default::default());
    let mut since = Instant::now();
    let mut levels: Vec<wgpu::Buffer> = vec![];
    let mut uniforms = vec![];
    for l in 1..sizes.len() {
        let (lw, lh) = sizes[l];
        let (sw, sh) = sizes[l - 1];
        let dst = g.buffer((f * lw * lh * 4) as u64, storage);
        let u = g.uniform(&Params { sw, sh, lw, lh, ..base });
        if l == 1 {
            g.dispatch(&mut encoder, &g.down_texture, &[u.as_entire_binding(), wgpu::BindingResource::TextureView(&luma_view), dst.as_entire_binding()], &[], groups(lw, lh, f));
        } else {
            g.dispatch(&mut encoder, &g.down_buffer, &[u.as_entire_binding(), levels[l - 2].as_entire_binding(), dst.as_entire_binding()], &[], groups(lw, lh, f));
        }
        uniforms.push(u);
        levels.push(dst);
    }
    g.profile(&mut encoder, "upload and pyramid", &mut since);

    // The motion, coarse to fine
    // Blocks of 8 pixels on the coarser levels, 16 at full size (each the same 16 pixels of the level above)
    let block_size = |l: usize| if l == 0 { 16 } else { 8 };
    let mut coarse: Option<(wgpu::Buffer, (u32, u32))> = None;
    for l in (0..sizes.len()).rev() {
        let (lw, lh) = sizes[l];
        let bs = block_size(l);
        let (bw, bh) = (lw.div_ceil(bs), lh.div_ceil(bs));
        let vecs = g.buffer(((f - 1) * bw * bh * 8) as u64, storage);
        let coarsest = l == sizes.len() - 1;
        let (cbw, cbh) = coarse.as_ref().map(|c| c.1).unwrap_or((0, 0));
        let u = g.uniform(&Params {
            sw: lw, sh: lh, lw, lh, bw, bh, cbw, cbh,
            search: if coarsest { 4 } else if l <= 1 { 1 } else { 2 },
            // Every other pixel of the window but on the coarsest level, where the noise is averaged away most
            step: if coarsest { 1 } else { 2 },
            bs, win: 16, fit: (l == 0) as u32,
            ..base
        });
        let dummy;
        let coarse_buf = match &coarse { Some((b, _)) => b, None => { dummy = g.buffer(16, storage); &dummy } };
        if l == 0 {
            g.dispatch(&mut encoder, &g.motion_texture, &[u.as_entire_binding(), wgpu::BindingResource::TextureView(&luma_view), coarse_buf.as_entire_binding(), vecs.as_entire_binding()], &[], groups(bw, bh, f - 1));
        } else {
            g.dispatch(&mut encoder, &g.motion_buffer, &[u.as_entire_binding(), levels[l - 1].as_entire_binding(), coarse_buf.as_entire_binding(), vecs.as_entire_binding()], &[], groups(bw, bh, f - 1));
        }
        uniforms.push(u);
        coarse = Some((vecs, (bw, bh)));
        g.profile(&mut encoder, &format!("motion at level {l}"), &mut since);
    }
    let (coarse_vecs, (bw, bh)) = coarse.unwrap();
    let vecs = g.buffer(((f - 1) * bw * bh * 8) as u64, storage);
    let full = Params { bw, bh, ..base };
    let u = g.uniform(&full);
    let [a, b, c, d] = sampled(&luma_view, &g.sampler, &u, &coarse_vecs);
    g.dispatch(&mut encoder, &g.refine, &[a, b, c, d, vecs.as_entire_binding()], &[], groups(bw, bh, f - 1));
    g.profile(&mut encoder, "quarter pixel", &mut since);

    // The noise, from the neighbours just before and after
    let hist = g.buffer((3 * BINS * STEPS * 4) as u64, storage);
    encoder.clear_buffer(&hist, 0, None);
    let [a, b, c, d] = sampled(&luma_view, &g.sampler, &u, &vecs);
    g.dispatch(&mut encoder, &g.noise, &[a, b, c, d, wgpu::BindingResource::TextureView(&chroma_view), hist.as_entire_binding()], &[2], groups(w.div_ceil(2), h.div_ceil(2), (f - 1).min(2)));
    g.submit_and_wait(encoder);
    let motion_ms = start.elapsed().as_secs_f64() * 1000.0;
    let hist: Vec<u32> = bytemuck::cast_slice(&g.read(&hist, (3 * BINS * STEPS * 4) as u64)?).to_vec();
    let sigma = sigma_from(&hist);

    // The frame put together
    let mut encoder = g.device.create_command_encoder(&Default::default());
    let u = g.uniform(&Params { sigma, ..full });
    let out_y = g.buffer((w * h * 4) as u64, storage);
    let plane = |_: ()| g.buffer((w * h * 4) as u64, storage);
    let (den_y, cons_y, speck, speck_small, speck_wide) = (plane(()), plane(()), plane(()), plane(()), plane(()));
    let [a, b, c, d] = sampled(&luma_view, &g.sampler, &u, &vecs);
    let mut since = Instant::now();
    g.dispatch(&mut encoder, &g.fuse_luma, &[a, b, c, d, den_y.as_entire_binding(), cons_y.as_entire_binding(), speck.as_entire_binding()], &[], groups(w, h, 1));
    g.dispatch(&mut encoder, &g.speck_size, &[u.as_entire_binding(), speck.as_entire_binding(), speck_small.as_entire_binding()], &[], groups(w, h, 1));
    g.dispatch(&mut encoder, &g.specks, &[u.as_entire_binding(), wgpu::BindingResource::TextureView(&luma_view), den_y.as_entire_binding(), cons_y.as_entire_binding(),
                                          speck_small.as_entire_binding(), out_y.as_entire_binding(), speck_wide.as_entire_binding()], &[], groups(w, h, 1));
    g.profile(&mut encoder, "luma", &mut since);
    let out_c = g.buffer((2 * cw * ch * 4) as u64, storage);
    if color {
        let [a, b, c, d] = sampled(&luma_view, &g.sampler, &u, &vecs);
        g.dispatch(&mut encoder, &g.fuse_chroma, &[a, b, c, d, wgpu::BindingResource::TextureView(&chroma_view), speck_wide.as_entire_binding(), out_c.as_entire_binding()], &[], groups(cw, ch, 1));
    }
    g.submit_and_wait(encoder);

    let narrow = |data: Vec<u8>| -> Vec<u16> { bytemuck::cast_slice::<u8, u32>(&data).iter().map(|&v| v as u16).collect() };
    let mut planes = vec![narrow(g.read(&out_y, (w * h * 4) as u64)?)];
    if color {
        let mut c = narrow(g.read(&out_c, (2 * cw * ch * 4) as u64)?);
        let v = c.split_off((cw * ch) as usize);
        planes.push(c);
        planes.push(v);
    }
    let millis = start.elapsed().as_secs_f64() * 1000.0;
    log::debug!("Still: motion and noise {motion_ms:.0} ms, all {millis:.0} ms");
    Ok(Output { planes, sigma: [sigma[8][0], sigma[8][1], sigma[8][2]], millis })
}

// ---------------------------------------------------------------------------------------------------
// The filter

struct StillFilter {
    node: *mut VSNode,
    time: f64,
    radius: c_int,
    fallback_duration: f64,
    settings: Settings,
    /// When mpv's seek loaded it, for the log
    created: Instant,
}

const STAGE_FRAME: usize = 1;
const STAGE_WINDOW: usize = 2;

unsafe fn is_skipped(api: &VSAPI, props: *const VSMap) -> bool {
    let mut err = VSMapPropertyError::Success;
    let v = unsafe { (api.mapGetInt)(props, crate::SKIPPED_PROP.as_ptr(), 0, &mut err) };
    err == VSMapPropertyError::Success && v != 0
}

impl StillFilter {
    unsafe fn time_of(&self, n: c_int, api: &VSAPI, frame: *const VSFrame) -> (f64, f64) {
        let props = unsafe { (api.getFramePropertiesRO)(frame) };
        let duration = unsafe { crate::frame_duration(api, props, self.fallback_duration) };
        (unsafe { crate::frame_pts(api, props, n, duration) }, duration)
    }

    /// The frame `n` (the paused one) from the window around it; Err leaves it as it is
    unsafe fn build(&self, n: c_int, api: &VSAPI, ctx: *mut VSFrameContext, core: *mut VSCore) -> Result<*const VSFrame, String> {
        let get = |k: c_int| unsafe { (api.getFrameFilter)(k, self.node, ctx) };
        let reference = get(n);
        let (t0, _) = unsafe { self.time_of(n, api, reference) };
        let format = unsafe { &*(api.getVideoFrameFormat)(reference) };
        let size = |fr: *const VSFrame| unsafe { ((api.getFrameWidth)(fr, 0), (api.getFrameHeight)(fr, 0)) };
        // Nearest first, before then after; the clip's ends repeat its first and last frame, which counts once
        let mut window = vec![reference];
        let mut times = vec![t0];
        for d in 1..=self.radius {
            for k in [n - d, n + d] {
                let fr = get(k);
                let (t, _) = unsafe { self.time_of(k, api, fr) };
                let props = unsafe { (api.getFramePropertiesRO)(fr) };
                let same = unsafe { (*(api.getVideoFrameFormat)(fr)).bytes_per_sample == format.bytes_per_sample } && size(fr) == size(reference);
                if same && !unsafe { is_skipped(api, props) } && times.iter().all(|&s| (s - t).abs() > 1.0e-4) {
                    window.push(fr);
                    times.push(t);
                } else {
                    unsafe { (api.freeFrame)(fr) };
                }
            }
        }
        let planes_of = |fr: *const VSFrame| -> Frame {
            Frame { planes: (0..format.num_planes).map(|p| unsafe {
                let (w, h) = ((api.getFrameWidth)(fr, p) as usize, (api.getFrameHeight)(fr, p) as usize);
                let stride = (api.getStride)(fr, p) as usize;
                Plane { data: std::slice::from_raw_parts((api.getReadPtr)(fr, p), stride * h), stride, width: w, height: h }
            }).collect() }
        };
        let frames: Vec<Frame> = window.iter().map(|&fr| planes_of(fr)).collect();
        let bytes = format.bytes_per_sample as usize;
        let max_value = ((1u64 << format.bits_per_sample) - 1) as u32;
        let sub = (format.sub_sampling_w as u32, format.sub_sampling_h as u32);
        // A panic mustn't cross into VapourSynth (mpv would go down with it)
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| denoise(&frames, bytes, max_value, sub, &self.settings)))
            .unwrap_or_else(|_| Err("panicked".into()));
        let result = result.map(|out| unsafe {
            let (w, h) = size(reference);
            let dst = (api.newVideoFrame)(format, w, h, reference, core);
            for (p, data) in out.planes.iter().enumerate() {
                let p = p as c_int;
                let (pw, ph) = ((api.getFrameWidth)(dst, p) as usize, (api.getFrameHeight)(dst, p) as usize);
                let stride = (api.getStride)(dst, p) as usize;
                let src_stride = (api.getStride)(reference, p) as usize;
                let base = (api.getWritePtr)(dst, p);
                let src = (api.getReadPtr)(reference, p);
                for y in 0..ph {
                    let row = base.add(y * stride);
                    // The last row of luma carries the stabilization's marker (dispersion.rs): as it was
                    if p == 0 && y == ph - 1 {
                        std::ptr::copy_nonoverlapping(src.add(y * src_stride), row, pw * bytes);
                        continue;
                    }
                    let line = &data[y * pw..(y + 1) * pw];
                    if bytes == 1 {
                        for (x, &v) in line.iter().enumerate() { *row.add(x) = v as u8; }
                    } else {
                        std::ptr::copy_nonoverlapping(line.as_ptr() as *const u8, row, pw * 2);
                    }
                }
            }
            log::info!("Still frame at {t0:.3} s: {} frames, noise {:.1}/{:.1}/{:.1} codes (10-bit), {:.0} ms ({} ms after the seek)",
                       frames.len(), out.sigma[0], out.sigma[1], out.sigma[2], out.millis, self.created.elapsed().as_millis());
            dst as *const VSFrame
        });
        for &fr in &window[1..] { unsafe { (api.freeFrame)(fr) }; }
        match result {
            Ok(dst) => {
                unsafe { (api.freeFrame)(reference) };
                Ok(dst)
            }
            Err(e) => {
                log::warn!("Still frame at {t0:.3} s left as it is: {e}");
                Ok(reference)
            }
        }
    }
}

unsafe extern "system-unwind" fn still_get_frame(n: c_int, reason: VSActivationReason, instance: *mut c_void, frame_data: *mut *mut c_void, ctx: *mut VSFrameContext, core: *mut VSCore, vsapi: *const VSAPI) -> *const VSFrame {
    let f = unsafe { &*(instance as *const StillFilter) };
    let api = unsafe { &*vsapi };
    match reason {
        VSActivationReason::Initial => unsafe {
            (api.requestFrameFilter)(n, f.node, ctx);
            *frame_data = STAGE_FRAME as *mut c_void;
            null()
        },
        VSActivationReason::AllFramesReady if unsafe { *frame_data } as usize == STAGE_FRAME => unsafe {
            let frame = (api.getFrameFilter)(n, f.node, ctx);
            let (t, duration) = f.time_of(n, api, frame);
            if (t - f.time).abs() >= duration * 0.5 || is_skipped(api, (api.getFramePropertiesRO)(frame)) {
                return frame;
            }
            // The paused frame: its neighbours too, then it's called again
            log::debug!("Still: frame {n} is the one, {} ms after the seek", f.created.elapsed().as_millis());
            (api.freeFrame)(frame);
            for k in n - f.radius..=n + f.radius { (api.requestFrameFilter)(k, f.node, ctx); }
            *frame_data = STAGE_WINDOW as *mut c_void;
            null()
        },
        VSActivationReason::AllFramesReady => unsafe {
            match f.build(n, api, ctx, core) {
                Ok(frame) => frame,
                Err(e) => {
                    let msg = CString::new(format!("Still: {e}")).unwrap_or_default();
                    (api.setFilterError)(msg.as_ptr(), ctx);
                    null()
                }
            }
        },
        _ => null()
    }
}

unsafe extern "system-unwind" fn still_free(instance: *mut c_void, _core: *mut VSCore, vsapi: *const VSAPI) {
    let f = unsafe { Box::from_raw(instance as *mut StillFilter) };
    unsafe { ((*vsapi).freeNode)(f.node) };
}

/// `core.gyroflow.Still(clip, time, radius=3, luma_strength, chroma_strength, luma_keep, chroma_keep, specks)`
pub unsafe extern "system-unwind" fn still_create(input: *const VSMap, out: *mut VSMap, _user_data: *mut c_void, core: *mut VSCore, vsapi: *const VSAPI) {
    crate::init_logging();
    let api = unsafe { &*vsapi };
    let mut err = VSMapPropertyError::Success;
    let node = unsafe { (api.mapGetNode)(input, c"clip".as_ptr(), 0, &mut err) };
    let vi = unsafe { (*(api.getVideoInfo)(node)).clone() };
    let fmt = &vi.format;
    if fmt.sample_type != VSSampleType::Integer || fmt.bytes_per_sample > 2 || !(fmt.num_planes == 1 || fmt.num_planes == 3) {
        return unsafe { crate::set_error(api, out, node, "Still", "only 8-16 bit integer YUV or gray".into()) };
    }
    let f64_arg = |key: &std::ffi::CStr| unsafe { crate::map_get_f64(api, input, key) };
    let Some(time) = f64_arg(c"time") else {
        return unsafe { crate::set_error(api, out, node, "Still", "time is required".into()) };
    };
    let radius = unsafe { crate::map_get_i64(api, input, c"radius") }.unwrap_or(3).clamp(1, MAX_RADIUS as i64) as c_int;
    let d = Settings::default();
    let settings = Settings {
        luma_strength: f64_arg(c"luma_strength").map(|v| v as f32).unwrap_or(d.luma_strength),
        chroma_strength: f64_arg(c"chroma_strength").map(|v| v as f32).unwrap_or(d.chroma_strength),
        luma_keep: f64_arg(c"luma_keep").map(|v| v as f32).unwrap_or(d.luma_keep),
        chroma_keep: f64_arg(c"chroma_keep").map(|v| v as f32).unwrap_or(d.chroma_keep),
        specks: f64_arg(c"specks").map(|v| v as f32).unwrap_or(d.specks),
    };
    warm_up();
    log::info!("Still: the frame at {time:.3} s, {radius} frames each side");
    let fallback_duration = if vi.fps_num > 0 && vi.fps_den > 0 { vi.fps_den as f64 / vi.fps_num as f64 } else { 1.0 / 30.0 };
    let filter = Box::new(StillFilter { node, time, radius, fallback_duration, settings, created: Instant::now() });
    let deps = [VSFilterDependency { source: node, request_pattern: VSRequestPattern::General }];
    unsafe {
        (api.createVideoFilter)(out, c"Still".as_ptr(), &vi, still_get_frame, Some(still_free), VSFilterMode::Parallel,
                                deps.as_ptr(), deps.len() as c_int, Box::into_raw(filter) as *mut c_void, core);
    }
}
