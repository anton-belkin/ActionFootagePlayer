// SPDX-License-Identifier: GPL-3.0-or-later
//
// The GPU warp's shader for a clip (docs/gpu-render.md, step 1: the spike): shaders/gyroflow/warp.glsl with the clip's
// table (src/gpu_table.rs) in it, for mpv's glsl-shaders. mpv then warps each frame on the GPU, finding it by its PTS.
//
//   gyroflow-gpu-table [options] <project.gyroflow | video> <out.glsl>
//     --mode <m>         exact: one pass, the geometry at every pixel; grid (default): the geometry on a coarse grid,
//                        one sampling pass at MAIN; planes: the grid, sampling passes at LUMA and CHROMA
//     --grid-step <px>   the grid's step in output pixels (default 16)
//     --chroma-bilinear  planes: the chroma bilinear whatever --interp says
//     --grid-linear      the grid interpolated bilinearly (4 reads a pixel, not 16; use a step of 8)
//     --knots <n>        quaternion knots per frame along its rows (default 33); 0: every row (Gyroflow's own, large)
//     --interp <i>       bilinear, bspline, bicubic, bicubic9 (the same from 9 bilinear fetches), lanczos4, ewa
//                        (default bicubic)
//     --shrink           widened taps where the warp shrinks the source (bicubic, lanczos4)
//     --frames <a>-<b>   only these frames in the table (the rest show the background)
//     --readout-ms <t>   the rolling shutter's readout time instead of the project's (to measure the knots on a clip
//                        that has none)
//
//   gyroflow-gpu-table --player <json> <out.glsl> [--server <socket>]
//     The player's GPU path (main.lua): its filter arguments in, the shader out, one JSON line printed (see player());
//     asked of the server at <socket> when one listens there
//
//   gyroflow-gpu-table --serve <socket>
//     The player's table server: keeps the clips it loaded, for changes that don't move the geometry (see serve())
//
//   gyroflow-gpu-table --check-grid <W>x<H> [--readout-ms <t>] <project.gyroflow | video>
//     No shader: the source coordinates on a coarse grid of a W x H output, interpolated (bilinear, Catmull-Rom),
//     against the exact ones at every pixel, for grid steps of 4 to 32 px, on the fastest-turning frames
//
//   gyroflow-gpu-table --check <n> [--readout-ms <t>] <project.gyroflow | video>
//     No shader: the n fastest-turning frames (and some through the clip) through Gyroflow's CPU kernel
//     (Stabilization::undistort_coord) with every row's matrix and with 9, 17, 33 and 65 knots: the largest difference
//     in source pixels over a grid of the output; and the time to compute a frame's transforms

use std::time::Instant;
use gyroflow_core::stabilization::Stabilization;
use nalgebra::Vector2;
use vs_gyroflow::gpu_table::{ self, Clip, Options, Table };

