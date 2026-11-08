//! Disk writes. Every file REDA writes goes through a temp file and a rename.

use std::io::Write;
use std::path::Path;

/// Writes `bytes` to `path` by writing a temp file beside it and renaming it into place,
/// so a crash never leaves a half-written file. The temp file is removed on failure.
pub fn write_atomic(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let name = path
        .file_name()
        .ok_or_else(|| std::io::Error::other(format!("{} has no file name", path.display())))?;
    let mut temp_name = std::ffi::OsString::from(".");
    temp_name.push(name);
    temp_name.push(format!(".{}.tmp", process_id()));
    let temp = path.with_file_name(temp_name);

    let result = (|| {
        let mut file = std::fs::File::create(&temp)?;
        file.write_all(bytes)?;
        file.sync_all()?;
        drop(file);
        std::fs::rename(&temp, path)
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&temp);
    }
    result
}

/// This process's id, for temp file names; 0 on wasm32, whose `std` has no process ids.
pub(crate) fn process_id() -> u32 {
    #[cfg(not(target_arch = "wasm32"))]
    {
        std::process::id()
    }
    #[cfg(target_arch = "wasm32")]
    {
        0
    }
}

/// A path as the user should read it: Windows' verbatim prefix (`\\?\C:\…`, which
/// `std::fs::canonicalize` returns there and which lets paths past the legacy 260-character
/// limit through) is dropped, so `\\?\C:\Work\Demo` shows as `C:\Work\Demo` and
/// `\\?\UNC\server\share\Demo` as `\\server\share\Demo`. Other paths show as they are.
pub fn display_path(path: &Path) -> String {
    display_text(&path.display().to_string())
}

fn display_text(text: &str) -> String {
    if let Some(rest) = text.strip_prefix(r"\\?\UNC\") {
        return format!(r"\\{rest}");
    }
    match text.strip_prefix(r"\\?\") {
        // Only a drive path: other verbatim forms (`\\?\Volume{…}`) have no plain spelling.
        Some(rest) if rest.as_bytes().get(1) == Some(&b':') => rest.to_owned(),
        _ => text.to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::display_text;

    #[test]
    fn the_verbatim_prefix_is_dropped_for_display() {
        assert_eq!(
            display_text(r"\\?\C:\Work\Demo\Demo.reda"),
            r"C:\Work\Demo\Demo.reda"
        );
        assert_eq!(
            display_text(r"\\?\UNC\server\share\Demo"),
            r"\\server\share\Demo"
        );
        assert_eq!(
            display_text(r"\\?\Volume{1234}\Demo"),
            r"\\?\Volume{1234}\Demo"
        );
        assert_eq!(display_text("/home/me/Demo"), "/home/me/Demo");
        assert_eq!(display_text(r"C:\Work"), r"C:\Work");
    }
}
