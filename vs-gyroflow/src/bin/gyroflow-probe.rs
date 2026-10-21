// SPDX-License-Identifier: GPL-3.0-or-later
//
// `gyroflow-probe <video>`: what the plugin can stabilize the video with, for gyroflow_auto.lua to decide whether to
// add the filter. Prints one JSON line:
//   {"mode":"embedded","width":3840,"height":2160}  a project stored in the video (by gyroflow-embed)
//   {"mode":"default","width":3840,"height":2160}   no project, but motion data of its own, recorded without in-camera
//                                                   stabilization (DJI with EIS off): stabilize it with the default preset
//   {"mode":null,"reason":"..."}                    nothing to do
// A .gyroflow next to the video is the Lua script's own check, before this one.

use std::sync::{ Arc, atomic::AtomicBool };
use gyroflow_core::{ filesystem, util };

#[path = "../embedded.rs"]
#[allow(dead_code)]
mod embedded;
use gyroflow_core::telemetry_parser::{ Input, tags_impl::* };

fn main() {
    let Some(path) = std::env::args().nth(1) else {
        eprintln!("usage: gyroflow-probe <video>");
        std::process::exit(2);
    };
    let out = match probe(&path) {
        Ok(v) => v,
        Err(reason) => serde_json::json!({ "mode": null, "reason": reason }),
    };
    println!("{out}");
}

fn probe(path: &str) -> Result<serde_json::Value, String> {
    let path = std::fs::canonicalize(path).map_err(|e| e.to_string())?.to_string_lossy().to_string();
    let path = path.as_str();
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
