// Photos (--photo), Live Photos and Motion Photos.
// One of gyroflow-export's parts: main.rs has the uses and main(); each part's items are the program's.

use super::*;

// ---------------------------------------------------------------------------------------------------
// Photos (--photo): the frame at a time as a still, stabilized as the player shows it (and turned as crop mode has
// the camera), cleaned up from its neighbours as the player's clean still frame is (still.rs), in the SDR look with
// the HDR one as a gain map (photo.rs): HEIC through afp-photo on macOS (macos/photo.swift), else an Ultra HDR JPEG

/// When the frame at `t` s into the clip was taken, and whether its zone is known: the clip's start as the Library
/// dates it (geotag::read: the camera's GPS time, else its clock read per camera, GoPro's TZON zone), plus `t`. The zone:
/// the camera's own (TZON), else a DJI name's (its local time against the UTC it writes), else this computer's,
/// assumed. An older GoPro writes its local time with no zone: that local time is kept as it is. None where the
/// camera's clock was never set and its name says nothing either
fn moment_dates(path: &Path, camera: &str, created: Option<&str>, t: f64) -> Option<(Dates, bool)> {
    let mvhd = created.and_then(parse_dates).map(|d| d.unix);
    let (clock, _) = vs_gyroflow::geotag::read(path, camera, mvhd.map(|c| c.round() as i64));
    let stem = path.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
    let named = clip_start_local(path).filter(|_| stem.starts_with("DJI_"));
    match (clock.basis, clock.utc) {
        ("local", Some(local)) => {
            let off = (local_offset(local as i64) / 60) as i32;
            Some((Dates { unix: local - off as f64 * 60.0 + t, offset_min: off }, false))
        }
        (_, Some(utc)) => {
            // An older GoPro (no TZON) writes its local time as mvhd's: against the GPS's UTC, that is its zone
            let gopro_local = (clock.basis == "gps" && camera.to_lowercase().starts_with("gopro")).then_some(()).and(mvhd).map(|m| ((m - utc) / 900.0).round() as i32 * 15)
                .filter(|z| z.abs() <= 14 * 60);
            let zone = clock.zone_min.or_else(|| mvhd.and_then(|m| zone_of_name(&stem, m))).or(gopro_local);
            let (off, known) = match zone { Some(z) => (z, true), None => ((local_offset(utc as i64) / 60) as i32, false) };
            Some((Dates { unix: utc + t, offset_min: off }, known))
        }
        // A clock never set: a DJI name's local time, if any
        _ => named.map(|local| {
            let off = (local_offset(local) / 60) as i32;
            (Dates { unix: (local - off as i64 * 60) as f64 + t, offset_min: off }, false)
        }),
    }
}

/// When the clip started, local time, as seconds since 1970 counted in local time: from a DJI file's name
/// (DJI_YYYYMMDDHHMMSS_…, the camera's own clock), else the container's creation time (UTC) in this computer's zone
fn clip_start_local(path: &Path) -> Option<i64> {
    let name = path.file_name()?.to_string_lossy().to_string();
    if let Some(digits) = name.strip_prefix("DJI_").and_then(|r| r.get(..14)).filter(|d| d.bytes().all(|b| b.is_ascii_digit())) {
        let n = |a: usize, b: usize| digits[a..b].parse::<i64>().ok();
        return Some(days_from_civil(n(0, 4)?, n(4, 6)?, n(6, 8)?) * 86400 + n(8, 10)? * 3600 + n(10, 12)? * 60 + n(12, 14)?);
    }
    let out = ffmpeg_command("ffprobe").args(["-v", "error", "-show_entries", "format_tags=creation_time", "-of", "default=nw=1:nk=1"])
        .arg(path).output().ok()?;
    let t = String::from_utf8_lossy(&out.stdout).trim().to_string();
    let n = |a: usize, b: usize| t.get(a..b)?.parse::<i64>().ok();
    let utc = days_from_civil(n(0, 4)?, n(5, 7)?, n(8, 10)?) * 86400 + n(11, 13)? * 3600 + n(14, 16)? * 60 + n(17, 19)?;
    Some(utc + local_offset(utc))
}

#[cfg(unix)]
fn local_offset(utc: i64) -> i64 {
    let t = utc as libc::time_t;
    let mut tm: libc::tm = unsafe { std::mem::zeroed() };
    if unsafe { libc::localtime_r(&t, &mut tm) }.is_null() { 0 } else { tm.tm_gmtoff as i64 }
}
#[cfg(not(unix))]
fn local_offset(_utc: i64) -> i64 { 0 }


/// A file's modified and (on a Mac) created dates
fn set_file_dates(path: &Path, at: std::time::SystemTime) {
    let Ok(f) = std::fs::File::options().write(true).open(path) else { return };
    let times = std::fs::FileTimes::new().set_modified(at).set_accessed(at);
    #[cfg(target_os = "macos")]
    let times = { use std::os::macos::fs::FileTimesExt; times.set_created(at) };
    let _ = f.set_times(times);
}

