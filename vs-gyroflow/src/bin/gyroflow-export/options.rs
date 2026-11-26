// The command line: --help's text, the options and their parsing, and --settings (the player's settings JSON).
// One of gyroflow-export's parts: main.rs has the uses and main(); each part's items are the program's.

use super::*;

pub(crate) const USAGE: &str = "usage: gyroflow-export [options] <video>...
       gyroflow-export --fingerprint <file>...  (each file's fingerprint, as a link keeps its original's: SHA-256 of
                                                 its first and last MiB)

Renders each video stabilized (and, for log, color-converted) into the _Exports/ folder of its project (the trip
folder with Project.afproject in it above the video, as the player initializes it), else into a stabilized/
folder next to it. A log video with nothing to stabilize it with is color-converted only.

  --settings <json|file|->  the settings JSON the player builds (docs/reference/contracts.md, \"The settings\"; its
                   keys: registry.json's settings_json): how the clip is shown, as the player shows it, in place of the
                   options below that it stands for; options after it change what it set
  --curve <curve>  the log curve: dlogm, dlog, gplog, gplog2, ilog, or none (colors as recorded) instead of the
                   one the camera's metadata says; or uw, the underwater colour (as --uw-color yes, whatever the clip)
  --uw-color <yes|no>  the player's underwater colour (#273: shaders/underwater_hlg.glsl, the dlogm plugin's curve
                   \"uw\") for a plain SDR clip, one that's neither log nor HDR: corrected and made HLG, then in the look
                   asked for as a log curve's HLG is. Its numbers are measured as the player measures them (src/uwcolor.rs):
                   the frame itself for a photo or a still; for a video a few times a second, half a second ahead, eased
                   from one to the next as playback has them (the GPU path can't change them by the frame: the pipes)
  --uw-strength <0..1>  its strength, between the camera's colour (0) and the correction (1, the default)
  --uw-highlights <yes|no>  the light end stretched into HDR's room (2.5x SDR white for the camera's white); no by default
  --uw-params <dr,db,k,ev>  its numbers as given (the player's at the frame shown), for every frame, not measured
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
  --denoise <off|light|full>  the temporal denoise as the player's GPU path plays it (Ctrl+p; its shader,
                   warp.glsl's dn_ passes): each frame averaged with 3 (light) or 5 (full) frames on each side, where
                   the motion between them puts them, some of the grain kept (--grain, as for a photo); off, the
                   default, as recorded. Stabilized only
  --denoise-strong the denoise's Stronger (the clip's transform: for a very noisy clip)
  --chromatic <yes|no>  the water's color fringes taken out where the lens is under water (Ctrl+a; the player's
                   chromatic.glsl), yes by default; no: as recorded. Stabilized only
  --prores         ProRes 422 HQ instead of HEVC 10-bit
  --dv-rpu <file>  HLG as Dolby Vision 8.4 (PQ as 8.1: an RPU made with afp-rpu --profile 8.1), with this file's RPUs (dovi_tool's or afp-rpu's .rpu: frame N of the
                   video gets RPU N, the last the frames after it) in place of VideoToolbox's, and the dvvC box Apple
                   needs to use them; the player's HDR + Dolby Vision look plays the same file (macOS, HLG only)
  --youtube <look> for YouTube: HDR10 (PQ, BT.2020, with the mastering display and the MaxCLL/MaxFALL measured) in
                   an MKV, a 3D LUT inside (application/x-cube, as YouTube's hdr_metadata tool attaches it) that YouTube
                   makes its SDR version with, in this SDR look: maker (DJI's LUT; D-Log M), nbcu (NBCU's conversion),
                   or auto (the maker's for D-Log M when its LUT is there, else NBCU's). A log or HDR (HLG, PQ) clip;
                   log and HLG become PQ for a 1000-nit display (BT.2408). <name>_stab_youtube.mkv
  --youtube-lut <full|data>  the LUT's levels: full (the default: 0 black, 1 white, as grading apps write them) or
                   data (0..1 the 10-bit codes, the signal at 64..940, as NBCU's LUTs)
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
  --gpu <auto|on|off>  the GPU path: one ffmpeg decoding, running the player's own shaders (libplacebo filter) and
                   encoding, the frames on the GPU throughout (docs/design/export-gpu-path.md). auto (default): where this
                   ffmpeg and the clip allow it, else the pipes, with a line saying why; on: only it
  --gpu-interpolation <i>  its warp's interpolation: bilinear, bspline, bicubic, bicubic9, lanczos, ewa (default:
                   the player's, lanczos up to 31 fps, bicubic9 above)
  --with-translation  the translation compensation kept for the clip (the player's Ctrl+j), on the GPU path
  --resolution <r> the output's short side at most: 2160, 1080, 720 (or any number of pixels), never enlarged;
                   original (the default) as it's always been: the clip's or its Gyroflow project's size. The GPU path
                   renders the warp at that size directly, the pipes scale the stabilized frames; a photo is scaled
                   at the end (docs/design/export-settings.md)
  --encode <fast|quality>  fast (the default): the encoder's real-time mode on the GPU path (VideoToolbox), its
                   default on the pipes, the source's bit rate for the output's size; quality: the encoder's quality
                   mode on both paths (VideoToolbox not real-time, NVENC p7, x265 slow) and 1.5x that bit rate
  --encoder <name> ffmpeg's HEVC encoder, e.g. hevc_nvenc, hevc_qsv, hevc_amf, hevc_vaapi, libx265
                   (default: VideoToolbox on macOS, else the first of these that works)
  --progress <file>  write the progress there as JSON, twice a second
  --background     start at background priority (SIGUSR1 / SIGUSR2 switch it later)
  --control <file> read stop / background / normal from this file while running (for Windows, which has no
                   SIGUSR1/SIGUSR2)

Previews, for the player's moments (docs/design/moments-and-workspaces.md): small H.264 MP4s in SDR that any system
shows and plays, the frame also their cover art (the icon)
  --preview still  one frame, stabilized as with the options above, in SDR (the maker's LUT for D-Log M, else
                   NBCU's conversion; HLG through NBCU's), held for --from to --to (the whole video without them);
                   by default <name>.afplink.mp4 in --out or next to the video (the player's
                   moments are _Moments/<moment>.afplink.mp4)
  --preview flipbook  the whole clip at a glance, for the player's _Clips/: a frame at each --at time (the keyframe
                   at or before it, so only keyframes are decoded), 2 a second, in SDR as for a still and not
                   stabilized, from the low-resolution copy beside the clip when there is one (DJI's .LRF, GoPro's
                   .LRV); without --at, about 12 frames through the clip, 10 s apart at least, after its first second
  --picture <image>  a still's frame as given (the player's frame as shown, an SDR picture): nothing is decoded or
                   stabilized, the video only gives the link's `hash` and the file's dates; needs --output, --from, --to
  --at <time>      the frame's time (default: the middle of --from to --to); a flipbook's: times, comma-separated
  --cover <time>   a flipbook's cover art (its icon): the frame nearest this time (default: the middle)
  --link <json>    the link to the original the player keeps in it (an MP4 key, ch.actionfootageplayer.link), its
                   `hash` added as for an export
  --dates <time>   its date, ISO 8601 with the time zone (2026-10-22T15:28:42+02:00): the MP4's creation time,
                   QuickTime's creation date (Photos' day) and the file's dates; without it, the video's dates
