// SPDX-License-Identifier: GPL-3.0-or-later
//
// A clip's lens found from its own picture (docs/design/optical-without-gyroflow.md, step 4), with Gyroflow's own code for it
// (gyroflow-core's lens_from_video, gyroflow/gyroflow#1296): the frames decoded here, tracked and judged there.
//
// - Find (`find`): the known lenses of the clip's camera, nearest first (lens_from_video::candidates: the one the player
//   takes now, then Gyroflow's database for this frame size, then other sizes scaled or cropped, then sibling cameras),
//   tried on three parts of the clip in air and under water. A lens without a readout, or with one for another frame
//   rate, gets it measured on the clip. Tens of seconds.
// - Estimate (`estimate`): a lens and its readout estimated from a stretch of the clip, then judged on the parts it
//   wasn't made from, as a known one is. Under a minute.
//
// What was found is kept for the clip (`Kept`, in the cache) with each lens as a profile of the clip's frame, the
// readout as measured. `keep` then makes one the lens of the clip's whole mode in the player's own database
// (lens_db::assign): nothing changes before that.

use std::io::Read;
use std::path::Path;
use std::process::{ Command, Stdio };
use gyroflow_core::{ filesystem, gyro_source::TrackingMethod, lens_profile::LensProfile, util,
                     stabilization_params::ReadoutDirection,
                     lens_from_video::{ self as lfv, Candidate, Fragment, FragmentBuilder, Verdict },
                     synchronization::optical_motion::{ KltDevice, Tracker } };
use serde::{ Deserialize, Serialize };
use crate::lens_pick::Clip;

/// One lens tried, or the estimate, as kept for the clip
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Tried {
    /// "0", "1"... in the order tried; "estimate"
    pub n: String,
    pub name: String,
    /// "good", "single", "loose", "poor" (lens_from_video::Verdict); an estimate not judged on another part of the
    /// clip: "unchecked"
    pub verdict: String,
    /// It fits (good or single), as Gyroflow's Auto takes it
    pub fits: bool,
    /// The light's refraction it fits in (1 in air, 1.33 under water), and whether the picture told the two apart
    pub refraction: f64,
    pub medium_sure: bool,
    /// The readout, ms, and whether it was measured on the clip (not the profile's own)
    pub readout_ms: Option<f64>,
    pub readout_measured: bool,
    /// How far the lens moved when let go, px, and what a lens that fits stays within
    pub moved: f64,
    pub limit: f64,
    /// The profile it is in the clip's frame, the readout as measured: what `keep` keeps (Gyroflow's JSON)
    pub profile: Option<serde_json::Value>,
}

/// What was found for a clip, kept in the cache until a lens of it is kept
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Kept {
    pub clip: String,
    /// The clip's mode as the player's own database keeps lenses (lens_db::ModeKey::describe)
    pub mode: String,
    pub tried: Vec<Tried>,
    /// The one that fits best (its `n`), as lens_from_video takes it
    pub accepted: Option<String>,
    /// Parts of the clip that had a scene to judge by; none moving through space (a camera turned on a tripod)
    pub fragments_with_scene: usize,
    pub no_translation: bool,
    pub estimate: Option<Tried>,
    pub doubts: Vec<String>,
    /// Found with Quick on (stopped at the first lens that fits), and how many lenses it left untried then
    #[serde(default)]
    pub quick: bool,
    #[serde(default)]
    pub untried: usize,
}

/// Where a clip's findings are kept
fn kept_file(video: &Path) -> Option<std::path::PathBuf> {
    crate::file_cache("lensfind", &video.to_string_lossy(), "1")
}

pub fn read_kept(video: &Path) -> Option<Kept> {
    serde_json::from_str(&std::fs::read_to_string(kept_file(video)?).ok()?).ok()
}

fn write_kept(video: &Path, k: &Kept) {
    if let Some(f) = kept_file(video) {
        if let Some(d) = f.parent() { let _ = std::fs::create_dir_all(d); }
        let _ = std::fs::write(&f, serde_json::to_string(k).unwrap_or_default());
    }
}

/// The clip as the lens is found for it: its path, size, rate, length, and its camera
struct Video {
    path: String,
    size: (usize, usize),
    fps: f64,
    duration_ms: f64,
    clip: Clip,
    /// Gyroflow's brand and model ("GoPro", "HERO4 Black")
    camera: (String, String),
}

