// SPDX-License-Identifier: GPL-3.0-or-later
//
// A part of a camera's MP4 kept without re-encoding (docs/lossless-cut.md): for a clip spoiled in part, the record
// button pressed by accident. The footage itself isn't changed: every sample of every track (video, audio, the
// camera's metadata, its cover) is copied byte for byte, and only the tables that say where they are and how long
// they last are written anew, with the durations and edit lists. The start goes back to the keyframe at or before it
// (a clip starts on one); the end can be any frame (the cameras handled here have no B-frames).
//
// What a camera keeps for the whole clip in the part cut off is put back the camera's own way where it can be, by an
// adapter per camera:
// - DJI (a `djmd` track): the clip's own data (product, lens, readout time, IMU rate, the color mode) is only in the
//   first sample of the metadata tracks (djmd, dbgi). Its top-level protobuf fields the new first sample lacks are put
//   in front of it, where a protobuf reader merges them. Without them Gyroflow takes the clip for an unknown DJI
//   camera (no lens profile), and the player neither stabilizes it nor converts its log.
// For every camera: the timecode sample moves on by the frames cut off, the audio is kept from the packet that holds
// the first frame's moment and lined up with an edit list, the creation times move on by the time cut off. And the
// cut gets our own key, ch.actionfootageplayer.cut: where it comes from (the original, its first frame) and how the
// player read the original (stabilization, log curve, lens), for what the camera's own way can't carry.
//
// verify() then reads the cut as the player reads the original (probe.rs, the motion data through telemetry-parser)
// and compares: a cut that doesn't read the same isn't kept. A camera without an adapter is cut only when it does.

use std::fs::File;
use std::io::{ BufWriter, Read, Seek, SeekFrom, Write };
use std::path::Path;
use std::sync::{ Arc, atomic::AtomicBool };

use crate::{ mp4meta, probe };
use gyroflow_core::telemetry_parser::{ Input, InputOptions, tags_impl::* };

/// Our key in a cut's moov/meta
pub const CUT_KEY: &str = "ch.actionfootageplayer.cut";

fn be32(b: &[u8], at: usize) -> u32 { u32::from_be_bytes(b[at..at + 4].try_into().unwrap()) }
fn be64(b: &[u8], at: usize) -> u64 { u64::from_be_bytes(b[at..at + 8].try_into().unwrap()) }

// ---------------------------------------------------------------------------------------------------- boxes

const CONTAINERS: &[&[u8; 4]] = &[b"moov", b"trak", b"mdia", b"minf", b"stbl", b"edts", b"dinf"];

/// A box of the moov, kept as its body, or as its children for the containers walked into
#[derive(Clone, Debug)]
struct Node { kind: [u8; 4], body: Vec<u8>, kids: Option<Vec<Node>> }

impl Node {
    fn leaf(kind: &[u8; 4], body: Vec<u8>) -> Node { Node { kind: *kind, body, kids: None } }
    fn child(&self, kind: &[u8; 4]) -> Option<&Node> { self.kids.as_ref()?.iter().find(|k| &k.kind == kind) }
    fn child_mut(&mut self, kind: &[u8; 4]) -> Option<&mut Node> { self.kids.as_mut()?.iter_mut().find(|k| &k.kind == kind) }
    fn path(&self, kinds: &[&[u8; 4]]) -> Option<&Node> {
        kinds.iter().try_fold(self, |n, k| n.child(k))
    }
    fn path_mut(&mut self, kinds: &[&[u8; 4]]) -> Option<&mut Node> {
        let mut n = self;
        for k in kinds { n = n.child_mut(k)?; }
        Some(n)
    }
    fn bytes(&self) -> Vec<u8> {
        let body: Vec<u8> = match &self.kids { Some(k) => k.iter().flat_map(|c| c.bytes()).collect(), None => self.body.clone() };
        let mut out = Vec::with_capacity(body.len() + 16);
        if body.len() + 8 > u32::MAX as usize {
            out.extend_from_slice(&1u32.to_be_bytes());
            out.extend_from_slice(&self.kind);
            out.extend_from_slice(&(body.len() as u64 + 16).to_be_bytes());
        } else {
            out.extend_from_slice(&(body.len() as u32 + 8).to_be_bytes());
            out.extend_from_slice(&self.kind);
        }
        out.extend(body);
        out
    }
}

fn parse_boxes(b: &[u8]) -> Result<Vec<Node>, String> {
    let mut out = Vec::new();
    let mut at = 0;
    while at + 8 <= b.len() {
        let (mut size, mut header) = (be32(b, at) as usize, 8);
        if size == 1 {
            if at + 16 > b.len() { return Err("a broken box".into()); }
            size = be64(b, at + 8) as usize;
            header = 16;
        } else if size == 0 {
            size = b.len() - at;
        }
        if size < header || at + size > b.len() { return Err("a broken box".into()); }
        let kind: [u8; 4] = b[at + 4..at + 8].try_into().unwrap();
        let body = &b[at + header..at + size];
        out.push(if CONTAINERS.contains(&&kind) {
            Node { kind, body: Vec::new(), kids: Some(parse_boxes(body)?) }
        } else {
            Node::leaf(&kind, body.to_vec())
        });
        at += size;
    }
    Ok(out)
}

/// The top-level boxes of a file: (start, header length, size, type). Anything after the last box that isn't one
/// (Insta360's trailer) is an error: such a file isn't cut
fn top_level(f: &mut File) -> Result<Vec<(u64, u64, u64, [u8; 4])>, String> {
    let len = f.metadata().map_err(|e| e.to_string())?.len();
    let mut out = Vec::new();
    let mut pos = 0u64;
    while pos < len {
        if pos + 8 > len { return Err("bytes after the last box".into()); }
        let mut head = [0u8; 16];
        f.seek(SeekFrom::Start(pos)).map_err(|e| e.to_string())?;
        f.read_exact(&mut head[..8]).map_err(|e| e.to_string())?;
        let (mut size, mut header) = (be32(&head, 0) as u64, 8u64);
        if size == 1 {
            f.read_exact(&mut head[8..16]).map_err(|e| e.to_string())?;
            size = be64(&head, 8);
            header = 16;
        } else if size == 0 {
            size = len - pos;
        }
        if size < header || pos + size > len { return Err("not an MP4 made of boxes to its end".into()); }
        out.push((pos, header, size, head[4..8].try_into().unwrap()));
        pos += size;
    }
    Ok(out)
}

