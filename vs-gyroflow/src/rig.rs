// SPDX-License-Identifier: GPL-3.0-or-later
//
// A 360° camera's rig (docs/360-cameras.md, "The plugin's part"): telemetry-parser's CameraRig, video-types' geometry,
// turned into what gyroflow-core renders each lens with: its lens profile (model, focal, centre in the pixels of its
// picture), its pose in the rig and its offset from the rig's centre (gyroflow-core's RigLens), where its picture is in
// the tracks joined side by side (mpv's lavfi-complex hstack), its validity, its blend and its masks.
//
// Conventions are video-types': OpenCV axes (+x right, +y down, +z forward), Hamilton quaternions, `rotation` the
// lens's q_rig_from_cam, `translation` in metres in the rig's axes, angles in radians. The rig's axes are those of the
// camera the motion data describes (gyroflow-core's, after imu_orientation): RigLens's.
//
// The projection is here too, in f64 (`Lens360::project`): the weight maps of the blend and the tests need where a
// direction lands in a lens without the GPU, and it's the same map the kernel's Insta360 model draws.

use std::sync::{ Arc, atomic::AtomicBool };
use nalgebra::{ Quaternion, UnitQuaternion, Vector3 };
use gyroflow_core::telemetry_parser::{ Input, InputOptions, tags_impl::*, rig::{ CameraRig, RigPicture, geometry as g } };

/// One lens of the rig, as the render needs it
#[derive(Clone, Debug, PartialEq)]
pub struct Lens360 {
    /// Its index in the rig
    pub index: usize,
    /// The picture (video track) it's in. Joined side by side in track order, its picture starts at input · width
    pub input: u32,
    /// Its part of that picture (x, y, w, h), the whole of it for a lens a track
    pub rect: (usize, usize, usize, usize),
    pub model: Model,
    /// q_rig_from_cam
    pub rotation: UnitQuaternion<f64>,
    /// Its entrance pupil from the rig's centre, metres, rig axes
    pub translation: [f64; 3],
    /// The largest angle from its axis it has picture for (θ max, the vendor's mask disc), radians
    pub validity: f64,
    /// Its crossfade: weight 1 to `limit - width`, 0 at `limit` (radians from its axis)
    pub blend: g::Blend,
    /// The image circle, in calibration pixels
    pub circle: Option<([f64; 2], f64)>,
    /// The masks (include polygons unioned, exclude ones taken off), in calibration pixels, drawn once
    pub masks: Option<Raster>,
    /// The rolling shutter: the frame's readout in ms, and whether the rows are read from the top
    pub readout: Option<(f64, bool)>,
}

/// A lens model gyroflow-core's kernel has: Insta360's unified sphere (Mei) with its distortion in the order of the
/// fork's `insta360` model, `k1 k2 k3 p1 p2 ξ k4 k5 q1 q2 s1 s2 s3 s4`
#[derive(Clone, Debug, PartialEq)]
pub struct Model {
    pub coeffs: [f64; 14],
    /// In calibration pixels
    pub focal: [f64; 2],
    pub centre: [f64; 2],
    pub calib_dim: [u32; 2],
    /// Calibration pixels to the lens's part of the picture: a scale per axis and an offset (no shear: refused)
    pub scale: [f64; 2],
    pub offset: [f64; 2],
    /// Mirrored about the centre's x in calibration space, before `scale`
    pub mirror: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Rig {
    pub brand: String,
    pub model: String,
    pub lenses: Vec<Lens360>,
    /// Feather (each lens by its blend) or the nearest lens alone
    pub nearest: bool,
    /// The maker's stitching distance, metres, when it states one
    pub convergence: Option<f64>,
    /// The video tracks it reads, each of `picture` pixels
    pub pictures: usize,
    pub picture: (usize, usize),
}

impl Model {
    /// From video-types' intrinsics, for the models the fork's kernel has
    fn new(lens: &g::Lens) -> Result<Model, String> {
        let i = &lens.intrinsics;
        let (xi, k) = match i.model {
            g::LensModel::Mei { xi, dist: g::MeiDistortion::BrownV3 { k, p } } =>
                (xi, [k[0], k[1], k[2], p[0], p[1], 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0]),
            // video-types: R = 1 + d0r² + .. + d4r¹⁰; x += (r² + 2x²)(d5 + d7r²) + 2xy(d6 + d8r²) + d9r² + d11r⁴,
            // y += (r² + 2y²)(d6 + d8r²) + 2xy(d5 + d7r²) + d10r² + d12r⁴. The kernel's p1 is OpenCV's (2xy in x): d6
            g::LensModel::Mei { xi, dist: g::MeiDistortion::RadtanPro { d } } =>
                (xi, [d[0], d[1], d[2], d[6], d[5], 0.0, d[3], d[4], d[8], d[7], d[9], d[11], d[10], d[12]]),
            other => return Err(format!("lens model {other:?} isn't in the player yet")),
        };
        let mut coeffs = k;
        coeffs[5] = xi;
        let a = lens.calib_to_picture;
        if a.a[0][1] != 0.0 || a.a[1][0] != 0.0 || a.a[0][0] <= 0.0 || a.a[1][1] <= 0.0 {
            return Err(format!("calibration to picture {:?}: only a scale and an offset", a));
        }
        if i.skew != 0.0 { return Err("a skewed calibration".into()); }
        Ok(Model { coeffs, focal: i.focal, centre: i.centre, calib_dim: i.calib_dim, scale: [a.a[0][0], a.a[1][1]], offset: a.t, mirror: lens.mirror })
    }

