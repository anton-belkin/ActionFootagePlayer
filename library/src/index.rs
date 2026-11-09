// SPDX-License-Identifier: GPL-3.0-or-later
//
// What the Library shows of a project: its footage, moments, photos and exports, read from the project's own files
// each time (no database: the files are the truth), and what it knows of each clip from the clip itself (its camera,
// color, motion data, the camera's marks: gyroflow-probe --library, kept by that tool per file and here per run).
//
// The views:
//   footage  the project's videos, but its own folders (_Moments, _Clips, _Exports, _Stills, the old ones), hidden
//            ones and link files: each with its own file (<clip>.afp.json: marks, input, its view), whether it has a
//            Gyroflow project beside it, and its clip preview (_Clips/)
//   moments  _Moments/**.afplink.mp4, each with its link (the part of which clip, its own transform)
//   stills   _Stills/: photos (a Live Photo's .mov beside its still is the same photo)
//   exports  _Exports/: videos, with their link when they have one
// Paths in a view are its tree's: "Day 1/A.MP4" for footage, "Reef/Turtle.afplink.mp4" in moments.

use serde_json::{ json, Value };
use std::collections::{ HashMap, HashSet, VecDeque };
use std::fs::File;
use std::io::{ Read, Seek, SeekFrom };
use std::path::{ Path, PathBuf };
use std::process::{ Command, Stdio };
use std::sync::{ Arc, Mutex };

pub const LINK_EXT: &str = ".afplink.mp4";
const OWN_FOLDERS: [&str; 9] = ["_moments", "_clips", "_exports", "_stills", "moments", "clips", "exports", "stabilized", "_actionfootageplayer"];
const MAX_DEPTH: usize = 8;

pub fn view_dir(view: &str) -> Option<&'static str> {
    match view { "footage" => Some(""), "moments" => Some("_Moments"), "stills" => Some("_Stills"), "exports" => Some("_Exports"), _ => None }
}

fn unix(t: std::io::Result<std::time::SystemTime>) -> i64 {
    t.ok().and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map(|d| d.as_secs() as i64).unwrap_or(0)
}

fn rel_join(dir: &str, name: &str) -> String { if dir.is_empty() { name.to_string() } else { format!("{dir}/{name}") } }

/// Every file and folder under `base` (not hidden), as (path in the tree, is a folder, size, modified), folders kept
/// out by `skip` (path in the tree, lower case name)
fn walk(base: &Path, skip: &dyn Fn(&str, &str) -> bool) -> Vec<(String, bool, u64, i64)> {
    let mut out = Vec::new();
    let mut todo = VecDeque::from([(base.to_path_buf(), String::new(), 0usize)]);
    while let Some((dir, rel, depth)) = todo.pop_front() {
        let Ok(rd) = std::fs::read_dir(&dir) else { continue };
        let mut entries: Vec<_> = rd.flatten().collect();
        entries.sort_by_key(|e| e.file_name());
        for e in entries {
            let name = e.file_name().to_string_lossy().into_owned();
            if name.starts_with('.') || name.starts_with('#') || name.starts_with("~~") { continue; }
            let Ok(meta) = e.metadata() else { continue };
            let r = rel_join(&rel, &name);
            if meta.is_dir() {
                if skip(&r, &name.to_lowercase()) { continue; }
                out.push((r.clone(), true, 0, unix(meta.modified())));
                if depth + 1 < MAX_DEPTH { todo.push_back((e.path(), r, depth + 1)); }
            } else {
                out.push((r, false, meta.len(), unix(meta.modified())));
            }
        }
    }
    out
}

fn parent_of(rel: &str) -> String { rel.rsplit_once('/').map(|(d, _)| d.to_string()).unwrap_or_default() }
fn name_of(rel: &str) -> &str { rel.rsplit_once('/').map(|(_, n)| n).unwrap_or(rel) }

// ---------- MP4 boxes: the link key and the cover ----------

fn be32(s: &[u8], i: usize) -> Option<u32> { s.get(i..i + 4).map(|b| u32::from_be_bytes([b[0], b[1], b[2], b[3]])) }