fn read_moov(f: &mut File) -> Result<(Vec<(u64, u64, u64, [u8; 4])>, Node), String> {
    let top = top_level(f)?;
    if top.iter().any(|b| matches!(&b.3, b"moof" | b"mfra")) { return Err("a fragmented MP4".into()); }
    let mut moovs = top.iter().filter(|b| &b.3 == b"moov");
    let &(at, header, size, _) = moovs.next().ok_or("no moov")?;
    if moovs.next().is_some() { return Err("two moov boxes".into()); }
    let mut body = vec![0u8; (size - header) as usize];
    f.seek(SeekFrom::Start(at + header)).map_err(|e| e.to_string())?;
    f.read_exact(&mut body).map_err(|e| e.to_string())?;
    Ok((top, Node { kind: *b"moov", body: Vec::new(), kids: Some(parse_boxes(&body)?) }))
}

// ---------------------------------------------------------------------------------------------------- tracks

#[derive(Clone, Copy, Debug)]
struct Sample { off: u64, size: u32, dur: u32, dts: u64, sync: bool }

#[derive(Debug)]
struct Track { handler: [u8; 4], fourcc: [u8; 4], timescale: u32, samples: Vec<Sample>, media_time: i64 }

/// (version, rest) of a full box's body
fn full(b: &[u8]) -> (u8, &[u8]) { (b[0], &b[4..]) }

fn timescale_of(mdhd: &[u8]) -> u32 {
    let (v, p) = full(mdhd);
    be32(p, if v == 1 { 16 } else { 8 })
}

fn read_track(trak: &Node) -> Result<Track, String> {
    let mdia = trak.child(b"mdia").ok_or("a track without mdia")?;
    let hdlr = &mdia.child(b"hdlr").ok_or("a track without hdlr")?.body;
    let handler: [u8; 4] = hdlr[8..12].try_into().unwrap();
    let timescale = timescale_of(&mdia.child(b"mdhd").ok_or("a track without mdhd")?.body);
    let stbl = trak.path(&[b"mdia", b"minf", b"stbl"]).ok_or("a track without sample tables")?;
    for k in stbl.kids.as_ref().unwrap() {
        if !matches!(&k.kind, b"stsd" | b"stts" | b"stss" | b"stsc" | b"stsz" | b"stco" | b"co64" | b"sdtp" | b"sgpd" | b"sbgp") {
            return Err(format!("{} in a track's sample tables isn't handled", String::from_utf8_lossy(&k.kind)));
        }
    }
    let stsd = &stbl.child(b"stsd").ok_or("no stsd")?.body;
    if be32(stsd, 4) != 1 { return Err("a track with more than one sample description".into()); }
    let fourcc: [u8; 4] = stsd[12..16].try_into().unwrap();

    let p = &stbl.child(b"stts").ok_or("no stts")?.body;
    let mut durs = Vec::new();
    for i in 0..be32(p, 4) as usize {
        let (n, d) = (be32(p, 8 + 8 * i), be32(p, 12 + 8 * i));
        durs.extend(std::iter::repeat_n(d, n as usize));
    }
    let p = &stbl.child(b"stsz").ok_or("no stsz")?.body;
    let (fixed, n) = (be32(p, 4), be32(p, 8) as usize);
    let sizes: Vec<u32> = if fixed != 0 { vec![fixed; n] } else { (0..n).map(|i| be32(p, 12 + 4 * i)).collect() };
    let chunks: Vec<u64> = if let Some(c) = stbl.child(b"stco") {
        (0..be32(&c.body, 4) as usize).map(|i| be32(&c.body, 8 + 4 * i) as u64).collect()
    } else {
        let c = stbl.child(b"co64").ok_or("no chunk offsets")?;
        (0..be32(&c.body, 4) as usize).map(|i| be64(&c.body, 8 + 8 * i)).collect()
    };
    let p = &stbl.child(b"stsc").ok_or("no stsc")?.body;
    let entries: Vec<(u32, u32)> = (0..be32(p, 4) as usize).map(|i| (be32(p, 8 + 12 * i), be32(p, 12 + 12 * i))).collect();
    let mut offs = Vec::with_capacity(n);
    for (e, &(first, per)) in entries.iter().enumerate() {
        let last = entries.get(e + 1).map(|x| x.0 - 1).unwrap_or(chunks.len() as u32);
        for ch in first..=last {
            let mut off = *chunks.get(ch as usize - 1).ok_or("a chunk past the offsets")?;
            for _ in 0..per {
                if offs.len() == n { break; }
                offs.push(off);
                off += sizes[offs.len() - 1] as u64;
            }
        }
    }
    if durs.len() != n || offs.len() != n { return Err("sample tables that don't add up".into()); }
    let sync: Option<std::collections::HashSet<u32>> = stbl.child(b"stss").map(|s| (0..be32(&s.body, 4) as usize).map(|i| be32(&s.body, 8 + 4 * i) - 1).collect());
    let mut dts = 0u64;
    let samples = (0..n).map(|i| {
        let s = Sample { off: offs[i], size: sizes[i], dur: durs[i], dts, sync: sync.as_ref().is_none_or(|x| x.contains(&(i as u32))) };
        dts += durs[i] as u64;
        s
    }).collect();

    let mut media_time = 0;
    if let Some(el) = trak.path(&[b"edts", b"elst"]) {
        let (v, p) = full(&el.body);
        let count = be32(p, 0);
        let mt = if v == 1 { be64(p, 12) as i64 } else { be32(p, 8) as i32 as i64 };
        let rate = be32(p, if v == 1 { 20 } else { 12 });
        if count != 1 || mt < 0 || rate != 0x10000 {
            return Err(format!("an edit list of {} edits, media time {mt}: not handled", count));
        }
        media_time = mt;
    }
    Ok(Track { handler, fourcc, timescale, samples, media_time })
}

