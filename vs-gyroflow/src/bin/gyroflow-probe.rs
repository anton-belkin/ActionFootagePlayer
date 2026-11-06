// SPDX-License-Identifier: GPL-3.0-or-later
//
// `gyroflow-probe <video>`: what the plugin can stabilize the video with, for the gyroflow mpv plugin to decide
// whether to add the filter, and the color profile the camera recorded it in (as the dlogm plugin reads it), for
// Steady Router. Prints one JSON line:
//   {"mode":"default","width":3840,"height":2160,...}   no project, but motion data of its own, recorded without
//                                                       in-camera stabilization (DJI with EIS off, GoPro with
//                                                       HyperSmooth off, Insta360 with FlowState off in the camera):
//                                                       stabilize it with the default preset
//   {"mode":null,"reason":"...","lens":"...",...}       nothing to stabilize it with; "lens": the lens profile Gyroflow
//                                                       takes for it ("in the file", a profile's name, "" for none),
//                                                       to correct the lens alone, and "camera"
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

fn video_size(path: &str) -> Option<(usize, usize, f64)> {
    let mut file = std::fs::File::open(path).ok()?;
    let size = file.metadata().ok()?.len() as usize;
    let m = gyroflow_core::util::get_video_metadata(&mut file, size, &gyroflow_core::filesystem::path_to_url(path)).ok()?;
    Some((m.width, m.height, m.fps))
}

fn main() {
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    let lens = args.first().map(|a| a == "--lens").unwrap_or(false);
    if lens { args.remove(0); }
    let Some(path) = args.into_iter().next() else {
        eprintln!("usage: gyroflow-probe [--lens] <video>");
        std::process::exit(2);
    };
    if lens {
        // Gyroflow's readers take paths as URLs: a relative one reads nothing
        let path = vs_gyroflow::canonicalize(&path).map(|p| p.to_string_lossy().to_string()).unwrap_or(path);
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
    // Nothing to stabilize with: whether the lens can be corrected on its own ("lens": the profile, "" for none)
    if out["mode"].is_null() {
        let (camera, lens) = camera_and_lens(&path).unwrap_or_default();
        // Its size and rate, for the plugin to set the filter up as it does for a clip it stabilizes
        if !lens.is_empty() {
            if let Some((w, h, fps)) = video_size(&path) {
                out["width"] = w.into(); out["height"] = h.into(); out["fps"] = fps.into();
            }
        }
        out["camera"] = camera.into();
        out["lens"] = lens.into();
    }
    out["color"] = color_profile(&path).into();
    out["water"] = water::recorded(&path).into();
    out["rotation"] = vs_gyroflow::mp4meta::display_rotation(std::path::Path::new(&path)).unwrap_or(0).into();
    println!("{out}");
}
