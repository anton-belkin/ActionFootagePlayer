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
// view and edge fill (edges.rs), as the plugin passes them to the VapourSynth filter.
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

const USAGE: &str = "usage: gyroflow-export [options] <video>...
       gyroflow-export --fingerprint <file>...  (each file's fingerprint, as a link keeps its original's: SHA-256 of
                                                 its first and last MiB)

Renders each video stabilized (and, for log, color-converted) into the _Exports/ folder of its project (the trip
folder with Project.afproject in it above the video, as the player initializes it), else into a stabilized/
folder next to it. A log video with nothing to stabilize it with is color-converted only.

  --curve <curve>  the log curve: dlogm, dlog, gplog, gplog2, ilog, or none (colors as recorded) instead of the
                   one the camera's metadata says
  --sdr            D-Log M to Rec.709 (DJI's LUT) instead of HLG
  --nbcu           to Rec.709 through NBCUniversal's HLG -> SDR conversion instead of HLG (any curve); an HDR file
                   that isn't log through NBCU's HLG or PQ -> SDR
  --no-stabilize   the color conversion only, as a clip plays with the stabilization off
  --lens-only      the lens corrected (and --projection), nothing smoothed: the stabilization off with the lens
                   kept, or a clip with a lens profile and no motion data
  --lens-recorded  stabilized with the lens left as recorded (no lens correction, no projection)
  --horizon-lock <%>  the horizon lock's amount (0-100, 0 off) instead of the project's or preset's
  --underwater <yes|no|auto|project>  the lens under water or not (Gyroflow's \"Lens is underwater\"), or as the
                   camera says over the clip, the default: its In water / Out of water (Osmo Action 6), elsewhere the
                   project's or preset's; project: the project's or preset's, whatever the camera says
  --wide           the gyroflow plugin's wider view (Ctrl+g): no zoom, at the FOV where the stabilized picture
                   covers about as much area as the frame
  --level <level>  for a video without a project, the stabilization's level: weak, medium (the default) or strong:
                   10, 20 or 30 % of the frame cropped, smoothness 50, 75 or 100 % (Ctrl+s)
  --fov <x>        Gyroflow's FOV (Advanced) instead of the project's
  --rotation <deg> the picture turned clockwise by 0, 90, 180 or 270 degrees (the camera on its side), in place of
                   the project's rotation; not stabilized, ffmpeg's transpose. On top of the turn the file asks for (a
                   camera held vertical: its display matrix), which every output has: upright as players show it
  --aspect <W:H>   the output cut to this aspect ratio after the rotation, such as 16:9, 4:3, 1:1, 4:5, 9:16: the
                   largest centered rectangle of it, sides rounded down to even (original, the default: the whole
                   frame); in place of the project's output size. Stabilized, Gyroflow's zoom fits the picture to it
  --projection <p> Gyroflow's output projection: rectilinear, stereographic, equidistant, equisolid or
                   orthographic, in place of the project's or preset's (rectilinear without one). Stabilized only: with
                   --no-stabilize, or nothing to stabilize with, there's no lens model and it's ignored
  --edges <mode>   what fills the edges the stabilized frame leaves: sphere, smooth, or off (the project's
                   background color, the default)
  --edges-blur <%> blur of the edge fill, % of the frame height (default 2)
  --prores         ProRes 422 HQ instead of HEVC 10-bit
  --dv-rpu <file>  HLG as Dolby Vision 8.4 (PQ as 8.1: an RPU made with afp-rpu --profile 8.1), with this file's RPUs (dovi_tool's or afp-rpu's .rpu: frame N of the
                   video gets RPU N, the last the frames after it) in place of VideoToolbox's, and the dvvC box Apple
                   needs to use them; the player's HDR + Dolby Vision look plays the same file (macOS, HLG only)
  --dv-live-l1     with --dv-rpu: each frame's own L1 in its RPU, measured on the GPU as the player's mpv measures
                   what it shows (--macos-dovi-live-l1): the lowest, average and highest max(R,G,B) of the frame
                   written, in PQ
  --from <time>    start at this time (seconds, or m:ss)
  --to <time>      end at this time
  --out <folder>   write here instead of <project>/Exports (or stabilized/)
  --output <file>  write this file (one video only)
  --alone          --from to --to stabilized on its own: its smoothing from its own motion, not the clip's around it
                   (a moment's); with a project, its \"only within the trim range\" on, whatever it says
  --whole          with a project, its \"only within the trim range\" off, whatever it says
  --link <json>    the link to the original the player keeps in an export (an MP4 key, ch.actionfootageplayer.link:
                   written in room ffmpeg leaves after the moov, so the file isn't written twice). Its `hash`, the
                   video's fingerprint, is added when it has none (as --fingerprint prints it)
  --project <file> stabilize with this Gyroflow project instead of the video's own (one video only): the player
                   passes the one it plays with. The video's own is <name>.gyroflow or <name.ext>.gyroflow next to
                   it, as the player's workspace plugin finds it
  --force          replace an existing output instead of skipping the video
  --interpolation  bilinear | bicubic | lanczos (default lanczos)
  --encoder <name> ffmpeg's HEVC encoder, e.g. hevc_nvenc, hevc_qsv, hevc_amf, hevc_vaapi, libx265
                   (default: VideoToolbox on macOS, else the first of these that works)
  --progress <file>  write the progress there as JSON, twice a second
  --background     start at background priority (SIGUSR1 / SIGUSR2 switch it later)
  --control <file> read stop / background / normal from this file while running (for Windows, which has no
                   SIGUSR1/SIGUSR2)

Previews, for the player's moments (docs/moments-and-workspaces.md): small H.264 MP4s in SDR that any system
shows and plays, the frame also their cover art (the icon)
  --preview still  one frame, stabilized as with the options above, in SDR (the maker's LUT for D-Log M, else
                   NBCU's conversion; HLG through NBCU's), held for --from to --to (the whole video without them);
                   by default <name>.afplink.mp4 in --out or next to the video (the player's
                   moments are _Moments/<moment>.afplink.mp4)
  --preview flipbook  the whole clip at a glance, for the player's _Clips/: a frame at each --at time (the keyframe
                   at or before it, so only keyframes are decoded), 2 a second, in SDR as for a still and not
                   stabilized, from the low-resolution copy beside the clip when there is one (DJI's .LRF, GoPro's
                   .LRV); without --at, about 12 frames through the clip, 10 s apart at least, after its first second
  --at <time>      the frame's time (default: the middle of --from to --to); a flipbook's: times, comma-separated
  --cover <time>   a flipbook's cover art (its icon): the frame nearest this time (default: the middle)
  --link <json>    the link to the original the player keeps in it (an MP4 key, ch.actionfootageplayer.link), its
                   `hash` added as for an export
  --dates <time>   its date, ISO 8601 with the time zone (2026-10-22T15:28:42+02:00): the MP4's creation time,
                   QuickTime's creation date (Photos' day) and the file's dates; without it, the video's dates
A 360 camera's own file (an Insta360 .insv: docs/360-cameras.md), as the player shows it (the sphere stitched from its
lenses, the horizon level by gravity, pointed along a heading mode); --from, --to, --out, --output, --force,
--encoder, --progress as above:
  --360            a stabilized 360 video: the whole sphere turned into the world and the heading, equirect, with
                   spherical metadata (YouTube, editors, VLC): <name>_360.mp4
  --reframe <keys> a flat video looking along the head's keyframes, \"t:yaw,pitch,roll,fov;...\" (seconds of the
                   clip, degrees: yaw right, pitch up, roll counterclockwise, the FOV across a 16:9 picture as in the
                   player; eased between keys, held before the first and after the last): <name>_reframe.mp4
  --heading <mode> what straight ahead follows: travel (along the way you move: GPS), camera, north, world. By
                   default travel where the clip has it, else camera, as the player picks
  --sphere-width <px>  the sphere's width (default 5760; the 360 video's size, its height half of it)
  --size <WxH>     a reframe's size (default 1920x1080)
  --view <p>       a reframe's projection: auto (default), rectilinear, stereographic, planet