// Photos: the project's _Stills (beside its _Exports, docs/design/moments-and-workspaces.md), else stabilized/ next to the clip
fn out_dir_for(path: &Path, opts: &Options) -> PathBuf {
    opts.out.clone().unwrap_or_else(|| match vs_gyroflow::workspace::project_root(path) {
        Some(root) => root.join("_Stills"),
        None => path.parent().unwrap_or(Path::new(".")).join("stabilized"),
    })
}

/// Whether any of `frames` leaves part of the output uncovered, by its mask (Engine::leaves_edges_at), in the view
/// and through the camera the engine's edges are configured with
pub(crate) fn frames_leave_edges(engine: &Engine, mut frames: impl Iterator<Item = u64>, fps: f64, limited_range: bool) -> Result<bool, String> {
    frames.try_fold(false, |any, i| {
        if any { return Ok(true); }
        engine.leaves_edges_at(((i as f64 / fps) * 1_000_000.0).round() as i64 + engine.frame_offset_us, limited_range)
    })
}

/// The HLG rendition as macOS shows it on an SDR screen: afp-photo's Core Image (macos/photo.swift)
fn macos_sdr(helper: &Path, hdr: &vs_gyroflow::photo::Image, dir: &Path) -> Result<vs_gyroflow::photo::Image, String> {
    std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let id = std::process::id();
    let (hdr_file, sdr_file) = (TempFile(dir.join(format!(".photo-{id}.hlg.rgb16"))), TempFile(dir.join(format!(".photo-{id}.sdr.rgb16"))));
    vs_gyroflow::photo::write_rgb16(&hdr_file.0, hdr)?;
    let out = Command::new(helper).args(["--width", &hdr.width.to_string(), "--height", &hdr.height.to_string()])
        .arg("--hdr").arg(&hdr_file.0).arg("--sdr-out").arg(&sdr_file.0).output().map_err(|e| format!("afp-photo: {e}"))?;
    if !out.status.success() { return Err(String::from_utf8_lossy(&out.stderr).trim().to_string()); }
    vs_gyroflow::photo::read_rgb16(&sdr_file.0, hdr.width, hdr.height)
}

/// A Live Photo's or Motion Photo's motion: this long around the photo, as an iPhone's, and at most this wide or high
const LIVE_SECONDS: f64 = 3.0;
const LIVE_MAX_SIDE: usize = 1920;
/// Its bit rate: about an iPhone's Live Photo's (1920x1440 at 30 fps in ~8 Mbit/s); 3 s of 1080p60 is ~5 MB
const LIVE_BITS_PER_PIXEL: f64 = 0.1;
/// And at most 30 frames a second, as an iPhone's: it's the photo's context, not a video of its own
const LIVE_MAX_FPS: f64 = 30.0;

/// Every how many frames a part at most `max_fps` takes: 59.94 and 50 fps every 2nd, 120 every 4th, 30 and less all
pub(crate) fn frame_step(fps: f64, max_fps: f64) -> u64 {
    (fps / max_fps - 0.01).ceil().max(1.0) as u64
}

/// A Live Photo's content identifier: a random UUID (version 4), as Apple's cameras write it
fn content_id() -> Result<String, String> {
    let mut b = [0u8; 16];
    std::fs::File::open("/dev/urandom").and_then(|mut f| f.read_exact(&mut b)).map_err(|e| format!("a random UUID: {e}"))?;
    b[6] = (b[6] & 0x0F) | 0x40;
    b[8] = (b[8] & 0x3F) | 0x80;
    let h: String = b.iter().map(|x| format!("{x:02X}")).collect();
    Ok(format!("{}-{}-{}-{}-{}", &h[..8], &h[8..12], &h[12..16], &h[16..20], &h[20..]))
}

/// A 16-bit RGBA PNG (mpv's screenshot with screenshot-high-bit-depth): its width, height and pixels, 8 bytes each
/// (big-endian, as PNG has them), unfiltered
fn read_png16(path: &Path) -> Result<(usize, usize, Vec<u8>), String> {
    use std::io::Read;
    let bad = |why: &str| format!("{}: {why}", path.display());
    let d = std::fs::read(path).map_err(|e| bad(&e.to_string()))?;
    if d.len() < 8 || &d[..8] != b"\x89PNG\r\n\x1a\n" { return Err(bad("not a PNG")); }
    let (mut i, mut w, mut h, mut idat) = (8, 0usize, 0usize, Vec::new());
    while i + 8 <= d.len() {
        let n = u32::from_be_bytes([d[i], d[i + 1], d[i + 2], d[i + 3]]) as usize;
        let (kind, body) = (&d[i + 4..i + 8], d.get(i + 8..i + 8 + n).ok_or_else(|| bad("cut short"))?);
        match kind {
            b"IHDR" => {
                w = u32::from_be_bytes([body[0], body[1], body[2], body[3]]) as usize;
                h = u32::from_be_bytes([body[4], body[5], body[6], body[7]]) as usize;
                if body[8] != 16 || body[9] != 6 || body[12] != 0 { return Err(bad("not 16-bit RGBA without interlacing")); }
            }
            b"IDAT" => idat.extend_from_slice(body),
            b"IEND" => break,
            _ => {}
        }
        i += 12 + n;
    }
    let (bpp, stride) = (8, w * 8);
    let mut raw = Vec::with_capacity((stride + 1) * h);
    flate2::read::ZlibDecoder::new(&idat[..]).read_to_end(&mut raw).map_err(|e| bad(&e.to_string()))?;
    if w == 0 || raw.len() < (stride + 1) * h { return Err(bad("short image data")); }
    let mut out = vec![0u8; stride * h];
    for y in 0..h {
        let f = raw[y * (stride + 1)];
        let line = &raw[y * (stride + 1) + 1..(y + 1) * (stride + 1)];
        let (prev, cur) = out.split_at_mut(y * stride);
        let up = if y > 0 { &prev[(y - 1) * stride..] } else { &[][..] };
        let cur = &mut cur[..stride];
        for x in 0..stride {
            let a = if x >= bpp { cur[x - bpp] as i32 } else { 0 };
            let b = if y > 0 { up[x] as i32 } else { 0 };
            let c = if x >= bpp && y > 0 { up[x - bpp] as i32 } else { 0 };
            let p = match f {
                0 => 0,
                1 => a,
                2 => b,
                3 => (a + b) / 2,
                4 => { let (pa, pb, pc) = ((b - c).abs(), (a - c).abs(), (a + b - 2 * c).abs()); if pa <= pb && pa <= pc { a } else if pb <= pc { b } else { c } }
                _ => return Err(bad("unknown filter")),
            };
            cur[x] = (line[x] as i32 + p) as u8;
        }
    }
    Ok((w, h, out))
}

