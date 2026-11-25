// SPDX-License-Identifier: GPL-3.0-or-later
//
// The player's own lens database (docs/optical-without-gyroflow.md, step 3): lens profiles in Gyroflow's own format,
// each kept for one exact camera mode, searched before Gyroflow's database. Built in: the profiles Anton estimated from
// his own footage (lens_profiles#41, in Gyroflow's database only from its v43). Then the user's own, beside the app's
// data (`user_dir`: <App Support>/ActionFootagePlayer/lenses): a lens assigned to a clip there is the default of its
// whole mode from then on (Anton, 2026-11-24).
//
// A clip's mode (`ModeKey`): its camera as the player names it, its lens mode (the curated list's type for it where its
// camera has types, else Gyroflow's name for the mode the camera recorded: "wide"), its in-camera stabilization as
// Gyroflow's camera identifier says it ("hseis"), its size and its frame rate. A profile here says the mode it's for in
// an "afp" block of its own ({"camera", "mode", "eis", "size": "WxH", "fps", "how", "from"}), which Gyroflow ignores:
// it's taken for a clip only when all of these are the clip's (a mode of "" stands for any: a camera that records none
// worth trusting, one lens a size). The rolling shutter's readout comes with the profile.

use std::path::PathBuf;
use std::sync::{ Arc, Mutex };
use gyroflow_core::lens_profile::LensProfile;
use crate::lens_pick::Clip;

/// The built-in profiles: (file name, its text)
const BUILT_IN: &[(&str, &str)] = &[
    ("DJI_Osmo_Action_6_Standard_FOV_RockSteady_off_4k_16by9_3840x2160-59.94fps.json", include_str!("../resources/lenses/DJI_Osmo_Action_6_Standard_FOV_RockSteady_off_4k_16by9_3840x2160-59.94fps.json")),
    ("GoPro_HERO3_White___1080p_16by9_1920x1080-29.97fps.json", include_str!("../resources/lenses/GoPro_HERO3_White___1080p_16by9_1920x1080-29.97fps.json")),
    ("GoPro_HERO4_Black_Medium__2.7k_16by9_2704x1520-23.98fps.json", include_str!("../resources/lenses/GoPro_HERO4_Black_Medium__2.7k_16by9_2704x1520-23.98fps.json")),
    ("GoPro_HERO4_Black_Medium__2.7k_16by9_2704x1520-29.97fps.json", include_str!("../resources/lenses/GoPro_HERO4_Black_Medium__2.7k_16by9_2704x1520-29.97fps.json")),
    ("GoPro_HERO4_Black_Medium__2.7k_16by9_2704x1520-59.94fps.json", include_str!("../resources/lenses/GoPro_HERO4_Black_Medium__2.7k_16by9_2704x1520-59.94fps.json")),
    ("GoPro_HERO4_Black_Wide__1080p_16by9_1920x1080-59.94fps.json", include_str!("../resources/lenses/GoPro_HERO4_Black_Wide__1080p_16by9_1920x1080-59.94fps.json")),
    ("GoPro_HERO4_Black_Wide__2.7k_16by9_2704x1520-59.94fps.json", include_str!("../resources/lenses/GoPro_HERO4_Black_Wide__2.7k_16by9_2704x1520-59.94fps.json")),
    ("GoPro_HERO4_Black_Wide__4k_16by9_3840x2160-29.97fps.json", include_str!("../resources/lenses/GoPro_HERO4_Black_Wide__4k_16by9_3840x2160-29.97fps.json")),
    ("GoPro_HERO7_Black_Linear_HS_EIS_2.7k_16by9_2704x1520-59.94fps.json", include_str!("../resources/lenses/GoPro_HERO7_Black_Linear_HS_EIS_2.7k_16by9_2704x1520-59.94fps.json")),
    ("GoPro_HERO7_Black_Wide_HS_EIS_2.7k_16by9_2704x1520-59.94fps.json", include_str!("../resources/lenses/GoPro_HERO7_Black_Wide_HS_EIS_2.7k_16by9_2704x1520-59.94fps.json")),
    ("GoPro_HERO7_Black_Wide_HS_EIS_4k_16by9_3840x2160-59.94fps.json", include_str!("../resources/lenses/GoPro_HERO7_Black_Wide_HS_EIS_4k_16by9_3840x2160-59.94fps.json")),
];

