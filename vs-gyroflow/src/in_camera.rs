// SPDX-License-Identifier: GPL-3.0-or-later
//
// Clips the camera stabilized itself, stabilized further: GoPro's HyperSmooth on the HERO12 and later (issue #86).
// GoPros from the HERO8 on record with each frame the camera's turn (CORI) and the turn HyperSmooth gave the picture
// (IORI), and telemetry-parser hands Gyroflow their product, the turn of the picture as the camera left it, so
// Gyroflow stabilizes what's left of the motion in the picture. Gyroflow's GoPro page
// (docs.gyroflow.xyz/app/getting-started/supported-cameras/gopro) lists HyperSmooth as supported from the HERO8 on,
// but only the HERO12 and later are taken here: Anton's call on PR #201 (HERO11 and older aren't re-stabilized; they
// play as recorded with HyperSmooth on), and only HERO12 Black clips were tried. The HERO5, 6 and 7 Black don't
// record IORI at all: their gyro is the camera's, of a picture HyperSmooth already turned (probe.rs).
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
    let on = stab.camera_id.read().as_ref().is_some_and(|id| id.brand == "GoPro" && hypersmooth_on(&id.additional) && restabilized_model(&id.model));
    on && stab.gyro.read().file_metadata.read().image_orientations.as_ref().is_some_and(|io| !io.is_empty())
}

/// Whether a GoPro model's HyperSmooth clips are stabilized further: the HERO12 Black and later HEROs ("HERO12 Black",
/// "HERO13 Black" as MINF names them, "GoPro " in front or not). Not the HERO11 and older, nor a model without a HERO
/// number (MAX, Fusion, MISSION)
pub fn restabilized_model(model: &str) -> bool {
    let upper = model.to_ascii_uppercase();
    let Some(i) = upper.find("HERO") else { return false };
    let digits: String = upper[i + 4..].trim_start().chars().take_while(|c| c.is_ascii_digit()).collect();
    digits.parse::<u32>().is_ok_and(|n| n >= 12)
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
    fn from_the_hero12_on() {
        for yes in ["HERO12 Black", "HERO13 Black", "GoPro HERO12 Black", "HERO 14"] { assert!(restabilized_model(yes), "{yes}"); }
        for no in ["HERO11 Black", "HERO11 Black Mini", "HERO10 Black", "HERO8 Black", "HERO7 Black", "MAX", "Fusion", ""] {
            assert!(!restabilized_model(no), "{no}");
        }
    }

    #[test]
    fn a_clip_without_motion_data_crops_all_of_the_level() {
        assert_eq!(crop_share(&StabilizationManager::default()), 1.0);
    }
}
