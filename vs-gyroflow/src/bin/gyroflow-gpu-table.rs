// SPDX-License-Identifier: GPL-3.0-or-later
//
// The GPU warp's shader for a clip (docs/gpu-render.md, step 1: the spike): shaders/gyroflow/warp.glsl with the clip's
// table (src/gpu_table.rs) in it, for mpv's glsl-shaders. mpv then warps each frame on the GPU, finding it by its PTS.
//
//   gyroflow-gpu-table [options] <project.gyroflow | video> <out.glsl>
//     --mode <m>         exact: one pass, the geometry at every pixel; grid (default): the geometry on a coarse grid,
//                        one sampling pass at MAIN; planes: the grid, sampling passes at LUMA and CHROMA
//     --grid-step <px>   the grid's step in output pixels (default 16)
//     --chroma-bilinear  planes: the chroma bilinear whatever --interp says
//     --grid-linear      the grid interpolated bilinearly (4 reads a pixel, not 16; use a step of 8)
//     --knots <n>        quaternion knots per frame along its rows (default 33); 0: every row (Gyroflow's own, large)
//     --interp <i>       bilinear, bspline, bicubic, bicubic9 (the same from 9 bilinear fetches), lanczos4, ewa
//                        (default bicubic)
//     --shrink           widened taps where the warp shrinks the source (bicubic, lanczos4)
//     --frames <a>-<b>   only these frames in the table (the rest show the background)
//     --readout-ms <t>   the rolling shutter's readout time instead of the project's (to measure the knots on a clip
//                        that has none)
//
//   gyroflow-gpu-table --check-grid <W>x<H> [--readout-ms <t>] <project.gyroflow | video>
//     No shader: the source coordinates on a coarse grid of a W x H output, interpolated (bilinear, Catmull-Rom),
//     against the exact ones at every pixel, for grid steps of 4 to 32 px, on the fastest-turning frames
//
//   gyroflow-gpu-table --check <n> [--readout-ms <t>] <project.gyroflow | video>
//     No shader: the n fastest-turning frames (and some through the clip) through Gyroflow's CPU kernel
//     (Stabilization::undistort_coord) with every row's matrix and with 9, 17, 33 and 65 knots: the largest difference
//     in source pixels over a grid of the output; and the time to compute a frame's transforms

use std::time::Instant;
use gyroflow_core::stabilization::Stabilization;
use nalgebra::Vector2;
use vs_gyroflow::gpu_table::{ self, Clip, Options, Table };

