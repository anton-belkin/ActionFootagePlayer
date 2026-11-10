// SPDX-License-Identifier: GPL-3.0-or-later
//
// Tags (docs/library-management.md, "Tags"): words on clips, moments, photos and exports, kept with each in the
// library: a clip's in its own file (`tags`, beside the clip, wherever it is over the locations), a moment's, clip
// preview's or export's in its link (written again in place), a photo's in its own XMP (`dc:subject`, written by
// afp-meta). Older versions kept a photo's in Project.afproject (`photo_tags`, by its path in _Stills): taken into
// the photo at its next change, and out of the project's file. The app's own ("Deleted in Photos") are tags like the others.

use crate::locations;
use serde_json::{ json, Value };
use std::path::{ Path, PathBuf };

fn changed(old: &Value, add: &[String], remove: &[String]) -> Vec<String> {
    let mut list: Vec<String> = old.as_array().into_iter().flatten().filter_map(|v| v.as_str().map(str::to_string)).collect();
    for a in add { let a = a.trim(); if !a.is_empty() && !list.iter().any(|x| x.eq_ignore_ascii_case(a)) { list.push(a.to_string()); } }
    list.retain(|x| !remove.iter().any(|r| r.eq_ignore_ascii_case(x)));
    list
}

pub(crate) fn write_json(file: &Path, doc: &Value) -> Result<(), String> {
    let tmp = PathBuf::from(format!("{}.tmp", file.display()));
    std::fs::write(&tmp, serde_json::to_vec(doc).unwrap()).and_then(|_| std::fs::rename(&tmp, file)).map_err(|e| format!("{}: {e}", file.display()))
}

/// A clip's own file with nothing of the clip's left in it (as the player's sidecar.lua has it): removed rather than written
pub(crate) fn holds_nothing(doc: &Value) -> bool {
    ["marks", "view", "source", "view360", "location", "tags"].iter().all(|k| doc.get(*k).is_none())
}

/// A clip's own file over the project's locations (workspace.rs's own_file, docs/library-management.md §8): the
/// copies in the library's locations in their order, then copies-only and this computer's own; the main file is beside
/// the first copy that has one, else beside the first copy that isn't copies only or local
pub fn clip_own(root: &Path, tree: &str) -> Result<(PathBuf, PathBuf), String> {
    clip_own_in(&locations::locations(root, &locations::mounts()), tree)
}

/// The same with the project's locations worked out already (many clips at once: the locations once, not per clip)
pub fn clip_own_in(locs: &[locations::Location], tree: &str) -> Result<(PathBuf, PathBuf), String> {
    let mut copies: Vec<(PathBuf, bool)> = locs.iter().filter_map(|l| locations::in_location(tree, l).filter(|f| f.is_file()).map(|f| (f, l.copies || l.local))).collect();
    copies.sort_by_key(|(_, late)| *late);
    let found: Vec<&PathBuf> = copies.iter().map(|(f, _)| f).filter(|f| f.with_extension("afp.json").is_file()).collect();
    let clip = found.first().map(|f| (*f).clone()).or_else(|| copies.iter().find(|(_, c)| !c).map(|(f, _)| f.clone()))
        .or_else(|| copies.first().map(|(f, _)| f.clone())).ok_or(format!("{tree} isn't in the project's locations"))?;
    Ok((clip.with_extension("afp.json"), clip))
}

