// SPDX-License-Identifier: GPL-3.0-or-later
//
// Taking one project into another, once (docs/library-management.md, "Taking one project into another"): the other
// project's locations that are already inside this one's tree move nothing and add nothing; the others are added,
// placed under a folder of the tree chosen for them. Its _Moments, _Clips, _Stills, _Exports, _Renders go into this
// project's, under its place in the tree (a folder is a bin); every path it kept gets that place in front (its links,
// its lists in Project.afproject); its Project.afproject and .afplocation files go to the Trash. Never footage: the
// clips' own files stay beside the clips.

use crate::locations::{ self, Location };
use serde_json::{ json, Value };
use std::path::{ Path, PathBuf };

const OWN: [&str; 6] = ["_Moments", "_Clips", "_Stills", "_Exports", "_Renders", "Generated"];

/// Where a location of the other project lands in this one: its place in this tree ("" the top), and whether it's
/// already inside this project's locations
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Landing { pub name: String, pub dir: Option<String>, pub at_there: String, pub at_here: String, pub inside: bool, pub path_here: String }

fn place_here(dir: &Path, here: &[Location]) -> Option<String> {
    // The deepest location of this project holding the folder: its place plus the path under it
    let mut best: Option<(usize, String)> = None;
    for l in here {
        let Some(d) = &l.dir else { continue };
        if dir == d.as_path() || dir.starts_with(d) {
            let rel = locations::relative(dir, d).unwrap_or_default();
            let n = d.components().count();
            let at = [l.at.as_str(), rel.as_str()].iter().filter(|s| !s.is_empty()).cloned().collect::<Vec<_>>().join("/");
            if best.as_ref().is_none_or(|(b, _)| n > *b) { best = Some((n, at)); }
        }
    }
    best.map(|(_, a)| a)
}

/// The plan: each of its locations' landing, its own folder's place here (where its moments go), and what it has
pub fn plan(root: &Path, other: &Path, chosen: &str) -> Result<Value, String> {
    if root == other { return Err("That's this project".into()); }
    if !other.join(locations::PROJECT_FILE).is_file() { return Err(format!("{} has no {}", other.display(), locations::PROJECT_FILE)); }
    let mounts = locations::mounts();
    let here: Vec<Location> = locations::locations(root, &mounts).into_iter().filter(|l| !l.local).collect();
    let there: Vec<Location> = locations::locations(other, &mounts).into_iter().filter(|l| !l.local).collect();
    let chosen = chosen.trim().trim_matches('/');
    let mut landings = Vec::new();
    for l in &there {
        let Some(dir) = &l.dir else {
            landings.push(Landing { name: l.name.clone(), dir: None, at_there: l.at.clone(), at_here: String::new(), inside: false, path_here: l.path.clone() });
            continue;
        };
        match place_here(dir, &here) {
            // Inside: where its folder already is here; its own place in the other tree is under it
            Some(at) => landings.push(Landing { name: l.name.clone(), dir: Some(dir.display().to_string()), at_there: l.at.clone(), at_here: at, inside: true, path_here: String::new() }),
            None => {
                let at = [chosen, l.at.as_str()].iter().filter(|s| !s.is_empty()).cloned().collect::<Vec<_>>().join("/");
                landings.push(Landing { name: if l.name.is_empty() || l.name == "Project folder" { other.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default() } else { l.name.clone() },
                                        dir: Some(dir.display().to_string()), at_there: l.at.clone(), at_here: at, inside: false,
                                        path_here: locations::path_for(dir, root, &mounts) });
            }
        }
    }
    // Its own folders' place here: where its tree's top lands (the location placed at its top), else the chosen one
    let top = landings.iter().find(|l| l.at_there.is_empty() && l.dir.is_some()).map(|l| l.at_here.clone())
        .unwrap_or_else(|| if chosen.is_empty() { other.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default() } else { chosen.to_string() });
    let count = |sub: &str| -> usize {
        let mut n = 0;
        let mut todo = vec![other.join(sub)];
        while let Some(d) = todo.pop() {
            for e in std::fs::read_dir(&d).into_iter().flatten().flatten() {
                if e.file_name().to_string_lossy().starts_with('.') { continue; }
                if e.path().is_dir() { todo.push(e.path()); } else { n += 1; }
            }
        }
        n
    };
    let counts: serde_json::Map<String, Value> = OWN.iter().map(|s| (s.to_string(), json!(count(s)))).collect();
    Ok(json!({ "other": other.display().to_string(), "name": other.file_name().map(|n| n.to_string_lossy().into_owned()),
               "landings": landings, "place": top, "counts": counts, "needs_place": landings.iter().any(|l| !l.inside && l.dir.is_some()) && chosen.is_empty() }))
}

