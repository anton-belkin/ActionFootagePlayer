// SPDX-License-Identifier: GPL-3.0-or-later
//
// HyperSmooth's tilt in each frame, from how the picture bends under water (research_notes/dynamic_lens, part 2: hs_ba.py,
// ported). Behind a flat port, a turn HyperSmooth gives the picture inside the housing doesn't move the picture as a
// turn of the camera would: the port refracts about the body's axis, so a tilt of the picture bends it, more at the
// edges than at the centre (κ(θ) there). The bending is the one thing in the picture that tells HyperSmooth's tilt from
// the camera's own motion at every frequency: a slow turn of the picture looks the same as the diver drifting, the
// bending doesn't. It's the port's axis in_camera_turn.rs keeps beside a HERO7's turn (`PortAxis`): the refraction is
// about it, the picture's orientation is the optical correction's.
//
// How it was chosen (`rigidity`, the research's held-out check): each window fitted again with half the tracks and the
// tilt held, the other half placed in the world that gives, and how far they miss where they were seen. A picture that
// floats misses at the edges. On a HERO12 (GX010010) the IORI it recorded misses least (edges 0.52 px), the bending's
// axis next (0.59), the model's 0.66, the turn the optical correction fits 0.75, no tilt 1.21.
//
// Model (hs_ba.py, model A): a world point X seen in frame f is at the picture's ray
//     d = Rxy(φ_f)ᵀ · ρ(R_f X + T_f)
// R_f, T_f the body's pose (the port's), ρ water to air about the port's axis, φ_f HyperSmooth's tilt: its turn
// J = Rz · Rxy(φ) less the roll, which a round port doesn't see. Fitted as a bundle adjustment of the tracks Gyroflow's
// analysis followed: in windows of a few seconds, each frame's pose and tilt and each point's place, the lens fixed
// (the points' air rays from gyroflow-core's own undistortion) and the poses started from the gyro. Cauchy loss for
// what moves on its own (fish, divers); a window whose tilt comes out larger than HyperSmooth turns, or whose points
// don't fit, is left out (fuse2.py): there the model stands.
//
// Axes: the kernel's (x right, y down, z forward), what gyroflow-core's point undistortion gives; the motion data's
// are y up, z backward (F = diag(1, -1, -1) between them, `kernel_axes` in gyroflow-core).

use gyroflow_core::StabilizationManager;
use gyroflow_core::gyro_source::{ GyroSource, Quat64, TimeQuat };
use gyroflow_core::stabilization::{ FrameTransform, undistort_points_for_optical_motion };
use gyroflow_core::synchronization::optical_motion::{ self, OpticalMeasurements, PointState };
use nalgebra::{ DMatrix, DVector, Matrix2x3, Matrix3, Rotation3, SMatrix, SVector, Vector2, Vector3 };
use std::collections::HashMap;
use std::sync::Arc;

/// A window's length and how much of it the next one overlaps, s
pub const WINDOW_S: f64 = 3.0;
pub const OVERLAP_S: f64 = 1.0;
/// Tracks fitted per window, at most, and the fewest frames one must be seen in
const MAX_POINTS: usize = 800;
const MIN_LEN: usize = 10;
/// Cauchy scale, px of the clip
const LOSS_PX: f64 = 1.5;
/// The pull of the tilt to the model's (one σ, degrees): only so a frame nothing holds stays put
const PRIOR_TILT_DEG: f64 = 5.0;
/// The pull of the body's turn to the gyro's (one σ, degrees): the world's own turn is free otherwise
const PRIOR_TURN_DEG: f64 = 3.0;
const ITERATIONS: usize = 25;
/// A window is left out when its tilt's 95th percentile is more than HyperSmooth turns, degrees, or its points fit
/// worse than this, px at 4K width (fuse2.py: a shark across the frame, a selfie at the surface, open water)
const MAX_TILT_P95_DEG: f64 = 9.0;
const MAX_RMS_PX_4K: f64 = 3.5;
/// The points' first distance: the scale's arbitrary, this is about a dive's
const DEPTH: f64 = 3.0;
/// A frame counts with this many points fitted in it
const MIN_FRAME_POINTS: usize = 30;

/// What the bending measured in each frame of the clip (the analysis's frame numbers)
#[derive(Debug, Clone, Default)]
pub struct Tilts {
    /// The tilt φ and the roll about the picture's axis (kernel axes, radians), where they were measured
    pub frames: Vec<Option<Vector3<f64>>>,
    /// Per window: its first frame, frames, points, how its points fit (rms px), its tilt's 95th percentile (degrees),
    /// whether it counted
    pub windows: Vec<WindowFit>,
}

#[derive(Debug, Clone, Default)]
pub struct WindowFit {
    pub first: usize,
    pub frames: usize,
    pub points: usize,
    pub observations: usize,
    pub rms_px: f64,
    pub tilt_p95_deg: f64,
    pub iterations: usize,
    pub counted: bool,
}

impl Tilts {
    pub fn measured_share(&self) -> f64 {
        self.frames.iter().filter(|f| f.is_some()).count() as f64 / self.frames.len().max(1) as f64
    }
}

/// F: the kernel's axes from the motion data's and back
fn f_mat() -> Matrix3<f64> { Matrix3::from_diagonal(&Vector3::new(1.0, -1.0, -1.0)) }
pub fn to_kernel(r: &Rotation3<f64>) -> Rotation3<f64> { let f = f_mat(); Rotation3::from_matrix_unchecked(f * r.matrix() * f) }
pub fn from_kernel(r: &Rotation3<f64>) -> Rotation3<f64> { to_kernel(r) }

/// The tilt as a turn: about an axis in the picture's plane
pub fn tilt(phi: &Vector2<f64>) -> Rotation3<f64> { Rotation3::from_scaled_axis(Vector3::new(phi.x, phi.y, 0.0)) }

