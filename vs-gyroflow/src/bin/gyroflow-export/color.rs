// The look: the log curves' LUTs, NBCUniversal's HLG/PQ to SDR, the underwater colour.
// One of gyroflow-export's parts: main.rs has the uses and main(); each part's items are the program's.

use super::*;

// NBCUniversal's HLG -> SDR conversion (Type III), in the dlogm plugin's luts/
pub(crate) const NBCU_HLG: &str = "3-NBCU_HLG2SDR_DL_v1.2.cube";
pub(crate) const NBCU_PQ: &str = "5-NBCU_PQ2SDR_DL_v1.2.cube";

// Tagged HDR (HLG or PQ) in its MP4/MOV, as the dlogm plugin reads it (video_color.lua): never log
pub(crate) fn hdr_in(path: &Path) -> bool {
    ffprobe(path).is_ok_and(|i| matches!(i.transfer.as_deref(), Some("arib-std-b67" | "smpte2084")))
}

pub(crate) fn yes_no(name: &str, v: &str) -> bool {
    match v { "yes" | "true" | "1" => true, "no" | "false" | "0" => false, _ => fail(&format!("{name} takes yes or no")) }
}

// The clip's curve as the dlogm plugin picks it: --curve, else the camera's metadata; with --uw-color, the underwater
// colour ("uw") for a clip with none that isn't HDR either (dlogm's uw_ok)
pub(crate) fn clip_curve(opts: &Options, path: &Path) -> Option<&'static str> {
    let curve = opts.curve.unwrap_or_else(|| probe::log_curve(&path.to_string_lossy()));
    if curve.is_none() && opts.uw && !hdr_in(path) { Some("uw") } else { curve }
}

// The underwater colour of one export (src/uwcolor.rs): its numbers frame by frame as playback has them, and a LUT of
// them (their median, for a video) for the paths that take only one: ffmpeg's lut3d, the GPU path's look shader,
// YouTube's, a flipbook, a 360 export. A video on the pipes gets each frame's own on the GPU (gpu_lut.rs)
pub(crate) struct Uw {
    schedule: vs_gyroflow::uwcolor::Schedule,
    settings: vs_gyroflow::uwcolor::Settings,
    // The same numbers throughout (a photo, a still, --uw-params): the GPU path can take it
    pub(crate) fixed: bool,
    pub(crate) cube: TempFile,
}

// What an export of the underwater colour is measured for
pub(crate) enum UwSpan {
    // One frame, as the player measures the frame it's paused on
    Still(f64),
    // Played from `from` to `to`, as playback measures and eases
    Played(f64, f64),
    // Frames here and there (a flipbook, a 360 export): one set of numbers, their median
    Median(Vec<f64>),
}

// uwcolor.rs's cube, 33 points a side as the curves' own LUTs (within 0.01 of the shader where it's smooth)
const UW_CUBE: usize = 33;

impl Uw {
    pub(crate) fn new(path: &Path, opts: &Options, span: UwSpan, dir: &Path, stem: &str) -> Result<Uw, String> {
        use vs_gyroflow::uwcolor::{ self as uw, Schedule };
        let (schedule, fixed) = match (opts.uw_params, span) {
            (Some(e), _) => (Schedule::fixed(e), true),
            (None, UwSpan::Still(t)) => (Schedule::fixed(uw_measure(path, &[t])?[0]), true),
            (None, UwSpan::Median(times)) => (Schedule::fixed(Schedule::new(0.0, uw_measure(path, &times)?).median()), true),
            (None, UwSpan::Played(from, to)) => (Schedule::new(from, uw_measure(path, &uw::measure_times(from, to))?), false),
        };
        let p = uw::Params::new(&schedule.median(), &opts.uw_settings).clamped();
        let cube = TempFile(dir.join(format!(".{stem}.uw.cube")));
        std::fs::write(&cube.0, p.cube(UW_CUBE, "the underwater colour, by gyroflow-export")).map_err(|e| format!("{}: {e}", cube.0.display()))?;
        Ok(Uw { schedule, settings: opts.uw_settings, fixed, cube })
    }

    /// The shader's numbers at each of `times` (seconds in the clip)
    pub(crate) fn params_at(&self, times: &[f64]) -> Vec<[f32; 6]> {
        self.schedule.at(times).iter().map(|e| vs_gyroflow::uwcolor::Params::new(e, &self.settings).clamped().array()).collect()
    }

    pub(crate) fn label(&self) -> String {
        let p = vs_gyroflow::uwcolor::Params::new(&self.schedule.median(), &self.settings);
        format!("{}strength {:.2}{}", if self.fixed { format!("red {:+.4}, blue {:+.4}, leak {:.4}, lift {:+.4} EV, ", p.dr, p.db, p.k, p.ev) } else { "measured as it plays, ".into() },
                p.s, if p.hp != 1.0 { ", highlights" } else { "" })
    }
}

