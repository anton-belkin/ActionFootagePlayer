// SPDX-License-Identifier: GPL-3.0-or-later
//
// What the Library shows of a project: its footage, moments, photos and exports, read from the project's own files
// each time (no database: the files are the truth), and what it knows of each clip from the clip itself (its camera,
// color, motion data, the camera's marks: gyroflow-probe --library, kept by that tool per file and here per run).
//
// The views:
//   footage  the project's videos, but its own folders (_Moments, _Clips, Generated…, the old ones), hidden ones and
//            link files: each with its own file (<clip>.afp.json: marks, input, its view), whether it has a Gyroflow
//            project beside it, and its clip preview (wherever it is in the project's folder)
//   moments  the moments, each with its link (the part of which clip, its own transform)
//   stills   the photos (a Live Photo's .mov beside its still is the same photo)
//   exports  the exports, with their link
// Since 0.11 (docs/library-management.md §8) the project's own files are wherever they were put in its folder, found
// by the link inside: Generated/<date> <time> <what>/ where a run made them, _Moments/ and the like from before, any
// folder of one's own. Paths in footage are its tree's ("Day 1/A.MP4"); in the others the project folder's
// ("Generated/2026-11-10 14-32 Moments/Turtle.afplink.mp4", "_Moments/Reef/Turtle.afplink.mp4").

use serde_json::{ json, Value };
use std::collections::{ BTreeMap, BTreeSet, HashMap, HashSet, VecDeque };
use std::fs::File;
use std::io::{ Read, Seek, SeekFrom };
use std::path::{ Path, PathBuf };
use std::process::{ Command, Stdio };
use std::sync::{ Arc, Mutex };

pub const LINK_EXT: &str = ".afplink.mp4";
/// A project's own folder, its name lower case: registry.json's own_folders, and the NAS's recycle bins
pub fn own_folder(lower: &str) -> bool { crate::lists::get().own_folder(lower, true) }
/// Where what's made goes since 0.11 (docs/library-management.md §8): Generated/<date> <time> <what>/
pub const GENERATED: &str = "Generated";

/// The footage's folders inside the project's own folder (a project next to its footage): its locations there
pub fn footage_dirs(root: &Path) -> Vec<PathBuf> {
    crate::locations::locations(root, &crate::locations::mounts()).into_iter().filter_map(|l| l.dir).filter(|d| d.starts_with(root)).collect()
}

/// Whether a path in the project's folder is the project's own (its files can be anything there): in one of its own
/// folders at its top (_Moments, Generated…), or anywhere that isn't among its footage
pub fn own_area(root: &Path, path: &Path, footage: &[PathBuf]) -> bool {
    let Ok(rel) = path.strip_prefix(root) else { return false };
    let top = rel.components().next().map(|c| c.as_os_str().to_string_lossy().to_lowercase()).unwrap_or_default();
    if top.starts_with('_') || own_folder(&top) { return true; }
    !footage.iter().any(|d| path.starts_with(d))
}

/// The project's own files of a view's kind, wherever they are in its folder (docs/library-management.md §8): every
/// file of the project's folder that `keep` takes (its path in the project, its full path, whether it's in the own
/// area), and the folders they're in, with the empty folders (a folder just made, to move things into). Among the footage only `.afplink.mp4` files are offered (not every clip read)
fn own_files(root: &Path, keep: &mut dyn FnMut(&str, &Path, bool) -> bool) -> (Vec<(String, u64, i64)>, Vec<String>) {
    let footage = footage_dirs(root);
    let mut files = Vec::new();
    let mut dirs: Vec<String> = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();
    let entries = walk(root, &|_, lower| lower == "#recycle");
    // Folders with anything in them (a file, or a folder: the walk lists a folder before what's in it)
    let full: HashSet<String> = entries.iter().map(|e| parent_of(&e.0)).collect();
    for (rel, is_dir, _, _) in &entries {
        // An empty folder holds no footage: shown wherever it is (made here to move things into)
        if *is_dir && !full.contains(rel) {
            let mut d = rel.clone();
            while !d.is_empty() && seen.insert(d.clone()) { dirs.push(d.clone()); d = parent_of(&d); }
        }
    }
    for (rel, is_dir, size, mtime) in entries {
        if is_dir { continue; }
        let path = root.join(&rel);
        let own = own_area(root, &path, &footage);
        if !own && !rel.to_lowercase().ends_with(LINK_EXT) { continue; }
        if !keep(&rel, &path, own) { continue; }
        let mut d = parent_of(&rel);
        while !d.is_empty() && seen.insert(d.clone()) { dirs.push(d.clone()); d = parent_of(&d); }
        files.push((rel, size, mtime));
    }
    dirs.sort();
    (files, dirs)
}
const MAX_DEPTH: usize = 8;

fn unix(t: std::io::Result<std::time::SystemTime>) -> i64 {
    t.ok().and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map(|d| d.as_secs() as i64).unwrap_or(0)
}

fn rel_join(dir: &str, name: &str) -> String { if dir.is_empty() { name.to_string() } else { format!("{dir}/{name}") } }

