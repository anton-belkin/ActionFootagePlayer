// SPDX-License-Identifier: GPL-3.0-or-later
//
// `gyroflow-probe <video>`: what the plugin can stabilize the video with, for the gyroflow mpv plugin to decide
// whether to add the filter, and the color profile the camera recorded it in (as the dlogm plugin reads it), for
// Steady Router. Prints one JSON line:
//   {"mode":"default","width":3840,"height":2160,...}   no project, but motion data of its own, recorded without
//                                                       in-camera stabilization (DJI with EIS off, GoPro with
//                                                       HyperSmooth off, Insta360 with FlowState off in the camera),
//                                                       or of the picture the camera stabilized (GoPro HERO12
//                                                       and later with HyperSmooth on: "stabilized_in_camera" says so,
//                                                       in_camera.rs): stabilize it with the default preset
//   {"mode":"default","motion":"optical","measured":{how, at, lens, in_water, readout_ms, frames, measured_frames,
//    readout_left_out},...}  (readout_left_out: the lens's readout, ms, when it was measured without it: measure again)
//                                                       motion measured from the picture beside it (<stem>.afp-motion,
//                                                       afp_motion.rs): stabilized with it, no horizon lock;
//                                                       "measured_error" instead when that file is broken or of another clip;
//                                                       a clip with motion data of its own the player uses plays with
//                                                       that, and says "measured_aside": true when it has such a file
//   {"mode":null,"reason":"...","lens":"...",...}       nothing to stabilize it with; "lens": the lens profile the
//                                                       player takes for it ("in the file", the label of the one picked
//                                                       from Gyroflow's database, lens_pick.rs, "" for none), to correct
//                                                       the lens alone, and "camera"
// and in each, "color": "dlogm", "normal", "hlg", ... from the camera's metadata, or null when it isn't known (not a
// DJI file, or a model whose metadata layout isn't known here), "water": whether the camera records when it's in
// the water, for the lens under water to follow it (water.rs), "stabilized_in_camera": how the camera stabilized the
// picture itself ("HyperSmooth (HS EIS)", "EIS: RockSteady", "FlowState"), false when it recorded that it didn't, null
// when it doesn't say (such a clip isn't stabilized again here, whatever motion data it has, except a GoPro that
// recorded how HyperSmooth turned the picture: mode "default" then), "rotation": how much a player turns the video to
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
// `gyroflow-probe --lens <video>`: the camera and the lens profile the player takes for it, for the side panel's Source
// tab, read once the clip plays, and the Library's lens type choice: {"camera":"DJI OsmoAction6","lens":"in the file",
// "types":[{"id":"wide","label":"Wide","profile":null},...],"type":null,"recorded":"wide"}. "lens": the label of the
// profile loaded (the database's pick with its name and stars, "Nearest: " when it isn't made for exactly this mode, or
// "<Type>, from the curated list: <name>": lens_pick.rs), or "" when it has none (Unknown: pick one in Gyroflow);
// "types": the lens types the curated list has for the camera (resources/curated_lenses.json, empty for most);
// "type": the one given to the clip by hand (source.lens_type in its .afp.json), "recorded": the one the camera wrote,
// "default": the one the list gives the camera's clips that have neither (a drone's),
// "mode": the camera's own number for it (DJI's fov_type; null for cameras without lens types), which the side panel
// shows while the list doesn't name it: "Unknown (mode 6)"; "curated_file": the user's own list laid over the built-in
// one (lens_pick::user_file), when there is one, and "curated_error" when it couldn't be read (it's left out then);
// "project", with a Gyroflow project beside the clip: the lens it uses ("lens", "key" of Gyroflow's database or null)
// and where the user's list would keep it ("camera", "type", "type_label", "size", "new_type"), "saved" when it does
// already, or "error" when it can't (probe::project_lens_answer).
//
// --lens also has "from" (where the lens comes from: "file", "own" the player's own database, "list" the curated
// list, "database" Gyroflow's made for exactly the clip, "" no ready lens), "mode_key" (the clip's mode as the player's
// own database keeps lenses) and "candidates": the lenses of its camera to choose from, nearest first ({"id", "label",
// "local", "exact", "readout"}, lens_pick::candidates).
//
// `gyroflow-probe --assign-lens <video> <id>`: one of those kept as the lens of the clip's whole mode in the player's
// own database (lens_db.rs): {"file", "mode", "lens"}, or {"error"} (exit 1). `gyroflow-probe --forget-lens <video>`:
// the lens assigned to the clip's mode taken away: {"removed", "mode"}.
//
// `gyroflow-probe --save-lens <video>`: that lens kept in the user's list as the default for the clip's camera, mode
// and size (lens_pick::save_default): {"camera", "type", "type_label", "size", "new_type", "lens", "profile", "file"},
// or {"error"} (exit 1).
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