Photos: --photo <time> the frame at that time as a still instead, with a gain map where there's an HDR look: an SDR
rendition as the image, the HDR one (HLG) above it
  --sdr-image <maker|nbcu|macos>  the SDR image: DJI's LUT (D-Log M; the default for it), NBCU's conversion of the
                   HLG (the default for the others), or the HLG as macOS shows it on an SDR screen (afp-photo)
  --radius <n>     clean it up from n frames on each side (0-7, default 5; 0: as it is), as the player's Ctrl+p
  --grain <x>      how much of its grain the clean-up keeps (0-1, default 0.25)
  --camera <yaw,pitch,roll,zoom>  the camera turned and zoomed as crop mode has it (the gyroflow plugin's crop.lua;
                   a video export takes it too)
  --crop <x,y,w,h> the part of the frame to keep, in shares of its width and height (0-1; a video export's too),
                   of the frame as --rotation and --aspect have it
  --format <heic|jpeg>  HEIC (macOS: afp-photo) or Ultra HDR JPEG (the default elsewhere)
  --live           with the motion around it, 1.5 s on each side, as the export plays it (--sdr, --nbcu, --dv-rpu
                   say how; at most 1920 wide or high): a HEIC becomes a Live Photo (a .mov next to it, paired by
                   afp-photo), a JPEG a Motion Photo (<name>.MP.jpg, the MP4 inside it, as Android's cameras write it;
                   its video in the image's SDR look, which the apps showing it all play alike)
  --quality <0-100>  the image's quality (default 92)";

// The log curves, as the dlogm plugin names them; luts/<id>_hlg.cube is each one's LUT to HLG
const CURVES: &[(&str, &str)] = &[("dlogm", "D-Log M"), ("dlog", "D-Log"), ("gplog", "GP-Log"), ("gplog2", "GP-Log2"), ("ilog", "I-Log")];

struct Options {
    // None: from the metadata; Some(None): none
    curve: Option<Option<&'static str>>,
    sdr: bool,
    nbcu: bool,
    no_stabilize: bool,
    // --lens-only, --lens-recorded: the lens corrected with nothing smoothed, or left as recorded while stabilized
    lens: vs_gyroflow::LensMode,
    horizon_lock: Option<f64>,
    underwater: Underwater,
    fov: Fov,
    level: Level,
    // --rotation, --aspect, --projection: the output frame turned and cut, its projection (vs_gyroflow::Frame)
    frame: vs_gyroflow::Frame,
    edges: EdgeMode,
    edge_settings: EdgeSettings,
    prores: bool,
    from: Option<f64>,
    to: Option<f64>,
    out: Option<PathBuf>,
    project: Option<PathBuf>,
    force: bool,
    interpolation: Interpolation,
    encoder: Option<String>,
    progress: Progress,
    control: Option<PathBuf>,
    dv_rpu: Option<PathBuf>,
    dv_live_l1: bool,
    output: Option<PathBuf>,
    // --alone: the part stabilized on its own
    alone: bool,
    // --alone / --whole: a project's "only within the trim range", whatever it says (as a moment is viewed)
    alone_project: Option<bool>,
    preview: Option<Preview>,
    at: Option<f64>,
    // --at for a flipbook: its frames' times
    at_list: Vec<f64>,
    cover: Option<f64>,
    link: Option<String>,
    dates: Option<Dates>,
    photo: Option<f64>,
    photo_radius: usize,
    photo_grain: f32,
    camera: Option<vs_gyroflow::Camera>,
    crop: Option<[f64; 4]>,
    jpeg: bool,
    quality: u8,
    sdr_image: Option<String>,
    live: bool,
    // A 360 camera's file: --360 or --reframe (export_360)
    sphere360: Option<Mode360>,
}

struct Mode360 {
    // None: the whole sphere (--360); else the head's keyframes (--reframe)
    keys: Option<Vec<vs_gyroflow::export360::HeadKey>>,
    heading: Option<vs_gyroflow::stamp::Heading>,
    sphere_width: usize,
    size: (usize, usize),
    projection: vs_gyroflow::view360::Projection,
}

impl Default for Mode360 {
    fn default() -> Self { Mode360 { keys: None, heading: None, sphere_width: 0, size: (1920, 1080), projection: vs_gyroflow::view360::Projection::Auto } }
}

// What --preview makes (flipbooks and video previews come later)
#[derive(Clone, Copy, PartialEq, Debug)]
enum Preview { Still, Flipbook }

// --dates: a time and the time zone it was given in
#[derive(Clone, Copy, Debug, PartialEq)]
struct Dates { unix: f64, offset_min: i32 }

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
    let mut opts = Options { curve: None, sdr: false, nbcu: false, no_stabilize: false, lens: vs_gyroflow::LensMode::Stabilized, horizon_lock: None, underwater: Underwater::Auto, fov: Fov::Project, level: Level::Medium, frame: vs_gyroflow::Frame::default(),
                             edges: EdgeMode::Off, edge_settings: EdgeSettings::default(), prores: false, from: None, to: None, out: None, project: None, force: false,
                             interpolation: Interpolation::Lanczos4, encoder: None, progress: Progress(None), control: None,
                             dv_rpu: None, dv_live_l1: false, output: None, alone: false, alone_project: None, preview: None, at: None, at_list: vec![], cover: None, link: None, dates: None,
                             photo: None, photo_radius: 5, photo_grain: 0.25, camera: None, crop: None,
                             jpeg: !cfg!(target_os = "macos"), quality: 92, sdr_image: None, live: false, sphere360: None };
    let mut files = Vec::new();
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        let mut value = |name: &str| args.next().unwrap_or_else(|| fail(&format!("{name} needs a value")));
        match a.as_str() {
            "--curve" => opts.curve = Some(match value("--curve").as_str() {
                "none" => None,
                c => Some(CURVES.iter().find(|(id, _)| *id == c).map(|(id, _)| *id)
                    .unwrap_or_else(|| fail(&format!("unknown curve {c} (dlogm, dlog, gplog, gplog2, ilog, none)")))),
            }),
            "--sdr" => opts.sdr = true,
            "--nbcu" => opts.nbcu = true,
            "--no-stabilize" => opts.no_stabilize = true,
            "--lens-only" => opts.lens = vs_gyroflow::LensMode::Only,
            "--lens-recorded" => opts.lens = vs_gyroflow::LensMode::Recorded,
            "--horizon-lock" => opts.horizon_lock = Some(value("--horizon-lock").parse::<f64>().ok().filter(|v| (0.0..=100.0).contains(v))
                .unwrap_or_else(|| fail("--horizon-lock takes 0-100"))),
            "--underwater" => opts.underwater = Underwater::parse(&value("--underwater")).unwrap_or_else(|| fail("--underwater takes yes, no, auto or project")),
            // The ranges the VapourSynth filter takes them in (lib.rs)
            "--wide" => opts.fov = Fov::Wide,
            "--level" => { let v = value("--level"); opts.level = Level::parse(&v).unwrap_or_else(|| fail(&format!("unknown level {v} (weak, medium, strong)"))); }
            "--fov" => opts.fov = Fov::Set(value("--fov").parse::<f64>().ok().filter(|v| *v > 0.0)
                .unwrap_or_else(|| fail("--fov takes a number above 0")).clamp(0.1, 4.0)),
            "--rotation" => { let v = value("--rotation"); opts.frame.rotation = v.trim().parse::<i64>().ok().and_then(vs_gyroflow::Frame::parse_rotation)
                .unwrap_or_else(|| fail(&format!("--rotation {v}: 0, 90, 180 or 270"))); }
            "--aspect" => { let v = value("--aspect"); opts.frame.aspect = vs_gyroflow::Frame::parse_aspect(&v)
                .unwrap_or_else(|| fail(&format!("--aspect {v}: original or W:H, such as 16:9 or 9:16"))); }
            "--projection" => { let v = value("--projection"); opts.frame.projection = Some(vs_gyroflow::Frame::parse_projection(&v)
                .unwrap_or_else(|| fail(&format!("--projection {v}: rectilinear, stereographic, equidistant, equisolid or orthographic")))); }
            "--edges" => { let v = value("--edges"); opts.edges = EdgeMode::parse(&v)
                .unwrap_or_else(|| fail(&format!("unknown edges {v} (off, sphere, smooth)"))); }
            "--edges-blur" => opts.edge_settings.blur = (value("--edges-blur").parse::<f32>()
                .unwrap_or_else(|_| fail("--edges-blur takes a number")) / 100.0).clamp(0.0, 0.2),
            "--prores" => opts.prores = true,
            "--dv-rpu" => opts.dv_rpu = Some(PathBuf::from(value("--dv-rpu"))),
            "--dv-live-l1" => opts.dv_live_l1 = true,
            "--from" => opts.from = Some(parse_time(&value("--from"))),
            "--to" => opts.to = Some(parse_time(&value("--to"))),
            "--out" => opts.out = Some(PathBuf::from(value("--out"))),
            "--output" => opts.output = Some(PathBuf::from(value("--output"))),
            "--alone" => { opts.alone = true; opts.alone_project = Some(true); }
            "--whole" => { opts.alone = false; opts.alone_project = Some(false); }
            "--preview" => opts.preview = Some(match value("--preview").as_str() {
                "still" => Preview::Still,
                "flipbook" => Preview::Flipbook,
                other => fail(&format!("unknown preview {other} (still, flipbook)")),
            }),
            "--at" => {
                opts.at_list = value("--at").split(',').filter(|t| !t.trim().is_empty()).map(|t| parse_time(t.trim())).collect();
                opts.at = opts.at_list.first().copied();
            }
            "--cover" => opts.cover = Some(parse_time(&value("--cover"))),
            "--link" => {
                let v = value("--link");
                let json: serde_json::Value = serde_json::from_str(&v).unwrap_or_else(|e| fail(&format!("--link: not JSON, {e}")));
                if !json.is_object() { fail("--link takes a JSON object"); }
                opts.link = Some(json.to_string());
            }
            "--dates" => { let v = value("--dates"); opts.dates = Some(parse_dates(&v).unwrap_or_else(|| fail(&format!("bad date {v} (2026-10-22T15:28:42+02:00)")))); }
            "--project" => opts.project = Some(PathBuf::from(value("--project"))),
            "--force" => opts.force = true,
            "--interpolation" => opts.interpolation = match value("--interpolation").as_str() {
                "bilinear" => Interpolation::Bilinear,
                "bicubic" => Interpolation::Bicubic,
                "lanczos" => Interpolation::Lanczos4,
                other => fail(&format!("unknown interpolation {other}")),
            },
            "--encoder" => opts.encoder = Some(value("--encoder")),
            "--progress" => opts.progress = Progress(Some(PathBuf::from(value("--progress")))),
            "--control" => opts.control = Some(PathBuf::from(value("--control"))),
            "--photo" => opts.photo = Some(parse_time(&value("--photo"))),
            "--radius" => opts.photo_radius = value("--radius").parse::<usize>().ok().filter(|r| *r <= vs_gyroflow::still::MAX_RADIUS)
                .unwrap_or_else(|| fail("--radius takes 0-7")),
            "--grain" => opts.photo_grain = value("--grain").parse::<f32>().ok().filter(|g| (0.0..=1.0).contains(g))
                .unwrap_or_else(|| fail("--grain takes 0-1")),
            "--camera" => opts.camera = Some(vs_gyroflow::Camera::parse(&value("--camera")).unwrap_or_else(|| fail("--camera takes yaw,pitch,roll,zoom"))),
            "--crop" => opts.crop = Some(value("--crop").split(',').map(|v| v.trim().parse::<f64>().ok()).collect::<Option<Vec<_>>>()
                .and_then(|v| <[f64; 4]>::try_from(v).ok())
                .filter(|c| c.iter().all(|v| (0.0..=1.0).contains(v)) && c[2] > 0.0 && c[3] > 0.0 && c[0] + c[2] <= 1.0 + 1e-6 && c[1] + c[3] <= 1.0 + 1e-6)
                .unwrap_or_else(|| fail("--crop takes x,y,w,h in 0-1"))),
            "--sdr-image" => opts.sdr_image = Some(match value("--sdr-image").as_str() {
                v @ ("maker" | "nbcu" | "macos") => v.to_string(),
                v => fail(&format!("unknown SDR image {v} (maker, nbcu, macos)")),
            }),
            "--format" => opts.jpeg = match value("--format").as_str() { "jpeg" | "jpg" => true, "heic" => false, f => fail(&format!("unknown format {f} (heic, jpeg)")) },
            "--quality" => opts.quality = value("--quality").parse::<u8>().ok().filter(|q| (1..=100).contains(q)).unwrap_or_else(|| fail("--quality takes 1-100")),
            "--live" => opts.live = true,
            "--360" => { opts.sphere360.get_or_insert_with(Mode360::default); }
            "--reframe" => {
                let keys = vs_gyroflow::export360::parse_keys(&value("--reframe")).unwrap_or_else(|e| fail(&format!("--reframe: {e}")));
                opts.sphere360.get_or_insert_with(Mode360::default).keys = Some(keys);
            }
            "--heading" => { let v = value("--heading"); opts.sphere360.get_or_insert_with(Mode360::default).heading = Some(vs_gyroflow::export360::parse_heading(&v)
                .unwrap_or_else(|| fail(&format!("--heading {v}: travel, camera, north or world")))); }
            "--sphere-width" => opts.sphere360.get_or_insert_with(Mode360::default).sphere_width = value("--sphere-width").parse::<usize>().ok()
                .filter(|w| (1024..=16384).contains(w)).map(|w| w / 4 * 4).unwrap_or_else(|| fail("--sphere-width takes 1024-16384")),
            "--size" => { let v = value("--size"); opts.sphere360.get_or_insert_with(Mode360::default).size = v.split_once('x')
                .and_then(|(w, h)| Some((w.trim().parse::<usize>().ok()?, h.trim().parse::<usize>().ok()?)))
                .filter(|(w, h)| (64..=8192).contains(w) && (64..=8192).contains(h)).map(|(w, h)| (w / 2 * 2, h / 2 * 2))
                .unwrap_or_else(|| fail(&format!("--size {v}: WxH, such as 1920x1080"))); }
            "--view" => { let v = value("--view"); opts.sphere360.get_or_insert_with(Mode360::default).projection = vs_gyroflow::view360::Projection::from_name(&v)
                .filter(|p| *p != vs_gyroflow::view360::Projection::Equirect)
                .unwrap_or_else(|| fail(&format!("--view {v}: auto, rectilinear, stereographic or planet"))); }
            "--background" => { BACKGROUND.store(true, Relaxed); set_background(&[0], true); }
            "-h" | "--help" => { println!("{USAGE}"); return; }
            _ if a.starts_with("--") => fail(&format!("unknown option {a}\n\n{USAGE}")),
            _ => files.push(a),
        }
    }
    if files.is_empty() { fail(USAGE); }
    if opts.project.is_some() && files.len() > 1 { fail("--project is for one video"); }
    if opts.output.is_some() && files.len() > 1 { fail("--output is for one video"); }
    if opts.preview.is_none() && (opts.at.is_some() || opts.dates.is_some() || opts.cover.is_some()) { fail("--at, --cover and --dates are for --preview"); }
    if opts.cover.is_some() && opts.preview != Some(Preview::Flipbook) { fail("--cover is for a flipbook: a still's cover is its frame"); }
    if opts.preview.is_some() && (opts.prores || opts.dv_rpu.is_some()) { fail("a preview is H.264: not with --prores or --dv-rpu"); }
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
            else if opts.preview == Some(Preview::Flipbook) { flipbook(file, &opts) } else { export(file, &opts, None) };
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

fn parse_time(s: &str) -> f64 {
    s.split(':').try_fold(0.0, |acc, part| part.parse::<f64>().map(|v| acc * 60.0 + v))
        .unwrap_or_else(|_| fail(&format!("bad time {s} (seconds, or m:ss)")))
}

// Days since 1970-01-01 of a date in the proleptic Gregorian calendar, and back (Howard Hinnant's algorithms)
fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + d - 1;
    era * 146097 + yoe * 365 + yoe / 4 - yoe / 100 + doy - 719468
}

