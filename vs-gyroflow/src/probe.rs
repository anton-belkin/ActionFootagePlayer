// SPDX-License-Identifier: GPL-3.0-or-later
//
// What a video can be stabilized with and the color profile it was recorded in: for gyroflow-probe (and through it
// the mpv scripts) and gyroflow-export, so both decide the same way.

use std::io::{ Read, Seek, SeekFrom };
use std::sync::{ Arc, atomic::{ AtomicBool, Ordering } };
use gyroflow_core::{ filesystem, util, camera_identifier::CameraIdentifier };
use crate::lens_pick;
use gyroflow_core::telemetry_parser::{ self, Input, InputOptions, util::SampleInfo, tags_impl::* };

/// The camera's maker and model as its metadata says (telemetry-parser's), for a still's Exif
pub fn camera(path: &str) -> Option<(String, Option<String>)> {
    let mut file = std::fs::File::open(path).ok()?;
    let size = file.metadata().ok()?.len() as usize;
    let options = InputOptions { probe_only: true, ..Default::default() };
    let input = Input::from_stream_with_options(&mut file, size, path, |_| (), Arc::new(AtomicBool::new(false)), options).ok()?;
    let maker = input.camera_type();
    if maker.is_empty() { return None; }
    Some((maker, input.camera_model().map(|m| m.to_string()).filter(|m| !m.is_empty())))
}

pub fn probe(path: &str) -> Result<serde_json::Value, String> {
    probe_with_camera_stabilization(path).0
}

/// [probe]'s answer, and whether the camera stabilized the picture itself (HyperSmooth, EIS, FlowState): its name
/// for it, "" when it recorded that it didn't, None when it doesn't say (or the file can't be read). Said whatever
/// motion data the clip has: a DJI with EIS on records none, a GoPro with HyperSmooth on records it all the same
pub fn probe_with_camera_stabilization(path: &str) -> (Result<serde_json::Value, String>, Option<String>) {
    let mut in_camera = None;
    let answer = probe_inner(path, &mut in_camera);
    (answer, in_camera)
}

fn probe_inner(path: &str, in_camera: &mut Option<String>) -> Result<serde_json::Value, String> {
    let url = filesystem::path_to_url(path);
    let mut file = std::fs::File::open(path).map_err(|e| e.to_string())?;
    let size = file.metadata().map_err(|e| e.to_string())?.len() as usize;

    let video = util::get_video_metadata(&mut file, size, &url).map_err(|e| e.to_string())?;
    // Up to the first metadata sample and gyro reading, which have what's decided here: a long clip has one sample
    // per frame spread over the whole file (36 538 in 7 GB of 4K60), ~20 s of reads from an SD card. The plugin
    // reads them all once it plays (Engine::load)
    let options = InputOptions { probe_only: true, ..Default::default() };
    let input = Input::from_stream_with_options(&mut file, size, path, |_| (), Arc::new(AtomicBool::new(false)), options).map_err(|e| e.to_string())?;
    let camera = input.camera_type();
    let samples = input.samples.as_deref().unwrap_or_default();
    *in_camera = stabilized_in_camera(&camera, samples);

    let mut motion_samples = 0;
    for s in samples {
        let Some(m) = s.tag_map.as_ref() else { continue };
        if let Some(TagValue::Vec_TimeQuaternion_f64(v)) = m.get(&GroupId::Quaternion).and_then(|g| g.get(&TagId::Data)).map(|t| &t.value) { motion_samples += v.get().len(); }
        if let Some(TagValue::Vec_TimeVector3_f64(v)) = m.get(&GroupId::Gyroscope).and_then(|g| g.get(&TagId::Data)).map(|t| &t.value) { motion_samples += v.get().len(); }
    }
    // Other cameras' gyro comes raw (GoPro: int16 with a scale), read the way Gyroflow reads it
    if motion_samples == 0 && telemetry_parser::util::normalized_imu(&input, None).is_ok_and(|v| v.iter().any(|x| x.gyro.is_some())) {
        motion_samples = 1;
    }
    if motion_samples == 0 {
        return Err(match camera.as_str() {
            "DJI" => "no motion data (DJI records it only with EIS off and the Wide FOV)".into(),
            _ => format!("{camera}: no motion data"),
        });
    }
    match in_camera.as_deref() {
        Some("") => (),
        // HyperSmooth on a GoPro that recorded how it turned the picture (IORI: the HERO8 and later): its motion data
        // is the picture's own, which is stabilized further (in_camera.rs). The HERO5-7 Black don't record it
        Some(_) if camera == "GoPro" && picture_turn_recorded(samples) => (),
        Some(how) => return Err(match camera.as_str() {
            "GoPro" => format!("{how} is on in the camera, which doesn't record how it turned the picture (HERO7 and older)"),
            "DJI" => format!("in-camera stabilization is on ({how})"),
            _ => format!("{how} is on in the camera"),
        }),
        None if ["DJI", "GoPro", "Insta360"].contains(&camera.as_str()) => return Err(format!("{camera}: in-camera stabilization state unknown")),
        None => return Err(format!("{camera}: only DJI, GoPro and Insta360 files are stabilized without a project")),
    }
    // DJI clips are left as they were before this check (the Osmo Action 6 plays well; no clip at hand to check that
    // its lens comes through here)
    if camera != "DJI" && !has_lens_profile(path, &input, samples, video.width, video.height, video.fps) {
        return Err(format!("{camera}: no lens profile for this camera and mode in Gyroflow's database"));
    }
    Ok(serde_json::json!({ "mode": "default", "width": video.width, "height": video.height, "fps": video.fps }))
}

