//! `app.cli.install` (docs/spec/v0.1/quality-and-release.md#packaging): on macOS the `reda`
//! CLI stays inside REDA.app, and this Command links it into `/usr/local/bin`. The linking is
//! tested in temporary folders; the Command itself never runs here against the real
//! `/usr/local/bin`.

use reda_testkit::TempDir;
use reda_ui::engine::Engine;
use reda_ui::engine::command::ErrorCode;
use reda_ui::shell::Shell;

fn shell() -> Shell {
    Shell::new(
        Engine::builder()
            .user_dirs(None)
            .starter_library(None)
            .headless(false)
            .build(),
    )
}

#[test]
fn the_command_is_offered_on_macos_only() {
    let shell = shell();
    let available = shell
        .engine()
        .registry()
        .available("app.cli.install", None, false);
    if cfg!(target_os = "macos") {
        assert_eq!(available, Ok(()));
    } else {
        assert_eq!(available.unwrap_err().code, ErrorCode::Unavailable);
    }
}

#[cfg(not(target_os = "macos"))]
#[test]
fn elsewhere_the_command_refuses_to_run() {
    let e = shell()
        .run("app.cli.install", serde_json::json!({}))
        .unwrap_err();
    assert_eq!(e.code, ErrorCode::Unavailable);
    assert!(e.message.contains("macOS"), "{}", e.message);
}

#[cfg(unix)]
mod linking {
    use std::path::{Path, PathBuf};

    use reda_ui::help::{CliInstalled, InstallError, install_cli};

    use super::*;

    /// A fake app folder holding the CLI, and an empty bin folder.
    fn setup(tmp: &TempDir) -> (PathBuf, PathBuf) {
        let macos = tmp.path().join("REDA.app/Contents/MacOS");
        std::fs::create_dir_all(&macos).unwrap();
        let cli = macos.join("reda");
        std::fs::write(&cli, "#!/bin/sh\n").unwrap();
        (cli, tmp.path().join("usr/local/bin"))
    }

    fn link_target(link: &Path) -> PathBuf {
        std::fs::read_link(link).unwrap()
    }

    #[test]
    fn links_the_cli_into_the_bin_folder() {
        let tmp = TempDir::new("cli-install");
        let (cli, bin) = setup(&tmp);
        let installed = install_cli(&cli, &bin).unwrap();
        assert_eq!(
            installed,
            CliInstalled {
                link: bin.join("reda").display().to_string(),
                target: cli.display().to_string(),
                already: false,
            }
        );
        assert_eq!(link_target(&bin.join("reda")), cli);
    }

    #[test]
    fn running_it_again_changes_nothing() {
        let tmp = TempDir::new("cli-install");
        let (cli, bin) = setup(&tmp);
        install_cli(&cli, &bin).unwrap();
        assert!(install_cli(&cli, &bin).unwrap().already);
        assert_eq!(link_target(&bin.join("reda")), cli);
    }

    #[test]
    fn a_link_to_an_old_copy_of_the_app_is_replaced() {
        let tmp = TempDir::new("cli-install");
        let (cli, bin) = setup(&tmp);
        std::fs::create_dir_all(&bin).unwrap();
        let old = tmp.path().join("Old REDA.app/Contents/MacOS/reda");
        std::os::unix::fs::symlink(&old, bin.join("reda")).unwrap();
        assert!(!install_cli(&cli, &bin).unwrap().already);
        assert_eq!(link_target(&bin.join("reda")), cli);
    }

    #[test]
    fn a_file_that_is_not_a_link_is_left_alone() {
        let tmp = TempDir::new("cli-install");
        let (cli, bin) = setup(&tmp);
        std::fs::create_dir_all(&bin).unwrap();
        std::fs::write(bin.join("reda"), "someone else's reda").unwrap();
        match install_cli(&cli, &bin) {
            Err(InstallError::Failed(e)) => assert_eq!(e.code, ErrorCode::Rejected),
            other => panic!("expected a refusal, got {other:?}"),
        }
        assert_eq!(
            std::fs::read_to_string(bin.join("reda")).unwrap(),
            "someone else's reda"
        );
    }

    #[test]
    fn a_missing_cli_is_reported() {
        let tmp = TempDir::new("cli-install");
        let (cli, bin) = setup(&tmp);
        std::fs::remove_file(&cli).unwrap();
        match install_cli(&cli, &bin) {
            Err(InstallError::Failed(e)) => assert_eq!(e.code, ErrorCode::NotFound),
            other => panic!("expected not found, got {other:?}"),
        }
    }

    #[test]
    fn a_bin_folder_the_user_cannot_write_asks_for_an_administrator() {
        use std::os::unix::fs::PermissionsExt;
        let tmp = TempDir::new("cli-install");
        let (cli, bin) = setup(&tmp);
        std::fs::create_dir_all(&bin).unwrap();
        std::fs::set_permissions(&bin, std::fs::Permissions::from_mode(0o555)).unwrap();
        // Root ignores the permission bits; there is nothing to test then.
        let probe = bin.join("probe");
        if std::fs::write(&probe, "").is_ok() {
            std::fs::remove_file(&probe).unwrap();
            return;
        }
        let result = install_cli(&cli, &bin);
        std::fs::set_permissions(&bin, std::fs::Permissions::from_mode(0o755)).unwrap();
        assert!(
            matches!(result, Err(InstallError::NeedsAdmin)),
            "{result:?}"
        );
    }
}
