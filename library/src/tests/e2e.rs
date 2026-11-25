// The player's batches, run as the Tasks run them, on a test project (a copy of a real clip, two marks): what each
// makes is in the views as it lands, read from the catalog with no look of its own (the runner started without the
// system's notices: a share's, which come late or never). By hand:
// AFL_E2E_DIR=/Volumes/ssd/<scratch> AFL_E2E_CLIP=<a camera clip> AFL_PLAYER=<ActionFootagePlayer.app> \
//   cargo test -- --ignored batches_show_as_they_land --nocapture

use super::*;
use std::sync::atomic::AtomicU64;

fn count(p: &Path, kind: &str) -> usize {
    let items = |v: &str| index::scan(p, v, false)["items"].as_array().cloned().unwrap_or_default();
    match kind {
        "moments" => items("moments").len(),
        "photos" => items("stills").len(),
        "export" => items("exports").len(),
        "render" => items("moments").iter().map(|m| m["renders"].as_object().map(|o| o.len()).unwrap_or(0)).sum(),
        // Clip previews: the empty ones (drafts), then finished ones
        "clips" => items("footage").iter().filter(|c| !c["preview"].is_null()).count(),
        _ => items("footage").iter().filter(|c| !c["preview"].is_null() && c["draft"].is_null()).count(),
    }
}

#[test]
#[ignore]
fn batches_show_as_they_land() {
    let (Some(dir), Some(clip)) = (std::env::var_os("AFL_E2E_DIR").map(PathBuf::from), std::env::var_os("AFL_E2E_CLIP").map(PathBuf::from)) else { return };
    let _one = seen::TESTS.lock().unwrap_or_else(|e| e.into_inner());
    let base = dir.join(format!("afl-e2e-{}", std::process::id()));
    let (p, f) = (base.join("P"), base.join("Footage/Day 1"));
    std::fs::create_dir_all(&p).unwrap();
    std::fs::create_dir_all(&f).unwrap();
    std::env::set_var("AFP_HOME", base.join(".home"));
    let name = clip.file_name().unwrap().to_string_lossy().into_owned();
    let t = Instant::now();
    std::fs::copy(&clip, f.join(&name)).unwrap();
    println!("clip copied in {:.1} s", t.elapsed().as_secs_f64());
    let size = std::fs::metadata(f.join(&name)).unwrap().len();
    std::fs::write(f.join(format!("{}.afp.json", name.rsplit_once('.').unwrap().0)),
                   json!({ "format": "actionfootageplayer-clip", "version": 1, "clip": { "file": name, "size": size }, "marks": [{ "t": 2.0 }, { "t": 5.0 }] }).to_string()).unwrap();
    std::fs::write(p.join(PROJECT_FILE), r#"{"format":"actionfootage-project","version":2,"id":"e2e-test","locations":[{"name":"Archive","at":"","path":"../Footage"}],"settings":{}}"#).unwrap();
    let mounts = locations::mounts();
    locations::write_pointers(&p, &locations::locations(&p, &mounts), &[], &mounts);
    let p = p.canonicalize().unwrap();
    // The catalog's thread, without the system's notices: only what's noted (and its own looks, minutes apart)
    let runner = seen::start(&p, Arc::new(AtomicU64::new(0)), Arc::new(Mutex::new(Instant::now())), false);
    let t = Instant::now();
    while !(seen::is_on(&p) && seen::status(Some(&runner)).is_empty()) && t.elapsed() < Duration::from_secs(120) { std::thread::sleep(Duration::from_millis(100)); }
    assert!(seen::is_on(&p));
    let mut rows = Vec::new();
    for kind in ["moments", "photos", "export", "render", "clips", "clips-finish"] {
        let mut request = json!({ "kind": kind, "project": p.display().to_string() });
        match kind {
            "moments" => request["alone"] = json!(false),
            "photos" => request["photo"] = json!("no"),
            "export" => request["sdr"] = json!(false),
            "render" => request["profiles"] = json!(["HDR"]),
            "clips" => { request["draft"] = json!(true); request["layout"] = json!("tree"); }
            _ => {}
        }
        let before = count(&p, kind);
        let mut inner = Inner { project: Some(p.clone()), ..Default::default() };
        inner.jobs.push(Job { id: 1, kind: kind.into(), title: kind.into(), project: p.display().to_string(), project_name: "P".into(), state: JobState::Waiting,
                              done: 0, total: 0, failed: 0, made: None, fraction: None, current: None, message: None, failures: Vec::new(), left_out: Vec::new(),
                              made_paths: Vec::new(), request, share: None, results: None });
        let t = Instant::now();
        let (mut made_at, mut shown_at): (Vec<f64>, Vec<f64>) = (Vec::new(), Vec::new());
        let mut done_at = None;
        loop {
            tick(&mut inner);
            let j = &inner.jobs[0];
            let made = j.made.unwrap_or(j.done.saturating_sub(j.failed)) as usize;
            while made_at.len() < made { made_at.push(t.elapsed().as_secs_f64()); }
            let shown = count(&p, kind).saturating_sub(before);
            while shown_at.len() < shown { shown_at.push(t.elapsed().as_secs_f64()); }
            if j.state.finished() && done_at.is_none() { done_at = Some(Instant::now()); }
            // Done: 5 s more for its last notes to be read (the catalog's journals every 3 s)
            if done_at.is_some_and(|d| d.elapsed() > Duration::from_secs(5)) || t.elapsed() > Duration::from_secs(900) { break; }
            std::thread::sleep(Duration::from_millis(250));
        }
        let j = &inner.jobs[0];
        let lag: Vec<String> = made_at.iter().zip(&shown_at).map(|(m, s)| format!("{:.1}", s - m)).collect();
        println!("{kind}: {:?} made {} failed {} {:?}; shown {} of them (lag s: {})", j.state, made_at.len(), j.failed, j.message, shown_at.len(), lag.join(" "));
        for f in &j.failures { println!("  failed: {f:?}"); }
        rows.push((kind, made_at.len(), shown_at.len().min(made_at.len().max(shown_at.len())), lag.join(" ")));
    }
    println!("\n| Batch | Made | In the view, no look | Lag after each landed (s) |\n|---|---|---|---|");
    for (k, m, s, l) in &rows { println!("| {k} | {m} | {s} | {l} |"); }
    drop(runner);
    std::thread::sleep(Duration::from_millis(500));
    *seen::SEEN.write().unwrap() = None;
    if std::env::var_os("AFL_E2E_KEEP").is_none() { let _ = std::fs::remove_dir_all(&base); }
}
