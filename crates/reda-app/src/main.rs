//! The REDA GUI binary.
//!
//! Usage: `reda-app [PROJECT]`, where PROJECT is a `.reda` file or a Project folder to
//! open at once. The OS-specific work happens here: the engine gets the OS trash for
//! deleted Documents' files and the machine's installed fonts, then `reda-ui` runs the
//! window.

mod fonts;
mod os_trash;

use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::Arc;

use reda_ui::engine::{Engine, Source, preferences, user_dirs};

use crate::fonts::SystemFonts;
use crate::os_trash::OsTrash;

fn main() -> ExitCode {
    let mut project = None;
    for arg in std::env::args_os().skip(1) {
        match arg.to_str() {
            Some("--version" | "-V") => {
                // Set by the build script: `X.Y.Z` in a release, `X.Y.Z-dev+<git hash>` locally.
                println!("reda-app {}", env!("REDA_VERSION"));
                return ExitCode::SUCCESS;
            }
            Some("--help" | "-h") => {
                println!("usage: reda-app [PROJECT]");
                return ExitCode::SUCCESS;
            }
            _ if project.is_none() => project = Some(PathBuf::from(arg)),
            _ => {
                eprintln!("usage: reda-app [PROJECT]");
                return ExitCode::from(2);
            }
        }
    }

    let dirs = user_dirs::discover();
    let prefs = preferences::current(dirs.as_ref());
    let engine = Engine::builder()
        .source(Source::Gui)
        .headless(false)
        .user_dirs(dirs)
        .trash(OsTrash)
        .undo_depth(prefs.general.undo_depth.map(|d| d.get() as usize))
        .build();
    let fonts = Arc::new(SystemFonts::scan_in_background(SystemFonts::os_dirs()));

    match reda_ui::run(engine, fonts, project) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("reda-app: {e}");
            ExitCode::FAILURE
        }
    }
}