/// A turn J (kernel axes) as roll · tilt, `J = twist · Rxy(φ)`: φ turns the port's axis as the picture sees it (Jᵀz)
/// onto the picture's
pub fn split(j: &Rotation3<f64>) -> (Vector2<f64>, Rotation3<f64>) {
    let u = j.inverse() * Vector3::z();
    let swing = Rotation3::rotation_between(&u, &Vector3::z()).unwrap_or_else(Rotation3::identity);
    let a = swing.scaled_axis();
    (Vector2::new(a.x, a.y), j * swing.inverse())
}

/// Water to air about the port's axis (z): None past the edge of what the port lets through
fn water_to_air(u: &Vector3<f64>, n: f64) -> Option<Vector3<f64>> {
    let s = u.x.hypot(u.y);
    let sa = n * s;
    if sa >= 0.999 || u.z <= 0.0 { return None; }
    if s < 1e-12 { return Some(Vector3::z()); }
    let k = sa / s;
    Some(Vector3::new(u.x * k, u.y * k, (1.0 - sa * sa).sqrt()))
}
fn air_to_water(a: &Vector3<f64>, n: f64) -> Vector3<f64> {
    let a = a.normalize();
    let s = a.x.hypot(a.y);
    if s < 1e-12 { return Vector3::z(); }
    let sw = s / n;
    let k = sw / s;
    Vector3::new(a.x * k, a.y * k, (1.0 - sw * sw).sqrt() * a.z.signum())
}

/// An observation: a track's air ray in a frame (unit, kernel axes) and two directions across it
#[derive(Clone, Copy)]
struct Obs { frame: usize, d: Vector3<f64>, e1: Vector3<f64>, e2: Vector3<f64> }
impl Obs {
    fn new(frame: usize, d: Vector3<f64>) -> Self {
        let d = d.normalize();
        let helper = if d.x.abs() < 0.9 { Vector3::x() } else { Vector3::y() };
        let e1 = d.cross(&helper).normalize();
        let e2 = d.cross(&e1);
        Obs { frame, d, e1, e2 }
    }
    /// How far the ray `pred` is from this one, px (the gnomonic plane at it, times the focal length)
    fn residual(&self, pred: &Vector3<f64>, focal: f64) -> Option<Vector2<f64>> {
        let c = pred.dot(&self.d);
        if c < 0.2 { return None; }
        Some(Vector2::new(self.e1.dot(pred), self.e2.dot(pred)) * (focal / c))
    }
}

/// The picture's ray of the body's point `b`, the tilt `phi`
fn predict(b: &Vector3<f64>, phi: &Vector2<f64>, n: f64) -> Option<Vector3<f64>> {
    let len = b.norm();
    if !(len > 1e-9) { return None; }
    let a = water_to_air(&(b / len), n)?;
    Some(tilt(phi).inverse() * a)
}

/// A window's problem: frames, tracks and their observations
struct Window {
    first: usize,
    /// The picture's turn as the gyro and the model's roll have it (world to camera, kernel axes: the fit's start and
    /// pull), the body's alone, and the model's tilt, per frame
    rot0: Vec<Rotation3<f64>>,
    body: Vec<Rotation3<f64>>,
    prior: Vec<Vector2<f64>>,
    /// Per point, its observations
    points: Vec<Vec<Obs>>,
    /// The pulls of the tilt to `prior` and of the pose to `rot0` (one σ, radians)
    tilt_sigma: f64,
    turn_sigma: f64,
}

struct Solution {
    rot: Vec<Rotation3<f64>>,
    t: Vec<Vector3<f64>>,
    phi: Vec<Vector2<f64>>,
    x: Vec<Vector3<f64>>,
}

const NB: usize = 8;
type Mat8 = SMatrix<f64, 8, 8>;
type Mat8x3 = SMatrix<f64, 8, 3>;
type Vec8 = SVector<f64, 8>;

fn skew(v: &Vector3<f64>) -> Matrix3<f64> {
    Matrix3::new(0.0, -v.z, v.y, v.z, 0.0, -v.x, -v.y, v.x, 0.0)
}

impl Window {
    fn nf(&self) -> usize { self.rot0.len() }

    fn start(&self, n: f64) -> Solution {
        let x = self.points.iter().map(|obs| {
            let o = obs[0];
            let w = air_to_water(&(tilt(&self.prior[o.frame]) * o.d), n);
            self.rot0[o.frame].inverse() * (w * DEPTH)
        }).collect();
        Solution { rot: self.rot0.clone(), t: vec![Vector3::zeros(); self.nf()], phi: self.prior.clone(), x }
    }

    /// The robust cost (with the priors), and the residuals (None: the point is off the port)
    fn cost(&self, s: &Solution, n: f64, focal: f64) -> (f64, Vec<Vec<Option<Vector2<f64>>>>) {
        let c2 = LOSS_PX * LOSS_PX;
        let mut cost = 0.0;
        let res: Vec<Vec<Option<Vector2<f64>>>> = self.points.iter().enumerate().map(|(p, obs)| obs.iter().map(|o| {
            let b = s.rot[o.frame] * s.x[p] + s.t[o.frame];
            let r = predict(&b, &s.phi[o.frame], n).and_then(|d| o.residual(&d, focal));
            cost += c2 * (1.0 + r.map_or(100.0, |r| r.norm_squared() / c2)).ln();
            r
        }).collect()).collect();
        cost += self.prior_cost(s);
        (cost, res)
    }
    fn prior_cost(&self, s: &Solution) -> f64 {
        let (wp, wr) = (self.tilt_sigma.powi(-2), self.turn_sigma.powi(-2));
        (0..self.nf()).map(|f| {
            wp * (s.phi[f] - self.prior[f]).norm_squared() + wr * (s.rot[f] * self.rot0[f].inverse()).scaled_axis().norm_squared()
        }).sum()
    }

