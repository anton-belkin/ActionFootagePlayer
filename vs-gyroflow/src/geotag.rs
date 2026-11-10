// SPDX-License-Identifier: GPL-3.0-or-later
//
// When a clip was recorded, as a moment in UTC, and where, if the camera itself knew: for Action Footage Library's
// locations (docs/geolocation.md). `gyroflow-probe --library` reports both as "clock" and "gps".
//
// What each camera writes as the MP4's own time (moov/mvhd creation_time), checked on Anton's clips 2026-11-10:
// - DJI (Osmo Action 6, Mini 4 Pro, Mavic): UTC. The file's name is the camera's local time: DJI_20261022152937_0069_D
//   has 13:29:38 inside (the camera on Swiss summer time, UTC+2); a drone in the Maldives, 16:38:14 in the name and
//   11:38:14 inside (UTC+5).
// - Insta360 (X5 sample): UTC, 13:36:28 inside, 14:36:28 in the name (BST); its first_gps_timestamp agrees to 7 ms.
// - GoPro HERO12 (and every GoPro that writes TZON, its time zone in minutes, into moov/udta/GPMF): UTC. HERO12 in
//   Egypt: mvhd 09:07:26, CDAT (its clock, local) 11:07:25, TZON 120: the camera's zone left on Swiss time, its UTC right.
// - GoPro without TZON (HERO7 and older): the camera's local time, with no zone. HERO7's clock is often never set
//   (2016-01-01); its GPS (GPSU) gives the real UTC.
// - Anything else: the MP4 standard says UTC; taken so, and said.
//
// The camera's own place: GoPro's GPS (GPS5 / GPS9 in the metadata track, the first fix with a 2D or 3D lock, its UTC
// from GPSU), Insta360's (nav.rs: from the GPS remote or the app), a DJI drone's .SRT beside the clip (its first block
// with a latitude and longitude). The Osmo Action 6's own GPS block (3.4.2) has had no fix in any clip seen.

use std::io::{ Read, Seek, SeekFrom };
use std::path::Path;
use serde::Serialize;

#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct Clock {
    /// The clip's start in unix seconds, UTC, as far as it's known (null: the camera's clock wasn't set)
    pub utc: Option<f64>,
    /// "gps" (the camera's GPS gave UTC), "utc" (the camera writes UTC), "local" (the camera's local time, its zone not
    /// known: the place's has to be assumed), "unset" (a clock never set), "assumed" (an unknown camera: UTC assumed)
    pub basis: &'static str,
    /// The camera's own time zone, minutes east of UTC, where it writes one (GoPro TZON)
    pub zone_min: Option<i32>,
    /// What was taken, for the Library to show
    pub why: String,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize)]
pub struct Fix {
    pub lat: f64,
    pub lon: f64,
    pub alt: Option<f64>,
    /// When, unix s UTC, where the camera says
    pub t: Option<f64>,
}

/// Before this a camera's clock wasn't set (cameras start at 2000, 2015 or 2016)
const SET_SINCE: f64 = 1_483_228_800.0; // 2017-01-01

fn be16(s: &[u8], i: usize) -> Option<u16> { s.get(i..i + 2).map(|b| u16::from_be_bytes([b[0], b[1]])) }
fn be32(s: &[u8], i: usize) -> Option<u32> { s.get(i..i + 4).map(|b| u32::from_be_bytes([b[0], b[1], b[2], b[3]])) }

/// A GoPro's GPMF items, depth first: (key, type, struct size, repeat, data)
fn klv(s: &[u8], out: &mut Vec<([u8; 4], u8, usize, usize, Vec<u8>)>) {
    let mut i = 0;
    while i + 8 <= s.len() {
        let key: [u8; 4] = s[i..i + 4].try_into().unwrap();
        if key == [0; 4] { break; }
        let (typ, size, rep) = (s[i + 4], s[i + 5] as usize, be16(s, i + 6).unwrap_or(0) as usize);
        let len = size * rep;
        let end = (i + 8 + len).min(s.len());
        if typ == 0 { klv(&s[i + 8..end], out); } else { out.push((key, typ, size, rep, s[i + 8..end].to_vec())); }
        i += 8 + len.div_ceil(4) * 4;
    }
}

