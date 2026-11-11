// SPDX-License-Identifier: GPL-3.0-or-later
//
// Tags and culling (docs/tags-and-culling.md): on an original, a moment or a still — never a clip preview or an export
// (derivatives) — one model of three parts: a pick (`"pick": "accept" | "reject"`, absent: neither), a rating
// (`"rating": 1–5`, absent: none) and custom tags (`"tags"`, compared without case). Kept with each in the library, its
// own store and the truth: an original's in its own file (beside it, wherever it is over the locations), a moment's in
// its link (written again in place), a still's custom tags in its own XMP (`dc:subject`, written by afp-meta; its pick
// and rating: designed). Older versions kept a photo's in Project.afproject (`photo_tags`): taken into the photo at its
// next change, and out of the project's file. The app's own ("Deleted in Photos") are custom tags like the others.
//
// The stores the library enables besides (Project.afproject's `tag_stores`: "finder", macOS Finder tags) are synced
// both ways, three-way (sync): the library now, the store now, and what they last agreed on (`synced.<store>` in the
// item's record). The player's twin is mpv/scripts/workspace/tags.lua.

use crate::locations;
use serde_json::{ json, Value };
use std::path::{ Path, PathBuf };

fn changed(old: &[String], add: &[String], remove: &[String]) -> Vec<String> {
    let mut list = old.to_vec();
    for a in add { let a = clean(a); if !a.is_empty() && !list.iter().any(|x| x.eq_ignore_ascii_case(&a)) { list.push(a); } }
    list.retain(|x| !remove.iter().any(|r| r.trim().eq_ignore_ascii_case(x)));
    list
}

fn clean(t: &str) -> String { t.split_whitespace().collect::<Vec<_>>().join(" ") }

pub(crate) fn write_json(file: &Path, doc: &Value) -> Result<(), String> {
    let tmp = PathBuf::from(format!("{}.tmp", file.display()));
    std::fs::write(&tmp, serde_json::to_vec(doc).unwrap()).and_then(|_| std::fs::rename(&tmp, file)).map_err(|e| format!("{}: {e}", file.display()))
}

/// A clip's own file with nothing of the clip's left in it (as the player's sidecar.lua has it): removed rather than written
pub(crate) fn holds_nothing(doc: &Value) -> bool {
    ["marks", "view", "source", "view360", "location", "tags", "rating", "pick"].iter().all(|k| doc.get(*k).is_none())
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

// ---------- the model ----------

/// An item's pick, rating and custom tags
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Model { pub pick: Option<String>, pub rating: Option<u8>, pub tags: Vec<String> }

fn list_of(v: &Value) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for t in v.as_array().into_iter().flatten().filter_map(Value::as_str) {
        let t = clean(t);
        if !t.is_empty() && !out.iter().any(|x| x.eq_ignore_ascii_case(&t)) { out.push(t); }
    }
    out
}

/// A rating as kept: 1–5, else none
pub fn rating_of(v: &Value) -> Option<u8> { v.as_f64().map(|r| r.round()).filter(|r| *r >= 1.0).map(|r| r.min(5.0) as u8) }
/// A pick as kept: "accept", "reject", else none
pub fn pick_of(v: &Value) -> Option<String> { v.as_str().map(str::to_lowercase).filter(|p| p == "accept" || p == "reject") }

impl Model {
    pub fn of(doc: &Value) -> Model { Model { pick: pick_of(&doc["pick"]), rating: rating_of(&doc["rating"]), tags: list_of(&doc["tags"]) } }
    /// Into a record: keys out when empty
    pub fn put(&self, doc: &mut Value) {
        let Some(o) = doc.as_object_mut() else { return };
        match &self.pick { Some(p) => { o.insert("pick".into(), json!(p)); } None => { o.remove("pick"); } }
        match self.rating { Some(r) => { o.insert("rating".into(), json!(r)); } None => { o.remove("rating"); } }
        if self.tags.is_empty() { o.remove("tags"); } else { o.insert("tags".into(), json!(self.tags)); }
    }
}

