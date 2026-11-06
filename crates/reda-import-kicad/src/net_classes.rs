//! Net classes from a KiCad 6+ project file (`.kicad_pro`, JSON) as REDA rule records.
//!
//! A KiCad net class holds the full set of sizes its nets are routed with. REDA keeps those
//! as `preferred` values: the `Default` class becomes board-wide values, and every other
//! class becomes a Net Class whose records hold only what differs from `Default`. Ranks
//! follow the file's order, first highest. Only clearance, track width, via diameter and
//! via drill map; the rest (micro vias, differential pairs, colours) is left out with a
//! warning when it is set.

use reda_model::geom::Nm;
use reda_model::{RuleField, RuleKind, RuleValue, RuleValues};
use serde_json::Value;

use crate::number::mm;
use crate::{ImportError, SourceFile};

/// The KiCad name of the class every net is in, which maps to the board-wide records.
const DEFAULT: &str = "Default";

/// The values a net class field maps to.
const FIELDS: [(&str, RuleKind, RuleField); 4] = [
    ("clearance", RuleKind::Clearance, RuleField::Value),
    ("track_width", RuleKind::TrackWidth, RuleField::Preferred),
    (
        "via_diameter",
        RuleKind::ViaSize,
        RuleField::DiameterPreferred,
    ),
    ("via_drill", RuleKind::ViaSize, RuleField::HolePreferred),
];

/// Net class fields REDA does not map, reported when a class sets them.
const LEFT_OUT: [&str; 6] = [
    "microvia_diameter",
    "microvia_drill",
    "diff_pair_width",
    "diff_pair_gap",
    "diff_pair_via_gap",
    "wire_width",
];

/// One mapped value: the rule kind, the value in its record and the length.
type Mapped = (RuleKind, RuleField, Nm);

/// One net class as a REDA Net Class.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ImportedNetClass {
    pub name: String,
    /// 1 ranks highest.
    pub rank: u32,
    /// Its rule records' values, one per kind, holding only what differs from `Default`.
    pub rules: Vec<(RuleKind, RuleValues)>,
}

/// What a project file's net classes map to, not yet written to any board.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ImportedRules {
    /// The `Default` class's values, to set on the board-wide records.
    pub board: Vec<(RuleKind, RuleValues)>,
    pub classes: Vec<ImportedNetClass>,
    /// Things REDA could not represent, in file order.
    pub warnings: Vec<String>,
}

fn error(message: impl Into<String>) -> ImportError {
    ImportError {
        file: SourceFile::Project,
        line: None,
        message: message.into(),
    }
}

/// A class's mapped values, by field.
fn values(class: &serde_json::Map<String, Value>) -> Result<Vec<Mapped>, ImportError> {
    let mut out = Vec::new();
    for (key, kind, field) in FIELDS {
        let Some(value) = class.get(key) else {
            continue;
        };
        let length = value
            .as_number()
            .and_then(|n| mm(&n.to_string()))
            .filter(|n| *n >= Nm::ZERO)
            .ok_or_else(|| error(format!("`{key}` is not a length in mm: {value}")))?;
        out.push((kind, field, length));
    }
    Ok(out)
}

/// Groups `values` into one record per kind, in kind order.
fn records(values: impl IntoIterator<Item = Mapped>) -> Vec<(RuleKind, RuleValues)> {
    let mut out: Vec<(RuleKind, RuleValues)> = Vec::new();
    for (kind, field, length) in values {
        let i = match out.iter().position(|(k, _)| *k == kind) {
            Some(i) => i,
            None => {
                out.push((kind, RuleValues::default()));
                out.len() - 1
            }
        };
        out[i]
            .1
            .set(field, Some(RuleValue::Length(length)))
            .expect("a mapped field holds a length");
    }
    out.sort_by_key(|(k, _)| *k);
    out
}