fn video(path: &Path) -> Result<Video, String> {
    let p = path.to_string_lossy().into_owned();
    // The camera's own calibration of this very clip: nothing to find (Anton, 2026-11-24: not even offered)
    if crate::probe::lens_in_the_file(&p) { return Err(crate::probe::LENS_IN_THE_FILE.into()); }
    let url = filesystem::path_to_url(&p);
    let mut file = std::fs::File::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let size = file.metadata().map_err(|e| e.to_string())?.len();
    let meta = util::get_video_metadata(&mut file, size, &url).map_err(|e| format!("not a video Gyroflow reads: {e}"))?;
    let options = gyroflow_core::telemetry_parser::InputOptions { probe_only: true, ..Default::default() };
    let clip = match gyroflow_core::telemetry_parser::Input::from_stream_with_options(&mut file, size, &p, |_| (), std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)), options) {
        Ok(input) => Clip::from_input(&p, &input, meta.width, meta.height, meta.fps),
        Err(_) => Clip::from_file(&p, meta.width, meta.height, meta.fps),
    };
    let camera = match &clip.id {
        Some(id) if !id.brand.is_empty() => (id.brand.clone(), id.model.clone()),
        // A camera Gyroflow doesn't read (a HERO3, a HERO4): its brand and model as the player names it
        _ => match clip.camera.split_once(' ') { Some((b, m)) => (b.to_string(), m.to_string()), None => (clip.camera.clone(), String::new()) },
    };
    Ok(Video { path: p, size: (meta.width, meta.height), fps: meta.fps, duration_ms: meta.duration_s * 1000.0, clip, camera })
}

/// A part of the clip tracked as lens_from_video tracks it: every `frame_step`th frame, at TRACKED_WIDTH, gray
fn track(v: &Video, range_ms: (f64, f64), stop: &dyn Fn() -> bool) -> Result<Fragment, String> {
    let step = lfv::frame_step(v.fps);
    let tw = lfv::TRACKED_WIDTH.min(v.size.0 as u32);
    let th = (((v.size.1 as f64 * tw as f64 / v.size.0 as f64) / 2.0).round() * 2.0) as u32;
    let frames = (((range_ms.1 - range_ms.0) / 1000.0 * v.fps) / step as f64).floor().max(1.0) as usize;
    let mut cmd = Command::new(crate::tools::tool("ffmpeg"));
    cmd.args(["-nostdin", "-v", "error", "-noautorotate", "-ss", &format!("{:.3}", range_ms.0 / 1000.0), "-i"]).arg(&v.path)
        .args(["-map", "0:v:0", "-vf", &format!("select=not(mod(n\\,{step})),scale={tw}:{th}:flags=area"), "-fps_mode", "passthrough",
               "-frames:v", &frames.to_string(), "-pix_fmt", "gray", "-f", "rawvideo", "-"])
        .stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::null());
    #[cfg(windows)]
    { use std::os::windows::process::CommandExt; cmd.creation_flags(0x0800_0000); }
    let mut ff = cmd.spawn().map_err(|e| format!("ffmpeg: {e}"))?;
    let mut out = ff.stdout.take().ok_or("ffmpeg: no output")?;
    let mut buf = vec![0u8; (tw * th) as usize];
    let mut tracker = Tracker::new(lfv::TRACKED_POINTS, TrackingMethod::Klt, KltDevice::Gpu(None))?;
    let interval = step as f64 * 1000.0 / v.fps;
    let mut builder = FragmentBuilder::new(v.size, (tw, th), interval);
    let mut slot = 0;
    while out.read_exact(&mut buf).is_ok() {
        if stop() { let _ = ff.kill(); let _ = ff.wait(); return Err("cancelled".into()); }
        builder.push_frame(range_ms.0 + slot as f64 * interval, &tracker.track(tw, th, tw as usize, &buf)?);
        slot += 1;
    }
    let _ = ff.wait();
    Ok(builder.finish())
}

/// The media a lens is tried in: the clip's own (source.underwater, else as the camera recorded), else both
fn media(path: &Path) -> Vec<f64> {
    match crate::water::refraction_for(path) {
        Some(r) => vec![r],
        None => vec![1.0, crate::water::WATER_REFRACTION],
    }
}

