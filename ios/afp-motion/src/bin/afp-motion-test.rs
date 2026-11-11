// Checks the motion side on a Mac: afp-motion-test <clip or .gyroflow> [json overrides] -> prints the sizes, writes the
// Metal source to /tmp/afp-warp.metal (and the GLSL to /tmp/afp-warp.glsl with AFP_DUMP=1)
fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let Some(path) = args.first() else { eprintln!("usage: afp-motion-test <clip> [json]"); std::process::exit(2) };
    let mut j = serde_json::json!({ "project": path });
    if let Some(extra) = args.get(1) {
        let e: serde_json::Value = serde_json::from_str(extra).expect("json");
        for (k, v) in e.as_object().unwrap() { j[k] = v.clone(); }
    }
    let t = std::time::Instant::now();
    match afp_motion::build(&j.to_string()) {
        Ok(w) => {
            std::fs::write("/tmp/afp-warp.metal", &w.msl).unwrap();
            println!("ok in {:.0} ms: src {:?} out {:?} fps {} frames {} table {}x{} note '{}' msl {} bytes",
                     t.elapsed().as_secs_f64() * 1000.0, w.src, w.out, w.fps, w.frames, w.table_w, w.table_h, w.note, w.msl.len());
        }
        Err(e) => { eprintln!("error: {e}"); std::process::exit(1) }
    }
}
