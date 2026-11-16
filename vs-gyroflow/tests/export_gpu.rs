// SPDX-License-Identifier: GPL-3.0-or-later
//
// gyroflow-export's GPU path against its pipes (docs/export-gpu-path.md), on a real clip: AFP_GPU_CLIP (a stabilized
// clip, e.g. ~/Movies/afp-perf-clips/DJI_20261022152937_0069_D.MP4) and AFP_GPU_FFMPEG (an ffmpeg the GPU path runs,
// tools/export-gpu/build_ffmpeg_libplacebo.sh's on a Mac). Without them it's skipped. Two seconds rendered both ways:
// the same number of frames, each the other's own (not its neighbour's), and the pictures within the warp's own
// difference (the player's 8 px grid and interpolation against Gyroflow's per-pixel engine: Y about 38-39 dB)

use std::path::Path;
use std::process::Command;

fn export(clip: &str, out: &Path, gpu: &str) -> std::path::PathBuf {
    let status = Command::new(env!("CARGO_BIN_EXE_gyroflow-export"))
        .args(["--gpu", gpu, "--chromatic", "no", "--from", "3", "--to", "5", "--force", "--out"]).arg(out).arg(clip)
        .status().unwrap();
    assert!(status.success(), "gyroflow-export --gpu {gpu} failed");
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
    let (Ok(clip), Ok(_)) = (std::env::var("AFP_GPU_CLIP"), std::env::var("AFP_GPU_FFMPEG")) else {
        eprintln!("AFP_GPU_CLIP / AFP_GPU_FFMPEG not set: skipped");
        return;
    };
    let dir = std::env::temp_dir().join(format!("afp-export-gpu-{}", std::process::id()));
    let (g, p) = (dir.join("gpu"), dir.join("pipes"));
    let (a, b) = (frames(&export(&clip, &g, "on")), frames(&export(&clip, &p, "off")));
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(a.len(), b.len(), "frame counts differ");
    assert!(a.len() >= 100, "{} frames", a.len());
    for i in [0, a.len() / 2, a.len() - 1] {
        let own = mse(&a[i], &b[i]);
        // Its own frame, by far: a neighbour's is the clip's motion away
        for j in [i.saturating_sub(1), (i + 1).min(a.len() - 1)].into_iter().filter(|j| *j != i) {
            assert!(own * 4.0 < mse(&a[i], &b[j]), "frame {i}: {own:.1} against frame {j}'s {:.1}", mse(&a[i], &b[j]));
        }
        // 8-bit luma at a quarter size: within the warp's difference (PSNR above 33 dB)
        let psnr = 10.0 * (255.0f64.powi(2) / own.max(1e-9)).log10();
        assert!(psnr > 33.0, "frame {i}: {psnr:.1} dB");
    }
}