    /// Levenberg-Marquardt, the points eliminated (Schur): hs_ba.py's `fit` with the tilt free
    fn solve(&self, n: f64, focal: f64) -> (Solution, usize, f64) {
        let nf = self.nf();
        let np = self.points.len();
        let nc = NB * nf;
        let c2 = LOSS_PX * LOSS_PX;
        let (wp, wr) = (self.tilt_sigma.powi(-2), self.turn_sigma.powi(-2));
        let wt = 1.0 / 100.0;
        let mut s = self.start(n);
        let (mut c0, mut res) = self.cost(&s, n, focal);
        let mut lambda = 1e-3;
        let mut calm = 0;
        let mut it = 0;
        while it < ITERATIONS {
            it += 1;
            // Per observation: its Jacobians, d residual / d camera (ω, T, φ) and / d point
            let mut u = vec![Mat8::zeros(); nf];
            let mut gc = vec![Vec8::zeros(); nf];
            let mut v = vec![Matrix3::<f64>::zeros(); np];
            let mut gp = vec![Vector3::<f64>::zeros(); np];
            let mut wblocks: Vec<Vec<Mat8x3>> = Vec::with_capacity(np);
            for (p, obs) in self.points.iter().enumerate() {
                let mut wp_ = Vec::with_capacity(obs.len());
                for (k, o) in obs.iter().enumerate() {
                    let Some(r) = res[p][k] else { wp_.push(Mat8x3::zeros()); continue };
                    let f = o.frame;
                    let rx = s.rot[f] * s.x[p];
                    let b = rx + s.t[f];
                    let w = 1.0 / (1.0 + r.norm_squared() / c2);
                    // Numerically, as hs_ba.py: d r / d b and d r / d φ
                    let mut jb = Matrix2x3::<f64>::zeros();
                    let eps_b = 1e-6 * b.norm();
                    for j in 0..3 {
                        let mut bb = b; bb[j] += eps_b;
                        if let Some(rr) = predict(&bb, &s.phi[f], n).and_then(|d| o.residual(&d, focal)) { jb.set_column(j, &((rr - r) / eps_b)); }
                    }
                    let mut jphi = nalgebra::Matrix2::<f64>::zeros();
                    for j in 0..2 {
                        let mut ph = s.phi[f]; ph[j] += 1e-6;
                        if let Some(rr) = predict(&b, &ph, n).and_then(|d| o.residual(&d, focal)) { jphi.set_column(j, &((rr - r) / 1e-6)); }
                    }
                    let mut jc = SMatrix::<f64, 2, 8>::zeros();
                    jc.fixed_view_mut::<2, 3>(0, 0).copy_from(&(jb * -skew(&rx)));
                    jc.fixed_view_mut::<2, 3>(0, 3).copy_from(&jb);
                    jc.fixed_view_mut::<2, 2>(0, 6).copy_from(&jphi);
                    let jp = jb * s.rot[f].matrix();
                    u[f] += jc.transpose() * jc * w;
                    gc[f] -= jc.transpose() * r * w;
                    v[p] += jp.transpose() * jp * w;
                    gp[p] -= jp.transpose() * r * w;
                    wp_.push(jc.transpose() * jp * w);
                }
                wblocks.push(wp_);
            }
            for f in 0..nf {
                let dev = (s.rot[f] * self.rot0[f].inverse()).scaled_axis();
                for j in 0..3 {
                    u[f][(j, j)] += wr; gc[f][j] -= wr * dev[j];
                    u[f][(3 + j, 3 + j)] += wt; gc[f][3 + j] -= wt * s.t[f][j];
                }
                for j in 0..2 { u[f][(6 + j, 6 + j)] += wp; gc[f][6 + j] -= wp * (s.phi[f][j] - self.prior[f][j]); }
            }
            loop {
                // The reduced camera system
                let mut sm = DMatrix::<f64>::zeros(nc, nc);
                let mut rhs = DVector::<f64>::zeros(nc);
                for f in 0..nf {
                    let mut uf = u[f];
                    for j in 0..NB { uf[(j, j)] += lambda * u[f][(j, j)].max(1e-9) + 1e-9; }
                    sm.view_mut((NB * f, NB * f), (NB, NB)).copy_from(&uf);
                    rhs.rows_mut(NB * f, NB).copy_from(&gc[f]);
                }
                let vinv: Vec<Matrix3<f64>> = v.iter().map(|vp| {
                    let damp = lambda * (vp.trace() / 3.0).max(1e-9);
                    (vp + Matrix3::identity() * (damp + 1e-12)).try_inverse().unwrap_or_else(Matrix3::zeros)
                }).collect();
                for (p, obs) in self.points.iter().enumerate() {
                    let wv: Vec<Mat8x3> = wblocks[p].iter().map(|w| w * vinv[p]).collect();
                    for (a, oa) in obs.iter().enumerate() {
                        let fa = oa.frame;
                        let r = -(wv[a] * gp[p]);
                        let mut seg = rhs.rows_mut(NB * fa, NB);
                        seg += r;
                        for (bi, ob) in obs.iter().enumerate() {
                            let blk = wv[a] * wblocks[p][bi].transpose();
                            let mut view = sm.view_mut((NB * fa, NB * ob.frame), (NB, NB));
                            view -= blk;
                        }
                    }
                }
                let dc = match sm.clone().cholesky() {
                    Some(ch) => ch.solve(&rhs),
                    None => match sm.lu().solve(&rhs) { Some(x) => x, None => { lambda *= 10.0; if lambda > 1e8 { break; } continue; } },
                };
                let mut next = Solution { rot: s.rot.clone(), t: s.t.clone(), phi: s.phi.clone(), x: s.x.clone() };
                for f in 0..nf {
                    let d = dc.rows(NB * f, NB);
                    next.rot[f] = Rotation3::from_scaled_axis(Vector3::new(d[0], d[1], d[2])) * s.rot[f];
                    next.t[f] += Vector3::new(d[3], d[4], d[5]);
                    next.phi[f] += Vector2::new(d[6], d[7]);
                }
                for (p, obs) in self.points.iter().enumerate() {
                    let mut g = gp[p];
                    for (a, oa) in obs.iter().enumerate() {
                        let d: Vec8 = dc.fixed_rows::<8>(NB * oa.frame).into_owned();
                        g -= wblocks[p][a].transpose() * d;
                    }
                    next.x[p] += vinv[p] * g;
                }
                let (c1, r1) = self.cost(&next, n, focal);
                if c1 < c0 {
                    let rel = (c0 - c1) / c0;
                    s = next; c0 = c1; res = r1;
                    lambda = (lambda / 4.0).max(1e-7);
                    calm = if rel < 2e-5 { calm + 1 } else { 0 };
                    break;
                }
                lambda *= 5.0;
                if lambda > 1e8 { calm = 99; break; }
            }
            if calm >= 2 && it >= 3 { break; }
        }
        // How the points fit: rms of the residuals, each held to 6 px (as hs_ba.py's check)
        let (mut sum, mut count) = (0.0, 0usize);
        for r in res.iter().flatten().flatten() { sum += r.norm_squared().min(36.0); count += 1; }
        (s, it, (sum / count.max(1) as f64).sqrt())
    }
}

