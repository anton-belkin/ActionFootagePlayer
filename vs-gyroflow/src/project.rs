// SPDX-License-Identifier: GPL-3.0-or-later
//
// The Gyroflow project the plugin stabilizes a video with when it has none (its own motion data, Gyroflow's default
// preset and then the plugin's), written out so the video can be opened in Gyroflow as it plays. Loaded the way
// Engine::load loads it, and saved by gyroflow-core's own project writer, so Gyroflow and the plugin read the same
// lens profile, smoothing, zoom and refraction from it. The refraction as the player has the lens under water: Auto
// writes the camera's In water / Out of water as keyframes, which Gyroflow shows on its timeline. The horizon lock and
// the view as the player has them too: the wider view is Gyroflow's zoom off at that view's FOV, so the clip looks
// the same in Gyroflow, and played with the project (its own view) as it did before.

use gyroflow_core::{ GyroflowProjectType, filesystem, stabilization::ComputeParams };

use crate::water::Underwater;

/// The project JSON for `video` stabilized from its own motion data with Gyroflow's default preset and then `preset`,
/// loaded as the plugin plays it (`load_manager`).
/// `project_path`: where it will be saved (macOS bookmarks to the video are made relative to it)
/// `underwater`: the lens under water, not, or as the camera says (water.rs)
/// `horizon_lock`: the horizon lock's amount (%, 0 off) instead of the preset's
/// `wide`: the plugin's wider view, as Gyroflow has it: its zoom off, at the FOV `wide_fov` finds
pub fn from_motion_data(video: &str, preset: Option<&str>, underwater: Underwater, horizon_lock: Option<f64>, wide: bool, project_path: &str) -> Result<String, String> {
    let (stab, _) = crate::load_manager(video, preset, horizon_lock, underwater)?;
    if !stab.gyro.read().has_motion() {
        return Err(format!("{video} has no motion data"));
    }
    if wide {
        stab.invalidate_smoothing();
        stab.recompute_blocking();
        let fov = crate::wide_fov(&ComputeParams::from_manager(&stab), stab.params.read().fps);
        stab.set_adaptive_zoom(0.0);
        stab.set_fov(fov);
    }
    // load_video_file leaves the project's video to the app
    stab.input_file.write().url = filesystem::path_to_url(video);
    // The size the plugin renders it at (the video's, or the lens profile's output size), which Gyroflow's export
    // settings also take from here
    let (w, h) = stab.params.read().output_size;
    let output = serde_json::json!({ "output": { "output_width": w, "output_height": h } });
    // Simple: the motion data stays in the video, where Gyroflow and the plugin read it from
    stab.export_gyroflow_data(GyroflowProjectType::Simple, &output.to_string(), Some(&filesystem::path_to_url(project_path)))
        .map_err(|e| format!("Can't make a project of {video}: {e:?}"))
}
