// SPDX-License-Identifier: GPL-3.0-or-later
//
// The hand's sideways jerks taken out, for clips shot up close (the GPU path's translation compensation). Gyroflow
// turns the picture back by the camera's rotation; up close, the hand's translation still shakes it. It's computed for
// a clip once, on demand (gyroflow-export --translation, the player's Ctrl+j), and kept; the player's shader then moves
// each frame by its shift (warp.glsl's trans_shift).
//
// The camera's accelerometer gives the jerks' shape: per frame, from its metadata (Osmo Action 6: FrameMeta's camera
// part 3.2, the accelerometer in g at 3.2.10.2-4, the attitude quaternion w, x, y, z at 3.2.9.1-4; one sample a frame)
//   - the position's jerks in the camera's axes (mm): the acceleration less gravity, into the world by the attitude,
//     integrated twice and high-passed without delay (integrated twice it drifts below ~1 Hz, where the motion is the
//     operator's anyway), back into the camera's axes;
//   - the attitude's jerks (mrad): the stabilization leaves a little of each turn (the lens's correction under water
//     is right only part of the time), the same shift all over the frame.
// The picture gives their size: how far a millimetre moves it depends on how far the subject is. The clip is rendered
// stabilized at 960 px, each frame's motion measured (blocks: a pyramid search, then Lucas-Kanade; the median of the
// more detailed half, the scene as a whole), and the picture's jerks (high-passed the same way) fitted to the basis
// over the 2 s around each frame, the frame itself and 5 on each side left out (fitted with them, the fit follows the
// noise), then scaled by the share of it that leaves the least over the clip (a clip whose motion can't be measured
// gets little or none). The shift: the basis times that fit, over the output camera's focal length, so it holds in
// any view and zoom.
//
// Measured on Anton's close-up dive clips through the player's shader (dense optical flow of the output, jitter above
// 1.5 Hz at 960 px wide): a stingray on sand 2.5 -> 0.9 px, the reef 1.7 -> 1.0, a scorpionfish 6.6 -> 3.7; a
// snorkelling clip with jolts past the search 20 -> 19 (a third of the fit used), 10.6 -> 8.8. Fitted live instead (the
// denoise's block motion, 5 frames ahead; an earlier try) it took out 12-46 %: the operator's own moves swamp a
// causal high-pass, so the fit has to see the whole clip.

use std::sync::{ Arc, atomic::{ AtomicBool, Ordering } };
use nalgebra::{ Matrix3, UnitQuaternion, Quaternion, Vector3 };
use gyroflow_core::telemetry_parser;

use crate::{ prefetch::Prefetched, probe::{ dji_schema, pb_bytes } };
use nalgebra::{ SMatrix, SVector };

/// The high-pass's corner (Hz): below it, the accelerometer drifts and the motion is the operator's. 1.5 left the
/// 1-2 Hz jerks, where most are, half taken out (forward and back, the filter halves 1.5 Hz); 0.7 took out no more
pub const CUTOFF_HZ: f64 = 1.0;
/// The corner that keeps the integration from drifting, well below CUTOFF_HZ
const DRIFT_HZ: f64 = 0.25;
/// Values a frame in the basis: position x, y, z (mm), attitude x, y, z (mrad)
pub const CHANNELS: usize = 6;

/// The models with an accelerometer and attitude a frame: their paths in FrameMeta's camera part (3.2)
const MODELS: &[(&str, u64, u64)] = &[
    // Osmo Action 6: Imu { accel x, y, z at 2-4 } at 10, the attitude quaternion { w, x, y, z at 1-4 } at 9
    ("dvtm_ac206.proto", 10, 9),
];

struct Sample { ms: f64, accel: [f64; 3], quat: [f64; 4] }

fn f32_at(msg: &[u8], field: u64) -> Option<f64> {
    pb_bytes(msg, field).filter(|b| b.len() == 4).map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]]) as f64)
}

