// The catalog through the scenarios a project lives through: one computer, two at once, one after the other, Finder,
// lost connections, a share not mounted. Each ends by comparing what each computer's catalog says with the files.

mod common;
use afp_catalog::journal;
use afp_catalog::sim::Fs;
use afp_catalog::state::{ key, File, Rec, State, Version };
use afp_catalog::{ Catalog, Error, Want };
use common::*;
use serde_json::json;
use std::path::Path;
use std::sync::Arc;

fn built(fs: &Arc<std::sync::Mutex<Fs>>) -> Computer {
    let mut a = Computer::new(fs, "a", 1);
    a.cat.load(&Want::All).unwrap();
    a.sweep().unwrap();
    a.cat.compact().unwrap();
    a
}

#[test]
fn journal_lines_cut_off_or_damaged_are_left_out() {
    let f = File { dir: false, size: 3, mtime: 7, meta: None, digest: None };
    let mut b = journal::header("a", 1);
    b.extend(journal::line(1, 5, &[("p".into(), "x".into(), Some(f.clone()))]));
    let mut bad = journal::line(2, 6, &[("p".into(), "y".into(), None)]);
    bad[3] ^= 1;
    b.extend(bad);
    b.extend(journal::line(3, 7, &[("p".into(), "z".into(), None)]));
    let cut = journal::line(4, 8, &[("p".into(), "w".into(), None)]);
    b.extend(&cut[..cut.len() - 5]);
    let p = journal::parse(&b);
    assert_eq!(p.lines.iter().map(|l| l.seq).collect::<Vec<_>>(), vec![1, 3]);
    assert_eq!(p.bad, 1);
    assert!(p.torn);
    assert_eq!(p.computer.as_deref(), Some("a"));
    assert_eq!(p.consumed, b.len() - (cut.len() - 5));
}

#[test]
fn records_win_by_version_whatever_order_they_are_read_in() {
    let mut rng = Rng(42);
    for _ in 0..200 {
        let mut recs = Vec::new();
        for i in 0..30 {
            let by: Arc<str> = Arc::from(["a", "b", "c"][rng.below(3) as usize]);
            let f = (rng.below(3) > 0).then(|| File { dir: false, size: rng.below(5), mtime: i, meta: None, digest: None });
            recs.push((format!("k{}", rng.below(6)), Rec { v: Version { l: rng.below(10), by }, f }));
        }
        let mut s1 = State::default();
        for (k, r) in &recs { s1.apply(k.clone(), r.clone()); }
        for _ in 0..5 {
            let mut order = recs.clone();
            for i in (1..order.len()).rev() { order.swap(i, rng.below(i as u64 + 1) as usize); }
            let mut s2 = State::default();
            for (k, r) in order { s2.apply(k, r); }
            // Two records of the same version are the same line read twice: any of them
            for (k, r) in &s1.recs { assert_eq!(s2.recs[k].v, r.v, "{k}"); }
        }
    }
}

#[test]
fn a_new_computer_reads_the_snapshot_not_the_files() {
    let fs = Fs::new();
    project(&fs, 20, 50, 3);
    let a = built(&fs);
    assert!(diff(&truth(&fs), &view(&a.cat)).is_empty());
    let mut b = Computer::new(&fs, "b", 2);
    let info = b.cat.load(&Want::All).unwrap();
    assert!(info.snapshot.is_some());
    assert!(diff(&truth(&fs), &view(&b.cat)).is_empty(), "{:?}", diff(&truth(&fs), &view(&b.cat)));
    let c = b.counts();
    // The catalog's folders listed, not the project's: snap, parts? no: journal, its computers
    assert!(c.lists <= 4, "{c:?}");
    // Loaded again: the parts come from its own disk
    b.restart();
    b.reset_counts();
    let info = b.cat.load(&Want::All).unwrap();
    assert_eq!(info.parts_read, 0);
    assert!(info.parts_cached > 0);
}

#[test]
fn looking_again_at_unchanged_folders_lists_none_and_writes_nothing() {
    let fs = Fs::new();
    project(&fs, 10, 30, 2);
    let mut a = built(&fs);
    a.reset_counts();
    let r = a.sweep().unwrap();
    assert_eq!(r.listed, 0, "{r:?}");
    assert_eq!(r.changes, 0);
    let c = a.counts();
    assert_eq!(c.writes, 0);
    // Only the journals' folders (what the others wrote), none of the project's
    assert!(c.lists <= 2, "{c:?}");
    // One look at each folder's time, the roots' markers
    assert!(c.stats as usize <= r.dirs + 4, "{c:?} {r:?}");
}

