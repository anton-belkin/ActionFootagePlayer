// SPDX-License-Identifier: GPL-3.0-or-later
//
// Still photos from a frame (gyroflow-export --photo): the frame in two renditions, the SDR look and the HDR one
// (HLG), and the two as one file that shows either, as the screen can: an SDR image with a gain map, the per-pixel
// ratio from it to the HDR one (ISO 21496-1). Apps and screens without HDR show the SDR image as any JPEG; with HDR,
// they raise it towards the HDR one by as much as the screen has room for.
//
// Here: the YUV frame to RGB (as mpv decodes it), the looks' LUTs on the CPU (the same .cube files the player's shaders
// are baked from and gyroflow-export uses), and the Ultra HDR JPEG (Android's name for a JPEG with an ISO 21496-1 gain
// map; Apple, Chrome and Android read it): the SDR image, the gain map as a second JPEG after it, MPF (CIPA DC-007) to
// say where it is, the gain map's metadata in an APP2 segment of each, as libultrahdr 2 writes them (its reference
// files were taken apart for the layout). macOS's HEIC is made by macos/photo.swift from the two renditions
// (write_rgb16), ImageIO computing its own gain map.

use crate::gpu_lut;

/// A frame in RGB, row by row, each value the signal (0..1, non-linear)
#[derive(Clone)]
pub struct Image {
    pub width: usize,
    pub height: usize,
    pub pixels: Vec<[f32; 3]>,
}

/// A .cube 3D LUT, applied by tetrahedral interpolation as the player's shaders and gyroflow-export's do
pub struct Cube {
    n: usize,
    data: Vec<f32>,
}

impl Cube {
    pub fn load(path: &std::path::Path) -> Result<Cube, String> {
        let (n, data) = gpu_lut::read_cube(path)?;
        Ok(Cube { n: n as usize, data })
    }
    pub fn apply(&self, image: &Image) -> Image {
        Image { width: image.width, height: image.height, pixels: image.pixels.iter().map(|&p| gpu_lut::tetrahedral(self.n, &self.data, p)).collect() }
    }
    /// A Type III LUT (NBCU's): the signal's 0..1 is (64 + 876 x) / 1023 of its domain, in and out (gpu_lut::compose_nbcu)
    pub fn apply_type3(&self, image: &Image) -> Image {
        let (lo, span) = (64.0 / 1023.0, 876.0 / 1023.0);
        Image { width: image.width, height: image.height, pixels: image.pixels.iter().map(|&p| {
            let o = gpu_lut::tetrahedral(self.n, &self.data, p.map(|v| v * span + lo));
            o.map(|v| ((v - lo) / span).clamp(0.0, 1.0))
        }).collect() }
    }
}

/// The YUV matrix's Kr and Kb, by gyroflow-export's names (yuv_matrix)
fn kr_kb(matrix: &str) -> (f32, f32) {
    match matrix {
        "bt2020" => (0.2627, 0.0593),
        "bt601" => (0.299, 0.114),
        _ => (0.2126, 0.0722),
    }
}

/// 10-bit 4:2:0 planes (Y, then U and V at half size, no padding) to RGB, of the rectangle `rect` (x, y, width,
/// height in pixels): the matrix and range the video has, chroma interpolated between its samples (MPEG-2's siting:
/// with the luma's left column, between its rows)
pub fn yuv420p10_to_rgb(planes: [&[u16]; 3], width: usize, height: usize, rect: (usize, usize, usize, usize), matrix: &str, full_range: bool) -> Image {
    let (cw, ch) = (width / 2, height / 2);
    let (kr, kb) = kr_kb(matrix);
    let kg = 1.0 - kr - kb;
    let (y_off, y_scale, c_scale) = if full_range { (0.0, 1023.0, 1023.0) } else { (64.0, 876.0, 896.0) };
    let chroma = |plane: &[u16], x: usize, y: usize| -> f32 {
        let cx = (x as f32 / 2.0).min((cw - 1) as f32);
        let cy = ((y as f32 + 0.5) / 2.0 - 0.5).clamp(0.0, (ch - 1) as f32);
        let (x0, y0) = (cx as usize, cy as usize);
        let (x1, y1) = ((x0 + 1).min(cw - 1), (y0 + 1).min(ch - 1));
        let (fx, fy) = (cx - x0 as f32, cy - y0 as f32);
        let at = |x: usize, y: usize| plane[y * cw + x] as f32;
        let top = at(x0, y0) + (at(x1, y0) - at(x0, y0)) * fx;
        let bottom = at(x0, y1) + (at(x1, y1) - at(x0, y1)) * fx;
        top + (bottom - top) * fy
    };
    let (rx, ry, rw, rh) = rect;
    let mut pixels = Vec::with_capacity(rw * rh);
    for y in ry..ry + rh {
        for x in rx..rx + rw {
            let yy = (planes[0][y * width + x] as f32 - y_off) / y_scale;
            let cb = (chroma(planes[1], x, y) - 512.0) / c_scale;
            let cr = (chroma(planes[2], x, y) - 512.0) / c_scale;
            let r = yy + 2.0 * (1.0 - kr) * cr;
            let b = yy + 2.0 * (1.0 - kb) * cb;
            let g = (yy - kr * r - kb * b) / kg;
            pixels.push([r.clamp(0.0, 1.0), g.clamp(0.0, 1.0), b.clamp(0.0, 1.0)]);
        }
    }
    Image { width: rw, height: rh, pixels }
}

