//! Stamps the git commit into the build as `REDA_GIT_HASH`, for `reda --version`.

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
    let hash = git(&["rev-parse", "--short", "HEAD"]).unwrap_or_else(|| "unknown".to_owned());
    println!("cargo:rustc-env=REDA_GIT_HASH={hash}");

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
