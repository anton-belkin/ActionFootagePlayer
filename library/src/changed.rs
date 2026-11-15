// SPDX-License-Identifier: GPL-3.0-or-later
//
// Pinpoint update (docs/library-responsiveness.md): what a changing command gives the page back, so that the page
// corrects only the items it changed and reads no view again. The items are index.rs's own (scan_only: each exactly
// as the whole view would have it), read after the write from what the catalog was just told, so they are what the
// disk has: the page takes them as they are, whatever it had shown.
//
// The same for what the catalog says changed elsewhere (the player, another computer, a task): `since` turns the
// files that changed into the items they are part of. When that can't be said (too many, a folder, the project's
// file) the page reads its views again, as before.

use crate::index::{ self, SharedProbes };
use crate::locations;
use serde::Serialize;
use serde_json::{ json, Value };
use std::collections::{ BTreeMap, BTreeSet };
use std::path::{ Path, PathBuf };

pub const VIEWS: [&str; 4] = ["footage", "moments", "stills", "exports"];

/// What a command changed. Every changing command of the Library answers with one (the others' answers: `answer`)
#[derive(Serialize, Default, Debug)]
pub struct Changed {
    /// The items as the views have them now, by view ("footage", "moments", "stills", "exports"): each replaces the
    /// page's item of the same id, or is new
    pub items: BTreeMap<String, Vec<Value>>,
    /// Items asked about that aren't in the view (deleted, renamed away, never there): by view, their ids
    pub gone: BTreeMap<String, Vec<String>>,
    /// Files that are gone, whatever they were (a clip preview: the page knows whose)
    pub gone_files: Vec<String>,
    /// Views to read again whole: their folders changed, or what changed can't be said item by item
    pub reload: Vec<String>,
    /// What the clips are, for the items given (as a scan's `probes`)
    pub probes: Value,
    /// What couldn't be done, in words (each names its file)
    pub problems: Vec<String>,
    /// The command's own answer (a renamed file's path, how many went to the Trash)
    pub answer: Value,
}

/// Whether the project's clip previews are laid out flat (main.rs's project_info, without counting its files)
pub fn flat_previews(root: &Path) -> bool {
    let s = locations::read_json(&root.join(locations::PROJECT_FILE)).and_then(|d| d.get("settings").cloned()).unwrap_or(json!({}));
    let flag = s.get("clips_flat").and_then(Value::as_bool).unwrap_or(false);
    s.get("clips_layout").and_then(Value::as_str).filter(|l| ["tree", "flat", "trips"].contains(l)).map(|l| l != "tree").unwrap_or(flag)
}

impl Changed {
    /// These items of a view as they are now: given when there, said gone when not
    pub fn add(&mut self, root: &Path, flat: bool, view: &str, ids: &[String]) {
        let mut ids: Vec<String> = ids.to_vec();
        ids.sort();
        ids.dedup();
        ids.retain(|i| !self.items.get(view).is_some_and(|l| l.iter().any(|it| it["id"] == i.as_str())));
        if ids.is_empty() { return; }
        let found = index::scan_only(root, view, flat, Some(&ids))["items"].as_array().cloned().unwrap_or_default();
        let have: BTreeSet<&str> = found.iter().filter_map(|it| it["id"].as_str()).collect();
        let gone: Vec<String> = ids.iter().filter(|i| !have.contains(i.as_str())).cloned().collect();
        if !gone.is_empty() { let g = self.gone.entry(view.to_string()).or_default(); g.extend(gone); g.sort(); g.dedup(); }
        // A moment whose clip was found away from where its link says: the scan itself writes the link again
        // (main.rs's scan); here it's only left out of what's sent
        let found: Vec<Value> = found.into_iter().map(|mut it| { if let Some(o) = it.as_object_mut() { o.remove("relink"); } it }).collect();
        if !found.is_empty() { self.items.entry(view.to_string()).or_default().extend(found); }
    }

    /// What the clips are, for the items given (asked for, as a scan does, when not known yet)
    pub fn with_probes(mut self, probes: &SharedProbes) -> Changed {
        let mut want: Vec<String> = Vec::new();
        for it in self.items.get("footage").into_iter().flatten() { if let Some(p) = it["path"].as_str() { want.push(p.to_string()); } }
        for it in self.items.get("moments").into_iter().flatten() { if let Some(o) = it["original"].as_str() { want.push(o.to_string()); } }
        let off: BTreeSet<&str> = self.items.get("footage").into_iter().flatten().filter(|it| it["offline"].as_bool() == Some(true)).filter_map(|it| it["path"].as_str()).collect();
        let ask: Vec<String> = want.iter().filter(|p| !off.contains(p.as_str())).cloned().collect();
        self.probes = { let mut p = probes.lock().unwrap(); p.want(&ask); p.known(&want) };
        self
    }

