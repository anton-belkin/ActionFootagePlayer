// Clip previews (--preview): a still, a flipbook.
// One of gyroflow-export's parts: main.rs has the uses and main(); each part's items are the program's.

use super::*;

// A preview's files on the way, removed with it: the frame (8-bit YUV, the preview's size) and the cover (JPEG)
pub(crate) struct PreviewFiles { pub(crate) frame: TempFile, pub(crate) cover: TempFile, pub(crate) size: (usize, usize) }

// The preview's MP4 from its frame: held for `length` seconds, the cover art its JPEG, the dates of --dates, no
// audio and none of the video's metadata; then its keys (the link, Photos' date) added in moov/meta
// `frames`: how many are in files.frame; one is held for `length` seconds, several play at 2 a second
// `dates`: the file's (--dates, or a clip's own); QuickTime's creation date only with its zone known
pub(crate) fn write_preview(files: &PreviewFiles, frames: usize, length: f64, tags: &[String], out: &Path, opts: &Options,
                 dates: Option<(Dates, bool)>) -> Result<(), String> {
    let enc = pick_h264()?;
    let (pw, ph) = files.size;
    let setparams = format!("setparams=color_primaries={}:color_trc={}:colorspace={}:range=tv", tags[0], tags[1], tags[2]);
    let mut cmd = ffmpeg_command("ffmpeg");
    cmd.args(["-nostdin", "-v", "error", "-y", "-f", "rawvideo", "-pix_fmt", "yuv420p", "-s", &format!("{pw}x{ph}"),
              "-framerate", if frames > 1 { "2" } else { "10" }, "-i"])
        .arg(&files.frame.0).arg("-i").arg(&files.cover.0);
    if frames > 1 {
        cmd.args(["-filter_complex", &format!("[0]{setparams}[v]"), "-map", "[v]", "-map", "1", "-frames:v:0", &frames.to_string()]);
    } else {
        cmd.args(["-filter_complex", &format!("[0]loop=loop=-1:size=1,{setparams}[v]"), "-map", "[v]", "-map", "1", "-t", &format!("{:.3}", length.max(0.04))]);
    }
    cmd.args(["-c:v:0", &enc.name]).args(&enc.args)
        .args(["-c:v:1", "copy", "-disposition:v:1", "attached_pic", "-map_metadata", "-1",
               "-color_primaries", &tags[0], "-color_trc", &tags[1], "-colorspace", &tags[2], "-color_range", "tv"]);
    if let Some((d, _)) = &dates { cmd.args(["-metadata", &format!("creation_time={}", d.utc())]); }
    cmd.args(["-movflags", "+faststart", "-f", "mp4"]).arg(out).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::piped());
    let result = cmd.output().map_err(|e| format!("ffmpeg: {e}"))?;
    if !result.status.success() { return Err(format!("the preview: {}", String::from_utf8_lossy(&result.stderr).trim())); }
    let quicktime = dates.filter(|(_, zoned)| *zoned).map(|(d, _)| d.quicktime());
    let mut keys = Vec::new();
    if let Some(link) = &opts.link { keys.push((vs_gyroflow::mp4meta::LINK_KEY, link.as_str())); }
    if let Some(q) = &quicktime { keys.push((vs_gyroflow::mp4meta::CREATION_DATE_KEY, q.as_str())); }
    if !keys.is_empty() { vs_gyroflow::mp4meta::write_keys(out, &keys)?; }
    Ok(())
}

