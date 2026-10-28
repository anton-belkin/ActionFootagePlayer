// SPDX-License-Identifier: GPL-3.0-or-later
//
// The smooth fill (edges.rs): where nothing is known, the colors around spread in, smoother the farther they go. In
// EdgeMode::Sphere it's what's made up on the sphere, in its wide view, from all it has; in EdgeMode::Smooth, past
// the frame on the screen, from the frame alone.
//
// It's pull-push (Gortler et al., The Lumigraph, 1996): the known cells, weighted by how much is known there, are
// averaged down a pyramid to one cell; back up, each level keeps what it knows and takes the rest from the coarser
// one, bilinear. A gap a few cells wide gets the colors on its sides, a wide one their average. On the coarse grid
// (~240x135) that's a fraction of a millisecond.
//
// What's made up is also muted, the farther from what's known: less saturated and a little darker, the way the
// periphery of a lens falls off, so it doesn't draw the eye from the picture. Not at the edge itself: muted by how
// little is known there, the fill was muted right where it blends into the picture over the mask's feather, which
// is inside the frame, and the picture next to it wasn't, a step of ~13 % in brightness along the edge.

/// How much of the saturation and of the brightness (above black) goes far from what's known
const DESATURATE: f32 = 0.35;
const DARKEN: f32 = 0.12;
/// How far from what's known the muting comes in fully, a fraction of the output grid's height
pub const MUTE_RAMP: f32 = 0.12;

/// `planes` with what `known` (0-1 per cell) doesn't cover filled in, smoothly. Where nothing at all is known, as is
pub fn fill(planes: &mut [Vec<f32>], known: &[f32], (w, h): (usize, usize)) {
    struct Level { size: (usize, usize), weight: Vec<f32>, planes: Vec<Vec<f32>> }
    let mut levels = vec![Level { size: (w, h), weight: known.iter().map(|k| k.clamp(0.0, 1.0)).collect(), planes: planes.to_vec() }];
    // Pull: colors averaged by weight, the weight summed and capped at 1 (a coarse cell with one known child of four
    // is known only a quarter)
    while { let s = levels.last().unwrap().size; s.0 > 1 || s.1 > 1 } {
        let fine = levels.last().unwrap();
        let (fw, fh) = fine.size;
        let (cw, ch) = (fw.div_ceil(2), fh.div_ceil(2));
        let mut weight = vec![0.0f32; cw * ch];
        let mut coarse = vec![vec![0.0f32; cw * ch]; planes.len()];
        for y in 0..ch {
            for x in 0..cw {
                let k = y * cw + x;
                let mut total = 0.0;
                for (fx, fy) in [(2 * x, 2 * y), (2 * x + 1, 2 * y), (2 * x, 2 * y + 1), (2 * x + 1, 2 * y + 1)] {
                    if fx >= fw || fy >= fh { continue; }
                    let i = fy * fw + fx;
                    let wt = fine.weight[i];
                    if wt <= 0.0 { continue; }
                    for (c, f) in coarse.iter_mut().zip(&fine.planes) { c[k] += f[i] * wt; }
                    total += wt;
                }
                if total > 1e-6 { for c in coarse.iter_mut() { c[k] /= total; } }
                weight[k] = total.min(1.0);
            }
        }
        levels.push(Level { size: (cw, ch), weight, planes: coarse });
    }
    if levels.last().unwrap().weight[0] <= 1e-6 { return; }
    // Push: each level's unknown part from the one above, bilinear
    for l in (0..levels.len() - 1).rev() {
        let (upper, lower) = levels.split_at_mut(l + 1);
        let (fine, coarse) = (&mut upper[l], &lower[0]);
        let (fw, fh) = fine.size;
        let (cw, ch) = coarse.size;
        for y in 0..fh {
            let cy = ((y as f32 + 0.5) * 0.5 - 0.5).clamp(0.0, (ch - 1) as f32);
            let (y0, ty) = (cy as usize, cy.fract());
            let y1 = (y0 + 1).min(ch - 1);
            for x in 0..fw {
                let i = y * fw + x;
                let wt = fine.weight[i];
                if wt >= 1.0 { continue; }
                let cx = ((x as f32 + 0.5) * 0.5 - 0.5).clamp(0.0, (cw - 1) as f32);
                let (x0, tx) = (cx as usize, cx.fract());
                let x1 = (x0 + 1).min(cw - 1);
                for (f, c) in fine.planes.iter_mut().zip(&coarse.planes) {
                    let a = c[y0 * cw + x0] + (c[y0 * cw + x1] - c[y0 * cw + x0]) * tx;
                    let b = c[y1 * cw + x0] + (c[y1 * cw + x1] - c[y1 * cw + x0]) * tx;
                    f[i] = f[i] * wt + (a + (b - a) * ty) * (1.0 - wt);
                }
                fine.weight[i] = 1.0;
            }
        }
    }
    for (p, l) in planes.iter_mut().zip(levels.swap_remove(0).planes) { *p = l; }
}

