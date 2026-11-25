// SPDX-License-Identifier: GPL-3.0-or-later
//
// Whether a clip's picture is HDR, log or SDR, as gyroflow-export decides its look (vs-gyroflow/src/probe.rs's log_curve,
// and the video's own transfer): the Library's Share renders an SDR clip once for both its HDR and SDR profiles
// (docs/design/export-gpu-path.md, "HDR in, HDR out"). Read from the MP4 alone, the way the dlogm plugin reads it
// (mpv/scripts/dlogm/dji_color.lua, gopro_color.lua): DJI's ColorMode in the first sample of its metadata track, GoPro's
// Protune color (PTCL) and firmware in moov/udta, and the first video track's `colr` (nclx) box. Keep the four in step.

use std::fs::File;
use std::io::{ Read, Seek, SeekFrom };
use std::path::Path;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Range {
    /// A log curve the exporter maps to HLG (dlogm, dlog, gplog, gplog2)
    Log(&'static str),
    /// HLG or PQ as recorded ("hlg", "pq")
    Hdr(&'static str),
    /// Neither: the exporter's HDR render is the SDR one
    Sdr,
}

const MAX_MOOV: u64 = 64 * 1024 * 1024;
const MAX_SAMPLE: u64 = 4 * 1024 * 1024;
/// Where each DJI model's schema puts ColorMode in StreamMeta (probe.rs's STREAM_COLOR_MODE_FIELD)
const STREAM_COLOR_MODE_FIELD: &[(&str, u64)] = &[("dvtm_ac206.proto", 4), ("dvtm_oq101.proto", 4)];
/// The GoPro cameras whose "LOG" is the exporter's gplog, from these firmwares (probe.rs's GPLOG_FIRMWARE)
const GPLOG_FIRMWARE: &[(&str, f64)] = &[("HERO12 Black", 1.20), ("HERO13 Black", 0.0)];

fn be32(s: &[u8], i: usize) -> Option<u64> { s.get(i..i + 4).map(|b| u32::from_be_bytes([b[0], b[1], b[2], b[3]]) as u64) }
fn be64(s: &[u8], i: usize) -> Option<u64> { s.get(i..i + 8).map(|b| u64::from_be_bytes(b.try_into().unwrap())) }

/// (type, start of its content, end) of each box in s[first..last]
fn boxes(s: &[u8], first: usize, last: usize) -> Vec<([u8; 4], usize, usize)> {
    let mut out = Vec::new();
    let mut pos = first;
    while pos + 8 <= last {
        let Some(mut size) = be32(s, pos).map(|v| v as usize) else { break };
        let mut header = 8;
        if size == 1 { size = be64(s, pos + 8).unwrap_or(0) as usize; header = 16; } else if size == 0 { size = last - pos; }
        if size < header || pos + size > last { break; }
        out.push((s[pos + 4..pos + 8].try_into().unwrap(), pos + header, pos + size));
        pos += size;
    }
    out
}

fn child(s: &[u8], first: usize, last: usize, wanted: &[u8; 4]) -> Option<(usize, usize)> {
    boxes(s, first, last).into_iter().find(|(t, _, _)| t == wanted).map(|(_, a, b)| (a, b))
}

fn read_moov(f: &mut File) -> Option<Vec<u8>> {
    let size = f.metadata().ok()?.len();
    let mut pos = 0u64;
    while pos + 8 <= size {
        let mut head = [0u8; 16];
        f.seek(SeekFrom::Start(pos)).ok()?;
        let n = f.read(&mut head).ok()?;
        if n < 8 { return None; }
        let (mut len, mut header) = (be32(&head, 0)?, 8);
        if len == 1 { len = be64(&head, 8)?; header = 16; } else if len == 0 { len = size - pos; }
        if len < header { return None; }
        if &head[4..8] == b"moov" {
            if len > MAX_MOOV { return None; }
            let mut moov = vec![0u8; (len - header) as usize];
            f.seek(SeekFrom::Start(pos + header)).ok()?;
            f.read_exact(&mut moov).ok()?;
            return Some(moov);
        }
        pos += len;
    }
    None
}

/// The first sample of more than 4 bytes in a track's first chunk: (offset, size)
fn first_sample(s: &[u8], stbl: (usize, usize)) -> Option<(u64, u64)> {
    let (mut offset, a) = match child(s, stbl.0, stbl.1, b"stco") {
        Some((a, _)) => (be32(s, a + 8)?, a),
        None => { let (a, _) = child(s, stbl.0, stbl.1, b"co64")?; (be64(s, a + 8)?, a) }
    };
    if be32(s, a + 4)? < 1 { return None; }
    let per_chunk = child(s, stbl.0, stbl.1, b"stsc").filter(|(a, _)| be32(s, a + 4).unwrap_or(0) >= 1).and_then(|(a, _)| be32(s, a + 12)).unwrap_or(1);
    let (a, _) = child(s, stbl.0, stbl.1, b"stsz")?;
    let (fixed, count) = (be32(s, a + 4)?, be32(s, a + 8)?);
    for k in 0..per_chunk.min(count) {
        let size = if fixed != 0 { fixed } else { be32(s, a + 12 + k as usize * 4)? };
        if size > 4 { return Some((offset, size)); }
        offset += size;
    }
    None
}

/// The tracks: (handler, stbl)
fn tracks(moov: &[u8]) -> Vec<([u8; 4], (usize, usize))> {
    boxes(moov, 0, moov.len()).into_iter().filter(|(t, _, _)| t == b"trak").filter_map(|(_, a, b)| {
        let (ma, mb) = child(moov, a, b, b"mdia")?;
        let (ha, _) = child(moov, ma, mb, b"hdlr")?;
        let handler: [u8; 4] = moov.get(ha + 8..ha + 12)?.try_into().ok()?;
        let (ia, ib) = child(moov, ma, mb, b"minf")?;
        Some((handler, child(moov, ia, ib, b"stbl")?))
    }).collect()
}

fn varint(s: &[u8], i: &mut usize) -> Option<u64> {
    let (mut v, mut shift) = (0u64, 0);
    while *i < s.len() {
        let b = s[*i];
        *i += 1;
        v |= ((b & 0x7f) as u64) << shift;
        if b < 128 { return Some(v); }
        shift += 7;
        if shift > 63 { return None; }
    }
    None
}

enum Pb<'a> { Num(u64), Bytes(&'a [u8]) }

