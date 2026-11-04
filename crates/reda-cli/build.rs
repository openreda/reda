//! Stamps the version the binary reports into the build as `REDA_VERSION`, for
//! `reda --version` and `reda-app --version` (reda-app uses this same file as its build script).
//!
//! - Local builds report the Cargo version plus `-dev` and the git hash: `0.0.0-dev+1a2b3c4`.
//! - Release builds report the plain version. The release workflow stamps the tag into the
//!   workspace version (`reda-release stamp`) and builds with `REDA_RELEASE=1`.

use std::process::Command;

fn git(args: &[&str]) -> Option<String> {
    let out = Command::new("git").args(args).output().ok()?;
    if !out.status.success() {
        return None;
    }
    let text = String::from_utf8(out.stdout).ok()?.trim().to_owned();
    (!text.is_empty()).then_some(text)
}

fn main() {
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