fn file_boxes(f: &mut File, first: u64, last: u64) -> Vec<([u8; 4], u64, u64)> {
    let mut out = Vec::new();
    let mut pos = first;
    while pos + 8 <= last {
        let mut head = [0u8; 16];
        if f.seek(SeekFrom::Start(pos)).is_err() { break; }
        let n = f.read(&mut head).unwrap_or(0);
        if n < 8 { break; }
        let mut size = be32(&head, 0).unwrap() as u64;
        let mut header = 8;
        if size == 1 && n >= 16 { size = u64::from_be_bytes(head[8..16].try_into().unwrap()); header = 16; }
        else if size == 0 { size = last - pos; }
        if size < header || pos + size > last { break; }
        out.push((head[4..8].try_into().unwrap(), pos + header, pos + size));
        pos += size;
    }
    out
}

fn mem_boxes(s: &[u8], first: usize, last: usize) -> Vec<([u8; 4], usize, usize)> {
    let mut out = Vec::new();
    let mut pos = first;
    while pos + 8 <= last {
        let Some(size) = be32(s, pos).map(|v| v as usize) else { break };
        if size < 8 || pos + size > last { break; }
        out.push((s[pos + 4..pos + 8].try_into().unwrap(), pos + 8, pos + size));
        pos += size;
    }
    out
}

/// The moov box's body, read whole (a preview's is some hundred KB with its cover), None when too big or missing
fn moov(path: &Path) -> Option<Vec<u8>> {
    let mut f = File::open(path).ok()?;
    let size = f.metadata().ok()?.len();
    let (_, a, b) = file_boxes(&mut f, 0, size).into_iter().find(|(t, _, _)| t == b"moov")?;
    if b - a > 32 * 1024 * 1024 { return None; }
    let mut buf = vec![0u8; (b - a) as usize];
    f.seek(SeekFrom::Start(a)).ok()?;
    f.read_exact(&mut buf).ok()?;
    Some(buf)
}

// A meta box's body: its handler and its boxes (with version and flags before them, or without, Apple's way)
fn meta_parts(s: &[u8], a: usize, b: usize) -> (Option<[u8; 4]>, Vec<([u8; 4], usize, usize)>) {
    let first = if s.get(a + 4..a + 8) == Some(b"hdlr") { a } else { a + 4 };
    let parts = mem_boxes(s, first, b);
    let handler = parts.iter().find(|(t, _, _)| t == b"hdlr").and_then(|(_, ha, _)| s.get(ha + 8..ha + 12)).map(|h| h.try_into().unwrap());
    (handler, parts)
}

/// The mdta keys of an MP4 (moov/meta, then moov/udta/meta), and its cover (moov/udta/meta/ilst/covr): JPEG or PNG
pub fn keys_and_cover(path: &Path) -> (HashMap<String, String>, Option<Vec<u8>>) {
    let mut keys = HashMap::new();
    let mut cover = None;
    let Some(s) = moov(path) else { return (keys, cover) };
    let mut metas = Vec::new();
    for (t, a, b) in mem_boxes(&s, 0, s.len()) {
        if &t == b"meta" { metas.insert(0, (a, b)); }
        if &t == b"udta" { for (ut, ua, ub) in mem_boxes(&s, a, b) { if &ut == b"meta" { metas.push((ua, ub)); } } }
    }
    for (a, b) in metas {
        let (handler, parts) = meta_parts(&s, a, b);
        let ilst = parts.iter().find(|(t, _, _)| t == b"ilst").map(|(_, ia, ib)| (*ia, *ib));
        let Some((ia, ib)) = ilst else { continue };
        if handler.as_ref() == Some(b"mdta") {
            let Some((_, ka, kb)) = parts.iter().find(|(t, _, _)| t == b"keys") else { continue };
            let mut names = Vec::new();
            let n = be32(&s, ka + 4).unwrap_or(0);
            let mut pos = ka + 8;
            for _ in 0..n {
                let Some(size) = be32(&s, pos).map(|v| v as usize) else { break };
                if size < 8 || pos + size > *kb { break; }
                names.push(String::from_utf8_lossy(&s[pos + 8..pos + size]).into_owned());
                pos += size;
            }
            for (t, a, b) in mem_boxes(&s, ia, ib) {
                let idx = u32::from_be_bytes(t) as usize;
                let Some(name) = idx.checked_sub(1).and_then(|i| names.get(i)) else { continue };
                for (dt, da, db) in mem_boxes(&s, a, b) {
                    if &dt == b"data" && db - da >= 8 && be32(&s, da) == Some(1) && !keys.contains_key(name) {
                        keys.insert(name.clone(), String::from_utf8_lossy(&s[da + 8..db]).into_owned());
                    }
                }
            }
        } else {
            for (t, a, b) in mem_boxes(&s, ia, ib) {
                if &t != b"covr" || cover.is_some() { continue; }
                for (dt, da, db) in mem_boxes(&s, a, b) {
                    if &dt == b"data" && db - da > 8 { cover = Some(s[da + 8..db].to_vec()); break; }
                }
            }
        }
    }
    (keys, cover)
}

