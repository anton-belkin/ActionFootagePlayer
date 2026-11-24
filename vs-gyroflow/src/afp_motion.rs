// SPDX-License-Identifier: GPL-3.0-or-later
//
// A clip's motion measured from its picture, kept beside it in a file of its own: `<stem>.afp-motion`
// (docs/optical-without-gyroflow.md). A clip without usable motion data of its own (an old GoPro, a DJI with EIS on, a
// GoPro whose HyperSmooth already stabilized the picture) plays stabilized with it like a clip with gyro: in the
// player's levels, without a Gyroflow project.
//
// What's in it is Gyroflow's own: the optical correction its "Analyze image optically" measures
// (gyroflow_core::gyro_source::OpticalCorrection: for a file without motion data the rotation between its frames,
// chained, and a spline of the rolling shutter's part), as a project keeps it. With it, everything it was measured
// with: the lens, the rolling shutter's readout, the light's refraction (in water or out), the sync. The correction
// applies only in that context (Gyroflow checks it: `context_checksum`), so it's what the clip plays with here too, and
// the clip it was measured on (its size and the time its MP4 says it was made), so a file of another clip with the same
// name isn't taken for it.
//
// Two lines of text: the header (JSON, `Header`), readable with `head -1`; then the correction, base91 of zlib'd CBOR as
// Gyroflow's projects have it. Read whole: ~60 bytes a frame, 2-4 MB for the longest clips, read cold from the NAS in
// 11-38 ms (measured 2026-11-24): there's nothing to gain from reading it in parts.

use std::path::{ Path, PathBuf };
use std::collections::BTreeMap;
use gyroflow_core::{ StabilizationManager, gyro_source::OpticalCorrection, util };
use serde::{ Deserialize, Serialize };
use serde_json::Value;

pub const EXT: &str = "afp-motion";
pub const FORMAT: &str = "actionfootageplayer-motion";
pub const VERSION: u32 = 1;

/// The clip's file of measured motion: `<stem>.afp-motion` beside it, as its `<stem>.afp.json`
pub fn path_for(video: &Path) -> PathBuf {
    video.with_extension(EXT)
}

/// The clip a file was measured on
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ClipId {
    /// Its file name, for a person reading the header
    pub name: String,
    /// Its size, bytes
    pub size: u64,
    /// When its MP4 says it was made (moov/mvhd, Unix s), what a copy keeps whatever the file system does with dates
    pub created: Option<i64>,
    pub width: usize,
    pub height: usize,
    pub fps: f64,
    pub duration_ms: f64,
    /// Its frames, as the analysis had them
    pub frames: usize,
}

impl ClipId {
    /// The clip on the disk: its size and its MP4's own time, as links tell clips apart (docs/library-management.md §8)
    pub fn of(video: &Path) -> Result<ClipId, String> {
        let size = std::fs::metadata(video).map_err(|e| format!("{}: {e}", video.display()))?.len();
        let path = video.to_string_lossy();
        let movie = crate::markers::movie(&path);
        Ok(ClipId {
            name: video.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default(),
            size,
            created: movie.as_ref().and_then(|m| m.created),
            width: movie.as_ref().map(|m| m.width as usize).unwrap_or_default(),
            height: movie.as_ref().map(|m| m.height as usize).unwrap_or_default(),
            duration_ms: movie.as_ref().map(|m| m.duration * 1000.0).unwrap_or_default(),
            ..Default::default()
        })
    }

    /// The same clip: its size, and its MP4's time when both have one
    pub fn same(&self, other: &ClipId) -> bool {
        self.size == other.size && (self.created.is_none() || other.created.is_none() || self.created == other.created)
    }
}

