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
// - In the world: the camera's heading at the clip's start, held. Along the camera: its heading smoothed over a few
//   seconds both ways (the whole clip's motion is known before it plays, so the look-ahead is free), as a mean of unit
//   vectors, so it turns through ±180° without a jump.
// - Along the way you move and north: no data yet (step 5); stamped as along the camera with confidence 0.

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
}

impl Headings {
    /// From the camera's yaw and its weight per frame (`yaw`), at `fps`
    pub fn new(yaws: &[(f64, f64)], fps: f64) -> Headings {
        let camera = smooth(yaws, CAMERA_SIGMA_S * fps.max(1.0));
        Headings { start: camera.first().copied().unwrap_or(0.0), camera }
    }

    pub fn frames(&self) -> usize { self.camera.len() }

    /// The stamp's headings at frame `i` (held past either end)
    pub fn at(&self, i: i64) -> [HeadingStamp; HEADINGS] {
        let camera = if self.camera.is_empty() { 0.0 } else { self.camera[i.clamp(0, self.camera.len() as i64 - 1) as usize] };
        let mut h = [HeadingStamp { yaw: camera, confidence: 0.0 }; HEADINGS];
        h[Heading::World as usize] = HeadingStamp { yaw: self.start, confidence: 1.0 };
        h[Heading::Camera as usize] = HeadingStamp { yaw: camera, confidence: 1.0 };
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
}
