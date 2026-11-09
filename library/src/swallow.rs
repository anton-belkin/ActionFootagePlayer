// SPDX-License-Identifier: GPL-3.0-or-later
//
// One location swallowing another that is inside it (its folder is in the first one's folder, or is it): the inner one
// goes from the project, and whatever reached clips through it reaches the same files through the outer one. A clip's
// path in the project's tree changes from <inner's place>/<path in it> to <outer's place>/<inner's folder in the
// outer's>/<path in it>, so:
//   - links of moments, clip previews, exports and renders that name the old path are written again (afp-cut
//     --set-key, the file's other keys kept);
//   - a clip preview sitting where the "tree" layout of _Clips put it moves to where that layout puts the new path;
//     two previews of one clip after that (the other was in _Clips/Duplicates because the clip was seen twice, once
//     through each location) leave one: the one outside Duplicates stays, the other goes to the Trash;
//   - the lines of Project.afproject's lists that name clips (clips_made, stills_made) get the new path;
//   - the inner location is taken out of the list and its .afplocation pointer file goes.
// Photos keep their link in their XMP, which nothing here writes again: they're counted, not touched. A link reaches a
// clip through the inner location when the inner one is the first of the library's locations (this computer's own
// copies left out) that has the file at its path; links whose clip isn't found anywhere are left for Find moved files.

use crate::locations::{ self, Location };
use serde_json::{ json, Value };
use std::path::{ Path, PathBuf };

const OWN: [&str; 4] = ["_Moments", "_Clips", "_Exports", "_Renders"];

fn join(parts: &[&str]) -> String {
    parts.iter().filter(|s| !s.is_empty()).cloned().collect::<Vec<_>>().join("/")
}

fn real(d: &Path) -> PathBuf { std::fs::canonicalize(d).unwrap_or_else(|_| d.to_path_buf()) }

/// The inner folder's path inside the outer one ("" the same folder), None: not inside
fn inside(inner: &Path, outer: &Path) -> Option<String> {
    let (i, o) = (real(inner), real(outer));
    if i == o { return Some(String::new()); }
    locations::relative(&i, &o)
}

/// The library's own locations (not this computer's), in their order
fn library_locations(root: &Path) -> Vec<Location> {
    locations::locations(root, &locations::mounts()).into_iter().filter(|l| !l.local).collect()
}

/// What swallowing does: how a tree path is mapped, and the files and lines it touches
struct Swallow { inner: usize, outer: usize, list: Vec<Location>, place: String }

impl Swallow {
    fn new(list: Vec<Location>, outer_path: &str, inner_path: &str) -> Result<Self, String> {
        let find = |p: &str| list.iter().position(|l| l.path == p).ok_or_else(|| format!("{p} isn't one of the project's locations"));
        let (outer, inner) = (find(outer_path)?, find(inner_path)?);
        if outer == inner { return Err("A location can't swallow itself".into()); }
        let od = list[outer].dir.clone().filter(|d| d.is_dir()).ok_or_else(|| format!("{} isn't reachable from this computer", list[outer].name))?;
        let id = list[inner].dir.clone().filter(|d| d.is_dir()).ok_or_else(|| format!("{} isn't reachable from this computer", list[inner].name))?;
        let rel = inside(&id, &od).ok_or_else(|| format!("{} isn't inside {}: only a location inside another can be swallowed by it", list[inner].name, list[outer].name))?;
        let place = join(&[&list[outer].at, &rel]);
        Ok(Swallow { inner, outer, list, place })
    }

    /// The tree path's new one, when it reaches its clip through the inner location
    fn map(&self, t: &str) -> Option<String> {
        let first = self.list.iter().position(|l| locations::in_location(t, l).is_some_and(|f| f.is_file()))?;
        if first != self.inner { return None; }
        let at = &self.list[self.inner].at;
        let rest = if at.is_empty() { t } else { t.strip_prefix(at.as_str())?.trim_start_matches('/') };
        let new = join(&[&self.place, rest]);
        (new != t).then_some(new)
    }
}

