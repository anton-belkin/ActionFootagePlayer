// SPDX-License-Identifier: GPL-3.0-or-later
//
// The trail on a sphere of directions: what was last seen in each direction of the world, rather than at each place
// of the screen. The screen trail is only right while the smoothed camera holds still; when it pans, what's stored
// at a place of the screen was in another direction. Here the coarse stabilized frame is written to a cube map by
// the direction each of its cells looks in, and the fill reads it back by the directions the uncovered cells look in.
//
// The direction of an output position is Gyroflow's kernel geometry (undistort_coord, rotate_and_distort in
// wgpu_undistort.wgsl), less the lens: the output projection turns the position into a ray of the output camera,
// and the smoothed orientation turns that into the world. The kernel takes the ray to the source camera of each row
// with `rᵀ`, where `r = D·Rimg·R(Sm⁻¹·O_row)·D` (FrameTransform::at_timestamp flips the axes after the video
// rotation; Sm the smoothed orientation, O the camera's, D the axis flip between camera and gyro). The source ray's
// direction in the world is `O_row · D · s`, so the output ray's is `Sm · D · Rimg · d`, the same for every row: the
// rolling shutter is already undone in the stabilized frame. Lens correction below 100 % bends the output rays a
// little off the projection; writing and reading go through the same mapping, so that mostly cancels. The tests
// below hold this against Gyroflow's own CPU paths.
//
// Only rotation is known, so near things that moved relative to the camera (parallax) or by themselves are off,
// as on the screen trail; it fades the same way.
//
// The texels are atomics, read and written without a lock: in playback the sphere is written on a thread of its own
// (edges.rs) while the frames read it. A frame may read a texel half written, or a weight a moment older than its
// value; under the blur of the fill that's nothing to see.

use std::sync::atomic::{ AtomicU32, Ordering };
use nalgebra::{ Matrix3, Vector3 };
use gyroflow_core::keyframes::KeyframeType;
use gyroflow_core::stabilization::{ ComputeParams, FrameTransform };
use gyroflow_core::stabilization::projection::OutputProjection;

/// How much wider than the source, unzoomed, the sphere's own view is
pub const WIDE: f64 = 1.6;

/// The parameters the sphere is written by: the smoothed camera as the output has it, but at a fixed field of view
/// `WIDE` times the source's and centered, so the source frame lands in it whole however the camera shook, and what
/// the output's zoom crops away is kept too. On a grid `WIDE` times the output's, it's about as fine as that one
/// unzoomed. Equidistant, so the cells are as fine at the sides as in the middle: rectilinear stretches the sides
/// as the tangent, and a fisheye's frame with some shake on top doesn't fit in it at any sensible size. Fully lens
/// corrected, so the view's directions are exact
pub fn wide_params(mut params: ComputeParams) -> ComputeParams {
    params.fovs.clear();
    params.fov_scale = WIDE;
    params.fov_overview = false;
    params.output_projection = OutputProjection::Equidistant as i32;
    params.lens_correction_amount = 1.0;
    params.adaptive_zoom_center_offset = (0.0, 0.0);
    for key in [KeyframeType::Fov, KeyframeType::ZoomingCenterX, KeyframeType::ZoomingCenterY] {
        params.keyframes.clear_type(&key);
    }
    params
}

/// One frame's output positions and directions in the world. Positions are normalized: the kernel renders pixel `i`
/// of a buffer `w` wide at `i / w` of the output (its compute path maps the pixel's index, not its middle), so a
/// grid cell is at `(i / gw, j / gh)`
pub struct View {
    /// World from the output camera
    rot: Matrix3<f32>,
    /// Output position (in the kernel's output pixels) to the normalized plane: (pos - c) / f
    f: (f32, f32),
    c: (f32, f32),
    size: (f32, f32),
    projection: i32,
}