/// A candidate as a profile of the clip's frame, the readout as measured on it (none when not measured and it has none)
fn profile_of(c: &Candidate, v: &Video, readout_ms: Option<f64>) -> Option<LensProfile> {
    let mut p = if c.loadable { c.profile.clone()? } else { lfv::brought_profile(c, v.size, v.fps)? };
    if let Some(r) = readout_ms {
        p.frame_readout_time = Some(r.abs());
        p.frame_readout_direction = Some(if r < 0.0 { ReadoutDirection::BottomToTop } else { ReadoutDirection::TopToBottom });
    }
    Some(p)
}

fn verdict_name(v: Verdict) -> &'static str {
    match v { Verdict::Good => "good", Verdict::Single => "single", Verdict::Loose => "loose", Verdict::Poor => "poor" }
}

/// The known lenses of the clip's camera, the one the player takes now first; not a GoPro's Max Lens Mod for a clip
/// recorded without it (GX010020, Linear: Find tried only HERO12 Max Lens Mod fisheyes)
fn candidates_for(v: &Video) -> Vec<Candidate> {
    let loaded = crate::lens_pick::for_clip(&v.path, &v.clip, false).map(|p| p.profile);
    lfv::candidates(crate::lens_pick::database(), (&v.camera.0, &v.camera.1, ""), v.size, v.fps, loaded.as_ref(), 1.0)
        .into_iter().filter(|c| {
            let p = c.profile.as_ref();
            !other_lens_mod(&v.camera.0, &v.camera.1, &v.clip.lens_mode, &[&c.name, p.map_or("", |p| &p.lens_model), p.map_or("", |p| &p.camera_setting)])
        }).collect()
}

/// Whether a profile (its name, lens model, camera setting: `names`) is a GoPro's Max Lens Mod one ("Max..." or "MLM"
/// a word of them, the MAX camera itself aside) offered for a clip whose lens mode isn't the mod's: another lens
fn other_lens_mod(brand: &str, model: &str, lens_mode: &str, names: &[&str]) -> bool {
    let max_word = |t: &str| t.split(|ch: char| !ch.is_ascii_alphanumeric()).map(|w| w.to_ascii_lowercase())
        .any(|w| (w.starts_with("max") && w != "maximum") || w == "mlm");
    if !brand.eq_ignore_ascii_case("gopro") || max_word(model) { return false; }
    names.iter().any(|n| max_word(n)) && !lens_mode.to_ascii_lowercase().contains("max")
}

