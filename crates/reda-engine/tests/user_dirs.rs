//! Where per-user files live: `REDA_HOME`, or the platform folders.

use std::path::Path;

use reda_engine::command::UserDirs;
use reda_engine::user_dirs::resolve;

#[test]
fn reda_home_moves_every_folder_under_it() {
    let home = Path::new("/somewhere/portable");
    let dirs = resolve(Some(home.as_os_str().to_owned())).expect("REDA_HOME always resolves");
    assert_eq!(dirs.config, home.join("config"));
    assert_eq!(dirs.data, home.join("data"));
    assert_eq!(dirs.runtime, home.join("runtime"));
    assert_eq!(dirs, UserDirs::under(home));
}

#[test]
fn files_have_fixed_names_inside_the_folders() {
    let dirs = UserDirs::under(Path::new("/h"));
    assert_eq!(
        dirs.preferences_file(),
        Path::new("/h/config/preferences.toml")
    );
    assert_eq!(dirs.state_file(), Path::new("/h/data/state.json"));
    assert_eq!(dirs.instances_dir(), Path::new("/h/runtime/instances"));
}

#[test]
fn an_empty_reda_home_is_ignored() {
    let dirs = resolve(Some("".into()));
    assert_ne!(dirs, Some(UserDirs::under(Path::new(""))));
}

#[test]
fn without_reda_home_the_platform_folders_are_used() {
    let dirs = resolve(None).expect("the test machine has a home folder");
    for dir in [&dirs.config, &dirs.data, &dirs.runtime] {
        assert!(dir.is_absolute(), "{} is not absolute", dir.display());
    }
    let runtime = dirs.runtime.to_string_lossy().to_lowercase();
    assert!(
        runtime.contains("reda"),
        "{runtime} is not REDA's own folder"
    );
}

#[cfg(unix)]
#[test]
fn the_runtime_folder_is_created_owner_only() {
    use std::os::unix::fs::PermissionsExt;
    let tmp = reda_testkit::TempDir::new("runtime-perms");
    let dirs = UserDirs::under(tmp.path());
    let instances = reda_engine::user_dirs::create_instances_dir(&dirs).expect("created");
    assert_eq!(instances, dirs.instances_dir());
    for dir in [&dirs.runtime, &instances] {
        let mode = std::fs::metadata(dir).expect("exists").permissions().mode();
        assert_eq!(mode & 0o777, 0o700, "{} is mode {mode:o}", dir.display());
    }
}
