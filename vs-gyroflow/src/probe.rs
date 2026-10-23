// SPDX-License-Identifier: GPL-3.0-or-later
//
// What a video can be stabilized with and the color profile it was recorded in: for gyroflow-probe (and through it
// the mpv scripts) and gyroflow-export, so both decide the same way.

use std::sync::{ Arc, atomic::{ AtomicBool, Ordering } };
use gyroflow_core::{ filesystem, util, camera_identifier::CameraIdentifier, lens_profile_database::LensProfileDatabase };
use gyroflow_core::telemetry_parser::{ self, Input, util::SampleInfo, tags_impl::* };

use crate::embedded;

pub fn probe(path: &str) -> Result<serde_json::Value, String> {
    let url = filesystem::path_to_url(path);
    let mut file = std::fs::File::open(path).map_err(|e| e.to_string())?;
    let size = file.metadata().map_err(|e| e.to_string())?.len() as usize;

    if let Some(project) = embedded::read(std::path::Path::new(path)).map_err(|e| e.to_string())? {
        let info = &project["video_info"];
        return Ok(serde_json::json!({ "mode": "embedded", "width": info["width"], "height": info["height"], "fps": info["fps"] }));
    }

    let (mut file, size) = embedded::Content::new(&mut file, size);
    let video = util::get_video_metadata(&mut file, size, &url).map_err(|e| e.to_string())?;
    let input = Input::from_stream(&mut file, size, path, |_| (), Arc::new(AtomicBool::new(false))).map_err(|e| e.to_string())?;
    let camera = input.camera_type();
    let samples = input.samples.as_deref().unwrap_or_default();

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
    in_camera_stabilization_off(&camera, samples)?;
    // DJI clips are left as they were before this check (the Osmo Action 6 plays well; no clip at hand to check that
    // its lens comes through here)
    if camera != "DJI" && !has_lens_profile(&input, samples, video.width, video.height, video.fps) {
        return Err(format!("{camera}: no lens profile for this camera and mode in Gyroflow's database"));
    }
    Ok(serde_json::json!({ "mode": "default", "width": video.width, "height": video.height, "fps": video.fps }))
}

// Ok when the camera recorded that it didn't stabilize the picture itself (EIS). Footage the camera already
// stabilized gets worse stabilized again, so on, or not said, is an Err with the reason: such clips play as they are.
// The signs are the ones Gyroflow reads to pick a lens profile (gyroflow-core's camera_identifier.rs).
fn in_camera_stabilization_off(camera: &str, samples: &[SampleInfo]) -> Result<(), String> {
    let default_tags = || samples.iter().filter_map(|s| s.tag_map.as_ref()?.get(&GroupId::Default));
    let metadata = || default_tags().filter_map(|m| match m.get(&TagId::Metadata).map(|t| &t.value) {
        Some(TagValue::Json(v)) => Some(v.get()),
        _ => None,
    });
    match camera {
        // ClipMeta.eis_status (dvtm_library.proto): EIS_OFF is 0. With EIS on the cameras don't record motion data at
        // all (Osmo Action 6), so this mostly confirms it. EIS_OFF_WITH_CROP (7) and the horizon modes count as on
        // until a clip shows what they do
        "DJI" => match metadata().find_map(|v| find_key(v, "eis_status").and_then(|x| x["status"].as_i64())) {
            Some(0) => Ok(()),
            Some(s) => Err(format!("in-camera stabilization is on (EIS status {s})")),
            None => Err("in-camera stabilization state unknown".into()),
        },
        // GoPro records the gyro whatever HyperSmooth is, so only its settings tell: EISA (what was applied: "N", "Y",
        // "HS EIS", "HS HLevel", ...; "N/A" says nothing), else EISE (enabled: "Y"/"N"). For HERO5/6 telemetry-parser
        // reads EISA from the SETT box. Checked on GoPro's sample clips (gopro/gpmf-parser): HERO5/6 "N", HERO7/8 "HS EIS"
        "GoPro" => {
            let tag = |fourcc: &[u8; 4]| default_tags().find_map(|m| match m.get(&TagId::Unknown(u32::from_be_bytes(*fourcc))).map(|t| &t.value) {
                Some(TagValue::String(v)) => Some(v.get().trim().to_string()).filter(|v| v != "N/A"),
                _ => None,
            });
            match tag(b"EISA").or_else(|| tag(b"EISE")).as_deref() {
                Some("N") => Ok(()),
                Some(v) => Err(format!("HyperSmooth is on ({v})")),
                None => Err("GoPro: HyperSmooth state unknown".into()),
            }
        },
        // Insta360 records the gyro always, and is_flowstate_online when the camera applied FlowState itself. A camera
        // that predates the field reads as false too (protobuf's default), as it does in Gyroflow
        "Insta360" => match metadata().find_map(|v| v.get("is_flowstate_online")?.as_bool()) {
            Some(false) => Ok(()),
            Some(true) => Err("FlowState is on in the camera".into()),
            None => Err("Insta360: FlowState state unknown".into()),
        },
        _ => Err(format!("{camera}: only DJI, GoPro and Insta360 files are stabilized without a project")),
    }
}

// Without a lens profile the stabilization is worse than none. Newer GoPros and some Insta360s record their lens in
// the file; the rest are looked up in Gyroflow's database the way Gyroflow does when it opens the video
fn has_lens_profile(input: &Input, samples: &[SampleInfo], width: usize, height: usize, fps: f64) -> bool {
    let builtin = samples.iter().filter_map(|s| s.tag_map.as_ref()?.get(&GroupId::Lens))
        .any(|m| m.contains_key(&TagId::Data) || m.contains_key(&TagId::Name));
    if builtin { return true; }
    let Ok(id) = CameraIdentifier::from_telemetry_parser(input, width, height, fps) else { return false };
    let id = id.get_identifier_for_autoload();
    if id.is_empty() { return false; }
    let mut db = LensProfileDatabase::default();
    db.load_all();
    db.contains_id(&id)
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
    let (mut file, size) = embedded::Content::new(&mut file, size);
    let mut mode = None;
    let cancel = Arc::new(AtomicBool::new(false));
    let cancel2 = cancel.clone();
    let _ = telemetry_parser::util::get_metadata_track_samples(&mut file, size, true, |_info, data: &[u8], _pos, _md| {
        if cancel2.load(Ordering::Relaxed) { return; }
        cancel2.store(true, Ordering::Relaxed);
        // ProductMeta { clip_meta = 1 { clip_meta_header = 1 { proto_file_name = 1 } }, stream_meta = 2 }
        let Some(header) = pb_bytes(data, 1).and_then(|c| pb_bytes(c, 1)) else { return };
        let Some(proto) = pb_bytes(header, 1).and_then(|s| std::str::from_utf8(s).ok()) else { return };
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
fn pb_bytes(msg: &[u8], field: u64) -> Option<&[u8]> {
    match pb_field(msg, field)? { PbValue::Bytes(b) => Some(b), _ => None }
}
fn pb_varint(msg: &[u8], field: u64) -> Option<u64> {
    match pb_field(msg, field)? { PbValue::Varint(v) => Some(v), _ => None }
}
