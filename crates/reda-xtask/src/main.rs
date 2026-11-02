//! `cargo xtask <chore>`: developer chores. Not shipped.

use std::path::{Path, PathBuf};
use std::process::ExitCode;

const USAGE: &str = "\
Usage:
  cargo xtask icons [--from <dir>]
      Vendors the placeholder icons named in crates/reda-ui/icons/sources.txt, and
      regenerates the set's README and crates/reda-ui/src/icons/table.rs. The npm
      packages are downloaded with curl into target/xtask, unless --from names a folder
      holding them unpacked as lucide/package and tabler/package.";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let ui = root.join("crates/reda-ui");
    let cache = root.join("target/xtask");
    let result = match args.as_slice() {
        ["icons"] => reda_xtask::icons::vendor(&ui, None, &cache),
        ["icons", "--from", dir] => {
            reda_xtask::icons::vendor(&ui, Some(&PathBuf::from(dir)), &cache)
        }
        _ => Err("expected `icons`".to_owned()),
    };
    match result {
        Ok(n) => {
            println!("xtask: vendored {n} icons");
            ExitCode::SUCCESS
        }
        Err(message) => {
            eprintln!("xtask: {message}\n{USAGE}");
            ExitCode::FAILURE
        }
    }
}