    pub fn reload_all(mut self) -> Changed { self.reload = VIEWS.iter().map(|v| v.to_string()).collect(); self }
}

/// The view an item of a kind is in
pub fn view_of(kind: &str) -> Option<&'static str> {
    match kind { "clip" => Some("footage"), "moment" => Some("moments"), "still" => Some("stills"), "export" => Some("exports"), _ => None }
}

/// What a command on these items ({ kind, id }) changed: each as it is now
pub fn of_items(root: &Path, items: &[Value], problems: Vec<String>) -> Changed {
    let mut by: BTreeMap<&str, Vec<String>> = BTreeMap::new();
    for it in items {
        if let (Some(v), Some(id)) = (it["kind"].as_str().and_then(view_of), it["id"].as_str()) { by.entry(v).or_default().push(id.to_string()); }
    }
    let mut c = Changed { problems, ..Default::default() };
    let flat = flat_previews(root);
    for (v, ids) in by { c.add(root, flat, v, &ids); }
    c
}

/// …on these clips (their paths in the tree)
pub fn of_clips(root: &Path, trees: &[String], problems: Vec<String>) -> Changed {
    let mut c = Changed { problems, ..Default::default() };
    c.add(root, flat_previews(root), "footage", trees);
    c
}

/// A path in the project's folder, as the views name it
fn rel_in(root: &Path, path: &Path) -> Option<String> { locations::relative(path, root) }

/// A file of the project's renamed (main.rs's rename_in): `from` gone from its view, `to` there. A folder: its views
/// read again (what's in it moved with it)
pub fn of_rename(root: &Path, view: &str, kind: &str, from: &Path, to: &str) -> Changed {
    let mut c = Changed { answer: json!(to), ..Default::default() };
    if kind == "folder" { return c.reload_all(); }
    let view = view_of(kind).unwrap_or(view);
    let flat = flat_previews(root);
    for p in [from, Path::new(to)] { if let Some(rel) = rel_in(root, p) { c.add(root, flat, view, &[rel]); } }
    c
}

/// What's to be known of `items` ({ kind, path }) before they're deleted (main.rs's delete_in): the clips whose
/// previews they are
pub fn before_delete(items: &[Value]) -> Vec<String> {
    items.iter().filter(|it| it["kind"] == "preview").filter_map(|it| index::link(Path::new(it["path"].as_str()?))).filter_map(|l| l["clip"].as_str().map(str::to_string)).collect()
}

/// …and what deleting them changed: `answer` is delete_in's, `clips` before_delete's
pub fn of_delete(root: &Path, items: &[Value], clips: &[String], answer: Value) -> Changed {
    let problems = answer["problems"].as_array().into_iter().flatten().filter_map(|p| p.as_str().map(str::to_string)).collect();
    let mut c = Changed { answer, problems, ..Default::default() };
    if items.iter().any(|it| it["kind"] == "folder") { return c.reload_all(); }
    let flat = flat_previews(root);
    c.add(root, flat, "footage", clips);
    for it in items {
        let (Some(view), Some(path)) = (it["kind"].as_str().and_then(view_of), it["path"].as_str()) else { continue };
        if let Some(rel) = rel_in(root, Path::new(path)) { c.add(root, flat, view, &[rel]); }
    }
    c
}

fn lower_name(p: &Path) -> String { p.file_name().map(|n| n.to_string_lossy().to_lowercase()).unwrap_or_default() }
fn is_video(lower: &str) -> bool { lower.ends_with(".mp4") || lower.ends_with(".mov") }
fn is_photo(lower: &str) -> bool { [".heic", ".heif", ".jpg", ".jpeg", ".png"].iter().any(|e| lower.ends_with(e)) }

/// The files beside `file` with its name before the last dot (a clip beside its own file, a photo beside its video)
fn beside(file: &Path, take: &dyn Fn(&str) -> bool) -> Vec<PathBuf> {
    let Some(dir) = file.parent() else { return Vec::new() };
    let name = file.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    // A.afp.json, A.marks.json, A.MP4.gyroflow, A.gyroflow, A.LRF: all A's
    let lower = name.to_lowercase();
    let stem = [".afp.json", ".marks.json", ".mp4.gyroflow", ".mov.gyroflow"].iter().find(|e| lower.ends_with(*e)).map(|e| &name[..name.len() - e.len()])
        .or_else(|| name.rsplit_once('.').map(|(s, _)| s)).unwrap_or(&name).to_lowercase();
    let names = crate::seen::names_in(dir).unwrap_or_else(|| std::fs::read_dir(dir).into_iter().flatten().flatten().map(|e| e.file_name().to_string_lossy().into_owned()).collect());
    names.into_iter().filter(|n| {
        let l = n.to_lowercase();
        let s = l.strip_suffix(index::LINK_EXT).or_else(|| l.rsplit_once('.').map(|(s, _)| s)).unwrap_or(&l);
        s == stem && take(&l)
    }).map(|n| dir.join(n)).collect()
}

