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
//                             dji-telemetry, highlights, footage-menu (their right-click menu), side-panel (their
//                             settings beside the video), agent (notes for an AI agent, and its IPC), workspace
//                             (the project a clip belongs to), any-layout (the keys in a Cyrillic keyboard layout
//                             too) and view360 (360° clips as a view to look around in)
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
// `ActionFootagePlayer.exe --batch <request.json>` is Action Footage Library's: mpv with no window making a project's
// clip previews, moments, photos or exports (the workspace plugin's batch mode, AFP_BATCH), waited for.
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
// Held by the launcher of the running player until its mpv has exited (play)
const PLAYER: &str = r"Local\ActionFootagePlayer-player";
// How long a player quitting is waited for, at most, before another starts anyway
const PLAYER_WAIT_MS: u32 = 60_000;
// How long the playing one has to answer the videos sent to it (ipc::send_videos)
const ANSWER_WAIT: std::time::Duration = std::time::Duration::from_secs(5);
const PLUGINS: [&str; 10] = [
    "workspace", "gyroflow", "view360", "dlogm", "dji-telemetry", "highlights", "footage-menu", "side-panel", "agent", "any-layout",
];
// Their settings' files, in script-opts\
const PLUGIN_OPTS: [&str; 7] = ["gyroflow", "dlogm", "dji_telemetry", "highlights", "footage_menu", "side_panel", "agent"];
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
    } else if args.first().is_some_and(|a| a == "--batch") && args.len() == 2 {
        batch(&app, &args[1])
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
        .env("GYROFLOW_DATA", data.join("gyroflow"))
        .env("AGENT_DATA", data.join("agent"));
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
    let starting = win::Mutex::acquire(r"Local\ActionFootagePlayer-start", u32::MAX);
    let files: Vec<&OsString> = args.iter().filter(|a| !is_option(a)).collect();
    if !files.is_empty() && ipc::send_videos(&files) {
        win::focus_mpv();
        return Ok(());
    }
    // A player quitting has closed its pipe but keeps its name until it has exited (its plugins finishing what they
    // do, seconds on a NAS); one started meanwhile would get no pipe, and later videos would each start another. The
    // launcher of each player holds PLAYER until its mpv has exited: waited for here, a while at most
    let player = win::Mutex::acquire(PLAYER, PLAYER_WAIT_MS);

    let mpv = app.join(r"mpv\mpv.exe");
    let mut cmd = Command::new(&mpv);
    environment(&mut cmd, app);
    // The window at once, not once the first frame is there: a clip on a NAS takes seconds to be read and set up for
    // the stabilization, and meanwhile there'd be no window and nothing in the taskbar (the plugins say what it does)
    cmd.arg("--player-operation-mode=pseudo-gui").arg("--force-window=immediate").arg(format!("--input-ipc-server={PIPE}"))
        .arg(format!("--config-dir={}", mpv_config().display())).arg("--load-scripts=no")
        .arg(format!("--include={}", app.join("app.conf").display()));
    // The menu says which version this is
    if let Ok(version) = std::fs::read_to_string(app.join("VERSION")) {
        cmd.arg(format!("--script-opts-append=footage_menu-version={}", version.trim()));
    }
    for plugin in PLUGINS {
        cmd.arg(format!("--script={}", app.join("plugins").join(plugin).display()));
    }
    cmd.args(&args);
    let mut child = cmd.spawn().map_err(|e| format!("Couldn't start {}: {e}", mpv.display()))?;
    // Its window in front: Windows gives the foreground to what the app the user clicked in (Explorer, Action Footage
    // Library) starts, this launcher, and not to what this starts in turn
    win::allow_foreground(child.id());
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
    // Its window (made at once, force-window=immediate) to the front, while this still may
    win::focus_window_of(child.id(), std::time::Duration::from_secs(5));
    // Videos opened from now on go to its pipe; this launcher stays (windowless) with PLAYER until mpv has exited
    drop(starting);
    let _ = child.wait();
    drop(player);
    Ok(())
}