/// Find: the known lenses tried on three parts of the clip, nearest first. `quick`: stopped at the first that fits (a
/// "good" verdict, with a readout: its own or measured), else every one tried and the nearest of those that fit taken
/// (Gyroflow's Auto). `progress`: 0-1; `partial`: what's found so far, after each lens (the dialogs show it as it comes)
pub fn find(path: &Path, quick: bool, progress: &(dyn Fn(f64) + Sync), stop: &(dyn Fn() -> bool + Sync), partial: &dyn Fn(&Kept)) -> Result<Kept, String> {
    let v = video(path)?;
    let candidates = candidates_for(&v);
    if candidates.is_empty() { return Err(format!("{} {}: no known lens of this camera to try", v.camera.0, v.camera.1)); }
    let parts = lfv::automatic_fragments(v.duration_ms, v.fps);
    let mut fragments = Vec::new();
    for (i, r) in parts.iter().enumerate() {
        fragments.push(track(&v, *r, stop)?);
        progress(0.6 * (i + 1) as f64 / parts.len().max(1) as f64);
    }
    let media = media(path);
    let current = media[0];
    let mut k = read_kept(path).unwrap_or_default();
    k.clip = v.path.clone();
    k.mode = crate::lens_db::ModeKey::of(&v.path, &v.clip).describe();
    k.quick = quick;
    k.tried.clear();
    k.accepted = None;
    k.untried = 0;
    // One lens at a time, each told on its own (what's found so far shown as it comes, and Quick stops early)
    let mut known = lfv::KnownProfiles::default();
    for (i, c) in candidates.iter().enumerate() {
        let n = candidates.len() as f64;
        let one = lfv::find_known(&fragments, std::slice::from_ref(c), &media, current, stop, &|p| progress(0.6 + 0.4 * (i as f64 + p) / n));
        if stop() { return Err("cancelled".into()); }
        let Some(r) = one.tried.into_iter().next() else { continue };
        known.fragments_with_scene = known.fragments_with_scene.max(one.fragments_with_scene);
        known.no_translation |= one.no_translation;
        let m = r.matching();
        let good = m.verdict == Verdict::Good;
        k.tried.push(Tried {
            n: i.to_string(), name: c.label(), verdict: verdict_name(m.verdict).into(), fits: good || m.verdict == Verdict::Single,
            refraction: r.refraction(), medium_sure: r.medium_sure, readout_ms: m.readout_ms, readout_measured: m.readout_measured,
            moved: m.moved, limit: m.limit,
            profile: profile_of(c, &v, m.readout_ms).and_then(|p| p.get_json().ok()).and_then(|j| serde_json::from_str(&j).ok()),
        });
        known.tried.push(r);
        k.fragments_with_scene = known.fragments_with_scene;
        k.no_translation = known.no_translation;
        // Kept as it goes, for the lens dialogs to show what's found so far (gyroflow-optical --lenses)
        write_kept(path, &k);
        partial(&k);
        // Good enough: it fits, with a readout to correct the rolling shutter by
        if quick && good && m.readout_ms.is_some() {
            k.untried = candidates.len() - i - 1;
            break;
        }
    }
    // The nearest of those that fit (Quick: the one it stopped at)
    k.accepted = (0..known.tried.len()).filter(|i| known.tried[*i].matching().verdict == Verdict::Good)
        .min_by(|a, b| known.tried[*a].matching().moved.total_cmp(&known.tried[*b].matching().moved)).map(|i| i.to_string());
    write_kept(path, &k);
    Ok(k)
}

/// Estimate: a lens and its readout from a stretch of the clip (the first MAX_FRAMES tracked frames), started from the
/// nearest known one, judged on the other parts of the clip. In the clip's medium, else the one Find saw, else air
pub fn estimate(path: &Path, progress: &(dyn Fn(f64) + Sync), stop: &(dyn Fn() -> bool + Sync)) -> Result<Kept, String> {
    let v = video(path)?;
    let mut k = read_kept(path).unwrap_or_default();
    let candidates = candidates_for(&v);
    // Started from what Find thought best, else the nearest known that Find didn't find to fit poorly (GX010020: an
    // estimate started from a fisheye of another lens), else a plain wide lens
    let poor = |i: usize| k.tried.iter().any(|t| t.n == i.to_string() && t.verdict == "poor");
    let start = k.accepted.as_ref().and_then(|n| n.parse::<usize>().ok()).and_then(|i| candidates.get(i).cloned())
        .or_else(|| candidates.iter().enumerate().find(|(i, _)| !poor(*i)).map(|(_, c)| c.clone()))
        .or_else(|| lfv::candidates::generic(v.size).into_iter().next())
        .ok_or("no lens to start from")?;
    let refraction = crate::water::refraction_for(path)
        .or_else(|| k.accepted.as_ref().and_then(|n| k.tried.iter().find(|t| &t.n == n)).map(|t| t.refraction))
        .unwrap_or(1.0);
    let parts = lfv::automatic_fragments(v.duration_ms, v.fps);
    let fitted = lfv::fragment_from(0.0, v.duration_ms, v.fps, lfv::MAX_FRAMES);
    let fragment = track(&v, fitted, stop)?;
    progress(0.3);
    let e = lfv::estimate(&fragment.tracks(), &start.lens.with_refraction(refraction), fragment.frame_ms, stop, &|p| progress(0.3 + 0.5 * p))
        .map_err(|e| format!("no estimate: {e:?}"))?;
    if stop() { return Err("cancelled".into()); }
    // Judged as a known lens is, on the parts of the clip it wasn't made from
    let mut apart = Vec::new();
    for i in lfv::apart_from(&parts, fitted) { apart.push(track(&v, parts[i], stop)?); }
    progress(0.9);
    let m = (!apart.is_empty()).then(|| lfv::check_lens(&apart, &e.lens, Some(e.readout_ms), stop));
    let (brand, model) = (&v.camera.0, &v.camera.1);
    let profile = lfv::profile(&e, Some(e.readout_ms), v.size, v.fps, refraction != 1.0, (brand, model), start.profile.as_ref(), "Action Footage Player, from the footage");
    // Not judged where the clip has no part apart from the stretch it was estimated from (a short clip)
    let verdict = m.map(|m| verdict_name(m.verdict)).unwrap_or("unchecked");
    k.clip = v.path.clone();
    k.mode = crate::lens_db::ModeKey::of(&v.path, &v.clip).describe();
    k.doubts = lfv::doubts(&e).into_iter().map(String::from).collect();
    k.estimate = Some(Tried {
        n: "estimate".into(), name: format!("Estimated from this clip (started from {})", start.label()), verdict: verdict.into(),
        fits: verdict == "good" || verdict == "single", refraction, medium_sure: false, readout_ms: Some(e.readout_ms), readout_measured: true,
        moved: m.map(|m| m.moved).unwrap_or_default(), limit: m.map(|m| m.limit).unwrap_or_default(),
        profile: profile.get_json().ok().and_then(|j| serde_json::from_str(&j).ok()),
    });
    write_kept(path, &k);
    progress(1.0);
    Ok(k)
}

