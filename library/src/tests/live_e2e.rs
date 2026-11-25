// Both ways at once, on one computer (afp-catalog's live.rs): a player "window" (no video out, nothing on the screen)
// plays the test clip; the Library renames its moment and gives it a lens type, the player marks it. Each side sees the
// other's change in a fraction of a second. By hand, as e2e above:
// AFL_E2E_DIR=… AFL_E2E_CLIP=… AFL_PLAYER=… cargo test -- --ignored both_ways_at_once --nocapture

use super::*;
use std::io::{ BufRead, Write };
use std::sync::atomic::{ AtomicU64, Ordering };

fn ipc(sock: &Path, cmd: Value) -> Value {
    let Ok(mut s) = std::os::unix::net::UnixStream::connect(sock) else { return Value::Null };
    let _ = s.set_read_timeout(Some(Duration::from_secs(3)));
    let _ = s.write_all(format!("{}\n", json!({ "command": cmd, "request_id": 7 })).as_bytes());
    for line in std::io::BufReader::new(s).lines().map_while(Result::ok) {
        let v: Value = serde_json::from_str(&line).unwrap_or(Value::Null);
        if v["request_id"] == 7 { return v["data"].clone(); }
    }
    Value::Null
}

fn until(what: &str, most: Duration, f: &mut dyn FnMut() -> bool) -> f64 {
    let t = Instant::now();
    while !f() {
        assert!(t.elapsed() < most, "{what}: not within {most:?}");
        std::thread::sleep(Duration::from_millis(20));
    }
    t.elapsed().as_secs_f64()
}

