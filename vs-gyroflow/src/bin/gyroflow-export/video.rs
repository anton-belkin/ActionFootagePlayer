// A video export: the engine's frames over pipes (the pipes path), or the GPU path (gpu.rs) when it can; YouTube's HDR10, Dolby Vision.
// One of gyroflow-export's parts: main.rs has the uses and main(); each part's items are the program's.

use super::*;

/// A photo's motion (a Live Photo's or Motion Photo's video): a part of the video, written to `out` as it'd export,
/// at most `max_side` wide or high, `max_fps` frames a second (every second, third… frame: `frame_step`) and
/// `bits_per_pixel` a pixel of each frame, as an MP4 (`mp4`), Dolby Vision as the look has it or not (`dv`)
pub(crate) struct Part {
    pub(crate) from: f64,
    pub(crate) to: f64,
    pub(crate) out: PathBuf,
    pub(crate) max_side: usize,
    pub(crate) bits_per_pixel: f64,
    pub(crate) max_fps: f64,
    pub(crate) mp4: bool,
    pub(crate) dv: bool,
    // In an SDR look ("maker", "nbcu"), the one its photo's image has; None: as the export plays (a Live Photo's HLG)
    pub(crate) sdr: Option<&'static str>,
    // Where its photo was taken (ISO 6709), as its QuickTime location
    pub(crate) location: Option<String>,
    // When its photo was taken, as its QuickTime creation date
    pub(crate) created: Option<String>,
    // Its photo's time: the underwater colour's numbers its frame's throughout, as the photo has them
    pub(crate) still: Option<f64>,
}

// How much a player turns the video to show it upright, clockwise (its display matrix, mp4meta.rs); 0 when unknown
pub(crate) fn display_rotation(path: &Path) -> u16 {
    vs_gyroflow::mp4meta::display_rotation(path).unwrap_or(0)
}

// The frames a decoder hands over (the sensor's, of `size`: -noautorotate) and the filters that make them so: for the
// engine as they are (it turns them itself, Engine::load), else turned upright as the file says, --rotation on top,
// and cut to --aspect
pub(crate) fn decoded_frame(stabilized: bool, path: &Path, size: (usize, usize), opts: &Options) -> ((usize, usize), String) {
    if stabilized { return (size, String::new()); }
    let frame = opts.frame.for_video(display_rotation(path), None);
    (frame.size(size), frame.ffmpeg_filter(size))
}

