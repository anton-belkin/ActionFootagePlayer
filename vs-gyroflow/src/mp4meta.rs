// Text keys in an MP4's QuickTime metadata (docs/moments-and-workspaces.md, "The link inside"): the link a moment's
// or clip's preview keeps to its original (`ch.actionfootageplayer.link`), and the day Photos files it under
// (`com.apple.quicktime.creationdate`).
//
// They're `mdta` keys in `moov/meta` (hdlr mdta, keys, ilst), with no version and flags before its boxes: the
// place and the form Apple's own files have, and the only one AVFoundation (Quick Look, Photos) reads them from.
// ffmpeg reads them there too (mpv: `metadata/by-key/ch.actionfootageplayer.link`). ffmpeg's own
// `-movflags use_metadata_tags` writes them in `moov/udta/meta` instead, with version and flags, where AVFoundation
// takes them for iTunes items, and it then leaves out the cover art: so gyroflow-export writes the file with ffmpeg
// (the cover in `udta/meta/ilst/covr`) and adds its keys after, with write_keys().
//
// The player's own reader is mpv/scripts/workspace/link.lua; it reads both places, as read_keys() here does.

use std::fs::File;
use std::io::{ Read, Seek, SeekFrom, Write };
use std::path::{ Path, PathBuf };

/// The link's key
pub const LINK_KEY: &str = "ch.actionfootageplayer.link";
/// The day the file was recorded, with its time zone ("2026-10-22T15:28:42+0200")
pub const CREATION_DATE_KEY: &str = "com.apple.quicktime.creationdate";

fn be32(b: &[u8], at: usize) -> u32 { u32::from_be_bytes(b[at..at + 4].try_into().unwrap()) }
fn be64(b: &[u8], at: usize) -> u64 { u64::from_be_bytes(b[at..at + 8].try_into().unwrap()) }

/// A box in a buffer: where it starts, its header's length, its size (header included), its type
#[derive(Clone, Copy, Debug)]
struct Box_ { at: usize, header: usize, size: usize, kind: [u8; 4] }

impl Box_ {
    fn body(&self) -> std::ops::Range<usize> { self.at + self.header..self.at + self.size }
}

/// The boxes in b[from..to]
fn boxes(b: &[u8], from: usize, to: usize) -> Result<Vec<Box_>, String> {
    let mut out = Vec::new();
    let mut at = from;
    while at + 8 <= to {
        let (mut size, mut header) = (be32(b, at) as usize, 8);
        if size == 1 {
            if at + 16 > to { return Err("a broken box".into()); }
            size = be64(b, at + 8) as usize;
            header = 16;
        } else if size == 0 {
            size = to - at;
        }
        if size < header || at + size > to { return Err("a broken box".into()); }
        out.push(Box_ { at, header, size, kind: b[at + 4..at + 8].try_into().unwrap() });
        at += size;
    }
    Ok(out)
}

/// The top-level boxes of a file, read by their headers: (start, size, type)
fn top_level(f: &mut File) -> Result<Vec<(u64, u64, [u8; 4])>, String> {
    let len = f.metadata().map_err(|e| e.to_string())?.len();
    let mut out = Vec::new();
    let mut pos = 0u64;
    while pos + 8 <= len {
        let mut head = [0u8; 16];
        f.seek(SeekFrom::Start(pos)).map_err(|e| e.to_string())?;
        f.read_exact(&mut head[..8]).map_err(|e| e.to_string())?;
        let mut size = be32(&head, 0) as u64;
        if size == 1 {
            f.read_exact(&mut head[8..16]).map_err(|e| e.to_string())?;
            size = be64(&head, 8);
        } else if size == 0 {
            size = len - pos;
        }
        if size < 8 || pos + size > len { return Err("a broken top-level box".into()); }
        out.push((pos, size, head[4..8].try_into().unwrap()));
        pos += size;
    }
    Ok(out)
}

fn make_box(kind: &[u8; 4], body: &[u8]) -> Vec<u8> {
    let mut b = Vec::with_capacity(8 + body.len());
    b.extend_from_slice(&(8 + body.len() as u32).to_be_bytes());
    b.extend_from_slice(kind);
    b.extend_from_slice(body);
    b
}

