// SPDX-License-Identifier: GPL-3.0-or-later
//
// The moments a camera marked in a clip while it recorded (a button, the remote, a voice command), and the movie's own
// header: the twin of the highlights mpv plugin's markers.lua, for Action Footage Library's index (gyroflow-probe
// --library). Where each camera keeps them in the MP4:
//   GoPro HERO5, HERO6    moov/udta/HMMT: their count, then each one's time (ms), 32-bit big-endian (100 at most)
//   GoPro HERO7 and later moov/udta/GPMF: the DEVC whose DVID is HLMT, its HLMT item a struct per highlight, laid out
//                         by the TYPE before it, the time (ms) first, divided by the first SCAL
//   DJI Osmo Action 4, 5 Pro, 6
//                         a free box (top level or in udta): "data", then a box hglg (32-bit big-endian size, its
//                         type, 5 bytes), then 5 bytes per highlight: one byte, and its time (ms), 32-bit little-endian
// Only the boxes' headers are read on the way (seeks: mdat isn't read), then the few that have the highlights.

use std::fs::File;
use std::io::{ Read, Seek, SeekFrom };

const MAX_READ: u64 = 1024 * 1024;
const HGLG_HEAD: usize = 17;

fn u32be(s: &[u8], i: usize) -> Option<u32> { s.get(i..i + 4).map(|b| u32::from_be_bytes([b[0], b[1], b[2], b[3]])) }
fn u64be(s: &[u8], i: usize) -> Option<u64> { s.get(i..i + 8).map(|b| u64::from_be_bytes(b.try_into().unwrap())) }
fn u32le(s: &[u8], i: usize) -> Option<u32> { s.get(i..i + 4).map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]])) }
fn i32be(s: &[u8], i: usize) -> Option<i32> { u32be(s, i).map(|v| v as i32) }

fn read_at(f: &mut File, at: u64, n: u64) -> Vec<u8> {
    let mut buf = Vec::new();
    if f.seek(SeekFrom::Start(at)).is_ok() {
        let _ = f.by_ref().take(n).read_to_end(&mut buf);
    }
    buf
}

/// The boxes from `first` to `last` (offsets, last excluded): type, payload start, payload end
fn boxes(f: &mut File, first: u64, last: u64) -> Vec<([u8; 4], u64, u64)> {
    let mut out = Vec::new();
    let mut pos = first;
    while pos + 8 <= last {
        let head = read_at(f, pos, 16);
        if head.len() < 8 { break; }
        let mut size = u32be(&head, 0).unwrap() as u64;
        let typ: [u8; 4] = head[4..8].try_into().unwrap();
        let mut header = 8;
        if size == 1 {
            let Some(s) = u64be(&head, 8) else { break };
            size = s;
            header = 16;
        } else if size == 0 {
            size = last - pos;
        }
        if size < header || pos + size > last { break; }
        out.push((typ, pos + header, pos + size));
        pos += size;
    }
    out
}

fn hmmt(s: &[u8], times: &mut Vec<f64>) {
    let Some(n) = u32be(s, 0) else { return };
    if n > 100 { return; }
    for k in 1..=n as usize {
        let Some(ms) = u32be(s, 4 * k) else { return };
        if ms > 0 { times.push(ms as f64 / 1000.0); }
    }
}

// GPMF KLV items over s[first..last]: key, type, size, count, where the values start
fn klv(s: &[u8], first: usize, last: usize) -> Vec<([u8; 4], u8, usize, usize, usize)> {
    let mut out = Vec::new();
    let mut pos = first;
    while pos + 8 <= last {
        let key: [u8; 4] = s[pos..pos + 4].try_into().unwrap();
        let typ = s[pos + 4];
        let size = s[pos + 5] as usize;
        let count = (s[pos + 6] as usize) * 256 + s[pos + 7] as usize;
        let start = pos + 8;
        let len = size * count;
        if start + len > last { break; }
        out.push((key, typ, size, count, start));
        pos = start + len.div_ceil(4) * 4;
    }
    out
}

