// SPDX-License-Identifier: GPL-3.0-or-later
//
// The 360 stamp (docs/360-cameras.md, "The stamp"): a frame's camera in the world and the heading of every heading
// mode, written into the frame itself, under the sphere, for the view shader to read from the frame it draws. So the
// angles are exact through dropped frames, seeks and pauses, with no mpv patch.
//
// In the luma plane only (chroma is subsampled and left alone), in the last ROWS rows, from the left: CODES codes,
// each a block of 2x2 pixels of one value. The shader reads a block with a texel fetch; the block size keeps a code
// whole through anything short of a scaler.
//
// - The header, codes 0-6, in coarse codes: 16 levels from black to white, each at the middle of its sixteenth, so the
//   shader reads them without knowing the plane's depth (floor(v * 16)): the magic 12, 3, 9, 6, then the depth the
//   stamp was written at less one, the bits per fine code less one, the version.
// - The fields, codes 7-31, in fine codes of `bits` bits each (the depth's, at most MAX_BITS). A fine code with fewer
//   bits than the depth sits at the middle of its step, as a coarse one does. Fixed point, most significant code first:
//   - the frame's time in µs, 4 codes (modulo 2^(4·bits): 32 bits at 8-bit, 40 at 10, 64 at 16);
//   - q_world_from_cam, [w, x, y, z], 2 codes a component, [-1, 1] over the codes' range;
//   - per heading mode (Heading), its yaw in the world, 2 codes, a turn over the codes' range (wrapping); then its
//     confidence, 1 code, [0, 1] over the code's range;
//   - a check: the sum of the fields' codes, modulo 2^bits.
//
// Two codes are 20 bits at 10-bit depth: a quaternion component to ~2·10⁻⁶, a ten-thousandth of a degree.
//
// shaders/stamp_probe.glsl reads it on screen; tools/stamp360/ tests that gpu-next hands it to a shader unchanged.

use std::ffi::{ c_int, c_void, CString };
use std::io::Write as _;
use std::ptr::null;

use parking_lot::Mutex;
use vapoursynth4_sys::*;

pub const VERSION: u32 = 1;
/// Pixel rows the stamp takes at the bottom of the luma plane
pub const ROWS: usize = 2;
/// A code's block is BLOCK x BLOCK pixels
pub const BLOCK: usize = 2;
pub const CODES: usize = 32;
/// Pixels the stamp takes across
pub const WIDTH: usize = CODES * BLOCK;
/// Bits a fine code carries at most, whatever the depth
pub const MAX_BITS: u32 = 16;

pub const MAGIC: [u32; 4] = [12, 3, 9, 6];
const HEADER: usize = 7;
const TIME: usize = HEADER;
const QUAT: usize = TIME + 4;
const MODES: usize = QUAT + 8;
const CHECK: usize = MODES + HEADINGS * 3;
const _: () = assert!(CHECK + 1 == CODES);

/// The heading modes, in the stamp's order (docs/360-cameras.md, "The heading modes")
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Heading { World, Camera, Travel, North }
pub const HEADINGS: usize = 4;

#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct HeadingStamp {
    /// The heading's yaw in the world, radians
    pub yaw: f64,
    /// Whether it's valid there, 0-1
    pub confidence: f64,
}

/// What a frame carries
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Stamp {
    pub time_us: i64,
    /// The camera at mid-frame in the world, Hamilton [w, x, y, z], unit
    pub q_world_from_cam: [f64; 4],
    pub headings: [HeadingStamp; HEADINGS],
}

impl Default for Stamp {
    fn default() -> Self { Self { time_us: 0, q_world_from_cam: [1.0, 0.0, 0.0, 0.0], headings: Default::default() } }
}

/// The bits of a fine code at a depth
pub fn bits_for(depth: u32) -> u32 { depth.min(MAX_BITS) }

fn mask(bits: u32) -> u64 { if bits >= 64 { u64::MAX } else { (1u64 << bits) - 1 } }

/// The stamp's fields as the fixed-point numbers written, at `bits` per code. What a reader gets back exactly
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct Fixed {
    pub bits: u32,
    /// The time in µs, modulo 2^(4·bits)
    pub time: u64,
    pub q: [u64; 4],
    pub yaw: [u64; HEADINGS],
    pub confidence: [u64; HEADINGS],
}

