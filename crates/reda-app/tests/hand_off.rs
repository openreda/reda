//! A file-association open (`reda-app Sheet.redasch`) while an instance holds the Project:
//! the new process hands the file over and exits, and the running instance focuses it.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

use reda_ui::bridge::Bridge;
use reda_ui::engine::Engine;
use reda_ui::engine::command::UserDirs;
use reda_ui::engine::model::DocumentKind;
use reda_ui::layout::TabKey;
use reda_ui::shell::Shell;

/// A folder under the system temp dir, removed on drop.
struct Temp(PathBuf);

impl Temp {
    fn new(label: &str) -> Temp {
        let dir = std::env::temp_dir().join(format!("reda-app-{label}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        Temp(dir)
    }
}

impl Drop for Temp {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn engine_in(home: &Path) -> Engine {
    Engine::builder()
        .user_dirs(Some(UserDirs::under(home)))
        .starter_library(None)
        .headless(false)
        .build()
}

#[test]
fn opening_a_document_file_hands_it_to_the_instance_holding_the_project() {
    let home = Temp::new("hand-off-home");
    let work = Temp::new("hand-off-work");
    let mut shell = Shell::new(engine_in(&home.0));
    shell.new_project(&work.0, "Demo").unwrap();
    let (pcb, name) = shell
        .engine()
        .project()
        .unwrap()
        .documents()
        .find(|d| d.kind == DocumentKind::Pcb)
        .map(|d| (d.uuid, d.name.clone()))
        .unwrap();
    let file = work.0.join("Demo").join(format!("{name}.redapcb"));
    let mut bridge = Bridge::start(&shell);
    assert!(bridge.is_on());

    let mut child = Command::new(env!("CARGO_BIN_EXE_reda-app"))
        .arg(&file)
        .env("REDA_HOME", &home.0)
        .spawn()
        .expect("run reda-app");
    let start = Instant::now();
    let status = loop {
        bridge.pump(&mut shell);
        if let Some(status) = child.try_wait().unwrap() {
            break status;
        }
        if start.elapsed() > Duration::from_secs(60) {
            let _ = child.kill();
            panic!("reda-app did not hand the file over and exit");
        }
        std::thread::sleep(Duration::from_millis(5));
    };
    assert!(status.success());
    assert!(bridge.take_focus_request());
    assert_eq!(shell.layout().focus, Some(TabKey::Document(pcb)));
    bridge.stop();
}