/// A tag renamed in a list (merged when the new name is there already); the list unchanged when it hasn't it
pub fn renamed(list: &[String], from: &str, to: &str) -> Vec<String> {
    let (from, to) = (clean(from), clean(to));
    let mut out = list.to_vec();
    if from.is_empty() || to.is_empty() { return out; }
    let Some(i) = out.iter().position(|x| x.eq_ignore_ascii_case(&from)) else { return out };
    match out.iter().position(|x| x.eq_ignore_ascii_case(&to)) {
        Some(j) if j != i => { out.remove(i); }
        _ => out[i] = to,
    }
    out
}

/// What's done to the items (the player's tags.lua does the same: its apply)
pub enum Op<'a> {
    /// Custom tags put on and taken off
    Tags { add: &'a [String], remove: &'a [String] },
    /// A tag renamed where it is; to a name it has already: merged into it
    Rename { from: &'a str, to: &'a str },
    /// 1–5 stars, None: none
    Rating(Option<u8>),
    /// Accept, Reject (one or the other), None: neither
    Pick(Option<&'a str>),
}

fn applied(m: &Model, op: &Op) -> Model {
    let mut out = m.clone();
    match op {
        Op::Tags { add, remove } => out.tags = changed(&m.tags, add, remove),
        Op::Rename { from, to } => out.tags = renamed(&m.tags, from, to),
        Op::Rating(r) => out.rating = r.filter(|r| *r >= 1).map(|r| r.min(5)),
        Op::Pick(p) => out.pick = p.map(str::to_lowercase).filter(|p| p == "accept" || p == "reject"),
    }
    out
}

// ---------- Finder's tags as the model ----------

pub const FINDER_ACCEPT: &str = "Accept\n2";
pub const FINDER_REJECT: &str = "Reject\n6";

/// Finder's strings read as the model: Accept / Reject (both: Reject), a tag of only stars the rating (several: the
/// most), the rest custom
pub fn from_finder(list: &[String]) -> Model {
    let mut m = Model::default();
    for f in list {
        let n = clean(afp_catalog::finder::name(f));
        if n.is_empty() { continue; }
        let stars = n.chars().filter(|c| *c == '★').count();
        if n.eq_ignore_ascii_case("reject") { m.pick = Some("reject".into()); }
        else if n.eq_ignore_ascii_case("accept") { if m.pick.is_none() { m.pick = Some("accept".into()); } }
        else if stars > 0 && n.chars().all(|c| c == '★') { m.rating = Some(m.rating.unwrap_or(0).max(stars.min(5) as u8)); }
        else if !m.tags.iter().any(|x| x.eq_ignore_ascii_case(&n)) { m.tags.push(n); }
    }
    m
}

/// The model as Finder's strings, the ones there (`now`) kept as they are (colour, order) where they say the same
pub fn to_finder(m: &Model, now: &[String]) -> Vec<String> {
    let mut want: Vec<(String, String)> = Vec::new();
    match m.pick.as_deref() { Some("accept") => want.push(("Accept".into(), FINDER_ACCEPT.into())), Some("reject") => want.push(("Reject".into(), FINDER_REJECT.into())), _ => {} }
    if let Some(r) = m.rating { let s = "★".repeat(r as usize); want.push((s.clone(), s)); }
    for t in &m.tags { want.push((t.clone(), t.clone())); }
    let mut used = vec![false; want.len()];
    let mut out = Vec::new();
    for f in now {
        if let Some(i) = (0..want.len()).find(|i| !used[*i] && afp_catalog::finder::name(f).eq_ignore_ascii_case(&want[*i].0)) { used[i] = true; out.push(f.clone()); }
    }
    for (i, w) in want.into_iter().enumerate() { if !used[i] { out.push(w.1); } }
    out
}

