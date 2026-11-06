// SPDX-License-Identifier: GPL-3.0-or-later
//
// The 360 view's orientation and headings (docs/360-cameras.md, "The heading modes"), for the stamp (stamp.rs): the
// camera in a world with gravity down, and per heading mode the yaw the view's "straight ahead" points along.
//
// - The world: video-types' axes (+x right, +y down, +z forward), +y along gravity. gyroflow-core integrates the gyro
//   alone (sphere360.rs asks it so) into a world of its own; the vertical comes from the accelerometer turned into that
//   world, averaged over GRAVITY_SIGMA_S each way, and is turned onto -y frame by frame (`World::new`). An AHRS that
//   trusts the accelerometer within a second or two (gyroflow-core's VQF) is fooled by a motorcycle: in a curve the
//   accelerometer leans with the bike (on the sample X5 clip the camera rolled 50° in one while the accelerometer barely
//   moved), and braking reads as a pitch. Averaged over many seconds both ways these mostly cancel; the gyro's drift over
//   that time is what's left. On the sample clip, against the horizon measured in the sphere every second: 10° rms, 18°
//   at worst (8 s: 10.6 and 17.8; the whole clip: 10.0 and 18.3). Its yaw is arbitrary:
//   no mode needs an absolute one yet (north is step 5).
// - The camera: gyroflow-core's quaternion takes its camera axes (x right, y up, z back) into its world; OpenCV's are
//   those turned half a turn about x. So world_from_cam = align · q · flip.
// - Yaw: about the world's vertical, positive to the right (video-types' R_y: +z → (sin a, 0, cos a)), the view shader's
//   rot_yaw. Of the camera's forward axis where it's level enough, of its right axis (less a quarter turn) where it
//   looks up or down, weighted between: a camera pointing at the sky still has a heading.
// - In the world: the camera's heading at the clip's start, held; where the clip has north or GPS, held in the real world
//   (the gyro's yaw drifts: 0.5°/s on the MAX 2 sample clip, 30° a minute, measured against both). Along the camera: its heading smoothed over a few
//   seconds both ways (the whole clip's motion is known before it plays, so the look-ahead is free), as a mean of unit
//   vectors, so it turns through ±180° without a jump.
// - Along the way you move and north: below (step 5); where a clip hasn't what they need, stamped as along the camera
//   with confidence 0.

use std::f64::consts::{ FRAC_PI_2, PI };
use nalgebra::{ UnitQuaternion, Vector3 };
use crate::stamp::{ Heading, HeadingStamp, HEADINGS };

/// How long the camera's heading is smoothed over: a Gaussian's sigma, seconds (it reaches about 2.5 sigma each way)
pub const CAMERA_SIGMA_S: f64 = 1.0;
/// How long the accelerometer is averaged over for the vertical, a Gaussian's sigma, seconds
pub const GRAVITY_SIGMA_S: f64 = 20.0;

/// gyroflow-core's world into video-types' (gravity along +y) per frame, and its camera axes into OpenCV's
#[derive(Clone, Debug)]
pub struct World {
    align: Vec<UnitQuaternion<f64>>,
}

/// The turn that puts `up` (gyroflow-core's world) on video-types' up, -y, about a horizontal axis
fn leveling(up: &Vector3<f64>) -> UnitQuaternion<f64> {
    UnitQuaternion::rotation_between(&up.normalize(), &-Vector3::y())
        // Exactly opposite: half a turn about any horizontal axis
        .unwrap_or_else(|| UnitQuaternion::from_axis_angle(&Vector3::x_axis(), PI))
}

impl World {
    /// `ups`: per frame, the accelerometer turned into gyroflow-core's world (summed over the frame's samples: it reads
    /// up when still), at `fps`; averaged over GRAVITY_SIGMA_S each way. Frames without samples take their neighbours'
    pub fn new(ups: &[Vector3<f64>], fps: f64) -> World { World::with_sigma(ups, fps, GRAVITY_SIGMA_S) }

    /// The same averaged over `sigma_s` seconds each way
    pub fn with_sigma(ups: &[Vector3<f64>], fps: f64, sigma_s: f64) -> World {
        let sigma = sigma_s * fps.max(1.0);
        let reach = (sigma * 3.0).ceil() as i64;
        let kernel: Vec<f64> = (-reach..=reach).map(|k| (-0.5 * (k as f64 / sigma).powi(2)).exp()).collect();
        let n = ups.len() as i64;
        let mut align = Vec::with_capacity(ups.len());
        let mut last = UnitQuaternion::identity();
        for i in 0..n {
            let mut sum = Vector3::zeros();
            for (k, g) in (-reach..=reach).zip(&kernel) {
                let j = i + k;
                if j >= 0 && j < n { sum += ups[j as usize] * *g; }
            }
            if sum.norm() > 1e-9 { last = leveling(&sum); }
            align.push(last);
        }
        if align.is_empty() { align.push(UnitQuaternion::identity()); }
        World { align }
    }

    /// One vertical for the whole clip
    pub fn constant(up: Vector3<f64>) -> World { World { align: vec![leveling(&up)] } }

    /// The turn that levels gyroflow-core's world at frame `frame` (the nearest)
    pub fn level(&self, frame: f64) -> UnitQuaternion<f64> {
        self.align[(frame.round().max(0.0) as usize).min(self.align.len() - 1)]
    }

    /// The camera in video-types' world at frame `frame` (between frames: the turn of the nearest), from gyroflow-core's
    /// quaternion (its camera axes into its world)
    pub fn world_from_cam(&self, frame: f64, q: &UnitQuaternion<f64>) -> UnitQuaternion<f64> {
        let i = (frame.round().max(0.0) as usize).min(self.align.len() - 1);
        self.align[i] * q * flip()
    }
}

/// OpenCV's camera axes into gyroflow-core's (and back): half a turn about x
pub fn flip() -> UnitQuaternion<f64> { UnitQuaternion::from_axis_angle(&Vector3::x_axis(), PI) }

/// The camera's yaw in the world and how well it's defined (0 to 1): its forward axis's (the yaw of a yaw, pitch, roll
/// decomposition) while that's within ~60° of level; towards straight up or down, where that one turns into the roll,
/// its right axis's (a quarter turn less) instead, blended between
pub fn yaw(world_from_cam: &UnitQuaternion<f64>) -> (f64, f64) {
    let f = world_from_cam * Vector3::z();
    let r = world_from_cam * Vector3::x();
    let (hf, hr) = (f.x * f.x + f.z * f.z, r.x * r.x + r.z * r.z);
    let (yf, yr) = (f.x.atan2(f.z), r.x.atan2(r.z) - FRAC_PI_2);
    let t = ((hf - 0.05) / 0.2).clamp(0.0, 1.0);
    let wf = t * t * (3.0 - 2.0 * t);
    let wr = 1.0 - wf;
    let (x, y) = (wf * yf.cos() + wr * yr.cos(), wf * yf.sin() + wr * yr.sin());
    // The forward and right axes' horizontal parts add up to 1 at least (each world axis's squares over the camera's
    // three sum to 1): one of them always has a yaw. Low only where both blend in disagreeing
    (y.atan2(x), (x.hypot(y) * (wf * hf.sqrt() + wr * hr.sqrt())).min(1.0))
}

