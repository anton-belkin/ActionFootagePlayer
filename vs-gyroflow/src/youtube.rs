// SPDX-License-Identifier: GPL-3.0-or-later
//
// An export for YouTube (gyroflow-export --youtube, issue #95): HDR10, i.e. PQ in BT.2020 with the mastering display
// and MaxCLL/MaxFALL, in an MKV that carries a 3D LUT (.cube) for YouTube's own SDR version, so a viewer on an SDR
// screen sees the clip in the player's SDR look (DJI's LUT or NBCU's conversion) instead of YouTube's generic one.
//
// What YouTube asks (support.google.com/youtube/answer/7126552, "Upload HDR videos"): PQ or HLG, BT.2020 primaries
// and matrix; with PQ the mastering display (SMPTE ST 2086) and MaxCLL/MaxFALL (CTA-861.3), else it assumes Sony's
// BVM-X300; HEVC, VP9 or AV1 in MOV, MP4 or MKV. Its SDR down-conversion takes a .cube as a hint, attached to an MKV
// as YouTube's own hdr_metadata tool does it (github.com/YouTubeHDR/hdr_metadata: mkvmerge --attachment-mime-type
// application/x-cube --attach-file <look>.cube). Uploaders report the LUT honored for PQ and ignored for HLG
// (forum.doom9.org/showthread.php?p=1966266), so the video is PQ: the player's HLG (a log curve's LUT, or an HLG
// clip) to display light on BT.2100's 1000-nit reference display (its OOTF, system gamma 1.2) and PQ-encoded, the
// HDR transcode of ITU-R BT.2408. A PQ clip stays as it is.
//
// The LUT inside maps that PQ signal to the SDR look, the way the player shows it from the HLG: PQ -> the HLG it was
// made from (the same OOTF backwards) -> NBCU's HLG -> SDR (luts/3-NBCU_*), or for D-Log M -> D-Log M (the inverse of
// luts/dlogm_hlg.cube, solved numerically: exact for the frames this export made from D-Log M) -> DJI's Rec.709 LUT,
// which isn't redistributed, so this LUT is made on the user's machine from theirs. A PQ clip: NBCU's PQ -> SDR
// (luts/5-NBCU_*), as the player shows it.
//
// The LUT's domain isn't documented by YouTube. Grading apps (Resolve's "Rec.2020 ST2084 to Rec.709" LUT export,
// which YouTube's page describes) and ffmpeg's lut3d take a video's RGB with black at 0 and the limited range's white
// at 1: Levels::Full, the default. Levels::Data is the other convention, NBCU's "DL" LUTs': 0..1 is the 10-bit code
// range, the limited-range signal at 64..940 in and out. Which one YouTube applies needs a test upload.

use std::path::Path;

use crate::gpu_lut::{ read_cube, tetrahedral };

/// BT.2100's reference display for HLG: its OOTF renders the HLG signal for this peak (nits), and HLG at 1.0 is PQ
/// at this many nits
pub const HLG_PEAK: f64 = 1000.0;
/// The grid of the LUTs made here: 65 points (the finest .cube grading apps write)
pub const GRID: usize = 65;

/// How the LUT's 0..1 maps to the video's signal, in and out
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Levels {
    /// 0 is black, 1 the limited range's white (the signal as RGB, as grading apps and ffmpeg's lut3d see it)
    Full,
    /// 0..1 is the 10-bit code range: the limited-range signal sits at 64/1023..940/1023 (NBCU's "DL" LUTs)
    Data,
}

impl Levels {
    pub fn parse(s: &str) -> Option<Levels> {
        match s { "full" => Some(Levels::Full), "data" => Some(Levels::Data), _ => None }
    }
    fn to_signal(self, x: f64) -> f64 {
        match self { Levels::Full => x, Levels::Data => (x * 1023.0 - 64.0) / 876.0 }
    }
    fn from_signal(self, s: f64) -> f64 {
        match self { Levels::Full => s, Levels::Data => (64.0 + 876.0 * s) / 1023.0 }
    }
}

/// The SDR look the LUT inside gives YouTube's SDR version
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Look { Maker, Nbcu }

impl Look {
    pub fn parse(s: &str) -> Option<Look> {
        match s { "maker" => Some(Look::Maker), "nbcu" => Some(Look::Nbcu), _ => None }
    }
    pub fn name(self) -> &'static str { match self { Look::Maker => "maker", Look::Nbcu => "nbcu" } }
}

const LUMA: [f64; 3] = [0.2627, 0.6780, 0.0593];