pub(crate) fn export(file: &str, opts: &Options, part: Option<&Part>) -> Result<Outcome, String> {
    // A photo's part: its range, not --from/--to
    let (opt_from, opt_to) = part.map_or((opts.from, opts.to), |p| (Some(p.from), Some(p.to)));
    let dv_rpu = if part.is_some_and(|p| !p.dv) { None } else { opts.dv_rpu.as_ref() };
    let prores = opts.prores && part.is_none();
    // For YouTube (src/youtube.rs): a video export's own kind
    let youtube = opts.youtube.filter(|_| part.is_none() && opts.preview.is_none());
    let path = vs_gyroflow::canonicalize(file).map_err(|e| e.to_string())?;
    let curve = clip_curve(opts, &path);
    // Nothing to stabilize with, or --no-stabilize: the color conversion alone, if there's one
    let source = if opts.no_stabilize { Err("stabilization off".to_string()) } else { find_source(&path, opts.project.as_deref(), opts.lens, opts.preset.as_deref()) };
    let source = match (source, curve) {
        (Ok(s), _) => Some(s),
        (Err(_), Some(_)) => None,
        // A preview shows the clip as it plays, whatever there is to change
        (Err(_), None) if opts.preview.is_some() => None,
        // Turned or cut (--rotation, --aspect): that much to change
        (Err(_), None) if !opts.frame.is_none() => None,
        // A photo's motion (Live or Motion Photo): the video around it, whatever there is to change
        (Err(_), None) if part.is_some() => None,
        // Made smaller (--resolution): that much to change
        (Err(_), None) if opts.resolution.is_some_and(|cap| ffprobe(&path).is_ok_and(|i| i.width.min(i.height) > cap)) => None,
        // HDR that isn't log, in SDR (--nbcu) or with Dolby Vision (--dv-rpu): the color to change
        (Err(_), None) if hdr_in(&path) && (opts.nbcu || dv_rpu.is_some() || youtube.is_some()) => None,
        (Err(why), None) => return Ok(Outcome::Skipped(format!("{why}, and not log: nothing to change"))),
    };
    let info = ffprobe(&path)?;

    let preview = opts.preview;
    let video_dir = path.parent().unwrap_or(Path::new("."));
    let out_dir = match (&opts.output, &opts.out) {
        (Some(file), _) => file.parent().filter(|p| !p.as_os_str().is_empty()).unwrap_or(Path::new(".")).to_path_buf(),
        (None, Some(dir)) => dir.clone(),
        (None, None) if preview.is_some() => video_dir.to_path_buf(),
        // A photo's motion: next to the photo
        (None, None) if part.is_some() => part.unwrap().out.parent().unwrap_or(Path::new(".")).to_path_buf(),
        (None, None) => default_out_dir(&path),
    };
    let stem = path.file_stem().map(|x| x.to_string_lossy().to_string()).unwrap_or_default();
    // A part of the video gets its range in the name, so it doesn't replace the whole one
    let range = match (opt_from, opt_to) {
        (None, None) => String::new(),
        (f, t) => format!("_{:.1}-{}s", f.unwrap_or(0.0), t.map(|t| format!("{t:.1}")).unwrap_or_else(|| "end".into())),
    };
    // A preview is a link to its original: .afplink.mp4, which still opens as an MP4
    let stem = if preview.is_some() { format!("{stem}.afplink") } else {
        format!("{stem}_{}{range}{}", if source.is_some() { "stab" } else { "color" },
                if dv_rpu.is_some() { "_dv" } else if youtube.is_some() { "_youtube" } else { "" })
    };
    let ext = if preview.is_some() { "mp4" } else if youtube.is_some() { "mkv" } else { "mov" };
    let out_path = match part {
        Some(p) => p.out.clone(),
        None => opts.output.clone().unwrap_or_else(|| out_dir.join(format!("{stem}.{ext}"))),
    };
    let stem = out_path.file_stem().map(|x| x.to_string_lossy().to_string()).unwrap_or(stem);
    if out_path.exists() && !opts.force && part.is_none() {
        return Ok(Outcome::Skipped(format!("{} exists (--force replaces it)", out_path.display())));
    }
    std::fs::create_dir_all(&out_dir).map_err(|e| format!("{}: {e}", out_dir.display()))?;
    let tmp_path = match part {
        Some(p) => p.out.with_file_name(format!(".{}.partial", p.out.file_name().unwrap_or_default().to_string_lossy())),
        // For YouTube: the encoder's MOV, then the MKV made from it (youtube_mux)
        None if youtube.is_some() => out_dir.join(format!(".{stem}.partial.mov")),
        None => out_dir.join(format!(".{stem}.partial.{ext}")),
    };
    let final_tmp = if youtube.is_some() { out_dir.join(format!(".{stem}.partial.{ext}")) } else { tmp_path.clone() };

    // The underwater colour (curve "uw"): its numbers over the export, as playback has them; a still's, its frame's.
    // A photo's motion, its photo's frame's (Part::still), so the two match
    let uw = match curve {
        Some("uw") => {
            let (f, t) = (opt_from.unwrap_or(0.0).max(0.0), opt_to.unwrap_or(info.duration).min(info.duration));
            let span = match (preview, part.and_then(|p| p.still)) {
                (Some(_), _) => UwSpan::Still(opts.at.unwrap_or((f + t) / 2.0)),
                (None, Some(at)) => UwSpan::Still(at),
                (None, None) => UwSpan::Played(f, t),
            };
            Some(Uw::new(&path, opts, span, &out_dir, &stem)?)
        }
        _ => None,
    };
    let curve_lut = |c: &str| match (&uw, c) { (Some(u), "uw") => Ok(u.cube.0.clone()), _ => lut(&curve_lut_file(c)) };
    // On the pipes' GPU LUT, each frame's own numbers, exactly as the shader (gpu_lut.rs): what comes after it, a LUT
    // from its HLG to SDR (NBCU's) or none
    let mut uw_after: Option<Option<PathBuf>> = None;

    // Color: the LUT works in RGB (mpv's shader too: it reads MAIN, the decoded RGB), from the source's matrix and
    // range, and the result is converted back to YUV in the target's matrix. On the GPU (gpu_lut.rs) when the
    // output size allows, else ffmpeg's lut3d on the CPU, at a third of the speed. Only D-Log M has a maker's
    // Rec.709 LUT: the others are HLG whatever --sdr says, as they play. --nbcu: the curve's LUT to HLG and NBCU's to
    // SDR as one, made for this export next to its output and removed with it
    let in_range = if info.full_range { "pc" } else { "tv" };
    let in_matrix = yuv_matrix(info.matrix.as_deref());
    let mut lut_job = None;
    let mut _temp_lut = None; // kept until the export ends
    let mut _uw_temp = None;
    // A preview is in SDR: D-Log M by the maker's LUT when it's there, everything else through NBCU's conversion. A
    // Motion Photo's video too, in its image's look (Part::sdr)
    let part_sdr = part.and_then(|p| p.sdr);
    let in_sdr = preview.is_some() || part_sdr.is_some();
    let maker_sdr = curve == Some("dlogm") && !opts.nbcu && part_sdr != Some("nbcu") && (opts.sdr || in_sdr)
        && (!in_sdr || lut("dji-dlogm-rec709.cube").is_ok());
    let nbcu = opts.nbcu || (in_sdr && !maker_sdr);
    // For YouTube: the SDR LUT that goes inside (made here), its name in the MKV, and the look it's in
    let mut youtube_sdr: Option<(TempFile, String, &'static str)> = None;
    let (graph, tags, color_label) = if let Some(want) = youtube {
        use vs_gyroflow::youtube::{ self as yt, Look, SdrFrom };
        let maker_lut = lut("dji-dlogm-rec709.cube");
        let look = match (want, curve) {
            (Some(Look::Maker), Some("dlogm")) => { maker_lut.as_ref().map_err(|e| format!("--youtube maker: DJI's LUT: {e}"))?; Look::Maker }
            (Some(Look::Maker), _) => return Err("--youtube maker: only D-Log M has a maker's LUT (DJI's)".into()),
            (Some(Look::Nbcu), _) => Look::Nbcu,
            (None, Some("dlogm")) if maker_lut.is_ok() => Look::Maker,
            (None, _) => Look::Nbcu,
        };
        let look_name = if look == Look::Maker { "DJI's LUT" } else { "NBCU" };
        // The video: a log curve's HLG, or an HLG clip, made PQ by one LUT; a PQ clip as it is
        let (log_hlg, name) = match (curve, info.transfer.as_deref()) {
            (Some(c), _) => (Some(curve_lut(c)?), curve_name(c)),
            (None, Some("arib-std-b67")) => (None, "HLG"),
            (None, Some("smpte2084")) => (None, "PQ"),
            _ => return Err("--youtube: an HDR video, for a log or HDR (HLG, PQ) clip".into()),
        };
        let (nbcu_hlg, nbcu_pq) = (lut(NBCU_HLG)?, lut(NBCU_PQ)?);
        let from = match (look, name, &log_hlg, &maker_lut) {
            (_, "PQ", _, _) => SdrFrom::PqNbcu { nbcu_pq: &nbcu_pq },
            (Look::Maker, _, Some(log), Ok(maker)) => SdrFrom::HlgMaker { log_hlg: log, maker },
            _ => SdrFrom::HlgNbcu { nbcu_hlg: &nbcu_hlg },
        };
        let sdr = TempFile(out_dir.join(format!(".{stem}.sdr.cube")));
        let title = format!("{stem}: SDR by {look_name}");
        std::fs::write(&sdr.0, yt::sdr_lut(&from, yt::GRID, opts.youtube_levels, &title)?).map_err(|e| format!("{}: {e}", sdr.0.display()))?;
        youtube_sdr = Some((sdr, format!("{stem}_sdr_{}.cube", look.name()), look_name));
        if name == "PQ" {
            let tags = vec![info.primaries.clone().unwrap_or_else(|| "bt2020".into()), "smpte2084".to_string(),
                            info.matrix.clone().unwrap_or_else(|| "bt2020nc".into()), in_range.to_string()];
            (String::new(), tags, format!("PQ as it is, for YouTube (SDR by {look_name})"))
        } else {
            let cube = out_dir.join(format!(".{stem}.pq.cube"));
            std::fs::write(&cube, yt::pq_lut(log_hlg.as_deref(), yt::GRID)?).map_err(|e| format!("{}: {e}", cube.display()))?;
            _temp_lut = Some(TempFile(cube.clone()));
            let graph = format!("scale=in_color_matrix={in_matrix}:in_range={in_range},format=gbrp16le,lut3d=file={}:interp=tetrahedral,\
                                 scale=out_color_matrix=bt2020:out_range=tv", filter_path(&cube));
            lut_job = Some((cube, "bt2020", graph));
            let tags = vec!["bt2020".to_string(), "smpte2084".to_string(), "bt2020nc".to_string(), "tv".to_string()];
            (String::new(), tags, format!("{name} -> PQ for YouTube (SDR by {look_name})"))
        }
    } else if let Some(curve) = curve {
        let name = curve_name(curve);
        let (cube, matrix, primaries, transfer, label) = if nbcu {
            let composed = out_dir.join(format!(".{stem}.nbcu.cube"));
            gpu_lut::compose_nbcu(&curve_lut(curve)?, &lut(NBCU_HLG)?, &composed)?;
            _temp_lut = Some(TempFile(composed.clone()));
            if uw.is_some() {
                let after = out_dir.join(format!(".{stem}.hlg-nbcu.cube"));
                gpu_lut::compose_nbcu_after_hlg(&lut(NBCU_HLG)?, &after)?;
                _uw_temp = Some(TempFile(after.clone()));
                uw_after = Some(Some(after));
            }
            (composed, "bt709", "bt709", "bt709", format!("{name} -> Rec.709 (NBCU)"))
        } else if maker_sdr {
            (lut("dji-dlogm-rec709.cube")?, "bt709", "bt709", "bt709", format!("{name} -> Rec.709 (DJI's LUT)"))
        } else {
            let no_sdr = if opts.sdr { " (no Rec.709 LUT for it)" } else { "" };
            if uw.is_some() { uw_after = Some(None); }
            (curve_lut(curve)?, "bt2020", "bt2020", "arib-std-b67", format!("{name} -> HLG{no_sdr}"))
        };
        let label = match &uw { Some(u) => format!("{label}: {}", u.label()), None => label };
        let graph = format!("scale=in_color_matrix={in_matrix}:in_range={in_range},format=gbrp16le,lut3d=file={}:interp=tetrahedral,\
                             scale=out_color_matrix={matrix}:out_range=tv", filter_path(&cube));
        lut_job = Some((cube, matrix, graph));
        let space = if matrix == "bt2020" { "bt2020nc" } else { "bt709" };
        (String::new(), vec![primaries.to_string(), transfer.to_string(), space.to_string(), "tv".to_string()], label)
    } else if let Some((cube, name)) = (in_sdr || nbcu).then(|| match info.transfer.as_deref() {
        Some("arib-std-b67") => Some((NBCU_HLG, "HLG")),
        Some("smpte2084") => Some((NBCU_PQ, "PQ")),
        _ => None,
    }).flatten() {
        // HDR that isn't log (a phone's, a camera's HLG mode, an export): NBCU's HLG or PQ -> SDR on its own
        let cube = lut(cube)?;
        let graph = format!("scale=in_color_matrix={in_matrix}:in_range={in_range},format=gbrp16le,lut3d=file={}:interp=tetrahedral,\
                             scale=out_color_matrix=bt709:out_range=tv", filter_path(&cube));
        lut_job = Some((cube, "bt709", graph));
        (String::new(), vec!["bt709".to_string(), "bt709".to_string(), "bt709".to_string(), "tv".to_string()], format!("{name} -> Rec.709 (NBCU)"))
    } else {
        let tags = vec![
            info.primaries.clone().unwrap_or_else(|| "bt709".into()),
            info.transfer.clone().unwrap_or_else(|| "bt709".into()),
            info.matrix.clone().unwrap_or_else(|| "bt709".into()),
            in_range.to_string(),
        ];
        (String::new(), tags, "colors as recorded".to_string())
    };

    // Dolby Vision 8.4: HLG HEVC from VideoToolbox, our RPUs in place of its own
    let dv_rpus = match dv_rpu {
        None => None,
        Some(file) => {
            if !cfg!(target_os = "macos") || prores || !(tags[1] == "arib-std-b67" || tags[1] == "smpte2084") {
                return Err("--dv-rpu: a Dolby Vision export is HLG (8.4) or PQ (8.1) in HEVC, from VideoToolbox (macOS)".into());
            }
            Some(dv_mux::read_rpu_file(file)?)
        }
    };

    let fps = info.fps.0 as f64 / info.fps.1 as f64;
    let from = opt_from.unwrap_or(0.0).max(0.0);
    let to = opt_to.unwrap_or(info.duration).min(info.duration.max(from));
    // A still: the frame at --at, else the middle
    let start = match preview {
        Some(_) => opts.at.unwrap_or((from + to) / 2.0).clamp(0.0, (info.duration - 1.0 / fps).max(0.0)),
        None => from,
    };
    // With -ss before -i, ffmpeg starts at the first frame at or after `start`
    let first_frame = (start * fps - 1e-6).ceil().max(0.0) as u64;
    // Where the decoder seeks: a quarter of a frame before that frame. ffmpeg's raw output keeps a constant frame
    // rate, and a first frame much after the seek point (half a frame) has it repeat one to fill the gap: every
    // frame after it then got the motion data of the one before, and the stabilized picture shook (its roll most)
    let seek = (first_frame as f64 - 0.25).max(0.0) / fps;
    // The audio from the first frame's time, with it
    let video_start = first_frame as f64 / fps;
    // A part at a lower frame rate: every `step`th frame from the first, the others not even rendered
    let step = part.map_or(1, |p| frame_step(fps, p.max_fps));
    let out_rate = (info.fps.0, info.fps.1 * step);
    let total_frames = if preview.is_some() { 1 }
        else { (if opt_from.is_some() || opt_to.is_some() { ((to - from) * fps).round().max(1.0) as u64 } else { info.frames.max(1) }).div_ceil(step) };

    // The engine, as the plugin sets it up for mpv: 10-bit YUV 4:2:0, the project's output size. Color only: none,
    // the frames go to the LUT as they are decoded
    let (w, h) = (info.width & !1, info.height & !1);
    let workers = 3;
    let engine = match &source {
        // All of the motion data before it starts (look-ahead 0): an export has the time
        Some(source) => Some(Arc::new(Engine::load(&source.project, source.preset.as_deref(), opts.fov, opts.horizon_lock, opts.underwater, opts.level, 0.0, (w, h), 3, (1, 1), None,
                                                   SampleKind::U16, 1023.0, true, opts.interpolation, workers,
                                                   // --alone: --from to --to's own motion, a photo's motion too
                                                   opts.alone.then(|| (opts.from.unwrap_or(from), opts.to.unwrap_or(to))), opts.alone_project, opts.frame, source.lens)?)),
        None => None,
    };
    // The edge fill, as playback has it. Frames whose stabilized picture covers the output, all of those exported, have
    // nothing to fill: off then, which saves the mask rendered for each frame (and a photo's motion its sphere) and
    // gives the same frames. A zoomed out camera (--camera) may leave edges where the view itself never does
    let camera = opts.camera.unwrap_or_default();
    let view = if opts.fov == Fov::Wide { vs_gyroflow::edges::VIEW_WIDE } else { vs_gyroflow::edges::VIEW_NORMAL };
    let last_frame = ((to * fps).ceil() as u64).saturating_sub(1).min(info.frames.max(1) - 1).max(first_frame);
    let mut edges = match &engine {
        Some(e) if e.leaves_edges_in(view, first_frame as usize..=last_frame as usize, fps, &camera) => opts.edges,
        _ => EdgeMode::Off,
    };
    // A photo's motion, a few seconds: each of its frames' masks says whether it leaves edges, where the clip can't
    // tell (no project, a turned camera)
    if let (Some(e), Some(_), true) = (&engine, part, edges != EdgeMode::Off) {
        e.edges.configure(edges, opts.edge_settings, view, opts.level, None);
        e.edges.set_camera(camera);
        if !frames_leave_edges(e, (first_frame..=last_frame).step_by(step as usize), fps, !info.full_range)? { edges = EdgeMode::Off; }
    }
    if let Some(e) = &engine {
        e.edges.configure(edges, opts.edge_settings, view, opts.level, None);
        e.edges.set_camera(camera);
    }
    // The frames as decoded, as the sensor has them (-noautorotate): the source's size for the engine (which turns them
    // itself, Engine::load), else turned upright as the file says and turned and cut by --rotation and --aspect
    let ((dw, dh), turn) = decoded_frame(engine.is_some(), &path, (w, h), opts);
    // Zoomed out, rendered that much bigger, as a photo is
    // A photo's part: rendered at the size it's saved at (--crop's part of it at most `max_side`), not rendered
    // bigger and scaled down after
    let (ow, oh) = engine.as_ref().map_or((dw, dh), |e| {
        let s = camera.zoom.clamp(1.0, 2.0);
        let (rw, rh) = (e.render_size.0 as f64 * s, e.render_size.1 as f64 * s);
        let [_, _, cw, ch] = opts.crop.unwrap_or([0.0, 0.0, 1.0, 1.0]);
        let k = part.map_or(1.0, |p| (p.max_side as f64 / (cw * rw).max(ch * rh)).min(1.0));
        (((rw * k).round() as usize) & !1, ((rh * k).round() as usize) & !1)
    });
    // --crop's part of it, on even pixels, and a part's size at most: cut and scaled by the encoder's ffmpeg
    let (crop, (fw, fh)) = match (opts.crop, part.map(|p| p.max_side)) {
        (None, None) => (None, (ow, oh)),
        (c, max) => {
            let [cx, cy, cw, ch] = c.unwrap_or([0.0, 0.0, 1.0, 1.0]);
            let even = |v: f64| (v.round() as usize) & !1;
            let (x0, y0) = (even(cx * ow as f64).min(ow - 2), even(cy * oh as f64).min(oh - 2));
            let (cw, ch) = (even(cw * ow as f64).clamp(2, ow - x0), even(ch * oh as f64).clamp(2, oh - y0));
            let k = max.map_or(1.0, |m| (m as f64 / cw.max(ch) as f64).min(1.0));
            (Some((x0, y0, cw, ch)), (even(cw as f64 * k).max(2), even(ch as f64 * k).max(2)))
        }
    };
    // --resolution: the short side at most that (a preview has its own size)
    let (fw, fh) = if preview.is_none() { capped((fw, fh), opts.resolution) } else { (fw, fh) };

    // The player's shaders over the stabilized frame (Post): the denoise takes frames on both sides of each one, so the
    // export decodes some before its first (as many again to settle its measure of the noise) and after its last
    let denoise = if preview.is_none() { opts.denoise } else { 0 };
    let pre_roll = if denoise > 0 { 2 * denoise as u64 + 10 } else { 0 };
    let dec_first = first_frame.saturating_sub(pre_roll);
    let dec_last = (last_frame + denoise as u64).min(info.frames.max(1) - 1);
    let post = match &engine {
        Some(e) if preview.is_none() => Post::new(e, opts, &info, (ow, oh), fps, (dec_first, dec_last), view, camera, denoise),
        _ => None,
    };
    // With the denoise, every frame goes through it (its frames are each other's neighbours), a part at a lower frame
    // rate too; and the frames before and after the export's
    let all_frames = post.as_ref().is_some_and(|p| p.radius > 0);
    let (dec_first, dec_last) = if all_frames { (dec_first, dec_last) } else { (first_frame, last_frame) };

    let mut graph = graph;
    // The look's LUT, for the GPU path's look shader
    let look_cube = lut_job.as_ref().map(|(c, _, _)| c.clone());
    let gpu_lut = match lut_job {
        None => None,
        Some((cube, matrix, cpu_graph)) => {
            let out_matrix = gpu_lut::matrix_coefficients(matrix).unwrap();
            // Each frame's L1: a Dolby Vision export's HLG, measured in the same pass
            // For YouTube, its MaxCLL and MaxFALL
            let measure = if youtube.is_some() { gpu_lut::Measure::PqNits }
                else if opts.dv_live_l1 && dv_rpus.is_some() && tags[1] == "arib-std-b67" { gpu_lut::Measure::HlgL1 }
                else { gpu_lut::Measure::None };
            let made = match &uw_after {
                Some(after) => GpuLut::with_look(after.as_deref(), true, ow, oh, gpu_lut::matrix_coefficients(in_matrix).unwrap(), info.full_range, out_matrix, measure),
                None => GpuLut::new(&cube, ow, oh, gpu_lut::matrix_coefficients(in_matrix).unwrap(), info.full_range, out_matrix, measure),
            };
            match made {
                Ok(l) => Some(l),
                Err(e) => {
                    eprintln!("  LUT on the CPU (slower): {e}");
                    // The underwater colour's numbers can't change by the frame there: their median throughout
                    if uw.as_ref().is_some_and(|u| !u.fixed) { eprintln!("  the underwater colour: one set of numbers for the whole export"); }
                    uw_after = None;
                    graph = cpu_graph;
                    None
                }
            }
        }
    };

    // Each frame's underwater colour numbers for the pipes' GPU LUT, by frame index from the first one decoded
    let uw_frames: Option<Arc<(u64, Vec<[f32; 6]>)>> = match (&uw, &uw_after, &gpu_lut) {
        (Some(u), Some(_), Some(_)) => {
            let base = dec_first.min(first_frame);
            let times: Vec<f64> = (base..=dec_last.max(last_frame)).map(|i| i as f64 / fps).collect();
            Some(Arc::new((base, u.params_at(&times))))
        }
        _ => None,
    };
    let uw_of = |f: &(u64, Vec<[f32; 6]>), index: u64| f.1[(index.saturating_sub(f.0) as usize).min(f.1.len() - 1)];

    let duration_label = { let t = (to - from).max(0.0) as u64; format!("{}:{:02}", t / 60, t % 60) };
    let mut source_label = source.as_ref().map_or("color only", |s| s.label).to_string();
    match (opts.fov, engine.as_ref().and_then(|e| e.wide_fov)) {
        (_, Some(fov)) => source_label += &format!(", wider view (FOV {fov:.2})"),
        (Fov::Set(fov), None) if source.is_some() => source_label += &format!(", FOV {fov}"),
        _ => {}
    }
    if edges != EdgeMode::Off { source_label += &format!(", edges {}", format!("{edges:?}").to_lowercase()); }
    if let Some(p) = &post { source_label += &format!(", {}", p.label); }
    eprintln!("{}: {color_label} · {source_label} · {}x{} -> {fw}x{fh} · {duration_label}", file_name(file), w, h);

    if stopped() { return Err("cancelled".into()); }
    // A preview's frame goes to ffmpeg as an export's would, then to the preview's encoder (write_preview)
    let enc = if preview.is_some() { encoder("", false) } else { pick_encoder(opts, prores)? };
    if !cfg!(target_os = "macos") && preview.is_none() { eprintln!("  encoder: {}", enc.name); }
    if dv_rpus.is_some() && enc.name != "hevc_videotoolbox" { return Err(format!("--dv-rpu: needs hevc_videotoolbox, not {}", enc.name)); }

    // The GPU path (docs/design/export-gpu-path.md): one ffmpeg, the frames on the GPU from the decoder to the encoder, the
    // player's own shaders. Else the pipes below, saying why
    if preview.is_none() && part.is_none() && opts.gpu != Some(false) {
        let why: Result<GpuPlan, String> = (|| {
            if youtube.is_some() { return Err("the YouTube export".to_string()); }
            if uw.as_ref().is_some_and(|u| !u.fixed) { return Err("the underwater colour's numbers change by the frame".to_string()); }
            // Asked for by name: only the GPU path draws it
            if opts.with_translation { gpu_setup(&path).map_err(|e| format!("--with-translation needs the GPU path: {e}"))?; }
            if opts.crop.is_some() { return Err("--crop".to_string()); }
            if dv_rpus.is_some() && opts.dv_live_l1 { return Err("each frame's Dolby Vision L1 is measured on the pipes' GPU LUT".to_string()); }
            if engine.is_none() && look_cube.is_none() && turn.is_empty() { return Err("nothing for the GPU to change".to_string()); }
            let setup = gpu_setup(&path)?;
            // Without the warp, the turn and the cut (--rotation, --aspect, the file's own) on the GPU too: VideoToolbox's
            // transpose before the filter, the filter's crop
            let (pre, crop) = if engine.is_none() { gpu_turn(&opts.frame.for_video(display_rotation(&path), None), (w, h)) } else { (String::new(), String::new()) };
            let first_decoded = if denoise > 0 { dec_first } else { first_frame };
            let mut text = String::new();
            if let Some(e) = &engine {
                let interp = opts.gpu_interp.unwrap_or(if fps <= PLAYER_SLOW_MAX_FPS { 3 } else { 5 });
                // The translation's cache key: the player's name for the clip, its project, else the video
                let key = source.as_ref().map_or(path.to_string_lossy().to_string(), |s| s.project.clone());
                text = warp_shader(e, opts, &key, view, camera, fps, first_decoded, edges, interp, denoise)?.0;
            }
            if let Some(cube) = &look_cube { text += "\n"; text += &vs_gyroflow::gpu_table::look_shader(cube)?; }
            let npass = vs_gyroflow::gpu_table::passes_per_frame(&text, 1);
            let text = if engine.is_some() { vs_gyroflow::gpu_table::for_libplacebo(&text, npass)? } else { text };
            // The denoise up to the clip's end: its last frame again, as the frames after it (the player shows the last
            // frames with fewer after them: only those differ)
            let short = (last_frame + denoise as u64).saturating_sub(dec_last);
            let pad = (denoise > 0 && short > 0).then_some((dec_last - dec_first, short));
            Ok(GpuPlan { setup, shader: text, pre, crop, first_decoded, skip: if denoise > 0 { first_frame + denoise as u64 - dec_first } else { 0 }, pad })
        })();
        match why {
            Ok(plan) => {
                // --resolution: the warp renders at that size (it's its own scaler: warp.glsl's RT is the output's)
                let job = GpuJob { path: &path, info: &info, opts, tags: &tags, enc: &enc, prores, size: (fw, fh), fps, out_rate,
                                   seek: if denoise > 0 { (dec_first as f64 - 0.25).max(0.0) / fps } else { seek },
                                   until: if denoise > 0 { Some((dec_last as f64 + 0.5) / fps) } else { opt_to.map(|_| to) },
                                   from_set: opt_from.is_some() || denoise > 0, video_start, to, opt_from, opt_to, total_frames, first_frame,
                                   tmp_path: &tmp_path, out_path: &out_path, in_range, dv_rpus, dv_rpu, started: Instant::now() };
                return export_gpu(job, plan);
            }
            Err(why) if opts.gpu == Some(true) || opts.with_translation => return Err(format!("--gpu on: {why}")),
            Err(why) => eprintln!("  through the pipes, not the GPU path: {why}"),
        }
    }

    // Decoder: raw 10-bit planar frames on stdout. Elsewhere than on macOS, whatever hardware decoder ffmpeg finds,
    // else the CPU
    let hwaccel = if cfg!(target_os = "macos") { Some("videotoolbox") } else if hwaccel_works(&path) { Some("auto") } else { None };
    let mut dec_cmd = ffmpeg_command("ffmpeg");
    dec_cmd.args(["-nostdin", "-v", "error"]);
    if let Some(h) = hwaccel { dec_cmd.args(["-hwaccel", h]); }
    if all_frames {
        // From the frames before the first, to those after the last
        if dec_first > 0 { dec_cmd.args(["-ss", &format!("{:.6}", (dec_first as f64 - 0.25).max(0.0) / fps)]); }
        if opt_to.is_some() { dec_cmd.args(["-to", &format!("{:.6}", (dec_last as f64 + 0.5) / fps)]); }
    } else {
        if preview.is_some() || opt_from.is_some() { dec_cmd.args(["-ss", &format!("{seek:.6}")]); }
        if preview.is_none() && opt_to.is_some() { dec_cmd.args(["-to", &format!("{to:.6}")]); }
    }
    dec_cmd.args(["-noautorotate", "-i"]).arg(&path).args(["-map", "0:v:0"]);
    if preview.is_some() { dec_cmd.args(["-frames:v", "1"]); }
    dec_cmd.args(["-vf", &format!("scale={w}:{h}{}{turn}", if turn.is_empty() { "" } else { "," }), "-f", "rawvideo", "-pix_fmt", "yuv420p10le", "-"])
        .stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped());
    let mut decoder = dec_cmd.spawn().map_err(|e| format!("ffmpeg: {e}"))?;

    // Encoder: the stabilized frames, audio and metadata from the source
    // A photo's motion: the first audio track alone (a Motion Photo has one at most; DJI's cameras record two)
    let audio = if part.is_some() { "1:a:0?" } else { "1:a?" };
    let mut bit_rate = export_bit_rate(info.bit_rate, w * h, fw * fh, opts.encode_quality);
    // A photo's motion: no more than a phone gives its own
    if let Some(p) = part { bit_rate = bit_rate.min(p.bits_per_pixel * (fw * fh) as f64 * fps / step as f64); }
    let mut enc_cmd = ffmpeg_command("ffmpeg");
    enc_cmd.args(["-nostdin", "-v", "error", "-y"]).args(&enc.global);
    enc_cmd.args([
                  "-f", "rawvideo", "-pix_fmt", "yuv420p10le", "-s", &format!("{ow}x{oh}"),
                  "-framerate", &format!("{}/{}", out_rate.0, out_rate.1),
                  "-color_range", in_range, "-colorspace", "bt709", "-color_primaries", "bt709", "-color_trc", "bt709",
                  "-i", "-"]);
    // The audio and metadata from the source; for Dolby Vision, the muxer takes them; a preview has neither
    let mut preview_files = None;
    if preview.is_some() {
    } else if dv_rpus.is_none() {
        if opt_from.is_some() { enc_cmd.args(["-ss", &format!("{video_start:.6}")]); }
        if opt_to.is_some() { enc_cmd.args(["-to", &format!("{to:.6}")]); }
        enc_cmd.arg("-i").arg(&path)
            .args(["-map", "0:v", "-map", audio, "-map_metadata", "1", "-c:a", "copy"]);
    } else {
        enc_cmd.args(["-map", "0:v"]);
    }
    let pix_fmt = enc.pix_fmt;
    // The frames carry the input's BT.709 tags through the filters, and the encoder writes the frames' tags, not
    // the -color_* options, so the target's are set on the frames
    let setparams = format!("setparams=color_primaries={}:color_trc={}:colorspace={}:range={}", tags[0], tags[1], tags[2], tags[3]);
    // --crop's part, and the size it's saved at (a photo's motion, --resolution): by --interpolation's filter (Lanczos
    // by default)
    let cut = match crop {
        Some((x, y, cw, ch)) => {
            let scale = scale_filter((cw, ch), (fw, fh), opts.interpolation);
            format!("crop={cw}:{ch}:{x}:{y}{}{scale},", if scale.is_empty() { "" } else { "," })
        }
        None => { let s = scale_filter((ow, oh), (fw, fh), opts.interpolation); if s.is_empty() { s } else { s + "," } }
    };
    let graph = if graph.is_empty() { format!("{cut}format={pix_fmt},{setparams}") } else { format!("{cut}{graph},format={pix_fmt},{setparams}") };
    let color = ["-color_primaries", &tags[0], "-color_trc", &tags[1], "-colorspace", &tags[2], "-color_range", &tags[3]];
    if preview.is_some() {
        // The frame as an export has it, then at the preview's size in 8 bits: as YUV for its video (write_preview),
        // and as a JPEG for its cover art (BT.601 and full range, as JPEGs are read)
        let size = preview_size(fw, fh);
        let files = PreviewFiles { frame: TempFile(out_dir.join(format!(".{stem}.frame.yuv"))),
                                   cover: TempFile(out_dir.join(format!(".{stem}.cover.jpg"))), size };
        enc_cmd.args(["-filter_complex", &format!("[0:v]{graph},scale={}:{}:flags=lanczos,split[a][b];[a]format=yuv420p[f];\
                                                   [b]scale=out_color_matrix=bt601:out_range=pc,format=yuvj420p[c]", size.0, size.1),
                      "-map", "[f]", "-frames:v", "1", "-f", "rawvideo"]).arg(&files.frame.0)
            .args(["-map", "[c]", "-frames:v", "1", "-q:v", "5", "-update", "1", "-f", "image2"]).arg(&files.cover.0)
            .stdout(Stdio::null());
        preview_files = Some(files);
    } else {
        enc_cmd.args(["-vf", &format!("{graph}{}", enc.upload)]);
        enc_cmd.args(["-c:v", &enc.name]).args(&enc.args);
        if !prores { enc_cmd.args(encode_mode_args(&enc.name, opts.encode_quality, false)); }
        if prores {
            enc_cmd.args(["-tag:v", "apch"]);
        } else if enc.name == "ffv1" {
            // Lossless, for comparing pictures (docs/design/export-gpu-path.md)
            enc_cmd.args(["-level", "3"]);
        } else {
            enc_cmd.args(["-tag:v", "hvc1", "-b:v", &format!("{}", bit_rate as u64)]);
        }
        enc_cmd.args(color);
    }
    // With a link to write: the moov first with room after it (-moov_size, as for Dolby Vision), which the link
    // then takes in place; else +faststart. Room for every frame's entries in each track (B-frames' ctts, the
    // audio), as the Dolby Vision export reserves it
    let moov_size = 512 * 1024 + total_frames * 64;
    if preview.is_some() {
    } else if dv_rpus.is_none() && opts.link.is_some() && part.is_none() && youtube.is_none() {
        enc_cmd.args(["-moov_size", &moov_size.to_string(), "-f", "mov"]).arg(&tmp_path).stdout(Stdio::null());
    } else if dv_rpus.is_none() {
        enc_cmd.args(["-movflags", "+faststart", "-f", if part.is_some_and(|p| p.mp4) { "mp4" } else { "mov" }]).arg(&tmp_path).stdout(Stdio::null());
    } else {
        // No B-frames: the access units come in display order, the RPUs with them (VideoToolbox writes none anyway)
        enc_cmd.args(["-bf", "0", "-f", "hevc", "-"]).stdout(Stdio::piped());
    }
    enc_cmd.stdin(Stdio::piped()).stderr(Stdio::piped());
    let mut encoder = enc_cmd.spawn().map_err(|e| format!("ffmpeg: {e}"))?;

    // Dolby Vision: the encoder's HEVC, its RPUs replaced with ours (rewriter), into a MOV with the source's audio and
    // metadata (muxer); -moov_size puts the moov first with room after it, which the dvvC box then takes in place
    let mut muxer = None;
    let mut rewriter = None;
    // --dv-live-l1: each frame's L1, from the GPU workers through the writer (in frame order) to the RPU rewriter
    let live_l1 = opts.dv_live_l1 && dv_rpus.is_some() && gpu_lut.is_some();
    if opts.dv_live_l1 && dv_rpus.is_some() && !live_l1 {
        eprintln!("  Dolby Vision: the RPU file's L1 (each frame's is measured with the LUT on the GPU)");
    }
    let (l1_tx, l1_rx) = mpsc::channel::<Option<[i32; 3]>>();
    let mut l1_tx = live_l1.then_some(l1_tx);
    if let Some(rpus) = dv_rpus {
        let mut mux_cmd = ffmpeg_command("ffmpeg");
        // -r, not the demuxer's -framerate: raw HEVC has no timestamps, and with -framerate they come out wrong
        mux_cmd.args(["-nostdin", "-v", "error", "-y", "-f", "hevc", "-r", &format!("{}/{}", out_rate.0, out_rate.1), "-i", "-"]);
        if opt_from.is_some() { mux_cmd.args(["-ss", &format!("{video_start:.6}")]); }
        if opt_to.is_some() { mux_cmd.args(["-to", &format!("{to:.6}")]); }
        mux_cmd.arg("-i").arg(&path)
            .args(["-map", "0:v", "-map", audio, "-map_metadata", "1", "-c", "copy", "-tag:v", "hvc1"]).args(color)
            .args(["-moov_size", &moov_size.to_string(), "-f", "mov"]).arg(&tmp_path)
            .stdin(Stdio::piped()).stdout(Stdio::null()).stderr(Stdio::piped());
        let mut mux = mux_cmd.spawn().map_err(|e| format!("ffmpeg: {e}"))?;
        let mut mux_in = std::io::BufWriter::with_capacity(1 << 20, mux.stdin.take().unwrap());
        let mut enc_out = encoder.stdout.take().unwrap();
        rewriter = Some(std::thread::spawn(move || -> Result<u64, String> {
            let mut injector = RpuInjector::new(rpus, first_frame).step(step);
            if live_l1 { injector = injector.live_l1(l1_rx, fps); }
            let mut buf = vec![0u8; 1 << 20];
            loop {
                let n = enc_out.read(&mut buf).map_err(|e| format!("reading the encoder: {e}"))?;
                if n == 0 { break; }
                injector.feed(&buf[..n], &mut mux_in).map_err(|e| format!("muxer stopped: {e}"))?;
            }
            injector.finish(&mut mux_in).and_then(|_| mux_in.flush()).map_err(|e| format!("muxer stopped: {e}"))?;
            Ok(injector.frames)
        }));
        muxer = Some(mux);
    }

    let in_frame = dw * dh * 3; // 4:2:0, 2 bytes per sample
    let out_frame = ow * oh * 3;
    let plane_dims = |(w, h): (usize, usize)| [(w, h), (w / 2, h / 2), (w / 2, h / 2)];

    // Reader -> GPU workers -> writer, in order. A few frames in flight keep the GPU and both ffmpegs busy
    let in_flight = workers + 2;
    let (frame_tx, frame_rx) = mpsc::sync_channel::<(u64, Vec<u8>)>(in_flight);
    let (done_tx, done_rx) = mpsc::sync_channel::<(u64, Result<(Vec<u8>, Option<[f64; 3]>), String>)>(in_flight);
    let mut dec_out = decoder.stdout.take().unwrap();
    let reader = std::thread::spawn(move || -> Result<u64, String> {
        // n: the frames written; `read`, decoded (every `step`th of them is written)
        let (mut n, mut read) = (0, 0u64);
        loop {
            let mut buf = vec![0u8; in_frame];
            match dec_out.read_exact(&mut buf) {
                Ok(()) => {}
                Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => return Ok(n),
                Err(e) => return Err(format!("reading decoded frames: {e}")),
            }
            read += 1;
            if !all_frames && (read - 1) % step != 0 { continue; }
            if frame_tx.send((n, buf)).is_err() { return Ok(n); }
            n += 1;
        }
    });
    let frame_rx = Arc::new(Mutex::new(frame_rx));
    // With the player's shaders, the LUT after them (the writer's, below); the fringes' marker in each frame
    let marker = post.as_ref().is_some_and(|p| p.chromatic);
    let gpu_threads: Vec<_> = (0..workers).map(|_| {
        let (frame_rx, done_tx, engine) = (frame_rx.clone(), done_tx.clone(), engine.clone());
        let full_range = info.full_range;
        let mut lut = if post.is_some() { None } else { gpu_lut.as_ref().map(|l| l.worker()) };
        let uw_frames = uw_frames.clone();
        std::thread::spawn(move || {
            loop {
                let Ok((n, src)) = frame_rx.lock().recv() else { break };
                // The underwater colour's numbers for this frame
                if let (Some(l), Some(f)) = (lut.as_mut(), &uw_frames) {
                    l.set_uw(uw_of(f, if all_frames { dec_first + n } else { first_frame + n * step }));
                }
                let frame = match &engine {
                    None => Ok(src),
                    Some(engine) => {
                        let mut dst = vec![0u8; out_frame];
                        // Frame times from the start of the file, as Gyroflow and mpv count them
                        let index = if all_frames { dec_first + n } else { first_frame + n * step };
                        let timestamp_us = ((index as f64 / fps) * 1_000_000.0).round() as i64 + engine.frame_offset_us;
                        let mut planes = Vec::with_capacity(3);
                        let (mut so, mut doff) = (0, 0);
                        for ((sw, sh), (dw, dh)) in plane_dims((w, h)).into_iter().zip(plane_dims((ow, oh))) {
                            planes.push(PlaneJob {
                                src: src[so..].as_ptr(), src_stride: sw * 2, src_w: sw, src_h: sh,
                                dst: dst[doff..].as_mut_ptr(), dst_stride: dw * 2, dst_w: dw, dst_h: dh,
                            });
                            so += sw * sh * 2;
                            doff += dw * dh * 2;
                        }
                        let t = Instant::now();
                        let r = engine.process(timestamp_us, planes, !full_range, marker).map(|_| dst);
                        ENGINE_NS.fetch_add(t.elapsed().as_nanos() as u64, Relaxed);
                        r
                    }
                };
                let t = Instant::now();
                let result = frame.and_then(|mut f| match lut.as_mut() { Some(l) => l.apply(&mut f).map(|l1| (f, l1)), None => Ok((f, None)) });
                LUT_NS.fetch_add(t.elapsed().as_nanos() as u64, Relaxed);
                if done_tx.send((n, result)).is_err() { break; }
            }
        })
    }).collect();
    drop(done_tx);

    // The writer: each frame as it comes, in order; or through the player's shaders on a thread of their own (with the
    // denoise a frame waits for those after it), then the LUT
    let written = Arc::new(std::sync::atomic::AtomicU64::new(0));
    let mut direct = None;
    let mut post_tx = None;
    let mut post_thread = None;
    let enc_in = encoder.stdin.take().unwrap();
    match post {
        None => direct = Some(enc_in),
        Some(p) => {
            let (tx, rx) = mpsc::sync_channel::<Vec<u8>>(2);
            post_tx = Some(tx);
            let mut lut = gpu_lut.as_ref().map(|l| l.worker());
            let l1_tx = l1_tx.take();
            let written = written.clone();
            let uw_frames = uw_frames.clone();
            let first = if all_frames { dec_first } else { first_frame };
            post_thread = Some(std::thread::spawn(move || -> Result<(), String> {
                // The LUT and the encoder on a thread of their own, beside the shaders
                let (out_tx, out_rx) = mpsc::sync_channel::<Vec<u8>>(2);
                let writer = std::thread::spawn(move || -> Result<(), String> {
                    let mut enc_in = enc_in;
                    for (k, mut f) in out_rx.into_iter().enumerate() {
                        // The export's own frames, every `step`th from the first (emit below)
                        if let (Some(l), Some(u)) = (lut.as_mut(), &uw_frames) { l.set_uw(uw_of(u, first_frame + k as u64 * step)); }
                        let l1 = match lut.as_mut() { Some(l) => l.apply(&mut f)?, None => None };
                        if let Some(tx) = &l1_tx { let _ = tx.send(l1.map(|v| v.map(|x| x.round() as i32))); }
                        enc_in.write_all(&f).map_err(|e| format!("encoder stopped: {e}"))?;
                        written.fetch_add(1, Relaxed);
                    }
                    Ok(())
                });
                let mut emit = |index: u64, f: Vec<u8>| -> Result<(), String> {
                    // Only the export's own frames (not those around them), every `step`th
                    if all_frames && (index < first_frame || index > last_frame || (index - first_frame) % step != 0) { return Ok(()); }
                    out_tx.send(f).map_err(|_| "the writer stopped".to_string())
                };
                let done = p.run(first, rx, &mut emit);
                drop(out_tx);
                // The writer's own error says more
                writer.join().map_err(|_| "the writer failed".to_string())??;
                done
            }));
        }
    }
    let mut pending = BTreeMap::new();
    let mut next = 0u64;
    let started = Instant::now();
    let mut last_report = Instant::now();
    let (mut reported, mut rate) = (0u64, 0.0);
    let mut background = None;
    let mut error = None;
    let mut last_control = Instant::now();
    // For YouTube: the brightest pixel's max(R,G,B) and the brightest frame's average of it, in nits; frames measured
    let (mut max_cll, mut max_fall, mut lit_frames) = (0.0f64, 0.0f64, 0u64);
    for (n, result) in done_rx {
        if let Some(control) = &opts.control {
            if last_control.elapsed().as_millis() >= 250 { read_control(control); last_control = Instant::now(); }
        }
        if stopped() { error = Some("cancelled".to_string()); break; }
        let want = BACKGROUND.load(Relaxed);
        if want && !cfg!(target_os = "macos") { std::thread::sleep(THROTTLE); }
        if background != Some(want) {
            let mut pids = vec![0, decoder.id(), encoder.id()];
            if let Some(m) = &muxer { pids.push(m.id()); }
            set_background(&pids, want);
            background = Some(want);
        }
        match result {
            Ok(buf) => { pending.insert(n, buf); }
            Err(e) => { error = Some(format!("frame {n}: {e}")); break; }
        }
        while let Some((buf, l1)) = pending.remove(&next) {
            // For YouTube: the brightest frame and the average, from the frames' measured L1 (MaxCLL, MaxFALL)
            if let Some(v) = l1.filter(|_| youtube.is_some()) { max_cll = max_cll.max(v[2]); max_fall = max_fall.max(v[1]); lit_frames += 1; }
            if let Some(enc_in) = direct.as_mut() {
                // The frame's L1 before the frame: the rewriter waits for it when the encoder hands the frame back
                if let Some(tx) = &l1_tx { let _ = tx.send(l1.map(|v| v.map(|x| x.round() as i32))); }
                if let Err(e) = enc_in.write_all(&buf) { error = Some(format!("encoder stopped: {e}")); break; }
                written.fetch_add(1, Relaxed);
            } else if let Some(tx) = &post_tx {
                if tx.send(buf).is_err() { error = Some("the player's shaders stopped".to_string()); break; }
            }
            next += 1;
        }
        if error.is_some() { break; }
        if last_report.elapsed().as_millis() >= 500 {
            // The recent rate, not the average: it changes with the priority
            let done = written.load(Relaxed);
            let now = done.saturating_sub(reported) as f64 / last_report.elapsed().as_secs_f64();
            rate = if reported == 0 { now } else { rate * 0.7 + now * 0.3 };
            last_report = Instant::now();
            reported = done;
            let done = done as f64;
            let pct = (done / total_frames as f64 * 100.0).min(100.0);
            let eta = ((total_frames as f64 - done).max(0.0) / rate.max(1e-3)).min(359_999.0) as u64;
            if part.is_none() { eprint!("\r  {pct:3.0}%  {rate:4.0} fps  ETA {}:{:02}   ", eta / 60, eta % 60); }
            opts.progress.write(serde_json::json!({
                "pid": std::process::id(), "done": done as u64, "total": total_frames, "fps": rate, "eta": eta,
                "background": want, "partial": tmp_path,
            }));
        }
    }
    drop(direct);
    // The shaders' last frames (the denoise's, with the frames there are after them), then the encoder's end
    drop(post_tx);
    drop(l1_tx.take()); // the rewriter's last frames, if any are left, keep the file's L1
    if error.is_some() || stopped() {
        let _ = decoder.kill();
    }
    // Cancelled: no point in the encoder finishing the file (the decoder may have stopped first, on Ctrl+C)
    if stopped() {
        let _ = encoder.kill();
        if let Some(m) = muxer.as_mut() { let _ = m.kill(); }
    }
    let shaded = post_thread.is_some();
    if let Some(t) = post_thread {
        // Its own error says more than the writer's "stopped"
        if let Err(e) = t.join().map_err(|_| "the player's shaders failed".to_string()).and_then(|r| r) { error = Some(e); }
    }
    for t in gpu_threads { let _ = t.join(); }
    // The frames written: with the player's shaders, those of the export alone (not those decoded around them)
    let read = reader.join().map_err(|_| "reader panicked".to_string())?.map(|n| if shaded { written.load(Relaxed) } else { n });
    let dec_status = decoder.wait().map_err(|e| e.to_string())?;
    let enc_status = encoder.wait().map_err(|e| e.to_string())?;
    let rewritten = rewriter.map(|r| r.join().map_err(|_| "the RPU rewriter panicked".to_string()));
    let mux_status = match muxer.as_mut() { Some(m) => Some(m.wait().map_err(|e| e.to_string())?), None => None };
    let stderr_of = |child: Option<std::process::ChildStderr>| {
        let mut s = String::new();
        if let Some(mut e) = child { let _ = e.read_to_string(&mut s); }
        s.trim().to_string()
    };
    let dec_err = stderr_of(decoder.stderr.take());
    let enc_err = stderr_of(encoder.stderr.take());
    let mux_err = stderr_of(muxer.as_mut().and_then(|m| m.stderr.take()));
    if part.is_none() { eprint!("\r{:60}\r", ""); }

    let result = (|| {
        if stopped() { return Err("cancelled".to_string()); }
        if let Some(e) = error { return Err(e); }
        let read = read?;
        if !dec_status.success() { return Err(format!("decoding: {dec_err}")); }
        if !enc_status.success() { return Err(format!("encoding: {enc_err}")); }
        if read == 0 { return Err("no frames decoded".into()); }
        if let Some(r) = rewritten {
            let frames = r??;
            if !mux_status.map_or(false, |s| s.success()) { return Err(format!("muxing: {mux_err}")); }
            if frames != read { return Err(format!("Dolby Vision: {frames} pictures for {read} frames")); }
            let (compatibility, profile) = if tags[1] == "smpte2084" { (1, "8.1") } else { (4, "8.4") };
            let level = dv_mux::patch_dvvc(&tmp_path, fps, compatibility)?;
            eprintln!("  Dolby Vision {profile}, level {level}: {} RPUs from {}", frames,
                      dv_rpu.map_or(String::new(), |p| p.display().to_string()));
        }
        Ok(read)
    })();
    // A preview: its MP4 from the frame. An export: its link, and a photo's motion its photo's place. For YouTube: the
    // MKV, with the HDR metadata and the LUT (no link: that's an MP4 key); MaxCLL/MaxFALL only when every frame was
    // measured (the LUT on the GPU)
    let light = (lit_frames > 0 && lit_frames == next).then(|| (max_cll.ceil() as u32, max_fall.ceil() as u32));
    let result = result.and_then(|read| match (&youtube_sdr, &preview_files, &opts.link) {
        (Some((cube, name, look)), _, _) => {
            youtube_mux(&tmp_path, &final_tmp, &cube.0, name, light)?;
            let _ = std::fs::remove_file(&tmp_path);
            match light {
                Some((cll, fall)) => eprintln!("  for YouTube: MaxCLL {cll}, MaxFALL {fall} nits; SDR by {look} in {name}"),
                None => eprintln!("  for YouTube: MaxCLL/MaxFALL not measured (YouTube assumes its own); SDR by {look} in {name}"),
            }
            Ok(read)
        }
        (None, Some(files), _) => write_preview(files, 1, (to - from).max(1.0 / fps), &tags, &tmp_path, opts, opts.dates.map(|d| (d, true))).map(|_| read),
        (None, None, link) => {
            let mut keys = Vec::new();
            if let Some(l) = link { keys.push((vs_gyroflow::mp4meta::LINK_KEY, l.as_str())); }
            if let Some(l) = part.and_then(|p| p.location.as_deref()) { keys.push((vs_gyroflow::mp4meta::LOCATION_KEY, l)); }
            if let Some(c) = part.and_then(|p| p.created.as_deref()) { keys.push((vs_gyroflow::mp4meta::CREATION_DATE_KEY, c)); }
            if keys.is_empty() { Ok(read) } else { vs_gyroflow::mp4meta::write_keys(&tmp_path, &keys).map(|_| read) }
        }
    });
    let frames = match result {
        Ok(f) => f,
        Err(e) => {
            let _ = std::fs::remove_file(&tmp_path);
            let _ = std::fs::remove_file(&final_tmp);
            return Err(e);
        }
    };
    let tmp_path = final_tmp;

    // The source's dates, so the clip sorts where it was shot, or --dates. On a Mac, a modification time before the
    // file's creation time moves that back too
    let modified = opts.dates.map(|d| d.system_time()).or_else(|| std::fs::metadata(&path).and_then(|m| m.modified()).ok());
    if let (Some(modified), Ok(f)) = (modified, std::fs::File::options().write(true).open(&tmp_path)) {
        let _ = f.set_modified(modified);
    }
    if out_path.exists() { copy_finder_attributes(&out_path, &tmp_path); }
    std::fs::rename(&tmp_path, &out_path).map_err(|e| e.to_string())?;
    let secs = started.elapsed().as_secs_f64();
    eprintln!("  {frames} frames in {:.0}:{:02.0} ({:.0} fps)", (secs / 60.0).floor(), secs % 60.0, frames as f64 / secs.max(1e-3));
    if std::env::var_os("AFP_TIMES").is_some() && frames > 0 {
        eprintln!("  GPU workers ({workers}): engine {:.1} ms a frame, LUT {:.1} ms a frame (summed over the workers)",
                  ENGINE_NS.load(Relaxed) as f64 / 1e6 / frames as f64, LUT_NS.load(Relaxed) as f64 / 1e6 / frames as f64);
    }
    Ok(Outcome::Done(out_path))
}

