// SPDX-License-Identifier: GPL-3.0-or-later
//
// gyroflow-export's GPU path against its pipes (docs/design/export-gpu-path.md), on a real clip: AFP_GPU_CLIP (a stabilized
// clip, e.g. ~/Movies/afp-perf-clips/DJI_20261022152937_0069_D.MP4) and AFP_GPU_FFMPEG (an ffmpeg the GPU path runs,
// tools/export-gpu/build_ffmpeg_libplacebo.sh's on a Mac), or AFP_GPU_EXPORT (a gyroflow-export to run instead of this
// build's, e.g. the one inside a built ActionFootagePlayer.app, which finds the app's own ffmpeg). Without them it's skipped. Two seconds rendered both ways:
// the same number of frames, each the other's own (not its neighbour's), and the pictures within the warp's own
// difference (the player's 8 px grid and interpolation against Gyroflow's per-pixel engine: Y about 38-39 dB).
// AFP_GPU_CLIP may name several clips, separated by ';' (the GoPro ones with their projects at lens correction 0 %).
//
// AFP_GPU_SDR_CLIPS (clips separated by ';'): an SDR clip turned without the warp (--no-stabilize --rotation 180) goes
// through the GPU path and keeps its colors: its tags (matrix, range, primaries, transfer) and its pixels, against the
// source turned by ffmpeg, differ only by the encoder

use std::path::{ Path, PathBuf };
use std::process::Command;

fn export(clip: &str, out: &Path, gpu: &str) -> std::path::PathBuf { export_with(clip, out, gpu, &["--chromatic", "no", "--from", "0.5", "--to", "2.5"]) }

fn export_with(clip: &str, out: &Path, gpu: &str, args: &[&str]) -> std::path::PathBuf {
    let t = std::time::Instant::now();
    let exe = std::env::var_os("AFP_GPU_EXPORT").unwrap_or_else(|| env!("CARGO_BIN_EXE_gyroflow-export").into());
    let status = Command::new(exe)
        .args(["--gpu", gpu]).args(args).args(["--force", "--out"]).arg(out).arg(clip)
        .status().unwrap();
    assert!(status.success(), "gyroflow-export --gpu {gpu} failed");
    eprintln!("{clip}: --gpu {gpu} {args:?}: {:.1} s", t.elapsed().as_secs_f64());
    std::fs::read_dir(out).unwrap().map(|e| e.unwrap().path()).find(|p| p.extension().is_some_and(|x| x == "mov")).unwrap()
}

/// Every frame's luma at 480x270, 8 bits
fn frames(video: &Path) -> Vec<Vec<u8>> {
    let out = Command::new("ffmpeg").args(["-v", "error", "-i"]).arg(video)
        .args(["-vf", "scale=480:270", "-f", "rawvideo", "-pix_fmt", "gray", "-"]).output().unwrap();
    out.stdout.chunks(480 * 270).map(|c| c.to_vec()).collect()
}

fn mse(a: &[u8], b: &[u8]) -> f64 {
    a.iter().zip(b).map(|(x, y)| (*x as f64 - *y as f64).powi(2)).sum::<f64>() / a.len() as f64
}

