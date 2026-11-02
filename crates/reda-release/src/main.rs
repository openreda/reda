//! The `reda-release` binary that `.github/workflows/release.yml` calls. Not shipped.

use std::io::Read;
use std::path::PathBuf;
use std::process::ExitCode;

use reda_release::{Bump, Plan, PlanInput, Version, parse_changes, plan, release_notes};

const USAGE: &str = "\
Usage:
  reda-release plan [--labels <labels>] [--message <commit message>] [--bump none|patch|minor|major] < tags
      Prints GitHub step outputs: release=true|false, version, tag, previous_tag (or reason).
      Labels are separated by newlines or commas; tags come one per line on stdin.
  reda-release stamp <version> [--root <workspace dir>]
      Writes the version into Cargo.toml's [workspace.package] and Cargo.lock.
  reda-release notes <version> < changes
      Prints the release notes; changes are `<pr number or empty>\\t<title>` lines.";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    let result = match args.as_slice() {
        ["plan", rest @ ..] => run_plan(rest),
        ["stamp", version, rest @ ..] => run_stamp(version, rest),
        ["notes", version] => run_notes(version),
        _ => Err("expected plan, stamp or notes".to_owned()),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("reda-release: {message}\n{USAGE}");
            ExitCode::FAILURE
        }
    }
}

/// Splits `--key value` pairs, rejecting anything not in `known`.
fn options<'a>(args: &[&'a str], known: &[&str]) -> Result<Vec<(&'a str, &'a str)>, String> {
    let mut pairs = Vec::new();
    let mut rest = args.iter();
    while let Some(key) = rest.next() {
        if !known.contains(key) {
            return Err(format!("unexpected argument '{key}'"));
        }
        let value = rest.next().ok_or(format!("{key} needs a value"))?;
        pairs.push((*key, *value));
    }
    Ok(pairs)
}

fn read_stdin() -> Result<String, String> {
    let mut text = String::new();
    std::io::stdin()
        .read_to_string(&mut text)
        .map_err(|e| format!("reading stdin: {e}"))?;
    Ok(text)
}

fn run_plan(args: &[&str]) -> Result<(), String> {
    let mut labels = "";
    let mut message = "";
    let mut forced = None;
    for (key, value) in options(args, &["--labels", "--message", "--bump"])? {
        match key {
            "--labels" => labels = value,
            "--message" => message = value,
            _ => forced = parse_bump(value)?,
        }
    }
    let labels: Vec<&str> = labels
        .split(['\n', ','])
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .collect();
    let tags_text = read_stdin()?;
    let tags: Vec<&str> = tags_text.lines().map(str::trim).collect();

    match plan(&PlanInput {
        tags: &tags,
        labels: &labels,
        message,
        forced,
    }) {
        Plan::Release { version, previous } => {
            println!("release=true");
            println!("version={version}");
            println!("tag={}", version.tag());
            println!(
                "previous_tag={}",
                previous.map(|p| p.tag()).unwrap_or_default()
            );
        }
        Plan::Skip { reason } => {
            println!("release=false");
            println!("reason={reason}");
        }
    }
    Ok(())
}

/// `workflow_dispatch`'s bump input; empty or `none` on an ordinary push.
fn parse_bump(text: &str) -> Result<Option<Bump>, String> {
    match text {
        "" | "none" => Ok(None),
        "patch" => Ok(Some(Bump::Patch)),
        "minor" => Ok(Some(Bump::Minor)),
        "major" => Ok(Some(Bump::Major)),
        other => Err(format!("unknown bump '{other}'")),
    }
}

fn run_stamp(version: &str, args: &[&str]) -> Result<(), String> {
    let version: Version = version.parse()?;
    let mut root = PathBuf::from(".");
    for (_, value) in options(args, &["--root"])? {
        root = PathBuf::from(value);
    }
    rewrite(&root.join("Cargo.toml"), |text| {
        reda_release::stamp_manifest(text, version)
    })?;
    rewrite(&root.join("Cargo.lock"), |text| {
        reda_release::stamp_lock(text, version)
    })
}

fn rewrite(
    path: &std::path::Path,
    edit: impl FnOnce(&str) -> Result<String, String>,
) -> Result<(), String> {
    let text =
        std::fs::read_to_string(path).map_err(|e| format!("reading {}: {e}", path.display()))?;
    std::fs::write(path, edit(&text)?).map_err(|e| format!("writing {}: {e}", path.display()))
}

fn run_notes(version: &str) -> Result<(), String> {
    let version: Version = version.parse()?;
    let changes = parse_changes(&read_stdin()?);
    print!("{}", release_notes(version, &changes));
    Ok(())
}