impl View {
    pub fn at(params: &ComputeParams, timestamp_us: i64) -> View {
        let timestamp_ms = timestamp_us as f64 / 1000.0;
        let frame = gyroflow_core::frame_at_timestamp(timestamp_ms, params.scaled_fps).max(0) as usize;
        let kp = FrameTransform::at_timestamp(params, timestamp_ms, frame).kernel_params;
        let stretch = if kp.input_horizontal_stretch > 0.01 { 1.0 / kp.input_horizontal_stretch } else { 1.0 };
        let size = (params.output_width as f32, params.output_height as f32);

        let sm = {
            let gyro = params.gyro.read();
            gyro.org_quat_at_timestamp(timestamp_ms) * gyro.smoothed_quat_at_timestamp(timestamp_ms).inverse()
        };
        let d = if params.framebuffer_inverted { Matrix3::from_diagonal(&Vector3::new(1.0, 1.0, -1.0)) }
                else { Matrix3::from_diagonal(&Vector3::new(1.0, -1.0, -1.0)) };
        let image_rotation = Matrix3::new_rotation(params.video_rotation.to_radians());
        let rot = sm.to_rotation_matrix().matrix() * d * image_rotation;
        View {
            rot: nalgebra::convert(rot),
            f: (kp.f[0] * stretch / kp.fov, kp.f[1] * stretch / kp.fov),
            c: (size.0 / 2.0 - kp.translation2d[0], size.1 / 2.0 - kp.translation2d[1]),
            size, projection: kp.output_projection,
        }
    }

    /// Output cells per radian at the middle, for a grid `cells` wide
    pub fn cells_per_radian(&self, cells: usize) -> f32 {
        self.f.0 * cells as f32 / self.size.0
    }

    /// Not of unit length
    pub fn dir(&self, u: f32, v: f32) -> Vector3<f32> {
        let n = ((u * self.size.0 - self.c.0) / self.f.0, (v * self.size.1 - self.c.1) / self.f.1);
        // Rectilinear, the usual one: the point of the plane is the ray
        if self.projection == 0 { return self.rot * Vector3::new(n.0, n.1, 1.0); }
        let r = (n.0 * n.0 + n.1 * n.1).sqrt();
        let theta = ray_theta(r, self.projection);
        let d = if r < 1e-9 { Vector3::new(0.0, 0.0, 1.0) } else {
            let s = theta.sin() / r;
            Vector3::new(n.0 * s, n.1 * s, theta.cos())
        };
        self.rot * d
    }

    /// Where a direction of the world is in the output, normalized; None where the projection doesn't reach
    pub fn project(&self, w: &Vector3<f32>) -> Option<(f32, f32)> {
        self.project_camera(&self.rot.tr_mul(w))
    }

    /// The same for a direction of the output camera
    fn project_camera(&self, d: &Vector3<f32>) -> Option<(f32, f32)> {
        if self.projection == 0 {
            if d.z <= 1e-9 * d.norm() { return None; }
            return Some(((d.x / d.z * self.f.0 + self.c.0) / self.size.0, (d.y / d.z * self.f.1 + self.c.1) / self.size.1));
        }
        let rxy = (d.x * d.x + d.y * d.y).sqrt();
        let theta = rxy.atan2(d.z);
        // The others reach past 90 degrees, all but the orthographic one; none straight back
        if (self.projection == 4 && d.z <= 0.0) || (rxy < 1e-6 * d.norm() && d.z < 0.0) { return None; }
        let (nx, ny) = if rxy < 1e-9 { (0.0, 0.0) } else {
            let k = ray_radius(theta, self.projection) / rxy;
            (d.x * k, d.y * k)
        };
        Some(((nx * self.f.0 + self.c.0) / self.size.0, (ny * self.f.1 + self.c.1) / self.size.1))
    }
}

// The output projections, as in the kernel (ray_unproject, ray_radius)
fn ray_theta(r: f32, projection: i32) -> f32 {
    match projection {
        1 => 2.0 * (r * 0.5).atan(),
        2 => r,
        3 => 2.0 * (r * 0.5).min(1.0).asin(),
        4 => r.min(1.0).asin(),
        _ => r.atan(),
    }
}
fn ray_radius(theta: f32, projection: i32) -> f32 {
    match projection {
        1 => 2.0 * (theta * 0.5).tan(),
        2 => theta,
        3 => 2.0 * (theta * 0.5).sin(),
        4 => theta.sin(),
        _ => theta.tan(),
    }
}

