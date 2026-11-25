// SPDX-License-Identifier: GPL-3.0-or-later
//
// Optical stabilization of clips the player can't stabilize with their own motion data: their motion measured from the
// picture (Gyroflow's own analysis, run here: measure.rs) and kept in each clip's <stem>.afp-motion (afp_motion.rs),
// which the player plays with as it plays gyro, without a Gyroflow project (docs/design/optical-without-gyroflow.md). Action
// Footage Library's "Stabilize optically" runs it on the selected clips, one after another (bin/gyroflow-optical.rs).
//
// Which clips (`verdict`):
// - one with a Gyroflow project beside it is left out: the player plays the project (made there, perhaps by hand),
//   and motion measured here would go unused;
// - one with its motion measured already is left out, unless it's asked for again (`redo`: a lens chosen anew, a
//   result to improve);
// - one with motion data the player stabilizes it with (the probe's "default": gyro, recorded with the camera's own
//   stabilization off) is left out: the gyro is better than what the picture says;
// - a 360° clip is left out (the 360 mode plays it);
// - one whose lens isn't known is left out: without a lens profile the picture's motion can't be turned into the
//   camera's. Known: the lens the file has (DJI, newer GoPros), else the profile Gyroflow's database has for the camera,
//   lens mode in Gyroflow's database (lens_pick.rs: as the player picks it);
// - the rest are measured: no motion data (DJI with EIS on, older cameras), or motion data the camera already
//   stabilized the picture with (HyperSmooth on a HERO11 or older, DJI's RockSteady): that motion data is set aside,
//   since the picture no longer moves as the camera did. (A HERO12 or later records how HyperSmooth turned the picture,
//   so the player stabilizes it from its own motion data: in_camera.rs.)

use std::path::{ Path, PathBuf };
use gyroflow_core::{ filesystem, util };
use gyroflow_core::telemetry_parser::{ InputOptions, tags_impl::* };

/// What's done with a clip
#[derive(Debug, Clone, PartialEq)]
pub enum Verdict {
    /// Measured, with this lens: its label ("in the file", or the profile's name), and the profile itself (None: the
    /// lens in the file); the motion file it replaces, measured before
    Stabilize { lens: String, profile: Option<String>, replaces: Option<PathBuf> },
    /// Left out, and why (said to the user)
    LeaveOut(String),
}

/// A Gyroflow project beside the video already: `<stem>.gyroflow` (Gyroflow's own name for it) or `<name>.gyroflow`
pub fn own_project(video: &Path) -> Option<PathBuf> {
    let dir = video.parent()?;
    let name = video.file_name()?.to_string_lossy().into_owned();
    let stem = video.file_stem()?.to_string_lossy().into_owned();
    [dir.join(format!("{stem}.gyroflow")), dir.join(format!("{name}.gyroflow"))].into_iter().find(|p| p.is_file())
}

/// What the clip is, as far as this decides: read from its metadata (probe_only: the first samples)
#[derive(Debug, Clone, Default)]
pub struct Facts {
    /// The camera ("GoPro HERO12 Black")
    pub camera: String,
    /// The player stabilizes it with its own motion data (probe::probe's "default")
    pub own_motion: bool,
    /// Why not, when it doesn't (probe::probe's reason)
    pub reason: String,
    /// A 360° clip: stitched (spherical metadata) or a 360 camera's own file
    pub spherical: bool,
    /// The lens: (its label, the profile to give Gyroflow, None when Gyroflow loads it by itself); None: not known
    pub lens: Option<(String, Option<String>)>,
    /// Its camera has lens types to choose from (lens_pick::lens_types): a lens not known can be given
    pub has_types: bool,
}

/// The decision, from what the clip is, the project it has (`project`), and its motion measured before (`measured`,
/// measured again when `redo`)
pub fn decide(project: Option<&Path>, measured: Option<&Path>, redo: bool, facts: &Facts) -> Verdict {
    if project.is_some() { return Verdict::LeaveOut("it has a Gyroflow project, which the player plays".into()); }
    // Before "measured already": a clip measured once and now stabilized from its own motion data (a HERO12+ with
    // HyperSmooth on) isn't measured again either way
    if facts.own_motion { return Verdict::LeaveOut("it has motion data the player stabilizes it with".into()); }
    if measured.is_some() && !redo { return Verdict::LeaveOut("its motion is measured already (Redo measures it again)".into()); }
    if facts.spherical { return Verdict::LeaveOut("a 360° clip: the player's 360 view plays it".into()); }
    match &facts.lens {
        Some((lens, profile)) => Verdict::Stabilize { lens: lens.clone(), profile: profile.clone(), replaces: measured.map(Path::to_path_buf) },
        None if facts.has_types => Verdict::LeaveOut(format!("{}: its lens isn't known (a mode not named yet): choose its type in Lens", facts.camera)),
        None => Verdict::LeaveOut(format!("{}: its lens isn't known (no lens profile for this camera and mode)",
            if facts.camera.is_empty() { "unknown camera" } else { &facts.camera })),
    }
}