/// A photo's link to its clip (photo.rs's AFP_NS): the clip's path in its project's tree (else its name), its size and
/// fingerprint, the frame's time. --link's, when given, with these added
fn photo_link(clip: &Path, opts: &Options, t: f64) -> String {
    use vs_gyroflow::workspace as ws;
    let mut o = opts.link.as_deref().and_then(|l| serde_json::from_str::<serde_json::Value>(l).ok()).and_then(|v| v.as_object().cloned()).unwrap_or_default();
    o.insert("v".into(), 1.into());
    o.insert("kind".into(), "photo".into());
    o.insert("t".into(), serde_json::json!((t * 1000.0).round() / 1000.0));
    if !o.contains_key("clip") {
        let root = ws::project_root(clip);
        let tree = root.as_ref().and_then(|r| ws::tree_path(clip, &ws::locations(r)));
        o.insert("clip".into(), tree.unwrap_or_else(|| clip.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()).into());
        if let Some(r) = root { o.entry("roots").or_insert_with(|| serde_json::json!([r.display().to_string()])); }
    }
    if let Ok(m) = std::fs::metadata(clip) { o.entry("size").or_insert(m.len().into()); }
    ws::with_fingerprint(&serde_json::Value::Object(o).to_string(), clip)
}

/// A place for afp-photo's --gps: "lat,lon[,alt]"
fn gps_arg(p: &vs_gyroflow::geotag::Fix) -> String {
    format!("{:.8},{:.8}{}", p.lat, p.lon, p.alt.map(|a| format!(",{a:.3}")).unwrap_or_default())
}

