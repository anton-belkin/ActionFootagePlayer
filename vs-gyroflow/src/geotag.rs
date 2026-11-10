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
const SET_SINCE: f64 = 1_483_228_800.0; // 2017-01-01 (a GPS time before it isn't trusted)

/// A clock never set: before 2005 (cameras start at 1904, 1970, 2000), or on the first day of a year a camera starts
/// its clock at (GoPro: 2015-01-01, 2016-01-01; HERO7's clips at 2016-01-01 00:24). Not "before 2017": Anton's HERO4
/// recorded the USA in May 2016 with its clock right (its photos' EXIF agree)
pub fn clock_unset(t: f64) -> bool {
    const DEFAULT_DAYS: [f64; 4] = [946_684_800.0, 1_262_304_000.0, 1_420_070_400.0, 1_451_606_400.0]; // 2000, 2010, 2015, 2016 Jan 1
    t < 1_104_537_600.0 || DEFAULT_DAYS.iter().any(|d| (*d..*d + 86_400.0).contains(&t))
}

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
fn gopro_udta(path: &Path) -> Option<Vec<u8>> { udta_box(path, b"GPMF") }

/// A box of moov/udta (GoPro's GPMF, QuickTime's ©xyz, ©mdl…), at most 1 MiB
/// A box `want` among the boxes from `from` to `to`: its contents' start and end
fn child(f: &mut std::fs::File, from: u64, to: u64, want: &[u8; 4]) -> Option<(u64, u64)> {
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
}

fn read_range(f: &mut std::fs::File, a: u64, b: u64, most: u64) -> Option<Vec<u8>> {
    if b < a || b - a > most { return None; }
    let mut buf = vec![0u8; (b - a) as usize];
    f.seek(SeekFrom::Start(a)).ok()?;
    f.read_exact(&mut buf).ok()?;
    Some(buf)
}

fn udta_box(path: &Path, name: &[u8; 4]) -> Option<Vec<u8>> {
    let mut f = std::fs::File::open(path).ok()?;
    let size = f.metadata().ok()?.len();
    let (ma, mb) = child(&mut f, 0, size, b"moov")?;
    let (ua, ub) = child(&mut f, ma, mb, b"udta")?;
    let (ga, gb) = child(&mut f, ua, ub, name)?;
    read_range(&mut f, ga, gb, 1 << 20)
}

/// The moov box whole (at most 8 MiB), for a look at what's written in it
fn moov(path: &Path) -> Option<Vec<u8>> {
    let mut f = std::fs::File::open(path).ok()?;
    let size = f.metadata().ok()?.len();
    let (ma, mb) = child(&mut f, 0, size, b"moov")?;
    read_range(&mut f, ma, mb, 8 << 20)
}