/// The player's GPU path (mpv/scripts/gyroflow/main.lua): `--player <json> <out.glsl> [--server <socket>]`, the JSON
/// main.lua hands the VapourSynth filter plus input_width/height, interpolation, chromatic (on/off), edges and
/// edges_blur. Writes the shader and prints one JSON line: { shader, output: [w, h], mode, chromatic, ms } or
/// { unsupported: why } or { error }. With a server (--serve) listening, it answers: the clip is loaded there already
/// for a change of the view, the edges, the color fringes, the denoise or the interpolation
fn player(json: &str, out: &str, server: Option<&str>) {
    let t = Instant::now();
    // Written before with these settings, for this clip as it is now: kept on disk (a clip opened again)
    let kept = serde_json::from_str::<serde_json::Value>(json).ok().and_then(|j| disk_cache(&j));
    if let Some((shader, reply)) = kept.as_ref().and_then(|(c, r)| std::fs::read_to_string(r).ok().map(|r| (c, r))) {
        if std::fs::copy(shader, out).is_ok() {
            if let Ok(mut r) = serde_json::from_str::<serde_json::Value>(&reply) {
                r["shader"] = out.into();
                r["ms"] = (t.elapsed().as_secs_f64() * 1000.0).round().into();
                println!("{r}");
                let now = std::fs::FileTimes::new().set_modified(std::time::SystemTime::now());
                let _ = std::fs::File::options().write(true).open(shader).and_then(|f| f.set_times(now));
                std::process::exit(0);
            }
        }
    }
    let keep = |reply: &str| if let Some((shader, r)) = kept.as_ref() {
        if !reply.contains("\"shader\"") { return; }
        if shader.parent().map(|d| std::fs::create_dir_all(d).is_ok()).unwrap_or(false) && std::fs::copy(out, shader).is_ok() {
            let _ = std::fs::write(r, reply);
            trim_disk_cache(shader.parent().unwrap());
        }
    };
    if let Some(sock) = server {
        if let Some(reply) = ask_server(sock, json, out) { keep(&reply); println!("{reply}"); std::process::exit(0); }
    }
    // AFP_TIMES: each step's time and gyroflow-core's log, on stderr
    let times = std::env::var_os("AFP_TIMES").is_some();
    if times { let _ = simplelog::TermLogger::init(log::LevelFilter::Debug, simplelog::Config::default(), simplelog::TerminalMode::Stderr, simplelog::ColorChoice::Never); }
    let reply = match serde_json::from_str::<serde_json::Value>(json) {
        Ok(j) => match gpu_table::PlayerClip::load(&j) {
            Ok(loaded) => {
                if times { eprintln!("  [{:7.1} ms] prepared", t.elapsed().as_secs_f64() * 1000.0); }
                player_shader(&loaded, &j, out, t)
            }
            Err(e) => serde_json::json!({ "error": e }),
        },
        Err(e) => serde_json::json!({ "error": format!("bad JSON: {e}") }),
    };
    keep(&reply.to_string());
    println!("{reply}");
    std::process::exit(0);
}

/// Where the shader for these settings is kept on disk, and its reply: by every setting but the per-player control
/// files' names, the project's (or video's) and preset's files as they are now, and this tool's build
fn disk_cache(j: &serde_json::Value) -> Option<(std::path::PathBuf, std::path::PathBuf)> {
    let mut j = j.clone();
    let obj = j.as_object_mut()?;
    for k in ["still_control", "edges_control", "workers", "lookahead"] { obj.remove(k); }
    let project = obj.get("project")?.as_str()?.to_string();
    let stamp = |p: &str| std::fs::metadata(p).ok().map(|m| format!("{}:{:?}", m.len(), m.modified().ok())).unwrap_or_default();
    let preset = obj.get("preset").and_then(|v| v.as_str()).map(stamp).unwrap_or_default();
    let build = std::env::current_exe().ok().map(|e| stamp(&e.to_string_lossy())).unwrap_or_default();
    // In key order: the player's JSON comes from a Lua table, in an order of its own each run
    let sorted: std::collections::BTreeMap<&String, &serde_json::Value> = obj.iter().collect();
    let base = vs_gyroflow::file_cache("gpu", &project, &format!("{}|{preset}|{build}", serde_json::to_string(&sorted).ok()?))?;
    Some((base.with_extension("glsl"), base.with_extension("json")))
}

/// The kept shaders under 400 MB (~100 of them), the least used going first
fn trim_disk_cache(dir: &std::path::Path) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    let mut files: Vec<(std::time::SystemTime, u64, std::path::PathBuf)> = entries.flatten()
        .filter(|e| e.path().extension().is_some_and(|x| x == "glsl"))
        .filter_map(|e| e.metadata().ok().map(|m| (m.modified().unwrap_or(std::time::UNIX_EPOCH), m.len(), e.path()))).collect();
    let mut total: u64 = files.iter().map(|f| f.1).sum();
    files.sort();
    for (_, len, path) in files {
        if total <= 400 << 20 { break; }
        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_file(path.with_extension("json"));
        total -= len;
    }
}

