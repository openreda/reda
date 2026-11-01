//! The `reda` binary.

mod commands;
mod run;

use std::process::ExitCode;

const USAGE: &str = "\
Usage:
  reda run <id> [--params <json> | --<key> <value>…] [--project <path>] [--save]
  reda commands [--json]
  reda --version";

/// The version string for a local build: the Cargo version plus `-dev` and the git hash.
fn version() -> String {
    format!(
        "{}-dev+{}",
        env!("CARGO_PKG_VERSION"),
        env!("REDA_GIT_HASH")
    )
}

fn usage_error(message: &str) -> ExitCode {
    eprintln!("reda: {message}\n{USAGE}");
    ExitCode::from(2)
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    match args.as_slice() {
        ["--version" | "-V"] => {
            println!("reda {}", version());
            ExitCode::SUCCESS
        }
        ["run", rest @ ..] => match run::parse(rest) {
            Ok(run_args) => run::execute(run_args),
            Err(message) => usage_error(&message),
        },
        ["commands", rest @ ..] => match commands::parse(rest) {
            Ok(json) => commands::execute(json),
            Err(message) => usage_error(&message),
        },
        [] => {
            eprintln!("{USAGE}");
            ExitCode::FAILURE
        }
        [first, ..] => usage_error(&format!("unrecognised argument '{first}'")),
    }
}
