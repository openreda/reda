//! The binaries declare themselves long-path aware on Windows, so Project paths past the
//! legacy 260-character limit work in the Win32 APIs they use beside Rust's std.

use std::path::Path;

const MANIFEST: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../packaging/windows/reda.manifest"
);

#[test]
fn the_windows_manifest_declares_long_path_awareness() {
    let text = std::fs::read_to_string(MANIFEST).unwrap();
    assert!(
        text.contains("<ws2:longPathAware>true</ws2:longPathAware>"),
        "{text}"
    );
    assert!(text.contains("http://schemas.microsoft.com/SMI/2016/WindowsSettings"));
    // The build script both binaries share embeds it.
    let build = Path::new(env!("CARGO_MANIFEST_DIR")).join("../reda-cli/build.rs");
    let build = std::fs::read_to_string(build).unwrap();
    assert!(build.contains("packaging/windows/reda.manifest"));
    assert!(build.contains("/MANIFESTINPUT:"));
}

/// On a Windows MSVC build the manifest is inside the executable.
#[cfg(all(windows, target_env = "msvc"))]
#[test]
fn reda_app_exe_embeds_the_long_path_manifest() {
    let exe = std::fs::read(env!("CARGO_BIN_EXE_reda-app")).unwrap();
    let needle = b"longPathAware";
    assert!(exe.windows(needle.len()).any(|w| w == needle));
}
