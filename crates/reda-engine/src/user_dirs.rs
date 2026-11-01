//! Finding the per-user folders: `REDA_HOME`, else the platform's own folders from
//! `directories::ProjectDirs("com", "openreda", "REDA")`.

use std::ffi::OsString;
use std::path::{Path, PathBuf};

use reda_command::UserDirs;

/// The environment variable that moves every per-user folder under one folder, for tests
/// and portable installs.
pub const REDA_HOME: &str = "REDA_HOME";

/// The per-user folders for this process: [`resolve`] with `REDA_HOME` from the
/// environment.
pub fn discover() -> Option<UserDirs> {
    resolve(std::env::var_os(REDA_HOME))
}

/// The per-user folders given the value of `REDA_HOME`. A non-empty `REDA_HOME` wins; an
/// unset or empty one means the platform folders. `None` when the platform has no home
/// folder (or no folders at all, as in a browser).
///
/// The runtime folder is the platform's (`$XDG_RUNTIME_DIR/reda` on Linux) where there is
/// one. macOS and Windows have none, so it falls back to the cache folder; create it with
/// [`create_instances_dir`], which makes it owner-only.
pub fn resolve(reda_home: Option<OsString>) -> Option<UserDirs> {
    match reda_home {
        Some(home) if !home.is_empty() => Some(UserDirs::under(Path::new(&home))),
        _ => platform(),
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn platform() -> Option<UserDirs> {
    let dirs = directories::ProjectDirs::from("com", "openreda", "REDA")?;
    Some(UserDirs {
        config: dirs.config_dir().to_path_buf(),
        data: dirs.data_dir().to_path_buf(),
        runtime: dirs.runtime_dir().unwrap_or(dirs.cache_dir()).to_path_buf(),
    })
}

#[cfg(target_arch = "wasm32")]
fn platform() -> Option<UserDirs> {
    None
}

/// Creates `<runtime>/instances`, making the runtime folder and it owner-only where the
/// platform has Unix permissions (on Windows the per-user local app-data folder already
/// is). Returns the instances folder.
pub fn create_instances_dir(dirs: &UserDirs) -> std::io::Result<PathBuf> {
    let instances = dirs.instances_dir();
    std::fs::create_dir_all(&instances)?;
    owner_only(&dirs.runtime)?;
    owner_only(&instances)?;
    Ok(instances)
}

#[cfg(unix)]
fn owner_only(dir: &Path) -> std::io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700))
}

#[cfg(not(unix))]
fn owner_only(_dir: &Path) -> std::io::Result<()> {
    Ok(())
}