/// Mutes what's made up, by how far it is from what's known (`known`, 0-1 per cell; half known counts), easing in
/// over `ramp` cells. YUV: the chroma toward neutral and the luma toward black (limited range, as cameras record);
/// otherwise the planes toward their mean
pub fn mute(planes: &mut [Vec<f32>], known: &[f32], (w, h): (usize, usize), ramp: f32, max_value: f32, is_yuv: bool) {
    let scale = (max_value + 1.0) / 256.0;
    let (black, neutral) = (16.0 * scale, 128.0 * scale);
    let ramp = ramp.max(1.0);
    let distance = distance(known, (w, h));
    for (k, &kn) in known.iter().enumerate() {
        let t = (distance[k] / ramp).min(1.0);
        let s = (1.0 - kn.clamp(0.0, 1.0)) * t * t * (3.0 - 2.0 * t);
        if s <= 0.0 { continue; }
        let (sat, bright) = (1.0 - DESATURATE * s, 1.0 - DARKEN * s);
        if is_yuv && planes.len() == 3 {
            planes[0][k] = black + (planes[0][k] - black) * bright;
            for p in &mut planes[1..] { p[k] = neutral + (p[k] - neutral) * sat; }
        } else {
            let mean = planes.iter().map(|p| p[k]).sum::<f32>() / planes.len() as f32;
            for p in planes.iter_mut() { p[k] = (mean + (p[k] - mean) * sat) * bright; }
        }
    }
}

/// Each cell's distance in cells to the nearest that's at least half known (chamfer, two passes); far where none is
pub fn distance(known: &[f32], (w, h): (usize, usize)) -> Vec<f32> {
    let far = (w + h) as f32 * 2.0;
    let mut d: Vec<f32> = known.iter().map(|&k| if k >= 0.5 { 0.0 } else { far }).collect();
    let diagonal = std::f32::consts::SQRT_2;
    for y in 0..h {
        for x in 0..w {
            let mut v = d[y * w + x];
            if x > 0 { v = v.min(d[y * w + x - 1] + 1.0); }
            if y > 0 {
                v = v.min(d[(y - 1) * w + x] + 1.0);
                if x > 0 { v = v.min(d[(y - 1) * w + x - 1] + diagonal); }
                if x + 1 < w { v = v.min(d[(y - 1) * w + x + 1] + diagonal); }
            }
            d[y * w + x] = v;
        }
    }
    for y in (0..h).rev() {
        for x in (0..w).rev() {
            let mut v = d[y * w + x];
            if x + 1 < w { v = v.min(d[y * w + x + 1] + 1.0); }
            if y + 1 < h {
                v = v.min(d[(y + 1) * w + x] + 1.0);
                if x + 1 < w { v = v.min(d[(y + 1) * w + x + 1] + diagonal); }
                if x > 0 { v = v.min(d[(y + 1) * w + x - 1] + diagonal); }
            }
            d[y * w + x] = v;
        }
    }
    d
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_cells_stay_and_gaps_take_their_sides() {
        let (w, h) = (16, 4);
        // Known at the left (10) and the right (30) ends, nothing between
        let mut planes = vec![(0..w * h).map(|k| if k % w < 3 { 10.0 } else if k % w >= w - 3 { 30.0 } else { 0.0 }).collect::<Vec<f32>>()];
        let known: Vec<f32> = (0..w * h).map(|k| if k % w < 3 || k % w >= w - 3 { 1.0 } else { 0.0 }).collect();
        fill(&mut planes, &known, (w, h));
        let row: Vec<f32> = planes[0][w..2 * w].to_vec();
        assert_eq!(row[0], 10.0);
        assert_eq!(row[w - 1], 30.0);
        // Between, from one to the other without going outside them
        assert!(row.iter().all(|&v| (10.0..=30.0).contains(&v)), "{row:?}");
        assert!(row[4] < row[w - 5], "{row:?}");
    }

    #[test]
    fn nothing_known_leaves_it_as_is() {
        let mut planes = vec![vec![5.0f32; 12]];
        fill(&mut planes, &[0.0; 12], (4, 3));
        assert!(planes[0].iter().all(|&v| v == 5.0));
    }

    #[test]
    fn muting_spares_what_is_known() {
        let mut planes = vec![vec![500.0, 500.0], vec![700.0, 700.0], vec![300.0, 300.0]];
        mute(&mut planes, &[1.0, 0.0], (2, 1), 1.0, 1023.0, true);
        assert_eq!((planes[0][0], planes[1][0], planes[2][0]), (500.0, 700.0, 300.0));
        assert!(planes[0][1] < 500.0 && planes[1][1] < 700.0 && planes[2][1] > 300.0);
    }

    /// Next to what's known hardly muted, so the fill meets the picture in its own colors; fully far from it
    #[test]
    fn muting_eases_in_away_from_the_edge() {
        let (w, h) = (100, 100);
        let known: Vec<f32> = (0..w * h).map(|k| if k % w < 10 { 1.0 } else { 0.0 }).collect();
        let mut planes = vec![vec![500.0f32; w * h]];
        mute(&mut planes, &known, (w, h), MUTE_RAMP * h as f32, 1023.0, false);
        let row = &planes[0][50 * w..51 * w];
        assert_eq!(row[5], 500.0);
        assert!(row[10] > 497.0, "{}", row[10]);
        assert!(row[11] > row[15] && row[15] > row[20], "{:?}", &row[10..25]);
        assert!((row[99] - 500.0 * (1.0 - DARKEN)).abs() < 0.01, "{}", row[99]);
    }
}
