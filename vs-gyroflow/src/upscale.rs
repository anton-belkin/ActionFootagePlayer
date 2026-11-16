// SPDX-License-Identifier: GPL-3.0-or-later
//
// A photo's upscaler (gyroflow-export --photo --upscale, docs/photo-upscaling.md). A photo cut from a zoomed-in part
// of the frame has fewer pixels of the camera's in it than it's saved with: the warp stretched them (Lanczos, as it
// plays). How much is the stretch: output pixels per source pixel across the crop (`stretch`, from the lens geometry
// gyroflow renders with). What makes the missing ones:
//   - none: the warp's own interpolation, as the player always did
//   - FSR: AMD's FSR 1 (the player's fsr.glsl, EASU + RCAS): edges kept sharp, nothing added
//   - Real-ESRGAN x4plus, SwinIR-L (real-world SR, GAN): neural networks that draw detail the frame doesn't have,
//     plausible texture, sometimes things that weren't there (a porthole on a yacht, rock on a deck). macOS only, by
//     afp-upscale (macos/upscale.swift, Core ML), from models downloaded on first use (MODELS)
//   - Auto: none when the crop isn't stretched, FSR for a weak stretch, SwinIR-L for a strong one (WEAK, STRONG)
//
// A neural one runs once, on the SDR rendition, at the source's own density (the crop shrunk by its stretch: the
// pixels the camera has there), x4, then brought to the photo's size by Lanczos. The HDR rendition is made from that
// upscaled SDR and the per-pixel ratio of HDR to SDR at the source's density, smoothly scaled up (hdr_from_sdr): the
// gain map follows the picture's light, not the texture the model drew, and the highlights keep their HDR.

use std::io::{ BufRead, BufReader, Read };
use std::path::{ Path, PathBuf };
use std::process::{ Command, Stdio };

use crate::photo::{ self, Image };

/// Up to this stretch the crop counts as not stretched: Auto leaves it as the warp drew it
pub const WEAK_STRETCH: f64 = 1.1;
/// From this stretch on Auto draws detail (SwinIR-L); between the two, FSR. Not 2: an Action 6 frame above water is
/// already stretched ~1.9 in its middle and ~2.2 at its edges by the lens correction and the stabilization's crop (an
/// unzoomed photo should get FSR), and Anton's strong sample was 2.46
pub const STRONG_STRETCH: f64 = 2.4;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Upscaler { None, Fsr, RealEsrgan, SwinIr }

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Choice { Auto, Fixed(Upscaler) }

impl Choice {
    /// auto, none, fsr, realesrgan, swinir
    pub fn parse(v: &str) -> Option<Choice> {
        Some(match v {
            "auto" => Choice::Auto,
            "none" | "warp" => Choice::Fixed(Upscaler::None),
            "fsr" => Choice::Fixed(Upscaler::Fsr),
            "realesrgan" | "real-esrgan" => Choice::Fixed(Upscaler::RealEsrgan),
            "swinir" | "swinir-l" => Choice::Fixed(Upscaler::SwinIr),
            _ => return None,
        })
    }

    /// The upscaler for a crop stretched this much (output pixels per source pixel)
    pub fn resolve(self, stretch: f64) -> Upscaler {
        match self {
            Choice::Fixed(u) => u,
            Choice::Auto if stretch < WEAK_STRETCH => Upscaler::None,
            Choice::Auto if stretch < STRONG_STRETCH => Upscaler::Fsr,
            // The neural ones run on macOS only (afp-upscale): elsewhere FSR all the way
            Choice::Auto if cfg!(target_os = "macos") => Upscaler::SwinIr,
            Choice::Auto => Upscaler::Fsr,
        }
    }
}

