// SPDX-License-Identifier: GPL-3.0-or-later
//
// Where the time goes when something is changed in the Library (docs/library-responsiveness.md): a project of
// thousands of clips made on this computer's disk, its catalog read as the Library's thread reads it, and each
// changing command timed with what the page asks for after it, once as the disk is and once with a share's slowness
// stood in for (afp_catalog::store::nas: each stat, read and write waits as long as one to a NAS takes).
//
//   cargo test --release -- --ignored where_the_time_goes --nocapture
//
// AFL_PERF_CLIPS: how many clips (10000); AFL_PERF_NAS: "stat,read,write" milliseconds (5,5,15); AFL_PERF_DIR: where
// (kept there afterwards, with views/<view>.json and timings.json, which the page's own measurement reads:
// library/ui-tests/page_perf.js); none: the temporary folder, removed afterwards.

use super::*;
use crate::index::tests::tests_mp4_with_link as mp4;
use afp_catalog::store::nas;

pub struct Rig {
    pub base: PathBuf,
    pub p: PathBuf,
    pub f: PathBuf,
    /// The clips' paths in the tree, the moments', photos' and exports' in the project
    pub clips: Vec<String>,
    pub moments: Vec<String>,
    pub stills: Vec<String>,
    pub exports: Vec<String>,
    pub probes: index::SharedProbes,
}

fn photo(clip: &str, tags: &[String]) -> String {
    let li: String = tags.iter().map(|t| format!("<rdf:li>{t}</rdf:li>")).collect();
    format!("\u{ff}\u{d8}<x:xmpmeta><afp:link>{{&quot;v&quot;:1,&quot;kind&quot;:&quot;photo&quot;,&quot;clip&quot;:&quot;{clip}&quot;,&quot;t&quot;:1.5}}</afp:link><dc:subject><rdf:Bag>{li}</rdf:Bag></dc:subject></x:xmpmeta>")
}

/// A moment's, export's or photo's link written again, as the player's tools do it (afp-cut, afp-meta: the file read,
/// then written; the tool's own start isn't counted)
pub fn relink(file: &Path, l: &str) -> Result<(), String> {
    nas::read();
    nas::write();
    std::fs::write(file, mp4(l)).map_err(|e| e.to_string())
}
pub fn photo_tags(file: &Path, t: &[String]) -> Result<(), String> {
    nas::read();
    nas::write();
    std::fs::write(file, photo("x", t)).map_err(|e| e.to_string())
}

