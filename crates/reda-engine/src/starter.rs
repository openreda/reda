//! Finding the starter Library folder that ships with REDA: `REDA_STARTER_LIBRARY`, else
//! `library/starter` beside the executable (or in a macOS bundle's `Resources`), else the
//! repo's own `library/starter` for builds run from a checkout.

use std::ffi::OsString;
use std::path::{Path, PathBuf};

/// The environment variable naming the starter Library folder; empty means none.
pub const REDA_STARTER_LIBRARY: &str = "REDA_STARTER_LIBRARY";

/// Where the starter Library sits relative to the folder holding the executable.
const SHIPPED: &str = "library/starter";

/// The starter Library for this process: [`resolve`] with `REDA_STARTER_LIBRARY` and the
/// running executable.
pub fn discover() -> Option<PathBuf> {
    resolve(
        std::env::var_os(REDA_STARTER_LIBRARY),
        std::env::current_exe().ok(),
    )
}

/// The starter Library folder given `REDA_STARTER_LIBRARY` and the executable's path. A
/// set variable wins (empty: no starter Library). Otherwise the first existing folder of
/// `<exe dir>/library/starter`, `<exe dir>/../Resources/library/starter` and the repo's
/// `library/starter`; `None` when there is none.
pub fn resolve(variable: Option<OsString>, exe: Option<PathBuf>) -> Option<PathBuf> {
    if let Some(folder) = variable {
        return (!folder.is_empty()).then(|| PathBuf::from(folder));
    }
    let mut candidates = vec![];
    if let Some(dir) = exe.as_deref().and_then(Path::parent) {
        candidates.push(dir.join(SHIPPED));
        candidates.push(dir.join("../Resources").join(SHIPPED));
    }
    candidates.push(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../..")
            .join(SHIPPED),
    );
    candidates.into_iter().find(|folder| folder.is_dir())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_variable_wins_and_empty_turns_it_off() {
        assert_eq!(
            resolve(Some("/x/lib".into()), None),
            Some(PathBuf::from("/x/lib"))
        );
        assert_eq!(resolve(Some("".into()), None), None);
    }

    #[test]
    fn a_folder_beside_the_executable_comes_before_the_repo() {
        let dir = std::env::temp_dir().join(format!("reda-starter-{}", std::process::id()));
        std::fs::create_dir_all(dir.join(SHIPPED)).unwrap();
        let found = resolve(None, Some(dir.join("reda")));
        std::fs::remove_dir_all(&dir).unwrap();
        assert_eq!(found, Some(dir.join(SHIPPED)));
    }

    #[test]
    fn a_checkout_finds_the_repo_library() {
        let found = resolve(None, None).expect("the repo has library/starter");
        assert!(found.join("Starter.redalib").is_file(), "{found:?}");
    }
}