impl Fixed {
    pub fn from_stamp(s: &Stamp, bits: u32) -> Self {
        let two = mask(2 * bits) as f64;
        let turn = (1u64 << (2 * bits)) as f64;
        let q = s.q_world_from_cam.map(|c| ((c.clamp(-1.0, 1.0) + 1.0) * 0.5 * two).round() as u64);
        let yaw = s.headings.map(|h| {
            let t = (h.yaw / std::f64::consts::TAU).rem_euclid(1.0);
            ((t * turn).round() as u64) & mask(2 * bits)
        });
        let confidence = s.headings.map(|h| (h.confidence.clamp(0.0, 1.0) * mask(bits) as f64).round() as u64);
        Self { bits, time: (s.time_us as u64) & mask(4 * bits), q, yaw, confidence }
    }

    /// Back to angles: each within half a step of what was written
    pub fn to_stamp(&self) -> Stamp {
        let two = mask(2 * self.bits) as f64;
        let turn = (1u64 << (2 * self.bits)) as f64;
        let mut headings = [HeadingStamp::default(); HEADINGS];
        for (i, h) in headings.iter_mut().enumerate() {
            let t = self.yaw[i] as f64 / turn;
            h.yaw = (if t > 0.5 { t - 1.0 } else { t }) * std::f64::consts::TAU;
            h.confidence = self.confidence[i] as f64 / mask(self.bits) as f64;
        }
        Stamp { time_us: self.time as i64, q_world_from_cam: self.q.map(|n| n as f64 / two * 2.0 - 1.0), headings }
    }

    /// Whether the time read back is `time_us`, modulo what the stamp keeps of it
    pub fn time_is(&self, time_us: i64) -> bool { self.time == (time_us as u64) & mask(4 * self.bits) }

    /// The fields' levels in code order, codes HEADER.. CHECK
    fn levels(&self) -> [u64; CHECK - HEADER] {
        let b = self.bits;
        let m = mask(b);
        let mut l = [0u64; CHECK - HEADER];
        for i in 0..4 { l[i] = (self.time >> (b * (3 - i as u32))) & m; }
        for c in 0..4 {
            l[QUAT - HEADER + 2 * c] = self.q[c] >> b;
            l[QUAT - HEADER + 2 * c + 1] = self.q[c] & m;
        }
        for h in 0..HEADINGS {
            l[MODES - HEADER + 3 * h] = self.yaw[h] >> b;
            l[MODES - HEADER + 3 * h + 1] = self.yaw[h] & m;
            l[MODES - HEADER + 3 * h + 2] = self.confidence[h];
        }
        l
    }

    /// The codes at `depth`
    pub fn codes(&self, depth: u32) -> [u32; CODES] {
        let mut codes = [0u32; CODES];
        let header = MAGIC.into_iter().chain([depth - 1, self.bits - 1, VERSION]);
        for (c, level) in codes.iter_mut().zip(header) { *c = coarse_code(level, depth); }
        let levels = self.levels();
        let check = levels.iter().sum::<u64>() & mask(self.bits);
        for (c, level) in codes[HEADER..].iter_mut().zip(levels.into_iter().chain([check])) {
            *c = fine_code(level as u32, self.bits, depth);
        }
        codes
    }