/// The tracks of the analysis, per frame: (track, air ray in kernel axes, where it is relative to the frame)
type FrameRays = Vec<Vec<(u32, Vector3<f64>, (f32, f32))>>;

fn rays_of(stab: &StabilizationManager, m: &OpticalMeasurements) -> (FrameRays, f64) {
    // Through the lens alone: no water, no turn (the model's refraction is the fit's own)
    let mut params = optical_motion::measurement_params(stab);
    params.light_refraction_coefficient = 1.0;
    let (w, h) = (params.width as f32, params.height as f32);
    let focal = FrameTransform::get_lens_data_at_timestamp(&params, 0.0, false).0[(0, 0)];
    let points: Vec<Vec<(u32, f32, f32)>> = (0..m.frames).map(|f| m.points_at(f).into_iter()
        .filter(|p| !matches!(p.3, PointState::Removed))
        .map(|p| (p.0, p.1, p.2)).collect()).collect();
    let threads = std::thread::available_parallelism().map_or(4, |n| n.get());
    let chunk = points.len().div_ceil(threads).max(1);
    let params = &params;
    let rays: FrameRays = std::thread::scope(|sc| {
        let handles: Vec<_> = points.chunks(chunk).enumerate().map(|(c, frames)| sc.spawn(move || {
            frames.iter().enumerate().map(|(i, pts)| {
                let f = c * chunk + i;
                if pts.is_empty() { return Vec::new(); }
                let ts = params.timestamp_at_frame(f as i32);
                let px: Vec<(f32, f32)> = pts.iter().map(|p| (p.1 * w, p.2 * h)).collect();
                let r = undistort_points_for_optical_motion(&px, ts, f, params, (w as u32, h as u32));
                pts.iter().zip(r).filter_map(|(p, r)| r.map(|r| (p.0, Vector3::new(r.0 as f64, r.1 as f64, r.2 as f64), (p.1, p.2)))).collect()
            }).collect::<Vec<_>>()
        })).collect();
        handles.into_iter().flat_map(|h| h.join().unwrap()).collect()
    });
    (rays, focal)
}

/// What every fit of a clip's windows starts from: the tracks' rays, the body's turn, the model's tilt and roll, the windows
struct Setup {
    rays: FrameRays,
    focal: f64,
    nf: usize,
    width: f64,
    body: Vec<Rotation3<f64>>,
    prior: Vec<Vector2<f64>>,
    rot0: Vec<Rotation3<f64>>,
    len: usize,
    step: usize,
    starts: Vec<usize>,
}

/// A track in a window: frames in it, air rays, where it is relative to the frame
type WindowTrack = Vec<(usize, Vector3<f64>, (f32, f32))>;

impl Setup {
    fn new(stab: &StabilizationManager, m: &OpticalMeasurements, gyro: &GyroSource, model: &TimeQuat) -> Self {
        let (rays, focal) = rays_of(stab, m);
        let nf = rays.len();
        let fps = stab.params.read().fps.max(1.0);
        let width = stab.params.read().size.0.max(1) as f64;
        let mut model_gyro = gyro.clone();
        model_gyro.quaternions = Arc::new(model.clone());
        let ts = |f: usize| gyroflow_core::timestamp_at_frame(f as i32, fps);
        // The body's turn, world to camera in kernel axes, and the model's tilt
        let body: Vec<Rotation3<f64>> = (0..nf).map(|f| {
            let b = gyro.org_quat_at_timestamp(ts(f)).to_rotation_matrix();
            let fm = f_mat();
            Rotation3::from_matrix_unchecked(fm * b.matrix().transpose())
        }).collect();
        // The fit's pose is the picture's less the tilt: d = Rxyᵀ ρ(Rzᵀ b), its roll Rz = body · poseᵀ
        let split_model: Vec<(Vector2<f64>, Rotation3<f64>)> = (0..nf).map(|f| split(&to_kernel(&model_gyro.org_quat_at_timestamp(ts(f)).to_rotation_matrix()))).collect();
        let prior: Vec<Vector2<f64>> = split_model.iter().map(|s| s.0).collect();
        let rot0: Vec<Rotation3<f64>> = (0..nf).map(|f| split_model[f].1.inverse() * body[f]).collect();

        let len = ((WINDOW_S * fps).round() as usize).max(MIN_LEN * 3);
        let step = (len - ((OVERLAP_S * fps).round() as usize).min(len / 2)).max(1);
        // Windows `step` apart, the last one ending at the clip's end
        let mut starts = Vec::new();
        let mut s = 0;
        loop {
            if s + len >= nf { let last = nf.saturating_sub(len); if starts.last() != Some(&last) { starts.push(last); } break; }
            starts.push(s);
            s += step;
        }
        Setup { rays, focal, nf, width, body, prior, rot0, len, step, starts }
    }

    /// The tracks seen in the window from `first` for at least MIN_LEN frames, of those `keep` keeps, longest first
    fn tracks(&self, first: usize, keep: &dyn Fn(u32) -> bool) -> Vec<WindowTrack> {
        let end = (first + self.len).min(self.nf);
        let mut tracks: HashMap<u32, WindowTrack> = HashMap::new();
        for f in first..end { for &(id, d, pos) in &self.rays[f] { if keep(id) { tracks.entry(id).or_default().push((f - first, d, pos)); } } }
        let mut long: Vec<(u32, WindowTrack)> = tracks.into_iter().filter(|(_, o)| o.len() >= MIN_LEN).collect();
        long.sort_by_key(|(id, o)| (std::cmp::Reverse(o.len()), *id));
        long.into_iter().map(|(_, o)| o).collect()
    }

