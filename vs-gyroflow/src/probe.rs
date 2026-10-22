// SPDX-License-Identifier: GPL-3.0-or-later
//
// What a video can be stabilized with and the color profile it was recorded in: for gyroflow-probe (and through it
// the mpv scripts) and gyroflow-export, so both decide the same way.

use std::sync::{ Arc, atomic::{ AtomicBool, Ordering } };
use gyroflow_core::{ filesystem, util };
use gyroflow_core::telemetry_parser::{ self, Input, tags_impl::* };

use crate::embedded;

pub fn probe(path: &str) -> Result<serde_json::Value, String> {
    let url = filesystem::path_to_url(path);
    let mut file = std::fs::File::open(path).map_err(|e| e.to_string())?;
    let size = file.metadata().map_err(|e| e.to_string())?.len() as usize;

    if let Some(project) = embedded::read(std::path::Path::new(path)).map_err(|e| e.to_string())? {
        let info = &project["video_info"];
        return Ok(serde_json::json!({ "mode": "embedded", "width": info["width"], "height": info["height"] }));
    }

    let (mut file, size) = embedded::Content::new(&mut file, size);
    let video = util::get_video_metadata(&mut file, size, &url).map_err(|e| e.to_string())?;
    let input = Input::from_stream(&mut file, size, path, |_| (), Arc::new(AtomicBool::new(false))).map_err(|e| e.to_string())?;
    let camera = input.camera_type();
    if camera != "DJI" {
        return Err(format!("{camera}: only DJI files are stabilized without a project"));
    }

    let mut motion_samples = 0;
    let mut eis_status = None;
    for s in input.samples.as_ref().into_iter().flatten() {
        let Some(m) = s.tag_map.as_ref() else { continue };
        if let Some(TagValue::Vec_TimeQuaternion_f64(v)) = m.get(&GroupId::Quaternion).and_then(|g| g.get(&TagId::Data)).map(|t| &t.value) { motion_samples += v.get().len(); }
        if let Some(TagValue::Vec_TimeVector3_f64(v)) = m.get(&GroupId::Gyroscope).and_then(|g| g.get(&TagId::Data)).map(|t| &t.value) { motion_samples += v.get().len(); }
        if eis_status.is_none() {
            if let Some(TagValue::Json(v)) = m.get(&GroupId::Default).and_then(|g| g.get(&TagId::Metadata)).map(|t| &t.value) {
                eis_status = find_key(v.get(), "eis_status").and_then(|x| x["status"].as_i64());
            }
        }
    }
    if motion_samples == 0 {
        return Err("no motion data (DJI records it only with EIS off and the Wide FOV)".into());
    }
    // EIS_OFF. Unknown (other models) counts as on: stabilizing footage the camera already stabilized makes it worse
    match eis_status {
        Some(0) => { },
        Some(s) => return Err(format!("in-camera stabilization is on (EIS status {s})")),
        None => return Err("in-camera stabilization state unknown".into()),
    }
    Ok(serde_json::json!({ "mode": "default", "width": video.width, "height": video.height }))
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
// Action 6 (its schema for dvtm_ac206 has no ColorMode), so it's read from the protobuf here.
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