/// The first `n` samples of the track whose sample description is `codec` (DJI's djmd), read straight from its
/// sample table: a few KB of reads, not the file's ends (from a share, telemetry-parser's open reads 15 MB at each)
fn first_samples(path: &Path, codec: &[u8; 4], n: usize) -> Vec<Vec<u8>> {
    let run = || -> Option<Vec<Vec<u8>>> {
        let mut f = std::fs::File::open(path).ok()?;
        let size = f.metadata().ok()?.len();
        let (ma, mb) = child(&mut f, 0, size, b"moov")?;
        let mut at = ma;
        while let Some((ta, tb)) = child(&mut f, at, mb, b"trak") {
            at = tb;
            let stbl = (|| { let (da, db) = child(&mut f, ta, tb, b"mdia")?; let (ia, ib) = child(&mut f, da, db, b"minf")?; child(&mut f, ia, ib, b"stbl") })();
            let Some((sa, sb)) = stbl else { continue };
            let Some(stsd) = child(&mut f, sa, sb, b"stsd").and_then(|(a, b)| read_range(&mut f, a, b, 64 << 10)) else { continue };
            if stsd.get(12..16) != Some(codec.as_slice()) { continue; }
            let stsz = child(&mut f, sa, sb, b"stsz").and_then(|(a, b)| read_range(&mut f, a, (a + 12 + 4 * n as u64).min(b), 1 << 20))?;
            let fixed = be32(&stsz, 4)?;
            let count = (be32(&stsz, 8)? as usize).min(n);
            let sizes: Vec<u64> = (0..count).map(|i| if fixed > 0 { fixed as u64 } else { be32(&stsz, 12 + 4 * i).unwrap_or(0) as u64 }).collect();
            let stsc = child(&mut f, sa, sb, b"stsc").and_then(|(a, b)| read_range(&mut f, a, b, 1 << 20))?;
            let chunks: Vec<(u32, u32)> = (0..be32(&stsc, 4)? as usize).filter_map(|i| Some((be32(&stsc, 8 + 12 * i)?, be32(&stsc, 12 + 12 * i)?))).collect();
            let (co, wide) = match child(&mut f, sa, sb, b"stco") { Some(x) => (x, false), None => (child(&mut f, sa, sb, b"co64")?, true) };
            let offs = read_range(&mut f, co.0, (co.0 + 8 + if wide { 8 } else { 4 } * (count as u64 + 1)).min(co.1), 1 << 20)?;
            let off = |c: usize| if wide { offs.get(8 + 8 * c..16 + 8 * c).map(|b| u64::from_be_bytes(b.try_into().unwrap())) } else { be32(&offs, 8 + 4 * c).map(u64::from) };
            // Each sample's place: its chunk's offset and the sizes before it in the chunk
            let mut out = Vec::new();
            let (mut chunk, mut in_chunk, mut pos) = (0usize, 0u32, off(0)?);
            for &sz in &sizes {
                let per = chunks.iter().rev().find(|(first, _)| (*first as usize) <= chunk + 1).map(|(_, p)| *p).unwrap_or(1);
                if in_chunk >= per { chunk += 1; in_chunk = 0; pos = off(chunk)?; }
                out.push(read_range(&mut f, pos, pos + sz, 1 << 20)?);
                pos += sz; in_chunk += 1;
            }
            return Some(out);
        }
        None
    };
    run().unwrap_or_default()
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

// ---------- DJI drones, and QuickTime's own location ----------
//
// What Anton's drones write (checked 2026-11-10 on /Volumes/photo/DATA):
// - Mavic Pro (camera FC220), 2017-2018 .MOV: moov/udta ©xyz "+45.8778+006.1432+000.000/" (ISO 6709) and ©mdl "FC220".
// - Mini 4 Pro (FC8482), 2024-2026 .MP4: a djmd track (protobuf, dvtm_Mini4_Pro.proto, not in telemetry-parser): the
//   clip's header 1.1.10 "DJI Mini4 Pro"; each frame's drone block 3.3: 3.3.4 = GpsBasic {1 PositionCoord {1 unit (0
//   radians, 1 degrees), 2 latitude, 3 longitude}, 2 altitude mm, 3 status (1 invalid)}. A .SRT beside it only when
//   the app was set to make one.
// - 2020-2023 .MOV/.MP4 (Mavic 2 / Air generation): nothing but "Lavf56.15.102" (DJI's muxer): no model, no place.

/// Protobuf fields of a message: (field, wire type, varint or 64-bit value, bytes)
fn pb_fields(b: &[u8]) -> Option<Vec<(u64, u8, u64, &[u8])>> {
    let mut out = Vec::new();
    let mut i = 0;
    let varint = |b: &[u8], i: &mut usize| -> Option<u64> {
        let (mut r, mut s) = (0u64, 0);
        loop { let c = *b.get(*i)?; *i += 1; r |= ((c & 0x7f) as u64) << s; s += 7; if c < 0x80 { return Some(r); } if s > 63 { return None; } }
    };
    while i < b.len() {
        let k = varint(b, &mut i)?;
        let (f, t) = (k >> 3, (k & 7) as u8);
        if f == 0 { return None; }
        match t {
            0 => { let v = varint(b, &mut i)?; out.push((f, t, v, &b[0..0])); }
            1 => { let v = u64::from_le_bytes(b.get(i..i + 8)?.try_into().ok()?); i += 8; out.push((f, t, v, &b[0..0])); }
            5 => { let v = u32::from_le_bytes(b.get(i..i + 4)?.try_into().ok()?) as u64; i += 4; out.push((f, t, v, &b[0..0])); }
            2 => { let n = varint(b, &mut i)? as usize; let sub = b.get(i..i + n)?; i += n; out.push((f, t, 0, sub)); }
            _ => return None,
        }
    }
    Some(out)
}

/// A GpsBasic anywhere in a djmd sample: a message whose field 1 is a PositionCoord (latitude 2 and longitude 3 as
/// doubles, unit 1: radians unless 1), its status (3) not 1 (invalid); altitude (2) in mm
fn dji_gps(b: &[u8], depth: u32) -> Option<Fix> {
    let fields = pb_fields(b)?;
    let coord = fields.iter().find(|(f, t, ..)| *f == 1 && *t == 2).and_then(|(_, _, _, sub)| {
        let c = pb_fields(sub)?;
        let dbl = |n: u64| c.iter().find(|(f, t, ..)| *f == n && *t == 1).map(|(_, _, v, _)| f64::from_bits(*v));
        let deg = c.iter().any(|(f, t, v, _)| *f == 1 && *t == 0 && *v == 1);
        let (lat, lon) = (dbl(2)?, dbl(3)?);
        let (lat, lon) = if deg { (lat, lon) } else { (lat.to_degrees(), lon.to_degrees()) };
        (lat.is_finite() && lon.is_finite() && lat.abs() <= 90.0 && lon.abs() <= 180.0 && !(lat == 0.0 && lon == 0.0)).then_some((lat, lon))
    });
    if let Some((lat, lon)) = coord {
        let status = fields.iter().find(|(f, t, ..)| *f == 3 && *t == 0).map(|x| x.2).unwrap_or(0);
        if status != 1 {
            let alt = fields.iter().find(|(f, t, ..)| *f == 2 && *t == 0).map(|x| x.2 as i64 as i32 as f64 / 1000.0);
            return Some(Fix { lat, lon, alt, t: None });
        }
    }
    if depth >= 6 { return None; }
    fields.iter().filter(|(_, t, ..)| *t == 2).find_map(|(_, _, _, sub)| dji_gps(sub, depth + 1))
}

/// The product name in a djmd clip header (1.1.10, "DJI Mini4 Pro")
fn dji_product(b: &[u8]) -> Option<String> {
    let top = pb_fields(b)?;
    let (_, _, _, h) = top.iter().find(|(f, t, ..)| *f == 1 && *t == 2)?;
    let (_, _, _, h1) = pb_fields(h)?.into_iter().find(|(f, t, ..)| *f == 1 && *t == 2)?;
    let (_, _, _, name) = pb_fields(h1)?.into_iter().find(|(f, t, ..)| *f == 10 && *t == 2)?;
    std::str::from_utf8(name).ok().map(str::trim).filter(|n| !n.is_empty() && n.is_ascii()).map(str::to_string)
}

/// A DJI clip's metadata track (djmd), read once a process (the probe asks it for the drone's name and its place: from a
/// share, opening the track reads the file's ends, seconds): the first fix in its first 5 s, and its product name
fn dji_track(path: &Path) -> (Option<Fix>, Option<String>) {
    static LAST: std::sync::Mutex<Option<(std::path::PathBuf, (Option<Fix>, Option<String>))>> = std::sync::Mutex::new(None);
    if let Some((p, r)) = LAST.lock().unwrap().as_ref() { if p == path { return r.clone(); } }
    let r = dji_track_read(path);
    *LAST.lock().unwrap() = Some((path.to_path_buf(), r.clone()));
    r
}

fn dji_track_read(path: &Path) -> (Option<Fix>, Option<String>) {
    // Its first samples (the first ~5 s at 30 fps), DJI's own metadata only: a header naming its dvtm_*.proto
    let samples = first_samples(path, b"djmd", 150);
    let Some(first) = samples.first() else { return (None, None) };
    if !first.windows(5).take(256).any(|w| w == b"dvtm_") { return (None, None); }
    let product = dji_product(first);
    (samples.iter().find_map(|b| dji_gps(b, 0)), product)
}

/// "+45.8778+006.1432+000.000/" (ISO 6709, as QuickTime's ©xyz and com.apple.quicktime.location.ISO6709 have it)
pub fn iso6709(s: &str) -> Option<Fix> {
    let s = s.trim().trim_end_matches('/');
    let mut parts: Vec<f64> = Vec::new();
    let mut start = 0;
    for (i, c) in s.char_indices().skip(1) {
        if c == '+' || c == '-' { parts.push(s[start..i].parse().ok()?); start = i; }
    }
    parts.push(s.get(start..)?.parse().ok()?);
    let (lat, lon) = (*parts.first()?, *parts.get(1)?);
    (lat.abs() <= 90.0 && lon.abs() <= 180.0 && !(lat == 0.0 && lon == 0.0)).then_some(Fix { lat, lon, alt: parts.get(2).copied().filter(|a| *a != 0.0), t: None })
}

/// A QuickTime udta text box (©xyz, ©mdl): 2 bytes length, 2 bytes language, the text; or the text alone
fn udta_text(path: &Path, name: &[u8; 4]) -> Option<String> {
    let b = udta_box(path, name)?;
    // Some write it as plain text, zero-padded (the Mavic Pro's ©mdl "FC220-Se")
    let t = if b.len() >= 2 && b[0].is_ascii_graphic() && b[1].is_ascii_graphic() { &b[..] } else {
        let n = u16::from_be_bytes([*b.first()?, *b.get(1)?]) as usize;
        b.get(4..4 + n).or_else(|| b.get(4..))?
    };
    Some(String::from_utf8_lossy(t).trim_matches(char::from(0)).trim().to_string()).filter(|t| !t.is_empty())
}

/// Where the file says it was recorded, as QuickTime has it (phones, the Mavic Pro, …)
pub fn quicktime_place(path: &Path) -> Option<Fix> {
    udta_text(path, b"\xa9xyz").and_then(|t| iso6709(&t))
        .or_else(|| crate::mp4meta::read_key(path, "com.apple.quicktime.location.ISO6709").ok().flatten().and_then(|t| iso6709(&t)))
}

/// DJI's camera codes (written where a model would be) to the drone's name
const DJI_CAMERAS: &[(&str, &str)] = &[("FC220", "Mavic Pro"), ("FC2103", "Mavic Air"), ("FC2204", "Mavic 2 Zoom"), ("L1D-20c", "Mavic 2 Pro"),
    ("FC3170", "Mavic Air 2"), ("FC3411", "Air 2S"), ("FC7203", "Mavic Mini"), ("FC7303", "Mini 2"), ("FC3582", "Mini 3 Pro"), ("FC3682", "Mini 3"),
    ("FC8482", "Mini 4 Pro"), ("L2D-20c", "Mavic 3"), ("FC4170", "Mavic 3 Pro"), ("FC6310", "Phantom 4 Pro"), ("FC330", "Phantom 4"), ("FC300X", "Phantom 3")];

/// A DJI drone's name, where telemetry-parser has none for it: the djmd header's product name ("DJI Mini4 Pro" →
/// "DJI Mini 4 Pro"), else the camera code QuickTime's ©mdl has ("FC220" → "DJI Mavic Pro"), else, for DJI's muxer's
/// files with nothing in them (DJI_0172.MOV, "Lavf56.15.102"), "DJI drone"
pub fn dji_model(path: &Path) -> Option<String> {
    let name = path.file_name().map(|n| n.to_string_lossy().to_uppercase()).unwrap_or_default();
    if let (_, Some(p)) = dji_track(path) {
        let p = p.trim_start_matches("DJI").trim();
        // "Mini4 Pro" → "Mini 4 Pro", as DJI names it
        let mut spaced = String::new();
        for (i, c) in p.char_indices() {
            if i > 0 && c.is_ascii_digit() && p[..i].ends_with(|x: char| x.is_ascii_alphabetic()) { spaced.push(' '); }
            spaced.push(c);
        }
        return Some(format!("DJI {spaced}"));
    }
    if let Some(m) = udta_text(path, b"\xa9mdl") {
        let code = m.split(['-', ' ']).next().unwrap_or(&m).to_string();
        if let Some((_, n)) = DJI_CAMERAS.iter().find(|(c, _)| c.eq_ignore_ascii_case(&code) || m.starts_with(c)) { return Some(format!("DJI {n}")); }
        if name.starts_with("DJI_") { return Some(format!("DJI {code}")); }
    }
    // DJI's muxer of those years ("Lavf56.15.102", in udta ©swr or ilst ©too) and nothing else in the file
    if name.starts_with("DJI_") && moov(path).is_some_and(|m| m.windows(6).any(|w| w == b"Lavf56")) {
        // The drone's own photos beside it say what it is (its videos don't): when they all agree
        return Some(sibling_drone(path).map(|n| format!("DJI {n}")).unwrap_or_else(|| "DJI drone".into()));
    }
    None
}

/// The drone the DJI_*.JPG / .DNG photos in a clip's folder were taken with (their EXIF Model is the camera's code, the
/// first 64 KB of a few of them looked at), when they agree on one
fn sibling_drone(clip: &Path) -> Option<&'static str> {
    let dir = clip.parent()?;
    let mut photos: Vec<std::path::PathBuf> = std::fs::read_dir(dir).ok()?.filter_map(|e| e.ok().map(|e| e.path())).filter(|p| {
        let n = p.file_name().map(|n| n.to_string_lossy().to_uppercase()).unwrap_or_default();
        n.starts_with("DJI_") && (n.ends_with(".JPG") || n.ends_with(".DNG"))
    }).collect();
    photos.sort();
    let step = (photos.len() / 4).max(1);
    let mut found: Option<&'static str> = None;
    for p in photos.iter().step_by(step).take(4) {
        let mut head = vec![0u8; 64 * 1024];
        let n = std::fs::File::open(p).and_then(|mut f| f.read(&mut head)).ok()?;
        let head = &head[..n];
        let this = DJI_CAMERAS.iter().find(|(c, _)| head.windows(c.len() + 1).any(|w| &w[..c.len()] == c.as_bytes() && w[c.len()] == 0)).map(|(_, n)| *n)?;
        if found.is_some_and(|f| f != this) { return None; }
        found = Some(this);
    }
    found
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
        fix = dji_track(path).0.or_else(|| quicktime_place(path)).or_else(|| srt_beside(path));
    }
    // Any other camera (a phone…): where QuickTime's own location says
    if fix.is_none() && !is_gopro && brand != "dji" { fix = quicktime_place(path); }
    let zone_min = udta.as_deref().and_then(gopro_zone);
    let clock = if let Some(u) = gps_clock {
        Clock { utc: Some(u), basis: "gps", zone_min, why: "the camera's GPS time".into() }
    } else {
        match mvhd {
            None => Clock { utc: None, basis: "unset", zone_min, why: "no recording time in the file".into() },
            Some(c) if clock_unset(c) => Clock { utc: None, basis: "unset", zone_min, why: "the camera's clock wasn't set".into() },
            Some(c) if is_gopro && zone_min.is_some() => {
                let z = zone_min.unwrap();
                Clock { utc: Some(c), basis: "utc", zone_min, why: format!("GoPro writes UTC (its own zone UTC{})", zone_text(z)) }
            }
            Some(c) if is_gopro => Clock { utc: Some(c), basis: "local", zone_min, why: "an older GoPro writes its local time, with no zone".into() },
            // The Mavic Pro writes its local time (its photos' EXIF, its card's file times and its videos agree)
            Some(c) if brand == "dji" && camera.contains("Mavic Pro") => Clock { utc: Some(c), basis: "local", zone_min, why: "the Mavic Pro writes its local time, with no zone".into() },
            Some(c) if brand == "dji" && (camera.contains("drone") || camera.contains("Air 2")) => Clock { utc: Some(c), basis: "assumed", zone_min, why: "UTC assumed (this DJI drone writes nothing else to tell)".into() },
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
    fn dji_gps_in_its_frame_metadata() {
        // A Mini 4 Pro's frame, as its djmd sample has it: 3.3.4 = GpsBasic (radians), 3.3.1.4 its camera
        fn tag(f: u64, t: u8) -> Vec<u8> { let mut v = Vec::new(); let mut k = (f << 3) | t as u64; loop { let b = (k & 0x7f) as u8; k >>= 7; if k == 0 { v.push(b); break; } v.push(b | 0x80); } v }
        fn msg(f: u64, body: &[u8]) -> Vec<u8> { let mut v = tag(f, 2); v.extend(tag(body.len() as u64, 0).iter().skip(0).take(0)); let mut n = body.len() as u64; loop { let b = (n & 0x7f) as u8; n >>= 7; if n == 0 { v.push(b); break; } v.push(b | 0x80); } v.extend(body); v }
        fn dbl(f: u64, x: f64) -> Vec<u8> { let mut v = tag(f, 1); v.extend(x.to_bits().to_le_bytes()); v }
        fn var(f: u64, x: u64) -> Vec<u8> { let mut v = tag(f, 0); let mut n = x; loop { let b = (n & 0x7f) as u8; n >>= 7; if n == 0 { v.push(b); break; } v.push(b | 0x80); } v }
        let coord = [dbl(2, 0.07925854736090637), dbl(3, 1.2811757241420525)].concat();
        let gps = [msg(1, &coord), var(2, 29000), var(4, 1)].concat();
        let drone = [msg(1, &[msg(4, b"DJI FC8482")].concat()), msg(4, &gps)].concat();
        let sample = [msg(3, &msg(3, &drone))].concat();
        let f = dji_gps(&sample, 0).unwrap();
        assert!((f.lat - 4.541176).abs() < 1e-5 && (f.lon - 73.4059).abs() < 1e-3 && f.alt == Some(29.0));
        // Invalid (status 1): none
        let bad = [msg(3, &msg(3, &[msg(4, &[msg(1, &coord), var(3, 1)].concat())].concat()))].concat();
        assert!(dji_gps(&bad, 0).is_none());
        // The clip's header: its product's name
        let head = msg(1, &msg(1, &[msg(1, b"dvtm_Mini4_Pro.proto"), msg(10, b"DJI Mini4 Pro")].concat()));
        assert_eq!(dji_product(&head).as_deref(), Some("DJI Mini4 Pro"));
    }

    #[test]
    fn quicktime_iso6709() {
        let f = iso6709("+45.8778+006.1432+000.000/").unwrap();
        assert!((f.lat - 45.8778).abs() < 1e-9 && (f.lon - 6.1432).abs() < 1e-9 && f.alt.is_none());
        let f = iso6709("-08.6500+115.2167+012.500/").unwrap();
        assert!((f.lat + 8.65).abs() < 1e-9 && f.alt == Some(12.5));
        assert!(iso6709("+00.0000+000.0000/").is_none() && iso6709("garbage").is_none());
    }

    #[test]
    fn clocks_never_set() {
        assert!(clock_unset(1_451_607_876.0)); // HERO7: 2016-01-01 00:24
        assert!(clock_unset(14.0)); // Mavic Pro: 1970-01-01 00:00:14
        assert!(!clock_unset(1_462_303_666.0)); // HERO4 in the USA: 2016-05-03
        assert!(!clock_unset(1_760_864_846.0));
    }

    #[test]
    fn civil_days() {
        assert_eq!(days_from_civil(1970, 1, 1), Some(0));
        assert_eq!(gpsu(b"000101000000.000"), Some(946_684_800.0));
    }
}