fn civil_from_days(z: i64) -> (i64, i64, i64) {
    let z = z + 719468;
    let era = z.div_euclid(146097);
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (yoe + era * 400 + if m <= 2 { 1 } else { 0 }, m, d)
}

// The zone a DJI clip was recorded in, minutes from UTC: its name is the camera's local time
// (DJI_20261024094541_0093_D), its creation_time UTC (the workspace plugin's moments.lua does the same)
fn zone_of_name(stem: &str, created: f64) -> Option<i32> {
    let d = stem.strip_prefix("DJI_")?.get(..14)?;
    if !d.bytes().all(|b| b.is_ascii_digit()) { return None; }
    let n = |a: usize, b: usize| d[a..b].parse::<i64>().unwrap();
    let local = (days_from_civil(n(0, 4), n(4, 6), n(6, 8)) * 86400 + n(8, 10) * 3600 + n(10, 12) * 60 + n(12, 14)) as f64;
    let offset = ((local - created) / 900.0).round() as i32 * 15;
    (offset.abs() <= 14 * 60).then_some(offset)
}

// --dates: 2026-10-22T15:28:42[.123](Z|+02:00|+0200)
fn parse_dates(s: &str) -> Option<Dates> {
    let (date, time) = s.split_once(['T', ' '])?;
    let mut d = date.splitn(3, '-').map(|x| x.parse::<i64>().ok());
    let (y, mo, da) = (d.next()??, d.next()??, d.next()??);
    let (clock, offset_min) = if let Some(c) = time.strip_suffix(['Z', 'z']) { (c, 0) } else {
        let at = time.rfind(['+', '-'])?;
        let (c, zone) = time.split_at(at);
        let sign = if zone.starts_with('-') { -1 } else { 1 };
        let digits: String = zone[1..].chars().filter(|c| *c != ':').collect();
        if digits.len() != 4 { return None; }
        let (h, m) = (digits[..2].parse::<i32>().ok()?, digits[2..].parse::<i32>().ok()?);
        (c, sign * (h * 60 + m))
    };
    let mut t = clock.splitn(3, ':');
    let (h, mi, sec) = (t.next()?.parse::<i64>().ok()?, t.next()?.parse::<i64>().ok()?, t.next()?.parse::<f64>().ok()?);
    if !(1..=12).contains(&mo) || !(1..=31).contains(&da) || !(0..24).contains(&h) || !(0..60).contains(&mi) || !(0.0..61.0).contains(&sec) { return None; }
    let local = (days_from_civil(y, mo, da) * 86400 + h * 3600 + mi * 60) as f64 + sec;
    Some(Dates { unix: local - offset_min as f64 * 60.0, offset_min })
}

impl Dates {
    // The time in its zone, or in UTC: (y, m, d, h, min, s, ms)
    fn fields(&self, offset_min: i32) -> (i64, i64, i64, i64, i64, i64, i64) {
        let ms = ((self.unix + offset_min as f64 * 60.0) * 1000.0).round() as i64;
        let (secs, ms) = (ms.div_euclid(1000), ms.rem_euclid(1000));
        let (y, m, d) = civil_from_days(secs.div_euclid(86400));
        let s = secs.rem_euclid(86400);
        (y, m, d, s / 3600, s / 60 % 60, s % 60, ms)
    }
    // The MP4's creation_time, for ffmpeg: UTC
    fn utc(&self) -> String {
        let (y, m, d, h, mi, s, ms) = self.fields(0);
        format!("{y:04}-{m:02}-{d:02}T{h:02}:{mi:02}:{s:02}.{ms:03}000Z")
    }
    // QuickTime's creation date, as Apple's cameras write it: local time and its offset
    fn quicktime(&self) -> String {
        let (y, m, d, h, mi, s, _) = self.fields(self.offset_min);
        let o = self.offset_min.abs();
        format!("{y:04}-{m:02}-{d:02}T{h:02}:{mi:02}:{s:02}{}{:02}{:02}", if self.offset_min < 0 { '-' } else { '+' }, o / 60, o % 60)
    }
    fn system_time(&self) -> std::time::SystemTime {
        let d = std::time::Duration::from_secs_f64(self.unix.abs());
        if self.unix >= 0.0 { std::time::UNIX_EPOCH + d } else { std::time::UNIX_EPOCH - d }
    }
}

// The Finder's own attributes of a file: its tags (and their colors), its label, its comment. A file replaced
// (--force: a moment updated, an export made again) keeps them, as it would if edited in place
#[cfg(target_os = "macos")]
const FINDER_ATTRIBUTES: &[&str] = &["com.apple.metadata:_kMDItemUserTags", "com.apple.FinderInfo", "com.apple.metadata:kMDItemFinderComment"];

