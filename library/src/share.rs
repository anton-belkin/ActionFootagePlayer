// Share (docs/albums-photos-share-concept.md, "Share replaces export"): what's selected, to a destination. Each item's
// render comes from the cache (renders.rs); what the cache lacks the player makes first (its batch `share`), then the
// file is handed to the destination. For now one destination, a folder: the render copied there under a readable
// name. Originals are never touched: only renders and files the project made are read, and only the chosen folder is
// written.
//
// Another destination later is a line in DESTINATIONS and an arm in `deliver`.

use crate::renders::{ self, Kind };
use serde::Serialize;
use serde_json::{ json, Value };
use std::path::{ Path, PathBuf };

/// Where a selection can go: (id, what the sheet calls it)
pub const DESTINATIONS: [(&str, &str); 1] = [("folder", "Save to a folder")];

#[derive(Serialize, Clone, Debug)]
pub struct Item {
    pub name: String,
    /// moment, photo, clip, or file (something that's its own pixels already: a rendered photo, an export)
    pub kind: String,
    /// waiting (for its render), rendering, copying, copied, failed
    pub state: String,
    pub message: Option<String>,
    /// Where it was saved
    pub saved: Option<String>,
    /// The file to hand over, once there is one
    #[serde(skip)]
    pub file: Option<PathBuf>,
    /// The name it's saved under
    #[serde(skip)]
    pub save_as: String,
    /// The render's file in the cache, each form it can take, with the name to save it under (until it's made)
    #[serde(skip)]
    pub expect: Vec<(PathBuf, String)>,
    /// Files that go along as they are (a Live Photo's video)
    #[serde(skip)]
    pub along: Vec<PathBuf>,
}

#[derive(Serialize, Clone, Debug)]
pub struct Share {
    pub dest: String,
    pub folder: String,
    pub items: Vec<Item>,
    /// The player's run ended: what it made is to be handed over (main.rs's worker)
    #[serde(skip)]
    pub ended: bool,
}

fn failed(name: &str, kind: &str, why: String) -> Item {
    Item { name: name.to_string(), kind: kind.to_string(), state: "failed".into(), message: Some(why), saved: None, file: None, save_as: String::new(),
           expect: Vec::new(), along: Vec::new() }
}

/// What sharing `items` takes: each one's file when the cache (or the item itself) has it, else what the player is to
/// render, as its batch's `items`. `items`: { kind: moment | photo | clip | file, path, name }, a moment's or photo
/// link's path in the project's folder, a clip's in the project's tree, a file's on disk. `look`: HDR or SDR, for videos
pub fn plan(root: &Path, items: &[Value], look: &str) -> (Vec<Item>, Vec<Value>) {
    let ctx = renders::Ctx::new(root, true);
    plan_in(&ctx, items, look)
}

pub fn plan_in(ctx: &renders::Ctx, items: &[Value], look: &str) -> (Vec<Item>, Vec<Value>) {
    let root = &ctx.root;
    let (mut out, mut asked): (Vec<Item>, Vec<Value>) = (Vec::new(), Vec::new());
    for it in items {
        let kind = it["kind"].as_str().unwrap_or("");
        let path = it["path"].as_str().unwrap_or("");
        let shown = it["name"].as_str().map(str::to_string).unwrap_or_else(|| path.rsplit('/').next().unwrap_or(path).to_string());
        if kind == "file" {
            // Its own pixels already: as it is, under its own name; a Live Photo's video with it
            let file = PathBuf::from(path);
            if !file.is_file() { out.push(failed(&shown, kind, "the file isn't there".into())); continue; }
            let name = file.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
            let along = if name.to_lowercase().ends_with(".heic") { vec![file.with_extension("mov")].into_iter().filter(|m| m.is_file()).collect() } else { Vec::new() };
            out.push(Item { name: shown, kind: kind.into(), state: "copying".into(), message: None, saved: None, file: Some(file), save_as: name, expect: Vec::new(), along });
            continue;
        }
        let item = match renders::item(root, kind, path) { Ok(i) => i, Err(e) => { out.push(failed(&shown, kind, e)); continue } };
        let profile = renders::profile(if item.kind == Kind::Photo { "Photo" } else if look == "SDR" { "SDR" } else { "HDR" }).unwrap();
        let found = renders::find(ctx, &item, profile);
        let ext_of = |f: &Path| f.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_else(|| profile.exts[0].to_string());
        if let Some(file) = found.file.clone() {
            let save_as = renders::readable(&item, profile, &ext_of(&file));
            out.push(Item { name: shown, kind: kind.into(), state: "copying".into(), message: None, saved: None, file: Some(file), save_as, expect: Vec::new(), along: Vec::new() });
            continue;
        }
        // To be rendered: from its original, which has to be here
        let Some(original) = ctx.original(&item) else { out.push(failed(&shown, kind, "its original isn't here now".into())); continue };
        let expect: Vec<(PathBuf, String)> = profile.exts.iter().map(|e| (renders::cache_path(root, &found.hash, profile, e), renders::readable(&item, profile, e))).collect();
        // A photo's file gets its extension from what the player makes
        let made = if profile.photo { expect[0].0.with_extension("") } else { expect[0].0.clone() };
        let made = made.display().to_string();
        // Two items of one recipe (a moment and its copy in another album) are one render
        if !asked.iter().any(|a| a["out"] == json!(made)) {
            asked.push(renders::trimmed_request(&json!({
                "of": item.kind.name(), "name": shown, "file": item.file.as_ref().map(|f| f.display().to_string()), "clip": original.display().to_string(),
                "t": item.link["t"], "crop": item.link["crop"], "camera": item.link["camera"], "sdr": profile.name == "SDR", "out": made,
                "link": renders::link(ctx, &item, profile, &found) })));
        }
        out.push(Item { name: shown, kind: kind.into(), state: "waiting".into(), message: None, saved: None, file: None, save_as: String::new(), expect, along: Vec::new() });
    }
    (out, asked)
}