fn files(base: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut todo = vec![base.to_path_buf()];
    while let Some(d) = todo.pop() {
        for e in std::fs::read_dir(&d).into_iter().flatten().flatten() {
            if e.file_name().to_string_lossy().starts_with('.') { continue; }
            let p = e.path();
            if p.is_dir() { todo.push(p); } else { out.push(p); }
        }
    }
    out.sort();
    out
}

fn is_video(p: &Path) -> bool { let l = p.to_string_lossy().to_lowercase(); l.ends_with(".mp4") || l.ends_with(".mov") }
fn is_photo(p: &Path) -> bool { let l = p.to_string_lossy().to_lowercase(); [".heic", ".jpg", ".jpeg"].iter().any(|e| l.ends_with(e)) }

#[derive(Debug)]
struct Change { file: PathBuf, link: Value, to: Option<String>, move_to: Option<PathBuf>, trash: bool }

struct Work { changes: Vec<Change>, lines: usize, photos: usize }

fn tree_layout(doc: &Value) -> bool {
    let s = &doc["settings"];
    match s.get("clips_layout").and_then(Value::as_str) { Some("flat") | Some("trips") => false, Some(_) => true, None => !s.get("clips_flat").and_then(Value::as_bool).unwrap_or(false) }
}

fn work(root: &Path, sw: &Swallow) -> Work {
    let doc = locations::read_json(&root.join(locations::PROJECT_FILE)).unwrap_or(Value::Null);
    let tree = tree_layout(&doc);
    let clips = root.join("_Clips");
    let mut changes = Vec::new();
    for sub in OWN {
        for f in files(&root.join(sub)) {
            if !is_video(&f) { continue; }
            let Some(link) = crate::index::link(&f) else { continue };
            let Some(c) = link.get("clip").and_then(Value::as_str).map(str::to_string) else { continue };
            let to = sw.map(&c);
            let mut move_to = None;
            if sub == "_Clips" && tree {
                if let (Some(to), Some(rel)) = (&to, locations::relative(&f, &clips)) {
                    if rel == crate::index::clip_preview_relative(&c, false) {
                        let mut dest = clips.clone();
                        for part in crate::index::clip_preview_relative(to, false).split('/') { dest.push(part); }
                        if dest != f { move_to = Some(dest); }
                    }
                }
            }
            if to.is_some() || sub == "_Clips" { changes.push(Change { file: f, link, to, move_to, trash: false }); }
        }
    }
    // Two previews of one clip now: the one outside _Clips/Duplicates stays (the first such), the others go
    let dup = clips.join("Duplicates");
    let mut by_clip: std::collections::HashMap<String, Vec<usize>> = std::collections::HashMap::new();
    for (i, c) in changes.iter().enumerate() {
        if !c.file.starts_with(&clips) { continue; }
        let clip = c.to.clone().or_else(|| c.link.get("clip").and_then(Value::as_str).map(str::to_string)).unwrap_or_default();
        by_clip.entry(clip).or_default().push(i);
    }
    for (_, ix) in by_clip {
        if ix.len() < 2 || ix.iter().all(|&i| changes[i].to.is_none()) { continue; }
        let keep = ix.iter().copied().find(|&i| !changes[i].file.starts_with(&dup)).unwrap_or(ix[0]);
        for i in ix { if i != keep { changes[i].trash = true; changes[i].move_to = None; } }
    }
    changes.retain(|c| c.to.is_some() || c.trash);
    let mut lines = 0;
    for key in ["clips_made", "stills_made"] {
        for line in doc.get(key).and_then(Value::as_array).into_iter().flatten().filter_map(Value::as_str) {
            if sw.map(line.split_once('\t').map_or(line, |(p, _)| p)).is_some() { lines += 1; }
        }
    }
    let photos = files(&root.join("_Stills")).iter().filter(|f| is_photo(f))
        .filter(|f| crate::index::link(f).and_then(|l| l.get("clip").and_then(Value::as_str).map(str::to_string)).is_some_and(|c| sw.map(&c).is_some())).count();
    Work { changes, lines, photos }
}

