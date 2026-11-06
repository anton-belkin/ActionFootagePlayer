// SPDX-License-Identifier: GPL-3.0-or-later
//
// A 360 camera's clip exported (docs/360-cameras.md, "Exports"; step 7 of docs/360-implementation-plan.md), frame by
// frame as the player shows it: the lens tracks joined side by side, the plugin's 360 mode (sphere360.rs) makes the
// sphere in the camera's axes with its stamp, the stamp is read back from the frame (stamp.rs, as the view shader
// reads it), and the view (view360.rs, the WGSL twin of the player's GLSL) draws:
// - a stabilized 360 video: the whole sphere turned into the world and the chosen heading, equirect, for YouTube and
//   editors (gyroflow-export adds the spherical metadata, mp4meta::write_spherical);
// - a flat reframe: the view along keyframes of the head (yaw, pitch, roll, FOV), as crop mode's photos are made.
// gyroflow-export runs ffmpeg around it (decoding, joining, encoding).
//
// The heading mode by default: along the way you move when the clip has it (GPS), else along the camera, as the player
// picks. Per frame the mode's yaw is the stamp's: where a mode is unsure, the plugin stamped the camera's heading
// there, which is what the player shows too.

use crate::{ PlaneJob, SampleKind, Interpolation };
use crate::sphere360::{ Sphere360, DEFAULT_WIDTH };
use crate::stamp::{ self, Heading };
use crate::view360::{ Plane, Projection, View360, ViewParams };

/// A keyframe of the head: at `t` seconds of the clip, degrees
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HeadKey { pub t: f64, pub yaw: f64, pub pitch: f64, pub roll: f64, pub fov: f64 }

/// "t:yaw,pitch,roll,fov;t:..." (pitch, roll and FOV may be left out: 0, 0, 100; t as seconds or m:ss), in time order
pub fn parse_keys(s: &str) -> Result<Vec<HeadKey>, String> {
    let mut keys = Vec::new();
    for part in s.split(';').map(str::trim).filter(|p| !p.is_empty()) {
        let (t, rest) = part.split_once(':').map(|(a, b)| {
            // m:ss:yaw... has two colons: the time is what's before the last one
            match b.split_once(':') { Some((ss, r)) if !a.contains(',') && !ss.contains(',') => (format!("{a}:{ss}"), r.to_string()), _ => (a.to_string(), b.to_string()) }
        }).ok_or_else(|| format!("a keyframe {part}: t:yaw,pitch,roll,fov"))?;
        let t = match t.split_once(':') {
            Some((m, s)) => m.trim().parse::<f64>().ok().zip(s.trim().parse::<f64>().ok()).map(|(m, s)| m * 60.0 + s),
            None => t.trim().parse::<f64>().ok(),
        }.ok_or_else(|| format!("a keyframe's time {t}"))?;
        let v: Vec<f64> = rest.split(',').map(|x| x.trim().parse::<f64>()).collect::<Result<_, _>>().map_err(|_| format!("a keyframe {part}: numbers"))?;
        if v.is_empty() || v.len() > 4 { return Err(format!("a keyframe {part}: yaw[,pitch[,roll[,fov]]]")); }
        let fov = v.get(3).copied().unwrap_or(100.0);
        if !(1.0..=340.0).contains(&fov) { return Err(format!("a keyframe {part}: the FOV is 1-340°")); }
        keys.push(HeadKey { t, yaw: v[0], pitch: v.get(1).copied().unwrap_or(0.0).clamp(-90.0, 90.0), roll: v.get(2).copied().unwrap_or(0.0), fov });
    }
    if keys.is_empty() { return Err("no keyframes".into()); }
    keys.sort_by(|a, b| a.t.total_cmp(&b.t));
    Ok(keys)
}

/// The head at `t`: held before the first key and after the last, eased (smoothstep) between two, each value as given
/// (a yaw from 0 to 360 turns all the way round)
pub fn head_at(keys: &[HeadKey], t: f64) -> HeadKey {
    let Some(first) = keys.first() else { return HeadKey { t, yaw: 0.0, pitch: 0.0, roll: 0.0, fov: 100.0 } };
    if t <= first.t { return HeadKey { t, ..*first }; }
    for w in keys.windows(2) {
        let (a, b) = (w[0], w[1]);
        if t <= b.t {
            let x = if b.t > a.t { (t - a.t) / (b.t - a.t) } else { 1.0 };
            let s = x * x * (3.0 - 2.0 * x);
            let mix = |p: f64, q: f64| p + (q - p) * s;
            return HeadKey { t, yaw: mix(a.yaw, b.yaw), pitch: mix(a.pitch, b.pitch), roll: mix(a.roll, b.roll), fov: mix(a.fov, b.fov) };
        }
    }
    HeadKey { t, ..*keys.last().unwrap() }
}