/// The yaws of the heading modes, per frame
#[derive(Clone, Debug)]
pub struct Headings {
    start: f64,
    camera: Vec<f64>,
    /// Along the way and north per frame (yaw, confidence), when the clip has what they need
    travel: Option<Vec<(f64, f64)>>,
    north: Option<Vec<(f64, f64)>>,
    /// In the world per frame, where the gyro's yaw drift is known (from north, or the GPS fit): the start's heading
    /// turned with the drift, so it stays put in the real world
    world: Option<Vec<f64>>,
}

impl Headings {
    /// From the camera's yaw and its weight per frame (`yaw`), at `fps`
    pub fn new(yaws: &[(f64, f64)], fps: f64) -> Headings {
        let camera = smooth(yaws, CAMERA_SIGMA_S * fps.max(1.0));
        Headings { start: camera.first().copied().unwrap_or(0.0), camera, travel: None, north: None, world: None }
    }

    /// The camera's smoothed heading per frame
    pub fn camera(&self) -> &[f64] { &self.camera }

    pub fn with_travel(mut self, v: Vec<(f64, f64)>) -> Headings { if !v.is_empty() { self.travel = Some(v); } self }
    pub fn with_north(mut self, v: Vec<(f64, f64)>) -> Headings { if !v.is_empty() { self.north = Some(v); } self }
    /// The world's drift per frame (radians: how far a fixed direction of the real world has turned in the gyro's
    /// world since the start), from north or the GPS fit
    pub fn with_drift(mut self, drift: Vec<f64>) -> Headings {
        if !drift.is_empty() { self.world = Some(drift.iter().map(|d| self.start + d).collect()); }
        self
    }
    pub fn travel(&self) -> Option<&[(f64, f64)]> { self.travel.as_deref() }
    pub fn north(&self) -> Option<&[(f64, f64)]> { self.north.as_deref() }

    pub fn frames(&self) -> usize { self.camera.len() }

    /// The stamp's headings at frame `i` (held past either end)
    pub fn at(&self, i: i64) -> [HeadingStamp; HEADINGS] {
        let camera = if self.camera.is_empty() { 0.0 } else { self.camera[i.clamp(0, self.camera.len() as i64 - 1) as usize] };
        let mut h = [HeadingStamp { yaw: camera, confidence: 0.0 }; HEADINGS];
        let world = self.world.as_ref().filter(|w| !w.is_empty()).map(|w| w[i.clamp(0, w.len() as i64 - 1) as usize]).unwrap_or(self.start);
        h[Heading::World as usize] = HeadingStamp { yaw: (world + PI).rem_euclid(2.0 * PI) - PI, confidence: 1.0 };
        h[Heading::Camera as usize] = HeadingStamp { yaw: camera, confidence: 1.0 };
        // Along the way and north where the clip has them; elsewhere along the camera with confidence 0
        for (mode, v) in [(Heading::Travel, &self.travel), (Heading::North, &self.north)] {
            if let Some(v) = v.as_ref().filter(|v| !v.is_empty()) {
                let (yaw, confidence) = v[i.clamp(0, v.len() as i64 - 1) as usize];
                h[mode as usize] = HeadingStamp { yaw: (yaw + PI).rem_euclid(2.0 * PI) - PI, confidence };
            }
        }
        h
    }
}

/// A Gaussian mean of the yaws as unit vectors (weighted), `sigma` frames each way; where they cancel out, the last
/// heading held
pub fn smooth(yaws: &[(f64, f64)], sigma: f64) -> Vec<f64> {
    let reach = (sigma * 3.0).ceil() as i64;
    let kernel: Vec<f64> = (-reach..=reach).map(|k| (-0.5 * (k as f64 / sigma).powi(2)).exp()).collect();
    let n = yaws.len() as i64;
    let mut out = Vec::with_capacity(yaws.len());
    let mut last = yaws.iter().find(|y| y.1 > 0.0).map(|y| y.0).unwrap_or(0.0);
    for i in 0..n {
        let (mut x, mut y) = (0.0, 0.0);
        for (k, g) in (-reach..=reach).zip(&kernel) {
            let j = i + k;
            if j < 0 || j >= n { continue; }
            let (a, w) = yaws[j as usize];
            x += g * w * a.cos();
            y += g * w * a.sin();
        }
        if x.hypot(y) > 1e-9 { last = y.atan2(x); }
        out.push(last);
    }
    out
}


// ---------------------------------------------------------------------------------------------------
// Along the way you move, and north (step 5 of docs/360-implementation-plan.md)
//
// The world's yaw is the gyro's own, arbitrary. GPS gives where the camera moves as azimuths (clockwise from true north),
// so the two are tied by one angle: the azimuth θ of the world's +z (a world yaw a is the azimuth θ + a: both turn
// clockwise seen from above, the world's x right, z ahead, y down being east, north, down turned about the vertical).
// θ drifts as the gyro does, slowly. It's fitted so that the accelerometer's horizontal acceleration, turned into the
// world, matches the change of the GPS velocity (`align`):
// - both on a 0.1 s grid; the GPS positions' second difference over ±1 s is the acceleration averaged by a triangle 2 s
//   wide, and the accelerometer is averaged by the same triangle;
// - both less their mean over ~5 s (a Gaussian, the same for both): a tilt of the world's vertical by a degree or two
//   (heading.rs's World, the accelerometer averaged over 20 s) leaks 0.2-0.3 m/s² of gravity into the horizontal, which
//   moves with the vertical's error, slowly;
// - as complex numbers (north + i·east, z + i·x), the GPS's g ≈ s·e^{iθ(t)}·w(t - lag): for each lag (±LAG_SEARCH_S) and
//   drift (±1.2°/s: an uncalibrated MEMS gyro's yaw drifts by up to ~0.5°/s) on a grid, θ at the clip's middle is the argument of Σ g·conj(w)·e^{-i·drift·(t - mid)} (Procrustes);
//   the largest sum wins, refined once on a finer grid;
// - how sure: its correlation ρ = |Σ| / √(Σ|g|²·Σ|w|²) and the angle's spread σ from the residuals (samples a second
//   apart counted as independent). The confidence is 1 to σ = 2°, 0 from 15°, and scaled down below ρ = 0.7; less than
//   30 s of acceleration over 0.3 m/s² (a straight road at a steady speed) is no fit.
// The course over ground is the GPS velocity's (positions over ±0.5 s), lag taken out, turned into the world by θ. While
// moving (from 1.5 m/s, wholly from 3) the heading is that, smoothed as the camera's is; standing, its offset from the
// camera's heading from just before is held until it moves again, so the view goes on looking the way the vehicle went
// and turns with the camera (on the sample clip the camera looks sideways: falling back to the camera's own heading
// would swing the view 90° at every stop). Its confidence: the fit's, times how much it's moving.
//
// North: a GoPro MAX 2's own magnetic north (MNOR, in the rig's axes per frame, with OCNF), turned into the world by
// the frame's camera; its yaw smoothed over a few seconds, weighted by OCNF. Without it, the GPS fit's true north (the
// world yaw -θ). Magnetic north is the declination off true north (~2.5° east in the Netherlands in 2025).