/// A frame on a coarse grid, its coverage, and where it looks
pub struct GridFrame<'a> {
    pub view: &'a View,
    pub grid: (usize, usize),
    pub planes: &'a [Vec<f32>],
    pub coverage: &'a [f32],
}

// ---------------------------------------------------------------------------------------------------
// The cube map

/// Which face a direction falls on, and where on it, in texels (centers at .5)
fn to_face(w: &Vector3<f32>, n: usize) -> (usize, f32, f32) {
    let (ax, ay, az) = (w.x.abs(), w.y.abs(), w.z.abs());
    let (face, a, b) = if ax >= ay && ax >= az {
        (if w.x > 0.0 { 0 } else { 1 }, w.y / ax, w.z / ax)
    } else if ay >= az {
        (if w.y > 0.0 { 2 } else { 3 }, w.x / ay, w.z / ay)
    } else {
        (if w.z > 0.0 { 4 } else { 5 }, w.x / az, w.y / az)
    };
    (face, (a + 1.0) * 0.5 * n as f32, (b + 1.0) * 0.5 * n as f32)
}

fn from_face(face: usize, x: f32, y: f32, n: usize) -> Vector3<f32> {
    let (a, b) = (x / n as f32 * 2.0 - 1.0, y / n as f32 * 2.0 - 1.0);
    match face {
        0 => Vector3::new(1.0, a, b),
        1 => Vector3::new(-1.0, a, b),
        2 => Vector3::new(a, 1.0, b),
        3 => Vector3::new(a, -1.0, b),
        4 => Vector3::new(a, b, 1.0),
        _ => Vector3::new(a, b, -1.0),
    }
}

/// An f32 shared between threads, relaxed: on ARM and x86 these are plain loads and stores
#[derive(Default)]
struct F32(AtomicU32);
impl F32 {
    #[inline] fn get(&self) -> f32 { f32::from_bits(self.0.load(Ordering::Relaxed)) }
    #[inline] fn set(&self, v: f32) { self.0.store(v.to_bits(), Ordering::Relaxed) }
}
fn texels(len: usize) -> Vec<F32> {
    (0..len).map(|_| F32::default()).collect()
}

pub struct Sphere {
    /// Texels per face side
    pub n: usize,
    half_life: f32,
    /// How much each texel holds, as of `epoch`: faded by half every `half_life`, all of them alike, so the weight
    /// is kept unfaded from `epoch` and faded when read, by one factor for the whole frame
    weight: Vec<F32>,
    epoch: F32,
    planes: Vec<Vec<F32>>,
}

impl Sphere {
    /// About as fine as the grid at the middle of the output: a face spans 90 degrees, 2 radians of tangent
    pub fn new(cells_per_radian: f32, num_planes: usize, half_life: f32) -> Sphere {
        let n = ((2.0 * cells_per_radian).round() as usize).clamp(32, 512);
        let len = 6 * n * n;
        Sphere { n, half_life, weight: texels(len), epoch: F32::default(), planes: (0..num_planes).map(|_| texels(len)).collect() }
    }

    pub fn reset(&self, now: f32) {
        self.weight.iter().for_each(|w| w.set(0.0));
        self.epoch.set(now);
    }

    /// What the weights are worth at `now` (s)
    pub fn fade(&self, now: f32) -> f32 {
        0.5f32.powf((now - self.epoch.get()) / self.half_life)
    }

