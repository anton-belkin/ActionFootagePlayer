// SPDX-License-Identifier: GPL-3.0-or-later
// Gyroflow's Default smoothing (gyroflow-core's smoothing/default_algo.rs, © Aphobius), made local in time, for the
// levels (levels.rs) and the motion data that comes in as the clip plays (motion.rs)
//
// As Default: the camera's speed, smoothed over 0.1 s, picks each moment's smoothing between a second (still) and 0.1 s
// (500 °/s times the level's smoothness, and faster); then a second pass smooths less where the path got far from the
// camera. Two things differ:
//
// - The second pass measures that distance against the level's margin, how far the camera may turn before the crop
//   runs out (levels.rs), not against the largest distance in the whole clip. Each moment depends only on the few
//   seconds around it: with 4 s of motion data ahead it's the same as with all of it (4 s held p99 under 1 px on the
//   clips measured, where Default's whole-clip maximum never settled), and it's known before the rest is read.
// - Each step's smoothing is from the time between its samples, not from the clip's average rate, so motion data with
//   holes in it (read around a seek before the part before it) smooths as well; across a hole of more than
//   HOLE_S the path starts again from the camera.

use std::collections::BTreeMap;
use gyroflow_core::gyro_source::{ Quat64, TimeQuat };
use gyroflow_core::smoothing::SmoothingAlgorithm;
use gyroflow_core::stabilization::ComputeParams;

const MAX_VELOCITY: f64 = 500.0;
const FOV_REFERENCE: f64 = 120.0;
/// Default's: a second while the camera holds still, 0.1 s at its max velocity
const MAX_SMOOTHNESS_S: f64 = 1.0;
const ALPHA_0_1S: f64 = 0.1;
/// Samples this far apart don't smooth into each other
const HOLE_S: f64 = 0.5;

#[derive(Clone)]
pub struct Predictive {
    /// Default's Smoothness (Level::smoothness)
    pub smoothness: f64,
    /// How far the camera may turn from the smoothed path before the crop runs out, radians (levels::margin)
    pub margin: f64,
}

impl SmoothingAlgorithm for Predictive {
    fn get_name(&self) -> String { "Predictive".into() }
    fn get_parameters_json(&self) -> serde_json::Value {
        serde_json::json!([{ "name": "smoothness", "value": self.smoothness }, { "name": "margin", "value": self.margin }])
    }
    fn get_status_json(&self) -> serde_json::Value { serde_json::json!([]) }
    fn set_parameter(&mut self, name: &str, val: f64) {
        match name { "smoothness" => self.smoothness = val, "margin" => self.margin = val, _ => {} }
    }
    fn get_parameter(&self, name: &str) -> f64 {
        // Within the trim range only, when there's one: a part stabilized on its own (a moment's, Engine::load's
        // trim); Gyroflow then smooths the frames outside it as the whole video's
        match name { "smoothness" => self.smoothness, "margin" => self.margin, "trim_range_only" => 1.0, _ => 0.0 }
    }
    fn get_checksum(&self) -> u64 {
        self.smoothness.to_bits() ^ self.margin.to_bits().rotate_left(17)
    }

    fn smooth(&self, quats: &TimeQuat, _duration_ms: f64, params: &ComputeParams) -> TimeQuat {
        // A part on its own: the motion outside it is held still at its edges, so it doesn't steer the path inside it
        let quats = &*gyroflow_core::smoothing::Smoothing::get_trimmed_quats(quats, params.scaled_duration_ms, true, &params.trim_ranges);
        if quats.len() < 2 { return quats.clone(); }
        let ts: Vec<i64> = quats.keys().copied().collect();
        let raw: Vec<Quat64> = quats.values().copied().collect();
        let n = ts.len();
        // The time from each sample to the next, s
        let dt: Vec<f64> = (0..n - 1).map(|i| (ts[i + 1] - ts[i]) as f64 / 1_000_000.0).collect();
        let alpha = |tc: f64, dt: f64| 1.0 - (-dt.max(0.0) / tc).exp();

        // The camera's speed, °/s, smoothed over 0.1 s, as a share of the max velocity
        let mut vel = vec![0.0; n];
        for i in 1..n {
            if dt[i - 1] > 0.0 && dt[i - 1] <= HOLE_S {
                vel[i] = (raw[i - 1].inverse() * raw[i]).angle().to_degrees() / dt[i - 1];
            }
        }
        ema_both_ways(&mut vel, &dt, |d| alpha(ALPHA_0_1S, d));
        for (i, v) in vel.iter_mut().enumerate() {
            let frame = gyroflow_core::frame_at_timestamp(ts[i] as f64 / 1000.0, params.scaled_fps) as usize;
            let mut fov_ratio = if params.camera_diagonal_fovs.len() == 1 {
                params.camera_diagonal_fovs[0] / FOV_REFERENCE
            } else {
                params.camera_diagonal_fovs.get(frame).map(|x| *x / FOV_REFERENCE).unwrap_or(1.0)
            };
            if let Some(limit) = params.smoothing_fov_limit_per_frame.get(frame) { fov_ratio *= *limit; }
            // Default with its second pass halves the max velocity, to zoom about as much as without it
            *v /= MAX_VELOCITY * self.smoothness * fov_ratio * 0.5;
        }

        // Smoothed between a second and 0.1 s by the speed
        let step = |r: f64, d: f64| alpha(MAX_SMOOTHNESS_S, d) * (1.0 - r) + alpha(ALPHA_0_1S, d) * r;
        let first = plain_both_ways(&raw, &dt, |i, d| step(vel[i], d));

        // How far that is from the camera, against the margin: under half of it counts as nothing, smoothed over 0.1 s,
        // then 0.5 (none) to 1 (all of the margin) and on past it
        let mut dist: Vec<f64> = (0..n).map(|i| {
            let d = (raw[i].inverse() * first[i]).angle() / self.margin.max(1e-6);
            if d < 0.5 { 0.0 } else { d }
        }).collect();
        ema_both_ways(&mut dist, &dt, |d| alpha(ALPHA_0_1S, d));
        for d in dist.iter_mut() { *d = (d.min(2.0) + 1.0) / 2.0; }

        let second = plain_both_ways(&first, &dt, |i, d| step(vel[i] * dist[i], d));
        ts.into_iter().zip(second).collect::<BTreeMap<_, _>>()
    }
}

