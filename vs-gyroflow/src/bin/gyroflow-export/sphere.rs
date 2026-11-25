// 360 cameras (--360, --reframe): a stabilized 360 video or a flat reframe.
// One of gyroflow-export's parts: main.rs has the uses and main(); each part's items are the program's.

use super::*;

/// A 360 camera's file as a stabilized 360 video or a flat reframe (vs_gyroflow::export360): ffmpeg decodes the lens
/// tracks joined side by side (as the player's lavfi-complex), the frames go through the plugin's 360 mode and the
/// view, ffmpeg encodes them (HEVC 10-bit, the source's audio copied); a 360 video gets its spherical metadata after
/// The picture hold's measuring (reframe.rs): the view rendered small along the recording's angle tidying, frame by
/// frame over its span, what's in its middle matched with the frame before
fn measure_hold(file: &str, opts: &Options, rec: &vs_gyroflow::reframe::Recording) -> Result<Vec<vs_gyroflow::reframe::HoldFrame>, String> {
    use vs_gyroflow::export360::{ HeadPath, Output, Render360 };
    use vs_gyroflow::reframe::{ measure_shift, HoldFrame, HOLD_SIZE };
    let path = vs_gyroflow::canonicalize(file).map_err(|e| e.to_string())?;
    let video = path.to_string_lossy().to_string();
    let rig = match vs_gyroflow::rig::read(&video) { Some(r) => r?, None => return Err("not a 360 camera's file".into()) };
    let info = ffprobe(&path)?;
    let depth_probe = ffmpeg_command("ffprobe").args(["-v", "error", "-select_streams", "v:0", "-show_entries", "stream=pix_fmt", "-of", "csv=p=0"]).arg(&path).output().map_err(|e| e.to_string())?;
    let src_fmt = String::from_utf8_lossy(&depth_probe.stdout).trim().to_string();
    let depth = if src_fmt.contains("10") || src_fmt.contains("12") || src_fmt.contains("16") { 10 } else { 8 };
    let raw_fmt = match (depth, info.full_range) { (8, true) => "yuvj420p", (8, false) => "yuv420p", _ => "yuv420p10le" };
    let angles = rec.tidied();
    let t0 = Instant::now();
    // A small sphere is enough for a 480-pixel view
    let render = Render360::new(&video, depth, !info.full_range, Output::Flat { size: HOLD_SIZE, head: HeadPath::Recording(angles.clone()) }, Some(rec.follows), 2048, opts.sphere.1)?;
    let fps = info.fps.0 as f64 / info.fps.1 as f64;
    let (from, to) = angles.span();
    let to = (to + 0.5 / fps).min(info.duration.max(from));
    let first = (from.max(0.0) * fps - 1e-6).ceil() as u64;
    let total_frames = (((to * fps) - 1e-6).ceil() as u64).saturating_sub(first).max(1);
    let (in_bytes, out_bytes) = render.frame_bytes();
    let (w, h) = HOLD_SIZE;
    eprintln!("{}: measuring the hold over {total_frames} frames", file_name(file));

    let inputs: String = (0..rig.pictures).map(|i| format!("[0:v:{i}]")).collect();
    let mut dec_cmd = ffmpeg_command("ffmpeg");
    dec_cmd.args(["-nostdin", "-v", "error"]);
    if first > 0 { dec_cmd.args(["-ss", &format!("{:.6}", first as f64 / fps)]); }
    dec_cmd.arg("-i").arg(&path).args(["-filter_complex", &format!("{inputs}hstack=inputs={},format={raw_fmt}[v]", rig.pictures),
        "-map", "[v]", "-frames:v", &total_frames.to_string(), "-f", "rawvideo", "-"])
        .stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::inherit());
    let mut decoder = dec_cmd.spawn().map_err(|e| format!("ffmpeg: {e}"))?;
    if BACKGROUND.load(Relaxed) { set_background(&[decoder.id()], true); }
    let mut dec_out = decoder.stdout.take().ok_or("no decoder output")?;
    let (in_tx, in_rx) = mpsc::sync_channel::<(u64, Vec<u8>)>(2);
    let in_rx = Arc::new(Mutex::new(in_rx));
    let (done_tx, done_rx) = mpsc::sync_channel::<(u64, Result<(Vec<u8>, f64), String>)>(4);
    let render = Arc::new(render);
    let reader = std::thread::spawn(move || {
        for n in 0..total_frames {
            let mut buf = vec![0u8; in_bytes];
            if dec_out.read_exact(&mut buf).is_err() { break; }
            if in_tx.send((n, buf)).is_err() { break; }
        }
    });
    // Each rendered frame's luma, a byte a pixel, and the yaw of what the view follows there (degrees)
    let workers: Vec<_> = (0..2).map(|_| {
        let (rx, tx, render) = (in_rx.clone(), done_tx.clone(), render.clone());
        std::thread::spawn(move || loop {
            let Ok((n, buf)) = rx.lock().recv() else { break };
            let t = (first + n) as f64 / fps;
            let mut dst = vec![0u8; out_bytes];
            let r = render.render(t, &buf, &mut dst).map(|st| (if depth > 8 {
                let y: &[u16] = bytemuck::cast_slice(&dst[..w * h * 2]);
                y.iter().map(|v| (v >> 2).min(255) as u8).collect()
            } else {
                dst[..w * h].to_vec()
            }, (st.headings[render.heading as usize].yaw as f64).to_degrees()));
            if tx.send((n, r)).is_err() { break; }
        })
    }).collect();
    drop(done_tx);
    let mut pending = BTreeMap::new();
    let (mut next, mut error) = (0u64, None);
    let (mut last_report, mut last_control) = (Instant::now(), Instant::now());
    let mut prev: Option<(Vec<u8>, f64)> = None;
    let mut frames = Vec::with_capacity(total_frames as usize);
    for (n, result) in done_rx {
        if let Some(control) = &opts.control {
            if last_control.elapsed().as_millis() >= 250 { read_control(control); last_control = Instant::now(); }
        }
        if stopped() { error = Some("cancelled".to_string()); break; }
        match result {
            Ok(r) => { pending.insert(n, r); }
            Err(e) => { error = Some(format!("frame {n}: {e}")); break; }
        }
        while let Some((cur, heading)) = pending.remove(&next) {
            let t = (first + next) as f64 / fps;
            if let Some((p, heading_before)) = &prev {
                // Less where the view's own turn moved it (the head's and what it follows), searched for around there:
                // what's left moved in the world
                let (head, projection) = angles.head_at(t);
                let (mut before, _) = angles.head_at(t - 1.0 / fps);
                let turn = (heading - heading_before + 540.0).rem_euclid(360.0) - 180.0;
                before.yaw -= turn;
                let f = vs_gyroflow::reframe::focal_px(projection, head.fov, h as f64);
                let e = vs_gyroflow::reframe::expected_shift(&before, &head, f);
                frames.push(HoldFrame::from_shift(t, measure_shift(p, &cur, w, h, e), e, projection, head.fov, h as f64));
            }
            prev = Some((cur, heading));
            next += 1;
        }
        if last_report.elapsed().as_millis() >= 500 {
            last_report = Instant::now();
            let rate = next as f64 / t0.elapsed().as_secs_f64();
            let eta = ((total_frames - next.min(total_frames)) as f64 / rate.max(1e-3)).min(359_999.0) as u64;
            eprint!("\r  measuring {:3.0}%  {rate:4.1} fps  ETA {}:{:02}   ", next as f64 / total_frames as f64 * 100.0, eta / 60, eta % 60);
            opts.progress.write(serde_json::json!({ "pid": std::process::id(), "done": next, "total": total_frames, "fps": rate, "eta": eta, "stage": "measuring" }));
        }
    }
    eprintln!();
    if error.is_some() || stopped() { let _ = decoder.kill(); }
    let _ = reader.join();
    for w in workers { let _ = w.join(); }
    let _ = decoder.wait();
    if let Some(e) = error.or_else(|| (next < total_frames).then(|| format!("{next} of {total_frames} frames decoded"))) { return Err(e); }
    let sure = frames.iter().map(|f| f.sure).sum::<f64>() / frames.len().max(1) as f64;
    eprintln!("{}: the hold measured over {next} frames in {:.0} s (sure {:.0} % on average)", file_name(file), t0.elapsed().as_secs_f64(), sure * 100.0);
    Ok(frames)
}