/// moov/udta/GPMF of a GoPro clip (its settings), if it has one
fn gopro_udta(path: &Path) -> Option<Vec<u8>> {
    let mut f = std::fs::File::open(path).ok()?;
    let size = f.metadata().ok()?.len();
    let find = |f: &mut std::fs::File, from: u64, to: u64, want: &[u8; 4]| -> Option<(u64, u64)> {
        let mut at = from;
        while at + 8 <= to {
            let mut h = [0u8; 16];
            f.seek(SeekFrom::Start(at)).ok()?;
            f.read_exact(&mut h[..8]).ok()?;
            let mut len = be32(&h, 0)? as u64;
            let mut head = 8;
            if len == 1 { f.read_exact(&mut h[8..16]).ok()?; len = u64::from_be_bytes(h[8..16].try_into().ok()?); head = 16; }
            if len == 0 { len = to - at; }
            if len < head { return None; }
            if &h[4..8] == want { return Some((at + head, at + len)); }
            at += len;
        }
        None
    };
    let (ma, mb) = find(&mut f, 0, size, b"moov")?;
    let (ua, ub) = find(&mut f, ma, mb, b"udta")?;
    let (ga, gb) = find(&mut f, ua, ub, b"GPMF")?;
    if gb - ga > 1 << 20 { return None; }
    let mut buf = vec![0u8; (gb - ga) as usize];
    f.seek(SeekFrom::Start(ga)).ok()?;
    f.read_exact(&mut buf).ok()?;
    Some(buf)
}

/// GoPro's time zone (TZON, minutes), when its udta has one
pub fn gopro_zone(udta: &[u8]) -> Option<i32> {
    let mut items = Vec::new();
    klv(udta, &mut items);
    items.iter().find(|(k, ..)| k == b"TZON").and_then(|(_, t, _, _, d)| match t {
        b's' => be16(d, 0).map(|v| v as i16 as i32),
        b'l' => be32(d, 0).map(|v| v as i32),
        _ => None,
    })
}

/// "yymmddhhmmss.sss" (GPSU), UTC → unix s
fn gpsu(s: &[u8]) -> Option<f64> {
    let s = std::str::from_utf8(s).ok()?.trim_end_matches('\0');
    if s.len() < 12 { return None; }
    let n = |a: usize, b: usize| s.get(a..b)?.parse::<i64>().ok();
    let (y, mo, d, h, mi) = (2000 + n(0, 2)?, n(2, 4)?, n(4, 6)?, n(6, 8)?, n(8, 10)?);
    let sec: f64 = s.get(10..)?.parse().ok()?;
    Some(days_from_civil(y, mo, d)? as f64 * 86400.0 + (h * 3600 + mi * 60) as f64 + sec)
}

fn days_from_civil(y: i64, m: i64, d: i64) -> Option<i64> {
    if !(1..=12).contains(&m) || !(1..=31).contains(&d) { return None; }
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    Some(era * 146097 + doe - 719468)
}

