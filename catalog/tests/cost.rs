// What each thing costs at 10 000 and 100 000 clips (and as many moments): the disk operations counted on the
// simulated NAS, priced at a local disk's, a wired NAS's, Wi-Fi's and a VPN's round trips and speeds, with the time
// this computer spends on it. Not a test of right or wrong: `cargo test --release --test cost -- --ignored --nocapture`

mod common;
use afp_catalog::sim::{ Counts, Fs };
use afp_catalog::{ Catalog, File, Want };
use common::*;
use std::path::Path;
use std::time::Instant;

const NETS: [(&str, f64, f64); 4] = [("local SSD", 0.05, 2000.0), ("NAS, wired", 0.4, 110.0), ("NAS, Wi-Fi", 4.0, 30.0), ("NAS, VPN", 40.0, 5.0)];

fn line(what: &str, c: &Counts, cpu: f64) {
    let t: Vec<String> = NETS.iter().map(|(_, rtt, mb)| fmt(c.seconds(*rtt, *mb) + cpu)).collect();
    println!("| {what} | {} | {} | {} |", c.round_trips(), fmt(cpu), t.join(" | "));
}

fn fmt(s: f64) -> String {
    if s < 1.0 { format!("{:.0} ms", s * 1000.0) } else if s < 120.0 { format!("{s:.1} s") } else if s < 7200.0 { format!("{:.0} min", s / 60.0) } else { format!("{:.1} h", s / 3600.0) }
}

fn measure<T>(c: &Computer, f: impl FnOnce() -> T) -> (T, Counts, f64) {
    c.reset_counts();
    let t = Instant::now();
    let r = f();
    (r, c.counts(), t.elapsed().as_secs_f64())
}

fn report(days: usize) {
    let fs = Fs::new();
    project(&fs, days, 100, 1);
    let n = days * 100;
    println!("\n### {n} clips, {n} moments, in {days} folders each\n");
    println!("| | round trips | this computer | {} |", NETS.iter().map(|n| n.0).collect::<Vec<_>>().join(" | "));
    println!("|---|---|---|---|---|---|---|");
    let mut a = Computer::new(&fs, "a", 1);
    a.cat.load(&Want::All).unwrap();
    let insp = a.insp();
    let (_, c, cpu) = { let a2 = &mut a; let s = a2.shared.clone(); let _ = s; measure(&Computer::new(&fs, "x", 9), || ()) };
    let _ = (c, cpu);
    a.reset_counts();
    let t = Instant::now();
    a.cat.sweep(&roots(), &insp, &Default::default()).unwrap();
    let (c1, cpu1) = (a.counts(), t.elapsed().as_secs_f64());
    line("Catalog built (every folder listed, every moment read: what each look cost before)", &c1, cpu1);
    a.reset_counts();
    let t = Instant::now();
    a.cat.compact().unwrap();
    line("  its snapshot written", &a.counts(), t.elapsed().as_secs_f64());

    let mut b = Computer::new(&fs, "b", 2);
    b.reset_counts();
    let t = Instant::now();
    b.cat.load(&Want::All).unwrap();
    line("A new computer reads it (the Library)", &b.counts(), t.elapsed().as_secs_f64());
    b.restart();
    b.reset_counts();
    let t = Instant::now();
    b.cat.load(&Want::All).unwrap();
    line("The Library opened again (parts on its disk)", &b.counts(), t.elapsed().as_secs_f64());

    let mut c = Computer::new(&fs, "c", 3);
    let keys = Catalog::keys_for(Some("h7x42"), Some("Day007/C0042.mp4"));
    c.reset_counts();
    let t = Instant::now();
    c.cat.load(&Want::Keys(keys.clone())).unwrap();
    let found = c.cat.derived(Some("h7x42"), None).len();
    assert_eq!(found, 1);
    line("The player opens a clip, first time on this computer", &c.counts(), t.elapsed().as_secs_f64());
    c.restart();
    c.reset_counts();
    let t = Instant::now();
    c.cat.load(&Want::Keys(Catalog::keys_for(Some("h8x43"), None))).unwrap();
    line("The player opens another clip", &c.counts(), t.elapsed().as_secs_f64());

    b.reset_counts();
    let t = Instant::now();
    b.cat.refresh().unwrap();
    line("Anything new? (the Library every few seconds)", &b.counts(), t.elapsed().as_secs_f64());

    a.reset_counts();
    let t = Instant::now();
    let r = a.cat.sweep(&roots(), &insp, &Default::default()).unwrap();
    assert_eq!(r.listed, 0);
    line("A look at the folders, nothing changed", &a.counts(), t.elapsed().as_secs_f64());

    // A moment saved by the player on c: its folder touched, noted; b sees it; a looks
    let rel = "Generated/2026-11-10 Moments/new.mp4";
    let bytes = moment_bytes("h7x42", "Day007/C0042.mp4", 1);
    fs.lock().unwrap().write(&Path::new(PROJECT).join(rel), &bytes);
    let mtime = fs.lock().unwrap().nodes[&Path::new(PROJECT).join(rel)].mtime;
    c.reset_counts();
    let t = Instant::now();
    c.cat.touch(&Path::new(PROJECT).join("Generated/2026-11-10 Moments")).unwrap();
    c.cat.note(vec![("p".into(), rel.into(), Some(File { dir: false, size: bytes.len() as u64, mtime, meta: read_meta(&bytes), digest: None }))]).unwrap();
    line("The player notes a moment it saved", &c.counts(), t.elapsed().as_secs_f64());
    b.reset_counts();
    let t = Instant::now();
    assert_eq!(b.cat.refresh().unwrap(), 1);
    line("  the other computer's Library sees it", &b.counts(), t.elapsed().as_secs_f64());
    a.reset_counts();
    let t = Instant::now();
    a.cat.sweep(&roots(), &insp, &Default::default()).unwrap();
    line("  the next look at the folders", &a.counts(), t.elapsed().as_secs_f64());

    a.reset_counts();
    let t = Instant::now();
    a.cat.sweep(&roots(), &insp, &afp_catalog::Sweep { deep: true, ..Default::default() }).unwrap();
    line("The deep look (every folder listed; once a day)", &a.counts(), t.elapsed().as_secs_f64());

    // Footage moved by hand: a folder of 100 clips into another, then found
    {
        let mut f = fs.lock().unwrap();
        f.rename(&Path::new(FOOTAGE).join("Day003"), &Path::new(FOOTAGE).join("Archive/Day003"));
    }
    a.reset_counts();
    let t = Instant::now();
    a.cat.sweep(&roots(), &insp, &Default::default()).unwrap();
    line("A folder of 100 clips moved in Finder: the look that finds it", &a.counts(), t.elapsed().as_secs_f64());

    let _ = c.cat.refresh();
    a.reset_counts();
    let t = Instant::now();
    a.cat.compact().unwrap();
    line("A new snapshot (the Library, now and then)", &a.counts(), t.elapsed().as_secs_f64());
    b.reset_counts();
    let t = Instant::now();
    b.cat.load(&Want::All).unwrap();
    line("  read by the Library on another computer", &b.counts(), t.elapsed().as_secs_f64());
}

#[test]
#[ignore]
fn costs() {
    let sizes: Vec<usize> = std::env::var("COST_DAYS").ok().map(|s| s.split(',').filter_map(|x| x.parse().ok()).collect()).unwrap_or(vec![100, 1000]);
    for d in sizes { report(d); }
}
