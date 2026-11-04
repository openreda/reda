//! The `reda` binary.

mod bridge;
mod commands;
mod mcp;
mod run;

use std::process::ExitCode;

const USAGE: &str = "\
Usage:
  reda mcp [--project <path>]
  reda run <id> [--params <json> | --<key> <value>…] [--project <path>] [--save]
  reda run --script <journal.jsonl> [--into <folder>] [--project <path>] [--save]
  reda commands [--json]
  reda --version";

fn usage_error(message: &str) -> ExitCode {
    eprintln!("reda: {message}\n{USAGE}");
    ExitCode::from(2)
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    match args.as_slice() {
        ["--version" | "-V"] => {
            // Set by build.rs: `X.Y.Z` in a release, `X.Y.Z-dev+<git hash>` locally.
            println!("reda {}", env!("REDA_VERSION"));
            ExitCode::SUCCESS
        }
        ["run", rest @ ..] => match run::parse(rest) {
            Ok(run_args) => run::execute(run_args),
            Err(message) => usage_error(&message),
        },
        ["mcp", rest @ ..] => match mcp::parse(rest) {
            Ok(mcp_args) => mcp::execute(mcp_args),
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