// Whether a GoPro recorded how HyperSmooth turned the picture: IORI, the image's orientation, next to CORI, the
// camera's (HERO8 and later, HyperSmooth on or off). telemetry-parser gives Gyroflow CORI × IORI then, the picture's
// own turn. Checked on HERO12 Black clips (HS EIS, HS AutoBoost: IORI turning the picture by up to several degrees) and a
// HERO7 Black's (HS EIS: no CORI, no IORI)
fn picture_turn_recorded(samples: &[SampleInfo]) -> bool {
    samples.iter().filter_map(|s| s.tag_map.as_ref()?.get(&GroupId::ImageOrientation)).any(|m| m.contains_key(&TagId::Data))
}

// Whether the camera stabilized the picture itself: its name for how ("HyperSmooth (HS EIS)", "EIS: RockSteady",
// "FlowState"), "" when it recorded that it didn't, None when it doesn't say. Footage the camera already stabilized
// gets worse stabilized again, so only "" is stabilized here, and HyperSmooth on a GoPro that recorded how it turned
// the picture (picture_turn_recorded); on otherwise, or not said, plays as it is. The signs are the ones Gyroflow
// reads to pick a lens profile (gyroflow-core's camera_identifier.rs).
fn stabilized_in_camera(camera: &str, samples: &[SampleInfo]) -> Option<String> {
    let default_tags = || samples.iter().filter_map(|s| s.tag_map.as_ref()?.get(&GroupId::Default));
    let metadata = || default_tags().filter_map(|m| match m.get(&TagId::Metadata).map(|t| &t.value) {
        Some(TagValue::Json(v)) => Some(v.get()),
        _ => None,
    });
    match camera {
        // ClipMeta.eis_status (dvtm_library.proto's EisStatusType), written with EIS on or off. With EIS on the
        // cameras don't record motion data at all (Osmo Action 6). Named only where the enum's name is DJI's own mode;
        // the Osmo Action 6 writes 4 (EIS_TRADEOFF) for a mode not matched to its menu yet. EIS_OFF_WITH_CROP (7)
        // counts as on until a clip shows what it does
        "DJI" => metadata().find_map(|v| find_key(v, "eis_status").and_then(|x| x["status"].as_i64())).map(|s| match s {
            0 => String::new(),
            1 => "EIS: RockSteady".into(),
            2 => "EIS: HorizonSteady".into(),
            5 => "EIS: HorizonBalancing".into(),
            s => format!("EIS (status {s})"),
        }),
        // GoPro records the gyro whatever HyperSmooth is, so only its settings tell: EISA (what was applied: "N", "Y",
        // "HS EIS", "HS HLevel", ...; "N/A" says nothing), else EISE (enabled: "Y"/"N"). For HERO5/6 telemetry-parser
        // reads EISA from the SETT box. Checked on GoPro's sample clips (gopro/gpmf-parser): HERO5/6 "N", HERO7/8 "HS EIS"
        "GoPro" => {
            let tag = |fourcc: &[u8; 4]| default_tags().find_map(|m| match m.get(&TagId::Unknown(u32::from_be_bytes(*fourcc))).map(|t| &t.value) {
                Some(TagValue::String(v)) => Some(v.get().trim().to_string()).filter(|v| v != "N/A"),
                _ => None,
            });
            tag(b"EISA").or_else(|| tag(b"EISE")).map(|v| match v.as_str() {
                "N" => String::new(),
                "Y" => "HyperSmooth".into(),
                v => format!("HyperSmooth ({v})"),
            })
        },
        // Insta360 records the gyro always, and is_flowstate_online when the camera applied FlowState itself. A camera
        // that predates the field reads as false too (protobuf's default), as it does in Gyroflow
        "Insta360" => metadata().find_map(|v| v.get("is_flowstate_online")?.as_bool()).map(|on| if on { "FlowState".into() } else { String::new() }),
        _ => None,
    }
}

// Without a lens profile the stabilization is worse than none. Newer GoPros and some Insta360s record their lens in
// the file; for the rest a curated profile of the clip's lens type, or the best profile of the camera and lens mode
// in Gyroflow's database (lens_pick.rs)
fn has_lens_profile(path: &str, input: &Input, samples: &[SampleInfo], width: usize, height: usize, fps: f64) -> bool {
    let builtin = lens_in_file(samples);
    builtin || chosen_lens(path, input, width, height, fps, builtin).is_some()
}

// Whether the file has its own lens (DJI with EIS off, newer GoPros)
fn lens_in_file(samples: &[SampleInfo]) -> bool {
    samples.iter().filter_map(|s| s.tag_map.as_ref()?.get(&GroupId::Lens))
        .any(|m| m.contains_key(&TagId::Data) || m.contains_key(&TagId::Name))
}