#[test]
fn what_the_player_writes_the_other_computer_sees_at_once() {
    let fs = Fs::new();
    project(&fs, 5, 10, 0);
    let mut a = built(&fs);
    let mut b = Computer::new(&fs, "b", 2);
    b.cat.load(&Want::All).unwrap();
    // The player on b: a moment saved, noted
    let rel = "Generated/2026-11-10 Moments/new.mp4";
    let bytes = moment_bytes("h1x1", "Day001/C0001.mp4", 3);
    fs.lock().unwrap().write(&Path::new(PROJECT).join(rel), &bytes);
    let st = b.shared.fs.lock().unwrap().nodes[&Path::new(PROJECT).join(rel)].clone();
    b.cat.note(vec![("p".into(), rel.into(), Some(File { dir: false, size: bytes.len() as u64, mtime: st.mtime, meta: read_meta(&bytes), digest: None }))]).unwrap();
    assert_eq!(a.cat.refresh().unwrap(), 1);
    assert_eq!(a.cat.derived(Some("h1x1"), None).len(), 1);
    // a's look at the folders lists only the one folder that changed, writes no new record for the moment
    let r = a.sweep().unwrap();
    // The project's top, Generated and the folder made in it
    assert_eq!(r.listed, 3, "{r:?}");
    assert!(diff(&truth(&fs), &view(&a.cat)).is_empty());
}

#[test]
fn finder_changes_are_found_by_folder_times() {
    let fs = Fs::new();
    project(&fs, 6, 10, 2);
    let mut a = built(&fs);
    let mut b = Computer::new(&fs, "b", 2);
    b.cat.load(&Want::All).unwrap();
    {
        let mut f = fs.lock().unwrap();
        let p = Path::new(PROJECT);
        // A folder of moments moved deeper (its times kept), one copied in from elsewhere (files' times kept), one
        // file renamed, one removed, a folder of footage removed
        f.rename(&p.join("Generated/2026-11-01 Moments"), &p.join("Trips/Egypt/Day one")).then_some(()).unwrap();
        f.copy(&p.join("Generated/2026-11-02 Moments"), &p.join("Best/copied"));
        f.rename(&p.join("Generated/2026-11-03 Moments/Day002/C0002 m.mp4"), &p.join("Generated/2026-11-03 Moments/Day002/renamed.mp4"));
        f.remove(&p.join("Generated/2026-11-04 Moments/Day003/C0004 m.mp4"));
        f.remove(&Path::new(FOOTAGE).join("Day005"));
    }
    a.sweep().unwrap();
    assert!(diff(&truth(&fs), &view(&a.cat)).is_empty(), "{:#?}", diff(&truth(&fs), &view(&a.cat)));
    b.cat.refresh().unwrap();
    assert!(diff(&truth(&fs), &view(&b.cat)).is_empty());
    // The folders that changed listed once more by the next look, then nothing more to see
    assert_eq!(a.sweep().unwrap().listed, 5);
    assert_eq!(a.sweep().unwrap().listed, 0);
}

#[test]
fn a_file_changed_in_place_is_seen_by_the_deep_look_or_its_note() {
    let fs = Fs::new();
    project(&fs, 2, 4, 1);
    let mut a = built(&fs);
    let rel = "Generated/2026-11-01 Moments/Day000/C0000 m.mp4";
    // Written over in place: its folder's time stays
    fs.lock().unwrap().write(&Path::new(PROJECT).join(rel), &moment_bytes("changed", "Day000/C0000.mp4", 9));
    a.sweep().unwrap();
    assert_eq!(diff(&truth(&fs), &view(&a.cat)).len(), 1, "the quick look doesn't see it");
    a.deep().unwrap();
    assert!(diff(&truth(&fs), &view(&a.cat)).is_empty());
}

#[test]
fn a_share_not_mounted_is_not_a_share_emptied() {
    let fs = Fs::new();
    project(&fs, 3, 5, 1);
    let mut a = built(&fs);
    let before = view(&a.cat);
    // The footage's share gone: its folder empty (a mount point left), its marker with it
    {
        let mut f = fs.lock().unwrap();
        f.remove(Path::new(FOOTAGE));
        f.mkdirs(Path::new(FOOTAGE));
    }
    let r = a.sweep().unwrap();
    assert_eq!(r.skipped_roots, vec!["f".to_string()]);
    assert_eq!(view(&a.cat).iter().filter(|(k, _)| k.0 == "f").count(), before.iter().filter(|(k, _)| k.0 == "f").count());
}

#[test]
fn a_connection_lost_during_a_look_removes_nothing() {
    let fs = Fs::new();
    project(&fs, 8, 10, 2);
    let mut a = built(&fs);
    fs.lock().unwrap().remove(&Path::new(PROJECT).join("Generated/2026-11-05 Moments"));
    fs.lock().unwrap().write(&Path::new(FOOTAGE).join("Day007/new.mp4"), b"new clip");
    let before = view(&a.cat);
    // Down from the start: nothing read, nothing written
    a.set(0.0, 0, true);
    assert!(a.sweep().is_err());
    assert_eq!(view(&a.cat), before);
    // Back: everything seen
    a.set(0.0, 0, false);
    a.sweep().unwrap();
    assert!(diff(&truth(&fs), &view(&a.cat)).is_empty());
}