impl Upscaler {
    pub fn label(self) -> &'static str {
        match self {
            Upscaler::None => "as the warp draws it",
            Upscaler::Fsr => "FSR 1",
            Upscaler::RealEsrgan => "Real-ESRGAN x4plus (draws detail)",
            Upscaler::SwinIr => "SwinIR-L (draws detail)",
        }
    }
    pub fn model(self) -> Option<&'static Model> {
        MODELS.iter().find(|m| Some(m.upscaler) == Some(self))
    }
}

// ---------------------------------------------------------------------------------------------------
// The models: Core ML conversions (tools/upscalers/convert.py), each a .mlpackage zipped, fetched into the app's
// data folder on first use and checked against its SHA-256. Kept out of the app and out of git (31 and 52 MB zipped).

pub struct Model {
    pub upscaler: Upscaler,
    /// The .mlpackage's name in the models folder
    pub package: &'static str,
    /// The zip it's fetched as, its SHA-256 and size
    pub archive: &'static str,
    pub sha256: &'static str,
    pub bytes: u64,
    pub source: &'static str,
}

pub const MODELS: &[Model] = &[
    Model { upscaler: Upscaler::RealEsrgan, package: "RealESRGAN_x4plus_256.mlpackage", archive: "RealESRGAN_x4plus_256.mlpackage.zip",
            sha256: "b54543f0773df6863de69556e681638d7c50101d97f7e97c49a40ba7414adbee", bytes: 31_031_414,
            source: "Real-ESRGAN x4plus, Xintao Wang et al., BSD-3-Clause: https://github.com/xinntao/Real-ESRGAN" },
    Model { upscaler: Upscaler::SwinIr, package: "SwinIR-L_x4_GAN_192.mlpackage", archive: "SwinIR-L_x4_GAN_192.mlpackage.zip",
            sha256: "65bebceb69ba13311a944b8afc2a70d30adfa8205805f570f7bdd8d4f18e096f", bytes: 52_447_316,
            source: "SwinIR-L x4 real-world SR (GAN), Jingyun Liang et al., Apache-2.0: https://github.com/JingyunLiang/SwinIR" },
];

/// Where the models are fetched from: nowhere yet (where to host them is Anton's call); $AFP_MODELS_URL for a test
pub const MODELS_URL: Option<&str> = None;

/// The app's data folder's models/ (macOS: ~/Library/Application Support/ActionFootagePlayer/models), or
/// $AFP_MODELS_DIR
pub fn models_dir() -> Option<PathBuf> {
    let env = |k: &str| std::env::var_os(k).filter(|v| !v.is_empty()).map(PathBuf::from);
    if let Some(d) = env("AFP_MODELS_DIR") { return Some(d); }
    crate::lens_pick::user_file().and_then(|f| f.parent().map(|d| d.join("models")))
}

/// The model's package, fetched first if it isn't there. `progress`: the share of it downloaded
pub fn ensure_model(model: &Model, progress: &dyn Fn(f64)) -> Result<PathBuf, String> {
    let dir = models_dir().ok_or("no folder for the upscaler's models")?;
    let package = dir.join(model.package);
    if package.is_dir() { return Ok(package); }
    let base = std::env::var("AFP_MODELS_URL").ok().filter(|v| !v.is_empty()).or(MODELS_URL.map(str::to_string))
        .ok_or_else(|| format!("{} isn't downloaded and there's nowhere to fetch it from yet: put {} in {}", model.upscaler.label(), model.package, dir.display()))?;
    std::fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let zip = dir.join(format!(".{}.part", model.archive));
    let url = format!("{}/{}", base.trim_end_matches('/'), model.archive);
    let mut curl = Command::new("curl").args(["-fsSL", "--retry", "2", "-o"]).arg(&zip).arg(&url)
        .stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::piped()).spawn().map_err(|e| format!("curl: {e}"))?;
    // How far: the file's size against the model's
    loop {
        if let Some(status) = curl.try_wait().map_err(|e| e.to_string())? {
            if !status.success() {
                let mut err = String::new();
                if let Some(mut s) = curl.stderr.take() { let _ = s.read_to_string(&mut err); }
                let _ = std::fs::remove_file(&zip);
                return Err(format!("{url}: {}", err.trim()));
            }
            break;
        }
        if model.bytes > 0 { progress(std::fs::metadata(&zip).map(|m| m.len() as f64 / model.bytes as f64).unwrap_or(0.0).min(1.0)); }
        std::thread::sleep(std::time::Duration::from_millis(250));
    }
    let sum = sha256_file(&zip)?;
    if !model.sha256.is_empty() && !sum.eq_ignore_ascii_case(model.sha256) {
        let _ = std::fs::remove_file(&zip);
        return Err(format!("{}: checksum {sum}, not {} (download damaged?)", model.archive, model.sha256));
    }
    unzip(&zip, &dir)?;
    let _ = std::fs::remove_file(&zip);
    if package.is_dir() { Ok(package) } else { Err(format!("{} has no {}", model.archive, model.package)) }
}