/// Three-way: the library's model and a store's, with what they last agreed on (None: nothing). Custom tags: the
/// changes of both; pick and rating: the side that changed, both: the newer (`store_newer`), a tie: the library's
pub fn merge(base: Option<&Model>, lib: &Model, store: &Model, store_newer: bool) -> Model {
    let empty = Model::default();
    let base = base.unwrap_or(&empty);
    let has = |l: &[String], t: &str| l.iter().any(|x| x.eq_ignore_ascii_case(t));
    let added: Vec<String> = store.tags.iter().filter(|t| !has(&base.tags, t)).cloned().collect();
    let removed: Vec<String> = base.tags.iter().filter(|t| !has(&store.tags, t)).cloned().collect();
    let mut out = Model { tags: changed(&lib.tags, &added, &removed), ..Default::default() };
    let take_store = |l_changed: bool, s_changed: bool| s_changed && (!l_changed || store_newer);
    out.pick = if take_store(lib.pick != base.pick, store.pick != base.pick) { store.pick.clone() } else { lib.pick.clone() };
    out.rating = if take_store(lib.rating != base.rating, store.rating != base.rating) { store.rating } else { lib.rating };
    out
}

// ---------- the items ----------

fn ctime(p: &Path) -> i64 {
    #[cfg(unix)] { use std::os::unix::fs::MetadataExt; std::fs::metadata(p).map(|m| m.ctime()).unwrap_or(0) }
    #[cfg(not(unix))] { let _ = p; 0 }
}
fn mtime(p: &Path) -> i64 {
    std::fs::metadata(p).and_then(|m| m.modified()).ok().and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map(|d| d.as_secs() as i64).unwrap_or(0)
}

/// Where an item keeps its record: an original's own file (and the original's main copy, whose attributes are its
/// Finder tags), a moment's link (its file)
enum Rec { Own { own: PathBuf, clip: PathBuf }, Link { file: PathBuf } }

impl Rec {
    fn read(&self) -> Result<Value, String> {
        match self {
            Rec::Own { own, .. } => {
                let doc = locations::read_json(own).unwrap_or(json!({ "format": "actionfootageplayer-clip", "version": 1 }));
                if doc.get("format").and_then(Value::as_str).is_some_and(|f| f != "actionfootageplayer-clip" && f != "actionfootageplayer-marks") {
                    return Err(format!("{}: not the player's file, left alone", own.display()));
                }
                Ok(doc)
            }
            Rec::Link { file } => crate::index::link(file).ok_or(format!("{}: no link inside to keep its tags in", file.display())),
        }
    }
    fn write(&self, mut doc: Value, relink: &dyn Fn(&Path, &str) -> Result<(), String>) -> Result<PathBuf, String> {
        match self {
            Rec::Own { own, clip } => {
                doc["format"] = json!("actionfootageplayer-clip");
                if doc.get("clip").is_none() {
                    let size = std::fs::metadata(clip).map(|m| m.len()).unwrap_or(0);
                    doc["clip"] = json!({ "file": clip.file_name().map(|n| n.to_string_lossy().into_owned()), "size": size });
                }
                if holds_nothing(&doc) {
                    if let Some(o) = doc.as_object_mut() { o.remove("synced"); }
                    std::fs::remove_file(own).or_else(|e| if own.exists() { Err(e) } else { Ok(()) }).map_err(|e| e.to_string())?;
                } else { write_json(own, &doc)?; }
                Ok(own.clone())
            }
            Rec::Link { file } => { relink(file, &doc.to_string()).map_err(|e| format!("{}: {e}", file.display()))?; Ok(file.clone()) }
        }
    }
    /// The file whose Finder tags are the item's, and the record's own (for their times)
    fn store_file(&self) -> &Path { match self { Rec::Own { clip, .. } => clip, Rec::Link { file } => file } }
    fn record_file(&self) -> &Path { match self { Rec::Own { own, .. } => own, Rec::Link { file } => file } }
}

/// The stores synced: `finder(file)` reads a file's Finder tags (None: can't here), `finder_write` writes them
pub struct Stores<'a> {
    pub list: Vec<String>,
    pub finder: &'a dyn Fn(&Path) -> Option<Vec<String>>,
    pub finder_write: &'a dyn Fn(&Path, &[String]) -> Result<(), String>,
}

impl Stores<'_> {
    /// The library's (Project.afproject's tag_stores), through the system's own Finder tags
    pub fn of(root: &Path) -> Stores<'static> {
        Stores { list: locations::tag_stores(root), finder: &afp_catalog::finder::read, finder_write: &afp_catalog::finder::write }
    }
}

