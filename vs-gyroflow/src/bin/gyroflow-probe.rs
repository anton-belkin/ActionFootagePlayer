// SPDX-License-Identifier: GPL-3.0-or-later
//
// `gyroflow-probe <video>`: what the plugin can stabilize the video with, for gyroflow_auto.lua to decide whether to
// add the filter, and the color profile the camera recorded it in, for dlog_auto.lua. Prints one JSON line:
//   {"mode":"embedded","width":3840,"height":2160,"fps":59.94,...}  a project stored in the video (by gyroflow-embed)
//   {"mode":"default","width":3840,"height":2160,...}   no project, but motion data of its own, recorded without
//                                                       in-camera stabilization (DJI with EIS off): stabilize it with
//                                                       the default preset
//   {"mode":null,"reason":"...",...}                    nothing to stabilize it with
// and in each, "color": "dlogm", "normal", "hlg", ... from the camera's metadata, or null when it isn't known (not a
// DJI file, or a model whose metadata layout isn't known here).
// A .gyroflow next to the video is the Lua script's own check, before this one.

use vs_gyroflow::probe::{ probe, color_profile };

fn main() {
    let Some(path) = std::env::args().nth(1) else {
        eprintln!("usage: gyroflow-probe <video>");
        std::process::exit(2);
    };
    let path = match std::fs::canonicalize(&path) {
        Ok(p) => p.to_string_lossy().to_string(),
        Err(e) => { println!("{}", serde_json::json!({ "mode": null, "reason": e.to_string(), "color": null })); return; }
    };
    let mut out = match probe(&path) {
        Ok(v) => v,
        Err(reason) => serde_json::json!({ "mode": null, "reason": reason }),
    };
    out["color"] = color_profile(&path).into();
    println!("{out}");
}
