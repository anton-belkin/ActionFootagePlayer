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
// DJI file, or a model whose metadata layout isn't known here), "water": whether the camera records when it's in
// the water, for the lens under water to follow it (water.rs), "rotation": how much a player turns the video to
// show it upright, clockwise, 0, 90, 180 or 270 (its display matrix, mp4meta.rs: a camera held vertical), which the
// plugin's output already has, and "spherical": what a stitched 360° video says it is (its projection, stereo
// layout, pose or initial view: probe::spherical), null for any other video. A 360 camera's own file (its lenses in
// their tracks, rig.rs) also has "rig360": {"brand", "model", "lenses", "tracks", "width", "height"} (a track's size),
// or {"error": why the player can't render it}; the plugin's 360 mode plays it.
// `gyroflow-probe --rig <video>` prints the rig whole, as telemetry-parser reads it; `gyroflow-probe --headings <video>`
// what a 360 camera's file has for the view's heading modes: {"travel": {"available", "confidence", "why", ...},
// "north": {...}, "world": {"held_by"}, "seconds": [[travel, north confidence], ...]} (sphere360::Motion's report).
// A .gyroflow next to the video is the Lua script's own check, before this one.
//
// `gyroflow-probe --lens <video>`: the camera and the lens profile Gyroflow takes for it, for the side panel's Source
// tab, read once the clip plays: {"camera":"DJI OsmoAction6","lens":"in the file"}, "lens" a profile's name from
// Gyroflow's database, or "" when it has none (Unknown: pick one in Gyroflow).
//
// `gyroflow-probe --library <video>`: all of the above for Action Footage Library's index, in one line, plus "camera"
// (as --lens), "movie" {duration, created (unix s), width, height, turn}, "marks" {camera, times}: the highlights
// the camera marked (markers.rs, as the highlights plugin reads them), or null, "clock" {utc, basis, zone_min, why}:
// the start in UTC as far as the camera says, and "gps" {lat, lon, alt, t}: the camera's own first fix, or null
// (geotag.rs). Kept per file as the plain answer is.
//
// `gyroflow-probe --reframe <video>`: the view recorded in the player as you looked around (the clip's .afp.json,
// view360.recording), tidied as its "tidy" says (reframe.rs): {"follows", "parts", "tidy", "hold"} as the player keeps a
// recording, for the view360 plugin to play while it shows the recording, "hold" false (not asked for), "applied" or
// "needed" (to be measured: gyroflow-export --reframe recording --measure-hold); {"error": why} when there's none.

use vs_gyroflow::{ probe::{ probe, color_profile, camera_and_lens, spherical }, water };

fn video_size(path: &str) -> Option<(usize, usize, f64)> {
    let mut file = std::fs::File::open(path).ok()?;
    let size = file.metadata().ok()?.len() as usize;
    let m = gyroflow_core::util::get_video_metadata(&mut file, size, &gyroflow_core::filesystem::path_to_url(path)).ok()?;
    Some((m.width, m.height, m.fps))
}

