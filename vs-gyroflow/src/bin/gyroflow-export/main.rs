// SPDX-License-Identifier: GPL-3.0-or-later
//
// `gyroflow-export <video>...`: renders videos the way mpv plays them, stabilized and with the log curve's LUT, in one
// pass: decode -> Gyroflow (the plugin's own engine, on the GPU) -> LUT -> encode. The source is decoded and
// encoded once, so there's no intermediate file to lose quality in.
//
// It picks the stabilization the same way the gyroflow mpv plugin does (a .gyroflow next to the video, or the clip's
// own motion data with the default preset) and the log curve from the camera's metadata, as the dlogm plugin does
// (DJI's ColorMode, GoPro's Protune color; --curve sets it). A log clip with
// nothing to stabilize it with, or with --no-stabilize, is color-converted only (no Gyroflow in the pipeline); a
// clip with neither is skipped, there being nothing to change. --wide and --edges give it the gyroflow plugin's wider
// view and edge fill (edges.rs), as the player plays them.
//
// Installed in the gyroflow plugin's folder (~~/scripts/gyroflow/), it takes the default preset from there and the
// LUTs from the dlogm plugin's (~~/scripts/dlogm/luts/); $GYROFLOW_DATA and $DLOGM_DATA, when set, come first,
// as for the plugins (ActionFootagePlayer keeps your preset and DJI's LUT there, its own copy of the plugins being
// read-only).
//
// Output: <project>/_Exports/<name>_stab.mov (with --from/--to, <name>_stab_<from>-<to>s.mov; color only, _color
// instead of _stab), 10-bit. The project is the one the player's workspace plugin finds (vs-gyroflow/src/workspace.rs);
// with none, <video's folder>/stabilized/. --out and --output write elsewhere
//   log:            HEVC Main10, Rec.2100 HLG (luts/<curve>_hlg.cube); D-Log M in Rec.709 with --sdr (DJI's LUT),
//                   any curve in Rec.709 with --nbcu (its HLG through NBCUniversal's HLG -> SDR, luts/3-NBCU_*)
//   HDR (HLG, PQ):  HEVC Main10 as it is; in Rec.709 with --nbcu (NBCUniversal's HLG or PQ -> SDR, luts/3-, 5-NBCU_*)
//   anything else:  HEVC Main10 in the source's own colors
//   --prores:       ProRes 422 HQ instead of HEVC
//   --dv-rpu:       HLG as Dolby Vision 8.4, PQ as 8.1, with the file's RPUs (src/dv_mux.rs; <name>_stab_dv.mov), macOS
//   --youtube:      HDR10 for YouTube: PQ (log and HLG through BT.2100's 1000-nit OOTF), the mastering display and
//                   MaxCLL/MaxFALL, in an MKV with a .cube inside for YouTube's SDR version in the player's SDR look
//                   (src/youtube.rs; <name>_stab_youtube.mkv)
//   --lens-only:    the lens corrected (and its projection), nothing smoothed: with the stabilization off, or for a
//                   clip with a lens profile and no motion data
//   --lens-recorded: the lens left as recorded (no correction) while it's stabilized
// Audio is copied as it is; the file gets the source's dates.
//
// Decoding and encoding are ffmpeg (Homebrew's on macOS; on Windows and Linux the one next to this binary, else the
// one in PATH), as child processes with raw frames over pipes; the LUT is ffmpeg's lut3d filter, in RGB, the same
// place mpv's shader applies it. The encoder is VideoToolbox on macOS; elsewhere the first of NVENC, Quick Sync,
// AMF (Windows) and VAAPI (Linux) that encodes a test frame, else x265 on the CPU (--encoder picks one).
//
// For a player running it (the gyroflow plugin's export.lua): --progress writes the progress to a file twice a
// second; SIGTERM (or Ctrl+C) stops it, the ffmpegs too, and removes the unfinished file; SIGUSR1 puts it and the
// ffmpegs at background priority (macOS's lowest, CPU, disk and GPU: playback keeps its frame rate, the export
// nearly stops) and SIGUSR2 back. Without macOS's background priority (Linux, Windows), background is a few frames
// a second instead. Windows has no such signals: --control <file> takes the same as a word in a file, polled
// (stop, background, normal).

