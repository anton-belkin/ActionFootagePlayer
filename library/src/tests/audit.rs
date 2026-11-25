// Every operation of the Library, on a test project, the views read right after each (as the page does), with no look of
// the catalog's own in between: on a share the system's notices come late or never, so what a view shows then is what
// the catalog was told. Run with seen::OLD (as before the 2026-11-10 audit: nothing noted) and without, for the table.
// Local: `cargo test audit`. On the NAS: AFL_AUDIT_DIR=/Volumes/<share>/<scratch> cargo test -- --ignored audit_on_a_share --nocapture

use super::*;
use crate::index::tests::tests_mp4_with_link as mp4;
use std::sync::atomic::Ordering;

fn link(kind: &str, clip: &str, extra: &str) -> String { format!(r#"{{"v":1,"kind":"{kind}","clip":"{clip}","in":1,"out":2{extra}}}"#) }

fn photo(tags: &[&str]) -> String {
    let li: String = tags.iter().map(|t| format!("<rdf:li>{t}</rdf:li>")).collect();
    format!("\u{ff}\u{d8}<x:xmpmeta><afp:link>{{&quot;v&quot;:1,&quot;kind&quot;:&quot;photo&quot;,&quot;clip&quot;:&quot;Day 1/A.MP4&quot;,&quot;t&quot;:1.5}}</afp:link><dc:subject><rdf:Bag>{li}</rdf:Bag></dc:subject></x:xmpmeta>")
}

/// A test project in `base`: P (its own files), Footage (a location beside it, as the NAS's archive), Q (a project to
/// take in); its catalog made and read, as the Library's thread does
fn setup(base: &Path) -> PathBuf {
    let _ = std::fs::remove_dir_all(base);
    let (p, f, q) = (base.join("P"), base.join("Footage"), base.join("Q"));
    for d in [p.join("_Moments/Reef"), p.join("_Renders/Reef"), p.join("_Exports"), p.join("_Stills"), f.join("Day 1"), q.join("_Moments")] { std::fs::create_dir_all(d).unwrap(); }
    std::fs::write(p.join(PROJECT_FILE), r#"{"format":"actionfootage-project","version":2,"id":"audit-test","locations":[{"name":"Archive","at":"","path":"../Footage"}]}"#).unwrap();
    std::fs::write(f.join("Day 1/A.MP4"), b"clip a").unwrap();
    std::fs::write(f.join("Day 1/B.MP4"), b"clip bb").unwrap();
    std::fs::write(f.join("Day 1/A.afp.json"), br#"{"format":"actionfootageplayer-clip","version":1,"clip":{"file":"A.MP4","size":6},"marks":[{"t":1.5}]}"#).unwrap();
    std::fs::write(p.join("_Moments/Reef/Turtle.afplink.mp4"), mp4(&link("moment", "Day 1/A.MP4", r#","size":6"#))).unwrap();
    std::fs::write(p.join("_Renders/Reef/Turtle.HDR.mov"), mp4(&link("render", "Day 1/A.MP4", r#","recipe":{"v":0}"#))).unwrap();
    std::fs::write(p.join("_Exports/E.mov"), mp4(&link("export", "Day 1/A.MP4", ""))).unwrap();
    std::fs::write(p.join("_Stills/S.jpg"), photo(&[])).unwrap();
    std::fs::write(q.join(PROJECT_FILE), r#"{"locations":[{"name":"Archive","at":"","path":"../Footage"}],"moment_names":["X"]}"#).unwrap();
    std::fs::write(q.join("_Moments/X.afplink.mp4"), mp4(&link("moment", "Day 1/B.MP4", ""))).unwrap();
    locations::write_pointers(&p, &locations::locations(&p, &locations::mounts()), &[], &locations::mounts());
    let p = p.canonicalize().unwrap();
    let (roots, ours) = afp_catalog::project::roots(&p);
    let store: Arc<dyn afp_catalog::Store> = Arc::new(afp_catalog::DiskStore);
    let mut cat = afp_catalog::Catalog::new(store.clone(), store, &p, &base.join(".cache"), "test");
    cat.load(&afp_catalog::Want::All).unwrap();
    cat.sweep(&roots, &ours, &Default::default()).unwrap();
    *seen::SEEN.write().unwrap() = Some(seen::Seen::of(&p, cat));
    p
}

fn item(p: &Path, view: &str, id: &str) -> Value {
    index::scan(p, view, false)["items"].as_array().unwrap().iter().find(|i| i["id"] == id).cloned().unwrap_or(Value::Null)
}
fn ids(p: &Path, view: &str) -> Vec<String> {
    let mut v: Vec<String> = index::scan(p, view, false)["items"].as_array().unwrap().iter().filter_map(|i| i["id"].as_str().map(str::to_string)).collect();
    v.sort();
    v
}
fn dirs(p: &Path, view: &str) -> Vec<String> {
    index::scan(p, view, false)["dirs"].as_array().unwrap().iter().filter_map(|i| i.as_str().map(str::to_string)).collect()
}
fn relink(file: &Path, l: &str) -> Result<(), String> { std::fs::write(file, mp4(l)).map_err(|e| e.to_string()) }

/// Each operation: (what, what the view shows of it right after); `old`: as before the audit
fn run(base: &Path, old: bool) -> Vec<(String, String)> {
    seen::OLD.store(old, Ordering::Relaxed);
    let p = setup(base);
    let f = base.join("Footage").canonicalize().unwrap();
    let mut out: Vec<(String, String)> = Vec::new();
    let mut row = |what: &str, shows: String| { println!("{:<44} {shows}", what); out.push((what.to_string(), shows)); };
    let fp = |rel: &str| f.join(rel).display().to_string();
    let pp = |rel: &str| p.join(rel).display().to_string();

    // Tags on a clip, a moment, an export, a photo
    let items = vec![json!({ "kind": "clip", "id": "Day 1/A.MP4", "path": fp("Day 1/A.MP4") }),
                     json!({ "kind": "moment", "id": "_Moments/Reef/Turtle.afplink.mp4", "path": pp("_Moments/Reef/Turtle.afplink.mp4") }),
                     json!({ "kind": "export", "id": "_Exports/E.mov", "path": pp("_Exports/E.mov") }),
                     json!({ "kind": "still", "id": "_Stills/S.jpg", "path": pp("_Stills/S.jpg") })];
    let photo_w = |file: &Path, t: &[String]| std::fs::write(file, photo(&t.iter().map(String::as_str).collect::<Vec<_>>())).map_err(|e| e.to_string());
    assert!(tags::set(&p, &items, &["reef".into()], &[], &relink, &photo_w).is_empty());
    row("Tags: clip", item(&p, "footage", "Day 1/A.MP4")["tags"].to_string());
    row("Tags: moment", item(&p, "moments", "_Moments/Reef/Turtle.afplink.mp4")["tags"].to_string());
    row("Tags: export", item(&p, "exports", "_Exports/E.mov")["tags"].to_string());
    row("Tags: photo", item(&p, "stills", "_Stills/S.jpg")["tags"].to_string());
    // Lens type, location
    assert!(tags::set_lens_type(&p, &["Day 1/B.MP4".into()], Some("normal")).is_empty());
    row("Lens type", item(&p, "footage", "Day 1/B.MP4")["own"]["source"]["lens_type"].to_string());
    let loc = geo::location_value(27.9, 34.3, None, "manual", Some(&json!({ "country": "Egypt" })), None, None);
    assert!(geo::set_location(&p, &["Day 1/B.MP4".into()], Some(&loc)).is_empty());
    row("Location", item(&p, "footage", "Day 1/B.MP4")["own"]["location"]["lat"].to_string());
    // A Gyroflow project saved beside the clip, told to the catalog
    std::fs::write(f.join("Day 1/B.gyroflow"), b"{}").unwrap();
    seen::note_now(&[f.join("Day 1/B.gyroflow")]);
    // Stabilize optically: its motion file, told as the task's status names the clip
    std::fs::write(f.join("Day 1/B.afp-motion"), b"x").unwrap();
    assert_eq!(optical_files(&[fp("Day 1/B.MP4")]), vec![f.join("Day 1/B.afp-motion")]);
    row("Stabilize optically (.gyroflow)", item(&p, "footage", "Day 1/B.MP4")["gyroflow"].to_string());
    // Finder moves a clip (the catalog's own look sees it), then Mend: the moment's link follows
    std::fs::create_dir_all(f.join("Day 2")).unwrap();
    for n in ["A.MP4", "A.afp.json"] { std::fs::rename(f.join("Day 1").join(n), f.join("Day 2").join(n)).unwrap(); }
    {
        let mut g = seen::SEEN.write().unwrap();
        let s = g.as_mut().unwrap();
        let (roots, ours) = afp_catalog::project::roots(&p);
        let only = vec![(roots[1].id.clone(), String::new()), (roots[1].id.clone(), "Day 1".into()), (roots[1].id.clone(), "Day 2".into())];
        s.cat.sweep(&roots, &ours, &afp_catalog::Sweep { only, ..Default::default() }).unwrap();
    }
    let issues = checks::missing(&p).unwrap();
    let found = checks::find_moved(&p, &issues);
    assert!(checks::apply(found.as_array().unwrap(), &relink).is_empty());
    row("Mend (link follows a moved clip)", item(&p, "moments", "_Moments/Reef/Turtle.afplink.mp4")["link"]["clip"].to_string());
    // Renders deleted
    let (n, problems) = renders::delete(&p, &["_Moments/Reef/Turtle.afplink.mp4".into()], false);
    assert!(n == 1 && problems.is_empty(), "{problems:?}");
    row("Delete renders", item(&p, "moments", "_Moments/Reef/Turtle.afplink.mp4")["renders"].to_string());
    // New folder, rename a moment, rename a folder
    let made = new_folder_in(&p, "moments", "_Moments").unwrap();
    row("New folder", dirs(&p, "moments").contains(&made).to_string());
    rename_in(&p, "moments", "moment", &pp("_Moments/Reef/Turtle.afplink.mp4"), "Shark").unwrap();
    row("Rename a moment", format!("{:?}", ids(&p, "moments")));
    rename_in(&p, "moments", "folder", &pp("_Moments/Reef"), "Sea").unwrap();
    row("Rename a folder", format!("{:?}", ids(&p, "moments")));
    // A moment the player made, noted by afp-catalog (its own journal) in a run's new folder of Generated/
    let run_dir = p.join("Generated/2026-11-10 12-00 Moments");
    std::fs::create_dir_all(&run_dir).unwrap();
    let made = run_dir.join("Y.afplink.mp4");
    std::fs::write(&made, mp4(&link("moment", "Day 1/B.MP4", ""))).unwrap();
    {
        let store: Arc<dyn afp_catalog::Store> = Arc::new(afp_catalog::DiskStore);
        let mut player = afp_catalog::Catalog::new(store.clone(), store, &p, &base.join(".cache-player"), "player");
        let _ = player.load(&afp_catalog::Want::Keys(Vec::new()));
        let st = std::fs::metadata(&made).unwrap();
        let mtime = afp_catalog::store::nanos(st.modified());
        player.note(vec![("p".into(), "Generated/2026-11-10 12-00 Moments/Y.afplink.mp4".into(),
                          Some(afp_catalog::File { dir: false, size: st.len(), mtime, meta: afp_catalog::inspect::read(&made).unwrap(), digest: None }))]).unwrap();
        let mut g = seen::SEEN.write().unwrap();
        g.as_mut().unwrap().cat.refresh().unwrap();
    }
    row("Moment noted by the player, new folder", ids(&p, "moments").iter().any(|i| i.ends_with("/Y.afplink.mp4")).to_string());
    // Take in another project
    let q = base.join("Q").canonicalize().unwrap();
    takein::take_in(&p, &q, "", &relink, false).unwrap();
    row("Take in", ids(&p, "moments").iter().any(|i| i.ends_with("/X.afplink.mp4")).to_string());
    // Delete to the Trash: a moment, a photo
    TRASH_REMOVES.store(!pp("").starts_with("/Volumes/"), Ordering::Relaxed);
    let r = delete_in(&p, &[json!({ "kind": "moment", "path": pp("_Moments/Sea/Shark.afplink.mp4") }), json!({ "kind": "still", "path": pp("_Stills/S.jpg") })]);
    assert_eq!(r["done"], json!(2), "{r}");
    row("Delete (moment, photo)", format!("{:?} {:?}", ids(&p, "moments"), ids(&p, "stills")));
    *seen::SEEN.write().unwrap() = None;
    seen::OLD.store(false, Ordering::Relaxed);
    out
}

fn table(base: &Path) -> Vec<(String, String, String)> {
    let before = run(&base.join("before"), true);
    let after = run(&base.join("after"), false);
    let rows: Vec<(String, String, String)> = before.into_iter().zip(after).map(|((w, b), (_, a))| (w, b, a)).collect();
    println!("\n| Operation | Before (view right after) | After |\n|---|---|---|");
    for (w, b, a) in &rows { println!("| {w} | `{b}` | `{a}` |"); }
    rows
}

fn check(rows: &[(String, String, String)]) {
    let want = ["[\"reef\"]", "[\"reef\"]", "[]", "[\"reef\"]", "\"normal\"", "27.9", "true", "\"Day 2/A.MP4\"", "{}", "true",
                "[\"_Moments/Reef/Shark.afplink.mp4\"]", "[\"_Moments/Sea/Shark.afplink.mp4\"]", "true", "true"];
    for ((w, _, a), x) in rows.iter().zip(want) { assert!(a.contains(x), "{w}: {a}, wanted {x}"); }
    let (_, _, del) = rows.last().unwrap();
    assert!(!del.contains("Shark") && !del.contains("S.jpg"), "Delete: {del}");
}

#[test]
fn every_operation_shows_at_once() {
    let _one = seen::TESTS.lock().unwrap_or_else(|e| e.into_inner());
    let base = std::env::temp_dir().join(format!("afl-audit-{}", std::process::id()));
    let rows = table(&base);
    check(&rows);
    let _ = std::fs::remove_dir_all(&base);
}

#[test]
#[ignore]
fn audit_on_a_share() {
    let Some(dir) = std::env::var_os("AFL_AUDIT_DIR").map(PathBuf::from) else { return };
    let _one = seen::TESTS.lock().unwrap_or_else(|e| e.into_inner());
    let base = dir.join(format!("afl-audit-{}", std::process::id()));
    let t = Instant::now();
    let rows = table(&base);
    println!("on {} in {:.1} s", base.display(), t.elapsed().as_secs_f64());
    check(&rows);
    let _ = std::fs::remove_dir_all(&base);
}