/// One of what was found for the clip (`n`: a lens tried, or "estimate") kept as the lens of its whole mode in the
/// player's own database. The file written and the mode
pub fn keep(path: &Path, n: &str) -> Result<(std::path::PathBuf, String, String), String> {
    let k = read_kept(path).ok_or("nothing found for this clip yet: Find lens or Estimate lens first")?;
    let t = k.estimate.iter().chain(k.tried.iter()).find(|t| t.n == n).ok_or_else(|| format!("{n}: not one of what was found for this clip"))?;
    // Only what fits (an estimate too short to judge on another part as well): one that fits loosely isn't reliable
    if !(t.fits || t.verdict == "unchecked") { return Err(format!("{}: it doesn't fit reliably ({}), so it isn't kept", t.name, t.verdict)); }
    let json = t.profile.as_ref().ok_or("that lens can't be kept as a profile of this clip's frame")?;
    let profile = LensProfile::from_json(&json.to_string()).map_err(|e| e.to_string())?;
    let v = video(path)?;
    let key = crate::lens_db::ModeKey::of(&v.path, &v.clip);
    let how = if n == "estimate" { "estimated from footage" } else if t.readout_measured { "found in the footage, readout measured" } else { "found in the footage" };
    let file = crate::lens_db::assign(&key, &profile, how, &format!("{} ({})", t.name, v.path))?;
    Ok((file, key.describe(), t.name.clone()))
}

#[cfg(test)]
mod tests {
    #[test]
    fn no_max_lens_mod_for_a_clip_without_it() {
        let gx0020 = ["GoPro_HERO12 Black_Max_NO-EIS_1080p_16by9_1920x1080-23.98fps (scaled)", "", ""];
        assert!(super::other_lens_mod("GoPro", "HERO12 Black", "Linear", &gx0020));
        assert!(super::other_lens_mod("GoPro", "HERO12 Black", "", &["x", "Max Lens Mod 2.0", ""]));
        assert!(!super::other_lens_mod("GoPro", "HERO12 Black", "Max SuperView", &gx0020));
        assert!(!super::other_lens_mod("GoPro", "HERO12 Black", "Linear", &["GoPro_HERO12 Black_Linear_4K", "", ""]));
        assert!(!super::other_lens_mod("GoPro", "MAX", "", &gx0020));
        assert!(!super::other_lens_mod("DJI", "OsmoAction6", "Wide", &["DJI Max wide", "", ""]));
    }
}