/// PQ (SMPTE ST 2084) signal -> nits
pub fn pq_eotf(e: f64) -> f64 {
    let (m1, m2, c1, c2, c3) = (0.1593017578125, 78.84375, 0.8359375, 18.8515625, 18.6875);
    let p = e.clamp(0.0, 1.0).powf(1.0 / m2);
    10000.0 * ((p - c1).max(0.0) / (c2 - c3 * p)).powf(1.0 / m1)
}

/// nits -> PQ signal
pub fn pq_oetf(nits: f64) -> f64 {
    let (m1, m2, c1, c2, c3) = (0.1593017578125, 78.84375, 0.8359375, 18.8515625, 18.6875);
    let y = (nits / 10000.0).clamp(0.0, 1.0).powf(m1);
    ((c1 + c2 * y) / (1.0 + c3 * y)).powf(m2)
}

const HLG_A: f64 = 0.17883277;
const HLG_B: f64 = 0.28466892;
const HLG_C: f64 = 0.55991073;

/// HLG signal -> scene light (0..1)
pub fn hlg_inverse_oetf(e: f64) -> f64 {
    let x = e.clamp(0.0, 1.0);
    if x <= 0.5 { x * x / 3.0 } else { (((x - HLG_C) / HLG_A).exp() + HLG_B) / 12.0 }
}

/// Scene light (0..1) -> HLG signal
pub fn hlg_oetf(l: f64) -> f64 {
    let l = l.clamp(0.0, 1.0);
    if l <= 1.0 / 12.0 { (3.0 * l).sqrt() } else { HLG_A * (12.0 * l - HLG_B).ln() + HLG_C }
}

/// HLG R'G'B' (BT.2020) -> PQ R'G'B': BT.2100's OOTF for a 1000-nit display (system gamma 1.2), as the player's Dolby
/// Vision measuring takes it (gpu_lut.rs) and BT.2408's HLG -> PQ transcode does
pub fn hlg_to_pq(hlg: [f64; 3]) -> [f64; 3] {
    let scene = hlg.map(hlg_inverse_oetf);
    let ys: f64 = (0..3).map(|i| LUMA[i] * scene[i]).sum();
    let gain = if ys > 0.0 { HLG_PEAK * ys.powf(0.2) } else { 0.0 };
    scene.map(|e| pq_oetf(gain * e))
}

/// PQ R'G'B' -> the HLG it was made from by hlg_to_pq; light above the 1000-nit display's is clipped there
pub fn pq_to_hlg(pq: [f64; 3]) -> [f64; 3] {
    let display = pq.map(|e| pq_eotf(e).min(HLG_PEAK) / HLG_PEAK);
    let yd: f64 = (0..3).map(|i| LUMA[i] * display[i]).sum();
    if yd <= 0.0 { return [0.0; 3]; }
    // Yd = Ys^1.2, display = Ys^0.2 * scene
    let ys = yd.powf(1.0 / 1.2);
    let k = ys.powf(0.2);
    display.map(|d| hlg_oetf(d / k))
}

/// A .cube LUT read for sampling (red fastest), its size and values
pub struct Cube { n: usize, data: Vec<f32> }

impl Cube {
    pub fn read(path: &Path) -> Result<Cube, String> {
        let (n, data) = read_cube(path)?;
        Ok(Cube { n: n as usize, data })
    }
    /// At rgb (0..1, clamped), by tetrahedral interpolation, as playback's shaders and gpu_lut's composing sample it
    pub fn at(&self, rgb: [f64; 3]) -> [f64; 3] {
        tetrahedral(self.n, &self.data, rgb.map(|v| v as f32)).map(|v| v as f64)
    }
    /// The input that gives `target`: Newton's method on the interpolated LUT, from the neutral axis' inverse, kept in
    /// 0..1 (a target the LUT never reaches ends at the nearest it does, along the way)
    pub fn invert(&self, target: [f64; 3], neutral: &NeutralInverse) -> [f64; 3] {
        let mut x = [0usize, 1, 2].map(|c| neutral.at(c, target[c]));
        let h = 0.5 / (self.n - 1) as f64;
        for _ in 0..30 {
            let y = self.at(x);
            let r = [y[0] - target[0], y[1] - target[1], y[2] - target[2]];
            if r.iter().all(|v| v.abs() < 1e-7) { break; }
            // The Jacobian by central differences over half a cell (the LUT is linear within its tetrahedra)
            let mut j = [[0.0; 3]; 3];
            for k in 0..3 {
                let (mut a, mut b) = (x, x);
                a[k] = (x[k] + h).min(1.0);
                b[k] = (x[k] - h).max(0.0);
                let (ya, yb) = (self.at(a), self.at(b));
                for i in 0..3 { j[i][k] = (ya[i] - yb[i]) / (a[k] - b[k]).max(1e-12); }
            }
            let Some(step) = solve3(j, r) else { break };
            let mut moved = 0.0f64;
            for k in 0..3 {
                let v = (x[k] - step[k]).clamp(0.0, 1.0);
                moved = moved.max((v - x[k]).abs());
                x[k] = v;
            }
            if moved < 1e-9 { break; }
        }
        x
    }
}