/// The GPS fit of the world's yaw
#[derive(Clone, Debug, PartialEq)]
pub struct Alignment {
    /// The azimuth of the world's +z at `mid` (s), radians, and its drift, radians a second
    pub theta: f64,
    pub drift: f64,
    pub mid: f64,
    /// How late the GPS velocity is behind the accelerometer, s
    pub lag: f64,
    /// The correlation of the fit, and the angle's spread, radians
    pub rho: f64,
    pub sigma: f64,
    /// Seconds with a horizontal acceleration over 0.3 m/s²
    pub excited_s: f64,
    /// The accelerometer's scale against GPS (1 when both are right)
    pub scale: f64,
}

impl Alignment {
    /// The azimuth of the world's +z at `t` (s)
    pub fn theta_at(&self, t: f64) -> f64 { self.theta + self.drift * (t - self.mid) }

    /// 0-1: 1 to σ = 2°, 0 from 15°, less below ρ = 0.7
    pub fn confidence(&self) -> f64 {
        let by_sigma = ((15.0 - self.sigma.to_degrees()) / 13.0).clamp(0.0, 1.0);
        let by_rho = ((self.rho - 0.3) / 0.4).clamp(0.0, 1.0);
        by_sigma * by_rho
    }
}

/// The grid of the fit
const DT: f64 = 0.1;
/// How far the GPS's clock may be off the video's either way, s: GoPro's GPS is in the video's clock; Insta360's fixes
/// are in unix time, placed by the clip's metadata (nav.rs), not checked on a clip
pub const LAG_SEARCH_S: f64 = 3.0;
/// The longest gap between fixes interpolated across, s (Insta360's GPS remote and phone app: 1 Hz or more)
pub const GPS_GAP_S: f64 = 2.5;

#[derive(Clone, Copy, Default)]
struct C { re: f64, im: f64 }
impl C {
    fn new(re: f64, im: f64) -> C { C { re, im } }
    fn polar(a: f64) -> C { C { re: a.cos(), im: a.sin() } }
    fn mul(self, o: C) -> C { C::new(self.re * o.re - self.im * o.im, self.re * o.im + self.im * o.re) }
    fn conj(self) -> C { C::new(self.re, -self.im) }
    fn add(self, o: C) -> C { C::new(self.re + o.re, self.im + o.im) }
    fn sub(self, o: C) -> C { C::new(self.re - o.re, self.im - o.im) }
    fn scale(self, k: f64) -> C { C::new(self.re * k, self.im * k) }
    fn norm2(self) -> f64 { self.re * self.re + self.im * self.im }
    fn arg(self) -> f64 { self.im.atan2(self.re) }
}

/// Positions on the grid from fixes (s, [east, north]): linear between fixes up to a second apart, None across a gap
fn on_grid(track: &[(f64, [f64; 2])], t0: f64, n: usize) -> Vec<Option<[f64; 2]>> {
    let mut out = vec![None; n];
    let mut j = 0;
    for (k, o) in out.iter_mut().enumerate() {
        let t = t0 + k as f64 * DT;
        while j + 1 < track.len() && track[j + 1].0 < t { j += 1; }
        if j + 1 >= track.len() { break; }
        let (a, b) = (&track[j], &track[j + 1]);
        if t < a.0 || t > b.0 || b.0 - a.0 > GPS_GAP_S || b.0 <= a.0 { continue; }
        let f = (t - a.0) / (b.0 - a.0);
        *o = Some([a.1[0] + (b.1[0] - a.1[0]) * f, a.1[1] + (b.1[1] - a.1[1]) * f]);
    }
    out
}

/// A Gaussian mean of valid samples, `sigma` samples each way (None where none is near)
fn gauss(v: &[Option<C>], sigma: f64) -> Vec<Option<C>> {
    let reach = (sigma * 3.0).ceil() as i64;
    let kernel: Vec<f64> = (-reach..=reach).map(|k| (-0.5 * (k as f64 / sigma).powi(2)).exp()).collect();
    (0..v.len() as i64).map(|i| {
        let (mut s, mut w) = (C::default(), 0.0);
        for (k, g) in (-reach..=reach).zip(&kernel) {
            if let Some(Some(x)) = v.get((i + k) as usize).filter(|_| i + k >= 0) { s = s.add(x.scale(*g)); w += g; }
        }
        (w > 0.3).then(|| s.scale(1.0 / w))
    }).collect()
}

