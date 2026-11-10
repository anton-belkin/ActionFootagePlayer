// SPDX-License-Identifier: GPL-3.0-or-later
//
// The programs running on this computer tell each other at once what they changed in a project: the catalog's journals
// are on the share, and reading them every few seconds is what this saves (docs/library-management.md §9, "Who tells
// the catalog"). In <cache>/live/, this computer's own folder:
//
// - library-<pid>.json  { pid, port }: an Action Footage Library listening on 127.0.0.1:<port>. Each player write goes
//   through `afp-catalog note`, which, once it's in the journal, sends the Library a line { project, files }: it reads
//   the journal at once and its page shows it.
// - player-<pid>.in     a player window's inbox: the Library appends a line { project, files } for each change of its
//   own; the player looks at the file's size four times a second (a local stat) and reads what's new.
//
// Nothing waits on these: a program not there, or gone without clearing its file, costs a failed connection or a line
// in a file nobody reads (and the file is cleared when its program is found gone).

use serde_json::Value;
use std::io::{ BufRead, Write };
use std::net::{ TcpListener, TcpStream };
use std::path::{ Path, PathBuf };
use std::time::Duration;

pub fn dir() -> PathBuf { crate::project::base_dirs().1.join("live") }

/// A Library listening: its file in dir() removed when dropped
pub struct Listening { file: PathBuf }

impl Drop for Listening {
    fn drop(&mut self) { let _ = std::fs::remove_file(&self.file); }
}

/// Listens for the lines other programs send (`on` hears each, parsed), on a thread of its own
pub fn listen(on: impl Fn(Value) + Send + Sync + 'static) -> std::io::Result<Listening> { listen_in(&dir(), on) }

fn listen_in(d: &Path, on: impl Fn(Value) + Send + Sync + 'static) -> std::io::Result<Listening> {
    let l = TcpListener::bind("127.0.0.1:0")?;
    let port = l.local_addr()?.port();
    std::fs::create_dir_all(d)?;
    let file = d.join(format!("library-{}.json", std::process::id()));
    let tmp = d.join(format!(".library-{}.json.tmp", std::process::id()));
    std::fs::write(&tmp, serde_json::json!({ "pid": std::process::id(), "port": port }).to_string())?;
    std::fs::rename(&tmp, &file)?;
    let on = std::sync::Arc::new(on);
    std::thread::Builder::new().name("live".into()).spawn(move || {
        for s in l.incoming().flatten() {
            let on = on.clone();
            std::thread::spawn(move || {
                let _ = s.set_read_timeout(Some(Duration::from_secs(2)));
                for line in std::io::BufReader::new(s).lines().map_while(Result::ok) {
                    if let Ok(v) = serde_json::from_str::<Value>(&line) { on(v); }
                }
            });
        }
    })?;
    Ok(Listening { file })
}

fn entries(d: &Path, prefix: &str, ext: &str) -> Vec<(PathBuf, u32)> {
    std::fs::read_dir(d).into_iter().flatten().flatten().filter_map(|e| {
        let n = e.file_name().to_string_lossy().into_owned();
        let pid = n.strip_prefix(prefix)?.strip_suffix(ext)?.parse::<u32>().ok()?;
        Some((e.path(), pid))
    }).collect()
}

/// Whether a process is there (Unix: signal 0; elsewhere taken as there)
fn alive(pid: u32) -> bool {
    #[cfg(unix)]
    { let r = unsafe { libc::kill(pid as libc::pid_t, 0) }; r == 0 || std::io::Error::last_os_error().raw_os_error() == Some(libc::EPERM) }
    #[cfg(not(unix))]
    { let _ = pid; true }
}

/// To every Library running here (what a player, through afp-catalog, wrote). How many heard it
pub fn tell_libraries(msg: &Value) -> usize { tell_libraries_in(&dir(), msg) }

fn tell_libraries_in(d: &Path, msg: &Value) -> usize {
    let line = format!("{msg}\n");
    let mut n = 0;
    for (file, pid) in entries(d, "library-", ".json") {
        let port = std::fs::read_to_string(&file).ok().and_then(|t| serde_json::from_str::<Value>(&t).ok()).and_then(|v| v["port"].as_u64());
        let sent = port.and_then(|p| TcpStream::connect_timeout(&([127, 0, 0, 1], p as u16).into(), Duration::from_millis(200)).ok())
            .is_some_and(|mut s| { let _ = s.set_write_timeout(Some(Duration::from_millis(200))); s.write_all(line.as_bytes()).is_ok() });
        if sent { n += 1; } else if !alive(pid) { let _ = std::fs::remove_file(&file); }
    }
    n
}

/// To every player window running here (what the Library changed): a line appended to each one's inbox. How many
pub fn tell_players(msg: &Value) -> usize { tell_players_in(&dir(), msg) }

fn tell_players_in(d: &Path, msg: &Value) -> usize {
    let line = format!("{msg}\n");
    let mut n = 0;
    for (file, pid) in entries(d, "player-", ".in") {
        if !alive(pid) { let _ = std::fs::remove_file(&file); continue; }
        let ok = std::fs::OpenOptions::new().append(true).open(&file).and_then(|mut f| f.write_all(line.as_bytes())).is_ok();
        if ok { n += 1; }
    }
    n
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn programs_tell_each_other() {
        let d = std::env::temp_dir().join(format!("afp-live-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        let (tx, rx) = std::sync::mpsc::channel();
        let tx = std::sync::Mutex::new(tx);
        let l = listen_in(&d, move |v| { let _ = tx.lock().unwrap().send(v); }).unwrap();
        let msg = serde_json::json!({ "project": "/p", "files": ["/p/a"] });
        assert_eq!(tell_libraries_in(&d, &msg), 1);
        assert_eq!(rx.recv_timeout(Duration::from_secs(2)).unwrap(), msg);
        // A player window's inbox: this process stands in for it; a gone one's is cleared
        let mine = d.join(format!("player-{}.in", std::process::id()));
        std::fs::write(&mine, "").unwrap();
        std::fs::write(d.join("player-999999.in"), "").unwrap();
        assert_eq!(tell_players_in(&d, &msg), 1);
        assert_eq!(std::fs::read_to_string(&mine).unwrap(), format!("{msg}\n"));
        assert!(!d.join("player-999999.in").exists());
        drop(l);
        assert_eq!(tell_libraries_in(&d, &msg), 0);
        let _ = std::fs::remove_dir_all(&d);
    }
}