use vs_gyroflow::{ probe::{ probe_with_camera_stabilization, color_profile, camera_and_lens, spherical }, water };

fn video_size(path: &str) -> Option<(usize, usize, f64)> {
    let mut file = std::fs::File::open(path).ok()?;
    let size = file.metadata().ok()?.len();
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
    // --save-lens: the lens of the Gyroflow project beside the clip kept in the user's lens list, for its camera, mode
    // and size (the Library's "Save as the default")
    let save_lens = args.first().is_some_and(|a| a == "--save-lens");
    if save_lens { args.remove(0); }
    // --assign-lens <video> <id>: a lens of --lens's "candidates" kept as the lens of the clip's whole mode;
    // --forget-lens <video>: the one assigned to it taken away
    let assign = args.first().is_some_and(|a| a == "--assign-lens");
    if assign { args.remove(0); }
    let forget = args.first().is_some_and(|a| a == "--forget-lens");
    if forget { args.remove(0); }
    // --library: everything Action Footage Library's index keeps about a clip
    let library = args.first().is_some_and(|a| a == "--library");
    if library { args.remove(0); }
    // --reframe: the recorded view, tidied, for the player's preview of the flat export
    let reframe = args.first().is_some_and(|a| a == "--reframe");
    if reframe { args.remove(0); }
    let id = args.get(1).cloned();
    let Some(path) = args.into_iter().next() else {
        eprintln!("usage: gyroflow-probe [--rig | --headings | --lens | --save-lens | --library | --reframe | --forget-lens] <video>\n       gyroflow-probe --assign-lens <video> <id>");
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
        println!("{}", serde_json::to_value(vs_gyroflow::probe::lens_info_with_candidates(&path, 16).unwrap_or_default()).unwrap_or_default());
        return;
    }
    if assign || forget {
        let path = vs_gyroflow::canonicalize(&path).map(|p| p.to_string_lossy().to_string()).unwrap_or(path);
        let out = if forget { vs_gyroflow::probe::forget_lens(&path) } else {
            match id { Some(id) => vs_gyroflow::probe::assign_lens(&path, &id), None => Err("which lens: --assign-lens <video> <id>".into()) }
        }.unwrap_or_else(|e| serde_json::json!({ "error": e }));
        println!("{out}");
        std::process::exit(if out["error"].is_null() { 0 } else { 1 });
    }
    if save_lens {
        let path = vs_gyroflow::canonicalize(&path).map(|p| p.to_string_lossy().to_string()).unwrap_or(path);
        let out = vs_gyroflow::probe::save_project_lens(&path).unwrap_or_else(|e| serde_json::json!({ "error": e }));
        println!("{out}");
        std::process::exit(if out["error"].is_null() { 0 } else { 1 });
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
    // The lens type given to the clip by hand decides its lens profile: an answer kept for another isn't this one
    // and so does the user's own curated list (lens_pick::user_file), as it is now
    // and the motion measured from the picture beside it (afp_motion.rs), as it is now
    // and the lens given to the clip alone (lens_pick::clip_lens)
    let build = format!("{build}|{}|{}|{}", vs_gyroflow::lens_pick::lens_state_stamp(&path), vs_gyroflow::lens_pick::user_file_stamp(),
                        vs_gyroflow::afp_motion::stamp(std::path::Path::new(&path)));
    if library {
        // Its answer has the lens of the Gyroflow project beside it too: kept while that project is as it is
        let project = vs_gyroflow::lens_pick::project_beside(&path).and_then(|f| std::fs::metadata(f).ok())
            .map(|m| format!("{}:{:?}", m.len(), m.modified().ok())).unwrap_or_default();
        let cache = vs_gyroflow::file_cache("library", &path, &format!("{build}|{project}"));
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
        // What this answer knows: 2 = drones' names and places too, 3 = clocks set before 2017 believed, 4 = an older
        // GoPro (HERO3) named by its firmware, 5 = a lens only when one is made for exactly the clip's mode (lens_pick.rs),
        // 6 = the lens as the player takes it ("lens_info", below), 7 = GoPro HERO12+ with HyperSmooth on stabilized (in_camera.rs),
        // 8 = the clip's own motion data over its .afp-motion ("measured_aside"), 9 = "builtin" lenses and
        // measured.readout_left_out
        // (the Library asks again for older answers: raise it with what's new here)
        out["geotag"] = 9.into();
        // The lens as the player takes it (probe::lens_info: the profile and where it comes from, the mode a lens is kept
        // for, the camera's lens types, the Gyroflow project's lens), for the Library's Source Settings and its filters
        // without reading the clip again; the lenses to choose from (candidates) are the mode's, asked when chosen
        out["lens_info"] = serde_json::to_value(vs_gyroflow::probe::lens_info(&path)).unwrap_or_default();
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
    let (answer, in_camera) = probe_with_camera_stabilization(&path);
    let mut out = match answer {
        Ok(v) => v,
        Err(reason) => serde_json::json!({ "mode": null, "reason": reason }),
    };
    // Motion measured from the picture beside it (afp_motion.rs): stabilized with it as with motion data of its own
    // ("motion": "optical"; the horizon can't be locked) where the clip has none the player uses. Where it has (its
    // gyro, or a HERO12+ with HyperSmooth on), that plays, and the measured motion is left aside ("measured_aside")
    let own = !out["mode"].is_null();
    match vs_gyroflow::afp_motion::header_for(std::path::Path::new(&path)) {
        Some(_) if own => { out["measured_aside"] = true.into(); }
        Some(Ok(h)) => {
            let (w, hh, fps) = video_size(&path).unwrap_or((h.clip.width, h.clip.height, h.clip.fps));
            out["mode"] = "default".into();
            out["motion"] = "optical".into();
            out["width"] = w.into(); out["height"] = hh.into(); out["fps"] = fps.into();
            out["measured"] = serde_json::json!({ "how": h.measured.how, "at": h.measured.at, "lens": h.lens_name, "in_water": h.in_water(),
                                                  "readout_ms": h.readout_ms, "frames": h.measured.frames, "measured_frames": h.measured.measured_frames,
                                                  "readout_left_out": h.readout_left_out() });
            if let Some(o) = out.as_object_mut() { o.remove("reason"); }
        }
        Some(Err(e)) => { out["measured_error"] = e.into(); }
        None => {}
    }
    out["stabilized_in_camera"] = match in_camera {
        Some(how) if how.is_empty() => false.into(),
        Some(how) => how.into(),
        None => serde_json::Value::Null,
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
        // A file telemetry-parser couldn't read, of a camera told from the file (a HERO3, an old DJI drone): it
        // recorded no motion data, which says more than the parser's "Unsupported file format"
        if out["reason"].as_str() == Some("Unsupported file format") && !camera.is_empty() {
            out["reason"] = format!("{camera}: no motion data").into();
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
