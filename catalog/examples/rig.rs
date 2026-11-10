// The catalog on real disks (a project made by tools/catalog_rig.py: footage files, and moments whose content is
// their link as JSON). Prints what each step cost: time, and the disk operations.
//
// rig <project> <footage> <computer> <cache> <command> [args]
//   build | load | player <hash> | refresh | sweep | deep | compact | clean | check
//   note <path in project>          a file written by "the player": its folder touched, noted
//   stress <seconds> <seed>         random notes, looks, folds, clean-ups (to be killed midway)
//   visible <path> <seconds>        how long until a folder's new time and new file are seen here

use afp_catalog::store::{ Counted, DiskStore };
use afp_catalog::{ Catalog, File, Inspect, Root, Store, Sweep, Want };
use serde_json::{ json, Value };
use std::path::{ Path, PathBuf };
use std::sync::Arc;
use std::time::Instant;

struct Insp { store: Arc<dyn Store> }
impl Inspect for Insp {
    fn skip(&self, _r: &str, _rel: &str, name: &str, _d: bool) -> bool { name.starts_with('.') || name.starts_with('#') || name.starts_with("@") }
    fn wants(&self, root: &str, rel: &str) -> bool { root == "p" && rel.ends_with(".mp4") }
    fn inspect(&self, path: &Path, _r: &str, _rel: &str) -> std::io::Result<Option<Value>> {
        Ok(serde_json::from_slice::<Value>(&self.store.read(path)?).ok().map(|l| json!({ "link": l })))
    }
}

fn walk(store: &dyn Store, dir: &Path, rel: &str, out: &mut Vec<(String, bool, u64, i64)>) {
    let Ok(items) = store.list(dir) else { return };
    for i in items {
        if i.name.starts_with('.') || i.name.starts_with('#') || i.name.starts_with('@') { continue; }
        let r = if rel.is_empty() { i.name.clone() } else { format!("{rel}/{}", i.name) };
        out.push((r.clone(), i.stat.dir, i.stat.size, i.stat.mtime));
        if i.stat.dir { walk(store, &dir.join(&i.name), &r, out); }
    }
}

