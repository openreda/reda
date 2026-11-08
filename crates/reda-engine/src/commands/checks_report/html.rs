//! The HTML form of a check report: one self-contained page with no scripts. Its rows
//! don't cross-probe in v0.1, because that needs a registered URL handler.

use std::fmt::Write as _;

use reda_model::{LENGTH_ARGS, Measurement, Violation, ViolationArg, Waiver};
use serde_json::Value;

use super::{Header, Report, is_health};

const STYLE: &str = "body{font-family:sans-serif;margin:2em;color:#222}\
table{border-collapse:collapse;margin:.5em 0 1.5em}\
th,td{border:1px solid #ccc;padding:.25em .6em;text-align:left;vertical-align:top}\
th{background:#f0f0f0}.error{color:#b00020}.warning{color:#8a5a00}\
.muted{color:#777}code{font-size:90%}.problem{color:#b00020}.note{color:#8a5a00}\
.banner{border:1px solid #b00020;padding:.5em 1em;color:#b00020}";

/// The whole page for `report`.
pub(super) fn page(report: &Report, header: &Header) -> String {
    let title = match report {
        Report::Checks { kind, .. } => format!("{} report", kind.to_uppercase()),
        Report::Sync(_) => "Change List".to_owned(),
    };
    let mut out = String::new();
    out.push_str("<!doctype html>\n<html lang=\"en\">\n<head>\n<meta charset=\"utf-8\">\n");
    let _ = writeln!(
        out,
        "<title>{} — {}</title>\n<style>{STYLE}</style>\n</head>\n<body>",
        esc(&title),
        esc(&header.project)
    );
    let _ = writeln!(
        out,
        "<h1>{}</h1>\n<p>Project <b>{}</b>, generated {}.</p>",
        esc(&title),
        esc(&header.project),
        esc(&header.utc)
    );
    match report {
        Report::Checks {
            kind,
            run,
            violations,
            stale_waivers,
            waivers,
        } => checks(&mut out, kind, run, violations, stale_waivers, waivers),
        Report::Sync(list) => change_list(&mut out, list),
    }
    out.push_str("</body>\n</html>\n");
    out
}

fn checks(
    out: &mut String,
    kind: &str,
    run: &Value,
    violations: &[Violation],
    stale: &[Waiver],
    waivers: &[Waiver],
) {
    if run["partial"] == true {
        out.push_str(
            "<p class=\"muted\">A scoped run: only the Violations in its scope are listed, \
             and no Waiver is marked stale.</p>\n",
        );
    }
    let health = |v: &&Violation| kind == "drc" && is_health(&v.check);
    let listed: Vec<&Violation> = (violations.iter())
        .filter(|v| v.waiver.is_none() && !health(v))
        .collect();
    let waived: Vec<&Violation> = violations.iter().filter(|v| v.waiver.is_some()).collect();
    let found: Vec<&Violation> = (violations.iter())
        .filter(|v| v.waiver.is_none() && health(v))
        .collect();

    out.push_str("<section id=\"summary\">\n<h2>Summary</h2>\n");
    let summary = Report::summary(violations);
    if summary.is_empty() {
        out.push_str("<p>No Violations.</p>\n");
    } else {
        out.push_str(
            "<table>\n<tr><th>Check</th><th>Severity</th><th>Violations</th><th>Waived</th></tr>\n",
        );
        for line in &summary {
            let severity = severity(line.severity);
            let _ = writeln!(
                out,
                "<tr><td>{}</td><td class=\"{severity}\">{severity}</td><td>{}</td><td>{}</td></tr>",
                esc(&line.check),
                line.count,
                line.waived
            );
        }
        out.push_str("</table>\n");
    }
    out.push_str("</section>\n");

    section(out, "violations", "Violations", &listed, |_| None);
    section(out, "waived", "Waived Violations", &waived, |v| {
        let w = waivers.iter().find(|w| Some(w.uuid) == v.waiver)?;
        Some(waiver_note(w))
    });

    out.push_str("<section id=\"stale-waivers\">\n<h2>Stale Waivers</h2>\n");
    if stale.is_empty() {
        out.push_str("<p>None.</p>\n");
    } else {
        out.push_str(
            "<table>\n<tr><th>Check</th><th>Objects</th><th>Reason</th><th>Author</th><th>Created</th><th>Waiver</th></tr>\n",
        );
        for w in stale {
            let _ = writeln!(
                out,
                "<tr><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td><code>{}</code></td></tr>",
                esc(&w.check),
                objects(&w.objects),
                esc(&w.reason),
                esc(&w.author),
                esc(&reda_command::Timestamp(w.created).utc()),
                w.uuid
            );
        }
        out.push_str("</table>\n");
    }
    out.push_str("</section>\n");

    if kind == "drc" {
        section(out, "health", "Health Checks", &found, |_| None);
    }
}

