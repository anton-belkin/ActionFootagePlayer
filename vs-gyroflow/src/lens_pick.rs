// SPDX-License-Identifier: GPL-3.0-or-later
//
// The lens profile for a clip that doesn't carry its own lens, picked from Gyroflow's database (the one built into
// gyroflow-core) as a person would in Gyroflow's lens profile list. Gyroflow itself loads one only on an exact match of
// its camera identifier: brand, model, lens mode, resolution, frame rate and in-camera stabilization ("gopro-hero7black-
// wide-3840x2160@59940-hseis"). A HERO7 clip in Wide at 4K 24 fps with HyperSmooth gets nothing then, while the database
// has a dozen profiles of that camera and mode. And when several profiles share the identifier, Gyroflow takes the
// first it read, not the best.
//
// Here a profile qualifies when it's of the same camera (Gyroflow's own model aliases: a HERO12/13 is a HERO11, a
// HERO11 Mini a HERO11), the same lens mode the camera recorded (GoPro's VFOV: Wide, Linear, SuperView...) and the
// same aspect ratio. The best of those, in this order:
//   1. Gyroflow's identifier, exactly (resolution, frame rate and in-camera stabilization all the same)
//   2. the same in-camera stabilization (HyperSmooth on or off changes the picture the lens is fitted to)
//   3. the nearest resolution, then 4. the nearest frame rate: what matters is the lens mode and the aspect ratio, these
//      only a little (some modes crop the sensor; the readout time, for the rolling shutter, goes with the rate).
//      Within 1 % is the same (59.94 and 60, 2704 and 2688)
//   5. official (calibrated by Gyroflow's authors, which Gyroflow shows instead of stars)
//   6. the most stars: Gyroflow users' votes, good / (good + bad) as Gyroflow shows them, a profile with few votes
//      counted closer to the middle (2.5 stars with none) so that one 5-star vote doesn't beat 40 good of 45
// A profile its users mostly voted down (under 1.5 stars from 3 votes or more) is taken only when there's nothing else.
// The votes are a snapshot (resources/lens_ratings.txt, tools/update_lens_ratings.sh): the player reads nothing online.

use std::collections::HashMap;
use std::sync::OnceLock;
use gyroflow_core::{ StabilizationManager, camera_identifier::CameraIdentifier, lens_profile::LensProfile, lens_profile_database::LensProfileDatabase };

/// A profile picked for a clip
#[derive(Clone, Debug)]
pub struct Pick {
    pub profile: LensProfile,
    /// As Gyroflow's list names it ("GoPro HERO7 Black 4k 16:9 Wide HS EIS 3840x2160 59.94fps")
    pub name: String,
    pub official: bool,
    /// The votes for it (good, bad)
    pub votes: (u32, u32),
    /// Gyroflow's identifier is the clip's, exactly: Gyroflow would load a profile by that name too
    pub exact: bool,
    /// Taken from the curated list for this lens type (its label, "Normal"), not picked from the database
    pub curated: Option<String>,
}

impl Pick {
    /// What the player says it is: its name, then official or its stars, and "nearest" when it isn't made for exactly
    /// this resolution, frame rate and stabilization; a curated one: its type and name
    pub fn label(&self) -> String {
        if let Some(t) = &self.curated { return format!("{t}, from the curated list: {}", self.name); }
        let (good, bad) = self.votes;
        let how = if self.official { "official".to_string() }
                  else if good + bad == 0 { "no votes".to_string() }
                  else { format!("{:.1}★, {} votes", 5.0 * good as f64 / (good + bad) as f64, good + bad) };
        format!("{}{} ({how})", if self.exact { "" } else { "Nearest: " }, self.name)
    }
}

/// Gyroflow's database, loaded once, with its list (what its lens profile search shows) made
fn db() -> &'static LensProfileDatabase {
    static DB: OnceLock<LensProfileDatabase> = OnceLock::new();
    DB.get_or_init(|| {
        let mut db = LensProfileDatabase::default();
        db.load_all();
        db.prepare_list_for_ui();
        db
    })
}

/// The votes, by the profile's checksum (Gyroflow's: LensProfileDatabase::load_all)
fn votes() -> &'static HashMap<String, (u32, u32)> {
    static VOTES: OnceLock<HashMap<String, (u32, u32)>> = OnceLock::new();
    VOTES.get_or_init(|| parse_votes(include_str!("../resources/lens_ratings.txt")))
}