/// `moov/meta` with these keys, as Apple writes it: no version and flags; hdlr mdta; keys; ilst, item N (1-based) a
/// `data` box of UTF-8 (type 1, locale 0)
fn meta_box(keys: &[(&str, &str)]) -> Vec<u8> {
    let mut hdlr = vec![0u8; 8];
    hdlr.extend_from_slice(b"mdta");
    hdlr.extend_from_slice(&[0u8; 13]); // reserved, and an empty name
    let mut key_list = vec![0u8; 4];
    key_list.extend_from_slice(&(keys.len() as u32).to_be_bytes());
    let mut items = Vec::new();
    for (i, (k, v)) in keys.iter().enumerate() {
        key_list.extend_from_slice(&(8 + k.len() as u32).to_be_bytes());
        key_list.extend_from_slice(b"mdta");
        key_list.extend_from_slice(k.as_bytes());
        let mut data = vec![0, 0, 0, 1, 0, 0, 0, 0];
        data.extend_from_slice(v.as_bytes());
        items.extend(make_box(&(i as u32 + 1).to_be_bytes(), &make_box(b"data", &data)));
    }
    let mut body = make_box(b"hdlr", &hdlr);
    body.extend(make_box(b"keys", &key_list));
    body.extend(make_box(b"ilst", &items));
    make_box(b"meta", &body)
}

/// A `moov/meta` (the box, header included) with these keys set in it: the mdta keys it had stay as they were, byte
/// for byte and whatever their type (a camera's own), ours replace the ones of the same name or come after them. With
/// no meta (None), a new one. For a cut (cut.rs), whose moov is written anew anyway
pub fn merged_meta(old: Option<&[u8]>, keys: &[(&str, &str)]) -> Result<Vec<u8>, String> {
    let Some(old) = old else { return Ok(meta_box(keys)) };
    let meta = *boxes(old, 0, old.len())?.first().ok_or("an empty meta")?;
    let kids = meta_children(old, &meta)?;
    if handler(old, &kids) != Some(*b"mdta") { return Err("a meta that isn't mdta".into()); }
    let r = meta.body();
    let versioned = !(r.len() >= 8 && &old[r.start + 4..r.start + 8] == b"hdlr");
    // The key entries as they are (size, namespace, name), and the items by their 1-based index
    let mut entries: Vec<Vec<u8>> = Vec::new();
    let mut keys_head = vec![0u8; 4];
    if let Some(k) = kids.iter().find(|k| &k.kind == b"keys") {
        let r = k.body();
        if r.len() < 8 { return Err("a broken keys box".into()); }
        keys_head = old[r.start..r.start + 4].to_vec();
        let mut at = r.start + 8;
        for _ in 0..be32(old, r.start + 4) {
            if at + 8 > r.end { return Err("a broken keys box".into()); }
            let size = be32(old, at) as usize;
            if size < 8 || at + size > r.end { return Err("a broken keys box".into()); }
            entries.push(old[at..at + size].to_vec());
            at += size;
        }
    }
    let mut items: Vec<(u32, Vec<u8>)> = Vec::new();
    if let Some(l) = kids.iter().find(|k| &k.kind == b"ilst") {
        for item in boxes(old, l.body().start, l.body().end)? {
            items.push((u32::from_be_bytes(item.kind), old[item.body()].to_vec()));
        }
    }
    for (k, v) in keys {
        let index = match entries.iter().position(|e| &e[4..8] == b"mdta" && &e[8..] == k.as_bytes()) {
            Some(i) => i as u32 + 1,
            None => {
                let mut e = (8 + k.len() as u32).to_be_bytes().to_vec();
                e.extend_from_slice(b"mdta");
                e.extend_from_slice(k.as_bytes());
                entries.push(e);
                entries.len() as u32
            }
        };
        items.retain(|(i, _)| *i != index);
        let mut data = vec![0, 0, 0, 1, 0, 0, 0, 0];
        data.extend_from_slice(v.as_bytes());
        items.push((index, make_box(b"data", &data)));
    }
    let mut key_list = keys_head;
    key_list.extend_from_slice(&(entries.len() as u32).to_be_bytes());
    for e in &entries { key_list.extend_from_slice(e); }
    let keys_box = make_box(b"keys", &key_list);
    let ilst_box = make_box(b"ilst", &items.iter().flat_map(|(i, b)| make_box(&i.to_be_bytes(), b)).collect::<Vec<u8>>());
    let mut body = if versioned { old[r.start..r.start + 4].to_vec() } else { Vec::new() };
    let (mut has_keys, mut has_ilst) = (false, false);
    for k in &kids {
        match &k.kind {
            b"keys" => { body.extend_from_slice(&keys_box); has_keys = true; }
            b"ilst" => {
                if !has_keys { body.extend_from_slice(&keys_box); has_keys = true; }
                body.extend_from_slice(&ilst_box);
                has_ilst = true;
            }
            _ => body.extend_from_slice(&old[k.at..k.at + k.size]),
        }
    }
    if !has_keys { body.extend_from_slice(&keys_box); }
    if !has_ilst { body.extend_from_slice(&ilst_box); }
    Ok(make_box(b"meta", &body))
}