/// HLG's signal to scene light (BT.2100's inverse OETF)
fn hlg_inverse_oetf(e: f32) -> f32 {
    const A: f32 = 0.178_832_77;
    const B: f32 = 0.284_668_92;
    const C: f32 = 0.559_910_7;
    if e <= 0.5 { e * e / 3.0 } else { (((e - C) / A).exp() + B) / 12.0 }
}

fn srgb_to_linear(v: f32) -> f32 {
    if v <= 0.04045 { v / 12.92 } else { ((v + 0.055) / 1.055).powf(2.4) }
}

/// The HDR rendition's peak, nits: HLG as graded, for a 1000-nit screen
pub const HLG_PEAK_NITS: f32 = 1000.0;
/// Where SDR's white is in HDR (BT.2408): the gain map's 1
pub const SDR_WHITE_NITS: f32 = 203.0;

/// HLG (BT.2020) to display light in BT.709's primaries, in SDR whites: the OOTF for a 1000-nit screen (system gamma
/// 1.2), the colors outside BT.709 clipped
fn hlg_to_linear_709(p: [f32; 3]) -> [f32; 3] {
    let s = p.map(hlg_inverse_oetf);
    let ys = 0.2627 * s[0] + 0.6780 * s[1] + 0.0593 * s[2];
    let k = HLG_PEAK_NITS / SDR_WHITE_NITS * ys.max(1e-6).powf(0.2);
    let d = s.map(|v| v * k);
    [
        (1.6605 * d[0] - 0.5876 * d[1] - 0.0728 * d[2]).max(0.0),
        (-0.1246 * d[0] + 1.1329 * d[1] - 0.0083 * d[2]).max(0.0),
        (-0.0182 * d[0] - 0.1006 * d[1] + 1.1187 * d[2]).max(0.0),
    ]
}

/// The gain map's offsets (k_sdr, k_hdr): keep the ratio sane in the blacks, as Adobe's and Apple's do
const OFFSET: f32 = 1.0 / 64.0;
/// The most the gain map raises or lowers by, stops
const MAX_STOPS: f32 = 6.0;
/// The gain map's resolution: one pixel for this many each way
pub const GAIN_MAP_SCALE: usize = 4;

/// The gain map: per color, log2 of the HDR rendition over the SDR one, in `min..max` as 0..255
pub struct GainMap {
    pub width: usize,
    pub height: usize,
    pub rgb: Vec<u8>,
    pub min: [f32; 3],
    pub max: [f32; 3],
    /// log2 of the HDR rendition's headroom over SDR's white
    pub hdr_headroom: f32,
}