/// The first occurrence of protobuf field `wanted` in `msg`
fn pb_field(msg: &[u8], wanted: u64) -> Option<Pb<'_>> {
    let mut i = 0;
    while i < msg.len() {
        let key = varint(msg, &mut i)?;
        let value = match key % 8 {
            0 => Pb::Num(varint(msg, &mut i)?),
            1 | 5 => { let n = if key % 8 == 1 { 8 } else { 4 }; let v = msg.get(i..i + n)?; i += n; Pb::Bytes(v) }
            2 => { let n = varint(msg, &mut i)? as usize; let v = msg.get(i..i + n)?; i += n; Pb::Bytes(v) }
            _ => return None,
        };
        if key / 8 == wanted { return Some(value); }
    }
    None
}
fn pb_bytes(msg: &[u8], field: u64) -> Option<&[u8]> { match pb_field(msg, field)? { Pb::Bytes(b) => Some(b), _ => None } }

/// DJI's ColorMode in the first metadata sample: "dlogm", "dlog", "hlg", "normal", … (dji_color.lua's COLOR_MODES)
fn dji_color(f: &mut File, moov: &[u8]) -> Option<&'static str> {
    let (_, stbl) = tracks(moov).into_iter().find(|(h, _)| h == b"meta")?;
    let (offset, size) = first_sample(moov, stbl)?;
    if size > MAX_SAMPLE { return None; }
    let mut sample = vec![0u8; size as usize];
    f.seek(SeekFrom::Start(offset)).ok()?;
    f.read_exact(&mut sample).ok()?;
    let proto = std::str::from_utf8(pb_bytes(pb_bytes(pb_bytes(&sample, 1)?, 1)?, 1)?).ok()?;
    let &(_, field) = STREAM_COLOR_MODE_FIELD.iter().find(|(p, _)| *p == proto)?;
    let mode = pb_bytes(pb_bytes(&sample, 2)?, field)?;
    let v = match pb_field(mode, 1) { Some(Pb::Num(v)) => v, _ => 0 };
    Some(match v { 0 => "normal", 1 => "d-cinelike", 2 => "dlog", 9 => "hlg", 19 => "dlogm", 22 => "dlog2", _ => "other" })
}

/// A GoPro KLV value in s[first..last] (gopro_color.lua's klv)
fn klv<'a>(s: &'a [u8], first: usize, last: usize, wanted: &[u8; 4]) -> Option<&'a [u8]> {
    let mut pos = first;
    while pos + 8 <= last {
        let typ = s[pos + 4];
        let n = s[pos + 5] as usize * (s[pos + 6] as usize * 256 + s[pos + 7] as usize);
        let a = pos + 8;
        if a + n > last { return None; }
        if &s[pos..pos + 4] == wanted { return Some(&s[a..a + n]); }
        if typ == 0 { if let Some(v) = klv(s, a, a + n, wanted) { return Some(v); } }
        pos = a + n.div_ceil(4) * 4;
    }
    None
}

fn text(v: &[u8]) -> Option<String> {
    let v = v.split(|b| *b == 0).next().unwrap_or(&[]);
    let t = String::from_utf8_lossy(v).trim().to_string();
    (!t.is_empty()).then_some(t)
}