use std::io::{ Read, Write };
use std::path::{ Path, PathBuf };
use std::process::{ Command, Stdio };
use std::sync::{ Arc, mpsc };
use std::sync::atomic::{ AtomicBool, AtomicI32, Ordering::Relaxed };
use std::collections::BTreeMap;
use std::time::Instant;

use parking_lot::Mutex;
use vs_gyroflow::dv_mux::{ self, RpuInjector };
use vs_gyroflow::{ Engine, Fov, Interpolation, Level, PlaneJob, SampleKind, Underwater, probe, gpu_lut::{ self, GpuLut }, edges::{ EdgeMode, EdgeSettings } };

// The log curves, as the dlogm plugin names them, and the underwater colour, which the player shows as one (its LUT:
// Uw's, made per clip): registry.json's (vs_gyroflow::lists), each with its LUT to HLG in luts/
use vs_gyroflow::lists::{ curve_lut as curve_lut_file, curve_name };

// Homebrew's ffmpeg: a Finder Quick Action or mpv doesn't have /opt/homebrew/bin in its PATH. On Windows, where
// ffmpeg is rarely in PATH, one copied next to this binary (the gyroflow plugin's folder) comes first. The one in
// GYROFLOW_TOOLS (ActionFootagePlayer.app's own) comes before all of them
use vs_gyroflow::tools::{ tool, app_contents };

// Its parts, each `use super::*;`: their items are the program's, as when they were one file
mod options;
mod dates;
mod source;
mod encode;
mod preview;
mod color;
mod translation;
mod video;
mod post;
mod gpu;
mod photo;
mod sphere;
#[cfg(test)]
mod tests;

use options::*;
use dates::*;
use source::*;
use encode::*;
use preview::*;
use color::*;
use translation::*;
use video::*;
use post::*;
use gpu::*;
use photo::*;
use sphere::*;

fn main() {
    let first = std::env::args().nth(1);
    if first.as_deref() == Some("--fingerprint") {
        let mut failed = false;
        for f in std::env::args().skip(2) {
            match vs_gyroflow::workspace::fingerprint(Path::new(&f)) {
                Ok(h) => println!("{h}"),
                Err(e) => { eprintln!("{f}: {e}"); println!(); failed = true; }
            }
        }
        std::process::exit(if failed { 1 } else { 0 });
    }
    vs_gyroflow::init_logging();
    catch_signals();
    let (mut opts, files) = parse_args(std::env::args().skip(1));
    if files.is_empty() { fail(USAGE); }
    if opts.project.is_some() && files.len() > 1 { fail("--project is for one video"); }
    if opts.output.is_some() && files.len() > 1 { fail("--output is for one video"); }
    if opts.preview.is_none() && (opts.at.is_some() || opts.dates.is_some() || opts.cover.is_some()) { fail("--at, --cover and --dates are for --preview"); }
    if opts.picture.is_some() && (opts.preview != Some(Preview::Still) || opts.output.is_none() || opts.from.is_none() || opts.to.is_none()) {
        fail("--picture is for --preview still, with --output, --from and --to");
    }
    if opts.cover.is_some() && opts.preview != Some(Preview::Flipbook) { fail("--cover is for a flipbook: a still's cover is its frame"); }
    if opts.preview.is_some() && (opts.prores || opts.dv_rpu.is_some()) { fail("a preview is H.264: not with --prores or --dv-rpu"); }
    if opts.youtube.is_some() && (opts.sdr || opts.nbcu || opts.dv_rpu.is_some() || opts.prores || opts.preview.is_some()
                                  || opts.photo.is_some() || opts.translation || opts.sphere360.is_some()) {
        fail("--youtube is an HDR video export of its own: not with --sdr, --nbcu, --dv-rpu, --prores, --preview, --photo, --translation, --360 or --reframe");
    }
    if let Some(p) = opts.project.as_ref().filter(|p| !p.is_file()) { fail(&format!("no Gyroflow project {}", p.display())); }
    if let (Some(f), Some(t)) = (opts.from, opts.to) { if t <= f { fail("--to must be after --from"); } }

    // The pid first: a player stops or reprioritizes it by it, from the start
    opts.progress.write(serde_json::json!({ "pid": std::process::id() }));
    let mut failed = 0;
    let link = opts.link.clone();
    for file in &files {
        // The original's fingerprint in its link, to find it again once it's moved (workspace::fingerprint)
        opts.link = link.as_deref().map(|l| vs_gyroflow::workspace::with_fingerprint(l, Path::new(file)));
        let outcome = if let Some(m) = &opts.sphere360 { export_360(file, &opts, m) }
            else if let Some(t) = opts.photo { photo(file, &opts, t) }
            else if opts.translation { translation(file, &opts) }
            else if opts.preview == Some(Preview::Flipbook) { flipbook(file, &opts) }
            else if let Some(p) = &opts.picture { still_from_picture(file, &opts, p) } else { export(file, &opts, None) };
        match outcome {
            Ok(Outcome::Done(out)) => eprintln!("  -> {}", out.display()),
            Ok(Outcome::Skipped(why)) => eprintln!("{}: skipped, {why}", file_name(file)),
            Err(_) if stopped() => {
                eprintln!("{}: cancelled", file_name(file));
                std::process::exit(128 + STOP.load(Relaxed));
            }
            Err(e) => { eprintln!("{}: failed, {e}", file_name(file)); failed += 1; }
        }
    }
    if failed > 0 { std::process::exit(1); }
}

