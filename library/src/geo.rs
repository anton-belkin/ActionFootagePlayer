// SPDX-License-Identifier: GPL-3.0-or-later
//
// Where clips were recorded (docs/geolocation.md): a location suggested from Apple Photos (the photos taken nearest in
// time, which have a place), set by hand, or the camera's own GPS; place names by reverse geocoding, cached on this
// computer so each place is looked up once.
//
// A location is an attribute of the ORIGINAL (Anton, 2026-11-10): it's written into the original's own file
// (`<clip>.afp.json`, the main one over the locations, tags.rs's clip_own), as
//   "location": { "lat", "lon", "alt"?, "source": "photos" | "manual", "place": { "site", "locality", "region",
//                 "country", "cc", "water" }, "photo_gap_s"?, "set": "<UTC time>" }
// never into clip previews, moments or photos made from it. The camera's own GPS isn't written: it's in the clip.

use serde_json::{ json, Value };
use std::collections::HashMap;
use std::path::{ Path, PathBuf };
use std::process::Command;
use std::sync::Mutex;
use std::time::{ Duration, Instant };

// ---------- Apple Photos, through afp-photos (macos/photos.swift) ----------

/// afp-photos: AFL_PHOTOS, else beside the Library's binary (Contents/MacOS)
pub fn photos_tool() -> Option<PathBuf> {
    if let Some(p) = std::env::var_os("AFL_PHOTOS") { return Some(PathBuf::from(p)); }
    if !cfg!(target_os = "macos") { return None; }
    let exe = std::env::current_exe().ok()?;
    let p = exe.parent()?.join("afp-photos");
    p.is_file().then_some(p)
}

/// No console window for a tool on Windows
#[allow(unused_mut)]
fn quiet(mut cmd: Command) -> Command {
    #[cfg(windows)]
    { use std::os::windows::process::CommandExt; cmd.creation_flags(0x0800_0000); }
    cmd
}

fn run_json(tool: &Path, args: &[String]) -> Result<Value, String> {
    let out = quiet(Command::new(tool)).args(args).output().map_err(|e| format!("afp-photos: {e}"))?;
    let text = String::from_utf8_lossy(&out.stdout);
    serde_json::from_str(text.trim()).map_err(|_| format!("afp-photos: {}", String::from_utf8_lossy(&out.stderr).trim()))
}

/// Access to Photos as it is now, without asking: "notDetermined", "denied", "authorized", … or "none" (not a Mac)
pub fn photos_status() -> String {
    let Some(t) = photos_tool() else { return "none".into() };
    run_json(&t, &["status".into()]).ok().and_then(|v| v["status"].as_str().map(str::to_string)).unwrap_or_else(|| "none".into())
}

/// Asks macOS (its prompt, the first time), then the status
pub fn photos_request() -> String {
    let Some(t) = photos_tool() else { return "none".into() };
    run_json(&t, &["request".into()]).ok().and_then(|v| v["status"].as_str().map(str::to_string)).unwrap_or_else(|| "none".into())
}

#[derive(Clone, Debug, PartialEq)]
pub struct Photo { pub t: f64, pub lat: f64, pub lon: f64, pub alt: Option<f64>, pub id: String }

/// The photos with a place taken within the windows (unix s), by time
pub fn photos_near(windows: &[(f64, f64)]) -> Result<Vec<Photo>, String> {
    let t = photos_tool().ok_or("Apple Photos is only on a Mac")?;
    let mut args = vec!["near".to_string()];
    for (a, b) in merge(windows.to_vec()) { args.push(format!("{a:.0}")); args.push(format!("{b:.0}")); }
    if args.len() == 1 { return Ok(Vec::new()); }
    let v = run_json(&t, &args)?;
    if let Some(e) = v["error"].as_str() { return Err(format!("Photos: {e} ({})", v["status"].as_str().unwrap_or(""))); }
    Ok(v["photos"].as_array().into_iter().flatten().filter_map(|p| Some(Photo {
        t: p["t"].as_f64()?, lat: p["lat"].as_f64()?, lon: p["lon"].as_f64()?, alt: p["alt"].as_f64(), id: p["id"].as_str().unwrap_or("").into(),
    })).collect())
}

pub fn photo_names(ids: &[String]) -> HashMap<String, String> {
    let Some(t) = photos_tool() else { return HashMap::new() };
    if ids.is_empty() { return HashMap::new(); }
    let mut args = vec!["names".to_string()];
    args.extend(ids.iter().cloned());
    run_json(&t, &args).ok().and_then(|v| v["names"].as_object().map(|o| o.iter().filter_map(|(k, v)| Some((k.clone(), v.as_str()?.to_string()))).collect()))
        .unwrap_or_default()
}

fn merge(mut w: Vec<(f64, f64)>) -> Vec<(f64, f64)> {
    w.sort_by(|a, b| a.0.total_cmp(&b.0));
    let mut out: Vec<(f64, f64)> = Vec::new();
    for (a, b) in w {
        match out.last_mut() { Some(l) if a <= l.1 => l.1 = l.1.max(b), _ => out.push((a, b)) }
    }
    out
}

