//! The `reda` binary.

use std::process::ExitCode;

const USAGE: &str = "Usage: reda --version";

/// The version string for a local build: the Cargo version plus `-dev` and the git hash.
fn version() -> String {
    format!(
        "{}-dev+{}",
        env!("CARGO_PKG_VERSION"),
        env!("REDA_GIT_HASH")
    )
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    match args.as_slice() {
        ["--version" | "-V"] => {
            println!("reda {}", version());
            ExitCode::SUCCESS
        }
        [] => {
            eprintln!("{USAGE}");
            ExitCode::FAILURE
        }
        [first, ..] => {
            eprintln!("reda: unrecognised argument '{first}'\n{USAGE}");
            ExitCode::FAILURE
        }
    }
}