/// The item synced with each store, three-way; its record and the store written where they differ from the result.
/// Some("Finder") when the store brought something in
fn sync_rec(rec: &Rec, stores: &Stores, relink: &dyn Fn(&Path, &str) -> Result<(), String>, written: &mut Vec<PathBuf>) -> Result<Option<&'static str>, String> {
    let mut came = None;
    if !stores.list.iter().any(|s| s == "finder") { return Ok(None); }
    let file = rec.store_file();
    let Some(now) = (stores.finder)(file) else { return Ok(None) };
    let doc = rec.read()?;
    let base_list: Option<Vec<String>> = doc.get("synced").and_then(|s| s.get("finder")).and_then(|v| serde_json::from_value(v.clone()).ok());
    let base = base_list.as_deref().map(from_finder);
    let lib = Model::of(&doc);
    let merged = merge(base.as_ref(), &lib, &from_finder(&now), ctime(file) > mtime(rec.record_file()) + 1);
    let strings = to_finder(&merged, &now);
    let agreed = (!strings.is_empty()).then(|| strings.clone());
    // The record first (a link written again may be a new file), then Finder's
    if merged != lib || base_list != agreed {
        let mut d = doc.clone();
        merged.put(&mut d);
        if let Some(o) = d.as_object_mut() {
            let mut synced = o.get("synced").and_then(Value::as_object).cloned().unwrap_or_default();
            match &agreed { Some(a) => { synced.insert("finder".into(), json!(a)); } None => { synced.remove("finder"); } }
            if synced.is_empty() { o.remove("synced"); } else { o.insert("synced".into(), Value::Object(synced)); }
        }
        written.push(rec.write(d, relink)?);
        if merged != lib { came = Some("Finder"); }
    }
    if strings != now { (stores.finder_write)(file, &strings)?; }
    Ok(came)
}

/// An item ({ kind, id, path }) as a record: originals (the Library's "clip") and moments; None for the others
fn rec_of(locs: &[locations::Location], it: &Value) -> Option<Result<Rec, String>> {
    let id = it["id"].as_str().unwrap_or("");
    match it["kind"].as_str() {
        Some("clip") => Some(clip_own_in(locs, id).map(|(own, clip)| Rec::Own { own, clip })),
        Some("moment") => Some(Ok(Rec::Link { file: PathBuf::from(it["path"].as_str().unwrap_or("")) })),
        _ => None,
    }
}

/// Changes the custom tags of `items`: `add` and `remove`. `relink` writes a link again. What couldn't be done
pub fn set(root: &Path, items: &[Value], add: &[String], remove: &[String], relink: &dyn Fn(&Path, &str) -> Result<(), String>,
           photo: &dyn Fn(&Path, &[String]) -> Result<(), String>) -> Vec<String> {
    apply(root, items, &Op::Tags { add, remove }, relink, photo, &Stores::of(root))
}