fn parse_votes(text: &str) -> HashMap<String, (u32, u32)> {
    text.lines().filter(|l| !l.starts_with('#')).filter_map(|l| {
        let mut it = l.split_ascii_whitespace();
        let crc = it.next()?.to_string();
        Some((crc, (it.next()?.parse().ok()?, it.next()?.parse().ok()?)))
    }).collect()
}

// An identifier as Gyroflow compares them (CameraIdentifier::get_identifier, get_identifier_for_autoload), with the
// lens modes Gyroflow's camera identifier and profile authors spell differently made the same
fn normalized(id: &str) -> String {
    let mut s = id.to_lowercase().replace(' ', "");
    while s.contains("--") { s = s.replace("--", "-"); }
    s.trim_matches('-')
        .replace("hero12", "hero11").replace("hero13", "hero11").replace("hero11blackmini", "hero11black")
        .replace("superview", "super").replace("hyperview", "hyper")
}

// What an identifier says before the resolution: the camera and its lens ("gopro-hero7black-wide-"), and the rest
// ("3840x2160@59940-hseis"). None when it has no resolution where Gyroflow puts it
fn split_identifier(id: &str) -> Option<(&str, &str)> {
    let at = id.find('@')?;
    let start = id[..at].rfind('-')? + 1;
    let res = &id[start..at];
    let (w, h) = res.split_once('x')?;
    if w.is_empty() || h.is_empty() || !w.bytes().chain(h.bytes()).all(|b| b.is_ascii_digit()) { return None; }
    Some((&id[..start], &id[start..]))
}

/// Gyroflow's identifier for a profile that has none: its camera and lens as the identifier would start
fn profile_prefix(p: &LensProfile) -> String {
    if !p.identifier.is_empty() {
        let id = normalized(&p.identifier);
        return split_identifier(&id).map(|(prefix, _)| prefix.to_string()).unwrap_or_default();
    }
    if p.camera_brand.is_empty() || p.camera_model.is_empty() || p.lens_model.is_empty() { return String::new(); }
    format!("{}-", normalized(&format!("{}-{}-{}", p.camera_brand, p.camera_model, p.lens_model)))
}

fn aspect(w: usize, h: usize) -> f64 { w as f64 / h.max(1) as f64 }

// How a qualifying profile ranks: larger is better, compared in order
#[derive(PartialEq, PartialOrd)]
struct Rank { not_voted_down: bool, exact: bool, same_eis: bool, size: i64, fps: i64, official: bool, stars: f64 }

// How near a profile's value is to the clip's, larger is nearer: minus the ratio's log in percent (0 within 1 %);
// unknown (0) is the farthest
fn nearness(of_profile: f64, of_clip: f64) -> i64 {
    if of_profile <= 0.0 || of_clip <= 0.0 { return i64::MIN; }
    -((of_profile / of_clip).ln().abs() * 100.0).round() as i64
}

/// The best profile in the database for a clip, as the module's header says; None when nothing qualifies
pub fn pick(id: &CameraIdentifier) -> Option<Pick> {
    pick_in(db(), votes(), id)
}