    /// The planes and how much there is, bilinear over the texels around `w`, each by its weight
    pub fn sample(&self, w: &Vector3<f32>, fade: f32, out: &mut [f32]) -> f32 {
        let n = self.n;
        let (face, x, y) = to_face(w, n);
        let (x, y) = ((x - 0.5).clamp(0.0, (n - 1) as f32), (y - 0.5).clamp(0.0, (n - 1) as f32));
        let (x0, y0) = (x as usize, y as usize);
        let (x1, y1) = ((x0 + 1).min(n - 1), (y0 + 1).min(n - 1));
        let (fx, fy) = (x - x0 as f32, y - y0 as f32);
        let base = face * n * n;
        let taps = [(x0, y0, (1.0 - fx) * (1.0 - fy)), (x1, y0, fx * (1.0 - fy)), (x0, y1, (1.0 - fx) * fy), (x1, y1, fx * fy)];
        out.iter_mut().for_each(|o| *o = 0.0);
        let mut total = 0.0;
        for (tx, ty, b) in taps {
            let i = base + ty * n + tx;
            let k = b * self.weight[i].get() * fade;
            if k <= 0.0 { continue; }
            for (o, p) in out.iter_mut().zip(&self.planes) { *o += p[i].get() * k; }
            total += k;
        }
        if total > 1e-6 { out.iter_mut().for_each(|o| *o /= total); }
        total
    }

    /// The mean difference of the first plane between the sphere and the frame, where both have it; None where they
    /// hardly overlap. Every third cell each way is enough
    pub fn mismatch(&self, frame: &GridFrame, now: f32) -> Option<f32> {
        let (gw, gh) = frame.grid;
        let fade = self.fade(now);
        let mut values = vec![0.0f32; self.planes.len()];
        let (mut diff, mut n) = (0.0f32, 0usize);
        for j in (0..gh).step_by(3) {
            for i in (0..gw).step_by(3) {
                let k = j * gw + i;
                if frame.coverage[k] < 0.999 { continue; }
                let w = frame.view.dir(i as f32 / gw as f32, j as f32 / gh as f32);
                if self.sample(&w, fade, &mut values) > 0.5 {
                    diff += (values[0] - frame.planes[0][k]).abs();
                    n += 1;
                }
            }
        }
        (n > gw * gh / 200).then(|| diff / n as f32)
    }

    /// Writes the frame where it covers: each texel in view takes the frame (bilinear on the grid) by its coverage,
    /// over what was there before, faded. Texels are gathered rather than cells splatted, so nothing's left out
    /// where the sphere is finer than the grid
    pub fn write(&self, frame: &GridFrame, now: f32) {
        let (view, grid, coverage) = (frame.view, frame.grid, frame.coverage);
        let n = self.n;
        // The unfaded weights grow by twice every half life after the epoch; well before they'd lose precision, fade
        // them all and start from here
        if now - self.epoch.get() > 20.0 * self.half_life {
            let fade = self.fade(now);
            self.weight.iter().for_each(|w| w.set(w.get() * fade));
            self.epoch.set(now);
        }
        let fade = self.fade(now);
        // The texels the frame covers, per face: the box around where every other covered cell lands, with room for
        // the texels between them (up to three times finer than the grid at a face's corners)
        let mut boxes = [(usize::MAX, usize::MAX, 0usize, 0usize); 6];
        for j in (0..grid.1).step_by(2).chain([grid.1 - 1]) {
            for i in (0..grid.0).step_by(2).chain([grid.0 - 1]) {
                let near = |i: usize, j: usize| coverage[j.min(grid.1 - 1) * grid.0 + i.min(grid.0 - 1)] > 0.01;
                if !(near(i, j) || near(i + 1, j) || near(i, j + 1) || near(i + 1, j + 1)) { continue; }
                let w = view.dir(i as f32 / grid.0 as f32, j as f32 / grid.1 as f32);
                let (face, x, y) = to_face(&w, n);
                let b = &mut boxes[face];
                let (x, y) = (x as usize, y as usize);
                *b = (b.0.min(x.saturating_sub(8)), b.1.min(y.saturating_sub(8)), b.2.max((x + 9).min(n)), b.3.max((y + 9).min(n)));
            }
        }
        // The rows go to a few threads, taken in turn: the rows of a box aren't all as busy
        let mut work: Vec<Vec<Row>> = (0..THREADS).map(|_| Vec::new()).collect();
        for (face, &(x0, y0, x1, y1)) in boxes.iter().enumerate() {
            for y in y0..y1 {
                let r = (face * n + y) * n;
                let k = work.iter().map(|w| w.len()).enumerate().min_by_key(|&(_, l)| l).map_or(0, |(k, _)| k);
                work[k].push(Row { face, y, xs: (x0, x1), weight: &self.weight[r..r + n], planes: self.planes.iter().map(|p| &p[r..r + n]).collect() });
            }
        }
        std::thread::scope(|scope| {
            for rows in work {
                scope.spawn(move || for row in rows { row.write(n, frame, fade) });
            }
        });
    }
}

