// SPDX-License-Identifier: GPL-3.0-or-later
//
// Albums (docs/albums-photos-share-concept.md, "Albums"; docs/library-management.md §8): folders of links in the
// project's folder, `_Clips/` being the user's own albums. A clip in an album is a light link: a small MP4,
// `<name>.afplink.mp4`, whose one video frame is the clip's thumbnail (a JPEG, as it is: QuickTime's Photo JPEG) and
// whose `ch.actionfootageplayer.link` key (moov/meta, as every link has it) is the link to the original,
// `{ v: 1, kind: "clip", clip, size, created, roots }`. Nothing else is in it. The player and the catalog read the key as
// they read a clip preview's or a moment's, so opening it plays the original; Finder draws its frame.
//
// It's written here, in one go, with no tool run: the thumbnail is the one the cards show (the camera's .THM or
// low-resolution file, else the system's, thumbs.rs), turned from log to normal colour when the clip's curve is known
// already (the probe's answer kept from before, or the curve chosen in its own file) by the LUTs the player uses
// (its dlogm plugin's luts/: <curve>_hlg.cube then NBCU's HLG -> SDR, as its clip previews were). A clip not read yet
// keeps its thumbnail as it is; no clip is read just for that.

use serde_json::{ json, Value };
use std::collections::{ HashMap, HashSet };
use std::io::Write;
use std::path::{ Path, PathBuf };
use std::sync::{ Arc, Mutex };

const LINK_KEY: &str = "ch.actionfootageplayer.link";
const NBCU_HLG: &str = "3-NBCU_HLG2SDR_DL_v1.2.cube";

// ---------- the link's file ----------

fn bx(kind: &[u8; 4], body: &[u8]) -> Vec<u8> {
    let mut b = Vec::with_capacity(8 + body.len());
    b.extend_from_slice(&(8 + body.len() as u32).to_be_bytes());
    b.extend_from_slice(kind);
    b.extend_from_slice(body);
    b
}

/// A JPEG's size, from its frame header
pub fn jpeg_size(b: &[u8]) -> Option<(u16, u16)> {
    if b.len() < 4 || b[0] != 0xFF || b[1] != 0xD8 { return None; }
    let mut i = 2;
    while i + 9 < b.len() {
        if b[i] != 0xFF { return None; }
        let m = b[i + 1];
        if m == 0xFF { i += 1; continue; }
        if (0xD0..=0xD9).contains(&m) || m == 0x01 { i += 2; continue; }
        let len = u16::from_be_bytes([b[i + 2], b[i + 3]]) as usize;
        if matches!(m, 0xC0..=0xC3 | 0xC5..=0xC7 | 0xC9..=0xCB | 0xCD..=0xCF) {
            let (h, w) = (u16::from_be_bytes([b[i + 5], b[i + 6]]), u16::from_be_bytes([b[i + 7], b[i + 8]]));
            return (w > 0 && h > 0).then_some((w, h));
        }
        i += 2 + len;
    }
    None
}

/// `moov/meta` with the link, as the player's mp4meta.rs writes it (Apple's form: no version and flags; hdlr mdta)
fn meta_box(link: &str) -> Vec<u8> {
    let mut hdlr = vec![0u8; 8];
    hdlr.extend_from_slice(b"mdta");
    hdlr.extend_from_slice(&[0u8; 13]);
    let mut keys = vec![0u8; 4];
    keys.extend_from_slice(&1u32.to_be_bytes());
    keys.extend_from_slice(&(8 + LINK_KEY.len() as u32).to_be_bytes());
    keys.extend_from_slice(b"mdta");
    keys.extend_from_slice(LINK_KEY.as_bytes());
    let mut data = vec![0, 0, 0, 1, 0, 0, 0, 0];
    data.extend_from_slice(link.as_bytes());
    let mut body = bx(b"hdlr", &hdlr);
    body.extend(bx(b"keys", &keys));
    body.extend(bx(b"ilst", &bx(&1u32.to_be_bytes(), &bx(b"data", &data))));
    bx(b"meta", &body)
}

/// A light link's bytes: ftyp, mdat (the JPEG, as it is), moov (one video track of one Photo JPEG frame held a second;
/// the link in moov/meta)
pub fn link_file(jpeg: &[u8], link: &str) -> Result<Vec<u8>, String> {
    let (w, h) = jpeg_size(jpeg).ok_or("the thumbnail isn't a JPEG")?;
    const SCALE: u32 = 1000;
    const UNITY: [u8; 36] = [0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0x40, 0, 0, 0];
    let ftyp = bx(b"ftyp", b"qt  \x00\x00\x02\x00qt  ");
    let offset = (ftyp.len() + 8) as u32;
    let u32b = |v: u32| v.to_be_bytes();

    let mut mvhd = vec![0u8; 12]; // version, flags, created, modified
    mvhd.extend(u32b(SCALE)); mvhd.extend(u32b(SCALE));
    mvhd.extend([0, 1, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]); // rate 1.0, volume 1.0, reserved
    mvhd.extend(UNITY);
    mvhd.extend([0u8; 24]);
    mvhd.extend(u32b(2)); // next track

    let mut tkhd = vec![0, 0, 0, 3]; // enabled, in the movie
    tkhd.extend([0u8; 8]);
    tkhd.extend(u32b(1)); tkhd.extend([0u8; 4]); tkhd.extend(u32b(SCALE));
    tkhd.extend([0u8; 16]); // reserved, layer, group, volume
    tkhd.extend(UNITY);
    tkhd.extend(u32b((w as u32) << 16)); tkhd.extend(u32b((h as u32) << 16));

    let mut mdhd = vec![0u8; 12];
    mdhd.extend(u32b(SCALE)); mdhd.extend(u32b(SCALE));
    mdhd.extend([0x7F, 0xFF, 0, 0]); // language undetermined
    let mut hdlr = vec![0u8; 4];
    hdlr.extend(b"mhlrvide"); hdlr.extend([0u8; 12]); hdlr.push(0);
    let mut dhdlr = vec![0u8; 4];
    dhdlr.extend(b"dhlrurl "); dhdlr.extend([0u8; 12]); dhdlr.push(0);
    let mut dref = vec![0u8; 4];
    dref.extend(u32b(1)); dref.extend(bx(b"url ", &[0, 0, 0, 1]));

    let mut entry = vec![0u8; 6];
    entry.extend([0, 1]); // data reference
    entry.extend([0u8; 4]); entry.extend(b"AFP "); // version, revision, vendor
    entry.extend(u32b(0x200)); entry.extend(u32b(0x200)); // temporal, spatial quality
    entry.extend(w.to_be_bytes()); entry.extend(h.to_be_bytes());
    entry.extend(u32b(0x0048_0000)); entry.extend(u32b(0x0048_0000)); // 72 dpi
    entry.extend(u32b(0)); entry.extend([0, 1]); // data size, frames per sample
    entry.extend([0u8; 32]); // compressor's name
    entry.extend([0, 0x18, 0xFF, 0xFF]); // depth 24, no colour table
    let mut stsd = vec![0u8; 4];
    stsd.extend(u32b(1)); stsd.extend(bx(b"jpeg", &entry));
    let mut stts = vec![0u8; 4];
    stts.extend(u32b(1)); stts.extend(u32b(1)); stts.extend(u32b(SCALE));
    let mut stsc = vec![0u8; 4];
    stsc.extend(u32b(1)); stsc.extend(u32b(1)); stsc.extend(u32b(1)); stsc.extend(u32b(1));
    let mut stsz = vec![0u8; 4];
    stsz.extend(u32b(jpeg.len() as u32)); stsz.extend(u32b(1));
    let mut stco = vec![0u8; 4];
    stco.extend(u32b(1)); stco.extend(u32b(offset));

    let mut stbl = bx(b"stsd", &stsd);
    for (k, b) in [(b"stts", &stts), (b"stsc", &stsc), (b"stsz", &stsz), (b"stco", &stco)] { stbl.extend(bx(k, b)); }
    let mut minf = bx(b"vmhd", &[0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0]);
    minf.extend(bx(b"hdlr", &dhdlr));
    minf.extend(bx(b"dinf", &bx(b"dref", &dref)));
    minf.extend(bx(b"stbl", &stbl));
    let mut mdia = bx(b"mdhd", &mdhd);
    mdia.extend(bx(b"hdlr", &hdlr));
    mdia.extend(bx(b"minf", &minf));
    let mut trak = bx(b"tkhd", &tkhd);
    trak.extend(bx(b"mdia", &mdia));
    let mut moov = bx(b"mvhd", &mvhd);
    moov.extend(bx(b"trak", &trak));
    moov.extend(meta_box(link));

    let mut out = ftyp;
    out.extend(bx(b"mdat", jpeg));
    out.extend(bx(b"moov", &moov));
    Ok(out)
}