/// How it was made
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Measured {
    /// "converted": taken from a Gyroflow project's analysis; "measured": by the player's own
    pub how: String,
    /// The project it was taken from, for "converted"
    pub from: String,
    /// When, Unix s
    pub at: i64,
    /// The tool that wrote it and its version
    pub tool: String,
    /// How the analysis followed the points (klt, dis), and the correction's settings it was fitted with
    pub tracking: String,
    pub strength: f64,
    pub per_frame: bool,
    /// Frames the analysis tracked, how many gave a measurement, and the correction's rms where measured, degrees
    pub frames: usize,
    pub measured_frames: usize,
    pub rms_deg: f64,
}

/// What the motion was measured with, and of what
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Header {
    pub format: String,
    pub version: u32,
    pub clip: ClipId,
    /// The lens profile, as Gyroflow's projects keep it (`calibration_data`), and its name for people
    pub lens: Value,
    pub lens_name: String,
    /// The rolling shutter's readout, ms (Gyroflow's frame_readout_time: negative bottom to top), and its direction
    /// as Gyroflow names it ("TopToBottom", ...)
    pub readout_ms: f64,
    pub readout_direction: String,
    /// The light's refraction the lens was taken with: 1 in air, 1.33 in water behind a flat port
    pub refraction: f64,
    /// Frames the video is shifted by against its motion (Gyroflow's frame_offset), usually 0
    pub frame_offset: i64,
    /// The sync points, µs: ms (Gyroflow's offsets), for motion data of the file's own that was corrected
    pub offsets: BTreeMap<i64, f64>,
    /// The file's own motion data set aside: it was of the picture before the camera stabilized it, or there's none
    pub ignore_file_motion: bool,
    pub measured: Measured,
}

impl Header {
    pub fn in_water(&self) -> bool { self.refraction > 1.0 }
}

/// A file of measured motion: its header and the correction
#[derive(Clone)]
pub struct MotionFile {
    pub header: Header,
    pub correction: OpticalCorrection,
    /// The correction as stored (base91), written back as it was read
    stored: String,
}

impl std::fmt::Debug for MotionFile {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MotionFile").field("header", &self.header).field("frames", &self.correction.video_base.len()).finish()
    }
}

impl MotionFile {
    pub fn new(header: Header, correction: OpticalCorrection) -> Result<MotionFile, String> {
        let stored = util::compress_to_base91_cbor(&correction).ok_or("the correction couldn't be encoded")?;
        Ok(MotionFile { header, correction, stored })
    }

    /// The file's text: the header's line, then the correction's
    pub fn to_text(&self) -> String {
        let mut header = self.header.clone();
        header.format = FORMAT.into();
        header.version = VERSION;
        format!("{}\n{}\n", serde_json::to_string(&header).unwrap_or_default(), self.stored)
    }

    pub fn parse(text: &str) -> Result<MotionFile, String> {
        let (first, rest) = text.split_once('\n').ok_or("not a motion file: one line")?;
        let header = read_header_line(first)?;
        let stored = rest.trim().to_string();
        let correction = util::decompress_from_base91_cbor::<OpticalCorrection>(&stored).map_err(|e| format!("its correction can't be read: {e:?}"))?;
        if correction.video_base.is_empty() && correction.coeffs.is_empty() { return Err("its correction is empty".into()); }
        Ok(MotionFile { header, correction, stored })
    }

    /// Written whole, through a file beside it renamed over it: a reader never sees half of one
    pub fn write(&self, path: &Path) -> Result<(), String> {
        let tmp = path.with_extension(format!("{EXT}.tmp"));
        std::fs::write(&tmp, self.to_text()).and_then(|_| std::fs::rename(&tmp, path)).map_err(|e| { let _ = std::fs::remove_file(&tmp); format!("{}: {e}", path.display()) })
    }
}

fn read_header_line(line: &str) -> Result<Header, String> {
    let header: Header = serde_json::from_str(line).map_err(|e| format!("not a motion file: {e}"))?;
    if header.format != FORMAT { return Err(format!("not a motion file: format {:?}", header.format)); }
    if header.version > VERSION { return Err(format!("a motion file of a newer version ({}): update the player", header.version)); }
    Ok(header)
}