/// Changes the tags of `items` ({ kind, id (its path in its view), path }): `add` and `remove`. `relink` writes a link
/// again. What couldn't be done
pub fn set(root: &Path, items: &[Value], add: &[String], remove: &[String], relink: &dyn Fn(&Path, &str) -> Result<(), String>,
           photo: &dyn Fn(&Path, &[String]) -> Result<(), String>) -> Vec<String> {
    let mut problems = Vec::new();
    let mut moved_in: Vec<String> = Vec::new();
    let old = locations::read_json(&root.join(locations::PROJECT_FILE)).and_then(|d| d.get("photo_tags").cloned()).unwrap_or(json!({}));
    for it in items {
        let id = it["id"].as_str().unwrap_or("");
        let path = PathBuf::from(it["path"].as_str().unwrap_or(""));
        match it["kind"].as_str() {
            Some("clip") => match clip_own(root, id) {
                Ok((own, clip)) => {
                    let mut doc = locations::read_json(&own).unwrap_or(json!({ "format": "actionfootageplayer-clip", "version": 1 }));
                    if doc.get("format").and_then(Value::as_str).is_some_and(|f| f != "actionfootageplayer-clip" && f != "actionfootageplayer-marks") {
                        problems.push(format!("{}: not the player's file, left alone", own.display())); continue;
                    }
                    let tags = changed(&doc["tags"], add, remove);
                    doc["format"] = json!("actionfootageplayer-clip");
                    if doc.get("clip").is_none() {
                        let size = std::fs::metadata(&clip).map(|m| m.len()).unwrap_or(0);
                        doc["clip"] = json!({ "file": clip.file_name().map(|n| n.to_string_lossy().into_owned()), "size": size });
                    }
                    if tags.is_empty() { doc.as_object_mut().unwrap().remove("tags"); } else { doc["tags"] = json!(tags); }
                    let r = if holds_nothing(&doc) { std::fs::remove_file(&own).or_else(|e| if own.exists() { Err(e) } else { Ok(()) }).map_err(|e| e.to_string()) } else { write_json(&own, &doc) };
                    if let Err(e) = r { problems.push(e); }
                }
                Err(e) => problems.push(e),
            },
            Some("moment") | Some("export") => {
                let Some(mut l) = crate::index::link(&path) else { problems.push(format!("{}: no link inside to keep its tags in", path.display())); continue };
                let tags = changed(&l["tags"], add, remove);
                if tags.is_empty() { l.as_object_mut().map(|o| o.remove("tags")); } else { l["tags"] = json!(tags); }
                if let Err(e) = relink(&path, &l.to_string()) { problems.push(format!("{}: {e}", path.display())); }
            }
            Some("still") => {
                let mut now = crate::index::photo_tags(&path);
                for t in old.get(id).and_then(Value::as_array).into_iter().flatten().filter_map(Value::as_str) {
                    if !now.iter().any(|x| x.eq_ignore_ascii_case(t)) { now.push(t.to_string()); }
                }
                let tags = changed(&json!(now), add, remove);
                match photo(&path, &tags) {
                    Ok(()) => if old.get(id).is_some() { moved_in.push(id.to_string()); },
                    Err(e) => problems.push(format!("{}: {e}", path.display())),
                }
            }
            _ => {}
        }
    }
    // What the photos now keep themselves goes out of the project's file
    if !moved_in.is_empty() {
        let file = root.join(locations::PROJECT_FILE);
        if let Some(mut doc) = locations::read_json(&file) {
            if let Some(o) = doc.get_mut("photo_tags").and_then(Value::as_object_mut) { for id in &moved_in { o.remove(id); } }
            if doc.get("photo_tags").and_then(Value::as_object).is_some_and(|o| o.is_empty()) { doc.as_object_mut().unwrap().remove("photo_tags"); }
            if let Err(e) = write_json(&file, &doc) { problems.push(e); }
        }
    }
    problems
}

