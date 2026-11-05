// SPDX-License-Identifier: GPL-3.0-or-later
//
// afp-cut: keeps a part of a camera's MP4 without re-encoding it, checked to read as the original does
// (vs-gyroflow/src/cut.rs, docs/lossless-cut.md). For the player's cutting of a clip spoiled in part.

use std::path::{ Path, PathBuf };
use std::process::ExitCode;

use vs_gyroflow::cut;

const USAGE: &str = "usage: afp-cut <video> --from <time> [--to <time>] [--out <file>] [--lrf] [--json]
       afp-cut --set-key <mp4> <key> <value>

Keeps the video's part from --from (back to the keyframe at or before it) to --to (the frame at or after it is the
first not kept; the end when left out), without re-encoding: every sample of every track as it is, the camera's clip
data put back where the cut took it (DJI), the timecode and creation times moved on, and the key
ch.actionfootageplayer.cut saying where it comes from. The cut is then read back as the player reads the original (the
samples, the stabilization's verdict, log curve, lens, the motion data); one that doesn't read the same is removed.

  --out <file>  where to write it (default: <video>_<from>-<to>s.<ext> next to it); never the video itself
  --lrf         the camera's low-resolution proxy next to it (DJI's .LRF) cut the same, next to the output
  --json        the outcome as JSON on stdout
  times         seconds, or m:ss

--set-key writes a text key into an MP4's moov/meta, its other keys kept (the player relinks moments with it).

Exit status: 0 cut and checked, 1 an error, 2 not cut (a file it can't cut), 3 cut but not the same: removed.
A camera without an adapter here (only DJI has one) is cut only when the check passes; it's said in the outcome.";

fn time(s: &str) -> Option<f64> {
    match s.split_once(':') {
        Some((m, sec)) => Some(m.parse::<f64>().ok()? * 60.0 + sec.parse::<f64>().ok()?),
        None => s.parse().ok(),
    }
}

fn default_out(video: &Path, from: f64, to: Option<f64>) -> PathBuf {
    let stem = video.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
    let ext = video.extension().map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| "mp4".into());
    let t = |x: f64| format!("{}", (x * 10.0).round() / 10.0);
    video.with_file_name(format!("{stem}_{}-{}s.{ext}", t(from), to.map(t).unwrap_or_else(|| "end".into())))
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.first().map(|s| s.as_str()) == Some("--set-key") {
        let [_, file, key, value] = args.as_slice() else { eprintln!("{USAGE}"); return ExitCode::from(1) };
        let path = Path::new(file);
        let mut keys = match vs_gyroflow::mp4meta::read_keys(path) { Ok(k) => k, Err(e) => { eprintln!("afp-cut: {file}: {e}"); return ExitCode::from(1); } };
        keys.retain(|(k, _)| k != key);
        keys.push((key.clone(), value.clone()));
        let refs: Vec<(&str, &str)> = keys.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();
        return match vs_gyroflow::mp4meta::write_keys(path, &refs) {
            Ok(()) => ExitCode::SUCCESS,
            Err(e) => { eprintln!("afp-cut: {file}: {e}"); ExitCode::from(1) }
        };
    }
    let (mut video, mut from, mut to, mut out, mut lrf, mut json) = (None, None, None, None, false, false);
    let mut i = 0;
    while i < args.len() {
        let next = |i: &mut usize| { *i += 1; args.get(*i).cloned() };
        match args[i].as_str() {
            "--from" => from = next(&mut i).and_then(|s| time(&s)),
            "--to" => to = next(&mut i).and_then(|s| time(&s)),
            "--out" => out = next(&mut i).map(PathBuf::from),
            "--lrf" => lrf = true,
            "--json" => json = true,
            "-h" | "--help" => { println!("{USAGE}"); return ExitCode::SUCCESS; }
            a if !a.starts_with("--") && video.is_none() => video = Some(PathBuf::from(a)),
            a => { eprintln!("afp-cut: unknown argument {a}\n\n{USAGE}"); return ExitCode::from(1); }
        }
        i += 1;
    }
    let (Some(video), Some(from)) = (video, from) else { eprintln!("{USAGE}"); return ExitCode::from(1) };
    // Absolute: Gyroflow's readers take a path as a URL
    let video = match video.canonicalize() { Ok(v) => v, Err(e) => { eprintln!("afp-cut: {}: {e}", video.display()); return ExitCode::from(1); } };
    let out = out.map(|o| if o.is_absolute() { o } else { std::env::current_dir().map(|d| d.join(&o)).unwrap_or(o) });
    let out = out.unwrap_or_else(|| default_out(&video, from, to));
    if out.canonicalize().ok().is_some_and(|o| video.canonicalize().ok() == Some(o)) {
        eprintln!("afp-cut: the output can't be the video itself");
        return ExitCode::from(1);
    }

    let mut results = Vec::new();
    let mut status = 0u8;
    let mut jobs = vec![(video.clone(), out.clone())];
    if lrf {
        let proxy = video.with_extension("LRF");
        if proxy.exists() { jobs.push((proxy, out.with_extension("LRF"))); }
    }
    // The proxy is cut to the main cut's moments
    let (mut from, mut to) = (from, to);
    for (n, (src, dst)) in jobs.into_iter().enumerate() {
        let r = cut_one(&src, &dst, from, to, n == 0);
        if n == 0 {
            if let Some(p) = r.get("plan") { from = p["start"].as_f64().unwrap_or(from); to = p["end"].as_f64().filter(|_| to.is_some()).or(to); }
        }
        let s = r["status"].as_u64().unwrap_or(1) as u8;
        if !json { report(&r); }
        status = status.max(s);
        results.push(r);
        if s != 0 { break; }
    }
    if json { println!("{}", serde_json::to_string_pretty(&serde_json::json!({ "results": results })).unwrap()); }
    ExitCode::from(status)
}