/// The shader for a loaded clip, with the JSON's view, edges, fringes, denoise and interpolation: the reply
fn player_shader(loaded: &gpu_table::PlayerClip, j: &serde_json::Value, out: &str, t: Instant) -> serde_json::Value {
    let lap = |what: &str| if std::env::var_os("AFP_TIMES").is_some() { eprintln!("  [{:7.1} ms] {what}", t.elapsed().as_secs_f64() * 1000.0) };
    let clip = loaded.clip(j);
    lap("the view's clip");
    let time_offset = j.get("time_offset").and_then(|v| v.as_f64()).unwrap_or(0.0);
    if let Some(why) = gpu_table::unsupported(&clip) { return serde_json::json!({ "unsupported": why }); }
    let s = |k: &str| j.get(k).and_then(|v| v.as_str()).unwrap_or("");
    let interp = match s("interpolation") { "bilinear" => 0, "lanczos" | "lanczos4" => 3, "ewa" => 4, "bspline" => 1, _ => 5 };
    let chromatic = j.get("gpu_chromatic").and_then(|v| v.as_bool()).unwrap_or(false);
    // Under water with the fringes taken out, the colors are read apart: the RGB layout. Else the planes
    let view = loaded.view(j);
    let wet = chromatic && loaded.wet(view, &clip);
    lap("under water?");
    let edge_fill = match s("edges") { "smooth" | "sphere" => Some(j.get("edges_blur").and_then(|v| v.as_f64()).unwrap_or(2.0) as f32 / 100.0), _ => None };
    // The sphere trail needs the RGB layout too (gpu_sphere: the player's say, where its GPU has the room)
    let sphere = s("edges") == "sphere" && j.get("gpu_sphere").and_then(|v| v.as_bool()).unwrap_or(true);
    let rgb = wet || sphere;
    let opts = Options { mode: if rgb { gpu_table::Mode::Grid } else { gpu_table::Mode::Planes }, grid_step: 8, grid_linear: true, chroma_bilinear: interp == 5,
                         chromatic: wet, chromatic_fast: true, edge_fill, sphere, time_offset,
                         denoise: j.get("gpu_denoise").and_then(|v| v.as_bool()).unwrap_or(false), knots: 33, interp, shrink: false, range: None };
    let table = match loaded.table(view, &clip, &opts) { Ok(t) => t, Err(e) => return serde_json::json!({ "error": e }) };
    lap("table");
    let text = gpu_table::shader(&clip, &opts, &table);
    lap("shader");
    if let Err(e) = std::fs::write(out, text) { return serde_json::json!({ "error": format!("{out}: {e}") }); }
    lap("written");
    // Crop mode's camera model (crop.lua): the output camera's focal length in output widths and heights, mid-clip
    let mid = (clip.frame_count / 2) as f64 * 1_000_000.0 / clip.fps;
    let (fx, fy) = vs_gyroflow::sphere::View::at(&clip.params, mid.round() as i64 + clip.frame_offset_us).focal();
    serde_json::json!({ "shader": out, "output": [clip.output_size.0, clip.output_size.1], "mode": if rgb { "grid" } else { "planes" },
                        "chromatic": wet, "sphere": sphere, "focal": [fx, fy], "ms": (t.elapsed().as_secs_f64() * 1000.0).round() })
}

/// The request to a server: one line of { json, out }, one line back. None: no server there (the caller does it).
/// One just started gets a moment to listen
#[cfg(not(unix))]
fn ask_server(_sock: &str, _json: &str, _out: &str) -> Option<String> { None }
#[cfg(unix)]
fn ask_server(sock: &str, json: &str, out: &str) -> Option<String> {
    use std::io::{ BufRead, Write };
    let started = Instant::now();
    let debug = std::env::var_os("AFP_TIMES").is_some();
    let mut stream = loop {
        match std::os::unix::net::UnixStream::connect(sock) {
            Ok(s) => break s,
            Err(_) if started.elapsed().as_millis() < 300 => std::thread::sleep(std::time::Duration::from_millis(10)),
            Err(e) => { if debug { eprintln!("no server at {sock}: {e}"); } return None }
        }
    };
    if debug { eprintln!("connected in {:.0} ms", started.elapsed().as_secs_f64() * 1000.0); }
    let req = serde_json::json!({ "json": json, "out": out });
    stream.write_all(format!("{req}\n").as_bytes()).ok()?;
    let mut line = String::new();
    std::io::BufReader::new(stream).read_line(&mut line).ok()?;
    let line = line.trim();
    (!line.is_empty()).then(|| line.to_string())
}