pub fn link(path: &Path) -> Option<Value> {
    // Kept by the file's size and time: a folder of them is read again on each look
    static LINKS: Mutex<Option<HashMap<PathBuf, (u64, i64, Option<Value>)>>> = Mutex::new(None);
    let st = stamp(&path.to_string_lossy());
    if let Some(st) = st {
        if let Some((s, m, v)) = LINKS.lock().unwrap().get_or_insert_with(HashMap::new).get(path) {
            if (*s, *m) == st { return v.clone(); }
        }
    }
    let v = keys_and_cover(path).0.get("ch.actionfootageplayer.link").and_then(|t| serde_json::from_str::<Value>(t).ok());
    if let Some((s, m)) = st { LINKS.lock().unwrap().get_or_insert_with(HashMap::new).insert(path.to_path_buf(), (s, m, v.clone())); }
    v
}

/// The clip previews in _Clips/, wherever they were moved in it: the clip each links to (its path in the project) ->
/// the preview
fn previews(root: &Path) -> HashMap<String, PathBuf> {
    let base = root.join("_Clips");
    let mut out = HashMap::new();
    for (rel, is_dir, _, _) in walk(&base, &|_, _| false) {
        if is_dir || !rel.to_lowercase().ends_with(LINK_EXT) { continue; }
        let p = base.join(&rel);
        if let Some(c) = link(&p).and_then(|l| l.get("clip").and_then(Value::as_str).map(str::to_string)) { out.entry(c).or_insert(p); }
    }
    out
}

// ---------- a clip's own files ----------

/// A clip preview's path in _Clips/, from the clip's (the camera's own folders, DCIM and the one in it, left out),
/// as the workspace plugin's moments.lua has it. `flat`: all in _Clips/ itself
pub fn clip_preview_relative(relative: &str, flat: bool) -> String {
    let stem = |n: &str| n.rsplit_once('.').map(|(s, _)| s.to_string()).unwrap_or_else(|| n.to_string());
    if flat { return format!("{}{LINK_EXT}", stem(name_of(relative))); }
    let parts: Vec<&str> = relative.split('/').collect();
    let mut out: Vec<String> = Vec::new();
    let mut i = 0;
    while i < parts.len() {
        if parts[i].eq_ignore_ascii_case("DCIM") && i + 2 < parts.len() { i += 2; } else { out.push(parts[i].to_string()); i += 1; }
    }
    if let Some(last) = out.last_mut() { *last = format!("{}{LINK_EXT}", stem(last)); }
    out.join("/")
}

fn read_json(path: &Path) -> Option<Value> {
    std::fs::read_to_string(path).ok().and_then(|t| serde_json::from_str(&t).ok())
}

/// A clip's own file: <clip>.afp.json, or the older <clip>.marks.json
fn own_file(clip: &Path) -> Option<Value> {
    let stem = clip.file_stem()?.to_string_lossy().into_owned();
    let dir = clip.parent()?;
    read_json(&dir.join(format!("{stem}.afp.json"))).or_else(|| read_json(&dir.join(format!("{stem}.marks.json"))))
}

