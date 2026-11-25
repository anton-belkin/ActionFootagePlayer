use super::*;

/// The list for the player: as tests/librarylist_ipc.py and the player's librarylist.lua read it
#[test]
fn list_for_the_player() {
    let s = |v: &[&str]| v.iter().map(|x| x.to_string()).collect::<Vec<_>>();
    let text = list_m3u(&s(&["/v/C.MP4", "/v/_Moments/Turtle.afplink.mp4", "/v/A.MP4"]), &s(&["C", "Turtle", "A"]), 1, "Originals › Day 1 · filtered").unwrap();
    assert_eq!(text, "#EXTM3U\n#PLAYLIST:Originals › Day 1 · filtered\n#AFP-START:1\n#EXTINF:-1,C\n/v/C.MP4\n#EXTINF:-1,Turtle\n/v/_Moments/Turtle.afplink.mp4\n#EXTINF:-1,A\n/v/A.MP4\n");
    // What an M3U can't hold left out, the start still the item opened; names on one line
    let text = list_m3u(&s(&["/v/a\nb.mp4", " /v/x.mp4", "/v/B.MP4"]), &s(&["a", "x", "B\r\nb"]), 2, "L\nM").unwrap();
    assert_eq!(text, "#EXTM3U\n#PLAYLIST:L M\n#AFP-START:0\n#EXTINF:-1,B  b\n/v/B.MP4\n");
    // The item opened one of those: none (it opens by itself)
    assert!(list_m3u(&s(&["#x.mp4"]), &s(&["x"]), 0, "").is_none());
    assert!(list_m3u(&s(&["/a.mp4"]), &s(&[]), 3, "").is_none());
    // Windows paths as they are; a missing name empty
    assert!(list_m3u(&s(&[r"\\NAS\share\A.MP4"]), &s(&[]), 0, "").unwrap().ends_with("#EXTINF:-1,\n\\\\NAS\\share\\A.MP4\n"));
}

