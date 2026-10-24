// SPDX-License-Identifier: GPL-3.0-or-later
//
// ActionFootagePlayer.exe, the Windows app (macos/player.sh is the Mac's): the mpv it carries, with this project's
// plugins, VapourSynth for the stabilization and ffmpeg for exports, all in the app's folder
// (tools/make_windows_app.sh):
//
//   ActionFootagePlayer.exe   this: what Explorer opens videos with
//   mpv\mpv.exe               mpv (shinchiro's build, with VapourSynth support), with this app's icon, and the VC++
//                             runtime VapourSynth needs
//   python\                   Python, with VapourSynth in Lib\site-packages\vapoursynth\ (VSSCRIPT_PATH tells mpv
//                             where its vsscript.dll is; it finds this Python from there)
//   plugins\                  the plugins: dlogm, gyroflow (with ffmpeg.exe and ffprobe.exe for its exports),
//                             dji-telemetry, highlights, and footage-menu (their right-click menu)
//   vulkan\vulkan-1.dll       the Vulkan loader, which mpv.exe can't start without, for a PC whose GPU driver hasn't
//                             installed one (last in mpv's PATH: the system's comes first)
//   tools\setup.py            what fetches DJI's LUT
//   app.conf                  the app's mpv settings (mpv/app.conf)
//
// The app is whole by itself: mpv gets the app's plugins and no others, and its settings (app.conf), with a config
// folder of its own, %LOCALAPPDATA%\ActionFootagePlayer\mpv\, where the plugins keep what you set (script-opts\). An
// mpv you may also have, its %APPDATA%\mpv (MPV_HOME), isn't read: the first time, only the plugins' settings
// are copied over from it. DJI's LUT and your default preset are in %LOCALAPPDATA%\ActionFootagePlayer\ too
// (DLOGM_DATA, GYROFLOW_DATA): the app's folder is replaced by every update. While DJI's LUT isn't there, it's
// fetched in the background (the installer does it first).
//
// A video opened while one plays takes its place, in the same window (mpv's IPC pipe). Several opened together
// (Explorer starts this once for each) go into one playlist.
//
// `ActionFootagePlayer.exe --export <video>...` is Explorer's Stabilize + LUT: gyroflow-export on the videos, in a
// console window that shows how it goes. Videos sent while it runs are exported after the others, in the same window.

#![windows_subsystem = "windows"]

use std::ffi::OsString;
use std::path::{ Path, PathBuf };
use std::process::Command;

const NAME: &str = "ActionFootagePlayer";
// The playing one's IPC pipe: videos opened with the app go there while it plays
const PIPE: &str = r"\\.\pipe\ActionFootagePlayer";
const PLUGINS: [&str; 5] = ["gyroflow", "dlogm", "dji-telemetry", "highlights", "footage-menu"];
// Their settings' files, in script-opts\
const PLUGIN_OPTS: [&str; 5] = ["gyroflow", "dlogm", "dji_telemetry", "highlights", "footage_menu"];
// A video sent to a player started less than this long ago goes into its playlist (several opened together), later
// in place of what plays
const TOGETHER: std::time::Duration = std::time::Duration::from_secs(3);

fn main() {
    let mut args: Vec<OsString> = std::env::args_os().skip(1).collect();
    let app = match std::env::current_exe().ok().and_then(|e| Some(e.parent()?.to_path_buf())) {
        Some(app) => app,
        None => return win::error("Can't find the app's folder"),
    };
    let result = if args.first().is_some_and(|a| a == "--export") {
        args.remove(0);
        export(&app, args)
    } else {
        play(&app, args)
    };
    if let Err(e) = result {
        win::error(&e);
    }
}

fn data_dir() -> PathBuf {
    let local = std::env::var_os("LOCALAPPDATA").map(PathBuf::from)
        .unwrap_or_else(|| std::env::temp_dir());
    local.join(NAME)
}

// mpv's config folder, the app's own. The first time, the plugins' settings come over from mpv's (MPV_HOME, else
// %APPDATA%\mpv: up to 0.6.0 the app read that one)
fn mpv_config() -> PathBuf {
    let config = data_dir().join("mpv");
    if !config.is_dir() {
        let opts = config.join("script-opts");
        let _ = std::fs::create_dir_all(&opts);
        let theirs = std::env::var_os("MPV_HOME").filter(|v| !v.is_empty()).map(PathBuf::from)
            .or_else(|| Some(PathBuf::from(std::env::var_os("APPDATA")?).join("mpv")));
        if let Some(theirs) = theirs {
            for name in PLUGIN_OPTS {
                let file = format!("{name}.conf");
                let _ = std::fs::copy(theirs.join("script-opts").join(&file), opts.join(&file));
            }
        }
    }
    config
}

