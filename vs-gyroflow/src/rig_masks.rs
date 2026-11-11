// SPDX-License-Identifier: GPL-3.0-or-later
//
// An Insta360 rig's stitch, as Insta360 Studio 6.0.2 does it: how far each lens is read (its mask disc), the crossfade
// at the seam, and the outline of the body and its accessories around each lens's picture (its mask). telemetry-parser
// worked these out until rev e4eb02e (the one Gyroflow 1.6 used); since rev eb2bcba (Gyroflow 1.7.0's) it states only
// each lens's field and the facts a vendor's software stitches by (`CameraRig::vendor`, Insta360Facts), the stitch
// being "the vendor software's choice and not the file's". The player stitches as Studio does, as before: below
// `stitch` is telemetry-parser e4eb02e's own code, unchanged, src/insta360/rig_body_masks.rs without its user-facing
// mask profiles, and src/insta360/rig_mask_coefficients.rs. Those are Adrian Eddy's, MIT OR Apache-2.0
// (telemetry-parser's LICENSE-MIT: Copyright (c) 2021 Adrian <adrian.eddy at gmail>), here under the GPL with the rest
// of the plugin; the comments in them are theirs.

use gyroflow_core::telemetry_parser::rig::{ Insta360Facts, geometry::{ Blend, Intrinsics, Lens, LensModel, MaskPolarity, MaskPoly, MeiDistortion } };
use coefficients::{ full_fov, polynomial };

/// Studio's stitch for the lenses of an Insta360 `model` with these facts, in place of the bare field telemetry-parser
/// states for them: each lens read to its mask disc (its lens type's field less 2°), a two-lens body crossfaded 6° about
/// the seam, and the body's outline masked. What telemetry-parser e4eb02e's `insta360::rig` set (`mask_disc_rad`,
/// `blend_of`, `rig_body_masks::automatic`)
pub fn stitch(model: &str, lenses: &mut [Lens], facts: &Insta360Facts) {
    if facts.lens_types.len() != lenses.len() { return; }
    let two = lenses.len() == 2;
    for (lens, &lens_type) in lenses.iter_mut().zip(&facts.lens_types) {
        let hard = (class_half_field_deg(lens_type) - 2.0).to_radians();
        lens.fov_limit = Some(if two { mask_disc_rad(lens_type) } else { hard });
        // `90° + 3°` in degrees, converted once, see `mask_disc_rad`
        lens.blend = if two { Blend { limit: (90.0 + BLEND_DEG / 2.0).to_radians(), width: BLEND_DEG.to_radians() } } else { Blend::hard(hard) };
        lens.masks.clear();
    }
    automatic(model, lenses, &facts.lens_types, facts.bullet_time);
}

/// The vendor's own fisheye mask disc, `GetFov/2 − 2°` (98° for a 200° lens, 93° for the 190° default): Studio 6.0.2
/// masks every fisheye with `TemplateBlenderImpl::calcFisheyeMaskv`, whose default field is `GetFov × 0.5 − 2.0`, a
/// disc no read of the picture ever crosses, whatever the blend
fn mask_disc_rad(lens_type: i32) -> f64 {
    // Subtracted in degrees and converted once, so that on the 190° class, where the disc and the blend's end are
    // both 93°, the two convert to the same bits and the blend is not a rounding error past the validity
    (full_fov(lens_type).unwrap_or(190.) / 2.0 - 2.0).to_radians()
}

/// Half the field of the vendor's lens-type table (`Insta360Lens::GetFov`), the lens class: 100° for type 113 (the
/// X5's 200° lens), 95° for a type the table has no entry for
fn class_half_field_deg(lens_type: i32) -> f64 {
    let full_deg = match lens_type {
        19 | 33 | 38 | 40 | 41 | 44 | 70 | 71 | 85 | 113 | 131 | 142 | 149 | 150 | 193 => 200.0,
        140 | 141 | 156 => 195.0,
        _ => 190.0,
    };
    full_deg / 2.0
}

/// The blend Studio applies to a two-lens body's video: a linear crossfade this many degrees wide, centred on the seam
/// meridians, so each lens is read to `90° + 3°` and the two cross at equal weight on the great circle between them
/// (`StitchingService::setupDynamicStitching`'s default for every body but some of Studio's own stitch modes)
const BLEND_DEG: f64 = 6.0;

// ---- telemetry-parser e4eb02e, src/insta360/rig_body_masks.rs (Studio 6.0.2 image-coverage contours) ----

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Family {
    OneX,
    X2,
    OneR,
    X3,
    X4,
    X5,
    X4Air,
    X6,
    PC,
    DM,
}
impl Family {
    fn model(model: &str) -> Option<Self> {
        Some(
            match model
                .trim_start_matches("Insta360 ")
                .to_ascii_uppercase()
                .as_str()
            {
                "ONE X" => Self::OneX,
                "ONE X2" | "X2" => Self::X2,
                "ONE R" | "ONE RS" | "ONER" | "ONERS" => Self::OneR,
                "X3" => Self::X3,
                "X4" => Self::X4,
                "X5" => Self::X5,
                "X4 AIR" => Self::X4Air,
                "X6" => Self::X6,
                "PC" => Self::PC,
                "DM" => Self::DM,
                _ => return None,
            },
        )
    }
}
#[derive(Clone, Copy)]
struct Recipe {
    family: Family,
    lens_type: i32,
    cooling: bool,
    variant: u8,
}