/// A path of the other project's tree, made one of this project's: the landing of the location holding it
fn map_tree(t: &str, landings: &[Landing]) -> String {
    let mut best: Option<&Landing> = None;
    for l in landings {
        let fits = l.at_there.is_empty() || t == l.at_there || t.starts_with(&format!("{}/", l.at_there));
        if fits && best.is_none_or(|b| l.at_there.len() > b.at_there.len()) { best = Some(l); }
    }
    let Some(l) = best else { return t.to_string() };
    let rest = if l.at_there.is_empty() { t } else { t[l.at_there.len()..].trim_start_matches('/') };
    [l.at_here.as_str(), rest].iter().filter(|s| !s.is_empty()).cloned().collect::<Vec<_>>().join("/")
}

fn unique(dest: PathBuf) -> PathBuf {
    if !dest.exists() { return dest; }
    let name = dest.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let (stem, ext) = match name.find(".afplink.mp4").or_else(|| name.rfind('.')) { Some(i) => (name[..i].to_string(), name[i..].to_string()), None => (name.clone(), String::new()) };
    for n in 2.. {
        let p = dest.with_file_name(format!("{stem} {n}{ext}"));
        if !p.exists() { return p; }
    }
    unreachable!()
}

/// To the Trash where the system has one for the file, else renamed `<name>.taken` beside it. What happened
pub fn to_trash(file: &Path) -> String {
    if cfg!(target_os = "macos") {
        let script = format!("tell application \"Finder\" to delete (POSIX file \"{}\" as alias)", file.display().to_string().replace('"', "\\\""));
        let ok = std::process::Command::new("/usr/bin/osascript").args(["-e", &script]).output().map(|o| o.status.success()).unwrap_or(false);
        if ok && !file.exists() { return "in the Trash".into(); }
    }
    let to = PathBuf::from(format!("{}.taken", file.display()));
    match std::fs::rename(file, &to) { Ok(_) => format!("renamed {}", to.display()), Err(e) => format!("left as it was ({e})") }
}