/// The value of a key in an mdta `meta` box (the box, header included), when it's text
pub fn key_in_meta(meta: &[u8], key: &str) -> Option<String> {
    let m = *boxes(meta, 0, meta.len()).ok()?.first()?;
    mdta_keys(meta, &m).ok()?.into_iter().find(|(k, _)| k == key).map(|(_, v)| v)
}

/// The handler of a `meta` box's body, either form (with version and flags or without): its boxes
fn meta_children(b: &[u8], meta: &Box_) -> Result<Vec<Box_>, String> {
    let r = meta.body();
    // hdlr first: its type is at 4 without version and flags, at 8 with them
    let skip = if r.len() >= 8 && &b[r.start + 4..r.start + 8] == b"hdlr" { 0 } else { 4 };
    boxes(b, (r.start + skip).min(r.end), r.end)
}

fn handler(b: &[u8], kids: &[Box_]) -> Option<[u8; 4]> {
    let h = kids.iter().find(|k| &k.kind == b"hdlr")?;
    let r = h.body();
    (r.len() >= 12).then(|| b[r.start + 8..r.start + 12].try_into().unwrap())
}

/// The keys and values of an mdta `meta` box
fn mdta_keys(b: &[u8], meta: &Box_) -> Result<Vec<(String, String)>, String> {
    let kids = meta_children(b, meta)?;
    if handler(b, &kids) != Some(*b"mdta") { return Ok(vec![]); }
    let (Some(keys), Some(ilst)) = (kids.iter().find(|k| &k.kind == b"keys"), kids.iter().find(|k| &k.kind == b"ilst")) else { return Ok(vec![]) };
    let r = keys.body();
    if r.len() < 8 { return Err("a broken keys box".into()); }
    let mut names = Vec::new();
    let mut at = r.start + 8;
    for _ in 0..be32(b, r.start + 4) {
        if at + 8 > r.end { return Err("a broken keys box".into()); }
        let size = be32(b, at) as usize;
        if size < 8 || at + size > r.end { return Err("a broken keys box".into()); }
        names.push(String::from_utf8_lossy(&b[at + 8..at + size]).to_string());
        at += size;
    }
    let mut out = Vec::new();
    for item in boxes(b, ilst.body().start, ilst.body().end)? {
        let index = u32::from_be_bytes(item.kind) as usize;
        let Some(name) = index.checked_sub(1).and_then(|i| names.get(i)) else { continue };
        for data in boxes(b, item.body().start, item.body().end)? {
            let r = data.body();
            // UTF-8 (1) only: what's written here
            if &data.kind == b"data" && r.len() >= 8 && be32(b, r.start) == 1 {
                out.push((name.clone(), String::from_utf8_lossy(&b[r.start + 8..r.end]).to_string()));
            }
        }
    }
    Ok(out)
}