/// For the selected location (by its path as the project writes it): the library's locations inside it, each with
/// what swallowing it would change; and the ones it is inside itself
pub fn plan(root: &Path, outer_path: &str) -> Result<Value, String> {
    let list = library_locations(root);
    let o = list.iter().find(|l| l.path == outer_path).ok_or("That isn't one of the project's own locations (this computer's copies are set in Project settings)")?;
    let od = o.dir.clone().filter(|d| d.is_dir()).ok_or_else(|| format!("{} isn't reachable from this computer", o.name))?;
    let mut candidates = Vec::new();
    let mut around = Vec::new();
    for l in &list {
        if l.path == outer_path { continue; }
        let Some(d) = l.dir.as_ref().filter(|d| d.is_dir()) else { continue };
        if inside(d, &od).is_some() {
            let sw = Swallow::new(list.clone(), outer_path, &l.path)?;
            let w = work(root, &sw);
            candidates.push(json!({ "name": l.name, "path": l.path, "at": l.at, "dir": d.display().to_string(), "place": sw.place,
                "links": w.changes.iter().filter(|c| c.to.is_some() && !c.trash).count(),
                "moved": w.changes.iter().filter(|c| c.move_to.is_some()).count(),
                "trashed": w.changes.iter().filter(|c| c.trash).count(),
                "lines": w.lines, "photos": w.photos }));
        } else if inside(&od, d).is_some() {
            around.push(json!({ "name": l.name, "path": l.path }));
        }
    }
    Ok(json!({ "name": o.name, "path": o.path, "at": o.at, "dir": od.display().to_string(), "candidates": candidates, "inside_of": around }))
}