    /// The model's own map: a direction in the lens's axes to calibration pixels, before the mirror. None behind the
    /// unified sphere's fold
    pub fn calibration_pixel(&self, d: Vector3<f64>) -> Option<[f64; 2]> {
        let k = &self.coeffs;
        let d = d.normalize();
        let w = d.z + k[5];
        if w <= 1e-9 { return None; }
        let (x, y) = (d.x / w, d.y / w);
        let r2 = x * x + y * y;
        let radial = 1.0 + r2 * (k[0] + r2 * (k[1] + r2 * (k[2] + r2 * (k[6] + r2 * k[7]))));
        let p1 = k[3] + k[8] * r2;
        let p2 = k[4] + k[9] * r2;
        let xd = x * radial + 2.0 * p1 * x * y + p2 * (r2 + 2.0 * x * x) + r2 * (k[10] + k[11] * r2);
        let yd = y * radial + 2.0 * p2 * x * y + p1 * (r2 + 2.0 * y * y) + r2 * (k[12] + k[13] * r2);
        Some([self.centre[0] + self.focal[0] * xd, self.centre[1] + self.focal[1] * yd])
    }

    /// Calibration pixels to the lens's part of the picture: the mirror, then the scale and offset
    pub fn to_picture(&self, c: [f64; 2]) -> [f64; 2] {
        let x = if self.mirror { 2.0 * self.centre[0] - c[0] } else { c[0] };
        [x * self.scale[0] + self.offset[0], c[1] * self.scale[1] + self.offset[1]]
    }

    /// The camera matrix in the lens's picture pixels, as gyroflow-core's kernel takes it: `u = fx·xd + cx`. The
    /// mirror (`2cx - u`) is a negative fx
    pub fn camera_matrix(&self) -> [[f64; 3]; 3] {
        let fx = self.focal[0] * self.scale[0] * if self.mirror { -1.0 } else { 1.0 };
        let fy = self.focal[1] * self.scale[1];
        let cx = self.centre[0] * self.scale[0] + self.offset[0];
        let cy = self.centre[1] * self.scale[1] + self.offset[1];
        [[fx, 0.0, cx], [0.0, fy, cy], [0.0, 0.0, 1.0]]
    }
}

fn quat(q: g::Quat) -> UnitQuaternion<f64> {
    UnitQuaternion::from_quaternion(Quaternion::new(q.w, q.x, q.y, q.z))
}

/// A lens's masks drawn into a grid over its calibration space, `STEP` calibration pixels a cell: a body mask's
/// polygon has hundreds of points (720 on an X5), too many to test per pixel of the sphere
#[derive(Clone, Debug, PartialEq)]
pub struct Raster {
    w: usize,
    h: usize,
    cells: Vec<bool>,
}

impl Raster {
    const STEP: f64 = 4.0;