/// What these files' changes are to the views (seen.rs's `since`: files written, made or removed here or elsewhere):
/// the items they're part of, as they are now. None: it can't be said item by item (the project's file, a folder, a
/// clip that came or went, too many): the views are read again
pub fn of_files(root: &Path, files: &[PathBuf]) -> Option<Changed> {
    let locs = locations::locations(root, &locations::mounts());
    let footage = index::footage_dirs(root);
    let mut by: BTreeMap<&str, BTreeSet<String>> = BTreeMap::new();
    let mut c = Changed::default();
    // A clip's path in the tree, from where it is in a location
    let tree_of = |p: &Path| -> Option<String> {
        locs.iter().filter_map(|l| { let d = l.dir.as_ref()?; let rel = locations::relative(p, d)?; Some((d.components().count(), if l.at.is_empty() { rel } else { format!("{}/{rel}", l.at) })) })
            .max_by_key(|(n, _)| *n).map(|(_, t)| t)
    };
    for p in files {
        let lower = lower_name(p);
        if lower.starts_with('.') || lower.ends_with(".tmp") { continue; }
        if *p == root.join(locations::PROJECT_FILE) { return None; }
        let there = crate::seen::file(p).map(|f| f.map(|f| f.dir)).unwrap_or_else(|| std::fs::metadata(p).ok().map(|m| m.is_dir()));
        // A folder made, renamed or removed (or something gone that had no file's name): the views' folders changed
        if there == Some(true) || (there.is_none() && !lower.contains('.')) { return None; }
        let own = p.starts_with(root) && (index::own_area(root, p, &footage) || lower.ends_with(index::LINK_EXT));
        if own {
            let rel = rel_in(root, p)?;
            if let Some(base) = rel.strip_prefix("_Renders/") {
                // A render: its moment's (index.rs's render_path, backwards)
                let Some(base) = index::RENDER_PROFILES.iter().find_map(|pr| base.strip_suffix(&format!(".{pr}.mov"))) else { continue };
                for m in [format!("_Moments/{base}{}", index::LINK_EXT), format!("{base}{}", index::LINK_EXT)] {
                    if crate::seen::is_file(&root.join(&m)) { by.entry("moments").or_default().insert(m); }
                }
            } else if is_video(&lower) {
                // A moment, an export, a clip preview (its clip's), or a Live Photo's video (its photo's)
                if there.is_none() { c.gone_files.push(p.display().to_string()); }
                for v in ["moments", "exports"] { by.entry(v).or_default().insert(rel.clone()); }
                if let Some(clip) = index::link(p).filter(|l| l["kind"] == "clip").and_then(|l| l["clip"].as_str().map(str::to_string)) { by.entry("footage").or_default().insert(clip); }
                for s in beside(p, &is_photo) { if let Some(r) = rel_in(root, &s) { by.entry("stills").or_default().insert(r); } }
            } else if is_photo(&lower) {
                by.entry("stills").or_default().insert(rel);
            } else if lower.ends_with(".gyroflow") {
                for m in beside(p, &is_video) { if let Some(r) = rel_in(root, &m) { by.entry("moments").or_default().insert(r); } }
            }
            continue;
        }
        // Among the footage: a clip's own file, its Gyroflow project, the camera's quick look: the clip's. A clip
        // itself that came or went: the tree's folders may have changed
        let Some(tree) = tree_of(p) else { continue };
        if is_video(&lower) {
            // (one that came: the page doesn't know its id, and reads Footage again)
            if there.is_none() { return None; }
            by.entry("footage").or_default().insert(tree);
        } else if lower.ends_with(".afp.json") || lower.ends_with(".marks.json") || lower.ends_with(".gyroflow") || lower.ends_with(".lrf") || lower.ends_with(".thm") {
            for clip in beside(p, &|l| is_video(l) && !l.ends_with(index::LINK_EXT)) { if let Some(t) = tree_of(&clip) { by.entry("footage").or_default().insert(t); } }
        }
    }
    if by.values().map(|s| s.len()).sum::<usize>() > 300 { return None; }
    let flat = flat_previews(root);
    for (v, ids) in by { c.add(root, flat, v, &ids.into_iter().collect::<Vec<_>>()); }
    Some(c)
}

