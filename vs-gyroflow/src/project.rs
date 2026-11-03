// SPDX-License-Identifier: GPL-3.0-or-later
//
// The Gyroflow project the plugin stabilizes a video with when it has none (its own motion data, Gyroflow's default
// preset and then the plugin's), written out so the video can be opened in Gyroflow as it plays. Loaded the way
// Engine::load loads it, and saved by gyroflow-core's own project writer, so Gyroflow and the plugin read the same
// lens profile, smoothing, zoom and refraction from it. The refraction as the player has the lens under water: Auto
// writes the camera's In water / Out of water as keyframes, which Gyroflow shows on its timeline. The horizon lock and
// the view as the player has them too: the level (levels.rs) is Gyroflow's zoom off at the level's FOV and its
// Smoothness, and the wider view the same at that view's FOV, so the clip looks about the same in Gyroflow. About:
// Gyroflow smooths with its Default, whose second pass measures against the clip's largest drift, not the level's
// margin (smoothing.rs). The motion data goes into the project, so Gyroflow doesn't read it from the video again.

use gyroflow_core::{ GyroflowProjectType, filesystem, stabilization::ComputeParams };

use crate::levels::{ self, Level };
use crate::water::Underwater;

/// The project JSON for `video` stabilized from its own motion data with Gyroflow's default preset and then `preset`,
/// loaded as the plugin plays it (`load_manager`).
/// `project_path`: where it will be saved (macOS bookmarks to the video are made relative to it)
/// `underwater`: the lens under water, not, or as the camera says (water.rs)
/// `horizon_lock`: the horizon lock's amount (%, 0 off) instead of the preset's
/// `level`: the level it plays in; `wide`: the plugin's wider view, as Gyroflow has it: its zoom off, at the FOV
/// `wide_fov` finds
pub fn from_motion_data(video: &str, preset: Option<&str>, underwater: Underwater, horizon_lock: Option<f64>, level: Level, wide: bool, project_path: &str) -> Result<String, String> {
    let (stab, _, _) = crate::load_manager(video, preset, horizon_lock, underwater, 0.0)?;
    if !stab.gyro.read().has_motion() {
        return Err(format!("{video} has no motion data"));
    }
    stab.set_smoothing_param("smoothness", level.smoothness());
    stab.set_adaptive_zoom(0.0);
    let fov = if wide {
        stab.invalidate_smoothing();
        stab.recompute_blocking();
        crate::wide_fov(&ComputeParams::from_manager(&stab), stab.params.read().fps)
    } else {
        level.fov(levels::still_fov(&stab))
    };
    stab.set_fov(fov);
    // load_video_file leaves the project's video to the app
    stab.input_file.write().url = filesystem::path_to_url(video);
    // The size the plugin renders it at (the video's, or the lens profile's output size), which Gyroflow's export
    // settings also take from here
    let (w, h) = stab.params.read().output_size;
    let output = serde_json::json!({ "output": { "output_width": w, "output_height": h } });
    // With the motion data, as Gyroflow's own Save writes it: Gyroflow opens the clip without reading it from the
    // video again, from a NAS seconds
    stab.export_gyroflow_data(GyroflowProjectType::WithGyroData, &output.to_string(), Some(&filesystem::path_to_url(project_path)))
        .map_err(|e| format!("Can't make a project of {video}: {e:?}"))
}