// ---------------------------------------------------------------------------------------------------- protobuf

/// The top-level fields of a protobuf message: (number, the field's bytes with its key). None when it isn't one
fn pb_fields(b: &[u8]) -> Option<Vec<(u64, &[u8])>> {
    let varint = |at: &mut usize| -> Option<u64> {
        let mut v = 0u64;
        for shift in (0..64).step_by(7) {
            let x = *b.get(*at)?;
            *at += 1;
            v |= ((x & 0x7f) as u64) << shift;
            if x & 0x80 == 0 { return Some(v); }
        }
        None
    };
    let mut out = Vec::new();
    let mut at = 0;
    while at < b.len() {
        let start = at;
        let key = varint(&mut at)?;
        let (num, wire) = (key >> 3, key & 7);
        if num == 0 { return None; }
        match wire {
            0 => { varint(&mut at)?; }
            1 => at += 8,
            5 => at += 4,
            2 => { let n = varint(&mut at)? as usize; at = at.checked_add(n)?; }
            _ => return None,
        }
        if at > b.len() { return None; }
        out.push((num, &b[start..at]));
    }
    Some(out)
}

// ---------------------------------------------------------------------------------------------------- the cut

/// Which camera's way a cut takes
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Adapter {
    /// DJI's metadata track (djmd): its first sample's clip data is put in front of the new first one
    Dji,
    /// Not a camera known here: the samples as they are, and our key. Kept only when verify() reads it as the original
    Unknown,
}

impl Adapter {
    pub fn name(self) -> &'static str { match self { Adapter::Dji => "DJI", Adapter::Unknown => "unknown" } }
}

/// What was kept
#[derive(Clone, Debug, serde::Serialize)]
pub struct Plan {
    /// The original's video frames kept: first..first+frames
    pub first_frame: usize,
    pub frames: usize,
    pub source_frames: usize,
    /// Its time in the original, s
    pub start: f64,
    pub end: f64,
    pub source_duration: f64,
    pub adapter: &'static str,
    /// For each track: the original's samples kept (first, count) and the ones written otherwise
    #[serde(skip)]
    tracks: Vec<TrackPlan>,
}

#[derive(Clone, Debug)]
struct TrackPlan { first: usize, count: usize, replaced: Vec<(usize, Replacement)> }

#[derive(Clone, Debug)]
enum Replacement { HeaderInFront(usize), Timecode(u32) }

fn video_track(tracks: &[Track]) -> Result<usize, String> {
    tracks.iter().enumerate()
        .filter(|(_, t)| &t.handler == b"vide" && matches!(&t.fourcc, b"hvc1" | b"hev1" | b"avc1" | b"avc3"))
        .max_by_key(|(_, t)| t.samples.len()).map(|(i, _)| i).ok_or_else(|| "no H.264/HEVC video track".into())
}

fn adapter_of(tracks: &[Track]) -> Adapter {
    if tracks.iter().any(|t| &t.fourcc == b"djmd") { Adapter::Dji } else { Adapter::Unknown }
}

fn read_sample(f: &mut File, s: &Sample) -> Result<Vec<u8>, String> {
    let mut b = vec![0u8; s.size as usize];
    f.seek(SeekFrom::Start(s.off)).map_err(|e| e.to_string())?;
    f.read_exact(&mut b).map_err(|e| e.to_string())?;
    Ok(b)
}