// What a command says it changed against a reading of the whole view after it: the same items, to the last field.
// On perf.rs's project (smaller), every command that answers with a Changed
#[cfg(test)]
mod tests {
    use super::*;
    use crate::perf::{ build, photo_tags, relink };
    use crate::{ geo, seen, tags };

    /// Every item given is the whole view's item of that id; every one said gone isn't in it; `want` were all said
    fn same_as_a_reading(root: &Path, c: &Changed, want: &[(&str, &str)], what: &str) {
        for (view, items) in &c.items {
            let full = index::scan(root, view, false);
            for it in items {
                let whole = full["items"].as_array().unwrap().iter().find(|x| x["id"] == it["id"]);
                assert_eq!(whole, Some(it), "{what}: {view} {}", it["id"]);
            }
        }
        for (view, ids) in &c.gone {
            let full = index::scan(root, view, false);
            for id in ids { assert!(!full["items"].as_array().unwrap().iter().any(|x| x["id"] == id.as_str()), "{what}: {view} {id} is said gone and is there"); }
        }
        for (view, id) in want {
            let given = c.items.get(*view).is_some_and(|l| l.iter().any(|it| it["id"] == *id));
            let gone = c.gone.get(*view).is_some_and(|l| l.iter().any(|g| g == id));
            assert!(given || gone, "{what}: nothing said of {view} {id}");
        }
        assert!(c.reload.is_empty(), "{what}: asks for {:?} to be read again", c.reload);
    }

