// SPDX-License-Identifier: GPL-3.0-or-later
//! A picture of a video for the page's cards, as fast as Finder or Explorer draws one: by the system's thumbnail
//! service, which decodes on the GPU, reads a clip's index and one keyframe (a few MB over SMB) and keeps the system's
//! own cache, so a clip the file browser has shown comes back at once.
//! - macOS: QLThumbnailGenerator, in afp-thumbs beside this app (macos/thumbs.swift): one helper for the app's life,
//!   many requests at once.
//! - Windows: the shell's thumbnail cache (IShellItemImageFactory), as Explorer's: HEVC only with Microsoft's HEVC
//!   Video Extensions installed.
//! A file it can't draw in time, and every file on Linux: the player's ffmpeg, decoding only the first keyframe, on the
//! GPU where it can (VideoToolbox, D3D11VA, VA-API).
//! Before all that, a picture the NAS already made beside the clip, when there is one: Synology's @eaDir, QNAP's
//! .@__thumb (UGREEN's UGOS keeps its own in /volumeN/@thumbnail, outside the shares: not reachable).
//! A log clip (D-Log M, GP-Log) looks flat, as it does in Finder.

use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// Longest side of a picture, in pixels (a card is at most ~360 wide)
const SIZE: u32 = 360;
/// How long one file may take the system's service before ffmpeg is asked instead
const QL_WAIT: Duration = Duration::from_secs(8);

/// `path`'s picture as a JPEG at `out`; false: none made
pub fn video_frame(path: &Path, out: &Path) -> bool {
    if nas_thumb(path, out) { return true; }
    if cfg!(target_os = "macos") && quick_look(path, out) { return true; }
    #[cfg(windows)]
    if shell::thumb(path, out) { return true; }
    ffmpeg_frame(path, out)
}

/// The first keyframe by the player's ffmpeg, decoded on the GPU where it can (falls back to the CPU by itself)
pub fn ffmpeg_frame(path: &Path, out: &Path) -> bool {
    let ffmpeg = crate::player_ffmpeg();
    let mut cmd = Command::new(ffmpeg.as_deref().unwrap_or(Path::new("ffmpeg")));
    cmd.args(["-v", "error", "-y"]);
    if cfg!(target_os = "macos") { cmd.args(["-hwaccel", "videotoolbox"]); }
    else if cfg!(windows) { cmd.args(["-hwaccel", "d3d11va"]); }
    else { cmd.args(["-hwaccel", "auto"]); }
    cmd.args(["-skip_frame", "nokey", "-i"]).arg(path)
        .args(["-an", "-sn", "-dn", "-frames:v", "1", "-vf", &format!("scale={SIZE}:-2"), "-q:v", "4"]).arg(out)
        .stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null());
    #[cfg(windows)]
    { use std::os::windows::process::CommandExt; cmd.creation_flags(0x0800_0000); } // no console window
    cmd.status().is_ok_and(|s| s.success()) && std::fs::metadata(out).is_ok_and(|m| m.len() > 0)
}

// ---------- a NAS's own pictures ----------

/// Folders of a NAS's pictures, by the folder they're in: (Synology's @eaDir, QNAP's .@__thumb) there, and when looked.
/// Looked once a folder, so a share without them costs two questions a folder, not some for each clip
static NAS_DIRS: Mutex<Option<HashMap<PathBuf, (bool, bool, std::time::Instant)>>> = Mutex::new(None);

/// A picture of `path` its NAS made (Synology Photos / Video Station, QNAP), copied to `out`. Untried on those NASes:
/// names as they're documented, and only a JPEG of a sensible size is taken
pub fn nas_thumb(path: &Path, out: &Path) -> bool {
    let (Some(dir), Some(name)) = (path.parent(), path.file_name().map(|n| n.to_string_lossy().into_owned())) else { return false };
    let (syno, qnap) = {
        let mut m = NAS_DIRS.lock().unwrap();
        let m = m.get_or_insert_with(HashMap::new);
        match m.get(dir) {
            Some(&(a, b, at)) if at.elapsed() < Duration::from_secs(600) => (a, b),
            _ => {
                let v = (dir.join("@eaDir").is_dir(), dir.join(".@__thumb").is_dir());
                m.insert(dir.to_path_buf(), (v.0, v.1, std::time::Instant::now()));
                v
            }
        }
    };
    let mut candidates: Vec<PathBuf> = Vec::new();
    if syno {
        let d = dir.join("@eaDir").join(&name);
        // Synology Photos: M 320 px, B 640, SM 240, S small; Video Station: a frame of the video
        for f in ["SYNOPHOTO_THUMB_M.jpg", "SYNOPHOTO_THUMB_B.jpg", "SYNOPHOTO_THUMB_SM.jpg", "SYNOVIDEO_VIDEO_SCREENSHOT.jpg", "SYNOPHOTO_THUMB_S.jpg"] {
            candidates.push(d.join(f));
        }
    }
    if qnap {
        let d = dir.join(".@__thumb");
        for size in ["s400", "s800", "s100"] {
            candidates.push(d.join(format!("{size}{name}")));
            candidates.push(d.join(format!("{size}{name}.jpg")));
        }
    }
    for c in candidates {
        let Ok(b) = std::fs::read(&c) else { continue };
        if b.len() >= 1024 && b.len() <= 512 * 1024 && b.starts_with(&[0xFF, 0xD8]) && std::fs::write(out, &b).is_ok() { return true; }
    }
    false
}

