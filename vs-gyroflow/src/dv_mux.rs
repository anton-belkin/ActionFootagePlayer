//! gyroflow-export's Dolby Vision exports: our RPU on every frame of VideoToolbox's HEVC in place of its own, and the
//! Dolby Vision configuration box (dvvC) the MOV needs for Apple to use it. VideoToolbox writes HLG as Dolby Vision 8.4
//! with an RPU of its own, but ffmpeg's MOV muxer writes no dvvC, and without one Apple ignores the RPU.
//!
//! The RPUs are a file as dovi_tool and afp-rpu write them (Annex B: each one after a 00 00 00 01 start code, the NAL
//! payload without its 7C 01 header), the same file the player attaches (--macos-dovi-rpu-file): frame N of the
//! source gets RPU N, the last one every frame after it, so a file of one RPU goes on all of them.

use std::fs::File;
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::Path;

/// The RPUs of a file, in frame order
pub fn read_rpu_file(path: &Path) -> Result<Vec<Vec<u8>>, String> {
    let data = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let mut rpus = Vec::new();
    let mut start = None;
    let mut i = 0;
    while i + 4 <= data.len() {
        if data[i..i + 4] == [0, 0, 0, 1] {
            if let Some(s) = start { if i > s { rpus.push(data[s..i].to_vec()); } }
            start = Some(i + 4);
            i += 4;
        } else {
            i += 1;
        }
    }
    if let Some(s) = start { if s < data.len() { rpus.push(data[s..].to_vec()); } }
    if rpus.is_empty() { return Err(format!("{}: no RPUs in it", path.display())); }
    Ok(rpus)
}

/// HEVC NAL unit types that matter here
const NAL_VPS: u8 = 32;
const NAL_AUD: u8 = 35;
const NAL_SEI_PREFIX: u8 = 39;
const NAL_UNSPEC62: u8 = 62; // Dolby Vision RPU
const NAL_UNSPEC63: u8 = 63; // Dolby Vision enhancement layer

/// Rewrites an Annex B HEVC stream access unit by access unit: drops the Dolby Vision NALs it has (types 62, 63) and
/// appends ours after each picture's slices, as Dolby Vision puts it. Fed in pieces of any size; `finish` writes the
/// last access unit.
pub struct RpuInjector {
    rpus: Vec<Vec<u8>>,
    first_frame: u64,
    buf: Vec<u8>,
    /// The NALs of the access unit being gathered, and whether it has a slice yet
    au: Vec<Vec<u8>>,
    au_has_vcl: bool,
    pub frames: u64,
}

impl RpuInjector {
    /// `first_frame`: the source frame the stream starts at (an export of a part)
    pub fn new(rpus: Vec<Vec<u8>>, first_frame: u64) -> Self {
        RpuInjector { rpus, first_frame, buf: Vec::new(), au: Vec::new(), au_has_vcl: false, frames: 0 }
    }

    pub fn feed(&mut self, data: &[u8], out: &mut impl Write) -> std::io::Result<()> {
        self.buf.extend_from_slice(data);
        // NALs whose end is known: up to the last start code in the buffer
        let starts = start_codes(&self.buf);
        if starts.len() < 2 { return Ok(()); }
        let last = *starts.last().unwrap();
        let complete: Vec<Vec<u8>> = starts.windows(2).map(|w| nal_between(&self.buf, w[0], w[1])).collect();
        self.buf.drain(..last.0);
        for nal in complete { self.nal(nal, out)?; }
        Ok(())
    }

    pub fn finish(&mut self, out: &mut impl Write) -> std::io::Result<()> {
        let starts = start_codes(&self.buf);
        if let Some(&(s, len)) = starts.first() {
            let nal = self.buf[s + len..].to_vec();
            self.buf.clear();
            if !nal.is_empty() { self.nal(nal, out)?; }
        }
        self.end_au(out)
    }

    fn nal(&mut self, nal: Vec<u8>, out: &mut impl Write) -> std::io::Result<()> {
        if nal.len() < 2 { return Ok(()); }
        let kind = (nal[0] >> 1) & 0x3f;
        let vcl = kind < 32;
        // A new access unit: a picture's first slice, or what can only come before its slices, after a picture
        let first_slice = vcl && nal.len() > 2 && nal[2] & 0x80 != 0;
        let prefix = (NAL_VPS..=NAL_AUD).contains(&kind) || kind == NAL_SEI_PREFIX || (41..=44).contains(&kind)
            || (48..=55).contains(&kind);
        if self.au_has_vcl && (first_slice || prefix) { self.end_au(out)?; }
        if kind == NAL_UNSPEC62 || kind == NAL_UNSPEC63 { return Ok(()); }
        if vcl { self.au_has_vcl = true; }
        self.au.push(nal);
        Ok(())
    }