fn pick_in(db: &LensProfileDatabase, votes: &HashMap<String, (u32, u32)>, id: &CameraIdentifier) -> Option<Pick> {
    let wanted = normalized(&id.get_identifier_for_autoload());
    let (prefix, _) = split_identifier(&wanted)?;
    // Its in-camera stabilization as the identifier ends ("-hseis", "-no-eis"); "" when the camera doesn't say
    let eis = if id.additional.is_empty() { String::new() } else { format!("-{}", normalized(&id.additional)) };
    let (w, h, fps) = (id.video_width, id.video_height, id.fps as f64 / 1000.0);
    if w == 0 || h == 0 { return None; }

    // The camera as Gyroflow's list names it: the search matches every word in a profile's name
    let model = id.model.replace("HERO12", "HERO11").replace("HERO13", "HERO11").replace("Black Mini", "Black");
    let query = format!("{} {}", id.brand, model);
    let mut best: Option<(Rank, Pick)> = None;
    for (name, key, crc, official, _, _, _) in db.search(&query, &Default::default(), 0, 0) {
        if key.ends_with(".gyroflow") { continue; }
        let Some(parent) = db.get_by_id(&key) else { continue };
        let (good, bad) = votes.get(&crc).copied().unwrap_or((0, 0));
        // One profile file can be made for several resolutions and rates (compatible_settings)
        for p in parent.get_all_matching_profiles() {
            if p.has_interpolations() || p.fisheye_params.camera_matrix.len() != 3 { continue; }
            if profile_prefix(&p) != prefix { continue; }
            let hs = if p.input_horizontal_stretch > 0.01 { p.input_horizontal_stretch } else { 1.0 };
            let vs = if p.input_vertical_stretch > 0.01 { p.input_vertical_stretch } else { 1.0 };
            let pa = aspect(p.calib_dimension.w, p.calib_dimension.h) * vs / hs;
            let (va, va_turned) = (aspect(w, h), aspect(h, w));
            if (pa / va - 1.0).abs() > 0.01 && (pa / va_turned - 1.0).abs() > 0.01 { continue; }
            let pid = normalized(&p.identifier);
            let n = (good + bad) as f64;
            let rank = Rank {
                not_voted_down: !(good + bad >= 3 && (good as f64) < 0.3 * n),
                exact: !pid.is_empty() && pid == wanted,
                same_eis: !eis.is_empty() && !pid.is_empty() && pid.ends_with(&eis),
                // The long sides: a turned profile is the same one
                size: nearness(p.calib_dimension.w.max(p.calib_dimension.h) as f64, w.max(h) as f64),
                fps: nearness(p.fps, fps),
                official,
                stars: 5.0 * (good as f64 + 1.0) / (n + 2.0),
            };
            if best.as_ref().is_none_or(|(r, _)| rank > *r) {
                let exact = rank.exact;
                best = Some((rank, Pick { profile: p, name: name.clone(), official, votes: (good, bad), exact, curated: None }));
            }
        }
    }
    best.map(|(_, p)| p)
}

/// Whether the video's own metadata has its lens (DJI with EIS off, newer GoPros), which comes before any profile
pub fn lens_in_file(stab: &StabilizationManager) -> bool {
    let gyro = stab.gyro.read();
    let md = gyro.file_metadata.read();
    md.lens_profile.as_ref().is_some_and(|l| l.is_object())
}

// ---- Lens types: a curated list for cameras the database has nothing for ----
//
// The Osmo Action 6 has no profile in Gyroflow's database, but another Action camera's fits it (Anton). Which one
// for which of its lens types is kept by hand in resources/curated_lenses.json. The type is the one the clip was given
// (source.lens_type in its .afp.json: the Library sets it for the clips selected, a whole dive at once), else the one
// the camera recorded (DJI's fov_type, through the list's "recorded"). A type whose profile is null takes what it
// would without the list: the lens in the file, else the database's pick.

#[derive(serde::Deserialize)]
struct Curated { cameras: HashMap<String, CuratedCamera> }
#[derive(serde::Deserialize)]
struct CuratedCamera { #[serde(default)] recorded: HashMap<String, String>, types: Vec<LensType> }

/// A lens type of a camera in the curated list
#[derive(serde::Deserialize, serde::Serialize, Clone, Debug)]
pub struct LensType {
    pub id: String,
    pub label: String,
    /// A key of Gyroflow's database; None: the lens in the file, else the database's pick
    pub profile: Option<String>,
    #[serde(default, skip_serializing)]
    pub note: String,
}

fn curated() -> &'static HashMap<String, CuratedCamera> {
    static CURATED: OnceLock<HashMap<String, CuratedCamera>> = OnceLock::new();
    CURATED.get_or_init(|| serde_json::from_str::<Curated>(include_str!("../resources/curated_lenses.json"))
        .map(|c| c.cameras.into_iter().map(|(k, v)| (camera_key(&k), v)).collect())
        .unwrap_or_else(|e| { log::error!("curated_lenses.json: {e}"); HashMap::new() }))
}

// A camera as the list and Gyroflow name it, compared without case and spaces ("DJI OsmoAction6", "DJI Osmo Action 6")
fn camera_key(camera: &str) -> String { camera.to_lowercase().replace(' ', "") }

/// The lens types the curated list has for a camera ("DJI OsmoAction6"), in its order: what the Library offers for
/// the clips selected. Empty for a camera it hasn't
pub fn lens_types(camera: &str) -> Vec<LensType> {
    curated().get(&camera_key(camera)).map(|c| c.types.clone()).unwrap_or_default()
}

