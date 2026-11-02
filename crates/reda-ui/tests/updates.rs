//! `app.update.check` (docs/spec/v0.1/quality-and-release.md#packaging): asks for the newest
//! release only when it runs, and links to it when it is newer. The release source here is
//! a fake; no test reaches the network.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use reda_testkit::TempDir;
use reda_ui::engine::Engine;
use reda_ui::engine::command::{ErrorCode, UserDirs};
use reda_ui::help::{self, Release, ReleaseSource, UpdateCheck};
use reda_ui::shell::Shell;
use serde_json::json;

/// Answers with a fixed release and counts the calls.
struct FakeReleases {
    answer: Mutex<Result<Release, String>>,
    calls: AtomicUsize,
}

impl FakeReleases {
    fn new(answer: Result<Release, String>) -> Arc<FakeReleases> {
        Arc::new(FakeReleases {
            answer: Mutex::new(answer),
            calls: AtomicUsize::new(0),
        })
    }

    fn calls(&self) -> usize {
        self.calls.load(Ordering::SeqCst)
    }
}

impl ReleaseSource for FakeReleases {
    fn latest(&self) -> Result<Release, String> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        self.answer.lock().unwrap().clone()
    }
}

fn release(version: &str) -> Release {
    Release {
        version: version.to_owned(),
        url: format!("https://github.com/openreda/reda/releases/tag/v{version}"),
    }
}

fn engine_in(home: &TempDir) -> Engine {
    Engine::builder()
        .user_dirs(Some(UserDirs::under(home.path())))
        .starter_library(None)
        .headless(false)
        .build()
}

/// A Shell whose build reports `version` and whose releases come from `source`.
fn shell_with(home: &TempDir, version: &str, source: Arc<FakeReleases>) -> Shell {
    let mut engine = engine_in(home);
    help::configure(&mut engine, version, Some(source));
    Shell::new(engine)
}

fn check(shell: &mut Shell) -> UpdateCheck {
    let out = shell.run("app.update.check", json!({})).unwrap();
    serde_json::from_value(out).unwrap()
}

#[test]
fn a_newer_release_is_linked() {
    let home = TempDir::new("updates");
    let source = FakeReleases::new(Ok(release("0.2.0")));
    let mut shell = shell_with(&home, "0.1.3", source.clone());
    assert_eq!(
        check(&mut shell),
        UpdateCheck {
            current: "0.1.3".into(),
            latest: "0.2.0".into(),
            newer: true,
            url: Some("https://github.com/openreda/reda/releases/tag/v0.2.0".into()),
        }
    );
    assert_eq!(source.calls(), 1);
}

#[test]
fn the_same_or_an_older_release_is_not_an_update() {
    let home = TempDir::new("updates");
    for (latest, current) in [("0.1.3", "0.1.3"), ("0.1.2", "0.1.3"), ("0.9.0", "1.0.0")] {
        let source = FakeReleases::new(Ok(release(latest)));
        let mut shell = shell_with(&home, current, source);
        let found = check(&mut shell);
        assert!(!found.newer, "{latest} is not newer than {current}");
        assert_eq!(found.url, None);
    }
}

#[test]
fn a_local_build_is_older_than_the_release_of_its_version() {
    let home = TempDir::new("updates");
    let source = FakeReleases::new(Ok(release("0.1.3")));
    let mut shell = shell_with(&home, "0.1.3-dev+1a2b3c4", source);
    assert!(check(&mut shell).newer);

    let source = FakeReleases::new(Ok(release("0.1.3")));
    let mut shell = shell_with(&home, "0.1.4-dev+1a2b3c4", source);
    assert!(!check(&mut shell).newer);
}

#[test]
fn versions_compare_by_number_not_text() {
    assert!(help::is_newer("0.10.0", "0.9.9"));
    assert!(help::is_newer("1.0.0", "1.0.0-rc.1"));
    assert!(help::is_newer("1.0.0-rc.11", "1.0.0-rc.2"));
    assert!(!help::is_newer("0.1.0+abc", "0.1.0+def"));
    assert!(!help::is_newer("not a version", "0.1.0"));
}

#[test]
fn nothing_asks_for_releases_until_the_check_runs() {
    let home = TempDir::new("updates");
    let source = FakeReleases::new(Ok(release("9.9.9")));
    let mut shell = shell_with(&home, "0.1.0", source.clone());
    shell
        .run("project.new", json!({"path": home.path(), "name": "Quiet"}))
        .unwrap();
    shell.run("project.save", json!({})).unwrap();
    assert_eq!(
        source.calls(),
        0,
        "no network call on startup or on other Commands"
    );

    check(&mut shell);
    assert_eq!(source.calls(), 1);
}

#[test]
fn without_a_release_source_the_check_is_unavailable() {
    // Tests and headless engines have none: reda-ui has no network code of its own.
    let home = TempDir::new("updates");
    let mut shell = Shell::new(engine_in(&home));
    let e = shell.run("app.update.check", json!({})).unwrap_err();
    assert_eq!(e.code, ErrorCode::Unavailable);
}

#[test]
fn a_failed_check_reports_why() {
    let home = TempDir::new("updates");
    let source = FakeReleases::new(Err("could not resolve api.github.com".into()));
    let mut shell = shell_with(&home, "0.1.0", source);
    let e = shell.run("app.update.check", json!({})).unwrap_err();
    assert!(
        e.message.contains("could not resolve api.github.com"),
        "{}",
        e.message
    );
}

#[test]
fn the_window_is_handed_each_check_once() {
    let home = TempDir::new("updates");
    let source = FakeReleases::new(Ok(release("0.2.0")));
    let mut shell = shell_with(&home, "0.1.0", source);
    let found = check(&mut shell);
    assert_eq!(help::take_checks(shell.engine()), [found]);
    assert!(help::take_checks(shell.engine()).is_empty());
}

#[test]
fn github_release_json_is_read() {
    let json = r#"{"tag_name": "v0.1.7", "html_url": "https://github.com/openreda/reda/releases/tag/v0.1.7", "name": "REDA 0.1.7", "draft": false}"#;
    assert_eq!(
        help::parse_github_release(json),
        Ok(Release {
            version: "0.1.7".into(),
            url: "https://github.com/openreda/reda/releases/tag/v0.1.7".into(),
        })
    );
    assert!(help::parse_github_release("{}").is_err());
    assert!(help::parse_github_release(r#"{"tag_name": "latest", "html_url": "x"}"#).is_err());
    assert!(help::parse_github_release("<html>").is_err());
}
