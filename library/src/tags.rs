// SPDX-License-Identifier: GPL-3.0-or-later
//
// Tags (docs/library-management.md, "Tags"): words on clips, moments, photos and exports, kept with each in the
// library: a clip's in its own file (`tags`, beside the clip, wherever it is over the locations), a moment's, clip
// preview's or export's in its link (written again in place), a photo's in Project.afproject (`photo_tags`, by its
// path in _Stills). The app's own ("Deleted in Photos") are tags like the others.

use crate::locations;
use serde_json::{ json, Value };
use std::path::{ Path, PathBuf };

fn changed(old: &Value, add: &[String], remove: &[String]) -> Vec<String> {
    let mut list: Vec<String> = old.as_array().into_iter().flatten().filter_map(|v| v.as_str().map(str::to_string)).collect();
    for a in add { let a = a.trim(); if !a.is_empty() && !list.iter().any(|x| x.eq_ignore_ascii_case(a)) { list.push(a.to_string()); } }
    list.retain(|x| !remove.iter().any(|r| r.eq_ignore_ascii_case(x)));
    list
}

fn write_json(file: &Path, doc: &Value) -> Result<(), String> {
    let tmp = PathBuf::from(format!("{}.tmp", file.display()));
    std::fs::write(&tmp, serde_json::to_vec(doc).unwrap()).and_then(|_| std::fs::rename(&tmp, file)).map_err(|e| format!("{}: {e}", file.display()))
}

/// A clip's own file over the project's locations (workspace.rs's own_file): the one there is, else beside the clip
/// in the first location that has it and isn't copies only. Err: two of them
fn clip_own(root: &Path, tree: &str) -> Result<(PathBuf, PathBuf), String> {
    let locs = locations::locations(root, &locations::mounts());
    let copies: Vec<(PathBuf, bool)> = locs.iter().filter_map(|l| locations::in_location(tree, l).filter(|f| f.is_file()).map(|f| (f, l.copies))).collect();
    let found: Vec<&PathBuf> = copies.iter().map(|(f, _)| f).filter(|f| f.with_extension("afp.json").is_file()).collect();
    if found.len() > 1 { return Err(format!("{tree} has {} own files: keep one first", found.len())); }
    let clip = found.first().map(|f| (*f).clone()).or_else(|| copies.iter().find(|(_, c)| !c).map(|(f, _)| f.clone()))
        .or_else(|| copies.first().map(|(f, _)| f.clone())).ok_or(format!("{tree} isn't in the project's locations"))?;
    Ok((clip.with_extension("afp.json"), clip))
}

/// Changes the tags of `items` ({ kind, id (its path in its view), path }): `add` and `remove`. `relink` writes a link
/// again. What couldn't be done
pub fn set(root: &Path, items: &[Value], add: &[String], remove: &[String], relink: &dyn Fn(&Path, &str) -> Result<(), String>) -> Vec<String> {
    let mut problems = Vec::new();
    let mut photos: Vec<(String, Vec<String>)> = Vec::new();
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
                    let empty = ["marks", "view", "source", "tags"].iter().all(|k| doc.get(*k).is_none());
                    let r = if empty { std::fs::remove_file(&own).or_else(|e| if own.exists() { Err(e) } else { Ok(()) }).map_err(|e| e.to_string()) } else { write_json(&own, &doc) };
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
            Some("still") => photos.push((id.to_string(), Vec::new())),
            _ => {}
        }
    }
    if !photos.is_empty() {
        let file = root.join(locations::PROJECT_FILE);
        let mut doc = locations::read_json(&file).unwrap_or(json!({}));
        if !doc.get("photo_tags").is_some_and(Value::is_object) { doc["photo_tags"] = json!({}); }
        for (id, _) in photos {
            let tags = changed(&doc["photo_tags"][&id], add, remove);
            if tags.is_empty() { doc["photo_tags"].as_object_mut().unwrap().remove(&id); } else { doc["photo_tags"][&id] = json!(tags); }
        }
        if let Err(e) = write_json(&file, &doc) { problems.push(e); }
    }
    problems
}

#[cfg(test)]
mod tests {
    use super::*;

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
        assert!(set(&p, &items, &["turtle".into(), "best".into()], &[], &|_, _| Ok(())).is_empty());
        // The archive's own file (the working copy is copies only), marks kept; B's made in the archive
        let a_doc = locations::read_json(&a.join("Day 1/A.afp.json")).unwrap();
        assert_eq!(a_doc["tags"], json!(["turtle", "best"]));
        assert_eq!(a_doc["marks"], json!([]));
        assert!(a.join("Day 1/B.afp.json").is_file() && !w.join("Day 1/B.afp.json").exists());
        assert_eq!(locations::read_json(&p.join(locations::PROJECT_FILE)).unwrap()["photo_tags"]["A 1.heic"], json!(["turtle", "best"]));
        // Removed: B's file, holding nothing else, goes
        assert!(set(&p, &items[1..2], &[], &["TURTLE".into(), "best".into()], &|_, _| Ok(())).is_empty());
        assert!(!a.join("Day 1/B.afp.json").exists());
        // Two own files: refused
        std::fs::write(w.join("Day 1/A.afp.json"), "{}").unwrap();
        assert_eq!(set(&p, &items[..1], &["x".into()], &[], &|_, _| Ok(())).len(), 1);
        let _ = std::fs::remove_dir_all(&top);
    }
}
