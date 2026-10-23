// SPDX-License-Identifier: GPL-3.0-or-later
//
// What a video can be stabilized with and the color profile it was recorded in: for gyroflow-probe (and through it
// the mpv scripts) and gyroflow-export, so both decide the same way.

use std::io::{ Read, Seek, SeekFrom };
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
    let (mut file, size) = embedded::Content::new(&mut file, size);
    let moov = read_moov(&mut file, size as u64)?;
    let udta = mp4_child(&moov, b"udta")?;
    let gpmf = mp4_child(udta, b"GPMF")?;
    let firmware = mp4_child(udta, b"FIRM").and_then(text).and_then(|f| firmware_version(&f));
    Some((text(klv(gpmf, b"PTCL")?)?, klv(gpmf, b"MINF").and_then(text), firmware))
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
    let mut pos = 0usize;
    while pos + 8 <= s.len() {
        let (size, header) = match u32::from_be_bytes(s[pos..pos + 4].try_into().ok()?) {
            1 => (usize::try_from(u64::from_be_bytes(s.get(pos + 8..pos + 16)?.try_into().ok()?)).ok()?, 16),
            0 => (s.len() - pos, 8),
            n => (n as usize, 8),
        };
        let end = pos.checked_add(size).filter(|&e| size >= header && e <= s.len())?;
        if &s[pos + 4..pos + 8] == wanted { return Some(&s[pos + header..end]); }
        pos = end;
    }
    None
}

// The data of the first KLV item with the key, looking into nested ones (type 0). An item: the key (4 bytes), its
// type (1), the size of one sample (1) and how many (2, big-endian), then the data, padded to 4 bytes
fn klv<'a>(s: &'a [u8], wanted: &[u8; 4]) -> Option<&'a [u8]> {
    let mut pos = 0usize;
    while pos + 8 <= s.len() {
        let n = s[pos + 5] as usize * u16::from_be_bytes([s[pos + 6], s[pos + 7]]) as usize;
        let data = s.get(pos + 8..pos + 8 + n)?;
        if &s[pos..pos + 4] == wanted { return Some(data); }
        if s[pos + 4] == 0 {
            if let Some(v) = klv(data, wanted) { return Some(v); }
        }
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
}