// The underwater colour's numbers for the frames at `times` (seconds), as the player's uwcolor.lua measures one: the
// frame decoded (VideoToolbox on macOS), 960x540 by area, bgr0 (uwcolor::estimate_bgr0). Times MIN_GAP apart from
// one decoder (a video's), others each by its own
pub(crate) fn uw_measure(path: &Path, times: &[f64]) -> Result<Vec<vs_gyroflow::uwcolor::Estimate>, String> {
    use vs_gyroflow::uwcolor::{ self as uw, GRID, MIN_GAP };
    let frame_bytes = GRID.0 * 3 * GRID.1 * 3 * 4;
    let run = |at: f64, count: usize, regular: bool| -> Result<Vec<uw::Estimate>, String> {
        let mut cmd = ffmpeg_command("ffmpeg");
        cmd.args(["-v", "error", "-nostdin"]);
        if cfg!(target_os = "macos") { cmd.args(["-hwaccel", "videotoolbox"]); }
        let scale = format!("scale={}:{}:flags=area", GRID.0 * 3, GRID.1 * 3);
        let vf = if regular { format!("fps=fps={:.6},{scale}", 1.0 / MIN_GAP) } else { scale };
        cmd.args(["-ss", &format!("{:.3}", at.max(0.0)), "-i"]).arg(path)
            .args(["-map", "0:v:0", "-frames:v", &count.to_string(), "-vf", &vf, "-f", "rawvideo", "-pix_fmt", "bgr0", "-"])
            .stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped());
        let mut child = cmd.spawn().map_err(|e| format!("ffmpeg: {e}"))?;
        let mut out = child.stdout.take().unwrap();
        let mut got = Vec::with_capacity(count);
        let mut buf = vec![0u8; frame_bytes];
        while got.len() < count && out.read_exact(&mut buf).is_ok() {
            if stopped() { let _ = child.kill(); return Err("cancelled".into()); }
            got.push(uw::estimate_bgr0(&buf)?);
        }
        let _ = child.kill();
        let status = child.wait_with_output().map_err(|e| format!("ffmpeg: {e}"))?;
        if got.is_empty() { return Err(format!("the underwater colour: no frame at {at:.2} s ({})", String::from_utf8_lossy(&status.stderr).trim())); }
        Ok(got)
    };
    let regular = times.len() > 2 && times.windows(2).all(|w| (w[1] - w[0] - MIN_GAP).abs() < 1e-6);
    let mut v = if regular {
        run(times[0], times.len(), true)?
    } else {
        // A few at a time
        let mut v = Vec::with_capacity(times.len());
        for chunk in times.chunks(4) {
            let got: Vec<Result<Vec<uw::Estimate>, String>> = std::thread::scope(|s| {
                let hs: Vec<_> = chunk.iter().map(|&t| s.spawn(move || run(t, 1, false))).collect();
                hs.into_iter().map(|h| h.join().unwrap_or_else(|_| Err("the underwater colour: a measurement failed".into()))).collect()
            });
            for g in got { v.push(g?[0]); }
        }
        v
    };
    // Past the clip's end: its last frame's
    while v.len() < times.len() { let last = *v.last().unwrap(); v.push(last); }
    Ok(v)
}

// A file removed when it goes out of scope, however the export ends
pub(crate) struct TempFile(pub(crate) PathBuf);

impl Drop for TempFile {
    fn drop(&mut self) { let _ = std::fs::remove_file(&self.0); }
}

pub(crate) fn lut(name: &str) -> Result<PathBuf, String> {
    let p = plugin_file("dlogm", &format!("luts/{name}"));
    if p.is_file() { Ok(p) } else { Err(format!("{} is missing (install the dlogm plugin: install.sh)", p.display())) }
}

// The YUV matrix the video is decoded with, as mpv does: the one it's tagged with, else BT.709 (DJI tags D-Log M
// BT.709; a GoPro's or Insta360's 10-bit may be tagged BT.2020). By the names of ffmpeg's scale and gpu_lut
pub(crate) fn yuv_matrix(tag: Option<&str>) -> &'static str {
    match tag {
        Some("bt2020nc" | "bt2020c") => "bt2020",
        Some("smpte170m" | "bt470bg") => "bt601",
        _ => "bt709",
    }
}

// ffmpeg's filter syntax: quote the path for the option value, then for the filter graph
pub(crate) fn filter_path(p: &Path) -> String {
    let s = p.to_string_lossy().replace('\\', "\\\\").replace('\'', "\\'").replace(':', "\\:");
    format!("'{s}'")
}

// The low-resolution copy a camera writes beside the clip, quicker to decode and far less to read from a NAS:
// DJI's <name>.LRF (the same log curve, checked on an Osmo Action 6's D-Log M), GoPro's GL<rest>.LRV for
// GX<rest>.MP4 or GH<rest>.MP4
pub(crate) fn lowres(path: &Path) -> Option<PathBuf> {
    let dir = path.parent()?;
    let stem = path.file_stem()?.to_string_lossy().to_string();
    let mut names = vec![format!("{stem}.LRF"), format!("{stem}.lrf")];
    if stem.len() > 2 && (stem.starts_with("GX") || stem.starts_with("GH")) {
        names.push(format!("GL{}.LRV", &stem[2..]));
        names.push(format!("GL{}.lrv", &stem[2..]));
    }
    // An empty or cut-short one (the camera stopped before writing it: seen, 0 bytes) isn't one
    names.into_iter().map(|n| dir.join(n)).find(|p| std::fs::metadata(p).is_ok_and(|m| m.is_file() && m.len() >= 1024))
}