#[test]
fn the_gpu_path_renders_what_the_pipes_do() {
    let Ok(clips) = std::env::var("AFP_GPU_CLIP") else { eprintln!("AFP_GPU_CLIP not set: skipped"); return; };
    if std::env::var_os("AFP_GPU_FFMPEG").is_none() && std::env::var_os("AFP_GPU_EXPORT").is_none() {
        eprintln!("neither AFP_GPU_FFMPEG nor AFP_GPU_EXPORT set: skipped");
        return;
    };
    for clip in clips.split(';').filter(|c| !c.is_empty()) {
    let dir = std::env::temp_dir().join(format!("afp-export-gpu-{}", std::process::id()));
    let (g, p) = (dir.join("gpu"), dir.join("pipes"));
    let (a, b) = (frames(&export(&clip, &g, "on")), frames(&export(&clip, &p, "off")));
    let _ = std::fs::remove_dir_all(&dir);
    // The GPU path writes the frames --from/--to ask for (frames:v); the pipes write one more past --to on a 29.97 fps
    // clip (GX010845: 61 against 60, 2.035 s against 2.002 s), their own off-by-one: the frames both have are compared
    if a.len() != b.len() { eprintln!("{clip}: {} frames on the GPU path, {} through the pipes", a.len(), b.len()); }
    assert!(b.len() >= a.len() && b.len() <= a.len() + 1, "frame counts differ: {} and {}", a.len(), b.len());
    assert!(a.len() >= 50, "{} frames", a.len());
    for i in [0, a.len() / 2, a.len() - 1] {
        let own = mse(&a[i], &b[i]);
        // Its own frame, by far: a neighbour's is the clip's motion away
        for j in [i.saturating_sub(1), (i + 1).min(a.len() - 1)].into_iter().filter(|j| *j != i) {
            assert!(own * 4.0 < mse(&a[i], &b[j]), "frame {i}: {own:.1} against frame {j}'s {:.1}", mse(&a[i], &b[j]));
        }
        // 8-bit luma at a quarter size: within the warp's difference (PSNR above 33 dB)
        let psnr = 10.0 * (255.0f64.powi(2) / own.max(1e-9)).log10();
        eprintln!("{clip}: frame {i}: GPU path against the pipes, Y {psnr:.1} dB");
        assert!(psnr > 33.0, "frame {i}: {psnr:.1} dB");
    }
    }
}

fn tags(video: &Path) -> String {
    let out = Command::new("ffprobe").args(["-v", "error", "-select_streams", "v:0", "-show_entries",
        "stream=color_range,color_space,color_primaries,color_transfer", "-of", "csv=p=0"]).arg(video).output().unwrap();
    String::from_utf8_lossy(&out.stdout).lines().next().unwrap_or("").trim().to_string()
}

/// The source turned as the export turns it, encoded by the same encoder at the export's bit rate: what the encoder
/// alone does to it
fn encoded_alone(clip: &str, out: &Path, bit_rate: &str) -> PathBuf {
    let f = out.join("reference.mov");
    let status = Command::new("ffmpeg").args(["-v", "error", "-y", "-ss", "0.5", "-noautorotate", "-i", clip, "-frames:v", "10", "-vf", "hflip,vflip",
        "-c:v", "hevc_videotoolbox", "-realtime", "1", "-prio_speed", "1", "-pix_fmt", "p010le", "-b:v", bit_rate, "-tag:v", "hvc1"]).arg(&f).status().unwrap();
    assert!(status.success(), "{clip}: the reference encode failed");
    f
}

fn bit_rate(video: &Path) -> String {
    let o = Command::new("ffprobe").args(["-v", "error", "-select_streams", "v:0", "-show_entries", "stream=bit_rate", "-of", "csv=p=0"]).arg(video).output().unwrap();
    String::from_utf8_lossy(&o.stdout).lines().next().unwrap_or("").trim().to_string()
}

/// Frames `n` from `ss` s, each plane at 10 bits (yuv420p10le), `vf` applied
fn planes(video: &Path, ss: f64, n: usize, vf: &str) -> (Vec<Vec<u16>>, (usize, usize)) {
    let probe = Command::new("ffprobe").args(["-v", "error", "-select_streams", "v:0", "-show_entries", "stream=width,height", "-of", "csv=p=0"])
        .arg(video).output().unwrap();
    let wh: Vec<usize> = String::from_utf8_lossy(&probe.stdout).lines().next().unwrap_or("").trim().trim_end_matches(',').split(',').map(|v| v.parse().unwrap()).collect();
    let out = Command::new("ffmpeg").args(["-v", "error", "-ss", &format!("{ss}"), "-noautorotate", "-i"]).arg(video)
        .args(["-frames:v", &n.to_string(), "-vf", vf, "-f", "rawvideo", "-pix_fmt", "yuv420p10le", "-"]).output().unwrap();
    let size = wh[0] * wh[1] * 3 / 2;
    (out.stdout.chunks(size * 2).map(|c| c.chunks(2).map(|b| u16::from_le_bytes([b[0], b[1]])).collect()).collect(), (wh[0], wh[1]))
}