fn read(file: &mut Prefetched, size: usize) -> Option<Vec<Sample>> {
    file.rewind();
    let mut layout = None;
    let mut out = Vec::new();
    let cancel = Arc::new(AtomicBool::new(false));
    let cancel2 = cancel.clone();
    let _ = telemetry_parser::util::get_metadata_track_samples(file, size, false, |info, data: &[u8], _pos, _md| {
        if cancel2.load(Ordering::Relaxed) { return; }
        if layout.is_none() {
            layout = dji_schema(data).and_then(|p| MODELS.iter().find(|m| m.0 == p));
            if layout.is_none() { return cancel2.store(true, Ordering::Relaxed); }
        }
        let &(_, acc, att) = layout.unwrap();
        let Some(camera) = pb_bytes(data, 3).and_then(|f| pb_bytes(f, 2)) else { return };
        let (Some(a), Some(q)) = (pb_bytes(camera, acc), pb_bytes(camera, att)) else { return };
        let (Some(ax), Some(ay), Some(az)) = (f32_at(a, 2), f32_at(a, 3), f32_at(a, 4)) else { return };
        let (Some(qw), Some(qx), Some(qy), Some(qz)) = (f32_at(q, 1), f32_at(q, 2), f32_at(q, 3), f32_at(q, 4)) else { return };
        out.push(Sample { ms: info.timestamp_ms, accel: [ax, ay, az], quat: [qw, qx, qy, qz] });
    }, cancel);
    layout?;
    (out.len() > 30).then_some(out)
}

/// The basis for the clip's frames (frame i at i / fps s plus `frame_offset_us`), `CHANNELS` values each; None when
/// the camera records no accelerometer here
pub fn basis(file: &mut Prefetched, size: usize, fps: f64, frame_count: usize, frame_offset_us: i64) -> Option<Vec<[f32; CHANNELS]>> {
    let t = std::time::Instant::now();
    let samples = read(file, size)?;
    let b = basis_of(&samples, fps, frame_count, frame_offset_us);
    log::info!("translation: {} samples, basis in {:.0} ms", samples.len(), t.elapsed().as_secs_f64() * 1000.0);
    Some(b)
}

/// The same for a video without its motion read yet (a clip with a project): every sample of it
pub fn basis_of_video(video: &str, fps: f64, frame_count: usize, frame_offset_us: i64) -> Option<Vec<[f32; CHANNELS]>> {
    let (mut file, size) = Prefetched::open(video).ok()?;
    basis(&mut file, size, fps, frame_count, frame_offset_us)
}