fn be32(s: &[u8], i: usize) -> Option<u32> { s.get(i..i + 4).map(|b| u32::from_be_bytes([b[0], b[1], b[2], b[3]])) }

fn find_box(s: &[u8], a: usize, b: usize, kind: &[u8; 4]) -> Option<(usize, usize)> {
    let mut pos = a;
    while pos + 8 <= b {
        let size = be32(s, pos)? as usize;
        if size < 8 || pos + size > b { return None; }
        if &s[pos + 4..pos + 8] == kind { return Some((pos + 8, pos + size)); }
        pos += size;
    }
    None
}

/// A light link's picture: its one frame, when that's a JPEG (a flip-book preview's or a moment's frame is H.264: None,
/// their picture is their cover). Only a small file is read
pub fn picture(path: &Path) -> Option<Vec<u8>> {
    if std::fs::metadata(path).ok()?.len() > 4 * 1024 * 1024 { return None; }
    let s = std::fs::read(path).ok()?;
    let mut r = find_box(&s, 0, s.len(), b"moov")?;
    for k in [b"trak", b"mdia", b"minf", b"stbl"] { r = find_box(&s, r.0, r.1, k)?; }
    let stsd = find_box(&s, r.0, r.1, b"stsd")?;
    if s.get(stsd.0 + 12..stsd.0 + 16)? != b"jpeg" { return None; }
    let (stsz, stco) = (find_box(&s, r.0, r.1, b"stsz")?, find_box(&s, r.0, r.1, b"stco")?);
    let (size, at) = (be32(&s, stsz.0 + 4)? as usize, be32(&s, stco.0 + 8)? as usize);
    let jpeg = s.get(at..at + size)?;
    jpeg.starts_with(&[0xFF, 0xD8]).then(|| jpeg.to_vec())
}

// ---------- log to normal colour ----------

/// A 3D LUT, red fastest: its grid's size and values
pub struct Lut { n: usize, data: Vec<f32> }

fn read_cube(path: &Path) -> Option<Lut> {
    let text = std::fs::read_to_string(path).ok()?;
    let mut n = 0usize;
    let mut data: Vec<f32> = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if let Some(v) = line.strip_prefix("LUT_3D_SIZE") { n = v.trim().parse().ok()?; data.reserve(n * n * n * 3); continue; }
        if !line.starts_with(|c: char| c.is_ascii_digit() || c == '-' || c == '.') { continue; }
        let mut it = line.split_ascii_whitespace().map(|x| x.parse::<f32>());
        match (it.next(), it.next(), it.next()) { (Some(Ok(r)), Some(Ok(g)), Some(Ok(b))) => data.extend([r, g, b]), _ => return None }
    }
    (n >= 2 && data.len() == n * n * n * 3).then_some(Lut { n, data })
}

impl Lut {
    /// At `rgb` (0..1), by tetrahedral interpolation (as the player's gpu_lut.rs)
    fn at(&self, rgb: [f32; 3]) -> [f32; 3] {
        let n = self.n;
        let p = rgb.map(|v| v.clamp(0.0, 1.0) * (n - 1) as f32);
        let i = p.map(|x| (x as usize).min(n - 2));
        let f = [p[0] - i[0] as f32, p[1] - i[1] as f32, p[2] - i[2] as f32];
        let at = |c: [usize; 3]| {
            let k = ((i[0] + c[0]) + (i[1] + c[1]) * n + (i[2] + c[2]) * n * n) * 3;
            [self.data[k], self.data[k + 1], self.data[k + 2]]
        };
        let mut axes = [0, 1, 2];
        axes.sort_by(|&a, &b| f[b].total_cmp(&f[a]));
        let mut corner = [0; 3];
        let mut prev = at(corner);
        let mut out = prev;
        for &axis in &axes {
            corner[axis] = 1;
            let cur = at(corner);
            for c in 0..3 { out[c] += f[axis] * (cur[c] - prev[c]); }
            prev = cur;
        }
        out
    }
}

/// A log curve's LUT to normal colour, as the player's clip previews had it (gyroflow-export's SDR): DJI's own Rec.709
/// LUT for D-Log M when it's installed, else the curve's LUT to HLG then NBCUniversal's HLG -> SDR (a Type III LUT: the
/// signal's 0..1 is (64 + 876 x) / 1023 of its domain, in and out), as one, at the log LUT's grid
fn compose(dir: &Path, curve: &str) -> Option<Lut> {
    if curve == "dlogm" { if let Some(l) = read_cube(&dir.join("dji-dlogm-rec709.cube")) { return Some(l); } }
    let log = read_cube(&dir.join(format!("{curve}_hlg.cube")))?;
    let nbcu = read_cube(&dir.join(NBCU_HLG))?;
    let (lo, span) = (64.0 / 1023.0, 876.0 / 1023.0);
    let mut data = Vec::with_capacity(log.data.len());
    for v in log.data.chunks(3) {
        let o = nbcu.at([v[0] * span + lo, v[1] * span + lo, v[2] * span + lo]);
        data.extend(o.map(|x| ((x - lo) / span).clamp(0.0, 1.0)));
    }
    Some(Lut { n: log.n, data })
}

/// The LUT of a curve ("dlogm", "gplog"…) from the player's LUT folder, read once; None: no LUT for it there
pub fn lut_for(dir: &Path, curve: &str) -> Option<Arc<Lut>> {
    static LUTS: Mutex<Option<HashMap<(PathBuf, String), Option<Arc<Lut>>>>> = Mutex::new(None);
    if curve.is_empty() || !curve.chars().all(|c| c.is_ascii_alphanumeric()) { return None; }
    let key = (dir.to_path_buf(), curve.to_string());
    if let Some(l) = LUTS.lock().unwrap().get_or_insert_with(HashMap::new).get(&key) { return l.clone(); }
    let l = compose(dir, curve).map(Arc::new);
    LUTS.lock().unwrap().get_or_insert_with(HashMap::new).insert(key, l.clone());
    l
}

/// The player's LUT folder: AFL_LUTS, else its dlogm plugin's luts/ (beside the probe tool's plugin)
pub fn luts_dir(probe_tool: Option<&Path>) -> Option<PathBuf> {
    if let Some(p) = std::env::var_os("AFL_LUTS").filter(|p| !p.is_empty()) { return Some(PathBuf::from(p)); }
    let d = probe_tool?.parent()?.parent()?.join("dlogm").join("luts");
    d.is_dir().then_some(d)
}