/// The fit (see above). `acc`: the accelerometer's specific force turned into the world (gravity along +y), (s, vector);
/// `track`: GPS fixes (s, [east, north] m)
pub fn align(acc: &[(f64, Vector3<f64>)], track: &[(f64, [f64; 2])]) -> Option<Alignment> {
    if track.len() < 20 || acc.is_empty() { return None; }
    let t0 = track[0].0.max(acc[0].0);
    let t1 = track[track.len() - 1].0.min(acc[acc.len() - 1].0);
    if t1 - t0 < 20.0 { return None; }
    let n = ((t1 - t0) / DT).floor() as usize;
    // The GPS's acceleration: the second difference over ±1 s
    let pos = on_grid(track, t0, n);
    const H: usize = 10;
    let mut g: Vec<Option<C>> = vec![None; n];
    for k in H..n.saturating_sub(H) {
        if let (Some(a), Some(b), Some(c)) = (pos[k - H], pos[k], pos[k + H]) {
            let h2 = (H as f64 * DT).powi(2);
            g[k] = Some(C::new((a[1] - 2.0 * b[1] + c[1]) / h2, (a[0] - 2.0 * b[0] + c[0]) / h2));
        }
    }
    // The accelerometer's horizontal part per cell, then the same triangle
    let mut sum = vec![(C::default(), 0usize); n];
    for (t, v) in acc {
        let k = ((t - t0) / DT).floor();
        if k >= 0.0 && (k as usize) < n { let e = &mut sum[k as usize]; e.0 = e.0.add(C::new(v.z, v.x)); e.1 += 1; }
    }
    let cells: Vec<Option<C>> = sum.iter().map(|(s, c)| (*c > 0).then(|| s.scale(1.0 / *c as f64))).collect();
    let mut w: Vec<Option<C>> = vec![None; n];
    for k in H..n.saturating_sub(H) {
        let (mut s, mut ok) = (C::default(), true);
        for j in -(H as i64 - 1)..=(H as i64 - 1) {
            match cells[(k as i64 + j) as usize] {
                Some(c) => s = s.add(c.scale((1.0 - j.abs() as f64 / H as f64) / H as f64)),
                None => { ok = false; break; }
            }
        }
        if ok { w[k] = Some(s); }
    }
    // Less their slow part, the same for both: the accelerometer only where GPS is too, or across a gap in the fixes
    // their means would be of different stretches
    let w: Vec<Option<C>> = w.iter().zip(&g).map(|(w, g)| g.and(*w)).collect();
    let sigma = 5.0 / DT;
    let (gs, ws) = (gauss(&g, sigma), gauss(&w, sigma));
    let hp = |v: &[Option<C>], s: &[Option<C>]| -> Vec<Option<C>> { v.iter().zip(s).map(|(a, b)| match (a, b) { (Some(a), Some(b)) => Some(a.sub(*b)), _ => None }).collect() };
    let (g, w) = (hp(&g, &gs), hp(&w, &ws));
    let excited_s = g.iter().flatten().filter(|c| c.norm2() > 0.09).count() as f64 * DT;
    if excited_s < 30.0 { return None; }
    let mid = t0 + n as f64 * DT / 2.0;
    // The sum for a lag (cells) and drift
    let score = |lag: i64, drift: f64| -> (C, f64, f64) {
        let (mut s, mut gg, mut ww) = (C::default(), 0.0, 0.0);
        for k in 0..n as i64 {
            let (Some(a), Some(b)) = (g[k as usize], w.get((k - lag) as usize).copied().flatten().filter(|_| k - lag >= 0)) else { continue };
            let t = t0 + k as f64 * DT;
            s = s.add(a.mul(b.conj()).mul(C::polar(-drift * (t - mid))));
            gg += a.norm2();
            ww += b.norm2();
        }
        (s, gg, ww)
    };
    // Coarse first (every lag, drift by 0.02°/s), then finer around the best (lag ±1 cell, drift by 0.001°/s)
    let lags = (LAG_SEARCH_S / DT).round() as i64;
    let mut best = (0i64, 0.0f64, f64::MIN);
    for lag in -lags..=lags {
        for d in -60..=60 {
            let drift = (d as f64 * 0.02).to_radians();
            let (s, _, _) = score(lag, drift);
            if s.norm2() > best.2 { best = (lag, drift, s.norm2()); }
        }
    }
    let coarse = best;
    for lag in coarse.0 - 1..=coarse.0 + 1 {
        for d in -20..=20 {
            let drift = coarse.1 + (d as f64 * 0.001).to_radians();
            let (s, _, _) = score(lag, drift);
            if s.norm2() > best.2 { best = (lag, drift, s.norm2()); }
        }
    }
    let (lag, drift) = (best.0, best.1);
    let (s, gg, ww) = score(lag, drift);
    let theta = s.arg();
    let scale = s.norm2().sqrt() / ww;
    // The residuals, and the angle's spread with samples a second apart independent
    let (mut r2, mut used) = (0.0, 0usize);
    for k in 0..n as i64 {
        let (Some(a), Some(b)) = (g[k as usize], w.get((k - lag) as usize).copied().flatten().filter(|_| k - lag >= 0)) else { continue };
        let t = t0 + k as f64 * DT;
        r2 += a.sub(b.mul(C::polar(theta + drift * (t - mid))).scale(scale)).norm2();
        used += 1;
    }
    // var θ = σr² / (2·s²·Σ|w|²) for independent samples, σr² the residual's variance per sample; the samples within a
    // second of each other aren't (the triangle), so ten times that
    let per_independent = 1.0 / DT;
    let sigma = (r2 / used.max(1) as f64 * per_independent / (2.0 * scale * scale * ww)).sqrt();
    Some(Alignment { theta, drift, mid, lag: lag as f64 * DT, rho: s.norm2().sqrt() / (gg * ww).sqrt(), sigma, excited_s, scale })
}

/// As `smooth`, but where the weights within reach add up to less than a quarter of a full kernel's, the last value
/// that had enough is held (the first one before it): a mean of the few samples at a stop's edge is their noise
pub fn smooth_held(yaws: &[(f64, f64)], sigma: f64) -> Vec<f64> {
    let reach = (sigma * 3.0).ceil() as i64;
    let kernel: Vec<f64> = (-reach..=reach).map(|k| (-0.5 * (k as f64 / sigma).powi(2)).exp()).collect();
    let enough = kernel.iter().sum::<f64>() / 4.0;
    let n = yaws.len() as i64;
    let mut out: Vec<Option<f64>> = (0..n).map(|i| {
        let (mut x, mut y, mut w) = (0.0, 0.0, 0.0);
        for (k, g) in (-reach..=reach).zip(&kernel) {
            let j = i + k;
            if j < 0 || j >= n { continue; }
            let (a, wt) = yaws[j as usize];
            x += g * wt * a.cos();
            y += g * wt * a.sin();
            w += g * wt;
        }
        (w >= enough && x.hypot(y) > 1e-9).then(|| y.atan2(x))
    }).collect();
    let first = out.iter().flatten().next().copied().unwrap_or(0.0);
    let mut last = first;
    for o in out.iter_mut() {
        match o { Some(v) => last = *v, None => *o = Some(last) }
    }
    out.into_iter().map(|o| o.unwrap_or(first)).collect()
}