/// The first GPS fix in one GPMF sample (GPS5 with GPSF ≥ 2, or GPS9 whose fix field is ≥ 2), with its UTC, and the
/// sample's UTC (GPSU) whatever the fix
fn gopro_fix(sample: &[u8]) -> (Option<Fix>, Option<f64>) {
    let mut items = Vec::new();
    klv(sample, &mut items);
    let (mut scal, mut fixq, mut utc): (Vec<f64>, Option<u32>, Option<f64>) = (Vec::new(), None, None);
    for (key, typ, size, rep, d) in &items {
        match key {
            b"SCAL" => {
                scal = match (typ, size) {
                    (b'l', 4) => (0..*rep).filter_map(|i| be32(d, i * 4)).map(|v| v as i32 as f64).collect(),
                    (b's', 2) => (0..*rep).filter_map(|i| be16(d, i * 2)).map(|v| v as i16 as f64).collect(),
                    (b'L', 4) => (0..*rep).filter_map(|i| be32(d, i * 4)).map(|v| v as f64).collect(),
                    (b'S', 2) => (0..*rep).filter_map(|i| be16(d, i * 2)).map(|v| v as f64).collect(),
                    _ => Vec::new(),
                };
            }
            b"GPSF" => fixq = be32(d, 0),
            b"GPSU" => utc = gpsu(d),
            b"GPS5" if *size == 20 && *rep > 0 && fixq.unwrap_or(0) >= 2 => {
                let s = |i: usize| scal.get(i).or(scal.first()).copied().filter(|v| *v != 0.0).unwrap_or(1.0);
                let v = |i: usize| be32(d, i * 4).map(|x| x as i32 as f64 / s(i));
                if let (Some(lat), Some(lon)) = (v(0), v(1)) {
                    if lat.abs() <= 90.0 && lon.abs() <= 180.0 && !(lat == 0.0 && lon == 0.0) {
                        return (Some(Fix { lat, lon, alt: v(2), t: utc }), utc);
                    }
                }
            }
            // GPS9 (HERO11 and later): lat, lon, alt, 2D speed, 3D speed, days since 2000, seconds of the day, DOP,
            // fix, as l l l l l l l S S (36 bytes)
            b"GPS9" if *size == 36 && *rep > 0 => {
                let s = |i: usize| scal.get(i).copied().filter(|v| *v != 0.0).unwrap_or(1.0);
                for r in 0..*rep {
                    let o = r * 36;
                    let l = |i: usize| be32(d, o + i * 4).map(|x| x as i32 as f64 / s(i));
                    let fix = be16(d, o + 34).map(|x| x as f64 / s(8)).unwrap_or(0.0);
                    if fix < 2.0 { continue; }
                    if let (Some(lat), Some(lon)) = (l(0), l(1)) {
                        if lat.abs() > 90.0 || (lat == 0.0 && lon == 0.0) { continue; }
                        let t = match (l(5), l(6)) { (Some(days), Some(secs)) => Some(946_684_800.0 + days * 86400.0 + secs), _ => utc };
                        return (Some(Fix { lat, lon, alt: l(2), t }), utc.or(t));
                    }
                }
            }
            _ => {}
        }
    }
    (None, utc)
}

/// A GoPro's first fix in its first `until` s, and (sample's UTC, sample's time in the clip) for the clock
fn gopro_gps(path: &Path, until: f64) -> (Option<(Fix, f64)>, Option<(f64, f64)>) {
    let Ok((mut file, _)) = crate::prefetch::Prefetched::open_until(path, Some(until)) else { return (None, None) };
    let samples: Vec<crate::prefetch::Sample> = file.samples().iter().copied().filter(|s| s.time <= until).collect();
    let mut first_utc = None;
    for s in samples {
        if s.end <= s.start || s.end - s.start > 1 << 20 { continue; }
        let mut buf = vec![0u8; (s.end - s.start) as usize];
        if file.seek(SeekFrom::Start(s.start)).is_err() || file.read_exact(&mut buf).is_err() { continue; }
        let (fix, utc) = gopro_fix(&buf);
        if first_utc.is_none() { if let Some(u) = utc { first_utc = Some((u, s.time)); } }
        if let Some(f) = fix { return (Some((f, s.time)), first_utc.or(utc.map(|u| (u, s.time)))); }
    }
    (None, first_utc)
}

