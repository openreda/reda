//! Stamps the version the binary reports into the build as `REDA_VERSION`, for
//! `reda --version` and `reda-app --version` (reda-app uses this same file as its build script).
//!
//! - Local builds report the Cargo version plus `-dev` and the git hash: `0.0.0-dev+1a2b3c4`.
//! - Release builds report the plain version. The release workflow stamps the tag into the
//!   workspace version (`reda-release stamp`) and builds with `REDA_RELEASE=1`.
//!
//! On Windows (MSVC) it also embeds `packaging/windows/reda.manifest`, which declares the
//! binaries long-path aware.

use std::process::Command;

fn git(args: &[&str]) -> Option<String> {
    let out = Command::new("git").args(args).output().ok()?;
    if !out.status.success() {
        return None;
    }
    let text = String::from_utf8(out.stdout).ok()?.trim().to_owned();
    (!text.is_empty()).then_some(text)
}

/// Embeds the Windows application manifest into the binaries of an MSVC build. The GNU
/// toolchain would need a resource compiler; its builds go without.
fn embed_windows_manifest() {
    let os = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    let env = std::env::var("CARGO_CFG_TARGET_ENV").unwrap_or_default();
    if os != "windows" || env != "msvc" {
        return;
    }
    let crate_dir = std::env::var("CARGO_MANIFEST_DIR").expect("cargo sets CARGO_MANIFEST_DIR");
    let manifest = std::path::Path::new(&crate_dir).join("../../packaging/windows/reda.manifest");
    println!("cargo:rerun-if-changed={}", manifest.display());
    println!("cargo:rustc-link-arg-bins=/MANIFEST:EMBED");
    println!(
        "cargo:rustc-link-arg-bins=/MANIFESTINPUT:{}",
        manifest.display()
    );
}

fn main() {
    embed_windows_manifest();
    let cargo_version = std::env::var("CARGO_PKG_VERSION").expect("cargo sets CARGO_PKG_VERSION");
    println!("cargo:rerun-if-env-changed=REDA_RELEASE");
    if std::env::var("REDA_RELEASE").is_ok_and(|v| v == "1") {
        println!("cargo:rustc-env=REDA_VERSION={cargo_version}");
        return;
    }

    let hash = git(&["rev-parse", "--short", "HEAD"]).unwrap_or_else(|| "unknown".to_owned());
    println!("cargo:rustc-env=REDA_VERSION={cargo_version}-dev+{hash}");

    // Rebuild when HEAD moves: on checkout (HEAD itself) or on commit (the branch ref).
    let mut watched = vec!["HEAD".to_owned(), "packed-refs".to_owned()];
    if let Some(branch_ref) = git(&["symbolic-ref", "-q", "HEAD"]) {
        watched.push(branch_ref);
    }
    for path in watched {
        if let Some(on_disk) = git(&["rev-parse", "--git-path", &path]) {
            println!("cargo:rerun-if-changed={on_disk}");
        }
    }
}