/// Every file and folder under `base` (not hidden), as (path in the tree, is a folder, size, modified), folders kept
/// out by `skip` (path in the tree, lower case name)
pub(crate) fn walk(base: &Path, skip: &dyn Fn(&str, &str) -> bool) -> Vec<(String, bool, u64, i64)> {
    // As the project's catalog last saw it (seen.rs): no question to the disk for each file
    if let Some(v) = crate::seen::walk(base, skip, MAX_DEPTH) { return v; }
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

/// A video's own creation time (moov/mvhd), unix seconds: what ffmpeg and mpv show as creation_time and links keep as
/// `created`. Written by the camera inside the file, so a copy, a move or a restore from a backup keeps it
pub fn created(path: &Path) -> Option<i64> {
    let mut f = File::open(path).ok()?;
    let size = f.metadata().ok()?.len();
    let (_, a, b) = file_boxes(&mut f, 0, size).into_iter().find(|(t, _, _)| t == b"moov")?;
    let (_, ma, _) = file_boxes(&mut f, a, b).into_iter().find(|(t, _, _)| t == b"mvhd")?;
    let mut h = [0u8; 12];
    f.seek(SeekFrom::Start(ma)).ok()?;
    f.read_exact(&mut h).ok()?;
    let t = if h[0] == 1 { u64::from_be_bytes(h[4..12].try_into().unwrap()) } else { u32::from_be_bytes(h[4..8].try_into().unwrap()) as u64 };
    (t > 0).then(|| t as i64 - 2_082_844_800)
}

/// A link's `created` ("2026-10-22T13:28:42.000000Z", "2026-10-22T15:28:42+02:00"), unix seconds
pub fn parse_time(s: &str) -> Option<i64> {
    let b = s.as_bytes();
    if b.len() < 19 { return None; }
    let n = |r: std::ops::Range<usize>| s.get(r).and_then(|x| x.parse::<i64>().ok());
    let (y, mo, d, h, mi, se) = (n(0..4)?, n(5..7)?, n(8..10)?, n(11..13)?, n(14..16)?, n(17..19)?);
    let mut rest = &s[19..];
    if rest.starts_with('.') { rest = rest.trim_start_matches(|c: char| c == '.' || c.is_ascii_digit()); }
    let offset = match rest {
        "" | "Z" | "z" => 0,
        r => {
            let sign = if r.starts_with('-') { -1 } else if r.starts_with('+') { 1 } else { return None };
            let digits: String = r[1..].chars().filter(|c| c.is_ascii_digit()).collect();
            if digits.len() != 4 { return None; }
            sign * (digits[0..2].parse::<i64>().ok()? * 60 + digits[2..4].parse::<i64>().ok()?)
        }
    };
    // Days from 1970-01-01 (Howard Hinnant's days_from_civil)
    let yy = if mo <= 2 { y - 1 } else { y };
    let era = if yy >= 0 { yy } else { yy - 399 } / 400;
    let yoe = yy - era * 400;
    let doy = (153 * (if mo > 2 { mo - 3 } else { mo + 9 }) + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146097 + doe - 719468;
    Some(days * 86400 + h * 3600 + mi * 60 + se - offset * 60)
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

/// A photo's link to its clip: `afp:link` in its XMP (afp-catalog's xmp.rs, the player's namespace,
/// https://actionfootageplayer.ch/ns/1.0/)
fn photo_link(path: &Path) -> Option<Value> {
    let xml = afp_catalog::xmp::of_file(path).ok()??;
    serde_json::from_str(&afp_catalog::xmp::link(&xml)?).ok()
}

/// A photo's tags: `dc:subject` in its XMP (vs-gyroflow's afp-meta writes them), which Photos and Lightroom read as
/// keywords
pub fn photo_tags(path: &Path) -> Vec<String> {
    afp_catalog::xmp::of_file(path).ok().flatten().map(|x| afp_catalog::xmp::tags(&x)).unwrap_or_default()
}

pub fn link(path: &Path) -> Option<Value> {
    // As the catalog last read it
    if let Some(Some(f)) = crate::seen::file(path) {
        return f.meta.and_then(|m| m.get("link").cloned()).filter(|l| !l.is_null());
    }
    // Kept by the file's size and time: a folder of them is read again on each look
    static LINKS: Mutex<Option<HashMap<PathBuf, (u64, i64, Option<Value>)>>> = Mutex::new(None);
    let st = stamp(&path.to_string_lossy());
    if let Some(st) = st {
        if let Some((s, m, v)) = LINKS.lock().unwrap().get_or_insert_with(HashMap::new).get(path) {
            if (*s, *m) == st { return v.clone(); }
        }
    }
    let lower = path.to_string_lossy().to_lowercase();
    let v = if [".heic", ".jpg", ".jpeg"].iter().any(|e| lower.ends_with(e)) { photo_link(path) }
        else { keys_and_cover(path).0.get("ch.actionfootageplayer.link").and_then(|t| serde_json::from_str::<Value>(t).ok()) };
    if let Some((s, m)) = st { LINKS.lock().unwrap().get_or_insert_with(HashMap::new).insert(path.to_path_buf(), (s, m, v.clone())); }
    v
}

/// The clip previews, wherever they are in the project's folder (found by the link inside, kind clip): the clip each
/// links to (its path in the project) -> the preview; and the folders they're in (paths in the project)
fn previews(root: &Path) -> (HashMap<String, PathBuf>, Vec<String>) {
    let mut out = HashMap::new();
    let (_, dirs) = own_files(root, &mut |rel, path, _| {
        let l = rel.to_lowercase();
        if !(l.ends_with(".mp4") || l.ends_with(".mov")) { return false; }
        match link(path).filter(|k| k["kind"] == "clip") {
            Some(k) => { if let Some(c) = k.get("clip").and_then(Value::as_str) { out.entry(c.to_string()).or_insert(path.to_path_buf()); } true }
            None => false,
        }
    });
    (out, dirs)
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

/// The names a clip's camera-made low-resolution copy (its sidecar) may have beside it, in the order looked for: DJI's
/// <stem>.LRF; GoPro HERO5 and later: GL<rest>.LRV of a GX<rest>/GH<rest> clip; GoPro HERO3/4: <stem>.LRV of a
/// GOPR<n>/GP<ch><n> clip. The player's seek-bar pictures look for the same (mpv/scripts/side-panel/scrub.lua):
/// tests/scrub_sidecar_cases.txt holds both to it
pub fn low_res_names(stem: &str) -> Vec<String> {
    let mut names: Vec<String> = ["LRF", "lrf"].iter().map(|e| format!("{stem}.{e}")).collect();
    if stem.len() > 2 && (stem.starts_with("GX") || stem.starts_with("GH")) {
        names.push(format!("GL{}.LRV", &stem[2..]));
        names.push(format!("GL{}.lrv", &stem[2..]));
    }
    if stem.starts_with("GOPR") || (stem.len() > 2 && stem.starts_with("GP")) {
        names.push(format!("{stem}.LRV"));
        names.push(format!("{stem}.lrv"));
    }
    names
}

/// A clip's camera-made quick-look file beside it: its sidecar (low_res_names), or the camera's .THM before GoPro's
/// LRV. An empty or cut-short one (the camera stopped before writing it) isn't one
pub fn camera_thumb(clip: &Path) -> Option<PathBuf> {
    let dir = clip.parent()?;
    let stem = clip.file_stem()?.to_string_lossy().into_owned();
    let mut names = low_res_names(&stem);
    // The .THM (a still) after the .LRF, before GoPro's LRV (a video), as it has been
    names.splice(2..2, ["THM", "thm"].iter().map(|e| format!("{stem}.{e}")));
    names.into_iter().map(|n| dir.join(n)).find(|p| crate::seen::size(p).is_some_and(|s| s >= 1024))
}

pub(crate) fn read_json(path: &Path) -> Option<Value> {
    afp_catalog::store::nas::read();
    std::fs::read_to_string(path).ok().and_then(|t| serde_json::from_str(&t).ok())
}

fn has_gyroflow(file: &Path) -> bool {
    let Some(dir) = file.parent() else { return false };
    let name = file.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let stem = if let Some(s) = name.strip_suffix(LINK_EXT) { s.to_string() } else { file.file_stem().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default() };
    crate::seen::is_file(&dir.join(format!("{stem}.gyroflow"))) || crate::seen::is_file(&dir.join(format!("{name}.gyroflow")))
}

// ---------- the views ----------

/// The footage view: every location of the project, each at its place in the tree, a path the first one has winning
/// (a working copy before the archive's). Each clip with where it plays from (`loc`), every location that has it
/// (`copies`), its own file wherever it is (two: `own_conflict`)
/// `only`: these clips alone (their paths in the tree), each looked up where a location would have it, nothing walked:
/// the items as the whole view would have them (changed.rs)
fn footage(root: &Path, flat_previews: bool, only: Option<&[String]>) -> Value {
    let mounts = crate::locations::mounts();
    let locs = crate::locations::locations(root, &mounts);
    // The NAS's other names (Project settings): an offline location's Connect offers each
    let hosts = read_json(&root.join(crate::locations::PROJECT_FILE)).and_then(|d| d.get("hosts").cloned()).unwrap_or(Value::Null);
    let (linked, clips_dirs) = previews(root);
    let mut dirs: Vec<String> = Vec::new();
    let mut seen_dirs: HashSet<String> = HashSet::new();
    let mut add_dir = |d: &str, dirs: &mut Vec<String>| {
        let mut parts = Vec::new();
        for p in d.split('/').filter(|p| !p.is_empty()) {
            parts.push(p);
            let x = parts.join("/");
            if seen_dirs.insert(x.clone()) { dirs.push(x); }
        }
    };
    let mut items: Vec<Value> = Vec::new();
    let mut by_tree: HashMap<String, usize> = HashMap::new();
    // Each location's own folders, as it is on its disk (the Library's Sources)
    let mut loc_dirs: Vec<Vec<String>> = vec![Vec::new(); locs.len()];
    // A location not reachable now (a share not mounted, a disk not plugged in): its clips as the project's catalog
    // last saw them, marked offline, after the reachable ones (a clip in both plays from the one here)
    let offline: Vec<bool> = locs.iter().map(|l| !l.local && l.dir.as_ref().is_none_or(|d| !d.is_dir())).collect();
    let mut order: Vec<usize> = (0..locs.len()).collect();
    order.sort_by_key(|&i| offline[i]);
    for li in order {
        let l = &locs[li];
        let off = offline[li];
        let id = format!("l:{}", l.path);
        // A project's own folders at the top of a location (this project's, or the one that was there before it moved)
        // aren't footage
        let skip = |rel: &str, lower: &str| !rel.contains('/') && own_folder(lower);
        let (dir, found) = if let Some(only) = only {
            let dir = if off { offline_dir(l) } else { match &l.dir { Some(d) => d.clone(), None => continue } };
            let found = only.iter().filter_map(|tree| {
                let rel = if l.at.is_empty() { Some(tree.as_str()) } else { tree.strip_prefix(&format!("{}/", l.at)) }?;
                let (size, mtime) = if off { crate::seen::file_in(&id, rel).filter(|f| !f.dir).map(|f| (f.size, crate::seen::seconds(f.mtime))) }
                    else { stamp(&rel.split('/').fold(dir.clone(), |d, p| d.join(p)).to_string_lossy()) }?;
                Some((rel.to_string(), false, size, mtime))
            }).collect();
            (dir, found)
        } else if off {
            let Some(found) = crate::seen::walk_root(&id, &skip, MAX_DEPTH) else { continue };
            (offline_dir(l), found)
        } else {
            let Some(dir) = &l.dir else { continue };
            (dir.clone(), walk(dir, &skip))
        };
        let dir = &dir;
        if !l.at.is_empty() { add_dir(&l.at, &mut dirs); }
        for (rel, is_dir, size, mtime) in found {
            let tree = if l.at.is_empty() { rel.clone() } else { format!("{}/{rel}", l.at) };
            if is_dir { add_dir(&tree, &mut dirs); loc_dirs[li].push(rel.clone()); continue; }
            let lower = rel.to_lowercase();
            if !(lower.ends_with(".mp4") || lower.ends_with(".mov")) || lower.ends_with(LINK_EXT) { continue; }
            let path = dir.join(&rel);
            // Offline: what's beside it as the catalog last saw it (its own file read from there, not the disk)
            let beside = |name: String| -> bool { crate::seen::file_in(&id, &rel_join(&parent_of(&rel), &name)).is_some_and(|f| !f.dir) };
            let stem = |ext: &str| Path::new(name_of(&rel)).with_extension(ext).to_string_lossy().into_owned();
            let own_here = if off {
                ["afp.json", "marks.json"].into_iter().map(|e| stem(e)).find(|n| beside(n.clone())).map(|n| path.with_file_name(n))
            } else { let f = path.with_extension("afp.json"); if crate::seen::is_file(&f) { Some(f) } else { let o = path.with_extension("marks.json"); crate::seen::is_file(&o).then_some(o) } };
            let gyroflow = if off { beside(stem("gyroflow")) || beside(format!("{}.gyroflow", name_of(&rel))) } else { has_gyroflow(&path) };
            let copy = json!({ "loc": l.name, "li": li, "rel": rel, "path": path.display().to_string(), "offline": off });
            if let Some(&i) = by_tree.get(&tree) {
                let it = &mut items[i];
                it["copies"].as_array_mut().unwrap().push(copy);
                if gyroflow { it["gyroflow"] = json!(true); }
                if let Some(o) = own_here { it["own_files"].as_array_mut().unwrap().push(json!(o.display().to_string())); }
                continue;
            }
            let mut it = json!({ "id": tree, "dir": parent_of(&tree), "name": name_of(&tree), "path": path.display().to_string(), "size": size,
                                 "mtime": mtime, "kind": "clip", "loc": l.name, "copies": [copy],
                                 "own_files": own_here.map(|o| vec![json!(o.display().to_string())]).unwrap_or_default() });
            it["gyroflow"] = json!(gyroflow);
            if off { it["offline"] = json!(true); }
            // What the camera wrote beside it for its own quick look (DJI's and GoPro's .LRF proxy, GoPro's .THM): the
            // clip's picture until it has a preview
            if !off { if let Some(t) = camera_thumb(&path) { it["thumb"] = json!(t.display().to_string()); } }
            let preview = linked.get(tree.as_str()).cloned().or_else(|| [clip_preview_relative(&tree, flat_previews), clip_preview_relative(&tree, !flat_previews)]
                .into_iter().map(|p| root.join("_Clips").join(p)).find(|p| crate::seen::is_file(p)));
            // Where it is as its preview was put (its folder in the project's): the Library shows footage that way
            it["pdir"] = preview.as_ref().and_then(|p| p.parent()).and_then(|d| crate::locations::relative(d, root).or_else(|| (d == root).then(String::new)))
                .map(Value::from).unwrap_or(Value::Null);
            // An empty one (its picture still to be made): the camera's quick look shows instead
            if preview.as_ref().and_then(|p| link(p)).is_some_and(|l| l.get("draft").and_then(Value::as_bool) == Some(true)) { it["draft"] = json!(true); }
            it["preview"] = preview.map(|p| json!(p.display().to_string())).unwrap_or(Value::Null);
            by_tree.insert(tree, items.len());
            items.push(it);
        }
    }
    // Each clip's own file: the one there is (two or more: none read, the clip shows the problem)
    for it in items.iter_mut() {
        let files: Vec<String> = it["own_files"].as_array().unwrap().iter().filter_map(|v| v.as_str().map(str::to_string)).collect();
        if files.len() > 1 { it["own_conflict"] = json!(files); }
        if let Some(f) = files.first() {
            // The sidecar's marks, view, source and tags as the catalog read them, else the file (offline: only the
            // catalog's, by its location's root)
            let off = it["offline"].as_bool() == Some(true);
            let seen = if off {
                let c = &it["copies"][0];
                let li = c["li"].as_u64().unwrap_or(0) as usize;
                let rel = Path::new(c["rel"].as_str().unwrap_or("")).parent().map(|d| d.to_string_lossy().replace('\\', "/")).unwrap_or_default();
                let name = Path::new(f).file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                locs.get(li).and_then(|l| crate::seen::file_in(&format!("l:{}", l.path), &rel_join(&rel, &name))).and_then(|f| f.meta).and_then(|m| m.get("own").cloned())
            } else { crate::seen::meta(Path::new(f)).flatten().and_then(|m| m.get("own").cloned()) };
            if let Some(own) = seen.or_else(|| if off { None } else { read_json(Path::new(f)) }) {
                it["own"] = json!({ "marks": own.get("marks").cloned().unwrap_or(Value::Null), "view": own.get("view").cloned().unwrap_or(Value::Null),
                                    "source": own.get("source").cloned().unwrap_or(Value::Null),
                                    "location": own.get("location").cloned().unwrap_or(Value::Null) });
                it["tags"] = own.get("tags").cloned().filter(Value::is_array).unwrap_or(json!([]));
                if let Some(r) = crate::tags::rating_of(&own["rating"]) { it["rating"] = json!(r); }
                if let Some(p) = crate::tags::pick_of(&own["pick"]) { it["pick"] = json!(p); }
                if let Some(c) = crate::tags::comment_of(&own["comment"]) { it["comment"] = json!(c); }
            }
        }
    }
    let locs_json: Vec<Value> = locs.iter().enumerate().map(|(i, l)| json!({ "name": l.name, "at": l.at, "path": l.path, "dir": l.dir.as_ref().map(|d| d.display().to_string()),
                                                             "copies": l.copies, "local": l.local, "dirs": loc_dirs[i], "offline": offline[i],
                                                             "connect": offline[i] && cfg!(target_os = "macos") && crate::locations::network(&l.path).is_some(),
                                                             "names": if offline[i] { crate::locations::network(&l.path).map(|(h, _, _)| crate::host_names(&h, Some(&hosts))).unwrap_or_default() } else { Vec::new() } })).collect();
    // Every clip link, wherever it is (albums.rs): the Albums view shows each as an item of its folder
    let (link_files, _) = own_files(root, &mut |rel, _, _| rel.to_lowercase().ends_with(LINK_EXT));
    let links = crate::albums::clip_links(root, &link_files);
    json!({ "dirs": dirs, "items": items, "locations": locs_json, "clips_dirs": clips_dirs, "links": links })
}

/// Where an offline location's clips would be: its folder when it names one (a disk not plugged in), else where the
/// Mac mounts its share (/Volumes/<share>, as Finder does: the clips' answers known from before are found by that
/// path), else its network path
fn offline_dir(l: &crate::locations::Location) -> PathBuf {
    if let Some(d) = &l.dir { return d.clone(); }
    match crate::locations::network(&l.path) {
        Some((_, share, sub)) if cfg!(target_os = "macos") => {
            let mut d = PathBuf::from("/Volumes").join(share);
            for p in sub.split('/').filter(|p| !p.is_empty()) { d.push(p); }
            d
        }
        _ => PathBuf::from(&l.path),
    }
}

pub fn scan(root: &Path, view: &str, flat_previews: bool) -> Value { scan_only(root, view, flat_previews, None) }

/// A view, or with `only` these items of it alone (a clip's path in the tree, a moment's, photo's or export's in the
/// project's folder), exactly as the whole view would have them: what a command says it changed (changed.rs). The
/// ones asked for that aren't in the view aren't in the answer; its folders aren't said
pub fn scan_only(root: &Path, view: &str, flat_previews: bool, only: Option<&[String]>) -> Value {
    if view == "footage" { return footage(root, flat_previews, only); }
    if !matches!(view, "moments" | "stills" | "exports") { return json!({ "dirs": [], "items": [] }) }
    // A moment's original, through the project's locations (the first that has it)
    let locs = if view == "moments" { crate::locations::locations(root, &crate::locations::mounts()) } else { Vec::new() };
    let at_hint = |link: &Value| -> Option<PathBuf> {
        let tree = link["clip"].as_str()?;
        locs.iter().filter_map(|l| crate::locations::in_location(tree, l))
            .find(|f| match crate::seen::file(f) {
                Some(r) => r.is_some_and(|r| !r.dir && link["size"].as_u64().is_none_or(|s| s == r.size)),
                None => std::fs::metadata(f).is_ok_and(|m| m.is_file() && link["size"].as_u64().is_none_or(|s| s == m.len())),
            })
    };
    // Not at its path: looked for by what it is (checks.rs's find_clip), the footage listed once for the view
    let never_rename = view == "moments" && crate::locations::never_rename(root);
    let mut clips: Option<Vec<crate::checks::Clip>> = None;
    let mut original_of = |link: &Value| -> (Option<PathBuf>, Option<String>) {
        if let Some(f) = at_hint(link) { return (Some(f), None); }
        let list = clips.get_or_insert_with(|| crate::checks::footage(&locs).0);
        let (hit, _, _) = crate::checks::find_clip(link, list, false, never_rename, &mut |p| crate::checks::fingerprint_kept(p), &mut |p| created(p));
        match hit { Some(c) => (Some(c.path.clone()), (!c.local).then(|| c.tree.clone())), None => (None, None) }
    };
    // Tags an older version kept for photos in the project's file, by their path in _Stills
    let photo_tags = if view == "stills" { read_json(&root.join("Project.afproject")).and_then(|d| d.get("photo_tags").cloned()).unwrap_or(json!({})) } else { json!({}) };
    // The files of this view's kind, wherever they are (docs/library-management.md §8): moments and exports by the link
    // inside (an export of before 0.11 in _Exports without one too), photos with a link or in _Stills
    let mut links: HashMap<String, Value> = HashMap::new();
    let mut keep = |rel: &str, path: &Path, _: bool| {
        let lower = rel.to_lowercase();
        let video = lower.ends_with(".mp4") || lower.ends_with(".mov");
        let photo = [".heic", ".jpg", ".jpeg", ".png"].iter().any(|e| lower.ends_with(e));
        let l = if video || photo { link(path) } else { None };
        let keep = match view {
            "moments" => video && (l.as_ref().is_some_and(|k| k["kind"] == "moment") || (lower.starts_with("_moments/") && lower.ends_with(LINK_EXT) && l.is_none())),
            // (a render saved into a folder of the project by Share is a file of the user's, as an export is; the
            // cache itself, _Renders, is the Library's own: never listed)
            "exports" => video && !lower.starts_with("_renders/") && (l.as_ref().is_some_and(|k| k["kind"] == "export" || k["kind"] == "render") || (lower.starts_with("_exports/") && l.is_none())),
            // (a photo of before links, moved into an album of the user's: still a photo there)
            _ => photo && !lower.starts_with("_renders/") && (l.is_some() || lower.starts_with("_stills/") || lower.starts_with("_clips/")),
        };
        if keep { if let Some(k) = l { links.insert(rel.to_string(), k); } }
        keep
    };
    let (entries, dirs) = match only {
        // Only these: each as the walk would have found it (there, the project's own or a link among its footage)
        Some(only) => {
            let footage = footage_dirs(root);
            (only.iter().filter_map(|rel| {
                let path = rel.split('/').fold(root.to_path_buf(), |d, p| d.join(p));
                let (size, mtime) = stamp(&path.to_string_lossy())?;
                if !own_area(root, &path, &footage) && !rel.to_lowercase().ends_with(LINK_EXT) { return None; }
                keep(rel, &path, true).then(|| (rel.clone(), size, mtime))
            }).collect(), Vec::new())
        }
        None => own_files(root, &mut keep),
    };
    let mut items = Vec::new();
    // The render cache, looked at once for them all
    let renders = crate::renders::Ctx::new(root, false);
    let files: HashSet<String> = walk_names(root, &entries);
    for (rel, size, mtime) in &entries {
        let rel = rel.as_str();
        let name = name_of(rel);
        let lower = name.to_lowercase();
        let path = root.join(rel);
        let mut it = json!({ "id": rel, "dir": parent_of(rel), "name": name, "path": path.display().to_string(), "size": size, "mtime": mtime });
        let l = links.get(rel).cloned().unwrap_or(Value::Null);
        match view {
            "moments" => {
                it["kind"] = json!("moment");
                let stem = if lower.ends_with(LINK_EXT) { &name[..name.len() - LINK_EXT.len()] } else { name.rsplit_once('.').map(|(s, _)| s).unwrap_or(name) };
                it["name"] = json!(stem);
                it["tags"] = l.get("tags").cloned().filter(Value::is_array).unwrap_or(json!([]));
                if let Some(r) = crate::tags::rating_of(&l["rating"]) { it["rating"] = json!(r); }
                if let Some(p) = crate::tags::pick_of(&l["pick"]) { it["pick"] = json!(p); }
                if let Some(c) = crate::tags::comment_of(&l["comment"]) { it["comment"] = json!(c); }
                it["gyroflow"] = json!(has_gyroflow(&path));
                it["renders"] = renders_of(&renders, crate::renders::Kind::Moment, rel, &path, &l);
                let (original, moved_to) = original_of(&l);
                it["original"] = original.map(|o| json!(o.display().to_string())).unwrap_or(Value::Null);
                // Found away from the path its link names: the new path, written into the link (main.rs's scan)
                if let Some(t) = moved_to { it["relink"] = json!(t); }
                it["link"] = l;
            }
            "stills" => {
                it["kind"] = json!("still");
                // Its own (XMP), and what older versions kept for it in Project.afproject, until written into it
                let mut tags = match crate::seen::meta(&path) {
                    Some(m) => m.and_then(|m| m.get("tags").cloned()).and_then(|t| serde_json::from_value::<Vec<String>>(t).ok()).unwrap_or_default(),
                    None => self::photo_tags(&path),
                };
                let old_id = rel.strip_prefix("_Stills/").unwrap_or(rel);
                for t in photo_tags.get(old_id).and_then(Value::as_array).into_iter().flatten().filter_map(Value::as_str) {
                    if !tags.iter().any(|x| x.eq_ignore_ascii_case(t)) { tags.push(t.to_string()); }
                }
                it["tags"] = json!(tags);
                if let Some(r) = crate::tags::rating_of(&l["rating"]) { it["rating"] = json!(r); }
                if let Some(p) = crate::tags::pick_of(&l["pick"]) { it["pick"] = json!(p); }
                if let Some(c) = crate::tags::comment_of(&l["comment"]) { it["comment"] = json!(c); }
                let stem = rel.rsplit_once('.').map(|(s, _)| s).unwrap_or(rel).to_lowercase();
                // A photo that's a link (albums.rs: a small picture, rendered when shared), or the rendered file
                it["linked"] = json!(lower.ends_with(crate::albums::PHOTO_EXT));
                it["form"] = json!(if lower.ends_with(crate::albums::PHOTO_EXT) { "Photo link" } else if lower.ends_with(".mp.jpg") { "Motion Photo" }
                    else if files.contains(&format!("{stem}.mov")) { "Live Photo" }
                    else if lower.ends_with(".heic") { "HEIC with gain map" } else { "Ultra HDR JPEG" });
                // A photo link's renders (a rendered photo is its own pixels)
                if lower.ends_with(crate::albums::PHOTO_EXT) && l["clip"].is_string() { it["renders"] = renders_of(&renders, crate::renders::Kind::Photo, rel, &path, &l); }
                // Its clip and time, from the photo itself (older ones have none: their name tells)
                it["link"] = l;
            }
            _ => {
                it["kind"] = json!("export");
                // Not tagged: a derivative (docs/tags-and-culling.md); tags an older version put in its link are left there
                it["tags"] = json!([]);
                it["link"] = l;
            }
        }
        items.push(it);
    }
    json!({ "dirs": dirs, "items": items })
}

/// The lower-case paths of the files beside these (a Live Photo's .mov beside its still)
fn walk_names(root: &Path, entries: &[(String, u64, i64)]) -> HashSet<String> {
    let mut out = HashSet::new();
    let dirs: HashSet<String> = entries.iter().map(|(r, _, _)| parent_of(r)).collect();
    for d in dirs {
        let dir = if d.is_empty() { root.to_path_buf() } else { root.join(&d) };
        if let Some(names) = crate::seen::names_in(&dir) {
            for n in names { out.insert(rel_join(&d, &n).to_lowercase()); }
            continue;
        }
        for e in std::fs::read_dir(&dir).into_iter().flatten().flatten() {
            out.insert(rel_join(&d, &e.file_name().to_string_lossy()).to_lowercase());
        }
    }
    out
}

// ---------- renders (the player's render batch; docs/library-management.md, "Renders") ----------

pub const RENDER_PROFILES: [&str; 2] = ["HDR", "SDR"];
const RENDER_VERSION: u64 = 1;

/// A value as the player's JSON keeps it: nulls and empty objects or arrays left out, for comparing recipes
pub fn trimmed(v: &Value) -> Option<Value> {
    match v {
        Value::Null => None,
        Value::Object(o) => {
            let m: serde_json::Map<String, Value> = o.iter().filter_map(|(k, x)| trimmed(x).map(|t| (k.clone(), t))).collect();
            (!m.is_empty()).then_some(Value::Object(m))
        }
        Value::Array(a) => {
            let l: Vec<Value> = a.iter().filter_map(trimmed).collect();
            (!l.is_empty()).then_some(Value::Array(l))
        }
        x => Some(x.clone()),
    }
}

pub(crate) fn same(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Number(x), Value::Number(y)) => (x.as_f64().unwrap_or(f64::NAN) - y.as_f64().unwrap_or(f64::NAN)).abs() < 1e-9,
        (Value::Object(x), Value::Object(y)) => x.len() == y.len() && x.iter().all(|(k, v)| y.get(k).is_some_and(|w| same(v, w))),
        (Value::Array(x), Value::Array(y)) => x.len() == y.len() && x.iter().zip(y).all(|(v, w)| same(v, w)),
        _ => a == b,
    }
}

/// The recipe of a moment's render in `profile` as renders were made before the cache by hash (v1, the player's
/// render_recipe of then): what a render at the moment's path is compared with (renders.rs)
pub fn render_recipe(root: &Path, moment: &Path, link: &Value, profile: &str) -> Value {
    let input = link["clip"].as_str().and_then(|c| crate::tags::clip_own(root, c).ok()).and_then(|(own, _)| read_json(&own)).map(|d| d["source"].clone()).unwrap_or(Value::Null);
    let name = moment.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let own = moment.with_file_name(format!("{}.gyroflow", name.trim_end_matches(LINK_EXT)));
    afp_catalog::store::nas::stat();
    let gyroflow = if own.is_file() { crate::checks::fingerprint(&own).map(Value::from).unwrap_or(Value::Null) } else { Value::Null };
    render_recipe_of(link, input, gyroflow, profile)
}

/// The same from its parts: the clip's own file's `source`, the moment's own Gyroflow project's fingerprint
pub fn render_recipe_of(link: &Value, input: Value, gyroflow: Value, profile: &str) -> Value {
    trimmed(&json!({ "v": RENDER_VERSION, "clip": { "hash": link["hash"], "size": link["size"] }, "in": link["in"], "out": link["out"],
                     "stab": link["stab"], "alone": link["alone"], "input": input, "gyroflow": gyroflow, "profile": profile })).unwrap_or(Value::Null)
}

/// Where a moment's render in a profile is: _Renders/<its path without .afplink.mp4>.<profile>.mov, its path being the
/// one in _Moments for a moment there (as before 0.11), else in the project's folder (`rel`: its path in the project's
/// folder, or in _Moments as before; the player's render_rel does the same)
pub fn render_path(root: &Path, rel: &str, profile: &str) -> PathBuf {
    let mut p = root.join("_Renders");
    let rel = rel.strip_prefix("_Moments/").unwrap_or(rel);
    let base = rel.strip_suffix(LINK_EXT).or_else(|| rel.rsplit_once('.').map(|(s, _)| s)).unwrap_or(rel);
    let parts: Vec<&str> = base.split('/').collect();
    for (i, part) in parts.iter().enumerate() { p.push(if i + 1 == parts.len() { format!("{part}.{profile}.mov") } else { part.to_string() }); }
    p
}

/// Each profile's render of a moment or a photo link: { HDR: "current" | "stale" }, absent: none (renders.rs: found
/// by the recipe's hash, the ones of before by the moment's path)
fn renders_of(ctx: &crate::renders::Ctx, kind: crate::renders::Kind, rel: &str, file: &Path, link: &Value) -> Value {
    let name = name_of(rel);
    crate::renders::states(ctx, &crate::renders::Item { kind, file: Some(file.to_path_buf()), rel: rel.to_string(), link: link.clone(), name: name.to_string() })
}

// ---------- what each clip is: gyroflow-probe --library, in the background ----------

/// The probe's answers this Library wants (gyroflow-probe --library's "geotag"): older ones are asked again
pub const GEOTAG: u64 = 9;

#[derive(Default)]
pub struct Probes {
    /// path -> (size, time, the lens list's stamp, answer)
    known: HashMap<String, (u64, i64, String, Value)>,
    queue: VecDeque<String>,
    queued: HashSet<String>,
    log: Vec<(u64, String)>,
    version: u64,
    pub tool: Option<PathBuf>,
    pub busy: bool,
    /// The player's lens database as last looked at (lens_list_check): None, not looked at yet
    lenses_seen: Option<(String, BTreeMap<String, String>)>,
    /// path -> its own file's and measured motion's stamps when its answer was worked out (beside_stamp)
    beside: HashMap<String, String>,
}

/// What else of a clip its probe answer is worked out from: its own file beside it (source.lens, a lens tried on it)
/// and its motion measured from the picture (.afp-motion), each as the project's catalog has it ("-": not there). No
/// question to the share: None when the catalog doesn't know them (the answer is then taken as it is)
pub fn beside_stamp(clip: &str) -> Option<String> {
    let p = Path::new(clip);
    let mut out = Vec::new();
    for ext in ["afp.json", "afp-motion"] {
        match crate::seen::file(&p.with_extension(ext))? {
            Some(f) => out.push(format!("{}:{}", f.size, crate::seen::seconds(f.mtime))),
            None => out.push("-".into()),
        }
    }
    Some(out.join("|"))
}

pub type SharedProbes = Arc<Mutex<Probes>>;

/// The user's own lens list (vs-gyroflow's lens_pick::user_file, the same place): what tells its versions apart, ""
/// when there's none. The probe's answer about a clip's lens is worked out with it, so it's kept only while that stays
/// the same (lens_pick::user_file_stamp)
fn curated_lenses_file() -> Option<PathBuf> {
    let env = |k: &str| std::env::var_os(k).filter(|v| !v.is_empty()).map(PathBuf::from);
    env("AFP_CURATED_LENSES").or_else(|| {
        let base = if cfg!(target_os = "macos") { env("HOME")?.join("Library/Application Support") }
                   else if cfg!(windows) { env("APPDATA")? }
                   else { env("XDG_CONFIG_HOME").or_else(|| env("HOME").map(|h| h.join(".config")))? };
        Some(base.join("ActionFootagePlayer").join("curated_lenses.json"))
    })
}

/// The player's own lens database (lens_db's user folder): each lens file's name -> its size and time
pub fn lens_dir_entries() -> BTreeMap<String, String> {
    let Some(d) = curated_lenses_file().and_then(|f| Some(f.parent()?.join("lenses"))) else { return BTreeMap::new() };
    std::fs::read_dir(&d).into_iter().flatten().flatten().filter_map(|e| {
        let m = e.metadata().ok()?;
        Some((e.file_name().to_string_lossy().into_owned(), format!("{}:{:?}", m.len(), m.modified().ok())))
    }).collect()
}

/// The cameras whose lenses differ between two looks at the lens database (`dir` its folder): a lens file read for its
/// camera_brand and camera_model, a removed one named for it ("GoPro_HERO4_Black-wide-….json": "GoPro HERO4 Black")
pub fn lens_cameras_changed(before: &BTreeMap<String, String>, now: &BTreeMap<String, String>, dir: Option<&Path>) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let names: BTreeSet<&String> = before.keys().chain(now.keys()).collect();
    for n in names {
        if before.get(n) == now.get(n) { continue; }
        let from_file = dir.and_then(|d| read_json(&d.join(n))).map(|v| format!("{} {}", v["camera_brand"].as_str().unwrap_or(""), v["camera_model"].as_str().unwrap_or("")).trim().to_string())
            .filter(|c| !c.is_empty());
        let camera = from_file.unwrap_or_else(|| n.split('-').next().unwrap_or("").replace('_', " ").trim().to_string());
        if !camera.is_empty() && !out.contains(&camera) { out.push(camera); }
    }
    out
}

pub fn lens_list_stamp() -> String {
    let file = curated_lenses_file();
    let list = file.as_ref().and_then(|f| std::fs::metadata(f).ok().map(|m| format!("{}|{}|{:?}", f.display(), m.len(), m.modified().ok())))
        .unwrap_or_default();
    // And the player's own lens database beside it (vs-gyroflow's lens_db::user_stamp, the same): a lens assigned to a
    // mode changes the answers of that mode's clips
    let mut own: Vec<String> = file.and_then(|f| Some(f.parent()?.join("lenses"))).map(|d| std::fs::read_dir(d).into_iter().flatten().flatten()
        .filter_map(|e| { let m = e.metadata().ok()?; Some(format!("{}:{}:{:?}", e.file_name().to_string_lossy(), m.len(), m.modified().ok())) }).collect())
        .unwrap_or_default();
    own.sort();
    if own.is_empty() { list } else { format!("{list}|{}", own.join("|")) }
}

/// A file's size and time: as the project's catalog has it (no question to the share for each clip: thousands of
/// clips took minutes, looking as though they were read again), else from the disk
fn stamp(path: &str) -> Option<(u64, i64)> {
    if let Some(r) = crate::seen::file(Path::new(path)) { return r.filter(|f| !f.dir).map(|f| (f.size, crate::seen::seconds(f.mtime))); }
    let m = std::fs::metadata(path).ok()?;
    Some((m.len(), unix(m.modified())))
}

impl Probes {
    /// What's known, kept between runs (in the Library's cache folder): read at start, written as it grows
    pub fn load(&mut self, file: &Path) {
        let Some(v) = read_json(file) else { return };
        for (k, e) in v.as_object().into_iter().flatten() {
            let (Some(s), Some(m)) = (e["s"].as_u64(), e["m"].as_i64()) else { continue };
            // An answer from an older probe (GEOTAG raised: the probe tells more now) is asked again, and shown until
            // then: dropping it left every clip "reading…" with no length, camera or mode for as long as the whole
            // project took to read again (Anton, 2026-11-24)
            let old = e["v"]["geotag"].as_u64().unwrap_or(0) < GEOTAG;
            let lenses = if old { "\u{0}again".to_string() } else { e["l"].as_str().unwrap_or_default().to_string() };
            self.known.insert(k.clone(), (s, m, lenses, e["v"].clone()));
            if let Some(b) = e["b"].as_str() { self.beside.insert(k.clone(), b.to_string()); }
        }
    }
    pub fn save(&self, file: &Path) {
        let m: serde_json::Map<String, Value> = self.known.iter().map(|(k, (s, m, l, v))| {
            let mut e = json!({ "s": s, "m": m, "l": l, "v": v });
            if let Some(b) = self.beside.get(k) { e["b"] = json!(b); }
            (k.clone(), e)
        }).collect();
        if let Some(d) = file.parent() { let _ = std::fs::create_dir_all(d); }
        let tmp = file.with_extension("tmp");
        if std::fs::write(&tmp, serde_json::to_vec(&Value::Object(m)).unwrap()).is_ok() { let _ = std::fs::rename(&tmp, file); }
    }
    /// Asked for these clips: the ones not known (or changed since, or known with another lens list, or their own file
    /// or measured motion changed: a lens tried on one in the player, the clip measured again) are queued
    pub fn want(&mut self, paths: &[String]) {
        let lenses = lens_list_stamp();
        for p in paths {
            let fresh = match (self.known.get(p), stamp(p)) { (Some((s, m, l, _)), Some((s2, m2))) => *s == s2 && *m == m2 && *l == lenses, _ => false };
            // An answer kept from before this was noted has none: taken as it is until it's worked out again
            let fresh = fresh && match (self.beside.get(p), beside_stamp(p)) { (Some(b), Some(now)) => *b == now, _ => true };
            if !fresh && self.queued.insert(p.clone()) { self.queue.push_back(p.clone()); }
        }
    }
    /// These clips' answers to be worked out again (a lens type chosen for them): kept until then, and queued
    pub fn forget(&mut self, paths: &[String]) {
        for p in paths { if let Some(e) = self.known.get_mut(p) { e.2 = "\u{0}again".into(); } }
        self.want(paths);
    }
    /// A lens was saved to the user's list as the default for a camera (`before`: the list's stamp until then): that
    /// camera's clips worked out again, first in the queue, the others' answers kept with the list as it is now (their
    /// lens is as it was). `whole`: saved for any mode of that camera at that size ("1920x1080"), its label: the clips
    /// of that size with no lens yet have it at once (what the page counts on: Stabilize optically) until their answer
    /// comes again. How many are asked again
    pub fn lens_saved_for(&mut self, camera: &str, before: &str, whole: Option<(&str, &str)>) -> usize {
        let now = lens_list_stamp();
        // This change is known: lens_list_check doesn't take it for the player's
        self.lenses_seen = Some((now.clone(), lens_dir_entries()));
        let key = |c: &str| c.to_lowercase().replace([' ', '_'], "");
        let mut again = Vec::new();
        let mut patched = Vec::new();
        for (p, e) in self.known.iter_mut() {
            if e.3["camera"].as_str().is_some_and(|c| !c.is_empty() && key(c) == key(camera)) {
                again.push(p.clone());
                let size = format!("{}x{}", e.3["width"].as_u64().unwrap_or(0), e.3["height"].as_u64().unwrap_or(0));
                if let Some((s, label)) = whole {
                    if size == s && e.3["mode"].is_null() && e.3["lens"].as_str() == Some("") { e.3["lens"] = json!(label); patched.push(p.clone()); }
                }
            }
            else if e.2 == before { e.2 = now.clone(); }
        }
        for p in patched { self.version += 1; self.log.push((self.version, p)); }
        self.forget(&again);
        // Asked before what was queued already: the page waits on these
        let set: HashSet<&String> = again.iter().collect();
        let (mut first, rest): (VecDeque<String>, VecDeque<String>) = self.queue.drain(..).partition(|p| set.contains(p));
        first.extend(rest);
        self.queue = first;
        again.len()
    }
    /// The lens database changed by something other than this Library (the player's lens dialog: Use for the mode,
    /// Forget, Estimate kept): the clips of the cameras whose lenses changed asked again, as when the Library saves
    /// one (lens_saved_for). How many are asked again
    pub fn lens_list_check(&mut self) -> usize {
        let now = lens_list_stamp();
        let Some((before, entries)) = self.lenses_seen.clone() else { self.lenses_seen = Some((now, lens_dir_entries())); return 0 };
        if before == now { return 0; }
        let entries_now = lens_dir_entries();
        let dir = curated_lenses_file().and_then(|f| Some(f.parent()?.join("lenses")));
        let cameras = lens_cameras_changed(&entries, &entries_now, dir.as_deref());
        self.lenses_seen = Some((now, entries_now));
        cameras.iter().map(|c| self.lens_saved_for(c, &before, None)).sum()
    }
    /// What's known that changed after `since`: (version now, path -> answer, how many wait)
    pub fn since(&self, since: u64) -> Value {
        let mut out = serde_json::Map::new();
        for (v, p) in self.log.iter().rev() {
            if *v <= since { break; }
            if !out.contains_key(p) { if let Some((_, _, _, val)) = self.known.get(p) { out.insert(p.clone(), val.clone()); } }
        }
        json!({ "version": self.version, "items": out, "pending": self.queue.len() + usize::from(self.busy) })
    }
    /// One clip's answer, if known and its file unchanged since
    pub fn get(&self, path: &str) -> Option<Value> {
        let (s, m, _, v) = self.known.get(path)?;
        (stamp(path) == Some((*s, *m))).then(|| v.clone())
    }
    /// An answer worked out elsewhere (geo.rs asks the tool itself when it needs one now)
    pub fn put(&mut self, path: &str, answer: Value) {
        if let Some((s, m)) = stamp(path) {
            self.version += 1;
            let v = self.version;
            self.known.insert(path.to_string(), (s, m, lens_list_stamp(), answer));
            match beside_stamp(path) { Some(b) => { self.beside.insert(path.to_string(), b); } None => { self.beside.remove(path); } }
            self.log.push((v, path.to_string()));
        }
    }
    pub fn known(&self, paths: &[String]) -> Value {
        let mut out = serde_json::Map::new();
        for p in paths { if let Some((_, _, _, v)) = self.known.get(p) { out.insert(p.clone(), v.clone()); } }
        json!({ "version": self.version, "items": out, "pending": self.queue.len() + usize::from(self.busy) })
    }
}

/// The worker: one clip at a time, the tool run as a process (it's the player's own; its answers kept per file)
pub fn probe_worker(probes: SharedProbes, keep: Option<PathBuf>) {
    let mut unsaved = 0;
    loop {
        let next = {
            let mut p = probes.lock().unwrap();
            let n = p.queue.pop_front();
            if let Some(path) = &n { p.queued.remove(path); }
            p.busy = n.is_some();
            n.zip(p.tool.clone())
        };
        let Some((path, tool)) = next else {
            if unsaved > 0 { if let Some(f) = &keep { probes.lock().unwrap().save(f); } unsaved = 0; }
            std::thread::sleep(std::time::Duration::from_millis(250));
            continue;
        };
        let stamp_before = stamp(&path);
        let lenses = lens_list_stamp();
        let beside_before = beside_stamp(&path);
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
            p.known.insert(path.clone(), (s, m, lenses, answer));
            match beside_before { Some(b) => { p.beside.insert(path.clone(), b); } None => { p.beside.remove(&path); } }
            p.log.push((v, path));
        }
        p.busy = false;
        unsaved += 1;
        if unsaved >= 50 { if let Some(f) = &keep { p.save(f); } unsaved = 0; }
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

/// A small picture of a file, as a data: URL: an MP4's cover (a moment's, a clip preview's), else one made by the
/// system (sips for photos on macOS; videos: thumbs.rs), kept in `cache`. None: none to show
pub fn thumb(path: &Path, cache: &Path) -> Option<String> { thumb_bytes(path, cache).map(|b| data_url(&b)) }

// Where a file's picture is kept: by its path, size and time (a file changed in place gets a new one)
fn thumb_key(path: &Path, size: u64, mtime: i64) -> String { fnv(&format!("{}|{size}|{mtime}", path.display())) }

/// A file or folder renamed or moved from `from` to `to` (done already): the pictures kept for it, and for every file in
/// it, follow it to the new paths, rather than being made again (a renamed album's links, moments, photos; a photo's
/// picture is sips' work, an export's Quick Look's). The files are the same, so their size and time are: read at the
/// new path, they give both keys; a folder's files as the catalog has them (the rename noted them: nothing is asked of a
/// share), else from the disk. How many followed
pub fn thumbs_follow(cache: &Path, from: &Path, to: &Path) -> usize {
    let one = |rest: &str, size: u64, mtime: i64| -> bool {
        let under = |base: &Path| rest.split('/').filter(|c| !c.is_empty()).fold(base.to_path_buf(), |p, c| p.join(c));
        let (old, new) = (under(from), under(to));
        let a = cache.join(format!("{}.img", thumb_key(&old, size, mtime)));
        let b = cache.join(format!("{}.img", thumb_key(&new, size, mtime)));
        a.is_file() && !b.exists() && std::fs::rename(&a, &b).is_ok()
    };
    let is_dir = match crate::seen::file(to) { Some(f) => f.is_some_and(|f| f.dir), None => to.is_dir() };
    if !is_dir { return stamp(&to.to_string_lossy()).is_some_and(|(s, m)| one("", s, m)) as usize; }
    let files: Vec<(String, u64, i64)> = match crate::seen::walk(to, &|_, _| false, usize::MAX) {
        Some(w) => w.into_iter().filter(|e| !e.1).map(|(rel, _, size, mtime)| (rel, size, mtime)).collect(),
        None => {
            let mut out = Vec::new();
            let mut todo = vec![to.to_path_buf()];
            while let Some(d) = todo.pop() {
                for e in std::fs::read_dir(&d).into_iter().flatten().flatten() {
                    let p = e.path();
                    let Ok(m) = e.metadata() else { continue };
                    if m.is_dir() { todo.push(p); continue; }
                    let Ok(rest) = p.strip_prefix(to) else { continue };
                    out.push((rest.to_string_lossy().replace('\\', "/"), m.len(), unix(m.modified())));
                }
            }
            out
        }
    };
    files.iter().filter(|(rel, size, mtime)| one(rel, *size, *mtime)).count()
}

/// The same picture, as its bytes (a JPEG, or a cover's PNG)
pub fn thumb_bytes(path: &Path, cache: &Path) -> Option<Vec<u8>> {
    let (size, mtime) = stamp(&path.to_string_lossy())?;
    let key = thumb_key(path, size, mtime);
    let file = cache.join(format!("{key}.img"));
    if let Ok(b) = std::fs::read(&file) { return (!b.is_empty()).then_some(b); }
    let _ = std::fs::create_dir_all(cache);
    let lower = path.to_string_lossy().to_lowercase();
    // A camera's proxy (.LRF: an MP4) or thumbnail (.THM: a JPEG): a frame by the player's ffmpeg (0.7 s for a DJI
    // .LRF on the NAS; Quick Look hangs on them)
    if lower.ends_with(".lrf") || lower.ends_with(".thm") {
        let out = cache.join(format!("{key}.jpg"));
        let ffmpeg = crate::player_ffmpeg();
        let mut cmd = Command::new(ffmpeg.as_deref().unwrap_or(Path::new("ffmpeg")));
        cmd.args(["-v", "error", "-y"]);
        if lower.ends_with(".lrf") { cmd.args(["-ss", "1"]); }
        let ok = cmd.arg("-i").arg(path).args(["-frames:v", "1", "-vf", "scale=360:-2", "-q:v", "4"]).arg(&out)
            .stdout(Stdio::null()).stderr(Stdio::null()).status().is_ok_and(|s| s.success());
        let bytes = if ok { std::fs::read(&out).ok() } else { None };
        let _ = std::fs::remove_file(&out);
        let _ = std::fs::write(&file, bytes.as_deref().unwrap_or(&[]));
        return bytes;
    }
    let mut bytes: Option<Vec<u8>> = None;
    if lower.ends_with(".mp4") || lower.ends_with(".mov") {
        bytes = keys_and_cover(path).1;
        // A light link (albums.rs): its one frame is its picture
        if bytes.is_none() && lower.ends_with(LINK_EXT) { bytes = crate::albums::picture(path); }
    }
    if bytes.is_none() {
        let out = cache.join(format!("{key}.jpg"));
        if cfg!(target_os = "macos") && [".heic", ".jpg", ".jpeg", ".png"].iter().any(|e| lower.ends_with(e)) {
            let ok = Command::new("/usr/bin/sips").args(["-s", "format", "jpeg", "-Z", "360"]).arg(path).arg("--out").arg(&out)
                .stdout(Stdio::null()).stderr(Stdio::null()).status().is_ok_and(|s| s.success());
            if ok { bytes = std::fs::read(&out).ok(); }
        } else if lower.ends_with(".mov") || lower.ends_with(".mp4") {
            // A video without a cover (an original clip, a draft clip preview): as Finder draws it (thumbs.rs)
            if crate::thumbs::video_frame(path, &out) { bytes = std::fs::read(&out).ok(); }
        }
        let _ = std::fs::remove_file(&out);
    }
    let _ = std::fs::write(&file, bytes.as_deref().unwrap_or(&[]));
    bytes
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    // The sidecar's names, as the player's seek bar has them (tests/scrub_sidecar_cases.txt, also read by
    // tests/scrub_test.lua)
    #[test]
    fn sidecar_names_as_the_player_has_them() {
        let cases = include_str!("../../tests/scrub_sidecar_cases.txt");
        let mut n = 0;
        for line in cases.lines().filter(|l| !l.starts_with('#') && l.contains(" -> ")) {
            let (clip, want) = line.split_once(" -> ").unwrap();
            let stem = clip.rsplit_once('.').unwrap().0;
            let want: Vec<String> = if want == "-" { vec![] } else { want.split('|').map(String::from).collect() };
            assert_eq!(low_res_names(stem), want, "{clip}");
            n += 1;
        }
        assert!(n >= 5);
    }

    // The quick-look file: the .LRF, else the .THM, else GoPro's LRV (HERO3/4's too)
    #[test]
    fn camera_thumb_order() {
        let dir = std::env::temp_dir().join(format!("afl-camera-thumb-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let big = vec![b'x'; 2048];
        std::fs::write(dir.join("GOPR1030.LRV"), &big).unwrap();
        assert_eq!(camera_thumb(&dir.join("GOPR1030.MP4")).unwrap().file_name().unwrap(), "GOPR1030.LRV");
        std::fs::write(dir.join("GOPR1030.THM"), &big).unwrap();
        assert_eq!(camera_thumb(&dir.join("GOPR1030.MP4")).unwrap().file_name().unwrap(), "GOPR1030.THM");
        std::fs::write(dir.join("GL010057.LRV"), &big).unwrap();
        assert_eq!(camera_thumb(&dir.join("GX010057.MP4")).unwrap().file_name().unwrap(), "GL010057.LRV");
        // Cut short: not one
        std::fs::write(dir.join("DJI_1_D.LRF"), b"x").unwrap();
        assert!(camera_thumb(&dir.join("DJI_1_D.MP4")).is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }

    // A folder renamed (an album, an album in it) or a file moved: the pictures kept for what's in it are found at the
    // new paths, not made again (Anton, 2026-11-18: a renamed album's previews grey for a long time)
    #[test]
    fn pictures_follow_a_rename() {
        let dir = std::env::temp_dir().join(format!("afl-thumbs-follow-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let (old, cache) = (dir.join("P/_Clips/Day 1"), dir.join("cache"));
        std::fs::create_dir_all(old.join("Dives")).unwrap();
        std::fs::create_dir_all(&cache).unwrap();
        // Files whose picture can't be made (made again, there'd be none): a kept one can only have followed
        let files = ["A.dat", "B.dat", "Dives/C.dat"];
        for (i, f) in files.iter().enumerate() {
            let p = old.join(f);
            std::fs::write(&p, vec![b'x'; 10 + i]).unwrap();
            let (s, m) = stamp(&p.to_string_lossy()).unwrap();
            std::fs::write(cache.join(format!("{}.img", thumb_key(&p, s, m))), format!("picture of {f}")).unwrap();
            assert_eq!(thumb_bytes(&p, &cache).unwrap(), format!("picture of {f}").into_bytes());
        }
        let new = dir.join("P/_Clips/Nile");
        std::fs::rename(&old, &new).unwrap();
        assert_eq!(thumbs_follow(&cache, &old, &new), 3);
        for f in files { assert_eq!(thumb_bytes(&new.join(f), &cache).unwrap(), format!("picture of {f}").into_bytes(), "{f}"); }
        // A file moved into another album
        std::fs::create_dir_all(dir.join("P/_Clips/Day 2")).unwrap();
        let (a, b) = (new.join("A.dat"), dir.join("P/_Clips/Day 2/A.dat"));
        std::fs::rename(&a, &b).unwrap();
        assert_eq!(thumbs_follow(&cache, &a, &b), 1);
        assert_eq!(thumb_bytes(&b, &cache).unwrap(), b"picture of A.dat".to_vec());
        // Nothing kept for it: nothing to follow
        let c = dir.join("P/_Clips/Day 2/C.dat");
        std::fs::rename(new.join("Dives/C.dat"), &c).unwrap();
        std::fs::write(&c, b"changed").unwrap();
        assert_eq!(thumbs_follow(&cache, &new.join("Dives/C.dat"), &c), 0, "a file changed meanwhile: its old picture isn't its");
        let _ = std::fs::remove_dir_all(&dir);
    }

    // A lens saved for one camera: its clips asked again, the others kept as they are
    #[test]
    fn lens_saved_asks_that_camera_again() {
        let dir = std::env::temp_dir().join(format!("afl-lens-saved-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let mut p = Probes::default();
        let mut paths = Vec::new();
        for (name, camera) in [("A.MP4", "DJI OsmoAction6"), ("B.MP4", "DJI Osmo Action 6"), ("C.MP4", "GoPro HERO7 Black"), ("D.MP4", "")] {
            let f = dir.join(name);
            std::fs::write(&f, name).unwrap();
            let path = f.display().to_string();
            let (s, m) = stamp(&path).unwrap();
            p.known.insert(path.clone(), (s, m, "old".into(), json!({ "camera": camera })));
            paths.push(path);
        }
        // One known with an older list still: it stays to be asked again
        p.known.get_mut(&paths[3]).unwrap().2 = "older".into();
        // Something queued before: the camera's clips go ahead of it
        p.queue.push_back("/elsewhere/Z.MP4".into());
        p.queued.insert("/elsewhere/Z.MP4".into());
        p.known.get_mut(&paths[0]).unwrap().3 = json!({ "camera": "DJI OsmoAction6", "width": 3840, "height": 2160, "mode": null, "lens": "" });
        p.known.get_mut(&paths[1]).unwrap().3 = json!({ "camera": "DJI Osmo Action 6", "width": 1920, "height": 1080, "mode": null, "lens": "" });
        let v0 = p.version;
        assert_eq!(p.lens_saved_for("DJI OsmoAction6", "old", Some(("3840x2160", "Any mode, from your list: X"))), 2);
        // The one of that size has it at once (and the page hears of it); the other size waits for its answer
        assert_eq!(p.known[&paths[0]].3["lens"], json!("Any mode, from your list: X"));
        assert_eq!(p.known[&paths[1]].3["lens"], json!(""));
        assert!(p.since(v0)["items"].get(&paths[0]).is_some());
        assert_eq!(p.queue.back().map(String::as_str), Some("/elsewhere/Z.MP4"));
        let now = lens_list_stamp();
        let queued: HashSet<&String> = p.queue.iter().collect();
        assert!(queued.contains(&paths[0]) && queued.contains(&paths[1]) && !queued.contains(&paths[2]));
        assert_eq!(p.known[&paths[2]].2, now);
        assert_eq!(p.known[&paths[3]].2, "older");
        let _ = std::fs::remove_dir_all(&dir);
    }

    // An answer from an older probe: shown at once, and asked again when its clip is wanted
    #[test]
    fn older_answers_shown_until_asked_again() {
        let dir = std::env::temp_dir().join(format!("afl-older-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let clip = dir.join("A.MP4");
        std::fs::write(&clip, b"clip").unwrap();
        let path = clip.display().to_string();
        let (s, m) = stamp(&path).unwrap();
        let file = dir.join("probes.json");
        std::fs::write(&file, json!({ &path: { "s": s, "m": m, "l": "", "v": { "geotag": GEOTAG - 1, "duration": 12.5 } } }).to_string()).unwrap();
        let mut p = Probes::default();
        p.load(&file);
        assert_eq!(p.get(&path).unwrap()["duration"], 12.5, "shown until its new answer comes");
        p.want(&[path.clone()]);
        assert_eq!(p.queue.len(), 1, "asked again");
        let _ = std::fs::remove_dir_all(&dir);
    }

    // The player's lens dialog changed the lens database: the cameras of the lens files that came, changed or went
    #[test]
    fn lens_cameras_changed_in_the_player() {
        let dir = std::env::temp_dir().join(format!("afl-lensdir-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("GoPro_HERO7_Black-linear-2704x1520-59940.json"), br#"{"camera_brand":"GoPro","camera_model":"HERO7 Black"}"#).unwrap();
        let m = |l: &[(&str, &str)]| l.iter().map(|(a, b)| (a.to_string(), b.to_string())).collect::<BTreeMap<_, _>>();
        let before = m(&[("GoPro_HERO4_Black-wide-1920x1080-59940.json", "1:a"), ("DJI_Action6-x.json", "1:a")]);
        let now = m(&[("DJI_Action6-x.json", "1:a"), ("GoPro_HERO7_Black-linear-2704x1520-59940.json", "2:b")]);
        assert_eq!(lens_cameras_changed(&before, &now, Some(&dir)), vec!["GoPro HERO4 Black".to_string(), "GoPro HERO7 Black".to_string()]);
        assert!(lens_cameras_changed(&now, &now, Some(&dir)).is_empty());
        // A camera named either way is the same one (lens_saved_for)
        let mut p = Probes::default();
        p.known.insert("/x/A.MP4".into(), (1, 1, "l".into(), json!({ "camera": "GoPro HERO4 Black" })));
        p.known.insert("/x/B.MP4".into(), (1, 1, "l".into(), json!({ "camera": "DJI Osmo Action 6" })));
        assert_eq!(p.lens_saved_for("GoPro_HERO4 Black", "l", None), 1);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn probes_again_with_another_lens_list() {
        let dir = std::env::temp_dir().join(format!("afl-lenses-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let (clip, list) = (dir.join("A.MP4"), dir.join("curated_lenses.json"));
        std::fs::write(&clip, b"clip").unwrap();
        std::env::set_var("AFP_CURATED_LENSES", &list);
        let path = clip.display().to_string();
        let (s, m) = stamp(&path).unwrap();
        let mut p = Probes::default();
        p.known.insert(path.clone(), (s, m, lens_list_stamp(), json!({ "lens": "" })));
        p.want(&[path.clone()]);
        assert!(p.queue.is_empty(), "the same list: known");
        std::fs::write(&list, br#"{"cameras": {}}"#).unwrap();
        p.want(&[path.clone()]);
        assert_eq!(p.queue.len(), 1, "a list made since: asked again");
        // Its answer is kept until then; a type chosen for it asks again too
        let mut p = Probes::default();
        p.known.insert(path.clone(), (s, m, lens_list_stamp(), json!({ "lens": "" })));
        p.forget(&[path.clone()]);
        assert_eq!(p.queue.len(), 1);
        assert_eq!(p.known(&[path.clone()])["items"][&path]["lens"], "");
        std::env::remove_var("AFP_CURATED_LENSES");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn preview_paths() {
        assert_eq!(clip_preview_relative("Day 1/A.MP4", false), "Day 1/A.afplink.mp4");
        assert_eq!(clip_preview_relative("DCIM/100MEDIA/GX01.MP4", false), "GX01.afplink.mp4");
        assert_eq!(clip_preview_relative("Card/DCIM/DJI_001/D.MP4", false), "Card/D.afplink.mp4");
        assert_eq!(clip_preview_relative("Day 1/A.MP4", true), "A.afplink.mp4");
    }

    /// An MP4 of the smallest kind with the player's link key in it (moov/meta: hdlr mdta, keys, ilst), as
    /// gyroflow-export writes it
    pub fn tests_mp4_with_link(link: &str) -> Vec<u8> {
        let bx = |t: &[u8], body: &[u8]| { let mut v = ((8 + body.len()) as u32).to_be_bytes().to_vec(); v.extend_from_slice(t); v.extend_from_slice(body); v };
        let key = b"ch.actionfootageplayer.link";
        let mut hdlr = vec![0u8; 8];
        hdlr.extend_from_slice(b"mdta");
        hdlr.extend_from_slice(&[0u8; 13]);
        let mut keys = vec![0u8; 4];
        keys.extend_from_slice(&1u32.to_be_bytes());
        keys.extend_from_slice(&((8 + key.len()) as u32).to_be_bytes());
        keys.extend_from_slice(b"mdta");
        keys.extend_from_slice(key);
        let mut data = 1u32.to_be_bytes().to_vec();
        data.extend_from_slice(&[0u8; 4]);
        data.extend_from_slice(link.as_bytes());
        let item = bx(&1u32.to_be_bytes(), &bx(b"data", &data));
        let mut meta = bx(b"hdlr", &hdlr);
        meta.extend(bx(b"keys", &keys));
        meta.extend(bx(b"ilst", &item));
        let mut out = bx(b"ftyp", b"isom\0\0\0\0isom");
        out.extend(bx(b"moov", &bx(b"meta", &meta)));
        out
    }

    #[test]
    fn creation_time_from_mvhd() {
        let bx = |t: &[u8], body: &[u8]| { let mut v = ((8 + body.len()) as u32).to_be_bytes().to_vec(); v.extend_from_slice(t); v.extend_from_slice(body); v };
        let t0 = parse_time("2026-10-22T13:28:42Z").unwrap();
        let mut mvhd = vec![0u8; 4];
        mvhd.extend_from_slice(&((t0 + 2_082_844_800) as u32).to_be_bytes());
        mvhd.extend_from_slice(&[0u8; 92]);
        let mut file = bx(b"ftyp", b"isom\0\0\0\0isom");
        file.extend(bx(b"mdat", &[1u8; 64]));
        file.extend(bx(b"moov", &bx(b"mvhd", &mvhd)));
        let p = std::env::temp_dir().join(format!("afl-created-{}.mp4", std::process::id()));
        std::fs::write(&p, file).unwrap();
        assert_eq!(created(&p), Some(t0));
        let _ = std::fs::remove_file(&p);
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
        assert_eq!(m["items"][0]["dir"], json!("_Moments/Reef"));
        // Anywhere in the project's folder, by the link inside (docs/library-management.md §8): a moment and a clip
        // preview a run left in Generated/, a moment moved into a folder of one's own and renamed; a folder just made
        let mk = |kind: &str| tests_mp4_with_link(&format!(r#"{{"v":1,"kind":"{kind}","clip":"Day 1/B.mov","in":1,"out":2}}"#));
        for d in ["Generated/2026-11-10 14-32 Moments", "Generated/2026-11-10 14-40 Clip previews/Day 1", "Mine/Best", "Empty"] { std::fs::create_dir_all(root.join(d)).unwrap(); }
        std::fs::write(root.join("Generated/2026-11-10 14-32 Moments/Gen.afplink.mp4"), mk("moment")).unwrap();
        std::fs::write(root.join("Mine/Best/renamed.mp4"), mk("moment")).unwrap();
        std::fs::write(root.join("Mine/Best/One.afplink.mp4"), mk("moment")).unwrap();
        std::fs::write(root.join("Generated/2026-11-10 14-40 Clip previews/Day 1/B.afplink.mp4"), mk("clip")).unwrap();
        let m = scan(&root, "moments", false);
        let ids: Vec<&str> = m["items"].as_array().unwrap().iter().map(|i| i["id"].as_str().unwrap()).collect();
        // This project is next to its footage: among it a plain .mp4 isn't read (not every clip), a .afplink.mp4 is
        assert_eq!(ids, ["Generated/2026-11-10 14-32 Moments/Gen.afplink.mp4", "Mine/Best/One.afplink.mp4", "_Moments/Reef/Turtle.afplink.mp4"]);
        assert_eq!(m["items"][1]["name"], json!("One"));
        assert!(m["dirs"].as_array().unwrap().contains(&json!("Empty")) && !m["dirs"].as_array().unwrap().contains(&json!("Day 1")));
        let f = scan(&root, "footage", false);
        let b = f["items"].as_array().unwrap().iter().find(|i| i["id"] == "Day 1/B.mov").unwrap();
        assert_eq!(b["pdir"], json!("Generated/2026-11-10 14-40 Clip previews/Day 1"));
        assert_eq!(render_path(&root, "Mine/Best/renamed.mp4", "HDR"), root.join("_Renders/Mine/Best/renamed.HDR.mov"));
        // A project apart from its footage: anything in its folder is its own, a plain .mp4 too
        std::fs::write(root.join("Project.afproject"), r#"{"locations":[{"at":"","path":"../elsewhere"}]}"#).unwrap();
        let m = scan(&root, "moments", false);
        assert!(m["items"].as_array().unwrap().iter().any(|i| i["id"] == "Mine/Best/renamed.mp4"));
        assert_eq!(render_path(&root, "_Moments/Reef/Turtle.afplink.mp4", "SDR"), root.join("_Renders/Reef/Turtle.SDR.mov"));
        let s = scan(&root, "stills", false);
        let forms: Vec<&str> = s["items"].as_array().unwrap().iter().map(|i| i["form"].as_str().unwrap()).collect();
        assert_eq!(forms, ["Live Photo", "Motion Photo"]);
        // A photo's link, read from its XMP
        std::fs::write(root.join("_Stills/B.MP.jpg"), "\u{ff}\u{d8}<x:xmpmeta><afp:link>{&quot;v&quot;:1,&quot;kind&quot;:&quot;photo&quot;,&quot;clip&quot;:&quot;Day 1/A &amp; B.MP4&quot;,&quot;t&quot;:2.5}</afp:link></x:xmpmeta>").unwrap();
        let s = scan(&root, "stills", false);
        assert_eq!(s["items"][1]["link"]["clip"], json!("Day 1/A & B.MP4"));
        assert_eq!(s["items"][0]["link"], Value::Null);
        assert_eq!(scan(&root, "exports", false)["items"].as_array().unwrap().len(), 1);
        let _ = std::fs::remove_dir_all(&root);
    }
}

// The views read from the project's catalog (seen.rs) are what they are read from the disks
#[cfg(test)]
mod from_the_catalog {
    use super::*;

    #[test]
    fn views_the_same_from_the_catalog_as_from_the_files() {
        let _one = crate::seen::TESTS.lock().unwrap_or_else(|e| e.into_inner());
        let root = std::env::temp_dir().join(format!("afl-seen-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let mk = |kind: &str, clip: &str| tests::tests_mp4_with_link(&format!(r#"{{"v":1,"kind":"{kind}","clip":"{clip}","in":1,"out":2,"tags":["a"]}}"#));
        for d in ["Day 1", "Day 2/Deep/Er", "_Moments/Reef", "_Clips/Day 1", "_Stills", "_Exports", "_Renders/Reef", ".hidden", "Generated/2026-11-10 14-32 Moments",
                  "Generated/2026-11-10 14-40 Clip previews/Day 1", "Mine/Best", "Empty"] { std::fs::create_dir_all(root.join(d)).unwrap(); }
        for f in ["Day 1/A.MP4", "Day 1/A.LRF", "Day 1/B.mov", "Day 2/Deep/Er/C.MP4", "_Stills/A 1.heic", "_Stills/A 1.mov", "_Exports/T.mov", ".hidden/X.MP4",
                  "_Renders/Reef/Turtle.HDR.mov"] {
            std::fs::write(root.join(f), b"x").unwrap();
        }
        std::fs::write(root.join("Project.afproject"), r#"{"id":"seen-test","locations":[{"name":"Here","at":"","path":"."}]}"#).unwrap();
        std::fs::write(root.join("Day 1/A.afp.json"), br#"{"marks":[{"t":1.5}],"view":{"view":"wide"},"tags":["reef"]}"#).unwrap();
        std::fs::write(root.join("Day 1/A.gyroflow"), b"{}").unwrap();
        std::fs::write(root.join("_Moments/Reef/Turtle.afplink.mp4"), mk("moment", "Day 1/A.MP4")).unwrap();
        std::fs::write(root.join("Generated/2026-11-10 14-32 Moments/Gen.afplink.mp4"), mk("moment", "Day 1/B.mov")).unwrap();
        std::fs::write(root.join("Mine/Best/One.afplink.mp4"), mk("moment", "Day 2/Deep/Er/C.MP4")).unwrap();
        std::fs::write(root.join("Generated/2026-11-10 14-40 Clip previews/Day 1/B.afplink.mp4"), mk("clip", "Day 1/B.mov")).unwrap();
        std::fs::write(root.join("_Exports/E.mp4"), mk("export", "Day 1/A.MP4")).unwrap();
        std::fs::write(root.join("_Stills/B.MP.jpg"), "\u{ff}\u{d8}<x:xmpmeta><afp:link>{&quot;v&quot;:1,&quot;kind&quot;:&quot;photo&quot;,&quot;clip&quot;:&quot;Day 1/A.MP4&quot;,&quot;t&quot;:2.5}</afp:link><dc:subject><rdf:Bag><rdf:li>sun</rdf:li></rdf:Bag></dc:subject></x:xmpmeta>").unwrap();
        let root = root.canonicalize().unwrap();
        let views = || ["footage", "moments", "stills", "exports"].map(|v| scan(&root, v, false));
        let from_files = views();
        // The catalog made (as the Library's thread does) and read
        let (roots, ours) = afp_catalog::project::roots(&root);
        let store: std::sync::Arc<dyn afp_catalog::Store> = std::sync::Arc::new(afp_catalog::DiskStore);
        let mut cat = afp_catalog::Catalog::new(store.clone(), store, &root, &root.join(".cache"), "test");
        cat.load(&afp_catalog::Want::All).unwrap();
        cat.sweep(&roots, &ours, &Default::default()).unwrap();
        *crate::seen::SEEN.write().unwrap() = Some(crate::seen::Seen::of(&root, cat));
        // Read from it: the disks not asked (a file removed behind its back still shows: it's what was last seen)
        let from_catalog = views();
        for (a, b) in from_files.iter().zip(from_catalog.iter()) { assert_eq!(a, b); }
        assert_eq!(from_catalog[1]["items"].as_array().unwrap().len(), 3);
        assert_eq!(from_catalog[2]["items"][1]["tags"], json!(["sun"]));
        std::fs::remove_file(root.join("Mine/Best/One.afplink.mp4")).unwrap();
        assert_eq!(scan(&root, "moments", false)["items"].as_array().unwrap().len(), 3);
        *crate::seen::SEEN.write().unwrap() = None;
        assert_eq!(scan(&root, "moments", false)["items"].as_array().unwrap().len(), 2);
        let _ = std::fs::remove_dir_all(&root);
    }
}

// A location on a share not mounted now: its clips as the catalog last saw them, marked offline (not gone)
#[cfg(test)]
mod offline {
    use super::*;

    #[test]
    fn a_share_not_mounted_shows_its_clips_offline() {
        let _one = crate::seen::TESTS.lock().unwrap_or_else(|e| e.into_inner());
        let base = std::env::temp_dir().join(format!("afl-offline-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let (root, share) = (base.join("P"), base.join("share"));
        for d in [root.join("_Clips/Day 1"), share.join("x/Day 1")] { std::fs::create_dir_all(d).unwrap(); }
        std::fs::write(share.join("x/Day 1/A.MP4"), b"clip").unwrap();
        std::fs::write(share.join("x/Day 1/A.afp.json"), br#"{"marks":[{"t":2}],"tags":["reef"]}"#).unwrap();
        std::fs::write(share.join("x/Day 1/A.gyroflow"), b"{}").unwrap();
        // Its marker: the catalog looks only at a location that says it's this project's
        std::fs::write(share.join("x").join(crate::locations::pointer_name(&root)), b"{}").unwrap();
        std::fs::write(root.join("_Clips/Day 1/A.afplink.mp4"), tests::tests_mp4_with_link(r#"{"v":1,"kind":"clip","clip":"Day 1/A.MP4"}"#)).unwrap();
        std::fs::write(root.join("Project.afproject"), r#"{"id":"offline-test","locations":[{"name":"NAS","at":"","path":"//NOHOST-AFL/share/x"}]}"#).unwrap();
        let root = root.canonicalize().unwrap();
        // The catalog made while the share was mounted (here: the share's folder given as where it is)
        let (roots, ours) = afp_catalog::project::roots_with(&root, &[("share".into(), share.canonicalize().unwrap())]);
        assert!(roots.iter().any(|r| r.id == "l://NOHOST-AFL/share/x"));
        let store: std::sync::Arc<dyn afp_catalog::Store> = std::sync::Arc::new(afp_catalog::DiskStore);
        let mut cat = afp_catalog::Catalog::new(store.clone(), store, &root, &root.join(".cache"), "test");
        cat.load(&afp_catalog::Want::All).unwrap();
        cat.sweep(&roots, &ours, &Default::default()).unwrap();
        // …and read now that it isn't
        *crate::seen::SEEN.write().unwrap() = Some(crate::seen::Seen::of(&root, cat));
        let f = scan(&root, "footage", false);
        *crate::seen::SEEN.write().unwrap() = None;
        let items = f["items"].as_array().unwrap();
        assert_eq!(items.len(), 1, "{f}");
        let a = &items[0];
        assert_eq!((a["id"].as_str(), a["offline"].as_bool(), a["copies"][0]["offline"].as_bool()), (Some("Day 1/A.MP4"), Some(true), Some(true)));
        if cfg!(target_os = "macos") { assert_eq!(a["path"], json!("/Volumes/share/x/Day 1/A.MP4")); }
        assert_eq!(a["own"]["marks"][0]["t"], json!(2));
        assert_eq!(a["tags"], json!(["reef"]));
        assert_eq!(a["gyroflow"], json!(true));
        assert!(a["preview"].as_str().unwrap().ends_with("_Clips/Day 1/A.afplink.mp4"));
        assert_eq!(f["locations"][0]["offline"], json!(true));
        assert_eq!(f["locations"][0]["connect"], json!(cfg!(target_os = "macos")));
        // Without a catalog that has looked at it: nothing to show (as before)
        assert!(scan(&root, "footage", false)["items"].as_array().unwrap().is_empty());
        let _ = std::fs::remove_dir_all(&base);
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
                    Some("moment") => it["original"].as_str().map(str::to_string),
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

/// GEOTAG goes with what gyroflow-probe --library answers (#227: the probe told more, GEOTAG wasn't raised, and every
/// clip's older answer stayed cached). The answer's fields are read from vs-gyroflow's sources (no link to it: the
/// Library runs the probe as a process): the keys gyroflow-probe.rs sets on it and writes in its plain answer, the
/// probe's own answer (probe.rs probe_inner), and the fields of what's serialized into it (markers::Movie,
/// geotag::Clock and Fix, probe::LensInfo, lens_pick::LensType). When they change, raise GEOTAG here and the probe's
/// "geotag" (gyroflow-probe.rs) with a line on what the new number means there, then change this test's line
#[cfg(test)]
mod probe_version {
    use super::GEOTAG;

    fn source(rel: &str) -> String {
        std::fs::read_to_string(format!("{}/../vs-gyroflow/src/{rel}", env!("CARGO_MANIFEST_DIR"))).unwrap_or_else(|e| panic!("{rel}: {e}"))
    }

    /// The text of the block that opens at the first { after `start` (strings and // comments passed over)
    fn block<'a>(text: &'a str, start: &str) -> &'a str {
        let at = text.find(start).unwrap_or_else(|| panic!("no {start}"));
        let b = text.as_bytes();
        let open = at + text[at..].find('{').unwrap();
        let (mut depth, mut i) = (0i32, open);
        while i < b.len() {
            match b[i] {
                b'"' => { i += 1; while i < b.len() && b[i] != b'"' { if b[i] == b'\\' { i += 1; } i += 1; } }
                b'/' if b.get(i + 1) == Some(&b'/') => { while i < b.len() && b[i] != b'\n' { i += 1; } }
                b'{' => depth += 1,
                b'}' => { depth -= 1; if depth == 0 { return &text[open..=i]; } }
                _ => {}
            }
            i += 1;
        }
        panic!("{start}: not closed")
    }

    fn ident(s: &str) -> bool { !s.is_empty() && s.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_') }

    /// The keys set as `out["key"] = ` and written as `"key": ` in json!
    fn keys(text: &str) -> Vec<String> {
        let mut out = Vec::new();
        for (i, _) in text.match_indices("out[\"") {
            let rest = &text[i + 5..];
            let Some(end) = rest.find("\"]") else { continue };
            let after = rest[end + 2..].trim_start();
            if after.starts_with('=') && !after.starts_with("==") && ident(&rest[..end]) { out.push(rest[..end].to_string()); }
        }
        for (i, _) in text.match_indices('"') {
            let rest = &text[i + 1..];
            if let Some(end) = rest.find('"') {
                if ident(&rest[..end]) && rest[end + 1..].trim_start().starts_with(':') && !rest[end + 1..].trim_start().starts_with("::") {
                    out.push(rest[..end].to_string());
                }
            }
        }
        out.sort();
        out.dedup();
        out
    }

    /// A struct's fields as serde writes them: #[serde(rename = "x")] as x, #[serde(skip)] left out
    fn fields(text: &str, name: &str) -> String {
        let body = block(text, &format!("pub struct {name} "));
        let (mut out, mut rename, mut skip) = (Vec::new(), None::<String>, false);
        for line in body.lines().map(str::trim) {
            if line.starts_with("#[serde(") {
                if line.contains("skip)") || line.contains("skip,") { skip = true; }
                if let Some(r) = line.split("rename = \"").nth(1).and_then(|r| r.split('"').next()) { rename = Some(r.to_string()); }
            } else if let Some(f) = line.strip_prefix("pub ").and_then(|l| l.split(':').next()) {
                if !skip { out.push(rename.take().unwrap_or_else(|| f.trim().to_string())); }
                rename = None;
                skip = false;
            }
        }
        out.join(" ")
    }

    #[test]
    fn geotag_goes_with_what_the_probe_answers() {
        let tool = source("bin/gyroflow-probe.rs");
        let probe = source("probe.rs");
        let mut answer = keys(block(&tool, "\nfn plain("));
        answer.extend(keys(block(&tool, "    if library {\n")));
        answer.extend(keys(block(&probe, "\nfn probe_inner(")));
        answer.sort();
        answer.dedup();
        let kept = format!("answer: {} / movie: {} / clock: {} / gps: {} / lens_info: {} / lens type: {}", answer.join(" "),
                           fields(&source("markers.rs"), "Movie"), fields(&source("geotag.rs"), "Clock"), fields(&source("geotag.rs"), "Fix"),
                           fields(&probe, "LensInfo"), fields(&source("lens_pick.rs"), "LensType"));
        assert_eq!((GEOTAG, kept.as_str()),
                   (9, "answer: at brand camera clock color error fps frames geotag gps height how in_water lens lens_info lenses marks measured measured_aside measured_error measured_frames mode model motion movie readout_left_out readout_ms reason rig360 rotation spherical stabilized_in_camera times tracks water width / movie: duration created width height turn / clock: utc basis zone_min why / gps: lat lon alt t / lens_info: camera lens types type recorded default mode curated_file curated_error project from mode_key candidates / lens type: id label profile sizes note file_first"),
                   "what gyroflow-probe --library answers changed, or GEOTAG did: they change together (see this test's comment)");
    }

    /// The probe says the same number
    #[test]
    fn the_probes_geotag_is_the_same() {
        let tool = source("bin/gyroflow-probe.rs");
        assert!(tool.contains(&format!("out[\"geotag\"] = {GEOTAG}.into();")), "gyroflow-probe.rs's \"geotag\" isn't {GEOTAG}");
    }
}

/// The recipe of a moment's render as the player makes it (mpv/scripts/workspace/main.lua's render_recipe and its
/// RENDER_VERSION) is render_recipe_of's: the same version, the same fields. A render the player made is current
/// while its link's recipe is the moment's now, as either works it out
#[cfg(test)]
mod render_recipe_twin {
    use super::*;

    /// The keys of a Lua table constructor (`{ a = 1, b = { c = 2 }, ["in"] = 3 }`), nested ones as "b.c"
    fn lua_keys(t: &str) -> Vec<String> {
        let (mut out, mut stack, mut word, mut last): (Vec<String>, Vec<String>, String, Option<String>) = (Vec::new(), Vec::new(), String::new(), None);
        let mut chars = t.chars().peekable();
        while let Some(c) = chars.next() {
            match c {
                '{' => { stack.push(last.take().unwrap_or_default()); word.clear(); }
                '}' => { stack.pop(); if stack.is_empty() { break; } }
                '=' if chars.peek() != Some(&'=') && !word.is_empty() => {
                    let path: Vec<&str> = stack.iter().skip(1).map(String::as_str).chain([word.as_str()]).collect();
                    out.push(path.join("."));
                    last = Some(word.clone());
                    word.clear();
                }
                '[' | '"' | ']' | ' ' | '\n' => {}
                c if c.is_alphanumeric() || c == '_' => word.push(c),
                _ => { word.clear(); if c == ',' { last = None; } }
            }
            if c == ',' { word.clear(); }
        }
        // A table's own key is its fields' parent ("clip" of "clip.hash")
        let parents: Vec<String> = out.iter().filter_map(|k| k.rsplit_once('.').map(|(p, _)| p.to_string())).collect();
        out.retain(|k| !parents.contains(k));
        out.sort();
        out
    }

    #[test]
    fn the_players_render_recipe_is_this_one() {
        let lua = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../mpv/scripts/workspace/main.lua")).unwrap();
        assert!(lua.contains(&format!("\nlocal RENDER_VERSION = {RENDER_VERSION}\n")), "workspace/main.lua's RENDER_VERSION isn't {RENDER_VERSION}");
        let a = lua.find("\nlocal function render_recipe(").expect("workspace/main.lua's render_recipe");
        let body = &lua[a..];
        let t = &body[body.find("trimmed({").unwrap() + "trimmed(".len()..];
        let lua_fields = lua_keys(t);
        let link = json!({ "hash": "h", "size": 1, "in": 1.5, "out": 4.0, "stab": { "view": "wide" }, "alone": true });
        let mut rust_fields = Vec::new();
        fn paths(v: &Value, at: &str, out: &mut Vec<String>) {
            for (k, x) in v.as_object().into_iter().flatten() {
                let p = if at.is_empty() { k.clone() } else { format!("{at}.{k}") };
                // a value of the link's or the clip's own file (stab, input) is one field, whatever it holds
                if x.is_object() && k == "clip" { paths(x, &p, out) } else { out.push(p) }
            }
        }
        paths(&render_recipe_of(&link, json!({ "curve": "dlogm" }), json!("fp"), "HDR"), "", &mut rust_fields);
        rust_fields.sort();
        assert_eq!(lua_fields, rust_fields, "the player's render_recipe and render_recipe_of hold other fields");
        assert_eq!(lua_keys("{ v = 1, clip = { hash = l.hash, size = l.size }, [\"in\"] = l[\"in\"], x = a == b }"), ["clip.hash", "clip.size", "in", "v", "x"]);
    }
}