fn cut_one(src: &Path, dst: &Path, from: f64, to: Option<f64>, as_played: bool) -> serde_json::Value {
    let fail = |status: u8, e: String| serde_json::json!({ "video": src, "status": status, "error": e });
    let original = cut::reading(src);
    // Hidden while it's made and checked; its extension kept, which telemetry-parser goes by
    let tmp = dst.with_file_name(format!(".{}.part.{}", dst.file_stem().map(|n| n.to_string_lossy().to_string()).unwrap_or_default(),
        dst.extension().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| "mp4".into())));
    let key = serde_json::json!({ "read_as": original, "tool": format!("afp-cut {}", env!("CARGO_PKG_VERSION")) });
    let plan = match cut::cut(src, &tmp, from, to, key) {
        Ok(p) => p,
        Err(e) => { let _ = std::fs::remove_file(&tmp); return fail(2, e); }
    };
    let verified = match cut::verify(src, &tmp, &plan, original, as_played) {
        Ok(v) => v,
        Err(e) => { let _ = std::fs::remove_file(&tmp); return fail(1, e); }
    };
    if !verified.ok {
        let _ = std::fs::remove_file(&tmp);
        return serde_json::json!({ "video": src, "status": 3, "plan": plan, "verified": verified });
    }
    if let Err(e) = std::fs::rename(&tmp, dst) { let _ = std::fs::remove_file(&tmp); return fail(1, e.to_string()); }
    // The file's dates: the original's, moved on by the time cut off (Finder sorts by them)
    if let Ok(m) = std::fs::metadata(src) {
        let shift = std::time::Duration::from_secs_f64(plan.start);
        let times = std::fs::FileTimes::new();
        let times = match m.modified() { Ok(t) => times.set_modified(t + shift), _ => times };
        #[cfg(target_os = "macos")]
        let times = { use std::os::macos::fs::FileTimesExt; match m.created() { Ok(t) => times.set_created(t + shift), _ => times } };
        #[cfg(windows)]
        let times = { use std::os::windows::fs::FileTimesExt; match m.created() { Ok(t) => times.set_created(t + shift), _ => times } };
        if let Ok(f) = std::fs::File::options().write(true).open(dst) { let _ = f.set_times(times); }
    }
    copy_xattrs(src, dst);
    serde_json::json!({ "video": src, "out": dst, "status": 0, "known_camera": plan.adapter != "unknown", "plan": plan, "verified": verified })
}

fn report(r: &serde_json::Value) {
    let video = r["video"].as_str().unwrap_or("?");
    match r["status"].as_u64() {
        Some(0) => {
            let p = &r["plan"];
            println!("{video}: frames {}..{} of {} kept ({:.3}-{:.3} s) -> {}", p["first_frame"], p["first_frame"].as_u64().unwrap_or(0) + p["frames"].as_u64().unwrap_or(0) - 1,
                p["source_frames"], p["start"].as_f64().unwrap_or(0.0), p["end"].as_f64().unwrap_or(0.0), r["out"].as_str().unwrap_or("?"));
            for n in r["verified"]["notes"].as_array().into_iter().flatten() { println!("  {}", n.as_str().unwrap_or("")); }
            println!("  reads as the original: {}", r["verified"]["cut"]);
            if r["known_camera"] == false { println!("  a camera without an adapter here: it reads the same, but what else it keeps isn't known"); }
        }
        Some(3) => {
            eprintln!("{video}: the cut doesn't read as the original, removed:");
            for p in r["verified"]["problems"].as_array().into_iter().flatten() { eprintln!("  {}", p.as_str().unwrap_or("")); }
        }
        _ => eprintln!("{video}: {}", r["error"].as_str().unwrap_or("failed")),
    }
}

/// The original's extended attributes on the cut: Finder's tags, color label, rating, comment. Not where it came from
/// (quarantine, provenance) nor the system's own
#[cfg(target_os = "macos")]
fn copy_xattrs(src: &Path, dst: &Path) {
    use std::ffi::CString;
    use std::os::unix::ffi::OsStrExt;
    const SKIP: &[&str] = &["com.apple.quarantine", "com.apple.provenance", "com.apple.macl", "com.apple.rootless", "com.apple.ResourceFork"];
    let (Ok(s), Ok(d)) = (CString::new(src.as_os_str().as_bytes()), CString::new(dst.as_os_str().as_bytes())) else { return };
    unsafe {
        let n = libc::listxattr(s.as_ptr(), std::ptr::null_mut(), 0, 0);
        if n <= 0 { return; }
        let mut names = vec![0u8; n as usize];
        if libc::listxattr(s.as_ptr(), names.as_mut_ptr() as *mut libc::c_char, names.len(), 0) <= 0 { return; }
        for name in names.split(|&c| c == 0).filter(|x| !x.is_empty()) {
            if SKIP.contains(&String::from_utf8_lossy(name).as_ref()) { continue; }
            let Ok(cn) = CString::new(name) else { continue };
            let len = libc::getxattr(s.as_ptr(), cn.as_ptr(), std::ptr::null_mut(), 0, 0, 0);
            if len < 0 { continue; }
            let mut v = vec![0u8; len as usize];
            if libc::getxattr(s.as_ptr(), cn.as_ptr(), v.as_mut_ptr() as *mut libc::c_void, v.len(), 0, 0) < 0 { continue; }
            libc::setxattr(d.as_ptr(), cn.as_ptr(), v.as_ptr() as *const libc::c_void, v.len(), 0, 0);
        }
    }
}

#[cfg(not(target_os = "macos"))]
fn copy_xattrs(_src: &Path, _dst: &Path) {}
