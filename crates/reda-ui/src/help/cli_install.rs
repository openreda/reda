//! `app.cli.install`: on macOS the `reda` CLI ships inside `REDA.app`, and this Command
//! links it into `/usr/local/bin` so a terminal finds it
//! (docs/spec/v0.1/quality-and-release.md#packaging). Other packages put the CLI on the PATH
//! themselves, so the Command is offered on macOS only.

use std::io::ErrorKind;
use std::path::{Path, PathBuf};

use reda_engine::command::{
    AvailabilityContext, Command, CommandError, Context, Kind, Meta, NoParams, Unavailable,
};
use schemars::JsonSchema;
use serde::Serialize;

/// Where the link goes.
pub const BIN_DIR: &str = "/usr/local/bin";

/// What [`install_cli`] did.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, JsonSchema)]
pub struct CliInstalled {
    /// The link, such as `/usr/local/bin/reda`.
    pub link: String,
    /// The CLI it points at.
    pub target: String,
    /// Whether the link was already there, pointing at this CLI.
    pub already: bool,
}

/// Why [`install_cli`] did not link the CLI.
#[derive(Debug)]
pub enum InstallError {
    /// The user may not write to the bin folder; an administrator has to.
    NeedsAdmin,
    Failed(CommandError),
}

/// Links `bin_dir/reda` to `cli`, creating `bin_dir` if needed. A link to another copy of
/// the CLI (an older app, say) is replaced; anything else named `reda` there is left alone.
pub fn install_cli(cli: &Path, bin_dir: &Path) -> Result<CliInstalled, InstallError> {
    let link = bin_dir.join("reda");
    let installed = |already| CliInstalled {
        link: link.display().to_string(),
        target: cli.display().to_string(),
        already,
    };
    if !cli.is_file() {
        return Err(InstallError::Failed(CommandError::not_found(format!(
            "the reda CLI is not at {}",
            cli.display()
        ))));
    }
    match std::fs::symlink_metadata(&link) {
        Ok(meta) if meta.file_type().is_symlink() => {
            if std::fs::read_link(&link).is_ok_and(|to| to == cli) {
                return Ok(installed(true));
            }
            std::fs::remove_file(&link).map_err(|e| io_error(&link, e))?;
        }
        Ok(_) => {
            return Err(InstallError::Failed(CommandError::rejected(format!(
                "{} exists and is not a link; move it away first",
                link.display()
            ))));
        }
        Err(e) if e.kind() == ErrorKind::NotFound => {}
        Err(e) => return Err(io_error(&link, e)),
    }
    std::fs::create_dir_all(bin_dir).map_err(|e| io_error(bin_dir, e))?;
    symlink(cli, &link).map_err(|e| io_error(&link, e))?;
    Ok(installed(false))
}

fn io_error(path: &Path, e: std::io::Error) -> InstallError {
    if e.kind() == ErrorKind::PermissionDenied {
        InstallError::NeedsAdmin
    } else {
        InstallError::Failed(CommandError::internal(format!("{}: {e}", path.display())))
    }
}

#[cfg(unix)]
fn symlink(target: &Path, link: &Path) -> std::io::Result<()> {
    std::os::unix::fs::symlink(target, link)
}

#[cfg(not(unix))]
fn symlink(_: &Path, _: &Path) -> std::io::Result<()> {
    Err(std::io::Error::new(
        ErrorKind::Unsupported,
        "symbolic links to the CLI are made on macOS only",
    ))
}

/// The AppleScript that links `cli` into `bin_dir` as an administrator: macOS shows its
/// own password prompt for it.
#[cfg(any(target_os = "macos", test))]
fn admin_script(cli: &Path, bin_dir: &str) -> String {
    fn sh(text: &str) -> String {
        format!("'{}'", text.replace('\'', r"'\''"))
    }
    let shell = format!(
        "mkdir -p {dir} && ln -sfn {cli} {link}",
        dir = sh(bin_dir),
        cli = sh(&cli.display().to_string()),
        link = sh(&format!("{bin_dir}/reda")),
    );
    let quoted = shell.replace('\\', r"\\").replace('"', r#"\""#);
    format!("do shell script \"{quoted}\" with administrator privileges")
}

#[cfg(target_os = "macos")]
fn install_as_admin(cli: &Path) -> Result<(), CommandError> {
    let out = std::process::Command::new("/usr/bin/osascript")
        .arg("-e")
        .arg(admin_script(cli, BIN_DIR))
        .output()
        .map_err(|e| CommandError::internal(format!("could not run osascript: {e}")))?;
    if out.status.success() {
        Ok(())
    } else {
        // Cancelling the password prompt lands here too.
        Err(CommandError::rejected(format!(
            "the CLI was not linked: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        )))
    }
}

#[cfg(not(target_os = "macos"))]
fn install_as_admin(_: &Path) -> Result<(), CommandError> {
    Err(CommandError::unavailable(ONLY_MACOS))
}

/// `app.cli.install`.
pub(super) struct InstallCli;

const ONLY_MACOS: &str = "only on macOS: other packages put the reda CLI on the PATH";

impl Command for InstallCli {
    const META: Meta = Meta {
        id: "app.cli.install",
        title: "Install the reda Command-Line Tool",
        description: "Links the reda command-line tool inside REDA.app into /usr/local/bin, asking for an administrator password if needed. macOS only.",
        category: "app",
        icon: "cli_install",
        kind: Kind::File,
    };
    type Params = NoParams;
    type Output = CliInstalled;

    fn available(&self, _: &AvailabilityContext<'_>) -> Result<(), Unavailable> {
        if cfg!(target_os = "macos") {
            Ok(())
        } else {
            Err(Unavailable {
                reason: ONLY_MACOS.into(),
            })
        }
    }

    fn run(&self, _: &mut Context<'_>, _: NoParams) -> Result<CliInstalled, CommandError> {
        if !cfg!(target_os = "macos") {
            return Err(CommandError::unavailable(ONLY_MACOS));
        }
        // The app binary sits beside the CLI in REDA.app/Contents/MacOS.
        let exe = std::env::current_exe()
            .map_err(|e| CommandError::internal(format!("could not find REDA.app: {e}")))?;
        let cli: PathBuf = exe.with_file_name("reda");
        match install_cli(&cli, Path::new(BIN_DIR)) {
            Ok(installed) => Ok(installed),
            Err(InstallError::Failed(e)) => Err(e),
            Err(InstallError::NeedsAdmin) => {
                install_as_admin(&cli)?;
                Ok(CliInstalled {
                    link: format!("{BIN_DIR}/reda"),
                    target: cli.display().to_string(),
                    already: false,
                })
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_admin_script_quotes_paths_for_the_shell_and_applescript() {
        let script = admin_script(Path::new("/Apps/It's \"REDA\".app/reda"), "/usr/local/bin");
        assert_eq!(
            script,
            r#"do shell script "mkdir -p '/usr/local/bin' && ln -sfn '/Apps/It'\\''s \"REDA\".app/reda' '/usr/local/bin/reda'" with administrator privileges"#
        );
    }
}