fn basis_of(samples: &[Sample], fps: f64, frame_count: usize, frame_offset_us: i64) -> Vec<[f32; CHANNELS]> {
    let n = frame_count.max(1);
    // Each frame's sample: the nearest by time, held past the ends
    let mut sorted: Vec<&Sample> = samples.iter().collect();
    sorted.sort_by(|a, b| a.ms.total_cmp(&b.ms));
    let mut at = Vec::with_capacity(n);
    let mut k = 0;
    for i in 0..n {
        let ms = (i as f64 / fps * 1_000_000.0 + frame_offset_us as f64) / 1000.0;
        while k + 1 < sorted.len() && (sorted[k + 1].ms - ms).abs() <= (sorted[k].ms - ms).abs() { k += 1; }
        at.push(sorted[k]);
    }
    let accel: Vec<Vector3<f64>> = at.iter().map(|s| Vector3::from(s.accel)).collect();
    let rot: Vec<UnitQuaternion<f64>> = at.iter().map(|s| UnitQuaternion::from_quaternion(Quaternion::new(s.quat[0], s.quat[1], s.quat[2], s.quat[3]))).collect();
    let mats: Vec<Matrix3<f64>> = rot.iter().map(|q| *q.to_rotation_matrix().matrix()).collect();

    // Gravity in the camera's axes: a row or a column of the attitude's matrix (which way the quaternion turns and
    // which axis is up aren't written down), scaled and mixed by a least-squares 3x3 (the accelerometer's own scale);
    // the one that fits best. What's left is the acceleration, in g
    let mut best: Option<(f64, bool, usize, Matrix3<f64>)> = None;
    for row in [true, false] {
        for ax in 0..3 {
            let g: Vec<Vector3<f64>> = mats.iter().map(|m| if row { m.row(ax).transpose() } else { m.column(ax).into_owned() }).collect();
            let (mut gtg, mut gta) = (Matrix3::zeros(), Matrix3::zeros());
            for (gi, ai) in g.iter().zip(&accel) { gtg += gi * gi.transpose(); gta += gi * ai.transpose(); }
            let Some(inv) = gtg.try_inverse() else { continue };
            let m = inv * gta;
            let err: f64 = g.iter().zip(&accel).map(|(gi, ai)| (ai - m.transpose() * gi).norm_squared()).sum();
            if best.as_ref().is_none_or(|b| err < b.0) { best = Some((err, row, ax, m)); }
        }
    }
    let Some((err, row, ax, m)) = best else { return vec![[0.0; CHANNELS]; n] };
    log::debug!("translation: gravity fit {} {ax}, {:.3} g rms", if row { "row" } else { "column" }, (err / n as f64).sqrt());
    // Camera to world: the matrix whose rows gave gravity maps the camera's axes into the world's
    let to_world: Vec<Matrix3<f64>> = mats.iter().map(|q| if row { *q } else { q.transpose() }).collect();
    let world_accel: Vec<Vector3<f64>> = (0..n).map(|i| {
        let g = if row { mats[i].row(ax).transpose() } else { mats[i].column(ax).into_owned() };
        to_world[i] * ((accel[i] - m.transpose() * g) * 9.80665)
    }).collect();

    // Twice integrated, m: only the drift held back on the way (a corner a sixth of the cutoff's), then high-passed
    // (zero phase) once at the cutoff, so what's just above it keeps its size
    let dt = 1.0 / fps;
    let mut chan: Vec<Vec<f64>> = (0..3).map(|c| world_accel.iter().map(|a| a[c]).collect()).collect();
    for c in chan.iter_mut() {
        *c = highpass_zero_phase(c, fps, DRIFT_HZ);
        *c = highpass_zero_phase(&integrate(c, dt), fps, DRIFT_HZ);
        *c = highpass_zero_phase(&integrate(c, dt), fps, CUTOFF_HZ);
    }
    // The frame-to-frame turns in the camera's axes, added up, high-passed: rad
    let mut turn = vec![Vector3::zeros(); n];
    for i in 1..n { turn[i] = turn[i - 1] + (rot[i - 1].inverse() * rot[i]).scaled_axis(); }
    let turn: Vec<Vec<f64>> = (0..3).map(|c| highpass_zero_phase(&turn.iter().map(|v| v[c]).collect::<Vec<_>>(), fps, CUTOFF_HZ)).collect();

    (0..n).map(|i| {
        let p = to_world[i].transpose() * Vector3::new(chan[0][i], chan[1][i], chan[2][i]) * 1000.0;
        [p.x as f32, p.y as f32, p.z as f32, (turn[0][i] * 1000.0) as f32, (turn[1][i] * 1000.0) as f32, (turn[2][i] * 1000.0) as f32]
    }).collect()
}

fn integrate(x: &[f64], dt: f64) -> Vec<f64> {
    let mut s = 0.0;
    let mut out = Vec::with_capacity(x.len());
    for (i, v) in x.iter().enumerate() {
        if i > 0 { s += 0.5 * (v + x[i - 1]) * dt; }
        out.push(s);
    }
    out
}

/// A second-order Butterworth high-pass (bilinear): b0, b1, b2, a1, a2
pub fn butter_hp(fs: f64, fc: f64) -> [f64; 5] {
    let k = (std::f64::consts::PI * fc / fs).tan();
    let norm = 1.0 / (1.0 + std::f64::consts::SQRT_2 * k + k * k);
    [norm, -2.0 * norm, norm, 2.0 * (k * k - 1.0) * norm, (1.0 - std::f64::consts::SQRT_2 * k + k * k) * norm]
}

fn filter(c: &[f64; 5], x: &[f64]) -> Vec<f64> {
    let (mut x1, mut x2, mut y1, mut y2) = (x[0], x[0], 0.0, 0.0);
    x.iter().map(|&v| {
        let y = c[0] * v + c[1] * x1 + c[2] * x2 - c[3] * y1 - c[4] * y2;
        (x2, x1, y2, y1) = (x1, v, y1, y);
        y
    }).collect()
}

