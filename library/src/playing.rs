// What's selected, opened in the player; shown in Finder
// One of the areas of the Library's commands: main.rs is the wiring (the state, main(), generate_handler!'s list)

use super::*;

// What's selected, opened in the player (one window, as a playlist): videos and moments, and photos too, as a phone
// shows them (its live-photo plugin: the picture, its live part under the Live icon; a photo's link opens its clip at
// the frame). Done once the player plays the first one (Windows: it says so on its pipe; a clip on a NAS takes
// seconds), for the window to show it's waiting: false when it didn't within a while
#[tauri::command]
pub(crate) async fn open_items(paths: Vec<String>, journal: Option<String>) -> Result<bool, String> {
    if paths.is_empty() { return Ok(true); }
    let player = player()?;
    // The player's window may come to the front over this one (Windows gives the foreground only to what the app
    // the user clicked in starts, and the player is started by what this starts)
    #[cfg(windows)]
    foreground::allow_any();
    let Some(first) = paths.first().cloned() else { return Ok(true) };
    // The player told which action this is: its first frame is that action's result (journal.lua takes it)
    journal::handoff(journal.as_deref(), &paths);
    if cfg!(target_os = "macos") {
        Command::new("open").arg("-a").arg(&player).args(&paths).spawn().map_err(|e| e.to_string())?;
        return Ok(true);
    }
    Command::new(&player).args(&paths).spawn().map_err(|e| e.to_string())?;
    #[cfg(windows)]
    return tauri::async_runtime::spawn_blocking(move || player_pipe::wait_playing(&first, std::time::Duration::from_secs(60)))
        .await.map_err(|e| e.to_string());
    #[cfg(not(windows))]
    { let _ = first; Ok(true) }
}

// The list the window shows, as the player's playlist (the page's playList): an M3U file the player reads into its
// playlist, from the item opened (`at`); the player's mpv/scripts/workspace/librarylist.lua says how. Opened as a
// file is, so a running player takes it in place of what it plays (macOS: an Apple Event; Windows: the launcher sends
// it down the player's pipe), and thousands of items don't go on a command line. Done as open_items is
#[tauri::command]
pub(crate) async fn open_list(app: tauri::AppHandle, paths: Vec<String>, titles: Vec<String>, at: usize, label: String, journal: Option<String>) -> Result<bool, String> {
    let opened = paths.get(at).cloned().ok_or("Nothing to open")?;
    let Some(text) = list_m3u(&paths, &titles, at, &label) else { return open_items(vec![opened], journal).await };
    let dir = app.path().app_cache_dir().map_err(|e| e.to_string())?.join("lists");
    let file = write_list(&dir, &text, std::time::SystemTime::now())?;
    // The player told which action this is (journal.lua takes it as it opens the list, or the item)
    journal::handoff(journal.as_deref(), &[file.display().to_string(), opened.clone()]);
    let player = player()?;
    #[cfg(windows)]
    foreground::allow_any();
    if cfg!(target_os = "macos") {
        Command::new("open").arg("-a").arg(&player).arg(&file).spawn().map_err(|e| e.to_string())?;
        return Ok(true);
    }
    Command::new(&player).arg(&file).spawn().map_err(|e| e.to_string())?;
    #[cfg(windows)]
    return tauri::async_runtime::spawn_blocking(move || player_pipe::wait_playing(&opened, std::time::Duration::from_secs(60)))
        .await.map_err(|e| e.to_string());
    #[cfg(not(windows))]
    { let _ = opened; Ok(true) }
}

pub(crate) const LIST_EXT: &str = ".afplist.m3u8";