/// The original's part from `from` (s, back to the keyframe at or before it) to `to` (s, the first frame at or after
/// it isn't kept; None: to the end), written to `dst`. `key` is what our key says besides the cut itself (how the
/// player read the original): merged into the JSON written. Nothing is verified here: see verify()
pub fn cut(src: &Path, dst: &Path, from: f64, to: Option<f64>, key: serde_json::Value) -> Result<Plan, String> {
    let mut f = File::open(src).map_err(|e| format!("{}: {e}", src.display()))?;
    let (top, mut moov) = read_moov(&mut f)?;
    let mvhd_ts = timescale_of(&moov.child(b"mvhd").ok_or("no mvhd")?.body);
    let traks: Vec<usize> = moov.kids.as_ref().unwrap().iter().enumerate().filter(|(_, n)| &n.kind == b"trak").map(|(i, _)| i).collect();
    let tracks: Vec<Track> = traks.iter().map(|&i| read_track(&moov.kids.as_ref().unwrap()[i])).collect::<Result<_, _>>()?;
    let adapter = adapter_of(&tracks);

    // The video's frames kept
    let vi = video_track(&tracks)?;
    let video = &tracks[vi];
    if video.media_time != 0 { return Err("a video edit list with a media time".into()); }
    let vts = video.timescale as f64;
    let want = (from.max(0.0) * vts).round() as u64;
    let k0 = video.samples.iter().enumerate().filter(|(_, s)| s.sync && s.dts <= want).map(|(i, _)| i).last().unwrap_or(0);
    let k1 = match to {
        Some(t) => { let w = (t * vts).round() as u64; video.samples.iter().position(|s| s.dts >= w).unwrap_or(video.samples.len()) }
        None => video.samples.len(),
    };
    if k1 <= k0 { return Err("nothing left to keep".into()); }
    if !video.samples[k0].sync { return Err("the video doesn't start on a keyframe".into()); }
    let end_of = |t: &Track| t.samples.last().map(|s| s.dts + s.dur as u64).unwrap_or(0);
    let t0 = video.samples[k0].dts as f64 / vts;
    let t1 = video.samples.get(k1).map(|s| s.dts).unwrap_or_else(|| end_of(video)) as f64 / vts;
    let span = t1 - t0;

    // Each track's samples kept, and what's written otherwise
    let mut plans = Vec::new();
    for (n, t) in tracks.iter().enumerate() {
        let ts = t.timescale as f64;
        let (lo, hi) = ((t0 * ts).round() as i64 + t.media_time, (t1 * ts).round() as i64 + t.media_time);
        let mut plan = TrackPlan { first: 0, count: 0, replaced: Vec::new() };
        if &t.fourcc == b"tmcd" {
            if t.samples.len() != 1 || t.samples[0].size != 4 { return Err("a timecode track that isn't one 4-byte sample".into()); }
            let stsd = &moov.kids.as_ref().unwrap()[traks[n]].path(&[b"mdia", b"minf", b"stbl", b"stsd"]).ok_or("no tmcd description")?.body;
            // tmcd: (entry) size, type, reserved 6, dref 2, reserved 4, flags 4, timescale, frame duration
            let (tc_ts, tc_dur) = (be32(stsd, 8 + 24) as f64, be32(stsd, 8 + 28) as f64);
            let old = u32::from_be_bytes(read_sample(&mut f, &t.samples[0])?.try_into().unwrap());
            let moved = (t0 * tc_ts / tc_dur).round() as u32;
            plan.count = 1;
            plan.replaced.push((0, Replacement::Timecode(old.wrapping_add(moved))));
        } else {
            let keep = |s: &Sample| if &t.handler == b"soun" {
                (s.dts + s.dur as u64) as i64 > lo && (s.dts as i64) < hi
            } else {
                s.dts as i64 >= lo && (s.dts as i64) < hi
            };
            let first = t.samples.iter().position(keep);
            if let Some(first) = first {
                let count = t.samples[first..].iter().take_while(|s| keep(s)).count();
                plan.first = first;
                plan.count = count;
                if adapter == Adapter::Dji && first > 0 && matches!(&t.fourcc, b"djmd" | b"dbgi") {
                    plan.replaced.push((first, Replacement::HeaderInFront(0)));
                }
            }
        }
        plans.push(plan);
    }

    // The replaced samples' bytes
    let mut replaced: std::collections::HashMap<(usize, usize), Vec<u8>> = Default::default();
    let mut carried = Vec::new();
    for (ti, (t, p)) in tracks.iter().zip(&plans).enumerate() {
        for (si, r) in &p.replaced {
            let bytes = match r {
                Replacement::Timecode(v) => v.to_be_bytes().to_vec(),
                Replacement::HeaderInFront(from_i) => {
                    let first = read_sample(&mut f, &t.samples[*from_i])?;
                    let new = read_sample(&mut f, &t.samples[*si])?;
                    match (pb_fields(&first), pb_fields(&new)) {
                        (Some(a), Some(b)) => {
                            let have: Vec<u64> = b.iter().map(|x| x.0).collect();
                            let head: Vec<u8> = a.iter().filter(|x| !have.contains(&x.0)).flat_map(|x| x.1.iter().copied()).collect();
                            let nums: Vec<u64> = a.iter().map(|x| x.0).filter(|n| !have.contains(n)).collect();
                            carried.push(serde_json::json!({ "track": String::from_utf8_lossy(&t.fourcc), "fields": nums, "bytes": head.len() }));
                            [head, new].concat()
                        }
                        _ => new, // not protobuf: left as it is
                    }
                }
            };
            replaced.insert((ti, *si), bytes);
        }
    }

    // The mdat: the kept samples in the original's byte order (its interleaving)
    let mut order: Vec<(u64, usize, usize)> = Vec::new();
    for (ti, (t, p)) in tracks.iter().zip(&plans).enumerate() {
        for si in p.first..p.first + p.count { order.push((t.samples[si].off, ti, si)); }
    }
    order.sort();
    let size_of = |ti: usize, si: usize| replaced.get(&(ti, si)).map(|b| b.len() as u64).unwrap_or(tracks[ti].samples[si].size as u64);
    let total: u64 = order.iter().map(|&(_, ti, si)| size_of(ti, si)).sum();
    // Before the mdat: the top-level boxes but the moov and mdats, as they were (ftyp, free, uuid...)
    let mut pre = Vec::new();
    for &(at, _, size, kind) in &top {
        if matches!(&kind, b"moov" | b"mdat") { continue; }
        let mut b = vec![0u8; size as usize];
        f.seek(SeekFrom::Start(at)).map_err(|e| e.to_string())?;
        f.read_exact(&mut b).map_err(|e| e.to_string())?;
        pre.extend(b);
    }
    let big = total + 8 > u32::MAX as u64;
    let mdat_head: Vec<u8> = if big {
        [1u32.to_be_bytes().as_slice(), b"mdat", (total + 16).to_be_bytes().as_slice()].concat()
    } else {
        [(total as u32 + 8).to_be_bytes().as_slice(), b"mdat"].concat()
    };
    let mut new_off: std::collections::HashMap<(usize, usize), u64> = Default::default();
    let mut pos = pre.len() as u64 + mdat_head.len() as u64;
    for &(_, ti, si) in &order { new_off.insert((ti, si), pos); pos += size_of(ti, si); }
    let wide = pos > u32::MAX as u64;

    // The moov written anew: durations, times, tables, edit lists
    let shift_s = t0.floor() as u64;
    let span_movie = (span * mvhd_ts as f64).round() as u64;
    let mvhd = moov.child_mut(b"mvhd").unwrap();
    mvhd.body = with_times(&mvhd.body, shift_s, Some(span_movie), 0);
    for (n, &ni) in traks.iter().enumerate() {
        let (t, p) = (&tracks[n], &plans[n]);
        let trak = &mut moov.kids.as_mut().unwrap()[ni];
        let kept = &t.samples[p.first..p.first + p.count];
        let tc_span = (span * t.timescale as f64).round() as u32;
        let durs: Vec<u32> = kept.iter().map(|s| if &t.fourcc == b"tmcd" { tc_span } else { s.dur }).collect();
        let sizes: Vec<u32> = (p.first..p.first + p.count).map(|si| size_of(n, si) as u32).collect();
        let offs: Vec<u64> = (p.first..p.first + p.count).map(|si| new_off[&(n, si)]).collect();
        let mut stts: Vec<(u32, u32)> = Vec::new();
        for &d in &durs {
            match stts.last_mut() { Some(e) if e.1 == d => e.0 += 1, _ => stts.push((1, d)) }
        }
        let stbl = trak.path_mut(&[b"mdia", b"minf", b"stbl"]).unwrap();
        for k in stbl.kids.as_mut().unwrap().iter_mut() {
            let mut body = vec![0u8; 4];
            match &k.kind {
                b"stts" => { body.extend((stts.len() as u32).to_be_bytes()); for e in &stts { body.extend(e.0.to_be_bytes()); body.extend(e.1.to_be_bytes()); } }
                b"stss" => {
                    let idx: Vec<u32> = kept.iter().enumerate().filter(|(_, s)| s.sync).map(|(i, _)| i as u32 + 1).collect();
                    body.extend((idx.len() as u32).to_be_bytes());
                    for i in idx { body.extend(i.to_be_bytes()); }
                }
                b"stsz" => { body.extend(0u32.to_be_bytes()); body.extend((sizes.len() as u32).to_be_bytes()); for s in &sizes { body.extend(s.to_be_bytes()); } }
                // One sample per chunk: the samples' order in the mdat is the original's, not grouped per track
                b"stsc" => { body.extend(1u32.to_be_bytes()); for v in [1u32, 1, 1] { body.extend(v.to_be_bytes()); } }
                b"stco" | b"co64" => {
                    k.kind = if wide { *b"co64" } else { *b"stco" };
                    body.extend((offs.len() as u32).to_be_bytes());
                    for &o in &offs { if wide { body.extend(o.to_be_bytes()) } else { body.extend((o as u32).to_be_bytes()) } }
                }
                // sdtp: one byte per sample after its version and flags
                b"sdtp" => { body = k.body[..4].to_vec(); body.extend_from_slice(&k.body[4 + p.first..4 + p.first + p.count]); }
                // sbgp: the samples' groups (AAC's roll distance...), runs of (count, group): sliced like the samples.
                // Their descriptions (sgpd) stay as they are
                b"sbgp" => body = slice_sbgp(&k.body, p.first, p.count)?,
                _ => continue,
            }
            k.body = body;
        }
        let media_dur: u64 = durs.iter().map(|&d| d as u64).sum();
        let mdhd = trak.path_mut(&[b"mdia", b"mdhd"]).unwrap();
        mdhd.body = with_times(&mdhd.body, shift_s, Some(media_dur), 0);
        // The edit list: from the media time of the first moment kept (audio: inside its first packet), as long as the
        // span, or as the media there is after it (the audio of a clip's end stops a little before the video: an edit
        // past its media makes AVFoundation take the file for unplayable)
        let media_time = if p.count > 0 && &t.handler == b"soun" {
            ((t0 * t.timescale as f64).round() as i64 + t.media_time - kept[0].dts as i64).max(0)
        } else { 0 };
        let has = (media_dur as i64 - media_time).max(0) as f64 / t.timescale as f64;
        let seg = span_movie.min((has * mvhd_ts as f64).round() as u64);
        let tkhd = trak.child_mut(b"tkhd").unwrap();
        tkhd.body = with_times(&tkhd.body, shift_s, Some(seg), 1);
        let elst = |v: u8| -> Vec<u8> {
            let mut b = vec![v, 0, 0, 0];
            b.extend(1u32.to_be_bytes());
            if v == 1 { b.extend(seg.to_be_bytes()); b.extend(media_time.to_be_bytes()); }
            else { b.extend((seg as u32).to_be_bytes()); b.extend((media_time as i32).to_be_bytes()); }
            b.extend(0x10000u32.to_be_bytes());
            b
        };
        if let Some(el) = trak.path_mut(&[b"edts", b"elst"]) {
            el.body = elst(el.body[0]);
        } else if media_time != 0 {
            let edts = Node { kind: *b"edts", body: Vec::new(), kids: Some(vec![Node::leaf(b"elst", elst(0))]) };
            let at = trak.kids.as_ref().unwrap().iter().position(|k| &k.kind == b"tkhd").map(|i| i + 1).unwrap_or(0);
            trak.kids.as_mut().unwrap().insert(at, edts);
        }
    }

    // Our key: the cut, then what the caller adds (how the original was read)
    let plan = Plan {
        first_frame: k0, frames: k1 - k0, source_frames: video.samples.len(), start: t0, end: t1,
        source_duration: end_of(video) as f64 / vts, adapter: adapter.name(), tracks: plans,
    };
    let previous = moov.child(b"meta").and_then(|m| mp4meta::key_in_meta(&m.bytes(), CUT_KEY))
        .and_then(|v| serde_json::from_str::<serde_json::Value>(&v).ok());
    let mut value = serde_json::json!({
        "version": 1,
        "source": src.file_name().map(|n| n.to_string_lossy().to_string()),
        "first_frame": plan.first_frame, "frames": plan.frames, "source_frames": plan.source_frames,
        "start": round6(t0), "end": round6(t1), "source_duration": round6(plan.source_duration),
        "adapter": plan.adapter, "carried": carried,
    });
    // A cut of a cut: where it is in the very first original
    let (origin, origin_start) = match &previous {
        Some(p) => (p["origin"].as_str().or(p["source"].as_str()).map(String::from), p["origin_start"].as_f64().or(p["start"].as_f64()).unwrap_or(0.0) + t0),
        None => (value["source"].as_str().map(String::from), t0),
    };
    value["origin"] = serde_json::json!(origin);
    value["origin_start"] = serde_json::json!(round6(origin_start));
    if let (Some(o), Some(k)) = (value.as_object_mut(), key.as_object()) {
        for (a, b) in k { o.insert(a.clone(), b.clone()); }
    }
    let json = serde_json::to_string(&value).unwrap();
    let old_meta = moov.child(b"meta").map(|m| m.bytes());
    let meta = mp4meta::merged_meta(old_meta.as_deref(), &[(CUT_KEY, &json)])?;
    let meta_node = parse_boxes(&meta)?.remove(0);
    let kids = moov.kids.as_mut().unwrap();
    match kids.iter().position(|k| &k.kind == b"meta") { Some(i) => kids[i] = meta_node, None => kids.push(meta_node) }

    // Written: what came before the mdat, the samples, the moov
    let mut out = BufWriter::with_capacity(1 << 22, File::create(dst).map_err(|e| format!("{}: {e}", dst.display()))?);
    let werr = |e: std::io::Error| format!("{}: {e}", dst.display());
    out.write_all(&pre).map_err(werr)?;
    out.write_all(&mdat_head).map_err(werr)?;
    let mut i = 0;
    while i < order.len() {
        let (_, ti, si) = order[i];
        if let Some(b) = replaced.get(&(ti, si)) { out.write_all(b).map_err(werr)?; i += 1; continue; }
        // Samples next to each other in the original are copied as one read
        let start = tracks[ti].samples[si].off;
        let mut end = start + tracks[ti].samples[si].size as u64;
        let mut j = i + 1;
        while j < order.len() && !replaced.contains_key(&(order[j].1, order[j].2)) && order[j].0 == end && end - start < 64 << 20 {
            end += tracks[order[j].1].samples[order[j].2].size as u64;
            j += 1;
        }
        f.seek(SeekFrom::Start(start)).map_err(|e| e.to_string())?;
        let n = std::io::copy(&mut (&mut f).take(end - start), &mut out).map_err(werr)?;
        if n != end - start { return Err(format!("{}: shorter than its tables say", src.display())); }
        i = j;
    }
    out.write_all(&moov.bytes()).map_err(werr)?;
    out.flush().map_err(werr)?;
    Ok(plan)
}

