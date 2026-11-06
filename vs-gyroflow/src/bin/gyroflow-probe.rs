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
// DJI file, or a model whose metadata layout isn't known here), "water": whether the camera records when it's in
// the water, for the lens under water to follow it (water.rs), "rotation": how much a player turns the video to
// show it upright, clockwise, 0, 90, 180 or 270 (its display matrix, mp4meta.rs: a camera held vertical), which the
// plugin's output already has, and "spherical": what a stitched 360° video says it is (its projection, stereo
// layout, pose or initial view: probe::spherical), null for any other video. A 360 camera's own file (its lenses in
// their tracks, rig.rs) also has "rig360": {"brand", "model", "lenses", "tracks", "width", "height"} (a track's size),
// or {"error": why the player can't render it}; the plugin's 360 mode plays it.
// `gyroflow-probe --rig <video>` prints the rig whole, as telemetry-parser reads it.
// A .gyroflow next to the video is the Lua script's own check, before this one.

use vs_gyroflow::{ probe::{ probe, color_profile, spherical }, water };

fn main() {
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    // --rig: the 360 rig as telemetry-parser reads it, whole (CameraRig), for looking at
    let dump_rig = args.first().is_some_and(|a| a == "--rig");
    if dump_rig { args.remove(0); }
    let Some(path) = args.into_iter().next() else {
        eprintln!("usage: gyroflow-probe [--rig] <video>");
        std::process::exit(2);
    };
    if dump_rig {
        println!("{}", serde_json::to_string_pretty(&vs_gyroflow::rig::camera_rig(&path)).unwrap_or_default());
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
    out["spherical"] = spherical(&path).into();
    // A 360 camera's own file (its lenses in their tracks): what the plugin's 360 mode renders, or why it can't
    if let Some(rig) = vs_gyroflow::rig::read(&path) {
        out["rig360"] = match rig {
            Ok(r) => serde_json::json!({ "brand": r.brand, "model": r.model, "lenses": r.lenses.len(), "tracks": r.pictures,
                                         "width": r.picture.0, "height": r.picture.1 }),
            Err(e) => serde_json::json!({ "error": e }),
        };
    }
    println!("{out}");
}