fn main() {
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    let usage = || -> ! {
        eprintln!("usage: gyroflow-gpu-table [--mode exact|grid|planes] [--grid-step px] [--knots n] [--interp bilinear|bspline|bicubic|bicubic9|lanczos4|ewa] [--shrink] [--frames a-b] [--readout-ms t] <project.gyroflow> <out.glsl>\n       gyroflow-gpu-table --check n [--readout-ms t] <project.gyroflow>");
        std::process::exit(2);
    };
    let mut value = |name: &str| -> Option<String> {
        let i = args.iter().position(|a| a == name)?;
        let v = (i + 1 < args.len()).then(|| args.remove(i + 1));
        args.remove(i);
        Some(v.unwrap_or_else(|| usage()))
    };
    let mode = match value("--mode").as_deref().unwrap_or("grid") {
        "exact" => gpu_table::Mode::Exact, "grid" => gpu_table::Mode::Grid, "planes" => gpu_table::Mode::Planes, _ => usage(),
    };
    let grid_step: usize = value("--grid-step").map(|v| v.parse().unwrap_or_else(|_| usage())).unwrap_or(16);
    let knots: usize = value("--knots").map(|v| v.parse().unwrap_or_else(|_| usage())).unwrap_or(33);
    let interp = match value("--interp").as_deref().unwrap_or("bicubic") {
        "bilinear" => 0, "bspline" => 1, "bicubic" => 2, "lanczos4" => 3, "ewa" => 4, "bicubic9" => 5, _ => usage(),
    };
    let readout: Option<f64> = value("--readout-ms").map(|v| v.parse().unwrap_or_else(|_| usage()));
    let check: Option<usize> = value("--check").map(|v| v.parse().unwrap_or_else(|_| usage()));
    let check_grid: Option<String> = value("--check-grid");
    let range: Option<(usize, usize)> = value("--frames").map(|v| {
        let (a, b) = v.split_once('-').unwrap_or_else(|| usage());
        (a.parse().unwrap_or_else(|_| usage()), b.parse().unwrap_or_else(|_| usage()))
    });
    let shrink = if let Some(i) = args.iter().position(|a| a == "--shrink") { args.remove(i); true } else { false };
    let chroma_bilinear = if let Some(i) = args.iter().position(|a| a == "--chroma-bilinear") { args.remove(i); true } else { false };
    let grid_linear = if let Some(i) = args.iter().position(|a| a == "--grid-linear") { args.remove(i); true } else { false };
    if args.is_empty() || (check.is_none() && check_grid.is_none() && args.len() != 2) { usage(); }
    if knots == 1 { eprintln!("--knots: 0, or at least 2"); std::process::exit(2); }

    let t = Instant::now();
    let clip = Clip::load(&args[0], readout).unwrap_or_else(|e| { eprintln!("{e}"); std::process::exit(1); });
    let p = &clip.params;
    eprintln!("Loaded {} in {:.0} ms: {}x{} -> {}x{}, {} frames at {:.3} fps, frame offset {} us, lens {}{}, readout {:.2} ms, correction {:.2}, projection {}",
              args[0], t.elapsed().as_secs_f64() * 1000.0, clip.size.0, clip.size.1, clip.output_size.0, clip.output_size.1, clip.frame_count, clip.fps,
              clip.frame_offset_us, p.distortion_model.id(), p.digital_lens.as_ref().map(|d| format!(" + digital {}", d.id())).unwrap_or_default(), p.frame_readout_time, p.lens_correction_amount, p.output_projection);
    if let Some(why) = gpu_table::unsupported(&clip) {
        eprintln!("Warning: the GPU warp doesn't render this clip as Gyroflow does ({why}); the player keeps VapourSynth for it");
    }

    if let Some(out) = check_grid {
        let (w, h) = out.split_once('x').and_then(|(w, h)| Some((w.parse().ok()?, h.parse().ok()?))).unwrap_or_else(|| usage());
        check_grid_error(&clip, (w, h));
        return;
    }
    if let Some(n) = check {
        check_knots(&clip, n);
        return;
    }

    let opts = Options { mode, grid_step, grid_linear, chroma_bilinear, knots, interp, shrink, range };
    let t = Instant::now();
    let table = Table::build(&clip, &opts).unwrap_or_else(|e| { eprintln!("{e}"); std::process::exit(1); });
    let built = t.elapsed().as_secs_f64() * 1000.0;
    let shader = gpu_table::shader(&clip, &opts, &table);
    std::fs::write(&args[1], &shader).unwrap_or_else(|e| { eprintln!("Can't write {}: {e}", args[1]); std::process::exit(1); });
    eprintln!("Wrote {} in {:.0} ms (the table in {built:.0}): table {}x{} rgba32f, {:.1} MB, {} {} a frame",
              args[1], t.elapsed().as_secs_f64() * 1000.0, table.width, table.height, shader.len() as f64 / 1e6, table.per_frame,
              if knots == 0 { "rows" } else { "knots" });
}