    #[test]
    fn what_a_command_says_is_what_a_reading_shows() {
        let _one = seen::TESTS.lock().unwrap_or_else(|e| e.into_inner());
        let base = std::env::temp_dir().join(format!("afl-changed-{}", std::process::id()));
        let r = build(&base, 300);
        let p = &r.p;
        let (clip, clip2) = (r.clips[1].clone(), r.clips[3].clone());
        let (moment, still, export) = (r.moments[2].clone(), r.stills[1].clone(), r.exports[1].clone());
        // Tags: a clip without its own file yet, one with, a moment, a photo, an export
        let items = vec![r.clip(1), r.clip(3), r.own("moment", &moment), r.own("still", &still), r.own("export", &export)];
        let problems = tags::set(p, &items, &["turtle".into()], &[], &relink, &photo_tags);
        let c = of_items(p, &items, problems);
        assert_eq!(c.items["footage"].len(), 2);
        assert_eq!(c.items["footage"][0]["tags"], json!(["turtle"]));
        same_as_a_reading(p, &c, &[("footage", &clip), ("footage", &clip2), ("moments", &moment), ("stills", &still), ("exports", &export)], "tags");
        // …taken off again: the clip's own file, holding nothing else, goes
        let problems = tags::set(p, &items[..1], &[], &["turtle".into()], &relink, &photo_tags);
        let c = of_items(p, &items[..1], problems);
        assert_eq!(c.items["footage"][0]["own_files"], json!([]));
        same_as_a_reading(p, &c, &[("footage", &clip)], "tag off");
        // A location, Source settings
        let loc = geo::location_value(27.9, 34.3, None, "manual", Some(&json!({ "country": "Egypt" })), None, None);
        let c = of_clips(p, &[clip.clone(), clip2.clone()], geo::set_location(p, &[clip.clone(), clip2.clone()], Some(&loc)));
        assert_eq!(c.items["footage"][0]["own"]["location"]["lat"], json!(27.9));
        same_as_a_reading(p, &c, &[("footage", &clip), ("footage", &clip2)], "location");
        let change: serde_json::Map<String, Value> = [("rotation".to_string(), json!(180))].into_iter().collect();
        let c = of_clips(p, &[clip.clone()], tags::set_source(p, &[(clip.clone(), change)]));
        assert_eq!(c.items["footage"][0]["own"]["source"], json!({ "rotation": 180 }));
        same_as_a_reading(p, &c, &[("footage", &clip)], "source");
        // A clip that isn't the project's: said in words, nothing given for it
        let c = of_clips(p, &["Nowhere/X.MP4".into()], tags::set_source(p, &[("Nowhere/X.MP4".into(), [("rotation".to_string(), json!(90))].into_iter().collect())]));
        assert_eq!((c.problems.len(), c.items.len(), c.gone["footage"].len()), (1, 0, 1));
        // Renamed: a moment, a photo, an export
        for (view, kind, rel, name) in [("moments", "moment", &moment, "Turtle"), ("stills", "still", &still, "Ray"), ("exports", "export", &export, "Shark")] {
            let from = p.join(rel);
            let to = crate::rename_in(p, view, kind, &from.display().to_string(), name).unwrap();
            let c = of_rename(p, view, kind, &from, &to);
            let new = locations::relative(Path::new(&to), p).unwrap();
            assert_eq!(c.gone[view], vec![rel.clone()], "{kind}");
            assert_eq!(c.items[view][0]["id"], json!(new), "{kind}");
            same_as_a_reading(p, &c, &[(view, rel), (view, &new)], "rename");
        }
        // A folder renamed: its views are read again
        let from = p.join("_Moments/Trip 01/Day 02");
        let to = crate::rename_in(p, "moments", "folder", &from.display().to_string(), "Reef").unwrap();
        assert_eq!(of_rename(p, "moments", "folder", &from, &to).reload.len(), 4);
        // Renders deleted
        let rendered = r.moments[8].clone();
        assert_eq!(index::scan_only(p, "moments", false, Some(std::slice::from_ref(&rendered)))["items"][0]["renders"], json!({ "HDR": "stale" }));
        index::delete_renders(p, std::slice::from_ref(&rendered), false);
        let mut c = Changed::default();
        c.add(p, false, "moments", std::slice::from_ref(&rendered));
        assert_eq!(c.items["moments"][0]["renders"], json!({}));
        same_as_a_reading(p, &c, &[("moments", &rendered)], "renders");
        // Deleted: a moment, a photo, a clip's preview (the clip then has none)
        crate::TRASH_REMOVES.store(true, std::sync::atomic::Ordering::Relaxed);
        let preview = p.join("_Clips").join(index::clip_preview_relative(&clip, false));
        let gone = vec![r.own("moment", &r.moments[5]), r.own("still", &r.stills[3]), json!({ "kind": "preview", "path": preview.display().to_string() })];
        let clips = before_delete(&gone);
        assert_eq!(clips, vec![clip.clone()]);
        let c = of_delete(p, &gone, &clips, crate::delete_in(p, &gone));
        crate::TRASH_REMOVES.store(false, std::sync::atomic::Ordering::Relaxed);
        assert_eq!(c.answer["done"], json!(3));
        assert_eq!(c.items["footage"][0]["preview"], Value::Null);
        same_as_a_reading(p, &c, &[("moments", &r.moments[5]), ("stills", &r.stills[3]), ("footage", &clip)], "delete");
        assert_eq!((c.gone["moments"].len(), c.gone["stills"].len()), (1, 1));

        // What the catalog says changed elsewhere, as items: a clip's own file written by the player, a moment made
        // in a folder there is, a render of it, a preview
        let own = r.f.join(&clip2).with_extension("afp.json");
        std::fs::write(&own, br#"{"format":"actionfootageplayer-clip","version":1,"marks":[{"t":2.5},{"t":7}],"tags":["player"]}"#).unwrap();
        let made = p.join("_Moments/Trip 01/Day 01/New one.afplink.mp4");
        std::fs::write(&made, crate::index::tests::tests_mp4_with_link(&format!(r#"{{"v":1,"kind":"moment","clip":"{clip2}","in":2,"out":5,"size":8}}"#))).unwrap();
        let render = p.join("_Renders/Trip 01/Day 01/New one.HDR.mov");
        std::fs::write(&render, crate::index::tests::tests_mp4_with_link(r#"{"v":1,"kind":"render","recipe":{"v":0}}"#)).unwrap();
        let files = vec![own, made.clone(), render];
        seen::note_now(&files);
        let c = of_files(p, &files).unwrap();
        let new = locations::relative(&made, p).unwrap();
        assert_eq!(c.items["footage"][0]["tags"], json!(["player"]));
        assert_eq!(c.items["moments"][0]["renders"], json!({ "HDR": "stale" }));
        same_as_a_reading(p, &c, &[("footage", &clip2), ("moments", &new)], "from elsewhere");
        // …and what can't be said item by item: the project's file, a folder, a clip gone
        assert!(of_files(p, &[p.join(locations::PROJECT_FILE)]).is_none());
        assert!(of_files(p, &[p.join("_Moments/Trip 01")]).is_none());
        let lost = r.f.join(&r.clips[7]);
        std::fs::remove_file(&lost).unwrap();
        seen::note_now(std::slice::from_ref(&lost));
        assert!(of_files(p, &[lost]).is_none());
        // The page's count of changes, and what's logged for it
        assert!(seen::since(0, 1).is_none());
        *seen::SEEN.write().unwrap() = None;
        let _ = std::fs::remove_dir_all(&base);
    }
}