/// A name in `folder` that nothing there has: `name`, else "stem 2.ext", "stem 3.ext"…
fn free_name(folder: &Path, name: &str) -> PathBuf {
    let (stem, ext) = match name.rsplit_once('.') { Some((s, e)) if !s.is_empty() => (s.to_string(), format!(".{e}")), _ => (name.to_string(), String::new()) };
    let mut n = 1;
    loop {
        let p = folder.join(if n == 1 { name.to_string() } else { format!("{stem} {n}{ext}") });
        if !p.exists() { return p; }
        n += 1;
    }
}

/// `from` copied into `folder` as `name` (a clone where the disk makes one), never over a file that's there: written
/// under a part's name first, so what's in the folder under its name is whole
pub fn save(from: &Path, folder: &Path, name: &str) -> Result<PathBuf, String> {
    std::fs::create_dir_all(folder).map_err(|e| format!("{}: {e}", folder.display()))?;
    let to = free_name(folder, name);
    let part = folder.join(format!(".{}.part", to.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()));
    let _ = std::fs::remove_file(&part);
    std::fs::copy(from, &part).map_err(|e| { let _ = std::fs::remove_file(&part); format!("couldn't copy it: {e}") })?;
    std::fs::rename(&part, &to).map_err(|e| { let _ = std::fs::remove_file(&part); format!("couldn't name it: {e}") })?;
    Ok(to)
}

/// An item that has its file handed to the destination. What was written
pub fn deliver(dest: &str, folder: &Path, item: &mut Item) -> Vec<PathBuf> {
    let Some(file) = item.file.clone() else { return Vec::new() };
    let mut written = Vec::new();
    match dest {
        "folder" => match save(&file, folder, &item.save_as) {
            Ok(to) => {
                // What goes along keeps its name beside it (a Live Photo is its two files of one name)
                for a in &item.along {
                    let ext = a.extension().map(|e| e.to_string_lossy().into_owned()).unwrap_or_default();
                    let beside = to.with_extension(ext);
                    if !beside.exists() && std::fs::copy(a, &beside).is_ok() { written.push(beside); }
                }
                item.state = "copied".into();
                item.saved = Some(to.display().to_string());
                written.push(to);
            }
            Err(e) => { item.state = "failed".into(); item.message = Some(e); }
        },
        d => { item.state = "failed".into(); item.message = Some(format!("no such destination: {d}")); }
    }
    written
}