/// `--serve <socket>`: the player's table server (main.lua starts one per player). It keeps the clips it loaded, by
/// the settings that change the geometry (gpu_table::PLAYER_GEOMETRY_KEYS and the project's file as it is now), so a
/// change of the view, the edges, the color fringes, the denoise or the interpolation writes the shader alone. It
/// goes with the player: when its parent is gone, or idle for an hour
#[cfg(not(unix))]
fn serve(_sock: &str) { std::process::exit(1) }
#[cfg(unix)]
fn serve(sock: &str) {
    use std::io::{ BufRead, Write };
    use std::sync::{ Arc, Mutex };
    // AFP_GPU_SERVER_LOG: gyroflow-core's log and each request's time, in that file
    if let Some(f) = std::env::var_os("AFP_GPU_SERVER_LOG").and_then(|p| std::fs::File::create(p).ok()) {
        // Info: at debug the telemetry parser logs every sample (seconds for a clip)
        let _ = simplelog::WriteLogger::init(log::LevelFilter::Info, simplelog::Config::default(), f);
    }
    let _ = std::fs::remove_file(sock);
    let listener = match std::os::unix::net::UnixListener::bind(sock) { Ok(l) => l, Err(e) => { eprintln!("{sock}: {e}"); std::process::exit(1) } };
    let parent = unsafe { libc::getppid() };
    let last = Arc::new(Mutex::new(Instant::now()));
    {
        let (sock, last) = (sock.to_string(), last.clone());
        std::thread::spawn(move || loop {
            std::thread::sleep(std::time::Duration::from_secs(2));
            if unsafe { libc::getppid() } != parent || last.lock().unwrap().elapsed().as_secs() > 3600 {
                let _ = std::fs::remove_file(&sock);
                std::process::exit(0);
            }
        });
    }
    // The clips loaded, the latest last; a few: a clip's views hold its whole motion
    let cache: Arc<Mutex<Vec<(String, Arc<gpu_table::PlayerClip>)>>> = Arc::new(Mutex::new(Vec::new()));
    // One load per key at a time: a second request for it waits for the first's
    let loading: Arc<Mutex<()>> = Arc::new(Mutex::new(()));
    for stream in listener.incoming() {
        let Ok(stream) = stream else { continue };
        let (cache, loading, last) = (cache.clone(), loading.clone(), last.clone());
        std::thread::spawn(move || {
            let t = Instant::now();
            *last.lock().unwrap() = t;
            log::info!("gyroflow-gpu-table server: a request");
            let mut line = String::new();
            let mut reader = std::io::BufReader::new(&stream);
            if reader.read_line(&mut line).is_err() { return; }
            let req: serde_json::Value = serde_json::from_str(&line).unwrap_or_default();
            let out = req.get("out").and_then(|v| v.as_str()).unwrap_or("").to_string();
            let reply = match req.get("json").and_then(|v| v.as_str()).map(serde_json::from_str::<serde_json::Value>) {
                Some(Ok(j)) => {
                    let key = geometry_key(&j);
                    let found = cache.lock().unwrap().iter().find(|(k, _)| *k == key).map(|(_, c)| c.clone());
                    let loaded = match found {
                        Some(c) => Ok(c),
                        None => {
                            let _one = loading.lock().unwrap();
                            let found = cache.lock().unwrap().iter().find(|(k, _)| *k == key).map(|(_, c)| c.clone());
                            match found {
                                Some(c) => Ok(c),
                                None => gpu_table::PlayerClip::load(&j).map(|c| {
                                    let c = Arc::new(c);
                                    let mut cache = cache.lock().unwrap();
                                    cache.retain(|(k, _)| *k != key);
                                    cache.push((key.clone(), c.clone()));
                                    if cache.len() > 4 { cache.remove(0); }
                                    c
                                }),
                            }
                        }
                    };
                    match loaded {
                        Ok(c) => {
                            // The latest used last
                            let mut list = cache.lock().unwrap();
                            if let Some(i) = list.iter().position(|(k, _)| *k == key) { let e = list.remove(i); list.push(e); }
                            drop(list);
                            player_shader(&c, &j, &out, t)
                        }
                        Err(e) => serde_json::json!({ "error": e }),
                    }
                }
                Some(Err(e)) => serde_json::json!({ "error": format!("bad JSON: {e}") }),
                None => serde_json::json!({ "error": "no json" }),
            };
            log::info!("gyroflow-gpu-table server: {:.0} ms for {out}", t.elapsed().as_secs_f64() * 1000.0);
            let _ = (&stream).write_all(format!("{reply}\n").as_bytes());
        });
    }
}