/// What comes out
#[derive(Clone, Debug)]
pub enum Output {
    /// The whole sphere, `width` by `width / 2`
    Sphere { width: usize },
    /// The view along the head's keyframes
    Flat { size: (usize, usize), projection: Projection, keys: Vec<HeadKey> },
}

impl Output {
    pub fn size(&self) -> (usize, usize) {
        match self { Output::Sphere { width } => (*width, *width / 2), Output::Flat { size, .. } => *size }
    }
}

pub fn heading_name(h: Heading) -> &'static str {
    match h { Heading::World => "world", Heading::Camera => "camera", Heading::Travel => "travel", Heading::North => "north" }
}

pub fn parse_heading(s: &str) -> Option<Heading> {
    Some(match s { "world" => Heading::World, "camera" => Heading::Camera, "travel" | "way" => Heading::Travel, "north" => Heading::North, _ => return None })
}

/// The clip's sphere and the view over it
pub struct Render360 {
    sphere: Sphere360,
    view: View360,
    pub joined: (usize, usize),
    pub out: Output,
    pub heading: Heading,
    /// Why the heading isn't the one asked for, if it isn't
    pub heading_note: Option<String>,
    depth: u32,
    limited_range: bool,
    pub fps: f64,
}

/// A frame's planes as ffmpeg's yuv420p / yuv420p10le carry them: Y, then U and V at half the size each way
fn plane_sizes((w, h): (usize, usize)) -> [(usize, usize); 3] { [(w, h), (w / 2, h / 2), (w / 2, h / 2)] }

impl Render360 {
    /// `depth`: 8 (yuv420p frames, a byte a sample) or 10 (yuv420p10le, two); `sphere_width`: the sphere's (0: the
    /// plugin's default); `heading`: None for the player's choice; `limited_range`: the frames' range
    pub fn new(video: &str, depth: u32, limited_range: bool, out: Output, heading: Option<Heading>, sphere_width: usize) -> Result<Render360, String> {
        let rig = crate::rig::read(video).ok_or_else(|| format!("{video}: not a 360 camera's file"))??;
        let joined = rig.joined_size();
        let kind = if depth > 8 { SampleKind::U16 } else { SampleKind::U8 };
        let width = if sphere_width > 0 { sphere_width } else if let Output::Sphere { width } = out { width.max(DEFAULT_WIDTH) } else { DEFAULT_WIDTH };
        let sphere = Sphere360::load(video, joined, 3, (kind, depth, true, (1, 1)), Interpolation::Bilinear, width, 0.0, 2)?;
        let has = |m: &str| sphere.report.get(m).and_then(|v| v.get("available")).and_then(|v| v.as_bool()) == Some(true);
        let why = |m: &str| sphere.report.get(m).and_then(|v| v.get("why")).and_then(|v| v.as_str()).unwrap_or("not in this clip").to_string();
        let (heading, heading_note) = match heading {
            None if has("travel") => (Heading::Travel, None),
            None => (Heading::Camera, Some(format!("along the camera: along the way isn't there, {}", why("travel")))),
            Some(Heading::Travel) if !has("travel") => (Heading::Camera, Some(format!("along the camera instead of along the way: {}", why("travel")))),
            Some(Heading::North) if !has("north") => (Heading::Camera, Some(format!("along the camera instead of north: {}", why("north")))),
            Some(h) => (h, None),
        };
        let fps = sphere.fps;
        Ok(Render360 { sphere, view: View360::new()?, joined, out, heading, heading_note, depth, limited_range, fps })
    }

    /// Bytes of a joined input frame, and of an output one
    pub fn frame_bytes(&self) -> (usize, usize) {
        let b = if self.depth > 8 { 2 } else { 1 };
        let n = |s: (usize, usize)| plane_sizes(s).iter().map(|p| p.0 * p.1).sum::<usize>() * b;
        (n(self.joined), n(self.out.size()))
    }