/// The profile the player loads for the video, as `lens_pick::apply` does when it plays: the curated one of its lens
/// type, else (unless the lens is in the file) the database's pick
fn chosen_lens(path: &str, input: &Input, width: usize, height: usize, fps: f64, in_file: bool) -> Option<lens_pick::Pick> {
    let id = CameraIdentifier::from_telemetry_parser(input, width, height, fps).ok()?;
    lens_pick::for_video(path, &id, in_file)
}

/// The camera and its lens as the player takes them, for the side panel's Source tab and the Library
#[derive(Debug, Default, serde::Serialize)]
pub struct LensInfo {
    /// As Gyroflow tells it: "DJI OsmoAction6"
    pub camera: String,
    /// "in the file", the label of the profile loaded (lens_pick.rs: the database's pick with its stars, "Nearest: "
    /// when it isn't made for exactly this mode, or "<Type>, from the curated list: <name>"), or "" when none
    pub lens: String,
    /// The lens types the curated list has for the camera ({id, label, profile}), empty for most cameras
    pub types: Vec<lens_pick::LensType>,
    /// The type given to the clip by hand (source.lens_type in its .afp.json), and the one the camera recorded
    #[serde(rename = "type")]
    pub chosen: Option<String>,
    pub recorded: Option<String>,
    /// The lens mode the camera wrote, as its number (DJI's fov_type, the HERO4's SETT), for a camera with lens types: shown as
    /// "Unknown (mode 6)" while the curated list doesn't name it
    pub mode: Option<u64>,
}

pub fn lens_info(path: &str) -> Option<LensInfo> {
    // A file telemetry-parser can't read (a Mavic Pro's or Mavic Air 2's MOV): the drone's name alone, from the file
    lens_read(path).or_else(|| crate::geotag::dji_model(std::path::Path::new(path)).map(|camera| LensInfo { camera, ..Default::default() }))
}

fn lens_read(path: &str) -> Option<LensInfo> {
    let url = filesystem::path_to_url(path);
    let mut file = std::fs::File::open(path).ok()?;
    let size = file.metadata().ok()?.len() as usize;
    let video = util::get_video_metadata(&mut file, size, &url).ok()?;
    let options = InputOptions { probe_only: true, ..Default::default() };
    let input = Input::from_stream_with_options(&mut file, size, path, |_| (), Arc::new(AtomicBool::new(false)), options).ok()?;
    let mut camera = format!("{} {}", input.camera_type(), input.camera_model().map(|s| s.as_str()).unwrap_or("")).trim().to_string();
    // A DJI drone telemetry-parser names only "DJI" (Mini 4 Pro) or not at all (Mavic Pro, Mavic 2): its name from the
    // file (geotag.rs)
    if camera.is_empty() || camera == "DJI" {
        if let Some(m) = crate::geotag::dji_model(std::path::Path::new(path)) { camera = m; }
    }
    let in_file = lens_in_file(input.samples.as_deref().unwrap_or_default());
    let lens = match chosen_lens(path, &input, video.width, video.height, video.fps, in_file) {
        Some(p) => p.label(),
        None if in_file => "in the file".to_string(),
        None => String::new(),
    };
    let types = lens_pick::lens_types(&camera);
    let recorded = if types.is_empty() { None } else { lens_pick::recorded_type(&camera, path) };
    let mode = if types.is_empty() { None } else { recorded_lens_mode(path) };
    Some(LensInfo { chosen: lens_pick::chosen_type(path), recorded, mode, types, camera, lens })
}

/// The camera as Gyroflow tells it ("DJI OsmoAction6") and the lens profile the player loads for the video
/// (`LensInfo::lens`): what the side panel shows, and what a cut must still read as (cut.rs)
pub fn camera_and_lens(path: &str) -> Option<(String, String)> {
    lens_info(path).map(|l| (l.camera, l.lens))
}

fn find_key<'a>(v: &'a serde_json::Value, key: &str) -> Option<&'a serde_json::Value> {
    match v {
        serde_json::Value::Object(map) => map.get(key).filter(|x| !x.is_null()).or_else(|| map.values().find_map(|x| find_key(x, key))),
        serde_json::Value::Array(arr) => arr.iter().find_map(|x| find_key(x, key)),
        _ => None
    }
}