/// The header of the clip's motion file, when it has one of its own: None without one, Err for one that's broken or of
/// another clip. Reads the first line only
pub fn header_for(video: &Path) -> Option<Result<Header, String>> {
    use std::io::BufRead;
    let path = path_for(video);
    let file = std::fs::File::open(&path).ok()?;
    let mut line = String::new();
    if let Err(e) = std::io::BufReader::new(file).read_line(&mut line) { return Some(Err(format!("{}: {e}", path.display()))); }
    Some(read_header_line(line.trim_end()).and_then(|h| of_this_clip(h, video)))
}

/// The clip's motion file read whole: None without one, Err for one that's broken or of another clip
pub fn read_for(video: &Path) -> Option<Result<MotionFile, String>> {
    let path = path_for(video);
    let text = match std::fs::read_to_string(&path) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return None,
        Err(e) => return Some(Err(format!("{}: {e}", path.display()))),
    };
    Some(MotionFile::parse(&text).and_then(|m| of_this_clip(m.header.clone(), video).map(|_| m)))
}

fn of_this_clip(header: Header, video: &Path) -> Result<Header, String> {
    let clip = ClipId::of(video)?;
    if !header.clip.same(&clip) {
        return Err(format!("{}: measured on another clip ({} bytes, made {:?}; this one {} bytes, made {:?})",
            path_for(video).display(), header.clip.size, header.clip.created, clip.size, clip.created));
    }
    Ok(header)
}

/// The file's stamp (size and time), for the caches of what a clip plays with: "" without one
pub fn stamp(video: &Path) -> String {
    std::fs::metadata(path_for(video)).ok().map(|m| format!("{}:{:?}", m.len(), m.modified().ok())).unwrap_or_default()
}

/// What `apply` did
#[derive(Debug, Clone)]
pub struct Applied {
    /// The correction composes onto the motion in full; false: the base plays, but its rolling shutter part was measured
    /// in another context (Gyroflow logs which)
    pub in_context: bool,
}

/// The measured motion into a manager that has the clip loaded (`load_video_file`), with what it was measured with:
/// its lens, readout, refraction, sync and frame offset, so the correction applies as measured. After the lens the
/// player picks and any preset: these are the motion's
pub fn apply(stab: &StabilizationManager, m: &MotionFile) -> Result<Applied, String> {
    let h = &m.header;
    if h.lens.is_object() {
        // As a project's calibration_data is loaded (import_gyroflow_data), for the same rays the analysis had
        let mut lens = stab.lens.write();
        lens.load_from_json_value(&h.lens);
        let db = stab.lens_profile_db.read();
        lens.resolve_interpolations(&db);
    }
    {
        // The frame timing the analysis had (the same clip: only the last bits may differ, see `from_project`)
        let mut p = stab.params.write();
        let c = &h.clip;
        if c.fps > 0.0 && (p.fps - c.fps).abs() < 1e-3 { p.fps = c.fps; }
        if c.frames > 0 && p.frame_count.abs_diff(c.frames) <= 1 { p.frame_count = c.frames; }
        if c.duration_ms > 0.0 && (p.duration_ms - c.duration_ms).abs() < 1.0 { p.duration_ms = c.duration_ms; }
        let _ = stab.gyro.write().set_video_timing(&p);
    }
    {
        use gyroflow_core::stabilization_params::ReadoutDirection;
        let mut p = stab.params.write();
        p.frame_readout_time = h.readout_ms;
        p.frame_readout_direction = if h.readout_direction.is_empty() {
            if h.readout_ms < 0.0 { ReadoutDirection::BottomToTop } else { ReadoutDirection::TopToBottom }
        } else { h.readout_direction.as_str().into() };
        p.frame_offset = h.frame_offset as i32;
    }
    stab.clear_keyframes_type(&gyroflow_core::keyframes::KeyframeType::LightRefractionCoeff);
    stab.set_light_refraction_coefficient(if h.refraction > 0.0 { h.refraction } else { 1.0 });
    stab.set_ignore_file_motion(h.ignore_file_motion);
    if !h.offsets.is_empty() {
        stab.gyro.write().set_offsets(h.offsets.clone());
    }
    stab.recompute_undistortion();
    let mut correction = m.correction.clone();
    correction.enabled = true;
    stab.set_optical_correction(Some(correction));
    let gyro = stab.gyro.read();
    if gyro.quaternions.len() < 2 {
        return Err("the measured motion gave no orientations".into());
    }
    Ok(Applied { in_context: gyro.optical_correction_applied })
}