#[test]
fn the_player_reads_only_its_clips_parts() {
    let fs = Fs::new();
    project(&fs, 100, 100, 2);
    let a = built(&fs);
    let total = a.cat.manifest.as_ref().unwrap().1.parts.len();
    assert!(total >= 8, "{total}");
    let mut c = Computer::new(&fs, "c", 3);
    let info = c.cat.load(&Want::Keys(Catalog::keys_for(Some("h42x10"), Some("Day042/C0010.mp4")))).unwrap();
    assert!(info.parts_read <= 2, "{info:?}");
    let d = c.cat.derived(Some("h42x10"), Some("Day042/C0010.mp4"));
    assert_eq!(d.len(), 1);
    assert_eq!(d[0].1, "Generated/2026-11-15 Moments/Day042/C0010 m.mp4");
}

#[test]
fn removed_segments_send_a_reader_to_the_newer_snapshot() {
    let fs = Fs::new();
    project(&fs, 3, 5, 1);
    let mut a = built(&fs);
    let mut b = Computer::new(&fs, "b", 2);
    b.cat.load(&Want::All).unwrap();
    let big = |i: usize| (0..400).map(|n| ("p".to_string(), format!("Generated/pad{i}/{n:04} a fairly long name to fill the line.mp4"),
                                         Some(File { dir: false, size: n, mtime: n as i64, meta: None, digest: None }))).collect::<Vec<_>>();
    // a writes, folds, cleans up again and again: its old segments go
    for i in 0..(afp_catalog::catalog::KEEP * 3) {
        a.cat.note(big(i)).unwrap();
        a.cat.note(big(i + 100)).unwrap();
        a.cat.compact().unwrap();
        a.cat.clean().unwrap();
    }
    let segs = fs.lock().unwrap().files_under(&Path::new(PROJECT).join(".afp/catalog/journal/a")).len();
    assert!(segs <= afp_catalog::catalog::KEEP + 2, "{segs} segments left");
    assert!(matches!(b.cat.refresh(), Err(Error::Gap(_))));
    b.restart();
    b.cat.load(&Want::All).unwrap();
    assert!(diff(&view(&a.cat), &view(&b.cat)).is_empty());
    // Parts and snapshots no longer used are gone too
    assert!(a.cat.snapshots().unwrap().len() <= afp_catalog::catalog::KEEP + 1);
}

#[test]
fn two_computers_folding_at_once_both_make_good_snapshots() {
    let fs = Fs::new();
    project(&fs, 4, 10, 2);
    let mut a = built(&fs);
    let mut b = Computer::new(&fs, "b", 2);
    b.cat.load(&Want::All).unwrap();
    fs.lock().unwrap().write(&Path::new(PROJECT).join("Generated/one.mp4"), &moment_bytes("h1", "Day000/C0000.mp4", 1));
    a.sweep().unwrap();
    fs.lock().unwrap().write(&Path::new(PROJECT).join("Generated/two.mp4"), &moment_bytes("h2", "Day000/C0000.mp4", 2));
    b.sweep().unwrap();
    a.cat.compact().unwrap();
    b.cat.compact().unwrap();
    assert_eq!(a.cat.snapshots().unwrap().len(), 3);
    let mut c = Computer::new(&fs, "c", 3);
    c.cat.load(&Want::All).unwrap();
    assert!(diff(&truth(&fs), &view(&c.cat)).is_empty(), "{:?}", diff(&truth(&fs), &view(&c.cat)));
}

#[test]
fn a_damaged_snapshot_is_passed_over() {
    let fs = Fs::new();
    project(&fs, 3, 5, 1);
    let mut a = built(&fs);
    fs.lock().unwrap().write(&Path::new(PROJECT).join("Generated/n.mp4"), &moment_bytes("h9", "Day000/C0000.mp4", 1));
    a.sweep().unwrap();
    let newest = a.cat.compact().unwrap();
    // Its list damaged on the share
    let p = Path::new(PROJECT).join(".afp/catalog/snap").join(&newest);
    fs.lock().unwrap().nodes.get_mut(&p).unwrap().data[5] ^= 1;
    let mut b = Computer::new(&fs, "b", 2);
    let info = b.cat.load(&Want::All).unwrap();
    assert_eq!(info.passed, 1);
    assert!(diff(&truth(&fs), &view(&b.cat)).is_empty());
}