// The color profile DJI recorded the video in: ColorMode (dvtm_library.proto) in the stream metadata of the first
// sample of the camera's metadata track. DJI tags D-Log M video as BT.709 and the file name doesn't tell D-Log M
// from Normal, so this is the only reliable way to know. telemetry-parser doesn't decode the field for the Osmo
// Action 6 (its schema for dvtm_ac206 has no ColorMode), so it's read from the protobuf here. The dlogm mpv plugin
// reads it the same way, in Lua (mpv/scripts/dlogm/dji_color.lua): keep the two in step.
pub fn color_profile(path: &str) -> Option<&'static str> {
    // Where each model's schema puts ColorMode in StreamMeta. Other models use field 4 for other things (FOVType in
    // dvtm_wa530), so only the ones checked against their .proto are read
    const STREAM_COLOR_MODE_FIELD: &[(&str, u64)] = &[
        ("dvtm_ac206.proto", 4), // Osmo Action 6
        ("dvtm_oq101.proto", 4), // Osmo 360
    ];
    let mut file = std::fs::File::open(path).ok()?;
    let size = file.metadata().ok()?.len() as usize;
    let mut mode = None;
    let cancel = Arc::new(AtomicBool::new(false));
    let cancel2 = cancel.clone();
    let _ = telemetry_parser::util::get_metadata_track_samples(&mut file, size, true, |_info, data: &[u8], _pos, _md| {
        if cancel2.load(Ordering::Relaxed) { return; }
        cancel2.store(true, Ordering::Relaxed);
        // ProductMeta { clip_meta = 1, stream_meta = 2 }
        let Some(proto) = dji_schema(data) else { return };
        let Some(&(_, field)) = STREAM_COLOR_MODE_FIELD.iter().find(|(p, _)| *p == proto) else { return };
        // A ColorMode message without its field is the default (0) in proto3
        mode = pb_bytes(data, 2).and_then(|s| pb_bytes(s, field)).map(|c| pb_varint(c, 1).unwrap_or(0));
    }, cancel);
    Some(match mode? {
        0 => "normal",
        1 => "d-cinelike",
        2 => "dlog",
        9 => "hlg",
        19 => "dlogm",
        22 => "dlog2",
        _ => "other",
    })
}

/// The lens mode (FOV) a DJI camera recorded the video in: StreamMeta.fov_type (dvtm_library.proto's FOVType: 0
/// default, 1 normal, 2 narrow, 3 wide, 4 snarrow, 5 natural wide, 6 extreme wide, 7 flat) in the first sample of its
/// metadata track. The Osmo Action 6 writes it in every clip, with EIS on or off and with or without motion data,
/// while its lens's own calibration only comes with EIS off in its usual mode. The enum's names aren't the camera's:
/// what each is on the Action 6 is in resources/curated_lenses.json (lens_pick.rs). None for other cameras
/// The lens mode the camera wrote, as its own number: DJI's fov_type, the HERO4's SETT (markers::hero4_lens_mode).
/// What curated_lenses.json's "recorded" names
pub fn recorded_lens_mode(path: &str) -> Option<u64> {
    dji_fov_type(path).or_else(|| crate::markers::hero4_lens_mode(path))
}

pub fn dji_fov_type(path: &str) -> Option<u64> {
    // Where each model's schema puts FOVType in StreamMeta (checked against the .proto, and on Action 6 clips)
    const STREAM_FOV_FIELD: &[(&str, u64)] = &[
        ("dvtm_ac206.proto", 5), // Osmo Action 6
        ("dvtm_oq101.proto", 5), // Osmo 360
    ];
    let mut file = std::fs::File::open(path).ok()?;
    let size = file.metadata().ok()?.len() as usize;
    let mut fov = None;
    let cancel = Arc::new(AtomicBool::new(false));
    let cancel2 = cancel.clone();
    let _ = telemetry_parser::util::get_metadata_track_samples(&mut file, size, true, |_info, data: &[u8], _pos, _md| {
        if cancel2.load(Ordering::Relaxed) { return; }
        cancel2.store(true, Ordering::Relaxed);
        let Some(proto) = dji_schema(data) else { return };
        let Some(&(_, field)) = STREAM_FOV_FIELD.iter().find(|(p, _)| *p == proto) else { return };
        // A FOVType message without its field is the default (0) in proto3
        fov = pb_bytes(data, 2).and_then(|s| pb_bytes(s, field)).map(|c| pb_varint(c, 1).unwrap_or(0));
    }, cancel);
    fov
}

// GP-Log as gplog_hlg.cube is made for: by the firmware where GoPro's own LUTs put 18 % grey where it does. Earlier
// HERO12 firmware recorded 2 stops darker, MAX2 2 stops brighter. The dlogm plugin's GPLOG_FIRMWARE: keep the two in step
const GPLOG_FIRMWARE: &[(&str, f64)] = &[("HERO12 Black", 1.20), ("HERO13 Black", 0.0)];

/// The log curve the video was recorded in, decided as the dlogm mpv plugin decides it (recorded_curve in
/// mpv/scripts/dlogm/main.lua): DJI's ColorMode (dlogm, dlog), else GoPro's Protune color (gplog2, or gplog from a
/// camera and firmware in GPLOG_FIRMWARE). None when it isn't log, or can't be told
pub fn log_curve(path: &str) -> Option<&'static str> {
    if let Some(color) = color_profile(path) {
        return match color { "dlogm" => Some("dlogm"), "dlog" => Some("dlog"), _ => None };
    }
    let (color, camera, firmware) = gopro_color(path)?;
    match color.as_str() {
        "GPLOG2" => Some("gplog2"),
        "LOG" => {
            let &(_, min) = GPLOG_FIRMWARE.iter().find(|(c, _)| Some(*c) == camera.as_deref())?;
            firmware.filter(|f| *f >= min).map(|_| "gplog")
        }
        _ => None,
    }
}