/// The net classes in `project` (the text of a `.kicad_pro`) as rule records.
pub fn import_net_classes(project: &str) -> Result<ImportedRules, ImportError> {
    let root: Value =
        serde_json::from_str(project).map_err(|e| error(format!("not a project file: {e}")))?;
    let Some(classes) = root.pointer("/net_settings/classes") else {
        return Ok(ImportedRules::default());
    };
    let classes = classes
        .as_array()
        .ok_or_else(|| error("`net_settings.classes` is not a list"))?;
    let mut out = ImportedRules::default();
    let mut default: Vec<Mapped> = Vec::new();
    let mut others: Vec<(String, Vec<Mapped>)> = Vec::new();
    for class in classes {
        let class = class
            .as_object()
            .ok_or_else(|| error("a net class is not an object"))?;
        let name = class
            .get("name")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|n| !n.is_empty())
            .ok_or_else(|| error("a net class has no name"))?;
        for key in LEFT_OUT {
            if class.get(key).is_some_and(|v| !v.is_null()) {
                out.warnings
                    .push(format!("net class {name:?}: `{key}` is not imported"));
            }
        }
        let mapped = values(class)?;
        if name.eq_ignore_ascii_case(DEFAULT) {
            default = mapped;
        } else if others.iter().any(|(n, _)| n.eq_ignore_ascii_case(name)) {
            out.warnings.push(format!(
                "net class {name:?} appears twice; the first is kept"
            ));
        } else {
            others.push((name.to_owned(), mapped));
        }
    }
    for (i, (name, mapped)) in others.into_iter().enumerate() {
        let overrides = mapped
            .into_iter()
            .filter(|(k, f, n)| !default.contains(&(*k, *f, *n)));
        out.classes.push(ImportedNetClass {
            name,
            rank: u32::try_from(i + 1).unwrap_or(u32::MAX),
            rules: records(overrides),
        });
    }
    out.board = records(default);
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    const PROJECT: &str = r#"{
      "board": {},
      "net_settings": {
        "classes": [
          {"name": "Default", "clearance": 0.2, "track_width": 0.25, "via_diameter": 0.6,
           "via_drill": 0.3, "diff_pair_width": 0.2},
          {"name": "Power", "clearance": 0.2, "track_width": 1.0, "via_diameter": 0.8,
           "via_drill": 0.4},
          {"name": "HV", "clearance": 1.5, "track_width": 0.25, "via_diameter": 0.6,
           "via_drill": 0.3},
          {"name": "power", "clearance": 9}
        ]
      }
    }"#;

    fn length(n: i64) -> Option<Nm> {
        Some(Nm::from_um(n))
    }

    #[test]
    fn the_default_class_seeds_the_board_and_other_classes_keep_their_differences() {
        let imported = import_net_classes(PROJECT).unwrap();
        let board: Vec<RuleKind> = imported.board.iter().map(|(k, _)| *k).collect();
        assert_eq!(
            board,
            [RuleKind::Clearance, RuleKind::TrackWidth, RuleKind::ViaSize]
        );
        assert_eq!(imported.board[0].1.value, length(200));
        assert_eq!(imported.board[1].1.preferred, length(250));
        let via = imported.board[2].1;
        assert_eq!(via.diameter.unwrap().preferred, length(600));
        assert_eq!(via.hole.unwrap().preferred, length(300));

        let names: Vec<(&str, u32)> = imported
            .classes
            .iter()
            .map(|c| (c.name.as_str(), c.rank))
            .collect();
        assert_eq!(
            names,
            [("Power", 1), ("HV", 2)],
            "file order; the duplicate is dropped"
        );
        let power = &imported.classes[0].rules;
        assert_eq!(
            power.len(),
            2,
            "clearance equals Default, so it is not overridden"
        );
        assert_eq!(power[0].0, RuleKind::TrackWidth);
        assert_eq!(power[0].1.preferred, length(1000));
        assert_eq!(power[1].1.hole.unwrap().preferred, length(400));
        let hv = &imported.classes[1].rules;
        assert_eq!(hv.len(), 1);
        assert_eq!(hv[0].1.value, length(1500));

        assert_eq!(imported.warnings.len(), 2, "{:?}", imported.warnings);
    }

    #[test]
    fn a_project_without_net_classes_imports_nothing() {
        assert_eq!(import_net_classes("{}").unwrap(), ImportedRules::default());
    }

    #[test]
    fn a_bad_length_or_file_is_refused() {
        let bad = r#"{"net_settings": {"classes": [{"name": "A", "clearance": "x"}]}}"#;
        assert!(import_net_classes(bad).is_err());
        assert!(import_net_classes("not json").is_err());
    }
}
