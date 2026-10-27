// SPDX-License-Identifier: GPL-3.0-or-later
//
// Color fringes under water, and what mpv needs to take them out. A flat port in water bends the light where it enters
// the housing (Gyroflow's refraction coefficient), and water bends blue more than red: for each color the lens sees a
// slightly different field, so towards the edges red and blue land apart from green, along the radius from the lens's
// axis (lateral chromatic aberration): 1-2 px of a 4K frame halfway out, more further out. The camera takes out its
// lens's own fringes, as they are in air; the water's it doesn't know of.
//
// The stabilization renders Y, U and V with one coefficient, Gyroflow's: green's, as far as the eye goes. The colors
// are apart only in RGB, which the frame is in only in mpv, on the GPU: mpv/scripts/gyroflow/chromatic.glsl moves red
// and blue there, each pixel to where its ray lands with that color's coefficient. Where that is depends on where the
// lens's axis is in the stabilized frame, which changes every frame, and mpv tells a shader nothing of which frame it
// draws. So the frame carries it: the start of its last row of luma is a `Marker`, written after the stabilization,
// that the shader reads and then paints over with the row above.

use gyroflow_core::stabilization::{ FrameTransform, KernelParamsFlags };

use crate::{ PlaneJob, SampleKind };

/// Water's refractive index as the camera's red, green and blue see it, from physics alone (tools/water_dispersion.py):
/// water's index over the spectrum (Quan & Fry), averaged by how much each wavelength counts in the channel, for a grey
/// object in daylight seen through 3 m of water, the channels being D-Gamut's (DJI's log gamut; a raw sensor's are
/// within ~10 % of it). Nothing is fitted to footage: the depth and what's filmed aren't known. The path moves red's
/// difference from green by about a fifth either way between the surface and 5 m, and hardly blue's. Salt and
/// temperature move all three alike; what parts the colors is the difference, which they hardly change
const WATER_RGB: [f64; 3] = [1.33369, 1.33530, 1.33875];

/// What the shader needs for a frame. Its coordinates are the output's pixels, as Gyroflow's kernel has them
/// (undistort_coord in wgpu_undistort.wgsl): a pixel's ray is `(pixel - middle - center) / focal`, under the output
/// projection
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Marker {
    /// Red's and blue's refractive index over green's: 1 out of the water
    pub k: [f32; 2],
    /// The output's focal length, pixels (x, y)
    pub focal: [f32; 2],
    /// Its center, from the middle of the frame (pixels)
    pub center: [f32; 2],
    /// Gyroflow's output projection (OutputProjection as i32: 0 rectilinear)
    pub projection: i32,
    /// The rolling shutter reads columns, not rows
    pub horizontal: bool,
    /// The lens's axis as the output's camera sees it (x, y of the unit vector; z towards the scene) when the sensor
    /// read its first and its last row (column)
    pub axis: [[f32; 2]; 2],
    /// The project's background color (RGB, 0-1), where the frame doesn't cover the output and nothing fills it: the
    /// shader moves no color there, or the picture's colors would spill onto it
    pub background: Option<[f32; 3]>,
}

/// The marker's first value, so the shader knows a frame that has none
const MAGIC: u16 = 0xCA7E;
const VERSION: u16 = 2;
/// 16-bit values, then their sum
const VALUES: usize = 15;
/// Each value is four pixels of four bits, highest first
pub const PIXELS: usize = VALUES * 4;