/// Each channel of a LUT along its neutral axis, inverted: where to start inverting the LUT
pub struct NeutralInverse { curves: [Vec<(f64, f64)>; 3] }

impl NeutralInverse {
    pub fn new(cube: &Cube) -> NeutralInverse {
        let steps = 1024;
        let samples: Vec<(f64, [f64; 3])> = (0..=steps).map(|i| { let g = i as f64 / steps as f64; (g, cube.at([g; 3])) }).collect();
        let curves = [0, 1, 2].map(|c| {
            // Kept increasing (a flat or folding stretch would make the inverse ambiguous)
            let mut v: Vec<(f64, f64)> = Vec::new();
            for (g, y) in &samples {
                if v.last().is_none_or(|&(last, _)| y[c] > last) { v.push((y[c], *g)); }
            }
            v
        });
        NeutralInverse { curves }
    }
    fn at(&self, c: usize, y: f64) -> f64 {
        let v = &self.curves[c];
        if v.is_empty() { return 0.5; }
        if y <= v[0].0 { return v[0].1; }
        let i = v.partition_point(|&(yy, _)| yy < y);
        if i >= v.len() { return v[v.len() - 1].1; }
        let ((y0, g0), (y1, g1)) = (v[i - 1], v[i]);
        g0 + (g1 - g0) * (y - y0) / (y1 - y0)
    }
}

fn solve3(m: [[f64; 3]; 3], v: [f64; 3]) -> Option<[f64; 3]> {
    let det = m[0][0] * (m[1][1] * m[2][2] - m[1][2] * m[2][1]) - m[0][1] * (m[1][0] * m[2][2] - m[1][2] * m[2][0])
        + m[0][2] * (m[1][0] * m[2][1] - m[1][1] * m[2][0]);
    if det.abs() < 1e-14 { return None; }
    let col = |k: usize| {
        let mut a = m;
        for i in 0..3 { a[i][k] = v[i]; }
        a[0][0] * (a[1][1] * a[2][2] - a[1][2] * a[2][1]) - a[0][1] * (a[1][0] * a[2][2] - a[1][2] * a[2][0])
            + a[0][2] * (a[1][0] * a[2][1] - a[1][1] * a[2][0])
    };
    Some([col(0) / det, col(1) / det, col(2) / det])
}

/// A Type III LUT's (NBCU's) value at a 0..1 signal, back to a 0..1 signal (clipped: its highlights above 100 % too,
/// as playback clips them)
fn type3(cube: &Cube, s: [f64; 3]) -> [f64; 3] {
    let (lo, span) = (64.0 / 1023.0, 876.0 / 1023.0);
    let o = cube.at(s.map(|v| v * span + lo));
    o.map(|v| ((v - lo) / span).clamp(0.0, 1.0))
}

