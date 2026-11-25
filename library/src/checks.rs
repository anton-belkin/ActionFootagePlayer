// SPDX-License-Identifier: GPL-3.0-or-later
//
// Check and Find moved files (docs/library-management.md, "Find moved files", "The same clip twice"), on what's
// selected: a whole big project would take long (fingerprints are 2 MiB read per clip, off a NAS).
//
// What Check finds:
//   own2      a clip with two own files (in two locations): the first location's is the main one, the others are
//             read, never written (docs/library-management.md §8)
//   orphan    a clip's own file with no clip beside it (the clip moved without it)
//   twice     the same clip (size and fingerprint) at two places in the tree
//   missing   a moment's, clip preview's or export's original not found where its link says
// Find moved files looks for the missing originals and the orphans' clips in the project's own locations (files of
// the same size first, then the fingerprint), and fixes what it found: links written again (afp-cut
// --set-key: the file's other keys kept), own files moved beside their clip, the lines in Project.afproject that name the old path.

use crate::locations::{ self, Location };
use serde_json::{ json, Value };
use std::collections::HashMap;
use std::path::{ Path, PathBuf };

/// A file's fingerprint as links keep it: SHA-256 of its first and last MiB (afp-catalog's workspace::fingerprint, which
/// gyroflow-export writes into links)
pub fn fingerprint(path: &Path) -> Option<String> {
    afp_catalog::workspace::fingerprint(path).ok()
}

/// The same, kept while the file's size and time stay (a moved clip searched for again on each look reads nothing)
pub fn fingerprint_kept(path: &Path) -> Option<String> {
    static KEPT: std::sync::Mutex<Option<HashMap<PathBuf, (u64, std::time::SystemTime, Option<String>)>>> = std::sync::Mutex::new(None);
    afp_catalog::store::nas::stat();
    let m = std::fs::metadata(path).ok()?;
    let st = (m.len(), m.modified().ok()?);
    if let Some((s, t, h)) = KEPT.lock().unwrap().get_or_insert_with(HashMap::new).get(path) { if (*s, *t) == st { return h.clone(); } }
    let h = fingerprint(path);
    KEPT.lock().unwrap().get_or_insert_with(HashMap::new).insert(path.to_path_buf(), (st.0, st.1, h.clone()));
    h
}

/// The same with the file's size and time as the project's catalog saw them (ns): nothing asked of the share but the
/// file's two parts, once in a run
pub fn fingerprint_seen(path: &Path, size: u64, mtime: i64) -> Option<String> {
    static KEPT: std::sync::Mutex<Option<HashMap<PathBuf, (u64, i64, Option<String>)>>> = std::sync::Mutex::new(None);
    if let Some((s, t, h)) = KEPT.lock().unwrap().get_or_insert_with(HashMap::new).get(path) { if (*s, *t) == (size, mtime) { return h.clone(); } }
    let h = fingerprint(path);
    KEPT.lock().unwrap().get_or_insert_with(HashMap::new).insert(path.to_path_buf(), (size, mtime, h.clone()));
    h
}

/// SHA-256 of some bytes, in hex (a render's recipe: renders.rs)
pub use afp_catalog::workspace::sha256_hex;

pub struct Clip { pub tree: String, pub path: PathBuf, pub size: u64, pub local: bool }

impl Clip {
    fn name(&self) -> &str { self.tree.rsplit('/').next().unwrap_or(&self.tree) }
}