fn automatic(model: &str, lenses: &mut [Lens], lens_types: &[i32], bullet_time: bool) {
    let Some(family) = Family::model(model) else {
        return;
    };
    if lenses.len() != 2 || lens_types.len() != 2 {
        return;
    }
    let Some(masks) = lenses
        .iter()
        .enumerate()
        .map(|(i, lens)| {
            mask(
                lens,
                Recipe {
                    family,
                    lens_type: lens_types[i],
                    cooling: false,
                    variant: u8::from(family == Family::X6 && bullet_time),
                },
                i,
                lens_types[0],
            )
        })
        .collect::<Option<Vec<_>>>()
    else {
        return;
    };
    // Native dispatch overrides the default disc for these accessories.
    // This belongs to the parsed rig; a later user mask override preserves it.
    let outer: Option<f64> = match (family, lens_types[0]) {
        (Family::X4, 106 | 107) | (Family::X4Air, 140 | 141 | 196) => Some(96.),
        (Family::X6, 198 | 199) => Some(94.),
        (Family::PC, 83) => Some(91.),
        (Family::DM, _) => Some(98.),
        _ => None,
    };
    for (lens, mask) in lenses.iter_mut().zip(masks) {
        lens.masks = vec![mask];
        if let Some(outer) = outer { lens.fov_limit = Some(outer.to_radians()); }
    }
}

fn mask(lens: &Lens, recipe: Recipe, index: usize, first_type: i32) -> Option<MaskPoly> {
    use Family::*;
    let Recipe {
        family,
        lens_type: t,
        cooling,
        variant,
    } = recipe;
    if family == X5 {
        let pair = if variant == 1 && first_type == 113 {
            [AI_LEFT, if cooling { COOLING } else { AI_RIGHT }]
        } else {
            tables(first_type, cooling)?
        };
        return outline(lens, pair[index], pair[0].last()?[1]);
    }
    let (pair, axis, outer) = match family {
        OneX if matches!(t, 19 | 27) => ([X2_TABLE; 2], if index == 0 { -90. } else { 90. }, None),
        X2 if matches!(t, 41 | 42) => ([X2_TABLE; 2], if index == 0 { 90. } else { -90. }, None),
        OneR if matches!(t, 33 | 83) => ([ONE_R_LEFT, ONE_R_RIGHT], 0., None),
        X2 | OneR if matches!(t, 38 | 39 | 40 | 43 | 51) => ([EMPTY; 2], 0., None),
        X3 if matches!(t, 71 | 76..=79 | 84 | 86 | 87) => (
            [if matches!(t, 86 | 87) {
                X3_DIVE
            } else {
                X3_TABLE
            }; 2],
            0.,
            None,
        ),
        X4 if matches!(t, 71 | 106..=108 | 86 | 87) => (
            if matches!(t, 86 | 87) {
                [X3_DIVE; 2]
            } else {
                [X4_LEFT, if cooling { X4_COOLING } else { X4_RIGHT }]
            },
            0.,
            matches!(t, 106 | 107).then_some(96.),
        ),
        X4Air if matches!(t, 131 | 140..=142 | 147..=150 | 196) => (
            if matches!(t, 147..=150) {
                [X3_DIVE; 2]
            } else {
                [X4_LEFT, if cooling { X4_COOLING } else { X4_RIGHT }]
            },
            0.,
            matches!(t, 140 | 141 | 196).then_some(96.),
        ),
        X6 if t == 193 => (
            if variant == 1 {
                [X6_BULLET_LEFT, X6_BULLET_RIGHT]
            } else {
                [X6_LEFT, X6_RIGHT]
            },
            0.,
            None,
        ),
        X6 if t == 197 => (
            [
                GUARD_LEFT,
                if cooling { GUARD_COOLING } else { GUARD_RIGHT },
            ],
            0.,
            None,
        ),
        X6 if matches!(t, 198 | 199) => ([DIVE; 2], 0., Some(94.)),
        PC if matches!(t, 33 | 83) => ([PC_TABLE; 2], 0., (t == 83).then_some(91.)),
        DM if matches!(t, 85 | 109) => (
            match variant {
                0 => [DM_LEFT, DM_RIGHT],
                1 => [DM_GUARD_LEFT, DM_GUARD_RIGHT],
                2 => [DM_LARGE_LEFT, DM_LARGE_RIGHT],
                3 => [DM_LEFT, DM_DONGLE_RIGHT],
                _ => return None,
            },
            // TemplateBlenderBase ctor (0x2b46c44): tail orientation = 1.
            0.,
            Some(98.),
        ),
        _ => return None,
    };
    let outer = outer.unwrap_or(full_fov(first_type)? / 2. - 2.);
    polynomial_outline(lens, recipe, pair[index], axis, outer, index, first_type)
}