/// Forward and back (fourth order, no delay), the ends padded by odd reflection over 2 s
pub fn highpass_zero_phase(x: &[f64], fs: f64, fc: f64) -> Vec<f64> {
    let n = x.len();
    if n < 4 { return vec![0.0; n]; }
    let p = ((2.0 * fs) as usize).min(n - 1);
    let mut padded = Vec::with_capacity(n + 2 * p);
    padded.extend((1..=p).rev().map(|i| 2.0 * x[0] - x[i]));
    padded.extend_from_slice(x);
    padded.extend((1..=p).map(|i| 2.0 * x[n - 1] - x[n - 1 - i]));
    let c = butter_hp(fs, fc);
    let mut y = filter(&c, &padded);
    y.reverse();
    let mut y = filter(&c, &y);
    y.reverse();
    y[p..p + n].to_vec()
}


// ---------------------------------------------------------------------------------------------------------- the cache

/// Where a clip's shifts are kept: by the player's name for it (its Gyroflow project, else the video), as it is now
pub fn cache_path(key: &str) -> Option<std::path::PathBuf> {
    crate::file_cache("translation", key, "v1").map(|p| p.with_extension("bin"))
}

const MAGIC: &[u8; 8] = b"AFPTRAN1";

/// The shifts kept for the clip, a frame each: over the output camera's focal length, x and y
pub fn cached(key: &str) -> Option<Vec<[f32; 2]>> {
    let data = std::fs::read(cache_path(key)?).ok()?;
    if data.len() < 12 || &data[..8] != MAGIC { return None; }
    let n = u32::from_le_bytes(data[8..12].try_into().ok()?) as usize;
    if data.len() != 12 + n * 8 { return None; }
    Some((0..n).map(|i| {
        let o = 12 + i * 8;
        [f32::from_le_bytes(data[o..o + 4].try_into().unwrap()), f32::from_le_bytes(data[o + 4..o + 8].try_into().unwrap())]
    }).collect())
}

pub fn write_cache(key: &str, shifts: &[[f32; 2]]) -> Result<std::path::PathBuf, String> {
    let path = cache_path(key).ok_or_else(|| format!("{key}: no cache folder"))?;
    if let Some(dir) = path.parent() { std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?; }
    let mut data = MAGIC.to_vec();
    data.extend((shifts.len() as u32).to_le_bytes());
    for s in shifts { data.extend(s[0].to_le_bytes()); data.extend(s[1].to_le_bytes()); }
    let tmp = path.with_extension("tmp");
    std::fs::write(&tmp, data).and_then(|_| std::fs::rename(&tmp, &path)).map_err(|e| format!("{}: {e}", path.display()))?;
    Ok(path)
}

// ------------------------------------------------------------------------------------------------- the picture's motion

/// A frame's luma (0-1) at full, half and quarter size
pub struct Pyramid { levels: Vec<(usize, usize, Vec<f32>)> }

impl Pyramid {
    pub fn new(w: usize, h: usize, luma: Vec<f32>) -> Pyramid {
        let mut levels = vec![(w, h, luma)];
        for _ in 0..2 {
            let (pw, ph, p) = levels.last().unwrap();
            let (w, h) = (pw / 2, ph / 2);
            let mut d = vec![0f32; w * h];
            for y in 0..h {
                for x in 0..w {
                    let i = 2 * y * pw + 2 * x;
                    d[y * w + x] = 0.25 * (p[i] + p[i + 1] + p[i + pw] + p[i + pw + 1]);
                }
            }
            levels.push((w, h, d));
        }
        Pyramid { levels }
    }
    fn at(&self, l: usize, x: i32, y: i32) -> f32 {
        let (w, h, d) = &self.levels[l];
        d[(y.clamp(0, *h as i32 - 1) as usize) * w + x.clamp(0, *w as i32 - 1) as usize]
    }
    fn bilinear(&self, x: f32, y: f32) -> f32 {
        let (x0, y0) = (x.floor(), y.floor());
        let (fx, fy) = (x - x0, y - y0);
        let (x0, y0) = (x0 as i32, y0 as i32);
        let a = self.at(0, x0, y0) + fx * (self.at(0, x0 + 1, y0) - self.at(0, x0, y0));
        let b = self.at(0, x0, y0 + 1) + fx * (self.at(0, x0 + 1, y0 + 1) - self.at(0, x0, y0 + 1));
        a + fy * (b - a)
    }
}