fn main() {
    let a: Vec<String> = std::env::args().skip(1).collect();
    let (project, footage, me, cache, cmd) = (PathBuf::from(&a[0]), PathBuf::from(&a[1]), a[2].clone(), PathBuf::from(&a[3]), a[4].as_str());
    let shared = Arc::new(Counted::new(DiskStore));
    let cat_store: Arc<dyn Store> = shared.clone();
    let mut cat = Catalog::new(cat_store.clone(), Arc::new(DiskStore), &project, &cache, &me);
    let roots = vec![Root { id: "p".into(), dir: project.clone(), marker: Some("Project.afproject".into()), exclude: vec![] },
                     Root { id: "f".into(), dir: footage.clone(), marker: Some("footage.afplocation".into()), exclude: vec![] }];
    let insp = Insp { store: cat_store.clone() };
    let t = Instant::now();
    let say = |what: &str, t: Instant, shared: &Counted<DiskStore>| {
        let c = shared.take();
        println!("{what}: {:.3} s; stats {} lists {} ({} entries) reads {} ({} B) writes {} ({} B) removes {}",
                 t.elapsed().as_secs_f64(), c[0], c[1], c[2], c[3], c[4], c[5], c[6], c[7]);
    };
    match cmd {
        "build" => {
            cat.load(&Want::All).unwrap();
            say("load", t, &shared);
            let t = Instant::now();
            let r = cat.sweep(&roots, &insp, &Sweep::default()).unwrap();
            say(&format!("sweep {r:?}"), t, &shared);
            let t = Instant::now();
            let n = cat.compact().unwrap();
            say(&format!("compact {n}"), t, &shared);
        }
        "load" => { let i = cat.load(&Want::All).unwrap(); say(&format!("load {i:?} records {}", cat.state.len_live()), t, &shared); }
        "player" => {
            let h = &a[5];
            let i = cat.load(&Want::Keys(Catalog::keys_for(Some(h), None))).unwrap();
            let d = cat.derived(Some(h), None);
            // As the player will: each found file looked at
            for (_, rel, _) in &d { let _ = shared.stat(&project.join(rel)); }
            say(&format!("player {h}: {} found; {i:?}", d.len()), t, &shared);
        }
        "refresh" => { cat.load(&Want::All).unwrap(); shared.take(); let t = Instant::now(); let n = cat.refresh().unwrap(); say(&format!("refresh {n} lines"), t, &shared); }
        "sweep" | "deep" => {
            cat.load(&Want::All).unwrap();
            shared.take();
            let t = Instant::now();
            let r = cat.sweep(&roots, &insp, &Sweep { deep: cmd == "deep", ..Default::default() }).unwrap();
            say(&format!("{cmd} {r:?}"), t, &shared);
        }
        "compact" => { cat.load(&Want::All).unwrap(); shared.take(); let t = Instant::now(); let n = cat.compact().unwrap(); say(&format!("compact {n}"), t, &shared); }
        "clean" => { cat.load(&Want::All).unwrap(); shared.take(); let t = Instant::now(); let n = cat.clean().unwrap(); say(&format!("clean {n} removed"), t, &shared); }
        "note" => {
            let rel = &a[5];
            let full = project.join(rel);
            let st = DiskStore.stat(&full).unwrap().unwrap();
            let meta = insp.inspect(&full, "p", rel).unwrap();
            cat.load(&Want::Keys(vec![])).unwrap();
            shared.take();
            let t = Instant::now();
            cat.touch(full.parent().unwrap()).unwrap();
            cat.note(vec![("p".into(), rel.clone(), Some(File { dir: false, size: st.size, mtime: st.mtime, meta, digest: None }))]).unwrap();
            say("note", t, &shared);
        }
        "check" => {
            cat.load(&Want::All).unwrap();
            let mut truth = std::collections::BTreeMap::new();
            for r in &roots {
                let mut out = Vec::new();
                walk(&DiskStore, &r.dir, "", &mut out);
                for (rel, dir, size, mtime) in out {
                    let meta = if !dir && r.id == "p" && rel.ends_with(".mp4") { insp.inspect(&r.dir.join(&rel), "p", &rel).ok().flatten() } else { None };
                    truth.insert((r.id.clone(), rel), (dir, if dir { 0 } else { size }, if dir { 0 } else { mtime }, meta));
                }
            }
            let mine: std::collections::BTreeMap<_, _> = cat.state.live().filter(|(_, rel, _)| !rel.is_empty())
                .map(|(r, rel, f)| ((r.to_string(), rel.to_string()), (f.dir, f.size, if f.dir { 0 } else { f.mtime }, f.meta.clone()))).collect();
            let mut bad = 0;
            for (k, v) in &truth { if mine.get(k) != Some(v) { bad += 1; if bad < 10 { println!("differs: {k:?} files {v:?} catalog {:?}", mine.get(k)); } } }
            for k in mine.keys() { if !truth.contains_key(k) { bad += 1; if bad < 10 { println!("extra: {k:?}"); } } }
            println!("check: {} files, {} in the catalog, {bad} differences", truth.len(), mine.len());
            if bad > 0 { std::process::exit(1); }
        }
        "stress" => {
            let secs: u64 = a[5].parse().unwrap();
            let mut x: u64 = a[6].parse::<u64>().unwrap() | 1;
            let mut rnd = || { x ^= x << 13; x ^= x >> 7; x ^= x << 17; x };
            let _ = cat.load(&Want::All);
            let mut n = 0;
            while t.elapsed().as_secs() < secs {
                n += 1;
                match rnd() % 10 {
                    0..=3 => {
                        let rel = format!("Generated/stress {me}/{n}.mp4");
                        let full = project.join(&rel);
                        let body = json!({ "kind": "moment", "hash": format!("s{}", rnd() % 50), "clip": "x", "in": n }).to_string();
                        std::fs::create_dir_all(full.parent().unwrap()).unwrap();
                        std::fs::write(&full, &body).unwrap();
                        let st = DiskStore.stat(&full).unwrap().unwrap();
                        let _ = cat.touch(full.parent().unwrap());
                        let _ = cat.note(vec![("p".into(), rel, Some(File { dir: false, size: st.size, mtime: st.mtime, meta: Some(json!({ "link": serde_json::from_str::<Value>(&body).unwrap() })), digest: None }))]);
                    }
                    4..=6 => { let _ = cat.sweep(&roots, &insp, &Sweep::default()); }
                    7 => { if cat.refresh().is_err() { let _ = cat.load(&Want::All); } }
                    8 => { let _ = cat.compact(); }
                    _ => { let _ = cat.clean(); }
                }
            }
            say(&format!("stress {n} steps"), t, &shared);
        }
        "visible" => {
            let p = PathBuf::from(&a[5]);
            let secs: f64 = a[6].parse().unwrap();
            let d0 = DiskStore.stat(&p).ok().flatten().map(|s| s.mtime);
            let n0 = DiskStore.list(&p).map(|l| l.len()).unwrap_or(0);
            println!("visible: watching {} (time {d0:?}, {n0} entries)", p.display());
            let (mut seen_t, mut seen_l) = (None, None);
            while t.elapsed().as_secs_f64() < secs && (seen_t.is_none() || seen_l.is_none()) {
                let d = DiskStore.stat(&p).ok().flatten().map(|s| s.mtime);
                let n = DiskStore.list(&p).map(|l| l.len()).unwrap_or(0);
                if seen_t.is_none() && d != d0 { seen_t = Some(t.elapsed().as_secs_f64()); println!("visible: time changed at {:.2} s (listing has {n})", seen_t.unwrap()); }
                if seen_l.is_none() && n != n0 { seen_l = Some(t.elapsed().as_secs_f64()); println!("visible: listing changed at {:.2} s", seen_l.unwrap()); }
                std::thread::sleep(std::time::Duration::from_millis(100));
            }
        }
        _ => eprintln!("unknown command"),
    }
}