// What the plugins and their binaries find their things by. Python's folder in PATH too: vsscript.dll loads its
// python3.dll, whose functions are python313.dll's, which Windows then has to find. And the app's Vulkan loader, last:
// Windows looks in System32 before PATH, so the system's is used when there is one
fn environment(cmd: &mut Command, app: &Path) {
    let data = data_dir();
    let python = app.join("python");
    let mut path = vec![python.clone()];
    path.extend(std::env::var_os("PATH").map(|p| std::env::split_paths(&p).collect::<Vec<_>>()).unwrap_or_default());
    path.push(app.join("vulkan"));
    if let Ok(path) = std::env::join_paths(path) {
        cmd.env("PATH", path);
    }
    cmd.env("VSSCRIPT_PATH", python.join(r"Lib\site-packages\vapoursynth\vsscript.dll"))
        .env("DLOGM_DATA", data.join("dlogm"))
        .env("GYROFLOW_DATA", data.join("gyroflow"));
}

// A video's full path (mpv gets them relative to its own folder otherwise); a URL as it is
fn full_path(video: &OsString) -> PathBuf {
    if video.to_string_lossy().contains("://") {
        return PathBuf::from(video);
    }
    std::path::absolute(video).unwrap_or_else(|_| PathBuf::from(video))
}

// Touched when a player starts: videos sent to it soon after were opened together with its own
fn started_file() -> PathBuf {
    data_dir().join("player-started")
}

fn is_option(arg: &OsString) -> bool {
    arg.to_string_lossy().starts_with("--")
}