/// A JPEG through a LUT, a JPEG again; None when it can't be read
pub fn recolour(jpeg: &[u8], lut: &Lut) -> Option<Vec<u8>> {
    use zune_jpeg::zune_core::{ bytestream::ZCursor, colorspace::ColorSpace, options::DecoderOptions };
    let mut dec = zune_jpeg::JpegDecoder::new_with_options(ZCursor::new(jpeg), DecoderOptions::default().jpeg_set_out_colorspace(ColorSpace::RGB));
    let mut px = dec.decode().ok()?;
    let info = dec.info()?;
    if px.len() != info.width as usize * info.height as usize * 3 { return None; }
    for p in px.chunks_exact_mut(3) {
        let o = lut.at([p[0] as f32 / 255.0, p[1] as f32 / 255.0, p[2] as f32 / 255.0]);
        for c in 0..3 { p[c] = (o[c].clamp(0.0, 1.0) * 255.0 + 0.5) as u8; }
    }
    let mut out = Vec::with_capacity(jpeg.len());
    jpeg_encoder::Encoder::new(&mut out, 85).encode(&px, info.width, info.height, jpeg_encoder::ColorType::Rgb).ok()?;
    Some(out)
}

/// A picture for a clip with none to take (offline, unreadable): a dark frame
fn dark() -> Vec<u8> {
    let mut out = Vec::new();
    let _ = jpeg_encoder::Encoder::new(&mut out, 60).encode(&[24u8; 64 * 36 * 3], 64, 36, jpeg_encoder::ColorType::Rgb);
    out
}

// ---------- the albums ----------

// ---------- a photo in an album: a small JPEG with the link in its XMP ----------

/// A photo's link file: `<name>.afplink.jpg` (docs/albums-photos-share-concept.md, "Photos as objects")
pub const PHOTO_EXT: &str = ".afplink.jpg";
const AFP_NS: &str = "https://actionfootageplayer.ch/ns/1.0/";
/// The longest side of a photo link's picture
const PHOTO_SIDE: u32 = 1280;

fn xml(s: &str) -> String { s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;") }

/// A photo's link file: the picture (a JPEG, as it is) with `afp:link` in an XMP packet of its own, put where the
/// photos the player renders have theirs (an APP1 segment before the image data, within the first 64 KiB), so the
/// Library (`index::link`), the catalog (`inspect::photo_meta`) and the player (`link.photo`) read it as they read a
/// rendered photo's. An XMP packet the picture had is dropped
pub fn photo_file(jpeg: &[u8], link: &str) -> Result<Vec<u8>, String> {
    jpeg_size(jpeg).ok_or("The picture isn't a JPEG")?;
    const HEAD: &[u8] = b"http://ns.adobe.com/xap/1.0/\0";
    let packet = format!("<?xpacket begin=\"\u{feff}\" id=\"W5M0MpCehiHzreSzNTczkc9d\"?><x:xmpmeta xmlns:x=\"adobe:ns:meta/\">\
<rdf:RDF xmlns:rdf=\"http://www.w3.org/1999/02/22-rdf-syntax-ns#\"><rdf:Description rdf:about=\"\" xmlns:afp=\"{AFP_NS}\">\
<afp:link>{}</afp:link></rdf:Description></rdf:RDF></x:xmpmeta><?xpacket end=\"w\"?>", xml(link));
    let len = 2 + HEAD.len() + packet.len();
    if len > 0xffff { return Err("The link is too long for a photo".into()); }
    let mut out = Vec::with_capacity(jpeg.len() + len + 2);
    out.extend_from_slice(&jpeg[..2]);
    // The segments before the image data: JFIF's and Exif's kept first (where readers expect them), ours after, an
    // older XMP left out
    let mut i = 2;
    let mut ours = Some((len, packet));
    while i + 4 <= jpeg.len() && jpeg[i] == 0xff {
        let m = jpeg[i + 1];
        let n = u16::from_be_bytes([jpeg[i + 2], jpeg[i + 3]]) as usize;
        let seg = jpeg.get(i..i + 2 + n).ok_or("The picture is cut short")?;
        let first = m == 0xe0 || (m == 0xe1 && seg[4..].starts_with(b"Exif\0"));
        if !first {
            if let Some((len, packet)) = ours.take() {
                out.extend_from_slice(&[0xff, 0xe1]);
                out.extend_from_slice(&(len as u16).to_be_bytes());
                out.extend_from_slice(HEAD);
                out.extend_from_slice(packet.as_bytes());
            }
        }
        if m == 0xe1 && seg[4..].starts_with(HEAD) { i += 2 + n; continue; }
        if m == 0xda { break; }
        out.extend_from_slice(seg);
        i += 2 + n;
    }
    if ours.is_some() { return Err("The picture has no image data".into()); }
    out.extend_from_slice(&jpeg[i..]);
    Ok(out)
}

/// The clip's frame at `t` seconds, a JPEG at most PHOTO_SIDE wide: by the player's ffmpeg (a seek to the keyframe
/// before, then decoded up to the frame). As the camera recorded it: not stabilized, not cropped (the render is)
fn frame_at(path: &Path, t: f64, scratch: &Path) -> Option<Vec<u8>> {
    use std::process::{ Command, Stdio };
    static N: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    std::fs::create_dir_all(scratch).ok()?;
    let out = scratch.join(format!("frame-{}-{}.jpg", std::process::id(), N.fetch_add(1, std::sync::atomic::Ordering::Relaxed)));
    let ffmpeg = crate::player_ffmpeg();
    let mut cmd = Command::new(ffmpeg.as_deref().unwrap_or(Path::new("ffmpeg")));
    cmd.args(["-v", "error", "-y", "-ss", &format!("{:.3}", t.max(0.0)), "-i"]).arg(path)
        .args(["-an", "-sn", "-dn", "-frames:v", "1", "-vf", &format!("scale='min({PHOTO_SIDE},iw)':-2"), "-q:v", "3", "-f", "mjpeg"]).arg(&out)
        .stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null());
    #[cfg(windows)]
    { use std::os::windows::process::CommandExt; cmd.creation_flags(0x0800_0000); } // no console window
    let ok = cmd.status().is_ok_and(|s| s.success());
    let bytes = if ok { std::fs::read(&out).ok().filter(|b| jpeg_size(b).is_some()) } else { None };
    let _ = std::fs::remove_file(&out);
    bytes
}

fn rel_of(root: &Path, path: &Path) -> String { path.strip_prefix(root).map(|r| r.to_string_lossy().replace('\\', "/")).unwrap_or_default() }

/// A folder of the project's own for links, from its path in the project ("_Clips/Egypt/Day 1"): not the project's
/// folder itself, not among its footage, no way out of it
fn album_dir(root: &Path, album: &str) -> Result<PathBuf, String> {
    let mut dir = root.to_path_buf();
    let mut n = 0;
    for p in album.split('/').filter(|p| !p.is_empty()) {
        if p == "." || p == ".." || p.starts_with('.') || p.contains('\\') || p.contains(':') { return Err("That isn't an album's name".into()); }
        dir.push(p);
        n += 1;
    }
    if n == 0 { return Err("Choose an album".into()); }
    if !crate::index::own_area(root, &dir, &crate::index::footage_dirs(root)) { return Err("An album can't be among the footage".into()); }
    Ok(dir)
}

/// Every clip link of the project's folder (a light link, a flip-book preview, a draft), wherever it is:
/// { path, rel (in the project's folder), clip (its original's path in the tree), draft, size, mtime }
pub fn clip_links(root: &Path, files: &[(String, u64, i64)]) -> Vec<Value> {
    files.iter().filter_map(|(rel, size, mtime)| {
        let path = root.join(rel);
        let l = crate::index::link(&path).filter(|l| l["kind"] == "clip")?;
        Some(json!({ "path": path.display().to_string(), "rel": rel, "clip": l["clip"], "draft": l["draft"].as_bool() == Some(true),
                     "size": size, "mtime": mtime }))
    }).collect()
}