fn fail(msg: &str) -> ! {
    eprintln!("{msg}");
    std::process::exit(2);
}

// Set by the signal handlers: the signal that asked to stop, and the priority asked for
static STOP: AtomicI32 = AtomicI32::new(0);
static BACKGROUND: AtomicBool = AtomicBool::new(false);

extern "C" fn on_signal(sig: libc::c_int) {
    match sig {
        #[cfg(unix)]
        libc::SIGUSR1 => BACKGROUND.store(true, Relaxed),
        #[cfg(unix)]
        libc::SIGUSR2 => BACKGROUND.store(false, Relaxed),
        _ => STOP.store(sig, Relaxed),
    }
}

fn catch_signals() {
    #[cfg(unix)]
    let signals = [libc::SIGINT, libc::SIGTERM, libc::SIGHUP, libc::SIGUSR1, libc::SIGUSR2];
    #[cfg(windows)]
    let signals = [libc::SIGINT, libc::SIGTERM];
    for sig in signals {
        unsafe { libc::signal(sig, on_signal as extern "C" fn(libc::c_int) as libc::sighandler_t); }
    }
}

fn stopped() -> bool { STOP.load(Relaxed) != 0 }
// AFP_TIMES: the GPU workers' time in the engine and the LUT, summed over the workers
static ENGINE_NS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
static LUT_NS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

// --control: the word in the file, as the signals would set it
fn read_control(path: &Path) {
    match std::fs::read_to_string(path).unwrap_or_default().trim() {
        "stop" => STOP.store(libc::SIGTERM, Relaxed),
        "background" => BACKGROUND.store(true, Relaxed),
        "normal" => BACKGROUND.store(false, Relaxed),
        _ => {}
    }
}

// PRIO_DARWIN_BG, for these processes (0: this one). Set on the ffmpegs too: the decoding and encoding compete
// with the player's as much as the GPU work does
#[cfg(target_os = "macos")]
fn set_background(pids: &[u32], on: bool) {
    for &pid in pids {
        unsafe { libc::setpriority(libc::PRIO_DARWIN_PROCESS, pid as libc::id_t, if on { libc::PRIO_DARWIN_BG } else { 0 }); }
    }
}

// Nothing like it elsewhere: a raised nice value can't be lowered again without privileges on Linux, and the GPU
// and the video engines have no priority to set. The export loop slows itself down instead (THROTTLE)
#[cfg(not(target_os = "macos"))]
fn set_background(_pids: &[u32], _on: bool) {}

// Background without macOS's priority: at most a frame per this long, a few fps that leave the GPU to playback
const THROTTLE: std::time::Duration = std::time::Duration::from_millis(150);

// --progress: the file is replaced whole (written next to it, then renamed), so a reader never sees half of it
struct Progress(Option<PathBuf>);

impl Progress {
    fn write(&self, value: serde_json::Value) {
        let Some(path) = &self.0 else { return };
        let tmp = PathBuf::from(format!("{}.tmp", path.display()));
        if std::fs::write(&tmp, value.to_string()).is_ok() { let _ = std::fs::rename(&tmp, path); }
    }
}