// ---------- place names ----------

/// Places looked up on this computer, by the point rounded to 0.01° (~1 km): `<cache>/places.json`
pub struct Places { file: PathBuf, map: HashMap<String, Value>, last_ask: Option<Instant> }

pub type SharedPlaces = std::sync::Arc<Mutex<Places>>;

pub fn key(lat: f64, lon: f64) -> String { format!("{:.2},{:.2}", lat, lon) }

impl Places {
    pub fn load(file: PathBuf) -> Places {
        let map = std::fs::read(&file).ok().and_then(|b| serde_json::from_slice::<HashMap<String, Value>>(&b).ok()).unwrap_or_default();
        Places { file, map, last_ask: None }
    }
    fn save(&self) {
        if let Some(d) = self.file.parent() { let _ = std::fs::create_dir_all(d); }
        let tmp = self.file.with_extension("tmp");
        if std::fs::write(&tmp, serde_json::to_vec(&self.map).unwrap()).is_ok() { let _ = std::fs::rename(&tmp, &self.file); }
    }
    pub fn known(&self, lat: f64, lon: f64) -> Option<Value> { self.map.get(&key(lat, lon)).cloned() }
}

/// What a lookup gave, tidied: { site, locality, region, country, cc, water, tz, tz_offset_s } (empty: nothing there)
fn tidy_apple(p: &Value) -> Value {
    let s = |k: &str| p[k].as_str().filter(|x| !x.is_empty()).map(str::to_string);
    let mut o = serde_json::Map::new();
    let mut put = |k: &str, v: Option<String>| { if let Some(v) = v { o.insert(k.into(), json!(v)); } };
    put("site", s("site"));
    put("locality", s("locality").or_else(|| s("sublocality")).or_else(|| s("subregion")));
    put("region", s("region"));
    put("country", s("country"));
    put("cc", s("cc"));
    put("water", s("ocean").or_else(|| s("water")));
    put("tz", s("tz"));
    if let Some(off) = p["tz_offset_s"].as_i64() { o.insert("tz_offset_s".into(), json!(off)); }
    o.insert("by".into(), json!("apple"));
    Value::Object(o)
}

/// Nominatim's answer (OpenStreetMap), tidied as above
pub fn tidy_osm(p: &Value) -> Value {
    let a = &p["address"];
    let s = |k: &str| a[k].as_str().filter(|x| !x.is_empty()).map(str::to_string);
    let mut o = serde_json::Map::new();
    let mut put = |k: &str, v: Option<String>| { if let Some(v) = v { o.insert(k.into(), json!(v)); } };
    put("site", s("tourism").or_else(|| s("leisure")).or_else(|| s("natural")).or_else(|| s("reef")));
    put("locality", s("city").or_else(|| s("town")).or_else(|| s("village")).or_else(|| s("hamlet")).or_else(|| s("suburb")).or_else(|| s("municipality")).or_else(|| s("county")));
    put("region", s("state").or_else(|| s("region")).or_else(|| s("province")).or_else(|| s("county")));
    put("country", s("country"));
    put("cc", s("country_code").map(|c| c.to_uppercase()));
    put("water", s("water").or_else(|| s("bay")).or_else(|| s("strait")));
    o.insert("by".into(), json!("osm"));
    Value::Object(o)
}

/// A point's place: from the cache, else looked up (Apple's geocoder on a Mac, else OpenStreetMap's Nominatim, at most
/// one a second as its terms ask, with the app named) and kept. None: it couldn't be looked up now (offline, refused)
pub fn place(places: &SharedPlaces, lat: f64, lon: f64, at: Option<f64>) -> Option<Value> {
    if let Some(v) = places.lock().unwrap().known(lat, lon) { return Some(v); }
    // One lookup at a time, spaced
    static ONE: Mutex<()> = Mutex::new(());
    let _one = ONE.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(v) = places.lock().unwrap().known(lat, lon) { return Some(v); }
    let wait = places.lock().unwrap().last_ask.map(|t| Duration::from_millis(1200).saturating_sub(t.elapsed())).unwrap_or_default();
    std::thread::sleep(wait);
    let (rlat, rlon) = ((lat * 100.0).round() / 100.0, (lon * 100.0).round() / 100.0);
    let found = if let Some(tool) = photos_tool() {
        let mut args = vec!["place".to_string()];
        if let Some(t) = at { args.push("--at".into()); args.push(format!("{t:.0}")); }
        args.push(format!("{lat:.6}")); args.push(format!("{lon:.6}"));
        run_json(&tool, &args).ok().and_then(|v| v["places"].get(0).cloned()).and_then(|p| {
            if p.get("error").is_some() { None } else if p["none"].as_bool() == Some(true) { Some(json!({ "by": "apple" })) } else { Some(tidy_apple(&p)) }
        })
    } else {
        nominatim(lat, lon)
    };
    places.lock().unwrap().last_ask = Some(Instant::now());
    let v = found?;
    let mut p = places.lock().unwrap();
    p.map.insert(key(rlat, rlon), v.clone());
    p.map.insert(key(lat, lon), v.clone());
    p.save();
    Some(v)
}