/// One clip to add: its path in the tree, its file, the file its thumbnail is taken from (the camera's quick-look file
/// when it has one, else the clip itself), its size, when it was recorded (unix seconds, when known), its log curve
/// when known ("dlogm"…: the thumbnail is turned to normal colour)
pub struct Add { pub tree: String, pub path: PathBuf, pub thumb: Option<PathBuf>, pub size: u64, pub created: Option<i64>, pub curve: Option<String> }

fn iso(t: i64) -> String {
    // Civil from days (Howard Hinnant)
    let (days, s) = (t.div_euclid(86400), t.rem_euclid(86400));
    let z = days + 719468;
    let era = z.div_euclid(146097);
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let (d, m) = (doy - (153 * mp + 2) / 5 + 1, if mp < 10 { mp + 3 } else { mp - 9 });
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!("{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}.000000Z", s / 3600, s % 3600 / 60, s % 60)
}

/// The clip's thumbnail for its link: the camera's .THM as it is (a JPEG), else the picture the cards show (index.rs's
/// thumb: kept in `cache`, so a clip seen in the window costs nothing more), in normal colour when its curve is known
fn thumbnail(a: &Add, cache: &Path, luts: Option<&Path>) -> Vec<u8> {
    let from = a.thumb.as_deref().unwrap_or(&a.path);
    let thm = from.extension().is_some_and(|e| e.eq_ignore_ascii_case("thm"));
    let raw = if thm { std::fs::read(from).ok().filter(|b| jpeg_size(b).is_some()) } else { None }
        .or_else(|| crate::index::thumb_bytes(from, cache))
        .or_else(|| if a.thumb.is_some() { crate::index::thumb_bytes(&a.path, cache) } else { None })
        .filter(|b| jpeg_size(b).is_some());
    let Some(raw) = raw else { return dark() };
    let lut = a.curve.as_deref().zip(luts).and_then(|(c, d)| lut_for(d, c));
    lut.and_then(|l| recolour(&raw, &l)).unwrap_or(raw)
}

/// Links to these clips made in an album (its path in the project's folder, made when it isn't there): one a clip, a
/// clip already in that album left as it is. { made: [paths], already, failed: [[clip, why]], ms }
pub fn add(root: &Path, album: &str, clips: &[Add], cache: &Path, luts: Option<&Path>) -> Result<Value, String> {
    let started = std::time::Instant::now();
    let dir = album_dir(root, album)?;
    std::fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    // What the album has: its names, and the clips linked from it
    let mut names: HashSet<String> = HashSet::new();
    let mut there: HashSet<String> = HashSet::new();
    for e in std::fs::read_dir(&dir).map_err(|e| e.to_string())?.flatten() {
        let name = e.file_name().to_string_lossy().into_owned();
        if name.to_lowercase().ends_with(crate::index::LINK_EXT) {
            if let Some(c) = crate::index::link(&e.path()).filter(|l| l["kind"] == "clip").and_then(|l| l["clip"].as_str().map(str::to_string)) { there.insert(c); }
        }
        names.insert(name.to_lowercase());
    }
    let mut jobs: Vec<(&Add, PathBuf)> = Vec::new();
    let mut already = 0usize;
    for a in clips {
        if !there.insert(a.tree.clone()) { already += 1; continue; }
        let stem = Path::new(a.tree.rsplit('/').next().unwrap_or(&a.tree)).file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_else(|| "clip".into());
        let mut name = format!("{stem}{}", crate::index::LINK_EXT);
        let mut n = 2;
        while !names.insert(name.to_lowercase()) { name = format!("{stem} {n}{}", crate::index::LINK_EXT); n += 1; }
        jobs.push((a, dir.join(name)));
    }
    let next = Mutex::new(0usize);
    let made = Mutex::new(Vec::new());
    let failed = Mutex::new(Vec::new());
    let root_text = root.display().to_string();
    std::thread::scope(|sc| {
        for _ in 0..jobs.len().clamp(1, 8) {
            sc.spawn(|| loop {
                let i = { let mut n = next.lock().unwrap(); let i = *n; *n += 1; i };
                let Some((a, out)) = jobs.get(i) else { break };
                let mut link = json!({ "v": 1, "kind": "clip", "clip": a.tree, "size": a.size, "roots": [root_text] });
                if let Some(t) = a.created { link["created"] = json!(iso(t)); }
                let r = link_file(&thumbnail(a, cache, luts), &link.to_string()).and_then(|bytes| {
                    let mut f = std::fs::OpenOptions::new().write(true).create_new(true).open(out).map_err(|e| e.to_string())?;
                    f.write_all(&bytes).map_err(|e| { let _ = std::fs::remove_file(out); e.to_string() })
                });
                match r { Ok(()) => made.lock().unwrap().push(out.clone()), Err(e) => failed.lock().unwrap().push(json!([a.tree, e])) }
            });
        }
    });
    let mut made = made.into_inner().unwrap();
    made.sort();
    // The catalog told at once: the views read the links from it
    let mut noted = made.clone();
    noted.push(dir.clone());
    crate::seen::note_now(&noted);
    Ok(json!({ "made": made.iter().map(|p| p.display().to_string()).collect::<Vec<_>>(), "already": already, "album": rel_of(root, &dir),
               "failed": failed.into_inner().unwrap(), "ms": started.elapsed().as_millis() as u64 }))
}

/// One photo to make: its clip (as `Add`), the frame's time in it, and the part of the frame wanted when one is set
/// (`crop`: x, y, w, h in shares of the frame; `camera`: the crop mode's yaw, pitch, roll, zoom), kept in the link
pub struct Photo { pub clip: Add, pub t: f64, pub crop: Option<Value>, pub camera: Option<Value> }