/// Each list in a file of its own; a day's old ones taken out, other files left
#[test]
fn lists_written_and_old_ones_dropped() {
    let dir = std::env::temp_dir().join(format!("afl-lists-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("other.txt"), "x").unwrap();
    let t0 = std::time::SystemTime::now();
    let a = write_list(&dir, "#EXTM3U\n", t0).unwrap();
    let b = write_list(&dir, "#EXTM3U\n", t0).unwrap();
    assert_ne!(a, b);
    assert!(a.to_string_lossy().ends_with(LIST_EXT));
    // Two days later: those two go, a new one and the other file stay
    let c = write_list(&dir, "#EXTM3U\n#AFP-START:0\n", t0 + std::time::Duration::from_secs(2 * 24 * 3600)).unwrap();
    assert!(!a.exists() && !b.exists() && c.exists() && dir.join("other.txt").exists());
    assert_eq!(std::fs::read_to_string(&c).unwrap(), "#EXTM3U\n#AFP-START:0\n");
    let _ = std::fs::remove_dir_all(&dir);
}

/// "Open at start": one project of the list, kept with it; opening projects keeps it, and never drops it off the list
#[test]
fn one_project_opens_at_start() {
    let e = |p: &str| RecentEntry { path: p.into(), ..Default::default() };
    let mut list = vec![e("/a"), e("/b"), e("/c")];
    assert!(start_entry(&list).is_none());
    assert!(set_start(&mut list, Some("/b")));
    assert_eq!(start_entry(&list).unwrap().path, "/b");
    // Another one: only one at a time
    assert!(set_start(&mut list, Some("/c")));
    assert_eq!(list.iter().filter(|x| x.at_start).count(), 1);
    assert_eq!(start_entry(&list).unwrap().path, "/c");
    // One that isn't in the list: said, none marked
    assert!(!set_start(&mut list, Some("/nowhere")));
    assert!(start_entry(&list).is_none());
    // As kept in the file: the mark only on the one, older lists (no mark, paths alone) read as before
    assert!(set_start(&mut list, Some("/b")));
    let text = serde_json::to_string(&list).unwrap();
    assert_eq!(text.matches("at_start").count(), 1);
    assert_eq!(parse_recent(&text), list);
    assert!(start_entry(&parse_recent(r#"["/a",{"path":"/b","share":"//u@nas/Trips","rel":"b"}]"#)).is_none());
    // Opened again: first in the list, still the one at start, its share kept as now
    let list = remembered(list, RecentEntry { path: "/b".into(), share: Some("//u@nas/Trips".into()), rel: "b".into(), ..Default::default() });
    assert_eq!(list.iter().map(|x| x.path.as_str()).collect::<Vec<_>>(), ["/b", "/a", "/c"]);
    assert!(list[0].at_start && list[0].share.is_some());
    // Ten others opened after it: it stays in the list (the last place), still marked
    let mut list = list;
    for i in 0..12 { list = remembered(list, e(&format!("/p{i}"))); }
    assert_eq!(list.len(), 10);
    assert_eq!(list[0].path, "/p11");
    assert_eq!(start_entry(&list).unwrap().path, "/b");
    // Taken off
    assert!(set_start(&mut list, None));
    assert!(start_entry(&list).is_none());
}

/// The project at start, where it is now: reachable at its path; else by its share, mounted now; else offline
#[test]
fn the_start_project_reachable_or_offline() {
    let top = std::env::temp_dir().join(format!("afl-atstart-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&top);
    let p = top.join("vol/Trips/Egypt");
    std::fs::create_dir_all(&p).unwrap();
    std::fs::write(p.join(PROJECT_FILE), "{}").unwrap();
    let here = RecentEntry { path: p.display().to_string(), at_start: true, ..Default::default() };
    assert_eq!(recent_here(&here, &[]), Some(p.clone()));
    // Kept at a path that isn't there now, its share mounted elsewhere: found under the mount
    let moved = RecentEntry { path: "/Volumes/Trips/Egypt".into(), share: Some("//me@nas.local/Trips".into()), rel: "Egypt".into(), at_start: true, ..Default::default() };
    assert_eq!(recent_here(&moved, &[("//me@nas.local/Trips".into(), top.join("vol/Trips"))]), Some(p.clone()));
    // Its share not mounted: offline (the start screen shows it, with Connect)
    assert_eq!(recent_here(&moved, &[]), None);
    let _ = std::fs::remove_dir_all(&top);
}

/// The tag stores chosen on a computer are in its own file for the project, beside its other choices, which stay
#[test]
fn tag_stores_in_this_computers_file() {
    let top = std::env::temp_dir().join(format!("afl-tagstores-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&top);
    std::fs::create_dir_all(&top).unwrap();
    let (file, root) = (top.join("abc.json"), top.join("P"));
    // None yet, none chosen: the file says where the project is here all the same (locations::note_here)
    write_tag_stores(&file, &root, &[]).unwrap();
    assert_eq!(locations::read_json(&file).unwrap()["project"], json!(root.display().to_string()));
    write_tag_stores(&file, &root, &["finder".into(), "nonsense".into()]).unwrap();
    let doc = locations::read_json(&file).unwrap();
    assert_eq!(doc["tag_stores"], json!(["finder"]));
    assert_eq!(locations::tag_stores_in(&doc), vec!["finder".to_string()]);
    // Its other choices stay
    std::fs::write(&file, r#"{"project":"/p","locations":[{"name":"SSD","at":"","path":"/ssd"}],"never_rename":true}"#).unwrap();
    write_tag_stores(&file, &root, &["finder".into()]).unwrap();
    let doc = locations::read_json(&file).unwrap();
    assert_eq!(doc["locations"][0]["path"], json!("/ssd"));
    assert_eq!(doc["never_rename"], json!(true));
    write_tag_stores(&file, &root, &[]).unwrap();
    let doc = locations::read_json(&file).unwrap();
    assert!(doc.get("tag_stores").is_none() && doc["never_rename"] == json!(true));
    // Nothing of this computer's left: the file stays, saying where the project is
    std::fs::write(&file, r#"{"project":"/p","locations":[],"tag_stores":["finder"]}"#).unwrap();
    write_tag_stores(&file, &root, &[]).unwrap();
    let doc = locations::read_json(&file).unwrap();
    assert!(doc.get("tag_stores").is_none() && doc["project"] == json!(root.display().to_string()));
    let _ = std::fs::remove_dir_all(&top);
}

#[test]
fn the_window_waits_for_changes_being_written() {
    assert_eq!(close_decision(false, 0, false), Close::Now);
    assert_eq!(close_decision(false, 2, false), Close::Wait);
    // Asked again while it waits (the page not answering): closed
    assert_eq!(close_decision(false, 2, true), Close::Now);
    // Batches queued: asked about first (the page then waits for the changes before it quits)
    assert_eq!(close_decision(true, 2, false), Close::Ask);
}

#[test]
fn changes_seen() {
    let _one = seen::TESTS.lock().unwrap_or_else(|e| e.into_inner());
    let root = std::env::temp_dir().join(format!("afl-watch-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(root.join("Generated")).unwrap();
    std::fs::write(root.join(PROJECT_FILE), "{}").unwrap();
    std::env::set_var("AFP_HOME", root.join(".home"));
    let root = root.canonicalize().unwrap();
    let w = watch(&root, false);
    // The project's catalog made and read first (it counts a change when it is)
    let t = Instant::now();
    while !seen::is_on(&root) && t.elapsed() < Duration::from_secs(10) { std::thread::sleep(Duration::from_millis(50)); }
    assert!(seen::is_on(&root), "the catalog wasn't read: {}", seen::status(Some(&w.runner)));
    std::thread::sleep(Duration::from_millis(300));
    let n = |w: &Watch| w.changes.load(std::sync::atomic::Ordering::Relaxed);
    let start = n(&w);
    // A hidden temporary file isn't a change; the file it becomes is
    std::fs::write(root.join("Generated/.a.tmp"), "x").unwrap();
    std::thread::sleep(Duration::from_millis(1500));
    assert_eq!(n(&w), start, "a hidden file counted");
    std::fs::write(root.join("Generated/Turtle.afplink.mp4"), "x").unwrap();
    let t = Instant::now();
    while n(&w) == start && t.elapsed() < Duration::from_secs(5) { std::thread::sleep(Duration::from_millis(50)); }
    assert!(n(&w) > start, "the new file not seen");
    // …and it's in the catalog the views read
    assert!(seen::file(&root.join("Generated/Turtle.afplink.mp4")).is_some_and(|f| f.is_some()));
    let _ = std::fs::remove_dir_all(&root);
}

// Move the project…: its catalog goes along and is read at once; its records of the folder it left (the footage,
// now a location) are looked at again before the views read it, so no clip shows as one of its own files
#[test]
fn moved_with_its_catalog() {
    let _one = seen::TESTS.lock().unwrap_or_else(|e| e.into_inner());
    let top = std::env::temp_dir().join(format!("afl-move-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&top);
    let p = top.join("Trip");
    for d in [p.join("Day 1"), p.join("_Moments"), top.join("SSD")] { std::fs::create_dir_all(d).unwrap(); }
    std::fs::write(p.join(PROJECT_FILE), r#"{"id":"move-test"}"#).unwrap();
    std::fs::write(p.join("Day 1/A.MP4"), b"x").unwrap();
    std::fs::write(p.join("Day 1/A.afplink.mp4"), crate::index::tests::tests_mp4_with_link(r#"{"v":1,"kind":"moment","clip":"Day 1/A.MP4"}"#)).unwrap();
    std::fs::write(p.join("_Moments/T.afplink.mp4"), crate::index::tests::tests_mp4_with_link(r#"{"v":1,"kind":"moment","clip":"Day 1/A.MP4"}"#)).unwrap();
    std::env::set_var("AFP_HOME", top.join(".home"));
    let p = p.canonicalize().unwrap();
    let dest = top.join("SSD").canonicalize().unwrap().join("Trip");
    let wait = |root: &Path, w: &Watch| {
        let t = Instant::now();
        while !(seen::is_on(root) && seen::status(Some(&w.runner)).is_empty()) && t.elapsed() < Duration::from_secs(20) { std::thread::sleep(Duration::from_millis(50)); }
        assert!(seen::is_on(root), "not read: {}", seen::status(Some(&w.runner)));
    };
    let w = watch(&p, false);
    wait(&p, &w);
    drop(w);
    std::thread::sleep(Duration::from_millis(400));
    move_to(&p, &dest, &|_| {}).unwrap();
    assert!(dest.join(".afp/catalog").is_dir() && !p.join(".afp").exists());
    let w = watch(&dest, true);
    wait(&dest, &w);
    // Its old folder's clip isn't one of its own files any more (the record of it under the project's folder gone)
    assert_eq!(seen::file(&dest.join("Day 1/A.MP4")), Some(None));
    let m = index::scan(&dest, "moments", false);
    let ids: Vec<&str> = m["items"].as_array().unwrap().iter().filter_map(|i| i["id"].as_str()).collect();
    assert!(ids.contains(&"_Moments/T.afplink.mp4"), "{ids:?}");
    assert!(!ids.iter().any(|i| i.starts_with("Day 1/")), "{ids:?}");
    let f = index::scan(&dest, "footage", false);
    assert!(f["items"].as_array().unwrap().iter().any(|i| i["id"] == "Day 1/A.MP4"));
    drop(w);
    std::thread::sleep(Duration::from_millis(300));
    let _ = std::fs::remove_dir_all(&top);
}

// Another program here noted a file (afp-catalog note, then its line to the Library: live.rs): read at once, and
// the page told (its count of changes up), not at the thread's next turn
#[test]
fn heard_from_another_program() {
    let _one = seen::TESTS.lock().unwrap_or_else(|e| e.into_inner());
    let root = temp("heard");
    std::fs::create_dir_all(root.join("_Moments")).unwrap();
    std::fs::write(root.join(PROJECT_FILE), r#"{"id":"heard-test"}"#).unwrap();
    let root = root.canonicalize().unwrap();
    let store: Arc<dyn afp_catalog::Store> = Arc::new(afp_catalog::DiskStore);
    let mut cat = afp_catalog::Catalog::new(store.clone(), store.clone(), &root, &root.join(".cache"), "library");
    cat.load(&afp_catalog::Want::All).unwrap();
    let (roots, ours) = afp_catalog::project::roots(&root);
    cat.sweep(&roots, &ours, &Default::default()).unwrap();
    let changes = Arc::new(std::sync::atomic::AtomicU64::new(0));
    let mut s = seen::Seen::of(&root, cat);
    s.bump = Some((changes.clone(), Arc::new(Mutex::new(Instant::now()))));
    *seen::SEEN.write().unwrap() = Some(s);
    let file = root.join("_Moments/T.afplink.mp4");
    std::fs::write(&file, crate::index::tests::tests_mp4_with_link(r#"{"v":1,"kind":"moment","clip":"A.MP4"}"#)).unwrap();
    let mut player = afp_catalog::Catalog::new(store.clone(), store, &root, &root.join(".cache-player"), "player");
    let _ = player.load(&afp_catalog::Want::Keys(Vec::new()));
    let st = std::fs::metadata(&file).unwrap();
    player.note(vec![("p".into(), "_Moments/T.afplink.mp4".into(), Some(afp_catalog::File { dir: false, size: st.len(),
        mtime: afp_catalog::store::nanos(st.modified()), meta: afp_catalog::inspect::read(&file).unwrap(), digest: None }))]).unwrap();
    assert!(index::scan(&root, "moments", false)["items"].as_array().unwrap().is_empty());
    seen::read_now();
    assert_eq!(index::scan(&root, "moments", false)["items"].as_array().unwrap().len(), 1);
    assert_eq!(changes.load(std::sync::atomic::Ordering::Relaxed), 1);
    *seen::SEEN.write().unwrap() = None;
}

// A setting written into the project's file isn't a change of its locations: its catalog isn't read again
#[test]
fn locations_key_ignores_settings() {
    let root = temp("lockey");
    std::fs::write(root.join(PROJECT_FILE), r#"{"locations":[{"path":"../A"}],"settings":{}}"#).unwrap();
    let k = locations_key(&root);
    update_doc(&root, |d| { d["settings"]["clips_layout"] = json!("flat"); d.insert("clips_made".into(), json!(["x"])); }).unwrap();
    assert_eq!(locations_key(&root), k);
    update_doc(&root, |d| { d.insert("locations".into(), json!([{ "path": "../B" }])); }).unwrap();
    assert_ne!(locations_key(&root), k);
}

// A location that wasn't reachable when the project was opened (the share mounted later) is noticed, and so is one
// that goes: the project is then watched again, as if opened now
#[test]
fn a_location_reached_later_is_noticed() {
    let base = temp("reach");
    let root = base.join("P");
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(root.join(PROJECT_FILE), r#"{"locations":[{"name":"A","path":"../A"}]}"#).unwrap();
    let before = reach_key(&root);
    let flag = Arc::new(std::sync::atomic::AtomicBool::new(false));
    poll_reach(&root, before.clone(), &flag);
    std::thread::sleep(Duration::from_millis(5500));
    assert!(!flag.load(std::sync::atomic::Ordering::Relaxed), "nothing changed yet");
    std::fs::create_dir_all(base.join("A")).unwrap();
    assert_ne!(reach_key(&root), before);
    let t = Instant::now();
    while !flag.load(std::sync::atomic::Ordering::Relaxed) && t.elapsed() < Duration::from_secs(8) { std::thread::sleep(Duration::from_millis(100)); }
    assert!(flag.load(std::sync::atomic::Ordering::Relaxed), "the location reached wasn't noticed");
    // …and going away again
    let back = reach_key(&root);
    let flag = Arc::new(std::sync::atomic::AtomicBool::new(false));
    poll_reach(&root, back, &flag);
    std::fs::remove_dir_all(base.join("A")).unwrap();
    let t = Instant::now();
    while !flag.load(std::sync::atomic::Ordering::Relaxed) && t.elapsed() < Duration::from_secs(8) { std::thread::sleep(Duration::from_millis(100)); }
    assert!(flag.load(std::sync::atomic::Ordering::Relaxed), "the location gone wasn't noticed");
}

// The projects opened before: kept with their share, found where it's mounted now, read from the old list of paths
#[test]
fn recent_projects_keep_their_share() {
    let old = parse_recent(r#"["/Volumes/ssd/Action Footage Library", {"path":"/Volumes/ssd/T","share":"//u@NAS._smb._tcp.local/ssd","rel":"T"}, 3]"#);
    assert_eq!(old.len(), 2);
    assert_eq!((old[0].path.as_str(), old[0].share.as_deref()), ("/Volumes/ssd/Action Footage Library", None));
    assert_eq!(old[1].share.as_deref(), Some("//u@NAS._smb._tcp.local/ssd"));
    let base = temp("recent");
    let (a, b) = (base.join("ssd"), base.join("ssd-1"));
    let mounts = vec![("//u@NAS._smb._tcp.local/ssd".to_string(), a.clone()), ("//u@NAS._smb._tcp.local/photo".to_string(), base.join("photo"))];
    assert_eq!(share_of(&a.join("Trips/P"), &mounts), Some(("//u@NAS._smb._tcp.local/ssd".to_string(), "Trips/P".to_string())));
    assert_eq!(share_of(Path::new("/elsewhere/P"), &mounts), None);
    // The same share mounted elsewhere now (another name for the NAS too): the project found there
    std::fs::create_dir_all(b.join("Trips/P")).unwrap();
    std::fs::write(b.join("Trips/P").join(PROJECT_FILE), "{}").unwrap();
    let e = RecentEntry { path: a.join("Trips/P").display().to_string(), share: Some("//u@NAS._smb._tcp.local/ssd".into()), rel: "Trips/P".into(), hosts: Vec::new(), at_start: false };
    assert_eq!(recent_here(&e, &[("//other@nas/SSD".to_string(), b.clone())]), Some(b.join("Trips/P")));
    // Not mounted: offline
    assert_eq!(recent_here(&e, &[]), None);
    // The NAS's names: as written first, then the project's others for it, each once as spelled
    let hosts = json!({ "DISKSTATIONDXP": ["web.nas", "192.168.50.186", "DISKSTATIONDXP._smb._tcp.local"], "OTHER": ["x"] });
    assert_eq!(host_names("antonadmin@DISKSTATIONDXP._smb._tcp.local.", Some(&hosts)), ["DISKSTATIONDXP._smb._tcp.local", "DISKSTATIONDXP", "web.nas", "192.168.50.186"]);
    assert_eq!(host_names("web.nas", Some(&hosts)), ["web.nas", "DISKSTATIONDXP", "192.168.50.186", "DISKSTATIONDXP._smb._tcp.local"]);
    assert_eq!(host_names("diskstationdxp", Some(&hosts)), ["diskstationdxp", "web.nas", "192.168.50.186", "DISKSTATIONDXP._smb._tcp.local"]);
    assert_eq!(host_names("nas2", Some(&hosts)), ["nas2"]);
    // …kept with a project on it, for when it's offline (its file, on the share, can't be read then)
    std::fs::write(b.join("Trips/P").join(PROJECT_FILE), json!({ "hosts": hosts }).to_string()).unwrap();
    let names = other_names(&b.join("Trips/P"), Some("//u@DISKSTATIONDXP._smb._tcp.local/ssd"));
    assert_eq!(names, ["DISKSTATIONDXP", "web.nas", "192.168.50.186"]);
    let e = RecentEntry { share: Some("//u@DISKSTATIONDXP._smb._tcp.local/ssd".into()), hosts: names, ..e };
    assert_eq!(recent_names(&e), ["DISKSTATIONDXP._smb._tcp.local", "DISKSTATIONDXP", "web.nas", "192.168.50.186"]);
    // One of a list of before (its path alone), offline: its share guessed from its path and the NAS known here (the
    // Mac's /Volumes)
    if cfg!(target_os = "macos") {
    let old = RecentEntry { path: "/Volumes/ssd/Action Footage Library".into(), ..Default::default() };
    let mounted = [("//antonadmin@DISKSTATIONDXP/photo".to_string(), PathBuf::from("/Volumes/photo"))];
    let (g, names) = guess_share(&old, &mounted, &["//antonadmin@DISKSTATIONDXP._smb._tcp.local/ssd".into()]).unwrap();
    assert_eq!((g.share.as_deref(), g.rel.as_str()), (Some("//antonadmin@DISKSTATIONDXP/ssd"), "Action Footage Library"));
    assert_eq!(names, ["DISKSTATIONDXP", "DISKSTATIONDXP._smb._tcp.local"]);
    // Nothing known, or not under /Volumes: no guess
    assert!(guess_share(&old, &[], &[]).is_none());
    assert!(guess_share(&RecentEntry { path: "/Users/a/P".into(), ..Default::default() }, &mounted, &[]).is_none());
    }
    let _ = std::fs::remove_dir_all(&base);
}

fn temp(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("afl-test-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

#[test]
fn finds_the_project() {
    let root = temp("find");
    std::fs::create_dir_all(root.join("Day 1")).unwrap();
    std::fs::write(root.join("Day 1/A.MP4"), b"").unwrap();
    assert_eq!(project_of(&root), None);
    std::fs::write(root.join(PROJECT_FILE), b"{}").unwrap();
    assert_eq!(project_of(&root.join(PROJECT_FILE)), Some(root.clone()));
    assert_eq!(project_of(&root), Some(root.clone()));
    assert_eq!(project_of(&root.join("Day 1")), Some(root.clone()));
    assert_eq!(project_of(&root.join("Day 1/A.MP4")), Some(root.clone()));
    let old = temp("legacy");
    std::fs::create_dir_all(old.join(LEGACY_DIR)).unwrap();
    assert_eq!(project_of(&old), Some(old.clone()));
}

#[test]
fn settings_keep_the_rest() {
    let root = temp("settings");
    // As the player's Lua writes it: an empty table is a JSON array
    std::fs::write(root.join(PROJECT_FILE), br#"{"format":"actionfootage-project","version":1,"settings":[],"clips_made":["a\t1"],"mine":2}"#).unwrap();
    update_doc(&root, |d| { d["settings"]["clips_flat"] = json!(true); }).unwrap();
    let doc = read_doc(&root);
    assert_eq!(doc["settings"]["clips_flat"], json!(true));
    assert_eq!(doc["clips_made"][0], json!("a\t1"));
    assert_eq!(doc["mine"], json!(2));
    let info = project_info(&root);
    assert!(info.clips_flat && !info.moments_alone && !info.legacy);
    // Someone else's file of that name is left alone
    std::fs::write(root.join(PROJECT_FILE), br#"{"format":"other"}"#).unwrap();
    assert!(update_doc(&root, |_| {}).is_err());
}

#[test]
fn moving_the_project_to_the_ssd() {
    let top = temp("move");
    let trip = top.join("HDD/Trip");
    std::fs::create_dir_all(trip.join("Day 1")).unwrap();
    std::fs::create_dir_all(trip.join("_Moments")).unwrap();
    std::fs::write(trip.join("Day 1/A.MP4"), b"x").unwrap();
    std::fs::write(trip.join("Day 1/A.afp.json"), b"{}").unwrap();
    std::fs::write(trip.join("_Moments/M.afplink.mp4"), b"x").unwrap();
    std::fs::write(trip.join(PROJECT_FILE), br#"{"format":"actionfootage-project","version":1,"settings":{"clips_flat":true}}"#).unwrap();
    let dest = top.join("SSD/Projects/Trip");
    assert!(move_to(&trip, &dest, &|_| {}).unwrap().is_empty());
    // The project's own files moved, the clip and its own file stayed
    assert!(dest.join(PROJECT_FILE).is_file() && dest.join("_Moments/M.afplink.mp4").is_file());
    assert!(!trip.join(PROJECT_FILE).exists() && trip.join("Day 1/A.afp.json").is_file());
    let doc = read_doc(&dest);
    assert_eq!(doc["settings"]["clips_flat"], json!(true));
    assert_eq!(doc["locations"][0]["path"], json!("../../../HDD/Trip"));
    assert!(doc["id"].as_str().is_some_and(|i| i.len() == 36));
    // The old folder points at the project
    let ptr = locations::read_json(&trip.join("Trip.afplocation")).unwrap();
    assert_eq!(locations::place(ptr["project"].as_str().unwrap(), &trip, None, &[]), Some(dest.clone()));
    let v = index::scan(&dest, "footage", false);
    assert_eq!(v["items"][0]["id"], json!("Day 1/A.MP4"));
    // Moved again: its locations follow
    let dest2 = top.join("SSD/Other/Trip");
    move_to(&dest, &dest2, &|_| {}).unwrap();
    assert_eq!(read_doc(&dest2)["locations"][0]["path"], json!("../../../HDD/Trip"));
    assert!(move_to(&dest2, &dest2.join("x"), &|_| {}).is_err());
}

#[test]
fn copying_counts_the_bytes() {
    let top = temp("copy");
    let from = top.join("_Renders");
    std::fs::create_dir_all(from.join("a/b")).unwrap();
    std::fs::write(from.join("a/b/big.mov"), vec![7u8; 9 << 20]).unwrap();
    std::fs::write(from.join("small.mov"), b"12345").unwrap();
    let total = tree_size(&from);
    assert_eq!(total, (9 << 20) + 5);
    let seen = std::cell::RefCell::new(Vec::new());
    let mut state = Moving { total, ..Default::default() };
    copy_tree(&from, &top.join("SSD/_Renders"), &mut state, &|m: Moving| seen.borrow_mut().push(m.done)).unwrap();
    assert_eq!(state.done, total);
    assert_eq!(std::fs::read(top.join("SSD/_Renders/a/b/big.mov")).unwrap().len(), 9 << 20);
    assert_eq!(std::fs::read(top.join("SSD/_Renders/small.mov")).unwrap(), b"12345");
    let seen = seen.into_inner();
    assert!(seen.windows(2).all(|w| w[0] <= w[1]) && *seen.last().unwrap() == total);
    let t = |p: &Path| std::fs::metadata(p).unwrap().modified().unwrap();
    assert_eq!(t(&from.join("small.mov")), t(&top.join("SSD/_Renders/small.mov")));
}

// The whole way, with the player itself: AFL_PLAYER (an ActionFootagePlayer.app) and AFL_TEST_TRIP (a copy of a
// trip with Project.afproject in it). Without them it's skipped
// Stabilize optically stepping aside (the project being moved): stopped, held, queued again without the clips it
// made already
#[cfg(unix)]
#[test]
fn an_optical_batch_steps_aside() {
    use std::os::unix::fs::PermissionsExt;
    let dir = std::env::temp_dir().join(format!("afl-optical-aside-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let tool = dir.join("gyroflow-optical");
    std::fs::write(&tool, "#!/bin/sh\n/usr/bin/python3 -I - \"$2\" <<'EOF'\n\
import json, os, sys, time\n\
req = json.load(open(sys.argv[1]))\n\
st = {'state': 'running', 'done': 1, 'total': 2, 'made': 1, 'current': 'B.MP4', 'made_paths': [req['clips'][0]], 'left_out': [], 'failures': []}\n\
open(req['status'], 'w').write(json.dumps(st))\n\
while not os.path.exists(req['stop']): time.sleep(0.05)\n\
st['state'] = 'stopped'; open(req['status'], 'w').write(json.dumps(st))\n\
EOF\n").unwrap();
    std::fs::set_permissions(&tool, std::fs::Permissions::from_mode(0o755)).unwrap();
    let _one = OPTICAL_TOOL.lock().unwrap_or_else(|e| e.into_inner());
    std::env::set_var("AFL_OPTICAL", &tool);
    let project = dir.to_string_lossy().into_owned();
    let mut inner = Inner::default();
    inner.jobs.push(Job {
        id: 1, kind: "optical".into(), title: "Stabilize optically".into(), project: project.clone(), project_name: "trip".into(),
        state: JobState::Waiting, done: 0, total: 0, failed: 0, made: None, fraction: None, current: None, message: None,
        failures: vec![], left_out: vec![], made_paths: vec![], request: json!({ "kind": "optical", "clips": ["/x/A.MP4", "/x/B.MP4"] }), share: None, results: None,
    });
    let t0 = Instant::now();
    while inner.jobs[0].made_paths.is_empty() && t0.elapsed() < Duration::from_secs(20) { tick(&mut inner); std::thread::sleep(Duration::from_millis(50)); }
    // As move_project asks it
    inner.held = Some(project.clone());
    let run = inner.running.as_mut().unwrap();
    let _ = std::fs::write(run.dir.join("stop"), b"");
    run.stop_asked = Some(Instant::now());
    run.yielding = true;
    while inner.running.is_some() && t0.elapsed() < Duration::from_secs(20) { tick(&mut inner); std::thread::sleep(Duration::from_millis(50)); }
    std::env::remove_var("AFL_OPTICAL");
    let job = &inner.jobs[0];
    assert_eq!(job.state, JobState::Waiting);
    assert!(inner.running.is_none(), "held while the project moves");
    assert_eq!(job.request["clips"], json!(["/x/B.MP4"]));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn lens_types_once_each() {
    let text = "{\"path\":\"/a\",\"camera\":\"DJI OsmoAction6\",\"types\":[[\"wide\",\"Wide\"],[\"normal\",\"Normal\"]]}\nnoise\n{\"path\":\"/b\",\"types\":[[\"normal\",\"Normal\"],[\"linear\",\"Linear\"]]}\n";
    let got = lens_types_of(text);
    assert_eq!(got.len(), 2);
    assert_eq!(got[0]["types"], json!([["wide", "Wide"], ["normal", "Normal"]]));
    assert_eq!(got[1]["path"], json!("/b"));
}

// Stabilize optically's job as the Library runs it: gyroflow-optical (here a stand-in that writes what the real one
// does) started with the request, its status read as it goes, what it left out kept with the job
static OPTICAL_TOOL: Mutex<()> = Mutex::new(());

#[cfg(unix)]
#[test]
fn an_optical_batch() {
    use std::os::unix::fs::PermissionsExt;
    let _one = OPTICAL_TOOL.lock().unwrap_or_else(|e| e.into_inner());
    let dir = std::env::temp_dir().join(format!("afl-optical-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let tool = dir.join("gyroflow-optical");
    std::fs::write(&tool, "#!/bin/sh\n[ \"$1\" = --batch ] || exit 2\n/usr/bin/python3 -I - \"$2\" <<'EOF'\n\
import json, sys, time\n\
req = json.load(open(sys.argv[1]))\n\
st = {'state': 'running', 'done': 0, 'total': 1, 'failed': 0, 'made': 0, 'fraction': 0.5, 'current': 'A.MP4',\n\
  'failures': [], 'left_out': [[req['clips'][1], 'it has motion data the player stabilizes it with']]}\n\
open(req['status'], 'w').write(json.dumps(st)); time.sleep(0.5)\n\
st.update(state='done', done=1, made=1, fraction=None, current=None, message='1 stabilized · 1 left out')\n\
open(req['status'], 'w').write(json.dumps(st))\n\
EOF\n").unwrap();
    std::fs::set_permissions(&tool, std::fs::Permissions::from_mode(0o755)).unwrap();
    std::env::set_var("AFL_OPTICAL", &tool);
    let mut inner = Inner::default();
    inner.next_id = 1;
    inner.jobs.push(Job {
        id: 1, kind: "optical".into(), title: "Stabilize optically".into(), project: dir.to_string_lossy().into(), project_name: "trip".into(),
        state: JobState::Waiting, done: 0, total: 0, failed: 0, made: None, fraction: None, current: None, message: None,
        failures: vec![], left_out: vec![], made_paths: vec![], request: json!({ "kind": "optical", "clips": ["/x/A.MP4", "/x/B.MP4"] }), share: None, results: None,
    });
    let t0 = Instant::now();
    let mut saw_running = false;
    while !(inner.jobs[0].state.finished() && inner.running.is_none()) && t0.elapsed() < Duration::from_secs(20) {
        tick(&mut inner);
        saw_running |= inner.jobs[0].state == JobState::Running;
        std::thread::sleep(Duration::from_millis(100));
    }
    std::env::remove_var("AFL_OPTICAL");
    let job = &inner.jobs[0];
    assert!(saw_running);
    assert_eq!(job.state, JobState::Done, "{:?}", job.message);
    assert_eq!((job.done, job.total, job.made), (1, 1, Some(1)));
    assert_eq!(job.left_out, vec![("/x/B.MP4".to_string(), "it has motion data the player stabilizes it with".to_string())]);
    assert_eq!(job.message.as_deref(), Some("1 stabilized · 1 left out"));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_batch_with_the_player() {
    let (Some(_), Some(trip)) = (std::env::var_os("AFL_PLAYER"), std::env::var_os("AFL_TEST_TRIP")) else { return };
    let shared: Shared = Arc::new(Mutex::new(Inner::default()));
    shared.lock().unwrap().project = Some(PathBuf::from(&trip));
    let mut inner = shared.lock().unwrap();
    inner.next_id = 1;
    inner.jobs.push(Job {
        id: 1, kind: "clips-again".into(), title: "t".into(), project: trip.to_string_lossy().into(), project_name: "trip".into(),
        state: JobState::Waiting, done: 0, total: 0, failed: 0, made: None, fraction: None, current: None, message: None,
        failures: vec![], left_out: vec![], made_paths: vec![], request: json!({ "kind": "clips-again", "project": trip.to_string_lossy() }), share: None, results: None,
    });
    let t0 = Instant::now();
    // Done once the player has said so and quit
    while !(inner.jobs[0].state.finished() && inner.running.is_none()) && t0.elapsed() < Duration::from_secs(120) {
        tick(&mut inner);
        drop(inner);
        std::thread::sleep(Duration::from_millis(300));
        inner = shared.lock().unwrap();
    }
    let j = &inner.jobs[0];
    assert_eq!(j.state, JobState::Done, "{:?}", j.message);
    assert!(inner.running.is_none());
}
