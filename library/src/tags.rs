// SPDX-License-Identifier: GPL-3.0-or-later
//
// Tags (docs/library-management.md, "Tags"): words on clips, moments, photos and exports, kept with each in the
// library: a clip's in its own file (`tags`, beside the clip, wherever it is over the locations), a moment's, clip
// preview's or export's in its link (written again in place), a photo's in its own XMP (`dc:subject`, written by
// afp-meta). Older versions kept a photo's in Project.afproject (`photo_tags`, by its path in _Stills): taken into
// the photo at its next change, and out of the project's file. The app's own ("Deleted in Photos") are tags like the others.
// A clip, moment or export also has 1–5 stars, `rating` beside `tags` (a photo's aren't kept yet). Finder's tags follow
// the library's on a Mac (afp_catalog::finder), on this computer's choice (locations::finder_tags: own files, all, off).
// The player's twin is mpv/scripts/workspace/tags.lua (K).

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
    ["marks", "view", "source", "view360", "location", "tags", "rating"].iter().all(|k| doc.get(*k).is_none())
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

/// What's done to the items' tags and stars (the player's tags.lua does the same: its apply)
pub enum Op<'a> {
    /// Tags put on and taken off
    Tags { add: &'a [String], remove: &'a [String] },
    /// A tag renamed where it is; to a name it has already: merged into it
    Rename { from: &'a str, to: &'a str },
    /// 1–5 stars, None: none (photos have none: their stars aren't kept yet)
    Rating(Option<u8>),
}

fn list_of(v: &Value) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for t in v.as_array().into_iter().flatten().filter_map(Value::as_str) {
        let t = t.split_whitespace().collect::<Vec<_>>().join(" ");
        if !t.is_empty() && !out.iter().any(|x| x.eq_ignore_ascii_case(&t)) { out.push(t); }
    }
    out
}

/// A tag renamed in a list (merged when the new name is there already); the list unchanged when it hasn't it
pub fn renamed(list: &[String], from: &str, to: &str) -> Vec<String> {
    let (from, to) = (from.trim(), to.trim());
    let mut out = list.to_vec();
    if from.is_empty() || to.is_empty() { return out; }
    let Some(i) = out.iter().position(|x| x.eq_ignore_ascii_case(from)) else { return out };
    match out.iter().position(|x| x.eq_ignore_ascii_case(to)) {
        Some(j) if j != i => { out.remove(i); }
        _ => out[i] = to.to_string(),
    }
    out
}

/// A rating as kept: 1–5, else none
pub fn rating_of(v: &Value) -> Option<u8> { v.as_f64().map(|r| r.round()).filter(|r| *r >= 1.0).map(|r| r.min(5.0) as u8) }

/// `doc`'s `tags` and `rating` (a sidecar, a link) changed by `op`: the tags before and after, or None when nothing
/// changed. Empty tags and no rating: the keys out
fn change(doc: &mut Value, op: &Op) -> Option<(Vec<String>, Vec<String>)> {
    let before = list_of(&doc["tags"]);
    let had = rating_of(&doc["rating"]);
    let (after, rating) = match op {
        Op::Tags { add, remove } => (changed(&json!(before), add, remove), had),
        Op::Rename { from, to } => (renamed(&before, from, to), had),
        Op::Rating(r) => (before.clone(), r.filter(|r| *r >= 1).map(|r| r.min(5))),
    };
    if after == before && rating == had && doc.get("rating").is_none_or(|r| rating_of(r).is_some()) { return None; }
    let o = doc.as_object_mut()?;
    if after.is_empty() { o.remove("tags"); } else { o.insert("tags".into(), json!(after)); }
    match rating { Some(r) => { o.insert("rating".into(), json!(r)); } None => { o.remove("rating"); } }
    Some((before, after))
}