/// Photos made in an album, each a link file `<clip>_<seconds>s.afplink.jpg`: the clip's frame at that time, small,
/// with `{ v: 1, kind: "photo", clip, t, size, roots, created, crop, camera }` in its XMP. Nothing is rendered: the
/// picture is the frame as recorded (in normal colour when the clip's curve is known), the render comes when the
/// photo is shared. One at that time of that clip already in the album is left as it is.
/// { made: [paths], already, album, failed: [[clip, why]], ms }
pub fn add_photos(root: &Path, album: &str, photos: &[Photo], cache: &Path, luts: Option<&Path>) -> Result<Value, String> {
    let started = std::time::Instant::now();
    let dir = album_dir(root, album)?;
    std::fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let mut names: HashSet<String> = HashSet::new();
    let mut there: Vec<(String, f64)> = Vec::new();
    for e in std::fs::read_dir(&dir).map_err(|e| e.to_string())?.flatten() {
        let name = e.file_name().to_string_lossy().into_owned();
        if name.to_lowercase().ends_with(PHOTO_EXT) {
            if let Some(l) = crate::index::link(&e.path()).filter(|l| l["kind"] == "photo") {
                if let (Some(c), Some(t)) = (l["clip"].as_str(), l["t"].as_f64()) { there.push((c.to_string(), t)); }
            }
        }
        names.insert(name.to_lowercase());
    }
    let mut jobs: Vec<(&Photo, PathBuf)> = Vec::new();
    let mut already = 0usize;
    for p in photos {
        if there.iter().any(|(c, t)| *c == p.clip.tree && (t - p.t).abs() < 0.05) { already += 1; continue; }
        there.push((p.clip.tree.clone(), p.t));
        let stem = Path::new(p.clip.tree.rsplit('/').next().unwrap_or(&p.clip.tree)).file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_else(|| "clip".into());
        let stem = format!("{stem}_{:.2}s", p.t);
        let mut name = format!("{stem}{PHOTO_EXT}");
        let mut n = 2;
        while !names.insert(name.to_lowercase()) { name = format!("{stem} {n}{PHOTO_EXT}"); n += 1; }
        jobs.push((p, dir.join(name)));
    }
    let next = Mutex::new(0usize);
    let made = Mutex::new(Vec::new());
    let failed = Mutex::new(Vec::new());
    let root_text = root.display().to_string();
    let scratch = cache.join("frames");
    std::thread::scope(|sc| {
        // Fewer at once than clip links: each is a decode of the clip up to the frame
        for _ in 0..jobs.len().clamp(1, 4) {
            sc.spawn(|| loop {
                let i = { let mut n = next.lock().unwrap(); let i = *n; *n += 1; i };
                let Some((p, out)) = jobs.get(i) else { break };
                let a = &p.clip;
                let mut link = json!({ "v": 1, "kind": "photo", "clip": a.tree, "t": (p.t * 1000.0).round() / 1000.0, "size": a.size, "roots": [root_text] });
                if let Some(t) = a.created { link["created"] = json!(iso(t)); }
                if let Some(c) = p.crop.as_ref().filter(|c| !c.is_null()) { link["crop"] = c.clone(); }
                if let Some(c) = p.camera.as_ref().filter(|c| !c.is_null()) { link["camera"] = c.clone(); }
                // The frame itself; a clip that can't be read now (offline, no ffmpeg): its thumbnail, so the photo is
                // made all the same (its link is what matters, the picture is a reminder)
                let lut = a.curve.as_deref().zip(luts).and_then(|(c, d)| lut_for(d, c));
                let picture = match frame_at(&a.path, p.t, &scratch) {
                    Some(f) => lut.and_then(|l| recolour(&f, &l)).unwrap_or(f),
                    None => thumbnail(a, cache, luts),
                };
                let r = photo_file(&picture, &link.to_string()).and_then(|bytes| {
                    let mut f = std::fs::OpenOptions::new().write(true).create_new(true).open(out).map_err(|e| e.to_string())?;
                    f.write_all(&bytes).map_err(|e| { let _ = std::fs::remove_file(out); e.to_string() })
                });
                match r { Ok(()) => made.lock().unwrap().push(out.clone()), Err(e) => failed.lock().unwrap().push(json!([a.tree, e])) }
            });
        }
    });
    let mut made = made.into_inner().unwrap();
    made.sort();
    let mut noted = made.clone();
    noted.push(dir.clone());
    crate::seen::note_now(&noted);
    Ok(json!({ "made": made.iter().map(|p| p.display().to_string()).collect::<Vec<_>>(), "already": already, "album": rel_of(root, &dir),
               "failed": failed.into_inner().unwrap(), "ms": started.elapsed().as_millis() as u64 }))
}

/// What an album's item is, from its file: "clip" or "moment" (a link MP4), "still" (a photo: its link file, or a
/// rendered one with a link or in the project's own folders), and its name split into the part that's renamed and
/// its ending (`.afplink.mp4`, `.afplink.jpg`, a Motion Photo's `.MP.jpg`, else the extension)
fn item_of(root: &Path, file: &Path) -> Option<(&'static str, String, String)> {
    let name = file.file_name()?.to_string_lossy().into_owned();
    let lower = name.to_lowercase();
    let cut = |n: usize| (name[..name.len() - n].to_string(), name[name.len() - n..].to_string());
    if lower.ends_with(crate::index::LINK_EXT) {
        let l = crate::index::link(file)?;
        let kind = if l["kind"] == "moment" { "moment" } else if l["kind"] == "clip" { "clip" } else { return None };
        let (stem, ext) = cut(crate::index::LINK_EXT.len());
        return Some((kind, stem, ext));
    }
    if [".heic", ".jpg", ".jpeg", ".png"].iter().any(|e| lower.ends_with(e)) {
        if crate::index::link(file).is_none() && !crate::index::own_area(root, file, &crate::index::footage_dirs(root)) { return None; }
        let n = if lower.ends_with(PHOTO_EXT) { PHOTO_EXT.len() } else if lower.ends_with(".mp.jpg") { 7 } else { name.len() - name.rfind('.')? };
        let (stem, ext) = cut(n);
        return Some(("still", stem, ext));
    }
    // A moment made before 0.11 (a video with the moment's link, any name)
    crate::index::link(file).filter(|l| l["kind"] == "moment")?;
    let (stem, ext) = cut(name.len() - name.rfind('.')?);
    Some(("moment", stem, ext))
}

/// An album's items (clip links, moments, photos: their files, of the project's own) moved into another album, each
/// with what belongs to it: a link's own Gyroflow project, a moment's renders (found by its path: they follow it), a
/// Live Photo's video. A name taken there gets a number. { moved, problems }
pub fn move_to(root: &Path, album: &str, files: &[PathBuf]) -> Result<Value, String> {
    let dir = album_dir(root, album)?;
    std::fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let (mut moved, mut problems, mut touched) = (0usize, Vec::<String>::new(), vec![dir.clone()]);
    for from in files {
        let Some((kind, stem, ext)) = (if from.starts_with(root) { item_of(root, from) } else { None }) else {
            problems.push(format!("{}: not a clip link, a moment or a photo of this project", from.display()));
            continue;
        };
        if from.parent() == Some(dir.as_path()) { continue; }
        let mut new_stem = stem.clone();
        let mut n = 2;
        while dir.join(format!("{new_stem}{ext}")).exists() { new_stem = format!("{stem} {n}"); n += 1; }
        let to = dir.join(format!("{new_stem}{ext}"));
        // A moment's renders, at the paths its place gives them, before it goes
        let rel = |p: &Path| p.strip_prefix(root).map(|r| r.to_string_lossy().replace('\\', "/")).unwrap_or_default();
        let renders: Vec<(PathBuf, PathBuf)> = if kind == "moment" {
            crate::index::RENDER_PROFILES.iter().map(|p| (crate::index::render_path(root, &rel(from), p), crate::index::render_path(root, &rel(&to), p))).collect()
        } else { Vec::new() };
        match std::fs::rename(from, &to) {
            Ok(()) => {
                moved += 1;
                touched.extend([from.clone(), to.clone()]);
                let mut also: Vec<(PathBuf, PathBuf)> = renders;
                if kind != "still" { also.push((from.with_file_name(format!("{stem}.gyroflow")), dir.join(format!("{new_stem}.gyroflow")))); }
                else { for e in [".mov", ".MOV"] { also.push((from.with_file_name(format!("{stem}{e}")), dir.join(format!("{new_stem}{e}")))); } }
                for (a, b) in also {
                    if !a.is_file() || b.exists() { continue; }
                    if let Some(d) = b.parent() { let _ = std::fs::create_dir_all(d); }
                    if std::fs::rename(&a, &b).is_ok() { touched.extend([a, b]); }
                }
            }
            Err(e) => problems.push(format!("{}: {e}", from.display())),
        }
    }
    crate::seen::note_now(&touched);
    Ok(json!({ "moved": moved, "album": rel_of(root, &dir), "problems": problems }))
}

