// SPDX-License-Identifier: GPL-3.0-or-later
//
// The lists the player, gyroflow-export and the Library share (mpv/scripts/workspace/registry.json): the curves, an
// export's settings, the transform's keys, the project's own folders. The types are here, for every program that
// links the catalog; each embeds the file itself (include_str!, its path relative to the crate) and parses it once:
//
//   static REG: OnceLock<Registry> = OnceLock::new();
//   REG.get_or_init(|| afp_catalog::registry::parse(include_str!("../../mpv/scripts/workspace/registry.json")))
//
// The catalog's own build (catalog/Dockerfile) is from its folder alone, so it doesn't embed it: inspect.rs's
// OWN_FOLDERS is checked against it by a test.

use serde::Deserialize;
use serde_json::Value;

/// The file's path from the repository's folder
pub const PATH: &str = "mpv/scripts/workspace/registry.json";

#[derive(Debug, Clone, Deserialize)]
pub struct Registry {
    pub curves: Vec<Curve>,
    pub export: Export,
    pub settings: Vec<Setting>,
    pub own_folders: Vec<String>,
    pub recycle_bins: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Curve {
    pub id: String,
    pub name: String,
    /// gyroflow-export's name for it, where it's another
    pub export_name: Option<String>,
    /// A log curve: in Ctrl+l's round, a source's curve in the Library
    pub log: bool,
    /// gyroflow-export's LUT to HLG (luts/), None where it's made per clip (uw)
    pub lut: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Export {
    pub settings: Vec<ExportSetting>,
    pub kinds: ExportKinds,
    pub photo_radius_max: usize,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ExportSetting {
    pub key: String,
    pub flag: Option<String>,
    pub choices: Vec<Choice>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Choice {
    pub value: String,
    pub label: String,
    pub note: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ExportKinds {
    pub video: Vec<String>,
    pub photo: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Setting {
    pub key: String,
    /// bundle, uw, input, opts
    pub group: String,
    /// enum, bool, number, ratio, curve
    pub kind: String,
    #[serde(default)]
    pub values: Vec<String>,
    pub min: Option<f64>,
    pub max: Option<f64>,
    pub neutral: Option<Value>,
    #[serde(default)]
    pub project: bool,
    pub conf: Option<String>,
    pub conf_default: Option<Value>,
    #[serde(default)]
    pub not_in_conf: Vec<String>,
    pub recipe: bool,
    pub exported: bool,
    #[serde(default)]
    pub cli: Vec<String>,
    pub gpu: Option<String>,
}

pub fn parse(text: &str) -> Registry {
    serde_json::from_str(text).unwrap_or_else(|e| panic!("{PATH}: {e}"))
}

impl Registry {
    pub fn curve(&self, id: &str) -> Option<&Curve> { self.curves.iter().find(|c| c.id == id) }
    pub fn setting(&self, key: &str) -> Option<&Setting> { self.settings.iter().find(|s| s.key == key) }
    /// The settings' keys of a group, in order
    pub fn keys(&self, group: &str) -> Vec<&str> { self.settings.iter().filter(|s| s.group == group).map(|s| s.key.as_str()).collect() }
    /// Each export setting's values, in order (the first its default)
    pub fn export_choices(&self) -> Vec<(&str, Vec<&str>)> {
        self.export.settings.iter().map(|s| (s.key.as_str(), s.choices.iter().map(|c| c.value.as_str()).collect())).collect()
    }
    /// A project's own folder, lower case (recycle: the NAS's recycle bins too)
    pub fn own_folder(&self, lower: &str, recycle: bool) -> bool {
        self.own_folders.iter().any(|x| x == lower) || (recycle && self.recycle_bins.iter().any(|x| x == lower))
    }
}

impl Setting {
    /// A value this setting takes (a bundle's or the underwater colour's: what transform.lua's valid() takes)
    pub fn valid(&self, v: &Value) -> bool {
        match self.kind.as_str() {
            "bool" => v.is_boolean(),
            "number" => v.as_f64().is_some_and(|x| self.min.is_none_or(|m| x >= m) && self.max.is_none_or(|m| x <= m)),
            "ratio" => {
                let s = v.as_str().unwrap_or("");
                match s.split_once(':') {
                    // Any ratio landscape-wise (W >= H)
                    Some((w, h)) if !w.is_empty() && !h.is_empty() && w.chars().chain(h.chars()).all(|c| c.is_ascii_digit()) => {
                        let (w, h): (f64, f64) = (w.parse().unwrap_or(0.0), h.parse().unwrap_or(0.0));
                        w >= h && h > 0.0
                    }
                    _ => self.values.iter().any(|x| x == s),
                }
            }
            _ => v.as_str().is_some_and(|s| self.values.iter().any(|x| x == s)),
        }
    }
    /// The values gyroflow.conf's option can say (the others: its default)
    pub fn conf_values(&self) -> Vec<&str> {
        self.values.iter().filter(|v| !self.not_in_conf.contains(v)).map(String::as_str).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn repo_registry() -> Registry {
        let f = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join(PATH);
        parse(&std::fs::read_to_string(&f).unwrap_or_else(|e| panic!("{}: {e}", f.display())))
    }

    /// inspect.rs's own folders (a const: the catalog's own build doesn't have the file) are the registry's
    #[test]
    fn own_folders_are_the_registry_s() {
        let r = repo_registry();
        let mut want: Vec<&str> = r.own_folders.iter().chain(&r.recycle_bins).map(String::as_str).collect();
        let mut have: Vec<&str> = crate::inspect::OWN_FOLDERS.to_vec();
        want.sort();
        have.sort();
        assert_eq!(have, want, "catalog/src/inspect.rs OWN_FOLDERS vs {PATH}");
    }

    /// range.rs's names for a source's curve (a match) know each of the registry's log curves
    #[test]
    fn range_knows_the_log_curves() {
        for c in repo_registry().curves.iter().filter(|c| c.log) {
            assert_eq!(crate::range::log_curve(&c.id), c.id, "catalog/src/range.rs log_curve vs {PATH}");
        }
    }

    #[test]
    fn it_reads() {
        let r = repo_registry();
        assert!(r.curve("dlogm").is_some_and(|c| c.log) && r.curve("uw").is_some_and(|c| !c.log && c.lut.is_none()));
        assert_eq!(r.keys("uw"), ["uw", "uw_highlights", "uw_strength"]);
        let s = r.setting("uw_strength").unwrap();
        assert!(s.valid(&serde_json::json!(0.5)) && !s.valid(&serde_json::json!(1.5)) && !s.valid(&serde_json::json!("0.5")));
        let a = r.setting("aspect").unwrap();
        assert!(a.valid(&serde_json::json!("original")) && a.valid(&serde_json::json!("21:9")) && !a.valid(&serde_json::json!("9:16")));
        assert_eq!(r.setting("projection").unwrap().conf_values().len(), 5);
    }
}