/// A project of `n` clips in `base`: P (its own files: a clip preview for four clips in five, a moment for one in
/// five, a render for one moment in eight, a photo for one clip in ten, an export for one in fifty), Footage beside it
/// (a location, as the NAS's archive: trips of ten days of fifty clips, one clip in three with its own file); its
/// catalog made and read as the Library's thread does, and what the clips are known as the probe would say
pub fn build(base: &Path, n: usize) -> Rig {
    let _ = std::fs::remove_dir_all(base);
    let (p, f) = (base.join("P"), base.join("Footage"));
    for d in [&p, &f] { std::fs::create_dir_all(d).unwrap(); }
    std::fs::write(p.join(PROJECT_FILE), r#"{"format":"actionfootage-project","version":2,"id":"perf-test","locations":[{"name":"Archive","at":"","path":"../Footage"}]}"#).unwrap();
    let (p, f) = (p.canonicalize().unwrap(), f.canonicalize().unwrap());
    let (mut clips, mut moments, mut stills, mut exports) = (Vec::new(), Vec::new(), Vec::new(), Vec::new());
    let mut known = serde_json::Map::new();
    let made: std::cell::RefCell<std::collections::HashSet<PathBuf>> = Default::default();
    let put = |file: PathBuf, data: &[u8]| {
        let d = file.parent().unwrap().to_path_buf();
        if made.borrow_mut().insert(d.clone()) { std::fs::create_dir_all(&d).unwrap(); }
        std::fs::write(file, data).unwrap();
    };
    for i in 0..n {
        let (trip, day, k) = (i / 500 + 1, i / 50 % 10 + 1, i % 50);
        let dir = format!("Trip {trip:02}/Day {day:02}");
        let stem = format!("DJI_2025{:02}{day:02}{:02}{:02}00_{:04}_D", (trip - 1) % 12 + 1, 8 + k / 6, k % 6 * 10, i + 1);
        let tree = format!("{dir}/{stem}.MP4");
        put(f.join(&tree), b"clip....");
        if i % 3 == 0 {
            let tags = if i % 6 == 0 { r#","tags":["reef"]"# } else { "" };
            let loc = if i % 9 == 0 { r#","location":{"lat":27.9,"lon":34.3,"source":"manual","place":{"country":"Egypt","region":"South Sinai","locality":"Sharm"}}"# } else { "" };
            put(f.join(format!("{dir}/{stem}.afp.json")), format!(r#"{{"format":"actionfootageplayer-clip","version":1,"clip":{{"file":"{stem}.MP4","size":8}},"marks":[{{"t":1.5}}]{tags}{loc}}}"#).as_bytes());
        }
        // A Gyroflow project beside one clip in fifteen (a render's recipe has its fingerprint)
        if i % 15 == 0 { put(f.join(format!("{dir}/{stem}.gyroflow")), format!(r#"{{"title":"{stem}"}}"#).as_bytes()); }
        if i % 5 != 0 { put(p.join(format!("_Clips/{dir}/{stem}.afplink.mp4")), &mp4(&format!(r#"{{"v":1,"kind":"clip","clip":"{tree}","size":8}}"#))); }
        if i % 5 == 0 {
            let rel = format!("_Moments/{dir}/{stem} m1.afplink.mp4");
            put(p.join(&rel), &mp4(&format!(r#"{{"v":1,"kind":"moment","clip":"{tree}","in":1,"out":4,"size":8}}"#)));
            if i % 40 == 0 { put(p.join(format!("_Renders/{dir}/{stem} m1.HDR.mov")), &mp4(&format!(r#"{{"v":1,"kind":"render","clip":"{tree}","recipe":{{"v":0}}}}"#))); }
            // (a moment's own Gyroflow project, one in six)
            if i % 30 == 0 { put(p.join(format!("_Moments/{dir}/{stem} m1.gyroflow")), format!(r#"{{"title":"{stem} m1"}}"#).as_bytes()); }
            moments.push(rel);
        }
        if i % 10 == 0 {
            let rel = format!("_Stills/Trip {trip:02}/{stem}_1.5s.jpg");
            put(p.join(&rel), photo(&tree, &[]).as_bytes());
            stills.push(rel);
        }
        if i % 50 == 0 {
            let rel = format!("_Exports/{stem} m1.mov");
            put(p.join(&rel), &mp4(&format!(r#"{{"v":1,"kind":"export","clip":"{tree}","in":1,"out":4,"look":"hlg"}}"#)));
            exports.push(rel);
        }
        let m = std::fs::metadata(f.join(&tree)).unwrap();
        let mtime = m.modified().unwrap().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs();
        let cam = ["DJI OsmoAction6", "GoPro HERO12 Black", "DJI Mini4Pro"][i % 3];
        let color = ["dlogm", "hlg", "normal"][i % 3];
        let mode = if i % 4 == 0 { Value::Null } else { json!("default") };
        let marks = if i % 7 == 0 { json!([2.0, 9.5]) } else { json!([]) };
        known.insert(f.join(&tree).display().to_string(), json!({ "s": 8, "m": mtime, "l": index::lens_list_stamp(), "v": {
            "geotag": index::GEOTAG, "camera": cam, "color": color, "mode": mode,
            "movie": { "duration": 20 + i % 90, "created": 1_735_700_000 + i * 60, "width": 3840, "height": 2160 }, "fps": 59.94,
            "marks": { "times": marks } } }));
        clips.push(tree);
    }
    locations::write_pointers(&p, &locations::locations(&p, &locations::mounts()), &[], &locations::mounts());
    let (roots, ours) = afp_catalog::project::roots(&p);
    let store: Arc<dyn afp_catalog::Store> = Arc::new(afp_catalog::DiskStore);
    let mut cat = afp_catalog::Catalog::new(store.clone(), store, &p, &base.join(".cache"), "test");
    cat.load(&afp_catalog::Want::All).unwrap();
    cat.sweep(&roots, &ours, &Default::default()).unwrap();
    *seen::SEEN.write().unwrap() = Some(seen::Seen::of(&p, cat));
    let probes: index::SharedProbes = Arc::new(Mutex::new(index::Probes::default()));
    let pf = base.join("probes.json");
    std::fs::write(&pf, serde_json::to_vec(&Value::Object(known)).unwrap()).unwrap();
    probes.lock().unwrap().load(&pf);
    Rig { base: base.to_path_buf(), p, f, clips, moments, stills, exports, probes }
}

impl Rig {
    pub fn clip(&self, i: usize) -> Value { json!({ "kind": "clip", "id": self.clips[i], "path": self.f.join(&self.clips[i]).display().to_string() }) }
    pub fn own(&self, kind: &str, rel: &str) -> Value { json!({ "kind": kind, "id": rel, "path": self.p.join(rel).display().to_string() }) }

    /// A view as the page's `scan` gets it (main.rs's scan, step for step: the project's settings, the view from the
    /// catalog, what the clips are, the view kept in this computer's cache): the answer, and its size as sent
    pub fn view(&self, view: &str) -> (Value, usize) {
        let flat = crate::changed::flat_previews(&self.p);
        seen::wait_ready(&self.p, Duration::from_secs(1));
        let out = index::scan(&self.p, view, flat);
        let mut want: Vec<String> = Vec::new();
        for it in out["items"].as_array().into_iter().flatten() {
            match it["kind"].as_str() {
                Some("clip") => if let Some(p) = it["path"].as_str() { want.push(p.to_string()); },
                Some("moment") => if let Some(o) = it["original"].as_str() { want.push(o.to_string()); },
                _ => {}
            }
        }
        let known = { let mut p = self.probes.lock().unwrap(); p.want(&want); p.known(&want) };
        let mut v = out.clone();
        v["root"] = json!(self.p.display().to_string());
        v["view"] = json!(view);
        let f = self.base.join("views").join(format!("{view}.cache.json"));
        let _ = std::fs::create_dir_all(f.parent().unwrap());
        let tmp = f.with_extension("tmp");
        if std::fs::write(&tmp, serde_json::to_vec(&v).unwrap()).is_ok() { let _ = std::fs::rename(&tmp, &f); }
        v["probes"] = known;
        let size = serde_json::to_vec(&v).unwrap().len();
        (v, size)
    }
}

/// One measurement: what was done, how long it took (ms), the questions it asked of the disk [stats, reads, writes]
#[derive(Clone)]
pub struct Row { pub what: String, pub ms: f64, pub asked: [u64; 3], pub note: String }

pub fn timed<T>(rows: &mut Vec<Row>, what: &str, f: impl FnOnce() -> T) -> T {
    nas::take();
    let t = Instant::now();
    let out = f();
    rows.push(Row { what: what.to_string(), ms: t.elapsed().as_secs_f64() * 1000.0, asked: nas::take(), note: String::new() });
    out
}

/// Every changing command once (and for 20 and 200 items where it takes many)
pub fn commands(r: &Rig, rows: &mut Vec<Row>) {
    let none = |v: Vec<String>, what: &str| assert!(v.is_empty(), "{what}: {v:?}");
    // (spread over the folders; a project of at least a thousand clips)
    let pick = |i: usize| (i * 7 + 1) % r.clips.len();
    let clips = |n: usize| -> Vec<Value> { (0..n).map(|i| r.clip(pick(i))).collect() };
    let trees = |n: usize| -> Vec<String> { (0..n).map(|i| r.clips[pick(i)].clone()).collect() };
    for n in [1, 20, 200] {
        let items = clips(n);
        none(timed(rows, &format!("set_tags: add to {n} clip(s)"), || tags::set(&r.p, &items, &["turtle".into()], &[], &relink, &photo_tags)), "tags");
    }
    let items = clips(200);
    none(timed(rows, "set_tags: remove from 200 clips", || tags::set(&r.p, &items, &[], &["turtle".into()], &relink, &photo_tags)), "untag");
    for n in [1, 20] {
        let items: Vec<Value> = r.moments.iter().skip(3).take(n).map(|m| r.own("moment", m)).collect();
        none(timed(rows, &format!("set_tags: add to {n} moment(s)"), || tags::set(&r.p, &items, &["turtle".into()], &[], &relink, &photo_tags)), "moment tags");
    }
    let items: Vec<Value> = r.stills.iter().take(20).map(|m| r.own("still", m)).collect();
    none(timed(rows, "set_tags: add to 20 photos", || tags::set(&r.p, &items, &["turtle".into()], &[], &relink, &photo_tags)), "photo tags");
    for n in [1, 20, 200] {
        let loc = geo::location_value(27.9 + n as f64 / 1000.0, 34.3, None, "manual", Some(&json!({ "country": "Egypt" })), None, None);
        let ids = trees(n);
        none(timed(rows, &format!("set_locations: {n} clip(s)"), || geo::set_location(&r.p, &ids, Some(&loc))), "location");
    }
    for n in [1, 20, 200] {
        let change: serde_json::Map<String, Value> = [("rotation".to_string(), json!(if n == 20 { 90 } else { 180 }))].into_iter().collect();
        let entries: Vec<(String, serde_json::Map<String, Value>)> = trees(n).into_iter().map(|t| (t, change.clone())).collect();
        none(timed(rows, &format!("set_source: {n} clip(s)"), || tags::set_source(&r.p, &entries)), "source");
    }
    let ids = trees(20);
    none(timed(rows, "set_lens_type: 20 clips", || tags::set_lens_type(&r.p, &ids, Some("normal"))), "lens");
    let m = r.p.join(&r.moments[0]).display().to_string();
    timed(rows, "rename_item: a moment", || rename_in(&r.p, "moments", "moment", &m, "Turtle").unwrap());
    let d = r.p.join("_Moments/Trip 01/Day 02").display().to_string();
    timed(rows, "rename_item: a folder of 10 moments", || rename_in(&r.p, "moments", "folder", &d, "Reef").unwrap());
    timed(rows, "new_folder", || new_folder_in(&r.p, "moments", "_Moments").unwrap());
    let rendered: Vec<String> = r.moments.iter().skip(8).step_by(8).take(20).cloned().collect();
    timed(rows, "delete_renders: 20 moments", || crate::renders::delete(&r.p, &rendered, false));
    // Saying what changed (changed.rs: what each command now does after its write, instead of the page reading views)
    for n in [1, 200] {
        let ids = trees(n);
        timed(rows, &format!("saying what changed: {n} clip(s)"), || crate::changed::of_clips(&r.p, &ids, Vec::new()).with_probes(&r.probes));
    }
    let items: Vec<Value> = r.moments.iter().skip(3).take(20).map(|m| r.own("moment", m)).collect();
    timed(rows, "saying what changed: 20 moments", || crate::changed::of_items(&r.p, &items, Vec::new()).with_probes(&r.probes));
    let own = vec![r.f.join(&r.clips[pick(0)]).with_extension("afp.json")];
    timed(rows, "what the catalog says changed, as items: 1 clip's own file", || crate::changed::of_files(&r.p, &own).map(|c| c.with_probes(&r.probes)));
    // Albums (albums.rs): links made in a new album, moved, photo links of marks; each then saying what it made
    // (changed.rs's of_album). The clips' pictures are in the window's cache already (it showed them): warmed first
    let cache = r.base.join("thumbcache");
    let add = |i: usize| crate::albums::Add { tree: r.clips[pick(i)].clone(), path: r.f.join(&r.clips[pick(i)]), thumb: None, size: 8, created: Some(1_700_000_000), curve: None };
    for i in 0..300 { let _ = index::thumb_bytes(&add(i).path, &cache); }
    for (n, album) in [(1, "_Clips/Perf one"), (300, "_Clips/Perf")] {
        let list: Vec<crate::albums::Add> = (0..n).map(add).collect();
        let v = timed(rows, &format!("album_add: {n} clip(s)"), || crate::albums::add(&r.p, album, &list, &cache, None).unwrap());
        timed(rows, &format!("saying what changed: album_add {n}"), || crate::albums::album_changed(&r.p, v).with_probes(&r.probes));
    }
    let links: Vec<PathBuf> = std::fs::read_dir(r.p.join("_Clips/Perf")).unwrap().flatten().map(|e| e.path()).filter(|p| p.to_string_lossy().ends_with(index::LINK_EXT)).take(50).collect();
    let v = timed(rows, "album_move: 50 links", || crate::albums::move_to(&r.p, "_Clips/Perf moved", &links).unwrap());
    timed(rows, "saying what changed: album_move 50", || crate::albums::album_changed(&r.p, v).with_probes(&r.probes));
    let photos: Vec<crate::albums::Photo> = (0..20).map(|i| crate::albums::Photo { clip: add(i % 5), t: 1.0 + i as f64, crop: None, camera: None }).collect();
    let v = timed(rows, "photo_add: 20 marks", || crate::albums::add_photos(&r.p, "_Stills/Generated", &photos, &cache, None).unwrap());
    timed(rows, "saying what changed: photo_add 20", || crate::albums::album_changed(&r.p, v).with_probes(&r.probes));
    TRASH_REMOVES.store(true, std::sync::atomic::Ordering::Relaxed);
    for (n, skip) in [(1, r.moments.len() / 2), (20, r.moments.len() * 6 / 10)] {
        let items: Vec<Value> = r.moments.iter().skip(skip).take(n).map(|m| r.own("moment", m)).collect();
        let done = timed(rows, &format!("delete_items: {n} moment(s)"), || delete_in(&r.p, &items));
        assert_eq!(done["done"], json!(n), "{done}");
    }
    TRASH_REMOVES.store(false, std::sync::atomic::Ordering::Relaxed);
}

pub fn nas_ms() -> (f64, f64, f64) {
    let t = std::env::var("AFL_PERF_NAS").unwrap_or_else(|_| "5,5,15".into());
    let v: Vec<f64> = t.split(',').map(|x| x.trim().parse().unwrap_or(0.0)).collect();
    (v.first().copied().unwrap_or(5.0), v.get(1).copied().unwrap_or(5.0), v.get(2).copied().unwrap_or(15.0))
}

/// The table, as the disk is and as a share would be; written into the rig's folder too (timings.json)
pub fn report(title: &str, local: &[Row], slow: &[Row]) -> Value {
    let (s, rd, w) = nas_ms();
    println!("\n### {title}\n\n| | This disk, ms | As a NAS ({s:.0}/{rd:.0}/{w:.0} ms a stat/read/write), ms | Stats · reads · writes | |\n|---|---:|---:|---|---|");
    let mut out = Vec::new();
    for (a, b) in local.iter().zip(slow) {
        println!("| {} | {:.0} | {:.0} | {} · {} · {} | {} |", a.what, a.ms, b.ms, b.asked[0], b.asked[1], b.asked[2], a.note);
        out.push(json!({ "what": a.what, "local_ms": a.ms, "nas_ms": b.ms, "asked": b.asked, "note": a.note }));
    }
    json!(out)
}

#[test]
#[ignore]
fn where_the_time_goes() {
    let _one = seen::TESTS.lock().unwrap_or_else(|e| e.into_inner());
    let n: usize = std::env::var("AFL_PERF_CLIPS").ok().and_then(|v| v.parse().ok()).unwrap_or(10_000);
    let keep = std::env::var_os("AFL_PERF_DIR").map(PathBuf::from);
    let base = keep.clone().unwrap_or_else(|| std::env::temp_dir().join(format!("afl-perf-{}", std::process::id())));
    let t = Instant::now();
    let r = build(&base, n);
    println!("{n} clips, {} moments, {} photos, {} exports in {} ({:.1} s to make and read)", r.moments.len(), r.stills.len(), r.exports.len(), base.display(), t.elapsed().as_secs_f64());
    let (s, rd, w) = nas_ms();
    let mut tables = serde_json::Map::new();
    // What the page reads after every command (each view; Footage is what Originals shows too)
    let run = |slow: bool| -> Vec<Row> {
        if slow { nas::set(s, rd, w) } else { nas::set(0.0, 0.0, 0.0) }
        let mut rows = Vec::new();
        timed(&mut rows, "project", || project_info(&r.p));
        for v in ["footage", "moments", "stills", "exports"] {
            let mut size = 0;
            timed(&mut rows, &format!("scan {v}"), || size = r.view(v).1);
            rows.last_mut().unwrap().note = format!("{:.1} MB to the page", size as f64 / 1e6);
        }
        rows
    };
    let (a, b) = (run(false), run(true));
    tables.insert("reads".into(), report(&format!("What the page reads again after a command ({n} clips)"), &a, &b));
    // The views as the page gets them, for its own measurement
    nas::set(0.0, 0.0, 0.0);
    for v in ["footage", "moments", "stills", "exports"] {
        std::fs::write(base.join("views").join(format!("{v}.json")), serde_json::to_vec(&r.view(v).0).unwrap()).unwrap();
    }
    // The commands: on this disk, then (the project made again: the same work) as a share
    let mut local = Vec::new();
    commands(&r, &mut local);
    *seen::SEEN.write().unwrap() = None;
    let views = std::fs::read_dir(base.join("views")).unwrap().flatten().map(|e| (e.file_name(), std::fs::read(e.path()).unwrap())).collect::<Vec<_>>();
    let r = build(&base, n);
    std::fs::create_dir_all(base.join("views")).unwrap();
    for (name, data) in views { std::fs::write(base.join("views").join(name), data).unwrap(); }
    nas::set(s, rd, w);
    let mut slow = Vec::new();
    commands(&r, &mut slow);
    nas::set(0.0, 0.0, 0.0);
    tables.insert("commands".into(), report(&format!("The commands themselves ({n} clips)"), &local, &slow));
    tables.insert("clips".into(), json!(n));
    tables.insert("nas_ms".into(), json!([s, rd, w]));
    std::fs::write(base.join("timings.json"), serde_json::to_vec_pretty(&Value::Object(tables)).unwrap()).unwrap();
    *seen::SEEN.write().unwrap() = None;
    if keep.is_none() { let _ = std::fs::remove_dir_all(&base); }
}

/// A view's look at the moments' renders from the catalog (renders::Ctx not fresh) finds exactly what the disk would:
/// the same recipe, the same render found, and asks the share nothing (but each Gyroflow project's fingerprint, once)
#[test]
fn renders_seen_from_the_catalog_are_the_disks() {
    let _one = seen::TESTS.lock().unwrap_or_else(|e| e.into_inner());
    let base = std::env::temp_dir().join(format!("afl-renders-seen-{}", std::process::id()));
    let r = build(&base, 240);
    let (fresh, view) = (renders::Ctx::with(&r.p, true, json!({})), renders::Ctx::with(&r.p, false, json!({})));
    let (mut gyroflow, mut own, mut old) = (0, 0, 0);
    nas::take();
    let mut asked_view = [0u64; 3];
    for rel in &r.moments {
        let item = renders::item(&r.p, "moment", rel).unwrap();
        for p in ["HDR", "SDR"] {
            let profile = renders::profile(p).unwrap();
            let a = renders::find(&fresh, &item, profile);
            nas::take();
            let b = renders::find(&view, &item, profile);
            for (x, y) in asked_view.iter_mut().zip(nas::take()) { *x += y; }
            assert_eq!(a.recipe, b.recipe, "{rel} {p}");
            assert_eq!((a.hash, a.file, a.stale.clone()), (b.hash, b.file, b.stale.clone()), "{rel} {p}");
            if p == "HDR" {
                gyroflow += a.recipe["gyroflow"].is_string() as usize;
                own += a.recipe["settings"].is_object() as usize;
                old += a.stale.is_some() as usize;
            }
        }
    }
    // (what the rig made: Gyroflow projects, clips with their own file, renders made the old way)
    assert!(gyroflow >= 8 && own >= 20 && old >= 4, "gyroflow {gyroflow}, own {own}, old {old}");
    assert_eq!(asked_view[0], 0, "no stat asked of the share: {asked_view:?}");
    // (the Gyroflow projects there are: beside the moments' clips, beside the moments)
    let projects = r.moments.iter().filter(|m| r.p.join(m.replace(index::LINK_EXT, ".gyroflow")).is_file()).count()
        + r.moments.iter().filter_map(|m| renders::item(&r.p, "moment", m).ok()?.link["clip"].as_str().map(|c| r.f.join(c).with_extension("gyroflow").is_file())).filter(|x| *x).count();
    assert!(asked_view[1] <= projects as u64, "reads: only the {projects} fingerprints, once: {asked_view:?}");
    *seen::SEEN.write().unwrap() = None;
    let _ = std::fs::remove_dir_all(&base);
}

/// The Library's start (2026-11-24 journal: the moments, 41 of them, took 1.2 to 4.4 s from the NAS while 4392 clips
/// took 0.1 to 0.3 s): each view read once as a new process reads it (nothing kept in memory yet), as a share first,
/// then as this disk. AFL_PERF_CLIPS (2000: 400 moments), AFL_PERF_NAS as above
///
///   cargo test --release -- --ignored start_reads --nocapture
#[test]
#[ignore]
fn start_reads() {
    let _one = seen::TESTS.lock().unwrap_or_else(|e| e.into_inner());
    let n: usize = std::env::var("AFL_PERF_CLIPS").ok().and_then(|v| v.parse().ok()).unwrap_or(2000);
    let base = std::env::temp_dir().join(format!("afl-start-{}", std::process::id()));
    let r = build(&base, n);
    println!("{n} clips, {} moments, {} photos, {} exports", r.moments.len(), r.stills.len(), r.exports.len());
    let (s, rd, w) = nas_ms();
    let run = |slow: bool| -> Vec<Row> {
        if slow { nas::set(s, rd, w) } else { nas::set(0.0, 0.0, 0.0) }
        let mut rows = Vec::new();
        for v in ["moments", "footage", "stills", "exports"] { timed(&mut rows, &format!("scan {v}"), || r.view(v)); }
        rows
    };
    let slow = run(true);
    let local = run(false);
    nas::set(0.0, 0.0, 0.0);
    report(&format!("The start's reads ({n} clips, {} moments; the share's first)", r.moments.len()), &local, &slow);
    *seen::SEEN.write().unwrap() = None;
    let _ = std::fs::remove_dir_all(&base);
}