/// Moments and photos put in another album too: a copy of each link file (a moment's `.afplink.mp4` with its own
/// Gyroflow project, a photo's `.afplink.jpg`), a separate item from then on, with its own name and tags (the concept:
/// "One item in two albums"). A rendered photo isn't copied (it's the pixels, not a link): it's moved, or shared.
/// { copied, problems }
pub fn copy_to(root: &Path, album: &str, files: &[PathBuf]) -> Result<Value, String> {
    let dir = album_dir(root, album)?;
    std::fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let (mut copied, mut problems, mut touched) = (0usize, Vec::<String>::new(), vec![dir.clone()]);
    for from in files {
        let item = if from.starts_with(root) { item_of(root, from) } else { None };
        let Some((kind, stem, ext)) = item.filter(|(_, _, ext)| ext.eq_ignore_ascii_case(crate::index::LINK_EXT) || ext.eq_ignore_ascii_case(PHOTO_EXT)) else {
            problems.push(format!("{}: not a link (a rendered photo is moved, not copied)", from.display()));
            continue;
        };
        if from.parent() == Some(dir.as_path()) { continue; }
        let mut new_stem = stem.clone();
        let mut n = 2;
        while dir.join(format!("{new_stem}{ext}")).exists() { new_stem = format!("{stem} {n}"); n += 1; }
        let to = dir.join(format!("{new_stem}{ext}"));
        match std::fs::copy(from, &to) {
            Ok(_) => {
                copied += 1;
                touched.push(to.clone());
                let g = from.with_file_name(format!("{stem}.gyroflow"));
                let tg = dir.join(format!("{new_stem}.gyroflow"));
                if kind != "still" && g.is_file() && !tg.exists() && std::fs::copy(&g, &tg).is_ok() { touched.push(tg); }
            }
            Err(e) => problems.push(format!("{}: {e}", from.display())),
        }
    }
    crate::seen::note_now(&touched);
    Ok(json!({ "copied": copied, "album": rel_of(root, &dir), "problems": problems }))
}

// ---------- the page's commands ----------

/// "Add to an album…": links to the selected clips, made at once. `clips`: what the page knows of each,
/// { tree, path, thumb, size, created, curve }; `album`: its path in the project's folder ("_Clips/Egypt")
#[tauri::command]
pub async fn album_add(app: tauri::AppHandle, shared: tauri::State<'_, crate::Shared>, probes: tauri::State<'_, crate::index::SharedProbes>,
                       album: String, clips: Vec<Value>) -> Result<Value, String> {
    use tauri::Manager;
    let root = shared.lock().unwrap().project.clone().ok_or("Open a project first")?;
    let cache = app.path().app_cache_dir().map_err(|e| e.to_string())?.join("thumbs");
    let luts = luts_dir(probes.lock().unwrap().tool.as_deref());
    tauri::async_runtime::spawn_blocking(move || {
        let list: Vec<Add> = clips.iter().filter_map(|c| Some(Add {
            tree: c["tree"].as_str()?.to_string(), path: PathBuf::from(c["path"].as_str()?), thumb: c["thumb"].as_str().map(PathBuf::from),
            size: c["size"].as_u64().unwrap_or(0), created: c["created"].as_i64(), curve: c["curve"].as_str().map(str::to_string) })).collect();
        add(&root, &album, &list, &cache, luts.as_deref())
    }).await.map_err(|e| e.to_string())?
}

/// "Move to an album…": the selected links' files into another album
#[tauri::command]
pub async fn album_move(shared: tauri::State<'_, crate::Shared>, album: String, files: Vec<String>) -> Result<Value, String> {
    let root = shared.lock().unwrap().project.clone().ok_or("Open a project first")?;
    tauri::async_runtime::spawn_blocking(move || move_to(&root, &album, &files.iter().map(PathBuf::from).collect::<Vec<_>>()))
        .await.map_err(|e| e.to_string())?
}

/// "Add to another album…" for moments and photos: a copy of each link file there
#[tauri::command]
pub async fn album_copy(shared: tauri::State<'_, crate::Shared>, album: String, files: Vec<String>) -> Result<Value, String> {
    let root = shared.lock().unwrap().project.clone().ok_or("Open a project first")?;
    tauri::async_runtime::spawn_blocking(move || copy_to(&root, &album, &files.iter().map(PathBuf::from).collect::<Vec<_>>()))
        .await.map_err(|e| e.to_string())?
}