/// The export for YouTube as an MKV (src/youtube.rs): the encoder's MOV copied in, the mastering display and MaxCLL/
/// MaxFALL (`light`, when measured) as the video's (Matroska's MasteringMetadata, MaxCLL, MaxFALL), and the SDR LUT
/// attached as YouTube's hdr_metadata tool attaches it (application/x-cube, a .cube name). ffmpeg's -mastering_display
/// and -content_light are recent input options; an ffmpeg without them makes the MKV without that metadata, and says so
fn youtube_mux(mov: &Path, mkv: &Path, cube: &Path, cube_name: &str, light: Option<(u32, u32)>) -> Result<(), String> {
    let run = |hdr: bool| -> Result<std::process::Output, String> {
        let mut cmd = ffmpeg_command("ffmpeg");
        cmd.args(["-nostdin", "-v", "error", "-y"]);
        if hdr {
            cmd.args(["-mastering_display:v:0", vs_gyroflow::youtube::MASTERING_DISPLAY]);
            if let Some((cll, fall)) = light { cmd.args(["-content_light:v:0", &format!("{cll},{fall}")]); }
        }
        cmd.arg("-i").arg(mov).args(["-map", "0", "-c", "copy", "-attach"]).arg(cube)
            .args(["-metadata:s:t:0", "mimetype=application/x-cube", "-metadata:s:t:0", &format!("filename={cube_name}"), "-f", "matroska"]).arg(mkv)
            .stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::piped());
        cmd.output().map_err(|e| format!("ffmpeg: {e}"))
    };
    let out = run(true)?;
    if out.status.success() { return Ok(()); }
    let err = String::from_utf8_lossy(&out.stderr).to_string();
    if err.contains("mastering_display") || err.contains("content_light") {
        eprintln!("  this ffmpeg can't write the mastering display (-mastering_display): the MKV without it");
        let out = run(false)?;
        if out.status.success() { return Ok(()); }
        return Err(format!("the MKV: {}", String::from_utf8_lossy(&out.stderr).trim()));
    }
    Err(format!("the MKV: {}", err.trim()))
}
