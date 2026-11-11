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
    /// A focal length alone (no distortion): nothing in the picture to correct, only what the stabilization needs
    pub focal_only: bool,
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
                best = Some((rank, Pick { profile: p, name: name.clone(), official, votes: (good, bad), exact, curated: None, focal_only: false }));
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
// mode, "Wide"), else the camera's "default" (a camera that records no mode: a drone). A type's profile is a key of Gyroflow's database, a lens profile file, or a focal length alone; per
// resolution or aspect ratio another ("sizes"). A type whose profile is null takes what it would without the list: the
// lens in the file, else the database's pick.
//
// The same list, in the same shape, can also be kept next to the app's data (`user_file`), for trying a profile without
// a new build: what it has for a camera is laid over the built-in one (its modes and types by id win), and it's read
// again as soon as it changes.

#[derive(serde::Deserialize, Default)]
struct Curated { #[serde(default)] cameras: HashMap<String, CuratedCamera> }
#[derive(serde::Deserialize, Default, Clone)]
struct CuratedCamera {
    #[serde(default)] recorded: HashMap<String, String>,
    #[serde(default)] types: Vec<LensType>,
    /// The type of a clip that neither was given one nor recorded one the list names
    #[serde(default)] default: Option<String>,
}

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
    /// The lens in the file, where a clip has it, comes before this type's profile, unless the clip was given the type
    /// by hand: set on a type a lens was saved to from a Gyroflow project (`save_default`), whose clips of the same mode
    /// may or may not carry their lens (an Action 6 in Wide does with EIS off, not with EIS on)
    #[serde(default)]
    pub file_first: bool,
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
        if c.default.is_some() { b.default = c.default; }
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

/// The type the camera recorded the clip in, as the curated list names it ("wide"), by the camera's own number for its
/// lens mode (`probe::recorded_lens_mode`). None when it doesn't say or the list doesn't know the mode
pub fn recorded_type(camera: &str, path: &str) -> Option<String> { recorded_type_named(camera, path, "") }

/// `recorded_type`, else by Gyroflow's name for the lens mode (`lens_mode`: "Wide", "Linear"; "" for none)
pub fn recorded_type_named(camera: &str, path: &str, lens_mode: &str) -> Option<String> {
    let l = lists();
    let c = l.cameras.get(&camera_key(camera))?;
    if let Some(t) = crate::probe::recorded_lens_mode(path).and_then(|m| c.recorded.get(&m.to_string())) { return Some(t.clone()); }
    let name = lens_mode.trim().to_lowercase();
    if name.is_empty() { return None; }
    c.recorded.iter().find(|(k, _)| k.trim().to_lowercase() == name).map(|(_, t)| t.clone())
}

/// The type the curated list gives a camera's clips that neither were given one nor recorded one it names
pub fn default_type(camera: &str) -> Option<String> {
    lists().cameras.get(&camera_key(camera))?.default.clone()
}

/// A camera as the player names it: Gyroflow's brand and model, or, when Gyroflow doesn't know the model (a Mini 4
/// Pro: "DJI") or the camera at all (a Mavic Pro's MOV, a HERO3), the one the file names (`geotag::dji_model`: DJI's
/// own header, the camera code in ©mdl, or for a clip saved again by DJI's muxer the drone's photos beside it;
/// `geotag::gopro_model`: an older GoPro's firmware)
pub fn camera_name(brand: &str, model: &str, path: &str) -> String {
    let gyroflow = format!("{brand} {model}").trim().to_string();
    if !model.trim().is_empty() { return gyroflow; }
    let p = std::path::Path::new(path);
    match crate::geotag::dji_model(p).or_else(|| crate::geotag::gopro_model(p)) {
        Some(c) if brand.trim().is_empty() || camera_key(&c).starts_with(&camera_key(brand)) => c,
        _ => gyroflow,
    }
}

/// A clip as its lens is decided: its camera (`camera_name`), Gyroflow's name for the lens mode it recorded, its
/// size, and Gyroflow's camera identifier where Gyroflow knows the camera (for the database's pick)
#[derive(Clone, Debug, Default)]
pub struct Clip {
    pub camera: String,
    pub lens_mode: String,
    pub width: usize,
    pub height: usize,
    pub id: Option<CameraIdentifier>,
}

impl Clip {
    pub fn from_id(path: &str, id: &CameraIdentifier) -> Clip {
        Clip { camera: camera_name(&id.brand, &id.model, path), lens_mode: id.lens_info.clone(), width: id.video_width, height: id.video_height, id: Some(id.clone()) }
    }
    /// From telemetry-parser's reading of the file and the video's size and rate
    pub fn from_input(path: &str, input: &gyroflow_core::telemetry_parser::Input, width: usize, height: usize, fps: f64) -> Clip {
        match CameraIdentifier::from_telemetry_parser(input, width, height, fps) {
            Ok(id) => Clip::from_id(path, &id),
            Err(_) => Clip { camera: camera_name(&input.camera_type(), input.camera_model().map(|s| s.as_str()).unwrap_or(""), path), width, height, ..Default::default() },
        }
    }
    /// A clip Gyroflow doesn't read: by what its file says alone
    pub fn from_file(path: &str, width: usize, height: usize) -> Clip {
        Clip { camera: camera_name("", "", path), width, height, ..Default::default() }
    }
    /// Its lens type: the one given by hand, else the one recorded, else its camera's default
    pub fn lens_type(&self, path: &str) -> Option<String> {
        chosen_type(path).or_else(|| recorded_type_named(&self.camera, path, &self.lens_mode)).or_else(|| default_type(&self.camera))
    }
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

/// The profile for a video as the player takes it: its type's curated profile (`Clip::lens_type`), else, unless the
/// lens is in the file, the database's pick. None: the lens in the file, or nothing. `id` is Gyroflow's camera
/// identifier, `in_file` whether the file has its own lens
pub fn for_video(path: &str, id: &CameraIdentifier, in_file: bool) -> Option<Pick> {
    for_clip(path, &Clip::from_id(path, id), in_file)
}

/// `for_video`, for a clip Gyroflow may not know the camera of
pub fn for_clip(path: &str, clip: &Clip, in_file: bool) -> Option<Pick> {
    let chosen = chosen_type(path);
    let ty = chosen.clone().or_else(|| recorded_type_named(&clip.camera, path, &clip.lens_mode)).or_else(|| default_type(&clip.camera));
    if let Some(t) = ty.and_then(|t| lens_types(&clip.camera).into_iter().find(|x| x.id == t)) {
        let file_first = in_file && t.file_first && chosen.as_deref() != Some(t.id.as_str());
        if let Some(p) = t.profile_for(clip.width, clip.height).filter(|_| !file_first) {
            let focal_only = matches!(p, Profile::Focal { .. });
            match load_profile(p, t.dir.as_deref(), clip.width, clip.height) {
                Ok((profile, name, official)) => return Some(Pick { profile, name, official, votes: (0, 0), exact: false, curated: Some(t.label.clone()), focal_only }),
                Err(e) => log::error!("Curated lenses, {} {}: {e}", clip.camera, t.id),
            }
        }
    }
    if in_file { return None; }
    pick(clip.id.as_ref()?)
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

// ---- A Gyroflow project's lens kept in the user's list, for every clip of its camera, mode and size ----
//
// A clip whose lens isn't known gets a Gyroflow project where a lens was picked by hand, and it works: the Library
// saves that lens to the user's list (`save_default`) as the clip's camera's, in the lens type the camera recorded
// the clip in (its mode), for its resolution: `types[...].sizes["3840x2160"]`. A mode the list doesn't name yet gets
// a type of its own ("mode-6": "Mode 6", or by Gyroflow's name for the mode, "wide": "Wide") and the camera's
// `recorded` names it; a camera that records no mode, its `default`. The profile is the key of Gyroflow's database
// the project's lens is (by its calibration, `db_key_of`), else the lens itself, written next to the list
// (lenses/<name>-<checksum>.json). Clips given another type by hand, and clips with their own project, keep theirs.

/// The Gyroflow project beside a clip, as the player finds it: <stem>.gyroflow, else <name>.gyroflow
pub fn project_beside(path: &str) -> Option<std::path::PathBuf> {
    let p = std::path::Path::new(path);
    let (dir, stem, name) = (p.parent()?, p.file_stem()?.to_string_lossy(), p.file_name()?.to_string_lossy());
    [format!("{stem}.gyroflow"), format!("{name}.gyroflow")].into_iter().map(|f| dir.join(f)).find(|f| f.is_file())
}

/// The lens a Gyroflow project uses (its calibration_data)
#[derive(Clone, Debug)]
pub struct ProjectLens {
    pub profile: LensProfile,
    /// As the project has it, to be written next to the list when it isn't in the database
    pub json: serde_json::Value,
    /// As Gyroflow's list names it, else the profile's own name
    pub name: String,
    /// The key of Gyroflow's database it is, None for a calibration of one's own
    pub key: Option<String>,
}

/// The lens of the Gyroflow project at `project`; an error when it has none
pub fn project_lens(project: &std::path::Path) -> Result<ProjectLens, String> {
    let text = std::fs::read_to_string(project).map_err(|e| format!("{}: {e}", project.display()))?;
    let doc: serde_json::Value = serde_json::from_str(&text).map_err(|e| format!("{}: {e}", project.display()))?;
    lens_of_project(&doc)
}

fn lens_of_project(doc: &serde_json::Value) -> Result<ProjectLens, String> {
    let json = doc.get("calibration_data").filter(|c| c.is_object()).cloned().ok_or("the project has no lens profile")?;
    let profile = LensProfile::from_value(json.clone()).map_err(|e| format!("the project's lens profile: {e}"))?;
    if profile.fisheye_params.camera_matrix.len() != 3 || profile.calib_dimension.w == 0 || profile.calib_dimension.h == 0 {
        return Err("the project has no lens profile".into());
    }
    let key = db_key_of(&profile);
    let name = key.as_deref().and_then(|k| db().get_by_id(k)).map(|p| p.get_display_name())
        .filter(|n| n != "---").unwrap_or_else(|| if profile.name.is_empty() { profile.get_name() } else { profile.name.clone() });
    Ok(ProjectLens { profile, json, name, key })
}

/// The key of Gyroflow's database a profile is: the one with the same calibration (LensProfile::get_checksum, what the
/// picture is projected with), by its identifier first, else among the profiles of its camera
fn db_key_of(p: &LensProfile) -> Option<String> {
    let sum = p.get_checksum();
    let same = |k: &str| db().get_by_id(k).is_some_and(|d| d.get_checksum() == sum);
    if !p.identifier.is_empty() && same(&p.identifier) { return Some(p.identifier.clone()); }
    for query in [format!("{} {}", p.camera_brand, p.camera_model), p.camera_brand.clone()] {
        let mut keys: Vec<String> = db().search(&query, &Default::default(), 0, 0).into_iter().map(|r| r.1).filter(|k| !k.ends_with(".gyroflow")).collect();
        keys.sort();
        keys.dedup();
        for k in keys {
            if same(&k) { return Some(k); }
            // One file's other resolutions (compatible_settings) are in the database by their own identifiers
            let Some(parent) = db().get_by_id(&k) else { continue };
            if let Some(sub) = parent.get_all_matching_profiles().into_iter().find(|s| !s.identifier.is_empty() && same(&s.identifier)) {
                return Some(sub.identifier);
            }
        }
    }
    None
}

/// Where in the list a clip's lens goes: its camera, the lens type the camera recorded it in, and its size
#[derive(Clone, Debug, PartialEq, serde::Serialize)]
pub struct ListKey {
    pub camera: String,
    #[serde(rename = "type")]
    pub type_id: String,
    pub type_label: String,
    /// "3840x2160"
    pub size: String,
    /// A type the list doesn't have yet
    pub new_type: bool,
    /// The mode `recorded` is to name (the camera's number for it, "6", or Gyroflow's name, "Wide"); None: it does already
    #[serde(skip)]
    recorded: Option<String>,
    /// The type is to be the camera's `default` (it records no mode)
    #[serde(skip)]
    default: bool,
}

// A lens type's id from a mode's name: what the Library keeps as source.lens_type (letters, digits, - and _)
fn type_id(name: &str) -> String {
    let s: String = name.trim().to_lowercase().chars().map(|c| if c.is_ascii_alphanumeric() { c } else { '-' }).collect();
    let s = s.split('-').filter(|x| !x.is_empty()).collect::<Vec<_>>().join("-");
    if s.is_empty() { "mode".into() } else { s }
}

/// Where the list keeps the lens of a clip at `path` (`clip`): the type the camera recorded it in (as the list names
/// it, or the camera's default), else a new one for the mode it recorded. The type given by hand isn't the key: it's
/// what the clip is, not what its camera recorded
pub fn list_key(path: &str, clip: &Clip) -> Result<ListKey, String> {
    let camera = clip.camera.trim().to_string();
    if camera.is_empty() { return Err("its camera isn't known".into()); }
    if clip.width == 0 || clip.height == 0 { return Err("its size isn't known".into()); }
    let size = format!("{}x{}", clip.width, clip.height);
    let types = lens_types(&camera);
    let label = |id: &str, or: String| types.iter().find(|t| t.id == id).map(|t| t.label.clone()).unwrap_or(or);
    let key = |type_id: String, type_label: String, recorded: Option<String>, default: bool| {
        let new_type = !types.iter().any(|t| t.id == type_id);
        ListKey { camera: camera.clone(), type_label: label(&type_id, type_label), type_id, size: size.clone(), new_type, recorded, default }
    };
    if let Some(t) = recorded_type_named(&camera, path, &clip.lens_mode) { return Ok(key(t.clone(), t, None, false)); }
    if let Some(m) = crate::probe::recorded_lens_mode(path) { return Ok(key(format!("mode-{m}"), format!("Mode {m}"), Some(m.to_string()), false)); }
    let mode = clip.lens_mode.trim();
    if !mode.is_empty() { return Ok(key(type_id(mode), mode.to_string(), Some(mode.to_string()), false)); }
    if let Some(t) = default_type(&camera) { return Ok(key(t.clone(), t, None, false)); }
    Ok(key("any".into(), "Any mode".into(), None, true))
}

/// What the list's entry for the lens is: its key in Gyroflow's database, else the file it's kept in next to the list
fn saved_profile_name(lens: &ProjectLens) -> String {
    if let Some(k) = &lens.key { return k.clone(); }
    let name: String = lens.name.chars().map(|c| if c.is_alphanumeric() || " -_.,()".contains(c) { c } else { '_' }).collect();
    format!("lenses/{}-{:08x}.json", name.trim().trim_matches('.'), lens.profile.get_checksum() as u32)
}

/// Whether the list already gives this lens for the key (for its size, its aspect ratio or the whole type)
pub fn is_default(key: &ListKey, lens: &ProjectLens) -> bool {
    let want = Profile::Named(saved_profile_name(lens));
    let Some(t) = lens_types(&key.camera).into_iter().find(|t| t.id == key.type_id) else { return false };
    let Some((w, h)) = key.size.split_once('x').and_then(|(w, h)| Some((w.parse::<usize>().ok()?, h.parse::<usize>().ok()?))) else { return false };
    t.profile_for(w, h) == Some(&want)
}

// "3840x2160" names this size, either way round
fn same_size(k: &str, w: usize, h: usize) -> bool {
    k.trim().split_once('x').and_then(|(a, b)| Some((a.trim().parse::<usize>().ok()?, b.trim().parse::<usize>().ok()?)))
        .is_some_and(|ab| ab == (w, h) || ab == (h, w))
}

// Written whole or not at all: next to it first, then put in its place
fn write_atomic(file: &std::path::Path, bytes: &[u8]) -> Result<(), String> {
    if let Some(d) = file.parent() { std::fs::create_dir_all(d).map_err(|e| format!("{}: {e}", d.display()))?; }
    let tmp = file.with_extension(format!("tmp{}", std::process::id()));
    std::fs::write(&tmp, bytes).map_err(|e| format!("{}: {e}", tmp.display()))?;
    std::fs::rename(&tmp, file).map_err(|e| { let _ = std::fs::remove_file(&tmp); format!("{}: {e}", file.display()) })
}

/// The user's list (its text, "" for none) with the lens kept for the key: the camera found as the list names it (or
/// added), the type taken from the user's list, else from the built-in one whole (the user's type replaces it), else
/// made; the size's entry set (one for the other way round taken out), the type marked `file_first`, and the mode named
fn with_default(text: &str, key: &ListKey, profile: &str) -> Result<serde_json::Value, String> {
    use serde_json::{ json, Value };
    let mut doc: Value = if text.trim().is_empty() { json!({}) } else { serde_json::from_str(text).map_err(|e| format!("the list can't be read, so it's left as it is: {e}"))? };
    if !doc.is_object() { return Err("the list isn't a JSON object, so it's left as it is".into()); }
    if !doc["cameras"].is_object() { doc["cameras"] = json!({}); }
    let cameras = doc["cameras"].as_object_mut().unwrap();
    let name = cameras.keys().find(|k| camera_key(k) == camera_key(&key.camera)).cloned().unwrap_or_else(|| key.camera.clone());
    let cam = cameras.entry(name).or_insert_with(|| json!({}));
    if !cam.is_object() { *cam = json!({}); }
    if !cam["types"].is_array() { cam["types"] = json!([]); }
    let types = cam["types"].as_array_mut().unwrap();
    let at = match types.iter().position(|t| t["id"].as_str() == Some(key.type_id.as_str())) {
        Some(i) => i,
        None => {
            let built_in: Value = serde_json::from_str(include_str!("../resources/curated_lenses.json")).unwrap_or_default();
            let from_built_in = built_in["cameras"].as_object().into_iter().flatten()
                .find(|(k, _)| camera_key(k) == camera_key(&key.camera))
                .and_then(|(_, c)| c["types"].as_array()?.iter().find(|t| t["id"].as_str() == Some(key.type_id.as_str())).cloned());
            types.push(from_built_in.unwrap_or_else(|| json!({ "id": key.type_id, "label": key.type_label, "profile": null })));
            types.len() - 1
        }
    };
    let t = &mut types[at];
    if !t["sizes"].is_object() { t["sizes"] = json!({}); }
    let (w, h) = key.size.split_once('x').and_then(|(w, h)| Some((w.parse::<usize>().ok()?, h.parse::<usize>().ok()?))).ok_or("no size")?;
    let sizes = t["sizes"].as_object_mut().unwrap();
    sizes.retain(|k, _| !same_size(k, w, h));
    sizes.insert(key.size.clone(), json!(profile));
    t["file_first"] = json!(true);
    if let Some(m) = &key.recorded {
        if !cam["recorded"].is_object() { cam["recorded"] = json!({}); }
        cam["recorded"][m.as_str()] = json!(key.type_id);
    }
    if key.default { cam["default"] = json!(key.type_id); }
    Ok(doc)
}

/// What `save_default` did
#[derive(Debug, serde::Serialize)]
pub struct Saved {
    #[serde(flatten)]
    pub key: ListKey,
    pub lens: String,
    /// The entry: a key of Gyroflow's database, or the lens's file next to the list
    pub profile: String,
    /// The user's list it's in
    pub file: String,
}

/// Keeps the lens of a Gyroflow project in the user's list as the one for the clip's camera, mode and size: every clip
/// of them that has no lens of its own (in the file, a type given by hand, a project) takes it from now on
pub fn save_default(path: &str, clip: &Clip, lens: &ProjectLens) -> Result<Saved, String> {
    let file = user_file().ok_or("there's no place for the lens list (no home folder)")?;
    let key = list_key(path, clip)?;
    let profile = saved_profile_name(lens);
    let dir = file.parent().ok_or("no folder for the lens list")?.to_path_buf();
    let text = match std::fs::read_to_string(&file) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(e) => return Err(format!("{}: {e}", file.display())),
    };
    let doc = with_default(&text, &key, &profile)?;
    let out = serde_json::to_string_pretty(&doc).map_err(|e| e.to_string())?;
    // As the player will read it
    parse_list(&out, Some(&dir)).map_err(|e| format!("the list wouldn't read: {e}"))?;
    if lens.key.is_none() {
        let json = serde_json::to_string_pretty(&lens.json).map_err(|e| e.to_string())?;
        write_atomic(&dir.join(&profile), json.as_bytes())?;
    }
    write_atomic(&file, out.as_bytes())?;
    log::info!("Lens list: {} {} {} = {profile} ({})", key.camera, key.type_id, key.size, file.display());
    Ok(Saved { key, lens: lens.name.clone(), profile, file: file.display().to_string() })
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
        // Drones record no mode: a default type, with a focal length alone
        for drone in ["DJI Mini 4 Pro", "DJI Mavic Pro", "DJI Mavic Air 2"] {
            let c = &base[&camera_key(drone)];
            assert_eq!(c.default.as_deref(), Some("main"), "{drone}");
            assert!(matches!(c.types[0].profile, Some(Profile::Focal { focal_35mm: Some(_), .. })), "{drone}");
        }
        for (camera, c) in &base {
            if let Some(d) = &c.default { assert!(c.types.iter().any(|x| &x.id == d), "{camera}: default {d}, no such type"); }
        }
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
        assert_eq!(a6.default, None);
        let mut top = HashMap::new();
        top.insert(camera_key("DJI Osmo Action 6"), CuratedCamera { default: Some("flat".into()), ..Default::default() });
        lay_over(&mut base, top);
        let a6 = &base[&camera_key("DJI OsmoAction6")];
        assert_eq!((a6.default.as_deref(), a6.types.len()), (Some("flat"), 3));
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

    const OSMO_STANDARD: &str = "DJI/DJI_Osmo Action_standard_,_4k_16by9_3840x2160-59.94fps.json";

    // A project's lens found in the database by its calibration: by the file's path for a profile without an
    // identifier (Anton's dive projects, Osmo Action "standard"), by the identifier for one with; one's own isn't there
    #[test]
    fn project_lens_key() {
        let osmo = db().get_by_id(OSMO_STANDARD).unwrap().get_json_value().unwrap();
        let l = lens_of_project(&serde_json::json!({ "version": 3, "calibration_data": osmo.clone() })).unwrap();
        assert_eq!(l.key.as_deref(), Some(OSMO_STANDARD));
        assert!(l.name.contains("Osmo Action"), "{}", l.name);
        assert_eq!(saved_profile_name(&l), OSMO_STANDARD);
        let hero7 = db().get_by_id("gopro-hero7black-wide-1920x1080@59940-hseis").expect("a HERO7 1080p60 Wide HS profile");
        let l = lens_of_project(&serde_json::json!({ "calibration_data": hero7.get_json_value().unwrap() })).unwrap();
        assert_eq!(l.key.as_deref(), Some("gopro-hero7black-wide-1920x1080@59940-hseis"));
        // Its own calibration: kept in a file next to the list
        let mut own = osmo.clone();
        own["fisheye_params"]["camera_matrix"][0][0] = serde_json::json!(1234.5);
        own["name"] = serde_json::json!("My Action 6 / wide");
        let l = lens_of_project(&serde_json::json!({ "calibration_data": own })).unwrap();
        assert_eq!(l.key, None);
        let f = saved_profile_name(&l);
        assert!(f.starts_with("lenses/My Action 6 _ wide-") && f.ends_with(".json"), "{f}");
        // No lens in the project (a preset), none at all
        assert!(lens_of_project(&serde_json::json!({ "calibration_data": {} })).is_err());
        assert!(lens_of_project(&serde_json::json!({ "stabilization": {} })).is_err());
    }

    fn key(camera: &str, type_id: &str, label: &str, size: &str, recorded: Option<&str>, default: bool) -> ListKey {
        ListKey { camera: camera.into(), type_id: type_id.into(), type_label: label.into(), size: size.into(), new_type: true, recorded: recorded.map(String::from), default }
    }

    // The list's entry for a key: a new camera and mode, a type of the built-in list copied whole, the same size the other
    // way round replaced, a camera without modes, a list that doesn't read left alone
    #[test]
    fn list_entry() {
        let doc = with_default("", &key("GoPro HERO7 Black", "wide", "Wide", "1920x1080", Some("Wide"), false), "gopro-x").unwrap();
        assert_eq!(doc, serde_json::json!({ "cameras": { "GoPro HERO7 Black": { "recorded": { "Wide": "wide" },
            "types": [ { "id": "wide", "label": "Wide", "profile": null, "sizes": { "1920x1080": "gopro-x" }, "file_first": true } ] } } }));
        // The user's own name for the camera; the built-in Normal (its profile) taken over with the size added
        let mine = r#"{ "cameras": { "DJI Osmo Action 6": { "recorded": { "6": "wide" }, "types": [ { "id": "flat", "label": "Flat", "profile": { "hfov": 90 } } ] } } }"#;
        let doc = with_default(mine, &key("DJI OsmoAction6", "normal", "Normal", "3840x2160", None, false), "mine.json").unwrap();
        let cam = &doc["cameras"]["DJI Osmo Action 6"];
        assert_eq!(doc["cameras"].as_object().unwrap().len(), 1);
        assert_eq!(cam["recorded"], serde_json::json!({ "6": "wide" }));
        assert_eq!(cam["types"][0]["id"], "flat");
        assert_eq!(cam["types"][1]["profile"], OSMO_STANDARD);
        assert_eq!(cam["types"][1]["sizes"], serde_json::json!({ "3840x2160": "mine.json" }));
        // The same size again, turned: one entry, the new one
        let doc = with_default(&doc.to_string(), &key("DJI OsmoAction6", "normal", "Normal", "2160x3840", None, false), "other.json").unwrap();
        assert_eq!(doc["cameras"]["DJI Osmo Action 6"]["types"][1]["sizes"], serde_json::json!({ "2160x3840": "other.json" }));
        // A mode the list didn't name, on the camera's number for it
        let doc = with_default(&doc.to_string(), &key("DJI OsmoAction6", "mode-7", "Mode 7", "3840x2160", Some("7"), false), "x").unwrap();
        assert_eq!(doc["cameras"]["DJI Osmo Action 6"]["recorded"], serde_json::json!({ "6": "wide", "7": "mode-7" }));
        let mut layered = parse_list(include_str!("../resources/curated_lenses.json"), None).unwrap();
        lay_over(&mut layered, parse_list(&doc.to_string(), None).unwrap());
        let a6 = &layered[&camera_key("DJI OsmoAction6")];
        assert_eq!(a6.types.iter().map(|t| t.id.as_str()).collect::<Vec<_>>(), ["wide", "normal", "flat", "mode-7"]);
        let normal = &a6.types[1];
        assert!(normal.file_first);
        assert_eq!(normal.profile_for(3840, 2160), Some(&Profile::Named("other.json".into())));
        assert_eq!(normal.profile_for(1920, 1080), Some(&Profile::Named(OSMO_STANDARD.into())));
        // No mode: the camera's default
        let doc = with_default("", &key("DJI drone", "any", "Any mode", "1920x1080", None, true), "x").unwrap();
        assert_eq!(doc["cameras"]["DJI drone"]["default"], "any");
        assert!(with_default("{ not json", &key("A", "b", "B", "1x1", None, false), "x").is_err());
        assert!(with_default("[]", &key("A", "b", "B", "1x1", None, false), "x").is_err());
        assert_eq!(type_id(" Super View+ "), "super-view");
        assert_eq!(type_id("??"), "mode");
    }

    // Saved through the user's list file ($AFP_CURATED_LENSES): the clips of the camera, mode and size take it, one
    // with its lens in the file keeps that, another size gets nothing; a lens of one's own is written next to the list
    #[test]
    fn saved_default_is_taken() {
        let dir = std::env::temp_dir().join(format!("afp-lens-save-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let list = dir.join("support/curated_lenses.json");
        // SAFETY: no other test reads this variable or the user's list
        unsafe { std::env::set_var("AFP_CURATED_LENSES", &list); }
        let video = dir.join("GX010001.MP4").display().to_string();
        let clip = Clip { camera: "GoPro HERO7 Black".into(), lens_mode: "Wide".into(), width: 1920, height: 1080, id: None };
        let hero7 = db().get_by_id("gopro-hero7black-wide-1920x1080@59940-hseis").unwrap().get_json_value().unwrap();
        let lens = lens_of_project(&serde_json::json!({ "calibration_data": hero7 })).unwrap();
        let k = list_key(&video, &clip).unwrap();
        assert_eq!((k.type_id.as_str(), k.type_label.as_str(), k.size.as_str(), k.new_type, k.recorded.as_deref()), ("wide", "Wide", "1920x1080", true, Some("Wide")));
        assert!(!is_default(&k, &lens));
        assert!(for_clip(&video, &clip, false).is_none());
        let saved = save_default(&video, &clip, &lens).unwrap();
        assert_eq!((saved.profile.as_str(), saved.file.as_str()), ("gopro-hero7black-wide-1920x1080@59940-hseis", list.display().to_string().as_str()));
        assert!(is_default(&list_key(&video, &clip).unwrap(), &lens));
        let p = for_clip(&video, &clip, false).expect("the saved lens");
        assert_eq!(p.curated.as_deref(), Some("Wide"));
        assert_eq!(p.profile.identifier, "gopro-hero7black-wide-1920x1080@59940-hseis");
        // Its lens in the file comes first; another size isn't this one
        assert!(for_clip(&video, &clip, true).is_none());
        assert!(for_clip(&video, &Clip { width: 2704, height: 1520, ..clip.clone() }, false).is_none());
        // A lens of one's own, for 2.7K: its file beside the list, the 1080p one kept
        let mut own = db().get_by_id("gopro-hero7black-wide-1920x1080@59940-hseis").unwrap().get_json_value().unwrap();
        own["fisheye_params"]["camera_matrix"][0][0] = serde_json::json!(1500.25);
        own["name"] = serde_json::json!("Mine");
        let lens2 = lens_of_project(&serde_json::json!({ "calibration_data": own })).unwrap();
        let c27 = Clip { width: 2704, height: 1520, ..clip.clone() };
        let saved = save_default(&video, &c27, &lens2).unwrap();
        assert!(saved.profile.starts_with("lenses/Mine-"), "{}", saved.profile);
        assert!(list.parent().unwrap().join(&saved.profile).is_file());
        let p = for_clip(&video, &c27, false).expect("the lens of one's own");
        assert!((p.profile.fisheye_params.camera_matrix[0][0] - 1500.25).abs() < 1e-9);
        assert!(for_clip(&video, &clip, false).is_some());
        let text = std::fs::read_to_string(&list).unwrap();
        assert!(!list.with_extension(format!("tmp{}", std::process::id())).exists());
        // The type is the camera's now: a clip of it is in it, by its mode
        assert_eq!(list_key(&video, &clip).unwrap().new_type, false);
        // The built-in list's Normal for the Action 6 is the dive projects' lens already: nothing to save
        let osmo = lens_of_project(&serde_json::json!({ "calibration_data": db().get_by_id(OSMO_STANDARD).unwrap().get_json_value().unwrap() })).unwrap();
        let a6 = ListKey { camera: "DJI OsmoAction6".into(), type_id: "normal".into(), type_label: "Normal".into(), size: "3840x2160".into(), new_type: false, recorded: None, default: false };
        assert!(is_default(&a6, &osmo));
        assert!(!is_default(&ListKey { type_id: "wide".into(), ..a6 }, &osmo));
        unsafe { std::env::remove_var("AFP_CURATED_LENSES"); }
        let _ = std::fs::remove_dir_all(&dir);
        assert!(text.contains("\"file_first\": true"), "{text}");
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
