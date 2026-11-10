//! `reda.exe` declares itself long-path aware on Windows, like the app (see
//! `crates/reda-app/tests/long_paths.rs`).

/// On a Windows MSVC build the manifest is inside the executable.
#[cfg(all(windows, target_env = "msvc"))]
#[test]
fn reda_exe_embeds_the_long_path_manifest() {
    let exe = std::fs::read(env!("CARGO_BIN_EXE_reda")).unwrap();
    let needle = b"longPathAware";
    assert!(exe.windows(needle.len()).any(|w| w == needle));
}