/// The type the camera recorded the clip in, as the curated list names it ("wide"), None when it doesn't say or the
/// list doesn't know the mode
pub fn recorded_type(camera: &str, path: &str) -> Option<String> {
    let c = curated().get(&camera_key(camera))?;
    let fov = crate::probe::recorded_lens_mode(path)?;
    c.recorded.get(&fov.to_string()).cloned()
}

/// The type the clip was given by hand: source.lens_type in its .afp.json
pub fn chosen_type(path: &str) -> Option<String> {
    let text = std::fs::read_to_string(crate::workspace::sidecar_file(std::path::Path::new(path))).ok()?;
    let doc: serde_json::Value = serde_json::from_str(&text).ok()?;
    doc.pointer("/source/lens_type")?.as_str().filter(|s| !s.is_empty()).map(String::from)
}

/// The profile for a video as the player takes it: its type's curated profile (the one given by hand, else the one
/// recorded), else, unless the lens is in the file, the database's pick. None: the lens in the file, or nothing.
/// `id` is Gyroflow's camera identifier, `in_file` whether the file has its own lens
pub fn for_video(path: &str, id: &CameraIdentifier, in_file: bool) -> Option<Pick> {
    let camera = format!("{} {}", id.brand, id.model);
    let chosen = chosen_type(path);
    let typ = chosen.clone().or_else(|| recorded_type(&camera, path));
    if let Some(t) = typ.and_then(|t| lens_types(&camera).into_iter().find(|x| x.id == t)) {
        if let Some(key) = &t.profile {
            match db().get_by_id(key) {
                Some(p) => return Some(Pick { profile: p.clone(), name: p.get_display_name(), official: p.official, votes: (0, 0), exact: false, curated: Some(t.label) }),
                None => log::error!("curated_lenses.json: {key} isn't in Gyroflow's database"),
            }
        }
    }
    if in_file { return None; }
    pick(id)
}

