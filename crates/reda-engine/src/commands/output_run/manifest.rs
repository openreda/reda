//! The Output Manifest (outputs.md, "Output Manifest"): `manifest.json` beside the zip, in
//! canonical JSON. It is the one output that holds the run's date, and it lists every file
//! the run wrote with its size and SHA-256, the zip's SHA-256, what was run and the run's
//! warnings. A zip can't hold its own hash, so the manifest stays out of it.

use reda_command::{CommandError, Timestamp};
use reda_model::{FabPreset, OutputJob, Uuid, Violation, ViolationArg};
use reda_output::sha256_hex;
use serde::Serialize;

use crate::canonical::CanonicalWriter;

/// The manifest's file name in the job's destination.
pub(super) const MANIFEST: &str = "manifest.json";

/// What one run wrote, for the manifest.
pub(super) struct Run<'a> {
    pub version: &'a str,
    pub job: &'a OutputJob,
    pub preset: &'a FabPreset,
    pub pcb: Uuid,
    pub revision: &'a str,
    pub created: Timestamp,
    /// The zip's path in the destination, `/`-separated, and its bytes.
    pub zip: (&'a str, &'a [u8]),
    /// Every file written but the manifest, in run order: path, function and bytes.
    pub files: &'a [(String, &'static str, Vec<u8>)],
    pub warnings: &'a [Violation],
}

#[derive(Serialize)]
struct JobRow<'a> {
    uuid: Uuid,
    name: &'a str,
}

#[derive(Serialize)]
struct PresetRow<'a> {
    name: &'a str,
    /// The SHA-256 of the preset copy's JSON, so a changed copy shows.
    sha256: String,
}

#[derive(Serialize)]
struct ZipRow<'a> {
    path: &'a str,
    sha256: String,
}

#[derive(Serialize)]
struct FileRow<'a> {
    path: &'a str,
    size: usize,
    sha256: String,
    kind: &'a str,
}

#[derive(Serialize)]
struct WarningRow {
    check: String,
    message: String,
}

/// The manifest's bytes.
pub(super) fn manifest(run: &Run<'_>) -> Result<Vec<u8>, CommandError> {
    let failed = |e: crate::canonical::CanonicalError| CommandError::internal(e.to_string());
    let preset =
        serde_json::to_vec(run.preset).map_err(|e| CommandError::internal(e.to_string()))?;
    let mut w = CanonicalWriter::new(1);
    w.field("reda", run.version).map_err(failed)?;
    w.field(
        "job",
        &JobRow {
            uuid: run.job.uuid,
            name: &run.job.name,
        },
    )
    .map_err(failed)?;
    w.field(
        "fab_preset",
        &PresetRow {
            name: &run.preset.name,
            sha256: sha256_hex(&preset),
        },
    )
    .map_err(failed)?;
    w.field("pcb", &run.pcb).map_err(failed)?;
    w.field("revision", run.revision).map_err(failed)?;
    w.field("date", &iso_time(run.created)).map_err(failed)?;
    w.field(
        "zip",
        &ZipRow {
            path: run.zip.0,
            sha256: sha256_hex(run.zip.1),
        },
    )
    .map_err(failed)?;
    w.list(
        "files",
        run.files.iter().map(|(path, kind, bytes)| FileRow {
            path,
            size: bytes.len(),
            sha256: sha256_hex(bytes),
            kind,
        }),
    )
    .map_err(failed)?;
    w.list(
        "warnings",
        run.warnings.iter().map(|v| WarningRow {
            check: v.check.clone(),
            message: message(v),
        }),
    )
    .map_err(failed)?;
    Ok(w.finish())
}

/// `at` as `YYYY-MM-DDTHH:MM:SSZ`.
pub(super) fn iso_time(at: Timestamp) -> String {
    // `utc()` is `YYYY-MM-DD HH:MM:SS UTC`.
    let utc = at.utc();
    let (date, rest) = utc.split_once(' ').unwrap_or((&utc, ""));
    let time = rest.trim_end_matches(" UTC");
    format!("{date}T{time}Z")
}

/// `at`'s date, `YYYY-MM-DD`, for the `${date}` token.
pub(super) fn iso_date(at: Timestamp) -> String {
    iso_time(at)[..10].to_owned()
}

fn arg(v: &Violation, name: &str) -> String {
    match v.args.get(name) {
        Some(ViolationArg::Text(text)) => text.clone(),
        Some(ViolationArg::Int(n)) => n.to_string(),
        None => String::new(),
    }
}

/// A length argument in nm, as `0.100 mm`.
fn mm(v: &Violation, name: &str) -> String {
    match v.args.get(name) {
        Some(ViolationArg::Int(nm)) => {
            let um = (nm.abs() + 500) / 1000;
            let sign = if *nm < 0 { "-" } else { "" };
            format!("{sign}{}.{:03} mm", um / 1000, um % 1000)
        }
        _ => arg(v, name),
    }
}

/// A run warning in words, for a reader of the manifest.
fn message(v: &Violation) -> String {
    match v.check.as_str() {
        "out_of_sync" => format!(
            "the PCB is out of sync with the schematic: {} change(s) to apply",
            arg(v, "changes")
        ),
        "unassigned_footprint" => format!(
            "{} has no Footprint, so it is not on the PCB: assign one and Sync",
            arg(v, "designator")
        ),
        "erc_errors" => format!("ERC reports {} error(s)", arg(v, "count")),
        "drc_errors" => format!("DRC reports {} error(s)", arg(v, "count")),
        "rule_outside_preset" => {
            let class = match v.args.get("net_class") {
                Some(_) => format!(" in Net Class {}", arg(v, "net_class")),
                None => String::new(),
            };
            let bound = if arg(v, "limit") == "max" {
                "at most"
            } else {
                "at least"
            };
            format!(
                "board allows {} {} ({}){class}, {} requires {bound} {}",
                mm(v, "board"),
                arg(v, "rule"),
                arg(v, "value"),
                arg(v, "preset"),
                mm(v, "preset_limit")
            )
        }
        "missing_font" => format!("the font \"{}\" is missing on this machine", arg(v, "font")),
        other => {
            let args: Vec<String> = v
                .args
                .keys()
                .map(|k| format!("{k} {}", arg(v, k)))
                .collect();
            if args.is_empty() {
                other.replace('_', " ")
            } else {
                format!("{}: {}", other.replace('_', " "), args.join(", "))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use reda_model::Severity;

    #[test]
    fn times_are_iso() {
        let at = Timestamp(1_767_225_600_000 + 90_000_000);
        assert_eq!(iso_time(at), "2026-01-02T01:00:00Z");
        assert_eq!(iso_date(at), "2026-01-02");
    }

    #[test]
    fn warnings_read_as_words() {
        let v = Violation::new("drc_errors", Severity::Warning).arg("count", 3i64);
        assert_eq!(message(&v), "DRC reports 3 error(s)");
        let v = Violation::new("rule_outside_preset", Severity::Warning)
            .arg("rule", "clearance")
            .arg("value", "value")
            .arg("board", 100_000i64)
            .arg("preset_limit", 152_400i64)
            .arg("limit", "min")
            .arg("preset", "OSH Park");
        assert_eq!(
            message(&v),
            "board allows 0.100 mm clearance (value), OSH Park requires at least 0.152 mm"
        );
    }
}