/// Swallows it. `relink(file, json)` writes a link again, `trash(file)` sends a file to the Trash (what happened).
/// When a link couldn't be written, the inner location stays (running it again does the rest)
pub fn run(root: &Path, outer_path: &str, inner_path: &str, relink: &dyn Fn(&Path, &str) -> Result<(), String>, trash: &dyn Fn(&Path) -> String) -> Result<Value, String> {
    let sw = Swallow::new(library_locations(root), outer_path, inner_path)?;
    let w = work(root, &sw);
    let mut problems = Vec::new();
    let (mut relinked, mut moved, mut trashed) = (0, 0, 0);
    for c in &w.changes {
        if c.trash {
            let r = trash(&c.file);
            if c.file.exists() { problems.push(format!("{}: {r}", c.file.display())); } else { trashed += 1; }
            continue;
        }
        let Some(to) = &c.to else { continue };
        let mut l = c.link.clone();
        l["clip"] = json!(to);
        if let Err(e) = relink(&c.file, &l.to_string()) { problems.push(format!("{}: link not written again ({e})", c.file.display())); continue; }
        relinked += 1;
        if let Some(dest) = &c.move_to {
            let dest = crate::takein::unique(dest.clone());
            if let Some(d) = dest.parent() { let _ = std::fs::create_dir_all(d); }
            match std::fs::rename(&c.file, &dest) {
                Ok(_) => {
                    moved += 1;
                    // Its folder, emptied, goes (only empty ones, up to _Clips)
                    let mut d = c.file.parent().map(Path::to_path_buf);
                    while let Some(dir) = d {
                        if dir == root.join("_Clips") { break; }
                        let _ = std::fs::remove_file(dir.join(".DS_Store"));
                        if std::fs::remove_dir(&dir).is_err() { break; }
                        d = dir.parent().map(Path::to_path_buf);
                    }
                }
                Err(e) => problems.push(format!("{}: not moved ({e})", c.file.display())),
            }
        }
    }
    let file = root.join(locations::PROJECT_FILE);
    let mut doc = locations::read_json(&file).ok_or("Project.afproject can't be read")?;
    let mut lines = 0;
    for key in ["clips_made", "stills_made"] {
        if let Some(list) = doc.get_mut(key).and_then(Value::as_array_mut) {
            for line in list.iter_mut() {
                let Some(s) = line.as_str() else { continue };
                let (p, rest) = s.split_once('\t').unwrap_or((s, ""));
                if let Some(to) = sw.map(p) { *line = json!(if s.contains('\t') { format!("{to}\t{rest}") } else { to }); lines += 1; }
            }
        }
    }
    let gone = sw.list[sw.inner].clone();
    let removed = problems.iter().all(|p| !p.contains("link not written"));
    if removed {
        if let Some(list) = doc.get_mut("locations").and_then(Value::as_array_mut) {
            if let Some(i) = list.iter().position(|l| l.get("path").and_then(Value::as_str) == Some(gone.path.as_str())) { list.remove(i); }
        }
    }
    let tmp = file.with_extension("afproject.tmp");
    std::fs::write(&tmp, serde_json::to_vec(&doc).unwrap()).and_then(|_| std::fs::rename(&tmp, &file)).map_err(|e| e.to_string())?;
    if removed {
        let mounts = locations::mounts();
        problems.extend(locations::write_pointers(root, &library_locations(root), &[gone.clone()], &mounts));
    }
    Ok(json!({ "outer": sw.list[sw.outer].name, "inner": gone.name, "place": sw.place, "relinked": relinked, "moved": moved, "trashed": trashed,
               "lines": lines, "photos": w.photos, "removed": removed, "problems": problems }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;

    #[test]
    fn paths_mapped() {
        let top = std::env::temp_dir().join(format!("afl-swallow-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&top);
        let (p, a) = (top.join("SSD/Footage"), top.join("HDD/Footage"));
        let inbox = a.join("Inbox/OA6");
        for d in [&p, &inbox.join("Day 1"), &a.join("2025")] { std::fs::create_dir_all(d).unwrap(); }
        std::fs::write(inbox.join("Day 1/A.MP4"), b"a").unwrap();
        std::fs::write(a.join("2025/B.MP4"), b"b").unwrap();
        std::fs::write(p.join(locations::PROJECT_FILE), r#"{"locations":[{"name":"Inbox","at":"Inbox","path":"../../HDD/Footage/Inbox/OA6"},{"name":"Archive","at":"","path":"../../HDD/Footage"}]}"#).unwrap();
        let sw = Swallow::new(library_locations(&p), "../../HDD/Footage", "../../HDD/Footage/Inbox/OA6").unwrap();
        assert_eq!(sw.place, "Inbox/OA6");
        assert_eq!(sw.map("Inbox/Day 1/A.MP4").as_deref(), Some("Inbox/OA6/Day 1/A.MP4"));
        assert_eq!(sw.map("Inbox/OA6/Day 1/A.MP4"), None);
        assert_eq!(sw.map("2025/B.MP4"), None);
        assert_eq!(sw.map("Inbox/Day 1/Gone.MP4"), None);
        // The outer one can't be swallowed by the one inside it, nor a location by itself
        assert!(Swallow::new(library_locations(&p), "../../HDD/Footage/Inbox/OA6", "../../HDD/Footage").is_err());
        assert!(Swallow::new(library_locations(&p), "../../HDD/Footage", "../../HDD/Footage").is_err());
        let _ = std::fs::remove_dir_all(&top);
    }

    #[test]
    fn swallowed_for_real() {
        // index::link reads MP4 keys: write minimal MP4s with the key, as afp-cut would
        let top = std::env::temp_dir().join(format!("afl-swallow2-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&top);
        let (p, a) = (top.join("SSD/Footage"), top.join("HDD/Footage"));
        let inbox = a.join("Inbox/OA6");
        for d in [p.join("_Stills"), inbox.join("Day 1"), a.join("2025")] { std::fs::create_dir_all(d).unwrap(); }
        std::fs::write(inbox.join("Day 1/A.MP4"), b"a").unwrap();
        std::fs::write(a.join("2025/B.MP4"), b"b").unwrap();
        std::fs::write(p.join(locations::PROJECT_FILE), serde_json::to_vec(&json!({
            "locations": [{ "name": "Inbox", "at": "Inbox", "path": "../../HDD/Footage/Inbox/OA6" }, { "name": "Archive", "at": "", "path": "../../HDD/Footage" }],
            "clips_made": ["Inbox/Day 1/A.MP4\t1", "2025/B.MP4\t1"] })).unwrap()).unwrap();
        let mk = |rel: &str, clip: &str| {
            let f = p.join(rel);
            std::fs::create_dir_all(f.parent().unwrap()).unwrap();
            std::fs::write(&f, crate::index::tests_mp4_with_link(&json!({ "v": 1, "kind": "clip", "clip": clip }).to_string())).unwrap();
            f
        };
        mk("_Moments/Reef/Turtle.afplink.mp4", "Inbox/Day 1/A.MP4");
        mk("_Moments/Reef/Old.afplink.mp4", "2025/B.MP4");
        mk("_Clips/Inbox/Day 1/A.afplink.mp4", "Inbox/Day 1/A.MP4");
        let dup = mk("_Clips/Duplicates/Archive/Inbox/OA6/Day 1/A.afplink.mp4", "Inbox/OA6/Day 1/A.MP4");
        std::fs::write(p.join("_Stills/A 1.heic"), "<afp:link>{&quot;v&quot;:1,&quot;kind&quot;:&quot;photo&quot;,&quot;clip&quot;:&quot;Inbox/Day 1/A.MP4&quot;}</afp:link>").unwrap();
        // A pointer file in the inbox, as write_pointers leaves it
        locations::write_pointers(&p, &library_locations(&p), &[], &[]);
        assert!(inbox.join("Footage.afplocation").is_file());

        let pl = plan(&p, "../../HDD/Footage").unwrap();
        assert_eq!(pl["candidates"].as_array().unwrap().len(), 1);
        let c = &pl["candidates"][0];
        assert_eq!((c["links"].clone(), c["moved"].clone(), c["trashed"].clone(), c["lines"].clone(), c["photos"].clone()), (json!(2), json!(1), json!(1), json!(1), json!(1)));
        assert_eq!(plan(&p, "../../HDD/Footage/Inbox/OA6").unwrap()["inside_of"][0]["name"], json!("Archive"));

        let written: RefCell<Vec<(PathBuf, String)>> = RefCell::new(Vec::new());
        let relink = |f: &Path, l: &str| { std::fs::write(f, crate::index::tests_mp4_with_link(l)).unwrap(); written.borrow_mut().push((f.to_path_buf(), l.to_string())); Ok(()) };
        let r = run(&p, "../../HDD/Footage", "../../HDD/Footage/Inbox/OA6", &relink, &|f| { std::fs::remove_file(f).unwrap(); "gone".into() }).unwrap();
        assert_eq!(r["problems"], json!([]), "{r}");
        assert_eq!((r["relinked"].clone(), r["moved"].clone(), r["trashed"].clone(), r["lines"].clone(), r["removed"].clone()), (json!(2), json!(1), json!(1), json!(1), json!(true)));
        assert_eq!(crate::index::link(&p.join("_Moments/Reef/Turtle.afplink.mp4")).unwrap()["clip"], json!("Inbox/OA6/Day 1/A.MP4"));
        assert_eq!(crate::index::link(&p.join("_Moments/Reef/Old.afplink.mp4")).unwrap()["clip"], json!("2025/B.MP4"));
        assert!(p.join("_Clips/Inbox/OA6/Day 1/A.afplink.mp4").is_file());
        assert!(!p.join("_Clips/Inbox/Day 1").exists());
        assert!(!dup.exists());
        let doc = locations::read_json(&p.join(locations::PROJECT_FILE)).unwrap();
        assert_eq!(doc["clips_made"], json!(["Inbox/OA6/Day 1/A.MP4\t1", "2025/B.MP4\t1"]));
        assert_eq!(doc["locations"].as_array().unwrap().len(), 1);
        assert!(!inbox.join("Footage.afplocation").exists());
        // Nothing left inside
        assert!(Swallow::new(library_locations(&p), "../../HDD/Footage", "../../HDD/Footage/Inbox/OA6").is_err());
        let _ = std::fs::remove_dir_all(&top);
    }
}
