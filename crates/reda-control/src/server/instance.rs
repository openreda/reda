//! The instance file: its name, the token, writing it owner-only, and clearing stale ones.

use std::io::Write;
use std::net::{Ipv4Addr, SocketAddr, TcpStream};
use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use crate::protocol::InstanceFile;

/// How many servers this process has started, so a second one in the same process (tests,
/// a future multi-window app) does not overwrite the first's file.
static STARTED: AtomicUsize = AtomicUsize::new(0);

/// `<pid>.json`, or `<pid>-<n>.json` for this process's n-th further server.
pub fn file_name() -> String {
    let pid = std::process::id();
    match STARTED.fetch_add(1, Ordering::SeqCst) {
        0 => format!("{pid}.json"),
        n => format!("{pid}-{n}.json"),
    }
}

/// 256 bits from the OS random source, as 64 lowercase hex digits.
pub fn token() -> std::io::Result<String> {
    let mut bytes = [0u8; 32];
    getrandom::fill(&mut bytes).map_err(|e| std::io::Error::other(e.to_string()))?;
    Ok(bytes.iter().map(|b| format!("{b:02x}")).collect())
}

/// Compares tokens in time that does not depend on where they first differ.
pub fn same_token(presented: &str, token: &str) -> bool {
    presented.len() == token.len()
        && presented
            .bytes()
            .zip(token.bytes())
            .fold(0u8, |diff, (a, b)| diff | (a ^ b))
            == 0
}

/// Writes the file whole (a temporary file renamed over it), readable by its owner only.
pub fn write(file: &Path, record: &InstanceFile) -> std::io::Result<()> {
    let mut bytes = serde_json::to_vec_pretty(record).map_err(std::io::Error::other)?;
    bytes.push(b'\n');
    let temp = file.with_extension("json.tmp");
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut out = options.open(&temp)?;
    out.write_all(&bytes)?;
    out.sync_all()?;
    drop(out);
    std::fs::rename(&temp, file)
}

/// Removes the instance files whose port no longer answers: instances that crashed. A
/// live instance binds its port before writing its file, so it always answers.
pub fn remove_stale(dir: &Path) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for path in entries.flatten().map(|e| e.path()) {
        if path.extension().is_none_or(|e| e != "json") {
            continue;
        }
        let Some(record) = std::fs::read(&path)
            .ok()
            .and_then(|bytes| serde_json::from_slice::<InstanceFile>(&bytes).ok())
        else {
            continue;
        };
        let addr = SocketAddr::from((Ipv4Addr::LOCALHOST, record.port));
        if TcpStream::connect_timeout(&addr, Duration::from_millis(500)).is_err() {
            let _ = std::fs::remove_file(&path);
        }
    }
}