    /// Even-odd per polygon at each cell's centre (video-types' masks); includes unioned (none: all of it), then the
    /// excludes taken off
    fn new(masks: &[(bool, Vec<[f64; 2]>)], calib_dim: [u32; 2]) -> Option<Raster> {
        if masks.is_empty() { return None; }
        let (w, h) = ((calib_dim[0] as f64 / Self::STEP).ceil() as usize, (calib_dim[1] as f64 / Self::STEP).ceil() as usize);
        let any_include = masks.iter().any(|m| m.0);
        let mut cells = vec![!any_include; w * h];
        let mut xs = Vec::new();
        for (include, poly) in masks.iter().filter(|m| m.1.len() >= 3) {
            for row in 0..h {
                let y = (row as f64 + 0.5) * Self::STEP;
                xs.clear();
                for i in 0..poly.len() {
                    let (a, b) = (poly[i], poly[(i + 1) % poly.len()]);
                    if (a[1] <= y) != (b[1] <= y) { xs.push(a[0] + (y - a[1]) / (b[1] - a[1]) * (b[0] - a[0])); }
                }
                xs.sort_by(|a, b| a.total_cmp(b));
                for pair in xs.chunks_exact(2) {
                    let from = ((pair[0] / Self::STEP - 0.5).ceil().max(0.0)) as usize;
                    let to = ((pair[1] / Self::STEP - 0.5).floor() + 1.0).clamp(0.0, w as f64) as usize;
                    for c in &mut cells[row * w + from.min(w)..row * w + to.max(from.min(w))] { *c = *include; }
                }
            }
        }
        Some(Raster { w, h, cells })
    }

    pub fn covers(&self, px: [f64; 2]) -> bool {
        let (x, y) = (px[0] / Self::STEP, px[1] / Self::STEP);
        if x < 0.0 || y < 0.0 { return false; }
        let (x, y) = (x as usize, y as usize);
        x < self.w && y < self.h && self.cells[y * self.w + x]
    }
}

impl Lens360 {
    /// A direction of the rig (unit, its centre's) as this lens sees it at the stitching distance `distance` (metres,
    /// 0 for infinity): the point `R·d` of the stitching sphere from where the lens is, in the lens's axes
    pub fn seen(&self, d: Vector3<f64>, distance: f64) -> Vector3<f64> {
        let p = if distance > 0.0 && distance.is_finite() { d * distance - Vector3::from(self.translation) } else { d };
        self.rotation.inverse() * p
    }

    /// The angle from the lens's axis of a direction in its own axes
    pub fn theta(c: &Vector3<f64>) -> f64 { c.x.hypot(c.y).atan2(c.z) }

    /// Where a direction of the rig lands in the lens's part of the picture, None where it has no picture there
    pub fn project(&self, d: Vector3<f64>, distance: f64) -> Option<[f64; 2]> {
        let c = self.seen(d, distance);
        if Lens360::theta(&c) > self.validity { return None; }
        let px = self.model.calibration_pixel(c)?;
        if !self.covers(px) { return None; }
        Some(self.model.to_picture(px))
    }

    /// Whether a calibration pixel is picture: in the image circle, and in the masks' union less what they exclude
    pub fn covers(&self, px: [f64; 2]) -> bool {
        if let Some((c, r)) = self.circle {
            if (px[0] - c[0]).hypot(px[1] - c[1]) > r { return false; }
        }
        self.masks.as_ref().is_none_or(|m| m.covers(px))
    }

    /// The lens's weight in the blend for a direction of the rig: its blend's ramp where it has picture, 0 elsewhere.
    /// `masked`: the masks count (the body, a stick mount); without, only the ramp and the validity
    pub fn weight(&self, d: Vector3<f64>, distance: f64, masked: bool) -> f64 {
        let c = self.seen(d, distance);
        let theta = Lens360::theta(&c);
        if theta > self.validity || theta > self.blend.limit { return 0.0; }
        let w = self.blend.weight(theta);
        if w <= 0.0 { return 0.0; }
        match self.model.calibration_pixel(c) {
            Some(px) if !masked || self.covers(px) => w,
            _ => 0.0,
        }
    }