/// The lens the clip is measured with, as the player takes it (lens_pick::for_video): the profile for the clip's
/// lens type (chosen in the Library, `source.lens_type` in its own file, else the one its camera recorded) where one is
/// curated for it, else the lens in the file, else the best of the camera and lens mode in Gyroflow's database
fn lens_of(path: &str, clip: &crate::lens_pick::Clip, in_file: bool) -> Option<(String, Option<String>)> {
    if let Some(pick) = crate::lens_pick::for_clip(path, clip, in_file) {
        return Some((pick.label(), pick.profile.get_json().ok()));
    }
    in_file.then(|| ("in the file".into(), None))
}

/// A clip as its lens is decided (lens_pick::Clip), and whether its lens is in the file. A file telemetry-parser
/// doesn't read (a Mavic Pro's MOV: "Unsupported file format") is named by what it says itself
fn read_clip(video: &Path, path: &str) -> Result<(crate::lens_pick::Clip, bool), String> {
    let url = filesystem::path_to_url(path);
    let mut file = std::fs::File::open(video).map_err(|e| e.to_string())?;
    let size = file.metadata().map_err(|e| e.to_string())?.len();
    let meta = util::get_video_metadata(&mut file, size, &url).map_err(|e| format!("not a video Gyroflow reads: {e}"))?;
    match crate::probe::probe_input(&mut file, size, path, InputOptions::default()) {
        Ok(input) => {
            let in_file = input.samples.as_deref().unwrap_or_default().iter().filter_map(|s| s.tag_map.as_ref()?.get(&GroupId::Lens))
                .any(|m| m.contains_key(&TagId::Data) || m.contains_key(&TagId::Name));
            Ok((crate::lens_pick::Clip::from_input(path, &input, meta.width, meta.height, meta.fps), in_file))
        }
        Err(_) => Ok((crate::lens_pick::Clip::from_file(path, meta.width, meta.height, meta.fps), false)),
    }
}

/// A clip's lens types, as the Library's picker shows them (lens_pick.rs)
#[derive(Debug, Clone, Default, PartialEq)]
pub struct LensTypes {
    /// The camera as the player names it ("DJI OsmoAction6", "DJI Mini4 Pro": lens_pick::camera_name)
    pub camera: String,
    /// What its camera has to choose from: [id, label]; none for a camera with no curated types (GoPro: its lens is
    /// in the file)
    pub types: Vec<(String, String)>,
    /// The type chosen for the clip by hand (source.lens_type in its own file)
    pub chosen: Option<String>,
    /// The type its camera recorded, where the curated list names that mode
    pub recorded: Option<String>,
    /// The type its camera's clips are taken as when none was chosen or recorded (a drone's one camera)
    pub default: Option<String>,
    /// The camera's own number for the lens mode it recorded (DJI's fov_type, a HERO4's setting), for a camera with
    /// types: "Unknown (mode 6)" where the list doesn't name it yet
    pub mode: Option<u64>,
}

pub fn lens_types(video: &Path) -> Result<LensTypes, String> {
    let path = video.to_string_lossy().into_owned();
    let (clip, _) = read_clip(video, &path)?;
    let camera = clip.camera.clone();
    let types: Vec<(String, String)> = crate::lens_pick::lens_types(&camera).into_iter().map(|t| (t.id, t.label)).collect();
    if types.is_empty() { return Ok(LensTypes { camera, ..Default::default() }); }
    Ok(LensTypes {
        chosen: crate::lens_pick::chosen_type(&path),
        recorded: crate::lens_pick::recorded_type_named(&camera, &path, &clip.lens_mode),
        default: crate::lens_pick::default_type(&camera),
        mode: crate::probe::recorded_lens_mode(&path),
        camera, types,
    })
}

