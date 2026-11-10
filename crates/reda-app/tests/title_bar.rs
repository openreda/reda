//! The window's title bar on Wayland: where the compositor draws none (GNOME), winit's
//! Adwaita title bar must be built in, or the window shows no title and ignores
//! double-click (docs/spec/v0.1/app.md#window-shell). eframe turns winit's default features
//! off, so reda-app turns this one back on.

// `cargo tree --offline` for the Linux target needs the Linux-only crates already fetched,
// which only a Linux checkout has, so this runs on Linux (the Linux CI job) only.
#![cfg(target_os = "linux")]

use std::path::Path;
use std::process::Command;

#[test]
fn the_app_builds_winits_adwaita_title_bar_with_its_text() {
    let out = Command::new(env!("CARGO"))
        .args([
            "tree",
            "--locked",
            "--offline",
            "-p",
            "reda-app",
            "-e",
            "features",
            "--target",
            "x86_64-unknown-linux-gnu",
            "-i",
            "sctk-adwaita",
        ])
        .current_dir(Path::new(env!("CARGO_MANIFEST_DIR")))
        .output()
        .expect("run cargo tree");
    let tree = String::from_utf8_lossy(&out.stdout);
    assert!(
        out.status.success(),
        "sctk-adwaita is not built into reda-app: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    // The full title bar, not `wayland-csd-adwaita-notitle`: the title is drawn with ab_glyph.
    assert!(
        tree.contains("winit feature \"wayland-csd-adwaita\""),
        "{tree}"
    );
    assert!(tree.contains("sctk-adwaita feature \"ab_glyph\""), "{tree}");
}
