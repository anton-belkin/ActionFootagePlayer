// SPDX-License-Identifier: GPL-3.0-or-later
//
// What a 360 camera records besides its motion that the heading modes use (heading.rs; docs/360-cameras.md, "The
// heading modes"): GPS fixes, and a compass where the camera has one. As telemetry-parser reads them.
//
// Insta360 (X3, X4, X5): GPS comes from a GPS remote or the phone app while recording, into the file's trailer (record
// type 0x700, `GPS/Data`: `GpsData` per fix: unix time with ms, 'A'cquired or 'V'oid, lat, lon, speed (telemetry-parser
// gives km/h), course, altitude). A clip recorded without one has no such record, or fixes all void and at 0, 0 (the
// sample X5 clip). No Insta360 has a magnetometer: north there comes only from the GPS fit (heading.rs).
// The fixes are in unix time; the video's clock is placed on it by the clip's metadata (`ExtraMetadata`):
// `first_gps_timestamp` is unix time in ms at the video's start, despite its name: on the sample X5 clip (no GPS at all)
// 1744810587993, 13:36:27.993 UTC, and its `creation_time` "20250416143628" local time (BST, UTC+1), the file's name
// the same. A fix's video time is then its unix time less that. Its precision isn't known (no clip with GPS is at
// hand): the GPS fit (heading::align) measures what's left as its lag, within LAG_SEARCH_S, and reports it.
// Without it, a numeric `creation_time` in unix seconds or ms would do; the X5's is a local date and time, not used.

use std::sync::{ Arc, atomic::AtomicBool };
use nalgebra::Vector3;
use gyroflow_core::telemetry_parser::{ Input, InputOptions, tags_impl::* };

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GpsFix {
    /// The video's clock, ms
    pub t_ms: f64,
    /// Degrees
    pub lat: f64,
    pub lon: f64,
    /// Over ground, m/s
    pub speed: f64,
    /// Dilution of precision; 0 where the camera doesn't say
    pub dop: f64,
    /// 0 none, 2 2D, 3 3D (Insta360: 3 when acquired: it says no more)
    pub fix: u32,
}

#[derive(Clone, Debug, Default)]
pub struct Telemetry {
    pub gps: Vec<GpsFix>,
    /// GPS records in the file at all (some void), for saying why there's no track
    pub gps_records: usize,
    /// How the fixes were put on the video's clock
    pub gps_clock: &'static str,
    /// Magnetic north in the rig's axes and its confidence 0-1, per frame (a camera with a compass; none on Insta360)
    pub north: Vec<(f64, Vector3<f64>, f64)>,
    /// Whether the camera has a compass at all
    pub has_compass: bool,
}

fn number(v: &serde_json::Value, key: &str) -> Option<f64> {
    let x = v.get(key)?;
    x.as_f64().or_else(|| x.as_str()?.parse().ok())
}

impl Telemetry {
    pub fn read(path: &str) -> Option<Telemetry> {
        let mut file = std::fs::File::open(path).ok()?;
        let size = file.metadata().ok()?.len();
        let mut options = InputOptions::default();
        options.dont_look_for_sidecar_files = true;
        let input = Input::from_stream_with_options(&mut file, size, path, |_| (), Arc::new(AtomicBool::new(false)), options).ok()?;
        let mut out = Telemetry::default();
        let mut raw: Vec<GpsData> = Vec::new();
        let mut meta: Option<serde_json::Value> = None;
        for s in input.samples.as_deref().unwrap_or_default() {
            let Some(groups) = s.tag_map.as_ref() else { continue };
            if let Some(m) = groups.get(&GroupId::GPS) {
                if let Some(v) = m.get_t(TagId::Data) as Option<&Vec<GpsData>> { raw.extend(v.iter().cloned()); }
            }
            if let Some(m) = groups.get(&GroupId::Default) {
                if let Some(v) = m.get_t(TagId::Metadata) as Option<&serde_json::Value> { meta.get_or_insert_with(|| v.clone()); }
            }
            if groups.contains_key(&GroupId::Magnetometer) { out.has_compass = true; }
        }
        out.gps_records = raw.len();
        (out.gps, out.gps_clock) = place(&raw, &meta.unwrap_or_default());
        Some(out)
    }

    /// Why there's no GPS track, for the panel
    pub fn why_no_gps(&self) -> &'static str {
        if self.gps_records == 0 { "no GPS in this clip (record with a GPS remote or the phone app's GPS)" }
        else if self.gps.is_empty() { "the GPS had no fix while recording" }
        else { "too few GPS fixes in this clip" }
    }
}

/// Insta360's fixes on the video's clock (see the top): the acquired ones, in time order, and what placed them
pub fn place(raw: &[GpsData], meta: &serde_json::Value) -> (Vec<GpsFix>, &'static str) {
    let Some(unix0) = raw.iter().find(|f| f.is_acquired && f.unix_timestamp > 0.0).map(|f| f.unix_timestamp) else { return (Vec::new(), "") };
    // Unix seconds or ms from 2001 to 2286 (a local date and time as a number, 2.0e13, is neither)
    let unix = |v: f64| if (1e9..1e10).contains(&v) { Some(v) } else if (1e12..1e13).contains(&v) { Some(v / 1000.0) } else { None };
    let (start, clock) = match (number(meta, "first_gps_timestamp").and_then(unix), number(meta, "creation_time").and_then(unix)) {
        (Some(g), _) => (g, "first_gps_timestamp"),
        (_, Some(c)) => (c, "creation_time"),
        _ => return (Vec::new(), ""),
    };
    // A start more than a minute after the first fix or a day before it is a wrong clock, not a lag
    if !(-60.0..86400.0).contains(&(unix0 - start)) { return (Vec::new(), ""); }
    let mut fixes: Vec<GpsFix> = raw.iter().filter(|f| f.is_acquired && f.unix_timestamp > 0.0).map(|f| GpsFix {
        t_ms: (f.unix_timestamp - start) * 1000.0, lat: f.lat, lon: f.lon, speed: f.speed / 3.6, dop: 0.0, fix: 3,
    }).collect();
    fixes.sort_by(|a, b| a.t_ms.total_cmp(&b.t_ms));
    fixes.dedup_by(|a, b| a.t_ms == b.t_ms);
    (fixes, clock)
}

