// SPDX-License-Identifier: GPL-3.0-or-later
//
// The underwater colour (#273) for gyroflow-export: what the player shows a plain SDR clip with through
// shaders/underwater_hlg.glsl ("uw", the dlogm plugin's pseudo-curve), here without mpv. Three parts, each the twin of
// the player's:
//   - Params::to_hlg: the shader, pixel by pixel (the same math as underwater_hlg.glsl; gpu_lut.rs has it in WGSL)
//   - estimate: a frame's numbers from a 320x180 grid of it (mpv/scripts/dlogm/uw_estimate.lua, line by line), and
//     grid_from_bgr0, the grid from the 960x540 bgr0 frame ffmpeg gives (the player's measure() reads the same)
//   - Schedule: the numbers each frame of an export gets, as playback has them (uwcolor.lua): measured a few times a
//     second, half a second ahead, eased towards the last measurement (quicker when far), taken at once at the start
//
// Tests check estimate against the Lua rule on the same grids (tests/uwcolor_cases.json, written by
// tests/uwcolor_test.lua).

/// The frame's numbers (the shader's uw_dr, uw_db, uw_k, uw_ev)
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct Estimate { pub dr: f64, pub db: f64, pub k: f64, pub ev: f64, pub f: f64 }

/// What the shader applies: the frame's numbers, the strength (uw_s) and the highlights' stretch (uw_hp)
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Params { pub dr: f32, pub db: f32, pub k: f32, pub ev: f32, pub s: f32, pub hp: f32 }

/// The clip's settings (the transform bundle's uw_strength, uw_highlights)
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Settings { pub strength: f64, pub highlights: bool }

impl Default for Settings { fn default() -> Self { Settings { strength: 1.0, highlights: false } } }

/// uwcolor.lua's HIGHLIGHTS: the camera's white at 2.5x SDR white
pub const HIGHLIGHTS: f32 = 2.5;

impl Params {
    /// As uwcolor.lua's send() writes them: the numbers to 4 places, the strength to 3
    pub fn new(e: &Estimate, s: &Settings) -> Params {
        let r4 = |x: f64| ((x * 1e4).round() / 1e4) as f32;
        Params { dr: r4(e.dr), db: r4(e.db), k: r4(e.k), ev: r4(e.ev), s: ((s.strength * 1e3).round() / 1e3) as f32,
                 hp: if s.highlights { HIGHLIGHTS } else { 1.0 } }
    }

    /// The shader's clamps (its PARAMs' MINIMUM and MAXIMUM)
    pub fn clamped(self) -> Params {
        Params { dr: self.dr.clamp(-3.0, 3.0), db: self.db.clamp(-3.0, 3.0), k: self.k.clamp(0.0, 1.0), ev: self.ev.clamp(-2.0, 3.0),
                 s: self.s.clamp(0.0, 2.0), hp: self.hp.clamp(1.0, 4.0) }
    }

    /// For gpu_lut.rs's uniform: dr, db, k, ev, s, hp
    pub fn array(&self) -> [f32; 6] { [self.dr, self.db, self.k, self.ev, self.s, self.hp] }