fn evaluate(coeff: [f64; 5], theta: f64) -> f64 {
    coeff.into_iter().rev().fold(0., |acc, c| acc * theta + c)
}

fn interpolate(table: Table, angle: f64) -> Option<f64> {
    table.windows(2).find_map(|s| {
        let [[a, x], [b, y]] = [s[0], s[1]];
        (angle >= a.min(b) && angle <= a.max(b)).then(|| x + (y - x) * (angle - a) / (b - a))
    })
}

fn polynomial_outline(
    lens: &Lens,
    recipe: Recipe,
    table: Table,
    axis: f64,
    outer: f64,
    index: usize,
    first_type: i32,
) -> Option<MaskPoly> {
    let circle = lens.image_circle.as_ref()?;
    let coeff = polynomial(recipe.lens_type)?;
    let denominator = evaluate(polynomial(first_type)?, full_fov(recipe.lens_type)? / 2.);
    if !(denominator.is_finite()
        && denominator > 0.
        && circle.radius.is_finite()
        && circle.radius > 0.)
    {
        return None;
    }
    let radius = |theta| circle.radius * evaluate(coeff, theta) / denominator;
    let base = radius(outer);
    if !(base.is_finite() && base > 0.) {
        return None;
    }
    let mut points = Vec::with_capacity(720);
    for step in 0..720 {
        let degrees = step as f64 * 0.5 - 180.;
        let (sin, cos) = degrees.to_radians().sin_cos();
        let phi = ((degrees - axis + 180.).rem_euclid(360.) - 180.).abs();
        let is_one_r = recipe.family == Family::OneR && !table.is_empty();
        let angle = if is_one_r {
            (-cos).atan2(sin).to_degrees()
        } else {
            phi
        };
        let allowed = if is_one_r {
            cos < -0.17364818
        } else if recipe.family == Family::DM {
            phi <= if recipe.variant == 2 { 180. } else { 90. }
        } else {
            phi < 90.
        };
        let mut r = if allowed {
            interpolate(table, angle).map_or(base, |t| radius(t).min(base))
        } else {
            base
        };
        if matches!(recipe.family, Family::OneX | Family::X2) && recipe.variant != 0 {
            let cold_axis = if index == 0 { 90. } else { -90. };
            let cold_phi = ((degrees - cold_axis + 180.).rem_euclid(360.) - 180.).abs();
            if recipe.variant == 1
                || (recipe.variant == 2 && cos < 0.)
                || (recipe.variant == 3 && cos >= 0.)
            {
                if let Some(theta) = interpolate(COLD_SHOE, cold_phi) {
                    r = r.min(radius(theta));
                }
            }
        }
        if !(r.is_finite() && r > 0.) {
            return None;
        }
        points.push([circle.centre[0] + r * sin, circle.centre[1] + r * cos]);
    }
    Some(MaskPoly {
        polarity: MaskPolarity::Include,
        points,
    })
}

