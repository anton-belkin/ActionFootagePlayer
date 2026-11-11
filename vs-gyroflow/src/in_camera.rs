// SPDX-License-Identifier: GPL-3.0-or-later
//
// Clips the camera stabilized itself, stabilized further: GoPro's HyperSmooth on the HERO8 and later (issue #86).
// Those cameras record with each frame the camera's turn (CORI) and the turn HyperSmooth gave the picture (IORI),
// and telemetry-parser hands Gyroflow their product, the turn of the picture as the camera left it. So Gyroflow
// stabilizes what's left of the motion in the picture, as its app does ("since HERO8 you can record with in-camera
// stabilization on and then stabilize further"). The HERO5, 6 and 7 Black don't record IORI: their gyro is the
// camera's, of a picture HyperSmooth already turned, so they're stabilized only with HyperSmooth off (probe.rs).
//
// What differs from a clip the camera didn't stabilize is the crop. HyperSmooth cropped the frame for its own margin
// already (a HERO12's Linear 4K: 100.6° across the diagonal off, 95.5° on, 90° boosted), and what it leaves (a slow
// sway it lets through, a little shake) needs less: the levels crop CROP_SHARE of their usual share (levels.rs).
// Measured on HERO12 Black clips (README, Stabilization): at medium, half the crop smooths as much as all of it.
//
// Tried and left out: under water, taking IORI's tilt and pan 1/1.33 of their way (HyperSmooth corrects as in air,
// while the lens behind the flat glass sees a turn 1.33 times larger). On two clips under water it shook 2-3 times
// more than CORI × IORI as recorded, which already shook less than the clip.

use gyroflow_core::StabilizationManager;

/// The levels' crop on top of HyperSmooth, as a share of theirs
pub const CROP_SHARE: f64 = 0.5;

/// Whether the clip's motion data is the turn of a picture the camera stabilized itself: a GoPro whose HyperSmooth was
/// on (its EISA, as Gyroflow reads it for the lens profile) and that recorded how it turned the picture (IORI)
pub fn stabilized(stab: &StabilizationManager) -> bool {
    let on = stab.camera_id.read().as_ref().is_some_and(|id| id.brand == "GoPro" && hypersmooth_on(&id.additional));
    on && stab.gyro.read().file_metadata.read().image_orientations.as_ref().is_some_and(|io| !io.is_empty())
}

/// Gyroflow's camera identifier's `additional` for a GoPro: EISA as written ("HS EIS", "HS AutoBoost", ...), "EIS-Y",
/// or "NO-EIS" / "" (off, or not said)
fn hypersmooth_on(additional: &str) -> bool {
    !additional.is_empty() && additional != "NO-EIS"
}

/// The levels' crop for the clip, as a share of theirs
pub fn crop_share(stab: &StabilizationManager) -> f64 {
    if stabilized(stab) { CROP_SHARE } else { 1.0 }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hypersmooth_as_gyroflow_names_it() {
        for on in ["HS EIS", "HS AutoBoost", "HS HLevel", "EIS-Y"] { assert!(hypersmooth_on(on), "{on}"); }
        for off in ["", "NO-EIS"] { assert!(!hypersmooth_on(off), "{off}"); }
    }

    #[test]
    fn a_clip_without_motion_data_crops_all_of_the_level() {
        assert_eq!(crop_share(&StabilizationManager::default()), 1.0);
    }
}