/// GoPro's log curve: GPLOG2, or LOG from a camera and firmware in GPLOG_FIRMWARE (probe.rs's log_curve)
fn gopro_log(moov: &[u8]) -> Option<&'static str> {
    let (ua, ub) = child(moov, 0, moov.len(), b"udta")?;
    let (ga, gb) = child(moov, ua, ub, b"GPMF")?;
    let color = text(klv(moov, ga, gb, b"PTCL")?)?;
    match color.as_str() {
        "GPLOG2" => Some("gplog2"),
        "LOG" => {
            let camera = klv(moov, ga, gb, b"MINF").and_then(text)?;
            let &(_, min) = GPLOG_FIRMWARE.iter().find(|(c, _)| *c == camera)?;
            let firm = child(moov, ua, ub, b"FIRM").and_then(|(a, b)| text(&moov[a..b]))?;
            let parts: Vec<&str> = firm.split('.').collect();
            let v = format!("{}.{}", parts.get(2)?, parts.get(3)?).parse::<f64>().ok()?;
            (v >= min).then_some("gplog")
        }
        _ => None,
    }
}

/// The first video track's colr (nclx) transfer: 16 PQ, 18 HLG
fn transfer(moov: &[u8]) -> Option<u16> {
    let (_, (sa, sb)) = tracks(moov).into_iter().find(|(h, _)| h == b"vide")?;
    let (da, db) = child(moov, sa, sb, b"stsd")?;
    // The sample entry after stsd's version and count; its boxes after the 78 bytes of a VisualSampleEntry
    let (_, ea, eb) = *boxes(moov, da + 8, db).first()?;
    let (ca, _) = boxes(moov, ea + 78, eb).into_iter().find(|(t, a, _)| t == b"colr" && moov.get(*a..*a + 4) == Some(b"nclx")).map(|(_, a, b)| (a, b))?;
    moov.get(ca + 6..ca + 8).map(|b| u16::from_be_bytes([b[0], b[1]]))
}

/// What the clip is, by its file; None where it can't be read. `curve`: the clip's own source curve (its .afp.json),
/// which goes over what the file says ("none": not log)
pub fn of(path: &Path, curve: Option<&str>) -> Option<Range> {
    let mut f = File::open(path).ok()?;
    let moov = read_moov(&mut f)?;
    let hdr = match transfer(&moov) { Some(16) => Some(Range::Hdr("pq")), Some(18) => Some(Range::Hdr("hlg")), _ => None };
    match curve.filter(|c| !c.is_empty() && *c != "auto") {
        Some("none") => return Some(hdr.unwrap_or(Range::Sdr)),
        Some(c) => return Some(Range::Log(match c { "dlogm" => "dlogm", "dlog" => "dlog", "gplog" => "gplog", "gplog2" => "gplog2", "ilog" => "ilog", _ => "other" })),
        None => {}
    }
    if let Some(c) = dji_color(&mut f, &moov) {
        match c { "dlogm" => return Some(Range::Log("dlogm")), "dlog" => return Some(Range::Log("dlog")), _ => {} }
    }
    if let Some(c) = gopro_log(&moov) { return Some(Range::Log(c)); }
    Some(hdr.unwrap_or(Range::Sdr))
}

#[cfg(test)]
mod tests {
    use super::*;

    // On real clips: AFP_RANGE_CLIPS="<path>=<want>;…" (want: dlogm, gplog, hlg, pq, sdr), skipped without it
    #[test]
    fn real_clips_read_as_the_exporter_reads_them() {
        let Ok(list) = std::env::var("AFP_RANGE_CLIPS") else { eprintln!("AFP_RANGE_CLIPS not set: skipped"); return };
        for entry in list.split(';').filter(|e| !e.is_empty()) {
            let (path, want) = entry.rsplit_once('=').unwrap();
            let got = of(Path::new(path), None).unwrap_or_else(|| panic!("{path}: unreadable"));
            let name = match got { Range::Log(c) | Range::Hdr(c) => c, Range::Sdr => "sdr" };
            eprintln!("{path}: {got:?}");
            assert_eq!(name, want, "{path}");
        }
    }

    #[test]
    fn the_clip_s_own_curve_goes_over_the_file() {
        // Any MP4 with a moov: a minimal one written here (ftyp + an empty moov)
        let dir = std::env::temp_dir().join(format!("afp-range-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join("a.mp4");
        std::fs::write(&p, [0, 0, 0, 8, b'm', b'o', b'o', b'v']).unwrap();
        assert_eq!(of(&p, None), Some(Range::Sdr));
        assert_eq!(of(&p, Some("dlogm")), Some(Range::Log("dlogm")));
        assert_eq!(of(&p, Some("none")), Some(Range::Sdr));
        assert_eq!(of(&p, Some("auto")), Some(Range::Sdr));
        assert_eq!(of(&dir.join("missing.mp4"), None), None);
        std::fs::remove_dir_all(&dir).ok();
    }
}