fn has_gyroflow(file: &Path) -> bool {
    let Some(dir) = file.parent() else { return false };
    let name = file.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let stem = if let Some(s) = name.strip_suffix(LINK_EXT) { s.to_string() } else { file.file_stem().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default() };
    dir.join(format!("{stem}.gyroflow")).is_file() || dir.join(format!("{name}.gyroflow")).is_file()
}

// ---------- the views ----------

pub fn scan(root: &Path, view: &str, flat_previews: bool) -> Value {
    let Some(sub) = view_dir(view) else { return json!({ "dirs": [], "items": [] }) };
    let base = if sub.is_empty() { root.to_path_buf() } else { root.join(sub) };
    let skip = |rel: &str, lower: &str| view == "footage" && !rel.contains('/') && OWN_FOLDERS.contains(&lower);
    let entries = walk(&base, &skip);
    let mut dirs = Vec::new();
    let mut items = Vec::new();
    let files: HashSet<String> = entries.iter().filter(|e| !e.1).map(|e| e.0.to_lowercase()).collect();
    let linked = if view == "footage" { previews(root) } else { HashMap::new() };
    for (rel, is_dir, size, mtime) in &entries {
        if *is_dir { dirs.push(json!(rel)); continue; }
        let name = name_of(rel);
        let lower = name.to_lowercase();
        let path = base.join(rel);
        let mut it = json!({ "id": rel, "dir": parent_of(rel), "name": name, "path": path.display().to_string(), "size": size, "mtime": mtime });
        match view {
            "footage" => {
                if !(lower.ends_with(".mp4") || lower.ends_with(".mov")) || lower.ends_with(LINK_EXT) { continue; }
                it["kind"] = json!("clip");
                if let Some(own) = own_file(&path) {
                    it["own"] = json!({ "marks": own.get("marks").cloned().unwrap_or(Value::Null), "view": own.get("view").cloned().unwrap_or(Value::Null),
                                        "source": own.get("source").cloned().unwrap_or(Value::Null) });
                }
                it["gyroflow"] = json!(has_gyroflow(&path));
                let preview = linked.get(rel.as_str()).cloned().or_else(|| [clip_preview_relative(rel, flat_previews), clip_preview_relative(rel, !flat_previews)]
                    .into_iter().map(|p| root.join("_Clips").join(p)).find(|p| p.is_file()));
                it["preview"] = preview.map(|p| json!(p.display().to_string())).unwrap_or(Value::Null);
            }
            "moments" => {
                if !lower.ends_with(LINK_EXT) { continue; }
                it["kind"] = json!("moment");
                it["name"] = json!(&name[..name.len() - LINK_EXT.len()]);
                it["link"] = link(&path).unwrap_or(Value::Null);
                it["gyroflow"] = json!(has_gyroflow(&path));
            }
            "stills" => {
                let photo = [".heic", ".jpg", ".jpeg", ".png"].iter().any(|e| lower.ends_with(e));
                if !photo { continue; }
                it["kind"] = json!("still");
                let stem = rel.rsplit_once('.').map(|(s, _)| s).unwrap_or(rel).to_lowercase();
                it["form"] = json!(if lower.ends_with(".mp.jpg") { "Motion Photo" }
                    else if files.contains(&format!("{stem}.mov")) { "Live Photo" }
                    else if lower.ends_with(".heic") { "HEIC with gain map" } else { "Ultra HDR JPEG" });
            }
            "exports" => {
                if !(lower.ends_with(".mov") || lower.ends_with(".mp4")) { continue; }
                it["kind"] = json!("export");
                it["link"] = link(&path).unwrap_or(Value::Null);
            }
            _ => {}
        }
        items.push(it);
    }
    json!({ "dirs": dirs, "items": items })
}

// ---------- what each clip is: gyroflow-probe --library, in the background ----------

#[derive(Default)]
pub struct Probes {
    known: HashMap<String, (u64, i64, Value)>,
    queue: VecDeque<String>,
    queued: HashSet<String>,
    log: Vec<(u64, String)>,
    version: u64,
    pub tool: Option<PathBuf>,
    pub busy: bool,
}

