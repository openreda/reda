//! The Library editors' edit Commands: every change to an open Symbol, Footprint or Part.
//!
//! An element is opened by the engine (`library.element.open` / `.new`), which puts it in
//! the Project on its own undo scope. These Commands replace it there in one transaction,
//! so each change is one undo entry on that element's stack. Elements of the starter
//! Library open read-only, and every edit of one is `rejected`.

mod footprint;
mod part;
mod symbol;

use reda_command::{Angle, CommandError, Context, Length, Registry};
use reda_model::geom::{Nm, Point};
use reda_model::{Footprint, LibraryElement, Part, Shape, Symbol, Uuid};
use schemars::JsonSchema;
use serde::{Deserialize, Deserializer, Serialize};

use crate::ElementKind;

pub use footprint::*;
pub use part::*;
pub use symbol::*;

pub(crate) fn register_all(registry: &mut Registry) {
    symbol::register_all(registry);
    footprint::register_all(registry);
    part::register_all(registry);
}

/// A point param: each coordinate an integer in nanometres or a length string.
#[derive(Clone, Copy, Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PointParam {
    pub x: Length,
    pub y: Length,
}

impl PointParam {
    pub fn point(self) -> Point {
        Point::new(self.x.nm(), self.y.nm())
    }
}

/// A shape param, tagged by `kind`. Coordinates are relative to the element's origin.
#[derive(Clone, Debug, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum ShapeParam {
    Line {
        start: PointParam,
        end: PointParam,
    },
    /// An axis-aligned rectangle between two opposite corners.
    Rect {
        start: PointParam,
        end: PointParam,
    },
    /// The arc from `start` around `center`, sweeping `sweep` counter-clockwise (negative
    /// is clockwise).
    Arc {
        center: PointParam,
        start: PointParam,
        sweep: Angle,
    },
    Circle {
        center: PointParam,
        radius: Length,
    },
    /// A closed polygon through at least three corners.
    Polygon {
        points: Vec<PointParam>,
    },
    Text {
        /// The text's anchor.
        at: PointParam,
        text: String,
        /// The text height.
        size: Length,
        #[serde(default)]
        rotation: Angle,
    },
}

impl ShapeParam {
    /// The shape, or why it cannot be drawn.
    pub fn shape(self) -> Result<Shape, String> {
        Ok(match self {
            ShapeParam::Line { start, end } => Shape::Line {
                start: start.point(),
                end: end.point(),
            },
            ShapeParam::Rect { start, end } => Shape::Rect {
                start: start.point(),
                end: end.point(),
            },
            ShapeParam::Arc {
                center,
                start,
                sweep,
            } => {
                if center.point() == start.point() {
                    return Err("an arc's start cannot be its centre".into());
                }
                if sweep.millideg().0 == 0 {
                    return Err("an arc needs a non-zero sweep".into());
                }
                Shape::Arc {
                    center: center.point(),
                    start: start.point(),
                    sweep: sweep.millideg(),
                }
            }
            ShapeParam::Circle { center, radius } => {
                positive(radius.nm(), "a circle's radius")?;
                Shape::Circle {
                    center: center.point(),
                    radius: radius.nm(),
                }
            }
            ShapeParam::Polygon { points } => {
                if points.len() < 3 {
                    return Err("a polygon needs at least three points".into());
                }
                Shape::Polygon {
                    points: points.into_iter().map(PointParam::point).collect(),
                }
            }
            ShapeParam::Text {
                at,
                text,
                size,
                rotation,
            } => {
                if text.is_empty() {
                    return Err("a text shape needs some text".into());
                }
                reda_model::text::check(&text).map_err(|e| format!("the text: {e}"))?;
                positive(size.nm(), "the text size")?;
                Shape::Text {
                    at: at.point(),
                    text,
                    size: size.nm(),
                    rotation: rotation.millideg(),
                }
            }
        })
    }
}

/// What adding a graphic returns.
#[derive(Debug, Serialize, JsonSchema)]
pub struct GraphicResult {
    /// The new graphic's UUID.
    pub graphic: Uuid,
}

/// Deserialises a present field, `null` included, as `Some`, so an omitted field (left
/// as `None` by `#[serde(default)]`) differs from one set to `null` to clear it.
pub(crate) fn present<'de, D, T>(d: D) -> Result<Option<Option<T>>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    Option::<T>::deserialize(d).map(Some)
}

pub(crate) fn positive(value: Nm, what: &str) -> Result<(), String> {
    if value.0 > 0 {
        Ok(())
    } else {
        Err(format!("{what} must be greater than zero"))
    }
}