#[cfg(target_os = "macos")]
fn copy_finder_attributes(from: &Path, to: &Path) {
    use std::os::unix::ffi::OsStrExt;
    let (Ok(from), Ok(to)) = (std::ffi::CString::new(from.as_os_str().as_bytes()), std::ffi::CString::new(to.as_os_str().as_bytes())) else { return };
    for name in FINDER_ATTRIBUTES {
        let name = std::ffi::CString::new(*name).unwrap();
        let mut buf = vec![0u8; 64 * 1024];
        let n = unsafe { libc::getxattr(from.as_ptr(), name.as_ptr(), buf.as_mut_ptr() as *mut libc::c_void, buf.len(), 0, 0) };
        if n <= 0 { continue; }
        unsafe { libc::setxattr(to.as_ptr(), name.as_ptr(), buf.as_ptr() as *const libc::c_void, n as usize, 0, 0); }
    }
}

#[cfg(not(target_os = "macos"))]
fn copy_finder_attributes(_from: &Path, _to: &Path) {}

fn file_name(path: &str) -> String {
    Path::new(path).file_name().map(|x| x.to_string_lossy().to_string()).unwrap_or_else(|| path.to_string())
}

enum Outcome { Done(PathBuf), Skipped(String) }

// What the gyroflow plugin stabilizes the video with
// `lens`: what's done with the lens and motion; a clip with no motion data has its lens alone corrected, whatever was asked
struct Source { project: String, preset: Option<String>, label: &'static str, lens: vs_gyroflow::LensMode }

// mpv's own default: %APPDATA%\mpv on Windows, $XDG_CONFIG_HOME/mpv (~/.config/mpv) elsewhere
fn mpv_dir() -> PathBuf {
    let env = |k: &str| std::env::var_os(k).filter(|v| !v.is_empty()).map(PathBuf::from);
    if let Some(dir) = env("MPV_CONFIG_DIR") { return dir; }
    if cfg!(windows) { return env("APPDATA").unwrap_or_default().join("mpv"); }
    if !cfg!(target_os = "macos") { if let Some(dir) = env("XDG_CONFIG_HOME") { return dir.join("mpv"); } }
    env("HOME").unwrap_or_default().join(".config/mpv")
}

// A file of one of the mpv plugins: in $<PLUGIN>_DATA when it's there, else in the scripts folder this binary is
// installed in (~~/scripts/gyroflow/), or in mpv's when it runs from elsewhere (a build)
fn plugin_file(plugin: &str, name: &str) -> PathBuf {
    if let Some(data) = std::env::var_os(format!("{}_DATA", plugin.to_uppercase())).filter(|d| !d.is_empty()) {
        let p = PathBuf::from(data).join(name);
        if p.is_file() { return p; }
    }
    let here = std::env::current_exe().ok().and_then(|e| vs_gyroflow::canonicalize(e).ok())
        .and_then(|e| Some(e.parent()?.parent()?.to_path_buf()))
        .filter(|scripts| scripts.join("gyroflow").join(format!("gyroflow-export{}", std::env::consts::EXE_SUFFIX)).is_file());
    here.unwrap_or_else(|| mpv_dir().join("scripts")).join(plugin).join(name)
}

// `project`: the one given (--project), else the video's own, as the workspace plugin finds it
fn find_source(path: &Path, project: Option<&Path>, lens: vs_gyroflow::LensMode) -> Result<Source, String> {
    let s = path.to_string_lossy().to_string();
    if let Some(p) = project {
        return Ok(Source { project: p.to_string_lossy().to_string(), preset: None, label: "project", lens });
    }
    if let Some(sidecar) = vs_gyroflow::workspace::gyroflow_project(path) {
        return Ok(Source { project: sidecar.to_string_lossy().to_string(), preset: None, label: "project", lens });
    }
    // Nothing to stabilize with: the lens alone when it has a lens profile and the lens is to be corrected
    let lens_alone = |why: String| -> Result<Source, String> {
        let has_lens = probe::camera_and_lens(&s).is_some_and(|(_, l)| !l.is_empty());
        if lens != vs_gyroflow::LensMode::Recorded && has_lens {
            Ok(Source { project: s.clone(), preset: None, label: "the lens alone", lens: vs_gyroflow::LensMode::Only })
        } else {
            Err(why)
        }
    };
    let info = match probe::probe(&s) {
        Ok(info) => info,
        Err(e) => return lens_alone(e),
    };
    match info["mode"].as_str() {
        Some("default") => {
            let preset = plugin_file("gyroflow", "default.gyroflow");
            Ok(Source { project: s.clone(), preset: preset.is_file().then(|| preset.to_string_lossy().to_string()), label: "default preset", lens })
        }
        _ => lens_alone("no project or motion data".into()),
    }
}

struct VideoInfo {
    width: usize,
    height: usize,
    fps: (u64, u64),
    frames: u64,
    duration: f64,
    bit_rate: Option<u64>,
    full_range: bool,
    // The format's creation_time (UTC)
    created: Option<String>,
    // Color tags, ffmpeg names
    primaries: Option<String>,
    transfer: Option<String>,
    matrix: Option<String>,
}

fn ffprobe(path: &Path) -> Result<VideoInfo, String> {
    let out = ffmpeg_command("ffprobe").args(["-v", "error", "-select_streams", "v:0", "-show_entries",
        "stream=width,height,r_frame_rate,nb_frames,duration,bit_rate,color_range,color_primaries,color_transfer,color_space:format=duration:format_tags=creation_time",
        "-of", "json"]).arg(path).output().map_err(|e| format!("ffprobe: {e}"))?;
    if !out.status.success() { return Err(format!("ffprobe: {}", String::from_utf8_lossy(&out.stderr).trim())); }
    let json: serde_json::Value = serde_json::from_slice(&out.stdout).map_err(|e| e.to_string())?;
    let st = &json["streams"][0];
    let num = |v: &serde_json::Value| v.as_str().and_then(|x| x.parse::<f64>().ok()).or_else(|| v.as_f64());
    let tag = |k: &str| st[k].as_str().filter(|x| !x.is_empty() && *x != "unknown").map(str::to_string);
    let fps = st["r_frame_rate"].as_str().and_then(|r| r.split_once('/'))
        .and_then(|(n, d)| Some((n.parse().ok()?, d.parse().ok()?))).filter(|&(n, d): &(u64, u64)| n > 0 && d > 0)
        .ok_or("no frame rate")?;
    let duration = num(&st["duration"]).or_else(|| num(&json["format"]["duration"])).unwrap_or(0.0);
    Ok(VideoInfo {
        width: st["width"].as_u64().ok_or("no video")? as usize,
        height: st["height"].as_u64().ok_or("no video")? as usize,
        fps,
        frames: num(&st["nb_frames"]).map(|x| x as u64).unwrap_or((duration * fps.0 as f64 / fps.1 as f64) as u64),
        duration,
        bit_rate: num(&st["bit_rate"]).map(|x| x as u64),
        full_range: st["color_range"].as_str() == Some("pc"),
        created: json["format"]["tags"]["creation_time"].as_str().map(str::to_string),
        primaries: tag("color_primaries"),
        transfer: tag("color_transfer"),
        matrix: tag("color_space"),
    })
}

// Homebrew's ffmpeg: a Finder Quick Action or mpv doesn't have /opt/homebrew/bin in its PATH. On Windows, where
// ffmpeg is rarely in PATH, one copied next to this binary (the gyroflow plugin's folder) comes first. The one in
// GYROFLOW_TOOLS (ActionFootagePlayer.app's own) comes before all of them
fn tool(name: &str) -> PathBuf {
    let name = format!("{name}{}", std::env::consts::EXE_SUFFIX);
    if let Some(p) = std::env::var_os("GYROFLOW_TOOLS").map(|d| Path::new(&d).join(&name)).filter(|p| p.is_file()) {
        return p;
    }
    let here = std::env::current_exe().ok().and_then(|e| Some(e.parent()?.join(&name))).filter(|p| p.is_file());
    if cfg!(windows) || cfg!(target_os = "linux") {
        if let Some(p) = here { return p; }
    }
    if cfg!(target_os = "macos") {
        for dir in ["/opt/homebrew/bin", "/usr/local/bin"] {
            let p = Path::new(dir).join(&name);
            if p.is_file() { return p; }
        }
    }
    PathBuf::from(name)
}

// Run without a console window of its own on Windows: mpv runs this one without a console, and each ffmpeg would
// open one
fn ffmpeg_command(name: &str) -> Command {
    #[allow(unused_mut)]
    let mut cmd = Command::new(tool(name));
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    cmd
}

// The HEVC encoder ffmpeg writes the export with: how to set it up, and the pixel format it takes
struct Encoder {
    name: String,
    // Before the inputs (a device to open)
    global: Vec<String>,
    // The encoder's own options
    args: Vec<String>,
    pix_fmt: &'static str,
    // After the format filter (uploading to the device)
    upload: &'static str,
}

const VAAPI_DEVICE: &str = "/dev/dri/renderD128";

fn encoder(name: &str, prores: bool) -> Encoder {
    let v = |a: &[&str]| a.iter().map(|x| x.to_string()).collect::<Vec<_>>();
    let mut e = Encoder { name: name.to_string(), global: vec![], args: vec![], pix_fmt: "p010le", upload: "" };
    match name {
        "prores_videotoolbox" => { e.args = v(&["-profile:v", "hq"]); e.pix_fmt = "p210le"; }
        "prores_ks" => { e.args = v(&["-profile:v", "3", "-vendor", "apl0"]); e.pix_fmt = "yuv422p10le"; }
        "hevc_videotoolbox" | "hevc_qsv" | "hevc_amf" => e.args = v(&["-profile:v", "main10"]),
        "hevc_nvenc" => e.args = v(&["-profile:v", "main10", "-preset", "p5"]),
        "hevc_vaapi" => {
            e.global = v(&["-vaapi_device", VAAPI_DEVICE]);
            e.args = v(&["-profile:v", "main10"]);
            e.upload = ",hwupload";
        }
        "libx265" => {
            e.args = v(&["-profile:v", "main10", "-preset", "fast", "-x265-params", "log-level=error"]);
            e.pix_fmt = "yuv420p10le";
        }
        _ => e.pix_fmt = if prores { "yuv422p10le" } else { "yuv420p10le" },
    }
    e
}

// Whether ffmpeg encodes a 10-bit frame with it: an encoder can be built in without the hardware it needs
fn encoder_works(e: &Encoder) -> bool {
    if e.name == "hevc_vaapi" && !Path::new(VAAPI_DEVICE).exists() { return false; }
    ffmpeg_command("ffmpeg").args(["-nostdin", "-v", "error"]).args(&e.global)
        .args(["-f", "lavfi", "-i", "color=black:s=640x360:r=30", "-frames:v", "1",
               "-vf", &format!("format={}{}", e.pix_fmt, e.upload), "-c:v", &e.name]).args(&e.args)
        .args(["-f", "null", "-"])
        .stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null())
        .status().is_ok_and(|s| s.success())
}

