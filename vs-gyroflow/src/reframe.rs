// SPDX-License-Identifier: GPL-3.0-or-later
//
// A 360° clip's view recorded as you look around (docs/360-reframe.md), as the export reads it: the recording the player
// keeps in the clip's own file, `<clip>.afp.json`, under `view360.recording`, and the head at any time of the clip.
// The player's side is mpv/scripts/view360/recording.lua; the two hold and interpolate the same way:
// - in a part, a straight line between two samples (the FOV in its log), held across a change of projection, which
//   is a cut at the later sample;
// - before the first part its first sample, between two parts the end of the one before, after the last its end.
//
// The tidying (docs/360-reframe.md, "Tidying") is here too, so the player's preview (`gyroflow-probe --reframe`) and
// the export are the same code:
// - level: the head's roll set to 0 (every heading frame is level already, so the picture is);
// - smooth the turns: a Gaussian over time on the head's rotation (SO(3), as quaternions) and on the FOV's log,
//   relative to what the view follows, its window shrinking near a run's ends so they stay where they were recorded.
// Parts that touch (a punch-in) are smoothed as one run, so where they meet is blended; a change of projection is a
// cut, so each projection's stretch is smoothed on its own. The result is a recording again, its samples thinned.

use std::path::{ Path, PathBuf };
use crate::export360::HeadKey;
use crate::stamp::Heading;
use crate::view360::Projection;

/// A sample: at `t` seconds of the clip, the head (degrees) relative to what the view follows, in a projection
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Sample { pub t: f64, pub yaw: f64, pub pitch: f64, pub roll: f64, pub fov: f64, pub projection: Projection }

#[derive(Clone, Debug, PartialEq)]
pub struct Part { pub from: f64, pub to: f64, pub samples: Vec<Sample> }

#[derive(Clone, Debug, PartialEq)]
pub struct Recording {
    /// What the head is relative to
    pub follows: Heading,
    /// In time order, not overlapping
    pub parts: Vec<Part>,
    /// The export's frame and size the player chose ("16:9", "1080p"), as written
    pub frame: Option<String>,
    pub size: Option<String>,
    /// How it's tidied
    pub tidy: Tidy,
}

/// How much the turns are smoothed: the Gaussian's σ
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Turns { Off, Light, Medium, Strong }

impl Turns {
    pub fn from_name(s: &str) -> Option<Turns> {
        Some(match s { "off" => Turns::Off, "light" => Turns::Light, "medium" => Turns::Medium, "strong" => Turns::Strong, _ => return None })
    }
    pub fn name(self) -> &'static str {
        match self { Turns::Off => "off", Turns::Light => "light", Turns::Medium => "medium", Turns::Strong => "strong" }
    }
    /// Seconds
    pub fn sigma(self) -> f64 {
        match self { Turns::Off => 0.0, Turns::Light => 0.25, Turns::Medium => 0.6, Turns::Strong => 1.5 }
    }
}

/// The recording's `tidy`: what's not there is the default, the turns smoothed (medium) and the horizon level
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Tidy { pub turns: Turns, pub level: bool, pub picture: bool }

impl Default for Tidy {
    fn default() -> Tidy { Tidy { turns: Turns::Medium, level: true, picture: false } }
}

impl Tidy {
    pub fn from_json(v: Option<&serde_json::Value>) -> Tidy {
        let d = Tidy::default();
        let Some(v) = v else { return d };
        Tidy {
            turns: v.get("turns").and_then(|x| x.as_str()).and_then(Turns::from_name).unwrap_or(d.turns),
            level: v.get("level").and_then(|x| x.as_bool()).unwrap_or(d.level),
            picture: v.get("picture").and_then(|x| x.as_bool()).unwrap_or(d.picture),
        }
    }
    /// Nothing to do: the samples as recorded
    pub fn none(&self) -> bool { self.turns == Turns::Off && !self.level }
}