/// A one-pole filter forward, then back; across a hole it starts again from the value there
fn ema_both_ways(v: &mut [f64], dt: &[f64], alpha: impl Fn(f64) -> f64) {
    for i in 1..v.len() {
        if dt[i - 1] <= HOLE_S { let a = alpha(dt[i - 1]); v[i] = v[i - 1] * (1.0 - a) + v[i] * a; }
    }
    for i in (0..v.len() - 1).rev() {
        if dt[i] <= HOLE_S { let a = alpha(dt[i]); v[i] = v[i + 1] * (1.0 - a) + v[i] * a; }
    }
}

/// Gyroflow's plain 3D smoothing forward, then back, each step's alpha from `alpha(sample, time to the one before)`
fn plain_both_ways(q: &[Quat64], dt: &[f64], alpha: impl Fn(usize, f64) -> f64) -> Vec<Quat64> {
    let n = q.len();
    let mut fwd = q.to_vec();
    for i in 1..n {
        if dt[i - 1] <= HOLE_S { fwd[i] = fwd[i - 1].slerp(&q[i], alpha(i, dt[i - 1]).min(1.0)); }
    }
    let mut out = fwd.clone();
    for i in (0..n - 1).rev() {
        if dt[i] <= HOLE_S { out[i] = out[i + 1].slerp(&fwd[i], alpha(i, dt[i]).min(1.0)); }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use gyroflow_core::gyro_source::Quat64;

    fn params() -> ComputeParams {
        let mut p = ComputeParams::default();
        p.scaled_fps = 60.0;
        p
    }

    /// A camera turning on one axis: a pan at `speed` °/s with a shake
    fn pan(seconds: f64, speed: f64, from_us: i64) -> TimeQuat {
        (0..(seconds * 1000.0) as i64).map(|i| {
            let t = i as f64 / 1000.0;
            let yaw = (speed * t + 2.0 * (t * 37.0).sin()).to_radians();
            (from_us + i * 1000, Quat64::from_euler_angles(0.0, yaw, 0.0))
        }).collect()
    }

    #[test]
    fn takes_the_shake_out_and_follows_the_pan() {
        let q = pan(10.0, 20.0, 0);
        let s = Predictive { smoothness: 0.75, margin: 6f64.to_radians() }.smooth(&q, 10_000.0, &params());
        // Mid-clip: the shake (±2°) mostly gone, the pan followed within the margin
        let mid: Vec<f64> = (3000..7000).map(|i| (q[&(i * 1000)].inverse() * s[&(i * 1000)]).angle().to_degrees()).collect();
        let max = mid.iter().cloned().fold(0.0, f64::max);
        assert!(max < 6.0, "{max}");
        let jitter: f64 = (3001..7000).map(|i| (s[&((i - 1) * 1000)].inverse() * s[&(i * 1000)]).angle().to_degrees()).fold(0.0, f64::max);
        assert!(jitter < 0.05, "{jitter}");
    }

    #[test]
    fn a_window_of_it_is_the_whole_of_it() {
        // Local in time: the moment 4 s before the end of what's known is as it is with all of it, within a pixel of
        // a 4K Osmo Action 6 frame (0.039°)
        let q = pan(30.0, 30.0, 0);
        let alg = Predictive { smoothness: 1.0, margin: 8f64.to_radians() };
        let whole = alg.smooth(&q, 30_000.0, &params());
        let part: TimeQuat = q.range(..=14_000_000).map(|(k, v)| (*k, *v)).collect();
        let s = alg.smooth(&part, 14_000.0, &params());
        let d = (whole[&10_000_000].inverse() * s[&10_000_000]).angle().to_degrees();
        assert!(d < 0.039, "{d}");
    }

    #[test]
    fn a_hole_starts_again() {
        let mut q = pan(5.0, 10.0, 0);
        q.extend(pan(5.0, 10.0, 60_000_000));
        let s = Predictive { smoothness: 0.5, margin: 5f64.to_radians() }.smooth(&q, 65_000.0, &params());
        // Right after the hole the path is the camera's, not pulled toward what was a minute before
        let d = (q[&60_000_000].inverse() * s[&60_000_000]).angle().to_degrees();
        assert!(d < 3.0, "{d}");
        assert_eq!(s.len(), q.len());
    }
}