pub fn sha256_file(path: &Path) -> Result<String, String> {
    use sha2::Digest;
    let mut f = std::fs::File::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let mut h = sha2::Sha256::new();
    let mut buf = vec![0u8; 1 << 20];
    loop {
        let n = f.read(&mut buf).map_err(|e| format!("{}: {e}", path.display()))?;
        if n == 0 { break; }
        h.update(&buf[..n]);
    }
    Ok(h.finalize().iter().map(|b| format!("{b:02x}")).collect())
}

fn unzip(zip: &Path, dir: &Path) -> Result<(), String> {
    let mut cmd = if cfg!(target_os = "macos") { let mut c = Command::new("ditto"); c.args(["-x", "-k"]).arg(zip).arg(dir); c }
                  else { let mut c = Command::new("unzip"); c.args(["-q", "-o"]).arg(zip).arg("-d").arg(dir); c };
    let out = cmd.output().map_err(|e| format!("unzip: {e}"))?;
    if out.status.success() { Ok(()) } else { Err(format!("unzip {}: {}", zip.display(), String::from_utf8_lossy(&out.stderr).trim())) }
}

// ---------------------------------------------------------------------------------------------------
// Resampling

#[derive(Clone, Copy)]
pub enum Filter { Lanczos3, Triangle }

impl Filter {
    fn support(self) -> f64 { match self { Filter::Lanczos3 => 3.0, Filter::Triangle => 1.0 } }
    fn weight(self, x: f64) -> f64 {
        let x = x.abs();
        match self {
            Filter::Triangle => (1.0 - x).max(0.0),
            Filter::Lanczos3 if x < 1e-9 => 1.0,
            Filter::Lanczos3 if x >= 3.0 => 0.0,
            Filter::Lanczos3 => { let p = std::f64::consts::PI * x; 3.0 * p.sin() * (p / 3.0).sin() / (p * p) }
        }
    }
}

/// Each output index's taps: (first source index, weights), the edges repeated
fn taps(src: usize, dst: usize, filter: Filter) -> Vec<(Vec<usize>, Vec<f32>)> {
    let scale = dst as f64 / src as f64;
    // Shrinking, the filter widens so every source pixel counts
    let stretch = (1.0 / scale).max(1.0);
    let support = filter.support() * stretch;
    (0..dst).map(|i| {
        let center = (i as f64 + 0.5) / scale - 0.5;
        let (lo, hi) = ((center - support).floor() as i64 + 1, (center + support).floor() as i64);
        let mut idx = Vec::new();
        let mut w = Vec::new();
        for j in lo..=hi {
            let v = filter.weight((j as f64 - center) / stretch);
            if v == 0.0 { continue; }
            idx.push(j.clamp(0, src as i64 - 1) as usize);
            w.push(v);
        }
        let sum: f64 = w.iter().sum();
        (idx, w.iter().map(|v| (v / sum) as f32).collect())
    }).collect()
}

