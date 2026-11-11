// Three computers and Finder doing random things to one project on a NAS whose connection drops operations (5–20%),
// goes down for a while and shows stale listings, for many seeds. Two run the Library (look at the folders, fold
// snapshots, clean up), one only the player (writes moments and notes them, reads its clip's part). Then the
// connection is good again, the Libraries look once, and every computer's catalog must say what the files say — and a
// second look must find nothing to list. CATALOG_SEEDS=n runs n seeds (200 by default).

mod common;
use afp_catalog::sim::Fs;
use afp_catalog::{ Catalog, Error, Want };
use common::*;
use std::path::{ Path, PathBuf };
use std::sync::{ Arc, Mutex };

fn files(fs: &Arc<Mutex<Fs>>, under: &str, dirs: bool) -> Vec<PathBuf> {
    let f = fs.lock().unwrap();
    f.nodes.range(PathBuf::from(under)..).take_while(|(k, _)| k.starts_with(under))
        .filter(|(k, n)| n.dir == dirs && !k.to_string_lossy().contains("/.") && k.as_path() != Path::new(under))
        .map(|(k, _)| k.clone()).filter(|k| !k.ends_with("Project.afproject") && !k.ends_with("footage.afplocation")).collect()
}

/// A load that's told to read again (Gap) does; a connection lost leaves it as it was
fn load(c: &mut Computer, want: &Want) -> bool {
    for _ in 0..3 {
        c.restart();
        match c.cat.load(want) {
            Ok(_) => return true,
            Err(Error::Gap(_)) => continue,
            Err(Error::Io(_)) => return false,
            Err(Error::Damaged(e)) => panic!("{}: damaged: {e}", c.name),
            Err(Error::TooNew(e)) => panic!("{}: {e}", c.name),
        }
    }
    false
}

fn refresh(c: &mut Computer) {
    match c.cat.refresh() {
        Ok(_) | Err(Error::Io(_)) => {}
        Err(Error::Gap(_)) => { load(c, &Want::All); }
        Err(Error::Damaged(e)) | Err(Error::TooNew(e)) => panic!("{e}"),
    }
}

fn derived_truth(fs: &Arc<Mutex<Fs>>, hash: &str) -> Vec<String> {
    let mut v: Vec<String> = truth(fs).into_iter().filter(|(_, (_, _, _, m))| m.as_ref().and_then(|m| m["link"]["hash"].as_str()) == Some(hash))
        .map(|((_, rel), _)| rel).collect();
    v.sort();
    v
}