    /// One pixel as the shader makes it: the decoded RGB signal (BT.1886, Rec.709) in, HLG Rec.2020 out
    pub fn to_hlg(&self, c: [f32; 3]) -> [f32; 3] {
        const LUM: [f32; 3] = [0.2627, 0.6780, 0.0593];
        const SDR_WHITE: f32 = 0.26496256;
        const EPS: f32 = 4e-5;
        let dot = |a: [f32; 3]| a[0] * LUM[0] + a[1] * LUM[1] + a[2] * LUM[2];
        let compose = |y: f32, rg: f32, bg: f32| {
            let (er, eb) = (rg.exp(), bg.exp());
            let g = y / (LUM[0] * er + LUM[1] + LUM[2] * eb);
            [g * er, g, g * eb]
        };
        let lin = c.map(|v| v.max(0.0).powf(2.4));
        let cam = mul3(&M709_2020, lin);
        let y = dot(cam);
        let l = cam.map(|v| v.max(EPS).ln());
        let (rg, bg) = (l[0] - l[1], l[2] - l[1]);
        let mut o = compose(y, rg + self.dr, bg + self.db);
        if self.k > 0.0 {
            let mut p = mul3(&M2020_P3, o);
            p[0] -= self.k * p[1];
            o = mul3(&MP3_2020, p);
            let k = y / dot(o).max(1e-6);
            o = o.map(|v| v * k);
        }
        if self.s != 1.0 {
            let lo = o.map(|v| v.max(EPS).ln());
            o = compose(y + self.s * (dot(o) - y), rg + self.s * (lo[0] - lo[1] - rg), bg + self.s * (lo[2] - lo[1] - bg));
        }
        let yo = dot(o);
        if self.hp != 1.0 && yo > 0.5 {
            let q = (yo - 0.5) / 0.5;
            let k = (yo + 0.5 * (self.hp - 1.0) * if q <= 1.0 { q * q } else { 2.0 * q - 1.0 }) / yo;
            o = o.map(|v| v * k);
        }
        let k = self.ev.exp2() * SDR_WHITE;
        o.map(|v| hlg((v * k).clamp(0.0, 1.0)))
    }

    /// A .cube of the shader with these numbers, `n` points a side (red fastest): for the paths that only take a LUT
    /// (ffmpeg's lut3d, the 360 export, a flipbook). Its input is the signal 0..1, clamped as a LUT's is
    pub fn cube(&self, n: usize, title: &str) -> String {
        let mut text = format!("TITLE \"{title}\"\nLUT_3D_SIZE {n}\n");
        let step = 1.0 / (n - 1) as f32;
        for b in 0..n {
            for g in 0..n {
                for r in 0..n {
                    let o = self.to_hlg([r as f32 * step, g as f32 * step, b as f32 * step]);
                    text += &format!("{:.6} {:.6} {:.6}\n", o[0], o[1], o[2]);
                }
            }
        }
        text
    }
}

fn hlg(e: f32) -> f32 {
    if e <= 1.0 / 12.0 { (3.0 * e).sqrt() } else { 0.17883277 * (12.0 * e - 0.28466892).ln() + 0.55991073 }
}

// Row-major, as uw_estimate.lua and the shader list them
const M709_2020: [f32; 9] = [0.6274, 0.3293, 0.0433, 0.0691, 0.9195, 0.0114, 0.0164, 0.0880, 0.8956];
const M2020_P3: [f32; 9] = [1.343578253, -0.2821796705, -0.06139858206, -0.06529745279, 1.075787916, -0.01049046306, 0.002821787262, -0.01959849452, 1.016776707];
const MP3_2020: [f32; 9] = [0.7538330344, 0.1985973691, 0.04756959659, 0.04574384897, 0.9417772198, 0.01247893122, -0.001210340355, 0.0176017173, 0.9836086231];

fn mul3(m: &[f32; 9], v: [f32; 3]) -> [f32; 3] {
    [m[0] * v[0] + m[1] * v[1] + m[2] * v[2], m[3] * v[0] + m[4] * v[1] + m[5] * v[2], m[6] * v[0] + m[7] * v[1] + m[8] * v[2]]
}

// --- The rule (uw_estimate.lua), in f64 as LuaJIT computes it ---