// Action Footage Library's batch: no window, no IPC pipe (a video opened meanwhile starts its own player), the same
// plugins and settings; this waits for it, so that the Library sees when it's done
fn batch(app: &Path, request: &OsString) -> Result<(), String> {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    let mpv = app.join(r"mpv\mpv.exe");
    let mut cmd = Command::new(&mpv);
    environment(&mut cmd, app);
    cmd.env("AFP_BATCH", request)
        .arg("--idle=yes").arg("--vo=null").arg("--ao=null").arg("--force-window=no").arg("--terminal=no")
        .arg(format!("--config-dir={}", mpv_config().display())).arg("--load-scripts=no")
        .arg(format!("--include={}", app.join("app.conf").display()));
    for plugin in PLUGINS {
        cmd.arg(format!("--script={}", app.join("plugins").join(plugin).display()));
    }
    cmd.creation_flags(CREATE_NO_WINDOW);
    let status = cmd.status().map_err(|e| format!("Couldn't start {}: {e}", mpv.display()))?;
    std::process::exit(status.code().unwrap_or(1));
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
    // opened together). False when no mpv of the app's is running, or it was quitting and didn't play them
    pub fn send_videos(videos: &[&OsString]) -> bool {
        let Some(mut pipe) = connect() else { return false };
        let together = std::fs::metadata(started_file()).and_then(|m| m.modified()).ok()
            .and_then(|t| t.elapsed().ok()).is_some_and(|age| age < TOGETHER);
        let mut commands = String::new();
        for (i, video) in videos.iter().enumerate() {
            let path = full_path(video);
            let mode = if together || i > 0 { "append-play" } else { "replace" };
            commands.push_str(&format!("{{\"command\":[\"loadfile\",{},\"{mode}\"],\"request_id\":{}}}\n",
                                       json_string(&path.to_string_lossy()), i + 1));
        }
        let Ok(reader) = pipe.try_clone() else { return false };
        let heard = listen(reader);
        pipe.write_all(commands.as_bytes()).is_ok() && answered(&heard, videos.len(), !together)
    }

    // What the pipe says: a command's reply, an event, or that it closed
    enum Heard { Reply(usize), Event(String), Closed }

    fn listen(pipe: std::fs::File) -> std::sync::mpsc::Receiver<Heard> {
        use std::io::BufRead;
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            for line in std::io::BufReader::new(pipe).lines().map_while(Result::ok) {
                // A reply is {"data":…,"request_id":<n>,"error":…}, an event {"event":"<name>",…}
                let heard = if let Some(at) = line.find("\"request_id\":") {
                    let digits: String = line[at + 13..].chars().take_while(|c| c.is_ascii_digit()).collect();
                    digits.parse().ok().map(Heard::Reply)
                } else if let Some(at) = line.find("\"event\":\"") {
                    line[at + 9..].split('"').next().map(|e| Heard::Event(e.to_string()))
                } else {
                    None
                };
                if let Some(h) = heard {
                    if tx.send(h).is_err() { return; }
                }
            }
            let _ = tx.send(Heard::Closed);
        });
        rx
    }

    // Whether mpv took the commands and plays them. A player quitting keeps its pipe a while (its plugins finishing
    // what they do) and answers a loadfile, then exits without playing it: when one takes the place of what plays,
    // its start (the start-file event) is waited for, and the pipe closing first means it's gone
    fn answered(heard: &std::sync::mpsc::Receiver<Heard>, last: usize, replaces: bool) -> bool {
        let until = std::time::Instant::now() + ANSWER_WAIT;
        let (mut replied, mut started) = (false, false);
        loop {
            let Some(left) = until.checked_duration_since(std::time::Instant::now()) else {
                // A player busy loading (a deferred hook on a NAS) may start the file later: it's alive
                return replied;
            };
            match heard.recv_timeout(left) {
                Ok(Heard::Reply(id)) if id == last => {
                    replied = true;
                    if !replaces || started { return true; }
                }
                Ok(Heard::Event(e)) if e == "start-file" => {
                    started = true;
                    if replied { return true; }
                }
                Ok(Heard::Closed) => return false,
                Ok(_) => {}
                Err(_) => return replied,
            }
        }
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
        fn AllowSetForegroundWindow(process: u32) -> i32;
        fn FindWindowExW(parent: Handle, after: Handle, class: *const u16, title: *const u16) -> Handle;
        fn GetWindowThreadProcessId(window: Handle, process: *mut u32) -> u32;
        fn IsWindowVisible(window: Handle) -> i32;
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

    // The process may bring its windows to the front (this one can: the app the user clicked in started it)
    pub fn allow_foreground(process: u32) {
        unsafe { AllowSetForegroundWindow(process) };
    }

    // The process's mpv window to the front, once it's shown (waited for, a while at most)
    pub fn focus_window_of(process: u32, wait: std::time::Duration) {
        let class = wide("mpv");
        let until = std::time::Instant::now() + wait;
        while std::time::Instant::now() < until {
            unsafe {
                let mut window = std::ptr::null_mut();
                loop {
                    window = FindWindowExW(std::ptr::null_mut(), window, class.as_ptr(), std::ptr::null());
                    if window.is_null() {
                        break;
                    }
                    let mut owner = 0u32;
                    GetWindowThreadProcessId(window, &mut owner);
                    if owner == process && IsWindowVisible(window) != 0 {
                        if IsIconic(window) != 0 {
                            ShowWindow(window, SW_RESTORE);
                        }
                        SetForegroundWindow(window);
                        return;
                    }
                }
            }
            std::thread::sleep(std::time::Duration::from_millis(50));
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
