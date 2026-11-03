//! `cargo xtask <chore>`: developer chores. Not shipped.

use std::path::{Path, PathBuf};
use std::process::ExitCode;

const USAGE: &str = "\
Usage:
  cargo xtask icons
      Regenerates the icon set's README and crates/reda-ui/src/icons/table.rs from
      crates/reda-ui/icons/reda.txt; fails when a listed key has no SVG.
  cargo xtask showcase --out <dir> [--version <version>]
      Draws the website's screenshots with the real window on Mesa's llvmpipe (the
      reda-ui `showcase` test), in the dark and light themes, into <dir> with a
      manifest.json. --version records the release they show.
  cargo xtask site --screenshots <dir> [--versions <dir>] --out <dir>
      Builds the website from site/ and a showcase folder, adding a gallery for each
      released version under --versions (one showcase folder per version, named by it).";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let ui = root.join("crates/reda-ui");
    let result = match args.as_slice() {
        ["icons"] => reda_xtask::icons::regenerate(&ui).map(|n| format!("listed {n} icons")),
        ["showcase", rest @ ..] => options(rest, &["--out"], &["--version"]).and_then(|o| {
            let out = PathBuf::from(o[0].unwrap());
            reda_xtask::showcase::render(&root, &out, o[1])
                .map(|n| format!("drew {n} screenshots into {}", out.display()))
        }),
        ["site", rest @ ..] => {
            options(rest, &["--screenshots", "--out"], &["--versions"]).and_then(|o| {
                let out = PathBuf::from(o[1].unwrap());
                reda_xtask::site::build(
                    &root.join("site"),
                    Path::new(o[0].unwrap()),
                    o[2].map(Path::new),
                    &out,
                )
                .and_then(|versions| {
                    // The app's own icon, so the site never holds a stale copy of it.
                    let icon = root.join("packaging/icons/src/app.svg");
                    std::fs::copy(&icon, out.join("icon.svg"))
                        .map_err(|e| format!("{}: {e}", icon.display()))?;
                    Ok(versions)
                })
                .map(|versions| {
                    format!(
                        "built the website into {} with {} released versions",
                        out.display(),
                        versions.len()
                    )
                })
            })
        }
        _ => Err("expected `icons`, `showcase` or `site`".to_owned()),
    };
    match result {
        Ok(done) => {
            println!("xtask: {done}");
            ExitCode::SUCCESS
        }
        Err(message) => {
            eprintln!("xtask: {message}\n{USAGE}");
            ExitCode::FAILURE
        }
    }
}

/// Reads `--flag value` pairs: the `required` flags, then the `optional` ones, in that
/// order; refuses anything else.
fn options<'a>(
    args: &[&'a str],
    required: &[&str],
    optional: &[&str],
) -> Result<Vec<Option<&'a str>>, String> {
    let flags: Vec<&str> = required.iter().chain(optional).copied().collect();
    let mut values = vec![None; flags.len()];
    let mut rest = args;
    while let [flag, value, tail @ ..] = rest {
        let i = flags
            .iter()
            .position(|f| f == flag)
            .ok_or(format!("unknown option {flag}"))?;
        values[i] = Some(*value);
        rest = tail;
    }
    if let [extra] = rest {
        return Err(format!("{extra} needs a value"));
    }
    for (flag, value) in required.iter().zip(&values) {
        if value.is_none() {
            return Err(format!("{flag} is required"));
        }
    }
    Ok(values)
}