const BLOCK: i32 = 32;

/// The blocks' motion from `prev` to `cur` (where each block's picture moved, px of level 0) and their detail; the
/// tenth of the frame along each edge left out (the stabilization's edges)
pub fn block_motions(prev: &Pyramid, cur: &Pyramid) -> Vec<([f32; 2], f32)> {
    let (w, h, _) = cur.levels[0];
    let (w, h) = (w as i32, h as i32);
    let (mx, my) = (w / 10, h / 10);
    let rows: Vec<i32> = (0..).map(|r| my + r * BLOCK).take_while(|by| by + BLOCK <= h - my).collect();
    // A row of blocks to each thread
    let threads = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4).clamp(1, 8);
    std::thread::scope(|scope| {
        let handles: Vec<_> = rows.chunks(rows.len().div_ceil(threads).max(1)).map(|part| scope.spawn(move || {
            part.iter().flat_map(|&by| block_row(prev, cur, by, mx, w)).collect::<Vec<_>>()
        })).collect();
        handles.into_iter().flat_map(|h| h.join().unwrap_or_default()).collect()
    })
}

fn block_row(prev: &Pyramid, cur: &Pyramid, by: i32, mx: i32, w: i32) -> Vec<([f32; 2], f32)> {
    let mut out = Vec::new();
    {
        let mut bx = mx;
        while bx + BLOCK <= w - mx {
            // Detail: the luma's steps over the block
            let mut detail = 0.0;
            for y in (by..by + BLOCK).step_by(2) {
                for x in (bx..bx + BLOCK).step_by(2) {
                    let c = cur.at(0, x, y);
                    detail += (cur.at(0, x + 1, y) - c).abs() + (cur.at(0, x, y + 1) - c).abs();
                }
            }
            // Where the block is in the frame before: v, cur(p) = prev(p + v). A quarter size search 6 px each way
            // (24 at full size), then half and full size within one, then Lucas-Kanade between pixels
            let sad = |l: usize, x0: i32, y0: i32, n: i32, d: (i32, i32)| -> f32 {
                let mut s = 0.0;
                for y in 0..n { for x in 0..n { s += (cur.at(l, x0 + x, y0 + y) - prev.at(l, x0 + x + d.0, y0 + y + d.1)).abs(); } }
                s
            };
            let search = |l: usize, c: (i32, i32), r: i32| -> (i32, i32) {
                let (s, n) = (1 << l, BLOCK >> l);
                let mut best = (f32::MAX, c);
                for dy in -r..=r {
                    for dx in -r..=r {
                        let d = (c.0 + dx, c.1 + dy);
                        let v = sad(l, bx / s, by / s, n, d);
                        if v < best.0 { best = (v, d); }
                    }
                }
                best.1
            };
            let v2 = search(2, (0, 0), 6);
            let v1 = search(1, (v2.0 * 2, v2.1 * 2), 1);
            let v0 = search(0, (v1.0 * 2, v1.1 * 2), 1);
            let mut v = [v0.0 as f32, v0.1 as f32];
            for _ in 0..2 {
                let (mut g00, mut g01, mut g11, mut r0, mut r1) = (0f32, 0f32, 0f32, 0f32, 0f32);
                for y in (by..by + BLOCK).step_by(2) {
                    for x in (bx..bx + BLOCK).step_by(2) {
                        let gx = 0.5 * (cur.at(0, x + 1, y) - cur.at(0, x - 1, y));
                        let gy = 0.5 * (cur.at(0, x, y + 1) - cur.at(0, x, y - 1));
                        let e = prev.bilinear(x as f32 + v[0], y as f32 + v[1]) - cur.at(0, x, y);
                        g00 += gx * gx; g01 += gx * gy; g11 += gy * gy;
                        r0 += gx * e; r1 += gy * e;
                    }
                }
                let det = g00 * g11 - g01 * g01;
                if det < 1e-9 { break; }
                v[0] += (-(g11 * r0 - g01 * r1) / det).clamp(-1.5, 1.5);
                v[1] += (-(g00 * r1 - g01 * r0) / det).clamp(-1.5, 1.5);
            }
            // The picture moved from p + v to p
            out.push(([-v[0], -v[1]], detail));
            bx += BLOCK;
        }
    }
    out
}