/// The Protune color a GoPro recorded the video in (PTCL: "NATURAL", "FLAT", "LOG", "GPLOG2", ...), the camera (MINF)
/// and its firmware (FIRM, H23.01.02.32.70 -> 2.32), from moov/udta. The same as mpv/scripts/dlogm/gopro_color.lua,
/// which says where they are: keep the two in step
pub fn gopro_color(path: &str) -> Option<(String, Option<String>, Option<f64>)> {
    let mut file = std::fs::File::open(path).ok()?;
    let size = file.metadata().ok()?.len() as usize;
    let moov = read_moov(&mut file, size as u64)?;
    let udta = mp4_child(&moov, b"udta")?;
    let gpmf = mp4_child(udta, b"GPMF")?;
    let firmware = mp4_child(udta, b"FIRM").and_then(text).and_then(|f| firmware_version(&f));
    Some((text(klv(gpmf, b"PTCL")?)?, klv(gpmf, b"MINF").and_then(text), firmware))
}

/// What a stitched 360° video says it is, from its first video track: Spherical Video V2 (`sv3d` and `st3d` in the
/// sample entry: the projection, its pose and, for an equirect, the bounds it covers) or V1 (the XMP in the track's
/// `uuid` box: Spherical, the projection, the stereo mode and the initial view). None for any other video. The view360
/// mpv script shows it as a sphere to look around in (docs/360-cameras.md, "Stitched clips")
pub fn spherical(path: &str) -> Option<serde_json::Value> {
    let mut file = std::fs::File::open(path).ok()?;
    let size = file.metadata().ok()?.len();
    spherical_in_moov(&read_moov(&mut file, size)?)
}

fn spherical_in_moov(moov: &[u8]) -> Option<serde_json::Value> {
    for (kind, trak) in mp4_children(moov) {
        if kind != *b"trak" { continue; }
        let Some(mdia) = mp4_child(trak, b"mdia") else { continue };
        // hdlr: version and flags, pre_defined, then the handler type
        if mp4_child(mdia, b"hdlr").and_then(|h| h.get(8..12)) != Some(b"vide") { continue; }
        let stereo_v2 = |entry: &[u8]| mp4_child(entry, b"st3d").and_then(|b| b.get(4)).map(|&m| match m {
            0 => "mono", 1 => "top-bottom", 2 => "left-right", _ => "other",
        });
        // The first sample entry's children, after the visual sample entry's 78 bytes (ISO 14496-12, 12.1.3)
        let entry = mp4_child(mdia, b"minf").and_then(|m| mp4_child(m, b"stbl")).and_then(|m| mp4_child(m, b"stsd"))
            .and_then(|stsd| stsd.get(8..)).and_then(|entries| mp4_children(entries).next()).and_then(|(_, e)| e.get(78..));
        if let Some(sv3d) = entry.and_then(|e| mp4_child(e, b"sv3d")) {
            let entry = entry.unwrap_or_default();
            let proj = mp4_child(sv3d, b"proj").unwrap_or_default();
            // prhd: version and flags, then yaw, pitch and roll in degrees, 16.16 fixed point
            let pose = mp4_child(proj, b"prhd").and_then(|p| p.get(4..16)).map(|p| {
                let at = |i: usize| i32::from_be_bytes(p[i..i + 4].try_into().unwrap()) as f64 / 65536.0;
                serde_json::json!({ "yaw": at(0), "pitch": at(4), "roll": at(8) })
            });
            let mut out = serde_json::json!({ "source": "sv3d", "stereo": stereo_v2(entry).unwrap_or("mono"), "pose": pose });
            if let Some(equi) = mp4_child(proj, b"equi") {
                out["projection"] = "equirect".into();
                // The share of the full sphere's picture cropped off each side, 0.32 fixed point; all 0 for a whole one
                if let Some(b) = equi.get(4..20) {
                    let at = |i: usize| u32::from_be_bytes(b[i..i + 4].try_into().unwrap()) as f64 / 4294967296.0;
                    if b.iter().any(|&x| x != 0) {
                        out["bounds"] = serde_json::json!({ "top": at(0), "bottom": at(4), "left": at(8), "right": at(12) });
                    }
                }
            } else if mp4_child(proj, b"cbmp").is_some() {
                out["projection"] = "cubemap".into();
            } else if mp4_child(proj, b"mshp").is_some() {
                out["projection"] = "mesh".into();
            } else {
                out["projection"] = "other".into();
            }
            return Some(out);
        }
        // V1: an XMP packet in trak/uuid with this UUID
        const V1_UUID: [u8; 16] = [0xff, 0xcc, 0x82, 0x63, 0xf8, 0x55, 0x4a, 0x93, 0x88, 0x14, 0x58, 0x7a, 0x02, 0x52, 0x1f, 0xdd];
        for (kind, uuid) in mp4_children(trak) {
            if kind != *b"uuid" || uuid.get(..16) != Some(&V1_UUID[..]) { continue; }
            let xmp = String::from_utf8_lossy(&uuid[16..]);
            if xmp_value(&xmp, "Spherical").is_none_or(|v| !v.eq_ignore_ascii_case("true")) { continue; }
            let projection = match xmp_value(&xmp, "ProjectionType").map(|v| v.to_ascii_lowercase()).as_deref() {
                Some("equirectangular") | None => "equirect",
                _ => "other",
            };
            let stereo = match xmp_value(&xmp, "StereoMode").map(|v| v.to_ascii_lowercase()).as_deref() {
                Some("top-bottom") => "top-bottom",
                Some("left-right") => "left-right",
                Some("mono") | None => "mono",
                _ => "other",
            };
            let degrees = |key: &str| xmp_value(&xmp, key).and_then(|v| v.trim().parse::<f64>().ok());
            let view = match (degrees("InitialViewHeadingDegrees"), degrees("InitialViewPitchDegrees"), degrees("InitialViewRollDegrees")) {
                (None, None, None) => serde_json::Value::Null,
                (h, p, r) => serde_json::json!({ "heading": h.unwrap_or(0.0), "pitch": p.unwrap_or(0.0), "roll": r.unwrap_or(0.0) }),
            };
            return Some(serde_json::json!({ "source": "xmp", "projection": projection, "stereo": stereo,
                                            "stitched": xmp_value(&xmp, "Stitched").map(|v| v.eq_ignore_ascii_case("true")),
                                            "initial_view": view }));
        }
    }
    None
}