fn par_rows<T: Send>(rows: usize, row_len: usize, out: &mut [T], f: impl Fn(usize, &mut [T]) + Sync) {
    let threads = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4).min(rows.max(1));
    let per = rows.div_ceil(threads);
    std::thread::scope(|s| {
        for (k, chunk) in out.chunks_mut(per * row_len).enumerate() {
            let f = &f;
            s.spawn(move || for (r, row) in chunk.chunks_mut(row_len).enumerate() { f(k * per + r, row) });
        }
    });
}

/// The picture at another size, each axis on its own
pub fn resize(img: &Image, w: usize, h: usize, filter: Filter) -> Image {
    if (img.width, img.height) == (w, h) { return img.clone(); }
    let (sw, sh) = (img.width, img.height);
    // Across, then down
    let tx = taps(sw, w, filter);
    let mut mid = vec![[0f32; 3]; w * sh];
    par_rows(sh, w, &mut mid, |y, row| {
        let src = &img.pixels[y * sw..(y + 1) * sw];
        for (x, (idx, wt)) in tx.iter().enumerate() {
            let mut a = [0f32; 3];
            for (&i, &k) in idx.iter().zip(wt) { for c in 0..3 { a[c] += src[i][c] * k; } }
            row[x] = a;
        }
    });
    let ty = taps(sh, h, filter);
    let mut pixels = vec![[0f32; 3]; w * h];
    par_rows(h, w, &mut pixels, |y, row| {
        let (idx, wt) = &ty[y];
        for (x, out) in row.iter_mut().enumerate() {
            let mut a = [0f32; 3];
            for (&i, &k) in idx.iter().zip(wt) { for c in 0..3 { a[c] += mid[i * w + x][c] * k; } }
            *out = a;
        }
    });
    Image { width: w, height: h, pixels }
}

fn clamp01(img: Image) -> Image {
    Image { pixels: img.pixels.into_iter().map(|p| p.map(|v| v.clamp(0.0, 1.0))).collect(), ..img }
}

// ---------------------------------------------------------------------------------------------------
// HDR from the upscaled SDR

/// Keeps the ratio sane in the blacks (as the gain map's offsets do)
const K: f32 = 1.0 / 64.0;

/// HLG's OETF: scene light (0..1) to the signal
fn hlg_oetf(l: f32) -> f32 {
    const A: f32 = 0.178_832_77;
    const B: f32 = 0.284_668_92;
    const C: f32 = 0.559_910_7;
    let l = l.max(0.0);
    if l <= 1.0 / 12.0 { (3.0 * l).sqrt() } else { A * (12.0 * l - B).ln() + C }
}

/// The HDR rendition (HLG) of `sdr_big`, an upscale of `sdr`, whose HDR rendition `hdr` is: per pixel and color, the
/// ratio of HLG's scene light to the SDR's light at the small size, in stops, scaled up smoothly (bilinear) and laid
/// on the big SDR. Where the SDR is clipped (a highlight), the ratio carries the HDR's brightness over
pub fn hdr_from_sdr(sdr: &Image, hdr: &Image, sdr_big: &Image) -> Image {
    assert_eq!((sdr.width, sdr.height), (hdr.width, hdr.height));
    let ratio = Image { width: sdr.width, height: sdr.height, pixels: sdr.pixels.iter().zip(&hdr.pixels).map(|(s, d)| {
        let mut r = [0f32; 3];
        for c in 0..3 { r[c] = ((photo::hlg_inverse_oetf(d[c]) + K) / (photo::srgb_to_linear(s[c]) + K)).log2(); }
        r
    }).collect() };
    let ratio = resize(&ratio, sdr_big.width, sdr_big.height, Filter::Triangle);
    let pixels = sdr_big.pixels.iter().zip(&ratio.pixels).map(|(s, r)| {
        let mut o = [0f32; 3];
        for c in 0..3 { o[c] = hlg_oetf((r[c].exp2() * (photo::srgb_to_linear(s[c].clamp(0.0, 1.0)) + K) - K).min(1.0)).clamp(0.0, 1.0); }
        o
    }).collect();
    Image { width: sdr_big.width, height: sdr_big.height, pixels }
}