    /// gyroflow-core's lens profile for it, in its picture's pixels (`lens_profile.rs`)
    pub fn lens_profile(&self, brand: &str, model: &str) -> serde_json::Value {
        let (w, h) = (self.rect.2, self.rect.3);
        serde_json::json!({
            "name": format!("{brand} {model} lens {}", self.index), "calibrated_by": "telemetry-parser (CameraRig)",
            "camera_brand": brand, "camera_model": model, "lens_model": format!("lens {}", self.index),
            "calib_dimension": { "w": w, "h": h }, "orig_dimension": { "w": w, "h": h },
            "calibrator_version": "---", "official": true,
            "distortion_model": "insta360",
            "frame_readout_time": self.readout.map(|(ms, top)| if top { ms } else { -ms }),
            "fisheye_params": { "camera_matrix": self.model.camera_matrix(), "distortion_coeffs": self.model.coeffs.to_vec() }
        })
    }
}

impl Rig {
    /// From a rig as telemetry-parser describes it, with each of its pictures `picture` pixels (a track's coded size)
    pub fn new(rig: &CameraRig, picture: (usize, usize)) -> Result<Rig, String> {
        let g::ProjectionKind::Rig { cameras, blend, convergence_m } = &rig.projection.kind else {
            return Err(format!("{} {}: not a rig of lenses ({:?})", rig.brand, rig.model, std::mem::discriminant(&rig.projection.kind)));
        };
        if cameras.is_empty() { return Err("a rig without lenses".into()); }
        if rig.projection.imu_from_rig != g::Quat::IDENTITY {
            // Not on any body this is used with yet (the DJI Osmo 360 states one, gyroflow-core's motion then needs it)
            return Err(format!("{} {}: its IMU isn't in the rig's axes", rig.brand, rig.model));
        }
        let pictures: Vec<u32> = rig.pictures.iter().map(|p| match p {
            RigPicture::VideoTrack(t) => Ok(*t),
            RigPicture::Sibling { .. } => Err(format!("{} {}: a lens in another file", rig.brand, rig.model)),
        }).collect::<Result<_, _>>()?;
        let lenses = cameras.iter().enumerate().map(|(index, l)| {
            if l.eye.is_some() { return Err("a stereo rig".to_string()); }
            let input = l.picture.input;
            // The picture input indexes rig.pictures, which say the track
            let track = *pictures.get(input as usize).ok_or_else(|| format!("lens {index}: picture {input} of {}", pictures.len()))?;
            let rect = match l.picture.rect {
                Some(r) => (r.x as usize, r.y as usize, r.w as usize, r.h as usize),
                None => (0, 0, picture.0, picture.1),
            };
            if rect.0 + rect.2 > picture.0 || rect.1 + rect.3 > picture.1 { return Err(format!("lens {index}: its rect {rect:?} is outside the picture")); }
            Ok(Lens360 {
                index, input: track, rect,
                model: Model::new(l).map_err(|e| format!("lens {index}: {e}"))?,
                rotation: quat(l.rotation),
                translation: l.translation,
                validity: l.effective_fov_limit(),
                blend: l.blend,
                circle: l.image_circle.map(|c| (c.centre, c.radius)),
                masks: Raster::new(&l.masks.iter().map(|m| (m.polarity == g::MaskPolarity::Include, m.points.clone())).collect::<Vec<_>>(), l.intrinsics.calib_dim),
                readout: l.readout.and_then(|r| match r.direction {
                    g::ReadoutDirection::TopDown => Some((r.time_us / 1000.0, true)),
                    g::ReadoutDirection::BottomUp => Some((r.time_us / 1000.0, false)),
                    // Across the frame: gyroflow-core has it, but no rig reads so
                    _ => None,
                }),
            })
        }).collect::<Result<Vec<_>, String>>()?;
        let mut lenses = lenses;
        // The stitching sphere around the middle of the lenses: the vendor's origin may be one of them (Insta360's is
        // lens 0's pupil), which would put all the parallax on the other
        let middle = lenses.iter().map(|l| Vector3::from(l.translation)).sum::<Vector3<f64>>() / lenses.len() as f64;
        for l in lenses.iter_mut() { l.translation = (Vector3::from(l.translation) - middle).into(); }
        let tracks = lenses.iter().map(|l| l.input as usize + 1).max().unwrap_or(1);
        Ok(Rig {
            brand: rig.brand.clone(), model: rig.model.clone(), lenses,
            nearest: *blend == g::BlendMode::Nearest, convergence: *convergence_m, pictures: tracks, picture,
        })
    }