fn nominatim(lat: f64, lon: f64) -> Option<Value> {
    let url = format!("https://nominatim.openstreetmap.org/reverse?format=jsonv2&lat={lat:.6}&lon={lon:.6}&zoom=14&addressdetails=1&accept-language=en");
    let out = quiet(Command::new("curl")).args(["-s", "-m", "20", "-A", concat!("ActionFootageLibrary/", env!("CARGO_PKG_VERSION"), " (+https://actionfootageplayer.ch)"), &url]).output().ok()?;
    if !out.status.success() { return None; }
    let v: Value = serde_json::from_slice(&out.stdout).ok()?;
    if v.get("error").is_some() { return Some(json!({ "by": "osm" })); }
    Some(tidy_osm(&v))
}

/// "Ras Muhammad National Park, South Sinai, Egypt"
pub fn place_text(p: &Value) -> String {
    let mut parts: Vec<&str> = Vec::new();
    for k in ["site", "locality", "region", "country"] {
        if let Some(s) = p[k].as_str() { if !parts.contains(&s) { parts.push(s); } }
    }
    if parts.is_empty() { if let Some(w) = p["water"].as_str() { parts.push(w); } }
    parts.join(", ")
}

// ---------- suggestions ----------

#[derive(Clone, Debug)]
pub struct ClipIn {
    pub id: String,
    pub path: String,
    pub folder: String,
    /// Start as the file says (unix s) and its length
    pub start: f64,
    pub dur: f64,
    /// geotag.rs's basis: gps, utc, local, assumed (unset ones aren't given)
    pub basis: String,
    pub why: String,
}

/// Clips in one folder whose recordings are within this of each other are one group (a dive, a walk)
pub const GAP_S: f64 = 60.0 * 60.0;
/// A photo further than this in time isn't suggested
pub const FAR_S: f64 = 24.0 * 3600.0;

pub fn groups(mut clips: Vec<ClipIn>) -> Vec<Vec<ClipIn>> {
    clips.sort_by(|a, b| a.folder.cmp(&b.folder).then(a.start.total_cmp(&b.start)));
    let mut out: Vec<Vec<ClipIn>> = Vec::new();
    for c in clips {
        let join = out.last().and_then(|g| g.last()).is_some_and(|l| l.folder == c.folder && c.start - (l.start + l.dur) <= GAP_S);
        if join { out.last_mut().unwrap().push(c); } else { out.push(vec![c]); }
    }
    out
}

/// The photo nearest in time to [start, start+dur] (0 inside it), and how far, s (negative: before the clip)
pub fn nearest<'a>(photos: &'a [Photo], start: f64, dur: f64) -> Option<(&'a Photo, f64)> {
    let end = start + dur.max(0.0);
    let gap = |p: &Photo| if p.t < start { p.t - start } else if p.t > end { p.t - end } else { 0.0 };
    // photos are in time order: the last before the end and the first after it are the candidates, and anything inside
    let i = photos.partition_point(|p| p.t < start);
    let mut best: Option<(&Photo, f64)> = None;
    for p in photos[i.saturating_sub(1)..].iter() {
        let g = gap(p);
        if best.is_none_or(|(_, b)| g.abs() < b.abs()) { best = Some((p, g)); }
        if p.t > end { break; }
    }
    best.filter(|(_, g)| g.abs() <= FAR_S)
}

pub fn km(a: (f64, f64), b: (f64, f64)) -> f64 {
    let (la1, la2) = (a.0.to_radians(), b.0.to_radians());
    let dla = la2 - la1;
    let dlo = (b.1 - a.1).to_radians();
    let h = (dla / 2.0).sin().powi(2) + la1.cos() * la2.cos() * (dlo / 2.0).sin().powi(2);
    6371.0 * 2.0 * h.sqrt().asin()
}

/// One group's suggestion, its clips shifted by `shift` s (the camera's clock taken as off by that much): each clip's
/// nearest photo, and the group's: the clip-photo pair closest in time
pub fn suggest_group(group: &[ClipIn], photos: &[Photo], shift: f64) -> Value {
    let mut clips = Vec::new();
    let mut best: Option<(&Photo, f64)> = None;
    for c in group {
        let found = nearest(photos, c.start + shift, c.dur);
        if let Some((p, g)) = found { if best.is_none_or(|(_, b)| g.abs() < b.abs()) { best = Some((p, g)); } }
        clips.push((c, found));
    }
    let Some((bp, bg)) = best else {
        return json!({ "clips": clips.iter().map(|(c, _)| json!({ "id": c.id, "path": c.path, "start": c.start + shift, "dur": c.dur })).collect::<Vec<_>>(), "none": true });
    };
    let worst = clips.iter().filter_map(|(_, f)| f.map(|(_, g)| g.abs())).fold(0.0, f64::max);
    let spread = clips.iter().filter_map(|(_, f)| f.map(|(p, _)| km((p.lat, p.lon), (bp.lat, bp.lon)))).fold(0.0, f64::max);
    json!({
        "lat": bp.lat, "lon": bp.lon, "alt": bp.alt, "photo": { "id": bp.id, "t": bp.t, "gap_s": bg },
        "worst_gap_s": worst, "spread_km": spread,
        "clips": clips.iter().map(|(c, f)| {
            let mut v = json!({ "id": c.id, "path": c.path, "start": c.start + shift, "dur": c.dur });
            if let Some((p, g)) = f { v["photo"] = json!({ "id": p.id, "t": p.t, "gap_s": g, "lat": p.lat, "lon": p.lon, "km": km((p.lat, p.lon), (bp.lat, bp.lon)) }); }
            v
        }).collect::<Vec<_>>(),
    })
}