/// The mode a profile is for, or a clip's
#[derive(Debug, Clone, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ModeKey {
    pub camera: String,
    #[serde(default)] pub mode: String,
    #[serde(default)] pub eis: String,
    /// "WxH"
    pub size: String,
    pub fps: f64,
}

fn norm(s: &str) -> String { s.to_lowercase().chars().filter(|c| c.is_alphanumeric()).collect() }

fn size_of(s: &str) -> Option<(usize, usize)> {
    let (w, h) = s.split_once('x')?;
    Some((w.trim().parse().ok()?, h.trim().parse().ok()?))
}

impl ModeKey {
    /// A clip's (`path` for its lens type: the one given to it, recorded, or its camera's default)
    pub fn of(path: &str, clip: &Clip) -> ModeKey {
        let mode = clip.lens_type(path).unwrap_or_else(|| clip.lens_mode.clone());
        // As Gyroflow's identifier says it ("HS EIS"), else as the camera's own metadata does (a DJI's EIS: "EIS:
        // RockSteady"; off: "")
        let eis = clip.id.as_ref().map(|i| i.additional.clone()).filter(|a| !a.is_empty()).or_else(|| clip.in_camera.clone()).unwrap_or_default();
        ModeKey { camera: clip.camera.clone(), mode: norm(&mode), eis: norm(&eis), size: format!("{}x{}", clip.width, clip.height), fps: clip.fps }
    }

    /// A profile for this mode (`self`) fits a clip's (`clip`): the same camera, mode (or any), in-camera
    /// stabilization, size (turned or not) and frame rate (within 0.5 %: 23.976 and 23.98)
    pub fn fits(&self, clip: &ModeKey) -> bool {
        if norm(&self.camera) != norm(&clip.camera) || norm(&self.eis) != norm(&clip.eis) { return false; }
        if !self.mode.is_empty() && norm(&self.mode) != norm(&clip.mode) { return false; }
        let (Some(a), Some(b)) = (size_of(&self.size), size_of(&clip.size)) else { return false };
        if a != b && a != (b.1, b.0) { return false; }
        self.fps > 0.0 && clip.fps > 0.0 && (self.fps / clip.fps - 1.0).abs() < 0.005
    }

    /// In a few words: "GoPro HERO4 Black · medium · 2704x1520 · 29.97 fps"
    pub fn describe(&self) -> String {
        let mut parts = vec![self.camera.clone()];
        if !self.mode.is_empty() { parts.push(self.mode.clone()); }
        if !self.eis.is_empty() { parts.push(self.eis.clone()); }
        parts.push(self.size.clone());
        parts.push(format!("{} fps", (self.fps * 100.0).round() / 100.0));
        parts.join(" · ")
    }
}

/// A profile of the player's own database
#[derive(Clone)]
pub struct LocalLens {
    /// Its file's name
    pub name: String,
    /// The user's (in `user_dir`), not built in
    pub own: bool,
    pub profile: LensProfile,
    pub key: ModeKey,
    /// "estimated from footage", "assigned"
    pub how: String,
    /// The profile's own name ("GoPro_HERO7 Black_Wide_HS EIS_..."), its file's where it has none
    pub shown: String,
}