pub const SDR_WHITE: f64 = 0.26496255978640015;
const LR: f64 = 0.2627;
const LG: f64 = 0.6780;
const LB: f64 = 0.0593;
const E709_2020: [f64; 9] = [0.6274, 0.3293, 0.0433, 0.0691, 0.9195, 0.0114, 0.0164, 0.0880, 0.8956];
const E2020_P3: [f64; 9] = [1.343578253, -0.2821796705, -0.06139858206, -0.06529745279, 1.075787916, -0.01049046306, 0.002821787262, -0.01959849452, 1.016776707];
const TO709: [f64; 9] = [1.660511208, -0.5877105887, -0.07280061924, -0.124561406, 1.132960514, -0.008399107835, -0.01816768656, -0.100560598, 1.118728285];
const P3I: [f64; 9] = [0.7538330344, 0.1985973691, 0.04756959659, 0.04574384897, 0.9417772198, 0.01247893122, -0.001210340355, 0.0176017173, 0.9836086231];
const OK1: [f64; 9] = [0.4122214708, 0.5363325363, 0.0514459929, 0.2119034982, 0.6806995451, 0.1073969566, 0.0883024619, 0.2817188376, 0.6299787005];
const OK2: [f64; 9] = [0.2104542553, 0.7936177850, -0.0040720468, 1.9779984951, -2.4285922050, 0.4505937099, 0.0259040371, 0.7827717662, -0.8086757660];

const TARGET_RG: f64 = -0.45;
const A_MAX: f64 = -0.052;
const A_FRAME: f64 = -0.030;
const K_CAP: f64 = 0.5;
const WATER_HUE_MIN: f64 = 228.0;
const WATER_MIN: f64 = 0.05;
const Y_TARGET: f64 = 0.0629;
const MAX_SAMPLES: usize = 4000;

/// The grid uwcolor.lua measures: 320x180 cells, from a 960x540 frame
pub const GRID: (usize, usize) = (320, 180);

fn mul(m: &[f64; 9], r: f64, g: f64, b: f64) -> (f64, f64, f64) {
    (m[0] * r + m[1] * g + m[2] * b, m[3] * r + m[4] * g + m[5] * b, m[6] * r + m[7] * g + m[8] * b)
}

fn cbrt(x: f64) -> f64 { if x < 0.0 { -(-x).powf(1.0 / 3.0) } else { x.powf(1.0 / 3.0) } }

fn oklab(r: f64, g: f64, b: f64) -> (f64, f64, f64) {
    let (l, m, s) = mul(&OK1, r, g, b);
    mul(&OK2, cbrt(l), cbrt(m), cbrt(s))
}

fn sorted(t: &[f64]) -> Vec<f64> {
    let mut c = t.to_vec();
    c.sort_by(|a, b| a.total_cmp(b));
    c
}

fn median(t: &[f64]) -> f64 {
    let n = t.len();
    if n == 0 { return 0.0; }
    let c = sorted(t);
    if n % 2 == 1 { c[(n + 1) / 2 - 1] } else { (c[n / 2 - 1] + c[n / 2]) / 2.0 }
}

fn quantile(t: &[f64], q: f64) -> f64 {
    if t.is_empty() { return 0.0; }
    let c = sorted(t);
    let pos = 1.0 + (c.len() - 1) as f64 * q;
    let i = pos.floor() as usize;
    let fr = pos - i as f64;
    let at = |j: usize| c[j - 1];
    at(i) + (if i < c.len() { at(i + 1) } else { at(i) } - at(i)) * fr
}

/// The grid (R, G, B: linear Rec.2020, SDR white at SDR_WHITE) from a bgr0 frame w x h, as uw_estimate.lua's
/// grid_from_bgr0: each cell the mean of 3x3 samples in linear light (BT.1886, 2.4)
pub fn grid_from_bgr0(data: &[u8], w: usize, h: usize, stride: usize, gw: usize, gh: usize) -> [Vec<f64>; 3] {
    let lut: Vec<f64> = (0..256).map(|q| (q as f64 / 255.0).powf(2.4)).collect();
    let (mut rr, mut gg, mut bb) = (Vec::with_capacity(gw * gh), Vec::with_capacity(gw * gh), Vec::with_capacity(gw * gh));
    for gy in 0..gh {
        for gx in 0..gw {
            let (mut sr, mut sg, mut sb) = (0.0, 0.0, 0.0);
            for sy in 0..3 {
                let y = ((((gy * 3 + sy) as f64) + 0.5) * h as f64 / (gh * 3) as f64).floor() as usize;
                for sx in 0..3 {
                    let x = ((((gx * 3 + sx) as f64) + 0.5) * w as f64 / (gw * 3) as f64).floor() as usize;
                    let o = y * stride + x * 4;
                    sb += lut[data[o] as usize];
                    sg += lut[data[o + 1] as usize];
                    sr += lut[data[o + 2] as usize];
                }
            }
            let (r, g, b) = mul(&E709_2020, sr / 9.0, sg / 9.0, sb / 9.0);
            rr.push(r * SDR_WHITE);
            gg.push(g * SDR_WHITE);
            bb.push(b * SDR_WHITE);
        }
    }
    [rr, gg, bb]
}