/// Where the player keeps the clip's file: the clip's path without its extension, `.afp.json` (sidecar.lua)
pub fn sidecar_path(clip: &Path) -> PathBuf {
    let name = clip.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
    let stem = match name.rfind('.') { Some(i) if i > 0 => &name[..i], _ => &name };
    clip.with_file_name(format!("{stem}.afp.json"))
}

fn follows(s: &str) -> Option<Heading> {
    Some(match s { "travel" => Heading::Travel, "camera" => Heading::Camera, "north" => Heading::North, "world" => Heading::World, _ => return None })
}

impl Recording {
    /// From the `view360.recording` JSON: parts without two good samples are left out
    pub fn from_json(v: &serde_json::Value) -> Result<Recording, String> {
        let f = v.get("follows").and_then(|f| f.as_str()).ok_or("the recording says nothing of what it follows")?;
        let follows = follows(f).ok_or_else(|| format!("the recording follows {f}, which isn't a heading mode"))?;
        let mut parts = Vec::new();
        for p in v.get("parts").and_then(|p| p.as_array()).map(|a| a.as_slice()).unwrap_or(&[]) {
            let mut samples: Vec<Sample> = Vec::new();
            let mut projection = Projection::Auto;
            for s in p.get("samples").and_then(|s| s.as_array()).map(|a| a.as_slice()).unwrap_or(&[]) {
                let Some(row) = s.as_array() else { continue };
                let n: Vec<f64> = row.iter().take(5).filter_map(|x| x.as_f64()).filter(|x| x.is_finite()).collect();
                if n.len() < 5 || n[4] <= 0.0 { continue; }
                if samples.last().is_some_and(|l| n[0] <= l.t) { continue; }
                if let Some(p) = row.get(5).and_then(|x| x.as_str()).and_then(Projection::from_name).filter(|p| *p != Projection::Equirect) {
                    projection = p;
                }
                samples.push(Sample { t: n[0], yaw: n[1], pitch: n[2], roll: n[3], fov: n[4], projection });
            }
            if samples.len() >= 2 {
                parts.push(Part { from: samples[0].t, to: samples[samples.len() - 1].t, samples });
            }
        }
        if parts.is_empty() { return Err("nothing recorded".into()); }
        parts.sort_by(|a, b| a.from.total_cmp(&b.from));
        let text = |k: &str| v.get(k).and_then(|x| x.as_str()).map(str::to_string);
        Ok(Recording { follows, parts, frame: text("frame"), size: text("size"), tidy: Tidy::from_json(v.get("tidy")) })
    }

    /// The clip's recording, from its file beside it
    pub fn read(clip: &Path) -> Result<Recording, String> {
        let file = sidecar_path(clip);
        let text = std::fs::read_to_string(&file).map_err(|e| format!("no recording: {}: {e}", file.display()))?;
        let doc: serde_json::Value = serde_json::from_str(&text).map_err(|e| format!("{}: {e}", file.display()))?;
        let rec = doc.get("view360").and_then(|v| v.get("recording"))
            .ok_or_else(|| format!("no recording in {}: record a view in the player's 360° tab first", file.display()))?;
        Recording::from_json(rec)
    }

    /// From the first sample to the last
    pub fn span(&self) -> (f64, f64) {
        (self.parts[0].from, self.parts.iter().map(|p| p.to).fold(f64::MIN, f64::max))
    }

    /// The head at `t`
    pub fn head_at(&self, t: f64) -> (HeadKey, Projection) {
        let first = &self.parts[0];
        if t < first.from { return key(&first.samples[0], t); }
        // The last part starting at or before t
        let i = self.parts.partition_point(|p| p.from <= t).saturating_sub(1);
        within(&self.parts[i].samples, t)
    }
}

fn key(s: &Sample, t: f64) -> (HeadKey, Projection) {
    (HeadKey { t, yaw: s.yaw, pitch: s.pitch, roll: s.roll, fov: s.fov }, s.projection)
}