impl LocalLens {
    fn parse(name: &str, text: &str, own: bool) -> Result<LocalLens, String> {
        let v: serde_json::Value = serde_json::from_str(text).map_err(|e| format!("{name}: {e}"))?;
        let key: ModeKey = serde_json::from_value(v.get("afp").cloned().ok_or_else(|| format!("{name}: no \"afp\" block saying its mode"))?)
            .map_err(|e| format!("{name}: its \"afp\" block: {e}"))?;
        let profile = LensProfile::from_json(text).map_err(|e| format!("{name}: {e}"))?;
        if profile.fisheye_params.camera_matrix.len() != 3 { return Err(format!("{name}: no calibration in it")); }
        // The built-in ones (lens_profiles#41) say "built in", so they don't look chosen by the user
        let how = if own { v.pointer("/afp/how").and_then(|h| h.as_str()).unwrap_or_default().to_string() } else { "built in".to_string() };
        let shown = v.get("name").and_then(|n| n.as_str()).filter(|n| !n.is_empty()).unwrap_or(name.trim_end_matches(".json"))
            .replace(" - estimated from footage", "");
        Ok(LocalLens { name: name.to_string(), own, profile, key, how, shown })
    }

    /// As the player shows it: "HERO4 Black Medium 2.7K 29.97, estimated from footage"
    pub fn label(&self) -> String {
        let name = self.shown.replace('_', " ").split_whitespace().collect::<Vec<_>>().join(" ");
        if self.how.is_empty() { name } else { format!("{name}, {}", self.how) }
    }
}

/// Where the user's own profiles are: `lenses` beside the user's curated list (lens_pick::user_file), where a
/// Gyroflow project's lens saved as a default (#231) goes too. Profiles there without an "afp" block are the list's
pub fn user_dir() -> Option<PathBuf> {
    Some(crate::lens_pick::user_file()?.parent()?.join("lenses"))
}

/// The user's folder as it is now: what tells its versions apart ("" without one)
pub fn user_stamp() -> String {
    let Some(dir) = user_dir() else { return String::new() };
    let mut items: Vec<String> = std::fs::read_dir(&dir).into_iter().flatten().flatten()
        .filter_map(|e| { let m = e.metadata().ok()?; Some(format!("{}:{}:{:?}", e.file_name().to_string_lossy(), m.len(), m.modified().ok())) })
        .collect();
    items.sort();
    items.join("|")
}

/// Every profile of the database: the user's first (they win), then the built-in ones. Read again when the user's
/// folder changes
pub fn all() -> Arc<Vec<LocalLens>> {
    static CACHE: Mutex<Option<(String, Arc<Vec<LocalLens>>)>> = Mutex::new(None);
    let stamp = user_stamp();
    let mut cache = CACHE.lock().unwrap();
    if let Some((s, v)) = cache.as_ref() { if *s == stamp { return v.clone(); } }
    let v = Arc::new(read_all(user_dir().as_deref()));
    *cache = Some((stamp, v.clone()));
    v
}

/// The user's profiles in `dir`, then the built-in ones
fn read_all(dir: Option<&std::path::Path>) -> Vec<LocalLens> {
    let mut out = Vec::new();
    if let Some(dir) = dir {
        let mut files: Vec<PathBuf> = std::fs::read_dir(&dir).into_iter().flatten().flatten().map(|e| e.path())
            .filter(|p| p.extension().is_some_and(|e| e == "json")).collect();
        files.sort();
        for f in files {
            let Ok(text) = std::fs::read_to_string(&f) else { continue };
            // A profile of the curated list's (saved from a project, #231): no mode of its own
            if !text.contains("\"afp\"") { continue; }
            match LocalLens::parse(&f.file_name().unwrap_or_default().to_string_lossy(), &text, true) {
                Ok(l) => out.push(l),
                Err(e) => log::warn!("Own lens database: {e}"),
            }
        }
    }
    for (name, text) in BUILT_IN {
        match LocalLens::parse(name, text, false) {
            Ok(l) => out.push(l),
            Err(e) => log::error!("Built-in lens {e}"),
        }
    }
    out
}

/// The profile for a clip's mode, the user's before the built-in ones
pub fn find(key: &ModeKey) -> Option<LocalLens> {
    all().iter().find(|l| l.key.fits(key)).cloned()
}

