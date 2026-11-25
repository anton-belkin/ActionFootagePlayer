// SPDX-License-Identifier: GPL-3.0-or-later
//
// The GPU path's table extended ahead of where a clip plays (docs/design/gpu-table-ahead.md), on a real clip without a project:
// AFP_TEST_CLIP (e.g. an Osmo Action 6 or GoPro clip with its own motion data); skipped without it. The first stretch
// (0-20 s, as the player opens it), extended to 50 s with it as the table before: the frames the first one had settled
// are the same, bit for bit (nothing on screen changes as the new table comes in), and the extended table against the
// whole clip's: within a pixel up to its own settled end (gpu_table::SETTLE_S, the smoothing's look-ahead)

use std::sync::Arc;
use vs_gyroflow::gpu_table::{ Options, PlayerClip, PARAM_TEXELS, SETTLE_S };

/// The largest difference in the frames' rotations (their knots' quaternions) between two tables, in pixels at the
/// lens's focal length (an angle's ~ 2 × the quaternion's change), over frames `range`
fn px_apart(a: &vs_gyroflow::gpu_table::Table, b: &vs_gyroflow::gpu_table::Table, range: std::ops::Range<usize>) -> f64 {
    range.map(|f| {
        let (x, y) = (a.frame_block(f), b.frame_block(f));
        let focal = x[0] as f64;
        x[PARAM_TEXELS * 4..].chunks(4).zip(y[PARAM_TEXELS * 4..].chunks(4))
            .map(|(p, q)| {
                // q and -q are the same turn
                let d: f64 = p.iter().zip(q).map(|(u, v)| (u - v) as f64).map(|d| d * d).sum::<f64>().sqrt();
                let e: f64 = p.iter().zip(q).map(|(u, v)| (u + v) as f64).map(|d| d * d).sum::<f64>().sqrt();
                2.0 * d.min(e) * focal
            }).fold(0.0, f64::max)
    }).fold(0.0, f64::max)
}

#[test]
fn extended_table_keeps_what_was_on_screen() {
    let Some(clip) = std::env::var_os("AFP_TEST_CLIP") else { eprintln!("AFP_TEST_CLIP not set: skipped"); return };
    let clip = clip.to_string_lossy().to_string();
    let j = serde_json::json!({ "project": clip, "level": "medium", "input_width": 3840, "input_height": 2160, "edges": "smooth", "start": 0.0 });
    let opts = Options::player(5, false, Some(0.02), false, false, 0.0);
    let first = Arc::new(PlayerClip::load(&j).unwrap());
    assert_eq!(first.covers, Some((0.0, 20.0)));
    let view = first.view(&j);
    let t1 = first.table(view, &first.clip(&j), &opts).unwrap();
    let settled1 = first.settled();
    assert!((settled1 - (20.0 - SETTLE_S)).abs() < 1e-9);
    let fps = first.fps;
    let mut j2 = j.clone();
    j2["until"] = 50.0.into();
    let second = PlayerClip::load_range(&j2, Some((0.0, 50.0)), Some(first.clone())).unwrap();
    let t2 = second.table(view, &second.clip(&j2), &opts).unwrap();
    // What the first had settled: the same, bit for bit
    let kept = (settled1 * fps).ceil() as usize - 1;
    for f in 0..kept { assert_eq!(t1.frame_block(f), t2.frame_block(f), "frame {f}"); }
    // Past it, the first one's unsettled frames against the second's: how much they'd have moved without the copy
    let moved = px_apart(&t1, &t2, kept..(20.0 * fps) as usize);
    // The second against the whole clip's, up to its own settled end
    let whole = PlayerClip::load_range(&j, None, None).unwrap();
    let tw = whole.table(view, &whole.clip(&j), &opts).unwrap();
    let apart = px_apart(&t2, &tw, (settled1 * fps) as usize..(second.settled() * fps) as usize);
    let apart_before = px_apart(&t2, &tw, 0..kept);
    eprintln!("first stretch's unsettled frames moved {moved:.2} px in the extended table; extended vs whole clip: {apart_before:.2} px over the copied frames, {apart:.2} px from there to its settled end");
    assert!(apart < 1.0 && apart_before < 1.0, "the extended table is {apart:.2} / {apart_before:.2} px from the whole clip's");
}
