//! A small Excellon reader of our own for the drill tests: the header's tools and comment
//! attributes, then drill hits and routed slots in absolute metric coordinates.

use std::collections::BTreeMap;

/// One hole: a drill hit (`to == None`) or a slot routed from `at` to `to`.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Hit {
    /// The tool diameter in nm.
    pub diameter: i64,
    pub at: (i64, i64),
    pub to: Option<(i64, i64)>,
    /// The `TA.AperFunction` of the tool, if the file has attributes.
    pub function: Option<String>,
}

#[derive(Debug, Default)]
pub struct Drill {
    /// `TF.<name>` file attributes, value as written.
    pub file: BTreeMap<String, String>,
    pub metric: bool,
    pub hits: Vec<Hit>,
    /// Whether the file ends with `M30`.
    pub ended: bool,
}

/// A decimal millimetre value as nm: `12.7` → 12_700_000.
fn nm(text: &str) -> i64 {
    let (sign, text) = match text.strip_prefix('-') {
        Some(rest) => (-1, rest),
        None => (1, text),
    };
    let (whole, frac) = text.split_once('.').expect("XNC numbers have a point");
    assert!(frac.len() <= 6, "finer than a nanometre: {text}");
    let whole: i64 = if whole.is_empty() {
        0
    } else {
        whole.parse().unwrap()
    };
    let frac: i64 = format!("{frac:0<6}").parse().unwrap();
    sign * (whole * 1_000_000 + frac)
}

fn xy(text: &str) -> (i64, i64) {
    let text = text
        .strip_prefix('X')
        .unwrap_or_else(|| panic!("no X in {text}"));
    let (x, y) = text
        .split_once('Y')
        .unwrap_or_else(|| panic!("no Y in {text}"));
    (nm(x), nm(y))
}

type Tool = (i64, Option<String>);

pub fn read(bytes: &[u8]) -> Drill {
    let text = std::str::from_utf8(bytes).expect("ASCII");
    let mut lines = text.lines();
    assert_eq!(lines.next(), Some("M48"), "starts with M48");
    let mut drill = Drill::default();
    let mut tools: BTreeMap<String, Tool> = BTreeMap::new();
    let mut function = None;
    for line in lines.by_ref() {
        if line == "%" {
            break;
        } else if let Some(attr) = line.strip_prefix("; #@! TF.") {
            let (name, value) = attr.split_once(',').unwrap_or((attr, ""));
            drill.file.insert(name.into(), value.into());
        } else if let Some(attr) = line.strip_prefix("; #@! TA.AperFunction,") {
            function = Some(attr.to_string());
        } else if line.starts_with(';') || line == "FMAT,2" {
            // a comment, or the format line
        } else if line == "METRIC" {
            drill.metric = true;
        } else if let Some(def) = line.strip_prefix('T') {
            let (n, dia) = def.split_once('C').expect("T<n>C<diameter>");
            tools.insert(n.into(), (nm(dia), function.take()));
        } else {
            panic!("unexpected header line {line:?}");
        }
    }
    let mut tool: Option<Tool> = None;
    let mut route_from = None;
    let mut plunged = None;
    for line in lines {
        let current = || tool.clone().expect("a tool is selected");
        if line == "G90" || line == "G05" {
            // absolute coordinates, drill mode
        } else if line == "M30" {
            drill.ended = true;
        } else if let Some(t) = line.strip_prefix('T') {
            tool = Some(tools.get(t).unwrap_or_else(|| panic!("tool T{t}")).clone());
        } else if let Some(p) = line.strip_prefix("G00") {
            route_from = Some(xy(p));
        } else if line == "M15" {
            plunged = Some(route_from.take().expect("G00 before M15"));
        } else if let Some(p) = line.strip_prefix("G01") {
            let (diameter, function) = current();
            let at = plunged.expect("M15 before G01");
            drill.hits.push(Hit {
                diameter,
                at,
                to: Some(xy(p)),
                function,
            });
        } else if line == "M16" {
            plunged = None;
        } else if line.starts_with('X') {
            let (diameter, function) = current();
            drill.hits.push(Hit {
                diameter,
                at: xy(line),
                to: None,
                function,
            });
        } else {
            panic!("unexpected body line {line:?}");
        }
    }
    drill
}