// ---------- the system's thumbnail service (macOS) ----------

type Waiting = Arc<Mutex<HashMap<u64, (mpsc::Sender<bool>, PathBuf)>>>;

struct Helper {
    child: Child,
    stdin: ChildStdin,
    waiting: Waiting,
    next: u64,
}

static HELPER: Mutex<Option<Helper>> = Mutex::new(None);
/// Timeouts in a row: from 3 on, the service seems stuck, and ffmpeg makes them for the rest of the app's run
static STUCK: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);

fn helper_path() -> Option<PathBuf> {
    if let Some(p) = std::env::var_os("AFL_THUMBS_HELPER").filter(|p| !p.is_empty()) { return Some(PathBuf::from(p)); }
    std::env::current_exe().ok()?.parent().map(|d| d.join("afp-thumbs")).filter(|h| h.is_file())
}

fn start() -> Option<Helper> {
    let mut child = Command::new(helper_path()?).args(["--size", &SIZE.to_string()])
        .stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::null()).spawn().ok()?;
    let stdin = child.stdin.take()?;
    let stdout = child.stdout.take()?;
    let waiting: Waiting = Arc::default();
    let w = waiting.clone();
    std::thread::spawn(move || {
        for line in BufReader::new(stdout).lines() {
            let Ok(line) = line else { break };
            let (id, what) = line.split_once('\t').unwrap_or((&line, ""));
            let Ok(id) = id.parse::<u64>() else { continue };
            let Some((tx, out)) = w.lock().unwrap().remove(&id) else { continue };
            // Its asker gave up waiting (QL_WAIT) and has ffmpeg's: a late picture is dropped
            if tx.send(what == "ok").is_err() { let _ = std::fs::remove_file(out); }
        }
        // It ended: whoever still waits hears no (their senders go), the next request starts another
        w.lock().unwrap().clear();
    });
    Some(Helper { child, stdin, waiting, next: 0 })
}

/// The system's picture of `path` to `out`, within QL_WAIT
fn quick_look(path: &Path, out: &Path) -> bool {
    let p = path.to_string_lossy();
    let o = out.to_string_lossy();
    if p.contains(['\t', '\n']) || o.contains(['\t', '\n']) { return false; }
    use std::sync::atomic::Ordering::Relaxed;
    if STUCK.load(Relaxed) >= 3 { return false; }
    let (rx, id, waiting) = {
        let mut h = HELPER.lock().unwrap();
        if h.as_mut().is_some_and(|h| !matches!(h.child.try_wait(), Ok(None))) { *h = None; }
        if h.is_none() { *h = start(); }
        let Some(helper) = h.as_mut() else { return false };
        let id = helper.next;
        helper.next += 1;
        let (tx, rx) = mpsc::channel();
        helper.waiting.lock().unwrap().insert(id, (tx, out.to_path_buf()));
        if writeln!(helper.stdin, "{id}\t{p}\t{o}").and_then(|_| helper.stdin.flush()).is_err() {
            let _ = helper.child.kill();
            *h = None;
            return false;
        }
        (rx, id, helper.waiting.clone())
    };
    match rx.recv_timeout(QL_WAIT) {
        Ok(ok) => { STUCK.store(0, Relaxed); ok && std::fs::metadata(out).is_ok_and(|m| m.len() > 0) }
        Err(mpsc::RecvTimeoutError::Timeout) => { waiting.lock().unwrap().remove(&id); STUCK.fetch_add(1, Relaxed); false }
        Err(mpsc::RecvTimeoutError::Disconnected) => false,
    }
}

// ---------- the shell's thumbnail cache (Windows) ----------