fn blur5(a: &[f64], gw: usize, gh: usize) -> Vec<f64> {
    let mut o = vec![0.0; gw * gh];
    for y in 0..gh as i64 {
        for x in 0..gw as i64 {
            let (mut s, mut n) = (0.0, 0.0);
            for dy in -2..=2 {
                let yy = y + dy;
                if yy < 0 || yy >= gh as i64 { continue; }
                for dx in -2..=2 {
                    let xx = x + dx;
                    if xx >= 0 && xx < gw as i64 { s += a[(yy * gw as i64 + xx) as usize]; n += 1.0; }
                }
            }
            o[(y * gw as i64 + x) as usize] = s / n;
        }
    }
    o
}

fn texture(y: &[f64], gw: usize, gh: usize) -> Vec<f64> {
    let ly: Vec<f64> = y.iter().map(|v| v.max(1e-4).ln()).collect();
    let sq: Vec<f64> = ly.iter().map(|v| v * v).collect();
    let (m, m2) = (blur5(&ly, gw, gh), blur5(&sq, gw, gh));
    let t: Vec<f64> = (0..gw * gh).map(|i| (((m2[i] - m[i] * m[i]).max(0.0).sqrt() - 0.03) / 0.07).clamp(0.0, 1.0)).collect();
    blur5(&t, gw, gh)
}