fn run(seed: u64) {
    let fs = Fs::new();
    project(&fs, 4, 6, 2);
    let mut rng = Rng(seed.wrapping_mul(0x9E3779B97F4A7C15) | 1);
    let mut cs = vec![Computer::new(&fs, "a", seed * 3 + 1), Computer::new(&fs, "b", seed * 3 + 2), Computer::new(&fs, "c", seed * 3 + 3)];
    // a builds the catalog
    assert!(load(&mut cs[0], &Want::All));
    cs[0].sweep().unwrap();
    cs[0].cat.compact().unwrap();
    assert!(load(&mut cs[1], &Want::All));
    let mut needs_deep = false;
    let mut n = 0u64;
    for step in 0..400 {
        if step % 50 == 0 {
            for c in &cs {
                let fail = [0.0, 0.05, 0.2][rng.below(3) as usize] * std::env::var("FAIL").ok().and_then(|s| s.parse().ok()).unwrap_or(1.0);
                let lag = [0, 0, 3][rng.below(3) as usize] * std::env::var("LAG").ok().and_then(|s| s.parse().ok()).unwrap_or(1);
                c.set(fail, lag, false);
            }
        }
        // Down for a while now and then
        if rng.below(60) == 0 && std::env::var("FAIL").as_deref() != Ok("0") { let i = rng.below(3) as usize; let f = cs[i].net.lock().unwrap().fail; cs[i].set(f, 0, true); }
        if rng.below(15) == 0 { for c in &cs { let (f, l) = { let n = c.net.lock().unwrap(); (n.fail, n.lag) }; c.set(f, l, false); } }
        n += 1;
        let p = Path::new(PROJECT);
        let op = rng.below(20);
        if std::env::var("TRACE").is_ok() {
            let w = std::env::var("TRACE").unwrap();
            let there = fs.lock().unwrap().exists(&p.join(&w));
            let recs: Vec<String> = cs.iter().map(|c| format!("{}:{:?}", c.name, c.cat.state.rec("p", &w).map(|r| (r.v.l, r.v.by.to_string(), r.f.is_some())))).collect();
            eprintln!("step {step} op {op} there {there} {}", recs.join(" "));
        }
        match op {
            // Finder
            0 => { let d = rng.pick(&files(&fs, PROJECT, true)).cloned().unwrap_or(p.join("Generated"));
                   fs.lock().unwrap().write(&d.join(format!("f{n}.mp4")), &moment_bytes(&format!("h{}x{}", rng.below(4), rng.below(6) / 2 * 2), "x", n)); }
            1 => { if let Some(f) = rng.pick(&files(&fs, PROJECT, false)).cloned() { fs.lock().unwrap().remove(&f); } }
            2 => { if let Some(f) = rng.pick(&files(&fs, PROJECT, false)).cloned() { fs.lock().unwrap().rename(&f, &f.with_file_name(format!("r{n}.mp4"))); } }
            3 => { if let Some(d) = rng.pick(&files(&fs, PROJECT, true)).cloned() {
                       let to = p.join(format!("Moved{}/{n}", rng.below(3)));
                       if !to.starts_with(&d) { fs.lock().unwrap().rename(&d, &to); } } }
            4 => { if let Some(d) = rng.pick(&files(&fs, PROJECT, true)).cloned() {
                       let to = p.join(format!("Copy{n}"));
                       fs.lock().unwrap().copy(&d, &to); } }
            5 => { if rng.below(3) == 0 { if let Some(d) = rng.pick(&files(&fs, PROJECT, true)).cloned() { fs.lock().unwrap().remove(&d); } } }
            6 => { let d = rng.pick(&files(&fs, FOOTAGE, true)).cloned().unwrap_or(PathBuf::from(FOOTAGE));
                   fs.lock().unwrap().write(&d.join(format!("N{n}.mp4")), b"new clip"); }
            7 => { if let Some(f) = rng.pick(&files(&fs, FOOTAGE, false)).cloned() { fs.lock().unwrap().remove(&f); } }
            // The player (or the Library's runs) writing a moment, and noting it
            8 | 9 | 10 => {
                let who = rng.below(3) as usize;
                let rel = format!("Generated/2026-11-10 Moments/p{n}.mp4");
                let bytes = moment_bytes(&format!("h{}x{}", rng.below(4), rng.below(3) * 2), "x", n);
                let full = p.join(&rel);
                fs.lock().unwrap().write(&full, &bytes);
                let mtime = fs.lock().unwrap().nodes[&full].mtime;
                let f = afp_catalog::File { dir: false, size: bytes.len() as u64, mtime, meta: read_meta(&bytes), digest: None };
                let _ = cs[who].cat.note(vec![("p".into(), rel, Some(f))]);
            }
            // Written over in place by one of ours (a link rewritten): its folder touched, noted
            11 => {
                if let Some(f) = rng.pick(&files(&fs, PROJECT, false)).cloned() {
                    let who = rng.below(3) as usize;
                    let bytes = moment_bytes(&format!("h{}x0", rng.below(4)), "x", n);
                    fs.lock().unwrap().write(&f, &bytes);
                    let touched = cs[who].cat.touch(f.parent().unwrap()).is_ok();
                    let mtime = fs.lock().unwrap().nodes[&f].mtime;
                    let rel = f.strip_prefix(p).unwrap().to_string_lossy().into_owned();
                    let rec = afp_catalog::File { dir: false, size: bytes.len() as u64, mtime, meta: read_meta(&bytes), digest: None };
                    let noted = cs[who].cat.note(vec![("p".into(), rel, Some(rec))]).is_ok();
                    // Both lost (the connection gone after the file was written): only the deep look can see it
                    if !touched || !noted { needs_deep = true; }
                }
            }
            // Written over in place by something else, its folder's time left: only the deep look sees it
            19 if rng.below(4) == 0 => {
                if let Some(f) = rng.pick(&files(&fs, PROJECT, false)).cloned() {
                    fs.lock().unwrap().write(&f, &moment_bytes("h0x0", "x", n));
                    needs_deep = true;
                }
            }
            // The Libraries
            12 | 13 | 14 => { let i = rng.below(2) as usize; refresh(&mut cs[i]); let _ = cs[i].sweep(); }
            15 => { let i = rng.below(2) as usize; refresh(&mut cs[i]); if !cs[i].cat.partial { let _ = cs[i].cat.compact(); } }
            16 => { let i = rng.below(2) as usize; let _ = cs[i].cat.clean(); }
            17 => { let i = rng.below(2) as usize; load(&mut cs[i], &Want::All); }
            // The player opening a clip
            18 => { let h = format!("h{}x{}", rng.below(4), rng.below(3) * 2);
                    load(&mut cs[2], &Want::Keys(Catalog::keys_for(Some(&h), None))); }
            _ => { refresh(&mut cs[2]); }
        }
    }
    // The connection good again
    for c in &cs { c.set(0.0, 0, false); }
    for i in 0..2 { if !load(&mut cs[i], &Want::All) { panic!("seed {seed}: {} can't load", cs[i].name); } }
    cs[0].sweep().unwrap();
    cs[0].sweep().unwrap();
    if needs_deep { cs[0].deep().unwrap(); }
    let t = truth(&fs);
    if let Ok(w) = std::env::var("TRACE") {
        eprintln!("end: a {:?}", cs[0].cat.state.rec("p", &w));
        let f = fs.lock().unwrap();
        for (k, v) in f.nodes.iter() {
            if k.to_string_lossy().contains("journal/") && !v.dir {
                for l in afp_catalog::journal::parse(&v.data).lines {
                    for (r, rel, e) in &l.entries { if rel.starts_with(w.rsplit_once('/').map(|x| x.0).unwrap_or("")) && r == "p" { eprintln!("{k:?} q{} l{} {rel} {}", l.seq, l.l, e.as_ref().map(|f| format!("{} {}", f.dir, f.mtime)).unwrap_or("gone".into())); } }
                }
            }
        }
    }
    let d = diff(&t, &view(&cs[0].cat));
    assert!(d.is_empty(), "seed {seed}: a differs from the files:\n{}", d.join("\n"));
    // Nothing more to list (after the second listing of what changed)
    cs[0].sweep().unwrap();
    let r = cs[0].sweep().unwrap();
    assert_eq!(r.listed, 0, "seed {seed}: a second look listed {r:?}");
    refresh(&mut cs[1]);
    let d = diff(&t, &view(&cs[1].cat));
    assert!(d.is_empty(), "seed {seed}: b differs:\n{}", d.join("\n"));
    // b folds, a cleans, a newcomer reads
    cs[1].cat.compact().unwrap();
    cs[0].cat.clean().unwrap();
    cs[0].cat.clean().unwrap();
    let mut e = Computer::new(&fs, "e", seed * 3 + 7);
    assert!(load(&mut e, &Want::All));
    let d = diff(&t, &view(&e.cat));
    assert!(d.is_empty(), "seed {seed}: a newcomer differs:\n{}", d.join("\n"));
    // The player's view of each clip
    for h in 0..4 {
        for x in [0, 2, 4] {
            let hash = format!("h{h}x{x}");
            assert!(load(&mut cs[2], &Want::Keys(Catalog::keys_for(Some(&hash), None))));
            let mut got: Vec<String> = cs[2].cat.derived(Some(&hash), None).into_iter().map(|(_, rel, _)| rel).collect();
            got.sort();
            assert_eq!(got, derived_truth(&fs, &hash), "seed {seed}: the player's moments of {hash}");
        }
    }
}

#[test]
fn many_computers_bad_connections_finder_all_agree_in_the_end() {
    let seeds: u64 = std::env::var("CATALOG_SEEDS").ok().and_then(|s| s.parse().ok()).unwrap_or(200);
    let from: u64 = std::env::var("CATALOG_SEED_FROM").ok().and_then(|s| s.parse().ok()).unwrap_or(0);
    for seed in from..from + seeds { run(seed); }
}