/// Changes the tags of `items` ({ kind, id (its path in its view), path }): `add` and `remove`. `relink` writes a link
/// again. What couldn't be done
pub fn set(root: &Path, items: &[Value], add: &[String], remove: &[String], relink: &dyn Fn(&Path, &str) -> Result<(), String>,
           photo: &dyn Fn(&Path, &[String]) -> Result<(), String>) -> Vec<String> {
    apply(root, items, &Op::Tags { add, remove }, relink, photo, &|f, l, r| afp_catalog::finder::set(f, l, r))
}

/// `op` done to `items` ({ kind, id (its path in its view), path, preview (a clip's preview, for Finder) }), each where
/// it keeps its tags: a clip's own file, a link (`relink` writes it again), a photo's XMP (`photo`). Finder's tags
/// follow on this computer's choice (locations::finder_tags): `finder(file, tags, removed)`. What couldn't be done
pub fn apply(root: &Path, items: &[Value], op: &Op, relink: &dyn Fn(&Path, &str) -> Result<(), String>,
             photo: &dyn Fn(&Path, &[String]) -> Result<(), String>, finder: &dyn Fn(&Path, &[String], &[String]) -> Result<bool, String>) -> Vec<String> {
    let mut problems = Vec::new();
    // What's written, noted to the catalog at the end (the views read it: on a share it'd see them only later)
    let mut written: Vec<PathBuf> = Vec::new();
    let mut moved_in: Vec<String> = Vec::new();
    let old = locations::read_json(&root.join(locations::PROJECT_FILE)).and_then(|d| d.get("photo_tags").cloned()).unwrap_or(json!({}));
    let mode = if cfg!(target_os = "macos") { locations::finder_tags(root) } else { "off".to_string() };
    let locs = locations::locations(root, &locations::mounts());
    let follow = |files: Vec<PathBuf>, before: &[String], after: &[String], problems: &mut Vec<String>| {
        if mode == "off" { return; }
        let removed: Vec<String> = before.iter().filter(|b| !after.iter().any(|a| a.eq_ignore_ascii_case(b))).cloned().collect();
        for f in files.iter().filter(|f| f.is_file()) { if let Err(e) = finder(f, after, &removed) { problems.push(e); } }
    };
    for it in items {
        let id = it["id"].as_str().unwrap_or("");
        let path = PathBuf::from(it["path"].as_str().unwrap_or(""));
        match it["kind"].as_str() {
            Some("clip") => match clip_own_in(&locs, id) {
                Ok((own, clip)) => {
                    let mut doc = locations::read_json(&own).unwrap_or(json!({ "format": "actionfootageplayer-clip", "version": 1 }));
                    if doc.get("format").and_then(Value::as_str).is_some_and(|f| f != "actionfootageplayer-clip" && f != "actionfootageplayer-marks") {
                        problems.push(format!("{}: not the player's file, left alone", own.display())); continue;
                    }
                    let Some((before, after)) = change(&mut doc, op) else { continue };
                    doc["format"] = json!("actionfootageplayer-clip");
                    if doc.get("clip").is_none() {
                        let size = std::fs::metadata(&clip).map(|m| m.len()).unwrap_or(0);
                        doc["clip"] = json!({ "file": clip.file_name().map(|n| n.to_string_lossy().into_owned()), "size": size });
                    }
                    let r = if holds_nothing(&doc) { std::fs::remove_file(&own).or_else(|e| if own.exists() { Err(e) } else { Ok(()) }).map_err(|e| e.to_string()) } else { write_json(&own, &doc) };
                    if let Err(e) = r { problems.push(e); }
                    written.push(own);
                    // Finder: on its clip preview (the clip as the project shows it); with "all", on the clip too
                    let mut files: Vec<PathBuf> = it["preview"].as_str().map(PathBuf::from).into_iter().collect();
                    if mode == "all" { files.push(clip); }
                    follow(files, &before, &after, &mut problems);
                }
                Err(e) => problems.push(e),
            },
            Some("moment") | Some("export") => {
                let Some(mut l) = crate::index::link(&path) else { problems.push(format!("{}: no link inside to keep its tags in", path.display())); continue };
                let Some((before, after)) = change(&mut l, op) else { continue };
                if let Err(e) = relink(&path, &l.to_string()) { problems.push(format!("{}: {e}", path.display())); continue; }
                written.push(path.clone());
                follow(vec![path], &before, &after, &mut problems);
            }
            Some("still") => {
                if matches!(op, Op::Rating(_)) { continue; }
                let mut now = crate::index::photo_tags(&path);
                for t in old.get(id).and_then(Value::as_array).into_iter().flatten().filter_map(Value::as_str) {
                    if !now.iter().any(|x| x.eq_ignore_ascii_case(t)) { now.push(t.to_string()); }
                }
                let mut doc = json!({ "tags": now.clone() });
                let tags = match change(&mut doc, op) { Some((_, after)) => after, None if old.get(id).is_some() => now.clone(), None => continue };
                match photo(&path, &tags) {
                    Ok(()) => if old.get(id).is_some() { moved_in.push(id.to_string()); },
                    Err(e) => { problems.push(format!("{}: {e}", path.display())); continue }
                }
                written.push(path.clone());
                follow(vec![path], &now, &tags, &mut problems);
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
    crate::seen::note_now(&written);
    problems
}

/// The items of the whole project with a tag (every view: footage, moments, photos, exports), as `apply` takes them
pub fn with_tag(root: &Path, tag: &str) -> Vec<Value> {
    let mut out = Vec::new();
    for view in ["footage", "moments", "stills", "exports"] {
        let v = crate::index::scan(root, view, false);
        for it in v["items"].as_array().into_iter().flatten() {
            if it["tags"].as_array().into_iter().flatten().filter_map(Value::as_str).any(|t| t.eq_ignore_ascii_case(tag.trim())) {
                out.push(json!({ "kind": it["kind"], "id": it["id"], "path": it["path"], "preview": it["preview"] }));
            }
        }
    }
    out
}

/// The tags used last on this computer (the player's too: its K offers them), `used` put first
pub fn remember(used: &[String]) {
    let Some(file) = locations::recent_tags_file() else { return };
    if used.is_empty() { return; }
    let before = locations::read_json(&file).map(|d| list_of(&d["recent"])).unwrap_or_default();
    let mut list: Vec<String> = Vec::new();
    for t in used.iter().chain(before.iter()) {
        let t = t.trim();
        if !t.is_empty() && !list.iter().any(|x| x.eq_ignore_ascii_case(t)) { list.push(t.to_string()); }
    }
    list.truncate(12);
    if let Some(d) = file.parent() { let _ = std::fs::create_dir_all(d); }
    let _ = write_json(&file, &json!({ "recent": list }));
}

/// The tags used last on this computer
pub fn recent() -> Vec<String> {
    locations::recent_tags_file().and_then(|f| locations::read_json(&f)).map(|d| list_of(&d["recent"])).unwrap_or_default()
}

/// The lens a clip was shot with, as chosen in the Library (Stabilize optically): `source.lens_type` in each clip's own
/// file ("wide", "normal"... as vs-gyroflow's lens_pick names them; None: what the camera recorded, the key taken out),
/// beside the clip as the player keeps it (clip_own), its other keys kept. What couldn't be done
pub fn set_lens_type(root: &Path, trees: &[String], lens_type: Option<&str>) -> Vec<String> {
    let mut problems = Vec::new();
    let mut written: Vec<PathBuf> = Vec::new();
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
        let empty = holds_nothing(&doc);
        let r = if empty { std::fs::remove_file(&own).or_else(|e| if own.exists() { Err(e) } else { Ok(()) }).map_err(|e| e.to_string()) } else { write_json(&own, &doc) };
        if let Err(e) = r { problems.push(e); }
        written.push(own);
    }
    crate::seen::note_now(&written);
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
    fn renamed_merged_rated() {
        let s = |v: &[&str]| v.iter().map(|x| x.to_string()).collect::<Vec<_>>();
        assert_eq!(renamed(&s(&["a", "turtle", "b"]), "Turtle", "Sea turtle"), s(&["a", "Sea turtle", "b"]));
        assert_eq!(renamed(&s(&["turtle", "reef", "Turtles"]), "turtles", "Turtle"), s(&["turtle", "reef"]));
        assert_eq!(renamed(&s(&["turtle"]), "turtle", "Turtle"), s(&["Turtle"]));
        assert_eq!(renamed(&s(&["reef"]), "turtle", "x"), s(&["reef"]));
        assert_eq!(rating_of(&json!(7)), Some(5));
        assert_eq!(rating_of(&json!(0)), None);
        assert_eq!(rating_of(&Value::Null), None);
        // A doc: unchanged is None; stars and tags out when none
        let mut d = json!({ "tags": ["reef"] });
        assert!(change(&mut d, &Op::Tags { add: &["Reef".into()], remove: &[] }).is_none());
        assert!(change(&mut d, &Op::Rating(Some(3))).is_some());
        assert_eq!(d, json!({ "tags": ["reef"], "rating": 3 }));
        assert!(change(&mut d, &Op::Rename { from: "REEF", to: "Coral reef" }).is_some());
        assert!(change(&mut d, &Op::Rating(None)).is_some());
        assert_eq!(d, json!({ "tags": ["Coral reef"] }));
    }

    /// A project the player tagged (headless, over IPC: K, the tag messages), read as the Library's views show it.
    /// AFL_TAGS_DIR=<the project> AFL_TAGS_WANT=<json: { "footage": {id: {tags, rating}}, "moments": {...} }>
    #[test]
    #[ignore]
    fn reads_what_the_player_tagged() {
        let dir = PathBuf::from(std::env::var("AFL_TAGS_DIR").expect("AFL_TAGS_DIR"));
        let want: Value = serde_json::from_str(&std::env::var("AFL_TAGS_WANT").expect("AFL_TAGS_WANT")).unwrap();
        for (view, items) in want.as_object().unwrap() {
            let got = crate::index::scan(&dir, view, false);
            for (id, w) in items.as_object().unwrap() {
                let it = got["items"].as_array().unwrap().iter().find(|i| i["id"] == id.as_str()).unwrap_or_else(|| panic!("{view}: no {id}"));
                println!("{view} {id}: tags {} rating {}", it["tags"], it["rating"]);
                assert_eq!(it["tags"], w["tags"], "{view} {id}");
                assert_eq!(it["rating"], w["rating"], "{view} {id}");
            }
        }
        // Renamed in the Library: the player's files carry the new name (links through the real afp-cut beside it)
        if let Ok(cut) = std::env::var("AFL_TAGS_CUT") {
            let relink = |f: &Path, l: &str| {
                let o = std::process::Command::new(&cut).arg("--set-key").arg(f).arg("ch.actionfootageplayer.link").arg(l).output().map_err(|e| e.to_string())?;
                if o.status.success() { Ok(()) } else { Err(String::from_utf8_lossy(&o.stderr).into_owned()) }
            };
            let items = with_tag(&dir, "turtle");
            assert!(apply(&dir, &items, &Op::Rename { from: "turtle", to: "Sea turtle" }, &relink, &|_, _| Ok(()), &|f, l, r| afp_catalog::finder::set(f, l, r)).is_empty());
            for view in ["footage", "moments"] {
                for it in crate::index::scan(&dir, view, false)["items"].as_array().unwrap() {
                    println!("after rename, {view} {}: {}", it["id"], it["tags"]);
                    assert!(!it["tags"].as_array().unwrap().iter().any(|t| t == "Turtle"));
                }
            }
        }
    }

    /// Over a project: a clip (its preview gets Finder's tags), a moment and an export (their links), a photo; renamed
    /// everywhere, rated, and the player's form of each read back as the Library's views show it
    #[test]
    fn rename_and_rate_over_the_project() {
        let top = std::env::temp_dir().join(format!("afl-tags-rename-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&top);
        let p = top.join("P");
        for d in [p.join("Day 1"), p.join("_Moments"), p.join("_Exports"), p.join("_Clips/Day 1")] { std::fs::create_dir_all(d).unwrap(); }
        std::fs::write(p.join(locations::PROJECT_FILE), r#"{"format":"actionfootage-project","version":3}"#).unwrap();
        std::fs::write(p.join("Day 1/A.MP4"), b"x").unwrap();
        // As the player writes them (tags.lua): the clip's own file, a moment's link
        std::fs::write(p.join("Day 1/A.afp.json"), r#"{"format":"actionfootageplayer-clip","version":1,"tags":["Turtles","reef"],"rating":4}"#).unwrap();
        let link = |kind: &str| crate::index::tests::tests_mp4_with_link(&format!(r#"{{"v":1,"kind":"{kind}","clip":"Day 1/A.MP4","in":1,"out":2,"tags":["turtles"],"rating":2}}"#));
        std::fs::write(p.join("_Moments/M.afplink.mp4"), link("moment")).unwrap();
        std::fs::write(p.join("_Exports/E.mov"), link("export")).unwrap();
        let clip_preview = crate::index::tests::tests_mp4_with_link(r#"{"v":1,"kind":"clip","clip":"Day 1/A.MP4"}"#);
        std::fs::write(p.join("_Clips/Day 1/A.afplink.mp4"), clip_preview).unwrap();
        // Read as the views show them
        let view = |v: &str, id: &str| crate::index::scan(&p, v, false)["items"].as_array().unwrap().iter().find(|i| i["id"] == id).cloned().unwrap();
        assert_eq!(view("footage", "Day 1/A.MP4")["tags"], json!(["Turtles", "reef"]));
        assert_eq!(view("footage", "Day 1/A.MP4")["rating"], json!(4));
        assert_eq!(view("moments", "_Moments/M.afplink.mp4")["rating"], json!(2));
        // Every item with the tag, whatever its case
        let items = with_tag(&p, "TURTLES");
        assert_eq!(items.len(), 3);
        // Links written by a stand-in for afp-cut; Finder's by a stand-in that records
        let relinked = std::cell::RefCell::new(Vec::<(PathBuf, Value)>::new());
        let relink = |f: &Path, l: &str| { relinked.borrow_mut().push((f.to_path_buf(), serde_json::from_str(l).unwrap())); Ok(()) };
        let finder = std::cell::RefCell::new(Vec::<(PathBuf, Vec<String>, Vec<String>)>::new());
        let fw = |f: &Path, l: &[String], r: &[String]| { finder.borrow_mut().push((f.to_path_buf(), l.to_vec(), r.to_vec())); Ok(true) };
        assert!(apply(&p, &items, &Op::Rename { from: "turtles", to: "Turtle" }, &relink, &|_, _| Ok(()), &fw).is_empty());
        assert_eq!(locations::read_json(&p.join("Day 1/A.afp.json")).unwrap()["tags"], json!(["Turtle", "reef"]));
        assert_eq!(locations::read_json(&p.join("Day 1/A.afp.json")).unwrap()["rating"], json!(4));
        assert_eq!(relinked.borrow().len(), 2);
        assert!(relinked.borrow().iter().all(|(_, l)| l["tags"] == json!(["Turtle"]) && l["rating"] == json!(2)));
        if cfg!(target_os = "macos") {
            // Finder: the clip's preview, the moment, the export; the old name off
            let f = finder.borrow();
            assert!(f.iter().any(|(file, l, r)| file.ends_with("_Clips/Day 1/A.afplink.mp4") && l == &vec!["Turtle".to_string(), "reef".into()] && r == &vec!["Turtles".to_string()]));
            assert!(f.iter().any(|(file, _, _)| file.ends_with("_Moments/M.afplink.mp4")));
            assert!(!f.iter().any(|(file, _, _)| file.ends_with("A.MP4")), "the original isn't touched by default");
        }
        // Stars on the clip: none, then the file keeps only its tags
        assert!(apply(&p, &items[..1], &Op::Rating(None), &relink, &|_, _| Ok(()), &fw).is_empty());
        assert!(locations::read_json(&p.join("Day 1/A.afp.json")).unwrap().get("rating").is_none());
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