/// The frame's motion: the median of the more detailed half of the blocks, each way (flat water follows the noise;
/// fish and particles moving on their own are outvoted). The scene as a whole, near and far: a robust mean that
/// leaned to one cluster of blocks stabilized the far part of a stingray clip and left the sand in front jerking
pub fn frame_motion(blocks: &[([f32; 2], f32)]) -> [f64; 2] {
    if blocks.is_empty() { return [0.0; 2]; }
    let mut details: Vec<f32> = blocks.iter().map(|b| b.1).collect();
    details.sort_by(f32::total_cmp);
    let floor = details[details.len() / 2];
    let median = |c: usize| {
        let mut v: Vec<f32> = blocks.iter().filter(|b| b.1 >= floor).map(|b| b.0[c]).collect();
        v.sort_by(f32::total_cmp);
        v[v.len() / 2] as f64
    };
    [median(0), median(1)]
}

// ------------------------------------------------------------------------------------------------------------ the fit

/// The frames on each side the fit takes, and those next to the frame it leaves out
const FIT_HALF: usize = 120;
const FIT_SKIP: usize = 5;

pub struct Fit {
    /// Each frame's shift (px of the motion's frame): where the picture's jerks put it
    pub shifts: Vec<[f64; 2]>,
    /// The picture's jerks above the cutoff, RMS px, as measured and with the shifts taken out
    pub before: f64,
    pub after: f64,
    /// The share of the fit used (0-1)
    pub share: f64,
}