/// The lens a clip was shot with, as chosen in the Library (Stabilize optically): `source.lens_type` in each clip's own
/// file ("wide", "normal"... as vs-gyroflow's lens_pick names them; None: what the camera recorded, the key taken out),
/// beside the clip as the player keeps it (clip_own), its other keys kept. What couldn't be done
pub fn set_lens_type(root: &Path, trees: &[String], lens_type: Option<&str>) -> Vec<String> {
    let mut problems = Vec::new();
    for tree in trees {
        let (own, clip) = match clip_own(root, tree) { Ok(x) => x, Err(e) => { problems.push(e); continue } };
        let had = locations::read_json(&own);
        if had.is_none() && lens_type.is_none() { continue; }
        let mut doc = had.unwrap_or(json!({ "format": "actionfootageplayer-clip", "version": 1 }));
        if doc.get("format").and_then(Value::as_str).is_some_and(|f| f != "actionfootageplayer-clip" && f != "actionfootageplayer-marks") {
            problems.push(format!("{}: not the player's file, left alone", own.display())); continue;
        }
        doc["format"] = json!("actionfootageplayer-clip");
        if doc.get("clip").is_none() {
            let size = std::fs::metadata(&clip).map(|m| m.len()).unwrap_or(0);
            doc["clip"] = json!({ "file": clip.file_name().map(|n| n.to_string_lossy().into_owned()), "size": size });
        }
        if !doc.get("source").is_some_and(Value::is_object) { doc["source"] = json!({}); }
        let src = doc["source"].as_object_mut().unwrap();
        match lens_type { Some(t) => { src.insert("lens_type".into(), json!(t)); } None => { src.remove("lens_type"); } }
        if src.is_empty() { doc.as_object_mut().unwrap().remove("source"); }
        let empty = ["marks", "view", "source", "tags"].iter().all(|k| doc.get(*k).is_none());
        let r = if empty { std::fs::remove_file(&own).or_else(|e| if own.exists() { Err(e) } else { Ok(()) }).map_err(|e| e.to_string()) } else { write_json(&own, &doc) };
        if let Err(e) = r { problems.push(e); }
    }
    problems
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lens_type_kept_with_the_clip() {
        let top = std::env::temp_dir().join(format!("afl-lens-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&top);
        let (p, a, w) = (top.join("P"), top.join("Archive"), top.join("Work"));
        for d in [p.clone(), a.join("Day 1"), w.join("Day 1")] { std::fs::create_dir_all(d).unwrap(); }
        std::fs::write(p.join(locations::PROJECT_FILE), r#"{"locations":[{"at":"","path":"../Work","copies":true},{"at":"","path":"../Archive"}]}"#).unwrap();
        for d in [&a, &w] { std::fs::write(d.join("Day 1/A.MP4"), b"x").unwrap(); std::fs::write(d.join("Day 1/B.MP4"), b"y").unwrap(); }
        std::fs::write(a.join("Day 1/A.afp.json"), r#"{"format":"actionfootageplayer-clip","version":1,"marks":[1],"source":{"curve":"dlogm"}}"#).unwrap();
        let trees = vec!["Day 1/A.MP4".to_string(), "Day 1/B.MP4".to_string()];
        assert!(set_lens_type(&p, &trees, Some("normal")).is_empty());
        // Beside the archive's copy (the working copy's location is copies only), the other keys kept
        let a_doc = locations::read_json(&a.join("Day 1/A.afp.json")).unwrap();
        assert_eq!(a_doc["source"], json!({ "curve": "dlogm", "lens_type": "normal" }));
        assert_eq!(a_doc["marks"], json!([1]));
        assert_eq!(locations::read_json(&a.join("Day 1/B.afp.json")).unwrap()["source"]["lens_type"], json!("normal"));
        assert!(!w.join("Day 1/B.afp.json").exists());
        // As the camera recorded: the key out, and a file left with nothing else gone
        assert!(set_lens_type(&p, &trees, None).is_empty());
        assert_eq!(locations::read_json(&a.join("Day 1/A.afp.json")).unwrap()["source"], json!({ "curve": "dlogm" }));
        assert!(!a.join("Day 1/B.afp.json").exists());
        let _ = std::fs::remove_dir_all(&top);
    }

    #[test]
    fn tags_kept_with_each() {
        let top = std::env::temp_dir().join(format!("afl-tags-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&top);
        let (p, a, w) = (top.join("P"), top.join("Archive"), top.join("Work"));
        for d in [p.join("_Stills"), a.join("Day 1"), w.join("Day 1")] { std::fs::create_dir_all(d).unwrap(); }
        std::fs::write(p.join(locations::PROJECT_FILE), r#"{"locations":[{"at":"","path":"../Work","copies":true},{"at":"","path":"../Archive"}]}"#).unwrap();
        for d in [&a, &w] { std::fs::write(d.join("Day 1/A.MP4"), b"x").unwrap(); }
        std::fs::write(a.join("Day 1/A.afp.json"), r#"{"format":"actionfootageplayer-clip","version":1,"marks":[]}"#).unwrap();
        std::fs::write(a.join("Day 1/B.MP4"), b"y").unwrap();
        std::fs::write(w.join("Day 1/B.MP4"), b"y").unwrap();
        let items = vec![json!({ "kind": "clip", "id": "Day 1/A.MP4", "path": w.join("Day 1/A.MP4").display().to_string() }),
                         json!({ "kind": "clip", "id": "Day 1/B.MP4", "path": w.join("Day 1/B.MP4").display().to_string() }),
                         json!({ "kind": "still", "id": "A 1.heic", "path": p.join("_Stills/A 1.heic").display().to_string() })];
        // A photo's tags go to its writer (afp-meta); what the project's file kept for it is taken along, then out
        std::fs::write(p.join("_Stills/A 1.heic"), b"x").unwrap();
        let mut doc = locations::read_json(&p.join(locations::PROJECT_FILE)).unwrap();
        doc["photo_tags"] = json!({ "A 1.heic": ["old"] });
        std::fs::write(p.join(locations::PROJECT_FILE), doc.to_string()).unwrap();
        let written = std::cell::RefCell::new(Vec::new());
        let photo = |f: &Path, t: &[String]| { written.borrow_mut().push((f.to_path_buf(), t.to_vec())); Ok(()) };
        assert!(set(&p, &items, &["turtle".into(), "best".into()], &[], &|_, _| Ok(()), &photo).is_empty());
        assert_eq!(written.borrow()[0], (p.join("_Stills/A 1.heic"), vec!["old".to_string(), "turtle".into(), "best".into()]));
        assert!(locations::read_json(&p.join(locations::PROJECT_FILE)).unwrap().get("photo_tags").is_none());
        // The archive's own file (the working copy is copies only), marks kept; B's made in the archive
        let a_doc = locations::read_json(&a.join("Day 1/A.afp.json")).unwrap();
        assert_eq!(a_doc["tags"], json!(["turtle", "best"]));
        assert_eq!(a_doc["marks"], json!([]));
        assert!(a.join("Day 1/B.afp.json").is_file() && !w.join("Day 1/B.afp.json").exists());
        // Removed: B's file, holding nothing else, goes
        assert!(set(&p, &items[1..2], &[], &["TURTLE".into(), "best".into()], &|_, _| Ok(()), &|_, _| Ok(())).is_empty());
        assert!(!a.join("Day 1/B.afp.json").exists());
        // Two own files: the archive's is the main one (the working folder is copies only), the other isn't touched
        std::fs::write(w.join("Day 1/A.afp.json"), "{}").unwrap();
        assert!(set(&p, &items[..1], &["x".into()], &[], &|_, _| Ok(()), &|_, _| Ok(())).is_empty());
        assert_eq!(locations::read_json(&a.join("Day 1/A.afp.json")).unwrap()["tags"], json!(["turtle", "best", "x"]));
        assert_eq!(std::fs::read_to_string(w.join("Day 1/A.afp.json")).unwrap(), "{}");
        let _ = std::fs::remove_dir_all(&top);
    }
}