type Table = &'static [[f64; 2]]; // azimuth, half FOV in degrees
const EMPTY: Table = &[];
// Builder addresses and variant provenance are recorded in INSTA360-MASKS.md.
const X2_TABLE: Table = &[
    [0., 90.5],
    [13., 90.5],
    [18., 91.5],
    [24., 92.5],
    [28., 93.5],
    [37., 94.5],
    [45., 95.5],
    [53.5, 96.5],
    [57., 97.5],
    [61., 98.5],
    [70., 99.5],
];
const COLD_SHOE: Table = &[[0., 90.5], [56., 90.5], [57., 97.5]];
const X3_TABLE: Table = &[
    [0., 91.],
    [13., 91.],
    [18., 92.],
    [24., 93.5],
    [28., 95.5],
    [37., 96.5],
    [45., 96.5],
    [53.5, 96.5],
    [57., 97.5],
    [61., 98.5],
    [70., 99.5],
];
const X3_DIVE: Table = &[
    [0., 91.],
    [13., 91.],
    [18., 91.],
    [24., 94.5],
    [28., 95.5],
    [37., 96.5],
    [45., 96.5],
    [53.5, 96.5],
    [57., 97.5],
    [61., 98.5],
    [70., 99.5],
];
const X4_LEFT: Table = &[
    [0., 91.3],
    [13., 91.3],
    [18., 93.25],
    [24., 94.5],
    [28., 95.5],
    [37., 96.5],
    [45., 96.5],
    [53.5, 96.5],
    [57., 97.5],
    [61., 98.5],
    [70., 99.5],
];
const X4_RIGHT: Table = &[
    [0., 91.5],
    [10., 91.5],
    [15., 92.],
    [18., 93.5],
    [23., 95.25],
    [30., 97.],
    [31., 97.5],
];
const X4_COOLING: Table = &[
    [0., 90.8],
    [14., 91.],
    [19., 91.5],
    [24., 92.5],
    [28., 93.5],
    [37., 94.5],
    [45., 94.5],
    [53.5, 95.5],
    [57., 96.5],
    [61., 97.5],
    [70., 99.5],
];
const X6_LEFT: Table = &[
    [0., 90.2],
    [13., 90.2],
    [18., 91.25],
    [24., 92.25],
    [28., 93.25],
    [37., 94.25],
    [45., 95.5],
    [53.5, 96.5],
    [57., 97.5],
    [61., 98.5],
    [70., 99.5],
];
const X6_RIGHT: Table = &[
    [0., 91.],
    [10., 91.],
    [13., 91.],
    [18., 91.5],
    [40., 95.5],
    [57., 98.5],
    [61., 100.],
];
const X6_BULLET_LEFT: Table = &[
    [0., 91.2],
    [13., 91.2],
    [18., 92.25],
    [24., 93.25],
    [28., 94.25],
    [37., 95.25],
    [45., 96.5],
    [53.5, 96.5],
    [57., 97.5],
    [61., 98.5],
    [70., 99.5],
];
const X6_BULLET_RIGHT: Table = &[
    [0., 91.5],
    [10., 91.5],
    [13., 91.5],
    [18., 92.],
    [40., 96.5],
    [57., 98.5],
    [61., 100.],
];
const PC_TABLE: Table = &[
    [0., 90.5],
    [18., 90.5],
    [24., 91.],
    [28., 91.5],
    [37., 93.5],
    [45., 95.],
    [53.5, 96.],
    [57., 97.],
    [61., 98.],
    [79., 98.],
];
const ONE_R_LEFT: Table = &[
    [130., 98.],
    [128., 98.],
    [126., 97.],
    [120., 95.],
    [112., 94.5],
    [111., 95.],
    [100., 95.],
    [80., 95.],
    [65., 94.],
    [50., 95.],
    [42., 95.5],
    [37., 96.],
    [20., 98.],
];
const ONE_R_RIGHT: Table = &[
    [164., 98.],
    [146., 98.],
    [138., 97.],
    [132., 96.],
    [129., 95.],
    [118., 94.],
    [114., 94.5],
    [68., 94.5],
    [67., 93.],
    [50., 93.],
    [45., 94.],
    [40., 96.],
    [39., 98.],
];
const DM_LEFT: Table = &[
    [0., 91.],
    [10., 90.8],
    [30., 90.5],
    [48., 90.5],
    [55., 93.],
    [60., 98.],
    [115., 95.5],
    [130., 95.5],
    [180., 98.],
];
const DM_RIGHT: Table = &[
    [0., 97.],
    [15., 96.],
    [25., 94.],
    [40., 96.],
    [77., 96.],
    [90., 98.],
];
const DM_GUARD_LEFT: Table = &[
    [0., 91.],
    [10., 90.8],
    [30., 90.5],
    [48., 90.5],
    [73., 95.],
    [110., 95.],
    [115., 93.],
    [125., 93.],
    [130., 96.],
    [180., 98.],
];
const DM_GUARD_RIGHT: Table = &[
    [0., 97.],
    [16., 96.],
    [17., 92.],
    [42., 92.],
    [50., 95.],
    [90., 95.],
    [98., 98.],
];
const DM_LARGE_LEFT: Table = &[
    [0., 91.],
    [10., 90.8],
    [30., 90.5],
    [48., 90.5],
    [55., 93.],
    [80., 94.],
    [90., 96.],
    [98., 93.],
    [135., 94.],
    [180., 98.],
];
const DM_LARGE_RIGHT: Table = &[
    [0., 95.],
    [15., 92.],
    [25., 91.25],
    [40., 91.5],
    [77., 91.75],
    [90., 92.25],
    [98., 94.5],
    [115., 96.],
];
const DM_DONGLE_RIGHT: Table = &[
    [0., 92.],
    [15., 92.5],
    [25., 93.5],
    [40., 96.],
    [77., 96.],
    [90., 98.],
];
const AI_LEFT: Table = &[[0., 90.5], [11., 90.5], [17., 92.], [33., 95.]];
const AI_RIGHT: Table = &[[0., 91.5], [10., 91.5], [15., 92.], [20., 93.], [27., 94.]];
const LEFT: Table = &[[0., 90.5], [11., 90.5], [17., 92.], [33., 95.7], [57., 97.]];
const RIGHT: Table = &[
    [0., 91.5],
    [10., 91.5],
    [15., 92.],
    [20., 93.],
    [27., 94.],
    [40., 97.],
];
const GUARD_LEFT: Table = &[[0., 90.5], [11., 90.5], [17., 92.], [33., 94.]];
const GUARD_RIGHT: Table = &[[0., 91.5], [10., 91.5], [15., 92.], [18., 93.5], [23., 94.]];
const COOLING: Table = &[
    [0., 90.5],
    [11., 90.5],
    [17., 90.5],
    [20., 91.5],
    [24., 92.],
    [28., 92.5],
    [37., 93.],
    [45., 94.5],
    [53.5, 95.5],
    [57., 96.5],
    [61., 97.],
];
const GUARD_COOLING: Table = &[
    [0., 90.5],
    [11., 90.5],
    [17., 90.5],
    [20., 91.5],
    [24., 92.],
    [28., 92.5],
    [37., 93.],
    [45., 94.],
];
const DIVE: Table = &[[0., 90.5], [10., 90.5], [20., 91.8], [60., 94.]];
const DIVE_ALT: Table = &[[0., 91.], [10., 91.], [32., 92.5], [55., 93.5]];