/// Clips whose camera writes local time with no zone ("local"): the shift that makes them UTC, from the zone of the
/// place of the photos near them. `zone_at(photo)` gives a place's offset east of UTC, s. Two rounds: the nearest photo
/// to the naive time, then the nearest to the time it gives
pub fn local_shift(group: &[ClipIn], photos: &[Photo], zone_at: &mut dyn FnMut(&Photo) -> Option<f64>) -> Option<f64> {
    let first = group.first()?;
    let mut shift = 0.0;
    for _ in 0..2 {
        let (p, _) = nearest(photos, first.start + shift, first.dur)?;
        let off = zone_at(p)?;
        if (-off - shift).abs() < 1.0 { break; }
        shift = -off;
    }
    Some(shift)
}

// ---------- writing ----------

/// Writes (or with None, removes) the location in the clips' own files. `ids`: their paths in the project's tree
pub fn set_location(root: &Path, ids: &[String], loc: Option<&Value>) -> Vec<String> {
    let mut problems = Vec::new();
    for id in ids {
        let (own, clip) = match crate::tags::clip_own(root, id) { Ok(x) => x, Err(e) => { problems.push(e); continue } };
        let mut doc = crate::locations::read_json(&own).unwrap_or(json!({ "format": "actionfootageplayer-clip", "version": 1 }));
        if doc.get("format").and_then(Value::as_str).is_some_and(|f| f != "actionfootageplayer-clip" && f != "actionfootageplayer-marks") {
            problems.push(format!("{}: not the player's file, left alone", own.display())); continue;
        }
        doc["format"] = json!("actionfootageplayer-clip");
        if doc.get("clip").is_none() {
            let size = std::fs::metadata(&clip).map(|m| m.len()).unwrap_or(0);
            doc["clip"] = json!({ "file": clip.file_name().map(|n| n.to_string_lossy().into_owned()), "size": size });
        }
        match loc { Some(l) => doc["location"] = l.clone(), None => { doc.as_object_mut().unwrap().remove("location"); } }
        let r = if crate::tags::holds_nothing(&doc) {
            std::fs::remove_file(&own).or_else(|e| if own.exists() { Err(e) } else { Ok(()) }).map_err(|e| e.to_string())
        } else { crate::tags::write_json(&own, &doc) };
        if let Err(e) = r { problems.push(e); }
    }
    problems
}

/// A location as written: what's given, with its source and when
pub fn location_value(lat: f64, lon: f64, alt: Option<f64>, source: &str, place: Option<&Value>, photo_gap_s: Option<f64>, site: Option<&str>) -> Value {
    let mut v = json!({ "lat": (lat * 1e6).round() / 1e6, "lon": (lon * 1e6).round() / 1e6, "source": source, "set": now_iso() });
    if let Some(a) = alt { v["alt"] = json!((a * 10.0).round() / 10.0); }
    let mut p = place.cloned().unwrap_or(json!({}));
    if let Some(o) = p.as_object_mut() { o.remove("by"); o.remove("tz"); o.remove("tz_offset_s"); }
    if let Some(s) = site.map(str::trim).filter(|s| !s.is_empty()) { p["site"] = json!(s); }
    if p.as_object().is_some_and(|o| !o.is_empty()) { v["place"] = p; }
    if let Some(g) = photo_gap_s { v["photo_gap_s"] = json!(g.round()); }
    v
}

fn now_iso() -> String {
    let s = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0);
    let (days, rem) = (s.div_euclid(86400), s.rem_euclid(86400));
    // civil from days (Howard Hinnant)
    let z = days + 719468;
    let era = z.div_euclid(146097);
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + if m <= 2 { 1 } else { 0 };
    format!("{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}Z", rem / 3600, rem % 3600 / 60, rem % 60)
}

