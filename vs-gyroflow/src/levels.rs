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
    gyro.smoothed_quaternions = Arc::new(gyro.quaternions.keys().map(|t| (*t, Quat64::identity())).collect());
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
/// the half of the output's shorter side the frame covers at `still_fov`, seen from the lens (`fx`, in the source's
/// pixels, which is also the scale of Gyroflow's undistorted plane at FOV 1, where the output's width is the source's
/// `width`). The shorter side, the shorter way the picture has to go: the height of a landscape output, the width of
/// a portrait one (a frame turned a quarter, or cut to 9:16). Seen through the output's `projection` (Gyroflow's
/// OutputProjection: rectilinear's angle is the arctangent)
pub fn margin(level: Level, still_fov: f64, fx: f64, output_aspect: f64, width: f64, projection: i32) -> f64 {
    use gyroflow_core::stabilization::projection::OutputProjection;
    let half_side = still_fov * width / output_aspect.max(1.0) / 2.0;
    OutputProjection::from_i32(projection).theta_of_radius(level.crop() * half_side / fx.max(1.0))
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
        let m: Vec<f64> = Level::ALL.iter().map(|l| margin(*l, 0.7, 1473.0, 16.0 / 9.0, 3840.0, 0).to_degrees()).collect();
        assert!(m[0] < m[1] && m[1] < m[2]);
        assert!((m[1] - 5.87).abs() < 0.1, "{m:?}");
    }

    #[test]
    fn a_portrait_output_has_the_margin_of_its_width() {
        // The same frame turned a quarter: 9:16, the output's width the source's 3840 at FOV 1, its height more
        let landscape = margin(Level::Medium, 0.7, 1473.0, 16.0 / 9.0, 3840.0, 0);
        let portrait = margin(Level::Medium, 0.4, 1473.0, 9.0 / 16.0, 3840.0, 0);
        let width_only = (Level::Medium.crop() * 0.4 * 3840.0 / 2.0 / 1473.0).atan();
        assert!((portrait - width_only).abs() < 1e-12 && portrait > 0.0 && landscape > 0.0);
        assert_eq!(margin(Level::Medium, 0.7, 1473.0, 1.0, 3840.0, 0), margin(Level::Medium, 0.7, 1473.0, 0.5, 3840.0, 0));
    }

    #[test]
    fn the_margin_as_the_projection_sees_it() {
        // Equidistant: the radius is the angle; rectilinear's the arctangent of it, the smaller
        let (rect, equi) = (margin(Level::Strong, 1.2, 1473.0, 16.0 / 9.0, 3840.0, 0), margin(Level::Strong, 1.2, 1473.0, 16.0 / 9.0, 3840.0, 2));
        assert!((equi - 0.3 * 1.2 * 3840.0 * 9.0 / 16.0 / 2.0 / 1473.0).abs() < 1e-9 && rect < equi);
    }
}