/// "Photos of the marks, as links" and a mark's "Make a photo": photo link files, made at once. `photos`: what the
/// page knows of each, { tree, path, thumb, size, created, curve, t, crop, camera }
#[tauri::command]
pub async fn photo_add(app: tauri::AppHandle, shared: tauri::State<'_, crate::Shared>, probes: tauri::State<'_, crate::index::SharedProbes>,
                       album: String, photos: Vec<Value>) -> Result<Value, String> {
    use tauri::Manager;
    let root = shared.lock().unwrap().project.clone().ok_or("Open a project first")?;
    let cache = app.path().app_cache_dir().map_err(|e| e.to_string())?.join("thumbs");
    let luts = luts_dir(probes.lock().unwrap().tool.as_deref());
    tauri::async_runtime::spawn_blocking(move || {
        let list: Vec<Photo> = photos.iter().filter_map(|c| Some(Photo {
            clip: Add { tree: c["tree"].as_str()?.to_string(), path: PathBuf::from(c["path"].as_str()?), thumb: c["thumb"].as_str().map(PathBuf::from),
                        size: c["size"].as_u64().unwrap_or(0), created: c["created"].as_i64(), curve: c["curve"].as_str().map(str::to_string) },
            t: c["t"].as_f64()?, crop: c.get("crop").cloned(), camera: c.get("camera").cloned() })).collect();
        add_photos(&root, &album, &list, &cache, luts.as_deref())
    }).await.map_err(|e| e.to_string())?
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    pub(crate) fn jpeg(w: u16, h: u16, rgb: [u8; 3]) -> Vec<u8> {
        let px: Vec<u8> = (0..w as usize * h as usize).flat_map(|_| rgb).collect();
        let mut out = Vec::new();
        jpeg_encoder::Encoder::new(&mut out, 90).encode(&px, w, h, jpeg_encoder::ColorType::Rgb).unwrap();
        out
    }

    fn scratch(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("afl-albums-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn a_link_keeps_its_picture_and_its_link() {
        let dir = scratch("link");
        let pic = jpeg(64, 36, [200, 40, 40]);
        assert_eq!(jpeg_size(&pic), Some((64, 36)));
        let link = r#"{"v":1,"kind":"clip","clip":"Day 1/A.MP4","size":5}"#;
        let bytes = link_file(&pic, link).unwrap();
        assert!(bytes.len() < pic.len() + 1200, "nothing else in it: {} bytes over the picture", bytes.len() - pic.len());
        let f = dir.join("A.afplink.mp4");
        std::fs::write(&f, &bytes).unwrap();
        // The link, as the Library reads every link's (and the catalog, and the player's link.lua: moov/meta, mdta)
        assert_eq!(crate::index::link(&f).unwrap()["clip"], "Day 1/A.MP4");
        assert_eq!(crate::index::keys_and_cover(&f).0.get(LINK_KEY).map(String::as_str), Some(link));
        // …and as the project's catalog reads it (afp-catalog: what the views are listed from)
        assert_eq!(afp_catalog::inspect::read(&f).unwrap().unwrap()["link"]["clip"], "Day 1/A.MP4");
        assert_eq!(picture(&f).as_deref(), Some(&pic[..]), "its frame is the JPEG, byte for byte");
        // A moment's or flip-book's MP4 (no JPEG frame): no picture this way
        let other = dir.join("B.afplink.mp4");
        std::fs::write(&other, crate::index::tests::tests_mp4_with_link(link)).unwrap();
        assert!(picture(&other).is_none());
        assert!(link_file(b"not a jpeg", link).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn times_as_links_keep_them() {
        assert_eq!(iso(crate::index::parse_time("2026-10-22T13:28:42Z").unwrap()), "2026-10-22T13:28:42.000000Z");
        assert_eq!(crate::index::parse_time(&iso(951_782_400)), Some(951_782_400)); // 2000-02-29
    }

    fn write_cube(path: &Path, n: usize, f: impl Fn([f32; 3]) -> [f32; 3]) {
        let mut t = format!("# test\nLUT_3D_SIZE {n}\n");
        for b in 0..n { for g in 0..n { for r in 0..n {
            let o = f([r as f32 / (n - 1) as f32, g as f32 / (n - 1) as f32, b as f32 / (n - 1) as f32]);
            t += &format!("{} {} {}\n", o[0], o[1], o[2]);
        } } }
        std::fs::write(path, t).unwrap();
    }

    #[test]
    fn a_known_curve_turns_the_thumbnail_to_normal_colour() {
        let dir = scratch("lut");
        // A "log" LUT that swaps red and blue, then NBCU's as the identity in its own domain
        write_cube(&dir.join("testlog_hlg.cube"), 9, |c| [c[2], c[1], c[0]]);
        write_cube(&dir.join(NBCU_HLG), 5, |c| c);
        let lut = lut_for(&dir, "testlog").expect("composed");
        let o = lut.at([0.8, 0.5, 0.2]);
        assert!((o[0] - 0.2).abs() < 0.01 && (o[1] - 0.5).abs() < 0.01 && (o[2] - 0.8).abs() < 0.01, "{o:?}");
        assert!(lut_for(&dir, "nocurve").is_none() && lut_for(&dir, "../x").is_none());
        let out = recolour(&jpeg(32, 18, [200, 100, 30]), &lut).unwrap();
        assert_eq!(jpeg_size(&out), Some((32, 18)));
        use zune_jpeg::zune_core::bytestream::ZCursor;
        let px = zune_jpeg::JpegDecoder::new(ZCursor::new(&out[..])).decode().unwrap();
        assert!(px[0] < 60 && px[2] > 170, "red and blue swapped: {:?}", &px[..3]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn clips_added_to_an_album_moved_and_found() {
        let root = scratch("add");
        std::fs::write(root.join("Project.afproject"), "{}").unwrap();
        let cache = root.join("cache");
        let clips: Vec<Add> = ["Day 1/A.MP4", "Day 2/A.MP4", "Day 2/B.MP4"].iter().map(|t| {
            let p = root.join("footage").join(t);
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            std::fs::write(&p, b"clip").unwrap();
            // The camera's thumbnail beside it: taken as it is
            let thm = p.with_extension("THM");
            std::fs::write(&thm, jpeg(64, 36, [10, 200, 10])).unwrap();
            Add { tree: t.to_string(), path: p, thumb: Some(thm), size: 4, created: Some(1_761_139_722), curve: None }
        }).collect();
        let r = add(&root, "_Clips/Egypt", &clips, &cache, None).unwrap();
        assert_eq!(r["made"].as_array().unwrap().len(), 3, "{r}");
        assert_eq!(r["album"], "_Clips/Egypt");
        // Two clips of one name: the second's link gets a number
        let names: Vec<String> = r["made"].as_array().unwrap().iter().map(|p| Path::new(p.as_str().unwrap()).file_name().unwrap().to_string_lossy().into_owned()).collect();
        assert_eq!(names, ["A 2.afplink.mp4", "A.afplink.mp4", "B.afplink.mp4"]);
        let first = root.join("_Clips/Egypt/A.afplink.mp4");
        let l = crate::index::link(&first).unwrap();
        assert_eq!((l["kind"].as_str(), l["clip"].as_str(), l["size"].as_u64()), (Some("clip"), Some("Day 1/A.MP4"), Some(4)));
        assert_eq!(l["created"], "2025-10-22T13:28:42.000000Z");
        assert_eq!(picture(&first).unwrap(), std::fs::read(&clips[0].thumb.clone().unwrap()).unwrap());
        // Again: each is there already, nothing written
        let again = add(&root, "_Clips/Egypt", &clips, &cache, None).unwrap();
        assert_eq!((again["made"].as_array().unwrap().len(), again["already"].as_u64()), (0, Some(3)));
        // No picture to take: a dark frame, the link all the same
        let bare = Add { tree: "Day 3/C.MP4".into(), path: root.join("nowhere/C.MP4"), thumb: None, size: 9, created: None, curve: None };
        let r = add(&root, "_Clips/Egypt/Dives", &[bare], &cache, None).unwrap();
        assert_eq!(r["made"].as_array().unwrap().len(), 1, "{r}");
        // Moved into another album; the album's listing finds them all
        let moved = move_to(&root, "_Clips/Best", &[first.clone(), root.join("_Clips/Egypt/B.afplink.mp4")]).unwrap();
        assert_eq!(moved["moved"], 2, "{moved}");
        assert!(!first.exists() && root.join("_Clips/Best/A.afplink.mp4").is_file());
        assert!(move_to(&root, "_Clips/Best", &[root.join("footage/Day 1/A.MP4")]).unwrap()["problems"].as_array().unwrap().len() == 1, "footage isn't moved");
        assert!(add(&root, "", &[], &cache, None).is_err() && add(&root, "../out", &[], &cache, None).is_err());
        let files: Vec<(String, u64, i64)> = ["_Clips/Best/A.afplink.mp4", "_Clips/Best/B.afplink.mp4", "_Clips/Egypt/A 2.afplink.mp4", "_Clips/Egypt/Dives/C.afplink.mp4"]
            .iter().map(|r| (r.to_string(), 1, 1)).collect();
        // The footage view lists every link, wherever it is: what the Albums view is made of
        let view = crate::index::scan(&root, "footage", false);
        let rels: Vec<&str> = view["links"].as_array().unwrap().iter().map(|l| l["rel"].as_str().unwrap()).collect();
        assert_eq!(rels, ["_Clips/Best/A.afplink.mp4", "_Clips/Best/B.afplink.mp4", "_Clips/Egypt/A 2.afplink.mp4", "_Clips/Egypt/Dives/C.afplink.mp4"], "{view}");
        assert!(view["clips_dirs"].as_array().unwrap().iter().any(|d| d == "_Clips/Egypt/Dives"));
        let links = clip_links(&root, &files);
        assert_eq!(links.iter().map(|l| l["clip"].as_str().unwrap()).collect::<Vec<_>>(), ["Day 1/A.MP4", "Day 2/B.MP4", "Day 2/A.MP4", "Day 3/C.MP4"]);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn photos_as_links_and_every_kind_moved_and_copied() {
        let root = scratch("photos");
        std::fs::write(root.join("Project.afproject"), "{}").unwrap();
        let cache = root.join("cache");
        let clip = root.join("footage/Day 1/A & B.MP4");
        std::fs::create_dir_all(clip.parent().unwrap()).unwrap();
        std::fs::write(&clip, b"clip").unwrap();
        let thm = clip.with_extension("THM");
        std::fs::write(&thm, jpeg(64, 36, [10, 200, 10])).unwrap();
        let add_of = || Add { tree: "Day 1/A & B.MP4".into(), path: clip.clone(), thumb: Some(thm.clone()), size: 4, created: Some(1_761_139_722), curve: None };
        // A photo: not a frame to be had of this "clip", so its thumbnail is the picture; the link all the same
        let photos = [Photo { clip: add_of(), t: 12.3456, crop: Some(json!([0.1, 0.2, 0.5, 0.5])), camera: None }, Photo { clip: add_of(), t: 40.0, crop: None, camera: None }];
        let r = add_photos(&root, "_Stills/Generated", &photos, &cache, None).unwrap();
        assert_eq!(r["made"].as_array().unwrap().len(), 2, "{r}");
        let file = root.join("_Stills/Generated/A & B_12.35s.afplink.jpg");
        assert!(file.is_file(), "{r}");
        // Read as a rendered photo's link is: by the Library, and by the catalog
        let l = crate::index::link(&file).unwrap();
        assert_eq!((l["kind"].as_str(), l["clip"].as_str(), l["t"].as_f64(), l["size"].as_u64()), (Some("photo"), Some("Day 1/A & B.MP4"), Some(12.346), Some(4)));
        assert_eq!(l["crop"], json!([0.1, 0.2, 0.5, 0.5]));
        assert_eq!(afp_catalog::inspect::read(&file).unwrap().unwrap()["link"], l);
        // Still a picture: the frame is the one put in, an XMP segment more
        let bytes = std::fs::read(&file).unwrap();
        assert_eq!(jpeg_size(&bytes), Some((64, 36)));
        assert_eq!(bytes.len(), std::fs::read(&thm).unwrap().len() + photo_file(&jpeg(64, 36, [0, 0, 0]), &l.to_string()).unwrap().len() - jpeg(64, 36, [0, 0, 0]).len());
        // A link written again replaces the one there
        let twice = photo_file(&bytes, "{\"v\":1}").unwrap();
        assert_eq!(String::from_utf8_lossy(&twice).matches("<afp:link>").count(), 1);
        // Again: there already
        assert_eq!(add_photos(&root, "_Stills/Generated", &photos, &cache, None).unwrap()["already"], 2);
        // The photos' view lists it, as a link
        let view = crate::index::scan(&root, "stills", false);
        let it = view["items"].as_array().unwrap().iter().find(|i| i["name"] == "A & B_12.35s.afplink.jpg").unwrap();
        assert_eq!((it["kind"].as_str(), it["form"].as_str(), it["linked"].as_bool()), (Some("still"), Some("Photo link"), Some(true)));
        // Renamed: its ending kept whole
        let renamed = crate::rename_in(&root, "stills", "still", &root.join("_Stills/Generated/A & B_40.00s.afplink.jpg").display().to_string(), "Reef").unwrap();
        assert!(renamed.ends_with("Reef.afplink.jpg"), "{renamed}");

        // A moment with its own Gyroflow project and a render, and a Live Photo: moved into an album whole
        let moment = root.join("_Moments/Generated/Turtle.afplink.mp4");
        std::fs::create_dir_all(moment.parent().unwrap()).unwrap();
        std::fs::write(&moment, link_file(&jpeg(64, 36, [1, 2, 3]), &json!({ "v": 1, "kind": "moment", "clip": "Day 1/A & B.MP4", "in": 1.0, "out": 4.0 }).to_string()).unwrap()).unwrap();
        std::fs::write(moment.with_file_name("Turtle.gyroflow"), "{}").unwrap();
        let render = crate::index::render_path(&root, "_Moments/Generated/Turtle.afplink.mp4", "HDR");
        std::fs::create_dir_all(render.parent().unwrap()).unwrap();
        std::fs::write(&render, b"render").unwrap();
        let live = root.join("_Stills/Generated/Ray.heic");
        std::fs::write(&live, b"heic").unwrap();
        std::fs::write(live.with_extension("mov"), b"mov").unwrap();
        let moved = move_to(&root, "_Clips/Egypt", &[moment.clone(), live.clone(), file.clone()]).unwrap();
        assert_eq!((moved["moved"].as_u64(), moved["problems"].as_array().unwrap().len()), (Some(3), 0), "{moved}");
        for f in ["Turtle.afplink.mp4", "Turtle.gyroflow", "Ray.heic", "Ray.mov", "A & B_12.35s.afplink.jpg"] { assert!(root.join("_Clips/Egypt").join(f).is_file(), "{f}"); }
        assert!(!render.exists() && crate::index::render_path(&root, "_Clips/Egypt/Turtle.afplink.mp4", "HDR").is_file(), "the render follows its moment");
        // Found where they are now, by what they are
        assert_eq!(crate::index::scan(&root, "moments", false)["items"][0]["id"], "_Clips/Egypt/Turtle.afplink.mp4");
        assert_eq!(crate::index::scan(&root, "moments", false)["items"][0]["renders"]["HDR"], "stale");
        let stills = crate::index::scan(&root, "stills", false);
        let ids: Vec<&str> = stills["items"].as_array().unwrap().iter().map(|i| i["id"].as_str().unwrap()).collect();
        assert!(ids.contains(&"_Clips/Egypt/Ray.heic") && ids.contains(&"_Clips/Egypt/A & B_12.35s.afplink.jpg"), "{ids:?}");
        assert_eq!(stills["items"].as_array().unwrap().iter().find(|i| i["id"] == "_Clips/Egypt/Ray.heic").unwrap()["form"], "Live Photo");
        // In another album too: links are copied (separate items), a rendered photo isn't
        let c = copy_to(&root, "_Clips/Best", &[root.join("_Clips/Egypt/Turtle.afplink.mp4"), root.join("_Clips/Egypt/A & B_12.35s.afplink.jpg"), root.join("_Clips/Egypt/Ray.heic")]).unwrap();
        assert_eq!((c["copied"].as_u64(), c["problems"].as_array().unwrap().len()), (Some(2), 1), "{c}");
        assert!(root.join("_Clips/Best/Turtle.gyroflow").is_file() && root.join("_Clips/Egypt/Turtle.afplink.mp4").is_file());
        // Footage is neither moved nor copied
        assert_eq!(move_to(&root, "_Clips/Best", &[clip.clone()]).unwrap()["moved"], 0);
        let _ = std::fs::remove_dir_all(&root);
    }

    /// How long links take over a folder of clips: AFL_ALBUM_DIR (its .mp4/.mov, sorted), into a scratch project beside
    /// the test's temp folder. First with no thumbnail kept (each taken as the cards take it: AFL_THUMBS_HELPER, the
    /// built afp-thumbs, else ffmpeg), then again into another album with the thumbnails kept, as after the clips were
    /// seen in the window. AFL_LUTS and AFL_ALBUM_CURVE: each thumbnail through that curve's LUT too
    #[test]
    #[ignore]
    fn links_timing() {
        let dir = PathBuf::from(std::env::var("AFL_ALBUM_DIR").expect("AFL_ALBUM_DIR"));
        let curve = std::env::var("AFL_ALBUM_CURVE").ok();
        let luts = luts_dir(None);
        let mut files: Vec<PathBuf> = Vec::new();
        fn walk(d: &Path, out: &mut Vec<PathBuf>) {
            let mut es: Vec<_> = std::fs::read_dir(d).into_iter().flatten().flatten().map(|e| e.path()).collect();
            es.sort();
            for p in es {
                let n = p.file_name().unwrap().to_string_lossy().to_lowercase();
                if p.is_dir() { walk(&p, out); } else if n.ends_with(".mp4") || n.ends_with(".mov") { out.push(p); }
            }
        }
        walk(&dir, &mut files);
        let root = scratch("timing");
        std::fs::write(root.join("Project.afproject"), "{}").unwrap();
        let clips: Vec<Add> = files.iter().map(|p| Add { tree: p.strip_prefix(&dir).unwrap().to_string_lossy().into_owned(), path: p.clone(),
            thumb: crate::index::camera_thumb(p), size: std::fs::metadata(p).unwrap().len(), created: crate::index::created(p), curve: curve.clone() }).collect();
        for (album, what) in [("_Clips/Cold", "no thumbnail kept"), ("_Clips/Warm", "thumbnails kept")] {
            let t = std::time::Instant::now();
            let r = add(&root, album, &clips, &root.join("cache"), luts.as_deref()).unwrap();
            let (n, ms) = (r["made"].as_array().unwrap().len(), t.elapsed().as_secs_f64() * 1000.0);
            let bytes: u64 = r["made"].as_array().unwrap().iter().map(|p| std::fs::metadata(p.as_str().unwrap()).unwrap().len()).sum();
            println!("{what}: {n} links in {ms:.0} ms, {:.1} ms a clip, {:.1} KB a link, {} failed", ms / n.max(1) as f64, bytes as f64 / 1024.0 / n.max(1) as f64, r["failed"].as_array().unwrap().len());
        }
        println!("the project: {}", root.display());
    }
}