/// Parses "27.7333, 34.25", "27.7333 34.25", "27°44'N 34°15'E" (as maps copy them) into (lat, lon)
pub fn parse_point(s: &str) -> Option<(f64, f64)> {
    let s = s.trim();
    let dms = |part: &str| -> Option<f64> {
        let neg = part.contains('S') || part.contains('W') || part.trim_start().starts_with('-');
        let nums: Vec<f64> = part.split(|c: char| !(c.is_ascii_digit() || c == '.')).filter(|x| !x.is_empty()).filter_map(|x| x.parse().ok()).collect();
        let v = nums.first()? + nums.get(1).unwrap_or(&0.0) / 60.0 + nums.get(2).unwrap_or(&0.0) / 3600.0;
        Some(if neg { -v } else { v })
    };
    let (a, b) = if let Some((a, b)) = s.split_once(',') { (a, b) } else {
        let i = s.find(|c: char| c == 'N' || c == 'S').map(|i| i + 1).or_else(|| s.find(char::is_whitespace))?;
        (&s[..i], &s[i..])
    };
    let (lat, lon) = (dms(a)?, dms(b)?);
    (lat.abs() <= 90.0 && lon.abs() <= 180.0).then_some((lat, lon))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn clip(id: &str, folder: &str, start: f64, dur: f64) -> ClipIn {
        ClipIn { id: id.into(), path: id.into(), folder: folder.into(), start, dur, basis: "utc".into(), why: String::new() }
    }
    fn photo(t: f64, lat: f64, lon: f64) -> Photo { Photo { t, lat, lon, alt: None, id: format!("p{t}") } }

    #[test]
    fn grouped_by_folder_and_time() {
        let g = groups(vec![clip("a", "D1", 0.0, 60.0), clip("b", "D1", 600.0, 60.0), clip("c", "D1", 600.0 + 60.0 + GAP_S + 1.0, 10.0),
                            clip("d", "D2", 700.0, 10.0)]);
        let ids: Vec<Vec<&str>> = g.iter().map(|x| x.iter().map(|c| c.id.as_str()).collect()).collect();
        assert_eq!(ids, vec![vec!["a", "b"], vec!["c"], vec!["d"]]);
    }

    #[test]
    fn nearest_photo_in_time() {
        let ps = vec![photo(100.0, 1.0, 1.0), photo(1000.0, 2.0, 2.0), photo(5000.0, 3.0, 3.0)];
        // Inside the clip: no gap
        let (p, g) = nearest(&ps, 900.0, 200.0).unwrap();
        assert_eq!((p.lat, g), (2.0, 0.0));
        // Before it: negative
        let (p, g) = nearest(&ps, 1300.0, 10.0).unwrap();
        assert_eq!((p.lat, g), (2.0, -300.0));
        // After it
        let (p, g) = nearest(&ps, 4000.0, 900.0).unwrap();
        assert_eq!((p.lat, g), (3.0, 100.0));
        assert!(nearest(&ps, 5000.0 + FAR_S + 10.0, 1.0).is_none());
        assert!(nearest(&[], 0.0, 1.0).is_none());
    }

    #[test]
    fn a_group_and_its_clips() {
        let ps = vec![photo(0.0, 27.0, 34.0), photo(10_000.0, 27.5, 34.0)];
        let g = vec![clip("a", "D", 1200.0, 600.0), clip("b", "D", 2400.0, 600.0)];
        let s = suggest_group(&g, &ps, 0.0);
        assert_eq!((s["lat"].as_f64(), s["photo"]["gap_s"].as_f64()), (Some(27.0), Some(-1200.0)));
        assert_eq!(s["worst_gap_s"].as_f64(), Some(2400.0));
        // Shifted 50 min later, b is nearer the second photo: the group spreads over ~55 km
        let s = suggest_group(&g, &ps, 3000.0);
        assert!(s["spread_km"].as_f64().unwrap() > 50.0);
    }

    #[test]
    fn local_time_cameras_shifted_by_the_places_zone() {
        // A camera on Cairo's local time (UTC+3): 12:00 local is 09:00 UTC, where the photo is
        let ps = vec![photo(9.0 * 3600.0, 27.0, 34.0), photo(20.0 * 3600.0, 27.0, 34.0)];
        let g = vec![clip("a", "D", 12.0 * 3600.0, 60.0)];
        assert_eq!(local_shift(&g, &ps, &mut |_| Some(3.0 * 3600.0)), Some(-3.0 * 3600.0));
    }

    #[test]
    fn points_as_maps_copy_them() {
        assert_eq!(parse_point("27.7333, 34.25"), Some((27.7333, 34.25)));
        assert_eq!(parse_point("-8.5 115.2"), Some((-8.5, 115.2)));
        let (a, b) = parse_point("27°44'00\"N 34°15'00\"E").unwrap();
        assert!((a - 27.7333).abs() < 1e-3 && (b - 34.25).abs() < 1e-9);
        assert!(parse_point("hello").is_none());
    }

    #[test]
    fn places_read_as_names() {
        assert_eq!(place_text(&json!({ "site": "Ras Muhammad National Park", "region": "South Sinai", "country": "Egypt" })), "Ras Muhammad National Park, South Sinai, Egypt");
        assert_eq!(place_text(&json!({ "water": "Red Sea" })), "Red Sea");
        let osm = tidy_osm(&json!({ "address": { "town": "Dahab", "state": "South Sinai", "country": "Egypt", "country_code": "eg" } }));
        assert_eq!((osm["locality"].as_str(), osm["cc"].as_str()), (Some("Dahab"), Some("EG")));
    }

    // On a real project (AFL_GEO_PROJECT, AFL_GEO_IDS: clips' paths in its tree, comma-separated): a location written
    // into their own files, seen by the Footage view, then taken out again: cargo test real_project_write -- --ignored
    #[test]
    #[ignore]
    fn real_project_write() {
        let root = PathBuf::from(std::env::var("AFL_GEO_PROJECT").unwrap());
        let ids: Vec<String> = std::env::var("AFL_GEO_IDS").unwrap().split(',').map(str::to_string).collect();
        let loc = location_value(27.9158, 34.3299, None, "photos", Some(&json!({ "locality": "Sharm el-Sheikh", "region": "South Sinai", "country": "Egypt", "cc": "EG" })), Some(-1234.0), Some("Test site"));
        assert!(set_location(&root, &ids, Some(&loc)).is_empty());
        let view = crate::index::scan(&root, "footage", false);
        for id in &ids {
            let it = view["items"].as_array().unwrap().iter().find(|i| i["id"].as_str() == Some(id)).unwrap();
            println!("{id}: {}", it["own"]["location"]);
            assert_eq!(it["own"]["location"]["place"]["site"], json!("Test site"));
        }
        assert!(set_location(&root, &ids, None).is_empty());
    }

    #[test]
    fn written_in_the_originals_own_file() {
        let top = std::env::temp_dir().join(format!("afl-geo-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&top);
        let (p, a) = (top.join("P"), top.join("Archive"));
        std::fs::create_dir_all(&p).unwrap();
        std::fs::create_dir_all(a.join("Day 1")).unwrap();
        std::fs::write(p.join(crate::locations::PROJECT_FILE), r#"{"locations":[{"at":"","path":"../Archive"}]}"#).unwrap();
        std::fs::write(a.join("Day 1/A.MP4"), b"x").unwrap();
        std::fs::write(a.join("Day 1/A.afp.json"), r#"{"format":"actionfootageplayer-clip","version":1,"marks":[{"t":1}]}"#).unwrap();
        std::fs::write(a.join("Day 1/B.MP4"), b"y").unwrap();
        let loc = location_value(27.73331234567, 34.25, Some(-12.34), "photos", Some(&json!({ "country": "Egypt", "by": "apple", "tz": "Africa/Cairo" })), Some(-240.4), Some(" Shark Reef "));
        assert!(set_location(&p, &["Day 1/A.MP4".into(), "Day 1/B.MP4".into()], Some(&loc)).is_empty());
        let a_doc = crate::locations::read_json(&a.join("Day 1/A.afp.json")).unwrap();
        assert_eq!(a_doc["marks"], json!([{ "t": 1 }]));
        assert_eq!(a_doc["location"]["lat"], json!(27.733312));
        assert_eq!(a_doc["location"]["place"], json!({ "country": "Egypt", "site": "Shark Reef" }));
        assert_eq!(a_doc["location"]["photo_gap_s"], json!(-240.0));
        assert!(a.join("Day 1/B.afp.json").is_file());
        // Taken away: A keeps its marks, B's file (holding nothing else) goes
        assert!(set_location(&p, &["Day 1/A.MP4".into(), "Day 1/B.MP4".into()], None).is_empty());
        assert!(crate::locations::read_json(&a.join("Day 1/A.afp.json")).unwrap().get("location").is_none());
        assert!(!a.join("Day 1/B.afp.json").exists());
        let _ = std::fs::remove_dir_all(&top);
    }
}

// ---------- the page's commands ----------

use crate::index::SharedProbes;
use tauri::State;

/// A clip's probe with its clock: what the Library knows, else the tool asked now (an answer from before geotag.rs
/// has no "clock")
fn probe_now(probes: &SharedProbes, path: &str) -> Option<Value> {
    if let Some(v) = probes.lock().unwrap().get(path).filter(|v| v.get("clock").is_some()) { return Some(v); }
    let tool = probes.lock().unwrap().tool.clone()?;
    let out = quiet(Command::new(tool)).arg("--library").arg(path).stdin(std::process::Stdio::null()).stderr(std::process::Stdio::null()).output().ok()?;
    let v: Value = String::from_utf8_lossy(&out.stdout).lines().last().and_then(|l| serde_json::from_str(l).ok())?;
    probes.lock().unwrap().put(path, v.clone());
    Some(v)
}

/// Whether the Library may read Photos: "authorized", "notDetermined", "denied", … ("none": no Photos here). `ask`:
/// macOS's prompt is shown if it hasn't been answered
#[tauri::command]
pub async fn photos_access(ask: bool) -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || if ask { photos_request() } else { photos_status() }).await.map_err(|e| e.to_string())
}

/// Suggestions for `clips` ([{ id, path, folder }]) from the photos nearest in time, grouped by folder and runs of
/// clips close in time. `shifts`: a group's camera clock taken as off by so many s (by the group's key)
#[tauri::command]
pub async fn suggest_locations(probes: State<'_, SharedProbes>, places: State<'_, SharedPlaces>, clips: Vec<Value>,
                               shifts: Option<HashMap<String, f64>>) -> Result<Value, String> {
    let (probes, places) = (probes.inner().clone(), places.inner().clone());
    tauri::async_runtime::spawn_blocking(move || suggest(&probes, &places, &clips, &shifts.unwrap_or_default())).await.map_err(|e| e.to_string())?
}

fn suggest(probes: &SharedProbes, places: &SharedPlaces, clips: &[Value], shifts: &HashMap<String, f64>) -> Result<Value, String> {
    let status = photos_status();
    if status != "authorized" && status != "limited" { return Ok(json!({ "status": status })); }
    // What each clip is, 4 at a time (from the share, each read is a wait)
    let next = std::sync::atomic::AtomicUsize::new(0);
    let answers: Mutex<Vec<(usize, Value)>> = Mutex::new(Vec::new());
    std::thread::scope(|s| for _ in 0..4 { s.spawn(|| loop {
        let i = next.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let Some(c) = clips.get(i) else { break };
        if let Some(v) = probe_now(probes, c["path"].as_str().unwrap_or("")) { answers.lock().unwrap().push((i, v)); }
    }); });
    let answers: HashMap<usize, Value> = answers.into_inner().unwrap().into_iter().collect();
    let (mut ins, mut own_gps, mut unset) = (Vec::new(), Vec::new(), Vec::new());
    for (i, c) in clips.iter().enumerate() {
        let id = c["id"].as_str().unwrap_or("").to_string();
        let Some(p) = answers.get(&i) else { unset.push(json!({ "id": id, "why": "couldn't be read" })); continue };
        if let (Some(lat), Some(lon)) = (p["gps"]["lat"].as_f64(), p["gps"]["lon"].as_f64()) {
            own_gps.push(json!({ "id": id, "lat": lat, "lon": lon })); continue;
        }
        let clock = &p["clock"];
        let Some(start) = clock["utc"].as_f64() else { unset.push(json!({ "id": id, "why": clock["why"].as_str().unwrap_or("no recording time") })); continue };
        ins.push(ClipIn { id, path: c["path"].as_str().unwrap_or("").into(), folder: c["folder"].as_str().unwrap_or("").into(), start,
                          dur: p["movie"]["duration"].as_f64().unwrap_or(0.0), basis: clock["basis"].as_str().unwrap_or("").into(),
                          why: clock["why"].as_str().unwrap_or("").into() });
    }
    let gs = groups(ins);
    // The photos around them all: a day either side, and the hours a zone can be off
    let reach = FAR_S + 14.0 * 3600.0;
    let windows: Vec<(f64, f64)> = gs.iter().map(|g| (g[0].start - reach, g.iter().map(|c| c.start + c.dur).fold(f64::MIN, f64::max) + reach)).collect();
    let photos = photos_near(&windows)?;
    let mut out = Vec::new();
    for g in &gs {
        let key = format!("{}|{}", g[0].folder, g[0].id);
        let local = g.iter().all(|c| c.basis == "local");
        let default_shift = if local {
            local_shift(g, &photos, &mut |p| place(places, p.lat, p.lon, Some(p.t)).and_then(|v| v["tz_offset_s"].as_f64())).unwrap_or(0.0)
        } else { 0.0 };
        let shift = shifts.get(&key).copied().unwrap_or(default_shift);
        let mut s = suggest_group(g, &photos, shift);
        if let (Some(lat), Some(lon)) = (s["lat"].as_f64(), s["lon"].as_f64()) {
            let pl = place(places, lat, lon, s["photo"]["t"].as_f64());
            s["place_text"] = json!(pl.as_ref().map(place_text).unwrap_or_default());
            s["place"] = pl.unwrap_or(Value::Null);
        }
        s["key"] = json!(key);
        s["folder"] = json!(g[0].folder);
        s["basis"] = json!(g[0].basis);
        s["why"] = json!(g[0].why);
        s["local"] = json!(local);
        s["shift"] = json!(shift);
        s["default_shift"] = json!(default_shift);
        out.push(s);
    }
    // The photos' own names, for the ones shown
    let mut ids: Vec<String> = out.iter().filter_map(|s| s["photo"]["id"].as_str().map(str::to_string)).collect();
    for s in &out { for c in s["clips"].as_array().into_iter().flatten() { if let Some(i) = c["photo"]["id"].as_str() { ids.push(i.into()); } } }
    ids.sort(); ids.dedup();
    let names = photo_names(&ids);
    let name = |v: &mut Value| { if let Some(n) = v["photo"]["id"].as_str().and_then(|i| names.get(i)) { v["photo"]["name"] = json!(n); } };
    for s in out.iter_mut() {
        name(s);
        for c in s["clips"].as_array_mut().into_iter().flatten() { name(c); }
    }
    Ok(json!({ "status": status, "groups": out, "own_gps": own_gps, "unset": unset, "photos": photos.len() }))
}

/// Writes locations: [{ ids, lat, lon, alt?, source, place?, photo_gap_s?, site? }]; a missing place is looked up
#[tauri::command]
pub async fn set_locations(shared: State<'_, crate::Shared>, places: State<'_, SharedPlaces>, entries: Vec<Value>) -> Result<Vec<String>, String> {
    let root = shared.lock().unwrap().project.clone().ok_or("Open a project first")?;
    let places = places.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let mut problems = Vec::new();
        for e in &entries {
            let ids: Vec<String> = e["ids"].as_array().into_iter().flatten().filter_map(|v| v.as_str().map(str::to_string)).collect();
            if e["remove"].as_bool() == Some(true) { problems.extend(set_location(&root, &ids, None)); continue; }
            let (Some(lat), Some(lon)) = (e["lat"].as_f64(), e["lon"].as_f64()) else { problems.push("no point given".into()); continue };
            let pl = e.get("place").filter(|p| p.is_object()).cloned().or_else(|| place(&places, lat, lon, None));
            let loc = location_value(lat, lon, e["alt"].as_f64(), e["source"].as_str().unwrap_or("manual"), pl.as_ref(), e["photo_gap_s"].as_f64(), e["site"].as_str());
            problems.extend(set_location(&root, &ids, Some(&loc)));
        }
        problems
    }).await.map_err(|e| e.to_string())
}