fn play(app: &Path, args: Vec<OsString>) -> Result<(), String> {
    // One at a time: of several started together, the first starts mpv and the others wait for its pipe
    let _starting = win::Mutex::acquire(r"Local\ActionFootagePlayer-start", u32::MAX);
    let files: Vec<&OsString> = args.iter().filter(|a| !is_option(a)).collect();
    if !files.is_empty() && ipc::send_videos(&files) {
        win::focus_mpv();
        return Ok(());
    }

    let mpv = app.join(r"mpv\mpv.exe");
    let mut cmd = Command::new(&mpv);
    environment(&mut cmd, app);
    cmd.arg("--player-operation-mode=pseudo-gui").arg(format!("--input-ipc-server={PIPE}"))
        .arg(format!("--config-dir={}", mpv_config().display())).arg("--load-scripts=no")
        .arg(format!("--include={}", app.join("app.conf").display()));
    for plugin in PLUGINS {
        cmd.arg(format!("--script={}", app.join("plugins").join(plugin).display()));
    }
    cmd.args(&args);
    cmd.spawn().map_err(|e| format!("Couldn't start {}: {e}", mpv.display()))?;
    let _ = std::fs::create_dir_all(data_dir());
    let _ = std::fs::write(started_file(), b"");

    // DJI's LUT (Ctrl+s, exports with --sdr), when it isn't there yet
    if !data_dir().join(r"dlogm\shaders\dlogm_lut3d.glsl").is_file() {
        let mut setup = Command::new(app.join(r"python\pythonw.exe"));
        setup.arg("-I").arg(app.join(r"tools\setup.py")).arg("lut");
        environment(&mut setup, app);
        let _ = setup.spawn();
    }

    // Until its pipe is there, videos opened after this one would start another mpv
    for _ in 0..50 {
        if ipc::running() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
    Ok(())
}

fn export(app: &Path, videos: Vec<OsString>) -> Result<(), String> {
    // The videos go into a queue, which the first one running exports, one batch after another: Explorer starts this
    // once for each video selected, and each would have its own window and export at the same time as the others
    let queue = data_dir().join("export-queue");
    std::fs::create_dir_all(&queue).map_err(|e| format!("{}: {e}", queue.display()))?;
    let stamp = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_nanos();
    for (i, video) in videos.iter().enumerate() {
        let path = full_path(video);
        let entry = queue.join(format!("{stamp:024}-{:08}-{i:04}.txt", std::process::id()));
        std::fs::write(&entry, path.to_string_lossy().as_bytes()).map_err(|e| format!("{}: {e}", entry.display()))?;
    }

    let mut console = false;
    loop {
        // The one exporting takes these too
        let Some(lock) = win::Mutex::acquire(r"Local\ActionFootagePlayer-export", 0) else { break };
        if !console {
            win::console(&format!("{NAME}: Stabilize + LUT"));
            console = true;
        }
        loop {
            let batch = take_queue(&queue);
            if batch.is_empty() {
                break;
            }
            run_export(app, &batch);
        }
        drop(lock);
        // A video queued as this one finished, which the one sending it left to this one
        if queued(&queue).is_empty() {
            break;
        }
    }
    if console {
        println!("\nDone. Press Enter to close this window.");
        let _ = std::io::stdin().read_line(&mut String::new());
    }
    Ok(())
}

fn queued(queue: &Path) -> Vec<PathBuf> {
    let mut entries: Vec<PathBuf> = std::fs::read_dir(queue).into_iter().flatten().flatten()
        .map(|e| e.path()).filter(|p| p.extension().is_some_and(|x| x == "txt")).collect();
    entries.sort();
    entries
}

fn take_queue(queue: &Path) -> Vec<PathBuf> {
    queued(queue).into_iter().filter_map(|entry| {
        let video = std::fs::read_to_string(&entry).ok()?;
        std::fs::remove_file(&entry).ok()?;
        Some(PathBuf::from(video))
    }).collect()
}

fn run_export(app: &Path, videos: &[PathBuf]) {
    let bin = app.join(r"plugins\gyroflow\gyroflow-export.exe");
    println!("Exporting {} video(s), each into a stabilized folder next to it:", videos.len());
    for video in videos {
        println!("  {}", video.display());
    }
    println!();
    let mut cmd = Command::new(&bin);
    environment(&mut cmd, app);
    match cmd.args(videos).status() {
        Ok(status) if !status.success() => println!("gyroflow-export stopped: {status}"),
        Err(e) => println!("Couldn't start {}: {e}", bin.display()),
        _ => {}
    }
    println!();
}

// mpv's JSON IPC, over the playing one's named pipe
mod ipc {
    use super::*;
    use std::io::Write;

    fn connect() -> Option<std::fs::File> {
        std::fs::OpenOptions::new().read(true).write(true).open(PIPE).ok()
    }

    pub fn running() -> bool {
        connect().is_some()
    }

    fn json_string(s: &str) -> String {
        let mut out = String::from("\"");
        for c in s.chars() {
            match c {
                '"' => out.push_str("\\\""),
                '\\' => out.push_str("\\\\"),
                c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
                c => out.push(c),
            }
        }
        out.push('"');
        out
    }

    // The videos to the playing mpv: in place of what plays, or into its playlist when it has just started (videos
    // opened together). False when no mpv of the app's is running
    pub fn send_videos(videos: &[&OsString]) -> bool {
        let Some(mut pipe) = connect() else { return false };
        let together = std::fs::metadata(started_file()).and_then(|m| m.modified()).ok()
            .and_then(|t| t.elapsed().ok()).is_some_and(|age| age < TOGETHER);
        let mut commands = String::new();
        for (i, video) in videos.iter().enumerate() {
            let path = full_path(video);
            let mode = if together || i > 0 { "append-play" } else { "replace" };
            commands.push_str(&format!("{{\"command\":[\"loadfile\",{},\"{mode}\"]}}\n",
                                       json_string(&path.to_string_lossy())));
        }
        pipe.write_all(commands.as_bytes()).is_ok()
    }
}

// The few Win32 calls it needs
mod win {
    use std::ffi::c_void;

    type Handle = *mut c_void;
    const WAIT_OBJECT_0: u32 = 0;
    const WAIT_ABANDONED: u32 = 0x80;
    const MB_ICONERROR: u32 = 0x10;
    const SW_RESTORE: i32 = 9;

    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn CreateMutexW(attributes: *const c_void, initial_owner: i32, name: *const u16) -> Handle;
        fn WaitForSingleObject(handle: Handle, milliseconds: u32) -> u32;
        fn ReleaseMutex(handle: Handle) -> i32;
        fn CloseHandle(handle: Handle) -> i32;
        fn AllocConsole() -> i32;
        fn SetConsoleTitleW(title: *const u16) -> i32;
    }

    #[link(name = "user32")]
    unsafe extern "system" {
        fn MessageBoxW(window: Handle, text: *const u16, caption: *const u16, kind: u32) -> i32;
        fn FindWindowW(class: *const u16, title: *const u16) -> Handle;
        fn IsIconic(window: Handle) -> i32;
        fn ShowWindow(window: Handle, command: i32) -> i32;
        fn SetForegroundWindow(window: Handle) -> i32;
    }

    fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(std::iter::once(0)).collect()
    }

    pub fn error(message: &str) {
        let (text, caption) = (wide(message), wide(super::NAME));
        unsafe { MessageBoxW(std::ptr::null_mut(), text.as_ptr(), caption.as_ptr(), MB_ICONERROR) };
    }

    // A console window of its own (the app has none), for what's printed and its children's output
    pub fn console(title: &str) {
        let title = wide(title);
        unsafe {
            AllocConsole();
            SetConsoleTitleW(title.as_ptr());
        }
    }

    // mpv's window to the front: it was started by Explorer, so this can
    pub fn focus_mpv() {
        let class = wide("mpv");
        unsafe {
            let window = FindWindowW(class.as_ptr(), std::ptr::null());
            if window.is_null() {
                return;
            }
            if IsIconic(window) != 0 {
                ShowWindow(window, SW_RESTORE);
            }
            SetForegroundWindow(window);
        }
    }

    // A named mutex, held until dropped
    pub struct Mutex(Handle);

    impl Mutex {
        // None when another process holds it for longer than the timeout (u32::MAX: as long as it takes)
        pub fn acquire(name: &str, timeout_ms: u32) -> Option<Mutex> {
            let name = wide(name);
            unsafe {
                let handle = CreateMutexW(std::ptr::null(), 0, name.as_ptr());
                if handle.is_null() {
                    return None;
                }
                match WaitForSingleObject(handle, timeout_ms) {
                    WAIT_OBJECT_0 | WAIT_ABANDONED => Some(Mutex(handle)),
                    _ => {
                        CloseHandle(handle);
                        None
                    }
                }
            }
        }
    }

    impl Drop for Mutex {
        fn drop(&mut self) {
            unsafe {
                ReleaseMutex(self.0);
                CloseHandle(self.0);
            }
        }
    }
}
