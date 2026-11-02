//! `cargo xtask showcase` stamps the manifest the renderer writes with what it shows.

use reda_xtask::showcase::stamp;
use reda_xtask::site::Manifest;

const DRAWN: &str = r#"{
  "scenes": [
    {"id": "start", "title": "Start page", "caption": "Hello.", "theme": "dark", "file": "start-dark.png"}
  ]
}
"#;

#[test]
fn the_manifest_is_stamped_with_the_commit_and_the_version() {
    let stamped = stamp(DRAWN, Some("0.1.3"), Some("0123456789ab")).unwrap();
    let manifest = Manifest::parse(&stamped).unwrap();
    assert_eq!(manifest.version.as_deref(), Some("0.1.3"));
    assert_eq!(manifest.commit.as_deref(), Some("0123456789ab"));
    assert_eq!(manifest.shots.len(), 1);
    assert_eq!(manifest.shots[0].file, "start-dark.png");
}

#[test]
fn without_a_version_or_commit_the_manifest_keeps_its_meaning() {
    let stamped = stamp(DRAWN, None, None).unwrap();
    assert_eq!(Manifest::parse(&stamped), Manifest::parse(DRAWN));
}

#[test]
fn a_manifest_with_no_scenes_is_refused() {
    let err = stamp(r#"{"scenes": []}"#, None, None).unwrap_err();
    assert!(err.contains("no scenes"), "{err}");
}
