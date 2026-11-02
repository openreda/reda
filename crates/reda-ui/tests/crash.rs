//! Crash Reports on disk: the next start offers each new one once, and "Report on
//! GitHub" only builds a link for the user's browser. The panic hook has its own test
//! binary (`crash_hook.rs`), since a hook is global to the process.

use reda_testkit::TempDir;
use reda_ui::crash::{self, Report};

#[test]
fn two_reports_in_the_same_second_are_both_kept() {
    let dir = TempDir::new("crash-twice");
    let report = Report {
        version: "1".into(),
        os: crash::os(),
        message: "m".into(),
        location: "a.rs:1:1".into(),
        backtrace: String::new(),
        journal: vec![],
    };
    let at = std::time::SystemTime::now();
    let first = crash::write(dir.path(), &report, at).unwrap();
    let second = crash::write(dir.path(), &report, at).unwrap();
    assert_ne!(first, second);
    assert_eq!(crash::unseen(dir.path()), [first, second]);
}

#[test]
fn the_github_link_is_a_prefilled_issue_without_the_journal() {
    let report = Report {
        version: "0.1.0".into(),
        os: "linux x86_64".into(),
        message: "index out of bounds".into(),
        location: "src/canvas.rs:10:5".into(),
        backtrace: "   0: reda_ui::canvas::draw\n   1: main".into(),
        journal: vec!["{\"id\":\"project.open\",\"params\":{\"path\":\"/home/me/secret\"}}".into()],
    };
    let url = crash::issue_url(&report.text());
    assert!(url.starts_with("https://github.com/openreda/reda/issues/new?title=Crash%3A%20index%20out%20of%20bounds&body="));
    assert!(url.contains("0.1.0"));
    assert!(url.contains("reda_ui%3A%3Acanvas%3A%3Adraw"));
    assert!(
        !url.contains("secret"),
        "the Journal stays on this computer"
    );
}
