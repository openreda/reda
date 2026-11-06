//! Crash recovery in the shell, without a window: Recovery Snapshots on their schedule,
//! the Journal mirror, deleting snapshots on save and clean close, and the recovery
//! prompt offered when a Project opens after an unclean exit.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use reda_testkit::TempDir;
use reda_ui::engine::Engine;
use reda_ui::engine::command::UserDirs;
use reda_ui::engine::model::Scope;
use reda_ui::engine::recovery::{self as snapshots, JOURNAL, MANIFEST};
use reda_ui::recovery::Choice;
use reda_ui::shell::Shell;
use serde_json::json;

const SHEET: &str = "00000000-0000-4000-8000-000000000002";

fn engine_in(home: &Path) -> Engine {
    Engine::builder()
        .user_dirs(Some(UserDirs::under(home)))
        .starter_library(None)
        .headless(false)
        .ids(reda_ui::engine::command::SeededIds::new(0))
        .build()
}

struct Fixture {
    _home: TempDir,
    work: TempDir,
    shell: Shell,
    t0: Instant,
    /// Wires drawn so far: each new one goes on its own row, so it never merges.
    wires: i64,
}

impl Fixture {
    /// A shell with a new Project `Demo`, created through the shell.
    fn new(label: &str) -> Fixture {
        let home = TempDir::new(&format!("{label}-home"));
        let work = TempDir::new(&format!("{label}-work"));
        let mut shell = Shell::new(engine_in(home.path()));
        shell.new_project(work.path(), "Demo").unwrap();
        Fixture {
            _home: home,
            work,
            shell,
            t0: Instant::now(),
            wires: 0,
        }
    }

    fn folder(&self) -> PathBuf {
        self.work.path().join("Demo")
    }

    fn dir(&self) -> PathBuf {
        snapshots::project_dir(&self.folder())
    }

    fn at(&mut self, secs: u64) {
        self.shell
            .tick_recovery(self.t0 + Duration::from_secs(secs));
        self.shell.flush_recovery();
    }

    fn wire(&mut self) {
        let y = format!("{}mm", self.wires * 10);
        self.wires += 1;
        self.shell
            .run(
                "schematic.wire.add",
                json!({"document": SHEET, "points": [{"x": "0mm", "y": y}, {"x": "5mm", "y": y}]}),
            )
            .unwrap();
    }

    fn sheet_snapshot(&self) -> PathBuf {
        self.dir().join(format!("{SHEET}.redasch"))
    }
}

/// Opens `folder` in a fresh shell, as the next session after a crash does.
fn reopen(folder: &Path, home: &Path) -> Shell {
    let mut shell = Shell::new(engine_in(home));
    shell.open_project(folder).unwrap();
    shell
}

#[test]
fn a_snapshot_is_written_about_five_seconds_after_the_last_change() {
    let mut f = Fixture::new("rec-quiet");
    f.at(0);
    f.wire();
    f.at(1);
    f.at(4);
    assert!(!f.sheet_snapshot().exists(), "too soon after the change");
    f.at(7);
    assert!(f.sheet_snapshot().is_file());
    assert!(f.dir().join(MANIFEST).is_file());
}

#[test]
fn steady_editing_still_snapshots_at_most_a_minute_apart() {
    let mut f = Fixture::new("rec-steady");
    let mut written_at = None;
    for second in (0..=70).step_by(2) {
        f.wire();
        f.at(second);
        if written_at.is_none() && f.sheet_snapshot().exists() {
            written_at = Some(second);
        }
    }
    let at = written_at.expect("a snapshot during steady editing");
    assert!((58..=62).contains(&at), "written at {at} s");
}

#[test]
fn only_scopes_changed_since_their_last_snapshot_are_written() {
    let mut f = Fixture::new("rec-changed");
    f.at(0);
    f.wire();
    f.at(1);
    f.at(10);
    assert!(f.sheet_snapshot().is_file());
    std::fs::remove_file(f.sheet_snapshot()).unwrap();

    f.shell
        .run("project.properties.set", json!({"title": "Amp"}))
        .unwrap();
    f.at(11);
    f.at(20);
    assert!(f.dir().join("project.reda").is_file());
    assert!(
        !f.sheet_snapshot().exists(),
        "the sheet did not change again, so it is not rewritten"
    );
}

#[test]
fn saving_deletes_the_snapshots_and_a_clean_close_deletes_everything() {
    let mut f = Fixture::new("rec-save");
    f.at(0);
    f.wire();
    f.at(1);
    f.at(10);
    assert!(f.sheet_snapshot().is_file());
    f.shell.run("project.save", json!({})).unwrap();
    f.at(11);
    assert!(!f.sheet_snapshot().exists());
    assert!(
        snapshots::scan(&f.dir(), Some(&f.folder())).is_empty(),
        "the manifest no longer lists it"
    );
    assert!(f.dir().join(JOURNAL).is_file());

    f.shell.close_cleanly();
    assert!(!f.dir().exists());
}

#[test]
fn the_journal_is_mirrored_line_by_line_from_the_command_that_opened_the_project() {
    let mut f = Fixture::new("rec-journal");
    f.at(0);
    f.wire();
    f.at(1);
    let text = std::fs::read_to_string(f.dir().join(JOURNAL)).unwrap();
    let ids: Vec<String> = text
        .lines()
        .map(|l| {
            serde_json::from_str::<serde_json::Value>(l).unwrap()["id"]
                .as_str()
                .unwrap()
                .to_owned()
        })
        .collect();
    assert_eq!(ids, ["project.new", "schematic.wire.add"]);
}