    /// The window from `first`: the tracks `keep` keeps, spread over the frame and the window, the tilt pulled to `prior`
    fn window(&self, first: usize, keep: &dyn Fn(u32) -> bool, prior: &[Vector2<f64>]) -> Window {
        let end = (first + self.len).min(self.nf);
        let long = self.tracks(first, keep);
        // Spread over the frame and the window (hs_ba.py): the longest of each cell first, round by round
        let mut seen: HashMap<(usize, usize, usize), usize> = HashMap::new();
        let mut ranked: Vec<(usize, usize)> = long.iter().enumerate().map(|(i, o)| {
            let k = o.len() as f32;
            let (mx, my) = o.iter().fold((0f32, 0f32), |a, b| (a.0 + b.2.0 / k, a.1 + b.2.1 / k));
            let cell = (o[0].0 / 12, ((mx * 6.0) as usize).min(5), ((my * 4.0) as usize).min(3));
            let r = seen.entry(cell).or_default(); *r += 1;
            (*r, i)
        }).collect();
        ranked.sort();
        let points = ranked.into_iter().take(MAX_POINTS).map(|(_, i)| long[i].iter().map(|(f, d, _)| Obs::new(*f, *d)).collect()).collect();
        Window { first, rot0: self.rot0[first..end].to_vec(), body: self.body[first..end].to_vec(), prior: prior[first..end].to_vec(), points,
            tilt_sigma: PRIOR_TILT_DEG.to_radians(), turn_sigma: PRIOR_TURN_DEG.to_radians() }
    }
}

/// Windows solved in parallel, a batch at a time (asked whether to stop between batches)
fn solve_all<T: Send>(windows: &[Window], cancelled: &dyn Fn() -> bool, each: impl Fn(&Window) -> T + Sync) -> Result<Vec<T>, String> {
    let threads = std::thread::available_parallelism().map_or(4, |n| n.get());
    let mut out = Vec::with_capacity(windows.len());
    for batch in windows.chunks(threads) {
        if cancelled() { return Err("cancelled".into()); }
        let each = &each;
        let done: Vec<T> = std::thread::scope(|sc| {
            let hs: Vec<_> = batch.iter().map(|w| sc.spawn(move || each(w))).collect();
            hs.into_iter().map(|h| h.join().unwrap()).collect()
        });
        out.extend(done);
    }
    Ok(out)
}

/// HyperSmooth's tilt in each frame from the bending, the turn `model` (motion data's axes, as `in_camera_turn::model`)
/// the prior and the start, the gyro `gyro` the bodies' start. `n` the water's index; the tracks `keep` keeps. None for
/// frames it can't say
pub fn measure(stab: &StabilizationManager, m: &OpticalMeasurements, gyro: &GyroSource, model: &TimeQuat, n: f64, cancelled: &dyn Fn() -> bool, keep: &dyn Fn(u32) -> bool) -> Result<Tilts, String> {
    let su = Setup::new(stab, m, gyro, model);
    let (nf, focal, width, len, step) = (su.nf, su.focal, su.width, su.len, su.step);
    let windows: Vec<Window> = su.starts.iter().map(|&first| su.window(first, keep, &su.prior)).collect();

    let fits: Vec<(Solution, WindowFit)> = solve_all(&windows, cancelled, |w| {
        let (s, iterations, rms_px) = w.solve(n, focal);
        let mut tilts: Vec<f64> = s.phi.iter().map(|p| p.norm().to_degrees()).collect();
        tilts.sort_by(|a, b| a.total_cmp(b));
        let tilt_p95_deg = tilts.get(tilts.len() * 95 / 100).copied().unwrap_or_default();
        let observations = w.points.iter().map(|o| o.len()).sum();
        let counted = tilt_p95_deg <= MAX_TILT_P95_DEG && rms_px * 3840.0 / width <= MAX_RMS_PX_4K && w.points.len() >= MIN_FRAME_POINTS;
        (s, WindowFit { first: w.first, frames: w.nf(), points: w.points.len(), observations, rms_px, tilt_p95_deg, iterations, counted })
    })?;

    // The windows blended where they overlap: each frame's tilt weighted by how far it is into its window
    let mut sum = vec![Vector3::<f64>::zeros(); nf];
    let mut weight = vec![0f64; nf];
    for ((s, fit), w) in fits.iter().zip(&windows) {
        if !fit.counted { continue; }
        let mut per_frame = vec![0usize; w.nf()];
        for obs in &w.points { for o in obs { per_frame[o.frame] += 1; } }
        let ramp = (len - step).max(1) as f64;
        for i in 0..w.nf() {
            if per_frame[i] < MIN_FRAME_POINTS { continue; }
            let edge = (i.min(w.nf() - 1 - i) as f64 + 1.0) / ramp;
            let k = edge.min(1.0);
            let roll = (w.body[i] * s.rot[i].inverse()).scaled_axis().z;
            sum[w.first + i] += Vector3::new(s.phi[i].x, s.phi[i].y, roll) * k;
            weight[w.first + i] += k;
        }
    }
    let frames = (0..nf).map(|f| (weight[f] > 0.0).then(|| sum[f] / weight[f])).collect();
    Ok(Tilts { frames, windows: fits.into_iter().map(|(_, w)| w).collect() })
}

/// How rigid the picture stays with a given tilt in each frame, on tracks the fit didn't see (the research's held-out check,
/// hs_ba.py): each window fitted with the tracks `fit` keeps, the tilt held at the given one (None: the model's), the
/// poses and the points free (so the camera's turn and its drift both); then each other track placed in that world by
/// the fitted poses alone, and how far it is from where it was seen, px of the clip at the centre's scale. A picture
/// that "floats" (bends in a way no rigid world and camera explain) misses there; what moves on its own misses too,
/// in every variant alike (medians). Per zone of the frame: centre, middle, edges (as `how_still_the_world_stays`)
#[derive(Debug, Clone, Default)]
pub struct Rigidity {
    pub median_px: [f64; 3],
    pub p75_px: [f64; 3],
    pub observations: [usize; 3],
    pub fit_rms_px: f64,
    /// Per window: its first frame and the median miss at the edges
    pub windows: Vec<(usize, f64)>,
}