    fn end_au(&mut self, out: &mut impl Write) -> std::io::Result<()> {
        if self.au.is_empty() { return Ok(()); }
        for nal in self.au.drain(..) {
            out.write_all(&[0, 0, 0, 1])?;
            out.write_all(&nal)?;
        }
        if self.au_has_vcl {
            let i = ((self.first_frame + self.frames) as usize).min(self.rpus.len() - 1);
            out.write_all(&[0, 0, 0, 1, 0x7c, 0x01])?;
            out.write_all(&self.rpus[i])?;
            self.frames += 1;
        }
        self.au_has_vcl = false;
        Ok(())
    }
}

/// The start codes in `buf`: (position, length 3 or 4)
fn start_codes(buf: &[u8]) -> Vec<(usize, usize)> {
    let mut found = Vec::new();
    let mut i = 0;
    while i + 3 <= buf.len() {
        if buf[i] == 0 && buf[i + 1] == 0 && buf[i + 2] == 1 {
            if i > 0 && buf[i - 1] == 0 { found.push((i - 1, 4)); } else { found.push((i, 3)); }
            i += 3;
        } else {
            i += 1;
        }
    }
    found
}

/// The NAL from one start code to the next, without the zero bytes that trail it
fn nal_between(buf: &[u8], a: (usize, usize), b: (usize, usize)) -> Vec<u8> {
    let mut end = b.0;
    while end > a.0 + a.1 && buf[end - 1] == 0 { end -= 1; }
    buf[a.0 + a.1..end].to_vec()
}

/// The Dolby Vision level for a size and frame rate: ffmpeg's table (libavcodec/dovi_rpuenc.c), maximum pixels per
/// second and width; 59.94 counts as 60
pub fn dv_level(w: u32, h: u32, fps: f64) -> u8 {
    const LEVELS: [(u64, u32); 13] = [
        (1280 * 720 * 24, 1280), (1280 * 720 * 30, 1280), (1920 * 1080 * 24, 1920), (1920 * 1080 * 30, 2560),
        (1920 * 1080 * 60, 3840), (3840 * 2160 * 24, 3840), (3840 * 2160 * 30, 3840), (3840 * 2160 * 48, 3840),
        (3840 * 2160 * 60, 3840), (3840 * 2160 * 120, 3840), (3840 * 2160 * 120, 7680), (7680 * 4320 * 60, 7680),
        (7680 * 4320 * 120, 7680),
    ];
    let pps = w as f64 * h as f64 * fps;
    for (i, (max_pps, max_w)) in LEVELS.iter().enumerate() {
        if pps <= *max_pps as f64 * 1.001 && w <= *max_w { return i as u8 + 1; }
    }
    13
}

/// The dvvC box for profile 8 with HLG compatibility (8.4): version 1.0, an RPU and a base layer, no enhancement layer
pub fn dvvc_box(level: u8) -> [u8; 32] {
    let mut b = [0u8; 32];
    b[0..4].copy_from_slice(&32u32.to_be_bytes());
    b[4..8].copy_from_slice(b"dvvC");
    b[8] = 1; // dv_version_major
    b[9] = 0;
    let bits: u16 = (8 << 9) | ((level as u16) << 3) | (1 << 2) | 1; // profile 8, level, rpu, (no el), bl
    b[10..12].copy_from_slice(&bits.to_be_bytes());
    b[12] = 4 << 4; // dv_bl_signal_compatibility_id 4: HLG
    b
}

const CONTAINERS: [&[u8; 4]; 5] = [b"moov", b"trak", b"mdia", b"minf", b"stbl"];

/// (position, size, type) of the boxes in buf[start..end]; 32-bit sizes only (the moov ffmpeg writes)
fn boxes(buf: &[u8], start: usize, end: usize) -> Result<Vec<(usize, usize, [u8; 4])>, String> {
    let mut found = Vec::new();
    let mut pos = start;
    while pos + 8 <= end {
        let size = u32::from_be_bytes(buf[pos..pos + 4].try_into().unwrap()) as usize;
        let kind: [u8; 4] = buf[pos + 4..pos + 8].try_into().unwrap();
        if size == 1 { return Err(format!("{} box with a 64-bit size in moov", String::from_utf8_lossy(&kind))); }
        let size = if size == 0 { end - pos } else { size };
        if size < 8 || pos + size > end { return Err("a broken box in moov".into()); }
        found.push((pos, size, kind));
        pos += size;
    }
    Ok(found)
}