#[cfg(windows)]
mod shell {
    use super::{QL_WAIT, SIZE};
    use std::path::Path;
    use windows::core::HSTRING;
    use windows::Win32::Foundation::{GENERIC_WRITE, SIZE as WSIZE};
    use windows::Win32::Graphics::Gdi::{DeleteObject, HPALETTE};
    use windows::Win32::Graphics::Imaging::*;
    use windows::Win32::System::Com::{CoCreateInstance, CoInitializeEx, CoUninitialize, CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED};
    use windows::Win32::UI::Shell::{IShellItemImageFactory, SHCreateItemFromParsingName, SIIGBF_THUMBNAILONLY};

    /// Explorer's picture of `path` (never its icon) to `out` as a JPEG, within QL_WAIT. On a thread of its own (the
    /// shell wants a single-threaded apartment; a provider that hangs keeps only that thread)
    pub fn thumb(path: &Path, out: &Path) -> bool {
        let (p, o) = (path.to_path_buf(), out.to_path_buf());
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let ok = unsafe {
                let init = CoInitializeEx(None, COINIT_APARTMENTTHREADED).is_ok();
                let ok = draw(&p, &o).is_ok();
                if init { CoUninitialize(); }
                ok
            };
            if tx.send(ok).is_err() { let _ = std::fs::remove_file(&o); }
        });
        rx.recv_timeout(QL_WAIT).unwrap_or(false) && std::fs::metadata(out).is_ok_and(|m| m.len() > 0)
    }

    unsafe fn draw(path: &Path, out: &Path) -> windows::core::Result<()> {
        unsafe {
            let item: IShellItemImageFactory = SHCreateItemFromParsingName(&HSTRING::from(path.as_os_str()), None)?;
            let side = SIZE as i32;
            let hbm = item.GetImage(WSIZE { cx: side, cy: side }, SIIGBF_THUMBNAILONLY)?;
            let wic: IWICImagingFactory = CoCreateInstance(&CLSID_WICImagingFactory, None, CLSCTX_INPROC_SERVER)?;
            let bitmap = wic.CreateBitmapFromHBITMAP(hbm, HPALETTE::default(), WICBitmapIgnoreAlpha);
            let _ = DeleteObject(hbm.into());
            let bitmap = bitmap?;
            let bgr = wic.CreateFormatConverter()?;
            bgr.Initialize(&bitmap, &GUID_WICPixelFormat24bppBGR, WICBitmapDitherTypeNone, None, 0.0, WICBitmapPaletteTypeCustom)?;
            let stream = wic.CreateStream()?;
            stream.InitializeFromFilename(&HSTRING::from(out.as_os_str()), GENERIC_WRITE.0)?;
            let enc = wic.CreateEncoder(&GUID_ContainerFormatJpeg, std::ptr::null())?;
            enc.Initialize(&stream, WICBitmapEncoderNoCache)?;
            let mut frame = None;
            let mut props = None;
            enc.CreateNewFrame(&mut frame, &mut props)?;
            let frame = frame.ok_or_else(windows::core::Error::empty)?;
            frame.Initialize(props.as_ref())?;
            frame.WriteSource(&bgr, std::ptr::null())?;
            frame.Commit()?;
            enc.Commit()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nas_pictures_beside_clips() {
        let root = std::env::temp_dir().join(format!("afl-nas-thumbs-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let jpeg = |n: usize| { let mut v = vec![0xFF, 0xD8, 0xFF, 0xE0]; v.resize(n, 7); v };
        let (syno, qnap, plain) = (root.join("syno"), root.join("qnap"), root.join("plain"));
        std::fs::create_dir_all(syno.join("@eaDir/GX01.MP4")).unwrap();
        std::fs::write(syno.join("@eaDir/GX01.MP4/SYNOPHOTO_THUMB_XL.jpg"), jpeg(4000)).unwrap(); // not one we take
        std::fs::write(syno.join("@eaDir/GX01.MP4/SYNOPHOTO_THUMB_B.jpg"), jpeg(3000)).unwrap();
        std::fs::create_dir_all(syno.join("@eaDir/GX02.MP4")).unwrap();
        std::fs::write(syno.join("@eaDir/GX02.MP4/SYNOPHOTO_THUMB_M.jpg"), b"not a jpeg, long enough?".repeat(100)).unwrap();
        std::fs::create_dir_all(qnap.join(".@__thumb")).unwrap();
        std::fs::write(qnap.join(".@__thumb/s400DJI_1.MP4"), jpeg(2000)).unwrap();
        std::fs::create_dir_all(&plain).unwrap();
        let out = root.join("out.jpg");
        assert!(nas_thumb(&syno.join("GX01.MP4"), &out));
        assert_eq!(std::fs::read(&out).unwrap().len(), 3000);
        assert!(!nas_thumb(&syno.join("GX02.MP4"), &out));
        assert!(nas_thumb(&qnap.join("DJI_1.MP4"), &out));
        assert_eq!(std::fs::read(&out).unwrap().len(), 2000);
        assert!(!nas_thumb(&qnap.join("DJI_2.MP4"), &out));
        assert!(!nas_thumb(&plain.join("A.MP4"), &out));
        let _ = std::fs::remove_dir_all(&root);
    }

    /// Page-like timing over a folder of clips: AFL_THUMBS_DIR (its .mp4/.mov, AFL_THUMBS_MAX of them, sorted; or
    /// AFL_THUMBS_LIST, a file of paths, one a line), as the
    /// page asks (index::thumb), AFL_THUMBS_JOBS at a time (6), into AFL_THUMBS_CACHE (kept; else a fresh one, removed).
    /// AFL_THUMBS_HELPER: afp-thumbs built from macos/thumbs.swift, AFL_PLAYER: the player whose ffmpeg is the
    /// fallback. AFL_THUMBS_FFMPEG_ONLY=1: ffmpeg's frame only
    #[test]
    #[ignore]
    fn thumbs_timing() {
        let dir = PathBuf::from(std::env::var("AFL_THUMBS_DIR").unwrap_or_default());
        let max: usize = std::env::var("AFL_THUMBS_MAX").ok().and_then(|s| s.parse().ok()).unwrap_or(300);
        let jobs: usize = std::env::var("AFL_THUMBS_JOBS").ok().and_then(|s| s.parse().ok()).unwrap_or(6);
        let only_ffmpeg = std::env::var("AFL_THUMBS_FFMPEG_ONLY").is_ok_and(|v| v == "1");
        fn walk(d: &Path, out: &mut Vec<PathBuf>) {
            let Ok(rd) = std::fs::read_dir(d) else { return };
            let mut es: Vec<_> = rd.flatten().map(|e| e.path()).collect();
            es.sort();
            for p in es {
                let n = p.file_name().unwrap().to_string_lossy().to_lowercase();
                if n.starts_with('.') || n.starts_with('#') { continue; }
                if p.is_dir() { walk(&p, out); } else if (n.ends_with(".mp4") || n.ends_with(".mov")) && !n.ends_with(crate::index::LINK_EXT) { out.push(p); }
            }
        }
        let mut files = Vec::new();
        match std::env::var_os("AFL_THUMBS_LIST") {
            Some(l) => files = std::fs::read_to_string(l).unwrap().lines().filter(|l| !l.is_empty()).map(PathBuf::from).collect(),
            None => walk(&dir, &mut files),
        }
        files.truncate(max);
        let kept = std::env::var_os("AFL_THUMBS_CACHE").map(PathBuf::from);
        let cache = kept.clone().unwrap_or_else(|| std::env::temp_dir().join(format!("afl-thumbs-{}", std::process::id())));
        std::fs::create_dir_all(&cache).unwrap();
        let queue = Arc::new(Mutex::new(files.clone().into_iter().enumerate().collect::<Vec<_>>()));
        let times = Arc::new(Mutex::new(Vec::new()));
        let t0 = std::time::Instant::now();
        let first = Arc::new(Mutex::new(None));
        let mut hs = Vec::new();
        for _ in 0..jobs {
            let (q, ts, c, f) = (queue.clone(), times.clone(), cache.clone(), first.clone());
            hs.push(std::thread::spawn(move || loop {
                let Some((i, p)) = q.lock().unwrap().pop() else { break };
                let t = std::time::Instant::now();
                let out = c.join(format!("{i}.jpg"));
                let ok = if only_ffmpeg { let ok = ffmpeg_frame(&p, &out); let _ = std::fs::remove_file(&out); ok }
                         else { crate::index::thumb(&p, &c).is_some() };
                let ms = t.elapsed().as_millis();
                f.lock().unwrap().get_or_insert(t0.elapsed().as_millis());
                ts.lock().unwrap().push((ms, ok, p));
            }));
        }
        for h in hs { h.join().unwrap(); }
        let total = t0.elapsed();
        let mut ts = times.lock().unwrap();
        ts.sort_by_key(|t| t.0);
        let ok = ts.iter().filter(|t| t.1).count();
        let pct = |q: f64| ts[((ts.len() as f64 - 1.0) * q) as usize].0;
        eprintln!("{} files, {ok} pictures, {jobs} at a time: total {:.1} s, first after {} ms, each median {} ms, p90 {} ms, max {} ms",
                  ts.len(), total.as_secs_f64(), first.lock().unwrap().unwrap_or(0), pct(0.5), pct(0.9), pct(1.0));
        for t in ts.iter().filter(|t| !t.1) { eprintln!("  none: {}", t.2.display()); }
        if kept.is_none() { let _ = std::fs::remove_dir_all(&cache); }
    }
}