pub fn rigidity(stab: &StabilizationManager, m: &OpticalMeasurements, gyro: &GyroSource, model: &TimeQuat, n: f64, tilts: &[Option<Vector2<f64>>], fit: &(dyn Fn(u32) -> bool + Sync), cancelled: &dyn Fn() -> bool) -> Result<Rigidity, String> {
    let su = Setup::new(stab, m, gyro, model);
    let prior: Vec<Vector2<f64>> = (0..su.nf).map(|f| tilts.get(f).copied().flatten().unwrap_or(su.prior[f])).collect();
    let windows: Vec<Window> = su.starts.iter().map(|&first| {
        let mut w = su.window(first, fit, &prior);
        // The tilt held; the pose only kept from wandering off as a whole (the world's turn is free otherwise)
        w.tilt_sigma = 1e-5;
        w.turn_sigma = 20f64.to_radians();
        w
    }).collect();
    let focal = su.focal;
    let per_window: Vec<([Vec<f64>; 3], f64)> = solve_all(&windows, cancelled, |w| {
        let (s, _, rms) = w.solve(n, focal);
        let mut zones: [Vec<f64>; 3] = Default::default();
        for track in su.tracks(w.first, &|id| !fit(id)) {
            let obs: Vec<Obs> = track.iter().map(|(f, d, _)| Obs::new(*f, *d)).collect();
            let Some(x) = triangulate(&s, &obs, n, focal) else { continue };
            for ((_, _, pos), o) in track.iter().zip(&obs) {
                let b = s.rot[o.frame] * x + s.t[o.frame];
                let Some(r) = predict(&b, &s.phi[o.frame], n).and_then(|d| o.residual(&d, focal)) else { continue };
                let rr = ((pos.0 as f64 - 0.5) / 0.5).hypot((pos.1 as f64 - 0.5) / 0.5) / 2f64.sqrt();
                zones[if rr < 0.3 { 0 } else if rr < 0.7 { 1 } else { 2 }].push(r.norm());
            }
        }
        (zones, rms)
    })?;
    let mut all: [Vec<f64>; 3] = Default::default();
    let mut rms = 0.0;
    for (z, r) in &per_window { for i in 0..3 { all[i].extend(&z[i]); } rms += r / per_window.len() as f64; }
    let med = |v: &[f64]| { let mut v = v.to_vec(); v.sort_by(|a, b| a.total_cmp(b)); v.get(v.len() / 2).copied().unwrap_or(f64::NAN) };
    let mut out = Rigidity { fit_rms_px: rms, windows: windows.iter().zip(&per_window).map(|(w, (z, _))| (w.first, med(&z[2]))).collect(), ..Default::default() };
    for i in 0..3 {
        let v = &mut all[i];
        v.sort_by(|a, b| a.total_cmp(b));
        out.observations[i] = v.len();
        if v.is_empty() { out.median_px[i] = f64::NAN; out.p75_px[i] = f64::NAN; continue; }
        out.median_px[i] = v[v.len() / 2];
        out.p75_px[i] = v[v.len() * 3 / 4];
    }
    Ok(out)
}

/// A point placed by the poses and tilts of `s` from its observations: Levenberg-Marquardt with the Cauchy loss, from a
/// few depths along its first ray. None when it doesn't settle in front of the camera
fn triangulate(s: &Solution, obs: &[Obs], n: f64, focal: f64) -> Option<Vector3<f64>> {
    let c2 = LOSS_PX * LOSS_PX;
    let cost = |x: &Vector3<f64>| -> f64 {
        obs.iter().map(|o| {
            let b = s.rot[o.frame] * x + s.t[o.frame];
            let r = predict(&b, &s.phi[o.frame], n).and_then(|d| o.residual(&d, focal));
            c2 * (1.0 + r.map_or(100.0, |r| r.norm_squared() / c2)).ln()
        }).sum()
    };
    let o0 = obs[0];
    let ray = air_to_water(&(tilt(&s.phi[o0.frame]) * o0.d), n);
    let mut best: Option<(f64, Vector3<f64>)> = None;
    for depth in [0.5, 1.5, 3.0, 8.0, 25.0] {
        let mut x = s.rot[o0.frame].inverse() * (ray * depth - s.t[o0.frame]);
        let mut c = cost(&x);
        let mut lambda = 1e-3;
        for _ in 0..20 {
            let (mut h, mut g) = (Matrix3::<f64>::zeros(), Vector3::<f64>::zeros());
            for o in obs {
                let b = s.rot[o.frame] * x + s.t[o.frame];
                let Some(r) = predict(&b, &s.phi[o.frame], n).and_then(|d| o.residual(&d, focal)) else { continue };
                let w = 1.0 / (1.0 + r.norm_squared() / c2);
                let mut jb = Matrix2x3::<f64>::zeros();
                let eps = 1e-6 * b.norm().max(1e-3);
                for j in 0..3 {
                    let mut bb = b; bb[j] += eps;
                    if let Some(rr) = predict(&bb, &s.phi[o.frame], n).and_then(|d| o.residual(&d, focal)) { jb.set_column(j, &((rr - r) / eps)); }
                }
                let jp = jb * s.rot[o.frame].matrix();
                h += jp.transpose() * jp * w;
                g -= jp.transpose() * r * w;
            }
            let mut stepped = false;
            while lambda < 1e8 {
                let mut hd = h;
                for j in 0..3 { hd[(j, j)] += lambda * h[(j, j)].max(1e-12) + 1e-12; }
                let Some(dx) = hd.try_inverse().map(|i| i * g) else { lambda *= 10.0; continue };
                let nx = x + dx;
                let nc = cost(&nx);
                if nc < c { x = nx; c = nc; lambda = (lambda / 4.0).max(1e-9); stepped = true; break; }
                lambda *= 10.0;
            }
            if !stepped { break; }
        }
        if best.is_none_or(|(bc, _)| c < bc) { best = Some((c, x)); }
    }
    let (_, x) = best?;
    // In front of the camera in the frames it was seen in
    obs.iter().all(|o| (s.rot[o.frame] * x + s.t[o.frame]).z > 0.0).then_some(x)
}

