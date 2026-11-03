// SPDX-License-Identifier: GPL-3.0-or-later
//
// The stabilization's levels for a clip without a project: how much of the frame the zoom gives up for it and how
// smooth the camera's path gets. Everything about them is known when the clip opens, from its lens alone, so the clip
// plays from its first seconds of motion data (motion.rs) instead of after a zoom measured over all of it.
//
// The crop is the share of the distance from the frame's center to its edge that the zoom takes away, from the
// widest view the frame covers while the camera holds still (`still_fov`). The smoothing (smoothing.rs) keeps the
// picture inside that crop as best it can: where the camera turns further than the crop has room for, it follows.

use std::sync::Arc;
use gyroflow_core::StabilizationManager;
use gyroflow_core::gyro_source::Quat64;
use gyroflow_core::stabilization::ComputeParams;
use parking_lot::RwLock;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Level { Weak, Medium, Strong }

impl Level {
    pub const ALL: [Level; 3] = [Level::Weak, Level::Medium, Level::Strong];

    pub fn parse(s: &str) -> Option<Level> {
        match s {
            "weak" => Some(Level::Weak),
            "medium" => Some(Level::Medium),
            "strong" => Some(Level::Strong),
            _ => None,
        }
    }

    pub fn name(self) -> &'static str {
        match self { Level::Weak => "weak", Level::Medium => "medium", Level::Strong => "strong" }
    }

    pub fn index(self) -> usize { self as usize }

    /// The share of the distance from the frame's center to its edge the zoom takes away
    pub fn crop(self) -> f64 {
        match self { Level::Weak => 0.10, Level::Medium => 0.20, Level::Strong => 0.30 }
    }

    /// Gyroflow's Smoothness (Default's `smoothness`: the camera's speed, from 500 °/s times it, up to which it's
    /// smoothed over a second)
    pub fn smoothness(self) -> f64 {
        match self { Level::Weak => 0.5, Level::Medium => 0.75, Level::Strong => 1.0 }
    }

    /// The FOV the level renders at, with Gyroflow's zoom off
    pub fn fov(self, still_fov: f64) -> f64 {
        still_fov * (1.0 - self.crop())
    }
}

/// The widest FOV whose output the frame covers while the camera holds still: the stabilization's correction left
/// out, only the lens and its rolling shutter. The median over the frames whose motion data is there (every 10th of
/// the first 300), as Gyroflow's zoom measures it (`FovIterative`, `minimal_fovs`); it hardly changes from frame to
/// frame, the lens being the same
pub fn still_fov(stab: &StabilizationManager) -> f64 {
    let mut gyro = stab.gyro.read().clone();
    // Gyroflow keeps the correction there, from the smoothed path to the camera's: none
    gyro.smoothed_quaternions = gyro.quaternions.keys().map(|t| (*t, Quat64::identity())).collect();
    let last_ms = gyro.quaternions.keys().next_back().map_or(0.0, |t| *t as f64 / 1000.0);
    let mut params = ComputeParams::from_manager(stab);
    params.gyro = Arc::new(RwLock::new(gyro));
    params.adaptive_zoom_window = 0.0;
    let fps = params.scaled_fps.max(1.0);
    let timestamps: Vec<(usize, f64)> = (0..300).step_by(10)
        .map(|frame| (frame, gyroflow_core::timestamp_at_frame(frame as i32, fps)))
        .filter(|(frame, ts)| *frame == 0 || *ts <= last_ms)
        .collect();
    let method = stab.params.read().adaptive_zoom_method.into();
    let (_, mut measured, _, _) = gyroflow_core::zooming::calculate_fovs(&params, &timestamps, method);
    measured.retain(|f| f.is_finite() && *f > 0.0);
    if measured.is_empty() { return 1.0; }
    measured.sort_by(|a, b| a.total_cmp(b));
    measured[measured.len() / 2].clamp(0.1, 4.0)
}

/// How far the camera may turn away from the smoothed path before the crop runs out, in radians: the level's crop of
/// the half height the frame covers at `still_fov`, seen from the lens (`fx`, in the source's pixels, which is also
/// the scale of Gyroflow's undistorted plane at FOV 1). The half height, the shorter way the picture has to go
pub fn margin(level: Level, still_fov: f64, fx: f64, output_aspect: f64, width: f64) -> f64 {
    let half_height = still_fov * width / output_aspect.max(0.1) / 2.0;
    (level.crop() * half_height / fx.max(1.0)).atan()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn levels_as_named() {
        for l in Level::ALL { assert_eq!(Level::parse(l.name()), Some(l)); }
        assert_eq!(Level::parse("extreme"), None);
        assert!((Level::Medium.fov(0.7) - 0.56).abs() < 1e-9);
    }

    #[test]
    fn margin_grows_with_the_crop() {
        // Osmo Action 6 at 4K: fx 1473, the frame covering FOV 0.7 at 16:9
        let m: Vec<f64> = Level::ALL.iter().map(|l| margin(*l, 0.7, 1473.0, 16.0 / 9.0, 3840.0).to_degrees()).collect();
        assert!(m[0] < m[1] && m[1] < m[2]);
        assert!((m[1] - 5.87).abs() < 0.1, "{m:?}");
    }
}
