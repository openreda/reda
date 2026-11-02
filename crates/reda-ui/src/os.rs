//! Small bridges to the operating system: showing a folder in the file manager and
//! writing a path relative to another.

use std::io;
use std::path::{Component, Path, PathBuf};
use std::process::Command;

/// Opens `folder` in the OS file manager, without waiting for it.
pub fn show_in_folder(folder: &Path) -> io::Result<()> {
    let program = if cfg!(target_os = "windows") {
        "explorer"
    } else if cfg!(target_os = "macos") {
        "open"
    } else {
        "xdg-open"
    };
    Command::new(program).arg(folder).spawn().map(drop)
}

/// `path` written relative to `base` (`../shared/libs`), or `path` itself when the two
/// share no root, such as folders on different Windows drives.
pub fn relative_path(base: &Path, path: &Path) -> PathBuf {
    let base: Vec<Component> = base.components().collect();
    let target: Vec<Component> = path.components().collect();
    let common = base.iter().zip(&target).take_while(|(a, b)| a == b).count();
    if common == 0 {
        return path.to_path_buf();
    }
    let mut out = PathBuf::new();
    for _ in common..base.len() {
        out.push("..");
    }
    for part in &target[common..] {
        out.push(part);
    }
    if out.as_os_str().is_empty() {
        out.push(".");
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn relative_paths_climb_out_of_the_base_and_down_into_the_target() {
        let base = Path::new("/work/Board");
        assert_eq!(
            relative_path(base, Path::new("/work/Board/parts")),
            Path::new("parts")
        );
        assert_eq!(
            relative_path(base, Path::new("/work/shared/libs")),
            Path::new("../shared/libs")
        );
        assert_eq!(relative_path(base, base), Path::new("."));
    }
}