/// A DJI drone's .SRT beside the clip: the first block with a place. Its lines carry "[latitude: 25.1] [longitude:
/// 55.2] [rel_alt: 1.2 abs_alt: 30.5]" (newer) or "GPS(55.2,25.1,30)" (older: longitude first)
pub fn srt_fix(text: &str) -> Option<Fix> {
    let num = |s: &str, key: &str| -> Option<f64> {
        let i = s.find(key)? + key.len();
        let rest = s[i..].trim_start_matches([':', ' ']);
        let end = rest.find(|c: char| !(c.is_ascii_digit() || c == '.' || c == '-')).unwrap_or(rest.len());
        rest[..end].parse().ok()
    };
    for line in text.lines() {
        if let (Some(lat), Some(lon)) = (num(line, "latitude"), num(line, "longitude")) {
            if lat.abs() <= 90.0 && lon.abs() <= 180.0 && !(lat == 0.0 && lon == 0.0) {
                return Some(Fix { lat, lon, alt: num(line, "abs_alt"), t: None });
            }
        }
        if let Some(i) = line.find("GPS(") {
            let inner = &line[i + 4..];
            let inner = &inner[..inner.find(')').unwrap_or(inner.len())];
            let v: Vec<f64> = inner.split(',').filter_map(|x| x.trim().parse().ok()).collect();
            if v.len() >= 2 && v[1].abs() <= 90.0 && !(v[0] == 0.0 && v[1] == 0.0) {
                return Some(Fix { lat: v[1], lon: v[0], alt: v.get(2).copied(), t: None });
            }
        }
    }
    None
}

fn srt_beside(path: &Path) -> Option<Fix> {
    for ext in ["SRT", "srt"] {
        if let Ok(b) = std::fs::read(path.with_extension(ext)) {
            if let Some(f) = srt_fix(&String::from_utf8_lossy(&b[..b.len().min(64 * 1024)])) { return Some(f); }
        }
    }
    None
}

/// The clip's clock and the camera's own place. `camera`: as probe::camera_and_lens names it ("GoPro HERO12 Black",
/// "DJI OsmoAction6", "Insta360 X5"); `created`: mvhd's creation time, unix s
pub fn read(path: &Path, camera: &str, created: Option<i64>) -> (Clock, Option<Fix>) {
    let brand = camera.split_whitespace().next().unwrap_or("").to_lowercase();
    let mvhd = created.map(|c| c as f64);
    let udta = gopro_udta(path);
    let is_gopro = brand == "gopro" || udta.is_some();
    let mut fix = None;
    let mut gps_clock: Option<f64> = None;
    if is_gopro {
        let (f, utc) = gopro_gps(path, 30.0);
        if let Some((f, at)) = f { fix = Some(f); if let Some(t) = f.t { gps_clock = Some(t - at); } }
        if gps_clock.is_none() { if let Some((u, at)) = utc { if u > SET_SINCE { gps_clock = Some(u - at); } } }
    } else if brand == "insta360" {
        if let Some(tel) = crate::nav::Telemetry::read(&path.to_string_lossy()) {
            if let (Some(g), Some(start)) = (tel.gps.first(), mvhd) {
                fix = Some(Fix { lat: g.lat, lon: g.lon, alt: None, t: Some(start + g.t_ms / 1000.0) });
            }
        }
    } else if brand == "dji" || path.file_name().is_some_and(|n| n.to_string_lossy().to_uppercase().starts_with("DJI_")) {
        fix = srt_beside(path);
    }
    let zone_min = udta.as_deref().and_then(gopro_zone);
    let clock = if let Some(u) = gps_clock {
        Clock { utc: Some(u), basis: "gps", zone_min, why: "the camera's GPS time".into() }
    } else {
        match mvhd {
            None => Clock { utc: None, basis: "unset", zone_min, why: "no recording time in the file".into() },
            Some(c) if c < SET_SINCE => Clock { utc: None, basis: "unset", zone_min, why: "the camera's clock wasn't set".into() },
            Some(c) if is_gopro && zone_min.is_some() => {
                let z = zone_min.unwrap();
                Clock { utc: Some(c), basis: "utc", zone_min, why: format!("GoPro writes UTC (its own zone UTC{})", zone_text(z)) }
            }
            Some(c) if is_gopro => Clock { utc: Some(c), basis: "local", zone_min, why: "an older GoPro writes its local time, with no zone".into() },
            Some(c) if brand == "dji" => Clock { utc: Some(c), basis: "utc", zone_min, why: "DJI writes UTC".into() },
            Some(c) if brand == "insta360" => Clock { utc: Some(c), basis: "utc", zone_min, why: "Insta360 writes UTC".into() },
            Some(c) => Clock { utc: Some(c), basis: "assumed", zone_min, why: "UTC assumed (as MP4 has it)".into() },
        }
    };
    (clock, fix)
}