/// The list as an M3U (mpv reads it into its playlist): the label, the item opened, each item's name and path. A path
/// an M3U can't hold (a line break in it, spaces at its ends, a # first) is left out; None when that's the one opened
pub(crate) fn list_m3u(paths: &[String], titles: &[String], at: usize, label: &str) -> Option<String> {
    let one_line = |s: &str| s.replace(['\r', '\n'], " ");
    let fits = |p: &str| !p.is_empty() && !p.contains(['\r', '\n']) && p.trim() == p && !p.starts_with('#');
    if !paths.get(at).is_some_and(|p| fits(p)) { return None; }
    let start = paths[..at].iter().filter(|p| fits(p)).count();
    let mut out = format!("#EXTM3U\n#PLAYLIST:{}\n#AFP-START:{start}\n", one_line(label));
    for (i, p) in paths.iter().enumerate().filter(|(_, p)| fits(p)) {
        out += &format!("#EXTINF:-1,{}\n{p}\n", one_line(titles.get(i).map(String::as_str).unwrap_or("")));
    }
    Some(out)
}

/// The list written into `dir` under a name of its own (a player may still read the one before); lists older than a
/// day taken out
pub(crate) fn write_list(dir: &Path, text: &str, now: std::time::SystemTime) -> Result<PathBuf, String> {
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    for e in std::fs::read_dir(dir).into_iter().flatten().flatten() {
        let old = e.metadata().and_then(|m| m.modified()).ok()
            .is_some_and(|t| now.duration_since(t).is_ok_and(|d| d > std::time::Duration::from_secs(24 * 3600)));
        if old && e.file_name().to_string_lossy().ends_with(LIST_EXT) { let _ = std::fs::remove_file(e.path()); }
    }
    let ms = now.duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis()).unwrap_or(0);
    let mut file = dir.join(format!("{ms}{LIST_EXT}"));
    let mut n = 1;
    while file.exists() { file = dir.join(format!("{ms}-{n}{LIST_EXT}")); n += 1; }
    std::fs::write(&file, text).map_err(|e| e.to_string())?;
    Ok(file)
}

#[cfg(windows)]
mod foreground {
    const ASFW_ANY: u32 = u32::MAX;
    #[link(name = "user32")]
    unsafe extern "system" { fn AllowSetForegroundWindow(process: u32) -> i32; }
    pub fn allow_any() { unsafe { AllowSetForegroundWindow(ASFW_ANY); } }
}

// The Windows player's JSON IPC pipe (windows/launcher: \\.\pipe\ActionFootagePlayer), asked what it plays
#[cfg(windows)]
mod player_pipe {
    use std::io::{ BufRead, Write };
    use std::time::{ Duration, Instant };

    fn same(a: &str, b: &str) -> bool {
        let norm = |s: &str| s.replace('/', "\\").to_lowercase();
        norm(a) == norm(b)
    }

    // The path mpv plays, or None (no player yet, or it didn't answer)
    fn playing() -> Option<String> {
        let mut pipe = std::fs::OpenOptions::new().read(true).write(true).open(r"\\.\pipe\ActionFootagePlayer").ok()?;
        pipe.write_all(b"{\"command\":[\"get_property\",\"path\"],\"request_id\":1}\n").ok()?;
        for line in std::io::BufReader::new(pipe).lines().map_while(Result::ok) {
            let v: serde_json::Value = serde_json::from_str(&line).ok()?;
            if v.get("request_id").and_then(|i| i.as_u64()) == Some(1) {
                return v.get("data").and_then(|d| d.as_str()).map(String::from);
            }
        }
        None
    }

    // Until the player plays `video` (its first frame may still be on its way: the player says that itself)
    pub fn wait_playing(video: &str, wait: Duration) -> bool {
        let until = Instant::now() + wait;
        while Instant::now() < until {
            if playing().is_some_and(|p| same(&p, video)) { return true; }
            std::thread::sleep(Duration::from_millis(150));
        }
        false
    }
}

#[tauri::command]
pub(crate) fn reveal_path(path: String) {
    let p = PathBuf::from(&path);
    let _ = if cfg!(target_os = "macos") { Command::new("open").arg("-R").arg(&p).spawn() }
        else if cfg!(windows) { Command::new("explorer").arg(format!("/select,{}", p.display())).spawn() }
        else { Command::new("xdg-open").arg(p.parent().unwrap_or(&p)).spawn() };
}