fn within(samples: &[Sample], t: f64) -> (HeadKey, Projection) {
    let (first, last) = (&samples[0], &samples[samples.len() - 1]);
    if t <= first.t { return key(first, t); }
    if t >= last.t { return key(last, t); }
    let j = samples.partition_point(|s| s.t <= t);
    let (a, b) = (&samples[j - 1], &samples[j]);
    if a.projection != b.projection || b.t <= a.t { return key(a, t); }
    let x = (t - a.t) / (b.t - a.t);
    let mix = |p: f64, q: f64| p + (q - p) * x;
    (HeadKey { t, yaw: mix(a.yaw, b.yaw), pitch: mix(a.pitch, b.pitch), roll: mix(a.roll, b.roll), fov: if a.fov == b.fov { a.fov } else { mix(a.fov.ln(), b.fov.ln()).exp() } }, a.projection)
}

// Tidying -----------------------------------------------------------------------------------------------------------

/// Samples a second on the grid the tidying works on
const GRID: f64 = 100.0;
/// Parts this close are one run (a punch-in's remains end and start 1 ms from it)
const TOUCH: f64 = 0.1;
/// The thinning's tolerance, degrees (the FOV as 100·ln), as the player's
pub const TOLERANCE: f64 = 0.05;

type Quat = [f64; 4];

fn qmul(a: Quat, b: Quat) -> Quat {
    [a[0] * b[0] - a[1] * b[1] - a[2] * b[2] - a[3] * b[3],
     a[0] * b[1] + a[1] * b[0] + a[2] * b[3] - a[3] * b[2],
     a[0] * b[2] - a[1] * b[3] + a[2] * b[0] + a[3] * b[1],
     a[0] * b[3] + a[1] * b[2] - a[2] * b[1] + a[3] * b[0]]
}

/// The head as the view composes it: yaw about up, then pitch, then roll (view360.glsl's rot_yaw · rot_pitch · rot_roll)
fn head_quat(yaw: f64, pitch: f64, roll: f64) -> Quat {
    let (y, p, r) = (yaw.to_radians() / 2.0, pitch.to_radians() / 2.0, roll.to_radians() / 2.0);
    qmul(qmul([y.cos(), 0.0, y.sin(), 0.0], [p.cos(), p.sin(), 0.0, 0.0]), [r.cos(), 0.0, 0.0, r.sin()])
}

/// Back to yaw, pitch, roll (degrees, yaw and roll in -180..180)
fn head_angles(q: Quat) -> (f64, f64, f64) {
    let [w, x, y, z] = q;
    // The rotation matrix's entries this needs: m = Ry·Rx·Rz, m12 = -sin pitch, m02/m22 the yaw, m10/m11 the roll
    let m02 = 2.0 * (x * z + w * y);
    let m22 = 1.0 - 2.0 * (x * x + y * y);
    let m12 = 2.0 * (y * z - w * x);
    let m10 = 2.0 * (x * y + w * z);
    let m11 = 1.0 - 2.0 * (x * x + z * z);
    let pitch = (-m12).clamp(-1.0, 1.0).asin();
    (m02.atan2(m22).to_degrees(), pitch.to_degrees(), m10.atan2(m11).to_degrees())
}

/// `a` turned by whole turns to be nearest `near`
fn unwrap_near(a: f64, near: f64) -> f64 { a + ((near - a) / 360.0).round() * 360.0 }