fn tables(lens_type: i32, cooling: bool) -> Option<[Table; 2]> {
    Some(match lens_type {
        113 => [LEFT, if cooling { COOLING } else { RIGHT }],
        115 | 156 => [
            GUARD_LEFT,
            if cooling { GUARD_COOLING } else { GUARD_RIGHT },
        ],
        117 | 118 => [DIVE, DIVE],
        119 | 120 => [DIVE_ALT, DIVE_ALT],
        _ => return None,
    })
}

fn outline(lens: &Lens, table: Table, outer_deg: f64) -> Option<MaskPoly> {
    let intr = &lens.intrinsics;
    let outer = outer_deg.min(lens.fov_limit?.to_degrees());
    let base = radius_squared(intr, outer, 0.)?;
    let radii = table
        .iter()
        .map(|&[phi, theta]| radius_squared(intr, theta, phi))
        .collect::<Option<Vec<_>>>()?;
    // ErodeCircleMethord3 (0x2b693dc) works on the right half AFTER a 90-degree
    // CCW rotation. In the original lens image this is the BOTTOM, symmetric
    // about cx. Interpolate squared PIXEL radius, never theta or radius itself.
    // A half-degree step retains each table knot and has <0.03 px circle sag
    // at the X5's 5376-pixel calibration size. No per-frame polygon generation.
    let mut points = Vec::with_capacity(720);
    for step in 0..720 {
        let degrees = step as f64 * 0.5 - 180.;
        let phi = degrees.abs();
        let mut r2 = base;
        for (i, segment) in table.windows(2).enumerate() {
            let [a, b] = [segment[0][0], segment[1][0]];
            if phi >= a && phi < b {
                r2 = base.min(radii[i] + (radii[i + 1] - radii[i]) * (phi - a) / (b - a));
                break;
            }
        }
        let (sin, cos) = degrees.to_radians().sin_cos();
        points.push([
            intr.centre[0] + r2.sqrt() * sin,
            intr.centre[1] + r2.sqrt() * cos,
        ]);
    }
    Some(MaskPoly {
        polarity: MaskPolarity::Include,
        points,
    })
}

/// The vendor cancels its camera rotation before sampling the dewarp map in
/// calcFisheyeCoordFromFOV. Evaluate the equivalent local Mei projection directly.
/// Its output azimuth is used only to measure radius; the outline is symmetric.
fn radius_squared(intr: &Intrinsics, theta: f64, phi: f64) -> Option<f64> {
    let LensModel::Mei { xi, dist } = intr.model else {
        return None;
    };
    let (st, ct) = theta.to_radians().sin_cos();
    let (sp, cp) = phi.to_radians().sin_cos();
    let den = xi + ct;
    if den <= 0. {
        return None;
    }
    let (x, y) = (st * sp / den, st * cp / den);
    let r2 = x * x + y * y;
    let (xd, yd) = match dist {
        MeiDistortion::BrownV3 { k, p } => {
            let radial = 1. + r2 * (k[0] + r2 * (k[1] + r2 * k[2]));
            (
                x * radial + 2. * p[0] * x * y + p[1] * (r2 + 2. * x * x),
                y * radial + p[0] * (r2 + 2. * y * y) + 2. * p[1] * x * y,
            )
        }
        MeiDistortion::RadtanPro { d } => {
            let radial = 1. + r2 * (d[0] + r2 * (d[1] + r2 * (d[2] + r2 * (d[3] + r2 * d[4]))));
            let a = d[5] + d[7] * r2;
            let b = d[6] + d[8] * r2;
            (
                x * radial + (r2 + 2. * x * x) * a + 2. * x * y * b + d[9] * r2 + d[11] * r2 * r2,
                y * radial + (r2 + 2. * y * y) * b + 2. * x * y * a + d[10] * r2 + d[12] * r2 * r2,
            )
        }
    };
    let radius = (intr.focal[0] * xd).powi(2) + (intr.focal[1] * yd).powi(2);
    (radius.is_finite() && radius > 0.).then_some(radius)
}

// ---- telemetry-parser e4eb02e, src/insta360/rig_mask_coefficients.rs ----