#[test]
fn a_note_cut_off_starts_a_new_segment_and_loses_nothing_else() {
    let fs = Fs::new();
    project(&fs, 2, 3, 1);
    let mut a = built(&fs);
    let mk = |n: u64| vec![("p".to_string(), format!("Generated/n{n}.mp4"), Some(File { dir: false, size: n, mtime: n as i64, meta: None, digest: None }))];
    a.cat.note(mk(1)).unwrap();
    a.net.lock().unwrap().wfail = 1.0;
    assert!(a.cat.note(mk(2)).is_err());
    a.net.lock().unwrap().wfail = 0.0;
    a.set(0.0, 0, false);
    a.cat.note(mk(3)).unwrap();
    a.restart();
    a.cat.note(mk(4)).unwrap();
    let mut b = Computer::new(&fs, "b", 2);
    b.cat.load(&Want::All).unwrap();
    for n in [1, 3, 4] { assert!(b.cat.state.get("p", &format!("Generated/n{n}.mp4")).is_some(), "{n}"); }
    let segs = fs.lock().unwrap().files_under(&Path::new(PROJECT).join(".afp/catalog/journal/a")).len();
    assert!(segs >= 2, "{segs}");
}

#[test]
fn a_computer_id_follows_the_machine_not_the_home_folder() {
    let d = std::env::temp_dir().join(format!("afp-cat-id-{}", std::process::id()));
    let a = afp_catalog::computer_id(&d, "MACHINE-1", "Anton's MacBook Pro").unwrap();
    assert!(a.starts_with("anton-s-macbook-pro-"), "{a}");
    assert_eq!(afp_catalog::computer_id(&d, "MACHINE-1", "x").unwrap(), a);
    assert_ne!(afp_catalog::computer_id(&d, "MACHINE-2", "Anton's MacBook Pro").unwrap(), a);
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn records_keys_sort_a_folder_before_its_files() {
    assert!(key("p", "a") < key("p", "a/b"));
    assert!(key("p", "") < key("p", "a"));
    let _ = json!(1);
}

#[test]
fn a_stale_listing_under_a_fresh_folder_time_is_listed_again_later() {
    let fs = Fs::new();
    project(&fs, 3, 4, 1);
    let mut a = built(&fs);
    a.set(0.0, 50, false);
    // Seen once: the listings kept
    a.cat.sweep(&roots(), &a.insp(), &afp_catalog::Sweep { deep: true, ..Default::default() }).unwrap();
    fs.lock().unwrap().write(&Path::new(FOOTAGE).join("Day001/late.mp4"), b"late");
    // The folder's new time seen, its old listing given
    a.net.lock().unwrap().forget_stats();
    a.sweep().unwrap();
    a.set(0.0, 0, false);
    a.sweep().unwrap();
    assert!(diff(&truth(&fs), &view(&a.cat)).is_empty(), "{:?}", diff(&truth(&fs), &view(&a.cat)));
}

#[test]
fn a_clean_up_behind_on_the_listings_removes_nothing_still_needed() {
    // Found on the real NAS: a Mac lists a share's new files ~30 s late, so a clean-up there saw another computer's
    // new snapshot's parts as needed by nothing, and removed them
    let fs = Fs::new();
    project(&fs, 4, 10, 2);
    let mut a = built(&fs);
    let mut b = Computer::new(&fs, "b", 2);
    b.cat.grace = std::time::Duration::from_secs(600);
    b.cat.load(&Want::All).unwrap();
    b.set(0.0, 1000, false);
    for i in 0..12 {
        fs.lock().unwrap().write(&Path::new(PROJECT).join(format!("Generated/n{i}.mp4")), &moment_bytes(&format!("hn{i}"), "x", i));
        a.sweep().unwrap();
        a.cat.compact().unwrap();
        // a cleans up after itself (its segments its snapshots have read go)
        a.cat.clean().unwrap();
        a.cat.clean().unwrap();
        // b cleans from its old listings, again and again
        b.cat.clean().unwrap();
        b.cat.clean().unwrap();
        let mut c = Computer::new(&fs, &format!("c{i}"), 9);
        assert!(c.cat.load(&Want::All).is_ok(), "round {i}: the catalog no longer reads");
        assert!(diff(&truth(&fs), &view(&c.cat)).is_empty());
    }
    // Ten minutes on, its listings fresh: what's no longer needed goes, the rest reads
    b.set(0.0, 0, false);
    b.cat.clock_ahead = 700;
    b.cat.clean().unwrap();
    b.cat.clock_ahead = 1400;
    b.cat.clean().unwrap();
    let parts = fs.lock().unwrap().files_under(&Path::new(PROJECT).join(".afp/catalog/parts")).len();
    assert!(parts <= afp_catalog::catalog::KEEP, "{parts} parts left");
    let mut c = Computer::new(&fs, "late", 9);
    c.cat.load(&Want::All).unwrap();
    assert!(diff(&truth(&fs), &view(&c.cat)).is_empty());
}