A 360 camera's own file (an Insta360 .insv: docs/design/360-cameras.md), as the player shows it (the sphere stitched from its
lenses, the horizon level by gravity, pointed along a heading mode); --from, --to, --out, --output, --force,
--encoder, --progress as above:
  --360            a stabilized 360 video: the whole sphere turned into the world and the heading, equirect, with
                   spherical metadata (YouTube, editors, VLC): <name>_360.mp4
  --reframe <keys> a flat video looking along the head's keyframes, \"t:yaw,pitch,roll,fov;...\" (seconds of the
                   clip, degrees: yaw right, pitch up, roll counterclockwise, the FOV across a 16:9 picture as in the
                   player; eased between keys, held before the first and after the last): <name>_reframe.mp4
  --reframe recording  along the view recorded in the player as you looked around (its 360° tab), from the clip's
                   <name>.afp.json: relative to what it follows, each sample's projection (a change is a cut), held
                   outside the recorded parts; by default from the first recorded part to the end of the last.
                   Tidied as the player's 360° tab says (smoothed, level; the hold on what's in the middle measured
                   first when it's asked for and not measured along this path yet)
  --measure-hold   with --reframe recording: only measure the hold on what's in the middle (the view rendered small
                   along the smoothed recording, its middle matched frame to frame), kept in the app's cache for the
                   player's preview and the export; --progress tells how far it is
  --heading <mode> what straight ahead follows: travel (along the way you move: GPS), camera, north, world. By
                   default travel where the clip has it, else camera, as the player picks
  --sphere-width <px>  the sphere's width (default 5760; the 360 video's size, its height half of it)
  --size <WxH>     a reframe's size (default 1920x1080)
  --view <p>       a reframe's projection: auto (default), rectilinear, stereographic, planet
