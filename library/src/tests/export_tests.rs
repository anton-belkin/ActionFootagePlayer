// The test project's library as JSON, into AFL_EXPORT_OUT: run by hand
#[test]
#[ignore]
fn export_trip() {
    let root = std::path::PathBuf::from(std::env::var("AFL_EXPORT_ROOT").unwrap());
    let probes: crate::index::SharedProbes = Default::default();
    let v = super::library_json(&root, &probes);
    std::fs::write(std::env::var("AFL_EXPORT_OUT").unwrap(), serde_json::to_vec_pretty(&v).unwrap()).unwrap();
}