fn hlmt(s: &[u8], first: usize, last: usize, times: &mut Vec<f64>) {
    let mut layout: Option<u8> = None;
    let mut scale = 1f64;
    for (key, typ, size, count, at) in klv(s, first, last) {
        match &key {
            b"TYPE" if typ == b'c' => layout = s.get(at).copied(),
            b"SCAL" if size == 4 && (typ == b'l' || typ == b'L') => {
                let v = if typ == b'l' { i32be(s, at).map(|v| v as f64) } else { u32be(s, at).map(|v| v as f64) };
                if let Some(v) = v.filter(|v| *v != 0.0) { scale = v; }
            }
            b"HLMT" if size >= 4 => {
                if layout != Some(b'L') && layout != Some(b'l') { return; }
                for k in 0..count {
                    let p = at + k * size;
                    let v = if layout == Some(b'l') { i32be(s, p).map(|v| v as f64) } else { u32be(s, p).map(|v| v as f64) };
                    if let Some(v) = v.filter(|v| *v > 0.0) { times.push(v / scale / 1000.0); }
                }
            }
            _ => {}
        }
    }
}

fn gpmf(s: &[u8], times: &mut Vec<f64>) {
    for (key, typ, size, count, at) in klv(s, 0, s.len()) {
        if &key == b"DEVC" && typ == 0 {
            let last = at + size * count;
            let mut highlights = false;
            for (k, t, ks, kc, a) in klv(s, at, last) {
                if &k == b"DVID" && t == b'F' && s.get(a..a + 4) == Some(b"HLMT") { highlights = true; }
                if &k == b"STRM" && t == 0 && highlights { hlmt(s, a, a + ks * kc, times); }
            }
        }
    }
}

fn hglg(f: &mut File, a: u64, b: u64, times: &mut Vec<f64>) -> bool {
    if b - a < HGLG_HEAD as u64 { return false; }
    let head = read_at(f, a, HGLG_HEAD as u64);
    if head.len() < HGLG_HEAD || &head[0..4] != b"data" || &head[8..12] != b"hglg" { return false; }
    let Some(len) = u32be(&head, 4).map(|l| l as u64) else { return false };
    if len < 13 || a + 4 + len > b || len > MAX_READ { return false; }
    let s = read_at(f, a, len + 4);
    for k in 0..((len as usize - 13) / 5) {
        let Some(ms) = u32le(&s, HGLG_HEAD + 5 * k + 1) else { break };
        times.push(ms as f64 / 1000.0);
    }
    true
}

/// The camera's highlights in a clip: "GoPro" or "DJI", and their times (s), in order, each once. None: none
pub fn highlights(path: &str) -> Option<(&'static str, Vec<f64>)> {
    let mut f = File::open(path).ok()?;
    let size = f.metadata().ok()?.len();
    let (mut gopro, mut dji) = (Vec::new(), Vec::new());
    let mut found_dji = false;
    for (typ, a, b) in boxes(&mut f, 0, size) {
        match &typ {
            b"free" | b"skip" => found_dji |= hglg(&mut f, a, b, &mut dji),
            b"moov" => for (mt, ma, mb) in boxes(&mut f, a, b) {
                if &mt != b"udta" { continue; }
                for (ut, ua, ub) in boxes(&mut f, ma, mb) {
                    match &ut {
                        b"HMMT" if ub - ua <= MAX_READ => { let s = read_at(&mut f, ua, ub - ua); hmmt(&s, &mut gopro); }
                        b"GPMF" if ub - ua <= MAX_READ => { let s = read_at(&mut f, ua, ub - ua); gpmf(&s, &mut gopro); }
                        b"free" | b"skip" => found_dji |= hglg(&mut f, ua, ub, &mut dji),
                        _ => {}
                    }
                }
            },
            _ => {}
        }
    }
    let (camera, mut times) = if !gopro.is_empty() { ("GoPro", gopro) } else if found_dji && !dji.is_empty() { ("DJI", dji) } else { return None };
    times.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let mut out: Vec<f64> = Vec::new();
    for t in times {
        if out.last().is_none_or(|l| t - l > 0.05) { out.push(t); }
    }
    Some((camera, out))
}