/// Adds 'delta' to every chunk offset (stco, co64) in the moov's tracks at or after `from`, where data moved
fn shift_chunks(b: &mut [u8], from: usize, to: usize, after: u64, delta: i64) -> Result<(), String> {
    for bx in boxes(b, from, to)? {
        let r = bx.body();
        match &bx.kind {
            b"trak" | b"mdia" | b"minf" | b"stbl" => shift_chunks(b, r.start, r.end, after, delta)?,
            b"stco" | b"co64" => {
                let wide = &bx.kind == b"co64";
                if r.len() < 8 { return Err("a broken chunk offset box".into()); }
                let n = be32(b, r.start + 4) as usize;
                let step = if wide { 8 } else { 4 };
                if r.start + 8 + n * step > r.end { return Err("a broken chunk offset box".into()); }
                for i in 0..n {
                    let p = r.start + 8 + i * step;
                    let o = if wide { be64(b, p) } else { be32(b, p) as u64 };
                    if o < after { continue; }
                    let moved = (o as i64 + delta) as u64;
                    if wide {
                        b[p..p + 8].copy_from_slice(&moved.to_be_bytes());
                    } else {
                        let v = u32::try_from(moved).map_err(|_| "a chunk offset past 4 GB in stco")?;
                        b[p..p + 4].copy_from_slice(&v.to_be_bytes());
                    }
                }
            }
            _ => {}
        }
    }
    Ok(())
}

/// Writes these keys into the file's `moov/meta`, in place of the mdta keys it had there. With room after the moov
/// (a `free` box, as ffmpeg's `-moov_size` leaves), the moov grows into it in place, and nothing else moves: what a
/// large export does. Else the file is written anew next to it and renamed over it: the moov grows, and the media
/// after it (with `+faststart`) moves, so the tracks' chunk offsets are moved with it
pub fn write_keys(path: &Path, keys: &[(&str, &str)]) -> Result<(), String> {
    let err = |e: std::io::Error| format!("{}: {e}", path.display());
    let mut f = File::open(path).map_err(err)?;
    let len = f.metadata().map_err(err)?.len();
    let top = top_level(&mut f)?;
    let &(moov_at, moov_size, _) = top.iter().find(|b| &b.2 == b"moov").ok_or("no moov")?;
    let mut moov = vec![0u8; moov_size as usize];
    f.seek(SeekFrom::Start(moov_at)).map_err(err)?;
    f.read_exact(&mut moov).map_err(err)?;
    let head = boxes(&moov, 0, moov.len())?[0];

    // The moov's boxes but an mdta meta, then the new one
    let mut kept = Vec::new();
    for kid in boxes(&moov, head.header, moov.len())? {
        if &kid.kind == b"meta" && handler(&moov, &meta_children(&moov, &kid)?) == Some(*b"mdta") { continue; }
        kept.push(kid);
    }
    let meta = meta_box(keys);
    let body_len: usize = kept.iter().map(|k| k.size).sum::<usize>() + meta.len();
    let new_size = 8 + body_len;
    let delta = new_size as i64 - moov_size as i64;
    let moov_end = moov_at + moov_size;
    // `shift`: what the media after the moov moves by
    let build = |shift: i64| -> Result<Vec<u8>, String> {
        let mut new_moov = Vec::with_capacity(new_size);
        new_moov.extend_from_slice(&u32::try_from(new_size).map_err(|_| "a moov over 4 GB")?.to_be_bytes());
        new_moov.extend_from_slice(b"moov");
        for kid in &kept {
            let mut b = moov[kid.at..kid.at + kid.size].to_vec();
            if &kid.kind == b"trak" && shift != 0 { shift_chunks(&mut b, kid.header, kid.size, moov_end, shift)?; }
            new_moov.extend(b);
        }
        new_moov.extend_from_slice(&meta);
        Ok(new_moov)
    };

    // In place: the free box right after the moov takes the difference, and keeps 8 bytes at least to stay a box
    // (or is taken whole)
    if let Some(&(free_at, free_size, _)) = top.iter().find(|b| b.0 == moov_end && matches!(&b.2, b"free" | b"skip")) {
        let room = moov_size + free_size;
        let left = room as i64 - new_size as i64;
        if left == 0 || left >= 8 {
            let new_moov = build(0)?;
            let mut out = File::options().write(true).open(path).map_err(err)?;
            out.seek(SeekFrom::Start(moov_at)).map_err(err)?;
            out.write_all(&new_moov).map_err(err)?;
            if left >= 8 {
                let mut free = Vec::with_capacity(8);
                free.extend_from_slice(&(u32::try_from(left).map_err(|_| "a free box over 4 GB")?).to_be_bytes());
                free.extend_from_slice(b"free");
                out.write_all(&free).map_err(err)?;
            }
            debug_assert!(free_at == moov_end);
            return out.flush().map_err(err);
        }
    }
    let new_moov = build(delta)?;

    let tmp = PathBuf::from(format!("{}.keys.tmp", path.display()));
    let result = (|| -> Result<(), String> {
        let mut out = std::io::BufWriter::new(File::create(&tmp).map_err(|e| format!("{}: {e}", tmp.display()))?);
        f.seek(SeekFrom::Start(0)).map_err(err)?;
        std::io::copy(&mut (&mut f).take(moov_at), &mut out).map_err(err)?;
        out.write_all(&new_moov).map_err(err)?;
        f.seek(SeekFrom::Start(moov_end)).map_err(err)?;
        std::io::copy(&mut (&mut f).take(len - moov_end), &mut out).map_err(err)?;
        out.flush().map_err(err)?;
        Ok(())
    })();
    if let Err(e) = result { let _ = std::fs::remove_file(&tmp); return Err(e); }
    std::fs::rename(&tmp, path).map_err(err)
}

