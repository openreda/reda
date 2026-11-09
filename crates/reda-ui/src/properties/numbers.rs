//! The selection's numeric properties (text height, Zone clearance…) and the
//! [numeric expressions](reda_engine::model::geom::expr) typed into them.
//!
//! An expression runs once per selected object, with `!` as that object's own value and
//! its other attributes by schema name. The results go out as one Command per object,
//! wrapped in one `batch` for a multi-selection, so they commit together with one undo
//! entry and the Journal records the resolved values, not the expression. Any failure
//! commits nothing.

use reda_engine::model::geom::Nm;
use reda_engine::model::geom::expr::{self, Attr, Field};
use reda_engine::model::{Object, TrackGeometry, Uuid};
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
    pub region: Region,
    /// In nanometres or millidegrees; `None` where an object has no value of its own.
    pub value: Shared<Option<i64>>,
}

/// Which region of the panel a numeric row belongs in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Region {
    /// X, Y and rotation.
    Location,
    /// Sizes: widths, heights, clearances.
    Graphical,
}

impl Region {
    fn of(name: &str) -> Region {
        match name {
            "x" | "y" | "rotation" => Region::Location,
            _ => Region::Graphical,
        }
    }
}

/// How a value reaches the object.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Apply {
    /// The object's own set Command takes it by name.
    Set,
    /// `pcb.move` by the difference, along X or Y.
    MoveX,
    MoveY,
    /// `pcb.rotate` in place by the difference.
    Rotate,
}

/// One selected object: the Command that edits it and the values expressions can use.
#[derive(Clone, Debug, PartialEq)]
struct Target {
    command: &'static str,
    /// The Command's param that names the object.
    key: &'static str,
    /// The param takes a list (`tracks: [..]`) rather than one UUID.
    list: bool,
    uuid: Uuid,
    editable: &'static [(&'static str, Kind, Apply)],
    attributes: Vec<(&'static str, Attr)>,
}

impl Target {
    fn of(object: &Object) -> Option<Target> {
        use Apply::{MoveX, MoveY, Rotate, Set};
        use Kind::{Angle, Length};
        let (command, key, list, uuid, editable, attributes) = match object {
            Object::PcbText(t) => (
                "pcb.text.set",
                "uuid",
                false,
                t.object.uuid,
                &[
                    ("x", Length, Set),
                    ("y", Length, Set),
                    ("rotation", Angle, Set),
                    ("height", Length, Set),
                    ("thickness", Length, Set),
                ][..],
                vec![
                    ("x", Some(Attr::Length(t.object.position.x))),
                    ("y", Some(Attr::Length(t.object.position.y))),
                    ("rotation", Some(Attr::Angle(t.object.rotation))),
                    ("height", Some(Attr::Length(t.object.style.height))),
                    ("thickness", Some(Attr::Length(t.object.style.thickness))),
                ],
            ),
            Object::SheetText(t) => (
                "schematic.text.set",
                "uuid",
                false,
                t.object.uuid,
                &[("rotation", Angle, Set), ("height", Length, Set)][..],
                vec![
                    ("rotation", Some(Attr::Angle(t.object.rotation))),
                    ("height", Some(Attr::Length(t.object.height))),
                ],
            ),
            Object::Zone(z) => (
                "pcb.zone.set",
                "zone",
                false,
                z.object.uuid,
                &[("clearance", Length, Set), ("min_width", Length, Set)][..],
                vec![
                    ("clearance", z.object.clearance.map(Attr::Length)),
                    ("min_width", z.object.min_width.map(Attr::Length)),
                    ("priority", Some(Attr::Number(i64::from(z.object.priority)))),
                ],
            ),
            Object::Track(t) => {
                let straight = match t.object.geometry {
                    TrackGeometry::Line { .. } => {
                        Some(Attr::Length(Nm(t.object.geometry.length().round() as i64)))
                    }
                    TrackGeometry::Arc { .. } => None,
                };
                (
                    "pcb.track.set",
                    "tracks",
                    true,
                    t.object.uuid,
                    if straight.is_some() {
                        &[("width", Length, Set), ("length", Length, Set)][..]
                    } else {
                        &[("width", Length, Set)][..]
                    },
                    vec![
                        ("width", Some(Attr::Length(t.object.width))),
                        ("length", straight),
                    ],
                )
            }
            Object::Via(v) => (
                "pcb.via.set",
                "vias",
                true,
                v.object.uuid,
                &[("diameter", Length, Set), ("hole", Length, Set)][..],
                vec![
                    ("diameter", Some(Attr::Length(v.object.diameter))),
                    ("hole", Some(Attr::Length(v.object.hole))),
                ],
            ),
            Object::FootprintInstance(f) => (
                "pcb.move",
                "objects",
                true,
                f.object.uuid,
                &[
                    ("x", Length, MoveX),
                    ("y", Length, MoveY),
                    ("rotation", Angle, Rotate),
                ][..],
                vec![
                    ("x", Some(Attr::Length(f.object.at.x))),
                    ("y", Some(Attr::Length(f.object.at.y))),
                    ("rotation", Some(Attr::Angle(f.object.rotation))),
                ],
            ),
            _ => return None,
        };
        Some(Target {
            command,
            key,
            list,
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

    /// The Command that gives row `name` the value `value`; `None` when it already has it
    /// and the edit is a move or turn by the difference.
    fn step(&self, name: &str, value: i64) -> Option<(&'static str, Value)> {
        let apply = self
            .editable
            .iter()
            .find(|(n, ..)| *n == name)
            .map_or(Apply::Set, |(.., a)| *a);
        let by = value - self.value(name).unwrap_or(0);
        match apply {
            Apply::Set => {
                let mut params = json!({});
                params[self.key] = if self.list {
                    json!([self.uuid])
                } else {
                    json!(self.uuid)
                };
                params[name] = json!(value);
                Some((self.command, params))
            }
            _ if by == 0 => None,
            Apply::MoveX => Some((
                "pcb.move",
                json!({"objects": [self.uuid], "by": {"x": by, "y": 0}}),
            )),
            Apply::MoveY => Some((
                "pcb.move",
                json!({"objects": [self.uuid], "by": {"x": 0, "y": by}}),
            )),
            Apply::Rotate => Some(("pcb.rotate", json!({"objects": [self.uuid], "angle": by}))),
        }
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
            .filter(|(name, ..)| {
                targets
                    .iter()
                    .all(|t| t.editable.iter().any(|(n, ..)| n == name))
            })
            .map(|&(name, kind, _)| NumberRow {
                name,
                kind,
                region: Region::of(name),
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
        let mut steps: Vec<(&'static str, Value)> = self
            .targets
            .iter()
            .zip(values)
            .filter_map(|(t, v)| t.step(row.name, v))
            .collect();
        if steps.len() <= 1 {
            return Ok(steps.pop());
        }
        let commands: Vec<Value> = steps
            .into_iter()
            .map(|(id, params)| json!({"id": id, "params": params}))
            .collect();
        Ok(Some(("batch", json!({ "commands": commands }))))
    }
}
