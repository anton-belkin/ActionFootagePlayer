// SPDX-License-Identifier: GPL-3.0-or-later
//
// `camera-telemetry <video>`: the telemetry a camera recorded alongside the video, read by telemetry-parser (the
// library Gyroflow reads camera metadata with), for the dji-telemetry mpv plugin. Prints one JSON line:
//   {"camera":"GoPro","model":"HERO6 Black","values":{"latitude_deg":{"t":[0.0,1.02,...],"v":[33.12651,false,...]},...}}
// Each series only where the value changes, false where it isn't recorded, times in seconds from the start of the
// clip; the names are telemetry.lua's where they mean the same (iso, exposure_time as [numerator, denominator],
// white_balance_k), and of their own otherwise (latitude_deg, longitude_deg: only with a fix; speed_ms).
// The plugin reads the DJI models it knows a layout for itself (telemetry.lua); what this adds is the cameras
// telemetry-parser knows. Exits with 1 and a message on stderr when the file has nothing it reads.
//
// What's read:
//   GoPro     GPS5 (HERO5-10) and GPS9 (HERO11 and later) with their fix, SHUT, ISOE, WBAL
//   any other the GPS telemetry-parser decodes (Vec_GpsData: Insta360's, DJI's)

use std::collections::BTreeMap;
use std::sync::{ Arc, atomic::AtomicBool };
use gyroflow_core::telemetry_parser::{ Input, tags_impl::* };
use serde_json::{ Value, json };

// A value over time, only where it changes
#[derive(Default)]
struct Series { t: Vec<f64>, v: Vec<Value> }

impl Series {
    fn push(&mut self, t: f64, v: Value) {
        if self.v.last() == Some(&v) { return; }
        self.t.push((t * 1000.0).round() / 1000.0);
        self.v.push(v);
    }
}

#[derive(Default)]
struct Values(BTreeMap<&'static str, Series>);

impl Values {
    fn push(&mut self, name: &'static str, t: f64, v: Value) {
        self.0.entry(name).or_default().push(t, v);
    }
    fn position(&mut self, t: f64, fix: Option<(f64, f64)>, speed_ms: Option<f64>) {
        let (lat, lon) = fix.map(|(a, b)| (json!(a), json!(b))).unwrap_or((json!(false), json!(false)));
        self.push("latitude_deg", t, lat);
        self.push("longitude_deg", t, lon);
        self.push("speed_ms", t, speed_ms.filter(|_| fix.is_some()).map(|s| json!((s * 100.0).round() / 100.0)).unwrap_or(json!(false)));
    }
}

fn numbers(v: &TagValue) -> Vec<f64> {
    fn flatten(v: &Value, out: &mut Vec<f64>) {
        match v {
            Value::Number(n) => out.extend(n.as_f64()),
            Value::Array(a) => a.iter().for_each(|x| flatten(x, out)),
            _ => {}
        }
    }
    let mut out = Vec::new();
    flatten(&serde_json::to_value(v).unwrap_or_default(), &mut out);
    out
}

// The rows of a tag's data ([[lat, lon, ...], ...]), each scaled by SCAL, as GPMF stores them
fn scaled_rows(map: &TagMap, data: &TagValue) -> Vec<Vec<f64>> {
    let scale = map.get(&TagId::Scale).map(|t| numbers(&t.value)).unwrap_or_default();
    let Ok(Value::Array(rows)) = serde_json::to_value(data) else { return Vec::new() };
    rows.iter().map(|row| {
        let mut r = Vec::new();
        if let Value::Array(a) = row { a.iter().for_each(|x| r.extend(x.as_f64())); }
        for (i, x) in r.iter_mut().enumerate() {
            let s = scale.get(i).or(scale.first()).copied().unwrap_or(1.0);
            if s != 0.0 { *x /= s; }
        }
        r
    }).collect()
}

// The times of n values spread over a sample, in seconds
fn times(s: &gyroflow_core::telemetry_parser::util::SampleInfo, n: usize) -> impl Iterator<Item = f64> + '_ {
    (0..n).map(move |i| (s.timestamp_ms + s.duration_ms * i as f64 / n as f64) / 1000.0)
}