/// The mdta keys of the file, from `moov/meta` and `moov/udta/meta` (ffmpeg's `use_metadata_tags`), in that order
pub fn read_keys(path: &Path) -> Result<Vec<(String, String)>, String> {
    let mut f = File::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let top = top_level(&mut f)?;
    let Some(&(moov_at, moov_size, _)) = top.iter().find(|b| &b.2 == b"moov") else { return Ok(vec![]) };
    let mut moov = vec![0u8; moov_size as usize];
    f.seek(SeekFrom::Start(moov_at)).map_err(|e| e.to_string())?;
    f.read_exact(&mut moov).map_err(|e| e.to_string())?;
    let head = boxes(&moov, 0, moov.len())?[0];
    let mut out = Vec::new();
    let kids = boxes(&moov, head.header, moov.len())?;
    for kid in kids.iter().filter(|k| &k.kind == b"meta") { out.extend(mdta_keys(&moov, kid)?); }
    for udta in kids.iter().filter(|k| &k.kind == b"udta") {
        for kid in boxes(&moov, udta.body().start, udta.body().end)?.iter().filter(|k| &k.kind == b"meta") {
            out.extend(mdta_keys(&moov, kid)?);
        }
    }
    Ok(out)
}

/// One key's value, None: none
pub fn read_key(path: &Path, key: &str) -> Result<Option<String>, String> {
    Ok(read_keys(path)?.into_iter().find(|(k, _)| k == key).map(|(_, v)| v))
}

/// The turn a player gives the video to show it upright, clockwise in degrees, 0, 90, 180 or 270: the display matrix of
/// its first video track's header (`moov/trak/tkhd`), as a camera held on its side writes it (an Osmo Action 6 shot
/// vertical: 3840x2160 turned 90, which ffprobe shows as `rotation: -90`, counterclockwise). The matrix's `a b c d`
/// in 16.16, a point (x, y) shown at (a·x + c·y, b·x + d·y): turned by atan2(b, a), rounded to a quarter. A mirrored
/// one is taken for its turn alone. 0 for a file without a moov or a video track
pub fn display_rotation(path: &Path) -> Result<u16, String> {
    let mut f = File::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let top = top_level(&mut f)?;
    let Some(&(moov_at, moov_size, _)) = top.iter().find(|b| &b.2 == b"moov") else { return Ok(0) };
    let mut moov = vec![0u8; moov_size as usize];
    f.seek(SeekFrom::Start(moov_at)).map_err(|e| e.to_string())?;
    f.read_exact(&mut moov).map_err(|e| e.to_string())?;
    Ok(rotation_in_moov(&moov)?.unwrap_or(0))
}