/// Ramer-Douglas-Peucker over samples of one projection: a sample is dropped where the straight line between the kept
/// ones misses it by less than `tol` in every angle and in the FOV's log (×100), as recording.lua thins
pub fn thin(samples: &[Sample], tol: f64) -> Vec<Sample> {
    let n = samples.len();
    if n <= 2 { return samples.to_vec(); }
    let mut keep = vec![false; n];
    keep[0] = true;
    keep[n - 1] = true;
    let mut stack = vec![(0usize, n - 1)];
    while let Some((a, b)) = stack.pop() {
        if b - a < 2 { continue; }
        let (sa, sb) = (&samples[a], &samples[b]);
        let dt = sb.t - sa.t;
        let (la, lb) = (100.0 * sa.fov.ln(), 100.0 * sb.fov.ln());
        let (mut worst, mut at) = (-1.0, a);
        for (i, s) in samples.iter().enumerate().take(b).skip(a + 1) {
            let x = if dt > 0.0 { (s.t - sa.t) / dt } else { 0.0 };
            let line = |p: f64, q: f64| p + (q - p) * x;
            let e = (s.yaw - line(sa.yaw, sb.yaw)).abs()
                .max((s.pitch - line(sa.pitch, sb.pitch)).abs())
                .max((s.roll - line(sa.roll, sb.roll)).abs())
                .max((100.0 * s.fov.ln() - line(la, lb)).abs());
            if e > worst { worst = e; at = i; }
        }
        if worst > tol {
            keep[at] = true;
            stack.push((a, at));
            stack.push((at, b));
        }
    }
    samples.iter().zip(keep).filter(|(_, k)| *k).map(|(s, _)| *s).collect()
}

/// A stretch of one projection, its samples in time order, smoothed and leveled as `tidy` says, on a grid of GRID a
/// second (its first and last sample's times kept)
fn tidy_stretch(samples: &[Sample], tidy: &Tidy) -> Vec<Sample> {
    let (t0, t1) = (samples[0].t, samples[samples.len() - 1].t);
    let projection = samples[0].projection;
    let n = (((t1 - t0) * GRID).ceil() as usize).max(1);
    let times: Vec<f64> = (0..=n).map(|i| if i == n { t1 } else { t0 + i as f64 / GRID }).collect();
    let raw: Vec<HeadKey> = times.iter().map(|&t| within(samples, t).0).collect();
    let roll = |h: &HeadKey| if tidy.level { 0.0 } else { h.roll };
    let mut quats: Vec<Quat> = raw.iter().map(|h| head_quat(h.yaw, h.pitch, roll(h))).collect();
    // One hemisphere along the way, so neighbours average
    for i in 1..quats.len() {
        let (p, q) = (quats[i - 1], quats[i]);
        if p[0] * q[0] + p[1] * q[1] + p[2] * q[2] + p[3] * q[3] < 0.0 { quats[i] = q.map(|c| -c); }
    }
    let logfov: Vec<f64> = raw.iter().map(|h| h.fov.ln()).collect();
    let sigma = tidy.turns.sigma();
    let mut out = Vec::with_capacity(times.len());
    for (i, &t) in times.iter().enumerate() {
        // The window shrinks near the stretch's ends, to nothing at them: the ends stay where they were recorded
        let s = sigma.min((t - t0).min(t1 - t) / 3.0);
        let (q, lf) = if s * GRID < 0.5 {
            (quats[i], logfov[i])
        } else {
            let reach = ((3.0 * s) * GRID).ceil() as usize;
            let (mut acc, mut lacc, mut wsum) = ([0.0f64; 4], 0.0, 0.0);
            for j in i.saturating_sub(reach)..=(i + reach).min(times.len() - 1) {
                let d = (times[j] - t) / s;
                if d.abs() > 3.0 { continue; }
                let w = (-0.5 * d * d).exp();
                for k in 0..4 { acc[k] += w * quats[j][k]; }
                lacc += w * logfov[j];
                wsum += w;
            }
            let len = acc.iter().map(|c| c * c).sum::<f64>().sqrt();
            (if len > 0.0 { acc.map(|c| c / len) } else { quats[i] }, lacc / wsum)
        };
        let (yaw, pitch, r) = head_angles(q);
        // As near the recorded angles as a whole turn allows: the recording's yaw is unwrapped, so is this
        let h = &raw[i];
        out.push(Sample { t, yaw: unwrap_near(yaw, h.yaw), pitch, roll: if tidy.level { 0.0 } else { unwrap_near(r, h.roll) },
                          fov: lf.exp(), projection });
    }
    // The yaw kept continuous along the stretch (unwrap_near follows the recording; this follows the result)
    for i in 1..out.len() {
        let prev = out[i - 1].yaw;
        out[i].yaw = unwrap_near(out[i].yaw, prev);
    }
    out
}