// A GSpherical value in Spherical Video V1's XMP, as an element (<GSpherical:Key>value</GSpherical:Key>) or an
// attribute (GSpherical:Key="value")
fn xmp_value<'a>(xmp: &'a str, key: &str) -> Option<&'a str> {
    let open = format!("<GSpherical:{key}>");
    if let Some(i) = xmp.find(&open) {
        let rest = &xmp[i + open.len()..];
        return rest.find('<').map(|j| rest[..j].trim());
    }
    let attr = format!("GSpherical:{key}=");
    let i = xmp.find(&attr)?;
    let rest = &xmp[i + attr.len()..];
    let quote = rest.chars().next().filter(|c| *c == '"' || *c == '\'')?;
    let rest = &rest[1..];
    rest.find(quote).map(|j| rest[..j].trim())
}

// The boxes in s (a box's payload), in order: their type and payload. Stops at the first that doesn't fit
fn mp4_children(s: &[u8]) -> impl Iterator<Item = ([u8; 4], &[u8])> {
    let mut pos = 0usize;
    std::iter::from_fn(move || {
        if pos + 8 > s.len() { return None; }
        let (size, header) = match u32::from_be_bytes(s[pos..pos + 4].try_into().ok()?) {
            1 => (usize::try_from(u64::from_be_bytes(s.get(pos + 8..pos + 16)?.try_into().ok()?)).ok()?, 16),
            0 => (s.len() - pos, 8),
            n => (n as usize, 8),
        };
        let end = pos.checked_add(size).filter(|&e| size >= header && e <= s.len())?;
        let kind: [u8; 4] = s[pos + 4..pos + 8].try_into().ok()?;
        let payload = &s[pos + header..end];
        pos = end;
        Some((kind, payload))
    })
}

// moov is read whole; a camera's is a few MB at most
const MAX_MOOV: u64 = 64 * 1024 * 1024;

// The moov box's payload, from the file: top-level boxes are walked with seeks, mdat isn't read
fn read_moov<R: Read + Seek>(f: &mut R, file_size: u64) -> Option<Vec<u8>> {
    let mut pos = 0u64;
    while pos + 8 <= file_size {
        f.seek(SeekFrom::Start(pos)).ok()?;
        let mut head = [0u8; 8];
        f.read_exact(&mut head).ok()?;
        let (mut size, mut header) = (u32::from_be_bytes(head[..4].try_into().ok()?) as u64, 8u64);
        if size == 1 {
            let mut large = [0u8; 8];
            f.read_exact(&mut large).ok()?;
            (size, header) = (u64::from_be_bytes(large), 16);
        } else if size == 0 {
            size = file_size - pos;
        }
        if size < header { return None; }
        if &head[4..] == b"moov" {
            if size > MAX_MOOV { return None; }
            let mut moov = vec![0u8; (size - header) as usize];
            f.read_exact(&mut moov).ok()?;
            return Some(moov);
        }
        pos = pos.checked_add(size)?;
    }
    None
}

// The payload of the first box of this type in s (a box's payload)
fn mp4_child<'a>(s: &'a [u8], wanted: &[u8; 4]) -> Option<&'a [u8]> {
    mp4_children(s).find(|(kind, _)| kind == wanted).map(|(_, payload)| payload)
}

// The data of the first KLV item with the key, looking into nested ones (type 0). An item: the key (4 bytes), its
// type (1), the size of one sample (1) and how many (2, big-endian), then the data, padded to 4 bytes
fn klv<'a>(s: &'a [u8], wanted: &[u8; 4]) -> Option<&'a [u8]> {
    let mut pos = 0usize;
    while pos + 8 <= s.len() {
        let n = s[pos + 5] as usize * u16::from_be_bytes([s[pos + 6], s[pos + 7]]) as usize;
        let data = s.get(pos + 8..pos + 8 + n)?;
        if &s[pos..pos + 4] == wanted { return Some(data); }
        if s[pos + 4] == 0 && let Some(v) = klv(data, wanted) { return Some(v); }
        pos += 8 + n.div_ceil(4) * 4;
    }
    None
}

// A string GoPro wrote, without its padding; None when empty
fn text(v: &[u8]) -> Option<String> {
    let v = v.split(|&b| b == 0).next().unwrap_or_default();
    Some(String::from_utf8_lossy(v).trim().to_string()).filter(|t| !t.is_empty())
}