/// A section listing `violations`, each with an optional note in the last column.
fn section(
    out: &mut String,
    id: &str,
    title: &str,
    violations: &[&Violation],
    note: impl Fn(&Violation) -> Option<String>,
) {
    let _ = writeln!(out, "<section id=\"{id}\">\n<h2>{title}</h2>");
    if violations.is_empty() {
        out.push_str("<p>None.</p>\n</section>\n");
        return;
    }
    out.push_str(
        "<table>\n<tr><th>Severity</th><th>Check</th><th>Details</th><th>Measured</th><th>Objects</th><th>At</th><th>Note</th></tr>\n",
    );
    for v in violations {
        let severity = severity(v.severity);
        let measured = match (&v.measured, &v.required) {
            (Some(m), Some(r)) => format!("{} (required {})", measure(m), measure(r)),
            (Some(m), None) => measure(m),
            _ => String::new(),
        };
        let at =
            v.at.map(|p| format!("({}, {})", mm(p.x.0), mm(p.y.0)))
                .unwrap_or_default();
        let _ = writeln!(
            out,
            "<tr><td class=\"{severity}\">{severity}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td></tr>",
            esc(&v.check),
            args(v),
            esc(&measured),
            objects(&v.objects),
            esc(&at),
            esc(&note(v).unwrap_or_default())
        );
    }
    out.push_str("</table>\n</section>\n");
}

fn waiver_note(w: &Waiver) -> String {
    let created = reda_command::Timestamp(w.created).utc();
    if w.reason.is_empty() {
        format!("Waived by {} on {created}", w.author)
    } else {
        format!("{} (waived by {} on {created})", w.reason, w.author)
    }
}

fn severity(s: reda_model::Severity) -> &'static str {
    match s {
        reda_model::Severity::Error => "error",
        reda_model::Severity::Warning => "warning",
        reda_model::Severity::Off => "off",
    }
}

fn args(v: &Violation) -> String {
    let parts: Vec<String> = (v.args.iter())
        .map(|(name, value)| {
            let value = match value {
                ViolationArg::Int(n) if LENGTH_ARGS.contains(&name.as_str()) => mm(*n),
                ViolationArg::Int(n) => n.to_string(),
                ViolationArg::Text(t) => t.clone(),
            };
            format!("{}: {}", esc(name), esc(&value))
        })
        .collect();
    parts.join("<br>")
}

fn objects(objects: &[reda_model::Uuid]) -> String {
    let parts: Vec<String> = objects
        .iter()
        .map(|o| format!("<code>{o}</code>"))
        .collect();
    parts.join("<br>")
}

fn measure(m: &Measurement) -> String {
    match m {
        Measurement::Length(n) => mm(n.0),
        Measurement::Area(a) => format!("{} mm²", decimal(*a, 1_000_000_000_000)),
    }
}

/// `n` nm in millimetres, exactly.
fn mm(n: i64) -> String {
    format!("{}mm", decimal(n, 1_000_000))
}

/// `n / unit` as an exact decimal with no trailing zeros.
fn decimal(n: i64, unit: i64) -> String {
    let sign = if n < 0 { "-" } else { "" };
    let (n, unit) = (n.unsigned_abs(), unit.unsigned_abs());
    let (whole, frac) = (n / unit, n % unit);
    if frac == 0 {
        return format!("{sign}{whole}");
    }
    let width = unit.to_string().len() - 1;
    let frac = format!("{frac:0width$}");
    format!("{sign}{whole}.{}", frac.trim_end_matches('0'))
}

/// The Change List, one table per group. Each row shows the change's kind, its other
/// fields and, once changes carry one, its status.
fn change_list(out: &mut String, list: &Value) {
    let _ = writeln!(
        out,
        "<p>{} change(s) pending; Problem rows can't apply and are not counted.</p>",
        list["count"].as_u64().unwrap_or(0)
    );
    if let Some(block) = list.get("duplicate_nets") {
        let _ = writeln!(
            out,
            "<p id=\"duplicate-nets\" class=\"banner\">{}</p>",
            esc(block["message"].as_str().unwrap_or_default())
        );
    }
    let groups = list["groups"].as_array().cloned().unwrap_or_default();
    out.push_str("<section id=\"changes\">\n");
    if groups.is_empty() {
        out.push_str("<p>The board is in sync with the schematic.</p>\n");
    }
    for group in &groups {
        let name = group["group"].as_str().unwrap_or_default();
        let _ = writeln!(out, "<h2>{}</h2>", esc(&name.replace('_', " ")));
        out.push_str("<table>\n<tr><th>Kind</th><th>Details</th><th>Status</th><th>Id</th></tr>\n");
        for change in group["changes"].as_array().into_iter().flatten() {
            let Some(fields) = change.as_object() else {
                continue;
            };
            let details: Vec<String> = (fields.iter())
                .filter(|(k, _)| {
                    !matches!(
                        k.as_str(),
                        "id" | "kind" | "requires" | "status" | "message"
                    )
                })
                .map(|(k, v)| format!("{}: {}", esc(k), esc(&plain(v))))
                .collect();
            let status = change["status"].as_str().unwrap_or_default();
            let message = match change["message"].as_str() {
                Some(m) => format!("<br>{}", esc(m)),
                None => String::new(),
            };
            let _ = writeln!(
                out,
                "<tr><td>{}</td><td>{}</td><td class=\"{status}\">{}{message}</td><td><code>{}</code></td></tr>",
                esc(change["kind"].as_str().unwrap_or_default()),
                details.join("<br>"),
                esc(status),
                esc(change["id"].as_str().unwrap_or_default())
            );
        }
        out.push_str("</table>\n");
    }
    out.push_str("</section>\n");
}

/// A JSON value as text: strings bare, everything else compact JSON.
fn plain(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}

/// `text` with HTML's special characters escaped.
fn esc(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            c => out.push(c),
        }
    }
    out
}