/// An sbgp's runs of (samples, group description) for the samples first..first+count
fn slice_sbgp(b: &[u8], first: usize, count: usize) -> Result<Vec<u8>, String> {
    let head = if b[0] == 1 { 12 } else { 8 };
    if b.len() < head + 4 { return Err("a broken sbgp".into()); }
    let n = be32(b, head) as usize;
    if b.len() < head + 4 + 8 * n { return Err("a broken sbgp".into()); }
    let mut runs: Vec<(u32, u32)> = Vec::new();
    let (mut at, end) = (0usize, first + count);
    for i in 0..n {
        let (c, g) = (be32(b, head + 4 + 8 * i) as usize, be32(b, head + 8 + 8 * i));
        let (lo, hi) = (at.max(first), (at + c).min(end));
        if hi > lo {
            match runs.last_mut() { Some(r) if r.1 == g => r.0 += (hi - lo) as u32, _ => runs.push(((hi - lo) as u32, g)) }
        }
        at += c;
    }
    let mut out = b[..head].to_vec();
    out.extend((runs.len() as u32).to_be_bytes());
    for (c, g) in runs { out.extend(c.to_be_bytes()); out.extend(g.to_be_bytes()); }
    Ok(out)
}

fn round6(x: f64) -> f64 { (x * 1e6).round() / 1e6 }