pub(crate) fn photo(file: &str, opts: &Options, t: f64) -> Result<Outcome, String> {
    use vs_gyroflow::photo::{ self as ph, Cube };
    use vs_gyroflow::still;
    // AFP_TIMES: how long each step took, on stderr
    let clock = Instant::now();
    let lap = |what: &str| if std::env::var_os("AFP_TIMES").is_some() { eprintln!("  [{:6.2} s] {what}", clock.elapsed().as_secs_f64()) };
    let path = vs_gyroflow::canonicalize(file).map_err(|e| e.to_string())?;
    let curve = clip_curve(opts, &path);
    let source = if opts.no_stabilize { None } else { find_source(&path, opts.project.as_deref(), opts.lens, opts.preset.as_deref()).ok() };
    let info = ffprobe(&path)?;
    if info.transfer.as_deref() == Some("smpte2084") && curve.is_none() {
        return Err("PQ video: no photo of it yet (HLG, log and SDR video only)".into());
    }
    let fps = info.fps.0 as f64 / info.fps.1 as f64;
    let total = info.frames.max(1);
    // The frame mpv shows at `t` (its time-pos)
    let n = ((t * fps).round().max(0.0) as u64).min(total - 1);
    let r = opts.photo_radius as u64;
    let (w, h) = (info.width & !1, info.height & !1);
    let camera = opts.camera.unwrap_or_default();

    // Where it goes, and as what
    let helper = std::env::current_exe().ok().and_then(|e| Some(e.parent()?.join("afp-photo"))).filter(|p| p.is_file())
        .unwrap_or_else(|| tool("afp-photo"));
    let out_dir = out_dir_for(&path, opts);
    std::fs::create_dir_all(&out_dir).map_err(|e| format!("{}: {e}", out_dir.display()))?;
    let stem = path.file_stem().map(|x| x.to_string_lossy().to_string()).unwrap_or_default();
    let mut jpeg = opts.jpeg;
    if !jpeg && !helper.is_file() {
        eprintln!("  no afp-photo: an Ultra HDR JPEG instead of HEIC");
        jpeg = true;
    }
    // A Motion Photo's name ends in MP, as Android's cameras name them; a Live Photo's video has the HEIC's name
    let ext = match (jpeg, opts.live) { (true, true) => "MP.jpg", (true, false) => "jpg", _ => "heic" };
    // A new name for each: the same frame saved again doesn't replace the last one
    let name = |k: usize| out_dir.join(if k < 2 { format!("{stem}_{t:.2}s.{ext}") } else { format!("{stem}_{t:.2}s_{k}.{ext}") });
    let taken_by = |p: &Path| p.exists() || (opts.live && !jpeg && p.with_extension("mov").exists());
    let mut k = 1;
    while taken_by(&name(k)) && !opts.force { k += 1; }
    let out_path = name(k);

    // The SDR look the image is: --sdr-image, else D-Log M by the maker's LUT, the rest through NBCU's conversion
    let sdr_look = opts.sdr_image.clone().unwrap_or_else(|| (if curve == Some("dlogm") && !opts.nbcu { "maker" } else { "nbcu" }).to_string());
    // A Motion Photo's video in that look too: the image is SDR (its gain map lifting it on an HDR screen), and the
    // apps that play its motion (Google Photos, the gallery apps, Windows' Photos) show HLG each its own way, some
    // compressing all of HLG's range into SDR's, white at a fifth. A Live Photo's (HEIC) stays HLG: Apple's Photos
    // shows it as the player does
    let motion_sdr = jpeg.then(|| if sdr_look == "maker" && curve == Some("dlogm") { "maker" } else { "nbcu" });

    // The image: the frame at `t`, cleaned up from its neighbours, in the HDR look and the SDR one the image is
    // (--sdr-image)
    let image = || -> Result<(ph::Image, Option<ph::Image>, String, usize), String> {
        // The player's own render (--image): the HDR look as it is, cut to the frame; the SDR image from it
        if let Some(img) = &opts.image {
            let (iw, ih, raw) = read_png16(img)?;
            let stride = iw * 8;
            let [cx, cy, cw, ch] = opts.crop.unwrap_or([0.0, 0.0, 1.0, 1.0]);
            let even = |v: f64| (v.round() as usize) & !1;
            let (x0, y0) = (even(cx * iw as f64).min(iw - 2), even(cy * ih as f64).min(ih - 2));
            let (w, h) = (even(cw * iw as f64).clamp(2, iw - x0), even(ch * ih as f64).clamp(2, ih - y0));
            let mut pixels = Vec::with_capacity(w * h);
            for y in y0..y0 + h {
                let row = &raw[y * stride..];
                for x in x0..x0 + w {
                    let p = &row[x * 8..x * 8 + 6];
                    let c = |i: usize| u16::from_be_bytes([p[i], p[i + 1]]) as f32 / 65535.0;
                    pixels.push([c(0), c(2), c(4)]);
                }
            }
            let hdr = ph::Image { width: w, height: h, pixels };
            eprintln!("{}: photo at {t:.2} s (frame {n}), {w}x{h} as the player rendered it", file_name(file));
            lap("the player's frame read");
            let (sdr, how) = if sdr_look == "macos" && cfg!(target_os = "macos") && helper.is_file() {
                (macos_sdr(&helper, &hdr, &out_dir)?, "as macOS shows the HLG")
            } else {
                (Cube::load(&lut(NBCU_HLG)?)?.apply_type3(&hdr), "NBCU")
            };
            lap("colors");
            return Ok((sdr, Some(hdr), format!("HLG as the player rendered it: SDR {how}, HDR HLG"), 0));
        }
        let engine = match &source {
            Some(s) => Some(Engine::load(&s.project, s.preset.as_deref(), opts.fov, opts.horizon_lock, opts.underwater, opts.level, 0.0,
                                         (w, h), 3, (1, 1), None, SampleKind::U16, 1023.0, true, opts.interpolation, 2,
                                         // --alone: the part (--from, --to) stabilized by its own motion, as a moment plays
                                         opts.alone.then(|| (opts.from.unwrap_or(0.0), opts.to.unwrap_or(info.duration))), opts.alone_project, opts.frame, s.lens)?),
            None => None,
        };
        let view = if opts.fov == Fov::Wide { vs_gyroflow::edges::VIEW_WIDE } else { vs_gyroflow::edges::VIEW_NORMAL };
        if let Some(e) = &engine { e.edges.set_camera(camera); }
        // The frames as decoded, as for an export (decoded_frame)
        let ((dw, dh), turn) = decoded_frame(engine.is_some(), &path, (w, h), opts);
        // Zoomed out (crop mode's camera starts so), rendered that much bigger: the frame's part keeps the pixels the
        // view has at its own zoom
        let (rw, rh) = match &engine {
            Some(e) => {
                let s = camera.zoom.clamp(1.0, 2.0);
                (((e.render_size.0 as f64 * s) as usize) & !1, ((e.render_size.1 as f64 * s) as usize) & !1)
            }
            None => (dw, dh),
        };
        let (near, last) = (n.saturating_sub(r), (n + r).min(total - 1));
        // The water's color fringes taken out after the clean-up, as the player does it (Post: chromatic.glsl, from the
        // marker the engine writes into each frame)
        let post = engine.as_ref().and_then(|e| Post::new(e, opts, &info, (rw, rh), fps, (n, n), view, camera, 0));
        let marker = post.as_ref().is_some_and(|p| p.chromatic);
        let hwaccel = if cfg!(target_os = "macos") { Some("videotoolbox") } else if hwaccel_works(&path) { Some("auto") } else { None };
        let plane_dims = |(w, h): (usize, usize)| [(w, h), (w / 2, h / 2), (w / 2, h / 2)];
        // Frames `first` to `last`, decoded and rendered: those from `near` on kept, the ones before them only rendered
        // (for the sphere), every `pre_step`th counting back from `near`
        let render = |first: u64, pre_step: u64| -> Result<(Vec<(u64, Vec<u8>)>, usize), String> {
            let mut dec_cmd = ffmpeg_command("ffmpeg");
            dec_cmd.args(["-nostdin", "-v", "error"]);
            if let Some(h) = hwaccel { dec_cmd.args(["-hwaccel", h]); }
            // With -ss before -i, the first frame at or after it
            if first > 0 { dec_cmd.args(["-ss", &format!("{:.6}", (first as f64 - 0.25) / fps)]); }
            dec_cmd.args(["-noautorotate", "-i"]).arg(&path)
                .args(["-map", "0:v:0", "-frames:v", &(last - first + 1).to_string(), "-vf", &format!("scale={w}:{h}{}{turn}", if turn.is_empty() { "" } else { "," }),
                       "-f", "rawvideo", "-pix_fmt", "yuv420p10le", "-"])
                .stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped());
            let mut decoder = dec_cmd.spawn().map_err(|e| format!("ffmpeg: {e}"))?;
            // Read on a thread of its own: the next frames come while one is rendered
            let mut dec_out = decoder.stdout.take().unwrap();
            let (frame_tx, frame_rx) = mpsc::sync_channel::<Vec<u8>>(2);
            let reader = std::thread::spawn(move || {
                loop {
                    let mut src = vec![0u8; dw * dh * 3];
                    if dec_out.read_exact(&mut src).is_err() || frame_tx.send(src).is_err() { break; }
                }
            });
            let mut window: Vec<(u64, Vec<u8>)> = Vec::new();
            let mut rendered = 0;
            let mut result = Ok(());
            for (index, src) in (first..=last).zip(frame_rx.iter()) {
                if stopped() { break; }
                let wanted = index >= near;
                if !wanted && (near - index) % pre_step != 0 { continue; }
                let frame = match &engine {
                    None => src,
                    // Before the photo's frames: only the sphere written from them
                    Some(engine) if !wanted => {
                        let mut planes = Vec::with_capacity(3);
                        let mut so = 0;
                        for (sw, sh) in plane_dims((w, h)) {
                            planes.push(PlaneJob { src: src[so..].as_ptr(), src_stride: sw * 2, src_w: sw, src_h: sh, dst: std::ptr::null_mut(), dst_stride: 0, dst_w: 0, dst_h: 0 });
                            so += sw * sh * 2;
                        }
                        let timestamp_us = ((index as f64 / fps) * 1_000_000.0).round() as i64 + engine.frame_offset_us;
                        if let Err(e) = engine.write_sphere(timestamp_us, planes, !info.full_range) { result = Err(e); break; }
                        continue;
                    }
                    Some(engine) => {
                        let mut dst = vec![0u8; rw * rh * 3];
                        let timestamp_us = ((index as f64 / fps) * 1_000_000.0).round() as i64 + engine.frame_offset_us;
                        let mut planes = Vec::with_capacity(3);
                        let (mut so, mut doff) = (0, 0);
                        for ((sw, sh), (dw, dh)) in plane_dims((w, h)).into_iter().zip(plane_dims((rw, rh))) {
                            planes.push(PlaneJob {
                                src: src[so..].as_ptr(), src_stride: sw * 2, src_w: sw, src_h: sh,
                                dst: dst[doff..].as_mut_ptr(), dst_stride: dw * 2, dst_w: dw, dst_h: dh,
                            });
                            so += sw * sh * 2;
                            doff += dw * dh * 2;
                        }
                        if let Err(e) = engine.process(timestamp_us, planes, !info.full_range, marker) { result = Err(e); break; }
                        dst
                    }
                };
                rendered += 1;
                if wanted { window.push((index, frame)); }
            }
            drop(frame_rx);
            let _ = decoder.kill();
            let _ = decoder.wait();
            let _ = reader.join();
            if stopped() { return Err("cancelled".into()); }
            result.map(|_| (window, rendered))
        };
        // The edges its frames leave, filled as the export fills them; only when they leave some (their masks say,
        // Edges::leaves_edges_at). The sphere is written from the second before them too, as it is playing: 15 frames
        // of it, as the sphere fades by time, not by frames, and each frame covers most of what the one before did
        let edges = match &engine {
            Some(e) if opts.edges != EdgeMode::Off && e.leaves_edges_in(view, near as usize..=last as usize, fps, &camera) => {
                e.edges.configure(opts.edges, opts.edge_settings, view, opts.level, None);
                e.edges.set_camera(camera);
                if frames_leave_edges(e, near..=last, fps, !info.full_range)? { opts.edges } else { EdgeMode::Off }
            }
            _ => EdgeMode::Off,
        };
        if let Some(e) = &engine {
            e.edges.configure(edges, opts.edge_settings, view, opts.level, None);
            e.edges.set_camera(camera);
        }
        let first = if edges == EdgeMode::Sphere { near.saturating_sub(fps.round() as u64) } else { near };
        let edge_label = if edges == EdgeMode::Off { String::new() } else { format!(", edges {}", format!("{edges:?}").to_lowercase()) };
        eprintln!("{}: photo at {t:.2} s (frame {n}), {}x{} from frames {near}-{last}{edge_label}", file_name(file), rw, rh);
        let (window, rendered) = render(first, (fps / 15.0).round().max(1.0) as u64)?;
        drop(engine);
        lap(&format!("frames rendered ({rendered})"));
        let target = window.iter().position(|(i, _)| *i == n).ok_or("the frame isn't in the video")?;

        // Cleaned up: the frame first, then its neighbours nearest first
        let as_u16 = |b: &[u8]| b.chunks_exact(2).map(|c| u16::from_le_bytes([c[0], c[1]])).collect::<Vec<u16>>();
        let (ysz, csz) = (rw * rh, rw * rh / 4);
        let mut planes: Vec<Vec<u16>> = {
            let f = &window[target].1;
            vec![as_u16(&f[..ysz * 2]), as_u16(&f[ysz * 2..(ysz + csz) * 2]), as_u16(&f[(ysz + csz) * 2..])]
        };
        let mut cleaned = 0;
        if r > 0 && window.len() > 1 {
            let mut order: Vec<usize> = (0..window.len()).collect();
            order.sort_by_key(|&i| ((window[i].0 as i64 - n as i64).abs(), window[i].0));
            let frames: Vec<still::Frame> = order.iter().map(|&i| {
                let f = &window[i].1;
                let plane = |at: usize, len: usize, (pw, ph): (usize, usize)| still::Plane { data: &f[at..at + len], stride: pw * 2, width: pw, height: ph };
                still::Frame { planes: vec![plane(0, ysz * 2, (rw, rh)), plane(ysz * 2, csz * 2, (rw / 2, rh / 2)), plane((ysz + csz) * 2, csz * 2, (rw / 2, rh / 2))] }
            }).collect();
            let settings = still::Settings { luma_keep: opts.photo_grain, ..Default::default() };
            match still::denoise(&frames, 2, 1023, (1, 1), &settings) {
                Ok(out) => {
                    planes = out.planes;
                    cleaned = frames.len();
                    // The last row of luma carries the fringes' marker (dispersion.rs): as it was, as the player's Still
                    // filter keeps it
                    if marker {
                        let f = &window[target].1;
                        planes[0][(rh - 1) * rw..].copy_from_slice(&as_u16(&f[(rh - 1) * rw * 2..ysz * 2]));
                    }
                }
                Err(e) => eprintln!("  not cleaned up: {e}"),
            }
        }
        drop(window);
        lap("cleaned up");
        if let Some(mut p) = post {
            // The clean-up keeps the target's marker row as it was (still.rs): chromatic.glsl reads it, then paints it over
            let bytes: Vec<u8> = planes.iter().flatten().flat_map(|v| v.to_le_bytes()).collect();
            match p.shaders.render(&bytes) {
                Ok(out) => {
                    let s = as_u16(&out);
                    planes = vec![s[..ysz].to_vec(), s[ysz..ysz + csz].to_vec(), s[ysz + csz..].to_vec()];
                    eprintln!("  {}", p.label);
                    lap(&p.label);
                }
                Err(e) => eprintln!("  {} not done: {e}", p.label),
            }
        }

        // The part to keep, on even pixels
        let [cx, cy, cw, ch] = opts.crop.unwrap_or([0.0, 0.0, 1.0, 1.0]);
        let even = |v: f64| (v.round() as usize) & !1;
        let (x0, y0) = (even(cx * rw as f64).min(rw - 2), even(cy * rh as f64).min(rh - 2));
        let rect = (x0, y0, even(cw * rw as f64).clamp(2, rw - x0), even(ch * rh as f64).clamp(2, rh - y0));
        let rgb = ph::yuv420p10_to_rgb_as([&planes[0], &planes[1], &planes[2]], rw, rh, rect, yuv_matrix(info.matrix.as_deref()), info.full_range,
                                          curve != Some("uw"));
        drop(planes);

        // The HDR look (HLG: the curve's LUT, or the video's own), and the SDR one the image is (--sdr-image). The
        // underwater colour pixel by pixel as the shader does it, with the numbers of the frame as the player has them
        // paused on it (--uw-params: as it has them now)
        let (hdr, recorded) = if curve == Some("uw") {
            use vs_gyroflow::uwcolor::Params;
            let e = match opts.uw_params { Some(e) => e, None => uw_measure(&path, &[t])?[0] };
            let p = Params::new(&e, &opts.uw_settings).clamped();
            lap("the underwater colour measured");
            let mut pixels = rgb.pixels.clone();
            let chunk = pixels.len().div_ceil(8).max(1);
            std::thread::scope(|s| for part in pixels.chunks_mut(chunk) { s.spawn(move || for c in part.iter_mut() { *c = p.to_hlg(*c); }); });
            let hdr = ph::Image { width: rgb.width, height: rgb.height, pixels };
            (Some(hdr), format!("underwater colour (red {:+.4}, blue {:+.4}, leak {:.4}, lift {:+.4} EV, strength {:.2}{})",
                                p.dr, p.db, p.k, p.ev, p.s, if p.hp != 1.0 { ", highlights" } else { "" }))
        } else if let Some(curve) = curve {
            (Some(Cube::load(&lut(&curve_lut_file(curve))?)?.apply(&rgb)), curve.to_string())
        } else if info.transfer.as_deref() == Some("arib-std-b67") {
            (Some(rgb.clone()), "HLG".to_string())
        } else {
            (None, "SDR".to_string())
        };
        let want = &sdr_look;
        let (sdr, how) = match &hdr {
            None => (rgb, "as recorded"),
            Some(h) if want == "macos" && cfg!(target_os = "macos") && helper.is_file() => (macos_sdr(&helper, h, &out_dir)?, "as macOS shows the HLG"),
            Some(_) if want == "maker" && curve == Some("dlogm") => (Cube::load(&lut("dji-dlogm-rec709.cube")?)?.apply(&rgb), "DJI's Rec.709"),
            Some(h) => (Cube::load(&lut(NBCU_HLG)?)?.apply_type3(h), "NBCU"),
        };
        let label = if hdr.is_some() { format!("{recorded}: SDR {how}, HDR HLG") } else { format!("SDR {how}") };
        lap("colors");
        Ok((sdr, hdr, label, cleaned))
    };

    // The camera, and where the frame was taken (the camera's GPS at its time, else the Library's location for the
    // clip): the photo's Exif GPS, its video's QuickTime location
    let maker = probe::camera(&path.to_string_lossy());
    let (make, model) = (maker.as_ref().map(|m| m.0.clone()), maker.and_then(|m| m.1));
    let place = vs_gyroflow::geotag::photo_place(&path, make.as_deref().unwrap_or(""), n as f64 / fps);
    lap("place");
    // When the frame was taken: the clip's start plus the frame's time (--dates: the clip's start as given)
    let camera = format!("{} {}", make.as_deref().unwrap_or(""), model.as_deref().unwrap_or("")).trim().to_string();
    let dates = match opts.dates {
        Some(d) => Some((Dates { unix: d.unix + n as f64 / fps, offset_min: d.offset_min }, true)),
        None => moment_dates(&path, &camera, info.created.as_deref(), n as f64 / fps),
    };
    let place = place.as_ref();

    std::thread::scope(|scope| {
        // The motion: the part of the video around it, exported as it plays, at the same time as the image
        let motion = opts.live.then(|| {
            let half = LIVE_SECONDS / 2.0;
            // From a frame the photo's is a whole number of steps after, so the video has the photo's frame
            let step = frame_step(fps, LIVE_MAX_FPS);
            let first = ((t - half).max(0.0) * fps - 1e-6).ceil().max(0.0) as u64;
            let first = n - (n.saturating_sub(first) / step) * step;
            let from = first as f64 / fps;
            let to = (t + half).min(info.duration.max(from + 1.0 / fps));
            let video = out_dir.join(format!(".{}.motion.{}", out_path.file_name().unwrap().to_string_lossy(), if jpeg { "mp4" } else { "mov" }));
            let part = Part { from, to, out: video.clone(), max_side: LIVE_MAX_SIDE, bits_per_pixel: LIVE_BITS_PER_PIXEL, max_fps: LIVE_MAX_FPS, mp4: jpeg, dv: !jpeg, sdr: motion_sdr,
                              location: place.map(|p| vs_gyroflow::geotag::to_iso6709(&p.0)), created: dates.map(|(d, _)| d.quicktime()), still: Some(t) };
            // The photo's frame in it: the part starts at `first`
            let at = (n - first) as f64 / fps;
            scope.spawn(move || {
                let done = export(file, opts, Some(&part)).map(|_| (TempFile(video), at));
                lap("motion exported");
                done
            })
        });
        let made = image();
        // No image: the motion is of no use either
        // (said here: the stop makes the caller say only "cancelled")
        if let Err(e) = &made { eprintln!("  no photo: {e}"); STOP.store(libc::SIGTERM, Relaxed); }
        let (sdr, hdr, label, cleaned) = made?;
        // --resolution: the image's short side at most that, both renditions alike
        let (sdr, hdr) = match capped((sdr.width, sdr.height), opts.resolution) {
            (w, h) if (w, h) != (sdr.width, sdr.height) => {
                eprintln!("  scaled to {w}x{h} (--resolution)");
                (sdr.downscaled(w, h), hdr.map(|i| i.downscaled(w, h)))
            }
            _ => (sdr, hdr),
        };
        // For tests (tests/uw_export_check.py): the two renditions as written into the file, 16-bit RGB, here
        if let Some(dir) = std::env::var_os("AFP_PHOTO_RENDITIONS").map(PathBuf::from) {
            let stem = out_path.file_stem().unwrap_or_default().to_string_lossy().to_string();
            ph::write_rgb16(&dir.join(format!("{stem}.sdr.{}x{}.rgb16", sdr.width, sdr.height)), &sdr)?;
            if let Some(h) = &hdr { ph::write_rgb16(&dir.join(format!("{stem}.hdr.{}x{}.rgb16", h.width, h.height)), h)?; }
        }
        let live = match motion.map(|m| m.join()) {
            None => None,
            Some(Ok(Ok(v))) => Some(v),
            Some(Ok(Err(e))) if stopped() => return Err(e),
            Some(Ok(Err(e))) => { eprintln!("  no motion, the photo alone: {e}"); None }
            Some(Err(_)) => { eprintln!("  no motion, the photo alone: its export failed"); None }
        };

        let taken = dates.map(|(d, known)| ph::Taken::at(d.unix, d.offset_min, known));
        let link = photo_link(&path, opts, n as f64 / fps);
        if jpeg {
            let exif = ph::exif(taken.as_ref(), make.as_deref(), model.as_deref(), place.map(|p| &p.0));
            let video = match &live { Some((f, _)) => Some(std::fs::read(&f.0).map_err(|e| format!("{}: {e}", f.0.display()))?), None => None };
            let motion = video.as_deref().zip(live.as_ref()).map(|(v, (_, at))| ph::Motion { video: v, presentation_us: (at * 1e6).round() as i64 });
            let bytes = ph::ultra_hdr_jpeg(&sdr, hdr.as_ref(), opts.quality, Some(exif), motion.as_ref(), Some(&link))?;
            std::fs::write(&out_path, bytes).map_err(|e| format!("{}: {e}", out_path.display()))?;
        } else {
            let name = out_path.file_name().unwrap().to_string_lossy().to_string();
            let sdr_file = TempFile(out_dir.join(format!(".{name}.sdr.rgb16")));
            ph::write_rgb16(&sdr_file.0, &sdr)?;
            let hdr_file = match &hdr { Some(h) => { let f = TempFile(out_dir.join(format!(".{name}.hdr.rgb16"))); ph::write_rgb16(&f.0, h)?; Some(f) } None => None };
            let mut cmd = Command::new(&helper);
            cmd.args(["--width", &sdr.width.to_string(), "--height", &sdr.height.to_string(), "--quality", &format!("{:.2}", opts.quality as f64 / 100.0)])
                .arg("--sdr").arg(&sdr_file.0).arg("--out").arg(&out_path);
            if let Some(f) = &hdr_file { cmd.arg("--hdr").arg(&f.0); }
            if let Some(t) = &taken {
                cmd.args(["--taken", &t.local, "--subsec", &t.subsec]);
                if let Some(o) = &t.offset { cmd.args(["--offset", o]); }
            }
            // The Live Photo's video: QuickTime's creation date (local time and zone, as an iPhone writes it)
            if let Some((d, _)) = &dates { cmd.args(["--created", &d.quicktime()]); }
            if let Some(m) = &make { cmd.args(["--make", m]); }
            if let Some(m) = &model { cmd.args(["--model", m]); }
            if let Some((p, _)) = place { cmd.args(["--gps", &gps_arg(p)]); }
            cmd.args(["--link", &link]);
            // A Live Photo: the video copied next to it, the two with one content identifier
            if let Some((video, at)) = &live {
                cmd.arg("--live-video").arg(&video.0).arg("--live-out").arg(out_path.with_extension("mov"))
                    .args(["--content-id", &content_id()?, "--still-time", &format!("{at:.6}")]);
            }
            let out = cmd.output().map_err(|e| format!("afp-photo: {e}"))?;
            if !out.status.success() { return Err(String::from_utf8_lossy(&out.stderr).trim().to_string()); }
        }
        // The files' own dates: the moment's too
        if let Some((d, _)) = &dates {
            let mov = out_path.with_extension("mov");
            for f in std::iter::once(&out_path).chain(live.as_ref().filter(|_| !jpeg).map(|_| &mov)) { set_file_dates(f, d.system_time()); }
        }
        lap("written");
        eprintln!("  {label} · {}x{}{} · {}", sdr.width, sdr.height,
                  if cleaned > 0 { format!(" · cleaned up from {cleaned} frames") } else { String::new() },
                  if hdr.is_some() { if jpeg { "Ultra HDR JPEG" } else { "HEIC with a gain map" } } else if jpeg { "JPEG" } else { "HEIC" });
        if live.is_some() { eprintln!("  {}", if jpeg { "Motion Photo: the video in the JPEG" } else { "Live Photo: the .mov next to it" }); }
        match place {
            Some((p, why)) => eprintln!("  at {:.6}, {:.6}{} ({why})", p.lat, p.lon, p.alt.map(|a| format!(", {a:.1} m")).unwrap_or_default()),
            None => eprintln!("  no place: no GPS in the clip, no location for it in the Library"),
        }
        Ok(Outcome::Done(out_path))
    })
}