/// A session that ends without a clean close: snapshots are left behind.
fn crashed_session(label: &str) -> (TempDir, TempDir, PathBuf) {
    let mut f = Fixture::new(label);
    f.at(0);
    f.wire();
    f.at(1);
    f.at(10);
    let folder = f.folder();
    let Fixture { _home, work, .. } = f;
    (_home, work, folder)
}

#[test]
fn opening_after_an_unclean_exit_offers_each_snapshot_and_restore_brings_it_back() {
    let (home, _work, folder) = crashed_session("rec-offer");
    let mut shell = reopen(&folder, home.path());
    let prompt = shell
        .recovery_prompt()
        .expect("leftover snapshots are offered");
    assert_eq!(prompt.rows.len(), 1);
    let row = &prompt.rows[0];
    assert_eq!(row.choice, Choice::Restore);
    assert!(!row.found.disk_changed);
    let file = row.found.snapshot.file.clone();

    shell.restore_snapshot(&file).unwrap();
    assert!(shell.recovery_prompt().is_none(), "every row is settled");
    assert!(
        shell
            .engine()
            .dirty_scopes()
            .contains(&Scope::Document(SHEET.parse().unwrap()))
    );
    let dir = snapshots::project_dir(&folder);
    assert!(
        !dir.join(&file).exists(),
        "the restored snapshot is gone until the next one is taken"
    );
}

#[test]
fn snapshots_are_not_touched_while_the_prompt_is_open() {
    let (home, _work, folder) = crashed_session("rec-paused");
    let mut shell = reopen(&folder, home.path());
    let dir = snapshots::project_dir(&folder);
    let before = std::fs::read(dir.join(MANIFEST)).unwrap();
    let t0 = Instant::now();
    shell.tick_recovery(t0);
    shell.tick_recovery(t0 + Duration::from_secs(120));
    shell.flush_recovery();
    assert_eq!(std::fs::read(dir.join(MANIFEST)).unwrap(), before);
    assert!(
        dir.join("journal.previous.jsonl").is_file(),
        "the crashed session's Journal is kept"
    );
}

#[test]
fn a_file_changed_on_disk_since_the_snapshot_defaults_to_discard() {
    let (home, _work, folder) = crashed_session("rec-changed-disk");
    let sheet = folder.join("Demo.redasch");
    let mut text = std::fs::read_to_string(&sheet).unwrap();
    text.push('\n');
    std::fs::write(&sheet, text).unwrap();
    let mut shell = reopen(&folder, home.path());
    let row = &shell.recovery_prompt().unwrap().rows[0];
    assert!(row.found.disk_changed);
    assert_eq!(row.choice, Choice::Discard);

    // Restore all leaves a row that defaults to Discard for the user to decide.
    shell.restore_all();
    assert_eq!(shell.recovery_prompt().unwrap().rows.len(), 1);
    shell.discard_all();
    assert!(shell.recovery_prompt().is_none());
    assert!(
        !snapshots::project_dir(&folder)
            .join(format!("{SHEET}.redasch"))
            .exists()
    );
    assert!(!shell.engine().is_dirty());
}

#[test]
fn an_unparsable_snapshot_offers_only_discard_and_stays_until_discarded() {
    let (home, _work, folder) = crashed_session("rec-bad");
    let dir = snapshots::project_dir(&folder);
    std::fs::write(dir.join(format!("{SHEET}.redasch")), b"garbage").unwrap();
    let mut shell = reopen(&folder, home.path());
    let row = &shell.recovery_prompt().unwrap().rows[0];
    assert!(row.error().is_some());
    assert!(!row.can_restore());
    let file = row.found.snapshot.file.clone();
    assert!(shell.restore_snapshot(&file).is_err());
    shell.restore_all();
    assert!(
        dir.join(&file).exists(),
        "never deleted without being asked"
    );
    shell.discard_snapshot(&file);
    assert!(shell.recovery_prompt().is_none());
    assert!(!dir.join(&file).exists());
}

#[test]
fn a_document_whose_file_is_gone_is_recreated_on_restore() {
    let mut f = Fixture::new("rec-gone");
    f.at(0);
    let added = f
        .shell
        .run("document.add", json!({"kind": "schematic"}))
        .unwrap();
    let uuid = added["result"]["uuid"].as_str().unwrap().to_owned();
    f.at(1);
    f.at(10);
    let folder = f.folder();
    let Fixture {
        _home: home,
        work: _work,
        ..
    } = f;

    let mut shell = reopen(&folder, home.path());
    let row = shell
        .recovery_prompt()
        .unwrap()
        .rows
        .iter()
        .find(|r| r.found.snapshot.file.starts_with(&uuid))
        .unwrap()
        .clone();
    assert!(row.found.missing, "the new sheet was never saved");
    shell.restore_snapshot(&row.found.snapshot.file).unwrap();
    let project = shell.engine().project().unwrap();
    assert!(project.document(uuid.parse().unwrap()).is_some());
}

#[test]
fn a_project_held_by_a_live_instance_is_handed_off_with_no_recovery() {
    let (home, _work, folder) = crashed_session("rec-live");
    let dirs = UserDirs::under(home.path());
    let held = folder.clone();
    let host = reda_control::headless::HeadlessHost::spawn(&dirs, move || {
        let mut engine = Engine::builder()
            .user_dirs(None)
            .starter_library(None)
            .build();
        engine.run("project.open", json!({"path": held})).unwrap();
        engine
    })
    .unwrap();

    let mut shell = Shell::new(engine_in(home.path()));
    shell.open_project(&folder).unwrap();
    assert!(
        shell.engine().project().is_none(),
        "the live instance keeps it"
    );
    assert!(shell.recovery_prompt().is_none());
    assert!(shell.message().is_some_and(|m| m.contains("Demo")));
    assert!(snapshots::project_dir(&folder).join(MANIFEST).is_file());
    host.stop();
}
