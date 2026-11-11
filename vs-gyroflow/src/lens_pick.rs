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
// for which of its lens types is kept by hand in resources/curated_lenses.json, built in. The type is the one the clip
// was given (source.lens_type in its .afp.json: the Library sets it for the clips selected, a whole dive at once), else
// the one the camera recorded (the list's "recorded": DJI's fov_type, the HERO4's SETT, or Gyroflow's name for the lens
// mode, "Wide"). A type's profile is a key of Gyroflow's database, a lens profile file, or a focal length alone; per
// resolution or aspect ratio another ("sizes"). A type whose profile is null takes what it would without the list: the
// lens in the file, else the database's pick.
//
// The same list, in the same shape, can also be kept next to the app's data (`user_file`), for trying a profile without
// a new build: what it has for a camera is laid over the built-in one (its modes and types by id win), and it's read
// again as soon as it changes.

#[derive(serde::Deserialize, Default)]
struct Curated { #[serde(default)] cameras: HashMap<String, CuratedCamera> }
#[derive(serde::Deserialize, Default, Clone)]
struct CuratedCamera { #[serde(default)] recorded: HashMap<String, String>, #[serde(default)] types: Vec<LensType> }

/// A lens type's profile, as the curated list gives it
#[derive(serde::Deserialize, Clone, Debug, PartialEq)]
#[serde(untagged)]
pub enum Profile {
    /// A key of Gyroflow's database (its file path under camera_presets: "GoPro/GoPro_HERO4 Black___....json"), else a
    /// lens profile file (.json, as Gyroflow's calibrator writes them): its path, or its name next to the list's file
    Named(String),
    /// No lens distortion: the picture taken as a pinhole camera's with this focal length, for a picture the camera has
    /// already made straight (in-camera stabilization, distortion correction). `focal_35mm`: its 35 mm equivalent, on the
    /// diagonal as cameras give it; or `hfov`: how wide the picture is, in degrees across
    Focal {
        #[serde(default)] focal_35mm: Option<f64>,
        #[serde(default)] hfov: Option<f64>,
    },
}

impl Profile {
    /// What it is, in a few words: the database's key, the file, "focal length only, 16 mm (35 mm equivalent)"
    pub fn describe(&self) -> String {
        match self {
            Profile::Named(s) => s.clone(),
            Profile::Focal { focal_35mm: Some(f), .. } => format!("focal length only, {f} mm (35 mm equivalent)"),
            Profile::Focal { hfov: Some(a), .. } => format!("focal length only, {a}° across"),
            Profile::Focal { .. } => "focal length only, but neither focal_35mm nor hfov given".into(),
        }
    }
}

/// A lens type of a camera in the curated list
#[derive(serde::Deserialize, Clone, Debug)]
pub struct LensType {
    pub id: String,
    pub label: String,
    /// None: the lens in the file, else the database's pick
    #[serde(default)]
    pub profile: Option<Profile>,
    /// Another profile for a resolution ("3840x2880") or an aspect ratio ("4:3"), either way round; null: as None above
    #[serde(default)]
    pub sizes: HashMap<String, Option<Profile>>,
    #[serde(default)]
    pub note: String,
    /// The folder of the list it's from, for a profile file named in it
    #[serde(skip)]
    dir: Option<std::path::PathBuf>,
}

// For the probe's answer (--lens): the Library and the side panel read id and label
impl serde::Serialize for LensType {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeStruct;
        let mut st = s.serialize_struct("LensType", 3)?;
        st.serialize_field("id", &self.id)?;
        st.serialize_field("label", &self.label)?;
        st.serialize_field("profile", &self.profile.as_ref().map(|p| p.describe()))?;
        st.end()
    }
}

impl LensType {
    /// Its profile for a video of this size: the one for exactly this resolution, else for its aspect ratio, else its own
    fn profile_for(&self, w: usize, h: usize) -> Option<&Profile> {
        let parse = |k: &str, sep: char| -> Option<(f64, f64)> {
            let (a, b) = k.trim().split_once(sep)?;
            Some((a.trim().parse().ok()?, b.trim().parse().ok()?))
        };
        let (w, h) = (w as f64, h as f64);
        let exact = self.sizes.iter().find(|(k, _)| parse(k, 'x').is_some_and(|(a, b)| (a, b) == (w, h) || (a, b) == (h, w)));
        let same_aspect = || self.sizes.iter().find(|(k, _)| parse(k, ':').is_some_and(|(a, b)| {
            b > 0.0 && h > 0.0 && ((a / b) / (w / h) - 1.0).abs() < 0.01 || a > 0.0 && w > 0.0 && ((b / a) / (w / h) - 1.0).abs() < 0.01
        }));
        match exact.or_else(same_aspect) {
            Some((_, p)) => p.as_ref(),
            None => self.profile.as_ref(),
        }
    }
}

/// The curated list as the player reads it: the built-in one with the user's over it
pub struct Lists {
    cameras: HashMap<String, CuratedCamera>,
    /// The user's file, when there is one, and what was wrong with it (it's left out then)
    pub user_file: Option<std::path::PathBuf>,
    pub user_error: Option<String>,
}

/// Where the user's own curated list is kept: $AFP_CURATED_LENSES, else curated_lenses.json in the app's data folder
/// (macOS: ~/Library/Application Support/ActionFootagePlayer; Windows: %APPDATA%\ActionFootagePlayer; elsewhere
/// $XDG_CONFIG_HOME/ActionFootagePlayer, ~/.config/ActionFootagePlayer). It needn't exist
pub fn user_file() -> Option<std::path::PathBuf> {
    use std::path::PathBuf;
    let env = |k: &str| std::env::var_os(k).filter(|v| !v.is_empty()).map(PathBuf::from);
    if let Some(f) = env("AFP_CURATED_LENSES") { return Some(f); }
    let base = if cfg!(target_os = "macos") { env("HOME")?.join("Library/Application Support") }
               else if cfg!(windows) { env("APPDATA")? }
               else { env("XDG_CONFIG_HOME").or_else(|| env("HOME").map(|h| h.join(".config")))? };
    Some(base.join("ActionFootagePlayer").join("curated_lenses.json"))
}

/// What tells the user's list's versions apart (its path, size and time), "" when there's none: for an answer worked
/// out with it to be kept only while it stays the same
pub fn user_file_stamp() -> String {
    user_file().and_then(|f| std::fs::metadata(&f).ok().map(|m| format!("{}|{}|{:?}", f.display(), m.len(), m.modified().ok())))
        .unwrap_or_default()
}

fn parse_list(text: &str, dir: Option<&std::path::Path>) -> Result<HashMap<String, CuratedCamera>, String> {
    let c: Curated = serde_json::from_str(text).map_err(|e| e.to_string())?;
    Ok(c.cameras.into_iter().map(|(k, mut v)| {
        for t in &mut v.types { t.dir = dir.map(|d| d.to_path_buf()); }
        (camera_key(&k), v)
    }).collect())
}

// The user's list over the built-in one: a camera's modes and types (by id) from it win, a new type goes last
fn lay_over(base: &mut HashMap<String, CuratedCamera>, top: HashMap<String, CuratedCamera>) {
    for (k, c) in top {
        let b = base.entry(k).or_default();
        b.recorded.extend(c.recorded);
        for t in c.types {
            match b.types.iter_mut().find(|x| x.id == t.id) {
                Some(x) => *x = t,
                None => b.types.push(t),
            }
        }
    }
}

fn read_lists() -> Lists {
    let mut cameras = parse_list(include_str!("../resources/curated_lenses.json"), None)
        .unwrap_or_else(|e| { log::error!("curated_lenses.json (built in): {e}"); HashMap::new() });
    let user_file = user_file().filter(|f| f.is_file());
    let mut user_error = None;
    if let Some(f) = &user_file {
        match std::fs::read_to_string(f).map_err(|e| e.to_string()).and_then(|t| parse_list(&t, f.parent())) {
            Ok(top) => { log::info!("Curated lenses: {} over the built-in list", f.display()); lay_over(&mut cameras, top); }
            Err(e) => { log::error!("{}: {e}", f.display()); user_error = Some(e); }
        }
    }
    Lists { cameras, user_file, user_error }
}

/// The curated list, read again when the user's file has changed
pub fn lists() -> std::sync::Arc<Lists> {
    static LISTS: std::sync::Mutex<Option<(String, std::sync::Arc<Lists>)>> = std::sync::Mutex::new(None);
    let stamp = user_file_stamp();
    let mut kept = LISTS.lock().unwrap_or_else(|e| e.into_inner());
    if let Some((s, l)) = kept.as_ref() { if *s == stamp { return l.clone(); } }
    let l = std::sync::Arc::new(read_lists());
    *kept = Some((stamp, l.clone()));
    l
}

// A camera as the list and Gyroflow name it, compared without case and spaces ("DJI OsmoAction6", "DJI Osmo Action 6")
fn camera_key(camera: &str) -> String { camera.to_lowercase().replace(' ', "") }

/// The lens types the curated list has for a camera ("DJI OsmoAction6"), in its order: what the Library offers for
/// the clips selected. Empty for a camera it hasn't
pub fn lens_types(camera: &str) -> Vec<LensType> {
    lists().cameras.get(&camera_key(camera)).map(|c| c.types.clone()).unwrap_or_default()
}

/// The type the camera recorded the clip in, as the curated list names it ("wide"): by the camera's own number for its
/// lens mode (`probe::recorded_lens_mode`), else by Gyroflow's name for it (`lens_mode`: "Wide", "Linear"; "" for none).
/// None when it doesn't say or the list doesn't know the mode
pub fn recorded_type(camera: &str, path: &str, lens_mode: &str) -> Option<String> {
    let l = lists();
    let c = l.cameras.get(&camera_key(camera))?;
    if let Some(t) = crate::probe::recorded_lens_mode(path).and_then(|m| c.recorded.get(&m.to_string())) { return Some(t.clone()); }
    let name = lens_mode.trim().to_lowercase();
    if name.is_empty() { return None; }
    c.recorded.iter().find(|(k, _)| k.trim().to_lowercase() == name).map(|(_, t)| t.clone())
}

/// The type the clip was given by hand: source.lens_type in its .afp.json
pub fn chosen_type(path: &str) -> Option<String> {
    let text = std::fs::read_to_string(crate::workspace::sidecar_file(std::path::Path::new(path))).ok()?;
    let doc: serde_json::Value = serde_json::from_str(&text).ok()?;
    doc.pointer("/source/lens_type")?.as_str().filter(|s| !s.is_empty()).map(String::from)
}

// 35 mm film's diagonal, which a 35 mm equivalent focal length is given against
const FILM_DIAGONAL_MM: f64 = 43.2666;

/// A lens profile of no distortion, with the focal length given, for a video of this size
fn focal_profile(focal_35mm: Option<f64>, hfov: Option<f64>, w: usize, h: usize) -> Result<LensProfile, String> {
    if w == 0 || h == 0 { return Err("the video's size isn't known".into()); }
    let (wf, hf) = (w as f64, h as f64);
    let f = match (focal_35mm, hfov) {
        (Some(mm), _) if mm > 0.0 => mm * (wf * wf + hf * hf).sqrt() / FILM_DIAGONAL_MM,
        (_, Some(a)) if a > 0.0 && a < 180.0 => wf / 2.0 / (a.to_radians() / 2.0).tan(),
        _ => return Err("give focal_35mm (mm, more than 0) or hfov (degrees, 0 to 180)".into()),
    };
    let json = serde_json::json!({
        "name": "Focal length only", "calibrated_by": "Action Footage Player", "calibrator_version": "afp-focal-only",
        "note": "no lens distortion, a pinhole camera's picture",
        "calib_dimension": { "w": w, "h": h }, "orig_dimension": { "w": w, "h": h },
        "distortion_model": "opencv_standard",
        "fisheye_params": {
            "camera_matrix": [[f, 0.0, wf / 2.0], [0.0, f, hf / 2.0], [0.0, 0.0, 1.0]],
            "distortion_coeffs": [0.0, 0.0, 0.0, 0.0, 0.0],
        },
    });
    LensProfile::from_value(json).map_err(|e| e.to_string())
}

/// The lens profile a curated type's profile is, for a video of this size, with the name it's shown by
fn load_profile(p: &Profile, dir: Option<&std::path::Path>, w: usize, h: usize) -> Result<(LensProfile, String, bool), String> {
    match p {
        Profile::Focal { focal_35mm, hfov } => Ok((focal_profile(*focal_35mm, *hfov, w, h)?, p.describe(), false)),
        Profile::Named(key) => {
            if let Some(lp) = db().get_by_id(key) { return Ok((lp.clone(), lp.get_display_name(), lp.official)); }
            let path = std::path::Path::new(key);
            let path = if path.is_absolute() { path.to_path_buf() } else if let Some(d) = dir { d.join(path) } else {
                return Err(format!("{key} isn't in Gyroflow's database"));
            };
            let text = std::fs::read_to_string(&path).map_err(|e| format!("{key}: not in Gyroflow's database, and {}: {e}", path.display()))?;
            let lp = LensProfile::from_json(&text).map_err(|e| format!("{}: {e}", path.display()))?;
            if lp.fisheye_params.camera_matrix.len() != 3 || lp.calib_dimension.w == 0 { return Err(format!("{}: not a lens profile with a calibration", path.display())); }
            let name = path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
            Ok((lp, name, false))
        }
    }
}

/// The profile for a video as the player takes it: its type's curated profile (the one given by hand, else the one
/// recorded), else, unless the lens is in the file, the database's pick. None: the lens in the file, or nothing.
/// `id` is Gyroflow's camera identifier, `in_file` whether the file has its own lens
pub fn for_video(path: &str, id: &CameraIdentifier, in_file: bool) -> Option<Pick> {
    let camera = format!("{} {}", id.brand, id.model);
    let typ = chosen_type(path).or_else(|| recorded_type(&camera, path, &id.lens_info));
    if let Some(t) = typ.and_then(|t| lens_types(&camera).into_iter().find(|x| x.id == t)) {
        if let Some(p) = t.profile_for(id.video_width, id.video_height) {
            match load_profile(p, t.dir.as_deref(), id.video_width, id.video_height) {
                Ok((profile, name, official)) => return Some(Pick { profile, name, official, votes: (0, 0), exact: false, curated: Some(t.label) }),
                Err(e) => log::error!("Curated lenses, {camera} {}: {e}", t.id),
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

    // Every profile the built-in curated list names is in the database built in, and every recorded mode names a type it has
    #[test]
    fn curated_list() {
        let base = parse_list(include_str!("../resources/curated_lenses.json"), None).expect("curated_lenses.json reads");
        for (camera, c) in &base {
            for t in &c.types {
                for p in t.profile.iter().chain(t.sizes.values().flatten()) {
                    if let Profile::Named(key) = p { assert!(db().get_by_id(key).is_some(), "{camera} {}: {key} not in the database", t.id); }
                }
            }
            for (mode, t) in &c.recorded { assert!(c.types.iter().any(|x| &x.id == t), "{camera}: mode {mode} -> {t}, no such type"); }
        }
        let ids = |c: &CuratedCamera| c.types.iter().map(|t| t.id.clone()).collect::<Vec<_>>();
        assert_eq!(ids(&base[&camera_key("DJI Osmo Action 6")]), ["wide", "normal"]);
        assert!(!base.contains_key(&camera_key("GoPro HERO7 Black")));
        assert_eq!(ids(&base[&camera_key("GoPro HERO4 Black")]), ["wide"]);
    }

    // The user's list over the built-in one, the profile by size, a focal length alone, a profile file next to the list
    #[test]
    fn user_list() {
        let mut base = parse_list(include_str!("../resources/curated_lenses.json"), None).unwrap();
        let dir = std::env::temp_dir().join(format!("afp-lens-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let normal = db().get_by_id("DJI/DJI_Osmo Action_standard_,_4k_16by9_3840x2160-59.94fps.json").unwrap().get_json().unwrap();
        std::fs::write(dir.join("mine.json"), normal).unwrap();
        let top = parse_list(r#"{ "cameras": {
            "DJI Osmo Action 6": { "recorded": { "6": "wide", "2": "flat" },
              "types": [ { "id": "wide", "label": "Wide (mine)", "profile": { "focal_35mm": 16 },
                           "sizes": { "3840x2880": "mine.json", "4:3": { "hfov": 90 }, "1:1": null } },
                         { "id": "flat", "label": "Flat", "profile": "mine.json" } ] },
            "GoPro HERO7 Black": { "recorded": { "Linear": "lin" }, "types": [ { "id": "lin", "label": "Linear", "profile": { "hfov": 86 } } ] } } }"#,
            Some(&dir)).unwrap();
        lay_over(&mut base, top);
        let a6 = &base[&camera_key("DJI OsmoAction6")];
        assert_eq!(a6.types.iter().map(|t| t.id.as_str()).collect::<Vec<_>>(), ["wide", "normal", "flat"]);
        assert_eq!((a6.recorded["1"].as_str(), a6.recorded["2"].as_str(), a6.recorded["6"].as_str()), ("wide", "flat", "wide"));
        let wide = &a6.types[0];
        assert_eq!(wide.label, "Wide (mine)");
        assert_eq!(wide.profile_for(3840, 2160), Some(&Profile::Focal { focal_35mm: Some(16.0), hfov: None }));
        assert_eq!(wide.profile_for(2880, 3840), Some(&Profile::Named("mine.json".into())));
        assert_eq!(wide.profile_for(1440, 1080), Some(&Profile::Focal { focal_35mm: None, hfov: Some(90.0) }));
        assert_eq!(wide.profile_for(1080, 1080), None);
        // 16 mm on the diagonal of 4K 16:9: 4406 px / 43.27 mm * 16 mm
        let (lp, name, _) = load_profile(wide.profile_for(3840, 2160).unwrap(), wide.dir.as_deref(), 3840, 2160).unwrap();
        let f = lp.fisheye_params.camera_matrix[0][0];
        assert!((f - 16.0 * (3840f64.powi(2) + 2160f64.powi(2)).sqrt() / 43.2666).abs() < 0.01, "{f}");
        assert!(name.starts_with("focal length only"), "{name}");
        // As Gyroflow takes a profile handed to it as text (StabilizationManager::load_lens_profile)
        assert!(LensProfile::default().load_from_data(&lp.get_json().unwrap()).is_ok());
        let (lp, name, _) = load_profile(wide.profile_for(2880, 3840).unwrap(), wide.dir.as_deref(), 2880, 3840).unwrap();
        assert_eq!((lp.calib_dimension.w, name.as_str()), (3840, "mine.json"));
        let (lp, _, _) = load_profile(&Profile::Focal { focal_35mm: None, hfov: Some(90.0) }, None, 1440, 1080).unwrap();
        assert!((lp.fisheye_params.camera_matrix[0][0] - 720.0).abs() < 0.01);
        assert!(load_profile(&Profile::Named("nope.json".into()), Some(&dir), 100, 100).is_err());
        assert!(load_profile(&Profile::Focal { focal_35mm: None, hfov: None }, None, 100, 100).is_err());
        assert!(parse_list("{ \"cameras\": { \"x\": { \"types\": 5 } } }", None).is_err());
        let _ = std::fs::remove_dir_all(&dir);
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