pub fn zone_text(min: i32) -> String {
    let (sign, m) = if min < 0 { ('−', -min) } else { ('+', min) };
    if m % 60 == 0 { format!("{sign}{}", m / 60) } else { format!("{sign}{}:{:02}", m / 60, m % 60) }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(key: &[u8; 4], typ: u8, size: u8, rep: u16, data: &[u8]) -> Vec<u8> {
        let mut v = key.to_vec();
        v.push(typ); v.push(size); v.extend(rep.to_be_bytes()); v.extend(data);
        while v.len() % 4 != 0 { v.push(0); }
        v
    }

    #[test]
    fn gopro_zone_and_gps5() {
        let udta = [item(b"CDAT", b'J', 8, 1, &1_760_872_045u64.to_be_bytes()), item(b"TZON", b's', 2, 1, &120i16.to_be_bytes())].concat();
        assert_eq!(gopro_zone(&udta), Some(120));
        let mut gps = Vec::new();
        for v in [27_257_900i32, 338_116_000, 1_500, 100, 100] { gps.extend(v.to_be_bytes()); }
        let mut scal = Vec::new();
        for v in [1_000_000i32, 10_000_000, 1_000, 1_000, 100] { scal.extend(v.to_be_bytes()); }
        let strm = [item(b"GPSF", b'L', 4, 1, &3u32.to_be_bytes()), item(b"GPSU", b'U', 16, 1, b"251019090726.500"),
                    item(b"SCAL", b'l', 4, 5, &scal), item(b"GPS5", b'l', 20, 1, &gps)].concat();
        let stream = item(b"STRM", 0, 1, strm.len() as u16, &strm);
        let sample = item(b"DEVC", 0, 1, stream.len() as u16, &stream);
        let (f, utc) = gopro_fix(&sample);
        let f = f.unwrap();
        assert!((f.lat - 27.2579).abs() < 1e-9 && (f.lon - 33.8116).abs() < 1e-9 && f.alt == Some(1.5));
        // 2025-10-19 09:07:26.5 UTC
        assert_eq!(utc, Some(1_760_864_846.5));
    }

    #[test]
    fn drone_srt() {
        let new = "1\n00:00:00,000 --> 00:00:00,033\n<font size=\"28\">FrameCnt: 1, DiffTime: 33ms\n2024-06-15 07:27:49.123\n[iso: 100] [shutter: 1/500.0] [fnum: 1.7] [ev: 0] [latitude: 4.175930] [longitude: 73.509120] [rel_alt: 1.200 abs_alt: 30.522] </font>";
        let f = srt_fix(new).unwrap();
        assert!((f.lat - 4.17593).abs() < 1e-9 && (f.lon - 73.50912).abs() < 1e-9 && f.alt == Some(30.522));
        let old = "1\n00:00:00,000 --> 00:00:01,000\nHOME(73.5,4.1) 2017.10.01 12:00:00\nGPS(73.5091,4.1759,20) BAROMETER:30.5";
        let f = srt_fix(old).unwrap();
        assert!((f.lat - 4.1759).abs() < 1e-9 && (f.lon - 73.5091).abs() < 1e-9);
        assert!(srt_fix("GPS(0.0,0.0,0)").is_none());
    }

    #[test]
    fn civil_days() {
        assert_eq!(days_from_civil(1970, 1, 1), Some(0));
        assert_eq!(gpsu(b"000101000000.000"), Some(946_684_800.0));
    }
}