    /// From the codes of a stamp at `depth`
    pub fn from_codes(codes: &[u32; CODES], depth: u32) -> Result<Self, ReadError> {
        let header: Vec<u32> = codes[..HEADER].iter().map(|&c| coarse_level(c, depth)).collect();
        if header[..4] != MAGIC { return Err(ReadError::NoStamp); }
        if header[4] + 1 != depth { return Err(ReadError::Depth(header[4] + 1)); }
        if header[6] != VERSION { return Err(ReadError::Version(header[6])); }
        let bits = header[5] + 1;
        if bits > depth { return Err(ReadError::Bits(bits)); }
        let l: Vec<u64> = codes[HEADER..].iter().map(|&c| fine_level(c, bits, depth) as u64).collect();
        let m = mask(bits);
        let fields = &l[..CHECK - HEADER];
        if fields.iter().sum::<u64>() & m != l[CHECK - HEADER] { return Err(ReadError::Check); }
        let mut f = Fixed { bits, ..Default::default() };
        for i in 0..4 { f.time = (f.time << bits) | fields[i]; }
        for c in 0..4 { f.q[c] = (fields[QUAT - HEADER + 2 * c] << bits) | fields[QUAT - HEADER + 2 * c + 1]; }
        for h in 0..HEADINGS {
            let i = MODES - HEADER + 3 * h;
            f.yaw[h] = (fields[i] << bits) | fields[i + 1];
            f.confidence[h] = fields[i + 2];
        }
        Ok(f)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReadError {
    /// Too small for a stamp, or no magic: any other frame
    NoStamp,
    /// The pixels of a code's block differ
    Block(usize),
    /// Written at another depth than the plane's
    Depth(u32),
    Bits(u32),
    Version(u32),
    /// The check doesn't add up
    Check,
}

/// A level of `bits` at `depth`: the level itself, or the middle of its step when the depth has more bits
fn fine_code(level: u32, bits: u32, depth: u32) -> u32 {
    let s = depth - bits;
    (level << s) | if s > 0 { 1 << (s - 1) } else { 0 }
}
fn fine_level(code: u32, bits: u32, depth: u32) -> u32 { code >> (depth - bits) }
fn coarse_code(level: u32, depth: u32) -> u32 { fine_code(level, 4, depth) }
fn coarse_level(code: u32, depth: u32) -> u32 { fine_level(code, 4, depth) }

/// A plane's sample: 8 or 16 bits
pub trait Sample: Copy {
    fn from_code(c: u32) -> Self;
    fn code(self) -> u32;
}
impl Sample for u8 {
    fn from_code(c: u32) -> Self { c as u8 }
    fn code(self) -> u32 { self as u32 }
}
impl Sample for u16 {
    fn from_code(c: u32) -> Self { c as u16 }
    fn code(self) -> u32 { self as u32 }
}

/// Writes the stamp into the last ROWS rows of a luma plane of `width` x `height` samples of `depth` bits, rows
/// `stride` samples apart; false (and nothing written) when the plane is too small
pub fn write<T: Sample>(plane: &mut [T], stride: usize, width: usize, height: usize, depth: u32, stamp: &Stamp) -> bool {
    if width < WIDTH || height < ROWS || !(8..=16).contains(&depth) { return false; }
    let codes = Fixed::from_stamp(stamp, bits_for(depth)).codes(depth);
    for dy in 0..ROWS {
        let row = &mut plane[(height - ROWS + dy) * stride..][..WIDTH];
        for (x, p) in row.iter_mut().enumerate() { *p = T::from_code(codes[x / BLOCK]); }
    }
    true
}

/// Reads a stamp back, strictly: every pixel of every block
pub fn read<T: Sample>(plane: &[T], stride: usize, width: usize, height: usize, depth: u32) -> Result<Fixed, ReadError> {
    if width < WIDTH || height < ROWS || !(8..=16).contains(&depth) { return Err(ReadError::NoStamp); }
    let mut codes = [0u32; CODES];
    for (i, c) in codes.iter_mut().enumerate() {
        let at = |dx: usize, dy: usize| plane[(height - ROWS + dy) * stride + i * BLOCK + dx].code();
        *c = at(0, 0);
        if (0..BLOCK).any(|dy| (0..BLOCK).any(|dx| at(dx, dy) != *c)) {
            // A block that differs is no stamp at all unless the header was one
            return Err(if i < 4 { ReadError::NoStamp } else { ReadError::Block(i) });
        }
    }
    Fixed::from_codes(&codes, depth)
}

/// Writes the stamp into a VapourSynth frame's first plane
pub unsafe fn write_frame(api: &VSAPI, frame: *mut VSFrame, stamp: &Stamp) -> bool {
    unsafe {
        let fmt = &*(api.getVideoFrameFormat)(frame);
        if fmt.sample_type != VSSampleType::Integer { return false; }
        let (w, h) = ((api.getFrameWidth)(frame, 0) as usize, (api.getFrameHeight)(frame, 0) as usize);
        let stride = (api.getStride)(frame, 0) as usize;
        let ptr = (api.getWritePtr)(frame, 0);
        let depth = fmt.bits_per_sample as u32;
        match fmt.bytes_per_sample {
            1 => write(std::slice::from_raw_parts_mut(ptr, stride * h), stride, w, h, depth, stamp),
            2 => write(std::slice::from_raw_parts_mut(ptr as *mut u16, stride / 2 * h), stride / 2, w, h, depth, stamp),
            _ => false,
        }
    }
}

// ---------------------------------------------------------------------------------------------------
// The test filter: `core.gyroflow.Stamp360Test(clip, log="", time_base=...)`, which only stamps, with values known
// from the frame's number. tools/stamp360/ plays it and compares what the probe shader read with the log

/// The test values of frame `n`: the extremes on the first frames, then values from a hash
pub fn test_stamp(n: i64, time_us: i64) -> Stamp {
    let mut x = (n as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ 0xD1B5_4A32_D192_ED03;
    let mut next = || {
        x ^= x >> 33; x = x.wrapping_mul(0xFF51_AFD7_ED55_8CCD); x ^= x >> 33;
        (x >> 11) as f64 / (1u64 << 53) as f64
    };
    let pi = std::f64::consts::PI;
    let (q, yaws, conf): ([f64; 4], [f64; 4], [f64; 4]) = match n {
        0 => ([1.0, 0.0, 0.0, 0.0], [0.0; 4], [0.0; 4]),
        1 => ([-1.0, 1.0, -1.0, 1.0], [pi, -pi, pi * 0.5, -pi * 0.5], [1.0; 4]),
        2 => ([0.5, -0.5, 0.5, -0.5], [-1e-9, 1e-9, 3.0, -3.0], [0.5, 0.25, 0.75, 1.0]),
        _ => {
            let mut q = [0.0; 4].map(|_| next() * 2.0 - 1.0);
            let norm = q.iter().map(|c| c * c).sum::<f64>().sqrt().max(1e-9);
            q.iter_mut().for_each(|c| *c /= norm);
            ([q[0], q[1], q[2], q[3]], [0.0; 4].map(|_| (next() * 2.0 - 1.0) * pi), [0.0; 4].map(|_| next()))
        }
    };
    let mut headings = [HeadingStamp::default(); HEADINGS];
    for i in 0..HEADINGS { headings[i] = HeadingStamp { yaw: yaws[i], confidence: conf[i] }; }
    Stamp { time_us, q_world_from_cam: q, headings }
}

struct TestFilter {
    node: *mut VSNode,
    fps: (i64, i64),
    time_base: i64,
    log: Option<Mutex<std::fs::File>>,
}
unsafe impl Send for TestFilter {}
unsafe impl Sync for TestFilter {}

unsafe extern "system-unwind" fn test_get_frame(n: c_int, reason: VSActivationReason, instance: *mut c_void, _frame_data: *mut *mut c_void, ctx: *mut VSFrameContext, core: *mut VSCore, vsapi: *const VSAPI) -> *const VSFrame {
    let f = unsafe { &*(instance as *const TestFilter) };
    let api = unsafe { &*vsapi };
    match reason {
        VSActivationReason::Initial => unsafe { (api.requestFrameFilter)(n, f.node, ctx); null() },
        VSActivationReason::AllFramesReady => unsafe {
            let src = (api.getFrameFilter)(n, f.node, ctx);
            let frame = (api.copyFrame)(src, core);
            (api.freeFrame)(src);
            let (num, den) = f.fps;
            let time_us = f.time_base.wrapping_add(if num > 0 { (n as i64 * 1_000_000 * den + num / 2) / num } else { n as i64 * 33_333 });
            let stamp = test_stamp(n as i64, time_us);
            let fmt = &*(api.getVideoFrameFormat)(frame);
            let depth = fmt.bits_per_sample as u32;
            if !write_frame(api, frame, &stamp) {
                let msg = CString::new("Stamp360Test: the frame is too small, or not 8-16 bit integer").unwrap_or_default();
                (api.setFilterError)(msg.as_ptr(), ctx);
                (api.freeFrame)(frame);
                return null();
            }
            if let Some(log) = &f.log {
                let x = Fixed::from_stamp(&stamp, bits_for(depth));
                let line = serde_json::json!({ "n": n, "depth": depth, "bits": x.bits, "time": x.time, "q": x.q,
                                               "yaw": x.yaw, "confidence": x.confidence, "time_us": time_us });
                let _ = writeln!(log.lock(), "{line}");
            }
            frame
        },
        _ => null(),
    }
}

unsafe extern "system-unwind" fn test_free(instance: *mut c_void, _core: *mut VSCore, vsapi: *const VSAPI) {
    let f = unsafe { Box::from_raw(instance as *mut TestFilter) };
    unsafe { ((*vsapi).freeNode)(f.node) };
}

pub unsafe extern "system-unwind" fn test_create(input: *const VSMap, out: *mut VSMap, _user_data: *mut c_void, core: *mut VSCore, vsapi: *const VSAPI) {
    crate::init_logging();
    let api = unsafe { &*vsapi };
    let mut err = VSMapPropertyError::Success;
    let node = unsafe { (api.mapGetNode)(input, c"clip".as_ptr(), 0, &mut err) };
    let vi = unsafe { (*(api.getVideoInfo)(node)).clone() };
    let fmt = &vi.format;
    if fmt.sample_type != VSSampleType::Integer || fmt.bytes_per_sample > 2 || (vi.width as usize) < WIDTH || (vi.height as usize) < ROWS {
        return unsafe { crate::set_error(api, out, node, "Stamp360Test", "only 8-16 bit integer clips at least 64x2".into()) };
    }
    let log = unsafe { crate::map_get_str(api, input, c"log") }.filter(|p| !p.is_empty());
    let log = match log.map(|p| std::fs::OpenOptions::new().create(true).append(true).open(p)) {
        Some(Ok(file)) => Some(Mutex::new(file)),
        Some(Err(e)) => return unsafe { crate::set_error(api, out, node, "Stamp360Test", format!("the log: {e}")) },
        None => None,
    };
    let time_base = unsafe { crate::map_get_i64(api, input, c"time_base") }.unwrap_or(0);
    let filter = Box::new(TestFilter { node, fps: (vi.fps_num, vi.fps_den), time_base, log });
    let deps = [VSFilterDependency { source: node, request_pattern: VSRequestPattern::StrictSpatial }];
    unsafe {
        (api.createVideoFilter)(out, c"Stamp360Test".as_ptr(), &vi, test_get_frame, Some(test_free), VSFilterMode::Parallel,
                                deps.as_ptr(), deps.len() as c_int, Box::into_raw(filter) as *mut c_void, core);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plane<T: Sample>(h: usize, stride: usize, fill: u32) -> Vec<T> { vec![T::from_code(fill); stride * h] }

    fn every_field(depth: u32, check: impl Fn(&Stamp, Fixed, &Stamp)) {
        for n in 0..200 {
            let time = [0, 1, 33_366, 1_234_567_890_123, i64::MAX, -1][n as usize % 6].wrapping_add(n);
            let s = test_stamp(n, time);
            let (w, h) = (100, 6);
            if depth == 8 {
                let mut p = plane::<u8>(h, w + 12, 77);
                assert!(write(&mut p, w + 12, w, h, depth, &s));
                check(&s, read(&p, w + 12, w, h, depth).unwrap(), &read(&p, w + 12, w, h, depth).unwrap().to_stamp());
            } else {
                let mut p = plane::<u16>(h, w + 7, 300);
                assert!(write(&mut p, w + 7, w, h, depth, &s));
                check(&s, read(&p, w + 7, w, h, depth).unwrap(), &read(&p, w + 7, w, h, depth).unwrap().to_stamp());
            }
        }
    }

    #[test]
    fn every_field_reads_back_at_every_depth() {
        for depth in [8, 10, 12, 16] {
            every_field(depth, |s, fixed, back| {
                let bits = bits_for(depth);
                assert_eq!(fixed, Fixed::from_stamp(s, bits), "the fixed-point fields at {depth} bits");
                assert!(fixed.time_is(s.time_us), "time at {depth} bits");
                let q_step = 2.0 / mask(2 * bits) as f64;
                for c in 0..4 {
                    assert!((back.q_world_from_cam[c] - s.q_world_from_cam[c]).abs() <= q_step * 0.5 + 1e-15,
                            "q[{c}] at {depth} bits: {} for {}", back.q_world_from_cam[c], s.q_world_from_cam[c]);
                }
                let yaw_step = std::f64::consts::TAU / (1u64 << (2 * bits)) as f64;
                for h in 0..HEADINGS {
                    let d = (back.headings[h].yaw - s.headings[h].yaw).rem_euclid(std::f64::consts::TAU);
                    assert!(d.min(std::f64::consts::TAU - d) <= yaw_step * 0.5 + 1e-12, "yaw {h} at {depth} bits");
                    assert!((back.headings[h].confidence - s.headings[h].confidence).abs() <= 0.5 / mask(bits) as f64 + 1e-15);
                }
            });
        }
    }

    #[test]
    fn resolution_as_the_architecture_says() {
        // 20 bits at 10-bit: a quaternion component to ~2e-6
        assert_eq!(bits_for(10) * 2, 20);
        assert!(2.0 / mask(20) as f64 <= 2e-6);
        assert_eq!(bits_for(16) * 2, 32);
        let f = Fixed::from_stamp(&Stamp { time_us: (1 << 40) + 5, ..Default::default() }, bits_for(16));
        assert_eq!(f.time, (1 << 40) + 5, "64 bits of time at 16-bit");
    }

    #[test]
    fn codes_stay_in_the_depth_and_the_header_reads_coarsely() {
        for depth in [8, 10, 12, 16] {
            let codes = Fixed::from_stamp(&test_stamp(1, -1), bits_for(depth)).codes(depth);
            assert!(codes.iter().all(|&c| c < 1 << depth));
            // As the shader reads the header: floor(v * 16) of the normalized value
            for (i, &c) in codes[..HEADER].iter().enumerate() {
                let v = c as f64 / ((1u32 << depth) - 1) as f64;
                let want = [MAGIC[0], MAGIC[1], MAGIC[2], MAGIC[3], depth - 1, bits_for(depth) - 1, VERSION][i];
                assert_eq!(((v * 16.0).floor() as u32).min(15), want);
            }
        }
    }

    #[test]
    fn coarser_codes_survive_small_errors() {
        // Fine codes of fewer bits than the depth sit mid-step: an error under half a step reads the same
        let (depth, bits) = (10, 8);
        let f = Fixed::from_stamp(&test_stamp(7, 123_456), bits);
        let mut codes = f.codes(depth);
        for (i, c) in codes.iter_mut().enumerate() { *c = if i % 2 == 0 { *c + 1 } else { *c - 2 }; }
        assert_eq!(Fixed::from_codes(&codes, depth), Ok(f));
    }

    #[test]
    fn other_frames_are_not_stamps() {
        let (w, h) = (128, 8);
        let p = plane::<u16>(h, w, 512);
        assert_eq!(read(&p, w, w, h, 10), Err(ReadError::NoStamp));
        let mut p = plane::<u16>(h, w, 512);
        assert!(!write(&mut p, w, 63, h, 10, &Stamp::default()), "too narrow");
        assert!(write(&mut p, w, w, h, 10, &Stamp::default()));
        // Written at 10 bits, read as 12: the header says so
        assert_eq!(read(&p, w, w, h, 12), Err(ReadError::NoStamp));
        // A code changed: the check
        let mut q = p.clone();
        let x = (QUAT + 1) * BLOCK;
        for dy in 0..ROWS { for dx in 0..BLOCK { q[(h - ROWS + dy) * w + x + dx] ^= 1; } }
        assert_eq!(read(&q, w, w, h, 10), Err(ReadError::Check));
        // One pixel of a block changed
        let mut q = p.clone();
        q[(h - 1) * w + QUAT * BLOCK + 1] ^= 1;
        assert_eq!(read(&q, w, w, h, 10), Err(ReadError::Block(QUAT)));
        // The rows above are left as they were
        assert!(p[..(h - ROWS) * w].iter().all(|&s| s == 512));
        assert!(p[(h - ROWS) * w + WIDTH..(h - ROWS + 1) * w].iter().all(|&s| s == 512));
    }
}