Translation: --translation  the hand's sideways jerks computed for each video instead of exporting it, and kept for
the player (vs-gyroflow's translation.rs; Osmo Action 6, which records its accelerometer): the video stabilized
at 960 px as with the options above (--project, --level, --underwater, --horizon-lock), each frame's motion
measured, fitted to the camera's own jerks. Prints a JSON line: the cache file (translation), the jitter above 1 Hz at 960 px
wide measured and left (before, after) and the share of the fit used (share); --progress writes the percent done
Photos: --photo <time> the frame at that time as a still instead, with a gain map where there's an HDR look: an SDR
rendition as the image, the HDR one (HLG) above it. Its place in its Exif GPS (a Live or Motion Photo's video in its
QuickTime location): the camera's GPS at that time, else the clip's location in the Library (docs/design/geolocation.md)
  --sdr-image <maker|nbcu|macos>  the SDR image: DJI's LUT (D-Log M; the default for it), NBCU's conversion of the
                   HLG (the default for the others), or the HLG as macOS shows it on an SDR screen (afp-photo)
  --radius <n>     clean it up from n frames on each side (0-7, default 5; 0: as it is), as the player's Ctrl+p
  --grain <x>      how much of its grain the clean-up keeps (0-1, default 0.25)
  --camera <yaw,pitch,roll,zoom>  the camera turned and zoomed as crop mode has it (the gyroflow plugin's crop.lua;
                   a video export takes it too)
  --crop <x,y,w,h> the part of the frame to keep, in shares of its width and height (0-1; a video export's too),
                   of the frame as --rotation and --aspect have it
  --image <file.png>  the photo's frame as the player rendered it on the GPU (its warp, crop mode's camera, the look
                   in HLG): mpv's 16-bit PNG screenshot in the video's own colors. In place of decoding, stabilizing,
                   cleaning up and the HDR look here; --crop cuts it, the SDR image is made from it (--sdr-image nbcu or
                   macos)
  --format <heic|jpeg>  HEIC (macOS: afp-photo) or Ultra HDR JPEG (the default elsewhere)
  --live           with the motion around it, 1.5 s on each side, as the export plays it (--sdr, --nbcu, --dv-rpu
                   say how; at most 1920 wide or high): a HEIC becomes a Live Photo (a .mov next to it, paired by
                   afp-photo), a JPEG a Motion Photo (<name>.MP.jpg, the MP4 inside it, as Android's cameras write it;
                   its video in the image's SDR look, which the apps showing it all play alike)
  --quality <0-100>  the image's quality (default 92)";

pub(crate) struct Options {
    // None: from the metadata; Some(None): none
    pub(crate) curve: Option<Option<&'static str>>,
    // --uw-color: the underwater colour where the clip has no curve (not log, not HDR); its strength and highlights;
    // --uw-params, its numbers as the player has them
    pub(crate) uw: bool,
    pub(crate) uw_settings: vs_gyroflow::uwcolor::Settings,
    pub(crate) uw_params: Option<vs_gyroflow::uwcolor::Estimate>,
    pub(crate) sdr: bool,
    pub(crate) nbcu: bool,
    pub(crate) no_stabilize: bool,
    // --lens-only, --lens-recorded: the lens corrected with nothing smoothed, or left as recorded while stabilized
    pub(crate) lens: vs_gyroflow::LensMode,
    pub(crate) horizon_lock: Option<f64>,
    pub(crate) underwater: Underwater,
    pub(crate) fov: Fov,
    pub(crate) level: Level,
    // --rotation, --aspect, --projection: the output frame turned and cut, its projection (vs_gyroflow::Frame)
    pub(crate) frame: vs_gyroflow::Frame,
    pub(crate) edges: EdgeMode,
    pub(crate) edge_settings: EdgeSettings,
    // --denoise: the temporal denoise's frames on each side (0: none), --denoise-strong its Stronger; --chromatic
    pub(crate) denoise: usize,
    pub(crate) denoise_strong: bool,
    pub(crate) chromatic: bool,
    pub(crate) prores: bool,
    pub(crate) from: Option<f64>,
    pub(crate) to: Option<f64>,
    pub(crate) out: Option<PathBuf>,
    pub(crate) project: Option<PathBuf>,
    pub(crate) force: bool,
    pub(crate) interpolation: Interpolation,
    // The GPU path (docs/design/export-gpu-path.md): None where it can, Some(true) only it, Some(false) never
    pub(crate) gpu: Option<bool>,
    // The GPU warp's interpolation (warp.glsl's INTERP); None: the player's for the clip's frame rate
    pub(crate) gpu_interp: Option<i32>,
    // The translation compensation kept for the clip, as the player draws it with Ctrl+j (GPU path); `translation_kept`:
    // the settings' (Ctrl+j on), where it's computed for the clip and the GPU path can, else without a word against it
    pub(crate) with_translation: bool,
    pub(crate) translation_kept: bool,
    // The preset a clip without a project is stabilized with (the settings': the player's), else the plugin's own
    pub(crate) preset: Option<PathBuf>,
    // A 360 camera's sphere: its width, its stitching distance (0: the camera's own), what straight ahead follows
    // (the settings', for --360 and --reframe)
    pub(crate) sphere: (usize, f64, Option<vs_gyroflow::stamp::Heading>),
    pub(crate) encoder: Option<String>,
    // --resolution: the output's short side at most (None: as it is); --encode quality
    pub(crate) resolution: Option<usize>,
    pub(crate) encode_quality: bool,
    pub(crate) progress: Progress,
    pub(crate) control: Option<PathBuf>,
    pub(crate) dv_rpu: Option<PathBuf>,
    pub(crate) dv_live_l1: bool,
    pub(crate) output: Option<PathBuf>,
    // --alone: the part stabilized on its own
    pub(crate) alone: bool,
    // --alone / --whole: a project's "only within the trim range", whatever it says (as a moment is viewed)
    pub(crate) alone_project: Option<bool>,
    pub(crate) preview: Option<Preview>,
    pub(crate) at: Option<f64>,
    // --at for a flipbook: its frames' times
    pub(crate) at_list: Vec<f64>,
    pub(crate) cover: Option<f64>,
    pub(crate) link: Option<String>,
    pub(crate) dates: Option<Dates>,
    // --translation: the translation compensation computed and kept instead of an export
    pub(crate) translation: bool,
    pub(crate) photo: Option<f64>,
    pub(crate) photo_radius: usize,
    pub(crate) photo_grain: f32,
    pub(crate) camera: Option<vs_gyroflow::Camera>,
    pub(crate) crop: Option<[f64; 4]>,
    /// --image: the player's own render of the frame (HLG, a 16-bit PNG)
    pub(crate) image: Option<PathBuf>,
    pub(crate) picture: Option<PathBuf>,
    pub(crate) jpeg: bool,
    pub(crate) quality: u8,
    pub(crate) sdr_image: Option<String>,
    pub(crate) live: bool,
    // --youtube: the SDR look of the LUT inside (None: auto), and its levels
    pub(crate) youtube: Option<Option<vs_gyroflow::youtube::Look>>,
    pub(crate) youtube_levels: vs_gyroflow::youtube::Levels,
    // A 360 camera's file: --360 or --reframe (export_360)
    pub(crate) sphere360: Option<Mode360>,
}

// Where a reframe looks: keyframes, or the view recorded in the player (--reframe recording)
#[derive(Clone)]
pub(crate) enum Head360 { Keys(Vec<vs_gyroflow::export360::HeadKey>), Recording }

pub(crate) struct Mode360 {
    // None: the whole sphere (--360); else the head's keyframes or the recording (--reframe)
    pub(crate) keys: Option<Head360>,
    pub(crate) heading: Option<vs_gyroflow::stamp::Heading>,
    pub(crate) sphere_width: usize,
    // Where its lenses' seam is right, in metres (0: the camera's own, else 2 m), as the player's sphere
    pub(crate) distance: f64,
    pub(crate) size: (usize, usize),
    pub(crate) projection: vs_gyroflow::view360::Projection,
    // --measure-hold: only the picture hold's measuring of the recording
    pub(crate) measure_hold: bool,
}

impl Default for Mode360 {
    fn default() -> Self { Mode360 { keys: None, heading: None, sphere_width: 0, distance: 0.0, size: (1920, 1080), projection: vs_gyroflow::view360::Projection::Auto, measure_hold: false } }
}

// What --preview makes (flipbooks and video previews come later)
#[derive(Clone, Copy, PartialEq, Debug)]
pub(crate) enum Preview { Still, Flipbook }

// --dates: a time and the time zone it was given in
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Dates { pub(crate) unix: f64, pub(crate) offset_min: i32 }

// The options as the command line sets them, the defaults first; and the videos
pub(crate) fn default_options() -> Options {
    Options { curve: None, uw: false, uw_settings: Default::default(), uw_params: None, sdr: false, nbcu: false, no_stabilize: false, lens: vs_gyroflow::LensMode::Stabilized, horizon_lock: None, underwater: Underwater::Auto, fov: Fov::Project, level: Level::Medium, frame: vs_gyroflow::Frame::default(),
                             edges: EdgeMode::Off, edge_settings: EdgeSettings::default(), denoise: 0, denoise_strong: false, chromatic: true, prores: false, from: None, to: None, out: None, project: None, force: false,
                             interpolation: Interpolation::Lanczos4, gpu: None, gpu_interp: None, with_translation: false, translation_kept: false, preset: None, sphere: (0, 0.0, None), encoder: None, resolution: None, encode_quality: false, progress: Progress(None), control: None,
                             dv_rpu: None, dv_live_l1: false, output: None, alone: false, alone_project: None, preview: None, at: None, at_list: vec![], cover: None, link: None, dates: None,
                             translation: false, photo: None, photo_radius: 5, photo_grain: 0.25, camera: None, crop: None, image: None, picture: None,
                             jpeg: !cfg!(target_os = "macos"), quality: 92, sdr_image: None, live: false,
                             youtube: None, youtube_levels: vs_gyroflow::youtube::Levels::Full, sphere360: None }
}

pub(crate) fn parse_args(mut args: impl Iterator<Item = String>) -> (Options, Vec<String>) {
    let mut opts = default_options();
    let mut files = Vec::new();
    while let Some(a) = args.next() {
        let mut value = |name: &str| args.next().unwrap_or_else(|| fail(&format!("{name} needs a value")));
        match a.as_str() {
            "--settings" => {
                let v = value("--settings");
                let j = read_settings(&v).unwrap_or_else(|e| fail(&format!("--settings: {e}")));
                apply_settings(&mut opts, &j).unwrap_or_else(|e| fail(&format!("--settings: {e}")));
            }
            "--curve" => opts.curve = Some(match value("--curve").as_str() {
                "none" => None,
                c => Some(vs_gyroflow::lists::curve(c).map(|x| x.id.as_str())
                    .unwrap_or_else(|| fail(&format!("unknown curve {c} ({}, none)",
                                                     vs_gyroflow::lists::get().curves.iter().map(|x| x.id.as_str()).collect::<Vec<_>>().join(", "))))),
            }),
            "--uw-color" => opts.uw = yes_no("--uw-color", &value("--uw-color")),
            "--uw-strength" => opts.uw_settings.strength = value("--uw-strength").parse::<f64>().ok().filter(|v| (0.0..=1.0).contains(v))
                .unwrap_or_else(|| fail("--uw-strength takes 0-1")),
            "--uw-highlights" => opts.uw_settings.highlights = yes_no("--uw-highlights", &value("--uw-highlights")),
            "--uw-params" => {
                let v: Vec<f64> = value("--uw-params").split(',').filter_map(|x| x.trim().parse::<f64>().ok()).filter(|x| x.is_finite()).collect();
                if v.len() != 4 { fail("--uw-params takes dr,db,k,ev: four numbers"); }
                opts.uw_params = Some(vs_gyroflow::uwcolor::Estimate { dr: v[0], db: v[1], k: v[2], ev: v[3], f: 1.0 });
            }
            "--sdr" => opts.sdr = true,
            "--nbcu" => opts.nbcu = true,
            "--no-stabilize" => opts.no_stabilize = true,
            "--lens-only" => opts.lens = vs_gyroflow::LensMode::Only,
            "--lens-recorded" => opts.lens = vs_gyroflow::LensMode::Recorded,
            "--horizon-lock" => opts.horizon_lock = Some(value("--horizon-lock").parse::<f64>().ok().filter(|v| (0.0..=100.0).contains(v))
                .unwrap_or_else(|| fail("--horizon-lock takes 0-100"))),
            "--underwater" => opts.underwater = Underwater::parse(&value("--underwater")).unwrap_or_else(|| fail("--underwater takes yes, no, auto or project")),
            // The ranges the player takes them in (lib.rs)
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
            "--denoise" => opts.denoise = match value("--denoise").as_str() {
                "off" => 0,
                "light" => 3,
                "full" => 5,
                other => fail(&format!("unknown denoise {other} (off, light, full)")),
            },
            "--denoise-strong" => opts.denoise_strong = true,
            "--chromatic" => opts.chromatic = match value("--chromatic").as_str() {
                "yes" => true,
                "no" => false,
                other => fail(&format!("--chromatic {other}: yes or no")),
            },
            "--prores" => opts.prores = true,
            "--dv-rpu" => opts.dv_rpu = Some(PathBuf::from(value("--dv-rpu"))),
            "--dv-live-l1" => opts.dv_live_l1 = true,
            "--youtube" => opts.youtube = Some(match value("--youtube").as_str() {
                "auto" => None,
                v => Some(vs_gyroflow::youtube::Look::parse(v).unwrap_or_else(|| fail(&format!("--youtube {v}: maker, nbcu or auto")))),
            }),
            "--youtube-lut" => { let v = value("--youtube-lut"); opts.youtube_levels = vs_gyroflow::youtube::Levels::parse(&v)
                .unwrap_or_else(|| fail(&format!("--youtube-lut {v}: full or data"))); }
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
            "--resolution" => { let v = value("--resolution"); opts.resolution = parse_resolution(&v).unwrap_or_else(|| fail(&format!("--resolution {v}: original, 2160, 1080, 720 or a number of pixels"))); }
            "--encode" => opts.encode_quality = match value("--encode").as_str() { "fast" => false, "quality" => true, e => fail(&format!("--encode {e}: fast or quality")) },
            "--gpu" => opts.gpu = match value("--gpu").as_str() {
                "auto" => None, "on" => Some(true), "off" => Some(false),
                other => fail(&format!("--gpu {other}: auto, on or off")),
            },
            "--gpu-interpolation" => opts.gpu_interp = Some(match value("--gpu-interpolation").as_str() {
                "bilinear" => 0, "bspline" => 1, "bicubic" => 2, "lanczos" | "lanczos4" => 3, "ewa" => 4, "bicubic9" => 5,
                other => fail(&format!("unknown GPU interpolation {other} (bilinear, bspline, bicubic, bicubic9, lanczos, ewa)")),
            }),
            "--with-translation" => opts.with_translation = true,
            "--progress" => opts.progress = Progress(Some(PathBuf::from(value("--progress")))),
            "--control" => opts.control = Some(PathBuf::from(value("--control"))),
            "--photo" => opts.photo = Some(parse_time(&value("--photo"))),
            "--translation" => opts.translation = true,
            "--radius" => opts.photo_radius = value("--radius").parse::<usize>().ok().filter(|r| *r <= vs_gyroflow::still::MAX_RADIUS)
                .unwrap_or_else(|| fail("--radius takes 0-7")),
            "--grain" => opts.photo_grain = value("--grain").parse::<f32>().ok().filter(|g| (0.0..=1.0).contains(g))
                .unwrap_or_else(|| fail("--grain takes 0-1")),
            "--camera" => opts.camera = Some(vs_gyroflow::Camera::parse(&value("--camera")).unwrap_or_else(|| fail("--camera takes yaw,pitch,roll,zoom"))),
            "--crop" => opts.crop = Some(value("--crop").split(',').map(|v| v.trim().parse::<f64>().ok()).collect::<Option<Vec<_>>>()
                .and_then(|v| <[f64; 4]>::try_from(v).ok())
                .filter(|c| c.iter().all(|v| (0.0..=1.0).contains(v)) && c[2] > 0.0 && c[3] > 0.0 && c[0] + c[2] <= 1.0 + 1e-6 && c[1] + c[3] <= 1.0 + 1e-6)
                .unwrap_or_else(|| fail("--crop takes x,y,w,h in 0-1"))),
            "--image" => opts.image = Some(PathBuf::from(value("--image"))),
            "--picture" => opts.picture = Some(PathBuf::from(value("--picture"))),
            "--sdr-image" => opts.sdr_image = Some(match value("--sdr-image").as_str() {
                v @ ("maker" | "nbcu" | "macos") => v.to_string(),
                v => fail(&format!("unknown SDR image {v} (maker, nbcu, macos)")),
            }),
            "--format" => opts.jpeg = match value("--format").as_str() { "jpeg" | "jpg" => true, "heic" => false, f => fail(&format!("unknown format {f} (heic, jpeg)")) },
            "--quality" => opts.quality = value("--quality").parse::<u8>().ok().filter(|q| (1..=100).contains(q)).unwrap_or_else(|| fail("--quality takes 1-100")),
            "--live" => opts.live = true,
            "--360" => { opts.sphere360.get_or_insert_with(Mode360::default); }
            "--reframe" => {
                let v = value("--reframe");
                let head = if v == "recording" { Head360::Recording } else {
                    Head360::Keys(vs_gyroflow::export360::parse_keys(&v).unwrap_or_else(|e| fail(&format!("--reframe: {e}"))))
                };
                opts.sphere360.get_or_insert_with(Mode360::default).keys = Some(head);
            }
            "--measure-hold" => opts.sphere360.get_or_insert_with(Mode360::default).measure_hold = true,
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
            "-h" | "--help" => { println!("{USAGE}"); std::process::exit(0); }
            _ if a.starts_with("--") => fail(&format!("unknown option {a}\n\n{USAGE}")),
            _ => files.push(a),
        }
    }
    // The settings' sphere, where the command line doesn't say
    if let Some(m) = opts.sphere360.as_mut() {
        if m.sphere_width == 0 { m.sphere_width = opts.sphere.0; }
        m.distance = opts.sphere.1;
        if m.heading.is_none() { m.heading = opts.sphere.2; }
    }
    (opts, files)
}

// --settings: the JSON itself ({...}), - for stdin, else a file holding it
fn read_settings(v: &str) -> Result<serde_json::Value, String> {
    let text = if v.trim_start().starts_with('{') || v.trim() == "[]" { v.to_string() }
        else if v == "-" { let mut t = String::new(); std::io::stdin().read_to_string(&mut t).map_err(|e| e.to_string())?; t }
        else { std::fs::read_to_string(v).map_err(|e| format!("{v}: {e}"))? };
    let j: serde_json::Value = serde_json::from_str(&text).map_err(|e| format!("not JSON, {e}"))?;
    // (mpv's format_json writes an empty table as [])
    if j.as_array().is_some_and(|a| a.is_empty()) { return Ok(serde_json::json!({})); }
    if !j.is_object() { return Err("not a JSON object".into()); }
    Ok(j)
}

/// The settings JSON (docs/reference/contracts.md, "The settings"; its keys: registry.json's settings_json) into the
/// options, as the flags it stands for would set them: the player's settings for the clip as it plays, or for a
/// saved view, sent as they are. Flags after --settings change what it set. A key the registry says gyroflow-export
/// reads and this doesn't is an error (the tests send every one); the others' (the table's, the request's) are left
pub(crate) fn apply_settings(opts: &mut Options, j: &serde_json::Value) -> Result<(), String> {
    use vs_gyroflow::LensMode;
    let obj = j.as_object().ok_or("not a JSON object")?;
    let reg = vs_gyroflow::lists::get();
    let (mut denoise_on, mut amount, mut radius) = (None, None, None);
    for (k, v) in obj {
        if v.is_null() { continue; }
        let text = || v.as_str().ok_or_else(|| format!("{k}: a string"));
        let num = || v.as_f64().or_else(|| v.as_str().and_then(|s| s.trim().parse().ok())).ok_or_else(|| format!("{k}: a number"));
        let flag = || v.as_bool().or_else(|| v.as_i64().map(|i| i != 0)).ok_or_else(|| format!("{k}: true or false"));
        match k.as_str() {
            // The video itself when it plays without one: the export finds the clip's own project then
            "project" => { let p = text()?; if vs_gyroflow::is_project_file(p) { opts.project = Some(PathBuf::from(p)); } }
            "preset" => { let p = PathBuf::from(text()?); if p.is_file() { opts.preset = Some(p); } }
            "fov" => { let f = num()?; if f > 0.0 { opts.fov = Fov::Set(f.clamp(0.1, 4.0)); } }
            "level" => opts.level = Level::parse(text()?).ok_or_else(|| format!("level {v}"))?,
            "wide" => if flag()? && !matches!(opts.fov, Fov::Set(_)) { opts.fov = Fov::Wide },
            "horizon_lock" => opts.horizon_lock = Some(num()?.clamp(0.0, 100.0)),
            "underwater" => opts.underwater = Underwater::parse(text()?).ok_or_else(|| format!("underwater {v}"))?,
            "rotation" => opts.frame.rotation = vs_gyroflow::Frame::parse_rotation(num()? as i64).ok_or_else(|| format!("rotation {v}"))?,
            "aspect" => opts.frame.aspect = vs_gyroflow::Frame::parse_aspect(text()?).ok_or_else(|| format!("aspect {v}"))?,
            "projection" => opts.frame.projection = Some(match v.as_str() {
                Some(name) => vs_gyroflow::Frame::parse_projection(name),
                None => v.as_i64().filter(|n| (0..=4).contains(n)).map(|n| n as i32),
            }.ok_or_else(|| format!("projection {v}"))?),
            "lens" => match text()? {
                "off" => opts.no_stabilize = true,
                l => { opts.lens = LensMode::parse(l).ok_or_else(|| format!("lens {v}"))?; opts.no_stabilize = false; }
            },
            // The GPU warp's, as gyroflow-gpu-table names them (bicubic: its bicubic9)
            "interpolation" => opts.gpu_interp = Some(match text()? {
                "bilinear" => 0, "bspline" => 1, "lanczos" | "lanczos4" => 3, "ewa" => 4, "bicubic" | "bicubic9" => 5,
                i => return Err(format!("interpolation {i}")),
            }),
            "engine_interpolation" => opts.interpolation = match text()? {
                "bilinear" => Interpolation::Bilinear, "bicubic" => Interpolation::Bicubic, "lanczos" => Interpolation::Lanczos4,
                i => return Err(format!("engine_interpolation {i}")),
            },
            "edges" => opts.edges = EdgeMode::parse(text()?).ok_or_else(|| format!("edges {v}"))?,
            "edges_blur" => opts.edge_settings.blur = (num()? as f32 / 100.0).clamp(0.0, 0.2),
            "chromatic" => opts.chromatic = flag()?,
            "denoise" => denoise_on = Some(flag()?),
            "temporal_denoise" => amount = Some(text()?.to_string()),
            "denoise_strong" => opts.denoise_strong = flag()?,
            "denoise_grain" => opts.photo_grain = num()?.clamp(0.0, 1.0) as f32,
            "still_radius" => radius = Some((num()?.max(0.0) as usize).min(vs_gyroflow::still::MAX_RADIUS)),
            "translation" => opts.translation_kept = flag()?,
            "curve" => opts.curve = Some(match text()? {
                "none" => None,
                c => Some(vs_gyroflow::lists::curve(c).map(|x| x.id.as_str()).ok_or_else(|| format!("curve {c}"))?),
            }),
            "uw" => opts.uw = flag()?,
            "uw_strength" => opts.uw_settings.strength = num()?.clamp(0.0, 1.0),
            "uw_highlights" => opts.uw_settings.highlights = flag()?,
            "uw_params" => {
                let n: Vec<f64> = v.as_array().ok_or("uw_params: [dr, db, k, ev]")?.iter().filter_map(|x| x.as_f64()).filter(|x| x.is_finite()).collect();
                if n.len() != 4 { return Err("uw_params: [dr, db, k, ev]".into()); }
                opts.uw_params = Some(vs_gyroflow::uwcolor::Estimate { dr: n[0], db: n[1], k: n[2], ev: n[3], f: 1.0 });
            }
            "sdr" => match text()? { "maker" => { opts.sdr = true; opts.nbcu = false; } "nbcu" => { opts.nbcu = true; opts.sdr = false; } x => return Err(format!("sdr {x}")) },
            "sdr_image" => opts.sdr_image = Some(match text()? { x @ ("maker" | "nbcu" | "macos") => x.to_string(), x => return Err(format!("sdr_image {x}")) }),
            "dv_rpu" => opts.dv_rpu = Some(PathBuf::from(text()?)),
            "dv_live_l1" => opts.dv_live_l1 = flag()?,
            "resolution" => { let r = v.as_str().map(str::to_string).unwrap_or_else(|| v.to_string()); opts.resolution = parse_resolution(&r).ok_or_else(|| format!("resolution {r}"))?; }
            "encode" => opts.encode_quality = match text()? { "fast" => false, "quality" => true, e => return Err(format!("encode {e}")) },
            "sphere_width" => { let w = num()?.max(0.0) as usize; opts.sphere.0 = if w == 0 { 0 } else { w.clamp(1024, 16384) / 4 * 4 }; }
            "stitching_distance" => opts.sphere.1 = num()?.max(0.0),
            "heading" => opts.sphere.2 = Some(vs_gyroflow::export360::parse_heading(text()?).ok_or_else(|| format!("heading {v}"))?),
            _ => if reg.json_key(k).is_some_and(|x| x.export) { return Err(format!("{k}: registry.json says gyroflow-export reads it, apply_settings doesn't")); },
        }
    }
    // The temporal denoise: on for the clip, and how much (none, light: 3 frames each side, full: 5)
    if denoise_on.is_some() || amount.is_some() {
        let on = denoise_on.unwrap_or(true);
        opts.denoise = match amount.as_deref().unwrap_or("light") {
            _ if !on => 0,
            "off" => 0, "light" => 3, "full" => 5,
            a => return Err(format!("temporal_denoise {a}")),
        };
    }
    // A photo cleaned up from its neighbours where the clip has the denoise on (#319: the settings', a recipe's too, not
    // this computer's), as many as asked
    opts.photo_radius = radius.unwrap_or(if opts.denoise == 0 && (denoise_on.is_some() || amount.is_some()) { 0 } else { opts.photo_radius });
    Ok(())
}