/// The boxes from moov's children down to the hvc1 sample entry (moov itself not included)
fn hvc1_chain(moov: &[u8]) -> Result<Vec<(usize, usize, [u8; 4])>, String> {
    fn walk(buf: &[u8], start: usize, end: usize, chain: &mut Vec<(usize, usize, [u8; 4])>) -> Result<bool, String> {
        for b in boxes(buf, start, end)? {
            if CONTAINERS.contains(&&b.2) {
                chain.push(b);
                if walk(buf, b.0 + 8, b.0 + b.1, chain)? { return Ok(true); }
                chain.pop();
            } else if &b.2 == b"stsd" {
                // full box header and entry count, then the sample entries
                for e in boxes(buf, b.0 + 16, b.0 + b.1)? {
                    if &e.2 == b"hvc1" || &e.2 == b"hev1" {
                        chain.push(b);
                        chain.push(e);
                        return Ok(true);
                    }
                }
            }
        }
        Ok(false)
    }
    let mut chain = Vec::new();
    if walk(moov, 8, moov.len(), &mut chain)? { Ok(chain) } else { Err("no hvc1 sample entry in moov".into()) }
}

/// Writes the dvvC box into the MOV's hvc1 sample entry, in place: the MOV has its moov before mdat with a free box
/// after it (ffmpeg's -moov_size), which gives up the room, so nothing else in the file moves. Returns the level.
pub fn patch_dvvc(path: &Path, fps: f64) -> Result<u8, String> {
    let mut f = File::options().read(true).write(true).open(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let len = f.metadata().map_err(|e| e.to_string())?.len();
    // The top-level boxes: ftyp, (wide,) moov, free, mdat
    let (mut pos, mut moov_at, mut free_after) = (0u64, None, None);
    let mut last_end = None;
    while pos + 8 <= len {
        let mut head = [0u8; 16];
        f.seek(SeekFrom::Start(pos)).map_err(|e| e.to_string())?;
        f.read_exact(&mut head[..8]).map_err(|e| e.to_string())?;
        let mut size = u32::from_be_bytes(head[0..4].try_into().unwrap()) as u64;
        let kind: [u8; 4] = head[4..8].try_into().unwrap();
        if size == 1 {
            f.read_exact(&mut head[8..16]).map_err(|e| e.to_string())?;
            size = u64::from_be_bytes(head[8..16].try_into().unwrap());
        } else if size == 0 {
            size = len - pos;
        }
        if &kind == b"moov" { moov_at = Some((pos, size)); }
        if matches!(&kind, b"free" | b"skip") && last_end == Some(pos) && free_after.is_none() { free_after = Some((pos, size)); }
        if &kind == b"mdat" && moov_at.is_none() { return Err("moov after mdat (not written with -moov_size)".into()); }
        last_end = if &kind == b"moov" { Some(pos + size) } else { None };
        if size < 8 { return Err("a broken top-level box".into()); }
        pos += size;
    }
    let (moov_pos, moov_size) = moov_at.ok_or("no moov")?;
    let (free_pos, free_size) = free_after.ok_or("no free box after moov to make room for dvvC (ffmpeg -moov_size)")?;
    if free_size < 32 + 8 { return Err("too little room after moov for dvvC".into()); }

    let mut moov = vec![0u8; moov_size as usize];
    f.seek(SeekFrom::Start(moov_pos)).map_err(|e| e.to_string())?;
    f.read_exact(&mut moov).map_err(|e| e.to_string())?;
    let chain = hvc1_chain(&moov)?;
    let hvc1 = *chain.last().unwrap();
    // The sample entry's fields (78 bytes after its header: width and height at 24), then its boxes
    let w = u16::from_be_bytes(moov[hvc1.0 + 8 + 24..hvc1.0 + 8 + 26].try_into().unwrap()) as u32;
    let h = u16::from_be_bytes(moov[hvc1.0 + 8 + 26..hvc1.0 + 8 + 28].try_into().unwrap()) as u32;
    let kids = boxes(&moov, hvc1.0 + 8 + 78, hvc1.0 + hvc1.1)?;
    if kids.iter().any(|k| &k.2 == b"dvvC" || &k.2 == b"dvcC") { return Err("it already has a Dolby Vision configuration".into()); }
    let hvcc = kids.iter().find(|k| &k.2 == b"hvcC").ok_or("no hvcC in hvc1")?;
    let level = dv_level(w, h, fps);
    let dvvc = dvvc_box(level);
    // Every box from moov down to hvc1 grows by the box
    for (p, s, _) in chain.iter().chain(std::iter::once(&(0usize, moov.len(), *b"moov"))) {
        moov[*p..*p + 4].copy_from_slice(&((*s + 32) as u32).to_be_bytes());
    }
    let at = hvcc.0 + hvcc.1;
    moov.splice(at..at, dvvc.iter().copied());
    // The free box, 32 bytes shorter, right after the longer moov
    let mut free_head = [0u8; 8];
    free_head[0..4].copy_from_slice(&((free_size - 32) as u32).to_be_bytes());
    free_head[4..8].copy_from_slice(b"free");
    f.seek(SeekFrom::Start(moov_pos)).map_err(|e| e.to_string())?;
    f.write_all(&moov).map_err(|e| e.to_string())?;
    debug_assert_eq!(moov_pos + moov.len() as u64, free_pos + 32);
    f.write_all(&free_head).map_err(|e| e.to_string())?;
    f.sync_all().map_err(|e| e.to_string())?;
    Ok(level)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn nal(kind: u8, first_slice: bool, body: &[u8]) -> Vec<u8> {
        let mut n = vec![kind << 1, 1];
        if kind < 32 { n.push(if first_slice { 0x80 } else { 0 }); }
        n.extend_from_slice(body);
        n
    }

    fn annexb(nals: &[Vec<u8>]) -> Vec<u8> {
        nals.iter().flat_map(|n| [0u8, 0, 0, 1].iter().copied().chain(n.iter().copied())).collect()
    }

    #[test]
    fn rpus_replace_the_streams_own_after_each_picture() {
        // VPS SPS PPS slice(first) slice RPU | slice(first) RPU, fed in odd pieces
        let stream = annexb(&[nal(32, false, b"v"), nal(33, false, b"s"), nal(34, false, b"p"), nal(19, true, b"a"),
                              nal(19, false, b"b"), nal(62, false, b"theirs"), nal(1, true, b"c"), nal(62, false, b"theirs2")]);
        let mut inj = RpuInjector::new(vec![b"R0".to_vec(), b"R1".to_vec(), b"R2".to_vec()], 1);
        let mut out = Vec::new();
        for piece in stream.chunks(5) { inj.feed(piece, &mut out).unwrap(); }
        inj.finish(&mut out).unwrap();
        let ours = |r: &[u8]| { let mut n = vec![0x7c, 0x01]; n.extend_from_slice(r); n };
        let want = annexb(&[nal(32, false, b"v"), nal(33, false, b"s"), nal(34, false, b"p"), nal(19, true, b"a"),
                            nal(19, false, b"b"), ours(b"R1"), nal(1, true, b"c"), ours(b"R2")]);
        assert_eq!(out, want);
        assert_eq!(inj.frames, 2);
    }

    #[test]
    fn one_rpu_goes_on_every_frame() {
        let stream = annexb(&[nal(19, true, b"a"), nal(1, true, b"b"), nal(1, true, b"c")]);
        let mut inj = RpuInjector::new(vec![b"R".to_vec()], 0);
        let mut out = Vec::new();
        inj.feed(&stream, &mut out).unwrap();
        inj.finish(&mut out).unwrap();
        assert_eq!(inj.frames, 3);
        assert_eq!(out.windows(7).filter(|w| *w == [0, 0, 0, 1, 0x7c, 0x01, b'R']).count(), 3);
    }

    #[test]
    fn levels_as_ffmpeg_has_them() {
        assert_eq!(dv_level(3840, 2160, 60000.0 / 1001.0), 9);
        assert_eq!(dv_level(3840, 2160, 30.0), 7);
        assert_eq!(dv_level(1920, 1080, 60000.0 / 1001.0), 5);
        assert_eq!(dv_level(3840, 2880, 60000.0 / 1001.0), 10);
        assert_eq!(dv_level(3840, 2160, 120.0), 10);
    }

    #[test]
    fn the_box_is_mp4boxs() {
        // MP4Box -add x.hevc:dvp=8.4 for 2160p59.94
        assert_eq!(&dvvc_box(9)[..13], &[0, 0, 0, 32, b'd', b'v', b'v', b'C', 1, 0, 0x10, 0x4d, 0x40]);
    }
}