pub(crate) fn invalid(pointer: &str) -> impl Fn(String) -> CommandError + '_ {
    move |why| CommandError::invalid_params(pointer, why)
}

/// Checks that `name` can name an element and its file on every OS REDA runs on.
pub fn check_element_name(name: &str) -> Result<(), String> {
    const RESERVED: &[char] = &['/', '\\', '<', '>', ':', '"', '|', '?', '*'];
    if name.trim().is_empty() {
        return Err("the name is empty".into());
    }
    if name == "." || name == ".." {
        return Err(format!("{name:?} cannot be a file name"));
    }
    if let Some(c) = name
        .chars()
        .find(|c| RESERVED.contains(c) || c.is_control())
    {
        return Err(format!(
            "the name contains {c:?}, which file names cannot hold"
        ));
    }
    if name.ends_with(['.', ' ']) {
        return Err("the name cannot end in a dot or a space".into());
    }
    Ok(())
}

/// Runs `f` on a copy of the open element `uuid`, which must be of `kind` and not
/// read-only, and replaces the element with the result in the edit's transaction.
fn edit_element<T>(
    cx: &mut Context<'_>,
    uuid: Uuid,
    kind: ElementKind,
    f: impl FnOnce(&mut LibraryElement) -> Result<T, CommandError>,
) -> Result<T, CommandError> {
    let tx = cx.tx()?;
    let project = tx.project();
    let not_open = || {
        let kind = match kind {
            ElementKind::Symbol => "Symbol",
            ElementKind::Footprint => "Footprint",
            ElementKind::Part => "Part",
        };
        CommandError::not_found(format!(
            "no open {kind} has UUID {uuid}; open it with library.element.open"
        ))
    };
    let (Some(source), Some(element)) = (project.element_source(uuid), project.element(uuid))
    else {
        return Err(not_open());
    };
    if ElementKind::of(element) != kind {
        return Err(not_open());
    }
    if source.read_only {
        return Err(CommandError::rejected(format!(
            "{} is in the starter Library, which is read-only; copy it into your own \
             Library to change it",
            element.name()
        )));
    }
    let mut element = element.clone();
    let out = f(&mut element)?;
    tx.replace(element.into())?;
    Ok(out)
}

pub(crate) fn edit_symbol<T>(
    cx: &mut Context<'_>,
    uuid: Uuid,
    f: impl FnOnce(&mut Symbol) -> Result<T, CommandError>,
) -> Result<T, CommandError> {
    edit_element(cx, uuid, ElementKind::Symbol, |e| match e {
        LibraryElement::Symbol(s) => f(s),
        _ => unreachable!("the kind was checked"),
    })
}

pub(crate) fn edit_footprint<T>(
    cx: &mut Context<'_>,
    uuid: Uuid,
    f: impl FnOnce(&mut Footprint) -> Result<T, CommandError>,
) -> Result<T, CommandError> {
    edit_element(cx, uuid, ElementKind::Footprint, |e| match e {
        LibraryElement::Footprint(fp) => f(fp),
        _ => unreachable!("the kind was checked"),
    })
}

pub(crate) fn edit_part<T>(
    cx: &mut Context<'_>,
    uuid: Uuid,
    f: impl FnOnce(&mut Part) -> Result<T, CommandError>,
) -> Result<T, CommandError> {
    edit_element(cx, uuid, ElementKind::Part, |e| match e {
        LibraryElement::Part(p) => f(p),
        _ => unreachable!("the kind was checked"),
    })
}

/// The item with `uuid` in a UUID-sorted table, or `not_found` naming `what`.
pub(crate) fn find_mut<'a, T>(
    rows: &'a mut [T],
    uuid: Uuid,
    uuid_of: impl Fn(&T) -> Uuid,
    what: &str,
) -> Result<&'a mut T, CommandError> {
    rows.iter_mut()
        .find(|r| uuid_of(r) == uuid)
        .ok_or_else(|| CommandError::not_found(format!("no {what} has UUID {uuid}")))
}

/// Removes the item with `uuid` from a table, or fails `not_found` naming `what`.
pub(crate) fn remove<T>(
    rows: &mut Vec<T>,
    uuid: Uuid,
    uuid_of: impl Fn(&T) -> Uuid,
    what: &str,
) -> Result<T, CommandError> {
    let at = rows
        .iter()
        .position(|r| uuid_of(r) == uuid)
        .ok_or_else(|| CommandError::not_found(format!("no {what} has UUID {uuid}")))?;
    Ok(rows.remove(at))
}
