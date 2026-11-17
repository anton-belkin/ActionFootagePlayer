// SPDX-License-Identifier: GPL-3.0-or-later
//
// A photo made from a clip carries the moment it shows, not when it was made: the clip's start (the camera's clock, as
// the Library dates the clip: geotag::read) plus the frame's time. On real clips, AFP_PHOTO_CLIPS: entries separated by
// ';', each "<clip>|<its start, local time>|<zone or ->" (e.g. "DJI_…MP4|2026-10-22T15:29:38|+02:00"; "-": the camera
// gives no zone, none is written). Each clip is rendered at t = 2.5 s as a HEIC, a Live Photo (HEIC + MOV) and a Motion
// Photo (JPEG), and read back with exiftool: Exif DateTimeOriginal / SubSecTimeOriginal / OffsetTimeOriginal, the MOV's
// and the Motion Photo's QuickTime creation date, and the files' modified dates. Skipped without it (and exiftool)

use std::path::{ Path, PathBuf };
use std::process::Command;

fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + d - 1;
    era * 146_097 + yoe * 365 + yoe / 4 - yoe / 100 + doy - 719_468
}

/// "YYYY-MM-DDTHH:MM:SS[.fff]" or "YYYY:MM:DD HH:MM:SS" -> seconds counted from 1970 in that time's own clock
fn secs(s: &str) -> f64 {
    let d: Vec<&str> = s.split(|c: char| !c.is_ascii_digit()).filter(|p| !p.is_empty()).take(7).collect();
    let n = |i: usize| d[i].parse::<i64>().unwrap();
    let frac = if d.len() > 6 && s.contains('.') { format!("0.{}", d[6]).parse::<f64>().unwrap() } else { 0.0 };
    (days_from_civil(n(0), n(1), n(2)) * 86400 + n(3) * 3600 + n(4) * 60 + n(5)) as f64 + frac
}

fn zone_min(z: &str) -> i64 {
    let sign = if z.starts_with('-') { -1 } else { 1 };
    let d: Vec<i64> = z[1..].split(':').map(|p| p.parse().unwrap()).collect();
    sign * (d[0] * 60 + d[1])
}

fn photo(clip: &str, out: &Path, args: &[&str]) -> Vec<PathBuf> {
    let _ = std::fs::remove_dir_all(out);
    let t = std::time::Instant::now();
    let status = Command::new(env!("CARGO_BIN_EXE_gyroflow-export")).args(["--photo", "2.5", "--force"]).args(args).arg("--out").arg(out).arg(clip)
        .status().unwrap();
    assert!(status.success(), "{clip}: --photo {args:?} failed");
    eprintln!("{clip}: --photo 2.5 {args:?}: {:.1} s", t.elapsed().as_secs_f64());
    let mut files: Vec<PathBuf> = std::fs::read_dir(out).unwrap().map(|e| e.unwrap().path()).filter(|p| !p.file_name().unwrap().to_string_lossy().starts_with('.')).collect();
    files.sort();
    files
}

fn exif(file: &Path) -> serde_json::Value {
    let out = Command::new("exiftool").args(["-j", "-G1", "-DateTimeOriginal", "-SubSecTimeOriginal", "-OffsetTimeOriginal", "-Keys:CreationDate",
        "-QuickTime:CreationDate"]).arg(file).output().unwrap();
    serde_json::from_slice::<serde_json::Value>(&out.stdout).unwrap()[0].clone()
}

fn modified(file: &Path) -> f64 {
    std::fs::metadata(file).unwrap().modified().unwrap().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs_f64()
}

#[test]
fn a_photo_carries_the_moment_it_shows() {
    let Ok(list) = std::env::var("AFP_PHOTO_CLIPS") else { eprintln!("AFP_PHOTO_CLIPS not set: skipped"); return };
    if Command::new("exiftool").arg("-ver").output().is_err() { eprintln!("no exiftool: skipped"); return; }
    let dir = std::env::temp_dir().join(format!("afp-photo-dates-{}", std::process::id()));
    for entry in list.split(';').filter(|e| !e.is_empty()) {
        let parts: Vec<&str> = entry.split('|').collect();
        let (clip, start, zone) = (parts[0], parts[1], parts[2]);
        let fps = {
            let o = Command::new("ffprobe").args(["-v", "error", "-select_streams", "v:0", "-show_entries", "stream=r_frame_rate", "-of", "csv=p=0", clip]).output().unwrap();
            let r = String::from_utf8_lossy(&o.stdout).lines().next().unwrap_or("").trim().trim_end_matches(',').to_string();
            let (a, b) = r.split_once('/').unwrap();
            a.parse::<f64>().unwrap() / b.parse::<f64>().unwrap()
        };
        // The frame shown: the one nearest 2.5 s
        let t = (2.5 * fps).round() / fps;
        let want_local = secs(start) + t;
        let utc = |local: f64| local - if zone == "-" { 0.0 } else { zone_min(zone) as f64 * 60.0 };
        for (label, args) in [("HEIC", vec!["--format", "heic"]), ("Live Photo", vec!["--format", "heic", "--live"]), ("Motion Photo", vec!["--format", "jpeg", "--live"])] {
            let files = photo(clip, &dir, &args);
            for f in &files {
                let x = exif(f);
                let ext = f.extension().unwrap().to_string_lossy().to_lowercase();
                eprintln!("{clip} {label} {}: {x}", f.file_name().unwrap().to_string_lossy());
                if ext == "heic" || ext == "jpg" {
                    let local = x["ExifIFD:DateTimeOriginal"].as_str().unwrap_or_else(|| panic!("{label}: no DateTimeOriginal"));
                    let sub: f64 = format!("0.{}", x["ExifIFD:SubSecTimeOriginal"].as_str().map(str::to_string).unwrap_or_else(|| x["ExifIFD:SubSecTimeOriginal"].to_string())).parse().unwrap();
                    let got = secs(local) + sub;
                    assert!((got - want_local).abs() < 0.002, "{label} {}: {local}.{sub} against {want_local}", f.display());
                    match zone {
                        "-" => assert!(x["ExifIFD:OffsetTimeOriginal"].is_null(), "{label}: a zone written that the camera doesn't give"),
                        z => assert_eq!(x["ExifIFD:OffsetTimeOriginal"].as_str(), Some(z), "{label}: OffsetTimeOriginal"),
                    }
                }
                if ext == "mov" || ext == "jpg" && label == "Motion Photo" {
                    // The video's QuickTime creation date: the moment, to the second (its own format has no fraction)
                    let c = x["Keys:CreationDate"].as_str().or(x["QuickTime:CreationDate"].as_str());
                    if ext == "mov" {
                        let c = c.unwrap_or_else(|| panic!("{label}: the MOV has no creation date"));
                        assert!((secs(c) - want_local.floor()).abs() < 1.0, "{label}: the MOV's creation date {c}, want {want_local}");
                    }
                }
                // The file's own date: the moment (to the second)
                assert!((modified(f) - utc(want_local)).abs() < 1.5, "{label} {}: modified {} against {}", f.display(), modified(f), utc(want_local));
            }
        }
    }
    let _ = std::fs::remove_dir_all(&dir);
}