/// The first video track's turn in a moov (the box, header included), None without one
fn rotation_in_moov(moov: &[u8]) -> Result<Option<u16>, String> {
    let head = boxes(moov, 0, moov.len())?[0];
    for trak in boxes(moov, head.header, moov.len())?.iter().filter(|k| &k.kind == b"trak") {
        let kids = boxes(moov, trak.body().start, trak.body().end)?;
        let video = kids.iter().filter(|k| &k.kind == b"mdia").any(|mdia| {
            boxes(moov, mdia.body().start, mdia.body().end).unwrap_or_default().iter()
                // hdlr: version and flags, pre_defined, then the handler type
                .any(|h| &h.kind == b"hdlr" && h.size >= h.header + 12 && &moov[h.body().start + 8..h.body().start + 12] == b"vide")
        });
        let Some(tkhd) = kids.iter().find(|k| &k.kind == b"tkhd") else { continue };
        if !video { continue; }
        let body = tkhd.body();
        // After version and flags: times, track ID, reserved and duration (20 bytes, 32 in version 1), then reserved
        // (8), layer, group, volume and reserved (8): the matrix
        let at = body.start + 4 + if moov[body.start] == 1 { 32 } else { 20 } + 16;
        if at + 36 > body.end { return Err("a broken tkhd".into()); }
        let v = |i: usize| be32(moov, at + 4 * i) as i32 as f64 / 65536.0;
        let (a, b) = (v(0), v(1));
        if a == 0.0 && b == 0.0 { return Ok(Some(0)); }
        let degrees = b.atan2(a).to_degrees();
        return Ok(Some(((degrees / 90.0).round() as i64 * 90).rem_euclid(360) as u16));
    }
    Ok(None)
}

#[cfg(test)]
mod tests {
    use super::*;

    // A moov of a sound track and a video track whose header has the matrix (a, b, c, d)
    fn turned_moov(m: [i32; 4], version: u8) -> Vec<u8> {
        let hdlr = |kind: &[u8; 4]| { let mut b = vec![0u8; 8]; b.extend_from_slice(kind); b.extend_from_slice(&[0u8; 12]); make_box(b"hdlr", &b) };
        let tkhd = |m: [i32; 4]| {
            let mut b = vec![version, 0, 0, 3];
            b.extend(std::iter::repeat(0u8).take(if version == 1 { 32 } else { 20 } + 16));
            for x in [m[0], m[1], 0, m[2], m[3], 0, 0, 0, 0x4000_0000] { b.extend_from_slice(&x.to_be_bytes()); }
            b.extend_from_slice(&[0u8; 8]);
            make_box(b"tkhd", &b)
        };
        let one = 0x10000;
        let sound = make_box(b"trak", &[tkhd([one, 0, 0, one]), make_box(b"mdia", &hdlr(b"soun"))].concat());
        let video = make_box(b"trak", &[tkhd(m), make_box(b"mdia", &hdlr(b"vide"))].concat());
        make_box(b"moov", &[sound, video].concat())
    }

    #[test]
    fn a_turn_by_the_video_tracks_matrix() {
        let one = 0x10000;
        // The Osmo Action 6 held vertical: (0, 1, -1, 0), ffprobe's -90
        assert_eq!(rotation_in_moov(&turned_moov([0, one, -one, 0], 0)).unwrap(), Some(90));
        assert_eq!(rotation_in_moov(&turned_moov([0, one, -one, 0], 1)).unwrap(), Some(90));
        assert_eq!(rotation_in_moov(&turned_moov([-one, 0, 0, -one], 0)).unwrap(), Some(180));
        assert_eq!(rotation_in_moov(&turned_moov([0, -one, one, 0], 0)).unwrap(), Some(270));
        assert_eq!(rotation_in_moov(&turned_moov([one, 0, 0, one], 0)).unwrap(), Some(0));
        assert_eq!(rotation_in_moov(&make_box(b"moov", &[])).unwrap(), None);
    }