/// After the player's run: each item still waiting gets its render from the cache, or says why not (`failures`: what
/// the player said, by the item's name; `stopped`: the run was stopped)
pub fn collect(share: &mut Share, failures: &[(String, String)], stopped: bool) -> Vec<PathBuf> {
    let folder = PathBuf::from(&share.folder);
    let mut written = Vec::new();
    for item in share.items.iter_mut().filter(|i| i.state == "waiting" || i.state == "rendering") {
        match item.expect.iter().find(|(f, _)| f.is_file()).cloned() {
            Some((file, name)) => {
                item.file = Some(file);
                item.save_as = name;
                written.extend(deliver(&share.dest, &folder, item));
            }
            None => {
                item.state = "failed".into();
                item.message = Some(failures.iter().find(|(n, _)| *n == item.name).map(|(_, why)| why.clone())
                    .unwrap_or_else(|| if stopped { "stopped before it was rendered".into() } else { "it wasn't rendered".into() }));
            }
        }
    }
    written
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("afl-share-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn a_selection_saved_to_a_folder() {
        let root = temp("project");
        let dest = temp("dest");
        std::fs::write(root.join("Project.afproject"), r#"{"format":"actionfootageplayer-project","version":1}"#).unwrap();
        for d in ["Day 1", "_Moments/Reef", "_Stills", "_Exports", "_Renders"] { std::fs::create_dir_all(root.join(d)).unwrap(); }
        std::fs::write(root.join("Day 1/A.MP4"), vec![7u8; 4096]).unwrap();
        let moment = crate::index::tests::tests_mp4_with_link(r#"{"v":1,"kind":"moment","clip":"Day 1/A.MP4","size":4096,"in":1,"out":3}"#);
        std::fs::write(root.join("_Moments/Reef/Turtle.afplink.mp4"), &moment).unwrap();
        std::fs::write(root.join("_Moments/Reef/Shark.afplink.mp4"), crate::index::tests::tests_mp4_with_link(r#"{"v":1,"kind":"moment","clip":"Day 1/A.MP4","size":4096,"in":5,"out":9}"#)).unwrap();
        std::fs::write(root.join("_Moments/Reef/Gone.afplink.mp4"), crate::index::tests::tests_mp4_with_link(r#"{"v":1,"kind":"moment","clip":"Day 9/Z.MP4","size":1,"in":5,"out":9}"#)).unwrap();
        let link = json!({ "v": 1, "kind": "photo", "clip": "Day 1/A.MP4", "size": 4096, "t": 2.5 });
        std::fs::write(root.join("_Stills/A_2.50s.afplink.jpg"), crate::albums::photo_file(&crate::albums::tests::jpeg(16, 8, [9, 9, 9]), &link.to_string()).unwrap()).unwrap();
        std::fs::write(root.join("_Stills/Old.heic"), b"heic").unwrap();
        std::fs::write(root.join("_Stills/Old.mov"), b"live").unwrap();
        std::fs::write(root.join("_Exports/T.mov"), b"export").unwrap();
        let ctx = || renders::Ctx::with(&root, true, json!({}));
        let sel = vec![
            json!({ "kind": "moment", "path": "_Moments/Reef/Turtle.afplink.mp4", "name": "Turtle" }),
            json!({ "kind": "moment", "path": "_Moments/Reef/Shark.afplink.mp4", "name": "Shark" }),
            json!({ "kind": "moment", "path": "_Moments/Reef/Gone.afplink.mp4", "name": "Gone" }),
            json!({ "kind": "photo", "path": "_Stills/A_2.50s.afplink.jpg", "name": "A_2.50s" }),
            json!({ "kind": "clip", "path": "Day 1/A.MP4", "name": "A.MP4" }),
            json!({ "kind": "file", "path": root.join("_Stills/Old.heic").display().to_string(), "name": "Old.heic" }),
            json!({ "kind": "file", "path": root.join("_Exports/T.mov").display().to_string(), "name": "T.mov" }),
            json!({ "kind": "moment", "path": "_Moments/Reef/Nothing.afplink.mp4" }),
        ];
        // Turtle is in the cache; the rest of the renders aren't
        let turtle = renders::item(&root, "moment", "_Moments/Reef/Turtle.afplink.mp4").unwrap();
        let hdr = renders::profile("HDR").unwrap();
        let hit = renders::find(&ctx(), &turtle, hdr);
        std::fs::write(renders::cache_path(&root, &hit.hash, hdr, "mov"), b"turtle in hdr").unwrap();
        let (mut items, asked) = plan_in(&ctx(), &sel, "HDR");
        let states: Vec<&str> = items.iter().map(|i| i.state.as_str()).collect();
        assert_eq!(states, ["copying", "waiting", "failed", "waiting", "waiting", "copying", "copying", "failed"]);
        assert_eq!(items[2].message.as_deref(), Some("its original isn't here now"));
        // What the player is asked for: the three the cache lacks, each with its place in the cache and its link
        assert_eq!(asked.iter().map(|a| a["of"].as_str().unwrap()).collect::<Vec<_>>(), ["moment", "photo", "clip"]);
        assert!(asked[0]["out"].as_str().unwrap().ends_with(".HDR.mov") && asked[0]["link"]["kind"] == "render" && asked[0]["file"].as_str().unwrap().ends_with("Shark.afplink.mp4"));
        assert!(asked[1]["out"].as_str().unwrap().ends_with(".Photo") && asked[1]["t"] == json!(2.5) && asked[1]["link"]["render"]["id"].is_string());
        assert_eq!(asked[2]["clip"], json!(root.join("Day 1/A.MP4").display().to_string()));
        assert!(asked[2].get("file").is_none() && asked[2]["sdr"] == json!(false));
        // In SDR they're other renders: Turtle isn't there in that look
        assert_eq!(plan_in(&ctx(), &sel[..1], "SDR").0[0].state, "waiting");
        // A moment and its copy in an album: one render asked for
        std::fs::create_dir_all(root.join("_Clips/Egypt")).unwrap();
        std::fs::copy(root.join("_Moments/Reef/Shark.afplink.mp4"), root.join("_Clips/Egypt/Shark copy.afplink.mp4")).unwrap();
        let twice = [sel[1].clone(), json!({ "kind": "moment", "path": "_Clips/Egypt/Shark copy.afplink.mp4", "name": "Shark copy" })];
        let (two, one) = plan_in(&ctx(), &twice, "HDR");
        assert_eq!((two.len(), one.len()), (2, 1));

        // What's there is saved now, under readable names; what's in the folder already is left as it is
        std::fs::write(dest.join("Turtle.mov"), b"mine").unwrap();
        let mut share = Share { dest: "folder".into(), folder: dest.display().to_string(), items: Vec::new(), ended: false };
        for it in items.iter_mut().filter(|i| i.state == "copying") { deliver("folder", &dest, it); }
        assert_eq!(std::fs::read(dest.join("Turtle.mov")).unwrap(), b"mine");
        assert_eq!(std::fs::read(dest.join("Turtle 2.mov")).unwrap(), b"turtle in hdr");
        assert_eq!(items[0].saved.as_deref(), Some(dest.join("Turtle 2.mov").to_str().unwrap()));
        assert_eq!(std::fs::read(dest.join("Old.heic")).unwrap(), b"heic");
        assert_eq!(std::fs::read(dest.join("Old.mov")).unwrap(), b"live");
        assert_eq!(std::fs::read(dest.join("T.mov")).unwrap(), b"export");
        // The player made the photo (a JPEG here) and the clip; Shark failed
        std::fs::write(format!("{}.jpg", asked[1]["out"].as_str().unwrap()), b"photo").unwrap();
        std::fs::write(asked[2]["out"].as_str().unwrap(), b"whole clip").unwrap();
        share.items = items;
        collect(&mut share, &[("Shark".into(), "gyroflow-export failed".into())], false);
        let states: Vec<&str> = share.items.iter().map(|i| i.state.as_str()).collect();
        assert_eq!(states, ["copied", "failed", "failed", "copied", "copied", "copied", "copied", "failed"]);
        assert_eq!(share.items[1].message.as_deref(), Some("gyroflow-export failed"));
        assert_eq!(std::fs::read(dest.join("A_2.50s.jpg")).unwrap(), b"photo");
        assert_eq!(std::fs::read(dest.join("A.mov")).unwrap(), b"whole clip");
        // Nothing half-written is left, and nothing of the project changed: the original, the moment, the cache
        assert!(std::fs::read_dir(&dest).unwrap().flatten().all(|e| !e.file_name().to_string_lossy().ends_with(".part")));
        assert_eq!(std::fs::read(root.join("Day 1/A.MP4")).unwrap(), vec![7u8; 4096]);
        assert_eq!(std::fs::read(root.join("_Moments/Reef/Turtle.afplink.mp4")).unwrap(), moment);
        assert!(renders::cache_path(&root, &hit.hash, hdr, "mov").is_file());
        // Shared again: everything made is a hit, nothing is asked of the player
        let (again, asked) = plan_in(&ctx(), &sel[3..5], "HDR");
        assert!(asked.is_empty() && again.iter().all(|i| i.state == "copying"));
        // Another destination isn't built
        let mut it = again[0].clone();
        deliver("airdrop", &dest, &mut it);
        assert_eq!(it.state, "failed");
        let _ = std::fs::remove_dir_all(&root);
        let _ = std::fs::remove_dir_all(&dest);
    }
}
