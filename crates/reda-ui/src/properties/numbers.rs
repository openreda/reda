//! The selection's numeric properties (text height, Zone clearance…) and the
//! [numeric expressions](reda_engine::model::geom::expr) typed into them.
//!
//! An expression runs once per selected object, with `!` as that object's own value and
//! its other attributes by schema name. The results go out as one Command per object,
//! wrapped in one `batch` for a multi-selection, so they commit together with one undo
//! entry and the Journal records the resolved values, not the expression. Any failure
//! commits nothing.

use reda_engine::model::geom::expr::{self, Attr, Field};
use reda_engine::model::{Object, Uuid};
use reda_engine::preferences::Units;
use serde_json::{Value, json};

pub use reda_engine::model::geom::expr::Kind;

use super::Shared;

/// The numeric properties every selected object has.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Numbers {
    /// The editable values, in the order the first object lists them.
    pub rows: Vec<NumberRow>,
    targets: Vec<Target>,
}

/// One numeric property of the selection.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NumberRow {
    /// Its English schema name, which expressions also use: `height`, `clearance`.
    pub name: &'static str,
    pub kind: Kind,
    /// In nanometres or millidegrees; `None` where an object has no value of its own.
    pub value: Shared<Option<i64>>,
}

/// One selected object: the Command that edits it and the values expressions can use.
#[derive(Clone, Debug, PartialEq)]
struct Target {
    command: &'static str,
    /// The Command's param that names the object.
    key: &'static str,
    uuid: Uuid,
    editable: &'static [(&'static str, Kind)],
    attributes: Vec<(&'static str, Attr)>,
}

impl Target {
    fn of(object: &Object) -> Option<Target> {
        use Kind::{Angle, Length};
        let (command, key, uuid, editable, attributes) = match object {
            Object::PcbText(t) => (
                "pcb.text.set",
                "uuid",
                t.object.uuid,
                &[
                    ("height", Length),
                    ("thickness", Length),
                    ("rotation", Angle),
                ][..],
                vec![
                    ("height", Some(Attr::Length(t.object.style.height))),
                    ("thickness", Some(Attr::Length(t.object.style.thickness))),
                    ("rotation", Some(Attr::Angle(t.object.rotation))),
                ],
            ),
            Object::SheetText(t) => (
                "schematic.text.set",
                "uuid",
                t.object.uuid,
                &[("height", Length), ("rotation", Angle)][..],
                vec![
                    ("height", Some(Attr::Length(t.object.height))),
                    ("rotation", Some(Attr::Angle(t.object.rotation))),
                ],
            ),
            Object::Zone(z) => (
                "pcb.zone.set",
                "zone",
                z.object.uuid,
                &[("clearance", Length), ("min_width", Length)][..],
                vec![
                    ("clearance", z.object.clearance.map(Attr::Length)),
                    ("min_width", z.object.min_width.map(Attr::Length)),
                    ("priority", Some(Attr::Number(i64::from(z.object.priority)))),
                ],
            ),
            _ => return None,
        };
        Some(Target {
            command,
            key,
            uuid,
            editable,
            attributes: attributes
                .into_iter()
                .filter_map(|(name, value)| Some((name, value?)))
                .collect(),
        })
    }

    /// `name`'s value in nanometres or millidegrees.
    fn value(&self, name: &str) -> Option<i64> {
        self.attributes
            .iter()
            .find(|(n, _)| *n == name)
            .map(|(_, attr)| match *attr {
                Attr::Length(nm) => nm.0,
                Attr::Angle(md) => md.0,
                Attr::Number(n) => n,
            })
    }

    fn step(&self, name: &str, value: i64) -> (&'static str, Value) {
        let mut params = json!({});
        params[self.key] = json!(self.uuid);
        params[name] = json!(value);
        (self.command, params)
    }
}

impl Numbers {
    /// The numeric properties of `objects`, when every one of them has some.
    pub(super) fn of(objects: &[Object]) -> Numbers {
        let Some(targets) = objects.iter().map(Target::of).collect::<Option<Vec<_>>>() else {
            return Numbers::default();
        };
        let Some(first) = targets.first() else {
            return Numbers::default();
        };
        let rows = first
            .editable
            .iter()
            .filter(|row| targets.iter().all(|t| t.editable.contains(row)))
            .map(|&(name, kind)| NumberRow {
                name,
                kind,
                value: Shared::of(targets.iter().map(|t| t.value(name))).expect("not empty"),
            })
            .collect();
        Numbers { rows, targets }
    }

    /// The Command that sets row `name` from what was typed, `text`, on every selected
    /// object: the object's own Command alone, or a `batch` of one per object. A bare
    /// number is in `units` (degrees for angles). `Ok(None)` when no row is called `name`;
    /// an error, written for the person who typed it, when the expression fails for any
    /// object.
    pub fn command(
        &self,
        name: &str,
        text: &str,
        units: Units,
    ) -> Result<Option<(&'static str, Value)>, String> {
        let Some(row) = self.rows.iter().find(|r| r.name == name) else {
            return Ok(None);
        };
        let field = Field {
            kind: row.kind,
            unit: match (row.kind, units) {
                (Kind::Angle, _) => "deg",
                (Kind::Length, Units::Mm) => "mm",
                (Kind::Length, Units::Mil) => "mil",
            },
        };
        let objects: Vec<expr::Object> = self
            .targets
            .iter()
            .map(|t| expr::Object {
                current: t.value(name),
                attributes: &t.attributes,
            })
            .collect();
        let values = expr::evaluate_each(text, field, &objects).map_err(|e| {
            if self.targets.len() > 1 {
                format!(
                    "object {} of {}: {}",
                    e.object + 1,
                    self.targets.len(),
                    e.error
                )
            } else {
                e.error.to_string()
            }
        })?;
        let mut steps = self
            .targets
            .iter()
            .zip(values)
            .map(|(t, v)| t.step(row.name, v));
        if self.targets.len() == 1 {
            return Ok(steps.next());
        }
        let commands: Vec<Value> = steps
            .map(|(id, params)| json!({"id": id, "params": params}))
            .collect();
        Ok(Some(("batch", json!({ "commands": commands }))))
    }
}
