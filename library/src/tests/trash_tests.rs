// Moves a file and a folder of the scratch space to the Trash (Finder): run by hand
#[test]
#[ignore]
fn to_the_trash() {
    let d = std::env::temp_dir().join(format!("afl-trash-{}", std::process::id()));
    std::fs::create_dir_all(d.join("folder/inner")).unwrap();
    std::fs::write(d.join("a file.txt"), b"x").unwrap();
    assert_eq!(super::to_trash(&d.join("a file.txt")).unwrap(), "the Trash");
    assert_eq!(super::to_trash(&d.join("folder")).unwrap(), "the Trash");
    assert!(super::to_trash(&d.join("gone")).is_err());
    let _ = std::fs::remove_dir_all(&d);
}