    /// Where lens `i`'s part is in the tracks joined side by side (x, y, w, h)
    pub fn source_rect(&self, i: usize) -> (usize, usize, usize, usize) {
        let l = &self.lenses[i];
        (l.input as usize * self.picture.0 + l.rect.0, l.rect.1, l.rect.2, l.rect.3)
    }

    /// The joined frame's size
    pub fn joined_size(&self) -> (usize, usize) { (self.picture.0 * self.pictures, self.picture.1) }

    /// Each lens's share of a direction (unit, the rig's), summing to 1 where any lens has picture; all 0 elsewhere.
    /// Where every lens is masked (an X5's nadir, its body under both lenses) the masks are left out: the body shows
    /// rather than a hole
    pub fn weights(&self, d: Vector3<f64>, distance: f64) -> Vec<f64> {
        let mut w: Vec<f64> = self.lenses.iter().map(|l| l.weight(d, distance, true)).collect();
        if w.iter().all(|v| *v <= 0.0) {
            w = self.lenses.iter().map(|l| l.weight(d, distance, false)).collect();
        }
        if self.nearest {
            let best = self.lenses.iter().enumerate().filter(|(i, _)| w[*i] > 0.0)
                .min_by(|a, b| Lens360::theta(&a.1.seen(d, distance)).total_cmp(&Lens360::theta(&b.1.seen(d, distance)))).map(|(i, _)| i);
            for (i, v) in w.iter_mut().enumerate() { *v = if Some(i) == best { 1.0 } else { 0.0 }; }
        }
        let sum: f64 = w.iter().sum();
        if sum > 0.0 { for v in w.iter_mut() { *v /= sum; } }
        w
    }
}

/// The direction of an equirect pixel `(x, y)` of a sphere `w` by `h` (pixel centres: x + 0.5, as the fork's wgpu
/// draws them): longitude 0 at +z, +90° at +x, latitude +90° up (-y)
pub fn equirect_direction(x: f64, y: f64, w: usize, h: usize) -> Vector3<f64> {
    let lon = ((x + 0.5) / w as f64 - 0.5) * std::f64::consts::TAU;
    let lat = (0.5 - (y + 0.5) / h as f64) * std::f64::consts::PI;
    Vector3::new(lat.cos() * lon.sin(), -lat.sin(), lat.cos() * lon.cos())
}

/// The rig a file describes, as telemetry-parser reads it (`Lens/Rig`); None for any other video
pub fn camera_rig(path: &str) -> Option<CameraRig> {
    let mut file = std::fs::File::open(path).ok()?;
    let size = file.metadata().ok()?.len() as usize;
    let input = Input::from_stream_with_options(&mut file, size, path, |_| (), Arc::new(AtomicBool::new(false)), InputOptions::default()).ok()?;
    for s in input.samples.as_deref().unwrap_or_default() {
        if let Some(TagValue::CameraRig(v)) = s.tag_map.as_ref().and_then(|m| m.get(&GroupId::Lens)).and_then(|g| g.get(&TagId::Rig)).map(|t| &t.value) {
            return Some(v.get().clone());
        }
    }
    None
}

/// The rig of a video and the size of its tracks, ready to render; None for any other video, an error for a rig the
/// player can't render yet
pub fn read(path: &str) -> Option<Result<Rig, String>> {
    let rig = camera_rig(path)?;
    let mut file = std::fs::File::open(path).ok()?;
    let size = file.metadata().ok()?.len() as usize;
    let video = gyroflow_core::util::get_video_metadata(&mut file, size, &gyroflow_core::filesystem::path_to_url(path)).ok()?;
    Some(Rig::new(&rig, (video.width, video.height)))
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// The sample X5 clip's rig, as telemetry-parser reads it (vid_20250416_143628_00_005): its numbers, written out
    pub fn x5() -> CameraRig {
        serde_json::from_str(include_str!("../tests/x5_rig.json")).unwrap()
    }

    #[test]
    fn an_x5_rig() {
        let rig = Rig::new(&x5(), (3840, 3840)).unwrap();
        assert_eq!(rig.lenses.len(), 2);
        assert_eq!(rig.pictures, 2);
        assert_eq!(rig.joined_size(), (7680, 3840));
        // Lens 0 is in the second track
        assert_eq!(rig.lenses[0].input, 1);
        assert_eq!(rig.source_rect(0), (3840, 0, 3840, 3840));
        assert_eq!(rig.source_rect(1), (0, 0, 3840, 3840));
        // Back to back: lens 0 looks along +z, lens 1 along -z, about a degree off
        let axis = |i: usize| rig.lenses[i].rotation * Vector3::z();
        assert!(axis(0).z > 0.999 && axis(1).z < -0.999, "{:?} {:?}", axis(0), axis(1));
        // 31.9 mm apart, about the middle
        let t = |i: usize| Vector3::from(rig.lenses[i].translation);
        assert!(((t(0) - t(1)).norm() - 0.0319).abs() < 0.001, "{}", (t(0) - t(1)).norm());
        assert!((t(0) + t(1)).norm() < 1e-12);
        // Each sees its own half and past it, to its mask disc; the ramp crosses at 90°
        assert!(rig.lenses.iter().all(|l| l.validity.to_degrees() > 92.0 && l.validity.to_degrees() < 105.0));
        assert!(rig.lenses.iter().all(|l| (l.blend.limit - l.blend.width / 2.0 - std::f64::consts::FRAC_PI_2).abs() < 1e-9));
        // RadtanPro: the r⁸ term is there
        assert!(rig.lenses[0].model.coeffs[6] != 0.0);
    }

    #[test]
    fn the_lens_axis_lands_on_the_principal_point() {
        let rig = Rig::new(&x5(), (3840, 3840)).unwrap();
        for l in &rig.lenses {
            let p = l.project(l.rotation * Vector3::z(), 0.0).unwrap();
            let m = l.model.camera_matrix();
            assert!((p[0] - m[0][2]).abs() < 1e-6 && (p[1] - m[1][2]).abs() < 1e-6, "{p:?}");
            // Inside the 3840² picture, near its middle
            assert!((p[0] - 1920.0).abs() < 50.0 && (p[1] - 1920.0).abs() < 50.0, "{p:?}");
        }
    }

    #[test]
    fn the_kernels_model_draws_the_same_pixels() {
        // gyroflow-core's insta360 model (the fork's, RadtanPro terms included) against the f64 projection here
        use gyroflow_core::stabilization::{ KernelParams, distortion_models::DistortionModel };
        let rig = Rig::new(&x5(), (3840, 3840)).unwrap();
        let model = DistortionModel::from_name("insta360");
        assert_eq!(model.id(), "insta360");
        for l in &rig.lenses {
            let mut kp = KernelParams::default();
            for (i, c) in l.model.coeffs.iter().enumerate() { kp.k[i] = *c as f32; }
            let m = l.model.camera_matrix();
            for i in 0..=48 {
                for j in 0..12 {
                    let (theta, phi) = ((i as f64 * 2.0).to_radians(), (j as f64 * 30.0).to_radians());
                    let c = Vector3::new(theta.sin() * phi.cos(), theta.sin() * phi.sin(), theta.cos());
                    let Some(px) = l.model.calibration_pixel(c) else { continue };
                    let want = l.model.to_picture(px);
                    let (u, v) = model.distort_point(c.x as f32, c.y as f32, c.z as f32, &kp);
                    let got = [m[0][0] * u as f64 + m[0][2], m[1][1] * v as f64 + m[1][2]];
                    // f32 in the kernel: a few thousandths of a pixel
                    assert!((got[0] - want[0]).abs() < 0.02 && (got[1] - want[1]).abs() < 0.02, "θ {} φ {}: {got:?} against {want:?}", i * 2, j * 30);
                }
            }
        }
    }

    #[test]
    fn the_weights_cross_at_the_seam() {
        let rig = Rig::new(&x5(), (3840, 3840)).unwrap();
        let axis0 = rig.lenses[0].rotation * Vector3::z();
        // Along lens 0's axis: lens 0 alone; behind it: lens 1 alone; on the great circle between: about even
        assert_eq!(rig.weights(axis0, 0.0), vec![1.0, 0.0]);
        assert_eq!(rig.weights(-axis0, 0.0), vec![0.0, 1.0]);
        let side = axis0.cross(&Vector3::y()).normalize();
        let w = rig.weights(side, 0.0);
        assert!((w[0] - 0.5).abs() < 0.1 && (w[0] + w[1] - 1.0).abs() < 1e-12, "{w:?}");
        // Everywhere on the sphere some lens has picture, if only the body's at the nadir. Masked there: a few degrees
        let mut masked = 0;
        for j in 0..90 {
            for i in 0..180 {
                let d = equirect_direction(i as f64, j as f64, 180, 90);
                let w = rig.weights(d, 1.0);
                assert!((w.iter().sum::<f64>() - 1.0).abs() < 1e-9, "{i} {j}: {w:?}");
                if rig.lenses.iter().all(|l| l.weight(d, 1.0, true) == 0.0) {
                    masked += 1;
                    assert!(d.y > 0.9, "masked off the nadir: {i} {j} {d:?}");
                }
            }
        }
        assert!(masked > 0 && masked < 200, "{masked}");
    }

    #[test]
    fn the_stitching_distance_moves_what_a_lens_sees() {
        // At R the lens sees R·d - t: a direction across its axis shifts by about |t| / R radians
        let rig = Rig::new(&x5(), (3840, 3840)).unwrap();
        let l = &rig.lenses[0];
        let d = (l.rotation * Vector3::new(1.0, 0.0, 1.0)).normalize();
        let far = l.seen(d, 0.0).normalize();
        let near = l.seen(d, 1.0).normalize();
        let shift = far.angle(&near);
        let t = Vector3::from(l.translation);
        let across = (t - d * d.dot(&t)).norm();
        assert!((shift - across).abs() < 1e-3, "{shift} {across}");
    }

    #[test]
    fn the_masks_are_drawn_as_their_polygons() {
        // A square include with a square hole
        let square = |a: f64, b: f64| vec![[a, a], [b, a], [b, b], [a, b]];
        let r = Raster::new(&[(true, square(100.0, 300.0)), (false, square(180.0, 220.0))], [400, 400]).unwrap();
        assert!(r.covers([150.0, 150.0]) && r.covers([250.0, 290.0]));
        assert!(!r.covers([50.0, 150.0]) && !r.covers([350.0, 350.0]) && !r.covers([200.0, 200.0]));
        // The X5's masks: about its image circle
        let rig = Rig::new(&x5(), (3840, 3840)).unwrap();
        let l = &rig.lenses[0];
        let (c, radius) = l.circle.unwrap();
        let m = l.masks.as_ref().unwrap();
        assert!(m.covers(c) && m.covers([c[0] + radius * 0.9, c[1]]) && !m.covers([c[0] + radius * 1.1, c[1]]));
    }

    #[test]
    fn equirect_pixels_are_centred() {
        let d = equirect_direction(1919.5, 959.5, 3840, 1920);
        assert!((d - Vector3::z()).norm() < 1e-12);
        let right = equirect_direction(2879.5, 959.5, 3840, 1920);
        assert!((right - Vector3::x()).norm() < 1e-12);
        let up = equirect_direction(0.0, -0.5, 3840, 1920);
        assert!((up + Vector3::y()).norm() < 1e-12);
    }
}
