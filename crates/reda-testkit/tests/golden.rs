use std::path::PathBuf;

use reda_testkit::assert_golden;

fn scratch_file(name: &str, contents: Option<&[u8]>) -> PathBuf {
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("golden-tests");
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join(name);
    let _ = std::fs::remove_file(&path);
    if let Some(bytes) = contents {
        std::fs::write(&path, bytes).unwrap();
    }
    path
}

fn blessing() -> bool {
    std::env::var_os("REDA_BLESS").is_some_and(|v| v == "1")
}

#[test]
fn matching_bytes_pass() {
    let path = scratch_file("match.txt", Some(b"same\n"));
    assert_golden(&path, b"same\n");
}

#[test]
fn differing_bytes_fail_and_name_the_file_and_the_fix() {
    if blessing() {
        return;
    }
    let path = scratch_file("differ.txt", Some(b"old\n"));
    let panic = std::panic::catch_unwind(|| assert_golden(&path, b"new\n"))
        .expect_err("a differing golden must fail");
    let message = panic.downcast_ref::<String>().unwrap();
    assert!(message.contains("differ.txt"), "{message}");
    assert!(message.contains("REDA_BLESS=1"), "{message}");
    assert_eq!(std::fs::read(&path).unwrap(), b"old\n", "file left alone");
}

#[test]
fn missing_golden_fails() {
    if blessing() {
        return;
    }
    let path = scratch_file("missing.txt", None);
    assert!(std::panic::catch_unwind(|| assert_golden(&path, b"x")).is_err());
}