/// mvhd/mdhd/tkhd with their creation and modification times moved on by `secs`, and a duration. `kind` 0: mvhd or
/// mdhd (times, timescale, duration), 1: tkhd (times, track id, reserved, duration)
fn with_times(body: &[u8], secs: u64, duration: Option<u64>, kind: u8) -> Vec<u8> {
    let mut b = body.to_vec();
    let v = b[0];
    if v == 1 {
        for at in [4, 12] { let t = be64(&b, at); if t != 0 { b[at..at + 8].copy_from_slice(&(t + secs).to_be_bytes()); } }
        if let Some(d) = duration { let at = if kind == 0 { 24 } else { 28 }; b[at..at + 8].copy_from_slice(&d.to_be_bytes()); }
    } else {
        for at in [4, 8] { let t = be32(&b, at); if t != 0 { b[at..at + 4].copy_from_slice(&(t.wrapping_add(secs as u32)).to_be_bytes()); } }
        if let Some(d) = duration { let at = if kind == 0 { 16 } else { 20 }; b[at..at + 4].copy_from_slice(&(d.min(u32::MAX as u64) as u32).to_be_bytes()); }
    }
    b
}

// ---------------------------------------------------------------------------------------------------- reading it back

/// How the player reads a clip: what verify() compares, and what the cut's key keeps of the original
#[derive(Clone, Debug, PartialEq, serde::Serialize)]
pub struct Reading {
    /// gyroflow-probe's answer: "ok" or why it isn't stabilized
    pub stabilization: String,
    pub log_curve: Option<String>,
    pub camera: String,
    /// Gyroflow's lens profile identifier ("" when none)
    pub lens: String,
}

pub fn reading(path: &Path) -> Reading {
    let p = path.to_string_lossy();
    let stabilization = match probe::probe(&p) { Ok(_) => "ok".into(), Err(e) => e };
    let (camera, lens) = probe::camera_and_lens(&p).unwrap_or_default();
    Reading { stabilization, log_curve: probe::log_curve(&p).map(String::from), camera, lens }
}