/// Loads the profile the player takes for the video at `video` (`for_video`) into a manager that has it loaded
/// (`load_video_file`, which doesn't keep the path), in place of the one Gyroflow took by its identifier, if any. The
/// lens in the file stays unless the clip's lens type says otherwise. The frame readout time comes with it as
/// Gyroflow's autoload takes it. Returns what was loaded
pub fn apply(stab: &StabilizationManager, video: &str) -> Option<Pick> {
    let id = stab.camera_id.read().clone()?;
    let pick = for_video(video, &id, lens_in_file(stab))?;
    let json = pick.profile.get_json().ok()?;
    if let Err(e) = stab.load_lens_profile(&json) {
        log::warn!("Lens profile {}: {e:?}", pick.name);
        return None;
    }
    let (fr, frd) = { let lens = stab.lens.read(); (lens.frame_readout_time, lens.frame_readout_direction) };
    if let Some(fr) = fr {
        let mut params = stab.params.write();
        params.frame_readout_time = fr.abs();
        params.frame_readout_direction = frd.unwrap_or(if fr < 0.0 { gyroflow_core::stabilization_params::ReadoutDirection::BottomToTop } else { gyroflow_core::stabilization_params::ReadoutDirection::TopToBottom });
    }
    log::info!("Lens profile: {}", pick.label());
    Some(pick)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identifiers() {
        assert_eq!(normalized("GoPro-HERO12 Black--Linear-3840x2160@29970-HS EIS"), "gopro-hero11black-linear-3840x2160@29970-hseis");
        assert_eq!(normalized("--gopro-hero7black-superview-1920x1080@59940-hseis"), "gopro-hero7black-super-1920x1080@59940-hseis");
        assert_eq!(split_identifier("gopro-hero7black-wide-3840x2160@59940-hseis"), Some(("gopro-hero7black-wide-", "3840x2160@59940-hseis")));
        assert_eq!(split_identifier("gopro-hero7black-wide"), None);
        let mut p = LensProfile::default();
        (p.camera_brand, p.camera_model, p.lens_model) = ("GoPro".into(), "HERO7 Black".into(), "Superview".into());
        assert_eq!(profile_prefix(&p), "gopro-hero7black-super-");
        p.identifier = "-gopro-hero11blackmini-linear-3840x2160@50000-no-eis".into();
        assert_eq!(profile_prefix(&p), "gopro-hero11black-linear-");
        let v = parse_votes("# x\n0000abcd 3 1\nbad line\n");
        assert_eq!(v.get("0000abcd"), Some(&(3, 1)));
        assert_eq!(v.len(), 1);
    }

    // Every profile the curated list names is in the database built in, and every recorded mode names a type it has
    #[test]
    fn curated_list() {
        assert!(!curated().is_empty(), "curated_lenses.json reads");
        for (camera, c) in curated() {
            for t in &c.types {
                if let Some(key) = &t.profile { assert!(db().get_by_id(key).is_some(), "{camera} {}: {key} not in the database", t.id); }
            }
            for (mode, t) in &c.recorded { assert!(c.types.iter().any(|x| &x.id == t), "{camera}: mode {mode} -> {t}, no such type"); }
        }
        let types = lens_types("DJI Osmo Action 6");
        assert_eq!(types.iter().map(|t| t.id.as_str()).collect::<Vec<_>>(), ["wide", "normal"]);
        assert!(lens_types("GoPro HERO7 Black").is_empty());
        assert_eq!(lens_types("GoPro HERO4 Black").iter().map(|t| t.id.as_str()).collect::<Vec<_>>(), ["wide"]);
    }

    fn camera(model: &str, lens: &str, w: usize, h: usize, fps: usize, eis: &str) -> CameraIdentifier {
        let mut id = CameraIdentifier { brand: "GoPro".into(), model: model.into(), lens_info: lens.into(), video_width: w, video_height: h, fps, additional: eis.into(), ..Default::default() };
        // As from_telemetry_parser makes it
        id.identifier = normalized(&format!("{}-{}-{}-{}-{}x{}@{}-{}", id.brand, id.model, id.lens_model, id.lens_info, w, h, fps, eis));
        id
    }

    // Against the database built into gyroflow-core and the votes in resources/: what the player picks for clips that
    // Gyroflow itself gives nothing (checked by hand in Gyroflow's list, 2026-11-10)
    #[test]
    fn picks_from_the_database() {
        // HERO7 1080p Wide at 24 fps with HyperSmooth: no profile for 23.976, a 1080p Wide HS EIS one
        let p = pick(&camera("HERO7 Black", "Wide", 1920, 1080, 23976, "HS EIS")).expect("a HERO7 profile");
        eprintln!("HERO7 1080p24 Wide HS: {} | {}", p.label(), p.profile.identifier);
        assert!(!p.exact);
        assert!(p.profile.identifier.contains("hero7black-wide-1920x1080@"), "{}", p.profile.identifier);
        assert!(p.profile.identifier.ends_with("-hseis"), "{}", p.profile.identifier);
        // HERO12 Linear 4K 30 with HyperSmooth: a HERO11 Linear 16:9 one
        let p = pick(&camera("HERO12 Black", "Linear", 3840, 2160, 29970, "HS EIS")).expect("a HERO11 Linear profile");
        eprintln!("HERO12 4K30 Linear HS: {} | {}", p.label(), p.profile.identifier);
        assert!(normalized(&p.profile.camera_model).starts_with("hero11black"), "{}", p.name);
        assert!(normalized(&p.profile.lens_model) == "linear", "{}", p.name);
        // An exact one is found as Gyroflow finds it
        let p = pick(&camera("HERO7 Black", "Wide", 3840, 2160, 59940, "HS EIS")).unwrap();
        eprintln!("HERO7 4K60 Wide HS: {} | {}", p.label(), p.profile.identifier);
        assert!(p.exact, "{}", p.name);
        // No such camera
        assert!(pick(&camera("HERO99 Black", "Wide", 3840, 2160, 59940, "HS EIS")).is_none());
        // An aspect ratio no profile of the camera and mode has (2:1): nothing rather than another shape's
        assert!(pick(&camera("HERO7 Black", "Wide", 1920, 960, 29970, "HS EIS")).is_none());
        // Another lens mode isn't taken for the one recorded
        let p = pick(&camera("HERO7 Black", "Linear", 2704, 1520, 29970, "HS EIS"));
        assert!(p.as_ref().is_none_or(|p| normalized(&p.profile.lens_model) == "linear"), "{:?}", p.map(|p| p.name));
    }
}