impl Recording {
    /// The recording tidied as its `tidy` says (not the picture hold, which renders: step 4), as a recording of its
    /// own: parts that touch as one, a part per stretch of one projection, the samples thinned. Nothing to tidy: itself
    pub fn tidied(&self) -> Recording {
        if self.tidy.none() { return self.clone(); }
        // Runs of parts that touch
        let mut runs: Vec<Vec<Sample>> = Vec::new();
        let mut last_to = f64::MIN;
        for p in &self.parts {
            if runs.is_empty() || p.from > last_to + TOUCH { runs.push(Vec::new()); }
            let run = runs.last_mut().unwrap();
            for s in &p.samples {
                if run.last().is_none_or(|l: &Sample| s.t > l.t) { run.push(*s); }
            }
            last_to = last_to.max(p.to);
        }
        let mut parts = Vec::new();
        for run in runs {
            // A stretch per projection: a change is a cut at the later sample
            let mut start = 0;
            for i in 1..=run.len() {
                if i == run.len() || run[i].projection != run[start].projection {
                    let stretch = &run[start..i];
                    if stretch.len() >= 2 {
                        let samples = thin(&tidy_stretch(stretch, &self.tidy), TOLERANCE);
                        parts.push(Part { from: samples[0].t, to: samples[samples.len() - 1].t, samples });
                    } else {
                        // One sample on its own (a projection held for a frame): kept as it is, held
                        parts.push(Part { from: stretch[0].t, to: stretch[0].t, samples: vec![stretch[0], stretch[0]] });
                    }
                    start = i;
                }
            }
        }
        Recording { parts, ..self.clone() }
    }

