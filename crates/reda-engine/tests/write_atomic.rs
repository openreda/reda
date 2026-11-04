use reda_engine::fs::write_atomic;

fn dir(name: &str) -> std::path::PathBuf {
    let dir = std::path::PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(name);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn replaces_the_file_and_leaves_no_temp_file_behind() {
    let dir = dir("write-atomic-replace");
    let path = dir.join("a.reda");
    std::fs::write(&path, b"old").unwrap();
    write_atomic(&path, b"new").unwrap();
    assert_eq!(std::fs::read(&path).unwrap(), b"new");
    let names: Vec<_> = std::fs::read_dir(&dir)
        .unwrap()
        .map(|e| e.unwrap().file_name())
        .collect();
    assert_eq!(names, ["a.reda"]);
}

#[test]
fn fails_without_touching_anything_when_the_folder_is_missing() {
    let dir = dir("write-atomic-missing");
    assert!(write_atomic(&dir.join("nope").join("a.reda"), b"x").is_err());
}