    // A file of ftyp, moov (one track whose chunks are in mdat, plus extra boxes) and mdat, in either order
    fn file(moov_first: bool, wide: bool, extra: &[Vec<u8>]) -> (Vec<u8>, Vec<u8>) {
        let payload: Vec<u8> = (0..64u8).collect();
        let ftyp = make_box(b"ftyp", b"isom\0\0\0\0isom");
        let offsets = |base: u64| -> Vec<u8> {
            let mut b = vec![0, 0, 0, 0, 0, 0, 0, 2];
            for o in [base + 8, base + 8 + 32] {
                if wide { b.extend_from_slice(&o.to_be_bytes()) } else { b.extend_from_slice(&(o as u32).to_be_bytes()) }
            }
            make_box(if wide { b"co64" } else { b"stco" }, &b)
        };
        let moov_for = |base: u64| -> Vec<u8> {
            let stbl = make_box(b"stbl", &offsets(base));
            let trak = make_box(b"trak", &make_box(b"mdia", &make_box(b"minf", &stbl)));
            let mut body = make_box(b"mvhd", &[0u8; 100]);
            body.extend(trak);
            for e in extra { body.extend_from_slice(e); }
            make_box(b"moov", &body)
        };
        let mdat = make_box(b"mdat", &payload);
        let mut out = ftyp.clone();
        if moov_first {
            let moov_len = moov_for(0).len() as u64;
            out.extend(moov_for(ftyp.len() as u64 + moov_len));
            out.extend(mdat);
        } else {
            out.extend(mdat);
            out.extend(moov_for(ftyp.len() as u64));
        }
        (out, payload)
    }

    // The chunk offsets of the file's one track, and the 32 bytes at each
    fn chunks(b: &[u8]) -> Vec<Vec<u8>> {
        let top = boxes(b, 0, b.len()).unwrap();
        let moov = top.iter().find(|x| &x.kind == b"moov").unwrap();
        let mut found = Vec::new();
        fn find(b: &[u8], from: usize, to: usize, found: &mut Vec<u64>) {
            for x in boxes(b, from, to).unwrap() {
                let r = x.body();
                match &x.kind {
                    b"trak" | b"mdia" | b"minf" | b"stbl" => find(b, r.start, r.end, found),
                    b"stco" => for i in 0..be32(b, r.start + 4) as usize { found.push(be32(b, r.start + 8 + 4 * i) as u64) },
                    b"co64" => for i in 0..be32(b, r.start + 4) as usize { found.push(be64(b, r.start + 8 + 8 * i)) },
                    _ => {}
                }
            }
        }
        find(b, moov.body().start, moov.body().end, &mut found);
        found.into_iter().map(|o| b[o as usize..o as usize + 32].to_vec()).collect()
    }

    fn temp(name: &str, bytes: &[u8]) -> PathBuf {
        let p = std::env::temp_dir().join(format!("mp4meta-{}-{name}.mp4", std::process::id()));
        std::fs::write(&p, bytes).unwrap();
        p
    }

    #[test]
    fn written_and_read_back() {
        let link = r#"{"v":1,"kind":"moment","name":"Черепаха"}"#;
        for (moov_first, wide) in [(true, false), (false, false), (true, true)] {
            let (bytes, payload) = file(moov_first, wide, &[]);
            let p = temp(&format!("rw-{moov_first}-{wide}"), &bytes);
            write_keys(&p, &[(LINK_KEY, link), (CREATION_DATE_KEY, "2026-10-22T15:28:42+0200")]).unwrap();
            let out = std::fs::read(&p).unwrap();
            // The media is where the offsets say, before and after
            assert_eq!(chunks(&out), vec![payload[..32].to_vec(), payload[32..].to_vec()], "moov first {moov_first}, co64 {wide}");
            assert_eq!(read_key(&p, LINK_KEY).unwrap().as_deref(), Some(link));
            assert_eq!(read_key(&p, CREATION_DATE_KEY).unwrap().as_deref(), Some("2026-10-22T15:28:42+0200"));
            // Written again: replaced, not added
            write_keys(&p, &[(LINK_KEY, "{}")]).unwrap();
            assert_eq!(read_keys(&p).unwrap(), vec![(LINK_KEY.to_string(), "{}".to_string())]);
            assert_eq!(chunks(&std::fs::read(&p).unwrap()).len(), 2);
            let _ = std::fs::remove_file(&p);
        }
    }