impl Marker {
    /// The frame's, from the transform Gyroflow renders its luma with, into a buffer of `output` pixels: Gyroflow's
    /// kernel stretches its output to it (the core's output size is the aspect, at the input's scale), so its
    /// coordinates are scaled to it. Out of the water, and where the output isn't the lens fully corrected (the colors'
    /// rays would be the lens's own, which the shader doesn't model), the colors stay where they are: the marker is
    /// still written, for the shader to paint over
    pub fn from_transform(t: &FrameTransform, output: (usize, usize)) -> Marker {
        let p = t.kernel_params;
        let flags = KernelParamsFlags::from_bits_truncate(p.flags);
        let (f, fov, stretch, translation) = (p.f, p.fov, p.input_horizontal_stretch, p.translation2d);
        let stretch = if stretch > 0.01 { 1.0 / stretch } else { 1.0 };
        let axis = |m: &[f32; 16]| {
            let l = (m[6] * m[6] + m[7] * m[7] + m[8] * m[8]).sqrt().max(1e-9);
            [m[6] / l, m[7] / l]
        };
        let (first, last) = (t.matrices.first(), t.matrices.last());
        // The plugin sets no rect of its own: an output rect is the whole buffer, scaled to as below
        let exact = p.lens_correction_amount >= 0.999 && !flags.intersects(KernelParamsFlags::HAS_DIGITAL_LENS);
        let scale = [output.0 as f32 / p.output_width.max(1) as f32, output.1 as f32 / p.output_height.max(1) as f32];
        let n = if flags.contains(KernelParamsFlags::ANY_UNDERWATER) { p.light_refraction_coefficient as f64 } else { 1.0 };
        Marker {
            k: if exact && first.is_some() { water_ratios(n) } else { [1.0, 1.0] },
            focal: [f[0] * stretch / fov * scale[0], f[1] * stretch / fov * scale[1]],
            center: [-translation[0] * scale[0], -translation[1] * scale[1]],
            projection: p.output_projection,
            horizontal: flags.contains(KernelParamsFlags::HORIZONTAL_RS),
            axis: [first.map(axis).unwrap_or([0.0, 0.0]), last.map(axis).unwrap_or([0.0, 0.0])],
            background: None,
        }
    }

    /// The 16-bit values, the last their sum
    pub fn values(&self) -> [u16; VALUES] {
        let signed = |v: f32, scale: f32| (v * scale).round().clamp(-32768.0, 32767.0) as i16 as u16;
        let unsigned = |v: f32, scale: f32| (v * scale).round().clamp(0.0, 65535.0) as u16;
        let byte = |v: f32| (v * 255.0).round().clamp(0.0, 255.0) as u16;
        let bg = self.background.unwrap_or([0.0; 3]);
        let mut v = [
            MAGIC,
            VERSION << 8 | (self.horizontal as u16) << 4 | (self.projection.clamp(0, 15) as u16),
            // ±0.0156 in steps of 5e-7
            signed(self.k[0] - 1.0, 2097152.0), signed(self.k[1] - 1.0, 2097152.0),
            // Half and quarter pixels
            unsigned(self.focal[0], 2.0), unsigned(self.focal[1], 2.0),
            signed(self.center[0], 4.0), signed(self.center[1], 4.0),
            signed(self.axis[0][0], 32767.0), signed(self.axis[0][1], 32767.0),
            signed(self.axis[1][0], 32767.0), signed(self.axis[1][1], 32767.0),
            // 8 bits a color, and whether there is one
            byte(bg[0]) << 8 | byte(bg[1]), byte(bg[2]) << 8 | self.background.is_some() as u16,
            0,
        ];
        v[VALUES - 1] = v[..VALUES - 1].iter().fold(0u16, |s, x| s.wrapping_add(*x));
        v
    }

    /// Into the start of the last row of a luma plane (`max_value`: its white, 1023 for 10-bit). A nibble is one of 17
    /// levels from black to white, so it reads back the same through limited or full range and through the noise
    /// reduction (shaders/denoise.glsl blends only what's within ~1 % of the pixel)
    pub fn write(&self, plane: &PlaneJob, kind: SampleKind, max_value: f32) {
        if plane.dst_w < PIXELS || plane.dst_h == 0 { return; }
        let row = plane.dst_h - 1;
        let codes = self.values().into_iter().flat_map(|v| [v >> 12, v >> 8 & 15, v >> 4 & 15, v & 15])
            .map(|nibble| ((nibble as f32 + 1.0) * max_value / 17.0).round());
        unsafe {
            let start = plane.dst.add(row * plane.dst_stride);
            match kind {
                SampleKind::U8 => {
                    let px = std::slice::from_raw_parts_mut(start, PIXELS);
                    for (p, c) in px.iter_mut().zip(codes) { *p = c as u8; }
                }
                SampleKind::U16 => {
                    let px = std::slice::from_raw_parts_mut(start as *mut u16, PIXELS);
                    for (p, c) in px.iter_mut().zip(codes) { *p = c as u16; }
                }
            }
        }
    }
}