mod coefficients {
// Generated by docs/360/oracle/insta_mask_coefficients.py in oxivideo.
// Studio 6.0.2 arm64: Lens::getCoeff 0x4c5e48, getFov table 0x3daf020.
// Mask polynomials in DEGREES. Legacy optics reuse only a verified subset.
pub(super) fn polynomial(lens_type: i32) -> Option<[f64; 5]> {
    Some(match lens_type {
        1 => [-0.0004, 0.0192, -7.897e-06, -1.045e-07, -2.494e-09],
        2 => [-0.0004, 0.0192, -7.897e-06, -1.045e-07, -2.494e-09],
        3 => [0.0, 0.033707, 0.00010995, -3.4202e-06, 9.3345e-09],
        4 => [0.0007, 0.0183, 8.474e-06, -5.955e-07, 1.014e-09],
        6 => [-0.0008, 0.018, -1.252e-05, -6.696e-08, -1.437e-09],
        8 => [-0.0025, 0.0169, -3.679e-05, 6.7575e-07, -4.1093e-09],
        9 => [0.0026, 0.0171, 1.219e-05, -2.7019e-07, -9.5596e-10],
        10 => [0.0, 0.0166, -1.1297e-05, 1.3325e-07, -2.015e-09],
        11 => [0.0, 0.0169, -1.8189e-05, 2.4828e-07, -2.6403e-09],
        12 => [-0.001, 0.0331, -1.6306e-05, 1.3047e-07, -3.2436e-09],
        25 => [-0.001, 0.0331, -1.6306e-05, 1.3047e-07, -3.2436e-09],
        13 => [-0.0028074, 0.024317, -5.045e-05, 1.1095e-06, -8.8568e-09],
        17 => [-0.0028074, 0.024317, -5.045e-05, 1.1095e-06, -8.8568e-09],
        26 => [-0.0028074, 0.024317, -5.045e-05, 1.1095e-06, -8.8568e-09],
        14 => [-0.00087373, 0.015051, -1.3445e-05, 2.2256e-07, -2.0802e-09],
        15 => [0.0, 0.018777, 1.3379e-05, -1.7029e-07, -3.38e-09],
        28 => [0.0, 0.018777, 1.3379e-05, -1.7029e-07, -3.38e-09],
        16 => [0.0, 0.018393, -2.1462e-05, 4.0165e-07, -3.7618e-09],
        18 => [0.0, 0.031036, -8.0394e-06, -3.3297e-07, -2.9722e-09],
        19 => [0.0, 0.024134, -7.4557e-05, 2.0846e-06, -1.4576e-08],
        33 => [0.0, 0.024134, -7.4557e-05, 2.0846e-06, -1.4576e-08],
        21 => [0.0, 1.0, -0.000198467, 4.4154e-06, -1.77966e-08],
        22 => [0.0, 1.0, 0.000268828, 1.82414e-05, -1.54088e-07],
        23 => [0.0, 0.020237, 1.2379e-05, -6.4541e-07, -4.954e-10],
        24 => [0.0, 0.0256, -7.0571e-05, 2.2326e-06, -1.7215e-08],
        27 => [0.0, 0.023693, -5.9584e-05, 1.8402e-06, -1.4809e-08],
        29 => [0.0, 0.024029, -6.86e-05, 2.0052e-06, -1.4266e-08],
        30 => [0.0, 0.019269, 6.4339e-06, 9.0452e-07, -9.2109e-09],
        31 => [0.0, 0.10392, 1.8477e-05, 3.6264e-08, -2.2362e-08],
        32 => [0.0, 1.0, -0.001663, 4.0685e-05, -3.4541e-07],
        34 => [0.0, 0.05247669, -2.57248e-05, 8.14169254e-07, -7.908613e-09],
        68 => [0.0, 0.05247669, -2.57248e-05, 8.14169254e-07, -7.908613e-09],
        35 => [
            0.0,
            0.09279641,
            -1.4925244e-05,
            1.7604798e-06,
            -2.254106e-09,
        ],
        37 => [0.0, 0.025880234, -0.000100010007, 5.29093e-06, -4.71066e-08],
        38 => [0.0, 0.024336, -7.55532e-05, 2.13171e-06, -1.5040305e-08],
        39 => [0.0, 0.0236785, -5.894477e-05, 1.810191e-06, -1.4521993e-08],
        40 => [0.0, 0.0259398, -7.3377467e-05, 2.2925e-06, -1.7776969e-08],
        41 => [
            0.0,
            0.02413266,
            -7.451592e-05,
            2.0839511e-06,
            -1.4573211e-08,
        ],
        42 => [
            0.0,
            0.024446313655691,
            -7.9118706055e-05,
            2.24180908e-06,
            -1.5675326e-08,
        ],
        51 => [
            0.0,
            0.024446313655691,
            -7.9118706055e-05,
            2.24180908e-06,
            -1.5675326e-08,
        ],
        43 => [0.0, 0.02586, -8.1e-05, 2.41e-06, -1.819e-08],
        44 => [0.0, 0.02434, -7.555e-05, 2.132e-06, -1.504e-08],
        45 => [
            0.0,
            0.02956668,
            -1.437188e-05,
            5.78260464e-07,
            -7.237882e-09,
        ],
        50 => [
            0.0,
            0.03638676938359,
            -5.4394828366e-05,
            1.194065668e-06,
            -1.8897389e-08,
        ],
        81 => [
            0.0,
            0.03638676938359,
            -5.4394828366e-05,
            1.194065668e-06,
            -1.8897389e-08,
        ],
        52 => [0.0, 0.02351, -4.995e-05, 1.609e-06, -1.324e-08],
        53 => [0.0, 0.02467, -8.474e-05, 2.384e-06, -1.664e-08],
        54 => [0.0, 0.02279, -6.703e-05, 1.849e-06, -1.206e-08],
        55 => [0.0, 0.02445, -7.925e-05, 2.245e-06, -1.57e-08],
        56 => [0.0, 0.01841, -2.251e-05, 4.293e-07, -3.919e-09],
        58 => [-0.004578, 0.05344, -0.0001393, 5.218e-06, -4.475e-08],
        74 => [-0.004578, 0.05344, -0.0001393, 5.218e-06, -4.475e-08],
        59 => [-0.003288, 0.02443, -7.682e-05, 2.184e-06, -1.536e-08],
        62 => [0.0, 0.04216, -4.022e-05, 1.604e-06, -1.5e-08],
        63 => [0.0, 0.08178, 0.0001012, 6.863e-06, 5.401e-08],
        64 => [0.0, 0.04272, -4.509e-05, 1.774e-06, -1.642e-08],
        70 => [0.0, 0.02235, -5.382e-05, 2.001e-06, -1.392e-08],
        71 => [0.0, 0.02249, -5.79e-05, 1.932e-06, -1.295e-08],
        75 => [0.0, 0.05271, -0.0001055, 4.62e-06, -4.118e-08],
        80 => [0.0, 0.05271, -0.0001055, 4.62e-06, -4.118e-08],
        76 => [0.0, 0.02302, -5.102e-05, 1.855e-06, -1.285e-08],
        77 => [0.0, 0.02296, -5.413e-05, 1.95e-06, -1.334e-08],
        78 => [0.0, 0.02481, -8.254e-05, 2.739e-06, -1.972e-08],
        79 => [0.0, 0.02252, -4.713e-05, 1.749e-06, -1.191e-08],
        82 => [0.0, 0.05343, -0.0001819, 4.973e-06, -3.637e-08],
        83 => [0.0, 0.024, -6.316e-05, 2.013e-06, -1.456e-08],
        84 => [0.0, 0.02299, -5.257e-05, 1.903e-06, -1.31e-08],
        85 => [0.0, 0.02513, -1.602e-05, 3.923e-07, -4.763e-09],
        86 => [0.0, 0.02056, 0.000135, -2.033e-06, 8.676e-09],
        87 => [0.0, 0.01859, 0.0002531, -4.285e-06, 1.96e-08],
        89 => [0.0, 0.07682, 0.0001, -7.677e-07, -1.688e-10],
        102 => [0.0, 0.04901, 0.0001308, -1.795e-06, 8.665e-09],
        103 => [0.0, 0.07046, -8.085e-05, 9.264e-06, -9.001e-08],
        106 => [0.0, 0.02279, -4.871e-05, 1.795e-06, -1.235e-08],
        107 => [0.0, 0.02264, -4.94e-05, 1.811e-06, -1.228e-08],
        108 => [0.0, 0.0227, -4.905e-05, 1.803e-06, -1.232e-08],
        109 => [0.0, 0.0265, -8.148e-06, 1.107e-07, -3.412e-09],
        111 => [0.0, 0.04953, 0.000107, -1.215e-06, 6.113e-09],
        112 => [0.0, 0.03139, -4.778e-05, 1.476e-06, -1.045e-08],
        113 => [0.0, 0.03159, -8.415e-05, 2.201e-06, -1.284e-08],
        114 => [0.0, 0.0373774417, -0.0001059214, 3.5811e-06, -2.03e-08],
        115 => [0.0, 0.0321906505, -9.37245e-05, 2.5248e-06, -1.5e-08],
        117 => [0.0, 0.03072, 2.395e-05, 6.076e-08, -2.092e-09],
        118 => [0.0, 0.02859, 2.395e-05, 6.076e-08, -2.092e-09],
        119 => [0.0, 0.03025, 5.341e-05, 1.623e-07, -6.208e-09],
        120 => [0.0, 0.0283, 0.0001661, -2.11e-06, 5.046e-09],
        121 => [0.0, 0.04758, -2.925e-05, 2.431e-06, -1.862e-08],
        122 => [0.0, 0.0572, 0.0005829, -1.6358e-05, 3.1562e-07],
        126 => [0.0, 0.06073, 0.0001221, -2.422e-07, 1.845e-08],
        131 => [
            0.0,
            0.023200178479423714,
            -2.6527512394617657e-05,
            1.2910990660481257e-06,
            -1.0421589669384779e-08,
        ],
        132 => [0.0, 0.0692350926, -0.0001582334, 4.2251e-06, -1.55e-08],
        140 => [
            0.0,
            0.02347874251877121,
            -2.7674053581301105e-05,
            1.3785975546613045e-06,
            -1.1205771662550173e-08,
        ],
        141 => [
            0.0,
            0.050058759932186774,
            -0.00010797875428738478,
            6.946467501059848e-06,
            -1.2074006897891668e-07,
        ],
        142 => [
            0.0,
            0.024508114438217286,
            -2.3584121988814713e-05,
            7.382934532711228e-07,
            -6.482923649818883e-09,
        ],
        144 => [
            0.0,
            0.12438555392403998,
            0.0002479751929076328,
            -3.7340206414873895e-06,
            3.2034028362148135e-07,
        ],
        242 => [
            0.0,
            0.12438555392403998,
            0.0002479751929076328,
            -3.7340206414873895e-06,
            3.2034028362148135e-07,
        ],
        145 => [
            0.0,
            0.25824308167628196,
            -0.00015105065170424535,
            5.064253775010642e-05,
            -3.3091941273973424e-07,
        ],
        147 => [0.0, 0.02311, 6.271e-05, -5.268e-07, -1.293e-09],
        148 => [0.0, 0.0245, 5.707e-05, -9.534e-07, 2.095e-09],
        149 => [0.0, 0.0213, 0.000204, -3.485e-06, 1.458e-08],
        150 => [0.0, 0.02271, 0.00019, -3.754e-06, 1.706e-08],
        154 => [0.0, 0.030634545, 1.4919e-05, 1.111e-06, -1.2e-08],
        155 => [0.0, 0.032408701, -8.8927e-05, 2.4e-06, -1.6e-08],
        156 => [0.0, 0.0319439907, -9.41567e-05, 2.5305e-06, -1.47e-08],
        176 => [0.0, 0.07343921, 0.00166989, -7.88e-05, 1.33e-06],
        177 => [0.0, 0.06491602, 0.00100608, -3.935e-05, 5.4e-07],
        178 => [0.0, 0.06512271, 0.00058398, -2.109e-05, 2.7e-07],
        179 => [0.0, 0.0660192, 0.00030978, -1.069e-05, 1.4e-07],
        190 => [0.0, 0.0687763, 0.00155428, -6.757e-05, 1.04e-06],
        191 => [0.0, 0.0506809, 0.00063007, -2.195e-05, 3.1e-07],
        193 => [
            0.0,
            0.0454490911,
            -8.74282443e-05,
            3.79173971e-06,
            -2.77278772e-08,
        ],
        194 => [0.0, 0.05975867, -3.482e-05, 5.6e-06, -3e-08],
        195 => [0.0, 0.0480692, -7.078e-05, 2.22e-06, -1e-08],
        196 => [
            0.0,
            0.023690503074,
            -3.0558236e-05,
            1.492252e-06,
            -1.2154e-08,
        ],
        197 => [
            0.0,
            0.0462993571,
            -9.4612701e-05,
            4.15071135e-06,
            -3.05585771e-08,
        ],
        198 => [
            0.0,
            0.043319707,
            0.000296024276,
            -3.03947899e-06,
            4.07985584e-09,
        ],
        199 => [
            0.0,
            0.037579787,
            0.000560197696,
            -7.55318454e-06,
            2.55466717e-08,
        ],
        _ => return None,
    })
}

pub(super) fn full_fov(lens_type: i32) -> Option<f64> {
    Some(match lens_type {
        1 | 8 => 220.0,
        2 | 9 | 13 | 17 | 26 | 14 | 16 | 32 | 56 => 210.0,
        3 => 170.0,
        4 => 236.0,
        6 => 211.0,
        10 => 200.3,
        11 => 214.0,
        12 | 25 | 19 | 33 | 31 | 38 | 40 | 41 | 44 | 45 | 62 | 64 | 70 | 71 | 85 | 112 | 113
        | 118 | 120 | 131 | 142 | 149 | 150 | 154 | 155 | 193 => 200.0,
        15 | 28 => 203.0,
        18 => 191.0,
        21 | 27 | 30 | 39 | 42 | 51 | 43 | 52 | 53 | 54 | 55 | 59 | 76 | 77 | 78 | 79 | 83 | 84
        | 86 | 87 | 106 | 107 | 108 | 109 | 117 | 119 | 147 | 148 | 197 | 198 | 199 => 190.0,
        22 => 208.0,
        23 => 205.0,
        24 | 29 => 196.0,
        34 | 68 => 151.0,
        35 => 156.6,
        37 => 133.774,
        50 | 81 | 58 | 74 | 75 | 80 | 82 | 89 | 121 => 150.0,
        63 => 82.0,
        102 | 114 => 158.0,
        103 => 155.0,
        111 => 147.1,
        115 | 140 | 141 | 156 | 196 => 195.0,
        122 => 110.8,
        126 => 157.3,
        132 => 156.0,
        144 | 242 => 91.74627778172767,
        145 => 39.682267222317165,
        176 => 90.6,
        177 => 110.4,
        178 => 121.0,
        179 => 126.4,
        190 => 98.0,
        191 => 106.6,
        194 => 145.0,
        195 => 189.0,
        _ => return None,
    })
}
}