/// The motion data as telemetry-parser reads it: (time, values), in the file's order
fn motion(path: &Path) -> Result<Vec<(f64, [f64; 4])>, String> {
    let mut file = File::open(path).map_err(|e| e.to_string())?;
    let size = file.metadata().map_err(|e| e.to_string())?.len() as usize;
    let input = Input::from_stream_with_options(&mut file, size, path, |_| (), Arc::new(AtomicBool::new(false)), InputOptions::default()).map_err(|e| e.to_string())?;
    let mut out = Vec::new();
    for s in input.samples.as_deref().unwrap_or_default() {
        let Some(m) = s.tag_map.as_ref() else { continue };
        if let Some(TagValue::Vec_TimeQuaternion_f64(v)) = m.get(&GroupId::Quaternion).and_then(|g| g.get(&TagId::Data)).map(|t| &t.value) {
            out.extend(v.get().iter().map(|q| (q.t, [q.v.w, q.v.x, q.v.y, q.v.z])));
        }
        if let Some(TagValue::Vec_TimeVector3_f64(v)) = m.get(&GroupId::Gyroscope).and_then(|g| g.get(&TagId::Data)).map(|t| &t.value) {
            out.extend(v.get().iter().map(|g| (g.t, [g.x, g.y, g.z, f64::NAN])));
        }
    }
    Ok(out)
}

/// What verify() found
#[derive(Clone, Debug, serde::Serialize)]
pub struct Verified {
    pub ok: bool,
    pub problems: Vec<String>,
    pub notes: Vec<String>,
    pub original: Reading,
    pub cut: Reading,
}