/// Fixes on a plane tangent at the first good one: east and north in metres, the fixes' times, speeds; only 2D or 3D
/// fixes with a DOP under `max_dop` (0: not given)
pub fn local_track(gps: &[GpsFix], max_dop: f64) -> Vec<(f64, [f64; 2], f64)> {
    const R: f64 = 6_371_000.0;
    let good: Vec<&GpsFix> = gps.iter().filter(|f| f.fix >= 2 && f.dop.is_finite() && f.dop < max_dop && f.lat.abs() <= 90.0 && !(f.lat == 0.0 && f.lon == 0.0)).collect();
    let Some(o) = good.first() else { return Vec::new() };
    let k = R * std::f64::consts::PI / 180.0;
    let c = o.lat.to_radians().cos();
    good.iter().map(|f| (f.t_ms, [(f.lon - o.lon) * k * c, (f.lat - o.lat) * k], f.speed)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_track_is_in_metres() {
        let fix = |t: f64, lat: f64, lon: f64| GpsFix { t_ms: t, lat, lon, speed: 1.0, dop: 1.0, fix: 3 };
        let tr = local_track(&[fix(0.0, 52.0, 5.0), fix(100.0, 52.0 + 1e-5, 5.0), fix(200.0, 52.0, 5.0 + 1e-5), fix(300.0, 0.0, 0.0)], 5.0);
        assert_eq!(tr.len(), 3);
        assert!((tr[1].1[1] - 1.112).abs() < 0.01 && tr[1].1[0].abs() < 1e-9);
        assert!((tr[2].1[0] - 1.112 * 52f64.to_radians().cos()).abs() < 0.01);
    }

    #[test]
    fn insta360_fixes_on_the_videos_clock() {
        let fix = |t: f64, ok: bool| GpsData { is_acquired: ok, unix_timestamp: t, lat: 46.2, lon: 6.1, speed: 36.0, track: 0.0, altitude: 0.0 };
        let raw = [fix(1744810590.5, true), fix(1744810589.5, true), fix(1744810591.5, false), fix(0.0, false)];
        // The sample X5's metadata: first_gps_timestamp unix ms, creation_time a local date
        let meta = serde_json::json!({ "first_gps_timestamp": "1744810587993", "creation_time": "20250416143628", "first_frame_timestamp": "2033409" });
        let (f, clock) = place(&raw, &meta);
        assert_eq!(clock, "first_gps_timestamp");
        assert_eq!(f.len(), 2);
        assert!((f[0].t_ms - 1507.0).abs() < 1e-3 && (f[1].t_ms - 2507.0).abs() < 1e-3, "{f:?}");
        assert!((f[0].speed - 10.0).abs() < 1e-12 && f[0].fix == 3);
        // Only a local date: no clock, no fixes
        assert!(place(&raw, &serde_json::json!({ "creation_time": "20250416143628" })).0.is_empty());
        // A unix creation time in seconds does
        assert_eq!(place(&raw, &serde_json::json!({ "creation_time": 1744810588 })).1, "creation_time");
        // A clock a week off: none
        assert!(place(&raw, &serde_json::json!({ "first_gps_timestamp": 1744810587993.0 - 7.0 * 86400e3 })).0.is_empty());
        // No fix acquired (the sample X5's record would be all void, at 0, 0)
        assert!(place(&[fix(0.0, false)], &meta).0.is_empty());
    }

    /// A clip's telemetry and the metadata that places it (`AFP_360_CLIP`)
    #[test]
    #[ignore]
    fn a_clips_telemetry() {
        let clip = std::env::var("AFP_360_CLIP").expect("AFP_360_CLIP");
        let t = Telemetry::read(&clip).unwrap();
        eprintln!("GPS records {}, fixes {}, clock {:?}, compass {}", t.gps_records, t.gps.len(), t.gps_clock, t.has_compass);
        for f in t.gps.iter().step_by(50) { eprintln!("GPS {f:?}"); }
        let mut file = std::fs::File::open(&clip).unwrap();
        let size = file.metadata().unwrap().len();
        let input = Input::from_stream_with_options(&mut file, size, &clip, |_| (), Arc::new(AtomicBool::new(false)), InputOptions::default()).unwrap();
        for s in input.samples.as_deref().unwrap_or_default() {
            let Some(g) = s.tag_map.as_ref() else { continue };
            if let Some(m) = g.get(&GroupId::Default).and_then(|m| m.get_t(TagId::Metadata) as Option<&serde_json::Value>) {
                for k in ["creation_time", "first_frame_timestamp", "first_gps_timestamp", "gps_sources", "gps", "total_time", "camera_type"] {
                    let v = m.get(k).map(|v| { let s = v.to_string(); s[..s.len().min(120)].to_string() });
                    eprintln!("META {k}: {v:?}");
                }
            }
            if let Some(v) = g.get(&GroupId::GPS).and_then(|m| m.get_t(TagId::Data) as Option<&Vec<GpsData>>) {
                for f in v.iter().take(3) { eprintln!("RAW {f:?}"); }
            }
        }
    }
}
