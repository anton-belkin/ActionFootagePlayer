// SPDX-License-Identifier: GPL-3.0-or-later
//
// A 360° clip's view recorded as you look around (docs/360-reframe.md), as the export reads it: the recording the player
// keeps in the clip's own file, `<clip>.afp.json`, under `view360.recording`, and the head at any time of the clip.
// The player's side is mpv/scripts/view360/recording.lua; the two hold and interpolate the same way:
// - in a part, a straight line between two samples (the FOV in its log), held across a change of projection, which
//   is a cut at the later sample;
// - before the first part its first sample, between two parts the end of the one before, after the last its end.
//
// Step 1 plays the samples as recorded. The tidying (smoothing, level, the picture hold) comes here in step 2.

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
        Ok(Recording { follows, parts, frame: text("frame"), size: text("size") })
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
}