    /// As the player keeps it (`view360.recording`'s follows and parts), for `gyroflow-probe --reframe`
    pub fn to_json(&self) -> serde_json::Value {
        let r = |v: f64, k: i32| { let m = 10f64.powi(k); (v * m).round() / m };
        let parts: Vec<serde_json::Value> = self.parts.iter().map(|p| {
            let mut projection = None;
            let samples: Vec<serde_json::Value> = p.samples.iter().map(|s| {
                let mut row = vec![serde_json::json!(r(s.t, 4)), serde_json::json!(r(s.yaw, 3)), serde_json::json!(r(s.pitch, 3)),
                                   serde_json::json!(r(s.roll, 3)), serde_json::json!(r(s.fov, 3))];
                if projection != Some(s.projection) {
                    row.push(serde_json::json!(s.projection.name()));
                    projection = Some(s.projection);
                }
                serde_json::Value::Array(row)
            }).collect();
            serde_json::json!({ "from": r(p.from, 4), "to": r(p.to, 4), "samples": samples })
        }).collect();
        serde_json::json!({
            "follows": crate::export360::heading_name(self.follows),
            "parts": parts,
            "tidy": { "turns": self.tidy.turns.name(), "level": self.tidy.level, "picture": self.tidy.picture },
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rec() -> Recording {
        let v: serde_json::Value = serde_json::from_str(r#"{
            "follows": "travel", "frame": "9:16", "size": "4K", "tidy": { "turns": "medium" },
            "parts": [
                { "from": 30.001, "to": 47, "samples": [[30.001, 30.001, 0, 0, 100, "auto"], [47, 47, 0, 0, 100]] },
                { "from": 12, "to": 19.999, "samples": [[12, 12, 0, 0, 100, "auto"], [19.999, 19.999, 0, 0, 100]] },
                { "from": 20, "to": 30, "samples": [[20, -90, 10, 5, 60, "rectilinear"], [24, -90, 10, 5, 60], [24.04, 0, 0, 0, 270, "planet"],
                                                    [30, 40, 0, 0, 270], [29, 1, 1, 1, 100], ["x"], [31, 0, 0, 0, 0]] },
                { "samples": [[50, 0, 0, 0, 100]] }
            ]
        }"#).unwrap();
        Recording::from_json(&v).unwrap()
    }

    #[test]
    fn reads_the_recording() {
        let r = rec();
        assert_eq!(r.follows, Heading::Travel);
        assert_eq!((r.frame.as_deref(), r.size.as_deref()), (Some("9:16"), Some("4K")));
        // In time order; a part of one sample, and samples out of order or bad, left out
        assert_eq!(r.parts.iter().map(|p| p.from).collect::<Vec<_>>(), vec![12.0, 20.0, 30.001]);
        assert_eq!(r.parts[1].samples.len(), 4);
        // The projection kept from the sample that lists it
        assert_eq!(r.parts[1].samples[1].projection, Projection::Rectilinear);
        assert_eq!(r.parts[1].samples[3].projection, Projection::Planet);
        assert_eq!(r.span(), (12.0, 47.0));
        assert!(Recording::from_json(&serde_json::json!({ "follows": "sideways", "parts": [] })).is_err());
        assert!(Recording::from_json(&serde_json::json!({ "follows": "camera", "parts": [] })).is_err());
    }

    #[test]
    fn head_in_parts_and_held_outside() {
        let r = rec();
        let at = |t: f64| r.head_at(t);
        // Before the first part: its first sample
        assert_eq!(at(3.0).0.yaw, 12.0);
        // In a part: a straight line
        assert!((at(15.5).0.yaw - 15.5).abs() < 1e-9);
        // The punch-in: its own, from its first frame to its last
        assert_eq!(at(20.0).0.yaw, -90.0);
        assert_eq!((at(22.0).0.pitch, at(22.0).0.roll, at(22.0).0.fov), (10.0, 5.0, 60.0));
        // After it, what was there
        assert!((at(35.0).0.yaw - 35.0).abs() < 1e-9);
        // After the last part: its end
        assert_eq!(at(90.0).0.yaw, 47.0);
        // Between parts: the end of the one before (19.999 to 20 here: a thousandth of a second)
        let gap = Recording { parts: vec![r.parts[0].clone(), r.parts[2].clone()], ..r.clone() };
        assert!((gap.head_at(25.0).0.yaw - 19.999).abs() < 1e-9);
    }

    #[test]
    fn projection_changes_are_cuts() {
        let r = rec();
        // Held up to the change, the new one from its sample on
        let (h, p) = r.head_at(24.02);
        assert_eq!((h.yaw, h.fov, p), (-90.0, 60.0, Projection::Rectilinear));
        let (h, p) = r.head_at(24.04);
        assert_eq!((h.fov, p), (270.0, Projection::Planet));
        // Within the planet part: interpolated, the FOV in its log
        let (h, p) = r.head_at(27.02);
        assert!((h.yaw - 20.0).abs() < 1e-9 && p == Projection::Planet);
    }

    #[test]
    fn fov_between_in_its_log() {
        let v = serde_json::json!({ "follows": "camera", "parts": [{ "samples": [[0, 0, 0, 0, 50], [2, 0, 0, 0, 200]] }] });
        let r = Recording::from_json(&v).unwrap();
        assert!((r.head_at(1.0).0.fov - 100.0).abs() < 1e-9);
    }

    #[test]
    fn sidecar_beside_the_clip() {
        assert_eq!(sidecar_path(Path::new("/a.b/VID_1_00_005.insv")), PathBuf::from("/a.b/VID_1_00_005.afp.json"));
        assert_eq!(sidecar_path(Path::new("clip")), PathBuf::from("clip.afp.json"));
    }

    fn path(from: f64, to: f64, hz: f64, f: impl Fn(f64) -> (f64, f64, f64, f64)) -> Vec<serde_json::Value> {
        let n = ((to - from) * hz).round() as usize;
        (0..=n).map(|i| { let t = from + i as f64 / hz; let (y, p, r, v) = f(t); serde_json::json!([t, y, p, r, v]) }).collect()
    }

    fn recording(parts: Vec<Vec<serde_json::Value>>, tidy: serde_json::Value) -> Recording {
        let parts: Vec<_> = parts.into_iter().map(|s| serde_json::json!({ "samples": s })).collect();
        Recording::from_json(&serde_json::json!({ "follows": "camera", "parts": parts, "tidy": tidy })).unwrap()
    }

    #[test]
    fn angles_round_trip_through_quaternions() {
        for &(y, p, r) in &[(0.0, 0.0, 0.0), (30.0, 20.0, -10.0), (-170.0, -60.0, 45.0), (120.0, 85.0, 0.0), (90.0, 0.0, 179.0)] {
            let (y2, p2, r2) = head_angles(head_quat(y, p, r));
            assert!((y - y2).abs() < 1e-9 && (p - p2).abs() < 1e-9 && (r - r2).abs() < 1e-9, "{y} {p} {r} → {y2} {p2} {r2}");
        }
    }

    #[test]
    fn tidy_defaults_and_off() {
        let r = recording(vec![path(0.0, 2.0, 60.0, |t| (t, 0.0, 3.0, 100.0))], serde_json::Value::Null);
        assert_eq!(r.tidy, Tidy { turns: Turns::Medium, level: true, picture: false });
        let off = recording(vec![path(0.0, 2.0, 60.0, |t| (t, 0.0, 3.0, 100.0))], serde_json::json!({ "turns": "off", "level": false }));
        assert!(off.tidy.none());
        assert_eq!(off.tidied(), off);
    }

    #[test]
    fn jittered_turn_comes_out_smooth() {
        // A steady turn of 20°/s with a 1° shake at 8 Hz, 10 s at 60 samples a second
        let jitter = |t: f64| (t * 8.0 * std::f64::consts::TAU).sin();
        let raw = recording(vec![path(0.0, 10.0, 60.0, |t| (20.0 * t + jitter(t), 5.0 + 0.5 * jitter(t + 0.3), 0.0, 90.0))],
                            serde_json::json!({ "turns": "medium" }));
        let r = raw.tidied();
        assert_eq!(r.parts.len(), 1);
        // Away from the ends the shake is gone
        let mut worst: f64 = 0.0;
        for i in 0..=200 {
            let t = 3.0 + i as f64 * 0.02;
            let h = r.head_at(t).0;
            worst = worst.max((h.yaw - 20.0 * t).abs()).max((h.pitch - 5.0).abs());
        }
        assert!(worst < 0.1, "worst {worst}°");
        // The ends stay where they were recorded
        let (a, b) = (r.head_at(0.0).0, r.head_at(10.0).0);
        assert!((a.yaw - raw.head_at(0.0).0.yaw).abs() < 1e-6 && (b.yaw - raw.head_at(10.0).0.yaw).abs() < 1e-6);
        // Thinned: far fewer samples than the 601 recorded
        assert!(r.parts[0].samples.len() < 300, "{} samples", r.parts[0].samples.len());
        // Stronger smooths more: a slow 2° sway (1 Hz) stays in part with Light, goes with Medium
        let sway = |turns: &str| recording(vec![path(0.0, 10.0, 60.0, |t| (20.0 * t + 2.0 * (t * std::f64::consts::TAU).sin(), 0.0, 0.0, 90.0))],
                                           serde_json::json!({ "turns": turns })).tidied();
        let left = |r: &Recording| (0..100).map(|i| { let t = 4.0 + i as f64 * 0.01; (r.head_at(t).0.yaw - 20.0 * t).abs() }).fold(0.0, f64::max);
        let (light, medium) = (left(&sway("light")), left(&sway("medium")));
        assert!(light > 0.4 && medium < 0.05, "light {light}°, medium {medium}°");
    }

    #[test]
    fn level_takes_out_the_roll() {
        let r = recording(vec![path(0.0, 4.0, 30.0, |t| (10.0 * t, 20.0, 15.0 * (t * 3.0).sin(), 100.0))],
                          serde_json::json!({ "turns": "off", "level": true })).tidied();
        for s in &r.parts[0].samples { assert_eq!(s.roll, 0.0); }
        // Leveling alone keeps the rest where it was
        let h = r.head_at(2.0).0;
        assert!((h.yaw - 20.0).abs() < 0.06 && (h.pitch - 20.0).abs() < 0.06, "{h:?}");
    }

    #[test]
    fn the_yaw_stays_continuous_past_180() {
        let r = recording(vec![path(0.0, 6.0, 60.0, |t| (150.0 + 20.0 * t, 0.0, 0.0, 100.0))], serde_json::json!({ "turns": "strong" })).tidied();
        for i in 0..120 {
            let (a, b) = (r.head_at(i as f64 * 0.05).0.yaw, r.head_at((i + 1) as f64 * 0.05).0.yaw);
            assert!((b - a).abs() < 2.0, "{a} → {b}");
        }
        assert!((r.head_at(3.0).0.yaw - 210.0).abs() < 0.2, "{}", r.head_at(3.0).0.yaw);
    }

    #[test]
    fn a_punch_in_is_blended_and_a_projection_change_is_a_cut() {
        // Two parts that touch (a punch-in): one run, so the step between them is smoothed
        let r = recording(vec![path(0.0, 9.999, 30.0, |_| (0.0, 0.0, 0.0, 100.0)), path(10.0, 20.0, 30.0, |_| (30.0, 0.0, 0.0, 100.0))],
                          serde_json::json!({ "turns": "medium" })).tidied();
        assert_eq!(r.parts.len(), 1);
        let mid = r.head_at(10.0).0.yaw;
        assert!(mid > 5.0 && mid < 25.0, "{mid}");
        assert!(r.head_at(9.0).0.yaw > 0.01 && r.head_at(11.0).0.yaw < 29.99);
        // Parts apart: each its own
        let apart = recording(vec![path(0.0, 5.0, 30.0, |_| (0.0, 0.0, 0.0, 100.0)), path(8.0, 12.0, 30.0, |_| (30.0, 0.0, 0.0, 100.0))],
                              serde_json::json!({ "turns": "medium" })).tidied();
        assert_eq!(apart.parts.len(), 2);
        assert!(apart.head_at(5.0).0.yaw.abs() < 1e-9 && (apart.head_at(8.0).0.yaw - 30.0).abs() < 1e-9);
        // A change of projection: a cut, each side smoothed on its own
        let v = serde_json::json!({ "follows": "world", "tidy": { "turns": "strong" }, "parts": [{ "samples": [
            [0, 0, 0, 0, 100, "auto"], [2, 10, 0, 0, 100], [2.5, 50, 0, 0, 270, "planet"], [5, 60, 0, 0, 270] ] }] });
        let r = Recording::from_json(&v).unwrap().tidied();
        assert_eq!(r.parts.len(), 2);
        assert_eq!(r.head_at(2.2).1, Projection::Auto);
        let (h, p) = r.head_at(2.5);
        assert_eq!(p, Projection::Planet);
        assert!((h.fov - 270.0).abs() < 1e-6 && (h.yaw - 50.0).abs() < 1e-6);
    }

    #[test]
    fn tidied_reads_back_as_the_player_keeps_it() {
        let r = recording(vec![path(0.0, 3.0, 60.0, |t| (30.0 * t, 0.0, 0.0, 80.0 + 10.0 * t))], serde_json::json!({ "turns": "light" })).tidied();
        let back = Recording::from_json(&r.to_json()).unwrap();
        assert_eq!(back.follows, r.follows);
        assert_eq!(back.tidy, r.tidy);
        for i in 0..30 {
            let t = i as f64 * 0.1;
            let (a, b) = (r.head_at(t).0, back.head_at(t).0);
            assert!((a.yaw - b.yaw).abs() < 0.01 && (a.fov - b.fov).abs() < 0.01);
        }
    }
}