// --preview still --picture: a still made from a picture already there (the player's frame as it shows it, in SDR),
// in a second or so: the clip isn't decoded nor its motion read (on a NAS the 30 s of a still rendered here). The
// video only gives the link's `hash` (main) and, without --dates, the file's dates; the rest is write_preview's, as
// for a still rendered here: the size, the H.264 held for --from to --to, the cover art, the keys
pub(crate) fn still_from_picture(file: &str, opts: &Options, picture: &Path) -> Result<Outcome, String> {
    let out_path = opts.output.clone().ok_or("--picture needs --output")?;
    let (from, to) = (opts.from.ok_or("--picture needs --from")?, opts.to.ok_or("--picture needs --to")?);
    if out_path.exists() && !opts.force {
        return Ok(Outcome::Skipped(format!("{} exists (--force replaces it)", out_path.display())));
    }
    let out_dir = out_path.parent().filter(|p| !p.as_os_str().is_empty()).unwrap_or(Path::new(".")).to_path_buf();
    std::fs::create_dir_all(&out_dir).map_err(|e| format!("{}: {e}", out_dir.display()))?;
    let stem = out_path.file_stem().map(|x| x.to_string_lossy().to_string()).unwrap_or_default();
    let pic = ffprobe(picture).map_err(|e| format!("the picture {}: {e}", picture.display()))?;
    let size = preview_size(pic.width, pic.height);
    let files = PreviewFiles { frame: TempFile(out_dir.join(format!(".{stem}.frame.yuv"))),
                               cover: TempFile(out_dir.join(format!(".{stem}.cover.jpg"))), size };
    // As a still rendered here has them: BT.709 for its video, BT.601 full range for the JPEG of its cover
    let graph = format!("[0:v]scale={}:{}:flags=lanczos,split[a][b];[a]scale=out_color_matrix=bt709:out_range=tv,format=yuv420p[f];\
                         [b]scale=out_color_matrix=bt601:out_range=pc,format=yuvj420p[c]", size.0, size.1);
    let result = ffmpeg_command("ffmpeg").args(["-nostdin", "-v", "error", "-y", "-i"]).arg(picture)
        .args(["-filter_complex", &graph, "-map", "[f]", "-frames:v", "1", "-f", "rawvideo"]).arg(&files.frame.0)
        .args(["-map", "[c]", "-frames:v", "1", "-q:v", "5", "-update", "1", "-f", "image2"]).arg(&files.cover.0)
        .stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::piped()).output().map_err(|e| format!("ffmpeg: {e}"))?;
    if !result.status.success() { return Err(format!("the picture: {}", String::from_utf8_lossy(&result.stderr).trim())); }
    let tags = ["bt709", "bt709", "bt709", "tv"].map(str::to_string);
    let tmp_path = out_dir.join(format!(".{stem}.partial.mp4"));
    if let Err(e) = write_preview(&files, 1, to - from, &tags, &tmp_path, opts, opts.dates.map(|d| (d, true))) {
        let _ = std::fs::remove_file(&tmp_path);
        return Err(e);
    }
    let modified = opts.dates.map(|d| d.system_time()).or_else(|| std::fs::metadata(file).and_then(|m| m.modified()).ok());
    if let (Some(modified), Ok(f)) = (modified, std::fs::File::options().write(true).open(&tmp_path)) {
        let _ = f.set_modified(modified);
    }
    if out_path.exists() { copy_finder_attributes(&out_path, &tmp_path); }
    std::fs::rename(&tmp_path, &out_path).map_err(|e| e.to_string())?;
    Ok(Outcome::Done(out_path))
}

// A flipbook's times without --at: about 12 through the clip, 10 s apart at least, after its first second
pub(crate) fn flipbook_times(duration: f64) -> Vec<f64> {
    if duration <= 1.5 { return vec![duration / 2.0]; }
    let span = duration - 1.0;
    let n = ((span / 10.0).floor() as usize).clamp(1, 12);
    (0..n).map(|i| 1.0 + span * (i as f64 + 0.5) / n as f64).collect()
}

