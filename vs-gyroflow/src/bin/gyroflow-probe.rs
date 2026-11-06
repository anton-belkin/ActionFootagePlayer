// SPDX-License-Identifier: GPL-3.0-or-later
//
// `gyroflow-probe <video>`: what the plugin can stabilize the video with, for the gyroflow mpv plugin to decide
// whether to add the filter, and the color profile the camera recorded it in (as the dlogm plugin reads it), for
// Steady Router. Prints one JSON line:
//   {"mode":"default","width":3840,"height":2160,...}   no project, but motion data of its own, recorded without
//                                                       in-camera stabilization (DJI with EIS off, GoPro with
//                                                       HyperSmooth off, Insta360 with FlowState off in the camera):
//                                                       stabilize it with the default preset
//   {"mode":null,"reason":"...",...}                    nothing to stabilize it with
// and in each, "color": "dlogm", "normal", "hlg", ... from the camera's metadata, or null when it isn't known (not a
// DJI file, or a model whose metadata layout isn't known here), and "water": whether the camera records when it's in
// the water, for the lens under water to follow it (water.rs), and "rotation": how much a player turns the video to
// show it upright, clockwise, 0, 90, 180 or 270 (its display matrix, mp4meta.rs: a camera held vertical), which the
// plugin's output already has.
// A .gyroflow next to the video is the Lua script's own check, before this one.
//
// `gyroflow-probe --lens <video>`: the camera and the lens profile Gyroflow takes for it, for the side panel's Source
// tab, read once the clip plays: {"camera":"DJI OsmoAction6","lens":"in the file"}, "lens" a profile's name from
// Gyroflow's database, or "" when it has none (Unknown: pick one in Gyroflow).

use vs_gyroflow::{ probe::{ probe, color_profile, camera_and_lens }, water };

fn main() {
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    let lens = args.first().map(|a| a == "--lens").unwrap_or(false);
    if lens { args.remove(0); }
    let Some(path) = args.into_iter().next() else {
        eprintln!("usage: gyroflow-probe [--lens] <video>");
        std::process::exit(2);
    };
    if lens {
        let (camera, lens) = camera_and_lens(&path).unwrap_or_default();
        println!("{}", serde_json::json!({ "camera": camera, "lens": lens }));
        return;
    }
    let path = match vs_gyroflow::canonicalize(&path) {
        Ok(p) => p.to_string_lossy().to_string(),
        Err(e) => { println!("{}", serde_json::json!({ "mode": null, "reason": e.to_string(), "color": null })); return; }
    };
    let mut out = match probe(&path) {
        Ok(v) => v,
        Err(reason) => serde_json::json!({ "mode": null, "reason": reason }),
    };
    out["color"] = color_profile(&path).into();
    out["water"] = water::recorded(&path).into();
    out["rotation"] = vs_gyroflow::mp4meta::display_rotation(std::path::Path::new(&path)).unwrap_or(0).into();
    println!("{out}");
}