/// Reads the cut back and compares it with the original: every sample (the replaced ones: what they hold), the
/// timecode, and the cut read as the player reads the original (probe.rs, the motion data). `original` is the
/// original's reading, made before the cut (it goes in the key). `as_played`: false for what the player doesn't play
/// (a proxy: telemetry-parser doesn't take DJI's .LRF), whose samples alone are compared
pub fn verify(src: &Path, dst: &Path, plan: &Plan, original: Reading, as_played: bool) -> Result<Verified, String> {
    let mut problems = Vec::new();
    let mut notes = Vec::new();
    let mut fs = File::open(src).map_err(|e| e.to_string())?;
    let mut fd = File::open(dst).map_err(|e| e.to_string())?;
    let (_, ms) = read_moov(&mut fs)?;
    let (_, md) = read_moov(&mut fd)?;
    let ts: Vec<Track> = ms.kids.as_ref().unwrap().iter().filter(|n| &n.kind == b"trak").map(read_track).collect::<Result<_, _>>()?;
    let td: Vec<Track> = md.kids.as_ref().unwrap().iter().filter(|n| &n.kind == b"trak").map(read_track).collect::<Result<_, _>>()?;
    if ts.len() != td.len() { problems.push(format!("{} tracks, the original {}", td.len(), ts.len())); }
    for (i, ((a, b), p)) in ts.iter().zip(&td).zip(&plan.tracks).enumerate() {
        let name = String::from_utf8_lossy(&a.fourcc).to_string();
        if a.fourcc != b.fourcc || a.timescale != b.timescale { problems.push(format!("track {i} ({name}) isn't the same kind")); continue; }
        if b.samples.len() != p.count { problems.push(format!("{name}: {} samples, {} meant", b.samples.len(), p.count)); continue; }
        for (j, sd) in b.samples.iter().enumerate() {
            let si = p.first + j;
            let (x, y) = (read_sample(&mut fs, &a.samples[si])?, read_sample(&mut fd, sd)?);
            let good = match p.replaced.iter().find(|r| r.0 == si).map(|r| &r.1) {
                None => x == y && (a.samples[si].sync == sd.sync) && (&a.fourcc == b"tmcd" || a.samples[si].dur == sd.dur || j + 1 == b.samples.len()),
                Some(Replacement::HeaderInFront(_)) => y.ends_with(&x),
                Some(Replacement::Timecode(v)) => y == v.to_be_bytes(),
            };
            if !good { problems.push(format!("{name}: sample {j} isn't the original's {si}")); break; }
        }
    }
    // A file telemetry-parser doesn't open at all (DJI's .LRF) reads the same as anything: its samples alone count
    let unreadable = original.stabilization.starts_with("Unsupported file format");
    if !as_played || unreadable {
        notes.push(if unreadable { "not a format the player's telemetry reader opens: its samples compared only" } else { "its samples compared only (not a file the player plays)" }.into());
        return Ok(Verified { ok: problems.is_empty(), problems, notes, cut: original.clone(), original });
    }
    let cut = reading(dst);
    if cut != original {
        problems.push(format!("read differently: {} (the original: {})", serde_json::to_string(&cut).unwrap(), serde_json::to_string(&original).unwrap()));
    }
    // The motion data: the cut's, a run of the original's, at times moved by the same amount
    let (a, b) = (motion(src)?, motion(dst)?);
    if a.is_empty() && b.is_empty() {
        notes.push("no motion data in either".into());
    } else if b.is_empty() {
        problems.push("no motion data in the cut".into());
    } else {
        let same = |x: &[f64; 4], y: &[f64; 4]| x.iter().zip(y).all(|(p, q)| p == q || (p.is_nan() && q.is_nan()));
        let found = (0..a.len()).filter(|&k| same(&a[k].1, &b[0].1)).find(|&k| k + b.len() <= a.len()
            && b.iter().enumerate().all(|(j, y)| same(&a[k + j].1, &y.1) && ((a[k + j].0 - a[k].0) - (y.0 - b[0].0)).abs() < 1e-3));
        match found {
            Some(k) => notes.push(format!("motion data: {} readings, the original's {}..{}, the same values at the same moments", b.len(), k, k + b.len() - 1)),
            None => problems.push(format!("motion data: the cut's {} readings aren't a run of the original's {}", b.len(), a.len())),
        }
    }
    Ok(Verified { ok: problems.is_empty(), problems, notes, original, cut })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn protobuf_fields() {
        // field 1 (bytes "ab"), field 3 (varint 150), field 2 (fixed32)
        let b = [0x0a, 2, b'a', b'b', 0x18, 0x96, 0x01, 0x15, 1, 2, 3, 4];
        let f = pb_fields(&b).unwrap();
        assert_eq!(f.iter().map(|x| x.0).collect::<Vec<_>>(), vec![1, 3, 2]);
        assert_eq!(f[1].1, &[0x18, 0x96, 0x01]);
        assert!(pb_fields(&[0x0a, 5, 1]).is_none());
        assert!(pb_fields(b"DJIMETA1").is_none() || pb_fields(b"DJIMETA1").unwrap().iter().all(|x| x.0 > 0));
    }

    #[test]
    fn sample_groups_sliced() {
        // version 0, 'roll', runs (1 sample, group 1), (9, 0), (5, 2)
        let mut b = vec![0, 0, 0, 0];
        b.extend_from_slice(b"roll");
        b.extend(3u32.to_be_bytes());
        for (c, g) in [(1u32, 1u32), (9, 0), (5, 2)] { b.extend(c.to_be_bytes()); b.extend(g.to_be_bytes()); }
        let s = slice_sbgp(&b, 8, 5).unwrap();
        assert_eq!(&s[4..8], b"roll");
        assert_eq!(be32(&s, 8), 2);
        assert_eq!((be32(&s, 12), be32(&s, 16), be32(&s, 20), be32(&s, 24)), (2, 0, 3, 2));
    }

    #[test]
    fn times_and_durations() {
        let mut mvhd = vec![0u8; 100];
        mvhd[4..8].copy_from_slice(&100u32.to_be_bytes());
        mvhd[8..12].copy_from_slice(&100u32.to_be_bytes());
        mvhd[12..16].copy_from_slice(&600u32.to_be_bytes());
        let b = with_times(&mvhd, 9, Some(1234), 0);
        assert_eq!((be32(&b, 4), be32(&b, 8), be32(&b, 12), be32(&b, 16)), (109, 109, 600, 1234));
    }

    /// A file ffmpeg makes (H.264, a keyframe every 30 frames, AAC, timecode), cut and read back: skipped without ffmpeg
    #[test]
    fn synthetic_cut() {
        let dir = std::env::temp_dir().join(format!("afp-cut-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let src = dir.join("src.mp4");
        let made = std::process::Command::new("ffmpeg").args(["-v", "error", "-y", "-f", "lavfi", "-i", "testsrc2=size=320x180:rate=30000/1001:duration=6",
            "-f", "lavfi", "-i", "sine=frequency=440:duration=6", "-c:v", "libx264", "-bf", "0", "-g", "30", "-c:a", "aac",
            "-timecode", "10:00:00;00", "-shortest"]).arg(&src).status();
        if !made.is_ok_and(|s| s.success()) { eprintln!("no ffmpeg with libx264: skipped"); return; }
        let dst = dir.join("cut.mp4");
        let plan = cut(&src, &dst, 2.5, Some(4.2), serde_json::json!({ "read_as": "test" })).unwrap();
        // Back to the keyframe at 2.002 s (frame 60); to the frame at or after 4.2 s (126)
        assert_eq!((plan.first_frame, plan.frames), (60, 66));
        assert_eq!(plan.adapter, "unknown");
        let v = verify(&src, &dst, &plan, reading(&src), true).unwrap();
        assert!(v.ok, "{:?}", v.problems);
        let mut f = File::open(&dst).unwrap();
        let (_, moov) = read_moov(&mut f).unwrap();
        let key = mp4meta::key_in_meta(&moov.child(b"meta").unwrap().bytes(), CUT_KEY).unwrap();
        let key: serde_json::Value = serde_json::from_str(&key).unwrap();
        assert_eq!((key["first_frame"].as_u64(), key["read_as"].as_str(), key["origin_start"].as_f64()), (Some(60), Some("test"), Some(2.002)));
        // A cut of the cut: where it is in the first original
        let dst2 = dir.join("cut2.mp4");
        let plan2 = cut(&dst, &dst2, 1.1, None, serde_json::json!({})).unwrap();
        assert_eq!(plan2.first_frame, 30);
        let v2 = verify(&dst, &dst2, &plan2, reading(&dst), true).unwrap();
        assert!(v2.ok, "{:?}", v2.problems);
        let (_, moov2) = read_moov(&mut File::open(&dst2).unwrap()).unwrap();
        let key2: serde_json::Value = serde_json::from_str(&mp4meta::key_in_meta(&moov2.child(b"meta").unwrap().bytes(), CUT_KEY).unwrap()).unwrap();
        assert_eq!((key2["origin"].as_str(), key2["origin_start"].as_f64()), (Some("src.mp4"), Some(3.003)));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn meta_keys_merged() {
        let base = mp4meta::merged_meta(None, &[("com.example.a", "1")]).unwrap();
        let both = mp4meta::merged_meta(Some(&base), &[(CUT_KEY, "{}")]).unwrap();
        assert_eq!(mp4meta::key_in_meta(&both, "com.example.a").as_deref(), Some("1"));
        assert_eq!(mp4meta::key_in_meta(&both, CUT_KEY).as_deref(), Some("{}"));
        let again = mp4meta::merged_meta(Some(&both), &[(CUT_KEY, "{\"x\":1}")]).unwrap();
        assert_eq!(mp4meta::key_in_meta(&again, CUT_KEY).as_deref(), Some("{\"x\":1}"));
        assert_eq!(mp4meta::key_in_meta(&again, "com.example.a").as_deref(), Some("1"));
    }
}
