//! The source scan: styling lives in `theme` and `widgets`, and nowhere else.
//!
//! It reads every `.rs` file under `crates/reda-ui/src` as text and fails on a colour, an
//! egui style access or a literal size outside the exempt modules
//! (`docs/spec/v0.1/design-language.md`, "Enforcement and tests"). It works on text, so an
//! alias or a macro can defeat it: it is there to stop accidents.
//!
//! - [`lists`] holds what changes as the migration goes: the exempt modules, the files
//!   not yet converted and the stock widgets that have a REDA widget.
//! - [`rules`] holds the banned patterns.

mod lex;
mod lists;
mod rules;
#[cfg(test)]
mod tests;

use std::path::Path;

use rules::StockWidget;

/// The lists a scan runs against.
pub struct Lists<'a> {
    /// Paths under `src` that the scan does not read: a file, or a folder ending in `/`.
    pub exempt: &'a [&'a str],
    /// Files under `src` that still fail, sorted. The scan skips them, and fails once
    /// one of them is clean.
    pub unconverted: &'a [&'a str],
    /// Stock widgets banned in favour of a REDA widget.
    pub stock_widgets: &'a [StockWidget],
}

/// What a scan of the whole tree found.
pub struct Report {
    /// One line per failure, each naming the file, and for a banned use the line and the
    /// pattern.
    pub failures: Vec<String>,
    /// How many `// theme-exempt: <reason>` markers the scanned files carry.
    pub markers: usize,
}

/// Where the scanned tree sits in the repository, for the failure text.
const ROOT: &str = "crates/reda-ui/src";

/// Checks `files` (each a path under `src` with `/` separators, and its text).
pub fn check(files: &[(String, String)], lists: &Lists) -> Report {
    let is_exempt = |path: &str| {
        lists.exempt.iter().any(|entry| {
            if entry.ends_with('/') {
                path.starts_with(entry)
            } else {
                path == *entry
            }
        })
    };
    let mut failures = Vec::new();
    let mut markers = 0;

    if !lists.unconverted.windows(2).all(|pair| pair[0] < pair[1]) {
        failures.push("the unconverted list must be sorted, with no file twice".to_owned());
    }
    for listed in lists.unconverted {
        if is_exempt(listed) {
            failures.push(format!(
                "{ROOT}/{listed}: is exempt, so it does not belong on the unconverted list"
            ));
        } else if !files.iter().any(|(path, _)| path == listed) {
            failures.push(format!(
                "{ROOT}/{listed}: is on the unconverted list, but there is no such file"
            ));
        }
    }

    for (path, text) in files {
        if is_exempt(path) {
            continue;
        }
        let scan = rules::scan(text, lists.stock_widgets);
        markers += scan.markers;
        let listed = lists.unconverted.contains(&path.as_str());
        if !listed {
            failures.extend(
                scan.findings
                    .iter()
                    .map(|finding| format!("{ROOT}/{path}:{}: {}", finding.line, finding.pattern)),
            );
        } else if scan.findings.is_empty() {
            failures.push(format!(
                "{ROOT}/{path}: is clean, so take it off the unconverted list"
            ));
        }
    }
    Report { failures, markers }
}

/// Every `.rs` file under `dir`, as its path below `dir` and its text, sorted by path.
fn rust_files(dir: &Path) -> Vec<(String, String)> {
    fn walk(root: &Path, dir: &Path, files: &mut Vec<(String, String)>) {
        for entry in std::fs::read_dir(dir).expect("read the source folder") {
            let path = entry.expect("read a folder entry").path();
            if path.is_dir() {
                walk(root, &path, files);
            } else if path.extension().is_some_and(|extension| extension == "rs") {
                let below: Vec<_> = path
                    .strip_prefix(root)
                    .expect("a path below the root")
                    .iter()
                    .map(|part| part.to_string_lossy())
                    .collect();
                files.push((
                    below.join("/"),
                    std::fs::read_to_string(&path).expect("read a source file"),
                ));
            }
        }
    }
    let mut files = Vec::new();
    walk(dir, dir, &mut files);
    files.sort();
    files
}

#[test]
fn styling_stays_in_theme_and_widgets() {
    let files = rust_files(&Path::new(env!("CARGO_MANIFEST_DIR")).join("src"));
    assert!(
        files.len() > 100,
        "the scan found only {} files: is it reading the right folder?",
        files.len()
    );
    let lists = Lists {
        exempt: lists::EXEMPT,
        unconverted: lists::UNCONVERTED,
        stock_widgets: lists::STOCK_WIDGETS,
    };
    let report = check(&files, &lists);
    println!(
        "source scan: {} files read, {} `{}` markers, {} files not yet converted",
        files.len(),
        report.markers,
        rules::MARKER,
        lists.unconverted.len()
    );
    assert!(
        report.failures.is_empty(),
        "{} styling failures. Read colours, sizes and text styles from `ui.tokens()` and draw controls with `widgets`; \
         see crates/reda-ui/tests/source_scan/lists.rs.\n{}",
        report.failures.len(),
        report.failures.join("\n")
    );
}