/// HyperSmooth's tilt in each frame from turns J (rotation vectors, motion data's axes; a HERO12's own IORI, say)
pub fn tilts_of_turns(turns: &[Vector3<f64>]) -> Vec<Option<Vector2<f64>>> {
    turns.iter().map(|v| Some(split(&to_kernel(&Rotation3::from_scaled_axis(*v))).0)).collect()
}

/// The tilt and the roll a turn has at each frame (kernel axes), as `measure` measures them
pub fn model_tilts(gyro: &GyroSource, model: &TimeQuat, frames: usize, fps: f64) -> Vec<Vector3<f64>> {
    let mut g = gyro.clone();
    g.quaternions = Arc::new(model.clone());
    (0..frames).map(|f| tilt_and_roll(&to_kernel(&g.org_quat_at_timestamp(gyroflow_core::timestamp_at_frame(f as i32, fps)).to_rotation_matrix()))).collect()
}

/// `split`, the roll as its angle about the picture's axis
pub fn tilt_and_roll(j: &Rotation3<f64>) -> Vector3<f64> {
    let (phi, twist) = split(j);
    Vector3::new(phi.x, phi.y, twist.scaled_axis().z)
}
/// Where two disjoint halves of the tracks agree on the tilt: their difference averaged over this, s, at most this,
/// degrees. Each half has half the points, so they differ by more than the whole would from the truth: GX010564 1-2°
/// where the scene is rigid, 2-5° where a fish or the open water is; 2.5° keeps 90 % of GX010010's and 80 % of
/// GX010564's frames and drops the stretches the held-out tracks miss most in
const AGREE_S: f64 = 0.5;
const AGREE_DEG: f64 = 2.5;

/// The tilt from the tracks `keep` keeps, taken only where two halves of them, measured apart, agree on it: a stretch the
/// bending can't pin down (a fish across the frame, the open water) gives the two different tilts
pub fn measure_agreed(stab: &StabilizationManager, m: &OpticalMeasurements, gyro: &GyroSource, model: &TimeQuat, n: f64, cancelled: &dyn Fn() -> bool, keep: &(dyn Fn(u32) -> bool + Sync)) -> Result<Tilts, String> {
    let whole = measure(stab, m, gyro, model, n, cancelled, keep)?;
    let a = measure(stab, m, gyro, model, n, cancelled, &|id| keep(id) && (id >> 1) & 1 == 0)?;
    let b = measure(stab, m, gyro, model, n, cancelled, &|id| keep(id) && (id >> 1) & 1 == 1)?;
    let nf = a.frames.len().min(b.frames.len());
    let fps = stab.params.read().fps.max(1.0);
    let reach = (AGREE_S * fps / 2.0).round() as usize;
    let apart: Vec<Option<f64>> = (0..nf).map(|f| match (a.frames[f], b.frames[f]) { (Some(x), Some(y)) => Some((x.xy() - y.xy()).norm()), _ => None }).collect();
    let agree = AGREE_DEG.to_radians();
    let frames = (0..nf.min(whole.frames.len())).map(|f| {
        let near: Vec<f64> = apart[f.saturating_sub(reach)..(f + reach + 1).min(nf)].iter().flatten().copied().collect();
        // Where one half couldn't say (its window left out), the whole's own checks stand
        if near.is_empty() { return whole.frames[f]; }
        let mean = near.iter().sum::<f64>() / near.len() as f64;
        if mean <= agree { whole.frames[f] } else { None }
    }).collect();
    let mut windows = whole.windows;
    windows.extend(a.windows);
    windows.extend(b.windows);
    Ok(Tilts { frames, windows })
}

/// The port's axis in each frame: the model's tilt, with what the bending measured in its place where it measured it
/// (frame by frame; or the difference smoothed over `smooth_s`, s, fading back to the model's where under a fifth of
/// that was measured)
pub fn port_axis(model: &[Vector2<f64>], measured: &[Option<Vector3<f64>>], fps: f64, smooth_s: f64) -> Vec<Vector2<f64>> {
    let nf = model.len().min(measured.len());
    if smooth_s <= 0.0 { return (0..nf).map(|f| measured[f].map_or(model[f], |p| p.xy())).collect(); }
    let sigma = smooth_s * fps;
    let reach = (3.0 * sigma).ceil() as isize;
    let kernel: Vec<f64> = (-reach..=reach).map(|d| (-0.5 * (d as f64 / sigma).powi(2)).exp()).collect();
    let full: f64 = kernel.iter().sum();
    (0..nf as isize).map(|f| {
        let (mut num, mut den) = (Vector2::zeros(), 0.0);
        for (k, g) in (-reach..=reach).zip(&kernel) {
            let j = f + k;
            if j < 0 || j >= nf as isize { continue; }
            let Some(phi) = measured[j as usize] else { continue };
            num += (phi.xy() - model[j as usize]) * *g;
            den += g;
        }
        let d = if den > 0.0 { num / den * (den / (0.2 * full)).min(1.0) } else { Vector2::zeros() };
        model[f as usize] + d
    }).collect()
}

