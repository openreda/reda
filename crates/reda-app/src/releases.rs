//! The release source behind `app.update.check`: the GitHub Releases API, asked through
//! the OS's `curl` (shipped with macOS, Windows 10 and later, and Linux desktops), so REDA
//! needs no HTTP or TLS stack of its own for its one network call. It is called only when
//! the user runs the check (docs/spec/v0.1/quality-and-release.md#packaging).

use std::process::Command;

use reda_ui::help::{LATEST_RELEASE_API, Release, ReleaseSource, parse_github_release};

pub struct GitHubReleases;

impl ReleaseSource for GitHubReleases {
    fn latest(&self) -> Result<Release, String> {
        let mut curl = Command::new("curl");
        curl.args([
            "--silent",
            "--show-error",
            "--fail",
            "--location",
            "--proto",
            "=https",
            "--max-time",
            "15",
            "--header",
            "Accept: application/vnd.github+json",
            "--user-agent",
            concat!("REDA/", env!("REDA_VERSION")),
            LATEST_RELEASE_API,
        ]);
        #[cfg(windows)]
        {
            // Without this a console window flashes up for curl.
            use std::os::windows::process::CommandExt;
            const CREATE_NO_WINDOW: u32 = 0x0800_0000;
            curl.creation_flags(CREATE_NO_WINDOW);
        }
        let out = curl
            .output()
            .map_err(|e| format!("could not run curl: {e}"))?;
        if !out.status.success() {
            let why = String::from_utf8_lossy(&out.stderr).trim().to_owned();
            return Err(if why.is_empty() {
                format!("curl failed ({})", out.status)
            } else {
                why
            });
        }
        parse_github_release(&String::from_utf8_lossy(&out.stdout))
    }
}