/// A location typed or pasted: "27.7333, 34.25" (as maps copy it). Its place, looked up, to show before it's set
#[tauri::command]
pub async fn parse_location(places: State<'_, SharedPlaces>, text: String) -> Result<Value, String> {
    let places = places.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let (lat, lon) = parse_point(&text).ok_or("Give it as latitude, longitude: 27.7333, 34.25")?;
        let pl = place(&places, lat, lon, None);
        Ok(json!({ "lat": lat, "lon": lon, "place": pl.clone().unwrap_or(Value::Null), "place_text": pl.as_ref().map(place_text).unwrap_or_default() }))
    }).await.map_err(|e| e.to_string())?
}

/// Places of points [[lat, lon], …]: the ones known at once; with `lookup`, the others looked up (a second or so each)
#[tauri::command]
pub async fn places_of(places: State<'_, SharedPlaces>, points: Vec<Vec<f64>>, lookup: bool) -> Result<Vec<Value>, String> {
    let places = places.inner().clone();
    tauri::async_runtime::spawn_blocking(move || points.iter().map(|p| {
        let (lat, lon) = (p.first().copied().unwrap_or(0.0), p.get(1).copied().unwrap_or(0.0));
        let v = if lookup { place(&places, lat, lon, None) } else { places.lock().unwrap().known(lat, lon) };
        v.unwrap_or(Value::Null)
    }).collect()).await.map_err(|e| e.to_string())
}