/// A frame's numbers from its grid (uw_estimate.lua's estimate)
pub fn estimate(rr: &[f64], gg: &[f64], bb: &[f64], gw: usize, gh: usize) -> Estimate {
    let n = gw * gh;
    let mut y = vec![0.0; n];
    let (mut rg, mut bg) = (vec![0.0; n], vec![0.0; n]);
    let (mut sr, mut sg, mut sb, mut cnt) = (0.0, 0.0, 0.0, 0usize);
    let mut rgs = Vec::new();
    let white = 0.9 * SDR_WHITE;
    for i in 0..n {
        let (r, g, b) = (rr[i], gg[i], bb[i]);
        let yy = LR * r + LG * g + LB * b;
        y[i] = yy;
        let (lr, lg, lb) = (r.max(1e-5).ln(), g.max(1e-5).ln(), b.max(1e-5).ln());
        rg[i] = lr - lg;
        bg[i] = lb - lg;
        if yy > 0.002 {
            rgs.push(rg[i]);
            if r.max(g).max(b) < white { sr += r; sg += g; sb += b; cnt += 1; }
        }
    }
    if cnt == 0 { return Estimate::default(); }
    let (c0, c1) = ((sr / sg).ln(), (sb / sg).ln());
    let p95 = if rgs.len() > 500 { quantile(&rgs, 0.95) } else { 0.0 };
    let f = ((0.7 - p95) / 0.4).clamp(0.0, 1.0);
    let t = texture(&y, gw, gh);
    let (mut wi, mut fi) = (Vec::new(), Vec::new());
    for i in 0..n {
        if y[i] > 0.01 {
            fi.push(i);
            if t[i] < 0.15 { wi.push(i); }
        }
    }
    let share = wi.len() as f64 / n as f64;
    let thin = |list: Vec<usize>| -> Vec<usize> {
        if list.len() <= MAX_SAMPLES { return list; }
        let step = list.len() as f64 / MAX_SAMPLES as f64;
        (0..MAX_SAMPLES).map(|j| list[(j as f64 * step).floor() as usize]).collect()
    };
    let (wi, fi) = (thin(wi), thin(fi));
    let dr = (TARGET_RG - c0) * f;
    let db = (0.39 - 0.32 * c1) * f;
    let compose = |i: usize, extra: f64| {
        let (r2, b2) = (rg[i] + dr, bg[i] + db + extra);
        let (er, eb) = (r2.exp(), b2.exp());
        let g = y[i] / (LR * er + LG + LB * eb);
        (g * er, g, g * eb)
    };
    let water = f > 0.0 && share >= WATER_MIN;
    let water_hue = |extra: f64| {
        let (mut cr, mut cg, mut cb) = (Vec::with_capacity(wi.len()), Vec::with_capacity(wi.len()), Vec::with_capacity(wi.len()));
        for &i in &wi {
            let (a, b, c) = compose(i, extra);
            let (r, g, b) = mul(&TO709, a, b, c);
            cr.push(r); cg.push(g); cb.push(b);
        }
        let (_, a, bb) = oklab(median(&cr), median(&cg), median(&cb));
        (bb.atan2(a).to_degrees() + 360.0) % 360.0
    };
    let mut dbf = 0.0;
    if water {
        let hue0 = water_hue(0.0);
        if hue0 > 150.0 && hue0 < WATER_HUE_MIN {
            let (mut lo, mut hi) = (0.0, 1.0);
            for _ in 0..20 {
                let mid = (lo + hi) / 2.0;
                if water_hue(mid * f) < WATER_HUE_MIN { lo = mid } else { hi = mid }
            }
            dbf = hi * f;
        }
    }
    struct P3 { r: Vec<f64>, g: Vec<f64>, mg: f64, mb: f64 }
    let p3_of = |list: &[usize]| {
        let (mut r, mut g, mut b) = (Vec::with_capacity(list.len()), Vec::with_capacity(list.len()), Vec::with_capacity(list.len()));
        for &i in list {
            let (x, yy, z) = compose(i, dbf);
            let (pr, pg, pb) = mul(&E2020_P3, x, yy, z);
            r.push(pr); g.push(pg); b.push(pb);
        }
        let (mg, mb) = (median(&g), median(&b));
        P3 { r, g, mg, mb }
    };
    let a_of = |p: &P3, k: f64| {
        let qr: Vec<f64> = p.r.iter().zip(&p.g).map(|(r, g)| r - k * g).collect();
        let (r, g, b) = mul(&P3I, median(&qr), p.mg, p.mb);
        let (r, g, b) = mul(&TO709, r, g, b);
        oklab(r, g, b).1
    };
    let k_for = |p: &P3, amax: f64| {
        if p.r.is_empty() || a_of(p, 0.0) <= amax { return 0.0; }
        let (mut lo, mut hi) = (0.0, 1.0);
        for _ in 0..25 {
            let mid = (lo + hi) / 2.0;
            if a_of(p, mid) > amax { lo = mid } else { hi = mid }
        }
        hi
    };
    let mut k = 0.0;
    if f > 0.0 {
        if water { k = k_for(&p3_of(&wi), A_MAX); }
        k = k.max(k_for(&p3_of(&fi), A_FRAME));
        k = k.min(K_CAP) * f;
    }
    let med_y = median(&y);
    let ev = ((Y_TARGET / med_y.max(1e-4)).ln() / 2f64.ln()).clamp(-1.0, 2.0) * f;
    Estimate { dr, db: db + dbf, k, ev, f }
}

/// A frame's numbers from the 960x540 bgr0 frame ffmpeg gives (uwcolor.lua's measure)
pub fn estimate_bgr0(data: &[u8]) -> Result<Estimate, String> {
    let (gw, gh) = GRID;
    let (w, h) = (gw * 3, gh * 3);
    if data.len() != w * h * 4 { return Err(format!("a {w}x{h} bgr0 frame is {} bytes, not {}", w * h * 4, data.len())); }
    let [r, g, b] = grid_from_bgr0(data, w, h, w * 4, gw, gh);
    Ok(estimate(&r, &g, &b, gw, gh))
}