/// From the SDR rendition (BT.709, an sRGB screen's signal, as the base image is shown) to the HDR one (HLG, BT.2020)
pub fn gain_map(sdr: &Image, hdr: &Image, scale: usize) -> GainMap {
    assert_eq!((sdr.width, sdr.height), (hdr.width, hdr.height));
    let (w, h) = (sdr.width.div_ceil(scale), sdr.height.div_ceil(scale));
    // Each block's mean log2 ratio, per color
    let mut logs = vec![[0f32; 3]; w * h];
    for by in 0..h {
        for bx in 0..w {
            let mut sum = [0f32; 3];
            let mut n = 0.0;
            for y in by * scale..((by + 1) * scale).min(sdr.height) {
                for x in bx * scale..((bx + 1) * scale).min(sdr.width) {
                    let i = y * sdr.width + x;
                    let s = sdr.pixels[i].map(srgb_to_linear);
                    let d = hlg_to_linear_709(hdr.pixels[i]);
                    for c in 0..3 { sum[c] += ((d[c] + OFFSET) / (s[c] + OFFSET)).log2(); }
                    n += 1.0;
                }
            }
            logs[by * w + bx] = sum.map(|v| v / n);
        }
    }
    let hdr_headroom = (HLG_PEAK_NITS / SDR_WHITE_NITS).log2();
    let mut min = [f32::MAX; 3];
    let mut max = [f32::MIN; 3];
    for l in &logs {
        for c in 0..3 { min[c] = min[c].min(l[c]); max[c] = max[c].max(l[c]); }
    }
    // A range that isn't empty, within what 8 bits spread over can hold. The boost may go past the HDR rendition's
    // headroom (dark in SDR, bright in HDR): a reader scales it all by how much of the headroom its screen has
    for c in 0..3 {
        min[c] = min[c].max(-MAX_STOPS);
        max[c] = max[c].clamp(min[c] + 1.0 / 64.0, MAX_STOPS);
    }
    let mut rgb = Vec::with_capacity(w * h * 3);
    for l in &logs {
        for c in 0..3 {
            let v = ((l[c] - min[c]) / (max[c] - min[c])).clamp(0.0, 1.0);
            rgb.push((v * 255.0).round() as u8);
        }
    }
    GainMap { width: w, height: h, rgb, min, max, hdr_headroom }
}

/// The signal (0..1) as 8-bit RGB
pub fn to_rgb8(image: &Image) -> Vec<u8> {
    image.pixels.iter().flat_map(|p| p.map(|v| (v.clamp(0.0, 1.0) * 255.0).round() as u8)).collect()
}

/// The signal (0..1) as 16-bit RGB, little-endian, for macos/photo.swift
pub fn write_rgb16(path: &std::path::Path, image: &Image) -> Result<(), String> {
    let bytes: Vec<u8> = image.pixels.iter().flat_map(|p| p.map(|v| (v.clamp(0.0, 1.0) * 65535.0).round() as u16))
        .flat_map(|v| v.to_le_bytes()).collect();
    std::fs::write(path, bytes).map_err(|e| format!("{}: {e}", path.display()))
}

// ---------------------------------------------------------------------------------------------------
// The Ultra HDR JPEG

const ISO_URN: &[u8] = b"urn:iso:std:iso:ts:21496:-1\0";

/// ISO 21496-1's metadata: its versions alone (in the primary image), or all of it (in the gain map's). Fractions
/// over 2^16 (the format's numerators and denominators: no common denominator); the gain map applies in the base
/// image's colors (BT.709)
fn iso_metadata(map: Option<&GainMap>) -> Vec<u8> {
    let mut v = ISO_URN.to_vec();
    v.extend_from_slice(&[0, 0, 0, 0]); // minimum_version, writer_version
    let Some(m) = map else { return v };
    const D: f32 = 65536.0;
    let signed = |v: &mut Vec<u8>, x: f32| { v.extend_from_slice(&((x * D).round() as i32).to_be_bytes()); v.extend_from_slice(&(D as u32).to_be_bytes()); };
    let unsigned = |v: &mut Vec<u8>, x: f32| { v.extend_from_slice(&((x * D).round().max(0.0) as u32).to_be_bytes()); v.extend_from_slice(&(D as u32).to_be_bytes()); };
    // is_multichannel, use_base_colour_space
    v.push(0x80 | 0x40);
    unsigned(&mut v, 0.0); // base_hdr_headroom: the base is SDR
    unsigned(&mut v, m.hdr_headroom);
    for c in 0..3 {
        signed(&mut v, m.min[c]);
        signed(&mut v, m.max[c]);
        unsigned(&mut v, 1.0); // gamma
        signed(&mut v, OFFSET);
        signed(&mut v, OFFSET);
    }
    v
}

