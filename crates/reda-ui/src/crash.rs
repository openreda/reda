//! Crash Reports: a panic hook writes a local report (version, OS, the panic, a backtrace
//! and the tail of the Journal) to the per-user data dir; the next start offers it with
//! "Open folder" and "Report on GitHub", which opens a pre-filled issue in the browser for
//! the user to review. Nothing is ever uploaded. The hook makes no emergency save.

use std::collections::VecDeque;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use reda_engine::command::UserDirs;

/// How many Journal lines a report carries.
pub const JOURNAL_TAIL: usize = 50;

/// Where new issues are filed.
const NEW_ISSUE: &str = "https://github.com/openreda/reda/issues/new";

/// The newest Journal lines, for the panic hook to read without the engine.
static TAIL: Mutex<VecDeque<String>> = Mutex::new(VecDeque::new());

/// Keeps the newest of these JSON lines for a Crash Report.
pub fn remember_journal(lines: &str) {
    let mut tail = TAIL.lock().unwrap_or_else(|p| p.into_inner());
    for line in lines.lines() {
        if tail.len() == JOURNAL_TAIL {
            tail.pop_front();
        }
        tail.push_back(line.to_owned());
    }
}

fn journal_tail() -> Vec<String> {
    // A panic while the lock is held must not deadlock the hook.
    match TAIL.try_lock() {
        Ok(tail) => tail.iter().cloned().collect(),
        Err(std::sync::TryLockError::Poisoned(p)) => p.into_inner().iter().cloned().collect(),
        Err(std::sync::TryLockError::WouldBlock) => Vec::new(),
    }
}

/// The folder Crash Reports go in.
pub fn reports_dir(dirs: &UserDirs) -> PathBuf {
    dirs.data.join("crash-reports")
}

/// What a report says.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Report {
    pub version: String,
    /// The OS and CPU architecture.
    pub os: String,
    /// The panic message.
    pub message: String,
    /// Where it panicked, `file:line:column`.
    pub location: String,
    pub backtrace: String,
    pub journal: Vec<String>,
}

impl Report {
    /// The report as written to disk: plain English text, never translated.
    pub fn text(&self) -> String {
        let mut out = String::new();
        let _ = writeln!(out, "REDA Crash Report");
        let _ = writeln!(out, "version: {}", self.version);
        let _ = writeln!(out, "os: {}", self.os);
        let _ = writeln!(out, "panic: {}", self.message);
        let _ = writeln!(out, "at: {}", self.location);
        let _ = writeln!(out, "\nbacktrace:\n{}", self.backtrace.trim_end());
        let _ = writeln!(out, "\njournal (newest last):");
        for line in &self.journal {
            let _ = writeln!(out, "{line}");
        }
        out
    }
}

/// Installs the panic hook. It writes a report into `dir` and then runs the hook that was
/// there before (which prints the panic).
pub fn install(dir: PathBuf, version: &'static str) {
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let message = info
            .payload()
            .downcast_ref::<&str>()
            .map(|s| (*s).to_owned())
            .or_else(|| info.payload().downcast_ref::<String>().cloned())
            .unwrap_or_else(|| "(no message)".into());
        let location = info
            .location()
            .map(|l| format!("{}:{}:{}", l.file(), l.line(), l.column()))
            .unwrap_or_default();
        let report = Report {
            version: version.to_owned(),
            os: os(),
            message,
            location,
            backtrace: std::backtrace::Backtrace::force_capture().to_string(),
            journal: journal_tail(),
        };
        let _ = write(&dir, &report, std::time::SystemTime::now());
        previous(info);
    }));
}

/// The OS and architecture REDA runs on.
pub fn os() -> String {
    format!("{} {}", std::env::consts::OS, std::env::consts::ARCH)
}

/// Writes `report` into `dir` as `crash-<time>-<n>.txt`, marked as not yet offered. Returns the
/// report's path.
pub fn write(dir: &Path, report: &Report, at: std::time::SystemTime) -> std::io::Result<PathBuf> {
    std::fs::create_dir_all(dir)?;
    let secs = at
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    // Names sort by time, then by order within the second.
    let name = (1..)
        .map(|n| format!("crash-{secs}-{n:02}"))
        .find(|name| !dir.join(format!("{name}.txt")).exists())
        .expect("a free name");
    let path = dir.join(format!("{name}.txt"));
    reda_engine::fs::write_atomic(&path, report.text().as_bytes())?;
    std::fs::write(dir.join(format!("{name}.unseen")), b"")?;
    Ok(path)
}

/// Reports in `dir` not yet offered to the user, oldest first.
pub fn unseen(dir: &Path) -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|e| e == "unseen"))
        .map(|p| p.with_extension("txt"))
        .filter(|p| p.is_file())
        .collect();
    out.sort();
    out
}

/// Marks every report in `dir` as offered; the reports themselves stay.
pub fn mark_seen(dir: &Path) {
    for path in unseen(dir) {
        let _ = std::fs::remove_file(path.with_extension("unseen"));
    }
}

/// A link that opens a new GitHub issue pre-filled from the report at `path`, for the user
/// to review before anything is sent. The Journal is left out of the link (it can hold
/// file paths); the issue asks for the report file to be attached instead.
pub fn issue_url(report_text: &str) -> String {
    let field = |key: &str| {
        report_text
            .lines()
            .find_map(|l| l.strip_prefix(&format!("{key}: ")))
            .unwrap_or("")
            .to_owned()
    };
    let backtrace: Vec<&str> = report_text
        .split("\nbacktrace:\n")
        .nth(1)
        .unwrap_or("")
        .split("\n\njournal")
        .next()
        .unwrap_or("")
        .lines()
        .take(40)
        .collect();
    let title = format!("Crash: {}", field("panic"));
    let body = format!(
        "REDA closed unexpectedly.\n\n**What I was doing:**\n\n\n\
         **Version:** {}\n**OS:** {}\n**Panic:** {}\n**At:** {}\n\n\
         ```\n{}\n```\n\n\
         The full Crash Report (with the last Commands run) is in REDA's crash-reports \
         folder; attach it if you are happy to share it.\n",
        field("version"),
        field("os"),
        field("panic"),
        field("at"),
        backtrace.join("\n"),
    );
    format!(
        "{NEW_ISSUE}?title={}&body={}",
        percent_encode(&title),
        percent_encode(&body)
    )
}

/// Percent-encodes everything but RFC 3986 unreserved characters.
fn percent_encode(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for b in text.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                out.push(b as char)
            }
            _ => {
                let _ = write!(out, "%{b:02X}");
            }
        }
    }
    out
}