/// What the clip is (`Facts`); Err: it can't be read
pub fn facts(video: &Path) -> Result<Facts, String> {
    let path = video.to_string_lossy().into_owned();
    let (clip, in_file) = read_clip(video, &path)?;
    let lens = lens_of(&path, &clip, in_file);
    let (own_motion, reason) = match crate::probe::probe(&path) { Ok(_) => (true, String::new()), Err(e) => (false, e) };
    let spherical = crate::probe::spherical(&path).is_some() || crate::rig::read(&path).is_some();
    let has_types = !crate::lens_pick::lens_types(&clip.camera).is_empty();
    Ok(Facts { camera: clip.camera, own_motion, reason, spherical, lens, has_types })
}

/// The decision for a clip on the disk
pub fn verdict(video: &Path, redo: bool) -> Verdict {
    if !video.is_file() { return Verdict::LeaveOut("not found".into()); }
    let project = own_project(video);
    let measured = Some(crate::afp_motion::path_for(video)).filter(|p| p.is_file());
    if project.is_some() { return decide(project.as_deref(), measured.as_deref(), redo, &Facts::default()); }
    match facts(video) {
        Ok(f) => decide(project.as_deref(), measured.as_deref(), redo, &f),
        // Measured once, whatever it is now
        Err(_) if measured.is_some() && !redo => decide(None, measured.as_deref(), redo, &Facts::default()),
        Err(e) => Verdict::LeaveOut(format!("couldn't be read: {e}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn facts(own_motion: bool, lens: Option<&str>) -> Facts {
        Facts { camera: "GoPro HERO12 Black".into(), own_motion, lens: lens.map(|l| (l.to_string(), None)), ..Default::default() }
    }

    #[test]
    fn decides() {
        let proj = Path::new("/x/A.gyroflow");
        let measured = Path::new("/x/A.afp-motion");
        let lens = facts(false, Some("in the file"));
        assert!(matches!(decide(Some(proj), None, false, &lens), Verdict::LeaveOut(r) if r.contains("Gyroflow project")));
        assert!(matches!(decide(Some(proj), Some(measured), true, &lens), Verdict::LeaveOut(r) if r.contains("Gyroflow project")));
        assert!(matches!(decide(None, Some(measured), false, &lens), Verdict::LeaveOut(r) if r.contains("measured already")));
        assert_eq!(decide(None, Some(measured), true, &lens), Verdict::Stabilize { lens: "in the file".into(), profile: None, replaces: Some(measured.to_path_buf()) });
        assert!(matches!(decide(None, None, false, &facts(true, Some("in the file"))), Verdict::LeaveOut(r) if r.contains("motion data")));
        // Its own motion data plays (a HERO12+ with HyperSmooth on): not measured again, Redo or not
        assert!(matches!(decide(None, Some(measured), true, &facts(true, Some("in the file"))), Verdict::LeaveOut(r) if r.contains("motion data")));
        assert!(matches!(decide(None, None, false, &facts(false, None)), Verdict::LeaveOut(r) if r.contains("GoPro HERO12 Black: its lens isn't known")));
        let typed = Facts { has_types: true, ..facts(false, None) };
        assert!(matches!(decide(None, None, false, &typed), Verdict::LeaveOut(r) if r.ends_with("choose its type in Lens")));
        assert_eq!(decide(None, None, false, &lens), Verdict::Stabilize { lens: "in the file".into(), profile: None, replaces: None });
        let mut f = facts(false, Some("in the file"));
        f.spherical = true;
        assert!(matches!(decide(None, None, false, &f), Verdict::LeaveOut(r) if r.contains("360")));
    }

    #[test]
    fn leaves_out_what_it_should() {
        let dir = std::env::temp_dir().join(format!("afp-optical-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let video = dir.join("GX010973.MP4");
        std::fs::write(&video, b"video").unwrap();
        assert!(own_project(&video).is_none());
        std::fs::write(dir.join("GX010973.gyroflow"), b"{}").unwrap();
        assert_eq!(own_project(&video), Some(dir.join("GX010973.gyroflow")));
        assert!(matches!(verdict(&video, true), Verdict::LeaveOut(r) if r.contains("Gyroflow project")));
        std::fs::remove_file(dir.join("GX010973.gyroflow")).unwrap();
        std::fs::write(crate::afp_motion::path_for(&video), b"x").unwrap();
        assert!(matches!(verdict(&video, false), Verdict::LeaveOut(r) if r.contains("measured already")));
        assert!(matches!(verdict(&dir.join("nope.MP4"), false), Verdict::LeaveOut(r) if r == "not found"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn under_water() {
        // As the clip's own file says
        let dir = std::env::temp_dir().join(format!("afp-optical-water-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let clip = dir.join("A.MP4");
        std::fs::write(&clip, b"not a video").unwrap();
        assert_eq!(crate::water::refraction_for(&clip), None, "nothing says");
        let side = crate::workspace::sidecar_file(&clip);
        std::fs::write(&side, r#"{"source":{"underwater":"yes"}}"#).unwrap();
        assert_eq!(crate::water::refraction_for(&clip), Some(1.33));
        std::fs::write(&side, r#"{"source":{"underwater":"no"}}"#).unwrap();
        assert_eq!(crate::water::refraction_for(&clip), Some(1.0));
        std::fs::write(&side, r#"{"source":{"underwater":"auto"}}"#).unwrap();
        assert_eq!(crate::water::refraction_for(&clip), None, "auto, and the camera doesn't say");
        let _ = std::fs::remove_dir_all(&dir);
    }

    // A project gyroflow-optical made, played as the player plays it (load_manager): the motion Gyroflow measured is
    // there, and it's smoothed. AFP_OPTICAL_PROJECT=<clip>.gyroflow cargo test --release --lib optical -- --ignored
    #[test]
    #[ignore]
    fn plays_with_the_project() {
        let Some(project) = std::env::var_os("AFP_OPTICAL_PROJECT") else { return };
        let project = project.to_string_lossy().into_owned();
        let (stab, _, _) = crate::load_manager(&project, None, None, crate::water::Underwater::Project, 0.0, false, None).unwrap();
        stab.recompute_blocking();
        let g = stab.gyro.read();
        assert!(g.optical_correction.is_some(), "the measured motion isn't in the player's engine");
        eprintln!("ignores the file's motion: {}, optical correction on: {:?}", g.ignores_file_motion(), g.optical_correction.as_ref().map(|c| c.enabled));
        assert!(g.ignores_file_motion(), "the camera's own motion data is used: it's of the picture before the camera stabilized it");
        let q: Vec<_> = g.quaternions.values().cloned().collect();
        let sm: Vec<_> = g.smoothed_quaternions.values().cloned().collect();
        assert!(q.len() > 10 && q.len() == sm.len());
        let travel: f64 = q.windows(2).map(|w| w[0].angle_to(&w[1])).sum::<f64>().to_degrees();
        let removed = (q.iter().zip(&sm).map(|(a, b)| a.angle_to(b).powi(2)).sum::<f64>() / q.len() as f64).sqrt().to_degrees();
        eprintln!("{project}: {} orientations, the camera turned {travel:.1}° along the way, {removed:.2}° rms taken out by smoothing", q.len());
        assert!(travel > 0.1 && removed > 0.01);
    }

    // #130: a project's optical correction applies on this machine as on the one that measured it. Projects separated
    // by ':' (';' on Windows): AFP_OPTICAL_APPLIES=<a.gyroflow>:<b.gyroflow> cargo test --release --lib optical_applies -- --ignored --nocapture
    #[test]
    #[ignore]
    fn optical_applies() {
        let Some(projects) = std::env::var_os("AFP_OPTICAL_APPLIES") else { return };
        let mut stale = Vec::new();
        for project in std::env::split_paths(&projects) {
            let project = project.to_string_lossy().into_owned();
            let (stab, _, _) = crate::load_manager(&project, None, None, crate::water::Underwater::Project, 0.0, false, None).unwrap();
            stab.refresh_optical_correction();
            let g = stab.gyro.read();
            let Some(c) = g.optical_correction.as_ref() else { eprintln!("{project}: no optical correction"); continue };
            // Computed here as there, or only up to the last bits (gyroflow-core's Fingerprint::matches)
            let (quats, context) = (g.optical_uncorrected.checksum(), g.optical_context.checksum());
            eprintln!("{project}: enabled {}, applied {} | quats {:016x} measured, {quats:016x} here | context {:016x} measured, {context:016x} here{}",
                      c.enabled, g.optical_correction_applied, c.quats_checksum, c.context_checksum,
                      if quats == c.quats_checksum && context == c.context_checksum { "" } else { " (not exactly)" });
            if c.enabled && !g.optical_correction_applied { stale.push(project); }
        }
        assert!(stale.is_empty(), "not applied: {stale:?}");
    }
}