/// The clip a link names ({ clip, size, created, hash }), by what it is (docs/library-management.md §8), among the
/// project's clips: only those of its size (sources never change), the steps in Anton's order, each taken while it
/// leaves one: name, size and creation time; size and creation time; name and size; size alone. With the link's
/// fingerprint, each step's clips are compared by it (its name's first), so nothing is a guess. `ask`: the last
/// step's clips are given back to choose from when there are several (Find moved files); else only a match by
/// fingerprint, or the one match of a step with the creation time or the name, is taken; with "I never rename my
/// footage" (`never_rename`) only the steps with the name.
/// -> the clip, how it was found, the clips to choose from
pub fn find_clip<'a>(link: &Value, clips: &'a [Clip], ask: bool, never_rename: bool, fp: &mut dyn FnMut(&Path) -> Option<String>,
                     made: &mut dyn FnMut(&Path) -> Option<i64>) -> (Option<&'a Clip>, &'static str, Vec<&'a Clip>) {
    let Some(size) = link["size"].as_u64() else { return (None, "", Vec::new()) };
    let name = link["clip"].as_str().map(|c| c.rsplit('/').next().unwrap_or(c).to_lowercase()).unwrap_or_default();
    let mut cands: Vec<&Clip> = clips.iter().filter(|c| c.size == size).collect();
    if cands.is_empty() { return (None, "", Vec::new()) }
    if let Some(h) = link["hash"].as_str().filter(|h| !h.is_empty()) {
        cands.sort_by_key(|c| (c.name().to_lowercase() != name, c.local));
        let hit = cands.into_iter().find(|c| fp(&c.path).as_deref() == Some(h));
        return (hit, if hit.is_some() { "fingerprint" } else { "" }, Vec::new());
    }
    let want = link["created"].as_str().and_then(crate::index::parse_time);
    let mut times: HashMap<PathBuf, Option<i64>> = HashMap::new();
    let mut at = |c: &Clip| *times.entry(c.path.clone()).or_insert_with(|| made(&c.path));
    let same_name = |c: &Clip| c.name().to_lowercase() == name;
    let steps: [(&'static str, bool, bool); 4] = [("name, size and creation time", true, true), ("size and creation time", false, true),
                                                    ("name and size", true, false), ("size alone", false, false)];
    for (by, by_name, by_time) in steps {
        if by_time && want.is_none() { continue; }
        if !ask && never_rename && !by_name { continue; }
        let hit: Vec<&Clip> = cands.iter().copied().filter(|c| (!by_name || same_name(c)) && (!by_time || at(c) == want)).collect();
        if hit.len() == 1 && (ask || by_time || by_name) { return (Some(hit[0]), by, Vec::new()); }
        if hit.len() > 1 && (by_time || by_name) && !(by_name && by_time) { return (None, by, if ask { hit } else { Vec::new() }); }
        if !by_name && !by_time && ask && hit.len() > 1 { return (None, by, hit); }
    }
    (None, "", Vec::new())
}

fn is_video(name: &str) -> bool {
    let l = name.to_lowercase();
    (l.ends_with(".mp4") || l.ends_with(".mov")) && !l.ends_with(crate::index::LINK_EXT)
}

/// Every clip of the project, at its tree path (all copies), and every own file found on the way: from the project's
/// catalog when it has them (seen.rs: no question to the share for each file), else from the disks
pub fn footage(locs: &[Location]) -> (Vec<Clip>, Vec<(String, PathBuf)>) {
    let mut clips = Vec::new();
    let mut owns = Vec::new();
    for l in locs {
        let Some(dir) = &l.dir else { continue };
        // The project's own folders (registry.json's; index.rs's walk skips # names)
        let skip = |rel: &str, lower: &str| !rel.contains('/') && crate::lists::get().own_folder(lower, false);
        for (r, is_dir, size, _) in crate::index::walk(dir, &skip) {
            if is_dir { continue; }
            let name = r.rsplit('/').next().unwrap_or(&r).to_string();
            let tree = if l.at.is_empty() { r.clone() } else { format!("{}/{r}", l.at) };
            if is_video(&name) {
                clips.push(Clip { tree, path: dir.join(&r), size, local: l.local });
            } else if name.to_lowercase().ends_with(".afp.json") || name.to_lowercase().ends_with(".marks.json") {
                owns.push((tree, dir.join(&r)));
            }
        }
    }
    (clips, owns)
}

fn under(tree: &str, sel: &[String]) -> bool {
    sel.is_empty() || sel.iter().any(|s| tree == s || tree.starts_with(&format!("{s}/")))
}

/// The project's link files (moments, clip previews, exports, renders, photos), wherever they are in its folder: from
/// the catalog when it has them (their links as it read them), else from the disks
fn links(root: &Path) -> Vec<(PathBuf, Value)> {
    let footage = crate::index::footage_dirs(root);
    let mut out = Vec::new();
    for (r, is_dir, _, _) in crate::index::walk(root, &|_, _| false) {
        if is_dir { continue; }
        let name = r.to_lowercase();
        let p = root.join(&r);
        let video = name.ends_with(".mp4") || name.ends_with(".mov");
        let photo = [".heic", ".jpg", ".jpeg"].iter().any(|x| name.ends_with(x));
        // Among the footage only link files by name (not every clip read)
        if !(video || photo) || (!crate::index::own_area(root, &p, &footage) && !name.ends_with(crate::index::LINK_EXT)) { continue; }
        if let Some(l) = crate::index::link(&p) { if l.get("clip").and_then(Value::as_str).is_some() { out.push((p, l)); } }
    }
    out
}

fn found_at(tree: &str, size: Option<u64>, by_tree: &HashMap<String, Vec<&Clip>>) -> bool {
    by_tree.get(tree).is_some_and(|v| v.iter().any(|c| size.is_none_or(|s| s == c.size)))
}

/// Check over the selection: `sel` paths of the footage tree (empty: everything), `links_sel` link files chosen
/// (moments, exports: their originals checked whatever `sel` is)
pub fn check(root: &Path, sel: &[String], links_sel: &[String]) -> Value {
    // Only what's selected: nothing selected, nothing checked (never the whole project on its own)
    if sel.is_empty() && links_sel.is_empty() { return json!({ "issues": [], "clips": 0 }); }
    let mounts = locations::mounts();
    let locs = locations::locations(root, &mounts);
    let (clips, owns) = footage(&locs);
    let mut by_tree: HashMap<String, Vec<&Clip>> = HashMap::new();
    for c in &clips { by_tree.entry(c.tree.clone()).or_default().push(c); }
    let mut issues = Vec::new();
    // Two own files of one clip, and own files with no clip beside them
    let mut owns_of: HashMap<String, Vec<&PathBuf>> = HashMap::new();
    for (tree, p) in &owns {
        let stem = tree.trim_end_matches(".afp.json").trim_end_matches(".marks.json").to_string();
        owns_of.entry(stem).or_default().push(p);
    }
    for (stem, files) in &owns_of {
        let clip = by_tree.keys().find(|t| t.rsplit_once('.').map(|(s, _)| s) == Some(stem.as_str())).cloned();
        let tree = clip.clone().unwrap_or_else(|| stem.clone());
        if sel.is_empty() || !under(&tree, sel) { continue; }
        // Their clip: beside each own file, in the same location (a clip copied elsewhere doesn't count for it)
        for f in files {
            let beside = ["MP4", "mp4", "MOV", "mov"].iter().any(|e| { let b = f.to_string_lossy(); let s = b.trim_end_matches(".afp.json").trim_end_matches(".marks.json"); crate::seen::is_file(Path::new(&format!("{s}.{e}"))) });
            if !beside {
                let doc = locations::read_json(f).unwrap_or(Value::Null);
                issues.push(json!({ "kind": "orphan", "file": f.display().to_string(), "tree": tree,
                                    "size": doc["clip"]["size"], "hash": doc["clip"]["hash"], "created": doc["clip"]["created"] }));
            }
        }
        let with_clip: Vec<&&PathBuf> = files.iter().filter(|f| { let b = f.to_string_lossy(); let s = b.trim_end_matches(".afp.json").trim_end_matches(".marks.json").to_string(); ["MP4", "mp4", "MOV", "mov"].iter().any(|e| crate::seen::is_file(Path::new(&format!("{s}.{e}")))) }).collect();
        if with_clip.len() > 1 && clip.is_some() {
            issues.push(json!({ "kind": "own2", "tree": tree, "files": with_clip.iter().map(|f| f.display().to_string()).collect::<Vec<_>>() }));
        }
    }
    // The same clip at two places in the tree: same size, then the fingerprint
    let mut by_size: HashMap<u64, Vec<&Clip>> = HashMap::new();
    for c in &clips { by_size.entry(c.size).or_default().push(c); }
    let mut prints: HashMap<PathBuf, Option<String>> = HashMap::new();
    let mut fp = |p: &Path| prints.entry(p.to_path_buf()).or_insert_with(|| fingerprint(p)).clone();
    let mut seen_pairs = std::collections::HashSet::new();
    for group in by_size.values().filter(|g| g.len() > 1) {
        for (i, a) in group.iter().enumerate() {
            for b in &group[i + 1..] {
                if a.tree == b.tree || sel.is_empty() || !(under(&a.tree, sel) || under(&b.tree, sel)) { continue; }
                let key = if a.tree < b.tree { (a.tree.clone(), b.tree.clone()) } else { (b.tree.clone(), a.tree.clone()) };
                if seen_pairs.contains(&key) { continue; }
                if fp(&a.path).is_some() && fp(&a.path) == fp(&b.path) {
                    seen_pairs.insert(key.clone());
                    issues.push(json!({ "kind": "twice", "trees": [key.0, key.1], "files": [a.path.display().to_string(), b.path.display().to_string()] }));
                }
            }
        }
    }
    // Links whose original isn't where they say
    for (file, l) in links(root) {
        let rel = locations::relative(&file, root).unwrap_or_default();
        let tree = l["clip"].as_str().unwrap_or("");
        if !(links_sel.iter().any(|s| rel == *s || rel.starts_with(&format!("{s}/"))) || (!sel.is_empty() && under(tree, sel))) { continue; }
        if !found_at(tree, l["size"].as_u64(), &by_tree) {
            issues.push(json!({ "kind": "missing", "file": file.display().to_string(), "link": l, "tree": tree }));
        }
    }
    json!({ "issues": issues, "clips": clips.len() })
}

/// The links whose original isn't at the path they name, as the project's catalog has the files (no file opened: done
/// on its own as the project changes, Anton 2026-11-10). None when a library location isn't reachable (its clips would
/// all look gone)
pub fn missing(root: &Path) -> Option<Vec<Value>> {
    let mounts = locations::mounts();
    let locs = locations::locations(root, &mounts);
    if locs.iter().any(|l| !l.local && l.dir.is_none()) { return None; }
    let (clips, _) = footage(&locs);
    let mut by_tree: HashMap<String, Vec<&Clip>> = HashMap::new();
    for c in &clips { by_tree.entry(c.tree.clone()).or_default().push(c); }
    let mut out = Vec::new();
    for (file, l) in links(root) {
        let tree = l["clip"].as_str().unwrap_or("").to_string();
        if !found_at(&tree, l["size"].as_u64(), &by_tree) {
            out.push(json!({ "kind": "missing", "file": file.display().to_string(), "link": l, "tree": tree }));
        }
    }
    Some(out)
}

/// Find moved files: for each missing original and orphan own file of `issues`, the clip it's now (find_clip, its
/// steps; the last one's clips to choose from when there are several). [{ issue, found: { tree, path } | null, by,
/// candidates: [{ tree, path }] }]
pub fn find_moved(root: &Path, issues: &[Value]) -> Value {
    let mounts = locations::mounts();
    let locs = locations::locations(root, &mounts);
    let (clips, _) = footage(&locs);
    let mut prints: HashMap<PathBuf, Option<String>> = HashMap::new();
    let mut fp = |p: &Path| prints.entry(p.to_path_buf()).or_insert_with(|| fingerprint_kept(p)).clone();
    let mut made = |p: &Path| crate::index::created(p);
    let mut out = Vec::new();
    for i in issues.iter().filter(|i| matches!(i["kind"].as_str(), Some("missing" | "orphan"))) {
        let link = if i["kind"] == "missing" { i["link"].clone() }
            else { json!({ "clip": i["tree"], "size": i["size"], "hash": i["hash"], "created": i["created"] }) };
        // One clip of its name and size there: it (originals don't change: nothing read); else by what it is
        let name = link["clip"].as_str().unwrap_or("").rsplit('/').next().unwrap_or("").to_lowercase();
        let same: Vec<&Clip> = match link["size"].as_u64() {
            Some(size) if !name.is_empty() => clips.iter().filter(|c| c.size == size && c.tree.rsplit('/').next().unwrap_or("").to_lowercase() == name).collect(),
            _ => Vec::new(),
        };
        let (found, by, choose) = if let [one] = same.as_slice() { (Some(*one), "name and size", Vec::new()) }
            else { find_clip(&link, &clips, true, false, &mut fp, &mut made) };
        let place = |c: &Clip| json!({ "tree": c.tree, "path": c.path.display().to_string() });
        out.push(json!({ "issue": i, "found": found.map(place), "by": by, "candidates": choose.into_iter().map(place).collect::<Vec<_>>() }));
    }
    json!(out)
}

/// The fixes for what Find moved files found: links written again (with `relink`: the tool that writes them), own
/// files moved beside their clip. Returns what couldn't be done
pub fn apply(found: &[Value], relink: &(dyn Fn(&Path, &str) -> Result<(), String> + Sync)) -> Vec<String> {
    let mut problems = Vec::new();
    // The links written first, 8 at once (each a tool started and a file rewritten on the share)
    let writes: Vec<(PathBuf, String)> = found.iter().filter_map(|f| {
        let to = f["found"]["tree"].as_str()?;
        let issue = &f["issue"];
        (issue["kind"] == "missing").then(|| {
            let mut l = issue["link"].clone();
            l["clip"] = json!(to);
            (PathBuf::from(issue["file"].as_str().unwrap_or("")), l.to_string())
        })
    }).collect();
    let results: Vec<Result<(), String>> = {
        let threads = writes.len().clamp(1, 8);
        std::thread::scope(|sc| {
            let hs: Vec<_> = (0..threads).map(|t| {
                let w = &writes;
                sc.spawn(move || w.iter().enumerate().skip(t).step_by(threads).map(|(i, (file, link))| (i, relink(file, link))).collect::<Vec<_>>())
            }).collect();
            let mut all: Vec<(usize, Result<(), String>)> = hs.into_iter().flat_map(|h| h.join().unwrap_or_default()).collect();
            all.sort_by_key(|(i, _)| *i);
            all.into_iter().map(|(_, r)| r).collect()
        })
    };
    // What changed, noted to the catalog at the end (a link written in place leaves its folder's time as it was: its
    // next look wouldn't see it)
    let mut written: Vec<PathBuf> = Vec::new();
    for ((file, _), r) in writes.iter().zip(results) {
        match r { Ok(()) => written.push(file.clone()), Err(e) => problems.push(format!("{}: {e}", file.display())) }
    }
    for f in found {
        if f["found"]["tree"].as_str().is_none() { continue; }
        let issue = &f["issue"];
        match issue["kind"].as_str() {
            Some("missing") => {}
            Some("orphan") => {
                let own = PathBuf::from(issue["file"].as_str().unwrap_or(""));
                let clip = PathBuf::from(f["found"]["path"].as_str().unwrap_or(""));
                let dest = clip.with_extension("afp.json");
                if dest.exists() { problems.push(format!("{}: its clip has an own file already ({})", own.display(), dest.display())); continue; }
                if std::fs::rename(&own, &dest).is_err() {
                    if let Err(e) = std::fs::copy(&own, &dest).and_then(|_| std::fs::remove_file(&own)) { problems.push(format!("{}: {e}", own.display())); continue; }
                }
                written.push(own.clone());
                written.push(dest.clone());
                // Its Gyroflow project beside it goes along
                for g in [own.to_string_lossy().trim_end_matches(".afp.json").trim_end_matches(".marks.json").to_string() + ".gyroflow"] {
                    let g = PathBuf::from(g);
                    let gd = clip.with_extension("gyroflow");
                    if g.is_file() && !gd.exists() && std::fs::rename(&g, &gd).is_ok() { written.push(g); written.push(gd); }
                }
            }
            _ => {}
        }
    }
    crate::seen::note_now(&written);
    problems
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn found_by_what_it_is() {
        let clip = |tree: &str, size: u64| Clip { tree: tree.into(), path: PathBuf::from(format!("/x/{tree}")), size, local: false };
        let clips = vec![clip("New/D.MP4", 5), clip("New/E.MP4", 5), clip("Trip/X.MP4", 3), clip("Trip/Y.MP4", 3), clip("G.MP4", 6)];
        let t0 = crate::index::parse_time("2026-10-24T06:45:41Z").unwrap();
        let times: HashMap<&str, i64> = [("/x/New/D.MP4", t0 + 5), ("/x/New/E.MP4", t0 + 9), ("/x/G.MP4", t0)].into_iter().collect();
        let mut fp = |p: &Path| Some(if p.ends_with("Trip/Y.MP4") { "h1".to_string() } else { "h2".to_string() });
        let mut made = |p: &Path| times.get(p.to_str().unwrap()).copied();
        let find = |l: Value, ask: bool, nr: bool, fp: &mut dyn FnMut(&Path) -> Option<String>, made: &mut dyn FnMut(&Path) -> Option<i64>| {
            let (c, by, ch) = find_clip(&l, &clips, ask, nr, fp, made);
            (c.map(|c| c.tree.clone()), by, ch.len())
        };
        // The fingerprint decides, whatever the name and time
        assert_eq!(find(json!({ "clip": "Day 1/X.MP4", "size": 3, "hash": "h1", "created": "1999-01-01T00:00:00Z" }), false, true, &mut fp, &mut made), (Some("Trip/Y.MP4".into()), "fingerprint", 0));
        assert_eq!(find(json!({ "clip": "X.MP4", "size": 3, "hash": "h9" }), true, false, &mut fp, &mut made).0, None);
        // Without one: creation time; changed, the name; size alone only when asked
        assert_eq!(find(json!({ "clip": "Old/Q.MP4", "size": 5, "created": "2026-10-24T06:45:50Z" }), false, false, &mut fp, &mut made), (Some("New/E.MP4".into()), "size and creation time", 0));
        assert_eq!(find(json!({ "clip": "Old/D.MP4", "size": 5, "created": "2025-01-01T00:00:00Z" }), false, false, &mut fp, &mut made), (Some("New/D.MP4".into()), "name and size", 0));
        assert_eq!(find(json!({ "clip": "Old/F.MP4", "size": 6, "created": "2025-01-01T00:00:00Z" }), false, false, &mut fp, &mut made).0, None);
        assert_eq!(find(json!({ "clip": "Old/F.MP4", "size": 6, "created": "2025-01-01T00:00:00Z" }), true, false, &mut fp, &mut made), (Some("G.MP4".into()), "size alone", 0));
        assert_eq!(find(json!({ "clip": "Old/Q.MP4", "size": 5 }), true, false, &mut fp, &mut made), (None, "size alone", 2));
        // "I never rename my footage": not by time alone
        assert_eq!(find(json!({ "clip": "Old/Q.MP4", "size": 5, "created": "2026-10-24T06:45:50Z" }), false, true, &mut fp, &mut made).0, None);
        assert_eq!(crate::index::parse_time("2026-10-24T09:45:41+03:00"), Some(t0));
        assert_eq!(crate::index::parse_time("2026-10-24T06:45:41.000000Z"), Some(t0));
    }

    #[test]
    fn sha256_and_fingerprint() {
        assert_eq!(sha256_hex(b"abc"), "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad");
        assert_eq!(sha256_hex(&vec![b'a'; 1000]), "41edece42d63e8d9bf515a9ba6932e1c20cbc9f5a5d134645adb5db1b9737ea3");
        let dir = std::env::temp_dir().join(format!("afl-fp-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("s"), b"abc").unwrap();
        // "abcabc", as workspace.rs's test
        assert_eq!(fingerprint(&dir.join("s")).unwrap(), "bbb59da3af939f7af5f360f2ceb80a496e3bae1cd87dde426db0ae40677e1c2c");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn check_and_find() {
        let top = std::env::temp_dir().join(format!("afl-checks-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&top);
        let (p, a, inbox) = (top.join("P"), top.join("Archive"), top.join("Inbox"));
        for d in [p.join("_Moments"), a.join("Egypt"), inbox.clone()] { std::fs::create_dir_all(d).unwrap(); }
        std::fs::write(p.join(locations::PROJECT_FILE), r#"{"locations":[{"name":"Archive","at":"","path":"../Archive"},{"name":"Inbox","at":"Action 6","path":"../Inbox"}],"clips_made":["Action 6/A.MP4\t9"]}"#).unwrap();
        // A clip moved from the inbox into the archive's Egypt folder, its own file left behind
        std::fs::write(a.join("Egypt/A.MP4"), b"clip-a-!").unwrap();
        let ha = fingerprint(&a.join("Egypt/A.MP4")).unwrap();
        std::fs::write(inbox.join("A.afp.json"), format!(r#"{{"clip":{{"size":8,"hash":"{ha}"}},"marks":[]}}"#)).unwrap();
        // The same clip twice
        std::fs::write(a.join("Egypt/B.MP4"), b"same").unwrap();
        std::fs::write(inbox.join("B2.MP4"), b"same").unwrap();
        // Two own files of one clip at one tree path (Archive and Inbox both have "Action 6/C.MP4")
        std::fs::create_dir_all(a.join("Action 6")).unwrap();
        for d in [a.join("Action 6"), inbox.clone()] { std::fs::write(d.join("C.MP4"), b"c").unwrap(); std::fs::write(d.join("C.afp.json"), b"{}").unwrap(); }
        // Nothing selected, nothing checked
        assert_eq!(check(&p, &[], &[])["issues"], json!([]));
        let all: Vec<String> = ["Egypt", "Action 6", "A", "B2.MP4", "C.MP4"].iter().map(|x| x.to_string()).collect();
        let r = check(&p, &all, &[]);
        let kinds: Vec<&str> = r["issues"].as_array().unwrap().iter().map(|i| i["kind"].as_str().unwrap()).collect();
        assert!(kinds.contains(&"orphan") && kinds.contains(&"twice") && kinds.contains(&"own2"), "{kinds:?}");
        // A moment whose original moved: its link's path no longer has it
        let issues = vec![json!({ "kind": "missing", "file": p.join("_Moments/M.afplink.mp4").display().to_string(),
                                  "link": { "v": 1, "kind": "moment", "clip": "Action 6/A.MP4", "size": 8, "hash": ha }, "tree": "Action 6/A.MP4" }),
                          r["issues"].as_array().unwrap().iter().find(|i| i["kind"] == "orphan").unwrap().clone()];
        let f = find_moved(&p, &issues);
        assert_eq!(f[0]["found"]["tree"], json!("Egypt/A.MP4"));
        assert_eq!(f[1]["found"]["tree"], json!("Egypt/A.MP4"));
        let written = std::sync::Mutex::new(Vec::new());
        let problems = apply(f.as_array().unwrap(), &|file, link| { written.lock().unwrap().push((file.to_path_buf(), link.to_string())); Ok(()) });
        assert!(problems.is_empty(), "{problems:?}");
        assert!(written.lock().unwrap()[0].1.contains("\"clip\":\"Egypt/A.MP4\""));
        assert!(a.join("Egypt/A.afp.json").is_file() && !inbox.join("A.afp.json").exists());
        // The project's file isn't rewritten (an old list in it left as it is)
        assert_eq!(locations::read_json(&p.join(locations::PROJECT_FILE)).unwrap()["clips_made"][0], json!("Action 6/A.MP4\t9"));
        let _ = std::fs::remove_dir_all(&top);
    }
}