/// A Gyroflow project's measured motion, as a motion file of the clip `video`: Err when it has none, or keyframes
/// what it was measured with (the refraction changing in the clip), which a motion file doesn't keep
pub fn from_project(project: &Value, project_path: &Path, video: &Path) -> Result<MotionFile, String> {
    let gyro = project.get("gyro_source").ok_or("no gyro_source in the project")?;
    let stored = gyro.get("optical_correction").and_then(Value::as_str).filter(|s| !s.is_empty()).ok_or("the project has no motion measured from the picture")?;
    let correction = util::decompress_from_base91_cbor::<OpticalCorrection>(stored).map_err(|e| format!("its correction can't be read: {e:?}"))?;
    if correction.video_base.is_empty() && correction.coeffs.is_empty() { return Err("the project's correction is empty".into()); }
    if project.pointer("/keyframes/LightRefractionCoeff").is_some() { return Err("the project keyframes the refraction: not kept in a motion file".into()); }
    let st = project.get("stabilization").cloned().unwrap_or(Value::Null);
    let lens = project.get("calibration_data").cloned().unwrap_or(Value::Null);
    let offsets: BTreeMap<i64, f64> = project.get("offsets").and_then(Value::as_object)
        .map(|o| o.iter().filter_map(|(k, v)| Some((k.parse().ok()?, v.as_f64()?))).collect()).unwrap_or_default();
    let mut clip = ClipId::of(video)?;
    if let Some(info) = project.get("video_info") {
        // The frame timing exactly as the analysis had it: the correction's context has the frames' times in it, and
        // Gyroflow's frame rate (from the decoder) can differ in its last bits from the one read from the MP4 here
        clip.fps = info.get("fps").and_then(Value::as_f64).unwrap_or_default();
        clip.frames = info.get("num_frames").and_then(Value::as_u64).unwrap_or_default() as usize;
        if let Some(d) = info.get("duration_ms").and_then(Value::as_f64).filter(|d| *d > 0.0) { clip.duration_ms = d; }
        if clip.width == 0 { clip.width = info.get("width").and_then(Value::as_u64).unwrap_or_default() as usize; }
        if clip.height == 0 { clip.height = info.get("height").and_then(Value::as_u64).unwrap_or_default() as usize; }
    }
    let header = Header {
        format: FORMAT.into(),
        version: VERSION,
        clip,
        lens_name: lens.get("name").and_then(Value::as_str).unwrap_or_default().to_string(),
        lens,
        readout_ms: st.get("frame_readout_time").and_then(Value::as_f64).unwrap_or_default(),
        readout_direction: st.get("frame_readout_direction").and_then(Value::as_str).unwrap_or_default().to_string(),
        refraction: project.get("light_refraction_coefficient").and_then(Value::as_f64).filter(|v| *v > 0.0).unwrap_or(1.0),
        frame_offset: st.get("frame_offset").and_then(Value::as_i64).unwrap_or_default(),
        offsets,
        ignore_file_motion: gyro.get("ignore_file_motion").and_then(Value::as_bool).unwrap_or(false),
        measured: Measured {
            how: "converted".into(),
            from: project_path.to_string_lossy().into_owned(),
            at: std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or_default(),
            tool: format!("vs-gyroflow {}", env!("CARGO_PKG_VERSION")),
            tracking: correction.tracking.name().into(),
            strength: correction.settings.strength,
            per_frame: correction.settings.per_frame,
            frames: correction.frames,
            measured_frames: correction.measured_frames,
            rms_deg: correction.rms_deg,
        },
    };
    Ok(MotionFile { header, correction, stored: stored.to_string() })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn correction() -> OpticalCorrection {
        OpticalCorrection {
            enabled: true, start_us: 0.0, spacing_us: 5555.0,
            coeffs: vec![[0.001, -0.002, 0.0005]; 12],
            video_base: (0..10).map(|i| (i * 33_366, [1.0, 0.0, 0.001 * i as f32, 0.0])).collect(),
            frames: 10, measured_frames: 9, rms_deg: 0.05,
            ..Default::default()
        }
    }

    #[test]
    fn round_trip() {
        let header = Header { clip: ClipId { name: "GOPR0001.MP4".into(), size: 1234, created: Some(1_462_303_666), ..Default::default() },
                              lens: serde_json::json!({ "name": "test" }), refraction: 1.33, readout_ms: 16.9, ..Default::default() };
        let m = MotionFile::new(header.clone(), correction()).unwrap();
        let text = m.to_text();
        assert_eq!(text.lines().count(), 2);
        assert!(text.lines().next().unwrap().contains("\"format\":\"actionfootageplayer-motion\""));
        let back = MotionFile::parse(&text).unwrap();
        assert_eq!(back.header.clip, header.clip);
        assert_eq!(back.header.refraction, 1.33);
        assert!(back.header.in_water());
        assert_eq!(back.correction.video_base, m.correction.video_base);
        assert_eq!(back.correction.coeffs.len(), 12);
        // Written back as it was read
        assert_eq!(back.to_text(), text);
    }

    #[test]
    fn refuses_what_isnt_one() {
        assert!(MotionFile::parse("{\"format\":\"something else\"}\nabc\n").is_err());
        assert!(MotionFile::parse("not json\nabc\n").is_err());
        let newer = format!("{{\"format\":\"{FORMAT}\",\"version\":{}}}\nabc\n", VERSION + 1);
        assert!(MotionFile::parse(&newer).unwrap_err().contains("newer"));
    }

    #[test]
    fn same_clip() {
        let a = ClipId { size: 10, created: Some(5), ..Default::default() };
        assert!(a.same(&ClipId { size: 10, created: Some(5), name: "other name".into(), ..Default::default() }));
        assert!(a.same(&ClipId { size: 10, created: None, ..Default::default() }));
        assert!(!a.same(&ClipId { size: 11, created: Some(5), ..Default::default() }));
        assert!(!a.same(&ClipId { size: 10, created: Some(6), ..Default::default() }));
    }

    /// A clip with a Gyroflow project of motion measured from the picture beside it (AFP_MOTION_CLIP=<video>): its
    /// project converted to a motion file (in a temporary folder, beside a link to the clip), and the clip loaded both
    /// ways as the plugin loads them. The camera's orientation must come out the same, the correction in full both times
    #[test]
    #[cfg(unix)]
    fn same_motion_as_the_project() {
        let Some(video) = std::env::var_os("AFP_MOTION_CLIP").map(PathBuf::from) else { return };
        let _ = simplelog::TermLogger::init(log::LevelFilter::Warn, simplelog::Config::default(), simplelog::TerminalMode::Stderr, simplelog::ColorChoice::Never);
        let project = crate::optical::own_project(&video).expect("a Gyroflow project beside the clip");
        let doc: Value = serde_json::from_slice(&std::fs::read(&project).unwrap()).unwrap();
        let dir = std::env::temp_dir().join(format!("afp-motion-clip-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let link = dir.join(video.file_name().unwrap());
        let _ = std::fs::remove_file(&link);
        std::os::unix::fs::symlink(&video, &link).unwrap();
        let m = from_project(&doc, &project, &video).unwrap();
        m.write(&path_for(&link)).unwrap();
        let t = std::time::Instant::now();
        let (a, _, _) = crate::load_manager(&project.to_string_lossy(), None, None, crate::Underwater::Project, 0.0, false, None).unwrap();
        let ta = t.elapsed();
        let t = std::time::Instant::now();
        let (b, _, _) = crate::load_manager(&link.to_string_lossy(), None, None, crate::Underwater::Auto, 0.0, false, None).unwrap();
        let tb = t.elapsed();
        // What the correction's context is made of, both ways, for when it differs
        for (name, stab) in [("project", &a), ("motion file", &b)] {
            use gyroflow_core::synchronization::optical_motion::measurement_params;
            let p = measurement_params(stab);
            let lens = stab.lens.read();
            eprintln!("{name}: {}x{} fps {} scaled {} duration {} scaled {} frames {} frame_times {} offsets {:?} readout {} {:?} offset {} k {:?} f {:?} input_stretch {} digital {:?}",
                p.width, p.height, stab.params.read().fps, p.scaled_fps, stab.params.read().duration_ms, p.scaled_duration_ms, p.frame_count, p.frame_times.is_some(),
                stab.gyro.read().get_offsets(), p.frame_readout_time, p.frame_readout_direction, stab.params.read().frame_offset,
                lens.fisheye_params.distortion_coeffs, lens.fisheye_params.camera_matrix.first(), lens.input_horizontal_stretch, lens.digital_lens);
        }
        let (ga, gb) = (a.gyro.read(), b.gyro.read());
        eprintln!("project {:.0} ms, motion file {:.0} ms; applied {} / {}; {} / {} orientations; refraction {} / {}; readout {} / {}",
            ta.as_secs_f64() * 1000.0, tb.as_secs_f64() * 1000.0, ga.optical_correction_applied, gb.optical_correction_applied,
            ga.quaternions.len(), gb.quaternions.len(), a.params.read().light_refraction_coefficient, b.params.read().light_refraction_coefficient,
            a.params.read().frame_readout_time, b.params.read().frame_readout_time);
        // As the project has it: one whose correction is stale in its own project (measured, then something it was
        // measured with changed in Gyroflow) plays without it there too
        assert_eq!(ga.optical_correction_applied, gb.optical_correction_applied, "the correction applies one way and not the other");
        assert_eq!(ga.quaternions.len(), gb.quaternions.len());
        let worst = ga.quaternions.iter().zip(gb.quaternions.iter()).map(|((ta, qa), (tb, qb))| {
            assert_eq!(ta, tb);
            qa.angle_to(qb)
        }).fold(0.0f64, f64::max);
        eprintln!("largest difference in the camera's orientation: {:.6}°", worst.to_degrees());
        assert!(worst.to_degrees() < 1e-4);
        drop((ga, gb));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn written_whole_and_found_beside_the_clip() {
        let dir = std::env::temp_dir().join(format!("afp-motion-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let video = dir.join("GOPR0001.MP4");
        std::fs::write(&video, vec![0u8; 1234]).unwrap();
        assert_eq!(path_for(&video), dir.join("GOPR0001.afp-motion"));
        assert!(read_for(&video).is_none());
        let header = Header { clip: ClipId::of(&video).unwrap(), ..Default::default() };
        MotionFile::new(header, correction()).unwrap().write(&path_for(&video)).unwrap();
        assert!(read_for(&video).unwrap().is_ok());
        assert!(header_for(&video).unwrap().is_ok());
        assert!(!stamp(&video).is_empty());
        // The clip changed (another file by the name): not its motion
        std::fs::write(&video, vec![0u8; 99]).unwrap();
        assert!(read_for(&video).unwrap().is_err());
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
