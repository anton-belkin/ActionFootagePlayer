// The lists the player, gyroflow-export and the Library share (mpv/scripts/workspace/registry.json, afp_catalog::registry's
// types): embedded here, parsed once
use afp_catalog::registry::{ Curve, Registry };
use std::sync::OnceLock;

pub const TEXT: &str = include_str!("../../mpv/scripts/workspace/registry.json");

pub fn get() -> &'static Registry {
    static REG: OnceLock<Registry> = OnceLock::new();
    REG.get_or_init(|| afp_catalog::registry::parse(TEXT))
}

/// A curve gyroflow-export takes (--curve): every one of the registry's, the underwater colour's too
pub fn curve(id: &str) -> Option<&'static Curve> { get().curve(id) }

/// A curve's name as gyroflow-export says it
pub fn curve_name(id: &str) -> &str {
    curve(id).map_or(id, |c| c.export_name.as_deref().unwrap_or(&c.name))
}

/// A curve's LUT to HLG, the file's name in luts/ (the underwater colour's is made per clip)
pub fn curve_lut(id: &str) -> String {
    curve(id).and_then(|c| c.lut.clone()).unwrap_or_else(|| format!("{id}_hlg.cube"))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// still.rs's radius at its best (a const, an array's size) is the registry's, which the player's Save frame offers
    #[test]
    fn photo_radius_max_is_the_registry_s() {
        assert_eq!(crate::still::MAX_RADIUS, get().export.photo_radius_max);
    }

    #[test]
    fn names_and_luts() {
        assert_eq!(curve_name("ilog"), "I-Log");
        assert_eq!(curve_name("dlogm"), "D-Log M");
        assert_eq!(curve_name("uw"), "Underwater colour");
        assert_eq!(curve_lut("gplog2"), "gplog2_hlg.cube");
        // Each log curve's LUT is in the repository or made by every packager (tests/curves_twin_test.lua)
        for c in get().curves.iter().filter(|c| c.log) { assert!(c.lut.is_some(), "{}: no LUT", c.id); }
    }
}