/// The point on a map in the browser: Apple Maps on a Mac, OpenStreetMap elsewhere
#[tauri::command]
pub fn open_map(lat: f64, lon: f64) -> Result<(), String> {
    let url = if cfg!(target_os = "macos") { format!("https://maps.apple.com/?ll={lat:.6},{lon:.6}&q={lat:.6},{lon:.6}&t=h") }
              else { format!("https://www.openstreetmap.org/?mlat={lat:.6}&mlon={lon:.6}#map=13/{lat:.6}/{lon:.6}") };
    #[cfg(target_os = "macos")] let r = Command::new("open").arg(&url).spawn();
    #[cfg(target_os = "windows")] let r = Command::new("explorer").arg(&url).spawn();
    #[cfg(all(not(target_os = "macos"), not(target_os = "windows")))] let r = Command::new("xdg-open").arg(&url).spawn();
    r.map(|_| ()).map_err(|e| e.to_string())
}

/// For checking on a real project without clicking (AFL_GEO_DUMP=<file>): once the project is open, asks for Photos
/// (macOS's prompt, from this app), suggests for all its clips and writes the answer to the file
pub fn dump_for_test(shared: crate::Shared, probes: SharedProbes, places: SharedPlaces, out: PathBuf) {
    std::thread::spawn(move || {
        let root = loop {
            if let Some(r) = shared.lock().unwrap().project.clone() { break r; }
            std::thread::sleep(Duration::from_millis(300));
        };
        let view = crate::index::scan(&root, "footage", false);
        let clips: Vec<Value> = view["items"].as_array().into_iter().flatten()
            .map(|it| json!({ "id": it["id"], "path": it["path"], "folder": it["dir"] })).collect();
        let status = photos_request();
        let started = Instant::now();
        let r = suggest(&probes, &places, &clips, &HashMap::new()).unwrap_or_else(|e| json!({ "error": e }));
        let _ = std::fs::write(&out, serde_json::to_vec_pretty(&json!({ "status": status, "clips": clips.len(), "seconds": started.elapsed().as_secs_f64(), "result": r })).unwrap());
    });
}