/// `motion`: each frame's motion from the one before (px), `basis` as `basis` makes it, frame for frame
pub fn fit(motion: &[[f64; 2]], basis: &[[f32; CHANNELS]], fps: f64) -> Fit {
    let n = motion.len().min(basis.len());
    if n < 2 * FIT_SKIP + 30 { return Fit { shifts: vec![[0.0; 2]; motion.len()], before: 0.0, after: 0.0, share: 0.0 }; }
    // Where the picture is, added up, high-passed as the basis is
    let place: Vec<Vec<f64>> = (0..2).map(|c| {
        let mut s = 0.0;
        let p: Vec<f64> = motion[..n].iter().map(|m| { s += m[c]; s }).collect();
        highpass_zero_phase(&p, fps, CUTOFF_HZ)
    }).collect();
    type M6 = SMatrix<f64, CHANNELS, CHANNELS>;
    type B6 = SMatrix<f64, CHANNELS, 2>;
    let x = |i: usize| SVector::<f64, CHANNELS>::from_iterator(basis[i].iter().map(|v| *v as f64));
    // Running sums of x x' and x g', so each window is two lookups
    let mut sm = vec![M6::zeros(); n + 1];
    let mut sb = vec![B6::zeros(); n + 1];
    for i in 0..n {
        let xi = x(i);
        sm[i + 1] = sm[i] + xi * xi.transpose();
        sb[i + 1] = sb[i] + xi * nalgebra::RowVector2::new(place[0][i], place[1][i]);
    }
    let range = |a: usize, b: usize| (sm[b] - sm[a], sb[b] - sb[a]);
    let mut shifts = vec![[0.0; 2]; motion.len()];
    let (mut before, mut after) = (0.0, 0.0);
    for i in 0..n {
        let (lo, hi) = (i.saturating_sub(FIT_HALF), (i + FIT_HALF + 1).min(n));
        let (skip_lo, skip_hi) = (i.saturating_sub(FIT_SKIP), (i + FIT_SKIP + 1).min(n));
        let (m1, b1) = range(lo, skip_lo);
        let (m2, b2) = range(skip_hi, hi);
        let (m, b) = (m1 + m2, b1 + b2);
        let ridge = 1e-6 * m.trace() / CHANNELS as f64 + 1e-9;
        if let Some(a) = (m + M6::identity() * ridge).cholesky().map(|c| c.solve(&b)) {
            let s = x(i).transpose() * a;
            shifts[i] = [s[0], s[1]];
        }
    }
    // How much of it to use: the share that leaves the least (0-1). Where the picture's motion can't be measured (a
    // jolt past the search, the camera out of the water), the fit misleads, and a clip like that gets less or none
    let (mut gs, mut ss) = (0.0, 0.0);
    for i in 0..n { for c in 0..2 { gs += place[c][i] * shifts[i][c]; ss += shifts[i][c] * shifts[i][c]; } }
    let share = if ss > 0.0 { (gs / ss).clamp(0.0, 1.0) } else { 0.0 };
    for s in shifts.iter_mut() { s[0] *= share; s[1] *= share; }
    for i in 0..n {
        before += place[0][i].powi(2) + place[1][i].powi(2);
        after += (place[0][i] - shifts[i][0]).powi(2) + (place[1][i] - shifts[i][1]).powi(2);
    }
    Fit { shifts, before: (before / n as f64).sqrt(), after: (after / n as f64).sqrt(), share }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn highpass_keeps_shake_drops_drift() {
        let fs = 60.0;
        let x: Vec<f64> = (0..1200).map(|i| { let t = i as f64 / fs; 5.0 * t + (2.0 * std::f64::consts::PI * 4.0 * t).sin() }).collect();
        let y = highpass_zero_phase(&x, fs, CUTOFF_HZ);
        // The 4 Hz shake through, in phase; the ramp gone
        let mid = &y[300..900];
        let shake: Vec<f64> = (300..900).map(|i| (2.0 * std::f64::consts::PI * 4.0 * i as f64 / fs).sin()).collect();
        let err = mid.iter().zip(&shake).map(|(a, b)| (a - b).powi(2)).sum::<f64>() / 600.0;
        assert!(err.sqrt() < 0.05, "{}", err.sqrt());
    }

    #[test]
    fn still_camera_no_shift() {
        // Lying still, turned: gravity alone, nothing to integrate
        let q = UnitQuaternion::from_euler_angles(0.3, -0.2, 0.1);
        let g = q.to_rotation_matrix().matrix().row(2).transpose() * -1.0;
        let s: Vec<Sample> = (0..600).map(|i| Sample { ms: i as f64 * 1000.0 / 60.0, accel: [g.x, g.y, g.z], quat: [q.w, q.i, q.j, q.k] }).collect();
        let b = basis_of(&s, 60.0, 600, 0);
        assert!(b.iter().all(|v| v.iter().all(|x| x.abs() < 1e-3)));
    }

    #[test]
    fn block_motion_finds_a_shift() {
        // A textured picture moved by (2.3, -1.7) px
        let (w, h) = (320usize, 192usize);
        let pic = |x: f32, y: f32| ((x * 0.31).sin() * (y * 0.23).cos() + (x * 0.07 + y * 0.11).sin()) * 0.25 + 0.5;
        let a: Vec<f32> = (0..w * h).map(|i| pic((i % w) as f32, (i / w) as f32)).collect();
        let b: Vec<f32> = (0..w * h).map(|i| pic((i % w) as f32 - 2.3, (i / w) as f32 + 1.7)).collect();
        let m = frame_motion(&block_motions(&Pyramid::new(w, h, a), &Pyramid::new(w, h, b)));
        assert!((m[0] - 2.3).abs() < 0.1 && (m[1] + 1.7).abs() < 0.1, "{m:?}");
    }

    #[test]
    fn fit_finds_the_scale() {
        // The picture jerks 3 px per mm of x and -2 px per mm of z, plus slow drift; the fit takes the jerks out
        let n = 900;
        let basis: Vec<[f32; CHANNELS]> = (0..n).map(|i| {
            let t = i as f32 / 60.0;
            [(t * 13.0).sin() * 0.8, (t * 7.0).cos() * 0.5, (t * 17.0).sin() * 0.6, (t * 11.0).sin() * 2.0, 0.0, 0.0]
        }).collect();
        let place: Vec<[f64; 2]> = (0..n).map(|i| { let b = basis[i]; [3.0 * b[0] as f64 + 0.01 * i as f64, -2.0 * b[2] as f64] }).collect();
        let motion: Vec<[f64; 2]> = (0..n).map(|i| if i == 0 { [0.0; 2] } else { [place[i][0] - place[i - 1][0], place[i][1] - place[i - 1][1]] }).collect();
        let f = fit(&motion, &basis, 60.0);
        assert!(f.after < 0.1 * f.before, "{} -> {}", f.before, f.after);
    }
}