fn gopro(input: &Input, values: &mut Values) {
    const GPS9: u32 = u32::from_be_bytes(*b"GPS9");
    const GPSF: u32 = u32::from_be_bytes(*b"GPSF");
    for s in input.samples.as_deref().unwrap_or_default() {
        let Some(m) = s.tag_map.as_ref() else { continue };
        // GPS5: lat, lon, alt, 2D speed, 3D speed; the fix (0 none, 2 2D, 3 3D) for the whole sample
        if let Some(g) = m.get(&GroupId::GPS) {
            if let Some(data) = g.get(&TagId::Data) {
                let fix = g.get(&TagId::Unknown(GPSF)).and_then(|t| numbers(&t.value).first().copied()).unwrap_or(0.0) >= 2.0;
                let rows = scaled_rows(g, &data.value);
                for (t, r) in times(s, rows.len()).zip(&rows) {
                    let pos = (fix && r.len() >= 4).then(|| (r[0], r[1]));
                    values.position(t, pos, r.get(3).copied());
                }
            }
        }
        // GPS9: lat, lon, alt, 2D speed, 3D speed, days, seconds, DOP, fix, per row
        if let Some(g) = m.get(&GroupId::Custom("GPS9".into())) {
            if let Some(data) = g.get(&TagId::Unknown(GPS9)) {
                let rows = scaled_rows(g, &data.value);
                for (t, r) in times(s, rows.len()).zip(&rows) {
                    let pos = (r.len() >= 9 && r[8] >= 2.0).then(|| (r[0], r[1]));
                    values.position(t, pos, r.get(3).copied());
                }
            }
        }
        // Per frame: the shutter in seconds, the sensor's ISO, the white balance in K
        if let Some(d) = m.get(&GroupId::Exposure).and_then(|g| g.get(&TagId::Data)) {
            let v = numbers(&d.value);
            for (t, x) in times(s, v.len()).zip(v) {
                values.push("exposure_time", t, if x > 0.0 { json!([1, (1.0 / x).round() as i64]) } else { json!(false) });
            }
        }
        for (group, name) in [("SensorISO", "iso"), ("WhiteBalanceTemperature", "white_balance_k")] {
            if let Some(d) = m.get(&GroupId::Custom(group.into())).and_then(|g| g.get(&TagId::Data)) {
                let v = numbers(&d.value);
                for (t, x) in times(s, v.len()).zip(v) {
                    values.push(name, t, json!(x.round() as i64));
                }
            }
        }
    }
}

fn gps(input: &Input, values: &mut Values) {
    for s in input.samples.as_deref().unwrap_or_default() {
        let Some(TagValue::Vec_GpsData(v)) = s.tag_map.as_ref().and_then(|m| m.get(&GroupId::GPS)?.get(&TagId::Data)).map(|t| &t.value) else { continue };
        let v = v.get();
        for (t, g) in times(s, v.len()).zip(v) {
            values.position(t, g.is_acquired.then_some((g.lat, g.lon)), Some(g.speed / 3.6));
        }
    }
}

fn read(path: &str) -> Result<Value, String> {
    let mut file = std::fs::File::open(path).map_err(|e| e.to_string())?;
    let size = file.metadata().map_err(|e| e.to_string())?.len() as usize;
    let input = Input::from_stream(&mut file, size, path, |_| (), Arc::new(AtomicBool::new(false))).map_err(|e| e.to_string())?;
    let camera = input.camera_type();
    let mut values = Values::default();
    match camera.as_str() {
        "GoPro" => gopro(&input, &mut values),
        _ => gps(&input, &mut values),
    }
    // A series that never had a value says nothing
    values.0.retain(|_, s| s.v.iter().any(|v| v != &json!(false)));
    if values.0.is_empty() {
        return Err(format!("{camera}: no telemetry this reads"));
    }
    let values: serde_json::Map<String, Value> = values.0.into_iter().map(|(k, s)| (k.to_string(), json!({ "t": s.t, "v": s.v }))).collect();
    Ok(json!({ "camera": camera, "model": input.camera_model(), "values": values }))
}

fn main() {
    let Some(path) = std::env::args().nth(1) else {
        eprintln!("usage: camera-telemetry <video>");
        std::process::exit(2);
    };
    match read(&path) {
        Ok(v) => println!("{v}"),
        Err(e) => { eprintln!("{e}"); std::process::exit(1); }
    }
}