// --- Playback's timing (uwcolor.lua) ---

/// Half a second ahead while it plays
pub const LEAD: f64 = 0.5;
/// Between the starts of two measurements while it plays: uwcolor.lua's MIN_GAP (0.35 s) checked by a 0.1 s timer
pub const MIN_GAP: f64 = 0.4;
/// How long a measurement takes to come back (the player's ffmpeg grab, ~290 ms on VideoToolbox)
pub const LATENCY: f64 = 0.3;
const TAU_SLOW: f64 = 1.2;
const TAU_FAST: f64 = 0.35;
const FAR: [f64; 4] = [0.12, 0.12, 0.12, 0.25];
const TICK: f64 = 0.05;

/// The times an export from `from` to `to` (seconds in the clip) measures, as playback started at `from` does: the
/// first (taken at once) the frame at `from + LEAD`, then one every MIN_GAP, each LEAD ahead of when it started
pub fn measure_times(from: f64, to: f64) -> Vec<f64> {
    let mut v = Vec::new();
    let mut s = from;
    while s <= to + 1e-9 { v.push(s + LEAD); s += MIN_GAP; }
    if v.is_empty() { v.push(from + LEAD); }
    v
}

/// The numbers each frame of an export gets, as playback eases them
pub struct Schedule { from: f64, measured: Vec<Estimate> }

impl Schedule {
    /// `measured[j]`: the frame at measure_times(from, to)[j]
    pub fn new(from: f64, measured: Vec<Estimate>) -> Schedule { Schedule { from, measured } }

    /// One set of numbers throughout (a photo, a still)
    pub fn fixed(e: Estimate) -> Schedule { Schedule { from: 0.0, measured: vec![e] } }

    /// The numbers at each of `times` (ascending, seconds in the clip): the first measurement taken at once, then each
    /// one the target from when it comes back (LATENCY after it started), eased towards at 20 Hz as uwcolor.lua does
    pub fn at(&self, times: &[f64]) -> Vec<Estimate> {
        let key = |e: &Estimate| [e.dr, e.db, e.k, e.ev];
        let Some(first) = self.measured.first() else { return vec![Estimate::default(); times.len()] };
        let mut cur = key(first);
        let mut f = first.f;
        let mut target = cur;
        let mut next = 1;
        let mut clock = self.from;
        let mut out = Vec::with_capacity(times.len());
        for &t in times {
            while clock + TICK <= t + 1e-9 {
                clock += TICK;
                // A measurement back by now: the new target
                while next < self.measured.len() && self.from + next as f64 * MIN_GAP + LATENCY <= clock + 1e-9 {
                    target = key(&self.measured[next]);
                    f = self.measured[next].f;
                    next += 1;
                }
                let far = (0..4).any(|i| (target[i] - cur[i]).abs() > FAR[i]);
                let a = 1.0 - (-TICK / if far { TAU_FAST } else { TAU_SLOW }).exp();
                for i in 0..4 {
                    let d = target[i] - cur[i];
                    if d.abs() > 0.002 { cur[i] += d * a; }
                }
            }
            out.push(Estimate { dr: cur[0], db: cur[1], k: cur[2], ev: cur[3], f });
        }
        out
    }