/// A lens as the dialogs compare names: "GoPro_HERO4 Black_Medium__2.7k_..._23.98fps - estimated from footage" and
/// "GoPro HERO4 Black Medium 2.7k ..._23.98fps, built in" are the same one
fn same_name(name: &str) -> String {
    let mut s = name.to_lowercase().replace('_', " ");
    for tail in [" - estimated from footage", ", built in", ", estimated from footage", ", assigned", ", found in the footage, readout measured",
                 ", found in the footage", " (scaled)", " (cropped)"] {
        if let Some(i) = s.find(tail) { s.truncate(i); }
    }
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// The lenses to choose from for a clip's mode, as the Library's and the player's lens dialog show them (`gyroflow-optical
/// --lenses`): the lens in use, then each lens with what Find or Estimate said of it on this clip (its verdict, its
/// readout, measured or its own) and how it's used: {"keep": n} (one found: kept with the readout measured, lens_find::keep)
/// or {"assign": id} (one of the mode's: lens_pick::assign). The ones found first, then those not tried
pub fn choices(path: &Path) -> Result<serde_json::Value, String> {
    use serde_json::json;
    let p = path.to_string_lossy().into_owned();
    let info = crate::probe::lens_info(&p).ok_or("the clip can't be read")?;
    if info.from == "file" {
        return Ok(json!({ "path": p, "camera": info.camera, "mode": info.mode_key, "in_file": true, "now": { "name": info.lens, "from": "file" }, "lenses": [] }));
    }
    let v = video(path)?;
    let now_pick = crate::lens_pick::for_clip(&v.path, &v.clip, false);
    let now = json!({ "name": info.lens, "from": info.from, "readout_ms": now_pick.as_ref().and_then(|p| p.profile.frame_readout_time).map(f64::abs) });
    // A lens given to this clip alone: shown first, its mode's lens named beside it
    let own = crate::lens_pick::clip_lens(&v.path);
    let mode_pick = crate::lens_pick::for_mode(&v.path, &v.clip, false);
    let mode_lens = json!({ "name": mode_pick.as_ref().map(|p| p.label()), "readout_ms": mode_pick.as_ref().and_then(|p| p.profile.frame_readout_time).map(f64::abs) });
    let now_name = if own.is_some() { String::new() } else { same_name(&info.lens) };
    let cands = crate::lens_pick::candidates(&v.path, &v.clip, 24);
    let source = |c: &crate::lens_pick::Candidate| if !c.local { "Gyroflow’s database" } else if c.label.ends_with(", built in") { "built into the player" } else { "the player’s own" };
    let k = read_kept(path);
    let mut rows: Vec<serde_json::Value> = Vec::new();
    let mut seen = std::collections::HashSet::new();
    if let Some(l) = &own {
        // Shown once: not again among the mode's lenses
        seen.insert(same_name(&l.name));
        rows.push(json!({ "name": l.name, "source": format!("for this clip only, {}", if l.how.is_empty() { "chosen" } else { &l.how }),
                          "verdict": "clip", "fits": true, "readout_ms": l.profile.frame_readout_time.filter(|r| *r != 0.0).map(f64::abs),
                          "readout_measured": false, "in_use": true, "clip": true, "use": { "promote": true } }));
    }
    if let Some(k) = &k {
        let row = |t: &Tried, estimated: bool| {
            let cand = cands.iter().find(|(c, _)| same_name(&c.label) == same_name(&t.name)).map(|(c, _)| c);
            let used = if t.profile.is_some() { json!({ "keep": t.n }) } else if let Some(c) = cand { json!({ "assign": c.id }) } else { serde_json::Value::Null };
            json!({ "name": if estimated { "Estimated from this clip".to_string() } else { cand.map(|c| c.label.clone()).unwrap_or_else(|| t.name.replace('_', " ")) },
                    "about": if estimated { t.name.clone() } else { String::new() },
                    "source": if estimated { "estimated from this clip" } else { cand.map(source).unwrap_or("Gyroflow’s database") },
                    "verdict": t.verdict, "fits": t.fits, "readout_ms": t.readout_ms.map(f64::abs), "readout_measured": t.readout_measured,
                    "moved": t.moved, "limit": t.limit, "refraction": t.refraction, "medium_sure": t.medium_sure,
                    "in_use": !estimated && same_name(&t.name) == now_name, "use": used })
        };
        if let Some(e) = &k.estimate { rows.push(row(e, true)); }
        for t in &k.tried { seen.insert(same_name(&t.name)); rows.push(row(t, false)); }
    }
    // Gyroflow's database has the built-in ones too (lens_profiles#41 is in it since v43): the player's own copy shown
    let file_key = |id: &str| { let f = id.rsplit(['/', ':']).next().unwrap_or(id); same_name(&f.split('#').next().unwrap_or(f).trim_end_matches(".json").replace(' ', "_")).replace(' ', "") };
    let local_keys: std::collections::HashSet<String> = cands.iter().filter(|(c, _)| c.local).map(|(c, _)| file_key(&c.id)).collect();
    for (c, prof) in &cands {
        if !c.local && local_keys.contains(&file_key(&c.id)) { continue; }
        if !seen.insert(same_name(&c.label)) { continue; }
        rows.push(json!({ "name": c.label, "source": source(c), "verdict": "untried", "fits": false,
                          "readout_ms": prof.frame_readout_time.filter(|r| *r != 0.0).map(f64::abs), "readout_measured": false,
                          "exact": c.exact, "in_use": same_name(&c.label) == now_name, "use": { "assign": c.id } }));
    }
    Ok(json!({ "path": p, "camera": info.camera, "mode": info.mode_key, "in_file": false, "now": now, "mode_lens": mode_lens, "clip_lens": own.is_some(), "lenses": rows,
               "found": k.as_ref().map(|k| json!({ "quick": k.quick, "untried": k.untried, "accepted": k.accepted, "fragments_with_scene": k.fragments_with_scene,
                                                     "estimated": k.estimate.is_some(), "tried": k.tried.len() })) }))
}

#[cfg(test)]
mod names {
    #[test]
    fn same_lens_by_either_name() {
        assert_eq!(super::same_name("GoPro_HERO4 Black_Medium__2.7k_16by9_2704x1520-23.98fps - estimated from footage"),
                   super::same_name("GoPro HERO4 Black Medium 2.7k 16by9 2704x1520-23.98fps, built in"));
        assert_ne!(super::same_name("GoPro HERO4 Black Medium 2.7k 16by9 2704x1520-23.98fps"), super::same_name("GoPro HERO4 Black Medium 2.7k 16by9 2704x1520-29.97fps"));
    }
}

/// A lens the dialogs offer, as a profile: one found on the clip (`how` "keep", `what` its n: with the readout measured)
/// or one of the mode's (`how` "assign", `what` its candidates id). Its profile, name, how it came, where from
pub fn lens_to_use(path: &Path, how: &str, what: &str) -> Result<(LensProfile, String, String, String), String> {
    match how {
        "keep" => {
            let k = read_kept(path).ok_or("nothing found for this clip yet: Find lens or Estimate lens first")?;
            let t = k.estimate.iter().chain(k.tried.iter()).find(|t| t.n == what).ok_or_else(|| format!("{what}: not one of what was found for this clip"))?;
            let json = t.profile.as_ref().ok_or("that lens can't be kept as a profile of this clip's frame")?;
            let profile = LensProfile::from_json(&json.to_string()).map_err(|e| e.to_string())?;
            let how = if what == "estimate" { "estimated from footage" } else if t.readout_measured { "found in the footage, readout measured" } else { "found in the footage" };
            Ok((profile, t.name.replace('_', " "), how.into(), format!("{} ({})", t.name, path.display())))
        }
        "assign" => {
            let v = video(path)?;
            let (profile, label) = crate::lens_pick::candidate_profile(&v.path, &v.clip, what)?;
            Ok((profile, label, "chosen".into(), what.to_string()))
        }
        _ => Err(format!("{how}: keep or assign")),
    }
}

/// A lens tried on this clip alone (lens_pick::set_clip_lens): its own file written
pub fn try_on_clip(path: &Path, how: &str, what: &str) -> Result<(std::path::PathBuf, String), String> {
    let (profile, name, how, from) = lens_to_use(path, how, what)?;
    let file = crate::lens_pick::set_clip_lens(&path.to_string_lossy(), Some((&profile, &name, &how, &from)))?;
    Ok((file, name))
}

/// The lens tried on this clip made its whole mode's (lens_db::assign); the clip's own stays until it's taken away
pub fn promote(path: &Path) -> Result<(std::path::PathBuf, String, String), String> {
    let p = path.to_string_lossy().into_owned();
    let l = crate::lens_pick::clip_lens(&p).ok_or("this clip has no lens of its own to give its mode")?;
    let v = video(path)?;
    let key = crate::lens_db::ModeKey::of(&v.path, &v.clip);
    let how = if l.how.is_empty() { "tried on a clip".to_string() } else { l.how.clone() };
    let file = crate::lens_db::assign(&key, &l.profile, &how, &format!("{} (tried on {})", l.name, p))?;
    Ok((file, key.describe(), l.name))
}
