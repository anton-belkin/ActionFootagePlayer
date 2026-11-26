// SPDX-License-Identifier: GPL-3.0-or-later
//
// afp-uw: the underwater colour's numbers for a render of the NAS stream (tools/stream/afp_stream.py pipes them into
// afp-render -uw), measured and eased as gyroflow-export does for an export from the same point (src/uwcolor.rs, the
// twin of the player's uwcolor.lua and uw_estimate.lua): one ffmpeg decodes the clip from the render's start, the frame
// half a second ahead measured every 0.4 s, and each tick of the easing goes out as soon as the measurements it needs
// are in. The render waits for the line it needs, so it never runs ahead of the measuring.

use std::io::{ Read, Write };
use std::path::PathBuf;
use std::process::{ Child, Command, ExitCode, Stdio };
use std::time::Instant;

use vs_gyroflow::uwcolor::{ self as uw, Estimate, Settings, FRAME_BYTES };

const USAGE: &str = "usage: afp-uw <clip> <from> <to> [--strength <0..1>] [--highlights yes|no] [--hwaccel vaapi|videotoolbox]

The underwater colour's numbers (shaders/underwater_hlg.glsl's uw_dr uw_db uw_k uw_ev uw_s uw_hp) for the frames of
<clip> from <from> to <to> (seconds in the file), as gyroflow-export has them for an export of that part: on stdout a
line per 50 ms tick of the easing, `<clock> <uw_dr> <uw_db> <uw_k> <uw_ev> <uw_s> <uw_hp>`, from <from> until the clock
passes <to>. A frame at time t takes the last line whose clock is at most t. Stops when stdout is closed.

  --strength    between the camera's colour (0) and the correction (1, the default): the clip's uw_strength
  --highlights  the light end stretched into HDR's room: the clip's uw_highlights (no by default)
  --hwaccel     ffmpeg's decoder (vaapi on the NAS: only the frames measured leave the GPU); ffmpeg's own if it fails";

/// A measurement run: ffmpeg giving frames, read one at a time
struct Run { child: Child, frame: Vec<u8> }

impl Run {
    fn start(path: &PathBuf, at: f64, count: usize, regular: bool, hwaccel: Option<&str>) -> Option<Run> {
        let mut cmd = Command::new("ffmpeg");
        uw::measure_args(&mut cmd, path, at, count, regular, hwaccel);
        let child = cmd.stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::inherit()).spawn()
            .map_err(|e| eprintln!("afp-uw: ffmpeg: {e}")).ok()?;
        Some(Run { child, frame: vec![0u8; FRAME_BYTES] })
    }

    fn next(&mut self) -> Option<Estimate> {
        self.child.stdout.as_mut()?.read_exact(&mut self.frame).ok()?;
        uw::estimate_bgr0(&self.frame).map_err(|e| eprintln!("afp-uw: {e}")).ok()
    }
}

impl Drop for Run {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut pos = Vec::new();
    let mut settings = Settings::default();
    let mut hwaccel: Option<String> = None;
    let mut it = args.iter();
    while let Some(a) = it.next() {
        let mut value = || it.next().cloned().unwrap_or_default();
        match a.as_str() {
            "--strength" => match value().parse::<f64>() { Ok(v) if (0.0..=1.0).contains(&v) => settings.strength = v, _ => return usage() },
            "--highlights" => match value().as_str() { "yes" | "true" | "1" => settings.highlights = true, "no" | "false" | "0" => settings.highlights = false, _ => return usage() },
            "--hwaccel" => hwaccel = Some(value()).filter(|v| !v.is_empty() && v != "none"),
            "-h" | "--help" => { println!("{USAGE}"); return ExitCode::SUCCESS; }
            _ => pos.push(a.clone()),
        }
    }
    let [path, from, to] = pos.as_slice() else { return usage() };
    let (Ok(from), Ok(to)) = (from.parse::<f64>(), to.parse::<f64>()) else { return usage() };
    let path = PathBuf::from(path);
    let times = uw::measure_times(from, to);
    let t0 = Instant::now();

    // The frames at `times`, from one decoder: on the GPU where asked, else (or where that gave nothing) ffmpeg's own;
    // none there (the render starts within half a second of the clip's end): the frame at its start
    let mut tries = Vec::new();
    if hwaccel.is_some() { tries.push((times[0], times.len(), true, hwaccel.as_deref())); }
    tries.push((times[0], times.len(), true, None));
    tries.push((from, 1, false, None));
    let mut first = None;
    let mut run = None;
    for (k, (at, count, regular, hw)) in tries.into_iter().enumerate() {
        if k > 0 { eprintln!("afp-uw: no frame there, trying from {at:.3} s with ffmpeg's own decoder"); }
        if let Some(mut r) = Run::start(&path, at, count, regular, hw) {
            if let Some(e) = r.next() { first = Some(e); run = Some(r); break; }
        }
    }
    let Some(first) = first else {
        eprintln!("afp-uw: no frame of {} to measure from {from:.3} s", path.display());
        return ExitCode::FAILURE;
    };
    let mut run = run.unwrap();
    let first_after = t0.elapsed().as_secs_f64();

    let mut given = 0usize;
    let mut out = std::io::stdout().lock();
    let mut closed = false;
    uw::track(from, to, &settings,
        || {
            given += 1;
            if given == 1 { Some(first) } else if given > times.len() { None } else { run.next() }
        },
        |clock, p| {
            let ok = writeln!(out, "{clock} {} {} {} {} {} {}", p.dr, p.db, p.k, p.ev, p.s, p.hp).and_then(|_| out.flush()).is_ok();
            closed = !ok;
            ok
        });
    drop(run);
    eprintln!("afp-uw: {} of {} frames measured from {from:.3} s{}, the first in {first_after:.2} s, all in {:.1} s",
              given.min(times.len()), times.len(), if closed { " (the render stopped)" } else { "" }, t0.elapsed().as_secs_f64());
    ExitCode::SUCCESS
}

fn usage() -> ExitCode {
    eprintln!("{USAGE}");
    ExitCode::from(2)
}