/// What a loaded clip is kept by: its settings that change the geometry, and its project's file (and preset's) as
/// it is now, so a project saved again in Gyroflow loads again
fn geometry_key(j: &serde_json::Value) -> String {
    let stamp = |k: &str| j.get(k).and_then(|v| v.as_str()).and_then(|p| std::fs::metadata(p).ok())
        .map(|m| format!("{}:{:?}", m.len(), m.modified().ok())).unwrap_or_default();
    let mut key: Vec<String> = gpu_table::PLAYER_GEOMETRY_KEYS.iter().map(|k| format!("{k}={}", j.get(*k).map(|v| v.to_string()).unwrap_or_default())).collect();
    key.push(stamp("project"));
    key.push(stamp("preset"));
    key.join("|")
}

fn main() {
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    if args.first().map(|a| a == "--player").unwrap_or(false) && args.len() == 3 { player(&args[1], &args[2], None); }
    if args.first().map(|a| a == "--player").unwrap_or(false) && args.len() == 5 && args[3] == "--server" { player(&args[1], &args[2], Some(&args[4])); }
    if args.first().map(|a| a == "--serve").unwrap_or(false) && args.len() == 2 { serve(&args[1]); return; }
    let usage = || -> ! {
        eprintln!("usage: gyroflow-gpu-table [--mode exact|grid|planes] [--grid-step px] [--knots n] [--interp bilinear|bspline|bicubic|bicubic9|lanczos4|ewa] [--shrink] [--frames a-b] [--readout-ms t] <project.gyroflow> <out.glsl>\n       gyroflow-gpu-table --check n [--readout-ms t] <project.gyroflow>");
        std::process::exit(2);
    };
    let mut value = |name: &str| -> Option<String> {
        let i = args.iter().position(|a| a == name)?;
        let v = (i + 1 < args.len()).then(|| args.remove(i + 1));
        args.remove(i);
        Some(v.unwrap_or_else(|| usage()))
    };
    let mode = match value("--mode").as_deref().unwrap_or("grid") {
        "exact" => gpu_table::Mode::Exact, "grid" => gpu_table::Mode::Grid, "planes" => gpu_table::Mode::Planes, _ => usage(),
    };
    let grid_step: usize = value("--grid-step").map(|v| v.parse().unwrap_or_else(|_| usage())).unwrap_or(16);
    let knots: usize = value("--knots").map(|v| v.parse().unwrap_or_else(|_| usage())).unwrap_or(33);
    let interp = match value("--interp").as_deref().unwrap_or("bicubic") {
        "bilinear" => 0, "bspline" => 1, "bicubic" => 2, "lanczos4" => 3, "ewa" => 4, "bicubic9" => 5, _ => usage(),
    };
    let readout: Option<f64> = value("--readout-ms").map(|v| v.parse().unwrap_or_else(|_| usage()));
    let check: Option<usize> = value("--check").map(|v| v.parse().unwrap_or_else(|_| usage()));
    let check_grid: Option<String> = value("--check-grid");
    let range: Option<(usize, usize)> = value("--frames").map(|v| {
        let (a, b) = v.split_once('-').unwrap_or_else(|| usage());
        (a.parse().unwrap_or_else(|_| usage()), b.parse().unwrap_or_else(|_| usage()))
    });
    let shrink = if let Some(i) = args.iter().position(|a| a == "--shrink") { args.remove(i); true } else { false };
    let chroma_bilinear = if let Some(i) = args.iter().position(|a| a == "--chroma-bilinear") { args.remove(i); true } else { false };
    let grid_linear = if let Some(i) = args.iter().position(|a| a == "--grid-linear") { args.remove(i); true } else { false };
    if args.is_empty() || (check.is_none() && check_grid.is_none() && args.len() != 2) { usage(); }
    if knots == 1 { eprintln!("--knots: 0, or at least 2"); std::process::exit(2); }

    let t = Instant::now();
    let clip = Clip::load(&args[0], readout).unwrap_or_else(|e| { eprintln!("{e}"); std::process::exit(1); });
    let p = &clip.params;
    eprintln!("Loaded {} in {:.0} ms: {}x{} -> {}x{}, {} frames at {:.3} fps, frame offset {} us, lens {}{}, readout {:.2} ms, correction {:.2}, projection {}",
              args[0], t.elapsed().as_secs_f64() * 1000.0, clip.size.0, clip.size.1, clip.output_size.0, clip.output_size.1, clip.frame_count, clip.fps,
              clip.frame_offset_us, p.distortion_model.id(), p.digital_lens.as_ref().map(|d| format!(" + digital {}", d.id())).unwrap_or_default(), p.frame_readout_time, p.lens_correction_amount, p.output_projection);
    if let Some(why) = gpu_table::unsupported(&clip) {
        eprintln!("Warning: the GPU warp doesn't render this clip as Gyroflow does ({why}); the player keeps VapourSynth for it");
    }

    if let Some(out) = check_grid {
        let (w, h) = out.split_once('x').and_then(|(w, h)| Some((w.parse().ok()?, h.parse().ok()?))).unwrap_or_else(|| usage());
        check_grid_error(&clip, (w, h));
        return;
    }
    if let Some(n) = check {
        check_knots(&clip, n);
        return;
    }

    let opts = Options { mode, grid_step, grid_linear, chroma_bilinear, chromatic: false, chromatic_fast: false, edge_fill: None, sphere: false, time_offset: 0.0, denoise: false, knots, interp, shrink, range };
    let t = Instant::now();
    let table = Table::build(&clip, &opts).unwrap_or_else(|e| { eprintln!("{e}"); std::process::exit(1); });
    let built = t.elapsed().as_secs_f64() * 1000.0;
    let shader = gpu_table::shader(&clip, &opts, &table);
    std::fs::write(&args[1], &shader).unwrap_or_else(|e| { eprintln!("Can't write {}: {e}", args[1]); std::process::exit(1); });
    eprintln!("Wrote {} in {:.0} ms (the table in {built:.0}): table {}x{} rgba32f, {:.1} MB, {} {} a frame",
              args[1], t.elapsed().as_secs_f64() * 1000.0, table.width, table.height, shader.len() as f64 / 1e6, table.per_frame,
              if knots == 0 { "rows" } else { "knots" });
}