// Whether ffmpeg decodes the video's first frame with -hwaccel auto. It falls back to the CPU when there's no
// hardware decoder, but stops when one fails to start (a D3D device on Windows, a VAAPI driver)
fn hwaccel_works(path: &Path) -> bool {
    ffmpeg_command("ffmpeg").args(["-nostdin", "-v", "error", "-hwaccel", "auto", "-i"]).arg(path)
        .args(["-map", "0:v:0", "-frames:v", "1", "-f", "null", "-"])
        .stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null())
        .status().is_ok_and(|s| s.success())
}

fn pick_encoder(opts: &Options, prores: bool) -> Result<Encoder, String> {
    static PICKED: std::sync::OnceLock<Option<String>> = std::sync::OnceLock::new();
    if let Some(name) = &opts.encoder { return Ok(encoder(name, prores)); }
    if cfg!(target_os = "macos") {
        return Ok(encoder(if prores { "prores_videotoolbox" } else { "hevc_videotoolbox" }, prores));
    }
    if prores { return Ok(encoder("prores_ks", true)); }
    let candidates: &[&str] = if cfg!(windows) { &["hevc_nvenc", "hevc_qsv", "hevc_amf", "libx265"] }
                              else { &["hevc_nvenc", "hevc_vaapi", "hevc_qsv", "libx265"] };
    let picked = PICKED.get_or_init(|| candidates.iter().find(|&&n| encoder_works(&encoder(n, false))).map(|n| n.to_string()));
    match picked {
        Some(name) => Ok(encoder(name, false)),
        None => Err("ffmpeg has no HEVC encoder that works here (hevc_nvenc, hevc_qsv, hevc_amf, hevc_vaapi, libx265)".into()),
    }
}

// Where an export goes by default: its project's _Exports/ (docs/moments-and-workspaces.md), else stabilized/ next to
// the video. A project is never made here: the player initializes one where its user chooses
fn default_out_dir(video: &Path) -> PathBuf {
    match vs_gyroflow::workspace::project_root(video) {
        Some(root) => root.join("_Exports"),
        None => video.parent().unwrap_or(Path::new(".")).join("stabilized"),
    }
}

// A preview's H.264 encoder: x264 first, which spends next to nothing on a frame repeated (some 40 bytes a frame,
// VideoToolbox some KB), then the hardware ones. One keyframe: the first frame is most of the file (100-200 KB at
// 960x540 for a camera's frame, its noise included), and a keyframe every 10 s, x264's default, made it 1 MB for 30 s
fn pick_h264() -> Result<Encoder, String> {
    static PICKED: std::sync::OnceLock<Option<String>> = std::sync::OnceLock::new();
    let candidates = ["libx264", "h264_videotoolbox", "h264_nvenc", "h264_qsv", "h264_amf", "h264_mf"];
    let picked = PICKED.get_or_init(|| candidates.iter().copied()
        .filter(|n| *n != "h264_videotoolbox" || cfg!(target_os = "macos"))
        .find(|n| encoder_works(&h264(n))).map(str::to_string));
    picked.as_deref().map(h264).ok_or_else(|| format!("ffmpeg has no H.264 encoder that works here ({})", candidates.join(", ")))
}

fn h264(name: &str) -> Encoder {
    let v = |a: &[&str]| a.iter().map(|x| x.to_string()).collect::<Vec<_>>();
    // For the first video stream only: the cover art is the second, copied
    let args = match name {
        "libx264" => v(&["-preset:v:0", "medium", "-crf:v:0", "26", "-g:v:0", "100000", "-profile:v:0", "high"]),
        _ => v(&["-b:v:0", "150k", "-g:v:0", "100000"]),
    };
    Encoder { name: name.to_string(), global: vec![], args, pix_fmt: "yuv420p", upload: "" }
}

// The size of a preview: the longer side 960 at most, even
fn preview_size(w: usize, h: usize) -> (usize, usize) {
    let scale = (960.0 / w.max(h) as f64).min(1.0);
    let even = |x: f64| ((x / 2.0).round() as usize * 2).max(2);
    (even(w as f64 * scale), even(h as f64 * scale))
}

// A preview's files on the way, removed with it: the frame (8-bit YUV, the preview's size) and the cover (JPEG)
struct PreviewFiles { frame: TempFile, cover: TempFile, size: (usize, usize) }

// The preview's MP4 from its frame: held for `length` seconds, the cover art its JPEG, the dates of --dates, no
// audio and none of the video's metadata; then its keys (the link, Photos' date) added in moov/meta
// `frames`: how many are in files.frame; one is held for `length` seconds, several play at 2 a second
// `dates`: the file's (--dates, or a clip's own); QuickTime's creation date only with its zone known
fn write_preview(files: &PreviewFiles, frames: usize, length: f64, tags: &[String], out: &Path, opts: &Options,
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

// NBCUniversal's HLG -> SDR conversion (Type III), in the dlogm plugin's luts/
const NBCU_HLG: &str = "3-NBCU_HLG2SDR_DL_v1.2.cube";
const NBCU_PQ: &str = "5-NBCU_PQ2SDR_DL_v1.2.cube";

// Tagged HDR (HLG or PQ) in its MP4/MOV, as the dlogm plugin reads it (video_color.lua): never log
fn hdr_in(path: &Path) -> bool {
    ffprobe(path).is_ok_and(|i| matches!(i.transfer.as_deref(), Some("arib-std-b67" | "smpte2084")))
}

// A file removed when it goes out of scope, however the export ends
struct TempFile(PathBuf);

impl Drop for TempFile {
    fn drop(&mut self) { let _ = std::fs::remove_file(&self.0); }
}

fn lut(name: &str) -> Result<PathBuf, String> {
    let p = plugin_file("dlogm", &format!("luts/{name}"));
    if p.is_file() { Ok(p) } else { Err(format!("{} is missing (install the dlogm plugin: install.sh)", p.display())) }
}

// The YUV matrix the video is decoded with, as mpv does: the one it's tagged with, else BT.709 (DJI tags D-Log M
// BT.709; a GoPro's or Insta360's 10-bit may be tagged BT.2020). By the names of ffmpeg's scale and gpu_lut
fn yuv_matrix(tag: Option<&str>) -> &'static str {
    match tag {
        Some("bt2020nc" | "bt2020c") => "bt2020",
        Some("smpte170m" | "bt470bg") => "bt601",
        _ => "bt709",
    }
}

// ffmpeg's filter syntax: quote the path for the option value, then for the filter graph
fn filter_path(p: &Path) -> String {
    let s = p.to_string_lossy().replace('\\', "\\\\").replace('\'', "\\'").replace(':', "\\:");
    format!("'{s}'")
}

// The low-resolution copy a camera writes beside the clip, quicker to decode and far less to read from a NAS:
// DJI's <name>.LRF (the same log curve, checked on an Osmo Action 6's D-Log M), GoPro's GL<rest>.LRV for
// GX<rest>.MP4 or GH<rest>.MP4
fn lowres(path: &Path) -> Option<PathBuf> {
    let dir = path.parent()?;
    let stem = path.file_stem()?.to_string_lossy().to_string();
    let mut names = vec![format!("{stem}.LRF"), format!("{stem}.lrf")];
    if stem.len() > 2 && (stem.starts_with("GX") || stem.starts_with("GH")) {
        names.push(format!("GL{}.LRV", &stem[2..]));
        names.push(format!("GL{}.lrv", &stem[2..]));
    }
    names.into_iter().map(|n| dir.join(n)).find(|p| p.is_file())
}

// A flipbook's times without --at: about 12 through the clip, 10 s apart at least, after its first second
fn flipbook_times(duration: f64) -> Vec<f64> {
    if duration <= 1.5 { return vec![duration / 2.0]; }
    let span = duration - 1.0;
    let n = ((span / 10.0).floor() as usize).clamp(1, 12);
    (0..n).map(|i| 1.0 + span * (i as f64 + 0.5) / n as f64).collect()
}