/// MPF's segment (APP2): two images, the primary at 0 and the gain map `offset` bytes after the TIFF header that
/// follows "MPF\0"
fn mpf(primary_size: u32, map_size: u32, map_offset: u32) -> Vec<u8> {
    let mut v = b"MPF\0".to_vec();
    v.extend_from_slice(b"MM\0\x2a"); // big-endian TIFF
    v.extend_from_slice(&8u32.to_be_bytes()); // the IFD right after
    v.extend_from_slice(&3u16.to_be_bytes());
    let entry = |v: &mut Vec<u8>, tag: u16, kind: u16, count: u32, value: [u8; 4]| {
        v.extend_from_slice(&tag.to_be_bytes()); v.extend_from_slice(&kind.to_be_bytes());
        v.extend_from_slice(&count.to_be_bytes()); v.extend_from_slice(&value);
    };
    entry(&mut v, 0xB000, 7, 4, *b"0100"); // MPF version
    entry(&mut v, 0xB001, 4, 1, 2u32.to_be_bytes()); // number of images
    // The MP entries after the IFD: 8 (header) + 2 + 3 * 12 + 4 (no next IFD)
    entry(&mut v, 0xB002, 7, 32, 50u32.to_be_bytes());
    v.extend_from_slice(&0u32.to_be_bytes());
    let image = |v: &mut Vec<u8>, attribute: u32, size: u32, offset: u32| {
        v.extend_from_slice(&attribute.to_be_bytes()); v.extend_from_slice(&size.to_be_bytes());
        v.extend_from_slice(&offset.to_be_bytes()); v.extend_from_slice(&[0, 0, 0, 0]);
    };
    image(&mut v, 0x0003_0000, primary_size, 0); // baseline primary
    image(&mut v, 0, map_size, map_offset);
    v
}

fn jpeg(rgb: &[u8], width: usize, height: usize, quality: u8, segments: Vec<Vec<u8>>, subsampled: bool) -> Result<Vec<u8>, String> {
    let mut out = Vec::new();
    let mut enc = jpeg_encoder::Encoder::new(&mut out, quality);
    enc.set_sampling_factor(if subsampled { jpeg_encoder::SamplingFactor::R_4_2_0 } else { jpeg_encoder::SamplingFactor::R_4_4_4 });
    enc.set_optimized_huffman_tables(true);
    for s in segments { enc.add_app_segment(2, s).map_err(|e| format!("JPEG: {e}"))?; }
    enc.encode(rgb, width as u16, height as u16, jpeg_encoder::ColorType::Rgb).map_err(|e| format!("JPEG: {e}"))?;
    Ok(out)
}

/// An Ultra HDR JPEG: `sdr` as the image, and `hdr`'s gain map if there's one (else a plain JPEG). `exif`: an APP1
/// Exif segment's payload, if any
pub fn ultra_hdr_jpeg(sdr: &Image, hdr: Option<&Image>, quality: u8, exif: Option<Vec<u8>>) -> Result<Vec<u8>, String> {
    let base = to_rgb8(sdr);
    let Some(hdr) = hdr else {
        let mut out = jpeg(&base, sdr.width, sdr.height, quality, vec![], true)?;
        if let Some(exif) = exif { out = insert_app1(out, exif)?; }
        return Ok(out);
    };
    let map = gain_map(sdr, hdr, GAIN_MAP_SCALE);
    let secondary = jpeg(&map.rgb, map.width, map.height, 90, vec![iso_metadata(Some(&map))], false)?;
    // The primary with a placeholder MPF of the final size, then its offsets in place
    let placeholder = mpf(0, 0, 0);
    let mut primary = jpeg(&base, sdr.width, sdr.height, quality, vec![iso_metadata(None), placeholder.clone()], true)?;
    if let Some(exif) = exif { primary = insert_app1(primary, exif)?; }
    let at = primary.windows(placeholder.len()).position(|w| w == placeholder.as_slice()).ok_or("JPEG: MPF segment not found")?;
    let tiff = at + 4;
    let real = mpf(primary.len() as u32, secondary.len() as u32, (primary.len() - tiff) as u32);
    primary[at..at + real.len()].copy_from_slice(&real);
    primary.extend_from_slice(&secondary);
    Ok(primary)
}

/// `exif` as an APP1 segment right after the JPEG's SOI and its JFIF APP0
fn insert_app1(jpeg: Vec<u8>, exif: Vec<u8>) -> Result<Vec<u8>, String> {
    if exif.len() > 65533 { return Err("Exif too large".into()); }
    let mut at = 2;
    if jpeg.get(2..4) == Some(&[0xFF, 0xE0]) { at += 2 + u16::from_be_bytes([jpeg[4], jpeg[5]]) as usize; }
    let mut out = Vec::with_capacity(jpeg.len() + exif.len() + 4);
    out.extend_from_slice(&jpeg[..at]);
    out.extend_from_slice(&[0xFF, 0xE1]);
    out.extend_from_slice(&((exif.len() + 2) as u16).to_be_bytes());
    out.extend_from_slice(&exif);
    out.extend_from_slice(&jpeg[at..]);
    Ok(out)
}

