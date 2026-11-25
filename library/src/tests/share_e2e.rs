/// Share the whole way, with the player itself rendering what the cache lacks (no window: the batch mode). AFL_PLAYER:
/// an ActionFootagePlayer.app with this branch's scripts; AFL_SHARE_TRIP: a folder with Project.afproject, a clip
/// `Day 1/A.MP4` and a small `thumb.jpg`. Without them it's skipped. A moment, a photo link and the whole clip are
/// shared to `<trip>/../shared`: each rendered into _Renders/<hash>.<profile>.<ext>, then copied

use super::*;

#[test]
fn a_share_with_the_player() {
    let (Some(_), Some(trip)) = (std::env::var_os("AFL_PLAYER"), std::env::var_os("AFL_SHARE_TRIP")) else { return };
    let root = PathBuf::from(&trip);
    // AFL_SHARE_CLIP: the clip's path in the trip (a real camera clip, with its .afp.json and .gyroflow if it has them)
    let clip_owned = std::env::var("AFL_SHARE_CLIP").unwrap_or_else(|_| "Day 1/A.MP4".into());
    let clip = clip_owned.as_str();
    let size = std::fs::metadata(root.join(clip)).unwrap().len();
    let jpeg = std::fs::read(root.join("thumb.jpg")).unwrap();
    if !root.join(PROJECT_FILE).is_file() {
        std::fs::write(root.join(PROJECT_FILE), r#"{"format":"actionfootage-project","version":3,"id":"share-e2e"}"#).unwrap();
    }
    for d in ["_Moments/Reef", "_Stills/Reef"] { std::fs::create_dir_all(root.join(d)).unwrap(); }
    let roots = json!([root.display().to_string()]);
    std::fs::write(root.join("_Moments/Reef/Turtle.afplink.mp4"),
                   albums::link_file(&jpeg, &json!({ "v": 1, "kind": "moment", "clip": clip, "size": size, "roots": roots, "in": 0.5, "out": 2.5 }).to_string()).unwrap()).unwrap();
    std::fs::write(root.join("_Stills/Reef/A_1.00s.afplink.jpg"),
                   albums::photo_file(&jpeg, &json!({ "v": 1, "kind": "photo", "clip": clip, "t": 1.0, "size": size, "roots": roots }).to_string()).unwrap()).unwrap();
    let folder = root.parent().unwrap().join("shared");
    let _ = std::fs::remove_dir_all(&folder);
    std::fs::create_dir_all(&folder).unwrap();
    let items = [json!({ "kind": "moment", "path": "_Moments/Reef/Turtle.afplink.mp4", "name": "Turtle" }),
                 json!({ "kind": "photo", "path": "_Stills/Reef/A_1.00s.afplink.jpg", "name": "A_1.00s" }),
                 json!({ "kind": "clip", "path": clip, "name": clip.rsplit('/').next().unwrap_or(clip).split('.').next().unwrap_or("A") })];
    let look = std::env::var("AFL_SHARE_LOOK").unwrap_or_else(|_| "HDR".into());
    // AFL_SHARE_SETTINGS: the export's settings as the sheet sends them (JSON), else the defaults
    let settings = std::env::var("AFL_SHARE_SETTINGS").ok().and_then(|s| serde_json::from_str::<Value>(&s).ok()).unwrap_or(Value::Null);
    let (mut planned, asked) = share::plan(&root, &items, &look, &settings);
    // What the cache has is handed over at once, as the command does
    for i in planned.iter_mut().filter(|i| i.state == "copying") { share::deliver("folder", &folder, i); }
    println!("planned: {}", serde_json::to_string(&planned.iter().map(|i| (&i.name, &i.state, &i.message)).collect::<Vec<_>>()).unwrap());
    println!("asked of the player: {}", serde_json::to_string_pretty(&asked).unwrap());
    let shared: Shared = Arc::new(Mutex::new(Inner::default()));
    {
        let mut inner = shared.lock().unwrap();
        inner.project = Some(root.clone());
        inner.next_id = 1;
        inner.jobs.push(Job {
            id: 1, kind: "share".into(), title: "t".into(), project: root.display().to_string(), project_name: "trip".into(),
            state: JobState::Waiting, done: 0, total: 0, failed: 0, made: None, fraction: None, current: None, message: None,
            failures: vec![], left_out: vec![], made_paths: vec![],
            request: json!({ "kind": "share", "project": root.display().to_string(), "items": asked }),
            share: Some(share::Share { dest: "folder".into(), folder: folder.display().to_string(), items: planned, ..Default::default() }),
            results: None,
        });
    }
    let t0 = Instant::now();
    loop {
        {
            let mut inner = shared.lock().unwrap();
            tick(&mut inner);
        }
        share_collect(&shared);
        let inner = shared.lock().unwrap();
        let j = &inner.jobs[0];
        let left = j.share.as_ref().is_some_and(|s| s.items.iter().any(|i| !matches!(i.state.as_str(), "copied" | "failed")));
        if (j.state.finished() && inner.running.is_none() && !left) || t0.elapsed() > Duration::from_secs(900) { break; }
        drop(inner);
        std::thread::sleep(Duration::from_millis(300));
    }
    let inner = shared.lock().unwrap();
    let j = &inner.jobs[0];
    println!("after {:.1} s: {:?} {:?} failures {:?}", t0.elapsed().as_secs_f64(), j.state, j.message, j.failures);
    for i in &j.share.as_ref().unwrap().items { println!("  {} ({}): {} {:?}", i.name, i.kind, i.state, i.message); }
    let list = |d: &Path| { let mut v: Vec<String> = std::fs::read_dir(d).into_iter().flatten().flatten().map(|e| format!("{} {}", e.file_name().to_string_lossy(), e.metadata().map(|m| m.len()).unwrap_or(0))).collect(); v.sort(); v };
    println!("_Renders: {:?}", list(&root.join("_Renders")));
    println!("shared: {:?}", list(&folder));
    assert!(j.share.as_ref().unwrap().items.iter().all(|i| i.state == "copied"), "not everything was saved");
    // The player rendered with the recipe's settings, and they're what it would have taken by itself
    assert!(!j.share.as_ref().unwrap().items.iter().any(|i| i.message.as_deref().is_some_and(|m| m.contains("differ"))),
            "the Library's recipe and the player's own settings differ");
    assert_eq!(j.state, JobState::Done, "{:?}", j.message);
}