pub type SharedProbes = Arc<Mutex<Probes>>;

fn stamp(path: &str) -> Option<(u64, i64)> {
    let m = std::fs::metadata(path).ok()?;
    Some((m.len(), unix(m.modified())))
}

impl Probes {
    /// Asked for these clips: the ones not known (or changed since) are queued
    pub fn want(&mut self, paths: &[String]) {
        for p in paths {
            let fresh = match (self.known.get(p), stamp(p)) { (Some((s, m, _)), Some((s2, m2))) => *s == s2 && *m == m2, _ => false };
            if !fresh && self.queued.insert(p.clone()) { self.queue.push_back(p.clone()); }
        }
    }
    /// What's known that changed after `since`: (version now, path -> answer, how many wait)
    pub fn since(&self, since: u64) -> Value {
        let mut out = serde_json::Map::new();
        for (v, p) in self.log.iter().rev() {
            if *v <= since { break; }
            if !out.contains_key(p) { if let Some((_, _, val)) = self.known.get(p) { out.insert(p.clone(), val.clone()); } }
        }
        json!({ "version": self.version, "items": out, "pending": self.queue.len() + usize::from(self.busy) })
    }
    pub fn known(&self, paths: &[String]) -> Value {
        let mut out = serde_json::Map::new();
        for p in paths { if let Some((_, _, v)) = self.known.get(p) { out.insert(p.clone(), v.clone()); } }
        json!({ "version": self.version, "items": out, "pending": self.queue.len() + usize::from(self.busy) })
    }
}

/// The worker: one clip at a time, the tool run as a process (it's the player's own; its answers kept per file)
pub fn probe_worker(probes: SharedProbes) {
    loop {
        let next = {
            let mut p = probes.lock().unwrap();
            let n = p.queue.pop_front();
            if let Some(path) = &n { p.queued.remove(path); }
            p.busy = n.is_some();
            n.zip(p.tool.clone())
        };
        let Some((path, tool)) = next else {
            std::thread::sleep(std::time::Duration::from_millis(250));
            continue;
        };
        let stamp_before = stamp(&path);
        let mut cmd = Command::new(&tool);
        cmd.arg("--library").arg(&path).stdin(Stdio::null()).stderr(Stdio::null());
        #[cfg(windows)]
        { use std::os::windows::process::CommandExt; cmd.creation_flags(0x0800_0000); }
        let answer = cmd.output().ok().and_then(|o| String::from_utf8(o.stdout).ok())
            .and_then(|t| t.lines().last().and_then(|l| serde_json::from_str::<Value>(l).ok()))
            .unwrap_or_else(|| json!({ "mode": null, "reason": "couldn't be read" }));
        let mut p = probes.lock().unwrap();
        if let Some((s, m)) = stamp_before {
            p.version += 1;
            let v = p.version;
            p.known.insert(path.clone(), (s, m, answer));
            p.log.push((v, path));
        }
        p.busy = false;
    }
}

/// The probe tool: AFL_PROBE, else the player's (its plugins' folder)
pub fn probe_tool(player: Option<&Path>) -> Option<PathBuf> {
    if let Some(p) = std::env::var_os("AFL_PROBE").filter(|p| !p.is_empty()) { return Some(PathBuf::from(p)); }
    let player = player?;
    let candidates = if cfg!(target_os = "macos") {
        let app = if player.extension().is_some_and(|e| e == "app") { player.to_path_buf() } else { player.ancestors().nth(3)?.to_path_buf() };
        vec![app.join("Contents/Resources/plugins/gyroflow/gyroflow-probe")]
    } else {
        let dir = player.parent()?.to_path_buf();
        vec![dir.join("plugins/gyroflow/gyroflow-probe.exe"), dir.join("portable_config/scripts/gyroflow/gyroflow-probe.exe")]
    };
    candidates.into_iter().find(|p| p.is_file())
}

// ---------- pictures for the browser ----------

