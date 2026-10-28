// SPDX-License-Identifier: GPL-3.0-or-later
//
// The lens under water or not. Water bends the light where it enters the lens, which then behaves like a longer one:
// Gyroflow corrects for it with a refraction coefficient (Lens profile → Advanced → "Lens is underwater", 1.33).
// Without it under water the rotation is corrected by the wrong amount, and with it in air too.
//
// Auto follows the camera over the clip: its In water / Out of water, as the dji-telemetry mpv plugin shows it (`water`
// in mpv/scripts/dji-telemetry/main.lua): the barometer's in-water flag, or the camera's own underwater confidence,
// which comes from the picture and so also works in a dive housing. Keep the two in step.

use std::sync::{ Arc, atomic::{ AtomicBool, Ordering } };
use gyroflow_core::{ StabilizationManager, telemetry_parser, keyframes::{ Easing, KeyframeType } };

use crate::{ prefetch::Prefetched, probe::{ dji_schema, pb_bytes, pb_varint } };

/// Under water, not, or as the camera says (where it says; elsewhere as the project or preset says)
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Underwater {
    Yes,
    No,
    /// The camera's In water / Out of water over the clip (`spans`), where it records it; elsewhere the project's or
    /// preset's. A project that keyframes the refraction itself keeps its keyframes
    Auto,
}

impl Underwater {
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "yes" => Some(Self::Yes),
            "no" => Some(Self::No),
            "auto" => Some(Self::Auto),
            _ => None,
        }
    }
}

/// Water's, as Gyroflow's "Lens is underwater" sets it
const WATER_REFRACTION: f64 = 1.33;

/// The models that record it, and where in their FrameMeta's camera part (3.2 of ProductMeta; the dji-telemetry
/// plugin's LAYOUTS has the same paths): the in-water flag, the underwater confidence
const MODELS: &[(&str, [u64; 2], [u64; 2])] = &[
    // Osmo Action 6: Barometer { is_in_water = 1 } at 3.2.13, the confidence (0-256) at 3.2.8.1
    ("dvtm_ac206.proto", [13, 1], [8, 1]),
];
/// The confidence it's in the water from (the plugin's UNDERWATER_CONFIDENCE): it crosses this about when the
/// barometer's flag changes, before or after it
const UNDERWATER_CONFIDENCE: u64 = 128;

/// A stretch of the clip in or out of the water: the times of its first and last sample (ms, the video's)
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Span { pub start_ms: f64, pub end_ms: f64, pub in_water: bool }

/// Whether the camera records it: the model the clip's first metadata sample names. Only that sample is read
pub fn recorded(path: &str) -> bool {
    let Ok(mut file) = std::fs::File::open(path) else { return false };
    let Ok(size) = file.metadata().map(|m| m.len() as usize) else { return false };
    let mut found = false;
    let cancel = Arc::new(AtomicBool::new(false));
    let cancel2 = cancel.clone();
    let _ = telemetry_parser::util::get_metadata_track_samples(&mut file, size, true, |_info, data: &[u8], _pos, _md| {
        if cancel2.swap(true, Ordering::Relaxed) { return; }
        found = dji_schema(data).is_some_and(|p| MODELS.iter().any(|m| m.0 == p));
    }, cancel);
    found
}

/// The clip in stretches in and out of the water, in order; None when its camera doesn't record it
pub fn spans(path: &str) -> Option<Vec<Span>> {
    let (mut file, size) = Prefetched::open(path).ok()?;
    let mut layout = None;
    let mut samples = Vec::new();
    let cancel = Arc::new(AtomicBool::new(false));
    let cancel2 = cancel.clone();
    let _ = telemetry_parser::util::get_metadata_track_samples(&mut file, size, false, |info, data: &[u8], _pos, _md| {
        if cancel2.load(Ordering::Relaxed) { return; }
        // The model, from the first sample: another camera's are left unread
        if layout.is_none() {
            layout = dji_schema(data).and_then(|p| MODELS.iter().find(|m| m.0 == p));
            if layout.is_none() { return cancel2.store(true, Ordering::Relaxed); }
        }
        let &(_, flag, confidence) = layout.unwrap();
        if let Some(w) = in_water(data, flag, confidence) { samples.push((info.timestamp_ms, w)); }
    }, cancel);
    layout?;
    samples.sort_by(|a, b| a.0.total_cmp(&b.0));
    Some(spans_of(&samples))
}