/// Takes it in. `relink(file, json)` writes a link again. Returns what was done and what couldn't be
pub fn take_in(root: &Path, other: &Path, chosen: &str, relink: &dyn Fn(&Path, &str) -> Result<(), String>, trash: bool) -> Result<Value, String> {
    let p = plan(root, other, chosen)?;
    if p["needs_place"] == json!(true) { return Err("Choose where its other locations go in this project's tree".into()); }
    let landings: Vec<Landing> = serde_json::from_value(json!(p["landings"])).map_err(|e| e.to_string()).unwrap_or_default();
    let place = p["place"].as_str().unwrap_or("").to_string();
    let mut problems = Vec::new();
    let mut moved = 0;
    // Every file and folder it changes, told to the catalog at the end: the views read it, and on a share the
    // system's notice comes late or never
    let mut written: Vec<PathBuf> = Vec::new();
    // Its own folders' files, into this project's under its place, links rewritten
    for sub in OWN {
        let from_top = other.join(sub);
        let mut todo = vec![from_top.clone()];
        while let Some(d) = todo.pop() {
            for e in std::fs::read_dir(&d).into_iter().flatten().flatten() {
                let path = e.path();
                let name = e.file_name().to_string_lossy().into_owned();
                if name.starts_with('.') { continue; }
                if path.is_dir() { todo.push(path); continue; }
                let rel = locations::relative(&path, &from_top).unwrap_or(name.clone());
                let mut dest = root.join(sub);
                for part in place.split('/').filter(|s| !s.is_empty()) { dest.push(part); }
                for part in rel.split('/') { dest.push(part); }
                let dest = unique(dest);
                if let Some(d) = dest.parent() { let _ = std::fs::create_dir_all(d); }
                if std::fs::rename(&path, &dest).is_err() {
                    if let Err(e) = std::fs::copy(&path, &dest).and_then(|_| std::fs::remove_file(&path)) { problems.push(format!("{}: {e}", path.display())); continue; }
                }
                moved += 1;
                written.push(path.clone());
                written.push(dest.clone());
                let lower = dest.to_string_lossy().to_lowercase();
                if lower.ends_with(".mp4") || lower.ends_with(".mov") {
                    if let Some(mut l) = crate::index::link(&dest) {
                        if let Some(c) = l.get("clip").and_then(Value::as_str) {
                            let mapped = map_tree(c, &landings);
                            if mapped != c {
                                l["clip"] = json!(mapped);
                                l["roots"] = json!([root.display().to_string()]);
                                if let Err(e) = relink(&dest, &l.to_string()) { problems.push(format!("{}: link not rewritten ({e})", dest.display())); }
                            }
                        }
                    }
                }
            }
        }
        // Its folders, emptied, removed (only empty ones)
        let mut dirs = Vec::new();
        let mut todo = vec![from_top.clone()];
        while let Some(d) = todo.pop() { for e in std::fs::read_dir(&d).into_iter().flatten().flatten() { if e.path().is_dir() { todo.push(e.path()); } } dirs.push(d); }
        for d in dirs.iter().rev() { let _ = std::fs::remove_file(d.join(".DS_Store")); if std::fs::remove_dir(d).is_ok() { written.push(d.clone()); } }
    }
    // Its lists, mapped, added to this project's
    let theirs = locations::read_json(&other.join(locations::PROJECT_FILE)).unwrap_or(Value::Null);
    let file = root.join(locations::PROJECT_FILE);
    let mut doc = locations::read_json(&file).unwrap_or(json!({}));
    for key in ["clips_made", "stills_made"] {
        let add: Vec<Value> = theirs.get(key).and_then(Value::as_array).into_iter().flatten().filter_map(|v| v.as_str()).map(|line| {
            let (path, rest) = line.split_once('\t').unwrap_or((line, ""));
            json!(format!("{}\t{rest}", map_tree(path, &landings)))
        }).collect();
        if add.is_empty() { continue; }
        let list = doc.as_object_mut().unwrap().entry(key).or_insert(json!([]));
        if !list.is_array() { *list = json!([]); }
        for a in add { if !list.as_array().unwrap().contains(&a) { list.as_array_mut().unwrap().push(a); } }
    }
    if let Some(names) = theirs.get("moment_names").and_then(Value::as_array) {
        let list = doc.as_object_mut().unwrap().entry("moment_names").or_insert(json!([]));
        if list.is_array() { for n in names { if !list.as_array().unwrap().contains(n) { list.as_array_mut().unwrap().push(n.clone()); } } }
    }
    // Its locations outside this one's, added after this project's
    let had: Vec<Value> = doc.get("locations").and_then(Value::as_array).cloned().unwrap_or_default();
    let mut list = had.clone();
    let added: Vec<Value> = landings.iter().filter(|l| !l.inside && l.dir.is_some()).map(|l| json!({ "name": l.name, "at": l.at_here, "path": l.path_here, "copies": false })).collect();
    if !added.is_empty() {
        // A project next to its footage, given locations: its own folder first stays a location
        if list.is_empty() { list.push(json!({ "name": root.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default(), "at": "", "path": ".", "copies": false })); }
        list.extend(added.iter().cloned());
        doc["locations"] = json!(list);
        doc["version"] = json!(2);
    }
    let tmp = file.with_extension("afproject.tmp");
    std::fs::write(&tmp, serde_json::to_vec(&doc).unwrap()).and_then(|_| std::fs::rename(&tmp, &file)).map_err(|e| e.to_string())?;
    let mounts = locations::mounts();
    let after: Vec<Location> = locations::locations(root, &mounts).into_iter().filter(|l| !l.local).collect();
    problems.extend(locations::write_pointers(root, &after, &[], &mounts));
    // Its pointer files out, its project file to the Trash
    let their_pointer = locations::pointer_name(other);
    for l in &landings { if let Some(d) = &l.dir { let f = Path::new(d).join(&their_pointer); if std::fs::remove_file(&f).is_ok() { written.push(f); } } }
    let gone = if trash { to_trash(&other.join(locations::PROJECT_FILE)) } else { "kept".into() };
    written.push(other.join(locations::PROJECT_FILE));
    written.push(file.clone());
    for l in &after { if let Some(d) = &l.dir { written.push(d.join(locations::pointer_name(root))); } }
    crate::seen::note_now(&written);
    Ok(json!({ "moved": moved, "place": place, "added": added.len(), "project_file": gone, "problems": problems }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_laptop_trip_copied_into_the_archive() {
        let top = std::env::temp_dir().join(format!("afl-takein-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&top);
        let (p, a) = (top.join("SSD/Footage"), top.join("HDD/Footage"));
        let trip = a.join("Action 6/2026-10 Egypt");
        for d in [p.join("_Moments/Reef"), trip.join("Day 1"), trip.join("_Moments/Reef"), trip.join("_Stills")] { std::fs::create_dir_all(d).unwrap(); }
        std::fs::write(p.join(locations::PROJECT_FILE), r#"{"locations":[{"name":"Archive","at":"","path":"../../HDD/Footage"}],"clips_made":["x\t1"]}"#).unwrap();
        std::fs::write(trip.join(locations::PROJECT_FILE), r#"{"clips_made":["Day 1/A.MP4\t8"],"moment_names":["Turtle"]}"#).unwrap();
        std::fs::write(trip.join("Day 1/A.MP4"), b"12345678").unwrap();
        std::fs::write(trip.join("Day 1/A.afp.json"), b"{}").unwrap();
        // Its moment, a link inside (written as the player does: the test fakes the relink)
        std::fs::write(trip.join("_Moments/Reef/Turtle.afplink.mp4"), b"not really an mp4").unwrap();
        std::fs::write(p.join("_Moments/Reef/Turtle.afplink.mp4"), b"another").unwrap();
        std::fs::write(trip.join("_Stills/A 1.heic"), b"x").unwrap();
        let pl = plan(&p, &trip, "").unwrap();
        assert_eq!(pl["landings"][0]["inside"], json!(true));
        assert_eq!(pl["place"], json!("Action 6/2026-10 Egypt"));
        assert_eq!(pl["needs_place"], json!(false));
        let r = take_in(&p, &trip, "", &|_, _| Ok(()), false).unwrap();
        assert_eq!(r["moved"], json!(2));
        assert!(p.join("_Moments/Action 6/2026-10 Egypt/Reef/Turtle.afplink.mp4").is_file());
        assert!(p.join("_Stills/Action 6/2026-10 Egypt/A 1.heic").is_file());
        // The footage and its own file stayed
        assert!(trip.join("Day 1/A.MP4").is_file() && trip.join("Day 1/A.afp.json").is_file());
        assert!(!trip.join("_Moments").exists());
        let doc = locations::read_json(&p.join(locations::PROJECT_FILE)).unwrap();
        assert_eq!(doc["clips_made"], json!(["x\t1", "Action 6/2026-10 Egypt/Day 1/A.MP4\t8"]));
        assert_eq!(doc["moment_names"], json!(["Turtle"]));
        assert_eq!(doc["locations"].as_array().unwrap().len(), 1);
        // Outside: added at the chosen place
        let ext = top.join("Card/Trip2");
        std::fs::create_dir_all(&ext).unwrap();
        std::fs::write(ext.join(locations::PROJECT_FILE), "{}").unwrap();
        assert_eq!(plan(&p, &ext, "").unwrap()["needs_place"], json!(true));
        assert!(take_in(&p, &ext, "", &|_, _| Ok(()), false).is_err());
        take_in(&p, &ext, "2025/Trip2", &|_, _| Ok(()), false).unwrap();
        let doc = locations::read_json(&p.join(locations::PROJECT_FILE)).unwrap();
        assert_eq!(doc["locations"][1]["at"], json!("2025/Trip2"));
        assert_eq!(doc["locations"][1]["path"], json!("../../Card/Trip2"));
        assert_eq!(map_tree("Day 1/A.MP4", &[Landing { name: "x".into(), dir: None, at_there: "".into(), at_here: "Action 6/2026-10 Egypt".into(), inside: true, path_here: "".into() }]),
                   "Action 6/2026-10 Egypt/Day 1/A.MP4");
        let _ = std::fs::remove_dir_all(&top);
    }
}