    /// One set for a whole export where only one can be (a LUT for ffmpeg): the median of what was measured
    pub fn median(&self) -> Estimate {
        let m = |g: fn(&Estimate) -> f64| median(&self.measured.iter().map(g).collect::<Vec<_>>());
        Estimate { dr: m(|e| e.dr), db: m(|e| e.db), k: m(|e| e.k), ev: m(|e| e.ev), f: m(|e| e.f) }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // The shader's numbers at rest: the camera's colour, SDR white at HLG 75 %
    #[test]
    fn uw_white_at_hlg_75() {
        let p = Params { dr: 0.0, db: 0.0, k: 0.0, ev: 0.0, s: 1.0, hp: 1.0 };
        let o = p.to_hlg([1.0, 1.0, 1.0]);
        for c in o { assert!((c - 0.75).abs() < 0.002, "{o:?}"); }
        let o = p.to_hlg([0.0, 0.0, 0.0]);
        for c in o { assert!(c.abs() < 0.02, "{o:?}"); }
    }

    // A LUT of the shader is close to the shader itself (the paths that only take a LUT)
    #[test]
    fn uw_cube_close_to_shader() {
        let p = Params { dr: 0.35, db: 0.6, k: 0.2, ev: 0.8, s: 0.8, hp: 2.5 };
        let n = 65;
        let text = p.cube(n, "test");
        let data: Vec<f32> = text.lines().skip(2).flat_map(|l| l.split_whitespace().map(|v| v.parse::<f32>().unwrap()).collect::<Vec<_>>()).collect();
        let mut worst = 0f32;
        for i in 0..2000 {
            let x = |k: u32| ((i * 7919 + k * 104729) % 1000) as f32 / 999.0;
            let c = [x(1), x(2), x(3)];
            let a = p.to_hlg(c);
            let b = crate::gpu_lut::tetrahedral(n, &data, c);
            for j in 0..3 { worst = worst.max((a[j] - b[j]).abs()); }
        }
        assert!(worst < 0.02, "worst {worst}");
    }

    // The rule against the Lua one, on the grids tests/uwcolor_test.lua writes (tests/uwcolor_cases.json)
    #[test]
    fn uw_estimate_as_lua() {
        let text = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../tests/uwcolor_cases.json")).unwrap();
        let cases: serde_json::Value = serde_json::from_str(&text).unwrap();
        let mut n = 0;
        for c in cases.as_array().unwrap() {
            let (gw, gh) = (c["w"].as_u64().unwrap() as usize, c["h"].as_u64().unwrap() as usize);
            let arr = |k: &str| c[k].as_array().unwrap().iter().map(|v| v.as_f64().unwrap()).collect::<Vec<f64>>();
            let e = estimate(&arr("R"), &arr("G"), &arr("B"), gw, gh);
            let want = &c["p"];
            for (k, v) in [("dr", e.dr), ("db", e.db), ("k", e.k), ("ev", e.ev), ("f", e.f)] {
                let w = want[k].as_f64().unwrap();
                assert!((v - w).abs() < 1e-6, "{}: {k} {v} vs Lua's {w}", c["name"]);
            }
            n += 1;
        }
        assert!(n >= 5);
    }

    // Playback's easing: taken at once at the start, then glides to a new scene's numbers within a second or two
    #[test]
    fn uw_schedule_eases() {
        let a = Estimate { dr: 0.2, db: 0.3, k: 0.1, ev: 0.5, f: 1.0 };
        let b = Estimate { dr: 0.6, db: 0.8, k: 0.3, ev: 1.0, f: 1.0 };
        let times = measure_times(10.0, 16.0);
        // The scene changes at 12 s: measurements of frames from 12 s on see b
        let measured: Vec<Estimate> = times.iter().map(|&t| if t < 12.0 { a } else { b }).collect();
        let s = Schedule::new(10.0, measured);
        let frames: Vec<f64> = (0..=180).map(|i| 10.0 + i as f64 / 30.0).collect();
        let p = s.at(&frames);
        assert_eq!(p[0].dr, a.dr);
        assert!((p[30].dr - a.dr).abs() < 1e-9, "unchanged through the first scene");
        // 12 s's frame is measured from 11.6 s on, back 0.3 s later: moving by 12 s, quickly while far (within 0.12
        // by ~12.3 s), then slowly: within 0.05 by 13.5 s, 0.01 by 16 s
        assert!(p[60].dr > a.dr, "{}", p[60].dr);
        assert!((p[105].dr - b.dr).abs() < 0.05, "{}", p[105].dr);
        assert!((p[180].dr - b.dr).abs() < 0.01, "{}", p[180].dr);
    }
}
