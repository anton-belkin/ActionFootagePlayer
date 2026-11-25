use super::*;

/// Writing the project never lowers its version: saving locations wrote "version": 2 over the 4 update_doc had
/// raised it to, so older programs (the NAS Helper's catalog among them) would take the project again (#249)
#[test]
fn writing_the_project_never_lowers_its_version() {
    let root = std::env::temp_dir().join(format!("afp-version-kept-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    let version = |root: &Path| read_doc(root).get("version").and_then(Value::as_u64);
    let now = afp_catalog::project::VERSION;
    // An older project is raised to this Library's version
    std::fs::write(root.join(PROJECT_FILE), r#"{"format":"actionfootage-project","version":2,"id":"v"}"#).unwrap();
    update_doc(&root, |_| {}).unwrap();
    assert_eq!(version(&root), Some(now));
    // A change that writes an older version (what saving locations and moving the project did) can't lower it
    update_doc(&root, |d| { d.insert("locations".into(), json!([{"name": "A", "at": "", "path": "../A"}])); d.insert("version".into(), json!(2u64)); }).unwrap();
    assert_eq!(version(&root), Some(now));
    assert!(read_doc(&root).get("locations").is_some_and(Value::is_array));
    // No code writes a fixed version into the project but update_doc: main.rs and the modules its commands are in
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let fixed = ["\"version\".into(), json!(", "2)"].concat();
    let mut read = 0;
    for f in std::fs::read_dir(&dir).unwrap().flatten().map(|e| e.path()).filter(|p| p.extension().is_some_and(|e| e == "rs")) {
        let src = std::fs::read_to_string(&f).unwrap();
        assert!(!src.contains(&fixed), "a fixed project version is written in {}", f.display());
        read += 1;
    }
    assert!(read > 10, "the Library's sources read: {read}");
    let _ = std::fs::remove_dir_all(&root);
}