fn check_knots(clip: &Clip, n: usize) {
    let t = Instant::now();
    let mut spin: Vec<(f64, usize)> = Vec::with_capacity(clip.frame_count);
    for frame in 0..clip.frame_count {
        let tr = clip.transform(frame, clip.output_size);
        // How much the camera turns over the frame's rows: the first row's rotation against the last's
        let q = gpu_table::row_quats(&[tr.matrices[0], *tr.matrices.last().unwrap()]);
        let dot = (q[0][0] * q[1][0] + q[0][1] * q[1][1] + q[0][2] * q[1][2] + q[0][3] * q[1][3]).abs().min(1.0);
        spin.push((2.0 * (dot as f64).acos(), frame));
    }
    let rows = clip.rows();
    eprintln!("Transforms: {:.3} ms a frame ({rows} rows), the whole clip of {} frames in {:.1} s",
              t.elapsed().as_secs_f64() * 1000.0 / clip.frame_count as f64, clip.frame_count, t.elapsed().as_secs_f64());
    if rows <= 1 { eprintln!("No rolling shutter (readout 0): one matrix a frame, the knots don't matter. Try --readout-ms"); return; }
    let mut by_spin = spin.clone();
    by_spin.sort_by(|a, b| b.0.total_cmp(&a.0));
    let mut frames: Vec<usize> = by_spin.iter().take(n - n / 3).map(|s| s.1).collect();
    for i in 0..n / 3 { frames.push(i * clip.frame_count / (n / 3).max(1)); }

    let out = clip.output_size;
    let knot_set = [9usize, 17, 33, 65];
    let mut worst = [0f32; 4];
    println!("frame  turn_over_frame_deg  knots  max_px  p99_px  mean_px");
    for &frame in &frames {
        let tr = clip.transform(frame, out);
        let kp = &tr.kernel_params;
        let f = gyroflow_core::stabilization::projection::output_focal(kp);
        let (out_c, out_f) = (Vector2::new(out.0 as f32 / 2.0, out.1 as f32 / 2.0), Vector2::new(f.0, f.1));
        let coord = |p: Vector2<f32>, m: &[[f32; 16]]| Stabilization::undistort_coord(p, kp, m, &clip.params.distortion_model, clip.params.digital_lens.as_ref(), &[], &out_c, &out_f);
        let full: Vec<Option<Vector2<f32>>> = grid(out, 8).map(|p| coord(p, &tr.matrices)).collect();
        for (ki, &k) in knot_set.iter().enumerate() {
            let m = gpu_table::from_knots(&tr.matrices, k);
            let mut d: Vec<f32> = grid(out, 8).zip(&full).filter_map(|(p, a)| {
                let a = (*a)?;
                // Only where the ray lands in the frame
                if a.x < 0.0 || a.y < 0.0 || a.x >= clip.size.0 as f32 || a.y >= clip.size.1 as f32 { return None; }
                Some((a - coord(p, &m)?).norm())
            }).collect();
            if d.is_empty() { continue; }
            d.sort_by(|a, b| a.total_cmp(b));
            let (max, p99, mean) = (*d.last().unwrap(), d[d.len() * 99 / 100], d.iter().sum::<f32>() / d.len() as f32);
            worst[ki] = worst[ki].max(max);
            println!("{frame:5}  {:8.3}  {k:3}  {max:.4}  {p99:.4}  {mean:.4}", spin[frame].0.to_degrees());
        }
    }
    for (ki, k) in knot_set.iter().enumerate() { println!("worst with {k} knots: {:.4} px", worst[ki]); }
}