/// A minimal Exif segment's payload: when it was taken (local time, "YYYY:MM:DD HH:MM:SS"), and the camera
pub fn exif(taken: Option<&str>, make: Option<&str>, model: Option<&str>) -> Vec<u8> {
    // Big-endian TIFF: IFD0 with Make, Model and the Exif IFD's pointer, then the Exif IFD with DateTimeOriginal
    let ascii = |s: &str| { let mut b = s.as_bytes().to_vec(); b.push(0); b };
    let mut ifd0: Vec<(u16, Vec<u8>)> = Vec::new();
    if let Some(m) = make { ifd0.push((0x010F, ascii(m))); }
    if let Some(m) = model { ifd0.push((0x0110, ascii(m))); }
    let exif_ifd: Vec<(u16, Vec<u8>)> = taken.map(|t| vec![(0x9003, ascii(t)), (0x9004, ascii(t))]).unwrap_or_default();
    let ifd_size = |n: usize| 2 + n * 12 + 4;
    let ifd0_entries = ifd0.len() + 1;
    let ifd0_at = 8usize;
    let mut data_at = ifd0_at + ifd_size(ifd0_entries);
    let data_len = |e: &[(u16, Vec<u8>)]| e.iter().map(|(_, d)| if d.len() > 4 { d.len() + (d.len() & 1) } else { 0 }).sum::<usize>();
    let exif_ifd_at = data_at + data_len(&ifd0);
    let mut tiff = b"MM\0\x2a".to_vec();
    tiff.extend_from_slice(&(ifd0_at as u32).to_be_bytes());
    let write_ifd = |tiff: &mut Vec<u8>, entries: &[(u16, Vec<u8>)], extra: Option<(u16, u32)>, data_at: &mut usize| {
        let mut all: Vec<(u16, u16, u32, [u8; 4])> = Vec::new();
        let mut blobs = Vec::new();
        for (tag, d) in entries {
            let value = if d.len() <= 4 { let mut b = [0u8; 4]; b[..d.len()].copy_from_slice(d); b }
                        else { let b = (*data_at as u32).to_be_bytes(); *data_at += d.len() + (d.len() & 1); blobs.push(d.clone()); b };
            all.push((*tag, 2, d.len() as u32, value));
        }
        if let Some((tag, ptr)) = extra { all.push((tag, 4, 1, ptr.to_be_bytes())); }
        all.sort_by_key(|e| e.0);
        tiff.extend_from_slice(&(all.len() as u16).to_be_bytes());
        for (tag, kind, count, value) in all {
            tiff.extend_from_slice(&tag.to_be_bytes()); tiff.extend_from_slice(&kind.to_be_bytes());
            tiff.extend_from_slice(&count.to_be_bytes()); tiff.extend_from_slice(&value);
        }
        tiff.extend_from_slice(&0u32.to_be_bytes());
        for b in blobs { tiff.extend_from_slice(&b); if b.len() & 1 == 1 { tiff.push(0); } }
    };
    write_ifd(&mut tiff, &ifd0, Some((0x8769, exif_ifd_at as u32)), &mut data_at);
    let mut data_at = exif_ifd_at + ifd_size(exif_ifd.len());
    write_ifd(&mut tiff, &exif_ifd, None, &mut data_at);
    let mut out = b"Exif\0\0".to_vec();
    out.extend_from_slice(&tiff);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hlg_reference_white_is_sdr_white() {
        // HLG's 75 % is BT.2408's reference white, 203 nits: 1 in SDR whites
        let w = hlg_to_linear_709([0.75; 3]);
        for c in w { assert!((c - 1.0).abs() < 0.02, "{w:?}"); }
        let peak = hlg_to_linear_709([1.0; 3]);
        assert!((peak[1] - HLG_PEAK_NITS / SDR_WHITE_NITS).abs() < 0.05, "{peak:?}");
    }

    #[test]
    fn yuv_gray_and_red() {
        // Limited range: Y 64..940, C 512 gray
        let (w, h) = (4, 2);
        let y = vec![502u16; w * h];
        let c = vec![512u16; w * h / 4];
        let img = yuv420p10_to_rgb([&y, &c, &c], w, h, (0, 0, w, h), "bt709", false);
        for p in &img.pixels { for v in p { assert!((v - 0.5).abs() < 0.002, "{p:?}"); } }
        // BT.709 red: Y 0.2126, Cr 0.5
        let y = vec![(64.0 + 876.0 * 0.2126f32).round() as u16; w * h];
        let cb = vec![(512.0f32 - 896.0 * 0.2126 / (2.0 * (1.0 - 0.0722))).round() as u16; w * h / 4];
        let cr = vec![(512.0 + 896.0 * 0.5f32).round() as u16; w * h / 4];
        let img = yuv420p10_to_rgb([&y, &cb, &cr], w, h, (1, 0, 2, 1), "bt709", false);
        assert_eq!((img.width, img.height), (2, 1));
        let p = img.pixels[0];
        assert!(p[0] > 0.99 && p[1] < 0.01 && p[2] < 0.01, "{p:?}");
    }

    #[test]
    fn gain_map_round_trip() {
        // The decoder's math (ISO 21496-1) on the map gets the HDR rendition back, within the 8 bits' steps
        let (w, h) = (16, 8);
        let sdr = Image { width: w, height: h, pixels: (0..w * h).map(|i| { let v = i as f32 / (w * h) as f32; [v, v * 0.9, v * 0.7] }).collect() };
        let hdr = Image { width: w, height: h, pixels: (0..w * h).map(|i| { let v = i as f32 / (w * h) as f32; [v, v * 0.95, v * 0.8] }).collect() };
        let m = gain_map(&sdr, &hdr, 1);
        for i in 0..w * h {
            let s = sdr.pixels[i].map(srgb_to_linear);
            let d = hlg_to_linear_709(hdr.pixels[i]);
            for c in 0..3 {
                let v = m.rgb[i * 3 + c] as f32 / 255.0;
                let log = m.min[c] + (m.max[c] - m.min[c]) * v;
                let back = (s[c] + OFFSET) * log.exp2() - OFFSET;
                if (d[c] + OFFSET).log2() - (s[c] + OFFSET).log2() <= m.max[c] {
                    assert!(((back + OFFSET).log2() - (d[c] + OFFSET).log2()).abs() < (m.max[c] - m.min[c]) / 255.0 + 1e-3, "{i} {c}: {back} vs {}", d[c]);
                }
            }
        }
    }

    /// A sample to look at with other readers: PHOTO_SAMPLE=<path.jpg> cargo test --lib photo -- --ignored
    #[test]
    #[ignore]
    fn write_sample() {
        let path = std::env::var("PHOTO_SAMPLE").unwrap();
        let (w, h) = (512, 256);
        // SDR: a gradient up to white; HDR: the same up to HLG's peak, a bright band in the middle
        let sdr = Image { width: w, height: h, pixels: (0..w * h).map(|i| { let x = (i % w) as f32 / w as f32; [x, x * 0.9, x * 0.6] }).collect() };
        let hdr = Image { width: w, height: h, pixels: (0..w * h).map(|i| {
            let (x, y) = ((i % w) as f32 / w as f32, (i / w) as f32 / h as f32);
            let v = if (0.4..0.6).contains(&y) { 0.75 + 0.25 * x } else { 0.75 * x };
            [v, v * 0.95, v * 0.8] }).collect() };
        std::fs::write(&path, ultra_hdr_jpeg(&sdr, Some(&hdr), 92, Some(exif(Some("2026:11:05 10:00:00"), Some("DJI"), Some("Osmo Action 6")))).unwrap()).unwrap();
    }

    #[test]
    fn ultra_hdr_layout() {
        let (w, h) = (64, 32);
        let sdr = Image { width: w, height: h, pixels: vec![[0.5, 0.4, 0.3]; w * h] };
        let hdr = Image { width: w, height: h, pixels: vec![[0.7, 0.6, 0.5]; w * h] };
        let file = ultra_hdr_jpeg(&sdr, Some(&hdr), 90, Some(exif(Some("2026:11:05 10:00:00"), Some("DJI"), Some("Osmo Action 6")))).unwrap();
        // The MPF's entries: the primary's size, and the gain map where it says
        let at = file.windows(4).position(|w| w == b"MPF\0").unwrap();
        let tiff = at + 4;
        let entries = tiff + 50;
        let be = |i: usize| u32::from_be_bytes(file[i..i + 4].try_into().unwrap()) as usize;
        let (primary, map_size, map_offset) = (be(entries + 4), be(entries + 20), be(entries + 24));
        assert_eq!(primary + map_size, file.len());
        assert_eq!(tiff + map_offset, primary);
        assert_eq!(&file[primary..primary + 2], &[0xFF, 0xD8]);
        assert!(file[primary..].windows(ISO_URN.len()).any(|w| w == ISO_URN));
        assert!(file[..primary].windows(6).any(|w| w == b"Exif\0\0"));
    }
}