fn fnv(s: &str) -> String {
    let mut h: u64 = 0xcbf29ce484222325;
    for b in s.bytes() { h ^= b as u64; h = h.wrapping_mul(0x100000001b3); }
    format!("{h:016x}")
}

pub fn base64(data: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    for c in data.chunks(3) {
        let n = (c[0] as u32) << 16 | (*c.get(1).unwrap_or(&0) as u32) << 8 | *c.get(2).unwrap_or(&0) as u32;
        for i in 0..4 {
            if i <= c.len() { out.push(T[(n >> (18 - 6 * i) & 63) as usize] as char); } else { out.push('='); }
        }
    }
    out
}

fn data_url(bytes: &[u8]) -> String {
    let mime = if bytes.starts_with(b"\x89PNG") { "image/png" } else { "image/jpeg" };
    format!("data:{mime};base64,{}", base64(bytes))
}

/// A small picture of a file, as a data: URL: an MP4's cover (a moment's, a clip preview's), else (macOS) one made
/// by the system (sips for photos, Quick Look for videos), kept in `cache`. None: none to show
pub fn thumb(path: &Path, cache: &Path) -> Option<String> {
    let (size, mtime) = stamp(&path.to_string_lossy())?;
    let key = fnv(&format!("{}|{size}|{mtime}", path.display()));
    let file = cache.join(format!("{key}.img"));
    if let Ok(b) = std::fs::read(&file) { return (!b.is_empty()).then(|| data_url(&b)); }
    let _ = std::fs::create_dir_all(cache);
    let lower = path.to_string_lossy().to_lowercase();
    let mut bytes: Option<Vec<u8>> = None;
    if lower.ends_with(".mp4") || lower.ends_with(".mov") {
        bytes = keys_and_cover(path).1;
    }
    if bytes.is_none() && cfg!(target_os = "macos") {
        let out = cache.join(format!("{key}.jpg"));
        if [".heic", ".jpg", ".jpeg", ".png"].iter().any(|e| lower.ends_with(e)) {
            let ok = Command::new("/usr/bin/sips").args(["-s", "format", "jpeg", "-Z", "360"]).arg(path).arg("--out").arg(&out)
                .stdout(Stdio::null()).stderr(Stdio::null()).status().is_ok_and(|s| s.success());
            if ok { bytes = std::fs::read(&out).ok(); }
        } else if lower.ends_with(".mov") || lower.ends_with(".mp4") {
            let dir = cache.join(format!("{key}.ql"));
            let _ = std::fs::create_dir_all(&dir);
            let _ = Command::new("/usr/bin/qlmanage").args(["-t", "-s", "360", "-o"]).arg(&dir).arg(path)
                .stdout(Stdio::null()).stderr(Stdio::null()).status();
            bytes = std::fs::read_dir(&dir).ok().and_then(|mut r| r.next()).and_then(|e| e.ok()).and_then(|e| std::fs::read(e.path()).ok());
            let _ = std::fs::remove_dir_all(&dir);
        }
        let _ = std::fs::remove_file(&out);
    }
    let _ = std::fs::write(&file, bytes.as_deref().unwrap_or(&[]));
    bytes.map(|b| data_url(&b))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preview_paths() {
        assert_eq!(clip_preview_relative("Day 1/A.MP4", false), "Day 1/A.afplink.mp4");
        assert_eq!(clip_preview_relative("DCIM/100MEDIA/GX01.MP4", false), "GX01.afplink.mp4");
        assert_eq!(clip_preview_relative("Card/DCIM/DJI_001/D.MP4", false), "Card/D.afplink.mp4");
        assert_eq!(clip_preview_relative("Day 1/A.MP4", true), "A.afplink.mp4");
    }

    #[test]
    fn base64_as_rfc4648() {
        assert_eq!(base64(b""), "");
        assert_eq!(base64(b"f"), "Zg==");
        assert_eq!(base64(b"fo"), "Zm8=");
        assert_eq!(base64(b"foo"), "Zm9v");
        assert_eq!(base64(b"foobar"), "Zm9vYmFy");
    }

    #[test]
    fn views_of_a_project() {
        let root = std::env::temp_dir().join(format!("afl-index-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        for d in ["Day 1", "_Moments/Reef", "_Clips/Day 1", "_Stills", "_Exports", ".hidden"] { std::fs::create_dir_all(root.join(d)).unwrap(); }
        for f in ["Day 1/A.MP4", "Day 1/A.LRF", "Day 1/B.mov", "_Moments/Reef/Turtle.afplink.mp4", "_Clips/Day 1/A.afplink.mp4",
                  "_Stills/A 1.heic", "_Stills/A 1.mov", "_Stills/B.MP.jpg", "_Exports/T.mov", ".hidden/X.MP4", "Project.afproject"] {
            std::fs::write(root.join(f), b"x").unwrap();
        }
        std::fs::write(root.join("Day 1/A.afp.json"), br#"{"marks":[{"t":1.5}],"view":{"view":"wide"}}"#).unwrap();
        std::fs::write(root.join("Day 1/A.gyroflow"), b"{}").unwrap();
        let f = scan(&root, "footage", false);
        let ids: Vec<&str> = f["items"].as_array().unwrap().iter().map(|i| i["id"].as_str().unwrap()).collect();
        assert_eq!(ids, ["Day 1/A.MP4", "Day 1/B.mov"]);
        assert_eq!(f["dirs"], json!(["Day 1"]));
        let a = &f["items"][0];
        assert_eq!(a["own"]["marks"][0]["t"], json!(1.5));
        assert_eq!(a["own"]["view"]["view"], json!("wide"));
        assert_eq!(a["gyroflow"], json!(true));
        assert!(a["preview"].as_str().unwrap().ends_with("_Clips/Day 1/A.afplink.mp4"));
        let m = scan(&root, "moments", false);
        assert_eq!(m["items"][0]["name"], json!("Turtle"));
        assert_eq!(m["items"][0]["dir"], json!("Reef"));
        let s = scan(&root, "stills", false);
        let forms: Vec<&str> = s["items"].as_array().unwrap().iter().map(|i| i["form"].as_str().unwrap()).collect();
        assert_eq!(forms, ["Live Photo", "Motion Photo"]);
        assert_eq!(scan(&root, "exports", false)["items"].as_array().unwrap().len(), 1);
        let _ = std::fs::remove_dir_all(&root);
    }
}

// The views of a project and its clips' probes, written as JSON for looking at the page outside the app:
// AFL_DUMP=<project> AFL_DUMP_OUT=<file> cargo test --release -- --ignored dump_views
#[cfg(test)]
mod dump {
    #[test]
    #[ignore]
    fn dump_views() {
        let (Some(root), Some(out)) = (std::env::var_os("AFL_DUMP"), std::env::var_os("AFL_DUMP_OUT")) else { return };
        let root = std::path::PathBuf::from(root);
        let tool = super::probe_tool(std::env::var_os("AFL_PLAYER").map(std::path::PathBuf::from).as_deref());
        let mut all = serde_json::Map::new();
        let mut probes = serde_json::Map::new();
        for view in ["footage", "moments", "stills", "exports"] {
            let v = super::scan(&root, view, false);
            for it in v["items"].as_array().into_iter().flatten() {
                let p = match it["kind"].as_str() {
                    Some("clip") => it["path"].as_str().map(str::to_string),
                    Some("moment") => it["link"]["clip"].as_str().map(|c| root.join(c).display().to_string()),
                    _ => None,
                };
                if let (Some(p), Some(t)) = (p, tool.as_ref()) {
                    if probes.contains_key(&p) { continue; }
                    let o = std::process::Command::new(t).arg("--library").arg(&p).output().unwrap();
                    if let Ok(v) = serde_json::from_slice::<serde_json::Value>(&o.stdout) { probes.insert(p, v); }
                }
            }
            all.insert(view.into(), v);
        }
        all.insert("probes".into(), serde_json::Value::Object(probes));
        all.insert("root".into(), serde_json::json!(root.display().to_string()));
        std::fs::write(out, serde_json::to_vec(&all).unwrap()).unwrap();
    }
}