fn main() {
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    // --rig: the 360 rig as telemetry-parser reads it, whole (CameraRig), for looking at
    let dump_rig = args.first().is_some_and(|a| a == "--rig");
    if dump_rig { args.remove(0); }
    // --headings: what a 360 camera's file has for the view's heading modes (along the way, north), for the view360
    // plugin's panel: the same as the plugin's 360 mode finds (sphere360::Motion)
    let headings = args.first().is_some_and(|a| a == "--headings");
    if headings { args.remove(0); }
    // --lens: the camera and the lens profile Gyroflow takes for it, for the side panel's Source tab
    let lens = args.first().is_some_and(|a| a == "--lens");
    if lens { args.remove(0); }
    // --library: everything Action Footage Library's index keeps about a clip
    let library = args.first().is_some_and(|a| a == "--library");
    if library { args.remove(0); }
    // --reframe: the recorded view, tidied, for the player's preview of the flat export
    let reframe = args.first().is_some_and(|a| a == "--reframe");
    if reframe { args.remove(0); }
    let Some(path) = args.into_iter().next() else {
        eprintln!("usage: gyroflow-probe [--rig | --headings | --lens | --library | --reframe] <video>");
        std::process::exit(2);
    };
    if reframe {
        // The hold's measuring is kept under the clip's full path, as gyroflow-export keeps it
        let path = vs_gyroflow::canonicalize(&path).map(|p| p.to_string_lossy().to_string()).unwrap_or(path);
        let out = match vs_gyroflow::reframe::Recording::read(std::path::Path::new(&path)) {
            Ok(r) => {
                let (t, needed) = r.tidied_for(&path);
                let mut j = t.to_json();
                j["hold"] = if !r.tidy.picture { false.into() } else if needed { "needed".into() } else { "applied".into() };
                j
            }
            Err(e) => serde_json::json!({ "error": e }),
        };
        println!("{out}");
        return;
    }
    if lens {
        // Gyroflow's readers take paths as URLs: a relative one reads nothing
        let path = vs_gyroflow::canonicalize(&path).map(|p| p.to_string_lossy().to_string()).unwrap_or(path);
        let (camera, lens) = camera_and_lens(&path).unwrap_or_default();
        println!("{}", serde_json::json!({ "camera": camera, "lens": lens }));
        return;
    }
    if headings {
        let report = match vs_gyroflow::rig::read(&path) {
            Some(Ok(rig)) => match vs_gyroflow::sphere360::Motion::load(&path, &rig) {
                Ok(m) => m.report,
                Err(e) => serde_json::json!({ "error": e }),
            },
            Some(Err(e)) => serde_json::json!({ "error": e }),
            None => serde_json::json!({ "error": "not a 360 camera's file" }),
        };
        println!("{report}");
        return;
    }
    if dump_rig {
        println!("{}", serde_json::to_string_pretty(&vs_gyroflow::rig::camera_rig(&path)).unwrap_or_default());
        return;
    }
    let path = match vs_gyroflow::canonicalize(&path) {
        Ok(p) => p.to_string_lossy().to_string(),
        Err(e) => { println!("{}", serde_json::json!({ "mode": null, "reason": e.to_string(), "color": null })); return; }
    };
    // Kept per file (and per build of this tool): opening a clip waits for this answer, and reading the file for it
    // takes ~0.4 s cold (from a NAS more)
    let build = std::env::current_exe().ok().and_then(|e| std::fs::metadata(e).ok()).and_then(|m| m.modified().ok()).map(|t| format!("{t:?}")).unwrap_or_default();
    if library {
        let cache = vs_gyroflow::file_cache("library", &path, &build);
        if let Some(text) = cache.as_ref().and_then(|c| std::fs::read_to_string(c).ok()).filter(|t| t.starts_with('{')) {
            println!("{}", text.trim());
            return;
        }
        let mut out = plain(&path, &build);
        if out["camera"].is_null() {
            out["camera"] = camera_and_lens(&path).map(|(c, _)| c).unwrap_or_default().into();
        }
        out["movie"] = serde_json::to_value(vs_gyroflow::markers::movie(&path)).unwrap_or_default();
        // When it was recorded, in UTC as far as the camera says, and where, if the camera knew (geotag.rs)
        let created = out["movie"]["created"].as_i64();
        let (clock, gps) = vs_gyroflow::geotag::read(std::path::Path::new(&path), out["camera"].as_str().unwrap_or(""), created);
        out["clock"] = serde_json::to_value(clock).unwrap_or_default();
        out["gps"] = serde_json::to_value(gps).unwrap_or_default();
        out["marks"] = vs_gyroflow::markers::highlights(&path)
            .map(|(camera, times)| serde_json::json!({ "camera": camera, "times": times })).unwrap_or_default();
        println!("{out}");
        keep(cache, &out);
        return;
    }
    println!("{}", plain(&path, &build));
}

fn keep(cache: Option<std::path::PathBuf>, out: &serde_json::Value) {
    if let Some(c) = cache {
        let tmp = c.with_extension("tmp");
        if c.parent().map(|d| std::fs::create_dir_all(d).is_ok()).unwrap_or(false) && std::fs::write(&tmp, out.to_string()).is_ok() { let _ = std::fs::rename(&tmp, &c); }
    }
}

// The plain answer (no option), from its cache or worked out and kept
fn plain(path: &str, build: &str) -> serde_json::Value {
    let path = path.to_string();
    let cache = vs_gyroflow::file_cache("probe", &path, build);
    if let Some(v) = cache.as_ref().and_then(|c| std::fs::read_to_string(c).ok()).and_then(|t| serde_json::from_str::<serde_json::Value>(&t).ok()) {
        return v;
    }
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
    out["spherical"] = spherical(&path).into();
    // A 360 camera's own file (its lenses in their tracks): what the plugin's 360 mode renders, or why it can't
    if let Some(rig) = vs_gyroflow::rig::read(&path) {
        out["rig360"] = match rig {
            Ok(r) => serde_json::json!({ "brand": r.brand, "model": r.model, "lenses": r.lenses.len(), "tracks": r.pictures,
                                         "width": r.picture.0, "height": r.picture.1 }),
            Err(e) => serde_json::json!({ "error": e }),
        };
    }
    keep(cache, &out);
    out
}
