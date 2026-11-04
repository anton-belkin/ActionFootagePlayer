// SPDX-License-Identifier: GPL-3.0-or-later
//
// `afp-rpu --look <look.json> [--l1 <min,avg,max | l1.json>] [--frames N] -o <out.rpu>`: Dolby Vision profile 8.4
// RPUs for a look (src/dovi.rs), one per frame, in the .rpu file the player (--macos-dovi-rpu-file) and
// gyroflow-export (--dv-rpu) read. What `dovi_tool generate -p 8.4` writes for DV Lab's config, without dovi_tool.
//   --look     a look as DV Lab saves it (mpv/scripts/dlogm/dv/dji_fit.json)
//   --l1       the frames' L1 in 12-bit PQ codes: one for all of them (default 12,1750,2700, dovi.rs's DEFAULT_L1), or
//              a JSON array with one {"min_pq", "avg_pq", "max_pq"} per frame, "scene_cut": true where a shot starts
//   --frames   how many RPUs: default 1 with a constant L1 (one RPU stands for every frame), the file's length with
//              one per frame
//   --config   also writes the generator config as JSON, for dovi_tool generate -j (tools/make_dv_rpu.sh)

use std::path::PathBuf;

use vs_gyroflow::dovi::{ self, L1, Look };

fn fail(msg: &str) -> ! {
    eprintln!("afp-rpu: {msg}");
    std::process::exit(2);
}

fn main() {
    let (mut look, mut l1, mut frames, mut out, mut config_out) = (None, None, None, None, None);
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        let mut value = |name: &str| args.next().unwrap_or_else(|| fail(&format!("{name} needs a value")));
        match a.as_str() {
            "--look" => look = Some(PathBuf::from(value("--look"))),
            "--l1" => l1 = Some(value("--l1")),
            "--frames" => frames = Some(value("--frames").parse::<usize>().ok().filter(|n| *n > 0)
                .unwrap_or_else(|| fail("--frames takes a number above 0"))),
            "-o" | "--out" => out = Some(PathBuf::from(value("-o"))),
            "--config" => config_out = Some(PathBuf::from(value("--config"))),
            "-h" | "--help" => {
                println!("usage: afp-rpu --look <look.json> [--l1 <min,avg,max | l1.json>] [--frames N] [--config <gen.json>] -o <out.rpu>");
                return;
            }
            _ => fail(&format!("unknown argument {a}")),
        }
    }
    let look_path = look.unwrap_or_else(|| fail("--look <look.json> is needed"));
    let out = out.unwrap_or_else(|| fail("-o <out.rpu> is needed"));
    let look: Look = std::fs::read_to_string(&look_path).map_err(|e| e.to_string())
        .and_then(|s| serde_json::from_str(&s).map_err(|e| e.to_string()))
        .unwrap_or_else(|e| fail(&format!("{}: {e}", look_path.display())));
    let l1 = match l1 {
        None => vec![dovi::DEFAULT_L1],
        Some(s) => match L1::parse(&s) {
            Some(v) => vec![v],
            None if s.contains(',') && !std::path::Path::new(&s).exists() => fail(&format!("--l1 {s}: min,avg,max or a file")),
            None => dovi::read_l1_file(s.as_ref()).unwrap_or_else(|e| fail(&e)),
        },
    };
    let frames = frames.unwrap_or(l1.len());
    let config = dovi::generator_config(&look, &l1, frames).unwrap_or_else(|e| fail(&e));
    if let Some(p) = config_out {
        std::fs::write(&p, dovi::config_json(&config)).unwrap_or_else(|e| fail(&format!("{}: {e}", p.display())));
    }
    let bytes = dovi::rpu_bytes(&config).unwrap_or_else(|e| fail(&e));
    std::fs::write(&out, bytes).unwrap_or_else(|e| fail(&format!("{}: {e}", out.display())));
}