    /// The frame at `t` seconds of the clip (its own time, as the decoder numbers it): `src` the joined frame, into
    /// `dst`. Returns the stamp read back from the sphere
    pub fn render(&self, t: f64, src: &[u8], dst: &mut [u8]) -> Result<stamp::Stamp, String> {
        let b = if self.depth > 8 { 2 } else { 1 };
        let (in_bytes, out_bytes) = self.frame_bytes();
        if src.len() != in_bytes || dst.len() != out_bytes { return Err(format!("frames of {} and {} bytes, not {in_bytes} and {out_bytes}", src.len(), dst.len())); }
        let (sw, sh) = self.sphere.output_size();
        let sphere_planes = plane_sizes((sw, sh));
        let mut sphere = vec![0u8; sphere_planes.iter().map(|p| p.0 * p.1).sum::<usize>() * b];
        // The planes as jobs: the joined frame in, the sphere out
        {
            let ins = plane_sizes(self.joined);
            let (mut si, mut so) = (0, 0);
            let mut jobs = Vec::with_capacity(3);
            for (i, o) in ins.iter().zip(&sphere_planes) {
                jobs.push(PlaneJob { src: src[si..].as_ptr(), src_stride: i.0 * b, src_w: i.0, src_h: i.1,
                                     dst: sphere[so..].as_mut_ptr(), dst_stride: o.0 * b, dst_w: o.0, dst_h: o.1 });
                si += i.0 * i.1 * b;
                so += o.0 * o.1 * b;
            }
            let ts = (t * 1e6).round() as i64 + self.sphere.frame_offset_us;
            self.sphere.process(ts, jobs, self.limited_range)?;
        }
        // The stamp, from the frame, as the player's shader reads it
        let st = if b == 2 {
            let y: &[u16] = bytemuck::cast_slice(&sphere[..sw * sh * 2]);
            stamp::read(y, sw, sw, sh, self.depth)
        } else {
            stamp::read(&sphere[..sw * sh], sw, sw, sh, self.depth)
        }.map_err(|e| format!("the 360 stamp didn't read back at {t:.3} s: {e:?}"))?.to_stamp();
        let mut p = ViewParams { q_world_from_cam: st.q_world_from_cam, heading_yaw: st.headings[self.heading as usize].yaw, ..Default::default() };
        match &self.out {
            Output::Sphere { .. } => p.projection = Projection::Equirect,
            Output::Flat { projection, keys, .. } => {
                let h = head_at(keys, t);
                (p.yaw, p.pitch, p.roll, p.fov, p.projection) = (h.yaw, h.pitch, h.roll, h.fov, *projection);
            }
        }
        // Each plane: the sphere's (its stamp rows, two in luma and one in chroma, left out of the view)
        let outs = plane_sizes(self.out.size());
        let max = ((1u32 << self.depth) - 1) as f32;
        let (mut si, mut so) = (0, 0);
        for (k, (sp, op)) in sphere_planes.iter().zip(&outs).enumerate() {
            let rows = if k == 0 { stamp::ROWS } else { stamp::ROWS / 2 };
            let data = &sphere[si..si + sp.0 * sp.1 * b];
            let values = if b == 2 {
                self.view.render(Plane::U16(bytemuck::cast_slice(data)), sp.0, sp.1, sp.0, rows, *op, &p)?
            } else {
                self.view.render(Plane::U8(data), sp.0, sp.1, sp.0, rows, *op, &p)?
            };
            // The texture normalized: a 10-bit sample in a 16-bit one reads code / 65535
            let scale = if b == 2 { 65535.0 } else { 255.0 };
            let out = &mut dst[so..so + op.0 * op.1 * b];
            if b == 2 {
                let o: &mut [u16] = bytemuck::cast_slice_mut(out);
                for (o, v) in o.iter_mut().zip(&values) { *o = (v * scale).round().clamp(0.0, max) as u16; }
            } else {
                for (o, v) in out.iter_mut().zip(&values) { *o = (v * scale).round().clamp(0.0, max) as u8; }
            }
            si += sp.0 * sp.1 * b;
            so += op.0 * op.1 * b;
        }
        Ok(st)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keyframes_parse_and_ease() {
        let k = parse_keys("5:90,10;0:0,0,0,100; 1:10:180,0,0,60").unwrap();
        assert_eq!(k.iter().map(|k| k.t).collect::<Vec<_>>(), vec![0.0, 5.0, 70.0]);
        assert_eq!(k[1], HeadKey { t: 5.0, yaw: 90.0, pitch: 10.0, roll: 0.0, fov: 100.0 });
        assert_eq!(head_at(&k, -1.0).yaw, 0.0);
        assert!((head_at(&k, 2.5).yaw - 45.0).abs() < 1e-9 && (head_at(&k, 2.5).pitch - 5.0).abs() < 1e-9);
        assert!(head_at(&k, 1.0).yaw < 18.0, "eased in");
        assert_eq!(head_at(&k, 100.0).fov, 60.0);
        assert!(parse_keys("").is_err() && parse_keys("1:a").is_err() && parse_keys("1:0,0,0,400").is_err());
        assert_eq!(parse_keys("3:-45").unwrap()[0], HeadKey { t: 3.0, yaw: -45.0, pitch: 0.0, roll: 0.0, fov: 100.0 });
    }
}