/// A file name for a mode's profile in the user's folder
fn file_name_for(key: &ModeKey) -> String {
    let s = format!("{}-{}-{}-{}-{}", key.camera, if key.mode.is_empty() { "any" } else { &key.mode }, if key.eis.is_empty() { "no-eis" } else { &key.eis },
                    key.size, (key.fps * 1000.0).round() as i64);
    let s: String = s.chars().map(|c| if c.is_alphanumeric() || c == '-' || c == '.' { c } else { '_' }).collect();
    format!("{s}.json")
}

/// `profile` kept as the lens of the clip's mode (`key`) in the user's folder, in place of what was there for it:
/// from now on every clip of that mode takes it. `how`: "assigned"; `from`: where it came from (a database key). The
/// file written
pub fn assign(key: &ModeKey, profile: &LensProfile, how: &str, from: &str) -> Result<PathBuf, String> {
    assign_in(&user_dir().ok_or("no folder for the app's data")?, key, profile, how, from)
}

fn assign_in(dir: &std::path::Path, key: &ModeKey, profile: &LensProfile, how: &str, from: &str) -> Result<PathBuf, String> {
    std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    forget_in(dir, key)?;
    let mut v: serde_json::Value = serde_json::from_str(&profile.get_json().map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
    // gyroflow-core calls a profile without one InvalidData (GOPR1585: its readout went unused)
    if v["calibrator_version"].as_str().unwrap_or_default().is_empty() { v["calibrator_version"] = format!("Action Footage Player, {how}").into(); }
    let mut afp = serde_json::to_value(key).map_err(|e| e.to_string())?;
    afp["how"] = how.into();
    afp["from"] = from.into();
    afp["date"] = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or_default().into();
    v["afp"] = afp;
    let file = dir.join(file_name_for(key));
    let tmp = file.with_extension("json.tmp");
    std::fs::write(&tmp, serde_json::to_string_pretty(&v).unwrap_or_default()).and_then(|_| std::fs::rename(&tmp, &file))
        .map_err(|e| format!("{}: {e}", file.display()))?;
    Ok(file)
}

/// The user's profiles for the clip's mode removed (the built-in ones stay): how many
pub fn forget(key: &ModeKey) -> Result<usize, String> {
    match user_dir() { Some(dir) => forget_in(&dir, key), None => Ok(0) }
}

fn forget_in(dir: &std::path::Path, key: &ModeKey) -> Result<usize, String> {
    let mut n = 0;
    for l in read_all(Some(dir)).iter().filter(|l| l.own && l.key.fits(key)) {
        std::fs::remove_file(dir.join(&l.name)).map_err(|e| format!("{}: {e}", l.name))?;
        n += 1;
    }
    Ok(n)
}

#[cfg(test)]
mod tests {
    use super::*;

    // GOPR1585: a built-in lens loaded without its readout (an empty calibrator_version made gyroflow-core say
    // InvalidData, and the readout wasn't set), so the clip was measured and played with no rolling shutter
    #[test]
    fn built_in_lenses_load_with_their_readout() {
        let pick = |p: &LensProfile, n: &str| crate::lens_pick::Pick { profile: p.clone(), name: n.into(), official: false, votes: (0, 0), exact: true,
                                                                     curated: None, focal_only: false, local: Some("built in".into()) };
        for (name, text) in BUILT_IN {
            let l = LocalLens::parse(name, text, false).unwrap();
            assert!(!l.profile.calibrator_version.is_empty(), "{name}: no calibrator_version");
            let stab = crate::lens_pick::manager();
            assert!(stab.load_lens_profile(&l.profile.get_json().unwrap()).is_ok(), "{name}: gyroflow-core doesn't load it");
            let stab = crate::lens_pick::manager();
            assert!(crate::lens_pick::load_into(&stab, &pick(&l.profile, name)));
            assert_eq!(stab.params.read().frame_readout_time, l.profile.frame_readout_time.unwrap_or(0.0).abs(), "{name}: its readout isn't set");
        }
        // One that gyroflow-core calls InvalidData all the same keeps its readout
        let mut l = LocalLens::parse(BUILT_IN[2].0, BUILT_IN[2].1, false).unwrap();
        l.profile.calibrator_version.clear();
        let stab = crate::lens_pick::manager();
        assert!(crate::lens_pick::load_into(&stab, &pick(&l.profile, "empty")));
        assert_eq!(stab.params.read().frame_readout_time, 30.6);
    }

    #[test]
    fn built_in_ones_are_named_so() {
        for (name, text) in BUILT_IN {
            let l = LocalLens::parse(name, text, false).unwrap();
            assert_eq!(l.how, "built in", "{name}");
            assert!(l.label().ends_with(", built in"), "{}", l.label());
        }
    }

    #[test]
    fn built_in_profiles_read() {
        let all: Vec<LocalLens> = BUILT_IN.iter().map(|(n, t)| LocalLens::parse(n, t, false).unwrap()).collect();
        assert_eq!(all.len(), 11);
        for l in &all {
            assert!(l.profile.frame_readout_time.is_some() || l.key.eis == "hseis", "{}: no readout", l.name);
            assert!(l.how == "built in");
        }
    }

    #[test]
    fn fits_only_the_exact_mode() {
        let p = ModeKey { camera: "GoPro HERO4 Black".into(), mode: "medium".into(), eis: "".into(), size: "2704x1520".into(), fps: 29.97 };
        let clip = |mode: &str, size: &str, fps: f64| ModeKey { camera: "GoPro HERO4 Black".into(), mode: mode.into(), eis: "".into(), size: size.into(), fps };
        assert!(p.fits(&clip("medium", "2704x1520", 29.97002997)));
        assert!(p.fits(&clip("Medium", "1520x2704", 29.97)), "turned, the mode's name in other letters");
        assert!(!p.fits(&clip("wide", "2704x1520", 29.97)));
        assert!(!p.fits(&clip("medium", "2704x1520", 59.94)));
        assert!(!p.fits(&clip("medium", "3840x2160", 29.97)));
        assert!(!p.fits(&ModeKey { eis: "hseis".into(), ..clip("medium", "2704x1520", 29.97) }));
        let any = ModeKey { mode: "".into(), ..p.clone() };
        assert!(any.fits(&clip("any", "2704x1520", 29.97)), "a profile for any mode");
        let hd = ModeKey { fps: 23.976, ..p.clone() };
        assert!(hd.fits(&clip("medium", "2704x1520", 23.98)));
    }

    #[test]
    fn assigned_and_forgotten() {
        let dir = std::env::temp_dir().join(format!("afp-lens-db-{}", std::process::id())).join("lenses");
        let _ = std::fs::remove_dir_all(&dir);
        let key = ModeKey { camera: "GoPro HERO9 Black".into(), mode: "wide".into(), eis: "".into(), size: "3840x2160".into(), fps: 29.97 };
        let find_in = |k: &ModeKey| read_all(Some(&dir)).into_iter().find(|l| l.key.fits(k));
        assert!(find_in(&key).is_none());
        let profile = LocalLens::parse(BUILT_IN[5].0, BUILT_IN[5].1, false).unwrap().profile;
        let file = assign_in(&dir, &key, &profile, "assigned", "test").unwrap();
        assert!(file.starts_with(&dir));
        let found = find_in(&key).unwrap();
        assert!(found.own && found.how == "assigned");
        // One found in the footage with no calibrator_version (Find's pick of a built-in-like profile): written with one
        let mut bare = profile.clone();
        bare.calibrator_version.clear();
        let file = assign_in(&dir, &key, &bare, "found in the footage", "test").unwrap();
        let written: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&file).unwrap()).unwrap();
        assert!(!written["calibrator_version"].as_str().unwrap().is_empty());
        // Assigned again: one file for the mode, the new one
        assign_in(&dir, &key, &profile, "assigned", "again").unwrap();
        assert_eq!(read_all(Some(&dir)).iter().filter(|l| l.own).count(), 1);
        assert_eq!(forget_in(&dir, &key).unwrap(), 1);
        assert!(find_in(&key).is_none());
        let _ = std::fs::remove_dir_all(dir.parent().unwrap());
    }
}