fn check_knots(clip: &Clip, n: usize) {
    let t = Instant::now();
    let mut spin: Vec<(f64, usize)> = Vec::with_capacity(clip.frame_count);
    for frame in 0..clip.frame_count {
        let tr = clip.transform(frame, clip.output_size);
        // How much the camera turns over the frame's rows: the first row's rotation against the last's
        let q = gpu_table::row_quats(&[tr.matrices[0], *tr.matrices.last().unwrap()]);
        let dot = (q[0][0] * q[1][0] + q[0][1] * q[1][1] + q[0][2] * q[1][2] + q[0][3] * q[1][3]).abs().min(1.0);
        spin.push((2.0 * (dot as f64).acos(), frame));
    }
    let rows = clip.rows();
    eprintln!("Transforms: {:.3} ms a frame ({rows} rows), the whole clip of {} frames in {:.1} s",
              t.elapsed().as_secs_f64() * 1000.0 / clip.frame_count as f64, clip.frame_count, t.elapsed().as_secs_f64());
    if rows <= 1 { eprintln!("No rolling shutter (readout 0): one matrix a frame, the knots don't matter. Try --readout-ms"); return; }
    let mut by_spin = spin.clone();
    by_spin.sort_by(|a, b| b.0.total_cmp(&a.0));
    let mut frames: Vec<usize> = by_spin.iter().take(n - n / 3).map(|s| s.1).collect();
    for i in 0..n / 3 { frames.push(i * clip.frame_count / (n / 3).max(1)); }

    let out = clip.output_size;
    let knot_set = [9usize, 17, 33, 65];
    let mut worst = [0f32; 4];
    println!("frame  turn_over_frame_deg  knots  max_px  p99_px  mean_px");
    for &frame in &frames {
        let tr = clip.transform(frame, out);
        let kp = &tr.kernel_params;
        let f = gyroflow_core::stabilization::projection::output_focal(kp);
        let (out_c, out_f) = (Vector2::new(out.0 as f32 / 2.0, out.1 as f32 / 2.0), Vector2::new(f.0, f.1));
        let coord = |p: Vector2<f32>, m: &[[f32; 16]]| Stabilization::undistort_coord(p, kp, m, &clip.params.distortion_model, clip.params.digital_lens.as_ref(), &[], &out_c, &out_f);
        let full: Vec<Option<Vector2<f32>>> = grid(out, 8).map(|p| coord(p, &tr.matrices)).collect();
        for (ki, &k) in knot_set.iter().enumerate() {
            let m = gpu_table::from_knots(&tr.matrices, k);
            let mut d: Vec<f32> = grid(out, 8).zip(&full).filter_map(|(p, a)| {
                let a = (*a)?;
                // Only where the ray lands in the frame
                if a.x < 0.0 || a.y < 0.0 || a.x >= clip.size.0 as f32 || a.y >= clip.size.1 as f32 { return None; }
                Some((a - coord(p, &m)?).norm())
            }).collect();
            if d.is_empty() { continue; }
            d.sort_by(|a, b| a.total_cmp(b));
            let (max, p99, mean) = (*d.last().unwrap(), d[d.len() * 99 / 100], d.iter().sum::<f32>() / d.len() as f32);
            worst[ki] = worst[ki].max(max);
            println!("{frame:5}  {:8.3}  {k:3}  {max:.4}  {p99:.4}  {mean:.4}", spin[frame].0.to_degrees());
        }
    }
    for (ki, k) in knot_set.iter().enumerate() { println!("worst with {k} knots: {:.4} px", worst[ki]); }
}

fn grid(size: (usize, usize), step: usize) -> impl Iterator<Item = Vector2<f32>> {
    (0..size.1).step_by(step).flat_map(move |y| (0..size.0).step_by(step).map(move |x| Vector2::new(x as f32, y as f32)))
}

