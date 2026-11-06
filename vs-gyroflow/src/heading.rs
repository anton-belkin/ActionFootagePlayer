// SPDX-License-Identifier: GPL-3.0-or-later
//
// The 360 view's orientation and headings (docs/360-cameras.md, "The heading modes"), for the stamp (stamp.rs): the
// camera in a world with gravity down, and per heading mode the yaw the view's "straight ahead" points along.
//
// - The world: video-types' axes (+x right, +y down, +z forward), +y along gravity. gyroflow-core integrates the gyro
//   with the accelerometer (VQF) into a world of its own, with its own vertical; `World::new` finds that vertical as the
//   accelerometer's mean direction in it (what the integrator itself leveled to) and turns it onto -y. Its yaw is
//   arbitrary: no mode needs an absolute one yet (north is step 5).
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

/// gyroflow-core's world into video-types' (gravity along +y), and its camera axes into OpenCV's
#[derive(Clone, Copy, Debug)]
pub struct World {
    align: UnitQuaternion<f64>,
}

impl World {
    /// `up`: the up direction in gyroflow-core's world (the accelerometer's mean there: it reads up when still)
    pub fn new(up: Vector3<f64>) -> World {
        let target = -Vector3::y();
        let align = UnitQuaternion::rotation_between(&up.normalize(), &target)
            // Exactly opposite: half a turn about any horizontal axis
            .unwrap_or_else(|| UnitQuaternion::from_axis_angle(&Vector3::x_axis(), PI));
        World { align }
    }

    /// The camera in video-types' world from gyroflow-core's quaternion (its camera axes into its world)
    pub fn world_from_cam(&self, q: &UnitQuaternion<f64>) -> UnitQuaternion<f64> {
        self.align * q * flip()
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
        let w = World::new(Vector3::z());
        let cam = w.world_from_cam(&UnitQuaternion::identity());
        assert!((cam * Vector3::z() - Vector3::y()).norm() < 1e-12, "{}", cam * Vector3::z());
        // Whatever the up, it ends on -y
        for up in [Vector3::new(0.3, -0.2, 0.9), Vector3::new(0.0, 0.0, -1.0), Vector3::x()] {
            let w = World::new(up);
            assert!((w.align * up.normalize() + Vector3::y()).norm() < 1e-12);
        }
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