    #[test]
    fn in_place_with_room() {
        // ftyp, moov, free (ffmpeg's -moov_size), mdat: the moov grows into the free box, the media stays put
        let (bytes, payload) = file(true, false, &[]);
        let top = boxes(&bytes, 0, bytes.len()).unwrap();
        let moov = top.iter().find(|b| &b.kind == b"moov").unwrap();
        let mut with_room = bytes[..moov.at + moov.size].to_vec();
        let room = 4096;
        with_room.extend(make_box(b"free", &vec![0u8; room - 8]));
        // The chunk offsets now point past the free box
        let mdat_at = with_room.len() as u64;
        let mut fixed = with_room.clone();
        let m = boxes(&fixed, 0, fixed.len()).unwrap().into_iter().find(|b| &b.kind == b"moov").unwrap();
        shift_chunks(&mut fixed, m.at + 8, m.at + m.size, 0, room as i64).unwrap();
        fixed.extend(make_box(b"mdat", &payload));
        let p = temp("inplace", &fixed);
        let len = fixed.len();
        write_keys(&p, &[(LINK_KEY, "{\"v\":1}")]).unwrap();
        let out = std::fs::read(&p).unwrap();
        assert_eq!(out.len(), len, "the file keeps its length");
        assert_eq!(&out[mdat_at as usize..], &fixed[mdat_at as usize..], "the media isn't moved");
        assert_eq!(chunks(&out), vec![payload[..32].to_vec(), payload[32..].to_vec()]);
        assert_eq!(read_key(&p, LINK_KEY).unwrap().as_deref(), Some("{\"v\":1}"));
        let _ = std::fs::remove_file(&p);
        // Too little room: written anew, the media moved
        let mut tight = with_room[..moov.at + moov.size].to_vec();
        tight.extend(make_box(b"free", &[0u8; 4]));
        let mut fixed = tight.clone();
        let m = boxes(&fixed, 0, fixed.len()).unwrap().into_iter().find(|b| &b.kind == b"moov").unwrap();
        shift_chunks(&mut fixed, m.at + 8, m.at + m.size, 0, 12).unwrap();
        fixed.extend(make_box(b"mdat", &payload));
        let p = temp("tight", &fixed);
        write_keys(&p, &[(LINK_KEY, "{}")]).unwrap();
        assert_eq!(chunks(&std::fs::read(&p).unwrap()), vec![payload[..32].to_vec(), payload[32..].to_vec()]);
        let _ = std::fs::remove_file(&p);
    }

    #[test]
    fn other_meta_boxes_kept() {
        // udta/meta of iTunes items (where ffmpeg puts the cover art), and an mdta one there as ffmpeg's
        // use_metadata_tags writes it (with version and flags)
        let mut itunes = vec![0u8; 4];
        let mut hdlr = vec![0u8; 8];
        hdlr.extend_from_slice(b"mdir");
        hdlr.extend_from_slice(&[0u8; 13]);
        itunes.extend(make_box(b"hdlr", &hdlr));
        itunes.extend(make_box(b"ilst", &make_box(b"covr", &make_box(b"data", &[0, 0, 0, 13, 0, 0, 0, 0, 0xff, 0xd8]))));
        let mut ffmpeg_mdta = vec![0u8; 4];
        ffmpeg_mdta.extend(meta_box(&[("from.ffmpeg", "yes")])[8..].to_vec());
        let mut udta = make_box(b"meta", &itunes);
        udta.extend(make_box(b"meta", &ffmpeg_mdta));
        let (bytes, _) = file(true, false, &[make_box(b"udta", &udta)]);
        let p = temp("other", &bytes);
        assert_eq!(read_keys(&p).unwrap(), vec![("from.ffmpeg".to_string(), "yes".to_string())]);
        write_keys(&p, &[(LINK_KEY, "{}")]).unwrap();
        let out = std::fs::read(&p).unwrap();
        assert!(out.windows(4).any(|w| w == b"covr"), "the cover art is kept");
        assert_eq!(read_keys(&p).unwrap(), vec![(LINK_KEY.to_string(), "{}".to_string()), ("from.ffmpeg".to_string(), "yes".to_string())]);
        let _ = std::fs::remove_file(&p);
    }
}