pub(crate) fn export_360(file: &str, opts: &Options, mode: &Mode360) -> Result<Outcome, String> {
    use vs_gyroflow::export360::{ HeadPath, Output, Render360, heading_name };
    let path = vs_gyroflow::canonicalize(file).map_err(|e| e.to_string())?;
    let video = path.to_string_lossy().to_string();
    let rig = match vs_gyroflow::rig::read(&video) {
        Some(Ok(r)) => r,
        Some(Err(e)) => return Err(e),
        None => return Ok(Outcome::Skipped("not a 360 camera's file (--360 and --reframe are for those)".into())),
    };
    let info = ffprobe(&path)?;
    let out_dir = match (&opts.output, &opts.out) {
        (Some(f), _) => f.parent().filter(|p| !p.as_os_str().is_empty()).unwrap_or(Path::new(".")).to_path_buf(),
        (None, Some(d)) => d.clone(),
        (None, None) => default_out_dir(&path),
    };
    let stem = path.file_stem().map(|x| x.to_string_lossy().to_string()).unwrap_or_default();
    let range = match (opts.from, opts.to) {
        (None, None) => String::new(),
        (f, t) => format!("_{:.1}-{}s", f.unwrap_or(0.0), t.map(|t| format!("{t:.1}")).unwrap_or_else(|| "end".into())),
    };
    let kind = if mode.keys.is_some() { "reframe" } else { "360" };
    // A reframe's frame in its name when it isn't 16:9 (_9x16, _1x1), so the shapes of one part don't replace each other
    let shape = {
        let (w, h) = mode.size;
        let g = (1..=w.min(h)).rev().find(|g| w % g == 0 && h % g == 0).unwrap_or(1);
        if kind == "reframe" && w * 9 != h * 16 { format!("_{}x{}", w / g, h / g) } else { String::new() }
    };
    // The recording: what it follows, unless --heading says otherwise, and the span recorded unless --from/--to do;
    // tidied as it says, the picture hold measured first when it's asked for and not measured along this path yet
    if mode.measure_hold && !matches!(mode.keys, Some(Head360::Recording)) { return Err("--measure-hold is for --reframe recording".into()); }
    let recording = match &mode.keys {
        Some(Head360::Recording) => {
            let rec = vs_gyroflow::reframe::Recording::read(&path)?;
            if mode.measure_hold {
                let frames = measure_hold(file, opts, &rec)?;
                rec.write_hold(&video, &frames)?;
                return Ok(Outcome::Done(rec.hold_file(&video).unwrap_or_default()));
            }
            let (mut r, needed) = rec.tidied_for(&video);
            if needed {
                eprintln!("{}: measuring the hold on what's in the middle first", file_name(file));
                let frames = measure_hold(file, opts, &rec)?;
                rec.write_hold(&video, &frames)?;
                r = rec.tidied_for(&video).0;
            }
            Some(r)
        }
        _ => None,
    };
    let range = match (&recording, opts.from, opts.to) {
        (Some(r), None, None) => { let (a, b) = r.span(); format!("_{a:.1}-{b:.1}s") }
        _ => range,
    };
    let out_path = opts.output.clone().unwrap_or_else(|| out_dir.join(format!("{stem}_{kind}{shape}{range}.mp4")));
    if out_path.exists() && !opts.force { return Ok(Outcome::Skipped(format!("{} exists (--force replaces it)", out_path.display()))); }
    std::fs::create_dir_all(&out_dir).map_err(|e| format!("{}: {e}", out_dir.display()))?;

    // 8-bit sources as 8-bit frames, the others as 16-bit samples
    let depth_probe = ffmpeg_command("ffprobe").args(["-v", "error", "-select_streams", "v:0", "-show_entries", "stream=pix_fmt", "-of", "csv=p=0"]).arg(&path).output().map_err(|e| e.to_string())?;
    let src_fmt = String::from_utf8_lossy(&depth_probe.stdout).trim().to_string();
    let depth = if src_fmt.contains("10") || src_fmt.contains("12") || src_fmt.contains("16") { 10 } else { 8 };
    let raw_fmt = match (depth, info.full_range) { (8, true) => "yuvj420p", (8, false) => "yuv420p", _ => "yuv420p10le" };
    // The recording is relative to what it follows: that one, whatever --heading says
    let heading = recording.as_ref().map(|r| r.follows).or(mode.heading);
    if let (Some(r), Some(h)) = (&recording, mode.heading) {
        if r.follows != h { eprintln!("{}: along {}, as the recording follows (not --heading {})", file_name(file), heading_name(r.follows), heading_name(h)); }
    }
    let out = match (&mode.keys, &recording) {
        (None, _) => Output::Sphere { width: if mode.sphere_width > 0 { mode.sphere_width } else { vs_gyroflow::sphere360::DEFAULT_WIDTH } },
        (Some(_), Some(r)) => Output::Flat { size: mode.size, head: HeadPath::Recording(r.clone()) },
        (Some(Head360::Keys(keys)), None) => Output::Flat { size: mode.size, head: HeadPath::Keys { keys: keys.clone(), projection: mode.projection } },
        (Some(Head360::Recording), None) => unreachable!(),
    };
    let t0 = Instant::now();
    let render = Render360::new(&video, depth, !info.full_range, out, heading, mode.sphere_width, mode.distance)?;
    if let Some(note) = &render.heading_note { eprintln!("{}: {note}", file_name(file)); }
    eprintln!("{}: {} {}, {} lenses; {}, along {} ({:.1} s to load)", file_name(file), rig.brand, rig.model, rig.lenses.len(),
              if kind == "360" { "a 360 video" } else { "a reframe" }, heading_name(render.heading), t0.elapsed().as_secs_f64());
    let fps = info.fps.0 as f64 / info.fps.1 as f64;
    let span = recording.as_ref().map(|r| r.span());
    let from = opts.from.or(span.map(|s| s.0)).unwrap_or(0.0).max(0.0);
    // The recording's last sample's frame included
    let to = opts.to.or(span.map(|s| s.1 + 0.5 / fps)).unwrap_or(info.duration).min(info.duration.max(from));
    let first = (from * fps - 1e-6).ceil() as u64;
    let total_frames = (((to * fps) - 1e-6).ceil() as u64).saturating_sub(first).max(1);
    let (in_bytes, out_bytes) = render.frame_bytes();
    let (ow, oh) = render.out.size();

    // Decoding: the tracks joined, from the first frame on
    let inputs: String = (0..rig.pictures).map(|i| format!("[0:v:{i}]")).collect();
    let mut dec_cmd = ffmpeg_command("ffmpeg");
    dec_cmd.args(["-nostdin", "-v", "error"]);
    if first > 0 { dec_cmd.args(["-ss", &format!("{:.6}", first as f64 / fps)]); }
    dec_cmd.arg("-i").arg(&path).args(["-filter_complex", &format!("{inputs}hstack=inputs={},format={raw_fmt}[v]", rig.pictures),
        "-map", "[v]", "-frames:v", &total_frames.to_string(), "-f", "rawvideo", "-"])
        .stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::inherit());
    let mut decoder = dec_cmd.spawn().map_err(|e| format!("ffmpeg: {e}"))?;
    // The look, as it plays: a log curve's LUT to HLG, or to Rec.709 (--sdr: the maker's LUT, D-Log M's; --nbcu:
    // NBCU's from the HLG), in RGB from the source's matrix; else the colors as recorded, BT.709
    let curve = clip_curve(opts, &path);
    let in_range = if info.full_range { "pc" } else { "tv" };
    let mut _temp_lut = None;
    // The underwater colour: one LUT, its numbers' median over what's exported
    let uw = match curve {
        Some("uw") => Some(Uw::new(&path, opts, UwSpan::Median(vs_gyroflow::uwcolor::measure_times(from, to)), &out_dir,
                                   &out_path.file_stem().map(|x| x.to_string_lossy().to_string()).unwrap_or_default())?),
        _ => None,
    };
    let curve_lut = |c: &str| match (&uw, c) { (Some(u), "uw") => Ok(u.cube.0.clone()), _ => lut(&curve_lut_file(c)) };
    let (look_vf, tags, look_label) = match curve {
        None => ("scale=out_range=tv:out_color_matrix=bt709".to_string(), ["bt709", "bt709", "bt709"], "colors as recorded".to_string()),
        Some(curve) => {
            let name = curve_name(curve);
            let (cube, tags, label) = if opts.nbcu {
                let composed = out_dir.join(format!(".{}.nbcu.cube", out_path.file_stem().map(|x| x.to_string_lossy().to_string()).unwrap_or_default()));
                gpu_lut::compose_nbcu(&curve_lut(curve)?, &lut(NBCU_HLG)?, &composed)?;
                _temp_lut = Some(TempFile(composed.clone()));
                (composed, ["bt709", "bt709", "bt709"], format!("{name} -> Rec.709 (NBCU)"))
            } else if opts.sdr && curve == "dlogm" {
                (lut("dji-dlogm-rec709.cube")?, ["bt709", "bt709", "bt709"], format!("{name} -> Rec.709 (DJI's LUT)"))
            } else {
                (curve_lut(curve)?, ["bt2020", "arib-std-b67", "bt2020nc"], format!("{name} -> HLG"))
            };
            let matrix = if tags[2] == "bt2020nc" { "bt2020" } else { "bt709" };
            (format!("scale=in_color_matrix={}:in_range={in_range},format=gbrp16le,lut3d=file={}:interp=tetrahedral,scale=out_color_matrix={matrix}:out_range=tv",
                     yuv_matrix(info.matrix.as_deref()), filter_path(&cube)), tags, label)
        }
    };
    eprintln!("{}: {look_label}", file_name(file));
    // Encoding: HEVC 10-bit at the source's bits per pixel, in that look, limited range, the source's audio
    let enc = pick_encoder(opts, false)?;
    let src_px = (rig.picture.0 * rig.picture.1 * rig.pictures) as f64;
    let bit_rate = (info.bit_rate.unwrap_or(60_000_000) as f64 * rig.pictures as f64 * (ow * oh) as f64 / src_px).clamp(4e6, 200e6);
    let tmp_path = out_dir.join(format!(".{}.partial.mp4", out_path.file_stem().map(|x| x.to_string_lossy().to_string()).unwrap_or_default()));
    let mut enc_cmd = ffmpeg_command("ffmpeg");
    enc_cmd.args(["-nostdin", "-v", "error", "-y"]).args(&enc.global)
        .args(["-f", "rawvideo", "-pix_fmt", raw_fmt, "-s", &format!("{ow}x{oh}"), "-r", &format!("{}/{}", info.fps.0, info.fps.1),
               "-color_range", if info.full_range { "pc" } else { "tv" }, "-i", "-"]);
    if first > 0 { enc_cmd.args(["-ss", &format!("{:.6}", first as f64 / fps)]); }
    enc_cmd.args(["-t", &format!("{:.6}", total_frames as f64 / fps)]).arg("-i").arg(&path)
        .args(["-map", "0:v", "-map", "1:a?", "-c:a", "copy",
               "-vf", &format!("{look_vf},format={},setparams=color_primaries={}:color_trc={}:colorspace={}:range=tv{}", enc.pix_fmt, tags[0], tags[1], tags[2], enc.upload),
               "-c:v", &enc.name]).args(&enc.args)
        .args(["-tag:v", "hvc1", "-b:v", &format!("{}", bit_rate as u64),
               "-color_primaries", tags[0], "-color_trc", tags[1], "-colorspace", tags[2], "-color_range", "tv", "-f", "mp4"]).arg(&tmp_path)
        .stdin(Stdio::piped()).stdout(Stdio::null()).stderr(Stdio::inherit());
    let mut encoder = enc_cmd.spawn().map_err(|e| { let _ = decoder.kill(); format!("ffmpeg: {e}") })?;
    if BACKGROUND.load(Relaxed) { set_background(&[decoder.id(), encoder.id()], true); }

    // Frames in order: read here, rendered by two threads (the plugin's GPU workers and the view in turn), written
    // in order
    let mut dec_out = decoder.stdout.take().ok_or("no decoder output")?;
    let mut enc_in = encoder.stdin.take().ok_or("no encoder input")?;
    let (in_tx, in_rx) = mpsc::sync_channel::<(u64, Vec<u8>)>(2);
    let in_rx = Arc::new(Mutex::new(in_rx));
    let (done_tx, done_rx) = mpsc::sync_channel::<(u64, Result<(Vec<u8>, f64), String>)>(4);
    let render = Arc::new(render);
    let reader = std::thread::spawn(move || {
        for n in 0..total_frames {
            let mut buf = vec![0u8; in_bytes];
            if dec_out.read_exact(&mut buf).is_err() { break; }
            if in_tx.send((n, buf)).is_err() { break; }
        }
    });
    let workers: Vec<_> = (0..2).map(|_| {
        let (rx, tx, render) = (in_rx.clone(), done_tx.clone(), render.clone());
        std::thread::spawn(move || loop {
            let Ok((n, buf)) = rx.lock().recv() else { break };
            let t = (first + n) as f64 / fps;
            let mut dst = vec![0u8; out_bytes];
            let r = render.render(t, &buf, &mut dst).map(|st| (dst, st.headings[render.heading as usize].confidence));
            if tx.send((n, r)).is_err() { break; }
        })
    }).collect();
    drop(done_tx);
    let mut pending = BTreeMap::new();
    let (mut next, mut error) = (0u64, None);
    let (mut last_report, mut last_control) = (Instant::now(), Instant::now());
    let mut sure = (0.0, 0u64);
    for (n, result) in done_rx {
        if let Some(control) = &opts.control {
            if last_control.elapsed().as_millis() >= 250 { read_control(control); last_control = Instant::now(); }
        }
        if stopped() { error = Some("cancelled".to_string()); break; }
        match result {
            Ok(r) => { pending.insert(n, r); }
            Err(e) => { error = Some(format!("frame {n}: {e}")); break; }
        }
        while let Some((buf, c)) = pending.remove(&next) {
            sure = (sure.0 + c, sure.1 + 1);
            if let Err(e) = enc_in.write_all(&buf) { error = Some(format!("encoder stopped: {e}")); break; }
            next += 1;
        }
        if error.is_some() { break; }
        if last_report.elapsed().as_millis() >= 500 {
            last_report = Instant::now();
            let rate = next as f64 / t0.elapsed().as_secs_f64();
            let eta = ((total_frames - next.min(total_frames)) as f64 / rate.max(1e-3)).min(359_999.0) as u64;
            eprint!("\r  {:3.0}%  {rate:4.1} fps  ETA {}:{:02}   ", next as f64 / total_frames as f64 * 100.0, eta / 60, eta % 60);
            opts.progress.write(serde_json::json!({ "pid": std::process::id(), "done": next, "total": total_frames, "fps": rate, "eta": eta, "partial": tmp_path }));
        }
    }
    drop(enc_in);
    eprintln!();
    if error.is_some() || stopped() { let _ = decoder.kill(); let _ = encoder.kill(); }
    let _ = reader.join();
    for w in workers { let _ = w.join(); }
    let _ = decoder.wait();
    let status = encoder.wait().map_err(|e| e.to_string())?;
    if let Some(e) = error.or_else(|| (!status.success()).then(|| "the encoder failed".to_string()))
        .or_else(|| (next < total_frames).then(|| format!("{next} of {total_frames} frames decoded"))) {
        let _ = std::fs::remove_file(&tmp_path);
        return Err(e);
    }
    if kind == "360" { vs_gyroflow::mp4meta::write_spherical(&tmp_path, "ActionFootagePlayer")?; }
    std::fs::rename(&tmp_path, &out_path).map_err(|e| format!("{}: {e}", out_path.display()))?;
    eprintln!("{}: {next} frames in {:.0} s, along {} (sure on {:.0} % of them on average)", file_name(file), t0.elapsed().as_secs_f64(),
              heading_name(render.heading), sure.0 / sure.1.max(1) as f64 * 100.0);
    Ok(Outcome::Done(out_path))
}