// ---------------------------------------------------------------------------------------------------
// The neural path

/// The size the crop has at the source's own density: shrunk by its stretch (never grown)
pub fn source_size((w, h): (usize, usize), stretch: f64) -> (usize, usize) {
    let s = stretch.max(1.0);
    (((w as f64 / s).round() as usize).max(8), ((h as f64 / s).round() as usize).max(8))
}

/// The photo's renditions through a neural model (x4) by afp-upscale (`helper`), as the module's comment says.
/// `dir` for its scratch files; `progress` gets the share done
pub fn neural(model: &Path, helper: &Path, dir: &Path, sdr: &Image, hdr: Option<&Image>, stretch: f64, progress: &dyn Fn(f64)) -> Result<(Image, Option<Image>), String> {
    let (w, h) = (sdr.width, sdr.height);
    let (sw, sh) = source_size((w, h), stretch);
    let sdr_s = clamp01(resize(sdr, sw, sh, Filter::Lanczos3));
    let hdr_s = hdr.map(|i| clamp01(resize(i, sw, sh, Filter::Lanczos3)));
    let id = format!("{}-{}", std::process::id(), std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.subsec_nanos()).unwrap_or(0));
    let (inp, out) = (TempFile(dir.join(format!(".upscale-{id}.in.rgb16"))), TempFile(dir.join(format!(".upscale-{id}.out.rgb16"))));
    photo::write_rgb16(&inp.0, &sdr_s)?;
    let mut child = Command::new(helper).arg("--model").arg(model).args(["--width", &sw.to_string(), "--height", &sh.to_string()])
        .arg("--in").arg(&inp.0).arg("--out").arg(&out.0)
        .stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::piped()).spawn().map_err(|e| format!("{}: {e}", helper.display()))?;
    let mut said = Vec::new();
    for line in BufReader::new(child.stderr.take().unwrap()).lines().map_while(Result::ok) {
        let mut f = line.split_whitespace();
        match (f.next(), f.next().and_then(|v| v.parse::<f64>().ok()), f.next().and_then(|v| v.parse::<f64>().ok())) {
            (Some("progress"), Some(a), Some(b)) if b > 0.0 => progress(a / b),
            (Some("done"), _, _) => {}
            _ => said.push(line),
        }
    }
    let status = child.wait().map_err(|e| e.to_string())?;
    if !status.success() { return Err(said.join("; ")); }
    let big = photo::read_rgb16(&out.0, sw * 4, sh * 4)?;
    let sdr_out = clamp01(resize(&big, w, h, Filter::Lanczos3));
    let hdr_out = hdr_s.as_ref().map(|hs| hdr_from_sdr(&sdr_s, hs, &sdr_out));
    Ok((sdr_out, hdr_out))
}

struct TempFile(PathBuf);
impl Drop for TempFile { fn drop(&mut self) { let _ = std::fs::remove_file(&self.0); } }

// ---------------------------------------------------------------------------------------------------
// The stretch