/// The coarse geometry's error: grid points every `step` output pixels, from one before the edge to two past it
fn check_grid_error(clip: &Clip, out: (usize, usize)) {
    let mut spin: Vec<(f64, usize)> = (0..clip.frame_count).map(|frame| {
        let tr = clip.transform(frame, out);
        let q = gpu_table::row_quats(&[tr.matrices[0], *tr.matrices.last().unwrap()]);
        let a = gpu_table::row_quats(&[clip.transform(frame.saturating_sub(1), out).matrices[0], tr.matrices[0]]);
        let turn = |q: &Vec<[f32; 4]>| 2.0 * ((q[0][0] * q[1][0] + q[0][1] * q[1][1] + q[0][2] * q[1][2] + q[0][3] * q[1][3]).abs().min(1.0) as f64).acos();
        (turn(&q) + turn(&a), frame)
    }).collect();
    spin.sort_by(|a, b| b.0.total_cmp(&a.0));
    let frames: Vec<usize> = spin.iter().take(4).map(|s| s.1).chain([0, clip.frame_count / 2]).collect();
    println!("frame  step  bilinear_max  bilinear_p99  catmull_max  catmull_p99   (source px)");
    for frame in frames {
        let tr = clip.transform(frame, out);
        let kp = &tr.kernel_params;
        let f = gyroflow_core::stabilization::projection::output_focal(kp);
        let (out_c, out_f) = (Vector2::new(kp.output_width as f32 / 2.0, kp.output_height as f32 / 2.0), Vector2::new(f.0, f.1));
        let coord = |x: f32, y: f32| Stabilization::undistort_coord(Vector2::new(x, y), kp, &tr.matrices, &clip.params.distortion_model, clip.params.digital_lens.as_ref(), &[], &out_c, &out_f);
        // Exact, every pixel, in parallel
        let exact: Vec<Option<Vector2<f32>>> = std::thread::scope(|s| {
            let chunks: Vec<_> = (0..8).map(|t| { let coord = &coord; s.spawn(move || {
                (0..out.1).filter(|y| y % 8 == t).flat_map(|y| (0..out.0).map(move |x| (y, coord(x as f32, y as f32)))).collect::<Vec<_>>()
            }) }).collect();
            let mut all = vec![None; out.0 * out.1];
            for c in chunks { for (i, (y, v)) in c.join().unwrap().into_iter().enumerate() { let x = i % out.0; all[y * out.0 + x] = v; } }
            all
        });
        for step in [4usize, 8, 16, 32] {
            let (gw, gh) = (out.0.div_ceil(step) + 3, out.1.div_ceil(step) + 3);
            let grid: Vec<Vector2<f32>> = (0..gh).flat_map(|j| (0..gw).map(move |i| (i, j))).map(|(i, j)| {
                coord((i as f32 - 1.0) * step as f32, (j as f32 - 1.0) * step as f32).unwrap_or(Vector2::new(f32::NAN, f32::NAN))
            }).collect();
            let g = |i: isize, j: isize| grid[(j.clamp(0, gh as isize - 1) as usize) * gw + i.clamp(0, gw as isize - 1) as usize];
            let cr = |t: f32| -> [f32; 4] { let t2 = t * t; let t3 = t2 * t;
                [-0.5 * t3 + t2 - 0.5 * t, 1.5 * t3 - 2.5 * t2 + 1.0, -1.5 * t3 + 2.0 * t2 + 0.5 * t, 0.5 * t3 - 0.5 * t2] };
            let (mut eb, mut ec) = (Vec::new(), Vec::new());
            for y in 0..out.1 {
                for x in 0..out.0 {
                    let Some(e) = exact[y * out.0 + x] else { continue };
                    if e.x < 0.0 || e.y < 0.0 || e.x >= clip.size.0 as f32 || e.y >= clip.size.1 as f32 { continue; }
                    let (gx, gy) = (x as f32 / step as f32 + 1.0, y as f32 / step as f32 + 1.0);
                    let (i, j) = (gx.floor() as isize, gy.floor() as isize);
                    let (fx, fy) = (gx - i as f32, gy - j as f32);
                    let b = g(i, j) * (1.0 - fx) * (1.0 - fy) + g(i + 1, j) * fx * (1.0 - fy) + g(i, j + 1) * (1.0 - fx) * fy + g(i + 1, j + 1) * fx * fy;
                    let (wx, wy) = (cr(fx), cr(fy));
                    let mut c = Vector2::zeros();
                    for dj in 0..4 { for di in 0..4 { c += g(i - 1 + di as isize, j - 1 + dj as isize) * (wx[di] * wy[dj]); } }
                    if b.x.is_finite() { eb.push((b - e).norm()); }
                    if c.x.is_finite() { ec.push((c - e).norm()); }
                }
            }
            let stat = |v: &mut Vec<f32>| { v.sort_by(|a, b| a.total_cmp(b)); (v.last().copied().unwrap_or(0.0), v.get(v.len() * 99 / 100).copied().unwrap_or(0.0)) };
            let ((bm, b99), (cm, c99)) = (stat(&mut eb), stat(&mut ec));
            println!("{frame:5}  {step:4}  {bm:12.4}  {b99:12.4}  {cm:11.4}  {c99:11.4}");
        }
    }
}
