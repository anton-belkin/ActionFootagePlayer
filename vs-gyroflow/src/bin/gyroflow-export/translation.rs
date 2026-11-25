// --translation: the camera's sideways jerks, measured.
// One of gyroflow-export's parts: main.rs has the uses and main(); each part's items are the program's.

use super::*;

// --translation: the video stabilized small, each frame's motion from the one before, fitted to the camera's jerks
// (vs_gyroflow::translation), kept by the player's name for the clip (its project, else the video)
pub(crate) fn translation(file: &str, opts: &Options) -> Result<Outcome, String> {
    use vs_gyroflow::translation as tr;
    let t0 = Instant::now();
    let path = Path::new(file);
    let info = ffprobe(path)?;
    let source = find_source(path, opts.project.as_deref(), opts.lens, opts.preset.as_deref())?;
    if source.lens == vs_gyroflow::LensMode::Only { return Ok(Outcome::Skipped("no motion data to stabilize with".into())); }
    let key = opts.project.as_ref().map_or(file.to_string(), |p| p.to_string_lossy().to_string());
    let fps = info.fps.0 as f64 / info.fps.1.max(1) as f64;
    let (sw, sh) = (960usize, ((960.0 * info.height as f64 / info.width.max(1) as f64).round() as usize) & !1);
    let engine = Engine::load(&source.project, source.preset.as_deref(), opts.fov, opts.horizon_lock, opts.underwater, opts.level,
                              0.0, (sw, sh), 1, (1, 1), None, SampleKind::U16, 1023.0, true, Interpolation::Bilinear, 2, None, None, opts.frame, source.lens)?;
    let (rw, rh) = engine.render_size;
    let view = if opts.fov == Fov::Wide { vs_gyroflow::edges::VIEW_WIDE } else { vs_gyroflow::edges::VIEW_NORMAL };
    let params = engine.views.params[engine.views.index(view, opts.level).min(engine.views.params.len() - 1)].clone();
    let frames = params.frame_count.max(1);
    // The camera's jerks first: no point stabilizing a clip whose camera records none
    let basis = tr::basis_of_video(file, fps, frames, engine.frame_offset_us).ok_or("the camera records no accelerometer (Osmo Action 6 does)")?;
    eprintln!("{}: translation · {} · {sw}x{sh} -> {rw}x{rh} · {frames} frames", file_name(file), source.label);

    let mut dec = ffmpeg_command("ffmpeg");
    dec.args(["-nostdin", "-v", "error"]);
    if cfg!(target_os = "macos") { dec.args(["-hwaccel", "videotoolbox"]); }
    dec.args(["-noautorotate", "-i"]).arg(path).args(["-map", "0:v:0", "-vf", &format!("scale={sw}:{sh}"), "-f", "rawvideo", "-pix_fmt", "gray10le", "-"])
        .stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::null());
    let mut decoder = dec.spawn().map_err(|e| format!("ffmpeg: {e}"))?;
    let mut out = decoder.stdout.take().unwrap();
    let mut src = vec![0u8; sw * sh * 2];
    let mut dst = vec![0u8; rw * rh * 2];
    // The frames measured on a thread of their own while the next ones are stabilized
    let (tx, rx) = mpsc::sync_channel::<Vec<f32>>(4);
    let measure = std::thread::spawn(move || {
        let mut motion: Vec<[f64; 2]> = Vec::new();
        let mut prev: Option<tr::Pyramid> = None;
        for luma in rx {
            let cur = tr::Pyramid::new(rw, rh, luma);
            motion.push(prev.as_ref().map_or([0.0; 2], |p| tr::frame_motion(&tr::block_motions(p, &cur))));
            prev = Some(cur);
        }
        motion
    });
    let mut last = Instant::now();
    let mut n = 0usize;
    while n < frames {
        if stopped() { let _ = decoder.kill(); return Err("cancelled".into()); }
        if out.read_exact(&mut src).is_err() { break; }
        let timestamp_us = ((n as f64 / fps) * 1_000_000.0).round() as i64 + engine.frame_offset_us;
        let job = PlaneJob { src: src.as_ptr(), src_stride: sw * 2, src_w: sw, src_h: sh, dst: dst.as_mut_ptr(), dst_stride: rw * 2, dst_w: rw, dst_h: rh };
        engine.process(timestamp_us, vec![job], !info.full_range, false)?;
        let luma: Vec<f32> = dst.chunks_exact(2).map(|b| u16::from_le_bytes([b[0], b[1]]) as f32 / 1023.0).collect();
        if tx.send(luma).is_err() { break; }
        n += 1;
        if last.elapsed().as_millis() >= 500 {
            last = Instant::now();
            opts.progress.write(serde_json::json!({ "pid": std::process::id(), "translation": (n as f64 / frames as f64 * 100.0).round() }));
        }
    }
    drop(tx);
    let _ = decoder.wait();
    let mut motion = measure.join().map_err(|_| "measuring the frames failed")?;
    motion.resize(frames, [0.0; 2]);
    let fit = tr::fit(&motion, &basis, fps);
    // Over the output camera's focal length (warp.glsl's output_ray), so the player's view and zoom don't matter
    let clip = vs_gyroflow::gpu_table::Clip { size: (params.width, params.height), output_size: (params.output_width, params.output_height), fps, frame_count: frames,
                                              frame_offset_us: engine.frame_offset_us, params };
    let to_out = clip.output_size.0 as f64 / rw as f64;
    let shifts: Vec<[f32; 2]> = (0..frames).map(|f| {
        let kp = clip.transform(f, clip.output_size).kernel_params;
        let stretch = if kp.input_horizontal_stretch > 0.01 { 1.0 / kp.input_horizontal_stretch as f64 } else { 1.0 };
        let fov = (kp.fov as f64).max(1e-6);
        let (fx, fy) = (kp.f[0] as f64 * stretch / fov, kp.f[1] as f64 / fov);
        [(fit.shifts[f][0] * to_out / fx) as f32, (fit.shifts[f][1] * to_out / fy) as f32]
    }).collect();
    let kept = tr::write_cache(&key, &shifts)?;
    let scale = 960.0 / rw as f64;
    eprintln!("  jitter above {} Hz: {:.2} -> {:.2} px at 960 wide ({:.0} % of the fit), {:.0} s", tr::CUTOFF_HZ, fit.before * scale, fit.after * scale,
              fit.share * 100.0, t0.elapsed().as_secs_f64());
    println!("{}", serde_json::json!({ "translation": kept, "before": fit.before * scale, "after": fit.after * scale, "share": fit.share }));
    opts.progress.write(serde_json::json!({ "pid": std::process::id(), "translation": 100 }));
    Ok(Outcome::Done(kept))
}