fn range_of(plane: &str, w: usize, h: usize) -> std::ops::Range<usize> {
    match plane { "Y" => 0..w * h, "Cb" => w * h..w * h * 5 / 4, _ => w * h * 5 / 4..w * h * 3 / 2 }
}

#[test]
fn an_sdr_clip_keeps_its_colors() {
    let (Ok(clips), Ok(_)) = (std::env::var("AFP_GPU_SDR_CLIPS"), std::env::var("AFP_GPU_FFMPEG")) else {
        eprintln!("AFP_GPU_SDR_CLIPS / AFP_GPU_FFMPEG not set: skipped");
        return;
    };
    for clip in clips.split(';').filter(|c| !c.is_empty()) {
        let dir = std::env::temp_dir().join(format!("afp-export-sdr-{}", std::process::id()));
        let out = export_with(clip, &dir, "on", &["--no-stabilize", "--rotation", "180", "--from", "0.5", "--to", "1.5"]);
        let (src_tags, out_tags) = (tags(Path::new(clip)), tags(&out));
        eprintln!("{clip}: tags {src_tags} -> {out_tags}");
        // The source's range, matrix, primaries and transfer (an unset one written as BT.709)
        let norm = |t: &str| t.split(',').map(|v| if v == "unknown" || v.is_empty() { "bt709".to_string() } else { v.to_string() }).collect::<Vec<_>>();
        assert_eq!(norm(&out_tags), norm(&src_tags), "{clip}: the tags changed");
        // The export starts at the frame at 0.5 s: the source's frames from there, turned as the export turns them
        let (a, (w, h)) = planes(&out, 0.0, 10, "null");
        let (b, _) = planes(Path::new(clip), 0.5, 10, "hflip,vflip");
        let (r, _) = planes(&encoded_alone(clip, &dir, &bit_rate(&out)), 0.0, 10, "null");
        let _ = std::fs::remove_dir_all(&dir);
        assert!(a.len() == 10 && b.len() == 10 && r.len() == 10, "{}, {} and {} frames", a.len(), b.len(), r.len());
        let psnr_of = |x: &[u16], y: &[u16]| {
            let mse = x.iter().zip(y).map(|(p, q)| (*p as f64 - *q as f64).powi(2)).sum::<f64>() / x.len() as f64;
            10.0 * (1023.0f64.powi(2) / mse.max(1e-9)).log10()
        };
        for (i, ((x, y), z)) in a.iter().zip(&b).zip(&r).enumerate() {
            for (name, range) in [("Y", 0..w * h), ("Cb", w * h..w * h * 5 / 4), ("Cr", w * h * 5 / 4..w * h * 3 / 2)] {
                let (xs, ys) = (&x[range.clone()], &y[range]);
                let n = xs.len() as f64;
                // A range squeeze, another matrix or a tone curve moves the mean by codes; the encoder doesn't
                let bias = xs.iter().zip(ys).map(|(p, q)| *p as f64 - *q as f64).sum::<f64>() / n;
                let psnr = psnr_of(xs, ys);
                // The same encoder on the source alone, at the same bit rate: the export may lose at most a little more
                let alone = psnr_of(&z[range_of(name, w, h)], ys);
                eprintln!("{clip}: frame {i} {name}: mean difference {bias:+.2} of 1023, {psnr:.1} dB (the encoder alone {alone:.1} dB)");
                assert!(bias.abs() < 1.0, "{clip}: frame {i} {name}: mean moved {bias:.2} codes");
                assert!(psnr > alone - 2.0, "{clip}: frame {i} {name}: {psnr:.1} dB against the encoder's own {alone:.1} dB");
            }
        }
    }
}