/// Red's and blue's index over green's, for a lens behind a flat port with refraction coefficient `n` (Gyroflow's,
/// green's): the water's spread between the colors, scaled down to nothing as `n` goes to air's 1
pub fn water_ratios(n: f64) -> [f32; 2] {
    if !(n > 1.0) { return [1.0, 1.0]; }
    let s = ((n - 1.0) / (WATER_RGB[1] - 1.0)).min(2.0);
    let color = |c: usize| ((n + (WATER_RGB[c] - WATER_RGB[1]) * s) / n) as f32;
    [color(0), color(2)]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn water_ratios_are_the_spread() {
        assert_eq!(water_ratios(1.0), [1.0, 1.0]);
        let [r, b] = water_ratios(1.33);
        assert!((r - 0.9988086).abs() < 2e-6, "{r}");
        assert!((b - 1.002553).abs() < 2e-6, "{b}");
    }

    #[test]
    fn values_round_trip() {
        let m = Marker { k: [0.99815, 1.00275], focal: [1834.5, 1834.5], center: [-3.25, 12.0], projection: 0, horizontal: false,
                         axis: [[0.031, -0.012], [0.029, -0.0135]], background: Some([0.0667, 0.0667, 0.5]) };
        let v = m.values();
        assert_eq!(v[0], MAGIC);
        assert_eq!(v[VALUES - 1], v[..VALUES - 1].iter().fold(0u16, |s, x| s.wrapping_add(*x)));
        let k = |x: u16| 1.0 + x as i16 as f32 / 2097152.0;
        assert!((k(v[2]) - m.k[0]).abs() < 1e-6 && (k(v[3]) - m.k[1]).abs() < 1e-6);
        assert_eq!(v[4] as f32 / 2.0, 1834.5);
        assert_eq!(v[6] as i16 as f32 / 4.0, -3.25);
        assert!((v[9] as i16 as f32 / 32767.0 - m.axis[0][1]).abs() < 1e-4);
        assert_eq!((v[12] >> 8, v[12] & 255, v[13] >> 8, v[13] & 1), (17, 17, 128, 1));
    }

    #[test]
    fn written_nibbles_read_back() {
        let m = Marker { k: [0.998, 1.003], focal: [2000.0, 2000.0], center: [0.0, 0.0], projection: 0, horizontal: true, axis: [[0.1, -0.2], [0.11, -0.19]], background: None };
        for (kind, max) in [(SampleKind::U16, 1023.0f32), (SampleKind::U8, 255.0)] {
            let bytes = if kind == SampleKind::U8 { 1 } else { 2 };
            let (w, h) = (64usize, 2usize);
            let mut buf = vec![0u8; w * h * bytes];
            let plane = PlaneJob { src: std::ptr::null(), src_stride: 0, src_w: 0, src_h: 0, dst: buf.as_mut_ptr(), dst_stride: w * bytes, dst_w: w, dst_h: h };
            m.write(&plane, kind, max);
            let row = &buf[w * bytes..];
            let code = |i: usize| if bytes == 1 { row[i] as f32 } else { u16::from_ne_bytes([row[2 * i], row[2 * i + 1]]) as f32 };
            // As the shader reads it: the level of 17, from the normalized value
            let nibble = |i: usize| ((code(i) / max * 17.0 - 1.0).round()) as u16;
            let read: Vec<u16> = (0..VALUES).map(|v| (0..4).fold(0, |s, j| s << 4 | nibble(v * 4 + j))).collect();
            assert_eq!(read, m.values().to_vec());
        }
    }
}