/// A sample's In water: the flag, or the confidence. A field the message leaves out is 0 (proto3); a sample without
/// the camera part says nothing
fn in_water(sample: &[u8], flag: [u64; 2], confidence: [u64; 2]) -> Option<bool> {
    let camera = pb_bytes(sample, 3).and_then(|f| pb_bytes(f, 2))?;
    let value = |path: [u64; 2]| pb_bytes(camera, path[0]).and_then(|m| pb_varint(m, path[1])).unwrap_or(0);
    Some(value(flag) == 1 || value(confidence) >= UNDERWATER_CONFIDENCE)
}

fn spans_of(samples: &[(f64, bool)]) -> Vec<Span> {
    let mut out: Vec<Span> = Vec::new();
    for &(t, w) in samples {
        match out.last_mut() {
            Some(last) if last.in_water == w => last.end_ms = t,
            _ => out.push(Span { start_ms: t, end_ms: t, in_water: w }),
        }
    }
    out
}

/// Sets the refraction of the loaded project for `underwater`. `video`: the clip, where Auto reads the camera's In
/// water / Out of water. Before the smoothing and zoom are computed: the zoom depends on it
pub fn apply(stab: &StabilizationManager, underwater: Underwater, video: &str) {
    let typ = KeyframeType::LightRefractionCoeff;
    // A project's own refraction under water (salt water's 1.34, say) is the one it keeps
    let project = stab.params.read().light_refraction_coefficient;
    let water = if project > 1.0 { project } else { WATER_REFRACTION };
    let refraction = |w: bool| if w { water } else { 1.0 };
    let fixed = |v: f64| {
        stab.clear_keyframes_type(&typ);
        stab.set_light_refraction_coefficient(v);
    };
    match underwater {
        Underwater::Yes => fixed(water),
        Underwater::No => fixed(1.0),
        Underwater::Auto => {
            if stab.is_keyframed(&typ) {
                return log::info!("{video}: the project keyframes the lens's refraction, kept");
            }
            let t = std::time::Instant::now();
            let Some(spans) = spans(video) else { return };
            match spans.as_slice() {
                [] => return,
                [only] => fixed(refraction(only.in_water)),
                _ => {
                    // Each stretch at its value from its first sample to its last; in the one frame between two, from
                    // one to the other
                    for s in &spans {
                        for ms in [s.start_ms, s.end_ms] {
                            let us = (ms * 1000.0).round() as i64;
                            stab.set_keyframe(&typ, us, refraction(s.in_water));
                            stab.set_keyframe_easing(&typ, us, Easing::NoEasing);
                        }
                    }
                    stab.invalidate_zooming();
                }
            }
            let stretches: Vec<String> = spans.iter().map(|s| format!("{} {:.1}-{:.1} s", if s.in_water { "in" } else { "out" }, s.start_ms / 1000.0, s.end_ms / 1000.0)).collect();
            log::info!("{video}: lens under water as the camera says, read in {:.0} ms: {}", t.elapsed().as_secs_f64() * 1000.0, stretches.join(", "));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stretches() {
        let s = [(0.0, true), (16.7, true), (33.4, false), (50.1, false), (66.8, true)];
        assert_eq!(spans_of(&s), vec![
            Span { start_ms: 0.0, end_ms: 16.7, in_water: true },
            Span { start_ms: 33.4, end_ms: 50.1, in_water: false },
            Span { start_ms: 66.8, end_ms: 66.8, in_water: true },
        ]);
        assert!(spans_of(&[]).is_empty());
    }

    // FrameMeta { camera = 2 { underwater = 8 { confidence = 1 }, barometer = 13 { is_in_water = 1 } } } in
    // ProductMeta's field 3
    fn sample(flag: Option<u64>, confidence: Option<u64>) -> Vec<u8> {
        fn msg(field: u8, body: &[u8]) -> Vec<u8> { [vec![field << 3 | 2, body.len() as u8], body.to_vec()].concat() }
        let varint = |v: u64| vec![1 << 3, v as u8];
        let mut camera = Vec::new();
        if let Some(c) = confidence { camera.extend(msg(8, &[vec![1 << 3, (c as u8) | 0x80, (c >> 7) as u8]].concat())); }
        if let Some(f) = flag { camera.extend(msg(13, &varint(f))); }
        msg(3, &msg(2, &camera))
    }

    #[test]
    fn flag_or_confidence() {
        let (flag, confidence) = ([13, 1], [8, 1]);
        assert_eq!(in_water(&sample(Some(1), None), flag, confidence), Some(true));
        assert_eq!(in_water(&sample(Some(0), Some(256)), flag, confidence), Some(true));
        assert_eq!(in_water(&sample(None, Some(127)), flag, confidence), Some(false));
        assert_eq!(in_water(&sample(None, None), flag, confidence), Some(false));
        assert_eq!(in_water(&[], flag, confidence), None);
    }
}