/// Output pixels per source pixel across `crop` (x, y, w, h in the render's pixels) of a frame rendered at `render`
/// from a source `source` pixels big, through `p` (the view's parameters, the camera applied: Camera::apply): a grid
/// of source pixels taken through the lens correction and stabilization as gyroflow renders them, each cell's area in
/// the output over its area in the source, the median of the cells that land in the crop (the one nearest its middle
/// when none do). None when nothing maps
pub fn stretch(p: &gyroflow_core::stabilization::ComputeParams, frame: usize, fps: f64, render: (usize, usize), crop: (usize, usize, usize, usize), source: (usize, usize)) -> Option<f64> {
    use gyroflow_core::keyframes::KeyframeType;
    use gyroflow_core::stabilization::{ is_valid_point, undistort_points_with_rolling_shutter };
    let (pw, ph) = (p.width as f32, p.height as f32);
    if pw <= 0.0 || ph <= 0.0 || p.output_width == 0 || source.0 == 0 { return None; }
    let (nx, ny) = (64usize, 36usize);
    let mut grid = Vec::with_capacity((nx + 1) * (ny + 1));
    for j in 0..=ny { for i in 0..=nx { grid.push((i as f32 * pw / nx as f32, j as f32 * ph / ny as f32)); } }
    let ts = gyroflow_core::timestamp_at_frame(frame as i32, fps.max(1.0));
    let amount = p.keyframes.value_at_video_timestamp(&KeyframeType::LensCorrectionStrength, ts).unwrap_or(p.lens_correction_amount);
    let out = undistort_points_with_rolling_shutter(&grid, ts, Some(frame), p, amount, true, false);
    if out.len() != grid.len() { return None; }
    // The output's points in the render's pixels, the source's cells in the source's
    let to_render = render.0 as f64 / p.output_width as f64;
    let src_cell = (pw as f64 / nx as f64) * (source.0 as f64 / pw as f64) * (ph as f64 / ny as f64) * (source.1 as f64 / ph as f64);
    let (cx0, cy0, cw, ch) = (crop.0 as f64, crop.1 as f64, crop.2 as f64, crop.3 as f64);
    let mid = (cx0 + cw / 2.0, cy0 + ch / 2.0);
    let mut inside = Vec::new();
    let mut nearest: Option<(f64, f64)> = None;
    for j in 0..ny {
        for i in 0..nx {
            let q = [out[j * (nx + 1) + i], out[j * (nx + 1) + i + 1], out[(j + 1) * (nx + 1) + i + 1], out[(j + 1) * (nx + 1) + i]];
            if !q.iter().all(|&v| is_valid_point(v) && v.0.abs() < 1e6 && v.1.abs() < 1e6) { continue; }
            let q: Vec<(f64, f64)> = q.iter().map(|v| (v.0 as f64 * to_render, v.1 as f64 * to_render)).collect();
            let area = q.iter().zip(q.iter().cycle().skip(1)).map(|(a, b)| a.0 * b.1 - b.0 * a.1).sum::<f64>().abs() / 2.0;
            let s = (area / src_cell).sqrt();
            if !s.is_finite() || s <= 0.0 { continue; }
            let c = (q.iter().map(|v| v.0).sum::<f64>() / 4.0, q.iter().map(|v| v.1).sum::<f64>() / 4.0);
            if c.0 >= cx0 && c.0 < cx0 + cw && c.1 >= cy0 && c.1 < cy0 + ch { inside.push(s); }
            let d = (c.0 - mid.0).hypot(c.1 - mid.1);
            if nearest.is_none_or(|(nd, _)| d < nd) { nearest = Some((d, s)); }
        }
    }
    if inside.is_empty() { return nearest.map(|n| n.1); }
    inside.sort_by(|a, b| a.total_cmp(b));
    Some(inside[inside.len() / 2])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn auto_by_stretch() {
        assert_eq!(Choice::Auto.resolve(1.0), Upscaler::None);
        assert_eq!(Choice::Auto.resolve(1.09), Upscaler::None);
        assert_eq!(Choice::Auto.resolve(1.3), Upscaler::Fsr);
        assert_eq!(Choice::Auto.resolve(2.46), if cfg!(target_os = "macos") { Upscaler::SwinIr } else { Upscaler::Fsr });
        assert_eq!(Choice::Auto.resolve(2.2), Upscaler::Fsr);
        assert_eq!(Choice::parse("realesrgan").unwrap().resolve(1.0), Upscaler::RealEsrgan);
        assert!(Choice::parse("bicubic").is_none());
    }

    #[test]
    fn resize_keeps_flat_and_ramps() {
        let img = Image { width: 40, height: 30, pixels: (0..1200).map(|i| { let x = (i % 40) as f32 / 39.0; [0.5, x, 0.25] }).collect() };
        for (w, h) in [(160, 120), (13, 9), (40, 30), (97, 31)] {
            let r = resize(&img, w, h, Filter::Lanczos3);
            assert_eq!(r.pixels.len(), w * h);
            assert!(r.pixels.iter().all(|p| (p[0] - 0.5).abs() < 1e-4 && (p[2] - 0.25).abs() < 1e-4));
            // The ramp stays a ramp in the middle
            let y = h / 2;
            let (a, b) = (r.pixels[y * w + w / 4][1], r.pixels[y * w + 3 * w / 4][1]);
            assert!((a - 0.25).abs() < 0.05 && (b - 0.75).abs() < 0.05, "{w}x{h}: {a} {b}");
        }
    }

    #[test]
    fn hdr_from_sdr_is_exact_at_the_same_size() {
        // The ratio laid back on the same SDR gives the HDR back, highlights (SDR clipped) included
        let sdr = Image { width: 3, height: 1, pixels: vec![[0.2, 0.5, 0.8], [1.0, 1.0, 1.0], [0.0, 0.01, 0.3]] };
        let hdr = Image { width: 3, height: 1, pixels: vec![[0.3, 0.55, 0.7], [0.98, 0.95, 0.9], [0.02, 0.05, 0.4]] };
        let back = hdr_from_sdr(&sdr, &hdr, &sdr);
        for (a, b) in back.pixels.iter().zip(&hdr.pixels) { for c in 0..3 { assert!((a[c] - b[c]).abs() < 1e-3, "{a:?} {b:?}"); } }
        // Upscaled: a flat patch stays its HDR
        let flat = |v: [f32; 3], w, h| Image { width: w, height: h, pixels: vec![v; w * h] };
        let big = hdr_from_sdr(&flat([0.4, 0.4, 0.4], 4, 4), &flat([0.6, 0.5, 0.45], 4, 4), &flat([0.4, 0.4, 0.4], 16, 16));
        assert!(big.pixels.iter().all(|p| (p[0] - 0.6).abs() < 1e-3 && (p[1] - 0.5).abs() < 1e-3));
    }

    #[test]
    fn a_clipped_highlight_keeps_its_hdr_upscaled() {
        // A 4x4 highlight clipped in SDR (1.0), HLG 0.95 in HDR, on a 0.5 / 0.6 ground, taken x4: its middle stays at
        // the HDR's level, the ground at the ground's
        let (w, h) = (16, 16);
        let spot = |i: usize| { let (x, y) = (i % w, i / w); (6..10).contains(&x) && (6..10).contains(&y) };
        let sdr = Image { width: w, height: h, pixels: (0..w * h).map(|i| if spot(i) { [1.0; 3] } else { [0.5; 3] }).collect() };
        let hdr = Image { width: w, height: h, pixels: (0..w * h).map(|i| if spot(i) { [0.95; 3] } else { [0.6; 3] }).collect() };
        let big_sdr = clamp01(resize(&sdr, 64, 64, Filter::Lanczos3));
        let big = hdr_from_sdr(&sdr, &hdr, &big_sdr);
        let at = |x: usize, y: usize| big.pixels[y * 64 + x][1];
        assert!((at(32, 32) - 0.95).abs() < 0.02, "the highlight's middle: {}", at(32, 32));
        assert!((at(4, 4) - 0.6).abs() < 0.01, "the ground: {}", at(4, 4));
    }

    #[test]
    fn hlg_oetf_inverts() {
        for e in [0.0, 0.1, 0.5, 0.6, 0.75, 1.0] { assert!((hlg_oetf(photo::hlg_inverse_oetf(e)) - e).abs() < 1e-5, "{e}"); }
    }

    #[test]
    fn source_size_shrinks_only() {
        assert_eq!(source_size((3072, 1728), 2.46), (1249, 702));
        assert_eq!(source_size((800, 600), 0.8), (800, 600));
    }
}