fn grid(size: (usize, usize), step: usize) -> impl Iterator<Item = Vector2<f32>> {
    (0..size.1).step_by(step).flat_map(move |y| (0..size.0).step_by(step).map(move |x| Vector2::new(x as f32, y as f32)))
}

/// The coarse geometry's error: grid points every `step` output pixels, from one before the edge to two past it
fn check_grid_error(clip: &Clip, out: (usize, usize)) {
    let mut spin: Vec<(f64, usize)> = (0..clip.frame_count).map(|frame| {
        let tr = clip.transform(frame, out);
        let q = gpu_table::row_quats(&[tr.matrices[0], *tr.matrices.last().unwrap()]);
        let a = gpu_table::row_quats(&[clip.transform(frame.saturating_sub(1), out).matrices[0], tr.matrices[0]]);
        let turn = |q: &Vec<[f32; 4]>| 2.0 * ((q[0][0] * q[1][0] + q[0][1] * q[1][1] + q[0][2] * q[1][2] + q[0][3] * q[1][3]).abs().min(1.0) as f64).acos();
        (turn(&q) + turn(&a), frame)
    }).collect();
    spin.sort_by(|a, b| b.0.total_cmp(&a.0));
    let frames: Vec<usize> = spin.iter().take(4).map(|s| s.1).chain([0, clip.frame_count / 2]).collect();
    println!("frame  step  bilinear_max  bilinear_p99  catmull_max  catmull_p99   (source px)");
    for frame in frames {
        let tr = clip.transform(frame, out);
        let kp = &tr.kernel_params;
        let f = gyroflow_core::stabilization::projection::output_focal(kp);
        let (out_c, out_f) = (Vector2::new(kp.output_width as f32 / 2.0, kp.output_height as f32 / 2.0), Vector2::new(f.0, f.1));
        let coord = |x: f32, y: f32| Stabilization::undistort_coord(Vector2::new(x, y), kp, &tr.matrices, &clip.params.distortion_model, clip.params.digital_lens.as_ref(), &[], &out_c, &out_f);
        // Exact, every pixel, in parallel
        let exact: Vec<Option<Vector2<f32>>> = std::thread::scope(|s| {
            let chunks: Vec<_> = (0..8).map(|t| { let coord = &coord; s.spawn(move || {
                (0..out.1).filter(|y| y % 8 == t).flat_map(|y| (0..out.0).map(move |x| (y, coord(x as f32, y as f32)))).collect::<Vec<_>>()
            }) }).collect();
            let mut all = vec![None; out.0 * out.1];
            for c in chunks { for (i, (y, v)) in c.join().unwrap().into_iter().enumerate() { let x = i % out.0; all[y * out.0 + x] = v; } }
            all
        });
        for step in [4usize, 8, 16, 32] {
            let (gw, gh) = (out.0.div_ceil(step) + 3, out.1.div_ceil(step) + 3);
            let grid: Vec<Vector2<f32>> = (0..gh).flat_map(|j| (0..gw).map(move |i| (i, j))).map(|(i, j)| {
                coord((i as f32 - 1.0) * step as f32, (j as f32 - 1.0) * step as f32).unwrap_or(Vector2::new(f32::NAN, f32::NAN))
            }).collect();
            let g = |i: isize, j: isize| grid[(j.clamp(0, gh as isize - 1) as usize) * gw + i.clamp(0, gw as isize - 1) as usize];
            let cr = |t: f32| -> [f32; 4] { let t2 = t * t; let t3 = t2 * t;
                [-0.5 * t3 + t2 - 0.5 * t, 1.5 * t3 - 2.5 * t2 + 1.0, -1.5 * t3 + 2.0 * t2 + 0.5 * t, 0.5 * t3 - 0.5 * t2] };
            let (mut eb, mut ec) = (Vec::new(), Vec::new());
            for y in 0..out.1 {
                for x in 0..out.0 {
                    let Some(e) = exact[y * out.0 + x] else { continue };
                    if e.x < 0.0 || e.y < 0.0 || e.x >= clip.size.0 as f32 || e.y >= clip.size.1 as f32 { continue; }
                    let (gx, gy) = (x as f32 / step as f32 + 1.0, y as f32 / step as f32 + 1.0);
                    let (i, j) = (gx.floor() as isize, gy.floor() as isize);
                    let (fx, fy) = (gx - i as f32, gy - j as f32);
                    let b = g(i, j) * (1.0 - fx) * (1.0 - fy) + g(i + 1, j) * fx * (1.0 - fy) + g(i, j + 1) * (1.0 - fx) * fy + g(i + 1, j + 1) * fx * fy;
                    let (wx, wy) = (cr(fx), cr(fy));
                    let mut c = Vector2::zeros();
                    for dj in 0..4 { for di in 0..4 { c += g(i - 1 + di as isize, j - 1 + dj as isize) * (wx[di] * wy[dj]); } }
                    if b.x.is_finite() { eb.push((b - e).norm()); }
                    if c.x.is_finite() { ec.push((c - e).norm()); }
                }
            }
            let stat = |v: &mut Vec<f32>| { v.sort_by(|a, b| a.total_cmp(b)); (v.last().copied().unwrap_or(0.0), v.get(v.len() * 99 / 100).copied().unwrap_or(0.0)) };
            let ((bm, b99), (cm, c99)) = (stat(&mut eb), stat(&mut ec));
            println!("{frame:5}  {step:4}  {bm:12.4}  {b99:12.4}  {cm:11.4}  {c99:11.4}");
        }
    }
}