/// The turn the port's axis `phi` stands for (kernel axes) as gyroflow-core takes image orientations: motion data's axes.
/// Only its tilt matters to the refraction, which is the same all round the port's axis
pub fn axis_turn(phi: &Vector2<f64>) -> Quat64 {
    Quat64::from_rotation_matrix(&from_kernel(&tilt(phi)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_port_axis_turn_is_its_tilt() {
        for phi in [Vector2::new(0.03, -0.02), Vector2::new(-0.1, 0.07), Vector2::zeros()] {
            let back = split(&to_kernel(&axis_turn(&phi).to_rotation_matrix())).0;
            assert!((back - phi).norm() < 1e-12, "{phi} -> {back}");
        }
    }

    #[test]
    fn the_bending_stands_where_it_measured_and_the_model_elsewhere() {
        let model: Vec<Vector2<f64>> = (0..100).map(|f| Vector2::new(0.01 * (f as f64 * 0.1).sin(), 0.0)).collect();
        let measured: Vec<Option<Vector3<f64>>> = (0..100).map(|f| (30..70).contains(&f).then(|| Vector3::new(0.02, -0.01, 0.3))).collect();
        let axis = port_axis(&model, &measured, 30.0, 0.0);
        for f in 30..70 { assert!((axis[f] - Vector2::new(0.02, -0.01)).norm() < 1e-9, "{f}"); }
        for f in (0..30).chain(70..100) { assert!((axis[f] - model[f]).norm() < 1e-9, "{f}"); }
    }

    #[test]
    fn a_turn_is_its_roll_and_its_tilt() {
        let j = Rotation3::from_scaled_axis(Vector3::new(0.03, -0.02, 0.05));
        let (phi, twist) = split(&j);
        let back = twist * tilt(&phi);
        assert!(back.angle_to(&j) < 1e-12);
        // The roll is about the picture's axis, and the tilt moves the port's axis onto it
        let a = twist.scaled_axis();
        assert!(a.x.abs() < 1e-12 && a.y.abs() < 1e-12, "{a:?}");
        assert!((tilt(&phi) * (j.inverse() * Vector3::z()) - Vector3::z()).norm() < 1e-12);
    }

    #[test]
    fn the_refraction_goes_both_ways() {
        let w = Vector3::new(0.3, -0.2, 0.9).normalize();
        let a = water_to_air(&w, 1.33).unwrap();
        assert!((air_to_water(&a, 1.33) - w).norm() < 1e-12);
    }

    /// A made-up dive: points in front of a camera that turns and drifts, the picture tilted by a known HyperSmooth
    /// tilt; the fit, started from a tilt of 0 and the body's turn a little off, gets the tilt back
    #[test]
    fn the_bending_gives_the_tilt_back() {
        let n = 1.33;
        let focal = 1800.0;
        let nf = 40;
        let mut rng = 12345u64;
        let mut rand = || { rng = rng.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407); ((rng >> 11) as f64) / ((1u64 << 53) as f64) };
        let truth_phi: Vec<Vector2<f64>> = (0..nf).map(|f| { let t = f as f64 / 30.0; Vector2::new(0.015 * (2.0 * t).sin(), 0.02 * (1.3 * t + 0.5).cos()) }).collect();
        let truth_roll: Vec<f64> = (0..nf).map(|f| 0.03 * (2.5 * f as f64 / 30.0).sin()).collect();
        let roll_of = |f: usize| Rotation3::from_scaled_axis(Vector3::new(0.0, 0.0, truth_roll[f]));
        let rot: Vec<Rotation3<f64>> = (0..nf).map(|f| { let t = f as f64 / 30.0; Rotation3::from_scaled_axis(Vector3::new(0.05 * t, 0.1 * (t * 1.7).sin(), 0.02 * t)) }).collect();
        let trans: Vec<Vector3<f64>> = (0..nf).map(|f| { let t = f as f64 / 30.0; Vector3::new(0.3 * t, -0.1 * t, 0.05 * t) }).collect();
        let mut points = Vec::new();
        for _ in 0..300 {
            // A point in front, from 1.5 to 6 m, anywhere in the picture
            let dir = Vector3::new(rand() * 1.6 - 0.8, rand() * 1.0 - 0.5, 1.0).normalize();
            let x = rot[0].inverse() * (dir * (1.5 + 4.5 * rand()) - trans[0]);
            let mut obs = Vec::new();
            for f in 0..nf {
                let b = rot[f] * x + trans[f];
                let Some(d) = predict(&(roll_of(f).inverse() * b), &truth_phi[f], n) else { continue };
                // Half a pixel of noise
                let o = Obs::new(f, d);
                let d = (d + (o.e1 * (rand() - 0.5) + o.e2 * (rand() - 0.5)) * (1.0 / focal)).normalize();
                obs.push(Obs::new(f, d));
            }
            if obs.len() >= MIN_LEN { points.push(obs); }
        }
        let off = Rotation3::from_scaled_axis(Vector3::new(0.004, -0.003, 0.002));
        let w = Window { first: 0, rot0: rot.iter().map(|r| off * r).collect(), body: rot.clone(), prior: vec![Vector2::zeros(); nf], points, tilt_sigma: PRIOR_TILT_DEG.to_radians(), turn_sigma: PRIOR_TURN_DEG.to_radians() };
        let (s, it, rms) = w.solve(n, focal);
        let err = (0..nf).map(|f| (s.phi[f] - truth_phi[f]).norm_squared()).sum::<f64>().sqrt() / (nf as f64).sqrt();
        let truth_rms = truth_phi.iter().map(|p| p.norm_squared()).sum::<f64>().sqrt() / (nf as f64).sqrt();
        eprintln!("{it} iterations, fit {rms:.2} px, tilt {:.3}° rms, off by {:.4}° rms", truth_rms.to_degrees(), err.to_degrees());
        assert!(err.to_degrees() < 0.1, "the tilt came back {:.3}° off", err.to_degrees());
        // The roll, from the poses against the body's: the same but for a constant (the world's own turn is free)
        let roll: Vec<f64> = (0..nf).map(|f| (w.body[f] * s.rot[f].inverse()).scaled_axis().z - truth_roll[f]).collect();
        let mean = roll.iter().sum::<f64>() / nf as f64;
        let roll_err = (roll.iter().map(|r| (r - mean).powi(2)).sum::<f64>() / nf as f64).sqrt();
        eprintln!("roll {:.3}° rms, off by {:.4}° rms and {:.3}° as a whole", (truth_roll.iter().map(|r| r * r).sum::<f64>() / nf as f64).sqrt().to_degrees(), roll_err.to_degrees(), mean.to_degrees());
        assert!(roll_err.to_degrees() < 0.05, "the roll came back {:.3}° off", roll_err.to_degrees());
    }
}