#[test]
#[ignore]
fn both_ways_at_once() {
    let (Some(dir), Some(clip), Some(app)) = (std::env::var_os("AFL_E2E_DIR").map(PathBuf::from), std::env::var_os("AFL_E2E_CLIP").map(PathBuf::from),
                                               std::env::var_os("AFL_PLAYER").map(PathBuf::from)) else { return };
    let _one = seen::TESTS.lock().unwrap_or_else(|e| e.into_inner());
    let base = dir.join(format!("afl-live-{}", std::process::id()));
    let (p, f) = (base.join("P"), base.join("Footage/Day 1"));
    std::fs::create_dir_all(&p).unwrap();
    std::fs::create_dir_all(&f).unwrap();
    let home = std::env::temp_dir().join(format!("afl-live-home-{}", std::process::id()));
    std::env::set_var("AFP_HOME", &home);
    let name = clip.file_name().unwrap().to_string_lossy().into_owned();
    let stem = name.rsplit_once('.').unwrap().0.to_string();
    std::fs::copy(&clip, f.join(&name)).unwrap();
    let size = std::fs::metadata(f.join(&name)).unwrap().len();
    std::fs::write(f.join(format!("{stem}.afp.json")), json!({ "format": "actionfootageplayer-clip", "version": 1, "clip": { "file": name, "size": size }, "marks": [{ "t": 2.0 }] }).to_string()).unwrap();
    std::fs::write(p.join(PROJECT_FILE), r#"{"format":"actionfootage-project","version":2,"id":"live-test","locations":[{"name":"Archive","at":"","path":"../Footage"}],"settings":{}}"#).unwrap();
    let mounts = locations::mounts();
    locations::write_pointers(&p, &locations::locations(&p, &mounts), &[], &mounts);
    let p = p.canonicalize().unwrap();
    let tree = format!("Day 1/{name}");
    let runner = seen::start(&p, Arc::new(AtomicU64::new(0)), Arc::new(Mutex::new(Instant::now())), false);
    until("the catalog read", Duration::from_secs(120), &mut || seen::is_on(&p) && seen::status(Some(&runner)).is_empty());
    let _live = afp_catalog::live::listen(|_| seen::read_now()).unwrap();
    seen::TELL_PLAYERS.store(true, Ordering::Relaxed);
    let mut rows: Vec<(String, f64)> = Vec::new();
    // A moment of the mark, made by the player's batch
    let mut inner = Inner { project: Some(p.clone()), ..Default::default() };
    inner.jobs.push(Job { id: 1, kind: "moments".into(), title: "moments".into(), project: p.display().to_string(), project_name: "P".into(), state: JobState::Waiting,
                          done: 0, total: 0, failed: 0, made: None, fraction: None, current: None, message: None, failures: Vec::new(), left_out: Vec::new(),
                          made_paths: Vec::new(), request: json!({ "kind": "moments", "project": p.display().to_string(), "alone": false }) , share: None, results: None });
    let moments = |p: &Path| index::scan(p, "moments", false)["items"].as_array().cloned().unwrap_or_default();
    let mut made_at = None;
    until("the moment made and in the view", Duration::from_secs(300), &mut || {
        tick(&mut inner);
        if made_at.is_none() && inner.jobs[0].made.unwrap_or(0) + inner.jobs[0].done > 0 { made_at = Some(Instant::now()); }
        !moments(&p).is_empty()
    });
    if let Some(m) = made_at { rows.push(("Player's batch makes a moment → Library's view".into(), m.elapsed().as_secs_f64())); }
    until("the batch over", Duration::from_secs(60), &mut || { tick(&mut inner); inner.running.is_none() });
    // The player "window": the clip, paused, no video out
    let sock = home.join("p.sock");
    let log = base.join("player.log");
    let exe = if app.extension().is_some_and(|e| e == "app") { app.join("Contents/MacOS/ActionFootagePlayer") } else { app.clone() };
    let mut player = Command::new(exe).args(["--idle=yes", "--vo=null", "--ao=null", "--force-window=no", "--pause", "--terminal=no"])
        .arg(format!("--input-ipc-server={}", sock.display())).arg(format!("--log-file={}", log.display())).arg(f.join(&name))
        .env("AFP_HOME", &home).env("AFL_TEST_NO_BUNDLE", "1").stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).spawn().unwrap();
    let names = || ipc(&sock, json!(["get_property", "user-data/workspace/moments"])).as_array().cloned().unwrap_or_default()
        .iter().filter_map(|m| m["name"].as_str().map(str::to_string)).collect::<Vec<_>>();
    until("the player playing, its moment shown", Duration::from_secs(60), &mut || names().len() == 1);
    // Ready: its first frame shown (the stabilization's set up: seconds for a 4K clip, the player answering nothing
    // meanwhile), and it answers at once
    let ready = |what: &str| { until(what, Duration::from_secs(180), &mut || {
        let t = Instant::now();
        ipc(&sock, json!(["get_property", "time-pos"])).is_number() && t.elapsed() < Duration::from_millis(300)
    }); };
    ready("the player ready");
    until("the player's inbox", Duration::from_secs(10), &mut || std::fs::read_dir(afp_catalog::live::dir()).into_iter().flatten().flatten()
        .any(|e| e.file_name().to_string_lossy().starts_with("player-")));
    // Library → player: the moment renamed
    let m = moments(&p)[0]["path"].as_str().unwrap().to_string();
    rename_in(&p, "moments", "moment", &m, "Renamed").unwrap();
    let s = until("the player shows the new name", Duration::from_secs(10), &mut || names() == vec!["Renamed".to_string()]);
    rows.push(("Library renames the moment → player's list".into(), s));
    // Library → player: a lens type for the clip playing
    assert!(tags::set_lens_type(&p, &[tree.clone()], Some("normal")).is_empty());
    let s = until("the player loads the clip again", Duration::from_secs(10), &mut || std::fs::read_to_string(&log).unwrap_or_default().contains("from the Library: the clip loaded again"));
    rows.push(("Library sets the lens type → player plays with it".into(), s));
    until("the player playing again", Duration::from_secs(60), &mut || names().len() == 1);
    ready("the player ready again");
    // Player → Library: a mark (M)
    let marks = |p: &Path| index::scan(p, "footage", false)["items"].as_array().unwrap().iter().find(|i| i["id"] == tree.as_str())
        .and_then(|i| i["own"]["marks"].as_array().map(|a| a.len())).unwrap_or(0);
    let before = marks(&p);
    ipc(&sock, json!(["seek", 4, "absolute"]));
    std::thread::sleep(Duration::from_millis(500));
    ipc(&sock, json!(["script-message-to", "highlights", "mark"]));
    let s = until("the Library's view shows the mark", Duration::from_secs(10), &mut || marks(&p) > before);
    rows.push(("Player marks the clip (M) → Library's view".into(), s));
    ipc(&sock, json!(["quit"]));
    let _ = player.wait();
    println!("\n| Change | Seen by the other side after (s) |\n|---|---|");
    for (w, s) in &rows { println!("| {w} | {s:.2} |"); }
    drop(runner);
    seen::TELL_PLAYERS.store(false, Ordering::Relaxed);
    std::thread::sleep(Duration::from_millis(500));
    *seen::SEEN.write().unwrap() = None;
    let _ = std::fs::remove_dir_all(&base);
    let _ = std::fs::remove_dir_all(&home);
}