// --preview flipbook: the clip at a glance (docs/moments-and-workspaces.md, "Clips"). Color only: only keyframes are
// decoded, one per time, from the low-resolution copy when there is one, through the SDR look of a still
fn flipbook(file: &str, opts: &Options) -> Result<Outcome, String> {
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

    let low = lowres(&path);
    let source = low.clone().unwrap_or_else(|| path.clone());
    let src = if low.is_some() { ffprobe(&source)? } else { ffprobe(&path)? };
    let last = (info.duration - 0.05).max(0.0);
    let times: Vec<f64> = if opts.at_list.is_empty() { flipbook_times(info.duration) } else { opts.at_list.clone() }
        .into_iter().map(|t| t.clamp(0.0, last)).collect();
    let cover_at = opts.cover.unwrap_or(info.duration / 2.0);

    // The look, as a still's: D-Log M by the maker's LUT, another curve through NBCU's, HLG through NBCU's
    let curve = opts.curve.unwrap_or_else(|| probe::log_curve(&path.to_string_lossy()));
    let mut _temp_lut = None;
    let cube = match curve {
        Some("dlogm") if !opts.nbcu && lut("dji-dlogm-rec709.cube").is_ok() => Some(lut("dji-dlogm-rec709.cube")?),
        Some(c) => {
            let composed = out_dir.join(format!(".{name}.nbcu.cube"));
            gpu_lut::compose_nbcu(&lut(&format!("{c}_hlg.cube"))?, &lut(NBCU_HLG)?, &composed)?;
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

/// A photo's motion (a Live Photo's or Motion Photo's video): a part of the video, written to `out` as it'd export,
/// at most `max_side` wide or high, `max_fps` frames a second (every second, third… frame: `frame_step`) and
/// `bits_per_pixel` a pixel of each frame, as an MP4 (`mp4`), Dolby Vision as the look has it or not (`dv`)
struct Part {
    from: f64,
    to: f64,
    out: PathBuf,
    max_side: usize,
    bits_per_pixel: f64,
    max_fps: f64,
    mp4: bool,
    dv: bool,
    // In an SDR look ("maker", "nbcu"), the one its photo's image has; None: as the export plays (a Live Photo's HLG)
    sdr: Option<&'static str>,
}

// How much a player turns the video to show it upright, clockwise (its display matrix, mp4meta.rs); 0 when unknown
fn display_rotation(path: &Path) -> u16 {
    vs_gyroflow::mp4meta::display_rotation(path).unwrap_or(0)
}

// The frames a decoder hands over (the sensor's, of `size`: -noautorotate) and the filters that make them so: for the
// engine as they are (it turns them itself, Engine::load), else turned upright as the file says, --rotation on top,
// and cut to --aspect
fn decoded_frame(stabilized: bool, path: &Path, size: (usize, usize), opts: &Options) -> ((usize, usize), String) {
    if stabilized { return (size, String::new()); }
    let frame = opts.frame.for_video(display_rotation(path), None);
    (frame.size(size), frame.ffmpeg_filter(size))
}

fn export(file: &str, opts: &Options, part: Option<&Part>) -> Result<Outcome, String> {
    // A photo's part: its range, not --from/--to
    let (opt_from, opt_to) = part.map_or((opts.from, opts.to), |p| (Some(p.from), Some(p.to)));
    let dv_rpu = if part.is_some_and(|p| !p.dv) { None } else { opts.dv_rpu.as_ref() };
    let prores = opts.prores && part.is_none();
    let path = vs_gyroflow::canonicalize(file).map_err(|e| e.to_string())?;
    let curve = opts.curve.unwrap_or_else(|| probe::log_curve(&path.to_string_lossy()));
    // Nothing to stabilize with, or --no-stabilize: the color conversion alone, if there's one
    let source = if opts.no_stabilize { Err("stabilization off".to_string()) } else { find_source(&path, opts.project.as_deref(), opts.lens) };
    let source = match (source, curve) {
        (Ok(s), _) => Some(s),
        (Err(_), Some(_)) => None,
        // A preview shows the clip as it plays, whatever there is to change
        (Err(_), None) if opts.preview.is_some() => None,
        // Turned or cut (--rotation, --aspect): that much to change
        (Err(_), None) if !opts.frame.is_none() => None,
        // A photo's motion (Live or Motion Photo): the video around it, whatever there is to change
        (Err(_), None) if part.is_some() => None,
        // HDR that isn't log, in SDR (--nbcu) or with Dolby Vision (--dv-rpu): the color to change
        (Err(_), None) if hdr_in(&path) && (opts.nbcu || dv_rpu.is_some()) => None,
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
        format!("{stem}_{}{range}{}", if source.is_some() { "stab" } else { "color" }, if dv_rpu.is_some() { "_dv" } else { "" })
    };
    let ext = if preview.is_some() { "mp4" } else { "mov" };
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
        None => out_dir.join(format!(".{stem}.partial.{ext}")),
    };

    // Color: the LUT works in RGB (mpv's shader too: it reads MAIN, the decoded RGB), from the source's matrix and
    // range, and the result is converted back to YUV in the target's matrix. On the GPU (gpu_lut.rs) when the
    // output size allows, else ffmpeg's lut3d on the CPU, at a third of the speed. Only D-Log M has a maker's
    // Rec.709 LUT: the others are HLG whatever --sdr says, as they play. --nbcu: the curve's LUT to HLG and NBCU's to
    // SDR as one, made for this export next to its output and removed with it
    let in_range = if info.full_range { "pc" } else { "tv" };
    let in_matrix = yuv_matrix(info.matrix.as_deref());
    let mut lut_job = None;
    let mut _temp_lut = None; // kept until the export ends
    // A preview is in SDR: D-Log M by the maker's LUT when it's there, everything else through NBCU's conversion. A
    // Motion Photo's video too, in its image's look (Part::sdr)
    let part_sdr = part.and_then(|p| p.sdr);
    let in_sdr = preview.is_some() || part_sdr.is_some();
    let maker_sdr = curve == Some("dlogm") && !opts.nbcu && part_sdr != Some("nbcu") && (opts.sdr || in_sdr)
        && (!in_sdr || lut("dji-dlogm-rec709.cube").is_ok());
    let nbcu = opts.nbcu || (in_sdr && !maker_sdr);
    let (graph, tags, color_label) = if let Some(curve) = curve {
        let name = CURVES.iter().find(|(id, _)| *id == curve).map_or(curve, |(_, name)| *name);
        let (cube, matrix, primaries, transfer, label) = if nbcu {
            let composed = out_dir.join(format!(".{stem}.nbcu.cube"));
            gpu_lut::compose_nbcu(&lut(&format!("{curve}_hlg.cube"))?, &lut(NBCU_HLG)?, &composed)?;
            _temp_lut = Some(TempFile(composed.clone()));
            (composed, "bt709", "bt709", "bt709", format!("{name} -> Rec.709 (NBCU)"))
        } else if maker_sdr {
            (lut("dji-dlogm-rec709.cube")?, "bt709", "bt709", "bt709", format!("{name} -> Rec.709 (DJI's LUT)"))
        } else {
            let no_sdr = if opts.sdr { " (no Rec.709 LUT for it)" } else { "" };
            (lut(&format!("{curve}_hlg.cube"))?, "bt2020", "bt2020", "arib-std-b67", format!("{name} -> HLG{no_sdr}"))
        };
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
    let key = format!("export|{}", path.display());
    let engine = match &source {
        // All of the motion data before it starts (look-ahead 0): an export has the time
        Some(source) => Some(Arc::new(Engine::load(key, &source.project, source.preset.as_deref(), opts.fov, opts.horizon_lock, opts.underwater, opts.level, 0.0, (w, h), 3, (1, 1), None,
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

    let mut graph = graph;
    let gpu_lut = match lut_job {
        None => None,
        Some((cube, matrix, cpu_graph)) => {
            let out_matrix = gpu_lut::matrix_coefficients(matrix).unwrap();
            // Each frame's L1: a Dolby Vision export's HLG, measured in the same pass
            let measure = opts.dv_live_l1 && dv_rpus.is_some() && tags[1] == "arib-std-b67";
            match GpuLut::new(&cube, ow, oh, gpu_lut::matrix_coefficients(in_matrix).unwrap(), info.full_range, out_matrix, measure) {
                Ok(l) => Some(l),
                Err(e) => {
                    eprintln!("  LUT on the CPU (slower): {e}");
                    graph = cpu_graph;
                    None
                }
            }
        }
    };

    let duration_label = { let t = (to - from).max(0.0) as u64; format!("{}:{:02}", t / 60, t % 60) };
    let mut source_label = source.as_ref().map_or("color only", |s| s.label).to_string();
    match (opts.fov, engine.as_ref().and_then(|e| e.wide_fov)) {
        (_, Some(fov)) => source_label += &format!(", wider view (FOV {fov:.2})"),
        (Fov::Set(fov), None) if source.is_some() => source_label += &format!(", FOV {fov}"),
        _ => {}
    }
    if edges != EdgeMode::Off { source_label += &format!(", edges {}", format!("{edges:?}").to_lowercase()); }
    eprintln!("{}: {color_label} · {source_label} · {}x{} -> {fw}x{fh} · {duration_label}", file_name(file), w, h);

    if stopped() { return Err("cancelled".into()); }
    // A preview's frame goes to ffmpeg as an export's would, then to the preview's encoder (write_preview)
    let enc = if preview.is_some() { encoder("", false) } else { pick_encoder(opts, prores)? };
    if !cfg!(target_os = "macos") && preview.is_none() { eprintln!("  encoder: {}", enc.name); }
    if dv_rpus.is_some() && enc.name != "hevc_videotoolbox" { return Err(format!("--dv-rpu: needs hevc_videotoolbox, not {}", enc.name)); }

    // Decoder: raw 10-bit planar frames on stdout. Elsewhere than on macOS, whatever hardware decoder ffmpeg finds,
    // else the CPU
    let hwaccel = if cfg!(target_os = "macos") { Some("videotoolbox") } else if hwaccel_works(&path) { Some("auto") } else { None };
    let mut dec_cmd = ffmpeg_command("ffmpeg");
    dec_cmd.args(["-nostdin", "-v", "error"]);
    if let Some(h) = hwaccel { dec_cmd.args(["-hwaccel", h]); }
    if preview.is_some() || opt_from.is_some() { dec_cmd.args(["-ss", &format!("{seek:.6}")]); }
    if preview.is_none() && opt_to.is_some() { dec_cmd.args(["-to", &format!("{to:.6}")]); }
    dec_cmd.args(["-noautorotate", "-i"]).arg(&path).args(["-map", "0:v:0"]);
    if preview.is_some() { dec_cmd.args(["-frames:v", "1"]); }
    dec_cmd.args(["-vf", &format!("scale={w}:{h}{}{turn}", if turn.is_empty() { "" } else { "," }), "-f", "rawvideo", "-pix_fmt", "yuv420p10le", "-"])
        .stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped());
    let mut decoder = dec_cmd.spawn().map_err(|e| format!("ffmpeg: {e}"))?;

    // Encoder: the stabilized frames, audio and metadata from the source
    // A photo's motion: the first audio track alone (a Motion Photo has one at most; DJI's cameras record two)
    let audio = if part.is_some() { "1:a:0?" } else { "1:a?" };
    let mut bit_rate = info.bit_rate.unwrap_or(80_000_000) as f64 * (fw * fh) as f64 / (w * h) as f64;
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
    let cut = crop.map(|(x, y, cw, ch)| {
        let scale = if (fw, fh) != (cw, ch) { format!(",scale={fw}:{fh}:flags=lanczos") } else { String::new() };
        format!("crop={cw}:{ch}:{x}:{y}{scale},")
    }).unwrap_or_default();
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
        if prores {
            enc_cmd.args(["-tag:v", "apch"]);
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
    } else if dv_rpus.is_none() && opts.link.is_some() && part.is_none() {
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
            if live_l1 { injector = injector.live_l1(l1_rx); }
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
            if (read - 1) % step != 0 { continue; }
            if frame_tx.send((n, buf)).is_err() { return Ok(n); }
            n += 1;
        }
    });
    let frame_rx = Arc::new(Mutex::new(frame_rx));
    let gpu_threads: Vec<_> = (0..workers).map(|_| {
        let (frame_rx, done_tx, engine) = (frame_rx.clone(), done_tx.clone(), engine.clone());
        let full_range = info.full_range;
        let mut lut = gpu_lut.as_ref().map(|l| l.worker());
        std::thread::spawn(move || {
            loop {
                let Ok((n, src)) = frame_rx.lock().recv() else { break };
                let frame = match &engine {
                    None => Ok(src),
                    Some(engine) => {
                        let mut dst = vec![0u8; out_frame];
                        // Frame times from the start of the file, as Gyroflow and mpv count them
                        let timestamp_us = (((first_frame + n * step) as f64 / fps) * 1_000_000.0).round() as i64 + engine.frame_offset_us;
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
                        engine.process(timestamp_us, planes, !full_range, false).map(|_| dst)
                    }
                };
                let result = frame.and_then(|mut f| match lut.as_mut() { Some(l) => l.apply(&mut f).map(|l1| (f, l1)), None => Ok((f, None)) });
                if done_tx.send((n, result)).is_err() { break; }
            }
        })
    }).collect();
    drop(done_tx);

    let mut enc_in = encoder.stdin.take().unwrap();
    let mut pending = BTreeMap::new();
    let mut next = 0u64;
    let started = Instant::now();
    let mut last_report = Instant::now();
    let (mut reported, mut rate) = (0u64, 0.0);
    let mut background = None;
    let mut error = None;
    let mut last_control = Instant::now();
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
            // The frame's L1 before the frame: the rewriter waits for it when the encoder hands the frame back
            if let Some(tx) = &l1_tx { let _ = tx.send(l1.map(|v| v.map(|x| x.round() as i32))); }
            if let Err(e) = enc_in.write_all(&buf) { error = Some(format!("encoder stopped: {e}")); break; }
            next += 1;
        }
        if error.is_some() { break; }
        if last_report.elapsed().as_millis() >= 500 {
            // The recent rate, not the average: it changes with the priority
            let now = (next - reported) as f64 / last_report.elapsed().as_secs_f64();
            rate = if reported == 0 { now } else { rate * 0.7 + now * 0.3 };
            last_report = Instant::now();
            reported = next;
            let done = next as f64;
            let pct = (done / total_frames as f64 * 100.0).min(100.0);
            let eta = ((total_frames as f64 - done).max(0.0) / rate.max(1e-3)).min(359_999.0) as u64;
            if part.is_none() { eprint!("\r  {pct:3.0}%  {rate:4.0} fps  ETA {}:{:02}   ", eta / 60, eta % 60); }
            opts.progress.write(serde_json::json!({
                "pid": std::process::id(), "done": next, "total": total_frames, "fps": rate, "eta": eta,
                "background": want, "partial": tmp_path,
            }));
        }
    }
    drop(enc_in);
    drop(l1_tx.take()); // the rewriter's last frames, if any are left, keep the file's L1
    if error.is_some() || stopped() {
        let _ = decoder.kill();
    }
    // Cancelled: no point in the encoder finishing the file (the decoder may have stopped first, on Ctrl+C)
    if stopped() {
        let _ = encoder.kill();
        if let Some(m) = muxer.as_mut() { let _ = m.kill(); }
    }
    for t in gpu_threads { let _ = t.join(); }
    let read = reader.join().map_err(|_| "reader panicked".to_string())?;
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
    // A preview: its MP4 from the frame. An export: its link
    let result = result.and_then(|read| match (&preview_files, &opts.link) {
        (Some(files), _) => write_preview(files, 1, (to - from).max(1.0 / fps), &tags, &tmp_path, opts, opts.dates.map(|d| (d, true))).map(|_| read),
        (None, Some(link)) => vs_gyroflow::mp4meta::write_keys(&tmp_path, &[(vs_gyroflow::mp4meta::LINK_KEY, link)]).map(|_| read),
        (None, None) => Ok(read),
    });
    let frames = match result {
        Ok(f) => f,
        Err(e) => { let _ = std::fs::remove_file(&tmp_path); return Err(e); }
    };

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
    Ok(Outcome::Done(out_path))
}

// ---------------------------------------------------------------------------------------------------
// Photos (--photo): the frame at a time as a still, stabilized as the player shows it (and turned as crop mode has
// the camera), cleaned up from its neighbours as the player's clean still frame is (still.rs), in the SDR look with
// the HDR one as a gain map (photo.rs): HEIC through afp-photo on macOS (macos/photo.swift), else an Ultra HDR JPEG

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

fn exif_time(local: i64) -> String {
    let (y, m, d) = civil_from_days(local.div_euclid(86400));
    let s = local.rem_euclid(86400);
    format!("{y:04}:{m:02}:{d:02} {:02}:{:02}:{:02}", s / 3600, s / 60 % 60, s % 60)
}

// Photos: the project's _Stills (beside its _Exports, docs/moments-and-workspaces.md), else stabilized/ next to the clip
fn out_dir_for(path: &Path, opts: &Options) -> PathBuf {
    opts.out.clone().unwrap_or_else(|| match vs_gyroflow::workspace::project_root(path) {
        Some(root) => root.join("_Stills"),
        None => path.parent().unwrap_or(Path::new(".")).join("stabilized"),
    })
}

/// Whether any of `frames` leaves part of the output uncovered, by its mask (Engine::leaves_edges_at), in the view
/// and through the camera the engine's edges are configured with
fn frames_leave_edges(engine: &Engine, mut frames: impl Iterator<Item = u64>, fps: f64, limited_range: bool) -> Result<bool, String> {
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
fn frame_step(fps: f64, max_fps: f64) -> u64 {
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

fn photo(file: &str, opts: &Options, t: f64) -> Result<Outcome, String> {
    use vs_gyroflow::photo::{ self as ph, Cube };
    use vs_gyroflow::still;
    // AFP_TIMES: how long each step took, on stderr
    let clock = Instant::now();
    let lap = |what: &str| if std::env::var_os("AFP_TIMES").is_some() { eprintln!("  [{:6.2} s] {what}", clock.elapsed().as_secs_f64()) };
    let path = vs_gyroflow::canonicalize(file).map_err(|e| e.to_string())?;
    let curve = opts.curve.unwrap_or_else(|| probe::log_curve(&path.to_string_lossy()));
    let source = if opts.no_stabilize { None } else { find_source(&path, opts.project.as_deref(), opts.lens).ok() };
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
        let engine = match &source {
            Some(s) => Some(Engine::load(format!("photo|{}", path.display()), &s.project, s.preset.as_deref(), opts.fov, opts.horizon_lock, opts.underwater, opts.level, 0.0,
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
                        if let Err(e) = engine.process(timestamp_us, planes, !info.full_range, false) { result = Err(e); break; }
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
                Ok(out) => { planes = out.planes; cleaned = frames.len(); }
                Err(e) => eprintln!("  not cleaned up: {e}"),
            }
        }
        drop(window);
        lap("cleaned up");

        // The part to keep, on even pixels
        let [cx, cy, cw, ch] = opts.crop.unwrap_or([0.0, 0.0, 1.0, 1.0]);
        let even = |v: f64| (v.round() as usize) & !1;
        let (x0, y0) = (even(cx * rw as f64).min(rw - 2), even(cy * rh as f64).min(rh - 2));
        let rect = (x0, y0, even(cw * rw as f64).clamp(2, rw - x0), even(ch * rh as f64).clamp(2, rh - y0));
        let rgb = ph::yuv420p10_to_rgb([&planes[0], &planes[1], &planes[2]], rw, rh, rect, yuv_matrix(info.matrix.as_deref()), info.full_range);
        drop(planes);

        // The HDR look (HLG: the curve's LUT, or the video's own), and the SDR one the image is (--sdr-image)
        let (hdr, recorded) = if let Some(curve) = curve {
            (Some(Cube::load(&lut(&format!("{curve}_hlg.cube"))?)?.apply(&rgb)), curve.to_string())
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
            let part = Part { from, to, out: video.clone(), max_side: LIVE_MAX_SIDE, bits_per_pixel: LIVE_BITS_PER_PIXEL, max_fps: LIVE_MAX_FPS, mp4: jpeg, dv: !jpeg, sdr: motion_sdr };
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
        if made.is_err() { STOP.store(libc::SIGTERM, Relaxed); }
        let (sdr, hdr, label, cleaned) = made?;
        let live = match motion.map(|m| m.join()) {
            None => None,
            Some(Ok(Ok(v))) => Some(v),
            Some(Ok(Err(e))) if stopped() => return Err(e),
            Some(Ok(Err(e))) => { eprintln!("  no motion, the photo alone: {e}"); None }
            Some(Err(_)) => { eprintln!("  no motion, the photo alone: its export failed"); None }
        };

        let taken = clip_start_local(&path).map(|s| exif_time(s + (n as f64 / fps) as i64));
        let maker = probe::camera(&path.to_string_lossy());
        let (make, model) = (maker.as_ref().map(|m| m.0.clone()), maker.and_then(|m| m.1));
        if jpeg {
            let exif = ph::exif(taken.as_deref(), make.as_deref(), model.as_deref());
            let video = match &live { Some((f, _)) => Some(std::fs::read(&f.0).map_err(|e| format!("{}: {e}", f.0.display()))?), None => None };
            let motion = video.as_deref().zip(live.as_ref()).map(|(v, (_, at))| ph::Motion { video: v, presentation_us: (at * 1e6).round() as i64 });
            let bytes = ph::ultra_hdr_jpeg(&sdr, hdr.as_ref(), opts.quality, Some(exif), motion.as_ref())?;
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
            if let Some(t) = &taken { cmd.args(["--taken", t]); }
            if let Some(m) = &make { cmd.args(["--make", m]); }
            if let Some(m) = &model { cmd.args(["--model", m]); }
            // A Live Photo: the video copied next to it, the two with one content identifier
            if let Some((video, at)) = &live {
                cmd.arg("--live-video").arg(&video.0).arg("--live-out").arg(out_path.with_extension("mov"))
                    .args(["--content-id", &content_id()?, "--still-time", &format!("{at:.6}")]);
            }
            let out = cmd.output().map_err(|e| format!("afp-photo: {e}"))?;
            if !out.status.success() { return Err(String::from_utf8_lossy(&out.stderr).trim().to_string()); }
        }
        lap("written");
        eprintln!("  {label} · {}x{}{} · {}", sdr.width, sdr.height,
                  if cleaned > 0 { format!(" · cleaned up from {cleaned} frames") } else { String::new() },
                  if hdr.is_some() { if jpeg { "Ultra HDR JPEG" } else { "HEIC with a gain map" } } else if jpeg { "JPEG" } else { "HEIC" });
        if live.is_some() { eprintln!("  {}", if jpeg { "Motion Photo: the video in the JPEG" } else { "Live Photo: the .mov next to it" }); }
        Ok(Outcome::Done(out_path))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dates() {
        let d = parse_dates("2026-10-22T15:28:42+02:00").unwrap();
        assert_eq!(d.offset_min, 120);
        assert_eq!(d.utc(), "2026-10-22T13:28:42.000000Z");
        assert_eq!(d.quicktime(), "2026-10-22T15:28:42+0200");
        assert_eq!(d.system_time().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs(), 1792675722);
        // Across midnight and the year, west of UTC, compact zone, fractions
        let d = parse_dates("2027-01-01T01:30:00.250-0500").unwrap();
        assert_eq!(d.utc(), "2027-01-01T06:30:00.250000Z");
        assert_eq!(d.quicktime(), "2027-01-01T01:30:00-0500");
        let d = parse_dates("2026-12-31T23:30:00+01:00").unwrap();
        assert_eq!(d.utc(), "2026-12-31T22:30:00.000000Z");
        assert_eq!(parse_dates("2024-02-29T12:00:00Z").unwrap().quicktime(), "2024-02-29T12:00:00+0000");
        for bad in ["2026-10-22", "2026-10-22T15:28:42", "2026-13-01T00:00:00Z", "2026-10-22T25:00:00Z", "yesterday", "2026-10-22T15:28:42+2"] {
            assert!(parse_dates(bad).is_none(), "{bad}");
        }
    }

    #[test]
    fn zones() {
        let created = parse_dates("2026-10-24T06:45:41Z").unwrap().unix;
        assert_eq!(zone_of_name("DJI_20261024094541_0093_D", created), Some(180));
        assert_eq!(zone_of_name("GX011057", created), None);
        assert_eq!(zone_of_name("DJI_2026", created), None);
    }

    #[test]
    fn flipbook_frames() {
        assert_eq!(flipbook_times(43.8).len(), 4);
        assert!(flipbook_times(43.8).windows(2).all(|w| w[1] - w[0] >= 10.0));
        assert!(flipbook_times(43.8)[0] > 1.0);
        assert_eq!(flipbook_times(600.0).len(), 12);
        assert_eq!(flipbook_times(5.0).len(), 1);
        assert_eq!(flipbook_times(1.0), vec![0.5]);
    }

    #[test]
    fn preview_sizes() {
        assert_eq!(preview_size(3840, 2160), (960, 540));
        assert_eq!(preview_size(2160, 3840), (540, 960));
        assert_eq!(preview_size(4000, 3000), (960, 720));
        assert_eq!(preview_size(640, 360), (640, 360));
        assert_eq!(preview_size(2688, 1512), (960, 540));
    }
}

/// A 360 camera's file as a stabilized 360 video or a flat reframe (vs_gyroflow::export360): ffmpeg decodes the lens
/// tracks joined side by side (as the player's lavfi-complex), the frames go through the plugin's 360 mode and the
/// view, ffmpeg encodes them (HEVC 10-bit, the source's audio copied); a 360 video gets its spherical metadata after
fn export_360(file: &str, opts: &Options, mode: &Mode360) -> Result<Outcome, String> {
    use vs_gyroflow::export360::{ Output, Render360, heading_name };
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
    let out_path = opts.output.clone().unwrap_or_else(|| out_dir.join(format!("{stem}_{kind}{range}.mp4")));
    if out_path.exists() && !opts.force { return Ok(Outcome::Skipped(format!("{} exists (--force replaces it)", out_path.display()))); }
    std::fs::create_dir_all(&out_dir).map_err(|e| format!("{}: {e}", out_dir.display()))?;

    // 8-bit sources as 8-bit frames, the others as 16-bit samples
    let depth_probe = ffmpeg_command("ffprobe").args(["-v", "error", "-select_streams", "v:0", "-show_entries", "stream=pix_fmt", "-of", "csv=p=0"]).arg(&path).output().map_err(|e| e.to_string())?;
    let src_fmt = String::from_utf8_lossy(&depth_probe.stdout).trim().to_string();
    let depth = if src_fmt.contains("10") || src_fmt.contains("12") || src_fmt.contains("16") { 10 } else { 8 };
    let raw_fmt = match (depth, info.full_range) { (8, true) => "yuvj420p", (8, false) => "yuv420p", _ => "yuv420p10le" };
    let out = match &mode.keys {
        None => Output::Sphere { width: if mode.sphere_width > 0 { mode.sphere_width } else { vs_gyroflow::sphere360::DEFAULT_WIDTH } },
        Some(keys) => Output::Flat { size: mode.size, projection: mode.projection, keys: keys.clone() },
    };
    let t0 = Instant::now();
    let render = Render360::new(&video, depth, !info.full_range, out, mode.heading, mode.sphere_width)?;
    if let Some(note) = &render.heading_note { eprintln!("{}: {note}", file_name(file)); }
    eprintln!("{}: {} {}, {} lenses; {}, along {} ({:.1} s to load)", file_name(file), rig.brand, rig.model, rig.lenses.len(),
              if kind == "360" { "a 360 video" } else { "a reframe" }, heading_name(render.heading), t0.elapsed().as_secs_f64());
    let fps = info.fps.0 as f64 / info.fps.1 as f64;
    let from = opts.from.unwrap_or(0.0).max(0.0);
    let to = opts.to.unwrap_or(info.duration).min(info.duration.max(from));
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
    // Encoding: HEVC 10-bit at the source's bits per pixel, BT.709 limited range, the source's audio
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
               "-vf", &format!("scale=out_range=tv:out_color_matrix=bt709,format={},setparams=color_primaries=bt709:color_trc=bt709:colorspace=bt709:range=tv{}", enc.pix_fmt, enc.upload),
               "-c:v", &enc.name]).args(&enc.args)
        .args(["-tag:v", "hvc1", "-b:v", &format!("{}", bit_rate as u64),
               "-color_primaries", "bt709", "-color_trc", "bt709", "-colorspace", "bt709", "-color_range", "tv", "-f", "mp4"]).arg(&tmp_path)
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