/// The movie's own header: its length (s), when it was made (seconds since 1970, UTC; None when the camera left it
/// 0), and its video's width, height and turn (clockwise) as tkhd has them
#[derive(Debug, Clone, Default, serde::Serialize)]
pub struct Movie {
    pub duration: f64,
    pub created: Option<i64>,
    pub width: u32,
    pub height: u32,
    pub turn: u32,
}

pub fn movie(path: &str) -> Option<Movie> {
    let mut f = File::open(path).ok()?;
    let size = f.metadata().ok()?.len();
    let mut out: Option<Movie> = None;
    for (typ, a, b) in boxes(&mut f, 0, size) {
        if &typ != b"moov" { continue; }
        let mut have_size = false;
        for (mt, ma, mb) in boxes(&mut f, a, b) {
            if &mt == b"mvhd" && mb - ma >= 32 {
                let h = read_at(&mut f, ma, 32);
                let (created, scale, duration) = if h.first() == Some(&1) {
                    (u64be(&h, 4), u32be(&h, 20), u64be(&h, 24))
                } else {
                    (u32be(&h, 4).map(u64::from), u32be(&h, 12), u32be(&h, 16).map(u64::from))
                };
                if let (Some(scale), Some(duration)) = (scale.filter(|s| *s > 0), duration) {
                    let m = out.get_or_insert_with(Movie::default);
                    m.duration = duration as f64 / scale as f64;
                    m.created = created.filter(|c| *c > 2_082_844_800).map(|c| (c - 2_082_844_800) as i64);
                }
            } else if &mt == b"trak" && !have_size {
                for (tt, ta, tb) in boxes(&mut f, ma, mb) {
                    if &tt != b"tkhd" { continue; }
                    let h = read_at(&mut f, ta, (tb - ta).min(96));
                    let off = if h.first() == Some(&1) { 12 } else { 0 };
                    if h.len() < 84 + off { continue; }
                    let (Some(w), Some(ht)) = (u32be(&h, 76 + off), u32be(&h, 80 + off)) else { continue };
                    if w == 0 || ht == 0 { continue; }
                    let (ma_, mb_) = (i32be(&h, 40 + off).unwrap_or(0), i32be(&h, 44 + off).unwrap_or(0));
                    let turn = if ma_ == 0 && mb_ > 0 { 90 } else if ma_ < 0 { 180 } else if ma_ == 0 && mb_ < 0 { 270 } else { 0 };
                    let m = out.get_or_insert_with(Movie::default);
                    m.width = w >> 16;
                    m.height = ht >> 16;
                    m.turn = turn;
                    have_size = true;
                }
            }
        }
        break;
    }
    out.filter(|m| m.duration > 0.0)
}

/// The HERO4's lens mode as its file has it: moov/udta/SETT's 8th byte, its low 4 bits (0, 2 or 4 in Anton's 292
/// HERO4 Black clips; 4K, which the HERO4 records only in Wide, always 0). Likely its FOV (Wide, Medium, Narrow, as the
/// HERO4's Wi-Fi API numbers them) times two, but not confirmed, so it's shown as "Unknown (mode N)" until the curated
/// list names it. None for any other file (FIRM not "HD4.")
pub fn hero4_lens_mode(path: &str) -> Option<u64> {
    let mut f = File::open(path).ok()?;
    let size = f.metadata().ok()?.len();
    let (_, ma, mb) = boxes(&mut f, 0, size).into_iter().find(|(t, _, _)| t == b"moov")?;
    let (_, ua, ub) = boxes(&mut f, ma, mb).into_iter().find(|(t, _, _)| t == b"udta")?;
    let kids = boxes(&mut f, ua, ub);
    let (_, fa, fb) = *kids.iter().find(|(t, _, _)| t == b"FIRM")?;
    if fb - fa > 64 || !read_at(&mut f, fa, fb - fa).starts_with(b"HD4.") { return None; }
    let (_, sa, sb) = *kids.iter().find(|(t, _, _)| t == b"SETT")?;
    if sb - sa < 8 { return None; }
    read_at(&mut f, sa, 8).get(7).map(|b| (b & 0x0f) as u64)
}