/// How much the camera is moving at `speed` m/s for the course to count: 0 to 1.5, 1 from 3
pub fn moving(speed: f64) -> f64 {
    let t = ((speed - 1.5) / 1.5).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Along the way you move, per frame (yaw in the world, confidence): `fps`, `frame_t(i)` the frame's time in the
/// track's seconds, `camera` the camera's smoothed heading per frame
pub fn travel(al: &Alignment, track: &[(f64, [f64; 2])], speeds: &[(f64, f64)], camera: &[f64], fps: f64, frame_t: impl Fn(usize) -> f64) -> Vec<(f64, f64)> {
    if track.len() < 2 { return Vec::new(); }
    let t0 = track[0].0;
    let n = ((track[track.len() - 1].0 - t0) / DT).floor() as usize + 1;
    let pos = on_grid(track, t0, n);
    const H: usize = 5;
    let speed_at = |t: f64| -> f64 {
        let k = speeds.partition_point(|s| s.0 < t);
        match (k.checked_sub(1).and_then(|j| speeds.get(j)), speeds.get(k)) {
            (Some(a), Some(b)) if b.0 > a.0 => a.1 + (b.1 - a.1) * (t - a.0) / (b.0 - a.0),
            (Some(a), _) => a.1,
            (_, Some(b)) => b.1,
            _ => 0.0,
        }
    };
    // The offset from the camera's heading where it's moving, weighted by how much
    let offsets: Vec<(f64, f64, f64)> = (0..camera.len()).map(|i| {
        let t = frame_t(i) + al.lag;
        let k = ((t - t0) / DT).round();
        if k < H as f64 || k as usize + H >= n { return (0.0, 0.0, 0.0); }
        let k = k as usize;
        let (Some(a), Some(b)) = (pos[k - H], pos[k + H]) else { return (0.0, 0.0, 0.0) };
        let (ve, vn) = (b[0] - a[0], b[1] - a[1]);
        let v = speed_at(t).max(0.0);
        let m = moving(v).min(moving((ve * ve + vn * vn).sqrt() / (2 * H) as f64 / DT));
        if m <= 0.0 { return (0.0, 0.0, 0.0); }
        let yaw = ve.atan2(vn) - al.theta_at(frame_t(i));
        ((yaw - camera[i] + PI).rem_euclid(2.0 * PI) - PI, m, v)
    }).collect();
    if offsets.iter().all(|o| o.1 < 0.3) { return Vec::new(); }
    // Slow, the course is noise (0.3 m of GPS over a second's way is 8° at 3 m/s): weighted by the inverse of its
    // variance, the speed squared (flat from 10 m/s), and by how much it's moving; nothing below 0.3 (~2 m/s). Where
    // too little near weighs, the offset from before is held
    let weighted: Vec<(f64, f64)> = offsets.iter().map(|(o, m, v)| (*o, if *m < 0.3 { 0.0 } else { m * (v / 10.0).min(1.0).powi(2) })).collect();
    let smoothed = smooth_held(&weighted, CAMERA_SIGMA_S * fps.max(1.0));
    let c = al.confidence();
    camera.iter().zip(smoothed).zip(&offsets).map(|((cam, o), (_, m, _))| (cam + o, c * m)).collect()
}

/// North per frame (yaw in the world, confidence) from directions to it in the camera's axes, each with its own
/// confidence (a MAX 2's MNOR and OCNF), turned into the world by the frame's camera
pub fn north(world_from_cam: &[UnitQuaternion<f64>], to_north: &[(Vector3<f64>, f64)], fps: f64) -> Vec<(f64, f64)> {
    let yaws: Vec<(f64, f64)> = world_from_cam.iter().zip(to_north).map(|(q, (n, c))| {
        let w = q * n;
        let level = w.x.hypot(w.z);
        (w.x.atan2(w.z), if level > 0.5 { *c } else { 0.0 })
    }).collect();
    let sigma = NORTH_SIGMA_S * fps.max(1.0);
    let yaw = smooth(&yaws, sigma);
    let reach = (sigma * 2.0).ceil() as i64;
    let n = yaws.len() as i64;
    (0..n).map(|i| {
        let (lo, hi) = ((i - reach).max(0) as usize, ((i + reach).min(n - 1)) as usize);
        let c = yaws[lo..=hi].iter().map(|y| y.1).sum::<f64>() / (hi - lo + 1) as f64;
        (yaw[i as usize], c)
    }).collect()
}

/// How long north is smoothed over, a Gaussian's sigma, seconds
pub const NORTH_SIGMA_S: f64 = 3.0;

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: f64, b: f64) -> bool { ((a - b + PI).rem_euclid(2.0 * PI) - PI).abs() < 1e-9 }
    /// video-types' R_y, R_x, R_z
    fn ry(a: f64) -> UnitQuaternion<f64> { UnitQuaternion::from_axis_angle(&Vector3::y_axis(), a) }
    fn rx(a: f64) -> UnitQuaternion<f64> { UnitQuaternion::from_axis_angle(&Vector3::x_axis(), a) }
    fn rz(a: f64) -> UnitQuaternion<f64> { UnitQuaternion::from_axis_angle(&Vector3::z_axis(), a) }

    #[test]
    fn the_world_has_gravity_down() {
        // gyroflow-core's world with z up (VQF's): a camera whose quaternion is identity looks along its -z (back is +z),
        // which there is the world's -z: straight down
        let w = World::constant(Vector3::z());
        let cam = w.world_from_cam(0.0, &UnitQuaternion::identity());
        assert!((cam * Vector3::z() - Vector3::y()).norm() < 1e-12, "{}", cam * Vector3::z());
        // Whatever the up, it ends on -y
        for up in [Vector3::new(0.3, -0.2, 0.9), Vector3::new(0.0, 0.0, -1.0), Vector3::x()] {
            let w = World::constant(up);
            assert!((w.align[0] * up.normalize() + Vector3::y()).norm() < 1e-12);
        }
    }

    #[test]
    fn braking_doesnt_tilt_the_horizon_much() {
        // A camera still, level, at 30 fps for 40 s: the accelerometer reads up (z in this world), 9.81 m/s². From 15 to
        // 18 s it brakes at 4 m/s² along x: an AHRS that trusts the accelerometer sees ~22° of pitch
        let fps = 30.0;
        let ups: Vec<Vector3<f64>> = (0..1200).map(|i| {
            let t = i as f64 / fps;
            Vector3::new(if (15.0..18.0).contains(&t) { 4.0 } else { 0.0 }, 0.0, 9.81)
        }).collect();
        let w = World::new(&ups, fps);
        let tilt = |i: usize| (w.align[i] * Vector3::z()).angle(&-Vector3::y()).to_degrees();
        let worst = (0..1200).map(tilt).fold(0.0, f64::max);
        assert!(worst < 4.0, "{worst}°");
        // Spread over the clip instead: a little everywhere
        assert!(tilt(100) < 3.0 && tilt(1150) < 3.0, "{} {}", tilt(100), tilt(1150));
        // A vertical that turns for good (the gyro's world drifting): followed over the sigma
        let n = 30 * 300;
        let ups: Vec<Vector3<f64>> = (0..n).map(|i| if i < n / 2 { Vector3::z() } else { Vector3::new(0.5, 0.0, 0.866) }).collect();
        let w = World::new(&ups, fps);
        let level = |i: usize, up: Vector3<f64>| (w.align[i] * up).angle(&-Vector3::y()).to_degrees();
        assert!(level(100, Vector3::z()) < 0.1 && level(n - 100, Vector3::new(0.5, 0.0, 0.866)) < 0.1);
    }

    #[test]
    fn a_level_camera_has_its_own_yaw() {
        for deg in [-170.0, -90.0, 0.0, 30.0, 135.0, 179.0] {
            let a = f64::to_radians(deg);
            let (y, w) = yaw(&ry(a));
            assert!(close(y, a) && (w - 1.0).abs() < 1e-12, "{deg}: {y} {w}");
            // Tilted and rolled a little, the same
            let (y, _) = yaw(&(ry(a) * rx(0.2) * rz(-0.3)));
            assert!(close(y, a), "{deg}: {y}");
        }
    }

    #[test]
    fn a_camera_looking_up_or_down_still_has_one() {
        for deg in [-120.0, 10.0, 80.0] {
            let a = f64::to_radians(deg);
            for pitch in [FRAC_PI_2, -FRAC_PI_2, 1.4] {
                let (y, w) = yaw(&(ry(a) * rx(pitch)));
                assert!(close(y, a) && w > 0.9, "{deg} {pitch}: {y} {w}");
            }
            // On its side: its forward axis says it
            let (y, _) = yaw(&(ry(a) * rz(FRAC_PI_2)));
            assert!(close(y, a));
        }
    }

    #[test]
    fn the_camera_heading_is_smoothed_both_ways() {
        let fps = 30.0;
        // Still at 0, then turned to 90° at frame 300
        let yaws: Vec<(f64, f64)> = (0..600).map(|i| (if i < 300 { 0.0 } else { FRAC_PI_2 }, 1.0)).collect();
        let h = Headings::new(&yaws, fps);
        let at = |i: i64| h.at(i)[Heading::Camera as usize].yaw;
        // Far from the turn: where the camera points; at it, halfway (the look-ahead); smooth through it
        assert!(close(at(0), 0.0) && close(at(599), FRAC_PI_2));
        assert!((at(300) - at(299) - 0.0).abs() < 0.05);
        assert!((at(299) + at(300) - FRAC_PI_2).abs() < 0.05, "{} {}", at(299), at(300));
        assert!(at(260) > 0.01 && at(340) < FRAC_PI_2 - 0.01);
        // In the world: where it started
        assert!(h.at(450).iter().zip([0.0, at(450)]).all(|(s, y)| close(s.yaw, y)));
        assert_eq!(h.at(450)[Heading::World as usize].confidence, 1.0);
        assert_eq!(h.at(450)[Heading::Travel as usize].confidence, 0.0);
    }

    #[test]
    fn turning_through_the_back_doesnt_jump() {
        // A steady turn of 30°/s through ±180°: smoothed, it stays a steady turn
        let fps = 30.0;
        let yaws: Vec<(f64, f64)> = (0..900).map(|i| (((i as f64 / fps) * 30.0f64.to_radians() + PI).rem_euclid(2.0 * PI) - PI, 1.0)).collect();
        let s = smooth(&yaws, CAMERA_SIGMA_S * fps);
        for i in 100..800 {
            let step = ((s[i + 1] - s[i] + PI).rem_euclid(2.0 * PI)) - PI;
            assert!((step - 1.0f64.to_radians()).abs() < 1e-6, "{i}: {step}");
            assert!(close(s[i], yaws[i].0), "{i}");
        }
    }

    #[test]
    fn the_view_shader_sees_the_horizon_level() {
        // The shader's ray into the camera: transpose(world_from_cam) · R_y(heading) · head. With the camera along the
        // heading and the head straight ahead, the view's forward is level and its up is up, whatever the camera's tilt
        for (a, pitch, roll) in [(0.3, 0.2, -0.4), (-2.0, -0.5, 0.3), (3.0, 0.1, 1.2)] {
            let cam = ry(a) * rx(pitch) * rz(roll);
            let (heading, _) = yaw(&cam);
            let view_in_world = ry(heading);
            let fwd = view_in_world * Vector3::z();
            let up = view_in_world * -Vector3::y();
            assert!(fwd.y.abs() < 1e-12 && (up + Vector3::y()).norm() < 1e-12);
            // And it looks where the camera does, about the vertical
            let cf = cam * Vector3::z();
            assert!(close(fwd.x.atan2(fwd.z), cf.x.atan2(cf.z)));
        }
    }

    /// A drive for the GPS fit: 300 s at 100 Hz, turns, starts and stops; the vehicle's azimuth and speed per sample
    struct Drive { t: Vec<f64>, azimuth: Vec<f64>, speed: Vec<f64>, east: Vec<f64>, north: Vec<f64> }

    fn drive(straight: bool) -> Drive {
        let dt = 0.01;
        let mut d = Drive { t: vec![], azimuth: vec![], speed: vec![], east: vec![], north: vec![] };
        let (mut az, mut e, mut n) = (0.4f64, 0.0, 0.0);
        for k in 0..30000 {
            let t = k as f64 * dt;
            // Speed: up to 14 m/s, a stop from 150 to 170 s, weaving with the traffic; on a straight road a steady 14
            let v = if straight { 14.0 } else if t < 10.0 { 1.4 * t } else if t < 140.0 { 14.0 + 2.0 * (t / 7.0).sin() }
                else if t < 150.0 { (14.0 + 2.0 * (140f64 / 7.0).sin()) * (150.0 - t) / 10.0 } else if t < 170.0 { 0.0 }
                else if t < 180.0 { 1.2 * (t - 170.0) } else { 12.0 + 1.5 * (t / 5.0).sin() };
            // Turns: a right of 90° at 40 s over 8 s, a left of 60° at 90 s, a roundabout at 200 s, a gentle weave
            let rate = if straight { 0.0 } else {
                (if (40.0..48.0).contains(&t) { FRAC_PI_2 / 8.0 } else { 0.0 })
                    + (if (90.0..96.0).contains(&t) { -PI / 3.0 / 6.0 } else { 0.0 })
                    + (if (200.0..212.0).contains(&t) { 2.0 * PI / 12.0 } else { 0.0 })
                    + 0.05 * (t / 3.0).sin()
            };
            if v > 0.0 { az += rate * dt; }
            e += v * az.sin() * dt;
            n += v * az.cos() * dt;
            d.t.push(t); d.azimuth.push(az); d.speed.push(v); d.east.push(e); d.north.push(n);
        }
        d
    }

    /// Normal noise, repeatable
    struct Noise(u64);
    impl Noise {
        fn next(&mut self) -> f64 {
            let u = &mut || { self.0 ^= self.0 << 13; self.0 ^= self.0 >> 7; self.0 ^= self.0 << 17; (self.0 >> 11) as f64 / (1u64 << 53) as f64 };
            let (a, b) = (u().max(1e-12), u());
            (-2.0 * a.ln()).sqrt() * (2.0 * PI * b).cos()
        }
    }

    /// The GPS fixes at 10 Hz, `lag` s late, 0.3 m of noise; the accelerometer at 100 Hz in a world whose +z is at
    /// azimuth theta0 + drift·(t - 150), with 0.2 m/s² of noise and 0.25 of slowly turning gravity leaking in; `mirror`:
    /// a world turned the wrong way (a gyro with its yaw backwards)
    fn sensors(d: &Drive, theta0: f64, drift: f64, lag: f64, mirror: bool) -> (Vec<(f64, Vector3<f64>)>, Vec<(f64, [f64; 2])>, Vec<(f64, f64)>) {
        let mut noise = Noise(0x9e3779b97f4a7c15);
        let mut track = Vec::new();
        let mut speeds = Vec::new();
        let back = (lag / 0.01).round() as usize;
        for k in (back..d.t.len()).step_by(10) {
            let j = k - back;
            track.push((d.t[k], [d.east[j] + 0.3 * noise.next(), d.north[j] + 0.3 * noise.next()]));
            speeds.push((d.t[k], d.speed[j]));
        }
        let mut acc = Vec::new();
        for k in 1..d.t.len() - 1 {
            // The acceleration, east and north, from the speed and azimuth's changes
            let ve = |i: usize| d.speed[i] * d.azimuth[i].sin();
            let vn = |i: usize| d.speed[i] * d.azimuth[i].cos();
            let (ae, an) = ((ve(k + 1) - ve(k - 1)) / 0.02, (vn(k + 1) - vn(k - 1)) / 0.02);
            let theta = theta0 + drift * (d.t[k] - 150.0);
            // World = e^{-iθ}·(north + i·east), as (z, x)
            let (c, s) = (theta.cos(), theta.sin());
            let (z, mut x) = (an * c + ae * s, ae * c - an * s);
            if mirror { x = -x; }
            let leak = 0.25 * (d.t[k] / 40.0);
            acc.push((d.t[k], Vector3::new(x + 0.2 * noise.next() + leak.sin() * 0.25, -9.81, z + 0.2 * noise.next() + leak.cos() * 0.25)));
        }
        (acc, track, speeds)
    }

    #[test]
    fn the_gps_fit_finds_the_worlds_azimuth() {
        let d = drive(false);
        let (theta0, drift, lag) = (37f64.to_radians(), 0.02f64.to_radians(), 0.3);
        let (acc, track, _) = sensors(&d, theta0, drift, lag, false);
        let a = align(&acc, &track).unwrap();
        let err = ((a.theta_at(150.0) - theta0 + PI).rem_euclid(2.0 * PI) - PI).to_degrees();
        assert!(err.abs() < 1.5, "θ off by {err}° ({a:?})");
        assert!((a.drift - drift).to_degrees().abs() < 0.01, "{a:?}");
        assert!((a.lag - lag).abs() < 0.15, "{a:?}");
        assert!(a.confidence() > 0.8 && a.sigma.to_degrees() < 4.0 && (a.scale - 1.0).abs() < 0.2, "{a:?}");
    }

    #[test]
    fn a_steady_straight_road_is_no_fit() {
        let d = drive(true);
        let (acc, track, _) = sensors(&d, 1.0, 0.0, 0.2, false);
        assert!(align(&acc, &track).is_none_or(|a| a.confidence() < 0.2));
    }

    #[test]
    fn a_gyro_turning_the_wrong_way_is_no_fit() {
        let d = drive(false);
        let (acc, track, _) = sensors(&d, 1.0, 0.0, 0.2, true);
        let a = align(&acc, &track);
        assert!(a.as_ref().is_none_or(|a| a.confidence() < 0.3), "{a:?}");
    }

    #[test]
    fn along_the_way_follows_the_course_and_holds_while_stopped() {
        let d = drive(false);
        let (theta0, drift) = (-120f64.to_radians(), -0.01f64.to_radians());
        let (acc, track, speeds) = sensors(&d, theta0, drift, 0.3, false);
        let a = align(&acc, &track).unwrap();
        // 30 fps; a camera looking 90° to the right of the way, its heading in the world
        let fps = 30.0;
        let frames = (299.0 * fps) as usize;
        let at = |t: f64| ((t / 0.01).round() as usize).min(d.t.len() - 1);
        let world = |t: f64, az: f64| az - (theta0 + drift * (t - 150.0));
        let camera: Vec<f64> = (0..frames).map(|i| { let t = i as f64 / fps; world(t, d.azimuth[at(t)]) + FRAC_PI_2 }).collect();
        let tr = travel(&a, &track, &speeds, &camera, fps, |i| i as f64 / fps);
        assert_eq!(tr.len(), frames);
        let diff = |x: f64, y: f64| ((x - y + PI).rem_euclid(2.0 * PI) - PI).to_degrees();
        let mut worst = 0.0f64;
        for i in (0..frames).step_by(7) {
            let t = i as f64 / fps;
            if d.speed[at(t)] < 4.0 || t < 5.0 { continue; }
            // Through the turns too: smoothed over a second, as the camera's heading
            worst = worst.max(diff(tr[i].0, world(t, d.azimuth[at(t)])).abs());
            assert!(tr[i].1 > 0.7, "{t}: {:?}", tr[i]);
        }
        assert!(worst < 4.0, "{worst}°");
        // Stopped (150-170 s): the way it went, held against the camera, and confidence 0
        for t in [152.0, 160.0, 168.0] {
            let i = (t * fps) as usize;
            assert!(diff(tr[i].0, camera[i] - FRAC_PI_2).abs() < 3.0 && tr[i].1 == 0.0, "{t}: {:?}", tr[i]);
        }
    }

    /// The track and speeds less the fixes in `gaps` (s), and kept every `every`-th (10 Hz to 1 Hz: 10)
    fn thinned(track: &[(f64, [f64; 2])], speeds: &[(f64, f64)], gaps: &[(f64, f64)], every: usize) -> (Vec<(f64, [f64; 2])>, Vec<(f64, f64)>) {
        let keep = |t: f64| !gaps.iter().any(|g| t >= g.0 && t < g.1);
        (track.iter().step_by(every).filter(|f| keep(f.0)).copied().collect(), speeds.iter().step_by(every).filter(|f| keep(f.0)).copied().collect())
    }

    /// Along the way less where the vehicle went, the worst over frames moving faster than 4 m/s outside `skip`, and the
    /// confidences there
    fn travel_error(d: &Drive, tr: &[(f64, f64)], theta0: f64, drift: f64, fps: f64, skip: &[(f64, f64)]) -> (f64, f64) {
        let at = |t: f64| ((t / 0.01).round() as usize).min(d.t.len() - 1);
        let (mut worst, mut lowest) = (0.0f64, 1.0f64);
        for (i, (yaw, c)) in tr.iter().enumerate().step_by(7) {
            let t = i as f64 / fps;
            if d.speed[at(t)] < 4.0 || t < 5.0 || skip.iter().any(|s| t >= s.0 && t < s.1) { continue; }
            let want = d.azimuth[at(t)] - (theta0 + drift * (t - 150.0));
            let e = ((yaw - want + PI).rem_euclid(2.0 * PI) - PI).to_degrees().abs();
            worst = worst.max(e);
            lowest = lowest.min(*c);
        }
        (worst, lowest)
    }

    #[test]
    fn gps_dropouts_are_bridged_by_the_gyro() {
        // A tunnel of 20 s in the first turn's lead-up and a few short losses; the fit and along the way over the rest,
        // and across the gaps along the way goes on with the camera (its offset from before held)
        let d = drive(false);
        let (theta0, drift) = (75f64.to_radians(), 0.015f64.to_radians());
        let (acc, track, speeds) = sensors(&d, theta0, drift, 0.3, false);
        let gaps = [(20.0, 40.0), (100.0, 103.0), (230.0, 236.0)];
        let (track, speeds) = thinned(&track, &speeds, &gaps, 1);
        let a = align(&acc, &track).unwrap();
        let err = ((a.theta_at(150.0) - theta0 + PI).rem_euclid(2.0 * PI) - PI).to_degrees();
        assert!(err.abs() < 2.0 && a.confidence() > 0.7, "θ off by {err}° ({a:?})");
        let fps = 30.0;
        let frames = (299.0 * fps) as usize;
        let at = |t: f64| ((t / 0.01).round() as usize).min(d.t.len() - 1);
        // A camera fixed on the vehicle looking 30° left of the way
        let camera: Vec<f64> = (0..frames).map(|i| { let t = i as f64 / fps; d.azimuth[at(t)] - (theta0 + drift * (t - 150.0)) - 0.5236 }).collect();
        let tr = travel(&a, &track, &speeds, &camera, fps, |i| i as f64 / fps);
        // Away from the gaps (and a second or two after, the smoothing's reach), as without them
        let skip: Vec<(f64, f64)> = gaps.iter().map(|g| (g.0 - 1.0, g.1 + 3.0)).collect();
        let (worst, lowest) = travel_error(&d, &tr, theta0, drift, fps, &skip);
        assert!(worst < 4.0 && lowest > 0.6, "{worst}° {lowest}");
        // In the gaps: the camera fixed on the vehicle, so its offset held is the way still
        let (worst, _) = travel_error(&d, &tr, theta0, drift, fps, &[(0.0, 20.0), (40.0, 100.0), (103.0, 230.0), (236.0, 400.0)]);
        assert!(worst < 4.0, "in the gaps: {worst}°");
    }

    #[test]
    fn one_fix_a_second_and_a_clock_two_seconds_off() {
        // Insta360's GPS: 1 Hz, and its clock placed on the video's by the metadata (here 2 s late)
        let d = drive(false);
        let (theta0, drift, lag) = (-160f64.to_radians(), 0.0, 2.0);
        let (acc, track, speeds) = sensors(&d, theta0, drift, lag, false);
        let (track, speeds) = thinned(&track, &speeds, &[], 10);
        let a = align(&acc, &track).unwrap();
        let err = ((a.theta_at(150.0) - theta0 + PI).rem_euclid(2.0 * PI) - PI).to_degrees();
        assert!(err.abs() < 2.5 && (a.lag - lag).abs() < 0.25 && a.confidence() > 0.6, "θ off by {err}° ({a:?})");
        let fps = 25.0;
        let frames = (299.0 * fps) as usize;
        let at = |t: f64| ((t / 0.01).round() as usize).min(d.t.len() - 1);
        let camera: Vec<f64> = (0..frames).map(|i| { let t = i as f64 / fps; d.azimuth[at(t)] - theta0 + PI }).collect();
        let tr = travel(&a, &track, &speeds, &camera, fps, |i| i as f64 / fps);
        // The last two seconds of video have no fixes after them (the GPS 2 s late): along the way fades out there
        let (worst, lowest) = travel_error(&d, &tr, theta0, drift, fps, &[(296.5, 300.0)]);
        assert!(worst < 5.0 && lowest > 0.5, "{worst}° {lowest}");
    }

    #[test]
    fn standing_still_the_whole_clip_is_no_fit() {
        // GPS fixes wandering by their noise around one spot, the accelerometer still: nothing to tie
        let mut noise = Noise(7);
        let track: Vec<(f64, [f64; 2])> = (0..1200).map(|k| (k as f64 * 0.1, [0.5 * noise.next(), 0.5 * noise.next()])).collect();
        let acc: Vec<(f64, Vector3<f64>)> = (0..12000).map(|k| (k as f64 * 0.01, Vector3::new(0.05 * noise.next(), -9.81, 0.05 * noise.next()))).collect();
        assert!(align(&acc, &track).is_none_or(|a| a.confidence() < 0.2));
    }

    #[test]
    fn north_is_found_whichever_way_the_camera_turns() {
        // North at world yaw 50°; the camera turning steadily and pitching a little; its MNOR in its own axes
        let fps = 30.0;
        let north_yaw = 50f64.to_radians();
        let to_north = ry(north_yaw) * Vector3::z();
        let cams: Vec<UnitQuaternion<f64>> = (0..900).map(|i| ry(i as f64 * 0.01) * rx(0.1 * (i as f64 / 50.0).sin())).collect();
        let seen: Vec<(Vector3<f64>, f64)> = cams.iter().enumerate().map(|(i, q)| (q.inverse() * to_north, if i < 450 { 1.0 } else { 0.5 })).collect();
        let n = north(&cams, &seen, fps);
        for (i, (yaw, c)) in n.iter().enumerate().step_by(37) {
            assert!(close(*yaw, north_yaw) || ((yaw - north_yaw + PI).rem_euclid(2.0 * PI) - PI).abs() < 1e-6, "{i}: {yaw}");
            assert!(*c > 0.45 && *c <= 1.0);
        }
    }
}