const THREADS: usize = 4;

/// One row of a face's texels, from `xs.0` to `xs.1`, to be written
struct Row<'a> {
    face: usize,
    y: usize,
    xs: (usize, usize),
    /// The whole row of the weights and of each plane
    weight: &'a [F32],
    planes: Vec<&'a [F32]>,
}

impl Row<'_> {
    fn write(self, n: usize, frame: &GridFrame, fade: f32) {
        let (view, (gw, gh), lo, coverage) = (frame.view, frame.grid, frame.planes, frame.coverage);
        let (face, y, (x0, x1)) = (self.face, self.y, self.xs);
        // Along a row of a face the direction, and so the camera's, steps by the same amount
        let start = from_face(face, x0 as f32 + 0.5, y as f32 + 0.5, n);
        let mut d = view.rot.tr_mul(&start);
        let step = view.rot.tr_mul(&(from_face(face, x0 as f32 + 1.5, y as f32 + 0.5, n) - start));
        for x in x0..x1 {
            if x > x0 { d += step; }
            let Some((u, v)) = view.project_camera(&d) else { continue };
            if !(0.0..=1.0).contains(&u) || !(0.0..=1.0).contains(&v) { continue; }
            // Bilinear on the grid. The planes by each tap's coverage too: past the frame's edge the grid has the
            // edge repeated at about half its value (blended with black), which would darken the line the edge leaves
            let (gx, gy) = ((u * gw as f32).min((gw - 1) as f32), (v * gh as f32).min((gh - 1) as f32));
            let (cx, cy) = (gx as usize, gy as usize);
            let (fx, fy) = (gx - cx as f32, gy - cy as f32);
            let (k00, k01) = (cy * gw + cx, cy * gw + (cx + 1).min(gw - 1));
            let down = if cy + 1 < gh { gw } else { 0 };
            let taps = [(k00, (1.0 - fx) * (1.0 - fy)), (k01, fx * (1.0 - fy)), (k00 + down, (1.0 - fx) * fy), (k01 + down, fx * fy)];
            let c = taps.iter().map(|&(k, t)| coverage[k] * t).sum::<f32>();
            if c <= 0.01 { continue; }
            let old = (self.weight[x].get() * fade).min(1.0) * (1.0 - c);
            let total = c + old;
            for (p, l) in self.planes.iter().zip(lo) {
                let new = taps.iter().map(|&(k, t)| l[k] * coverage[k] * t).sum::<f32>();
                p[x].set((new + p[x].get() * old) / total);
            }
            self.weight[x].set(total / fade);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nalgebra::{ Rotation3, Vector3 };

    /// Narrow enough that the corners are inside the orthographic projection's disc
    fn view(yaw: f32, pitch: f32, projection: i32) -> View {
        let rot = *(Rotation3::from_axis_angle(&Vector3::y_axis(), yaw) * Rotation3::from_axis_angle(&Vector3::x_axis(), pitch)).matrix();
        View { rot, f: (2400.0, 2400.0), c: (1920.0 + 30.0, 1080.0 - 20.0), size: (3840.0, 2160.0), projection }
    }

    #[test]
    fn project_inverts_dir() {
        for projection in 0..5 {
            let v = view(0.3, -0.1, projection);
            for &(u, w) in &[(0.5, 0.5), (0.0, 0.0), (1.0, 0.3), (0.2, 1.0), (0.77, 0.12)] {
                let (u2, w2) = v.project(&v.dir(u, w)).unwrap();
                assert!((u - u2).abs() < 1e-4 && (w - w2).abs() < 1e-4, "{projection}: {u},{w} -> {u2},{w2}");
            }
        }
    }

    #[test]
    fn faces_round_trip() {
        let n = 100;
        for face in 0..6 {
            for &(x, y) in &[(0.5, 0.5), (50.0, 50.0), (99.5, 0.5), (13.2, 77.7)] {
                let (f2, x2, y2) = to_face(&from_face(face, x, y, n).normalize(), n);
                assert_eq!(face, f2);
                assert!((x - x2).abs() < 1e-3 && (y - y2).abs() < 1e-3);
            }
        }
    }

    /// A pan: what the first view saw at its right edge is, after turning right, nearer the middle of the second
    #[test]
    fn pan_reads_back_where_the_world_is() {
        let grid = (96, 54);
        let a = view(0.0, 0.0, 0);
        let b = view(0.2, 0.0, 0);
        // A vertical stripe pattern, by the first view's columns
        let lo: Vec<Vec<f32>> = vec![(0..grid.0 * grid.1).map(|k| ((k % grid.0) / 8) as f32).collect()];
        let coverage = vec![1.0; grid.0 * grid.1];
        let s = Sphere::new(a.cells_per_radian(grid.0), 1, 3.0);
        s.write(&GridFrame { view: &a, grid, planes: &lo, coverage: &coverage }, 0.0);
        assert!(s.mismatch(&GridFrame { view: &a, grid, planes: &lo, coverage: &coverage }, 0.0).unwrap() < 0.3);
        let mut out = [0.0f32];
        let mut checked = 0;
        for i in 0..grid.0 {
            let w = b.dir(i as f32 / grid.0 as f32, 0.5);
            let Some((u, _)) = a.project(&w) else { continue };
            if !(0.05..0.95).contains(&u) { continue; }
            let k = s.sample(&w, s.fade(0.0), &mut out);
            assert!(k > 0.5);
            let want = ((u * grid.0 as f32).round() as usize / 8) as f32;
            // Stripes are 8 cells wide; near their edges the bilinear reading is in between
            assert!((out[0] - want).abs() <= 1.0, "column {i}: {} vs {want}", out[0]);
            checked += 1;
        }
        assert!(checked > 20);
    }
}

/// The directions against Gyroflow's own: an output position's direction in the world, taken back to the source
/// camera by the camera's orientation and imaged by the lens, has to be the source pixel Gyroflow reads for it
#[cfg(test)]
mod gyroflow_geometry {
    use std::collections::BTreeMap;
    use nalgebra::{ UnitQuaternion, Vector3 };
    use gyroflow_core::StabilizationManager;
    use gyroflow_core::stabilization::{ ComputeParams, Interpolation, Luma16, Stabilization, undistort_points_with_rolling_shutter };
    use super::{ View, wide_params };
    use crate::{ PlaneJob, PlaneStab, process_plane };

    const SIZE: (usize, usize) = (1920, 1080);
    const F: f64 = 1100.0;

    fn manager(moving: bool) -> StabilizationManager {
        let stab = StabilizationManager::default();
        stab.init_from_video_data(10_000.0, 30.0, 300, SIZE);
        // An equidistant fisheye: OpenCV's model with (next to) no distortion terms; with none at all it's a pinhole
        let lens = serde_json::json!({
            "name": "test", "calibrated_by": "test", "camera_brand": "test", "camera_model": "test",
            "calib_dimension": { "w": SIZE.0, "h": SIZE.1 }, "orig_dimension": { "w": SIZE.0, "h": SIZE.1 },
            "fps": 30.0, "calibrator_version": "---",
            "fisheye_params": {
                "camera_matrix": [[F, 0.0, 960.0], [0.0, F, 540.0], [0.0, 0.0, 1.0]],
                "distortion_coeffs": [1e-6, 0.0, 0.0, 0.0]
            }
        }).to_string();
        stab.load_lens_profile(&lens).unwrap();

        // The camera turns about all three axes; the smoothed orientation follows it slower. Gyroflow keeps the
        // smoothed one as the correction from it to the camera's (GyroSource::recompute_smoothness)
        let k = if moving { 1.0 } else { 0.0 };
        let camera = |t: f64| UnitQuaternion::from_euler_angles(k * 0.2 * (t * 1.3).sin(), k * (0.3 * (t * 0.7).sin() + 0.1), k * 0.15 * (t * 2.1).cos());
        let smoothed = |t: f64| UnitQuaternion::from_euler_angles(k * 0.05 * (t * 0.3).sin(), k * (0.3 * (t * 0.7 - 0.4).sin() + 0.1), k * 0.02);
        {
            let mut gyro = stab.gyro.write();
            gyro.duration_ms = 10_000.0;
            let ts: Vec<i64> = (0..=10_000).map(|ms| ms * 1000).collect();
            gyro.quaternions = ts.iter().map(|&us| (us, camera(us as f64 / 1e6))).collect::<BTreeMap<_, _>>();
            gyro.smoothed_quaternions = ts.iter().map(|&us| { let t = us as f64 / 1e6; (us, smoothed(t).inverse() * camera(t)) }).collect();
        }
        stab
    }

    fn params(stab: &StabilizationManager, video_rotation: f64, zoom_center: (f64, f64), readout_ms: f64) -> ComputeParams {
        let mut p = ComputeParams::from_manager(stab);
        p.video_rotation = video_rotation;
        p.adaptive_zoom_center_offset = zoom_center;
        p.frame_readout_time = readout_ms;
        p
    }

    /// The source pixel a world direction is imaged at: with the camera's orientation when its row was read out
    fn source_pixel(p: &ComputeParams, w: &Vector3<f32>, t_ms: f64) -> (f32, f32) {
        let w: Vector3<f64> = nalgebra::convert(*w);
        let mut px = (0.0, SIZE.1 as f64 / 2.0);
        for _ in 0..20 {
            let t = t_ms + p.frame_readout_time * (px.1 / SIZE.1 as f64 - 0.5);
            let body = p.gyro.read().org_quat_at_timestamp(t).inverse() * w;
            // Camera = D · body, D = diag(1, -1, -1)
            let s = Vector3::new(body.x, -body.y, -body.z);
            let rxy = s.x.hypot(s.y);
            let theta = rxy.atan2(s.z);
            px = (F * theta * s.x / rxy + 960.0, F * theta * s.y / rxy + 540.0);
        }
        (px.0 as f32, px.1 as f32)
    }

    /// Gyroflow's CPU path for points (the zoom's and the sync's), from the source to the output
    #[test]
    fn points_land_where_their_directions_are() {
        let stab = manager(true);
        for video_rotation in [0.0, 7.0] {
            let p = params(&stab, video_rotation, (0.0, 0.0), 0.0);
            for t_ms in [500.0, 2345.0, 7010.0] {
                let view = View::at(&p, (t_ms * 1000.0) as i64);
                for &(u, v) in &[(0.5, 0.5), (0.1, 0.1), (0.9, 0.2), (0.3, 0.85), (0.95, 0.95)] {
                    let src = source_pixel(&p, &view.dir(u, v), t_ms);
                    let out = undistort_points_with_rolling_shutter(&[src], t_ms, None, &p, 1.0, true, false)[0];
                    let want = (u * p.output_width as f32, v * p.output_height as f32);
                    assert!((out.0 - want.0).hypot(out.1 - want.1) < 0.01, "rotation {video_rotation}, {t_ms} ms, ({u}, {v}): Gyroflow puts it at {out:?}, the sphere at {want:?}");
                }
            }
        }
    }

    /// The kernel itself, on the CPU, into a buffer the grid's size: a source whose samples are their own x (or y)
    /// shows where each output pixel was read from
    fn rendered(p: &ComputeParams, t_ms: f64, label: &str) {
        let (ow, oh) = (240usize, 135usize);
        let mut plane = {
            let mut s = Stabilization::default();
            s.interpolation = Interpolation::Bilinear;
            s.set_device(-1);
            s.init_size(SIZE, (p.output_width, p.output_height));
            s.set_compute_params(p.clone());
            PlaneStab::new(s, 0, -1)
        };
        let ts = (t_ms * 1000.0) as i64;
        let mut read = |coord: usize| -> Vec<u16> {
            let src: Vec<u16> = (0..SIZE.0 * SIZE.1).map(|k| (if coord == 0 { k % SIZE.0 } else { k / SIZE.0 } * 16) as u16).collect();
            let mut dst = vec![0u16; ow * oh];
            let job = PlaneJob {
                src: src.as_ptr() as *const u8, src_stride: SIZE.0 * 2, src_w: SIZE.0, src_h: SIZE.1,
                dst: dst.as_mut_ptr() as *mut u8, dst_stride: ow * 2, dst_w: ow, dst_h: oh,
            };
            process_plane::<Luma16>(&mut plane, ts, 65535.0, &job).unwrap();
            dst
        };
        let (xs, ys) = (read(0), read(1));
        let view = View::at(p, ts);
        let (mut worst, mut n) = (0.0f32, 0);
        for j in (5..oh - 5).step_by(7) {
            for i in (5..ow - 5).step_by(11) {
                let (x, y) = (xs[j * ow + i] as f32 / 16.0, ys[j * ow + i] as f32 / 16.0);
                let want = source_pixel(p, &view.dir(i as f32 / ow as f32, j as f32 / oh as f32), t_ms);
                // Off the source, or at its edge where the interpolation takes in the background
                if !(2.0..SIZE.0 as f32 - 3.0).contains(&want.0) || !(2.0..SIZE.1 as f32 - 3.0).contains(&want.1) { continue; }
                worst = worst.max((x - want.0).hypot(y - want.1));
                n += 1;
            }
        }
        assert!(n > 40, "{label}: only {n} samples");
        // The samples are sixteenths of a pixel, the kernel's arithmetic f32, the orientations interpolated per row
        assert!(worst < 0.25, "{label}, {t_ms} ms: the kernel reads up to {worst:.3} source pixels away from the sphere's direction");
    }

    #[test]
    fn rendered_pixels_are_where_their_directions_are() {
        rendered(&params(&manager(false), 0.0, (0.0, 0.0), 0.0), 500.0, "still");
        let stab = manager(true);
        rendered(&params(&stab, 0.0, (0.0, 0.0), 0.0), 500.0, "turning");
        rendered(&params(&stab, 7.0, (0.0, 0.0), 0.0), 500.0, "turning, video rotated");
        rendered(&params(&stab, 0.0, (0.04, -0.03), 0.0), 500.0, "turning, zoom off center");
        // The rows read at different times, and the direction is the same for all of them
        rendered(&params(&stab, 0.0, (0.0, 0.0), 25.0), 500.0, "turning, rolling shutter");
        for t_ms in [500.0, 2345.0, 7010.0] {
            rendered(&params(&stab, 7.0, (0.04, -0.03), 25.0), t_ms, "all");
            rendered(&wide_params(params(&stab, 7.0, (0.04, -0.03), 25.0)), t_ms, "wide");
        }
    }

    /// The source's corners and the middles of its sides all land in the wide view while the camera turns: a
    /// fisheye seeing 57 degrees to its corners, shaken by up to 16 degrees off the smoothed view
    #[test]
    fn the_wide_view_holds_the_source() {
        let stab = manager(true);
        let p = wide_params(params(&stab, 0.0, (0.04, -0.03), 0.0));
        for t_ms in (0..20).map(|k| 250.0 + k as f64 * 480.0) {
            let view = View::at(&p, (t_ms * 1000.0) as i64);
            let o = p.gyro.read().org_quat_at_timestamp(t_ms);
            for (x, y) in [(0.0, 0.0), (960.0, 0.0), (1920.0, 0.0), (0.0, 540.0), (1920.0, 540.0), (0.0, 1080.0), (960.0, 1080.0), (1920.0, 1080.0)] {
                // The ray the lens images there, in the world: camera = D · body
                let (dx, dy) = ((x - 960.0) / F, (y - 540.0) / F);
                let theta = dx.hypot(dy);
                let s = Vector3::new(dx / theta * theta.sin(), dy / theta * theta.sin(), theta.cos());
                let w = o * Vector3::new(s.x, -s.y, -s.z);
                let (u, v) = view.project(&nalgebra::convert(w)).unwrap();
                assert!((0.0..=1.0).contains(&u) && (0.0..=1.0).contains(&v), "{t_ms} ms: the source's ({x}, {y}) is at ({u}, {v})");
            }
        }
    }
}
