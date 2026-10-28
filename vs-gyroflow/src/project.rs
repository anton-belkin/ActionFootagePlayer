// SPDX-License-Identifier: GPL-3.0-or-later
//
// The Gyroflow project the plugin stabilizes a video with when it has none (its own motion data, Gyroflow's default
// preset and then the plugin's), written out so the video can be opened in Gyroflow as it plays. Loaded the way
// Engine::load loads it, and saved by gyroflow-core's own project writer, so Gyroflow and the plugin read the same
// lens profile, smoothing, zoom and refraction from it. The refraction as the player has the lens under water: Auto
// writes the camera's In water / Out of water as keyframes, which Gyroflow shows on its timeline.

use std::sync::{ Arc, atomic::AtomicBool };
use gyroflow_core::{ StabilizationManager, GyroflowProjectType, filesystem };

use crate::{ prefetch::Prefetched, water::{ self, Underwater } };

/// The project JSON for `video` stabilized from its own motion data with Gyroflow's default preset and then `preset`.
/// `project_path`: where it will be saved (macOS bookmarks to the video are made relative to it)
/// `underwater`: the lens under water, not, or as the camera says (water.rs)
pub fn from_motion_data(video: &str, preset: Option<&str>, underwater: Underwater, project_path: &str) -> Result<String, String> {
    let stab = StabilizationManager::default();
    let url = filesystem::path_to_url(video);
    let (mut content, size) = Prefetched::open(video).map_err(|e| format!("Can't open {video}: {e}"))?;
    stab.load_video_file(&mut content, size, &url, None, true).map_err(|e| format!("Can't load {video}: {e:?}"))?;
    if !stab.gyro.read().has_motion() {
        return Err(format!("{video} has no motion data"));
    }
    if let Some(preset) = preset {
        stab.import_gyroflow_file(&filesystem::path_to_url(preset), true, |_| (), Arc::new(AtomicBool::new(false)), true)
            .map_err(|e| format!("Can't load preset {preset}: {e:?}"))?;
    }
    water::apply(&stab, underwater, video);
    // load_video_file leaves the project's video to the app
    stab.input_file.write().url = url;
    // The size the plugin renders it at (the video's, or the lens profile's output size), which Gyroflow's export
    // settings also take from here
    let (w, h) = stab.params.read().output_size;
    let output = serde_json::json!({ "output": { "output_width": w, "output_height": h } });
    // Simple: the motion data stays in the video, where Gyroflow and the plugin read it from
    stab.export_gyroflow_data(GyroflowProjectType::Simple, &output.to_string(), Some(&filesystem::path_to_url(project_path)))
        .map_err(|e| format!("Can't make a project of {video}: {e:?}"))
}