// H23.01.02.32.70 -> 2.32
fn firmware_version(firm: &str) -> Option<f64> {
    let parts: Vec<&str> = firm.split('.').collect();
    let digits = |s: &str| !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit());
    if parts.len() < 4 || parts[0].is_empty() || !parts[0].bytes().all(|b| b.is_ascii_alphanumeric()) || !parts[1..4].iter().all(|p| digits(p)) {
        return None;
    }
    Some(parts[2].parse::<f64>().ok()? + parts[3].parse::<f64>().ok()? / 100.0)
}

/// The schema a DJI metadata sample (ProductMeta) names, which tells the model: "dvtm_ac206.proto" (clip_meta = 1 {
/// clip_meta_header = 1 { proto_file_name = 1 } })
pub(crate) fn dji_schema(sample: &[u8]) -> Option<&str> {
    let header = pb_bytes(sample, 1).and_then(|c| pb_bytes(c, 1))?;
    pb_bytes(header, 1).and_then(|s| std::str::from_utf8(s).ok())
}

// Minimal protobuf reading: the first occurrence of a field in a message
enum PbValue<'a> { Varint(u64), Bytes(&'a [u8]) }

fn pb_field(mut msg: &[u8], wanted: u64) -> Option<PbValue<'_>> {
    fn varint(buf: &mut &[u8]) -> Option<u64> {
        let mut v = 0u64;
        for shift in (0..64).step_by(7) {
            let (&b, rest) = buf.split_first()?;
            *buf = rest;
            v |= ((b & 0x7f) as u64) << shift;
            if b & 0x80 == 0 { return Some(v); }
        }
        None
    }
    while !msg.is_empty() {
        let key = varint(&mut msg)?;
        let value = match key & 7 {
            0 => PbValue::Varint(varint(&mut msg)?),
            1 => { let (v, rest) = msg.split_at_checked(8)?; msg = rest; PbValue::Bytes(v) }
            2 => { let len = varint(&mut msg)? as usize; let (v, rest) = msg.split_at_checked(len)?; msg = rest; PbValue::Bytes(v) }
            5 => { let (v, rest) = msg.split_at_checked(4)?; msg = rest; PbValue::Bytes(v) }
            _ => return None,
        };
        if key >> 3 == wanted { return Some(value); }
    }
    None
}
pub(crate) fn pb_bytes(msg: &[u8], field: u64) -> Option<&[u8]> {
    match pb_field(msg, field)? { PbValue::Bytes(b) => Some(b), _ => None }
}
pub(crate) fn pb_varint(msg: &[u8], field: u64) -> Option<u64> {
    match pb_field(msg, field)? { PbValue::Varint(v) => Some(v), _ => None }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(key: &[u8; 4], typ: u8, data: &[u8]) -> Vec<u8> {
        let mut v = key.to_vec();
        v.extend([typ, 1, 0, data.len() as u8]);
        v.extend(data);
        v.resize(8 + data.len().div_ceil(4) * 4, 0);
        v
    }

    fn mp4_box(typ: &[u8; 4], payload: &[u8]) -> Vec<u8> {
        let mut v = ((payload.len() + 8) as u32).to_be_bytes().to_vec();
        v.extend(typ);
        v.extend(payload);
        v
    }

    #[test]
    fn gopro_settings() {
        // DEVC { DVID, STRM { MINF, PTCL } }, as a HERO12 writes its global settings
        let strm = [item(b"MINF", b'c', b"HERO12 Black"), item(b"PTCL", b'c', b"LOG\0")].concat();
        let devc = [item(b"DVID", b'L', &[0, 0, 0, 1]), item(b"STRM", 0, &strm)].concat();
        let gpmf = item(b"DEVC", 0, &devc);
        let udta = [mp4_box(b"FIRM", b"H23.01.01.20.00"), mp4_box(b"GPMF", &gpmf)].concat();
        let moov = mp4_box(b"udta", &udta);
        let udta = mp4_child(&moov, b"udta").unwrap();
        let gpmf = mp4_child(udta, b"GPMF").unwrap();
        assert_eq!(klv(gpmf, b"PTCL").and_then(text).as_deref(), Some("LOG"));
        assert_eq!(klv(gpmf, b"MINF").and_then(text).as_deref(), Some("HERO12 Black"));
        assert_eq!(klv(gpmf, b"XXXX"), None);
        let firm = mp4_child(udta, b"FIRM").and_then(text).unwrap();
        assert_eq!(firmware_version(&firm), Some(1.20));
        assert_eq!(firmware_version("H23.01.02.32.70"), Some(2.32));
        assert_eq!(firmware_version("garbage"), None);
    }

    #[test]
    fn moov_from_a_file() {
        let mut file = [mp4_box(b"ftyp", b"isom"), mp4_box(b"mdat", &[0; 100]), mp4_box(b"moov", &mp4_box(b"udta", b""))].concat();
        let len = file.len() as u64;
        let moov = read_moov(&mut std::io::Cursor::new(&mut file), len).unwrap();
        assert!(mp4_child(&moov, b"udta").is_some());
        // Truncated: no moov
        let mut cut = file[..20].to_vec();
        assert!(read_moov(&mut std::io::Cursor::new(&mut cut), 20).is_none());
    }

    // A video track with these boxes after its sample entry's 78 bytes, and these next to its mdia
    fn video_trak(entry_children: &[u8], beside: &[u8]) -> Vec<u8> {
        let mut entry = vec![0u8; 78];
        entry.extend(entry_children);
        let stsd = [vec![0, 0, 0, 0, 0, 0, 0, 1], mp4_box(b"hvc1", &entry)].concat();
        let stbl = mp4_box(b"stbl", &mp4_box(b"stsd", &stsd));
        let hdlr = mp4_box(b"hdlr", &[&[0u8; 8][..], b"vide", &[0u8; 13]].concat());
        let mdia = mp4_box(b"mdia", &[hdlr, mp4_box(b"minf", &stbl)].concat());
        mp4_box(b"trak", &[mdia, beside.to_vec()].concat())
    }

    #[test]
    fn spherical_v2() {
        let fixed = |v: f64| ((v * 65536.0) as i32).to_be_bytes();
        let prhd = mp4_box(b"prhd", &[&[0u8; 4][..], &fixed(90.0), &fixed(-10.5), &fixed(0.0)].concat());
        let equi = mp4_box(b"equi", &[0u8; 20]);
        let sv3d = mp4_box(b"sv3d", &[mp4_box(b"svhd", b"\0\0\0\0test\0"), mp4_box(b"proj", &[prhd, equi].concat())].concat());
        let st3d = mp4_box(b"st3d", &[0, 0, 0, 0, 1]);
        // An audio track first: only video counts
        let audio = mp4_box(b"trak", &mp4_box(b"mdia", &mp4_box(b"hdlr", &[&[0u8; 8][..], b"soun"].concat())));
        let moov = [audio, video_trak(&[st3d, sv3d].concat(), b"")].concat();
        let v = spherical_in_moov(&moov).unwrap();
        assert_eq!(v["source"], "sv3d");
        assert_eq!(v["projection"], "equirect");
        assert_eq!(v["stereo"], "top-bottom");
        assert_eq!(v["pose"]["yaw"], 90.0);
        assert_eq!(v["pose"]["pitch"], -10.5);
        assert!(v.get("bounds").is_none());
        // A partial one: its bounds
        let mut b = vec![0u8; 4];
        for share in [0.25f64, 0.25, 0.0, 0.0] { b.extend(((share * 4294967296.0) as u32).to_be_bytes()); }
        let sv3d = mp4_box(b"sv3d", &mp4_box(b"proj", &mp4_box(b"equi", &b)));
        let v = spherical_in_moov(&video_trak(&sv3d, b"")).unwrap();
        assert_eq!(v["stereo"], "mono");
        assert_eq!(v["bounds"]["top"], 0.25);
        assert_eq!(v["bounds"]["left"], 0.0);
        // A cubemap
        let sv3d = mp4_box(b"sv3d", &mp4_box(b"proj", &mp4_box(b"cbmp", &[0u8; 12])));
        assert_eq!(spherical_in_moov(&video_trak(&sv3d, b"")).unwrap()["projection"], "cubemap");
        // An ordinary video
        assert!(spherical_in_moov(&video_trak(b"", b"")).is_none());
    }

    #[test]
    fn spherical_v1() {
        let xmp = "<?xml version=\"1.0\"?><rdf:SphericalVideo xmlns:rdf=\"http://www.w3.org/1999/02/22-rdf-syntax-ns#\" \
                   xmlns:GSpherical=\"http://ns.google.com/videos/1.0/spherical/\"><GSpherical:Spherical>true</GSpherical:Spherical>\
                   <GSpherical:Stitched>true</GSpherical:Stitched><GSpherical:StitchingSoftware>test</GSpherical:StitchingSoftware>\
                   <GSpherical:ProjectionType>equirectangular</GSpherical:ProjectionType>\
                   <GSpherical:InitialViewHeadingDegrees>45</GSpherical:InitialViewHeadingDegrees></rdf:SphericalVideo>";
        let uuid = [&[0xff, 0xcc, 0x82, 0x63, 0xf8, 0x55, 0x4a, 0x93, 0x88, 0x14, 0x58, 0x7a, 0x02, 0x52, 0x1f, 0xdd][..], xmp.as_bytes()].concat();
        let v = spherical_in_moov(&video_trak(b"", &mp4_box(b"uuid", &uuid))).unwrap();
        assert_eq!(v["source"], "xmp");
        assert_eq!(v["projection"], "equirect");
        assert_eq!(v["stereo"], "mono");
        assert_eq!(v["stitched"], true);
        assert_eq!(v["initial_view"]["heading"], 45.0);
        assert_eq!(xmp_value("<x GSpherical:Spherical='true'/>", "Spherical"), Some("true"));
        // Another uuid box, or Spherical false: not a sphere
        let other = [&[0u8; 16][..], xmp.as_bytes()].concat();
        assert!(spherical_in_moov(&video_trak(b"", &mp4_box(b"uuid", &other))).is_none());
        let flat = xmp.replace(">true</GSpherical:Spherical>", ">false</GSpherical:Spherical>");
        let uuid = [&uuid[..16], flat.as_bytes()].concat();
        assert!(spherical_in_moov(&video_trak(b"", &mp4_box(b"uuid", &uuid))).is_none());
    }
}
