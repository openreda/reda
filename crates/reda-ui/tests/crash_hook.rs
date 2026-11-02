//! The panic hook writes a local Crash Report. It is the only test in this binary because
//! the hook is global: another test's failure would be reported too.

use reda_testkit::TempDir;
use reda_ui::crash;

#[test]
fn a_panic_writes_a_crash_report_with_the_journal_tail() {
    let dir = TempDir::new("crash-hook");
    crash::remember_journal("{\"id\":\"project.open\"}\n{\"id\":\"schematic.wire.add\"}\n");
    crash::install(dir.path().to_owned(), "9.9.9-test");
    let _ = std::thread::spawn(|| panic!("the canvas fell over")).join();
    let _ = std::panic::take_hook();

    let reports = crash::unseen(dir.path());
    assert_eq!(reports.len(), 1);
    let text = std::fs::read_to_string(&reports[0]).unwrap();
    assert!(text.contains("version: 9.9.9-test"));
    assert!(text.contains(&format!("os: {}", std::env::consts::OS)));
    assert!(text.contains("panic: the canvas fell over"));
    assert!(text.contains("backtrace:"));
    assert!(text.contains("schematic.wire.add"), "{text}");

    crash::mark_seen(dir.path());
    assert!(crash::unseen(dir.path()).is_empty(), "offered once");
    assert!(reports[0].is_file(), "the report itself stays");
}