/// Where the SDR look comes from, for the LUT inside
pub enum SdrFrom<'a> {
    /// The video is a log curve's HLG (`log_hlg`, its LUT) made PQ: back to HLG, then NBCU's HLG -> SDR
    HlgNbcu { nbcu_hlg: &'a Path },
    /// ... back to HLG, then to the log signal (the inverse of `log_hlg`), then the maker's LUT (DJI's)
    HlgMaker { log_hlg: &'a Path, maker: &'a Path },
    /// The video was PQ already: NBCU's PQ -> SDR
    PqNbcu { nbcu_pq: &'a Path },
}

/// The LUT for YouTube's SDR version: the export's PQ signal -> Rec.709 (BT.1886, as the player's SDR looks are), `n`
/// points, in `levels`
pub fn sdr_lut(from: &SdrFrom, n: usize, levels: Levels, title: &str) -> Result<String, String> {
    let mut text = format!("TITLE \"{}\"\n# PQ (BT.2100, BT.2020) -> SDR (BT.709), by gyroflow-export --youtube; {}\nLUT_3D_SIZE {n}\nDOMAIN_MIN 0 0 0\nDOMAIN_MAX 1 1 1\n",
                           title.replace('"', "'"), match levels { Levels::Full => "0 black, 1 white", Levels::Data => "0..1 the 10-bit code range" });
    let f: Box<dyn Fn([f64; 3]) -> [f64; 3] + Sync> = match from {
        SdrFrom::HlgNbcu { nbcu_hlg } => {
            let nbcu = Cube::read(nbcu_hlg)?;
            Box::new(move |pq| type3(&nbcu, pq_to_hlg(pq)))
        }
        SdrFrom::HlgMaker { log_hlg, maker } => {
            let (log, maker) = (Cube::read(log_hlg)?, Cube::read(maker)?);
            let neutral = NeutralInverse::new(&log);
            Box::new(move |pq| maker.at(log.invert(pq_to_hlg(pq), &neutral)).map(|v| v.clamp(0.0, 1.0)))
        }
        SdrFrom::PqNbcu { nbcu_pq } => {
            let nbcu = Cube::read(nbcu_pq)?;
            Box::new(move |pq| type3(&nbcu, pq))
        }
    };
    // Each blue plane on its own thread: the inverse takes some Newton steps per point
    let planes: Vec<String> = std::thread::scope(|s| {
        let f = &f;
        let jobs: Vec<_> = (0..n).map(|b| s.spawn(move || {
            let mut out = String::with_capacity(n * n * 28);
            for g in 0..n {
                for r in 0..n {
                    let x = [r, g, b].map(|i| levels.to_signal(i as f64 / (n - 1) as f64).clamp(0.0, 1.0));
                    let o = f(x).map(|v| levels.from_signal(v));
                    out += &format!("{:.6} {:.6} {:.6}\n", o[0], o[1], o[2]);
                }
            }
            out
        })).collect();
        jobs.into_iter().map(|j| j.join().unwrap_or_default()).collect()
    });
    for p in planes { text += &p; }
    Ok(text)
}

/// The video's own LUT: a log curve's LUT to HLG (`log_hlg`), or nothing (an HLG clip), then HLG -> PQ, at `n` points
/// (the log LUT sampled between its points as playback interpolates it, the PQ curve computed at each)
pub fn pq_lut(log_hlg: Option<&Path>, n: usize) -> Result<String, String> {
    let log = log_hlg.map(Cube::read).transpose()?;
    let what = log_hlg.and_then(|p| p.file_name()).map_or("HLG".to_string(), |f| f.to_string_lossy().to_string());
    let mut text = format!("# {what} then HLG -> PQ (BT.2100's OOTF for 1000 nits), by gyroflow-export --youtube\nLUT_3D_SIZE {n}\n");
    for b in 0..n {
        for g in 0..n {
            for r in 0..n {
                let x = [r, g, b].map(|i| i as f64 / (n - 1) as f64);
                let hlg = match &log { Some(c) => c.at(x), None => x };
                let o = hlg_to_pq(hlg.map(|v| v.clamp(0.0, 1.0)));
                text += &format!("{:.6} {:.6} {:.6}\n", o[0], o[1], o[2]);
            }
        }
    }
    Ok(text)
}

/// The mastering display, in ffmpeg's -mastering_display syntax (x, y in 1/50000, luminance in 1/10000 nits): the
/// one YouTube's hdr_metadata tool writes by default, Sony's BVM-X300: P3 primaries, D65, 1000 nits peak, 0.01 black
pub const MASTERING_DISPLAY: &str = "G(13250,34500)B(7500,3000)R(34000,16000)WP(15635,16450)L(10000000,100)";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pq_round_trip() {
        for nits in [0.0, 0.005, 1.0, 100.0, 203.0, 1000.0, 4000.0, 10000.0] {
            assert!((pq_eotf(pq_oetf(nits)) - nits).abs() < 1e-6 * nits.max(1.0), "{nits}");
        }
        // BT.2408: 203 nits (HDR reference white) is PQ 58 %
        assert!((pq_oetf(203.0) - 0.58).abs() < 0.002);
    }

    #[test]
    fn hlg_pq_round_trip() {
        // HLG 75 % grey (reference white) on the 1000-nit display: 203 nits, PQ 58 % (BT.2408)
        let w = hlg_to_pq([0.75; 3]);
        assert!((pq_eotf(w[0]) - 203.0).abs() < 1.5, "{}", pq_eotf(w[0]));
        // HLG's peak is the display's
        assert!((pq_eotf(hlg_to_pq([1.0; 3])[0]) - 1000.0).abs() < 0.01);
        for c in [[0.1, 0.5, 0.9], [0.75, 0.2, 0.3], [0.0, 0.0, 0.6], [0.95, 0.95, 0.1], [0.3, 0.31, 0.32]] {
            let back = pq_to_hlg(hlg_to_pq(c));
            for i in 0..3 { assert!((back[i] - c[i]).abs() < 1e-6, "{c:?} -> {back:?}"); }
        }
    }

    #[test]
    fn levels() {
        assert_eq!(Levels::Data.from_signal(0.0), 64.0 / 1023.0);
        assert!((Levels::Data.to_signal(Levels::Data.from_signal(0.37)) - 0.37).abs() < 1e-12);
        assert_eq!(Levels::Full.to_signal(0.5), 0.5);
    }

    fn luts() -> std::path::PathBuf { Path::new(env!("CARGO_MANIFEST_DIR")).join("../luts") }

    #[test]
    fn invert_dlogm() {
        let cube = Cube::read(&luts().join("dlogm_hlg.cube")).unwrap();
        let neutral = NeutralInverse::new(&cube);
        for x in [[0.1, 0.1, 0.1], [0.4, 0.35, 0.3], [0.62, 0.55, 0.41], [0.2, 0.6, 0.5], [0.8, 0.7, 0.75], [0.33, 0.33, 0.6]] {
            let back = cube.invert(cube.at(x), &neutral);
            let (y, yb) = (cube.at(x), cube.at(back));
            for i in 0..3 { assert!((yb[i] - y[i]).abs() < 1e-5, "{x:?}: {back:?}"); }
        }
    }

    // The whole chain as the video and YouTube would run it from D-Log M: the export's PQ LUT, then the LUT inside
    // (both written as .cube and read back, at `n` points), against the player's own SDR look at points between the
    // grids. The worst and the mean error, in 10-bit codes of the SDR signal
    fn chain_error(from: &SdrFrom, want: &dyn Fn([f64; 3]) -> [f64; 3], n: usize) -> (f64, f64) {
        let dir = std::env::temp_dir().join(format!("afp-youtube-test-{}-{n}-{:?}", std::process::id(), std::thread::current().id()));
        std::fs::create_dir_all(&dir).unwrap();
        let (video, sdr) = (dir.join("v.cube"), dir.join("s.cube"));
        std::fs::write(&video, pq_lut(Some(&luts().join("dlogm_hlg.cube")), GRID).unwrap()).unwrap();
        std::fs::write(&sdr, sdr_lut(from, n, Levels::Full, "test").unwrap()).unwrap();
        let (video, sdr) = (Cube::read(&video).unwrap(), Cube::read(&sdr).unwrap());
        let _ = std::fs::remove_dir_all(&dir);
        let (mut worst, mut sum, mut count) = (0.0f64, 0.0f64, 0.0f64);
        let k = 23;
        for b in 0..k { for g in 0..k { for r in 0..k {
            let x = [r, g, b].map(|i| (i as f64 + 0.37) / k as f64);
            let (w, got) = (want(x), sdr.at(video.at(x)));
            for i in 0..3 { let e = (got[i] - w[i]).abs() * 876.0; worst = worst.max(e); sum += e; count += 1.0; }
        }}}
        (worst, sum / count)
    }

    #[test]
    fn nbcu_chain_matches_playback() {
        let (log, nbcu_path) = (Cube::read(&luts().join("dlogm_hlg.cube")).unwrap(), luts().join("3-NBCU_HLG2SDR_DL_v1.2.cube"));
        let nbcu = Cube::read(&nbcu_path).unwrap();
        for n in [33, GRID] {
            let (worst, mean) = chain_error(&SdrFrom::HlgNbcu { nbcu_hlg: &nbcu_path }, &|x| type3(&nbcu, log.at(x)), n);
            eprintln!("D-Log M through NBCU, {n} points: worst {worst:.2}, mean {mean:.3} 10-bit codes");
            if n == GRID { assert!(worst < 12.0 && mean < 1.0, "worst {worst:.2}, mean {mean:.3}"); }
        }
    }

    // DJI's LUT isn't in the repository: AFP_DJI_LUT=<dji-dlogm-rec709.cube> cargo test maker_chain -- --nocapture
    #[test]
    fn maker_chain_matches_playback() {
        let Some(maker_path) = std::env::var_os("AFP_DJI_LUT").map(std::path::PathBuf::from) else { return };
        let log_path = luts().join("dlogm_hlg.cube");
        let maker = Cube::read(&maker_path).unwrap();
        for n in [33, GRID] {
            let (worst, mean) = chain_error(&SdrFrom::HlgMaker { log_hlg: &log_path, maker: &maker_path }, &|x| maker.at(x), n);
            eprintln!("D-Log M through DJI's LUT, {n} points: worst {worst:.2}, mean {mean:.3} 10-bit codes");
        }
    }
}