/// `op` done to `items` ({ kind, id (its path in its view), path }), each in its record (an original's own file, a
/// moment's link, a still's XMP through `photo`), then synced with the library's stores. Exports and clip previews
/// aren't tagged. What couldn't be done
pub fn apply(root: &Path, items: &[Value], op: &Op, relink: &dyn Fn(&Path, &str) -> Result<(), String>,
             photo: &dyn Fn(&Path, &[String]) -> Result<(), String>, stores: &Stores) -> Vec<String> {
    let mut problems = Vec::new();
    // What's written, noted to the catalog at the end (the views read it: on a share it'd see them only later)
    let mut written: Vec<PathBuf> = Vec::new();
    let mut moved_in: Vec<String> = Vec::new();
    let old = locations::read_json(&root.join(locations::PROJECT_FILE)).and_then(|d| d.get("photo_tags").cloned()).unwrap_or(json!({}));
    let locs = locations::locations(root, &locations::mounts());
    for it in items {
        let id = it["id"].as_str().unwrap_or("");
        let path = PathBuf::from(it["path"].as_str().unwrap_or(""));
        if let Some(rec) = rec_of(&locs, it) {
            let rec = match rec { Ok(r) => r, Err(e) => { problems.push(e); continue } };
            let doc = match rec.read() { Ok(d) => d, Err(e) => { problems.push(e); continue } };
            let before = Model::of(&doc);
            let after = applied(&before, op);
            if after != before {
                let mut d = doc;
                after.put(&mut d);
                match rec.write(d, relink) { Ok(f) => written.push(f), Err(e) => { problems.push(e); continue } }
            }
            if let Err(e) = sync_rec(&rec, stores, relink, &mut written) { problems.push(e); }
            continue;
        }
        if it["kind"].as_str() != Some("still") { continue; }
        // A still: its custom tags in its XMP (its pick and rating: designed)
        if matches!(op, Op::Rating(_) | Op::Pick(_)) { continue; }
        let mut now = crate::index::photo_tags(&path);
        for t in old.get(id).and_then(Value::as_array).into_iter().flatten().filter_map(Value::as_str) {
            if !now.iter().any(|x| x.eq_ignore_ascii_case(t)) { now.push(t.to_string()); }
        }
        let m = Model { tags: now.clone(), ..Default::default() };
        let after = applied(&m, op);
        if after.tags == now && old.get(id).is_none() { continue; }
        match photo(&path, &after.tags) {
            Ok(()) => if old.get(id).is_some() { moved_in.push(id.to_string()); },
            Err(e) => { problems.push(format!("{}: {e}", path.display())); continue }
        }
        written.push(path);
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

/// `items` synced with the library's stores (Sync tags, a file's attributes changed): how many brought something in,
/// and what couldn't be done
pub fn sync(root: &Path, items: &[Value], relink: &dyn Fn(&Path, &str) -> Result<(), String>, stores: &Stores) -> (usize, Vec<String>) {
    let locs = locations::locations(root, &locations::mounts());
    let (mut came, mut problems, mut written) = (0, Vec::new(), Vec::new());
    for it in items {
        match rec_of(&locs, it) {
            Some(Ok(rec)) => match sync_rec(&rec, stores, relink, &mut written) { Ok(Some(_)) => came += 1, Ok(None) => {}, Err(e) => problems.push(e) },
            Some(Err(e)) => problems.push(e),
            None => {}
        }
    }
    crate::seen::note_now(&written);
    (came, problems)
}

/// The item a changed file is (its Finder tags changed: the watch): an original by its path in the tree, a moment
pub fn item_of_file(root: &Path, file: &Path) -> Option<Value> {
    let lower = file.to_string_lossy().to_lowercase();
    if lower.ends_with(".afplink.mp4") || (file.starts_with(root) && crate::index::own_area(root, file, &crate::index::footage_dirs(root))) {
        let l = crate::index::link(file)?;
        return (l["kind"] == "moment").then(|| json!({ "kind": "moment", "id": file.display().to_string(), "path": file.display().to_string() }));
    }
    if !afp_catalog::inspect::is_video(&lower) { return None; }
    for l in locations::locations(root, &locations::mounts()) {
        let Some(dir) = &l.dir else { continue };
        if let Some(rel) = locations::relative(file, dir) {
            let tree = if l.at.is_empty() { rel } else { format!("{}/{rel}", l.at) };
            return Some(json!({ "kind": "clip", "id": tree, "path": file.display().to_string() }));
        }
    }
    None
}

/// The items of the whole project with a custom tag (originals, moments, stills), as `apply` takes them
pub fn with_tag(root: &Path, tag: &str) -> Vec<Value> {
    let mut out = Vec::new();
    for view in ["footage", "moments", "stills"] {
        let v = crate::index::scan(root, view, false);
        for it in v["items"].as_array().into_iter().flatten() {
            if it["tags"].as_array().into_iter().flatten().filter_map(Value::as_str).any(|t| t.eq_ignore_ascii_case(tag.trim())) {
                out.push(json!({ "kind": it["kind"], "id": it["id"], "path": it["path"] }));
            }
        }
    }
    out
}

/// Every original and moment of the project (Sync tags over all of it)
pub fn every_item(root: &Path) -> Vec<Value> {
    let mut out = Vec::new();
    for view in ["footage", "moments"] {
        for it in crate::index::scan(root, view, false)["items"].as_array().into_iter().flatten() {
            out.push(json!({ "kind": it["kind"], "id": it["id"], "path": it["path"] }));
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
        let t = clean(t);
        if !t.is_empty() && !list.iter().any(|x| x.eq_ignore_ascii_case(&t)) { list.push(t); }
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
    fn model_finder_merge() {
        let s = |v: &[&str]| v.iter().map(|x| x.to_string()).collect::<Vec<_>>();
        assert_eq!(renamed(&s(&["a", "turtle", "b"]), "Turtle", "Sea turtle"), s(&["a", "Sea turtle", "b"]));
        assert_eq!(renamed(&s(&["turtle", "reef", "Turtles"]), "turtles", "Turtle"), s(&["turtle", "reef"]));
        assert_eq!(renamed(&s(&["turtle"]), "turtle", "Turtle"), s(&["Turtle"]));
        assert_eq!(rating_of(&json!(7)), Some(5));
        assert_eq!(pick_of(&json!("Reject")), Some("reject".into()));
        // A record: keys out when empty
        let mut d = json!({ "tags": ["reef"], "marks": [] });
        let m = applied(&Model::of(&d), &Op::Pick(Some("accept")));
        let m = applied(&m, &Op::Rating(Some(3)));
        m.put(&mut d);
        assert_eq!(d, json!({ "tags": ["reef"], "marks": [], "pick": "accept", "rating": 3 }));
        let m = applied(&applied(&m, &Op::Pick(None)), &Op::Rating(None));
        m.put(&mut d);
        assert_eq!(d, json!({ "tags": ["reef"], "marks": [] }));
        // Finder's strings, both ways (the player's tags.lua: the same cases in tests/tags_test.lua)
        let f = from_finder(&s(&["Accept\n2", "★★★", "Turtle\n6", "reef", "★"]));
        assert_eq!(f, Model { pick: Some("accept".into()), rating: Some(3), tags: s(&["Turtle", "reef"]) });
        assert_eq!(from_finder(&s(&["accept", "REJECT"])).pick.as_deref(), Some("reject"));
        let m = Model { pick: Some("reject".into()), rating: Some(2), tags: s(&["turtle", "new"]) };
        assert_eq!(to_finder(&m, &s(&["Turtle\n6", "Accept\n2", "★★★"])), s(&["Turtle\n6", "Reject\n6", "★★", "new"]));
        // Three-way
        let base = Model { pick: Some("accept".into()), rating: Some(3), tags: s(&["a", "b"]) };
        let lib = Model { pick: Some("accept".into()), rating: Some(4), tags: s(&["a", "b", "lib"]) };
        let store = Model { pick: Some("reject".into()), rating: Some(3), tags: s(&["a", "store"]) };
        assert_eq!(merge(Some(&base), &lib, &store, false), Model { pick: Some("reject".into()), rating: Some(4), tags: s(&["a", "lib", "store"]) });
        let lib2 = Model { pick: Some("reject".into()), rating: Some(5), tags: vec![] };
        let store2 = Model { pick: None, rating: Some(1), tags: vec![] };
        assert_eq!(merge(Some(&base), &lib2, &store2, false).rating, Some(5));
        assert_eq!(merge(Some(&base), &lib2, &store2, true), Model { pick: None, rating: Some(1), tags: vec![] });
        assert_eq!(merge(None, &Model { tags: s(&["x"]), ..Default::default() }, &Model { pick: Some("accept".into()), tags: s(&["y"]), ..Default::default() }, false),
                   Model { pick: Some("accept".into()), rating: None, tags: s(&["x", "y"]) });
    }

    /// A project the player tagged (headless, over IPC), read as the Library's views show it; then a Finder change and
    /// a Library change synced. AFL_TAGS_DIR=<the project> AFL_TAGS_WANT=<json: { view: { id: { pick, rating, tags } } }>,
    /// AFL_TAGS_CUT=<afp-cut> (links written for real)
    #[test]
    #[ignore]
    fn reads_what_the_player_tagged() {
        let dir = PathBuf::from(std::env::var("AFL_TAGS_DIR").expect("AFL_TAGS_DIR"));
        let want: Value = serde_json::from_str(&std::env::var("AFL_TAGS_WANT").expect("AFL_TAGS_WANT")).unwrap();
        for (view, items) in want.as_object().unwrap() {
            let got = crate::index::scan(&dir, view, false);
            for (id, w) in items.as_object().unwrap() {
                let it = got["items"].as_array().unwrap().iter().find(|i| i["id"] == id.as_str()).unwrap_or_else(|| panic!("{view}: no {id}"));
                println!("{view} {id}: pick {} rating {} tags {}", it["pick"], it["rating"], it["tags"]);
                for k in ["pick", "rating", "tags"] { assert_eq!(it[k], w[k], "{view} {id} {k}"); }
            }
        }
        if let Ok(cut) = std::env::var("AFL_TAGS_CUT") {
            let relink = |f: &Path, l: &str| {
                let o = std::process::Command::new(&cut).arg("--set-key").arg(f).arg("ch.actionfootageplayer.link").arg(l).output().map_err(|e| e.to_string())?;
                if o.status.success() { Ok(()) } else { Err(String::from_utf8_lossy(&o.stderr).into_owned()) }
            };
            // Renamed in the Library: the player's records and Finder's tags carry the new name
            let items = with_tag(&dir, "turtle");
            assert!(apply(&dir, &items, &Op::Rename { from: "turtle", to: "Sea turtle" }, &relink, &|_, _| Ok(()), &Stores::of(&dir)).is_empty());
            for view in ["footage", "moments"] {
                for it in crate::index::scan(&dir, view, false)["items"].as_array().unwrap() {
                    println!("after rename, {view} {}: {}", it["id"], it["tags"]);
                    assert!(!it["tags"].as_array().unwrap().iter().any(|t| t == "Turtle"));
                }
            }
            if cfg!(target_os = "macos") {
                // A tag set in Finder by hand on the original: Sync brings it in
                let clip = dir.join("Day 1/A.MP4");
                let mut f = afp_catalog::finder::read(&clip).unwrap();
                println!("Finder on the original: {f:?}");
                assert!(f.iter().any(|t| t == "Sea turtle"));
                f.push("Set in Finder\n5".into());
                std::thread::sleep(std::time::Duration::from_millis(2100));
                afp_catalog::finder::write(&clip, &f).unwrap();
                let (came, problems) = sync(&dir, &every_item(&dir), &relink, &Stores::of(&dir));
                assert!(problems.is_empty(), "{problems:?}");
                assert_eq!(came, 1);
                let own = locations::read_json(&dir.join("Day 1/A.afp.json")).unwrap();
                println!("the original's own file after sync: {own}");
                assert!(own["tags"].as_array().unwrap().iter().any(|t| t == "Set in Finder"));
                // Synced again: nothing to do
                assert_eq!(sync(&dir, &every_item(&dir), &relink, &Stores::of(&dir)).0, 0);
            }
        }
    }

    /// Over a project with Finder's store on: an original and a moment tagged, accepted, rated in the Library; Finder's
    /// tags follow (a stand-in); a change in Finder comes back in; a clip preview and an export aren't tagged
    #[test]
    fn culling_and_finder_over_the_project() {
        let top = std::env::temp_dir().join(format!("afl-tags-cull-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&top);
        let p = top.join("P");
        for d in [p.join("Day 1"), p.join("_Moments"), p.join("_Exports"), p.join("_Clips/Day 1")] { std::fs::create_dir_all(d).unwrap(); }
        std::fs::write(p.join(locations::PROJECT_FILE), r#"{"format":"actionfootage-project","version":3,"tag_stores":["finder"]}"#).unwrap();
        std::fs::write(p.join("Day 1/A.MP4"), b"x").unwrap();
        std::fs::write(p.join("Day 1/A.afp.json"), r#"{"format":"actionfootageplayer-clip","version":1,"tags":["Turtles","reef"],"rating":4,"pick":"accept"}"#).unwrap();
        let link = |kind: &str| crate::index::tests::tests_mp4_with_link(&format!(r#"{{"v":1,"kind":"{kind}","clip":"Day 1/A.MP4","in":1,"out":2,"tags":["turtles"],"rating":2}}"#));
        std::fs::write(p.join("_Moments/M.afplink.mp4"), link("moment")).unwrap();
        std::fs::write(p.join("_Exports/E.mov"), link("export")).unwrap();
        let view = |v: &str, id: &str| crate::index::scan(&p, v, false)["items"].as_array().unwrap().iter().find(|i| i["id"] == id).cloned().unwrap();
        assert_eq!(view("footage", "Day 1/A.MP4")["pick"], json!("accept"));
        assert_eq!(view("footage", "Day 1/A.MP4")["rating"], json!(4));
        assert_eq!(view("exports", "_Exports/E.mov")["tags"], json!([]), "exports aren't tagged");
        // Links and Finder by stand-ins
        let links = std::cell::RefCell::new(std::collections::HashMap::<PathBuf, Value>::new());
        // Written as afp-cut would: the file again, with the link inside
        let relink = |f: &Path, l: &str| { std::fs::write(f, crate::index::tests::tests_mp4_with_link(l)).unwrap(); links.borrow_mut().insert(f.to_path_buf(), serde_json::from_str(l).unwrap()); Ok(()) };
        let finder = std::cell::RefCell::new(std::collections::HashMap::<PathBuf, Vec<String>>::new());
        let fr = |f: &Path| Some(finder.borrow().get(f).cloned().unwrap_or_default());
        let fw = |f: &Path, l: &[String]| { finder.borrow_mut().insert(f.to_path_buf(), l.to_vec()); Ok(()) };
        let stores = Stores { list: vec!["finder".into()], finder: &fr, finder_write: &fw };
        let items = vec![json!({ "kind": "clip", "id": "Day 1/A.MP4", "path": p.join("Day 1/A.MP4").display().to_string() }),
                         json!({ "kind": "moment", "id": "_Moments/M.afplink.mp4", "path": p.join("_Moments/M.afplink.mp4").display().to_string() }),
                         json!({ "kind": "export", "id": "_Exports/E.mov", "path": p.join("_Exports/E.mov").display().to_string() })];
        assert!(apply(&p, &items, &Op::Pick(Some("reject")), &relink, &|_, _| Ok(()), &stores).is_empty());
        let own = locations::read_json(&p.join("Day 1/A.afp.json")).unwrap();
        assert_eq!(own["pick"], json!("reject"));
        assert_eq!(own["synced"]["finder"], json!(["Reject\n6", "★★★★", "Turtles", "reef"]));
        assert_eq!(finder.borrow()[&p.join("Day 1/A.MP4")], vec!["Reject\n6".to_string(), "★★★★".into(), "Turtles".into(), "reef".into()]);
        assert_eq!(links.borrow()[&p.join("_Moments/M.afplink.mp4")]["pick"], json!("reject"));
        assert!(!links.borrow().contains_key(&p.join("_Exports/E.mov")), "the export isn't tagged");
        assert!(!finder.borrow().contains_key(&p.join("_Clips/Day 1/A.afplink.mp4")), "nor a clip preview");
        // Finder: reef off, "From Finder" on, 2 stars (the store newer)
        finder.borrow_mut().insert(p.join("Day 1/A.MP4"), vec!["Reject\n6".into(), "★★".into(), "Turtles".into(), "From Finder\n4".into()]);
        let (came, problems) = sync(&p, &items[..1], &relink, &stores);
        assert!(problems.is_empty(), "{problems:?}");
        assert_eq!(came, 1);
        let own = locations::read_json(&p.join("Day 1/A.afp.json")).unwrap();
        assert_eq!(own["tags"], json!(["Turtles", "From Finder"]));
        assert_eq!(own["rating"], json!(2), "only Finder changed the rating");
        // Nothing left to do
        assert_eq!(sync(&p, &items[..1], &relink, &stores), (0, vec![]));
        // Without the store: the record alone
        let none = Stores { list: vec![], finder: &fr, finder_write: &fw };
        assert!(apply(&p, &items[..1], &Op::Tags { add: &["x".into()], remove: &[] }, &relink, &|_, _| Ok(()), &none).is_empty());
        assert!(!finder.borrow()[&p.join("Day 1/A.MP4")].contains(&"x".to_string()));
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