// --preview flipbook: the clip at a glance (docs/design/moments-and-workspaces.md, "Clips"). Color only: only keyframes are
// decoded, one per time, from the low-resolution copy when there is one, through the SDR look of a still
pub(crate) fn flipbook(file: &str, opts: &Options) -> Result<Outcome, String> {
    let path = vs_gyroflow::canonicalize(file).map_err(|e| e.to_string())?;
    let info = ffprobe(&path)?;
    let stem = path.file_stem().map(|x| x.to_string_lossy().to_string()).unwrap_or_default();
    let video_dir = path.parent().unwrap_or(Path::new(".")).to_path_buf();
    let out_path = opts.output.clone().unwrap_or_else(|| opts.out.clone().unwrap_or(video_dir).join(format!("{stem}.afplink.mp4")));
    if out_path.exists() && !opts.force {
        return Ok(Outcome::Skipped(format!("{} exists (--force replaces it)", out_path.display())));
    }
    let out_dir = out_path.parent().filter(|p| !p.as_os_str().is_empty()).unwrap_or(Path::new(".")).to_path_buf();
    std::fs::create_dir_all(&out_dir).map_err(|e| format!("{}: {e}", out_dir.display()))?;
    let name = out_path.file_stem().map(|x| x.to_string_lossy().to_string()).unwrap_or_default();

    // A low-resolution copy that doesn't read (damaged, cut short): the clip itself, then
    let (low, src) = match lowres(&path).map(|l| { let p = ffprobe(&l); (l, p) }) {
        Some((l, Ok(p))) => (Some(l), p),
        _ => (None, ffprobe(&path)?),
    };
    let source = low.clone().unwrap_or_else(|| path.clone());
    let last = (info.duration - 0.05).max(0.0);
    let times: Vec<f64> = if opts.at_list.is_empty() { flipbook_times(info.duration) } else { opts.at_list.clone() }
        .into_iter().map(|t| t.clamp(0.0, last)).collect();
    let cover_at = opts.cover.unwrap_or(info.duration / 2.0);

    // The look, as a still's: D-Log M by the maker's LUT, another curve through NBCU's, HLG through NBCU's; the
    // underwater colour by its numbers at the flipbook's frames (their median: one LUT for them all)
    let curve = clip_curve(opts, &path);
    let uw = match curve { Some("uw") => Some(Uw::new(&source, opts, UwSpan::Median(times.clone()), &out_dir, &name)?), _ => None };
    let curve_lut = |c: &str| match (&uw, c) { (Some(u), "uw") => Ok(u.cube.0.clone()), _ => lut(&curve_lut_file(c)) };
    let mut _temp_lut = None;
    let cube = match curve {
        Some("dlogm") if !opts.nbcu && lut("dji-dlogm-rec709.cube").is_ok() => Some(lut("dji-dlogm-rec709.cube")?),
        Some(c) => {
            let composed = out_dir.join(format!(".{name}.nbcu.cube"));
            gpu_lut::compose_nbcu(&curve_lut(c)?, &lut(NBCU_HLG)?, &composed)?;
            _temp_lut = Some(TempFile(composed.clone()));
            Some(composed)
        }
        None if info.transfer.as_deref() == Some("arib-std-b67") => Some(lut(NBCU_HLG)?),
        None => None,
    };
    // Decoded as the sensor has it (-noautorotate), then turned upright as the file says (its display matrix: the low
    // resolution copy's own) and as --rotation and --aspect say, before it's scaled down
    let frame = opts.frame.for_video(display_rotation(&source), None);
    let turn = frame.ffmpeg_filter((src.width, src.height));
    let (pw, ph) = { let (fw, fh) = frame.size((src.width, src.height)); preview_size(fw, fh) };
    let in_range = if src.full_range { "pc" } else { "tv" };
    let look = match &cube {
        Some(c) => format!(",scale=in_color_matrix={}:in_range={in_range},format=gbrp16le,lut3d=file={}:interp=tetrahedral,\
                            scale=out_color_matrix=bt709:out_range=tv", yuv_matrix(src.matrix.as_deref()), filter_path(c)),
        None => String::new(),
    };
    let tags: Vec<String> = if cube.is_some() { vec!["bt709".into(), "bt709".into(), "bt709".into()] } else {
        vec![src.primaries.clone().unwrap_or_else(|| "bt709".into()), src.transfer.clone().unwrap_or_else(|| "bt709".into()),
             src.matrix.clone().unwrap_or_else(|| "bt709".into())]
    };
    // Smaller first: the LUT on 960 pixels, not 4K
    let vf = format!("{turn}{}scale={pw}:{ph}:flags=lanczos{look},format=yuv420p", if turn.is_empty() { "" } else { "," });
    let size = pw * ph * 3 / 2;
    eprintln!("{}: flipbook of {} frames{} · {}", file_name(file), times.len(),
              low.as_ref().map_or(String::new(), |l| format!(" from {}", l.file_name().unwrap_or_default().to_string_lossy())),
              if cube.is_some() { "SDR" } else { "colors as recorded" });

    let started = Instant::now();
    let mut frames = Vec::with_capacity(size * times.len());
    let (mut count, mut cover_frame, mut cover_gap) = (0usize, 0usize, f64::MAX);
    // The keyframe at or before t; a clip shorter than its keyframe interval (a second or less) has none after its
    // first frame there, which -skip_frame nokey then skips: the frame at t is decoded the usual way instead
    let decode = |t: f64, keyframes: bool| -> Result<std::process::Output, String> {
        let mut cmd = ffmpeg_command("ffmpeg");
        cmd.args(["-nostdin", "-v", "error"]);
        if keyframes { cmd.args(["-skip_frame", "nokey", "-noaccurate_seek"]); }
        cmd.args(["-ss", &format!("{t:.3}"), "-noautorotate", "-i"]).arg(&source)
            .args(["-map", "0:v:0", "-frames:v", "1", "-vf", &vf, "-f", "rawvideo", "-pix_fmt", "yuv420p", "-"])
            .stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped());
        let child = cmd.spawn().map_err(|e| format!("ffmpeg: {e}"))?;
        if BACKGROUND.load(Relaxed) { set_background(&[child.id()], true); }
        child.wait_with_output().map_err(|e| format!("ffmpeg: {e}"))
    };
    for t in &times {
        if stopped() { return Err("cancelled".into()); }
        if let Some(c) = &opts.control { read_control(c); }
        let mut out = decode(*t, true)?;
        if out.stdout.len() < size { out = decode(*t, false)?; }
        if out.stdout.len() < size {
            eprintln!("  no frame at {t:.1} s: {}", String::from_utf8_lossy(&out.stderr).trim());
            continue;
        }
        frames.extend_from_slice(&out.stdout[..size]);
        if (t - cover_at).abs() < cover_gap { cover_gap = (t - cover_at).abs(); cover_frame = count; }
        count += 1;
    }
    if count == 0 { return Err("no frames decoded".into()); }

    let files = PreviewFiles { frame: TempFile(out_dir.join(format!(".{name}.frames.yuv"))),
                               cover: TempFile(out_dir.join(format!(".{name}.cover.jpg"))), size: (pw, ph) };
    std::fs::write(&files.frame.0, &frames).map_err(|e| format!("{}: {e}", files.frame.0.display()))?;
    // The cover: JPEG's BT.601 and full range
    let mut cover = ffmpeg_command("ffmpeg");
    cover.args(["-nostdin", "-v", "error", "-y", "-f", "rawvideo", "-pix_fmt", "yuv420p", "-s", &format!("{pw}x{ph}"),
                "-color_range", "tv", "-colorspace", &tags[2], "-i", "-",
                "-vf", "scale=out_color_matrix=bt601:out_range=pc,format=yuvj420p", "-frames:v", "1", "-q:v", "5", "-update", "1", "-f", "image2"])
        .arg(&files.cover.0).stdin(Stdio::piped()).stdout(Stdio::null()).stderr(Stdio::piped());
    let mut child = cover.spawn().map_err(|e| format!("ffmpeg: {e}"))?;
    let _ = child.stdin.take().unwrap().write_all(&frames[cover_frame * size..(cover_frame + 1) * size]);
    let out = child.wait_with_output().map_err(|e| format!("ffmpeg: {e}"))?;
    if !out.status.success() { return Err(format!("the cover: {}", String::from_utf8_lossy(&out.stderr).trim())); }

    let tmp_path = out_dir.join(format!(".{name}.partial.mp4"));
    // The clip's own date without --dates: its creation_time, in the zone a DJI name says
    let dates = opts.dates.map(|d| (d, true)).or_else(|| {
        let mut d = parse_dates(info.created.as_deref()?)?;
        match zone_of_name(&stem, d.unix) { Some(z) => { d.offset_min = z; Some((d, true)) } None => Some((d, false)) }
    });
    if let Err(e) = write_preview(&files, count, 0.0, &tags, &tmp_path, opts, dates) {
        let _ = std::fs::remove_file(&tmp_path);
        return Err(e);
    }
    let modified = dates.map(|(d, _)| d.system_time()).or_else(|| std::fs::metadata(&path).and_then(|m| m.modified()).ok());
    if let (Some(modified), Ok(f)) = (modified, std::fs::File::options().write(true).open(&tmp_path)) { let _ = f.set_modified(modified); }
    if out_path.exists() { copy_finder_attributes(&out_path, &tmp_path); }
    std::fs::rename(&tmp_path, &out_path).map_err(|e| e.to_string())?;
    eprintln!("  {count} frames in {:.1} s", started.elapsed().as_secs_f64());
    Ok(Outcome::Done(out_path))
}
